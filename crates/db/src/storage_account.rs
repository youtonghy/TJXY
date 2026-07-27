use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr,
    sea_query::{Alias, Expr, Query},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageAccountBinding {
    account: Uuid,
    credential: Uuid,
    provider_drive: String,
}

impl StorageAccountBinding {
    #[must_use]
    pub const fn account_id(&self) -> Uuid {
        self.account
    }

    #[must_use]
    pub const fn credential_id(&self) -> Uuid {
        self.credential
    }

    #[must_use]
    pub fn provider_drive_id(&self) -> &str {
        &self.provider_drive
    }
}

pub struct StorageAccountRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> StorageAccountRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Lists distinct active account/credential/drive bindings for one provider.
    ///
    /// # Errors
    ///
    /// Returns errors for invalid provider input, malformed credential references,
    /// or database failures.
    pub async fn active_provider_bindings(
        &self,
        provider: &str,
    ) -> Result<Vec<StorageAccountBinding>, StorageAccountRepositoryError> {
        if provider.trim().is_empty()
            || provider.len() > 255
            || provider.chars().any(char::is_control)
        {
            return Err(StorageAccountRepositoryError::InvalidProvider);
        }
        let account = Alias::new("binding_account");
        let root = Alias::new("binding_root");
        let relation = Alias::new("binding_relation");
        let object = Alias::new("binding_object");
        let query = Query::select()
            .distinct()
            .expr_as(
                Expr::col((account.clone(), Alias::new("id"))),
                Alias::new("account_id"),
            )
            .expr_as(
                Expr::col((account.clone(), Alias::new("credential_ref"))),
                Alias::new("credential_ref"),
            )
            .expr_as(
                Expr::col((object.clone(), Alias::new("provider_drive_id"))),
                Alias::new("provider_drive_id"),
            )
            .from_as(Alias::new("storage_accounts"), account.clone())
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_roots"),
                root.clone(),
                Expr::col((root.clone(), Alias::new("storage_account_id")))
                    .equals((account.clone(), Alias::new("id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_root_objects"),
                relation.clone(),
                Expr::col((relation.clone(), Alias::new("storage_root_id")))
                    .equals((root, Alias::new("id"))),
            )
            .join_as(
                sea_orm::sea_query::JoinType::InnerJoin,
                Alias::new("storage_objects"),
                object.clone(),
                Expr::col((object.clone(), Alias::new("id")))
                    .equals((relation, Alias::new("storage_object_id"))),
            )
            .and_where(Expr::col((account.clone(), Alias::new("provider"))).eq(provider))
            .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
            .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_all(backend.build(&query))
            .await?
            .iter()
            .map(|row| {
                let credential_ref: String = row.try_get("", "credential_ref")?;
                Ok(StorageAccountBinding {
                    account: row.try_get("", "account_id")?,
                    credential: Uuid::parse_str(&credential_ref)
                        .map_err(|_| StorageAccountRepositoryError::InvalidCredentialReference)?,
                    provider_drive: row.try_get("", "provider_drive_id")?,
                })
            })
            .collect()
    }

    /// Disables one account after its committed runtime binding could not be activated.
    ///
    /// The encrypted credential and durable storage graph are retained for explicit recovery.
    /// Returns `true` only when an active account was fenced by this call.
    ///
    /// # Errors
    ///
    /// Returns a database error without reporting a successful fence.
    pub async fn disable_after_activation_failure(
        &self,
        account_id: Uuid,
    ) -> Result<bool, StorageAccountRepositoryError> {
        let update = Query::update()
            .table(Alias::new("storage_accounts"))
            .value(Alias::new("status"), "Disabled")
            .and_where(Expr::col(Alias::new("id")).eq(account_id))
            .and_where(Expr::col(Alias::new("status")).eq("Active"))
            .to_owned();
        let backend = self.database.get_database_backend();
        Ok(self
            .database
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            == 1)
    }
}

#[derive(Debug, Error)]
pub enum StorageAccountRepositoryError {
    #[error("storage provider identifier is invalid")]
    InvalidProvider,
    #[error("storage account credential reference is not a UUID")]
    InvalidCredentialReference,
    #[error("storage account database operation failed: {0}")]
    Database(#[from] DbErr),
}
