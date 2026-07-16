use serde::{Deserialize, Serialize};
use tjxy_common::{CatalogItemId, MediaSourceId, PresentationKey};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CatalogItemKind {
    Movie,
    Series,
    Season,
    Episode,
    Folder,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CatalogItem {
    id: CatalogItemId,
    kind: CatalogItemKind,
    name: String,
}

impl CatalogItem {
    #[must_use]
    pub fn new(kind: CatalogItemKind, name: impl Into<String>) -> Self {
        Self {
            id: CatalogItemId::new(),
            kind,
            name: name.into(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> CatalogItemId {
        self.id
    }

    #[must_use]
    pub const fn kind(&self) -> CatalogItemKind {
        self.kind
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MediaLocation {
    storage_object_id: String,
}

impl MediaLocation {
    #[must_use]
    pub fn new(storage_object_id: impl Into<String>) -> Self {
        Self {
            storage_object_id: storage_object_id.into(),
        }
    }

    #[must_use]
    pub fn storage_object_id(&self) -> &str {
        &self.storage_object_id
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MediaSource {
    id: MediaSourceId,
    catalog_item_id: CatalogItemId,
    presentation_key: PresentationKey,
    locations: Vec<MediaLocation>,
}

impl MediaSource {
    #[must_use]
    pub fn new(catalog_item_id: CatalogItemId) -> Self {
        Self {
            id: MediaSourceId::new(),
            catalog_item_id,
            presentation_key: PresentationKey::new(),
            locations: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_location(mut self, location: MediaLocation) -> Self {
        self.locations.push(location);
        self
    }

    #[must_use]
    pub const fn id(&self) -> MediaSourceId {
        self.id
    }

    #[must_use]
    pub const fn catalog_item_id(&self) -> CatalogItemId {
        self.catalog_item_id
    }

    #[must_use]
    pub const fn presentation_key(&self) -> PresentationKey {
        self.presentation_key
    }

    #[must_use]
    pub fn locations(&self) -> &[MediaLocation] {
        &self.locations
    }
}
