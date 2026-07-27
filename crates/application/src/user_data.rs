use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{CatalogItemId, UserId};
use tjxy_db::{
    UserDataCommit, UserDataPatch, UserDataRecord, UserDataRepository, UserDataRepositoryError,
};

pub struct UserDataService {
    database: DatabaseConnection,
}

impl UserDataService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Reads user data only for a visible item and returns protocol defaults when absent.
    ///
    /// # Errors
    ///
    /// Returns [`UserDataServiceError`] for impersonation or persistence failures.
    pub async fn get(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        item_id: CatalogItemId,
    ) -> Result<Option<UserDataRecord>, UserDataServiceError> {
        authorize_user(principal, requested_user)?;
        Ok(UserDataRepository::new(&self.database)
            .get_visible(principal, item_id)
            .await?
            .map(|data| {
                data.unwrap_or(UserDataRecord {
                    user_id: principal,
                    catalog_item_id: item_id,
                    playback_position_ticks: 0,
                    is_played: false,
                    play_count: 0,
                    is_favorite: false,
                    last_played_at: None,
                    updated_at: None,
                })
            }))
    }

    /// Applies one field-level user data patch after checking item visibility.
    ///
    /// # Errors
    ///
    /// Returns [`UserDataServiceError`] for impersonation, invalid patches, or persistence failures.
    pub async fn commit(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        item_id: CatalogItemId,
        patch: UserDataPatch,
    ) -> Result<Option<UserDataCommit>, UserDataServiceError> {
        authorize_user(principal, requested_user)?;
        UserDataRepository::new(&self.database)
            .commit_visible(principal, item_id, patch)
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug, Error)]
pub enum UserDataServiceError {
    #[error("requested user does not match the authenticated principal")]
    UnauthorizedUser,
    #[error("user data persistence failed: {0}")]
    Repository(#[from] UserDataRepositoryError),
}

fn authorize_user(
    principal: UserId,
    requested_user: Option<UserId>,
) -> Result<(), UserDataServiceError> {
    if requested_user.is_some_and(|requested| requested != principal) {
        return Err(UserDataServiceError::UnauthorizedUser);
    }
    Ok(())
}
