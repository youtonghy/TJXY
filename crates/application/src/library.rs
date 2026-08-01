use sea_orm::DatabaseConnection;
use thiserror::Error;
use tjxy_common::{LibraryId, StorageRootId};
use tjxy_db::{
    CreatedFilesystemLibrary, DisabledStorageRuntime, FilesystemRootDraft, LibraryPolicyUpdate,
    LibraryRepository, LibraryRepositoryError, StorageAccountRepository,
    StorageAccountRepositoryError, VirtualFolderRecord,
};
use tjxy_domain::{
    EffectiveScanPolicy, MetadataPolicy, ObjectSelectionScope, ProbePolicy, ScanProfile,
    StructureExpansionPolicy,
};

pub struct LibraryService {
    database: DatabaseConnection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LibraryPolicyOverrides<'value> {
    pub object_selection_scope: &'value str,
    pub metadata_policy: &'value str,
    pub expansion_policy: &'value str,
    pub probe_policy: &'value str,
}

impl LibraryService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Returns configured libraries and their persisted effective scan policies.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] when the SQL read model is unavailable or invalid.
    pub async fn virtual_folders(&self) -> Result<Vec<VirtualFolderRecord>, LibraryServiceError> {
        LibraryRepository::new(&self.database)
            .virtual_folders()
            .await
            .map_err(Into::into)
    }

    /// Creates an empty virtual folder. Storage roots are attached through storage admin flows.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] for invalid policy/input or persistence failure.
    pub async fn create_virtual_folder(
        &self,
        name: &str,
        collection_type: &str,
        profile: &str,
        enabled: bool,
        metadata_source_mode: &str,
    ) -> Result<LibraryId, LibraryServiceError> {
        let update = policy_update(profile, enabled, None, Some(metadata_source_mode))?;
        LibraryRepository::new(&self.database)
            .create(name, collection_type, &update)
            .await
            .map_err(Into::into)
    }

    /// Creates a virtual folder and one validated filesystem root atomically.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] for invalid policy/input or persistence failure.
    pub async fn create_virtual_folder_with_filesystem_root(
        &self,
        name: &str,
        collection_type: &str,
        profile: &str,
        enabled: bool,
        metadata_source_mode: &str,
        root: &FilesystemRootDraft,
    ) -> Result<CreatedFilesystemLibrary, LibraryServiceError> {
        let update = policy_update(profile, enabled, None, Some(metadata_source_mode))?;
        LibraryRepository::new(&self.database)
            .create_with_filesystem_root(name, collection_type, &update, root)
            .await
            .map_err(Into::into)
    }

    /// Attaches one server-validated filesystem root to an existing library.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] when the library is unavailable or binding fails.
    pub async fn attach_filesystem_root(
        &self,
        library_id: LibraryId,
        root: &FilesystemRootDraft,
    ) -> Result<CreatedFilesystemLibrary, LibraryServiceError> {
        LibraryRepository::new(&self.database)
            .attach_filesystem_root(library_id, root)
            .await
            .map_err(Into::into)
    }

    /// Renames one virtual folder by exact current name.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] for invalid, missing, ambiguous, or conflicting names.
    pub async fn rename_virtual_folder(
        &self,
        current_name: &str,
        new_name: &str,
    ) -> Result<(), LibraryServiceError> {
        LibraryRepository::new(&self.database)
            .rename_by_name(current_name, new_name)
            .await
            .map_err(Into::into)
    }

    /// Detaches one opaque storage root and disables an orphaned storage runtime binding.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] when the library or root membership is missing.
    pub async fn detach_root(
        &self,
        name: &str,
        root_id: StorageRootId,
    ) -> Result<Vec<DisabledStorageRuntime>, LibraryServiceError> {
        LibraryRepository::new(&self.database)
            .detach_root_by_name(name, root_id)
            .await
            .map_err(Into::into)
    }

    /// Deletes a virtual folder by name while preserving shared catalog and storage entities.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] when the library is missing, referenced, or unavailable.
    pub async fn delete_virtual_folder(
        &self,
        name: &str,
    ) -> Result<Vec<DisabledStorageRuntime>, LibraryServiceError> {
        LibraryRepository::new(&self.database)
            .delete_by_name(name)
            .await
            .map_err(Into::into)
    }

    /// Fences a durable account when runtime activation failed after binding commit.
    ///
    /// # Errors
    ///
    /// Returns a database error without reporting a successful fence.
    pub async fn disable_storage_account_after_activation_failure(
        &self,
        account_id: uuid::Uuid,
    ) -> Result<bool, LibraryServiceError> {
        StorageAccountRepository::new(&self.database)
            .disable_after_activation_failure(account_id)
            .await
            .map_err(Into::into)
    }

    /// Applies one named scan profile and its effective policies with optimistic concurrency.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] for an invalid profile, stale version, missing library,
    /// or persistence failure.
    pub async fn update_profile(
        &self,
        library_id: LibraryId,
        profile: &str,
        expected_version: i32,
        enabled: bool,
        metadata_source_mode: Option<&str>,
        overrides: Option<LibraryPolicyOverrides<'_>>,
    ) -> Result<i32, LibraryServiceError> {
        let update = policy_update(profile, enabled, overrides, metadata_source_mode)?;
        LibraryRepository::new(&self.database)
            .update_policy(library_id, expected_version, &update)
            .await
            .map_err(Into::into)
    }
}

