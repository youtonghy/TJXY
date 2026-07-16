use serde_json::json;
use tjxy_api::{
    BaseItemDto, BaseItemDtoQueryResult, BaseItemKind, ClientCapabilitiesDto, CollectionType,
    UserItemDataDto,
};
use uuid::Uuid;

#[test]
fn client_capabilities_accept_the_pinned_optional_shape() {
    let capabilities: ClientCapabilitiesDto = serde_json::from_value(json!({
        "PlayableMediaTypes": ["Video", "Audio"],
        "SupportedCommands": ["DisplayContent"],
        "SupportsMediaControl": true,
        "SupportsPersistentIdentifier": true,
        "DeviceProfile": {"Name": "Findroid"}
    }))
    .unwrap();

    assert_eq!(capabilities.playable_media_types, ["Video", "Audio"]);
    assert_eq!(capabilities.supported_commands, ["DisplayContent"]);
    assert!(capabilities.supports_media_control);
    assert!(capabilities.supports_persistent_identifier);
}

#[test]
fn library_and_item_query_results_use_stable_pascal_case_fields() {
    let server_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let library_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
    let item_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    let library =
        BaseItemDto::library_view(library_id, "Movies", server_id, CollectionType::Movies);
    let user_data = UserItemDataDto::new(item_id, true, false, 1, 900_000);
    let movie = BaseItemDto::catalog_item(
        item_id,
        "Arrival",
        server_id,
        BaseItemKind::Movie,
        Some(library_id),
        Some(2016),
        Some("First contact.".to_owned()),
        Some(user_data),
    );

    assert_eq!(
        serde_json::to_value(BaseItemDtoQueryResult::new(vec![library, movie], 0, 2)).unwrap(),
        json!({
            "Items": [
                {
                    "Name": "Movies",
                    "ServerId": server_id,
                    "Id": library_id,
                    "Type": "CollectionFolder",
                    "CollectionType": "movies",
                    "IsFolder": true,
                    "ImageTags": {}
                },
                {
                    "Name": "Arrival",
                    "ServerId": server_id,
                    "Id": item_id,
                    "ParentId": library_id,
                    "Type": "Movie",
                    "MediaType": "Video",
                    "ProductionYear": 2016,
                    "Overview": "First contact.",
                    "IsFolder": false,
                    "ImageTags": {},
                    "UserData": {
                        "Key": item_id,
                        "ItemId": item_id,
                        "IsFavorite": true,
                        "Played": false,
                        "PlayCount": 1,
                        "PlaybackPositionTicks": 900000
                    }
                }
            ],
            "TotalRecordCount": 2,
            "StartIndex": 0
        })
    );
}
