use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tjxy_common::PresentationKey;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct PlaybackTicketRequest {
    pub media_source_id: PresentationKey,
    pub play_session_id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackTicketResponse {
    pub id: Uuid,
    pub ticket: String,
    pub expires_at: DateTime<Utc>,
    pub stream_url: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum MediaProtocol {
    File,
}

impl MediaProtocol {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "File",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum MediaStreamType {
    Audio,
    Video,
    Subtitle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum DeliveryMethod {
    External,
    Embed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)] // The pinned OpenAPI schema defines these flags.
pub struct MediaStream {
    pub codec: Option<String>,
    pub language: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub channels: Option<i32>,
    pub profile: Option<String>,
    pub level: Option<i32>,
    #[serde(rename = "Type")]
    pub stream_type: MediaStreamType,
    pub index: i32,
    pub is_external: bool,
    pub delivery_method: Option<DeliveryMethod>,
    pub delivery_url: Option<String>,
    pub is_external_url: bool,
    pub is_text_subtitle_stream: bool,
    pub supports_external_stream: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub is_default: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub is_forced: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)] // The pinned OpenAPI schema defines these flags.
pub struct MediaSourceInfo {
    protocol: MediaProtocol,
    id: PresentationKey,
    path: Option<String>,
    container: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bitrate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "is_false")]
    is_default: bool,
    is_remote: bool,
    supports_transcoding: bool,
    supports_direct_stream: bool,
    supports_direct_play: bool,
    media_streams: Vec<MediaStream>,
    transcoding_url: Option<String>,
    direct_stream_url: String,
    required_http_headers: std::collections::BTreeMap<String, String>,
}

impl MediaSourceInfo {
    #[must_use]
    pub fn media_streams(&self) -> &[MediaStream] {
        &self.media_streams
    }

    /// Builds a byte-for-byte direct-stream source using only TJXY routes.
    ///
    /// # Errors
    ///
    /// Returns [`PlaybackInfoError`] when a media or subtitle URL points to an
    /// upstream service instead of an authenticated local TJXY route.
    pub fn direct_play(
        id: PresentationKey,
        container: impl Into<String>,
        direct_stream_url: impl Into<String>,
        media_streams: Vec<MediaStream>,
        supports_direct_play: bool,
    ) -> Result<Self, PlaybackInfoError> {
        let direct_stream_url = direct_stream_url.into();
        if !is_local_media_route(&direct_stream_url) {
            return Err(PlaybackInfoError::UnsafeMediaRoute);
        }
        if media_streams
            .iter()
            .filter_map(|stream| stream.delivery_url.as_deref())
            .any(|url| !is_local_subtitle_route(url))
        {
            return Err(PlaybackInfoError::UnsafeSubtitleRoute);
        }

        Ok(Self {
            protocol: MediaProtocol::File,
            id,
            path: None,
            container: container.into(),
            name: None,
            bitrate: None,
            run_time_ticks: None,
            is_default: false,
            is_remote: false,
            supports_transcoding: false,
            supports_direct_stream: supports_direct_play,
            supports_direct_play: false,
            media_streams,
            transcoding_url: None,
            direct_stream_url,
            required_http_headers: std::collections::BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn with_details(
        mut self,
        name: Option<String>,
        bitrate: Option<i64>,
        run_time_ticks: Option<i64>,
        is_default: bool,
    ) -> Self {
        self.name = name;
        self.bitrate = bitrate;
        self.run_time_ticks = run_time_ticks;
        self.is_default = is_default;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoResponse {
    pub media_sources: Vec<MediaSourceInfo>,
    pub play_session_id: String,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PlaybackInfoError {
    #[error("media route must be a local TJXY path")]
    UnsafeMediaRoute,
    #[error("subtitle route must be a local TJXY path")]
    UnsafeSubtitleRoute,
}

fn is_local_media_route(url: &str) -> bool {
    (url.starts_with("/Videos/") || url.starts_with("/Audio/")) && !url.contains("://")
}

fn is_local_subtitle_route(url: &str) -> bool {
    url.starts_with("/Videos/") && url.contains("/Subtitles/") && !url.contains("://")
}

// Serde's skip_serializing_if callback contract requires a shared reference.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
}
