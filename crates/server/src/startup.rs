use std::{fmt, sync::Arc};

use chrono::Duration;
use sea_orm::{ConnectionTrait, Database, DbBackend, DbErr, Statement};
use sea_orm_migration::MigratorTrait;
use thiserror::Error;
use tjxy_application::{AuthError, AuthService, SystemClock};

use crate::{AppState, ServerIdentity};

pub struct BootstrapAdmin {
    username: String,
    password: String,
}

impl BootstrapAdmin {
    #[must_use]
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }
}

impl fmt::Debug for BootstrapAdmin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BootstrapAdmin")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug)]
pub struct StartupOptions {
    database_url: String,
    identity: ServerIdentity,
    bootstrap_admin: Option<BootstrapAdmin>,
    legacy_auth_enabled: bool,
    session_lifetime: Option<Duration>,
    max_concurrent_password_hashes: usize,
}

impl StartupOptions {
    #[must_use]
    pub fn new(database_url: impl Into<String>, identity: ServerIdentity) -> Self {
        Self {
            database_url: database_url.into(),
            identity,
            bootstrap_admin: None,
            legacy_auth_enabled: true,
            session_lifetime: None,
            max_concurrent_password_hashes: 2,
        }
    }

    #[must_use]
    pub fn with_bootstrap_admin(mut self, admin: BootstrapAdmin) -> Self {
        self.bootstrap_admin = Some(admin);
        self
    }

    #[must_use]
    pub const fn with_legacy_auth_enabled(mut self, enabled: bool) -> Self {
        self.legacy_auth_enabled = enabled;
        self
    }
}

/// Connects the SQL source of truth, applies migrations, optionally creates the
/// first administrator, and returns ready application state.
///
/// # Errors
///
/// Returns [`InitializationError`] without exposing bootstrap credentials when
/// connection, migration, or authentication setup fails.
pub async fn initialize(options: StartupOptions) -> Result<AppState, InitializationError> {
    let database = Database::connect(&options.database_url).await?;
    if database.get_database_backend() == DbBackend::Sqlite {
        database
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys = ON".to_owned(),
            ))
            .await?;
    }
    tjxy_db::Migrator::up(&database, None).await?;
    let auth = Arc::new(
        AuthService::new(
            database,
            SystemClock,
            options.session_lifetime,
            options.max_concurrent_password_hashes,
        )
        .await?,
    );
    if !auth.has_enabled_admin().await? {
        let admin = options
            .bootstrap_admin
            .ok_or(InitializationError::MissingInitialAdministrator)?;
        if admin.password.is_empty() {
            return Err(InitializationError::EmptyBootstrapPassword);
        }
        auth.create_initial_admin(&admin.username, &admin.password)
            .await?;
    }
    let has_enabled_admin = auth.has_enabled_admin().await?;
    Ok(AppState::new(
        options
            .identity
            .with_startup_wizard_completed(has_enabled_admin),
    )
    .with_auth(auth)
    .with_legacy_auth_enabled(options.legacy_auth_enabled)
    .with_ready(true))
}

#[derive(Debug, Error)]
pub enum InitializationError {
    #[error("a bootstrap administrator is required for a database with no users")]
    MissingInitialAdministrator,
    #[error("the bootstrap administrator password must not be empty")]
    EmptyBootstrapPassword,
    #[error("database initialization failed: {0}")]
    Database(#[from] DbErr),
    #[error("authentication initialization failed: {0}")]
    Authentication(#[from] AuthError),
}
