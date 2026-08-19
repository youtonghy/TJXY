use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{MediaSourceInfo, MediaStream};

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
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
    Audio,
    Series,
    Season,
    Episode,
    Folder,
}

impl BaseItemKind {
    const fn is_folder(self) -> bool {
        !matches!(self, Self::Movie | Self::Audio | Self::Episode)
    }

    const fn media_type(self) -> Option<MediaType> {
        match self {
            Self::Movie | Self::Episode => Some(MediaType::Video),
            Self::Audio => Some(MediaType::Audio),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum MediaType {
    Video,
    Audio,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum LocationType {
    FileSystem,
    Remote,
    Virtual,
    Offline,
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
    #[serde(rename = "music")]
    Music,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemDto {
    name: String,
    server_id: Uuid,
    id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    series_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    season_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    series_primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_primary_image_item_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_primary_image_tag: Option<String>,
    #[serde(rename = "Type")]
    item_type: BaseItemKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_type: Option<MediaType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection_type: Option<CollectionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    production_year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    overview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    community_rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    index_number: Option<i32>,
    is_folder: bool,
    image_tags: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_data: Option<UserItemDataDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tagline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vote_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_created: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location_type: Option<LocationType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    premiere_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    official_rating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    genres: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    studios: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    countries: Option<Vec<ItemNamedCodeDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    languages: Option<Vec<ItemNamedCodeDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    people: Option<Vec<ItemPersonDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_ids: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    has_media_sources: Option<bool>,
    backdrop_image_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_image_aspect_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_sources: Option<Vec<MediaSourceInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_streams: Option<Vec<MediaStream>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata_state: Option<String>,
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
            series_id: None,
            season_id: None,
            series_primary_image_tag: None,
            parent_primary_image_item_id: None,
            parent_primary_image_tag: None,
            item_type: BaseItemKind::CollectionFolder,
            media_type: None,
            collection_type: Some(collection_type),
            original_title: None,
            production_year: None,
            overview: None,
            community_rating: None,
            index_number: None,
            is_folder: true,
            image_tags: BTreeMap::new(),
            user_data: None,
            tagline: None,
            vote_count: None,
            run_time_ticks: None,
            date_created: None,
            location_type: None,
            premiere_date: None,
            end_date: None,
            status: None,
            official_rating: None,
            original_language: None,
            genres: None,
            studios: None,
            countries: None,
            languages: None,
            people: None,
            provider_ids: None,
            has_media_sources: None,
            backdrop_image_tags: Vec::new(),
            primary_image_aspect_ratio: None,
            media_sources: None,
            media_streams: None,
            metadata_state: None,
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
            series_id: (item_type == BaseItemKind::Season)
                .then_some(parent_id)
                .flatten(),
            season_id: (item_type == BaseItemKind::Episode)
                .then_some(parent_id)
                .flatten(),
            series_primary_image_tag: None,
            parent_primary_image_item_id: None,
            parent_primary_image_tag: None,
            item_type,
            media_type: item_type.media_type(),
            collection_type: None,
            original_title: None,
            production_year,
            overview,
            community_rating: None,
            index_number: None,
            is_folder: item_type.is_folder(),
            image_tags: BTreeMap::new(),
            user_data,
            tagline: None,
            vote_count: None,
            run_time_ticks: None,
            date_created: None,
            location_type: None,
            premiere_date: None,
            end_date: None,
            status: None,
            official_rating: None,
            original_language: None,
            genres: None,
            studios: None,
            countries: None,
            languages: None,
            people: None,
            provider_ids: None,
            has_media_sources: None,
            backdrop_image_tags: Vec::new(),
            primary_image_aspect_ratio: None,
            media_sources: None,
            media_streams: None,
            metadata_state: None,
        }
    }

    #[must_use]
    pub fn with_image_tags(mut self, image_tags: BTreeMap<String, String>) -> Self {
        self.image_tags = image_tags;
        self
    }

    #[must_use]
    pub fn with_series_image(mut self, series_id: Option<Uuid>, tag: Option<String>) -> Self {
        if series_id.is_some() {
            self.series_id = series_id;
        }
        self.series_primary_image_tag.clone_from(&tag);
        if tag.is_some() {
            self.parent_primary_image_item_id = self.series_id;
            self.parent_primary_image_tag = tag;
        }
        self
    }

    #[must_use]
    pub fn with_list_metadata(
        mut self,
        original_title: Option<String>,
        community_rating: Option<f64>,
        index_number: Option<i32>,
    ) -> Self {
        self.original_title = original_title;
        self.community_rating = community_rating;
        self.index_number = index_number;
        self
    }

    #[must_use]
    pub const fn with_runtime_ticks(mut self, runtime_ticks: Option<i64>) -> Self {
        self.run_time_ticks = runtime_ticks;
        self
    }

    #[must_use]
    pub fn with_metadata_state(mut self, metadata_state: impl Into<String>) -> Self {
        self.metadata_state = Some(metadata_state.into());
        self
    }

    #[must_use]
    pub fn with_catalog_metadata(
        mut self,
        date_created: DateTime<Utc>,
        location_type: LocationType,
        backdrop_image_tags: Vec<String>,
        primary_image_aspect_ratio: Option<f64>,
    ) -> Self {
        self.date_created = Some(date_created);
        self.location_type = Some(location_type);
        self.backdrop_image_tags = backdrop_image_tags;
        self.primary_image_aspect_ratio = primary_image_aspect_ratio;
        self
    }

    #[must_use]
    pub fn with_media_sources(mut self, media_sources: Vec<MediaSourceInfo>) -> Self {
        self.media_streams = Some(
            media_sources
                .first()
                .map_or_else(Vec::new, |source| source.media_streams().to_vec()),
        );
        self.media_sources = Some(media_sources);
        self
    }

    #[must_use]
    #[allow(clippy::too_many_arguments)] // Mirrors the bounded rich detail wire contract.
    pub fn with_rich_details(
        mut self,
        tagline: Option<String>,
        vote_count: Option<i64>,
        run_time_ticks: Option<i64>,
        premiere_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        status: Option<String>,
        official_rating: Option<String>,
        original_language: Option<String>,
        genres: Vec<String>,
        studios: Vec<String>,
        countries: Vec<ItemNamedCodeDto>,
        languages: Vec<ItemNamedCodeDto>,
        people: Vec<ItemPersonDto>,
        provider_ids: BTreeMap<String, String>,
        has_media_sources: bool,
    ) -> Self {
        self.tagline = tagline;
        self.vote_count = vote_count;
        self.run_time_ticks = run_time_ticks;
        self.premiere_date = premiere_date;
        self.end_date = end_date;
        self.status = status;
        self.official_rating = official_rating;
        self.original_language = original_language;
        self.genres = Some(genres);
        self.studios = Some(studios);
        self.countries = Some(countries);
        self.languages = Some(languages);
        self.people = Some(people);
        self.provider_ids = Some(provider_ids);
        self.has_media_sources = Some(has_media_sources);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemNamedCodeDto {
    code: String,
    name: String,
}

impl ItemNamedCodeDto {
    #[must_use]
    pub fn new(code: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            name: name.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemPersonDto {
    id: Uuid,
    name: String,
    role: String,
    #[serde(rename = "Type", skip_serializing_if = "Option::is_none")]
    person_type: Option<String>,
}

impl ItemPersonDto {
    #[must_use]
    pub fn new(
        id: Uuid,
        name: impl Into<String>,
        role: impl Into<String>,
        person_type: Option<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            role: role.into(),
            person_type,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    last_played_date: Option<DateTime<Utc>>,
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
            last_played_date: None,
        }
    }

    #[must_use]
    pub const fn with_last_played_date(mut self, value: Option<DateTime<Utc>>) -> Self {
        self.last_played_date = value;
        self
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct UpdateUserItemDataDto {
    pub is_favorite: Option<bool>,
    pub played: Option<bool>,
    pub play_count: Option<i32>,
    pub playback_position_ticks: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemDtoQueryResult {
    items: Vec<BaseItemDto>,
    total_record_count: u64,
    start_index: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHint {
    id: Uuid,
    name: String,
    #[serde(rename = "Type")]
    item_type: BaseItemKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    production_year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    community_rating: Option<f64>,
}

impl SearchHint {
    #[must_use]
    pub fn new(id: Uuid, name: impl Into<String>, item_type: BaseItemKind) -> Self {
        Self {
            id,
            name: name.into(),
            item_type,
            production_year: None,
            primary_image_tag: None,
            community_rating: None,
        }
    }

    #[must_use]
    pub fn with_metadata(
        mut self,
        production_year: Option<i32>,
        primary_image_tag: Option<String>,
        community_rating: Option<f64>,
    ) -> Self {
        self.production_year = production_year;
        self.primary_image_tag = primary_image_tag;
        self.community_rating = community_rating;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchHintResult {
    search_hints: Vec<SearchHint>,
    total_record_count: u64,
    start_index: u64,
}

impl SearchHintResult {
    #[must_use]
    pub const fn new(
        search_hints: Vec<SearchHint>,
        start_index: u64,
        total_record_count: u64,
    ) -> Self {
        Self {
            search_hints,
            total_record_count,
            start_index,
        }
    }
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
