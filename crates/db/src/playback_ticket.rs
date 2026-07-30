use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, Query},
};
use thiserror::Error;
use tjxy_common::{CatalogItemId, PresentationKey, UserId};
use uuid::Uuid;

const MAX_ACTIVE_TICKETS_PER_SESSION: i64 = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackTicketDraft {
    pub id: Uuid,
    pub auth_session_id: Uuid,
    pub user_id: UserId,
    pub item_id: CatalogItemId,
    pub media_source_id: PresentationKey,
    pub play_session_id: Uuid,
    pub token_digest: [u8; 32],
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackTicketGrant {
    ticket_id: Uuid,
    auth_session_id: Uuid,
    user_id: UserId,
    item_id: CatalogItemId,
    media_source_id: PresentationKey,
    play_session_id: Uuid,
}

impl PlaybackTicketGrant {
    #[must_use]
    pub const fn ticket_id(&self) -> Uuid {
        self.ticket_id
    }

    #[must_use]
    pub const fn auth_session_id(&self) -> Uuid {
        self.auth_session_id
    }

    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[must_use]
    pub const fn item_id(&self) -> CatalogItemId {
        self.item_id
    }

    #[must_use]
    pub const fn media_source_id(&self) -> PresentationKey {
        self.media_source_id
    }

    #[must_use]
    pub const fn play_session_id(&self) -> Uuid {
        self.play_session_id
    }
}

pub struct PlaybackTicketRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> PlaybackTicketRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    pub async fn issue(
        &self,
        draft: PlaybackTicketDraft,
    ) -> Result<DateTime<Utc>, PlaybackTicketRepositoryError> {
        if draft.expires_at <= draft.created_at {
            return Err(PlaybackTicketRepositoryError::InvalidDraft);
        }
        let transaction = self.database.begin().await?;
        let result = issue_on(&transaction, &draft).await;
        finish(transaction, result).await
    }

    pub async fn authorize(
        &self,
        token_digest: &[u8; 32],
        now: DateTime<Utc>,
        item_id: CatalogItemId,
        media_source_id: PresentationKey,
    ) -> Result<Option<PlaybackTicketGrant>, PlaybackTicketRepositoryError> {
        let ticket = Alias::new("ticket");
        let session = Alias::new("session");
        let user = Alias::new("ticket_user");
        let query = Query::select()
            .expr_as(
                Expr::col((ticket.clone(), Alias::new("id"))),
                Alias::new("ticket_id"),
            )
            .expr_as(
                Expr::col((ticket.clone(), Alias::new("auth_session_id"))),
                Alias::new("auth_session_id"),
            )
            .expr_as(
                Expr::col((ticket.clone(), Alias::new("user_id"))),
                Alias::new("user_id"),
            )
            .expr_as(
                Expr::col((ticket.clone(), Alias::new("item_id"))),
                Alias::new("item_id"),
            )
            .expr_as(
                Expr::col((ticket.clone(), Alias::new("media_source_id"))),
                Alias::new("media_source_id"),
            )
            .expr_as(
                Expr::col((ticket.clone(), Alias::new("play_session_id"))),
                Alias::new("play_session_id"),
            )
            .from_as(Alias::new("playback_tickets"), ticket.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("auth_sessions"),
                session.clone(),
                Expr::col((session.clone(), Alias::new("id")))
                    .equals((ticket.clone(), Alias::new("auth_session_id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("users"),
                user.clone(),
                Expr::col((user.clone(), Alias::new("id")))
                    .equals((ticket.clone(), Alias::new("user_id"))),
            )
            .and_where(
                Expr::col((ticket.clone(), Alias::new("token_digest"))).eq(token_digest.to_vec()),
            )
            .and_where(Expr::col((ticket.clone(), Alias::new("item_id"))).eq(item_id.as_uuid()))
            .and_where(
                Expr::col((ticket.clone(), Alias::new("media_source_id")))
                    .eq(media_source_id.as_uuid()),
            )
            .and_where(Expr::col((ticket.clone(), Alias::new("revoked_at"))).is_null())
            .and_where(Expr::col((ticket.clone(), Alias::new("expires_at"))).gt(now))
            .and_where(
                Expr::col((session.clone(), Alias::new("user_id")))
                    .equals((ticket.clone(), Alias::new("user_id"))),
            )
            .and_where(Expr::col((session.clone(), Alias::new("revoked_at"))).is_null())
            .cond_where(
                Cond::any()
                    .add(Expr::col((session.clone(), Alias::new("expires_at"))).is_null())
                    .add(Expr::col((session.clone(), Alias::new("expires_at"))).gt(now)),
            )
            .and_where(
                Expr::col((session.clone(), Alias::new("auth_revision")))
                    .equals((user.clone(), Alias::new("auth_revision"))),
            )
            .and_where(Expr::col((user, Alias::new("disabled_at"))).is_null())
            .limit(1)
            .to_owned();
        self.database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .map(grant_from_row)
            .transpose()
    }

    pub async fn revoke(
        &self,
        auth_session_id: Uuid,
        ticket_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<bool, PlaybackTicketRepositoryError> {
        let update = Query::update()
            .table(Alias::new("playback_tickets"))
            .value(Alias::new("revoked_at"), now)
            .and_where(Expr::col(Alias::new("id")).eq(ticket_id))
            .and_where(Expr::col(Alias::new("auth_session_id")).eq(auth_session_id))
            .and_where(Expr::col(Alias::new("revoked_at")).is_null())
            .to_owned();
        Ok(self
            .database
            .execute(self.database.get_database_backend().build(&update))
            .await?
            .rows_affected()
            == 1)
    }
}

async fn issue_on(
    transaction: &DatabaseTransaction,
    draft: &PlaybackTicketDraft,
) -> Result<DateTime<Utc>, PlaybackTicketRepositoryError> {
    let lock = Query::update()
        .table(Alias::new("auth_sessions"))
        .value(
            Alias::new("last_seen_at"),
            Expr::col(Alias::new("last_seen_at")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(draft.auth_session_id))
        .and_where(Expr::col(Alias::new("user_id")).eq(draft.user_id.as_uuid()))
        .and_where(Expr::col(Alias::new("revoked_at")).is_null())
        .cond_where(
            Cond::any()
                .add(Expr::col(Alias::new("expires_at")).is_null())
                .add(Expr::col(Alias::new("expires_at")).gt(draft.created_at)),
        )
        .to_owned();
    if transaction
        .execute(transaction.get_database_backend().build(&lock))
        .await?
        .rows_affected()
        != 1
    {
        return Err(PlaybackTicketRepositoryError::SessionRejected);
    }

    let session_expiry_query = Query::select()
        .column(Alias::new("expires_at"))
        .from(Alias::new("auth_sessions"))
        .and_where(Expr::col(Alias::new("id")).eq(draft.auth_session_id))
        .limit(1)
        .to_owned();
    let session_expiry = transaction
        .query_one(
            transaction
                .get_database_backend()
                .build(&session_expiry_query),
        )
        .await?
        .ok_or(PlaybackTicketRepositoryError::SessionRejected)?
        .try_get::<Option<DateTime<Utc>>>("", "expires_at")?;
    let actual_expiry = session_expiry.map_or(draft.expires_at, |expires_at| {
        expires_at.min(draft.expires_at)
    });

    let count = Query::select()
        .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
        .from(Alias::new("playback_tickets"))
        .and_where(Expr::col(Alias::new("auth_session_id")).eq(draft.auth_session_id))
        .and_where(Expr::col(Alias::new("revoked_at")).is_null())
        .and_where(Expr::col(Alias::new("expires_at")).gt(draft.created_at))
        .to_owned();
    let active: i64 = transaction
        .query_one(transaction.get_database_backend().build(&count))
        .await?
        .ok_or(PlaybackTicketRepositoryError::MissingCount)?
        .try_get("", "count")?;
    if active >= MAX_ACTIVE_TICKETS_PER_SESSION {
        return Err(PlaybackTicketRepositoryError::CapacityReached);
    }

    let insert = Query::insert()
        .into_table(Alias::new("playback_tickets"))
        .columns([
            Alias::new("id"),
            Alias::new("auth_session_id"),
            Alias::new("user_id"),
            Alias::new("item_id"),
            Alias::new("media_source_id"),
            Alias::new("play_session_id"),
            Alias::new("token_digest"),
            Alias::new("expires_at"),
            Alias::new("revoked_at"),
            Alias::new("created_at"),
        ])
        .values_panic([
            draft.id.into(),
            draft.auth_session_id.into(),
            draft.user_id.as_uuid().into(),
            draft.item_id.as_uuid().into(),
            draft.media_source_id.as_uuid().into(),
            draft.play_session_id.into(),
            draft.token_digest.to_vec().into(),
            actual_expiry.into(),
            Option::<DateTime<Utc>>::None.into(),
            draft.created_at.into(),
        ])
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&insert))
        .await?;
    Ok(actual_expiry)
}

fn grant_from_row(row: QueryResult) -> Result<PlaybackTicketGrant, PlaybackTicketRepositoryError> {
    Ok(PlaybackTicketGrant {
        ticket_id: row.try_get("", "ticket_id")?,
        auth_session_id: row.try_get("", "auth_session_id")?,
        user_id: UserId::from_uuid(row.try_get("", "user_id")?),
        item_id: CatalogItemId::from_uuid(row.try_get("", "item_id")?),
        media_source_id: PresentationKey::from_uuid(row.try_get("", "media_source_id")?),
        play_session_id: row.try_get("", "play_session_id")?,
    })
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, PlaybackTicketRepositoryError>,
) -> Result<T, PlaybackTicketRepositoryError> {
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
pub enum PlaybackTicketRepositoryError {
    #[error("invalid playback ticket draft")]
    InvalidDraft,
    #[error("playback ticket login session was rejected")]
    SessionRejected,
    #[error("playback ticket capacity reached")]
    CapacityReached,
    #[error("playback ticket count row is missing")]
    MissingCount,
    #[error(transparent)]
    Database(#[from] DbErr),
}
