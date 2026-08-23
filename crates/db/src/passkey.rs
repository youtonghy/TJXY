use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PasskeyCredential {
    pub id: Uuid,
    pub user_id: Uuid,
    pub credential_id: String,
    pub public_key: Vec<u8>,
    pub counter: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PasskeyChallenge {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub kind: String,
    pub state: Vec<u8>,
    pub expires_at: DateTime<Utc>,
}

pub struct PasskeyRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> PasskeyRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Lists credentials owned by a user.
    ///
    /// # Errors
    /// Returns a database error when the query or row decoding fails.
    pub async fn list(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<PasskeyCredential>, PasskeyRepositoryError> {
        let query = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("user_id"),
                Alias::new("credential_id"),
                Alias::new("public_key"),
                Alias::new("counter"),
                Alias::new("name"),
                Alias::new("created_at"),
                Alias::new("last_used_at"),
            ])
            .from(Alias::new("passkey_credentials"))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id))
            .to_owned();
        let rows = self
            .database
            .query_all(self.database.get_database_backend().build(&query))
            .await?;
        rows.iter().map(row_credential).collect()
    }

    /// Persists a newly verified credential.
    ///
    /// # Errors
    /// Returns a database error when the insert fails.
    pub async fn insert(
        &self,
        credential: &PasskeyCredential,
    ) -> Result<(), PasskeyRepositoryError> {
        let query = Query::insert()
            .into_table(Alias::new("passkey_credentials"))
            .columns([
                Alias::new("id"),
                Alias::new("user_id"),
                Alias::new("credential_id"),
                Alias::new("public_key"),
                Alias::new("counter"),
                Alias::new("name"),
                Alias::new("created_at"),
                Alias::new("last_used_at"),
            ])
            .values_panic([
                credential.id.into(),
                credential.user_id.into(),
                credential.credential_id.clone().into(),
                credential.public_key.clone().into(),
                credential.counter.into(),
                credential.name.clone().into(),
                credential.created_at.into(),
                credential.last_used_at.into(),
            ])
            .to_owned();
        self.database
            .execute(self.database.get_database_backend().build(&query))
            .await?;
        Ok(())
    }

    /// Finds a credential globally by its authenticator identifier.
    ///
    /// # Errors
    /// Returns a database error when the query or row decoding fails.
    pub async fn find_by_credential_id(
        &self,
        credential_id: &str,
    ) -> Result<Option<PasskeyCredential>, PasskeyRepositoryError> {
        let query = Query::select()
            .columns([
                Alias::new("id"),
                Alias::new("user_id"),
                Alias::new("credential_id"),
                Alias::new("public_key"),
                Alias::new("counter"),
                Alias::new("name"),
                Alias::new("created_at"),
                Alias::new("last_used_at"),
            ])
            .from(Alias::new("passkey_credentials"))
            .and_where(Expr::col(Alias::new("credential_id")).eq(credential_id))
            .to_owned();
        self.database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
            .as_ref()
            .map(row_credential)
            .transpose()
    }

    /// Updates credential state after a successful authentication.
    ///
    /// # Errors
    /// Returns a database error when the update fails.
    pub async fn update_payload(
        &self,
        id: Uuid,
        payload: Vec<u8>,
        counter: i64,
        used_at: DateTime<Utc>,
    ) -> Result<(), PasskeyRepositoryError> {
        let query = Query::update()
            .table(Alias::new("passkey_credentials"))
            .values([
                (Alias::new("public_key"), payload.into()),
                (Alias::new("counter"), counter.into()),
                (Alias::new("last_used_at"), used_at.into()),
            ])
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        self.database
            .execute(self.database.get_database_backend().build(&query))
            .await?;
        Ok(())
    }

    /// Deletes a credential only when it belongs to the supplied user.
    ///
    /// # Errors
    /// Returns a database error when the delete fails.
    pub async fn delete(&self, user_id: Uuid, id: Uuid) -> Result<bool, PasskeyRepositoryError> {
        let query = Query::delete()
            .from_table(Alias::new("passkey_credentials"))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id))
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        Ok(self
            .database
            .execute(self.database.get_database_backend().build(&query))
            .await?
            .rows_affected()
            == 1)
    }

    /// Stores short-lived server-side WebAuthn ceremony state.
    ///
    /// # Errors
    /// Returns a database error when cleanup or insertion fails.
    pub async fn put_challenge(
        &self,
        challenge: &PasskeyChallenge,
        now: DateTime<Utc>,
    ) -> Result<(), PasskeyRepositoryError> {
        let cleanup = Query::delete()
            .from_table(Alias::new("passkey_challenges"))
            .and_where(Expr::col(Alias::new("expires_at")).lt(now))
            .to_owned();
        self.database
            .execute(self.database.get_database_backend().build(&cleanup))
            .await?;
        let query = Query::insert()
            .into_table(Alias::new("passkey_challenges"))
            .columns([
                Alias::new("id"),
                Alias::new("user_id"),
                Alias::new("kind"),
                Alias::new("state"),
                Alias::new("created_at"),
                Alias::new("expires_at"),
            ])
            .values_panic([
                challenge.id.into(),
                challenge.user_id.into(),
                challenge.kind.clone().into(),
                challenge.state.clone().into(),
                now.into(),
                challenge.expires_at.into(),
            ])
            .to_owned();
        self.database
            .execute(self.database.get_database_backend().build(&query))
            .await?;
        Ok(())
    }

    /// Atomically consumes a challenge and rejects expired state.
    ///
    /// # Errors
    /// Returns a database error when selection, deletion, or transaction finalization fails.
    pub async fn take_challenge(
        &self,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<PasskeyChallenge>, PasskeyRepositoryError> {
        let transaction = self.database.begin().await?;
        let query = Query::select()
            .columns([
                Alias::new("user_id"),
                Alias::new("kind"),
                Alias::new("state"),
                Alias::new("expires_at"),
            ])
            .from(Alias::new("passkey_challenges"))
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        let Some(row) = transaction
            .query_one(transaction.get_database_backend().build(&query))
            .await?
        else {
            transaction.rollback().await?;
            return Ok(None);
        };
        let challenge = PasskeyChallenge {
            id,
            user_id: row.try_get("", "user_id")?,
            kind: row.try_get("", "kind")?,
            state: row.try_get("", "state")?,
            expires_at: row.try_get("", "expires_at")?,
        };
        let delete = Query::delete()
            .from_table(Alias::new("passkey_challenges"))
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        if transaction
            .execute(transaction.get_database_backend().build(&delete))
            .await?
            .rows_affected()
            != 1
        {
            transaction.rollback().await?;
            return Ok(None);
        }
        transaction.commit().await?;
        Ok((challenge.expires_at > now).then_some(challenge))
    }
}

fn row_credential(row: &QueryResult) -> Result<PasskeyCredential, PasskeyRepositoryError> {
    Ok(PasskeyCredential {
        id: row.try_get("", "id")?,
        user_id: row.try_get("", "user_id")?,
        credential_id: row.try_get("", "credential_id")?,
        public_key: row.try_get("", "public_key")?,
        counter: row.try_get("", "counter")?,
        name: row.try_get("", "name")?,
        created_at: row.try_get("", "created_at")?,
        last_used_at: row.try_get("", "last_used_at")?,
    })
}

#[derive(Debug, Error)]
pub enum PasskeyRepositoryError {
    #[error(transparent)]
    Database(#[from] DbErr),
}
