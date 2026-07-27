use std::collections::HashSet;

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, JoinType, Order, Query},
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{LibraryId, SortKey, StorageObjectRecordId, StorageRootId, WorkJobId};
use uuid::Uuid;

use crate::{WorkJobSpec, WorkScope, WorkTaskKind, natural_key};

const FILESYSTEM_PROVIDER_DRIVE_ID: &str = "local";
const INITIAL_STORAGE_SYNC_PRIORITY: i32 = 50;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemRootDraft {
    root_path: String,
    provider_object_id: String,
    display_name: String,
}

impl FilesystemRootDraft {
    /// Creates one canonical filesystem root ready for durable binding.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryRepositoryError::InvalidFilesystemRoot`] for relative or unbounded data.
    pub fn new(
        root_path: impl Into<String>,
        provider_object_id: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, LibraryRepositoryError> {
        let draft = Self {
            root_path: root_path.into(),
            provider_object_id: provider_object_id.into(),
            display_name: display_name.into(),
        };
        if !std::path::Path::new(&draft.root_path).is_absolute()
            || !valid_bounded(&draft.root_path, 4096)
            || !valid_bounded(&draft.provider_object_id, 2048)
            || !valid_bounded(&draft.display_name, 2048)
        {
            return Err(LibraryRepositoryError::InvalidFilesystemRoot);
        }
        Ok(draft)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreatedFilesystemLibrary {
    library: LibraryId,
    account: Uuid,
    root: StorageRootId,
    initial_sync_job: WorkJobId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisabledStorageRuntime {
    account_id: Uuid,
    provider_drive_id: String,
}

impl DisabledStorageRuntime {
    #[must_use]
    pub const fn account_id(&self) -> Uuid {
        self.account_id
    }

    #[must_use]
    pub fn provider_drive_id(&self) -> &str {
        &self.provider_drive_id
    }
}

impl CreatedFilesystemLibrary {
    #[must_use]
    pub const fn library_id(self) -> LibraryId {
        self.library
    }

    #[must_use]
    pub const fn account_id(self) -> Uuid {
        self.account
    }

    #[must_use]
    pub const fn root_id(self) -> StorageRootId {
        self.root
    }

    #[must_use]
    pub const fn initial_sync_job_id(self) -> WorkJobId {
        self.initial_sync_job
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemRootConfiguration {
    account_id: Uuid,
    root_path: String,
}

impl FilesystemRootConfiguration {
    #[must_use]
    pub const fn account_id(&self) -> Uuid {
        self.account_id
    }

    #[must_use]
    pub fn root_path(&self) -> &str {
        &self.root_path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VirtualFolderRecord {
    id: LibraryId,
    name: String,
    collection_type: String,
    is_enabled: bool,
    scan_profile: String,
    profile_version: i32,
    object_selection_scope: String,
    metadata_policy: String,
    expansion_policy: String,
    probe_policy: String,
    roots: Vec<VirtualFolderRoot>,
}

impl VirtualFolderRecord {
    #[must_use]
    pub const fn id(&self) -> LibraryId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn collection_type(&self) -> &str {
        &self.collection_type
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    #[must_use]
    pub fn scan_profile(&self) -> &str {
        &self.scan_profile
    }

    #[must_use]
    pub const fn profile_version(&self) -> i32 {
        self.profile_version
    }

    #[must_use]
    pub fn object_selection_scope(&self) -> &str {
        &self.object_selection_scope
    }

    #[must_use]
    pub fn metadata_policy(&self) -> &str {
        &self.metadata_policy
    }

    #[must_use]
    pub fn expansion_policy(&self) -> &str {
        &self.expansion_policy
    }

    #[must_use]
    pub fn probe_policy(&self) -> &str {
        &self.probe_policy
    }

    #[must_use]
    pub fn roots(&self) -> &[VirtualFolderRoot] {
        &self.roots
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtualFolderRoot {
    id: Uuid,
}

impl VirtualFolderRoot {
    #[must_use]
    pub fn location(self) -> String {
        format!("tjxy://storage-root/{}", self.id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryPolicyUpdate {
    scan_profile: String,
    object_selection_scope: String,
    metadata_policy: String,
    expansion_policy: String,
    probe_policy: String,
    is_enabled: bool,
}

impl LibraryPolicyUpdate {
    /// Creates one validated effective library policy update.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryRepositoryError::InvalidStoredPolicy`] for unknown policy values.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        scan_profile: impl Into<String>,
        object_selection_scope: impl Into<String>,
        metadata_policy: impl Into<String>,
        expansion_policy: impl Into<String>,
        probe_policy: impl Into<String>,
        is_enabled: bool,
    ) -> Result<Self, LibraryRepositoryError> {
        let update = Self {
            scan_profile: scan_profile.into(),
            object_selection_scope: object_selection_scope.into(),
            metadata_policy: metadata_policy.into(),
            expansion_policy: expansion_policy.into(),
            probe_policy: probe_policy.into(),
            is_enabled,
        };
        validate_policy_values(
            &update.scan_profile,
            &update.object_selection_scope,
            &update.metadata_policy,
            &update.expansion_policy,
            &update.probe_policy,
        )?;
        Ok(update)
    }
}

pub struct LibraryRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> LibraryRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Creates an empty library with one persisted effective scan policy.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryRepositoryError`] for invalid input, a duplicate name, or SQL failure.
    pub async fn create(
        &self,
        name: &str,
        collection_type: &str,
        policy: &LibraryPolicyUpdate,
    ) -> Result<LibraryId, LibraryRepositoryError> {
        validate_library_identity(name, collection_type)?;
        let transaction = self.database.begin().await?;
        lock_library_mutations(&transaction).await?;
        ensure_name_available(&transaction, name).await?;
        let library_id = LibraryId::new();
        insert_library(&transaction, library_id, name, collection_type, policy).await?;
        crate::advance_catalog_generation(&transaction).await?;
        transaction.commit().await?;
        Ok(library_id)
    }

    /// Creates a library and one durable filesystem root in the same transaction.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryRepositoryError`] when validation, uniqueness, job creation, or SQL fails.
    pub async fn create_with_filesystem_root(
        &self,
        name: &str,
        collection_type: &str,
        policy: &LibraryPolicyUpdate,
        root: &FilesystemRootDraft,
    ) -> Result<CreatedFilesystemLibrary, LibraryRepositoryError> {
        validate_library_identity(name, collection_type)?;
        let transaction = self.database.begin().await?;
        lock_library_mutations(&transaction).await?;
        ensure_name_available(&transaction, name).await?;
        let library_id = LibraryId::new();
        insert_library(&transaction, library_id, name, collection_type, policy).await?;
        let created = bind_filesystem_root(&transaction, library_id, root).await?;
        crate::advance_catalog_generation(&transaction).await?;
        transaction.commit().await?;
        Ok(CreatedFilesystemLibrary {
            library: library_id,
            account: created.account,
            root: created.root,
            initial_sync_job: created.initial_sync_job,
        })
    }

    /// Renames one library by its exact current name and updates its stable sort key atomically.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryRepositoryError`] for invalid, missing, ambiguous, or conflicting names.
    pub async fn rename_by_name(
        &self,
        current_name: &str,
        new_name: &str,
    ) -> Result<(), LibraryRepositoryError> {
        validate_library_name(current_name)?;
        validate_library_name(new_name)?;
        let transaction = self.database.begin().await?;
        lock_library_mutations(&transaction).await?;
        let library_id = unique_library_id(&transaction, current_name).await?;
        if current_name != new_name {
            ensure_name_available(&transaction, new_name).await?;
            let update = Query::update()
                .table(Alias::new("libraries"))
                .value(Alias::new("name"), new_name)
                .value(
                    Alias::new("sort_key"),
                    SortKey::from_text(new_name).into_bytes(),
                )
                .and_where(Expr::col(Alias::new("id")).eq(library_id))
                .to_owned();
            if transaction
                .execute(transaction.get_database_backend().build(&update))
                .await?
                .rows_affected()
                != 1
            {
                return Err(LibraryRepositoryError::NotFound);
            }
            crate::advance_catalog_generation(&transaction).await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Removes one root membership and disables an orphaned storage runtime configuration.
    ///
    /// Durable storage objects are retained for later reattachment or explicit purge.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryRepositoryError`] when the library or membership is missing.
    pub async fn detach_root_by_name(
        &self,
        name: &str,
        root_id: StorageRootId,
    ) -> Result<Vec<DisabledStorageRuntime>, LibraryRepositoryError> {
        validate_library_name(name)?;
        let transaction = self.database.begin().await?;
        lock_library_mutations(&transaction).await?;
        let library_id = unique_library_id(&transaction, name).await?;
        let delete = Query::delete()
            .from_table(Alias::new("library_storage_roots"))
            .and_where(Expr::col(Alias::new("library_id")).eq(library_id))
            .and_where(Expr::col(Alias::new("storage_root_id")).eq(root_id.as_uuid()))
            .to_owned();
        if transaction
            .execute(transaction.get_database_backend().build(&delete))
            .await?
            .rows_affected()
            != 1
        {
            return Err(LibraryRepositoryError::RootNotAttached);
        }
        let disabled =
            disable_orphaned_storage_accounts(&transaction, &[root_id.as_uuid()]).await?;
        crate::advance_catalog_generation(&transaction).await?;
        transaction.commit().await?;
        Ok(disabled)
    }

    /// Loads active persisted filesystem runtime roots without exposing them through DTOs.
    ///
    /// # Errors
    ///
    /// Returns a database error when the configuration query fails.
    pub async fn active_filesystem_roots(
        &self,
    ) -> Result<Vec<FilesystemRootConfiguration>, LibraryRepositoryError> {
        let config = Alias::new("filesystem_config");
        let account = Alias::new("filesystem_account");
        let query = Query::select()
            .expr_as(
                Expr::col((account.clone(), Alias::new("id"))),
                Alias::new("storage_account_id"),
            )
            .expr_as(
                Expr::col((config.clone(), Alias::new("root_path"))),
                Alias::new("root_path"),
            )
            .from_as(Alias::new("filesystem_storage_configs"), config.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_accounts"),
                account.clone(),
                Expr::col((account.clone(), Alias::new("id")))
                    .equals((config, Alias::new("storage_account_id"))),
            )
            .and_where(Expr::col((account.clone(), Alias::new("provider"))).eq("filesystem"))
            .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
            .order_by(Alias::new("storage_account_id"), Order::Asc)
            .to_owned();
        let backend = self.database.get_database_backend();
        self.database
            .query_all(backend.build(&query))
            .await?
            .into_iter()
            .map(|row| {
                Ok(FilesystemRootConfiguration {
                    account_id: row.try_get("", "storage_account_id")?,
                    root_path: row.try_get("", "root_path")?,
                })
            })
            .collect()
    }

    /// Deletes a library by its Jellyfin-visible name while retaining shared catalog/storage rows.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryRepositoryError::Referenced`] when an import source still targets it.
    pub async fn delete_by_name(
        &self,
        name: &str,
    ) -> Result<Vec<DisabledStorageRuntime>, LibraryRepositoryError> {
        validate_library_name(name)?;
        let transaction = self.database.begin().await?;
        lock_library_mutations(&transaction).await?;
        let backend = transaction.get_database_backend();
        let matching = transaction
            .query_all(
                backend.build(
                    Query::select()
                        .column(Alias::new("id"))
                        .from(Alias::new("libraries"))
                        .and_where(Expr::col(Alias::new("name")).eq(name))
                        .limit(2),
                ),
            )
            .await?;
        if matching.len() > 1 {
            return Err(LibraryRepositoryError::NameConflict);
        }
        let library_id = matching
            .first()
            .ok_or(LibraryRepositoryError::NotFound)?
            .try_get::<Uuid>("", "id")?;
        let referenced = Query::select()
            .expr(Expr::val(1_i32))
            .from(Alias::new("import_sources"))
            .and_where(Expr::col(Alias::new("target_library_id")).eq(library_id))
            .limit(1)
            .to_owned();
        if transaction
            .query_one(backend.build(&referenced))
            .await?
            .is_some()
        {
            return Err(LibraryRepositoryError::Referenced);
        }
        let detached_roots = transaction
            .query_all(
                backend.build(
                    Query::select()
                        .column(Alias::new("storage_root_id"))
                        .from(Alias::new("library_storage_roots"))
                        .and_where(Expr::col(Alias::new("library_id")).eq(library_id)),
                ),
            )
            .await?
            .into_iter()
            .map(|row| row.try_get::<Uuid>("", "storage_root_id"))
            .collect::<Result<Vec<_>, _>>()?;
        for table in ["library_catalog_items", "library_storage_roots"] {
            transaction
                .execute(
                    backend.build(
                        Query::delete()
                            .from_table(Alias::new(table))
                            .and_where(Expr::col(Alias::new("library_id")).eq(library_id)),
                    ),
                )
                .await?;
        }
        let deleted = transaction
            .execute(
                backend.build(
                    Query::delete()
                        .from_table(Alias::new("libraries"))
                        .and_where(Expr::col(Alias::new("id")).eq(library_id)),
                ),
            )
            .await?;
        if deleted.rows_affected() != 1 {
            return Err(LibraryRepositoryError::NotFound);
        }
        let disabled = disable_orphaned_storage_accounts(&transaction, &detached_roots).await?;
        crate::advance_catalog_generation(&transaction).await?;
        transaction.commit().await?;
        Ok(disabled)
    }

    /// Reads all configured libraries and their opaque root identities in stable order.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryRepositoryError`] for invalid stored policy values or SQL failures.
    pub async fn virtual_folders(
        &self,
    ) -> Result<Vec<VirtualFolderRecord>, LibraryRepositoryError> {
        let library = Alias::new("virtual_folder_library");
        let mapping = Alias::new("virtual_folder_root_mapping");
        let root = Alias::new("virtual_folder_root");
        let query = Query::select()
            .columns([
                (library.clone(), Alias::new("id")),
                (library.clone(), Alias::new("name")),
                (library.clone(), Alias::new("collection_type")),
                (library.clone(), Alias::new("is_enabled")),
                (library.clone(), Alias::new("scan_profile")),
                (library.clone(), Alias::new("profile_version")),
                (library.clone(), Alias::new("object_selection_scope")),
                (library.clone(), Alias::new("metadata_policy")),
                (library.clone(), Alias::new("expansion_policy")),
                (library.clone(), Alias::new("probe_policy")),
            ])
            .expr_as(
                Expr::col((root.clone(), Alias::new("id"))),
                Alias::new("storage_root_id"),
            )
            .from_as(Alias::new("libraries"), library.clone())
            .join_as(
                JoinType::LeftJoin,
                Alias::new("library_storage_roots"),
                mapping.clone(),
                Expr::col((mapping.clone(), Alias::new("library_id")))
                    .equals((library.clone(), Alias::new("id"))),
            )
            .join_as(
                JoinType::LeftJoin,
                Alias::new("storage_roots"),
                root.clone(),
                Expr::col((root.clone(), Alias::new("id")))
                    .equals((mapping, Alias::new("storage_root_id"))),
            )
            .order_by((library.clone(), Alias::new("sort_key")), Order::Asc)
            .order_by((library, Alias::new("id")), Order::Asc)
            .order_by((root, Alias::new("id")), Order::Asc)
            .to_owned();
        let backend = self.database.get_database_backend();
        aggregate_folders(&self.database.query_all(backend.build(&query)).await?)
    }

    /// Updates one library's profile and effective policies using a version fence.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryRepositoryError::NotFound`] for an unknown library,
    /// [`LibraryRepositoryError::StaleProfile`] for a stale version, or a database error.
    pub async fn update_policy(
        &self,
        library_id: LibraryId,
        expected_version: i32,
        update: &LibraryPolicyUpdate,
    ) -> Result<i32, LibraryRepositoryError> {
        if expected_version < 0 {
            return Err(LibraryRepositoryError::InvalidProfileVersion);
        }
        let next_version = expected_version
            .checked_add(1)
            .ok_or(LibraryRepositoryError::InvalidProfileVersion)?;
        let statement = Query::update()
            .table(Alias::new("libraries"))
            .value(Alias::new("scan_profile"), update.scan_profile.as_str())
            .value(
                Alias::new("object_selection_scope"),
                update.object_selection_scope.as_str(),
            )
            .value(
                Alias::new("metadata_policy"),
                update.metadata_policy.as_str(),
            )
            .value(
                Alias::new("expansion_policy"),
                update.expansion_policy.as_str(),
            )
            .value(Alias::new("probe_policy"), update.probe_policy.as_str())
            .value(Alias::new("is_enabled"), update.is_enabled)
            .value(Alias::new("profile_version"), next_version)
            .and_where(Expr::col(Alias::new("id")).eq(library_id.as_uuid()))
            .and_where(Expr::col(Alias::new("profile_version")).eq(expected_version))
            .to_owned();
        let backend = self.database.get_database_backend();
        if self
            .database
            .execute(backend.build(&statement))
            .await?
            .rows_affected()
            == 1
        {
            return Ok(next_version);
        }
        let exists = Query::select()
            .expr(Expr::val(1_i32))
            .from(Alias::new("libraries"))
            .and_where(Expr::col(Alias::new("id")).eq(library_id.as_uuid()))
            .limit(1)
            .to_owned();
        if self
            .database
            .query_one(backend.build(&exists))
            .await?
            .is_some()
        {
            Err(LibraryRepositoryError::StaleProfile)
        } else {
            Err(LibraryRepositoryError::NotFound)
        }
    }
}

struct BoundFilesystemRoot {
    account: Uuid,
    root: StorageRootId,
    initial_sync_job: WorkJobId,
}

async fn ensure_name_available(
    transaction: &DatabaseTransaction,
    name: &str,
) -> Result<(), LibraryRepositoryError> {
    let query = Query::select()
        .expr(Expr::val(1_i32))
        .from(Alias::new("libraries"))
        .and_where(Expr::col(Alias::new("name")).eq(name))
        .limit(1)
        .to_owned();
    if transaction
        .query_one(transaction.get_database_backend().build(&query))
        .await?
        .is_some()
    {
        return Err(LibraryRepositoryError::NameConflict);
    }
    Ok(())
}

async fn unique_library_id(
    transaction: &DatabaseTransaction,
    name: &str,
) -> Result<Uuid, LibraryRepositoryError> {
    let rows = transaction
        .query_all(
            transaction.get_database_backend().build(
                Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new("libraries"))
                    .and_where(Expr::col(Alias::new("name")).eq(name))
                    .limit(2),
            ),
        )
        .await?;
    if rows.len() > 1 {
        return Err(LibraryRepositoryError::NameConflict);
    }
    rows.first()
        .ok_or(LibraryRepositoryError::NotFound)?
        .try_get("", "id")
        .map_err(Into::into)
}

async fn insert_library(
    transaction: &DatabaseTransaction,
    library_id: LibraryId,
    name: &str,
    collection_type: &str,
    policy: &LibraryPolicyUpdate,
) -> Result<(), DbErr> {
    let insert = Query::insert()
        .into_table(Alias::new("libraries"))
        .columns([
            Alias::new("id"),
            Alias::new("name"),
            Alias::new("scan_profile"),
            Alias::new("object_selection_scope"),
            Alias::new("metadata_policy"),
            Alias::new("expansion_policy"),
            Alias::new("probe_policy"),
            Alias::new("profile_version"),
            Alias::new("collection_type"),
            Alias::new("sort_key"),
            Alias::new("is_enabled"),
        ])
        .values_panic([
            library_id.as_uuid().into(),
            name.into(),
            policy.scan_profile.as_str().into(),
            policy.object_selection_scope.as_str().into(),
            policy.metadata_policy.as_str().into(),
            policy.expansion_policy.as_str().into(),
            policy.probe_policy.as_str().into(),
            1_i32.into(),
            collection_type.into(),
            SortKey::from_text(name).into_bytes().into(),
            policy.is_enabled.into(),
        ])
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&insert))
        .await?;
    Ok(())
}

#[allow(clippy::too_many_lines)] // One transaction writes the complete filesystem identity graph.
async fn bind_filesystem_root(
    transaction: &DatabaseTransaction,
    library_id: LibraryId,
    draft: &FilesystemRootDraft,
) -> Result<BoundFilesystemRoot, LibraryRepositoryError> {
    if let Some(existing) = existing_filesystem_root(transaction, &draft.root_path).await? {
        if existing.provider_object_id != draft.provider_object_id {
            return Err(LibraryRepositoryError::FilesystemRootIdentityChanged);
        }
        let backend = transaction.get_database_backend();
        transaction
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new("library_storage_roots"))
                        .columns([
                            Alias::new("id"),
                            Alias::new("library_id"),
                            Alias::new("storage_root_id"),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            library_id.as_uuid().into(),
                            existing.root_id.as_uuid().into(),
                        ]),
                ),
            )
            .await?;
        transaction
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("storage_accounts"))
                        .value(Alias::new("status"), "Active")
                        .and_where(Expr::col(Alias::new("id")).eq(existing.account_id)),
                ),
            )
            .await?;
        let initial_sync_job_id = enqueue_filesystem_sync(
            transaction,
            existing.root_id,
            existing.root_object_id,
            existing.sync_revision,
        )
        .await?;
        return Ok(BoundFilesystemRoot {
            account: existing.account_id,
            root: existing.root_id,
            initial_sync_job: initial_sync_job_id,
        });
    }
    let account_id = Uuid::new_v4();
    let root_id = StorageRootId::new();
    let root_object_id = StorageObjectRecordId::new();
    let identity = format!(
        "filesystem:{:x}",
        Sha256::digest(draft.root_path.as_bytes())
    );
    let backend = transaction.get_database_backend();
    let filesystem_config = if backend == sea_orm::DbBackend::MySql {
        Query::insert()
            .into_table(Alias::new("filesystem_storage_configs"))
            .columns([
                Alias::new("storage_account_id"),
                Alias::new("root_path"),
                Alias::new("root_path_key"),
            ])
            .values_panic([
                account_id.into(),
                draft.root_path.as_str().into(),
                natural_key::hash(&[draft.root_path.as_str()]).into(),
            ])
            .to_owned()
    } else {
        Query::insert()
            .into_table(Alias::new("filesystem_storage_configs"))
            .columns([Alias::new("storage_account_id"), Alias::new("root_path")])
            .values_panic([account_id.into(), draft.root_path.as_str().into()])
            .to_owned()
    };
    for statement in [
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
                draft.display_name.as_str().into(),
                identity.into(),
                format!("filesystem-config:{account_id}").into(),
                "Active".into(),
            ])
            .to_owned(),
        filesystem_config,
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
                draft.provider_object_id.as_str().into(),
                0_i64.into(),
                0_i64.into(),
            ])
            .to_owned(),
        Query::insert()
            .into_table(Alias::new("library_storage_roots"))
            .columns([
                Alias::new("id"),
                Alias::new("library_id"),
                Alias::new("storage_root_id"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                library_id.as_uuid().into(),
                root_id.as_uuid().into(),
            ])
            .to_owned(),
        Query::insert()
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
                root_object_id.as_uuid().into(),
                account_id.into(),
                FILESYSTEM_PROVIDER_DRIVE_ID.into(),
                draft.provider_object_id.as_str().into(),
                natural_key::hash(&[
                    FILESYSTEM_PROVIDER_DRIVE_ID,
                    draft.provider_object_id.as_str(),
                ])
                .into(),
                draft.display_name.as_str().into(),
                String::from_utf8(SortKey::from_text(&draft.display_name).into_bytes())
                    .map_err(|_| DbErr::Custom("filesystem sort key is invalid UTF-8".into()))?
                    .into(),
                "Directory".into(),
                0_i64.into(),
                root_id.as_uuid().into(),
                false.into(),
                0_i64.into(),
                "ProviderStableId".into(),
                "Present".into(),
            ])
            .to_owned(),
        Query::insert()
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
                root_id.as_uuid().into(),
                root_object_id.as_uuid().into(),
                0_i64.into(),
                false.into(),
                0_i64.into(),
                "Present".into(),
            ])
            .to_owned(),
    ] {
        transaction.execute(backend.build(&statement)).await?;
    }
    let initial_sync_job_id =
        enqueue_filesystem_sync(transaction, root_id, root_object_id, 0).await?;
    Ok(BoundFilesystemRoot {
        account: account_id,
        root: root_id,
        initial_sync_job: initial_sync_job_id,
    })
}

