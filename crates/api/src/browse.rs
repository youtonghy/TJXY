use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ClientCapabilitiesDto {
    #[serde(default)]
    pub playable_media_types: Vec<String>,
    #[serde(default)]
    pub supported_commands: Vec<String>,
    #[serde(default)]
    pub supports_media_control: bool,
    #[serde(default)]
    pub supports_persistent_identifier: bool,
    pub device_profile: Option<Value>,
    pub app_store_url: Option<String>,
    pub icon_url: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum BaseItemKind {
    CollectionFolder,
    Movie,
    Series,
    Season,
    Episode,
    Folder,
}

impl BaseItemKind {
    const fn is_folder(self) -> bool {
        !matches!(self, Self::Movie | Self::Episode)
    }

    const fn media_type(self) -> Option<MediaType> {
        match self {
            Self::Movie | Self::Episode => Some(MediaType::Video),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum MediaType {
    Video,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum CollectionType {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "folders")]
    Folders,
    #[serde(rename = "movies")]
    Movies,
    #[serde(rename = "tvshows")]
    TvShows,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemDto {
    name: String,
    server_id: Uuid,
    id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<Uuid>,
    #[serde(rename = "Type")]
    item_type: BaseItemKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_type: Option<MediaType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection_type: Option<CollectionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    production_year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    overview: Option<String>,
    is_folder: bool,
    image_tags: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_data: Option<UserItemDataDto>,
}

impl BaseItemDto {
    #[must_use]
    pub fn library_view(
        id: Uuid,
        name: impl Into<String>,
        server_id: Uuid,
        collection_type: CollectionType,
    ) -> Self {
        Self {
            name: name.into(),
            server_id,
            id,
            parent_id: None,
            item_type: BaseItemKind::CollectionFolder,
            media_type: None,
            collection_type: Some(collection_type),
            production_year: None,
            overview: None,
            is_folder: true,
            image_tags: BTreeMap::new(),
            user_data: None,
        }
    }

    #[must_use]
    #[allow(clippy::too_many_arguments)] // Mirrors the minimal BaseItem projection.
    pub fn catalog_item(
        id: Uuid,
        name: impl Into<String>,
        server_id: Uuid,
        item_type: BaseItemKind,
        parent_id: Option<Uuid>,
        production_year: Option<i32>,
        overview: Option<String>,
        user_data: Option<UserItemDataDto>,
    ) -> Self {
        Self {
            name: name.into(),
            server_id,
            id,
            parent_id,
            item_type,
            media_type: item_type.media_type(),
            collection_type: None,
            production_year,
            overview,
            is_folder: item_type.is_folder(),
            image_tags: BTreeMap::new(),
            user_data,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserItemDataDto {
    key: Uuid,
    item_id: Uuid,
    is_favorite: bool,
    played: bool,
    play_count: i32,
    playback_position_ticks: i64,
}

impl UserItemDataDto {
    #[must_use]
    pub const fn new(
        item_id: Uuid,
        is_favorite: bool,
        played: bool,
        play_count: i32,
        playback_position_ticks: i64,
    ) -> Self {
        Self {
            key: item_id,
            item_id,
            is_favorite,
            played,
            play_count,
            playback_position_ticks,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemDtoQueryResult {
    items: Vec<BaseItemDto>,
    total_record_count: u64,
    start_index: u64,
}

impl BaseItemDtoQueryResult {
    #[must_use]
    pub const fn new(items: Vec<BaseItemDto>, start_index: u64, total_record_count: u64) -> Self {
        Self {
            items,
            total_record_count,
            start_index,
        }
    }
}
