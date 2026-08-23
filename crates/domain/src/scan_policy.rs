use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum MetadataSourceMode {
    #[default]
    AutomaticScrape,
    LocalOnly,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum LocalMetadataAccessMode {
    #[default]
    Import,
    Direct,
    ImportMetadataOnly,
    ImportImagesOnly,
}

impl LocalMetadataAccessMode {
    #[must_use]
    pub const fn from_imports(import_metadata: bool, import_images: bool) -> Self {
        match (import_metadata, import_images) {
            (true, true) => Self::Import,
            (true, false) => Self::ImportMetadataOnly,
            (false, true) => Self::ImportImagesOnly,
            (false, false) => Self::Direct,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Direct => "direct",
            Self::ImportMetadataOnly => "import_metadata_only",
            Self::ImportImagesOnly => "import_images_only",
        }
    }
}

impl FromStr for LocalMetadataAccessMode {
    type Err = InvalidLocalMetadataAccessMode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "import" => Ok(Self::Import),
            "direct" => Ok(Self::Direct),
            "import_metadata_only" => Ok(Self::ImportMetadataOnly),
            "import_images_only" => Ok(Self::ImportImagesOnly),
            _ => Err(InvalidLocalMetadataAccessMode),
        }
    }
}

impl LocalMetadataAccessMode {
    #[must_use]
    pub const fn imports_metadata(self) -> bool {
        matches!(self, Self::Import | Self::ImportMetadataOnly)
    }

    #[must_use]
    pub const fn imports_images(self) -> bool {
        matches!(self, Self::Import | Self::ImportImagesOnly)
    }

    #[must_use]
    pub const fn uses_direct_metadata(self) -> bool {
        !self.imports_metadata()
    }

    #[must_use]
    pub const fn uses_direct_images(self) -> bool {
        !self.imports_images()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidLocalMetadataAccessMode;

impl fmt::Display for InvalidLocalMetadataAccessMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid local metadata access mode")
    }
}

impl std::error::Error for InvalidLocalMetadataAccessMode {}

impl MetadataSourceMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AutomaticScrape => "automatic_scrape",
            Self::LocalOnly => "local_only",
        }
    }
}

impl FromStr for MetadataSourceMode {
    type Err = InvalidMetadataSourceMode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "automatic_scrape" => Ok(Self::AutomaticScrape),
            "local_only" => Ok(Self::LocalOnly),
            _ => Err(InvalidMetadataSourceMode),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidMetadataSourceMode;

impl fmt::Display for InvalidMetadataSourceMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid metadata source mode")
    }
}

impl std::error::Error for InvalidMetadataSourceMode {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ScanProfile {
    Full,
    Lazy,
    Manual,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ObjectSelectionScope {
    EntireRoot,
    OnDemandSubtree,
    ExplicitOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MetadataPolicy {
    Full,
    Basic,
    ExplicitOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StructureExpansionPolicy {
    Eager,
    OnAccess,
    Background,
    ExplicitOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProbePolicy {
    Eager,
    OnPlaybackInfo,
    ExplicitOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EffectiveScanPolicy {
    pub object_selection: ObjectSelectionScope,
    pub metadata: MetadataPolicy,
    pub expansion: StructureExpansionPolicy,
    pub probe: ProbePolicy,
}

impl EffectiveScanPolicy {
    #[must_use]
    pub const fn for_profile(profile: ScanProfile) -> Self {
        match profile {
            ScanProfile::Full => Self {
                object_selection: ObjectSelectionScope::EntireRoot,
                metadata: MetadataPolicy::Full,
                expansion: StructureExpansionPolicy::Eager,
                probe: ProbePolicy::Eager,
            },
            ScanProfile::Lazy => Self {
                object_selection: ObjectSelectionScope::OnDemandSubtree,
                metadata: MetadataPolicy::Basic,
                expansion: StructureExpansionPolicy::OnAccess,
                probe: ProbePolicy::OnPlaybackInfo,
            },
            ScanProfile::Manual => Self {
                object_selection: ObjectSelectionScope::ExplicitOnly,
                metadata: MetadataPolicy::ExplicitOnly,
                expansion: StructureExpansionPolicy::ExplicitOnly,
                probe: ProbePolicy::OnPlaybackInfo,
            },
        }
    }
}
