use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use thiserror::Error;
use tjxy_common::{CatalogItemId, PresentationKey, UserId};
use uuid::Uuid;

use crate::{
    catalog_publication::CatalogPublicationError,
    catalog_query::lock_catalog_item_visibility,
    source_publication::active_presentation_exists,
    user_data::{UserDataCommit, UserDataPatch, UserDataRepositoryError, commit_in_transaction},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackSessionCommit {
    user_data: Option<UserDataCommit>,
    replayed: bool,
}

impl PlaybackSessionCommit {
    #[must_use]
    pub const fn user_data(&self) -> Option<&UserDataCommit> {
        self.user_data.as_ref()
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

pub struct PlaystateRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> PlaystateRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Returns the most recently used source for one user-visible catalog item.
    ///
    /// # Errors
    ///
    /// Returns [`PlaystateRepositoryError`] when the persistence query fails.
    pub async fn last_presentation_key(
        &self,
        user_id: UserId,
        item_id: CatalogItemId,
    ) -> Result<Option<PresentationKey>, PlaystateRepositoryError> {
        let query = Query::select()
            .column(Alias::new("presentation_key"))
            .from(Alias::new("playback_sessions"))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
            .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid()))
            .order_by(Alias::new("started_at"), sea_orm::sea_query::Order::Desc)
            .order_by(Alias::new("id"), sea_orm::sea_query::Order::Desc)
            .limit(1)
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_one(backend.build(&query))
            .await?
            .map(|row| {
                row.try_get("", "presentation_key")
                    .map(PresentationKey::from_uuid)
            })
            .transpose()
            .map_err(Into::into)
    }

    /// Starts one durable playback session and increments `PlayCount` exactly once.
    ///
    /// # Errors
    ///
    /// Returns [`PlaystateRepositoryError`] for invalid identity, position, or persistence.
    pub async fn start(
        &self,
        auth_session_id: Uuid,
        play_session_id: Uuid,
        user_id: UserId,
        item_id: CatalogItemId,
        presentation_key: PresentationKey,
        position_ticks: i64,
    ) -> Result<Option<PlaybackSessionCommit>, PlaystateRepositoryError> {
        validate_position(position_ticks)?;
        let transaction = self.database.begin().await?;
        let result = start_in_transaction(
            &transaction,
            auth_session_id,
            play_session_id,
            user_id,
            item_id,
            presentation_key,
            position_ticks,
            Utc::now(),
        )
        .await;
        finish(transaction, result).await
    }

    /// Persists a changed playback position for an active durable session.
    ///
    /// # Errors
    ///
    /// Returns [`PlaystateRepositoryError`] for invalid or stopped sessions and persistence errors.
    pub async fn progress(
        &self,
        auth_session_id: Uuid,
        play_session_id: Uuid,
        user_id: UserId,
        item_id: CatalogItemId,
        presentation_key: PresentationKey,
        position_ticks: i64,
    ) -> Result<Option<PlaybackSessionCommit>, PlaystateRepositoryError> {
        self.advance(
            auth_session_id,
            play_session_id,
            user_id,
            item_id,
            presentation_key,
            position_ticks,
            false,
        )
        .await
    }

    /// Stops a playback session once and persists its final changed position.
    ///
    /// # Errors
    ///
    /// Returns [`PlaystateRepositoryError`] for invalid session identity or persistence errors.
    pub async fn stop(
        &self,
        auth_session_id: Uuid,
        play_session_id: Uuid,
        user_id: UserId,
        item_id: CatalogItemId,
        presentation_key: PresentationKey,
        position_ticks: i64,
    ) -> Result<Option<PlaybackSessionCommit>, PlaystateRepositoryError> {
        self.advance(
            auth_session_id,
            play_session_id,
            user_id,
            item_id,
            presentation_key,
            position_ticks,
            true,
        )
        .await
    }

    /// Refreshes one active playback session without changing `UserData` revision.
    ///
    /// # Errors
    ///
    /// Returns [`PlaystateRepositoryError`] when the update fails.
    pub async fn ping(
        &self,
        auth_session_id: Uuid,
        play_session_id: Uuid,
    ) -> Result<bool, PlaystateRepositoryError> {
        let update = Query::update()
            .table(Alias::new("playback_sessions"))
            .value(Alias::new("last_event_at"), Utc::now())
            .and_where(Expr::col(Alias::new("auth_session_id")).eq(auth_session_id))
            .and_where(Expr::col(Alias::new("play_session_id")).eq(play_session_id))
            .and_where(Expr::col(Alias::new("stopped_at")).is_null())
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .execute(backend.build(&update))
            .await
            .map(|result| result.rows_affected() == 1)
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    async fn advance(
        &self,
        auth_session_id: Uuid,
        play_session_id: Uuid,
        user_id: UserId,
        item_id: CatalogItemId,
        presentation_key: PresentationKey,
        position_ticks: i64,
        stop: bool,
    ) -> Result<Option<PlaybackSessionCommit>, PlaystateRepositoryError> {
        validate_position(position_ticks)?;
        let transaction = self.database.begin().await?;
        let result = advance_in_transaction(
            &transaction,
            auth_session_id,
            play_session_id,
            user_id,
            item_id,
            presentation_key,
            position_ticks,
            stop,
            Utc::now(),
        )
        .await;
        finish(transaction, result).await
    }
}

#[derive(Debug, Error)]
pub enum PlaystateRepositoryError {
    #[error("position ticks cannot be negative")]
    NegativePosition,
    #[error("media source is not active for the requested item")]
    InvalidPresentation,
    #[error("playback session does not exist")]
    MissingSession,
    #[error("playback session identity does not match the request")]
    SessionIdentityMismatch,
    #[error("playback session is already stopped")]
    SessionStopped,
    #[error("catalog publication failed: {0}")]
    Publication(#[from] CatalogPublicationError),
    #[error("user data failed: {0}")]
    UserData(#[from] UserDataRepositoryError),
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

#[allow(clippy::too_many_arguments)]
async fn start_in_transaction(
    transaction: &DatabaseTransaction,
    auth_session_id: Uuid,
    play_session_id: Uuid,
    user_id: UserId,
    item_id: CatalogItemId,
    presentation_key: PresentationKey,
    position_ticks: i64,
    now: DateTime<Utc>,
) -> Result<Option<PlaybackSessionCommit>, PlaystateRepositoryError> {
    if !lock_catalog_item_visibility(transaction, item_id).await? {
        return Ok(None);
    }
    if !active_presentation_exists(transaction, item_id, presentation_key).await? {
        return Err(PlaystateRepositoryError::InvalidPresentation);
    }
    let proposed_id = Uuid::new_v4();
    let backend = transaction.get_database_backend();
    let conflict = if backend == sea_orm::DbBackend::MySql {
        OnConflict::new()
            .update_column(Alias::new("play_session_id"))
            .to_owned()
    } else {
        OnConflict::columns([Alias::new("auth_session_id"), Alias::new("play_session_id")])
            .do_nothing()
            .to_owned()
    };
    let insert = Query::insert()
        .into_table(Alias::new("playback_sessions"))
        .columns([
            Alias::new("id"),
            Alias::new("auth_session_id"),
            Alias::new("play_session_id"),
            Alias::new("user_id"),
            Alias::new("catalog_item_id"),
            Alias::new("presentation_key"),
            Alias::new("last_position_ticks"),
            Alias::new("started_at"),
            Alias::new("last_event_at"),
        ])
        .values_panic([
            proposed_id.into(),
            auth_session_id.into(),
            play_session_id.into(),
            user_id.as_uuid().into(),
            item_id.as_uuid().into(),
            presentation_key.as_uuid().into(),
            position_ticks.into(),
            now.into(),
            now.into(),
        ])
        .on_conflict(conflict)
        .to_owned();
    transaction.execute(backend.build(&insert)).await?;
    lock_session(transaction, auth_session_id, play_session_id).await?;
    let session = load_session(transaction, auth_session_id, play_session_id).await?;
    if session.id != proposed_id {
        validate_identity(&session, user_id, item_id, presentation_key)?;
        return Ok(Some(PlaybackSessionCommit {
            user_data: None,
            replayed: true,
        }));
    }
    let patch = UserDataPatch::default()
        .with_playback_position_ticks(position_ticks)
        .with_play_count_delta(1)
        .with_last_played_at(now);
    let user_data = commit_in_transaction(transaction, user_id, item_id, &patch, now).await?;
    Ok(Some(PlaybackSessionCommit {
        user_data: Some(user_data),
        replayed: false,
    }))
}

#[allow(clippy::too_many_arguments)]
async fn advance_in_transaction(
    transaction: &DatabaseTransaction,
    auth_session_id: Uuid,
    play_session_id: Uuid,
    user_id: UserId,
    item_id: CatalogItemId,
    presentation_key: PresentationKey,
    position_ticks: i64,
    stop: bool,
    now: DateTime<Utc>,
) -> Result<Option<PlaybackSessionCommit>, PlaystateRepositoryError> {
    if !lock_catalog_item_visibility(transaction, item_id).await? {
        return Ok(None);
    }
    lock_session(transaction, auth_session_id, play_session_id).await?;
    let session = load_session(transaction, auth_session_id, play_session_id).await?;
    validate_identity(&session, user_id, item_id, presentation_key)?;
    if session.stopped_at.is_some() {
        if stop {
            return Ok(Some(PlaybackSessionCommit {
                user_data: None,
                replayed: true,
            }));
        }
        return Err(PlaystateRepositoryError::SessionStopped);
    }
    let changed = session.last_position_ticks != position_ticks;
    let mut update = Query::update()
        .table(Alias::new("playback_sessions"))
        .value(Alias::new("last_position_ticks"), position_ticks)
        .value(Alias::new("last_event_at"), now)
        .and_where(Expr::col(Alias::new("auth_session_id")).eq(auth_session_id))
        .and_where(Expr::col(Alias::new("play_session_id")).eq(play_session_id))
        .to_owned();
    if stop {
        update.value(Alias::new("stopped_at"), now);
    }
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&update)).await?;
    let user_data = if changed {
        let patch = UserDataPatch::default().with_playback_position_ticks(position_ticks);
        Some(commit_in_transaction(transaction, user_id, item_id, &patch, now).await?)
    } else {
        None
    };
    Ok(Some(PlaybackSessionCommit {
        user_data,
        replayed: !changed && stop,
    }))
}

async fn lock_session(
    transaction: &DatabaseTransaction,
    auth_session_id: Uuid,
    play_session_id: Uuid,
) -> Result<(), PlaystateRepositoryError> {
    let update = Query::update()
        .table(Alias::new("playback_sessions"))
        .value(
            Alias::new("last_event_at"),
            Expr::col(Alias::new("last_event_at")),
        )
        .and_where(Expr::col(Alias::new("auth_session_id")).eq(auth_session_id))
        .and_where(Expr::col(Alias::new("play_session_id")).eq(play_session_id))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(PlaystateRepositoryError::MissingSession);
    }
    Ok(())
}

async fn load_session(
    transaction: &DatabaseTransaction,
    auth_session_id: Uuid,
    play_session_id: Uuid,
) -> Result<PlaybackSessionRow, PlaystateRepositoryError> {
    let query = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("user_id"),
            Alias::new("catalog_item_id"),
            Alias::new("presentation_key"),
            Alias::new("last_position_ticks"),
            Alias::new("stopped_at"),
        ])
        .from(Alias::new("playback_sessions"))
        .and_where(Expr::col(Alias::new("auth_session_id")).eq(auth_session_id))
        .and_where(Expr::col(Alias::new("play_session_id")).eq(play_session_id))
        .to_owned();
    let backend = transaction.get_database_backend();
    let row = transaction
        .query_one(backend.build(&query))
        .await?
        .ok_or(PlaystateRepositoryError::MissingSession)?;
    PlaybackSessionRow::from_row(&row)
}

