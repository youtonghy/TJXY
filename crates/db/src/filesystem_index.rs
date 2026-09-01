use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use tjxy_common::StorageRootId;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilesystemIndexState {
    Ready,
    Rebuilding,
    Failed,
}

pub struct FilesystemIndexRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> FilesystemIndexRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Reconciles one runtime mount identity with its persisted read-index state.
    ///
    /// Returns `true` when a single-flight validation must be scheduled.
    ///
    /// # Errors
    ///
    /// Returns a database error when the filesystem configuration cannot be read or updated.
    pub async fn prepare_mount(
        &self,
        account_id: Uuid,
        physical_identity: &str,
        namespace_changed: bool,
    ) -> Result<bool, DbErr> {
        let transaction = self.database.begin().await?;
        let result = prepare_mount(
            &transaction,
            account_id,
            physical_identity,
            namespace_changed,
        )
        .await;
        finish(transaction, result).await
    }

    /// Reads whether object access is allowed for one filesystem account.
    ///
    /// # Errors
    ///
    /// Returns a database error when the configuration cannot be read.
    pub async fn state(&self, account_id: Uuid) -> Result<FilesystemIndexState, DbErr> {
        let query = Query::select()
            .column(Alias::new("path_index_state"))
            .from(Alias::new("filesystem_storage_configs"))
            .and_where(Expr::col(Alias::new("storage_account_id")).eq(account_id))
            .limit(1)
            .to_owned();
        let Some(row) = self
            .database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
        else {
            return Ok(FilesystemIndexState::Ready);
        };
        parse_state(
            row.try_get::<Option<String>>("", "path_index_state")?
                .as_deref(),
        )
    }

    /// Marks a rebuilding filesystem root failed after terminal validation.
    ///
    /// # Errors
    ///
    /// Returns a database error when the state cannot be persisted.
    pub async fn mark_failed(&self, root_id: StorageRootId, error: &str) -> Result<(), DbErr> {
        let account_id = root_account_id(self.database, root_id).await?;
        let update = Query::update()
            .table(Alias::new("filesystem_storage_configs"))
            .value(Alias::new("path_index_state"), "Failed")
            .value(Alias::new("path_index_error"), error)
            .and_where(Expr::col(Alias::new("storage_account_id")).eq(account_id))
            .and_where(Expr::col(Alias::new("path_index_state")).eq("Rebuilding"))
            .to_owned();
        self.database
            .execute(self.database.get_database_backend().build(&update))
            .await?;
        Ok(())
    }
}

pub(crate) async fn publish_ready_after_root_sync(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    revision: i64,
) -> Result<(), DbErr> {
    let account_id = root_account_id(transaction, root_id).await?;
    let update = Query::update()
        .table(Alias::new("filesystem_storage_configs"))
        .value(Alias::new("path_index_state"), "Ready")
        .value(
            Alias::new("verified_physical_root_identity"),
            Expr::col(Alias::new("pending_physical_root_identity")),
        )
        .value(
            Alias::new("pending_physical_root_identity"),
            Option::<String>::None,
        )
        .value(Alias::new("path_index_revision"), revision)
        .value(Alias::new("path_index_error"), Option::<String>::None)
        .and_where(Expr::col(Alias::new("storage_account_id")).eq(account_id))
        .and_where(Expr::col(Alias::new("path_index_state")).eq("Rebuilding"))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&update))
        .await?;
    Ok(())
}

async fn prepare_mount(
    transaction: &DatabaseTransaction,
    account_id: Uuid,
    physical_identity: &str,
    namespace_changed: bool,
) -> Result<bool, DbErr> {
    let query = Query::select()
        .columns([
            Alias::new("path_index_state"),
            Alias::new("verified_physical_root_identity"),
            Alias::new("pending_physical_root_identity"),
        ])
        .from(Alias::new("filesystem_storage_configs"))
        .and_where(Expr::col(Alias::new("storage_account_id")).eq(account_id))
        .limit(1)
        .to_owned();
    let row = transaction
        .query_one(transaction.get_database_backend().build(&query))
        .await?
        .ok_or_else(|| DbErr::RecordNotFound("filesystem storage configuration".to_owned()))?;
    let state: Option<String> = row.try_get("", "path_index_state")?;
    let verified: Option<String> = row.try_get("", "verified_physical_root_identity")?;
    let pending: Option<String> = row.try_get("", "pending_physical_root_identity")?;
    if state.as_deref() == Some("Ready") && verified.as_deref() == Some(physical_identity) {
        return Ok(false);
    }
    if !namespace_changed && verified.is_none() && pending.is_none() {
        let update = Query::update()
            .table(Alias::new("filesystem_storage_configs"))
            .value(Alias::new("path_index_state"), "Ready")
            .value(
                Alias::new("verified_physical_root_identity"),
                physical_identity,
            )
            .value(Alias::new("path_index_error"), Option::<String>::None)
            .and_where(Expr::col(Alias::new("storage_account_id")).eq(account_id))
            .to_owned();
        transaction
            .execute(transaction.get_database_backend().build(&update))
            .await?;
        return Ok(false);
    }
    let update = Query::update()
        .table(Alias::new("filesystem_storage_configs"))
        .value(Alias::new("path_index_state"), "Rebuilding")
        .value(
            Alias::new("pending_physical_root_identity"),
            physical_identity,
        )
        .value(Alias::new("path_index_error"), Option::<String>::None)
        .and_where(Expr::col(Alias::new("storage_account_id")).eq(account_id))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&update))
        .await?;
    Ok(true)
}

