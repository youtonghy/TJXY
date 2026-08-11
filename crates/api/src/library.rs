use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualFolderInfo {
    name: String,
    locations: Vec<String>,
    collection_type: String,
    library_options: LibraryOptionsDto,
    item_id: Uuid,
    primary_image_item_id: Option<Uuid>,
    refresh_progress: Option<u8>,
    refresh_status: Option<String>,
    unavailable_locations: Vec<String>,
}

impl VirtualFolderInfo {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        locations: Vec<String>,
        collection_type: impl Into<String>,
        library_options: LibraryOptionsDto,
        item_id: Uuid,
    ) -> Self {
        Self {
            name: name.into(),
            library_options: library_options.with_locations(&locations),
            locations,
            collection_type: collection_type.into(),
            item_id,
            primary_image_item_id: None,
            refresh_progress: None,
            refresh_status: None,
            unavailable_locations: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_unavailable_locations(mut self, locations: Vec<String>) -> Self {
        self.unavailable_locations = locations;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryOptionsDto {
    enabled: bool,
    enable_realtime_monitor: bool,
    path_infos: Vec<MediaPathInfoDto>,
    scan_profile: String,
    profile_version: i32,
    object_selection_scope: String,
    metadata_policy: String,
    metadata_source_mode: String,
    expansion_policy: String,
    probe_policy: String,
}

impl LibraryOptionsDto {
    #[must_use]
    #[allow(clippy::too_many_arguments)] // Mirrors the persisted effective policy contract.
    pub fn new(
        enabled: bool,
        scan_profile: impl Into<String>,
        profile_version: i32,
        object_selection_scope: impl Into<String>,
        metadata_policy: impl Into<String>,
        metadata_source_mode: impl Into<String>,
        expansion_policy: impl Into<String>,
        probe_policy: impl Into<String>,
    ) -> Self {
        Self {
            enabled,
            enable_realtime_monitor: false,
            path_infos: Vec::new(),
            scan_profile: scan_profile.into(),
            profile_version,
            object_selection_scope: object_selection_scope.into(),
            metadata_policy: metadata_policy.into(),
            metadata_source_mode: metadata_source_mode.into(),
            expansion_policy: expansion_policy.into(),
            probe_policy: probe_policy.into(),
        }
    }

    fn with_locations(mut self, locations: &[String]) -> Self {
        self.path_infos = locations
            .iter()
            .cloned()
            .map(|path| MediaPathInfoDto { path })
            .collect();
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
struct MediaPathInfoDto {
    path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AddVirtualFolderDto {
    #[serde(default)]
    library_options: Option<CreateLibraryOptions>,
    #[serde(default)]
    filesystem_selection: Option<FilesystemSelectionDto>,
}

impl AddVirtualFolderDto {
    #[must_use]
    pub const fn library_options(&self) -> Option<&CreateLibraryOptions> {
        self.library_options.as_ref()
    }

    #[must_use]
    pub const fn filesystem_selection(&self) -> Option<&FilesystemSelectionDto> {
        self.filesystem_selection.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FilesystemSelectionDto {
    root_id: Uuid,
    relative_path: String,
}

impl FilesystemSelectionDto {
    #[must_use]
    pub const fn root_id(&self) -> Uuid {
        self.root_id
    }

    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AttachVirtualFolderPathDto {
    library_id: Uuid,
    filesystem_selection: FilesystemSelectionDto,
}

impl AttachVirtualFolderPathDto {
    #[must_use]
    pub const fn library_id(&self) -> Uuid {
        self.library_id
    }

    #[must_use]
    pub const fn filesystem_selection(&self) -> &FilesystemSelectionDto {
        &self.filesystem_selection
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateLibraryOptions {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_scan_profile")]
    scan_profile: String,
    #[serde(default = "default_metadata_source_mode")]
    metadata_source_mode: String,
}

impl CreateLibraryOptions {
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn scan_profile(&self) -> &str {
        &self.scan_profile
    }

    #[must_use]
    pub fn metadata_source_mode(&self) -> &str {
        &self.metadata_source_mode
    }
}

const fn default_true() -> bool {
    true
}

fn default_scan_profile() -> String {
    "Lazy".to_owned()
}

fn default_metadata_source_mode() -> String {
    "automatic_scrape".to_owned()
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateLibraryOptionsDto {
    id: Uuid,
    library_options: UpdateLibraryOptions,
}

impl UpdateLibraryOptionsDto {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn library_options(&self) -> &UpdateLibraryOptions {
        &self.library_options
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateLibraryOptions {
    enabled: bool,
    scan_profile: String,
    profile_version: i32,
    object_selection_scope: Option<String>,
    metadata_policy: Option<String>,
    metadata_source_mode: Option<String>,
    expansion_policy: Option<String>,
    probe_policy: Option<String>,
}

impl UpdateLibraryOptions {
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
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
    pub fn object_selection_scope(&self) -> Option<&str> {
        self.object_selection_scope.as_deref()
    }

    #[must_use]
    pub fn metadata_policy(&self) -> Option<&str> {
        self.metadata_policy.as_deref()
    }

    #[must_use]
    pub fn metadata_source_mode(&self) -> Option<&str> {
        self.metadata_source_mode.as_deref()
    }

    #[must_use]
    pub fn expansion_policy(&self) -> Option<&str> {
        self.expansion_policy.as_deref()
    }

    #[must_use]
    pub fn probe_policy(&self) -> Option<&str> {
        self.probe_policy.as_deref()
    }
}
