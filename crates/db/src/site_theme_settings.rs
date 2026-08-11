use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, SqlErr,
    TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const DEFAULT_SITE_THEME_ID: &str = "classic";
pub const DEFAULT_SITE_THEME_SCHEMA_VERSION: u32 = 1;
const MAX_THEME_CONFIGURATIONS: usize = 32;
const MAX_THEME_OPTIONS_BYTES: usize = 16 * 1024;
const MAX_THEME_OPTION_DEPTH: usize = 8;
const MAX_THEME_OPTION_ENTRIES: usize = 64;
const MAX_THEME_OPTION_STRING_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SiteThemeConfiguration {
    schema_version: u32,
    options: Value,
}

impl SiteThemeConfiguration {
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub const fn options(&self) -> &Value {
        &self.options
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SiteThemeSettingsRecord {
    active_theme_id: String,
    configurations: BTreeMap<String, SiteThemeConfiguration>,
    revision: i64,
    updated_at: DateTime<Utc>,
}

impl SiteThemeSettingsRecord {
    #[must_use]
    pub fn active_theme_id(&self) -> &str {
        &self.active_theme_id
    }

    #[must_use]
    pub const fn configurations(&self) -> &BTreeMap<String, SiteThemeConfiguration> {
        &self.configurations
    }

    #[must_use]
    /// Returns the configuration selected by this validated settings record.
    ///
    /// # Panics
    ///
    /// Panics only if the record's private invariants are violated internally. Repository reads
    /// reject records whose active theme does not have a corresponding configuration.
    pub fn active_configuration(&self) -> &SiteThemeConfiguration {
        self.configurations
            .get(&self.active_theme_id)
            .expect("validated theme settings contain the active configuration")
    }

    #[must_use]
    pub const fn revision(&self) -> i64 {
        self.revision
    }

    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SiteThemeSelectionInput {
    pub theme_id: String,
    pub schema_version: u32,
    pub options: Value,
}

pub struct SiteThemeSettingsRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> SiteThemeSettingsRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Loads the singleton site theme settings row.
    ///
    /// # Errors
    ///
    /// Returns a repository error when the row cannot be queried or contains invalid data.
    pub async fn get(
        &self,
    ) -> Result<Option<SiteThemeSettingsRecord>, SiteThemeSettingsRepositoryError> {
        get_on(self.database).await
    }

    /// Selects a theme, stores its normalized options, and preserves other theme configurations.
    ///
    /// # Errors
    ///
    /// Returns a repository error for invalid input, revision conflicts, or database failures.
    pub async fn put(
        &self,
        input: &SiteThemeSelectionInput,
        expected_revision: Option<i64>,
    ) -> Result<SiteThemeSettingsRecord, SiteThemeSettingsRepositoryError> {
        let input = validate_selection(input)?;
        if expected_revision.is_some_and(|revision| revision <= 0 || revision == i64::MAX) {
            return Err(SiteThemeSettingsRepositoryError::InvalidRevision);
        }
        let current = get_on(self.database).await?;
        let mut configurations = match (&current, expected_revision) {
            (None, None) => BTreeMap::new(),
            (None, Some(_)) | (Some(_), None) => {
                return Err(SiteThemeSettingsRepositoryError::Conflict);
            }
            (Some(record), Some(expected)) if record.revision == expected => {
                record.configurations.clone()
            }
            (Some(_), Some(_)) => return Err(SiteThemeSettingsRepositoryError::Conflict),
        };
        configurations.insert(
            input.theme_id.clone(),
            SiteThemeConfiguration {
                schema_version: input.schema_version,
                options: input.options.clone(),
            },
        );
        if configurations.len() > MAX_THEME_CONFIGURATIONS {
            return Err(SiteThemeSettingsRepositoryError::InvalidConfigurations);
        }
        let transaction = self.database.begin().await?;
        let result = put_on(
            &transaction,
            input.theme_id,
            configurations,
            expected_revision,
        )
        .await;
        finish(transaction, result).await
    }
}

async fn put_on(
    transaction: &DatabaseTransaction,
    active_theme_id: String,
    configurations: BTreeMap<String, SiteThemeConfiguration>,
    expected_revision: Option<i64>,
) -> Result<SiteThemeSettingsRecord, SiteThemeSettingsRepositoryError> {
    let stored_configurations = serde_json::to_value(
        configurations
            .iter()
            .map(|(theme_id, configuration)| {
                (
                    theme_id.clone(),
                    StoredThemeConfiguration {
                        schema_version: configuration.schema_version,
                        options: configuration.options.clone(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )
    .map_err(|_| SiteThemeSettingsRepositoryError::InvalidConfigurations)?;
    let now = Utc::now();
    let backend = transaction.get_database_backend();
    match expected_revision {
        None => {
            let statement = Query::insert()
                .into_table(Alias::new("site_theme_settings"))
                .columns([
                    Alias::new("id"),
                    Alias::new("active_theme_id"),
                    Alias::new("configurations"),
                    Alias::new("revision"),
                    Alias::new("created_at"),
                    Alias::new("updated_at"),
                ])
                .values_panic([
                    1_i32.into(),
                    active_theme_id.into(),
                    stored_configurations.into(),
                    1_i64.into(),
                    now.into(),
                    now.into(),
                ])
                .to_owned();
            if let Err(error) = transaction.execute(backend.build(&statement)).await {
                if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
                    return Err(SiteThemeSettingsRepositoryError::Conflict);
                }
                return Err(error.into());
            }
        }
        Some(expected) => {
            if expected <= 0 {
                return Err(SiteThemeSettingsRepositoryError::InvalidRevision);
            }
            let revision = expected
                .checked_add(1)
                .ok_or(SiteThemeSettingsRepositoryError::InvalidRevision)?;
            let statement = Query::update()
                .table(Alias::new("site_theme_settings"))
                .values([
                    (Alias::new("active_theme_id"), active_theme_id.into()),
                    (Alias::new("configurations"), stored_configurations.into()),
                    (Alias::new("revision"), revision.into()),
                    (Alias::new("updated_at"), now.into()),
                ])
                .and_where(Expr::col(Alias::new("id")).eq(1_i32))
                .and_where(Expr::col(Alias::new("revision")).eq(expected))
                .to_owned();
            if transaction
                .execute(backend.build(&statement))
                .await?
                .rows_affected()
                != 1
            {
                return Err(SiteThemeSettingsRepositoryError::Conflict);
            }
        }
    }
    get_on(transaction)
        .await?
        .ok_or(SiteThemeSettingsRepositoryError::MissingPersistedSettings)
}

async fn get_on(
    connection: &impl ConnectionTrait,
) -> Result<Option<SiteThemeSettingsRecord>, SiteThemeSettingsRepositoryError> {
    let query = Query::select()
        .columns([
            Alias::new("active_theme_id"),
            Alias::new("configurations"),
            Alias::new("revision"),
            Alias::new("updated_at"),
        ])
        .from(Alias::new("site_theme_settings"))
        .and_where(Expr::col(Alias::new("id")).eq(1_i32))
        .to_owned();
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await?
        .as_ref()
        .map(settings_from_row)
        .transpose()
}

fn settings_from_row(
    row: &QueryResult,
) -> Result<SiteThemeSettingsRecord, SiteThemeSettingsRepositoryError> {
    let active_theme_id: String = row.try_get("", "active_theme_id")?;
    if !valid_theme_id(&active_theme_id) {
        return Err(SiteThemeSettingsRepositoryError::InvalidPersistedSettings);
    }
    let stored = serde_json::from_value::<BTreeMap<String, StoredThemeConfiguration>>(
        row.try_get("", "configurations")?,
    )
    .map_err(|_| SiteThemeSettingsRepositoryError::InvalidPersistedSettings)?;
    if stored.is_empty() || stored.len() > MAX_THEME_CONFIGURATIONS {
        return Err(SiteThemeSettingsRepositoryError::InvalidPersistedSettings);
    }
    let mut configurations = BTreeMap::new();
    for (theme_id, configuration) in stored {
        let validated = validate_selection(&SiteThemeSelectionInput {
            theme_id: theme_id.clone(),
            schema_version: configuration.schema_version,
            options: configuration.options,
        })
        .map_err(|_| SiteThemeSettingsRepositoryError::InvalidPersistedSettings)?;
        configurations.insert(
            theme_id,
            SiteThemeConfiguration {
                schema_version: validated.schema_version,
                options: validated.options,
            },
        );
    }
    if !configurations.contains_key(&active_theme_id) {
        return Err(SiteThemeSettingsRepositoryError::InvalidPersistedSettings);
    }
    let revision: i64 = row.try_get("", "revision")?;
    if revision <= 0 {
        return Err(SiteThemeSettingsRepositoryError::InvalidPersistedSettings);
    }
    Ok(SiteThemeSettingsRecord {
        active_theme_id,
        configurations,
        revision,
        updated_at: row.try_get("", "updated_at")?,
    })
}

fn validate_selection(
    input: &SiteThemeSelectionInput,
) -> Result<SiteThemeSelectionInput, SiteThemeSettingsRepositoryError> {
    if !valid_theme_id(&input.theme_id) {
        return Err(SiteThemeSettingsRepositoryError::InvalidThemeId);
    }
    if input.schema_version == 0 || input.schema_version > 1_000 {
        return Err(SiteThemeSettingsRepositoryError::InvalidSchemaVersion);
    }
    if !input.options.is_object()
        || serde_json::to_vec(&input.options)
            .map_err(|_| SiteThemeSettingsRepositoryError::InvalidOptions)?
            .len()
            > MAX_THEME_OPTIONS_BYTES
        || !valid_options_value(&input.options, 0)
    {
        return Err(SiteThemeSettingsRepositoryError::InvalidOptions);
    }
    Ok(input.clone())
}

fn valid_theme_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn valid_options_value(value: &Value, depth: usize) -> bool {
    if depth > MAX_THEME_OPTION_DEPTH {
        return false;
    }
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => true,
        Value::String(value) => value.len() <= MAX_THEME_OPTION_STRING_BYTES,
        Value::Array(values) => {
            values.len() <= MAX_THEME_OPTION_ENTRIES
                && values
                    .iter()
                    .all(|value| valid_options_value(value, depth + 1))
        }
        Value::Object(values) => {
            values.len() <= MAX_THEME_OPTION_ENTRIES
                && values.iter().all(|(key, value)| {
                    !key.is_empty()
                        && key.len() <= 64
                        && !key.chars().any(char::is_control)
                        && valid_options_value(value, depth + 1)
                })
        }
    }
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, SiteThemeSettingsRepositoryError>,
) -> Result<T, SiteThemeSettingsRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(SiteThemeSettingsRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

#[derive(Debug, Error)]
pub enum SiteThemeSettingsRepositoryError {
    #[error("site theme settings revision conflict")]
    Conflict,
    #[error("invalid site theme id")]
    InvalidThemeId,
    #[error("invalid site theme schema version")]
    InvalidSchemaVersion,
    #[error("invalid site theme options")]
    InvalidOptions,
    #[error("invalid site theme configurations")]
    InvalidConfigurations,
    #[error("invalid persisted site theme settings")]
    InvalidPersistedSettings,
    #[error("invalid site theme settings revision")]
    InvalidRevision,
    #[error("site theme settings were not persisted")]
    MissingPersistedSettings,
    #[error("site theme settings rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
    #[error(transparent)]
    Database(#[from] DbErr),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredThemeConfiguration {
    schema_version: u32,
    options: Value,
}
