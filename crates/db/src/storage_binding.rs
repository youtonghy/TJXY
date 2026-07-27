use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Query},
};
use thiserror::Error;
use tjxy_common::{LibraryId, StorageObjectRecordId, StorageRootId, WorkJobId};
use tjxy_credentials::CredentialEnvelope;
use tjxy_storage::ChangeCursor;
use uuid::Uuid;

use crate::{WorkJobRepositoryError, WorkJobSpec, WorkScope, WorkTaskKind, natural_key};

const MAX_NAME_CHARS: usize = 2048;
const INITIAL_STORAGE_SYNC_PRIORITY: i32 = 50;

#[derive(Clone, Debug)]
pub struct StorageBindingDraft {
    provider: String,
    display_name: String,
    account_identity: String,
    credential_id: Uuid,
    target_library_id: LibraryId,
    envelope: CredentialEnvelope,
    provider_drive_id: String,
    root_object_id: String,
    root_name: String,
    cursor: ChangeCursor,
}

impl StorageBindingDraft {
    /// Defines one fully validated provider binding ready for atomic persistence.
    ///
    /// # Errors
    ///
    /// Returns [`StorageBindingRepositoryError::InvalidDraft`] for unsupported providers or
    /// empty, control-containing, or unbounded identity fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: impl Into<String>,
        display_name: impl Into<String>,
        account_identity: impl Into<String>,
        credential_id: Uuid,
        target_library_id: LibraryId,
        envelope: CredentialEnvelope,
        provider_drive_id: impl Into<String>,
        root_object_id: impl Into<String>,
        root_name: impl Into<String>,
        cursor: ChangeCursor,
    ) -> Result<Self, StorageBindingRepositoryError> {
        let draft = Self {
            provider: provider.into(),
            display_name: display_name.into(),
            account_identity: account_identity.into(),
            credential_id,
            target_library_id,
            envelope,
            provider_drive_id: provider_drive_id.into(),
            root_object_id: root_object_id.into(),
            root_name: root_name.into(),
            cursor,
        };
        if !matches!(draft.provider.as_str(), "google-drive" | "onedrive")
            || !valid(&draft.display_name)
            || !valid(&draft.account_identity)
            || !valid(&draft.provider_drive_id)
            || !valid(&draft.root_object_id)
            || !valid(&draft.root_name)
        {
            return Err(StorageBindingRepositoryError::InvalidDraft);
        }
        Ok(draft)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreatedStorageBinding {
    account: Uuid,
    root: Uuid,
    root_object: Uuid,
    credential: Uuid,
    initial_sync_job: WorkJobId,
}

impl CreatedStorageBinding {
    #[must_use]
    pub const fn account_id(self) -> Uuid {
        self.account
    }

    #[must_use]
    pub const fn root_id(self) -> Uuid {
        self.root
    }

    #[must_use]
    pub const fn root_object_id(self) -> Uuid {
        self.root_object
    }

    #[must_use]
    pub const fn credential_id(self) -> Uuid {
        self.credential
    }

    #[must_use]
    pub const fn initial_sync_job_id(self) -> WorkJobId {
        self.initial_sync_job
    }
}

pub struct StorageBindingRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> StorageBindingRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Atomically creates the encrypted credential, account, root object, and active cursor.
    ///
    /// # Errors
    ///
    /// Returns uniqueness, foreign-key, commit, or rollback failures.
    pub async fn create(
        &self,
        draft: &StorageBindingDraft,
    ) -> Result<CreatedStorageBinding, StorageBindingRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = create(&transaction, draft).await;
        finish(transaction, result).await
    }
}

#[derive(Debug, Error)]
pub enum StorageBindingRepositoryError {
    #[error("storage binding draft is invalid")]
    InvalidDraft,
    #[error("storage binding database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("initial storage sync job could not be created: {0}")]
    WorkJob(#[from] WorkJobRepositoryError),
    #[error("storage binding rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

async fn create(
    transaction: &DatabaseTransaction,
    draft: &StorageBindingDraft,
) -> Result<CreatedStorageBinding, StorageBindingRepositoryError> {
    let account_id = Uuid::new_v4();
    let root_id = Uuid::new_v4();
    let root_object_id = Uuid::new_v4();
    let now = Utc::now();
    insert_credential(transaction, draft, now).await?;
    insert_account(transaction, draft, account_id).await?;
    insert_root(transaction, draft, account_id, root_id).await?;
    insert_library_root(transaction, draft, root_id).await?;
    insert_root_object(transaction, draft, account_id, root_id, root_object_id).await?;
    insert_root_relation(transaction, root_id, root_object_id).await?;
    insert_cursor(transaction, draft, root_id).await?;
    let initial_sync_job = crate::work_job::enqueue_in_transaction(
        transaction,
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(StorageObjectRecordId::from_uuid(root_object_id)),
            0,
            INITIAL_STORAGE_SYNC_PRIORITY,
        )?
        .with_storage_root_affinity(StorageRootId::from_uuid(root_id))?,
        now,
    )
    .await?
    .job()
    .id();
    Ok(CreatedStorageBinding {
        account: account_id,
        root: root_id,
        root_object: root_object_id,
        credential: draft.credential_id,
        initial_sync_job,
    })
}

async fn insert_library_root(
    transaction: &DatabaseTransaction,
    draft: &StorageBindingDraft,
    root_id: Uuid,
) -> Result<(), DbErr> {
    let insert = Query::insert()
        .into_table(Alias::new("library_storage_roots"))
        .columns([
            Alias::new("id"),
            Alias::new("library_id"),
            Alias::new("storage_root_id"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            draft.target_library_id.as_uuid().into(),
            root_id.into(),
        ])
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&insert))
        .await?;
    Ok(())
}

