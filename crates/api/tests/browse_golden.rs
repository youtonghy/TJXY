use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use serde_json::json;
use tjxy_api::{
    BaseItemDto, BaseItemDtoQueryResult, BaseItemKind, ClientCapabilitiesDto, CollectionType,
    ItemNamedCodeDto, ItemPersonDto, UserItemDataDto,
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
fn rich_item_details_are_pascal_case_and_list_only_fields_remain_compact() {
    let server_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let item_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    let person_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
    let premiere = Utc.with_ymd_and_hms(2019, 5, 6, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2019, 6, 3, 0, 0, 0).unwrap();
    let provider_ids = BTreeMap::from([
        ("imdb".to_owned(), "tt7366338".to_owned()),
        ("tmdb".to_owned(), "87108".to_owned()),
    ]);
    let detail = BaseItemDto::catalog_item(
        item_id,
        "Chernobyl",
        server_id,
        BaseItemKind::Series,
        None,
        Some(2019),
        Some("A disaster and its aftermath.".to_owned()),
        None,
    )
    .with_list_metadata(Some("Chernobyl".to_owned()), Some(8.7), None)
    .with_rich_details(
        Some("Every lie we tell incurs a debt.".to_owned()),
        Some(7_000),
        Some(36_000_000_000),
        Some(premiere),
        Some(end),
        Some("Ended".to_owned()),
        Some("TV-MA".to_owned()),
        Some("en".to_owned()),
        vec!["Drama".to_owned()],
        vec!["HBO".to_owned()],
        vec![ItemNamedCodeDto::new("US", "United States")],
        vec![ItemNamedCodeDto::new("en", "English")],
        vec![ItemPersonDto::new(
            person_id,
            "Johan Renck",
            "Director",
            Some("Crew".to_owned()),
        )],
        provider_ids,
        false,
    );

    assert_eq!(
        serde_json::to_value(detail).unwrap(),
        json!({
            "Name": "Chernobyl",
            "ServerId": server_id,
            "Id": item_id,
            "Type": "Series",
            "OriginalTitle": "Chernobyl",
            "ProductionYear": 2019,
            "Overview": "A disaster and its aftermath.",
            "CommunityRating": 8.7,
            "IsFolder": true,
            "ImageTags": {},
            "Tagline": "Every lie we tell incurs a debt.",
            "VoteCount": 7000,
            "RunTimeTicks": 36_000_000_000_i64,
            "PremiereDate": "2019-05-06T00:00:00Z",
            "EndDate": "2019-06-03T00:00:00Z",
            "Status": "Ended",
            "OfficialRating": "TV-MA",
            "OriginalLanguage": "en",
            "Genres": ["Drama"],
            "Studios": ["HBO"],
            "Countries": [{"Code": "US", "Name": "United States"}],
            "Languages": [{"Code": "en", "Name": "English"}],
            "People": [{
                "Id": person_id,
                "Name": "Johan Renck",
                "Role": "Director",
                "Type": "Crew"
            }],
            "ProviderIds": {
                "imdb": "tt7366338",
                "tmdb": "87108"
            },
            "HasMediaSources": false
        })
    );
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
    )
    .with_runtime_ticks(Some(69_600_000_000));

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
                    "RunTimeTicks": 69_600_000_000_i64,
                    "IsFolder": false,
                    "ImageTags": {},
                    "UserData": {
                        "Key": item_id,
                        "ItemId": item_id,
                        "IsFavorite": true,
                        "Played": false,
                        "PlayCount": 1,
                        "PlaybackPositionTicks": 900_000
                    }
                }
            ],
            "TotalRecordCount": 2,
            "StartIndex": 0
        })
    );
}

#[test]
fn music_library_view_uses_the_jellyfin_collection_type() {
    let server_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let library_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();

    let library = BaseItemDto::library_view(library_id, "Music", server_id, CollectionType::Music);

    assert_eq!(
        serde_json::to_value(library).unwrap()["CollectionType"],
        "music"
    );
}
