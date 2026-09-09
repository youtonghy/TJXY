use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use thiserror::Error;

pub const DEFAULT_LOG_RETENTION_DAYS: u16 = 30;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LogMode {
    Error,
    #[default]
    Info,
    Debug,
}

impl LogMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::Info => "Info",
            Self::Debug => "Debug",
        }
    }
}

impl fmt::Display for LogMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for LogMode {
    type Err = LoggingSettingsRepositoryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "Error" => Ok(Self::Error),
            "Info" => Ok(Self::Info),
            "Debug" => Ok(Self::Debug),
            _ => Err(LoggingSettingsRepositoryError::InvalidMode),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoggingSettingsRecord {
    mode: LogMode,
    retention_days: u16,
    normal_mode: LogMode,
    debug_expires_at: Option<DateTime<Utc>>,
    budget: LogBudget,
    revision: i64,
    updated_at: DateTime<Utc>,
}

impl LoggingSettingsRecord {
    #[must_use]
    pub const fn mode(&self) -> LogMode {
        self.mode
    }
    #[must_use]
    pub fn effective_mode_at(&self, now: DateTime<Utc>) -> LogMode {
        if self.mode == LogMode::Debug && self.debug_expires_at.is_none_or(|expires| expires <= now)
        {
            self.normal_mode
        } else {
            self.mode
        }
    }
    #[must_use]
    pub const fn normal_mode(&self) -> LogMode {
        self.normal_mode
    }
    #[must_use]
    pub const fn debug_expires_at(&self) -> Option<DateTime<Utc>> {
        self.debug_expires_at
    }
    #[must_use]
    pub const fn budget(&self) -> LogBudget {
        self.budget
    }
    #[must_use]
    pub const fn retention_days(&self) -> u16 {
        self.retention_days
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoggingSettingsInput {
    pub mode: LogMode,
    pub retention_days: u16,
}

impl Default for LoggingSettingsInput {
    fn default() -> Self {
        Self {
            mode: LogMode::Info,
            retention_days: DEFAULT_LOG_RETENTION_DAYS,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogBudget {
    pub max_file_bytes: u64,
    pub max_directory_bytes: u64,
}
impl Default for LogBudget {
    fn default() -> Self {
        Self {
            max_file_bytes: 32 * 1024 * 1024,
            max_directory_bytes: 256 * 1024 * 1024,
        }
    }
}
impl LogBudget {
    /// # Errors
    /// Rejects limits outside 1–1024 MiB per file or 1–16384 MiB total.
    pub fn validate(self) -> Result<(), LoggingSettingsRepositoryError> {
        if !(1024 * 1024..=1024 * 1024 * 1024).contains(&self.max_file_bytes)
            || self.max_directory_bytes < self.max_file_bytes
            || self.max_directory_bytes > 16 * 1024 * 1024 * 1024
        {
            return Err(LoggingSettingsRepositoryError::InvalidBudget);
        }
        Ok(())
    }
}

pub struct LoggingSettingsRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> LoggingSettingsRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Loads the singleton logging settings row.
    ///
    /// # Errors
    /// Returns an error for database failures or invalid persisted values.
    pub async fn get(
        &self,
    ) -> Result<Option<LoggingSettingsRecord>, LoggingSettingsRepositoryError> {
        get_on(self.database).await
    }

    /// Persists logging settings using revision-based optimistic concurrency.
    ///
    /// # Errors
    /// Returns an error for invalid input, revision conflicts, or database failures.
    pub async fn put(
        &self,
        input: LoggingSettingsInput,
        expected_revision: Option<i64>,
    ) -> Result<LoggingSettingsRecord, LoggingSettingsRepositoryError> {
        self.put_with_budget(input, expected_revision, None).await
    }

    /// Persists optional capacity limits; omitted limits preserve existing settings.
    /// # Errors
    /// Returns validation, revision conflict or database errors.
    #[allow(clippy::too_many_lines)] // One transaction preserves revision checks and all budget fields.
    pub async fn put_with_budget(
        &self,
        input: LoggingSettingsInput,
        expected_revision: Option<i64>,
        budget: Option<LogBudget>,
    ) -> Result<LoggingSettingsRecord, LoggingSettingsRepositoryError> {
        validate(input)?;
        let transaction = self.database.begin().await?;
        let current = get_on(&transaction).await?;
        let now = Utc::now();
        let budget = budget.unwrap_or_else(|| {
            current
                .as_ref()
                .map_or_else(LogBudget::default, LoggingSettingsRecord::budget)
        });
        budget.validate()?;
        let normal_mode = if input.mode == LogMode::Debug {
            current.as_ref().map_or(LogMode::Info, |record| {
                if record.mode == LogMode::Debug {
                    record.normal_mode
                } else {
                    record.mode
                }
            })
        } else {
            input.mode
        };
        let expires = (input.mode == LogMode::Debug).then_some(now + chrono::Duration::minutes(30));
        let legacy_mode = if input.mode == LogMode::Info {
            LogMode::Error
        } else {
            input.mode
        };
        match (current, expected_revision) {
            (None, None) => {
                transaction
                    .execute(
                        transaction.get_database_backend().build(
                            &Query::insert()
                                .into_table(Alias::new("logging_settings"))
                                .columns([
                                    "id",
                                    "mode",
                                    "configured_mode",
                                    "normal_mode",
                                    "debug_expires_at",
                                    "max_file_bytes",
                                    "max_directory_bytes",
                                    "retention_days",
                                    "revision",
                                    "created_at",
                                    "updated_at",
                                ])
                                .values_panic([
                                    1_i32.into(),
                                    legacy_mode.as_str().into(),
                                    input.mode.as_str().into(),
                                    normal_mode.as_str().into(),
                                    expires.into(),
                                    i64::try_from(budget.max_file_bytes)
                                        .map_err(|_| LoggingSettingsRepositoryError::InvalidBudget)?
                                        .into(),
                                    i64::try_from(budget.max_directory_bytes)
                                        .map_err(|_| LoggingSettingsRepositoryError::InvalidBudget)?
                                        .into(),
                                    i32::from(input.retention_days).into(),
                                    1_i64.into(),
                                    now.into(),
                                    now.into(),
                                ])
                                .to_owned(),
                        ),
                    )
                    .await?;
            }
            (Some(current), Some(revision)) if current.revision == revision => {
                let result = transaction
                    .execute(
                        transaction.get_database_backend().build(
                            &Query::update()
                                .table(Alias::new("logging_settings"))
                                .values([
                                    (Alias::new("mode"), legacy_mode.as_str().into()),
                                    (Alias::new("configured_mode"), input.mode.as_str().into()),
                                    (Alias::new("normal_mode"), normal_mode.as_str().into()),
                                    (Alias::new("debug_expires_at"), expires.into()),
                                    (
                                        Alias::new("max_file_bytes"),
                                        i64::try_from(budget.max_file_bytes)
                                            .map_err(|_| {
                                                LoggingSettingsRepositoryError::InvalidBudget
                                            })?
                                            .into(),
                                    ),
                                    (
                                        Alias::new("max_directory_bytes"),
                                        i64::try_from(budget.max_directory_bytes)
                                            .map_err(|_| {
                                                LoggingSettingsRepositoryError::InvalidBudget
                                            })?
                                            .into(),
                                    ),
                                    (
                                        Alias::new("retention_days"),
                                        i32::from(input.retention_days).into(),
                                    ),
                                    (Alias::new("revision"), (revision + 1).into()),
                                    (Alias::new("updated_at"), now.into()),
                                ])
                                .and_where(Expr::col(Alias::new("id")).eq(1_i32))
                                .and_where(Expr::col(Alias::new("revision")).eq(revision))
                                .to_owned(),
                        ),
                    )
                    .await?;
                if result.rows_affected() != 1 {
                    return Err(LoggingSettingsRepositoryError::Conflict);
                }
            }
            _ => return Err(LoggingSettingsRepositoryError::Conflict),
        }
        transaction.commit().await?;
        self.get()
            .await?
            .ok_or(LoggingSettingsRepositoryError::MissingPersistedSettings)
    }
}

fn validate(input: LoggingSettingsInput) -> Result<(), LoggingSettingsRepositoryError> {
    if !(1..=365).contains(&input.retention_days) {
        return Err(LoggingSettingsRepositoryError::InvalidRetentionDays);
    }
    Ok(())
}

async fn get_on(
    connection: &impl ConnectionTrait,
) -> Result<Option<LoggingSettingsRecord>, LoggingSettingsRepositoryError> {
    let query = Query::select()
        .columns([
            "mode",
            "configured_mode",
            "normal_mode",
            "debug_expires_at",
            "max_file_bytes",
            "max_directory_bytes",
            "retention_days",
            "revision",
            "updated_at",
        ])
        .from(Alias::new("logging_settings"))
        .and_where(Expr::col(Alias::new("id")).eq(1_i32))
        .to_owned();
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await?
        .as_ref()
        .map(record_from_row)
        .transpose()
}

fn record_from_row(
    row: &QueryResult,
) -> Result<LoggingSettingsRecord, LoggingSettingsRepositoryError> {
    let days: i32 = row.try_get("", "retention_days")?;
    let retention_days =
        u16::try_from(days).map_err(|_| LoggingSettingsRepositoryError::InvalidRetentionDays)?;
    let record = LoggingSettingsRecord {
        mode: row
            .try_get::<Option<String>>("", "configured_mode")?
            .unwrap_or(row.try_get("", "mode")?)
            .parse()?,
        normal_mode: row.try_get::<String>("", "normal_mode")?.parse()?,
        debug_expires_at: row.try_get("", "debug_expires_at")?,
        budget: LogBudget {
            max_file_bytes: u64::try_from(row.try_get::<i64>("", "max_file_bytes")?)
                .map_err(|_| LoggingSettingsRepositoryError::InvalidBudget)?,
            max_directory_bytes: u64::try_from(row.try_get::<i64>("", "max_directory_bytes")?)
                .map_err(|_| LoggingSettingsRepositoryError::InvalidBudget)?,
        },
        retention_days,
        revision: row.try_get("", "revision")?,
        updated_at: row.try_get("", "updated_at")?,
    };
    record.budget.validate()?;
    if record.normal_mode == LogMode::Debug {
        return Err(LoggingSettingsRepositoryError::InvalidMode);
    }
    validate(LoggingSettingsInput {
        mode: record.mode,
        retention_days: record.retention_days,
    })?;
    Ok(record)
}

#[derive(Debug, Error)]
pub enum LoggingSettingsRepositoryError {
    #[error("logging capacity limits are invalid")]
    InvalidBudget,
    #[error("logging mode is invalid")]
    InvalidMode,
    #[error("logging retention days must be from 1 through 365")]
    InvalidRetentionDays,
    #[error("logging settings revision conflict")]
    Conflict,
    #[error("persisted logging settings disappeared")]
    MissingPersistedSettings,
    #[error("logging settings database operation failed: {0}")]
    Database(#[from] DbErr),
}
