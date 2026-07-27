use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use serde_json::Value;
use thiserror::Error;
use tjxy_common::UserId;
use uuid::Uuid;

const MAX_CLIENT_CHARS: usize = 256;
const MAX_DOCUMENT_BYTES: usize = 64 * 1024;

pub struct DisplayPreferencesRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> DisplayPreferencesRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Reads one exact user, display-id, and client preference document.
    ///
    /// # Errors
    ///
    /// Returns [`DisplayPreferencesRepositoryError`] for an invalid client or database failure.
    pub async fn get(
        &self,
        user_id: UserId,
        display_preferences_id: Uuid,
        client: &str,
    ) -> Result<Option<Value>, DisplayPreferencesRepositoryError> {
        validate_client(client)?;
        let query = Query::select()
            .column(Alias::new("document"))
            .from(Alias::new("display_preferences"))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
            .and_where(Expr::col(Alias::new("display_preferences_id")).eq(display_preferences_id))
            .and_where(Expr::col(Alias::new("client")).eq(client))
            .limit(1)
            .to_owned();
        self.database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .map(|row| row.try_get("", "document").map_err(Into::into))
            .transpose()
    }

    /// Atomically replaces one exact user, display-id, and client preference document.
    ///
    /// # Errors
    ///
    /// Returns [`DisplayPreferencesRepositoryError`] for invalid input or database failure.
    pub async fn replace(
        &self,
        user_id: UserId,
        display_preferences_id: Uuid,
        client: &str,
        document: &Value,
    ) -> Result<(), DisplayPreferencesRepositoryError> {
        validate_client(client)?;
        validate_document(document)?;
        let now = Utc::now();
        let query = Query::insert()
            .into_table(Alias::new("display_preferences"))
            .columns([
                Alias::new("id"),
                Alias::new("user_id"),
                Alias::new("display_preferences_id"),
                Alias::new("client"),
                Alias::new("document"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                user_id.as_uuid().into(),
                display_preferences_id.into(),
                client.into(),
                document.clone().into(),
                now.into(),
                now.into(),
            ])
            .on_conflict(
                OnConflict::columns([
                    Alias::new("user_id"),
                    Alias::new("display_preferences_id"),
                    Alias::new("client"),
                ])
                .update_columns([Alias::new("document"), Alias::new("updated_at")])
                .to_owned(),
            )
            .to_owned();
        self.database
            .execute(self.database.get_database_backend().build(&query))
            .await?;
        Ok(())
    }
}

fn validate_client(client: &str) -> Result<(), DisplayPreferencesRepositoryError> {
    if client.is_empty()
        || client.trim() != client
        || client.chars().count() > MAX_CLIENT_CHARS
        || client.chars().any(char::is_control)
    {
        return Err(DisplayPreferencesRepositoryError::InvalidClient);
    }
    Ok(())
}

fn validate_document(document: &Value) -> Result<(), DisplayPreferencesRepositoryError> {
    if !document.is_object()
        || serde_json::to_vec(document)
            .map_err(|_| DisplayPreferencesRepositoryError::InvalidDocument)?
            .len()
            > MAX_DOCUMENT_BYTES
    {
        return Err(DisplayPreferencesRepositoryError::InvalidDocument);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum DisplayPreferencesRepositoryError {
    #[error("display preferences client is invalid")]
    InvalidClient,
    #[error("display preferences document is invalid")]
    InvalidDocument,
    #[error("display preferences database operation failed: {0}")]
    Database(#[from] DbErr),
}
