use serde::Serialize;
use thiserror::Error;
use tjxy_common::PresentationKey;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum MediaProtocol {
    Http,
}

impl MediaProtocol {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "Http",
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
    #[serde(rename = "Type")]
    pub stream_type: MediaStreamType,
    pub index: i32,
    pub is_external: bool,
    pub delivery_method: Option<DeliveryMethod>,
    pub delivery_url: Option<String>,
    pub is_external_url: bool,
    pub is_text_subtitle_stream: bool,
    pub supports_external_stream: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)] // The pinned OpenAPI schema defines these flags.
pub struct MediaSourceInfo {
    protocol: MediaProtocol,
    id: PresentationKey,
    path: Option<String>,
    container: String,
    is_remote: bool,
    supports_transcoding: bool,
    supports_direct_stream: bool,
    supports_direct_play: bool,
    media_streams: Vec<MediaStream>,
    transcoding_url: Option<String>,
    direct_stream_url: String,
}

impl MediaSourceInfo {
    /// Builds a byte-for-byte direct-play source using only TJXY routes.
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
            protocol: MediaProtocol::Http,
            id,
            path: None,
            container: container.into(),
            is_remote: false,
            supports_transcoding: false,
            supports_direct_stream: false,
            supports_direct_play: true,
            media_streams,
            transcoding_url: None,
            direct_stream_url,
        })
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
