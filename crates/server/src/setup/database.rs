use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{DatabaseConfiguration, DatabaseTlsMode, SecretString};

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_VERSION_CHARS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseBackend {
    Sqlite,
    PostgreSql,
    Mysql,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(
    tag = "Backend",
    rename_all = "lowercase",
    rename_all_fields = "PascalCase",
    deny_unknown_fields
)]
pub enum DatabaseDraft {
    Sqlite {
        path: PathBuf,
    },
    #[serde(rename = "postgresql")]
    PostgreSql {
        host: String,
        port: u16,
        database: String,
        username: String,
        password: SecretString,
        tls: DatabaseTlsMode,
    },
    Mysql {
        host: String,
        port: u16,
        database: String,
        username: String,
        password: SecretString,
        tls: DatabaseTlsMode,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DatabaseTestResult {
    backend: DatabaseBackend,
    version: String,
    elapsed_milliseconds: u64,
}

impl DatabaseTestResult {
    #[must_use]
    pub const fn backend(&self) -> DatabaseBackend {
        self.backend
    }

    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    #[must_use]
    pub const fn elapsed_milliseconds(&self) -> u64 {
        self.elapsed_milliseconds
    }
}

#[derive(Clone, Debug)]
pub struct SetupValidator {
    sqlite_roots: Vec<PathBuf>,
}

impl SetupValidator {
    /// Creates a validator from canonical server-side `SQLite` roots.
    ///
    /// # Errors
    ///
    /// Returns an unsafe-path error when a configured root is missing or not a directory.
    pub fn new(sqlite_roots: Vec<PathBuf>) -> Result<Self, SetupError> {
        let sqlite_roots = sqlite_roots
            .into_iter()
            .map(|root| {
                root.canonicalize()
                    .ok()
                    .filter(|root| root.is_dir())
                    .ok_or_else(|| SetupError::new(SetupErrorCode::UnsafeDatabasePath))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { sqlite_roots })
    }

    /// Tests a bounded database connection and returns only safe metadata.
    ///
    /// # Errors
    ///
    /// Returns a stable setup category without retaining a raw driver error.
    pub async fn test_database(
        &self,
        draft: &DatabaseDraft,
    ) -> Result<DatabaseTestResult, SetupError> {
        let started = Instant::now();
        let (database, configuration, backend) = self.connect_for_install(draft).await?;
        let version = database_version(&database, backend).await?;
        database
            .close()
            .await
            .map_err(|_| SetupError::new(SetupErrorCode::DatabaseUnavailable))?;
        drop(configuration);
        Ok(DatabaseTestResult {
            backend,
            version,
            elapsed_milliseconds: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }

    pub(super) async fn connect_for_install(
        &self,
        draft: &DatabaseDraft,
    ) -> Result<(DatabaseConnection, DatabaseConfiguration, DatabaseBackend), SetupError> {
        let (backend, configuration, url) = self.prepare_configuration(draft)?;
        let database = self.connect_url(&url).await?;
        enable_sqlite_foreign_keys(&database).await?;
        Ok((database, configuration, backend))
    }

    pub(super) fn prepare_configuration(
        &self,
        draft: &DatabaseDraft,
    ) -> Result<(DatabaseBackend, DatabaseConfiguration, Zeroizing<String>), SetupError> {
        validate_database_draft(draft)?;
        let (backend, configuration, url) = match draft {
            DatabaseDraft::Sqlite { path } => {
                let path = self.sqlite_path(path)?;
                configuration_with_url(
                    DatabaseBackend::Sqlite,
                    DatabaseConfiguration::Sqlite { path },
                )?
            }
            DatabaseDraft::PostgreSql {
                host,
                port,
                database,
                username,
                password,
                tls,
            } => {
                let configuration = DatabaseConfiguration::PostgreSql {
                    host: host.clone(),
                    port: *port,
                    database: database.clone(),
                    username: username.clone(),
                    password: password.clone(),
                    tls: *tls,
                };
                configuration_with_url(DatabaseBackend::PostgreSql, configuration)?
            }
            DatabaseDraft::Mysql {
                host,
                port,
                database,
                username,
                password,
                tls,
            } => {
                let configuration = DatabaseConfiguration::Mysql {
                    host: host.clone(),
                    port: *port,
                    database: database.clone(),
                    username: username.clone(),
                    password: password.clone(),
                    tls: *tls,
                };
                configuration_with_url(DatabaseBackend::Mysql, configuration)?
            }
        };
        Ok((backend, configuration, url))
    }

    async fn connect_url(&self, url: &str) -> Result<DatabaseConnection, SetupError> {
        let mut options = ConnectOptions::new(url);
        options
            .max_connections(1)
            .min_connections(1)
            .connect_timeout(CONNECTION_TIMEOUT)
            .acquire_timeout(CONNECTION_TIMEOUT)
            .sqlx_logging(false);
        Database::connect(options)
            .await
            .map_err(|_| SetupError::new(SetupErrorCode::DatabaseUnavailable))
    }

    pub(super) async fn connect_configuration(
        &self,
        configuration: &DatabaseConfiguration,
    ) -> Result<DatabaseConnection, SetupError> {
        let url = configuration
            .connection_url()
            .map_err(|_| SetupError::new(SetupErrorCode::DatabaseConfigurationInvalid))?;
        let database = self.connect_url(&url).await?;
        enable_sqlite_foreign_keys(&database).await?;
        Ok(database)
    }

    fn sqlite_path(&self, path: &Path) -> Result<PathBuf, SetupError> {
        let file_name = path
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| SetupError::new(SetupErrorCode::UnsafeDatabasePath))?;
        let parent = path
            .parent()
            .ok_or_else(|| SetupError::new(SetupErrorCode::UnsafeDatabasePath))?
            .canonicalize()
            .map_err(|_| SetupError::new(SetupErrorCode::UnsafeDatabasePath))?;
        if !self
            .sqlite_roots
            .iter()
            .any(|root| parent.starts_with(root))
        {
            return Err(SetupError::new(SetupErrorCode::UnsafeDatabasePath));
        }
        let candidate = parent.join(file_name);
        if fs::symlink_metadata(&candidate)
            .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
        {
            return Err(SetupError::new(SetupErrorCode::UnsafeDatabasePath));
        }
        Ok(candidate)
    }
}

fn validate_database_draft(draft: &DatabaseDraft) -> Result<(), SetupError> {
    let valid = match draft {
        DatabaseDraft::Sqlite { .. } => true,
        DatabaseDraft::PostgreSql {
            host,
            port,
            database,
            username,
            password,
            ..
        }
        | DatabaseDraft::Mysql {
            host,
            port,
            database,
            username,
            password,
            ..
        } => {
            !host.is_empty()
                && host.len() <= 255
                && *port > 0
                && !database.is_empty()
                && database.len() <= 128
                && !username.is_empty()
                && username.len() <= 128
                && password.expose().len() <= 4_096
        }
    };
    valid
        .then_some(())
        .ok_or_else(|| SetupError::new(SetupErrorCode::DatabaseConfigurationInvalid))
}

fn configuration_with_url(
    backend: DatabaseBackend,
    configuration: DatabaseConfiguration,
) -> Result<(DatabaseBackend, DatabaseConfiguration, Zeroizing<String>), SetupError> {
    let url = configuration
        .connection_url()
        .map_err(|_| SetupError::new(SetupErrorCode::DatabaseConfigurationInvalid))?;
    Ok((backend, configuration, url))
}

async fn enable_sqlite_foreign_keys(database: &DatabaseConnection) -> Result<(), SetupError> {
    if database.get_database_backend() == DbBackend::Sqlite {
        database
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys = ON".to_owned(),
            ))
            .await
            .map_err(|_| SetupError::new(SetupErrorCode::DatabaseUnavailable))?;
    }
    Ok(())
}

async fn database_version(
    database: &DatabaseConnection,
    backend: DatabaseBackend,
) -> Result<String, SetupError> {
    let (db_backend, statement) = match backend {
        DatabaseBackend::Sqlite => (DbBackend::Sqlite, "SELECT sqlite_version() AS version"),
        DatabaseBackend::PostgreSql => (
            DbBackend::Postgres,
            "SELECT current_setting('server_version') AS version",
        ),
        DatabaseBackend::Mysql => (DbBackend::MySql, "SELECT VERSION() AS version"),
    };
    let row = database
        .query_one(Statement::from_string(db_backend, statement.to_owned()))
        .await
        .map_err(|_| SetupError::new(SetupErrorCode::DatabaseUnavailable))?
        .ok_or_else(|| SetupError::new(SetupErrorCode::DatabaseUnavailable))?;
    let version: String = row
        .try_get("", "version")
        .map_err(|_| SetupError::new(SetupErrorCode::DatabaseUnavailable))?;
    if version.is_empty() || version.chars().count() > MAX_VERSION_CHARS {
        return Err(SetupError::new(SetupErrorCode::DatabaseResponseInvalid));
    }
    Ok(version)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupErrorCode {
    UnsafeDatabasePath,
    DatabaseConfigurationInvalid,
    DatabaseUnavailable,
    DatabaseResponseInvalid,
    ConfigurationWriteFailed,
    InstallationFailed,
    InstallationConflict,
    AdministratorInvalid,
    AdministratorExists,
    SystemSettingsInvalid,
    NetworkInvalid,
    BrandingInvalid,
    BrandingWriteFailed,
    ConfigurationReadFailed,
    RecoveryAuthenticationFailed,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("setup operation failed: {code:?}")]
pub struct SetupError {
    code: SetupErrorCode,
}

impl SetupError {
    pub(super) const fn new(code: SetupErrorCode) -> Self {
        Self { code }
    }

    #[must_use]
    pub const fn code(self) -> SetupErrorCode {
        self.code
    }
}
