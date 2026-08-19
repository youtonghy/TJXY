use serde_json::Value;
use tjxy_api::{
    DeliveryMethod, MediaProtocol, MediaSourceInfo, MediaStream, MediaStreamType,
    PlaybackInfoResponse,
};
use tjxy_common::PresentationKey;

#[test]
fn direct_play_response_matches_the_pinned_pascal_case_golden() {
    let source = MediaSourceInfo::direct_play(
        PresentationKey::from_uuid(
            uuid::Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11").unwrap(),
        ),
        "mkv",
        "/Videos/41/stream?static=true&mediaSourceId=018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11",
        vec![MediaStream {
            codec: Some("srt".to_owned()),
            language: Some("eng".to_owned()),
            width: None,
            height: None,
            channels: None,
            profile: None,
            level: None,
            stream_type: MediaStreamType::Subtitle,
            index: 3,
            is_external: true,
            delivery_method: Some(DeliveryMethod::External),
            delivery_url: Some(
                "/Videos/41/018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11/Subtitles/3/Stream.srt".to_owned(),
            ),
            is_external_url: false,
            is_text_subtitle_stream: true,
            supports_external_stream: true,
            is_default: true,
            is_forced: false,
        }],
        true,
    )
    .unwrap()
    .with_details(
        Some("Director's Cut".to_owned()),
        Some(8_000_000),
        Some(72_000_000_000),
        true,
    );
    let response = PlaybackInfoResponse {
        media_sources: vec![source],
        play_session_id: "session-1".to_owned(),
    };

    let actual = serde_json::to_value(response).unwrap();
    let expected: Value =
        serde_json::from_str(include_str!("golden/playback_info_direct_play.json")).unwrap();

    assert_eq!(actual, expected);
    assert_eq!(
        actual["MediaSources"][0]["Protocol"],
        MediaProtocol::File.as_str()
    );
}

#[test]
fn direct_play_urls_must_be_local_tjxy_routes() {
    let error = MediaSourceInfo::direct_play(
        PresentationKey::new(),
        "mkv",
        "https://drive.google.com/temporary-download",
        Vec::new(),
        true,
    )
    .unwrap_err();

    assert_eq!(error.to_string(), "media route must be a local TJXY path");

    let subtitle = MediaStream {
        codec: Some("srt".to_owned()),
        language: None,
        width: None,
        height: None,
        channels: None,
        profile: None,
        level: None,
        stream_type: MediaStreamType::Subtitle,
        index: 0,
        is_external: true,
        delivery_method: Some(DeliveryMethod::External),
        delivery_url: Some("https://graph.microsoft.com/download".to_owned()),
        is_external_url: true,
        is_text_subtitle_stream: true,
        supports_external_stream: true,
        is_default: false,
        is_forced: false,
    };
    let error = MediaSourceInfo::direct_play(
        PresentationKey::new(),
        "mkv",
        "/Videos/41/stream?static=true",
        vec![subtitle],
        true,
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "subtitle route must be a local TJXY path"
    );
}
