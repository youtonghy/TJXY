use serde_json::json;
use tjxy_api::DisplayPreferencesDto;

#[test]
fn default_display_preferences_match_the_jellyfin_pascal_case_shape() {
    assert_eq!(
        serde_json::to_value(DisplayPreferencesDto::default()).unwrap(),
        json!({
            "Id": null,
            "ViewType": null,
            "SortBy": null,
            "IndexBy": null,
            "RememberIndexing": false,
            "PrimaryImageHeight": 250,
            "PrimaryImageWidth": 250,
            "CustomPrefs": {},
            "ScrollDirection": "Horizontal",
            "ShowBackdrop": true,
            "RememberSorting": false,
            "SortOrder": "Ascending",
            "ShowSidebar": false,
            "Client": null
        })
    );
}