struct ExistingFilesystemRoot {
    account_id: Uuid,
    root_id: StorageRootId,
    root_object_id: StorageObjectRecordId,
    provider_object_id: String,
    sync_revision: i64,
}

async fn existing_filesystem_root(
    transaction: &DatabaseTransaction,
    root_path: &str,
) -> Result<Option<ExistingFilesystemRoot>, DbErr> {
    let config = Alias::new("existing_filesystem_config");
    let account = Alias::new("existing_filesystem_account");
    let root = Alias::new("existing_filesystem_root");
    let relation = Alias::new("existing_filesystem_relation");
    let query = Query::select()
        .expr_as(
            Expr::col((account.clone(), Alias::new("id"))),
            Alias::new("storage_account_id"),
        )
        .expr_as(
            Expr::col((root.clone(), Alias::new("id"))),
            Alias::new("storage_root_id"),
        )
        .expr_as(
            Expr::col((relation.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((root.clone(), Alias::new("provider_root_id"))),
            Alias::new("provider_root_id"),
        )
        .expr_as(
            Expr::col((root.clone(), Alias::new("sync_revision"))),
            Alias::new("sync_revision"),
        )
        .from_as(Alias::new("filesystem_storage_configs"), config.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            account.clone(),
            Expr::col((account.clone(), Alias::new("id")))
                .equals((config.clone(), Alias::new("storage_account_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("storage_account_id")))
                .equals((account, Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            relation.clone(),
            Expr::col((relation.clone(), Alias::new("storage_root_id")))
                .equals((root, Alias::new("id"))),
        )
        .and_where(Expr::col((config, Alias::new("root_path"))).eq(root_path))
        .and_where(Expr::col((relation, Alias::new("parent_storage_object_id"))).is_null())
        .limit(1)
        .to_owned();
    transaction
        .query_one(transaction.get_database_backend().build(&query))
        .await?
        .as_ref()
        .map(|row| {
            Ok(ExistingFilesystemRoot {
                account_id: row.try_get("", "storage_account_id")?,
                root_id: StorageRootId::from_uuid(row.try_get("", "storage_root_id")?),
                root_object_id: StorageObjectRecordId::from_uuid(
                    row.try_get("", "storage_object_id")?,
                ),
                provider_object_id: row.try_get("", "provider_root_id")?,
                sync_revision: row.try_get("", "sync_revision")?,
            })
        })
        .transpose()
}

async fn enqueue_filesystem_sync(
    transaction: &DatabaseTransaction,
    root_id: StorageRootId,
    root_object_id: StorageObjectRecordId,
    expected_revision: i64,
) -> Result<WorkJobId, LibraryRepositoryError> {
    Ok(crate::work_job::enqueue_in_transaction(
        transaction,
        &WorkJobSpec::new(
            WorkTaskKind::ScopedStorageSync,
            WorkScope::StorageObject(root_object_id),
            expected_revision,
            INITIAL_STORAGE_SYNC_PRIORITY,
        )?
        .with_storage_root_affinity(root_id)?,
        chrono::Utc::now(),
    )
    .await?
    .job()
    .id())
}

async fn disable_orphaned_storage_accounts(
    transaction: &DatabaseTransaction,
    root_ids: &[Uuid],
) -> Result<Vec<DisabledStorageRuntime>, DbErr> {
    let backend = transaction.get_database_backend();
    let mut visited_accounts = HashSet::new();
    let mut disabled = Vec::new();
    for root_id in root_ids {
        let root = Alias::new("orphan_root");
        let root_relation = Alias::new("orphan_root_relation");
        let root_object = Alias::new("orphan_root_object");
        let Some(root_scope) = transaction
            .query_one(
                backend.build(
                    Query::select()
                        .expr_as(
                            Expr::col((root.clone(), Alias::new("storage_account_id"))),
                            Alias::new("storage_account_id"),
                        )
                        .expr_as(
                            Expr::col((root_object.clone(), Alias::new("provider_drive_id"))),
                            Alias::new("provider_drive_id"),
                        )
                        .from_as(Alias::new("storage_roots"), root.clone())
                        .join_as(
                            JoinType::InnerJoin,
                            Alias::new("storage_root_objects"),
                            root_relation.clone(),
                            Expr::col((root_relation.clone(), Alias::new("storage_root_id")))
                                .equals((root.clone(), Alias::new("id"))),
                        )
                        .join_as(
                            JoinType::InnerJoin,
                            Alias::new("storage_objects"),
                            root_object.clone(),
                            Expr::col((root_object, Alias::new("id")))
                                .equals((root_relation.clone(), Alias::new("storage_object_id"))),
                        )
                        .and_where(Expr::col((root, Alias::new("id"))).eq(*root_id))
                        .and_where(
                            Expr::col((root_relation, Alias::new("parent_storage_object_id")))
                                .is_null(),
                        )
                        .limit(1),
                ),
            )
            .await?
        else {
            continue;
        };
        let account_id: Uuid = root_scope.try_get("", "storage_account_id")?;
        if !visited_accounts.insert(account_id) {
            continue;
        }
        let provider_drive_id: String = root_scope.try_get("", "provider_drive_id")?;
        let mapping = Alias::new("active_root_mapping");
        let attached_root = Alias::new("active_storage_root");
        let still_attached = Query::select()
            .expr(Expr::val(1_i32))
            .from_as(Alias::new("library_storage_roots"), mapping.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_roots"),
                attached_root.clone(),
                Expr::col((attached_root.clone(), Alias::new("id")))
                    .equals((mapping, Alias::new("storage_root_id"))),
            )
            .and_where(Expr::col((attached_root, Alias::new("storage_account_id"))).eq(account_id))
            .limit(1)
            .to_owned();
        if transaction
            .query_one(backend.build(&still_attached))
            .await?
            .is_some()
        {
            continue;
        }
        let result = transaction
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("storage_accounts"))
                        .value(Alias::new("status"), "Disabled")
                        .and_where(Expr::col(Alias::new("id")).eq(account_id))
                        .and_where(Expr::col(Alias::new("status")).eq("Active")),
                ),
            )
            .await?;
        if result.rows_affected() == 1 {
            disabled.push(DisabledStorageRuntime {
                account_id,
                provider_drive_id,
            });
        }
    }
    Ok(disabled)
}

fn valid_bounded(value: &str, max_chars: usize) -> bool {
    !value.is_empty() && value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

fn aggregate_folders(
    rows: &[QueryResult],
) -> Result<Vec<VirtualFolderRecord>, LibraryRepositoryError> {
    let mut folders = Vec::<VirtualFolderRecord>::new();
    for row in rows {
        let id = LibraryId::from_uuid(row.try_get("", "id")?);
        if folders.last().is_none_or(|folder| folder.id != id) {
            folders.push(folder_from_row(row, id)?);
        }
        if let Some(root_id) = row.try_get::<Option<Uuid>>("", "storage_root_id")? {
            folders
                .last_mut()
                .expect("the current folder was inserted above")
                .roots
                .push(VirtualFolderRoot { id: root_id });
        }
    }
    Ok(folders)
}

fn folder_from_row(
    row: &QueryResult,
    id: LibraryId,
) -> Result<VirtualFolderRecord, LibraryRepositoryError> {
    let record = VirtualFolderRecord {
        id,
        name: row.try_get("", "name")?,
        collection_type: row.try_get("", "collection_type")?,
        is_enabled: row.try_get("", "is_enabled")?,
        scan_profile: row.try_get("", "scan_profile")?,
        profile_version: row.try_get("", "profile_version")?,
        object_selection_scope: row.try_get("", "object_selection_scope")?,
        metadata_policy: row.try_get("", "metadata_policy")?,
        expansion_policy: row.try_get("", "expansion_policy")?,
        probe_policy: row.try_get("", "probe_policy")?,
        roots: Vec::new(),
    };
    validate_policy(&record)?;
    Ok(record)
}

fn validate_policy(record: &VirtualFolderRecord) -> Result<(), LibraryRepositoryError> {
    validate_policy_values(
        &record.scan_profile,
        &record.object_selection_scope,
        &record.metadata_policy,
        &record.expansion_policy,
        &record.probe_policy,
    )?;
    if record.profile_version < 0 {
        return Err(LibraryRepositoryError::InvalidStoredPolicy);
    }
    Ok(())
}

fn validate_policy_values(
    scan_profile: &str,
    object_selection_scope: &str,
    metadata_policy: &str,
    expansion_policy: &str,
    probe_policy: &str,
) -> Result<(), LibraryRepositoryError> {
    if !matches!(scan_profile, "Full" | "Lazy" | "Hybrid" | "Manual") {
        return Err(LibraryRepositoryError::InvalidStoredPolicy);
    }
    validate_effective_policy_values(
        object_selection_scope,
        metadata_policy,
        expansion_policy,
        probe_policy,
    )
}

pub(crate) fn validate_effective_policy_values(
    object_selection_scope: &str,
    metadata_policy: &str,
    expansion_policy: &str,
    probe_policy: &str,
) -> Result<(), LibraryRepositoryError> {
    if matches!(
        object_selection_scope,
        "all_synced_objects" | "title_layer" | "library_roots"
    ) && matches!(metadata_policy, "full" | "basic" | "none")
        && matches!(
            expansion_policy,
            "eager" | "on_browse" | "background" | "manual"
        )
        && matches!(probe_policy, "eager" | "on_playback" | "manual")
    {
        Ok(())
    } else {
        Err(LibraryRepositoryError::InvalidStoredPolicy)
    }
}

fn validate_library_identity(
    name: &str,
    collection_type: &str,
) -> Result<(), LibraryRepositoryError> {
    validate_library_name(name)?;
    if !matches!(
        collection_type,
        "movies"
            | "tvshows"
            | "music"
            | "musicvideos"
            | "homevideos"
            | "boxsets"
            | "books"
            | "photos"
            | "mixed"
    ) {
        return Err(LibraryRepositoryError::InvalidCollectionType);
    }
    Ok(())
}

fn validate_library_name(name: &str) -> Result<(), LibraryRepositoryError> {
    if name.is_empty() || name != name.trim() || name.len() > 256 {
        return Err(LibraryRepositoryError::InvalidName);
    }
    Ok(())
}

async fn lock_library_mutations(
    transaction: &sea_orm::DatabaseTransaction,
) -> Result<(), LibraryRepositoryError> {
    let statement = Query::update()
        .table(Alias::new("catalog_state"))
        .value(
            Alias::new("generation"),
            Expr::col(Alias::new("generation")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(1_i32))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&statement))
        .await?
        .rows_affected()
        != 1
    {
        return Err(DbErr::Custom("catalog generation row is missing".to_owned()).into());
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum LibraryRepositoryError {
    #[error("library name is invalid")]
    InvalidName,
    #[error("library collection type is invalid")]
    InvalidCollectionType,
    #[error("filesystem root configuration is invalid")]
    InvalidFilesystemRoot,
    #[error("filesystem root identity changed at the configured path")]
    FilesystemRootIdentityChanged,
    #[error("library name already exists")]
    NameConflict,
    #[error("library is referenced by an import source")]
    Referenced,
    #[error("stored library scan policy is invalid")]
    InvalidStoredPolicy,
    #[error("library profile version is invalid")]
    InvalidProfileVersion,
    #[error("library does not exist")]
    NotFound,
    #[error("storage root is not attached to the library")]
    RootNotAttached,
    #[error("library profile changed since it was read")]
    StaleProfile,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("initial filesystem sync job could not be created: {0}")]
    WorkJob(#[from] crate::WorkJobRepositoryError),
}
