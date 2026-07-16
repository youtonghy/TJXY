use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ScanProfile {
    Full,
    Lazy,
    Hybrid,
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
            ScanProfile::Hybrid => Self {
                object_selection: ObjectSelectionScope::EntireRoot,
                metadata: MetadataPolicy::Basic,
                expansion: StructureExpansionPolicy::Background,
                probe: ProbePolicy::OnPlaybackInfo,
            },
            ScanProfile::Manual => Self {
                object_selection: ObjectSelectionScope::ExplicitOnly,
                metadata: MetadataPolicy::ExplicitOnly,
                expansion: StructureExpansionPolicy::ExplicitOnly,
                probe: ProbePolicy::ExplicitOnly,
            },
        }
    }
}