async fn root_account_id(
    connection: &impl ConnectionTrait,
    root_id: StorageRootId,
) -> Result<Uuid, DbErr> {
    let query = Query::select()
        .column(Alias::new("storage_account_id"))
        .from(Alias::new("storage_roots"))
        .and_where(Expr::col(Alias::new("id")).eq(root_id.as_uuid()))
        .limit(1)
        .to_owned();
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await?
        .ok_or_else(|| DbErr::RecordNotFound("filesystem storage root".to_owned()))?
        .try_get("", "storage_account_id")
}

fn parse_state(value: Option<&str>) -> Result<FilesystemIndexState, DbErr> {
    match value {
        Some("Ready") => Ok(FilesystemIndexState::Ready),
        Some("Rebuilding") | None => Ok(FilesystemIndexState::Rebuilding),
        Some("Failed") => Ok(FilesystemIndexState::Failed),
        Some(value) => Err(DbErr::Custom(format!(
            "invalid filesystem path index state {value}"
        ))),
    }
}

async fn finish<T>(transaction: DatabaseTransaction, result: Result<T, DbErr>) -> Result<T, DbErr> {
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

#[cfg(test)]
mod tests {
    use sea_orm::{
        ConnectionTrait, Database, TransactionTrait,
        sea_query::{Alias, Query},
    };
    use sea_orm_migration::MigratorTrait;
    use tjxy_common::StorageRootId;
    use uuid::Uuid;

    use super::{FilesystemIndexRepository, FilesystemIndexState, publish_ready_after_root_sync};

    #[tokio::test]
    async fn mount_drift_is_gated_until_the_reconciled_revision_is_published() {
        let database = Database::connect("sqlite::memory:").await.unwrap();
        crate::Migrator::up(&database, None).await.unwrap();
        let backend = database.get_database_backend();
        let account_id = Uuid::new_v4();
        let root_id = StorageRootId::new();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_accounts"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("provider"),
                            Alias::new("display_name"),
                            Alias::new("account_identity"),
                            Alias::new("credential_ref"),
                            Alias::new("status"),
                        ])
                        .values_panic([
                            account_id.into(),
                            "filesystem".into(),
                            "Filesystem".into(),
                            Uuid::new_v4().to_string().into(),
                            "filesystem-test".into(),
                            "Active".into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("filesystem_storage_configs"))
                        .columns([Alias::new("storage_account_id"), Alias::new("root_path")])
                        .values_panic([account_id.into(), "/media".into()]),
                ),
            )
            .await
            .unwrap();
        database
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("storage_roots"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("storage_account_id"),
                            Alias::new("provider_root_id"),
                            Alias::new("sync_revision"),
                            Alias::new("reconciled_sync_revision"),
                        ])
                        .values_panic([
                            root_id.as_uuid().into(),
                            account_id.into(),
                            "root/root".into(),
                            2_i64.into(),
                            2_i64.into(),
                        ]),
                ),
            )
            .await
            .unwrap();
        let repository = FilesystemIndexRepository::new(&database);
        assert!(
            !repository
                .prepare_mount(account_id, "old", false)
                .await
                .unwrap()
        );
        assert_eq!(
            repository.state(account_id).await.unwrap(),
            FilesystemIndexState::Ready
        );
        assert!(
            repository
                .prepare_mount(account_id, "new", true)
                .await
                .unwrap()
        );
        assert_eq!(
            repository.state(account_id).await.unwrap(),
            FilesystemIndexState::Rebuilding
        );

        let transaction = database.begin().await.unwrap();
        publish_ready_after_root_sync(&transaction, root_id, 2)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(
            repository.state(account_id).await.unwrap(),
            FilesystemIndexState::Ready
        );
    }
}
