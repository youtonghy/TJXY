use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum ScrollDirection {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)] // Mirrors the Jellyfin wire contract exactly.
pub struct DisplayPreferencesDto {
    pub id: Option<String>,
    pub view_type: Option<String>,
    pub sort_by: Option<String>,
    pub index_by: Option<String>,
    pub remember_indexing: bool,
    pub primary_image_height: i32,
    pub primary_image_width: i32,
    pub custom_prefs: BTreeMap<String, Option<String>>,
    pub scroll_direction: ScrollDirection,
    pub show_backdrop: bool,
    pub remember_sorting: bool,
    pub sort_order: SortOrder,
    pub show_sidebar: bool,
    pub client: Option<String>,
}

impl Default for DisplayPreferencesDto {
    fn default() -> Self {
        Self {
            id: None,
            view_type: None,
            sort_by: None,
            index_by: None,
            remember_indexing: false,
            primary_image_height: 250,
            primary_image_width: 250,
            custom_prefs: BTreeMap::new(),
            scroll_direction: ScrollDirection::Horizontal,
            show_backdrop: true,
            remember_sorting: false,
            sort_order: SortOrder::Ascending,
            show_sidebar: false,
            client: None,
        }
    }
}
