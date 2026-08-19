use serde::{Deserialize, Deserializer};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackStateRequest {
    #[serde(default, deserialize_with = "empty_uuid_as_none")]
    pub item_id: Option<Uuid>,
    #[serde(default, deserialize_with = "empty_uuid_as_none")]
    pub media_source_id: Option<Uuid>,
    #[serde(default, deserialize_with = "empty_uuid_as_none")]
    pub play_session_id: Option<Uuid>,
    #[serde(default)]
    pub position_ticks: i64,
    #[serde(default, deserialize_with = "empty_uuid_as_none")]
    pub user_id: Option<Uuid>,
}

fn empty_uuid_as_none<'de, D>(deserializer: D) -> Result<Option<Uuid>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    match value.as_deref() {
        None | Some("") => Ok(None),
        Some(value) => Uuid::parse_str(value)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}
