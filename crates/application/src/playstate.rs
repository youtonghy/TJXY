use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{CatalogItemId, PresentationKey, UserId};
use tjxy_db::{PlaybackSessionCommit, PlaystateRepository, PlaystateRepositoryError};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackEvent {
    Started,
    Progress,
    Stopped,
}

pub struct PlaystateService {
    database: DatabaseConnection,
}

impl PlaystateService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Applies one authenticated playback event to its durable session.
    ///
    /// # Errors
    ///
    /// Returns [`PlaystateServiceError`] for impersonation or persistence failures.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply(
        &self,
        principal: UserId,
        auth_session_id: Uuid,
        requested_user: Option<UserId>,
        event: PlaybackEvent,
        play_session_id: Uuid,
        item_id: CatalogItemId,
        presentation_key: PresentationKey,
        position_ticks: i64,
    ) -> Result<Option<PlaybackSessionCommit>, PlaystateServiceError> {
        if requested_user.is_some_and(|requested| requested != principal) {
            return Err(PlaystateServiceError::UnauthorizedUser);
        }
        let repository = PlaystateRepository::new(&self.database);
        match event {
            PlaybackEvent::Started => {
                repository
                    .start(
                        auth_session_id,
                        play_session_id,
                        principal,
                        item_id,
                        presentation_key,
                        position_ticks,
                    )
                    .await
            }
            PlaybackEvent::Progress => {
                repository
                    .progress(
                        auth_session_id,
                        play_session_id,
                        principal,
                        item_id,
                        presentation_key,
                        position_ticks,
                    )
                    .await
            }
            PlaybackEvent::Stopped => {
                repository
                    .stop(
                        auth_session_id,
                        play_session_id,
                        principal,
                        item_id,
                        presentation_key,
                        position_ticks,
                    )
                    .await
            }
        }
        .map_err(Into::into)
    }

    /// Refreshes one active session without changing user catalog state.
    ///
    /// # Errors
    ///
    /// Returns [`PlaystateServiceError`] when persistence fails.
    pub async fn ping(
        &self,
        auth_session_id: Uuid,
        play_session_id: Uuid,
    ) -> Result<bool, PlaystateServiceError> {
        PlaystateRepository::new(&self.database)
            .ping(auth_session_id, play_session_id)
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug, Error)]
pub enum PlaystateServiceError {
    #[error("requested user does not match the authenticated principal")]
    UnauthorizedUser,
    #[error("playstate persistence failed: {0}")]
    Repository(#[from] PlaystateRepositoryError),
}
