use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ClientCapabilitiesDto;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceInfoDto {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom_name: Option<String>,
    id: String,
    last_user_name: String,
    app_name: String,
    app_version: String,
    last_user_id: Uuid,
    date_last_activity: DateTime<Utc>,
    capabilities: ClientCapabilitiesDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon_url: Option<String>,
}

impl DeviceInfoDto {
    #[must_use]
    #[allow(clippy::too_many_arguments)] // Mirrors Jellyfin's device identity and capability tuple.
    pub fn new(
        name: impl Into<String>,
        custom_name: Option<String>,
        id: impl Into<String>,
        last_user_name: impl Into<String>,
        app_name: impl Into<String>,
        app_version: impl Into<String>,
        last_user_id: Uuid,
        date_last_activity: DateTime<Utc>,
        capabilities: ClientCapabilitiesDto,
        icon_url: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            custom_name,
            id: id.into(),
            last_user_name: last_user_name.into(),
            app_name: app_name.into(),
            app_version: app_version.into(),
            last_user_id,
            date_last_activity,
            capabilities,
            icon_url,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceInfoDtoQueryResult {
    items: Vec<DeviceInfoDto>,
    total_record_count: usize,
    start_index: usize,
}

impl DeviceInfoDtoQueryResult {
    #[must_use]
    pub fn new(items: Vec<DeviceInfoDto>) -> Self {
        let total_record_count = items.len();
        Self {
            items,
            total_record_count,
            start_index: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceOptionsDto {
    #[serde(default, alias = "id")]
    pub id: i32,
    #[serde(alias = "deviceId")]
    pub device_id: Option<String>,
    #[serde(alias = "customName")]
    pub custom_name: Option<String>,
}

impl DeviceOptionsDto {
    #[must_use]
    pub fn new(id: i32, device_id: impl Into<String>, custom_name: Option<String>) -> Self {
        Self {
            id,
            device_id: Some(device_id.into()),
            custom_name,
        }
    }
}
