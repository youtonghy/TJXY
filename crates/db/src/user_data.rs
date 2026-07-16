use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use thiserror::Error;
use tjxy_common::{CatalogItemId, UserId};
use uuid::Uuid;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UserDataPatch {
    playback_position_ticks: Option<i64>,
    is_played: Option<bool>,
    play_count_delta: Option<i32>,
    is_favorite: Option<bool>,
    last_played_at: Option<DateTime<Utc>>,
}

impl UserDataPatch {
    #[must_use]
    pub const fn favorite(value: bool) -> Self {
        Self {
            playback_position_ticks: None,
            is_played: None,
            play_count_delta: None,
            is_favorite: Some(value),
            last_played_at: None,
        }
    }

    #[must_use]
    pub const fn with_playback_position_ticks(mut self, value: i64) -> Self {
        self.playback_position_ticks = Some(value);
        self
    }

    #[must_use]
    pub const fn with_played(mut self, value: bool) -> Self {
        self.is_played = Some(value);
        self
    }

    #[must_use]
    pub const fn with_play_count_delta(mut self, value: i32) -> Self {
        self.play_count_delta = Some(value);
        self
    }

    #[must_use]
    pub fn with_last_played_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_played_at = Some(value);
        self
    }

    const fn is_empty(&self) -> bool {
        self.playback_position_ticks.is_none()
            && self.is_played.is_none()
            && self.play_count_delta.is_none()
            && self.is_favorite.is_none()
            && self.last_played_at.is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserDataRecord {
    pub user_id: UserId,
    pub catalog_item_id: CatalogItemId,
    pub playback_position_ticks: i64,
    pub is_played: bool,
    pub play_count: i32,
    pub is_favorite: bool,
    pub last_played_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserDataCommit {
    pub data: UserDataRecord,
    pub user_revision: i64,
}

pub struct UserDataRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> UserDataRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Atomically applies a field-level patch and increments the user's cache revision.
    ///
    /// # Errors
    ///
    /// Returns [`UserDataRepositoryError`] for an empty/invalid patch, a database
    /// failure, a failed rollback, or an invariant violation while reading back
    /// the committed data.
    pub async fn commit(
        &self,
        user_id: UserId,
        catalog_item_id: CatalogItemId,
        patch: UserDataPatch,
    ) -> Result<UserDataCommit, UserDataRepositoryError> {
        validate_patch(&patch)?;
        let transaction = self.database.begin().await?;
        let result =
            commit_in_transaction(&transaction, user_id, catalog_item_id, &patch, Utc::now()).await;
        match result {
            Ok(commit) => {
                transaction.commit().await?;
                Ok(commit)
            }
            Err(original) => match transaction.rollback().await {
                Ok(()) => Err(original),
                Err(rollback) => Err(UserDataRepositoryError::RollbackFailed {
                    original: original.to_string(),
                    rollback,
                }),
            },
        }
    }

    /// Reads the current SQL revision for a user.
    ///
    /// # Errors
    ///
    /// Returns [`UserDataRepositoryError`] when the query fails.
    pub async fn revision(&self, user_id: UserId) -> Result<Option<i64>, UserDataRepositoryError> {
        read_revision(self.database, user_id).await
    }

    /// Reads the current user data for one catalog item.
    ///
    /// # Errors
    ///
    /// Returns [`UserDataRepositoryError`] when the query fails or stored data
    /// cannot be decoded.
    pub async fn get(
        &self,
        user_id: UserId,
        catalog_item_id: CatalogItemId,
    ) -> Result<Option<UserDataRecord>, UserDataRepositoryError> {
        read_user_data(self.database, user_id, catalog_item_id).await
    }
}

#[derive(Debug, Error)]
pub enum UserDataRepositoryError {
    #[error("user data patch must change at least one field")]
    EmptyPatch,
    #[error("playback position ticks cannot be negative")]
    NegativePlaybackPosition,
    #[error("play count delta must be positive")]
    InvalidPlayCountDelta,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("user catalog state disappeared during commit")]
    MissingRevision,
    #[error("user data disappeared during commit")]
    MissingUserData,
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

impl PartialEq for UserDataRepositoryError {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::EmptyPatch, Self::EmptyPatch)
                | (
                    Self::NegativePlaybackPosition,
                    Self::NegativePlaybackPosition
                )
                | (Self::InvalidPlayCountDelta, Self::InvalidPlayCountDelta)
                | (Self::MissingRevision, Self::MissingRevision)
                | (Self::MissingUserData, Self::MissingUserData)
        )
    }
}

fn validate_patch(patch: &UserDataPatch) -> Result<(), UserDataRepositoryError> {
    if patch.is_empty() {
        return Err(UserDataRepositoryError::EmptyPatch);
    }
    if patch.playback_position_ticks.is_some_and(|value| value < 0) {
        return Err(UserDataRepositoryError::NegativePlaybackPosition);
    }
    if patch.play_count_delta.is_some_and(|value| value <= 0) {
        return Err(UserDataRepositoryError::InvalidPlayCountDelta);
    }
    Ok(())
}