async fn insert_credential(
    transaction: &DatabaseTransaction,
    draft: &StorageBindingDraft,
    now: chrono::DateTime<Utc>,
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    let credential = Query::insert()
        .into_table(Alias::new("storage_credentials"))
        .columns([
            Alias::new("id"),
            Alias::new("encrypted_payload"),
            Alias::new("key_version"),
            Alias::new("refresh_state"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            draft.credential_id.into(),
            draft.envelope.payload().to_vec().into(),
            draft.envelope.key_version().into(),
            "Ready".into(),
            now.into(),
            now.into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&credential)).await?;
    Ok(())
}

async fn insert_account(
    transaction: &DatabaseTransaction,
    draft: &StorageBindingDraft,
    account_id: Uuid,
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    let account = Query::insert()
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
            draft.provider.clone().into(),
            draft.display_name.clone().into(),
            draft.account_identity.clone().into(),
            draft.credential_id.to_string().into(),
            "Active".into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&account)).await?;
    Ok(())
}

async fn insert_root(
    transaction: &DatabaseTransaction,
    draft: &StorageBindingDraft,
    account_id: Uuid,
    root_id: Uuid,
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    let root = Query::insert()
        .into_table(Alias::new("storage_roots"))
        .columns([
            Alias::new("id"),
            Alias::new("storage_account_id"),
            Alias::new("provider_root_id"),
            Alias::new("sync_revision"),
            Alias::new("reconciled_sync_revision"),
        ])
        .values_panic([
            root_id.into(),
            account_id.into(),
            draft.root_object_id.clone().into(),
            0_i64.into(),
            0_i64.into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&root)).await?;
    Ok(())
}

async fn insert_root_object(
    transaction: &DatabaseTransaction,
    draft: &StorageBindingDraft,
    account_id: Uuid,
    root_id: Uuid,
    root_object_id: Uuid,
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    let object = Query::insert()
        .into_table(Alias::new("storage_objects"))
        .columns([
            Alias::new("id"),
            Alias::new("storage_account_id"),
            Alias::new("provider_drive_id"),
            Alias::new("provider_object_id"),
            Alias::new("identity_key"),
            Alias::new("name"),
            Alias::new("normalized_name"),
            Alias::new("object_type"),
            Alias::new("observed_sync_revision"),
            Alias::new("facts_observed_storage_root_id"),
            Alias::new("children_indexed"),
            Alias::new("children_index_revision"),
            Alias::new("identity_quality"),
            Alias::new("presence_state"),
        ])
        .values_panic([
            root_object_id.into(),
            account_id.into(),
            draft.provider_drive_id.clone().into(),
            draft.root_object_id.clone().into(),
            natural_key::hash(&[&draft.provider_drive_id, &draft.root_object_id]).into(),
            draft.root_name.clone().into(),
            draft.root_name.to_lowercase().into(),
            "Directory".into(),
            0_i64.into(),
            root_id.into(),
            false.into(),
            0_i64.into(),
            "ProviderStableId".into(),
            "Present".into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&object)).await?;
    Ok(())
}

async fn insert_root_relation(
    transaction: &DatabaseTransaction,
    root_id: Uuid,
    root_object_id: Uuid,
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    let relation = Query::insert()
        .into_table(Alias::new("storage_root_objects"))
        .columns([
            Alias::new("id"),
            Alias::new("storage_root_id"),
            Alias::new("storage_object_id"),
            Alias::new("observed_sync_revision"),
            Alias::new("children_indexed"),
            Alias::new("children_index_revision"),
            Alias::new("presence_state"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            root_id.into(),
            root_object_id.into(),
            0_i64.into(),
            false.into(),
            0_i64.into(),
            "Present".into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&relation)).await?;
    Ok(())
}

async fn insert_cursor(
    transaction: &DatabaseTransaction,
    draft: &StorageBindingDraft,
    root_id: Uuid,
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    let cursor = Query::insert()
        .into_table(Alias::new("storage_sync_cursors"))
        .columns([
            Alias::new("id"),
            Alias::new("storage_root_id"),
            Alias::new("cursor_type"),
            Alias::new("cursor_value"),
            Alias::new("status"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            root_id.into(),
            "Changes".into(),
            draft.cursor.as_str().into(),
            "Active".into(),
        ])
        .to_owned();
    transaction.execute(backend.build(&cursor)).await?;
    Ok(())
}

fn valid(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_NAME_CHARS
        && !value.chars().any(char::is_control)
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, StorageBindingRepositoryError>,
) -> Result<T, StorageBindingRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(StorageBindingRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