fn policy_update(
    profile: &str,
    enabled: bool,
    overrides: Option<LibraryPolicyOverrides<'_>>,
    metadata_source_mode: Option<&str>,
) -> Result<LibraryPolicyUpdate, LibraryServiceError> {
    let profile = parse_profile(profile)?;
    let policy = EffectiveScanPolicy::for_profile(profile);
    let (object_selection, metadata, expansion, probe) = overrides.map_or_else(
        || {
            (
                object_selection_name(policy.object_selection),
                metadata_name(policy.metadata),
                expansion_name(policy.expansion),
                probe_name(policy.probe),
            )
        },
        |overrides| {
            (
                overrides.object_selection_scope,
                overrides.metadata_policy,
                overrides.expansion_policy,
                overrides.probe_policy,
            )
        },
    );
    let update = LibraryPolicyUpdate::new(
        profile_name(profile),
        object_selection,
        metadata,
        expansion,
        probe,
        enabled,
    )?;
    match metadata_source_mode {
        Some(mode) => update.with_metadata_source_mode(mode).map_err(Into::into),
        None => Ok(update),
    }
}

fn parse_profile(value: &str) -> Result<ScanProfile, LibraryServiceError> {
    match value {
        "Full" => Ok(ScanProfile::Full),
        "Lazy" => Ok(ScanProfile::Lazy),
        "Hybrid" => Ok(ScanProfile::Hybrid),
        "Manual" => Ok(ScanProfile::Manual),
        _ => Err(LibraryServiceError::InvalidProfile),
    }
}

const fn profile_name(value: ScanProfile) -> &'static str {
    match value {
        ScanProfile::Full => "Full",
        ScanProfile::Lazy => "Lazy",
        ScanProfile::Hybrid => "Hybrid",
        ScanProfile::Manual => "Manual",
    }
}

const fn object_selection_name(value: ObjectSelectionScope) -> &'static str {
    match value {
        ObjectSelectionScope::EntireRoot => "all_synced_objects",
        ObjectSelectionScope::OnDemandSubtree => "title_layer",
        ObjectSelectionScope::ExplicitOnly => "library_roots",
    }
}

const fn metadata_name(value: MetadataPolicy) -> &'static str {
    match value {
        MetadataPolicy::Full => "full",
        MetadataPolicy::Basic => "basic",
        MetadataPolicy::ExplicitOnly => "none",
    }
}

const fn expansion_name(value: StructureExpansionPolicy) -> &'static str {
    match value {
        StructureExpansionPolicy::Eager => "eager",
        StructureExpansionPolicy::OnAccess => "on_browse",
        StructureExpansionPolicy::Background => "background",
        StructureExpansionPolicy::ExplicitOnly => "manual",
    }
}

const fn probe_name(value: ProbePolicy) -> &'static str {
    match value {
        ProbePolicy::Eager => "eager",
        ProbePolicy::OnPlaybackInfo => "on_playback",
        ProbePolicy::ExplicitOnly => "manual",
    }
}

#[derive(Debug, Error)]
pub enum LibraryServiceError {
    #[error("scan profile is invalid")]
    InvalidProfile,
    #[error("library operation failed: {0}")]
    Repository(#[from] LibraryRepositoryError),
    #[error("storage account status update failed: {0}")]
    StorageAccount(#[from] StorageAccountRepositoryError),
    #[error("runtime storage activation failed")]
    RuntimeStorageActivation,
}
