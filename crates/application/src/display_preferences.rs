use sea_orm::DatabaseConnection;
use serde_json::Value;
use thiserror::Error;
use tjxy_common::UserId;
use tjxy_db::{DisplayPreferencesRepository, DisplayPreferencesRepositoryError};
use uuid::Uuid;

pub struct DisplayPreferencesService {
    database: DatabaseConnection,
}

impl DisplayPreferencesService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Reads one preference document owned by the authenticated user.
    ///
    /// # Errors
    ///
    /// Returns [`DisplayPreferencesServiceError`] for impersonation or persistence failures.
    pub async fn get(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        display_preferences_id: Uuid,
        client: &str,
    ) -> Result<Option<Value>, DisplayPreferencesServiceError> {
        authorize_user(principal, requested_user)?;
        DisplayPreferencesRepository::new(&self.database)
            .get(principal, display_preferences_id, client)
            .await
            .map_err(Into::into)
    }

    /// Atomically replaces one preference document owned by the authenticated user.
    ///
    /// # Errors
    ///
    /// Returns [`DisplayPreferencesServiceError`] for impersonation, invalid input, or
    /// persistence failures.
    pub async fn replace(
        &self,
        principal: UserId,
        requested_user: Option<UserId>,
        display_preferences_id: Uuid,
        client: &str,
        document: &Value,
    ) -> Result<(), DisplayPreferencesServiceError> {
        authorize_user(principal, requested_user)?;
        DisplayPreferencesRepository::new(&self.database)
            .replace(principal, display_preferences_id, client, document)
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug, Error)]
pub enum DisplayPreferencesServiceError {
    #[error("requested user does not match the authenticated principal")]
    UnauthorizedUser,
    #[error("display preferences persistence failed: {0}")]
    Repository(#[from] DisplayPreferencesRepositoryError),
}

fn authorize_user(
    principal: UserId,
    requested_user: Option<UserId>,
) -> Result<(), DisplayPreferencesServiceError> {
    if requested_user.is_some_and(|requested| requested != principal) {
        return Err(DisplayPreferencesServiceError::UnauthorizedUser);
    }
    Ok(())
}