struct PlaybackSessionRow {
    id: Uuid,
    user_id: UserId,
    item_id: CatalogItemId,
    presentation_key: PresentationKey,
    last_position_ticks: i64,
    stopped_at: Option<DateTime<Utc>>,
}

impl PlaybackSessionRow {
    fn from_row(row: &QueryResult) -> Result<Self, PlaystateRepositoryError> {
        Ok(Self {
            id: row.try_get("", "id")?,
            user_id: UserId::from_uuid(row.try_get("", "user_id")?),
            item_id: CatalogItemId::from_uuid(row.try_get("", "catalog_item_id")?),
            presentation_key: PresentationKey::from_uuid(row.try_get("", "presentation_key")?),
            last_position_ticks: row.try_get("", "last_position_ticks")?,
            stopped_at: row.try_get("", "stopped_at")?,
        })
    }
}

fn validate_identity(
    session: &PlaybackSessionRow,
    user_id: UserId,
    item_id: CatalogItemId,
    presentation_key: PresentationKey,
) -> Result<(), PlaystateRepositoryError> {
    if session.user_id != user_id
        || session.item_id != item_id
        || session.presentation_key != presentation_key
    {
        return Err(PlaystateRepositoryError::SessionIdentityMismatch);
    }
    Ok(())
}

fn validate_position(position_ticks: i64) -> Result<(), PlaystateRepositoryError> {
    if position_ticks < 0 {
        return Err(PlaystateRepositoryError::NegativePosition);
    }
    Ok(())
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, PlaystateRepositoryError>,
) -> Result<T, PlaystateRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(PlaystateRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
