//! Catalog domain model. Storage paths are deliberately excluded from identity.

mod catalog;
mod presence;
mod scan_policy;

pub use catalog::{CatalogItem, CatalogItemKind, MediaLocation, MediaSource};
pub use presence::PresenceState;
pub use scan_policy::{
    EffectiveScanPolicy, InvalidLocalMetadataAccessMode, InvalidMetadataSourceMode,
    LocalMetadataAccessMode, MetadataPolicy, MetadataSourceMode, ObjectSelectionScope, ProbePolicy,
    ScanProfile, StructureExpansionPolicy,
};