async fn commit_in_transaction(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    catalog_item_id: CatalogItemId,
    patch: &UserDataPatch,
    now: DateTime<Utc>,
) -> Result<UserDataCommit, UserDataRepositoryError> {
    ensure_revision_row(transaction, user_id, now).await?;
    increment_revision(transaction, user_id, now).await?;
    upsert_user_data(transaction, user_id, catalog_item_id, patch, now).await?;
    let user_revision = read_revision(transaction, user_id)
        .await?
        .ok_or(UserDataRepositoryError::MissingRevision)?;
    let data = read_user_data(transaction, user_id, catalog_item_id)
        .await?
        .ok_or(UserDataRepositoryError::MissingUserData)?;
    Ok(UserDataCommit {
        data,
        user_revision,
    })
}

async fn ensure_revision_row(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<(), DbErr> {
    let statement = Query::insert()
        .into_table(Alias::new("user_catalog_state"))
        .columns([
            Alias::new("id"),
            Alias::new("user_id"),
            Alias::new("revision"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            user_id.as_uuid().into(),
            0_i64.into(),
            now.into(),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("user_id"))
                .do_nothing()
                .to_owned(),
        )
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&statement)).await?;
    Ok(())
}

async fn increment_revision(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    now: DateTime<Utc>,
) -> Result<(), UserDataRepositoryError> {
    let statement = Query::update()
        .table(Alias::new("user_catalog_state"))
        .value(
            Alias::new("revision"),
            Expr::col(Alias::new("revision")).add(1_i64),
        )
        .value(Alias::new("updated_at"), now)
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    let result = transaction.execute(backend.build(&statement)).await?;
    if result.rows_affected() != 1 {
        return Err(UserDataRepositoryError::MissingRevision);
    }
    Ok(())
}

async fn upsert_user_data(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    catalog_item_id: CatalogItemId,
    patch: &UserDataPatch,
    now: DateTime<Utc>,
) -> Result<(), DbErr> {
    let mut conflict = OnConflict::columns([Alias::new("user_id"), Alias::new("catalog_item_id")]);
    if patch.playback_position_ticks.is_some() {
        conflict.update_column(Alias::new("playback_position_ticks"));
    }
    if patch.is_played.is_some() {
        conflict.update_column(Alias::new("is_played"));
    }
    if let Some(delta) = patch.play_count_delta {
        conflict.value(
            Alias::new("play_count"),
            Expr::col((Alias::new("user_data"), Alias::new("play_count"))).add(delta),
        );
    }
    if patch.is_favorite.is_some() {
        conflict.update_column(Alias::new("is_favorite"));
    }
    if patch.last_played_at.is_some() {
        conflict.update_column(Alias::new("last_played_at"));
    }
    conflict.update_column(Alias::new("updated_at"));

    let statement = Query::insert()
        .into_table(Alias::new("user_data"))
        .columns([
            Alias::new("id"),
            Alias::new("user_id"),
            Alias::new("catalog_item_id"),
            Alias::new("playback_position_ticks"),
            Alias::new("is_played"),
            Alias::new("play_count"),
            Alias::new("is_favorite"),
            Alias::new("last_played_at"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            user_id.as_uuid().into(),
            catalog_item_id.as_uuid().into(),
            patch.playback_position_ticks.unwrap_or(0).into(),
            patch.is_played.unwrap_or(false).into(),
            patch.play_count_delta.unwrap_or(0).into(),
            patch.is_favorite.unwrap_or(false).into(),
            patch.last_played_at.into(),
            now.into(),
        ])
        .on_conflict(conflict.clone())
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&statement)).await?;
    Ok(())
}

async fn read_revision(
    connection: &impl ConnectionTrait,
    user_id: UserId,
) -> Result<Option<i64>, UserDataRepositoryError> {
    let statement = Query::select()
        .column(Alias::new("revision"))
        .from(Alias::new("user_catalog_state"))
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&statement))
        .await?
        .map(|row| row.try_get("", "revision").map_err(Into::into))
        .transpose()
}

async fn read_user_data(
    connection: &impl ConnectionTrait,
    user_id: UserId,
    catalog_item_id: CatalogItemId,
) -> Result<Option<UserDataRecord>, UserDataRepositoryError> {
    let statement = Query::select()
        .columns([
            Alias::new("user_id"),
            Alias::new("catalog_item_id"),
            Alias::new("playback_position_ticks"),
            Alias::new("is_played"),
            Alias::new("play_count"),
            Alias::new("is_favorite"),
            Alias::new("last_played_at"),
            Alias::new("updated_at"),
        ])
        .from(Alias::new("user_data"))
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
        .and_where(Expr::col(Alias::new("catalog_item_id")).eq(catalog_item_id.as_uuid()))
        .to_owned();
    let backend = connection.get_database_backend();
    connection
        .query_one(backend.build(&statement))
        .await?
        .map(|row| user_data_from_row(&row))
        .transpose()
}

fn user_data_from_row(row: &QueryResult) -> Result<UserDataRecord, UserDataRepositoryError> {
    Ok(UserDataRecord {
        user_id: UserId::from_uuid(row.try_get("", "user_id")?),
        catalog_item_id: CatalogItemId::from_uuid(row.try_get("", "catalog_item_id")?),
        playback_position_ticks: row.try_get("", "playback_position_ticks")?,
        is_played: row.try_get("", "is_played")?,
        play_count: row.try_get("", "play_count")?,
        is_favorite: row.try_get("", "is_favorite")?,
        last_played_at: row.try_get("", "last_played_at")?,
        updated_at: row.try_get("", "updated_at")?,
    })
}
