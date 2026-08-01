use serde_json::json;
use tjxy_api::{
    AddVirtualFolderDto, LibraryOptionsDto, UpdateLibraryOptionsDto, VirtualFolderInfo,
};
use uuid::Uuid;

#[test]
fn virtual_folder_uses_jellyfin_fields_and_tjxy_effective_policy_extensions() {
    let id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11").unwrap();
    let dto = VirtualFolderInfo::new(
        "Movies",
        vec!["tjxy://storage-root/018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12".to_owned()],
        "movies",
        LibraryOptionsDto::new(
            true,
            "Lazy",
            3,
            "title_layer",
            "basic",
            "local_only",
            "on_browse",
            "on_playback",
        ),
        id,
    );

    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        json!({
            "Name": "Movies",
            "Locations": ["tjxy://storage-root/018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12"],
            "CollectionType": "movies",
            "LibraryOptions": {
                "Enabled": true,
                "EnableRealtimeMonitor": false,
                "PathInfos": [{"Path": "tjxy://storage-root/018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12"}],
                "ScanProfile": "Lazy",
                "ProfileVersion": 3,
                "ObjectSelectionScope": "title_layer",
                "MetadataPolicy": "basic",
                "MetadataSourceMode": "local_only",
                "ExpansionPolicy": "on_browse",
                "ProbePolicy": "on_playback"
            },
            "ItemId": id,
            "PrimaryImageItemId": null,
            "RefreshProgress": null,
            "RefreshStatus": null
        })
    );
}

#[test]
fn library_options_update_accepts_the_versioned_tjxy_profile_contract() {
    let payload: UpdateLibraryOptionsDto = serde_json::from_value(json!({
        "Id": "018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11",
        "LibraryOptions": {
            "Enabled": false,
            "ScanProfile": "Hybrid",
            "ProfileVersion": 3,
            "MetadataSourceMode": "local_only",
            "EnableRealtimeMonitor": true
        }
    }))
    .unwrap();

    assert_eq!(payload.library_options().scan_profile(), "Hybrid");
    assert_eq!(payload.library_options().profile_version(), 3);
    assert!(!payload.library_options().enabled());
    assert_eq!(
        payload.library_options().metadata_source_mode(),
        Some("local_only")
    );
}

#[test]
fn virtual_folder_create_accepts_jellyfin_options_and_tjxy_profile_extensions() {
    let payload: AddVirtualFolderDto = serde_json::from_value(json!({
        "LibraryOptions": {
            "Enabled": false,
            "ScanProfile": "Manual",
            "EnableRealtimeMonitor": true,
            "EnablePhotos": false
        }
    }))
    .unwrap();
    let options = payload.library_options().unwrap();

    assert!(!options.enabled());
    assert_eq!(options.scan_profile(), "Manual");
    assert_eq!(options.metadata_source_mode(), "automatic_scrape");
}
