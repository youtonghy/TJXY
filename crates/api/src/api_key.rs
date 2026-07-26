use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticationInfoDto {
    id: i64,
    access_token: String,
    device_id: Option<String>,
    app_name: String,
    app_version: Option<String>,
    device_name: Option<String>,
    user_id: Uuid,
    is_active: bool,
    date_created: DateTime<Utc>,
    date_revoked: Option<DateTime<Utc>>,
    date_last_activity: Option<DateTime<Utc>>,
    user_name: String,
}

impl AuthenticationInfoDto {
    #[must_use]
    pub fn new(
        id: i64,
        access_token: impl Into<String>,
        app_name: impl Into<String>,
        user_id: Uuid,
        user_name: impl Into<String>,
        date_created: DateTime<Utc>,
        date_last_activity: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id,
            access_token: access_token.into(),
            device_id: None,
            app_name: app_name.into(),
            app_version: None,
            device_name: None,
            user_id,
            is_active: true,
            date_created,
            date_revoked: None,
            date_last_activity,
            user_name: user_name.into(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticationInfoQueryResult {
    items: Vec<AuthenticationInfoDto>,
    total_record_count: usize,
    start_index: usize,
}

impl AuthenticationInfoQueryResult {
    #[must_use]
    pub fn new(items: Vec<AuthenticationInfoDto>) -> Self {
        let total_record_count = items.len();
        Self {
            items,
            total_record_count,
            start_index: 0,
        }
    }
}
