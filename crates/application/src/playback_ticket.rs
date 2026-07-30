use std::fmt;

use chrono::{DateTime, Utc};
use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{CatalogItemId, PresentationKey};
use tjxy_db::{
    AuthenticatedPrincipal, PlaybackTicketDraft, PlaybackTicketGrant, PlaybackTicketRepository,
    PlaybackTicketRepositoryError,
};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    AuthClock,
    auth::{digest_token, generate_token},
};

const MAX_TICKET_LIFETIME: chrono::Duration = chrono::Duration::hours(6);

pub struct SecretPlaybackTicket(Zeroizing<String>);

impl SecretPlaybackTicket {
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretPlaybackTicket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretPlaybackTicket([REDACTED])")
    }
}

pub struct IssuedPlaybackTicket {
    id: Uuid,
    secret: SecretPlaybackTicket,
    expires_at: DateTime<Utc>,
}

impl IssuedPlaybackTicket {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn secret(&self) -> &SecretPlaybackTicket {
        &self.secret
    }

    #[must_use]
    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

pub struct PlaybackTicketService<Clock> {
    database: DatabaseConnection,
    clock: Clock,
}

impl<Clock> PlaybackTicketService<Clock>
where
    Clock: AuthClock,
{
    #[must_use]
    pub const fn new(database: DatabaseConnection, clock: Clock) -> Self {
        Self { database, clock }
    }

    pub async fn issue(
        &self,
        principal: &AuthenticatedPrincipal,
        item_id: CatalogItemId,
        media_source_id: PresentationKey,
        play_session_id: Uuid,
    ) -> Result<IssuedPlaybackTicket, PlaybackTicketServiceError> {
        let auth_session_id = principal
            .session_id()
            .ok_or(PlaybackTicketServiceError::SessionRequired)?;
        let now = self.clock.now();
        let secret = SecretPlaybackTicket(Zeroizing::new(generate_token()));
        let id = Uuid::new_v4();
        let expires_at = PlaybackTicketRepository::new(&self.database)
            .issue(PlaybackTicketDraft {
                id,
                auth_session_id,
                user_id: principal.user().id(),
                item_id,
                media_source_id,
                play_session_id,
                token_digest: digest_token(secret.expose_secret()),
                expires_at: now + MAX_TICKET_LIFETIME,
                created_at: now,
            })
            .await
            .map_err(map_repository_error)?;
        Ok(IssuedPlaybackTicket {
            id,
            secret,
            expires_at,
        })
    }

    pub async fn authorize(
        &self,
        raw_ticket: &str,
        item_id: CatalogItemId,
        media_source_id: PresentationKey,
    ) -> Result<Option<PlaybackTicketGrant>, PlaybackTicketServiceError> {
        if !valid_raw_ticket(raw_ticket) {
            return Err(PlaybackTicketServiceError::InvalidTicket);
        }
        PlaybackTicketRepository::new(&self.database)
            .authorize(
                &digest_token(raw_ticket),
                self.clock.now(),
                item_id,
                media_source_id,
            )
            .await
            .map_err(map_repository_error)
    }

    pub async fn revoke(
        &self,
        principal: &AuthenticatedPrincipal,
        ticket_id: Uuid,
    ) -> Result<bool, PlaybackTicketServiceError> {
        let auth_session_id = principal
            .session_id()
            .ok_or(PlaybackTicketServiceError::SessionRequired)?;
        PlaybackTicketRepository::new(&self.database)
            .revoke(auth_session_id, ticket_id, self.clock.now())
            .await
            .map_err(map_repository_error)
    }
}

fn valid_raw_ticket(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn map_repository_error(error: PlaybackTicketRepositoryError) -> PlaybackTicketServiceError {
    match error {
        PlaybackTicketRepositoryError::CapacityReached => PlaybackTicketServiceError::Capacity,
        PlaybackTicketRepositoryError::SessionRejected => {
            PlaybackTicketServiceError::SessionRequired
        }
        other => PlaybackTicketServiceError::Repository(other),
    }
}

#[derive(Debug, Error)]
pub enum PlaybackTicketServiceError {
    #[error("a login session is required for playback tickets")]
    SessionRequired,
    #[error("invalid playback ticket")]
    InvalidTicket,
    #[error("playback ticket capacity reached")]
    Capacity,
    #[error(transparent)]
    Repository(PlaybackTicketRepositoryError),
}
