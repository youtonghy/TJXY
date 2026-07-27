use serde::Deserialize;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackStateRequest {
    pub item_id: Option<Uuid>,
    pub media_source_id: Option<Uuid>,
    pub play_session_id: Option<Uuid>,
    #[serde(default)]
    pub position_ticks: i64,
    pub user_id: Option<Uuid>,
}
