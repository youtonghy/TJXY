use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use thiserror::Error;
use tjxy_common::UserId;
use uuid::Uuid;

const SINGLETON_KEY: i32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallationStatus {
    Pending,
    Completed,
}

impl InstallationStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Completed => "completed",
        }
    }

    fn parse(value: &str) -> Result<Self, InstallationRepositoryError> {
        match value {
            "pending" => Ok(Self::Pending),
            "completed" => Ok(Self::Completed),
            _ => Err(InstallationRepositoryError::InvalidStoredValue),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallationRecord {
    installation_id: Uuid,
    server_id: Uuid,
    status: InstallationStatus,
    administrator_id: Option<UserId>,
    revision: i64,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

impl InstallationRecord {
    #[must_use]
    pub const fn installation_id(&self) -> Uuid {
        self.installation_id
    }
    #[must_use]
    pub const fn server_id(&self) -> Uuid {
        self.server_id
    }
    #[must_use]
    pub const fn status(&self) -> InstallationStatus {
        self.status
    }
    #[must_use]
    pub const fn administrator_id(&self) -> Option<UserId> {
        self.administrator_id
    }
    #[must_use]
    pub const fn revision(&self) -> i64 {
        self.revision
    }
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub const fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }
}

pub struct InstallationRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> InstallationRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Loads the singleton installation record.
    ///
    /// # Errors
    ///
    /// Returns a database or stored-value error when the record cannot be read safely.
    pub async fn find(&self) -> Result<Option<InstallationRecord>, InstallationRepositoryError> {
        find_on(self.database).await
    }

    /// Begins one installation or idempotently returns the matching existing record.
    ///
    /// # Errors
    ///
    /// Returns `Conflict` when another installation already owns the database.
    pub async fn begin(
        &self,
        installation_id: Uuid,
        server_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<InstallationRecord, InstallationRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = async {
            if let Some(existing) = find_on(&transaction).await? {
                return matching(existing, installation_id, server_id);
            }
            let insert = Query::insert()
                .into_table(Alias::new("installation_records"))
                .columns([
                    Alias::new("installation_id"),
                    Alias::new("server_id"),
                    Alias::new("singleton_key"),
                    Alias::new("status"),
                    Alias::new("administrator_id"),
                    Alias::new("revision"),
                    Alias::new("created_at"),
                    Alias::new("completed_at"),
                ])
                .values_panic([
                    installation_id.into(),
                    server_id.into(),
                    SINGLETON_KEY.into(),
                    InstallationStatus::Pending.as_str().into(),
                    Option::<Uuid>::None.into(),
                    1_i64.into(),
                    now.into(),
                    Option::<DateTime<Utc>>::None.into(),
                ])
                .to_owned();
            let backend = transaction.get_database_backend();
            if transaction.execute(backend.build(&insert)).await.is_err() {
                return find_on(&transaction)
                    .await?
                    .ok_or(InstallationRepositoryError::Conflict)
                    .and_then(|record| matching(record, installation_id, server_id));
            }
            find_on(&transaction)
                .await?
                .ok_or(InstallationRepositoryError::Missing)
        }
        .await;
        finish(transaction, result).await
    }

    /// Attaches the first administrator using a revision fence.
    ///
    /// # Errors
    ///
    /// Returns `Conflict` for a stale or invalid transition.
    pub async fn attach_initial_admin(
        &self,
        installation_id: Uuid,
        administrator_id: UserId,
        expected_revision: i64,
        _now: DateTime<Utc>,
    ) -> Result<InstallationRecord, InstallationRepositoryError> {
        self.revision_update(
            installation_id,
            expected_revision,
            Some(("administrator_id", administrator_id.as_uuid().into())),
            None,
        )
        .await
    }

    /// Completes the installation or idempotently returns an already completed record.
    ///
    /// # Errors
    ///
    /// Returns `Conflict` for a stale or invalid transition.
    pub async fn complete(
        &self,
        installation_id: Uuid,
        expected_revision: i64,
        now: DateTime<Utc>,
    ) -> Result<InstallationRecord, InstallationRepositoryError> {
        if let Some(existing) = self.find().await?
            && existing.installation_id == installation_id
            && existing.status == InstallationStatus::Completed
        {
            return Ok(existing);
        }
        self.revision_update(
            installation_id,
            expected_revision,
            Some(("status", InstallationStatus::Completed.as_str().into())),
            Some(now),
        )
        .await
    }

    async fn revision_update(
        &self,
        installation_id: Uuid,
        expected_revision: i64,
        value: Option<(&'static str, sea_orm::Value)>,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<InstallationRecord, InstallationRepositoryError> {
        if expected_revision < 1 || expected_revision == i64::MAX {
            return Err(InstallationRepositoryError::Conflict);
        }
        let transaction = self.database.begin().await?;
        let result = async {
            let current = find_on(&transaction)
                .await?
                .ok_or(InstallationRepositoryError::Missing)?;
            if current.installation_id != installation_id
                || current.status != InstallationStatus::Pending
                || current.revision != expected_revision
            {
                return Err(InstallationRepositoryError::Conflict);
            }
            if value.as_ref().is_some_and(|(column, _)| {
                *column == "administrator_id" && current.administrator_id.is_some()
            }) || (completed_at.is_some() && current.administrator_id.is_none())
            {
                return Err(InstallationRepositoryError::Conflict);
            }
            let mut update = Query::update();
            update
                .table(Alias::new("installation_records"))
                .value(Alias::new("revision"), expected_revision + 1)
                .and_where(Expr::col(Alias::new("installation_id")).eq(installation_id))
                .and_where(Expr::col(Alias::new("revision")).eq(expected_revision));
            if let Some((column, value)) = value {
                update.value(Alias::new(column), value);
            }
            if let Some(completed_at) = completed_at {
                update.value(Alias::new("completed_at"), completed_at);
            }
            let backend = transaction.get_database_backend();
            if transaction
                .execute(backend.build(&update.clone()))
                .await?
                .rows_affected()
                != 1
            {
                return Err(InstallationRepositoryError::Conflict);
            }
            find_on(&transaction)
                .await?
                .ok_or(InstallationRepositoryError::Missing)
        }
        .await;
        finish(transaction, result).await
    }
}

fn matching(
    record: InstallationRecord,
    installation_id: Uuid,
    server_id: Uuid,
) -> Result<InstallationRecord, InstallationRepositoryError> {
    if record.installation_id == installation_id && record.server_id == server_id {
        Ok(record)
    } else {
        Err(InstallationRepositoryError::Conflict)
    }
}

async fn find_on<Connection>(
    connection: &Connection,
) -> Result<Option<InstallationRecord>, InstallationRepositoryError>
where
    Connection: ConnectionTrait,
{
    let query = Query::select()
        .columns([
            Alias::new("installation_id"),
            Alias::new("server_id"),
            Alias::new("status"),
            Alias::new("administrator_id"),
            Alias::new("revision"),
            Alias::new("created_at"),
            Alias::new("completed_at"),
        ])
        .from(Alias::new("installation_records"))
        .and_where(Expr::col(Alias::new("singleton_key")).eq(SINGLETON_KEY))
        .limit(1)
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&query))
        .await?
        .map(|row| record_from_row(&row))
        .transpose()
}

fn record_from_row(row: &QueryResult) -> Result<InstallationRecord, InstallationRepositoryError> {
    Ok(InstallationRecord {
        installation_id: row.try_get("", "installation_id")?,
        server_id: row.try_get("", "server_id")?,
        status: InstallationStatus::parse(&row.try_get::<String>("", "status")?)?,
        administrator_id: row
            .try_get::<Option<Uuid>>("", "administrator_id")?
            .map(UserId::from_uuid),
        revision: row.try_get("", "revision")?,
        created_at: row.try_get("", "created_at")?,
        completed_at: row.try_get("", "completed_at")?,
    })
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, InstallationRepositoryError>,
) -> Result<T, InstallationRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

#[derive(Debug, Error)]
pub enum InstallationRepositoryError {
    #[error("the installation conflicts with stored state")]
    Conflict,
    #[error("the installation record is missing")]
    Missing,
    #[error("the installation record contains an invalid value")]
    InvalidStoredValue,
    #[error("the installation database operation failed")]
    Database(#[from] DbErr),
}
