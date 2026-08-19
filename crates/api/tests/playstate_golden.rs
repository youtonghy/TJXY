use serde_json::json;
use tjxy_api::PlaybackStateRequest;
use uuid::Uuid;

#[test]
fn playback_state_identity_fields_follow_the_optional_jellyfin_contract() {
    let telemetry_only: PlaybackStateRequest =
        serde_json::from_value(json!({ "CanSeek": true })).expect("optional identity request");

    assert_eq!(telemetry_only.item_id, None);
    assert_eq!(telemetry_only.media_source_id, None);
    assert_eq!(telemetry_only.play_session_id, None);
    assert_eq!(telemetry_only.position_ticks, 0);

    let empty_identity: PlaybackStateRequest = serde_json::from_value(json!({
        "ItemId": null,
        "MediaSourceId": "",
        "PlaySessionId": "",
        "UserId": ""
    }))
    .expect("JMP empty optional identity fields");
    assert_eq!(empty_identity.item_id, None);
    assert_eq!(empty_identity.media_source_id, None);
    assert_eq!(empty_identity.play_session_id, None);
    assert_eq!(empty_identity.user_id, None);

    assert!(
        serde_json::from_value::<PlaybackStateRequest>(json!({"PlaySessionId": "not-a-uuid"}))
            .is_err()
    );

    let item_id = Uuid::new_v4();
    let media_source_id = Uuid::new_v4();
    let play_session_id = Uuid::new_v4();
    let complete: PlaybackStateRequest = serde_json::from_value(json!({
        "ItemId": item_id,
        "MediaSourceId": media_source_id,
        "PlaySessionId": play_session_id,
        "PositionTicks": 42
    }))
    .expect("complete identity request");

    assert_eq!(complete.item_id, Some(item_id));
    assert_eq!(complete.media_source_id, Some(media_source_id));
    assert_eq!(complete.play_session_id, Some(play_session_id));
    assert_eq!(complete.position_ticks, 42);
}
