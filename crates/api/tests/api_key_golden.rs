use chrono::{TimeZone, Utc};
use serde_json::json;
use tjxy_api::{AuthenticationInfoDto, AuthenticationInfoQueryResult};
use uuid::Uuid;

#[test]
fn api_key_uses_the_pinned_jellyfin_contract() {
    let user_id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f31").unwrap();
    let created_at = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let last_activity = Utc.with_ymd_and_hms(2026, 7, 26, 12, 3, 0).unwrap();
    let dto = AuthenticationInfoDto::new(
        7,
        "0123456789abcdef",
        "Kodi Sync",
        user_id,
        "Admin",
        created_at,
        Some(last_activity),
    );

    assert_eq!(
        serde_json::to_value(AuthenticationInfoQueryResult::new(vec![dto])).unwrap(),
        json!({
            "Items": [{
                "Id": 7,
                "AccessToken": "0123456789abcdef",
                "DeviceId": null,
                "AppName": "Kodi Sync",
                "AppVersion": null,
                "DeviceName": null,
                "UserId": user_id,
                "IsActive": true,
                "DateCreated": "2026-07-26T12:00:00Z",
                "DateRevoked": null,
                "DateLastActivity": "2026-07-26T12:03:00Z",
                "UserName": "Admin"
            }],
            "TotalRecordCount": 1,
            "StartIndex": 0
        })
    );
}

#[test]
fn api_key_serializes_absent_last_activity_as_null() {
    let user_id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f31").unwrap();
    let created_at = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let dto = AuthenticationInfoDto::new(
        7,
        "0123456789abcdef",
        "Kodi Sync",
        user_id,
        "Admin",
        created_at,
        None,
    );

    assert_eq!(
        serde_json::to_value(dto).unwrap()["DateLastActivity"],
        json!(null)
    );
}
