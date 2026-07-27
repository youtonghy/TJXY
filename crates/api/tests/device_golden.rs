use chrono::{TimeZone, Utc};
use serde_json::json;
use tjxy_api::{ClientCapabilitiesDto, DeviceInfoDto, DeviceInfoDtoQueryResult, DeviceOptionsDto};
use uuid::Uuid;

#[test]
fn device_info_uses_the_safe_jellyfin_pascal_case_contract() {
    let user_id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f31").unwrap();
    let activity = Utc.with_ymd_and_hms(2026, 7, 26, 12, 0, 0).unwrap();
    let capabilities = ClientCapabilitiesDto {
        playable_media_types: vec!["Video".to_owned(), "Audio".to_owned()],
        supported_commands: vec!["Play".to_owned()],
        supports_media_control: true,
        supports_persistent_identifier: true,
        device_profile: Some(json!({"Name": "Findroid"})),
        app_store_url: Some("https://example.invalid/app".to_owned()),
        icon_url: Some("https://example.invalid/icon".to_owned()),
    };
    let device = DeviceInfoDto::new(
        "Pixel",
        Some("Living room".to_owned()),
        "phone-1",
        "Alice",
        "Findroid",
        "0.16.0",
        user_id,
        activity,
        capabilities,
        Some("https://example.invalid/icon".to_owned()),
    );

    assert_eq!(
        serde_json::to_value(DeviceInfoDtoQueryResult::new(vec![device])).unwrap(),
        json!({
            "Items": [{
                "Name": "Pixel",
                "CustomName": "Living room",
                "Id": "phone-1",
                "LastUserName": "Alice",
                "AppName": "Findroid",
                "AppVersion": "0.16.0",
                "LastUserId": user_id,
                "DateLastActivity": "2026-07-26T12:00:00Z",
                "Capabilities": {
                    "PlayableMediaTypes": ["Video", "Audio"],
                    "SupportedCommands": ["Play"],
                    "SupportsMediaControl": true,
                    "SupportsPersistentIdentifier": true,
                    "DeviceProfile": {"Name": "Findroid"},
                    "AppStoreUrl": "https://example.invalid/app",
                    "IconUrl": "https://example.invalid/icon"
                },
                "IconUrl": "https://example.invalid/icon"
            }],
            "TotalRecordCount": 1,
            "StartIndex": 0
        })
    );
}

#[test]
fn device_options_accept_the_pinned_shape_and_compatible_extensions() {
    let options: DeviceOptionsDto = serde_json::from_value(json!({
        "Id": 7,
        "DeviceId": "phone-1",
        "CustomName": "Living room"
    }))
    .unwrap();

    assert_eq!(options.id, 7);
    assert_eq!(options.device_id.as_deref(), Some("phone-1"));
    assert_eq!(options.custom_name.as_deref(), Some("Living room"));
    let compatible: DeviceOptionsDto = serde_json::from_value(json!({
        "id": 8,
        "deviceId": "phone-2",
        "customName": "Bedroom",
        "FutureField": true
    }))
    .unwrap();
    assert_eq!(compatible.id, 8);
    assert_eq!(compatible.device_id.as_deref(), Some("phone-2"));
    assert_eq!(compatible.custom_name.as_deref(), Some("Bedroom"));
}
