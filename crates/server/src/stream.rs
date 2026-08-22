use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use tjxy_application::MediaReadError;
use tjxy_common::{CatalogItemId, PresentationKey};
use tjxy_storage::ByteRange;
use uuid::Uuid;

use crate::{AppState, auth};

const MAX_BITRATE_TEST_BYTES: usize = 4 * 1024 * 1024;

const STREAM_COMPATIBILITY_HINTS: &[&str] = &[
    "deviceId",
    "DeviceId",
    "playSessionId",
    "PlaySessionId",
    "tag",
    "Tag",
    "maxStreamingBitrate",
    "MaxStreamingBitrate",
    "audioStreamIndex",
    "AudioStreamIndex",
    "subtitleStreamIndex",
    "SubtitleStreamIndex",
    "startTimeTicks",
    "StartTimeTicks",
    "container",
    "Container",
    "params",
    "Params",
    "deviceProfileId",
    "DeviceProfileId",
    "segmentContainer",
    "SegmentContainer",
    "segmentLength",
    "SegmentLength",
    "minSegments",
    "MinSegments",
    "audioCodec",
    "AudioCodec",
    "enableAutoStreamCopy",
    "EnableAutoStreamCopy",
    "allowVideoStreamCopy",
    "AllowVideoStreamCopy",
    "allowAudioStreamCopy",
    "AllowAudioStreamCopy",
    "audioSampleRate",
    "AudioSampleRate",
    "maxAudioBitDepth",
    "MaxAudioBitDepth",
    "audioBitRate",
    "AudioBitRate",
    "audioChannels",
    "AudioChannels",
    "maxAudioChannels",
    "MaxAudioChannels",
    "profile",
    "Profile",
    "level",
    "Level",
    "framerate",
    "Framerate",
    "maxFramerate",
    "MaxFramerate",
    "copyTimestamps",
    "CopyTimestamps",
    "width",
    "Width",
    "height",
    "Height",
    "maxWidth",
    "MaxWidth",
    "maxHeight",
    "MaxHeight",
    "videoBitRate",
    "VideoBitRate",
    "subtitleMethod",
    "SubtitleMethod",
    "maxRefFrames",
    "MaxRefFrames",
    "maxVideoBitDepth",
    "MaxVideoBitDepth",
    "requireAvc",
    "RequireAvc",
    "deInterlace",
    "DeInterlace",
    "requireNonAnamorphic",
    "RequireNonAnamorphic",
    "transcodingMaxAudioChannels",
    "TranscodingMaxAudioChannels",
    "cpuCoreLimit",
    "CpuCoreLimit",
    "liveStreamId",
    "LiveStreamId",
    "enableMpegtsM2TsMode",
    "EnableMpegtsM2TsMode",
    "videoCodec",
    "VideoCodec",
    "subtitleCodec",
    "SubtitleCodec",
    "transcodeReasons",
    "TranscodeReasons",
    "videoStreamIndex",
    "VideoStreamIndex",
    "context",
    "Context",
    "enableAudioVbrEncoding",
    "EnableAudioVbrEncoding",
];

pub(crate) async fn bitrate_test(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Ok(mut query) = auth::request_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    query.remove("ApiKey");
    query.remove("api_key");
    let lower = query.remove("size");
    let upper = query.remove("Size");
    if !query.is_empty() || lower.is_some() && upper.is_some() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let requested = lower
        .or(upper)
        .map_or(Ok(102_400_usize), |value| value.parse::<usize>());
    let Ok(requested) = requested else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if requested == 0 || requested > MAX_BITRATE_TEST_BYTES {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let size = requested.next_power_of_two();
    let mut response = Body::from(vec![0_u8; size]).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store"),
    );
    response
}

pub(crate) async fn get(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    serve(state, item_id, headers, raw_query, false).await
}

pub(crate) async fn head(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    serve(state, item_id, headers, raw_query, true).await
}

pub(crate) async fn get_with_container(
    State(state): State<AppState>,
    Path((item_id, _container)): Path<(Uuid, String)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    serve(state, item_id, headers, raw_query, false).await
}

pub(crate) async fn head_with_container(
    State(state): State<AppState>,
    Path((item_id, _container)): Path<(Uuid, String)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    serve(state, item_id, headers, raw_query, true).await
}

#[allow(clippy::too_many_lines)]
async fn serve(
    state: AppState,
    item_id: Uuid,
    headers: HeaderMap,
    raw_query: Option<String>,
    head_only: bool,
) -> Response {
    let query = match stream_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err((status, message)) => return response(status, message),
    };
    let authenticated = headers.contains_key(header::AUTHORIZATION)
        || query.api_key_present
        || headers.contains_key("X-Emby-Token")
        || headers.contains_key("X-MediaBrowser-Token");
    let (user_id, presentation) = if authenticated {
        let principal =
            match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
                Ok(principal) => principal,
                Err(response) => return response,
            };
        let requested = query
            .presentation
            .filter(|presentation| presentation.as_uuid() != item_id);
        let presentation = if let Some(requested) = requested {
            requested
        } else {
            let Some(catalog) = state.catalog.as_ref() else {
                return response(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
            };
            let sources = match catalog
                .available_playback_sources(
                    principal.user().id(),
                    None,
                    CatalogItemId::from_uuid(item_id),
                )
                .await
            {
                Ok(Some(sources)) => sources,
                Ok(None) => return response(StatusCode::NOT_FOUND, "media source was not found"),
                Err(_) => {
                    return response(StatusCode::SERVICE_UNAVAILABLE, "catalog is unavailable");
                }
            };
            let Some(presentation) = sources
                .first()
                .map(tjxy_application::PlaybackSource::presentation_key)
            else {
                return response(StatusCode::NOT_FOUND, "media source was not found");
            };
            presentation
        };
        (principal.user().id(), presentation)
    } else if let Some(ticket) = query.playback_ticket.as_deref() {
        let Some(presentation) = query.presentation else {
            return response(StatusCode::BAD_REQUEST, "invalid stream query");
        };
        let Some(service) = state.playback_tickets.as_ref() else {
            return response(
                StatusCode::SERVICE_UNAVAILABLE,
                "playback tickets are unavailable",
            );
        };
        match service
            .authorize(ticket, CatalogItemId::from_uuid(item_id), presentation)
            .await
        {
            Ok(Some(grant)) => (grant.user_id(), presentation),
            Ok(None) | Err(_) => {
                return response(StatusCode::UNAUTHORIZED, "authentication required");
            }
        }
    } else {
        return response(StatusCode::UNAUTHORIZED, "authentication required");
    };
    let Some(media) = state.media.as_ref() else {
        return response(
            StatusCode::SERVICE_UNAVAILABLE,
            "media backend is unavailable",
        );
    };
    let resolved = match media
        .resolve(user_id, CatalogItemId::from_uuid(item_id), presentation)
        .await
    {
        Ok(Some(resolved)) => resolved,
        Ok(None) => return response(StatusCode::NOT_FOUND, "media source was not found"),
        Err(MediaReadError::BackendUnavailable) => {
            return response(
                StatusCode::SERVICE_UNAVAILABLE,
                "media backend is unavailable",
            );
        }
        Err(_) => return response(StatusCode::SERVICE_UNAVAILABLE, "media is unavailable"),
    };
    let size = resolved.size();
    let use_range = headers
        .get(header::IF_RANGE)
        .is_none_or(|value| value.as_bytes() == resolved.etag().as_bytes());
    if size == 0 {
        if use_range && headers.contains_key(header::RANGE) {
            return unsatisfied(0);
        }
        return build_response(
            StatusCode::OK,
            0,
            resolved.etag(),
            resolved.content_type(),
            None,
            Body::empty(),
        );
    }
    let selected = if use_range {
        match requested_range(&headers, size) {
            Ok(range) => range,
            Err(()) => return unsatisfied(size),
        }
    } else {
        None
    };
    let (start, end_exclusive, status) = selected
        .map_or((0, size, StatusCode::OK), |(start, end)| {
            (start, end, StatusCode::PARTIAL_CONTENT)
        });
    let length = end_exclusive - start;
    let content_range = (status == StatusCode::PARTIAL_CONTENT)
        .then(|| format!("bytes {start}-{}/{size}", end_exclusive - 1));
    if head_only {
        return build_response(
            status,
            length,
            resolved.etag(),
            resolved.content_type(),
            content_range.as_deref(),
            Body::empty(),
        );
    }
    let Ok(range) = ByteRange::new(start, end_exclusive) else {
        return unsatisfied(size);
    };
    match resolved.open_range(range).await {
        Ok(opened) => {
            let etag = opened.etag().to_owned();
            build_response(
                status,
                length,
                &etag,
                resolved.content_type(),
                content_range.as_deref(),
                Body::from_stream(opened.into_stream()),
            )
        }
        Err(MediaReadError::RangeNotSatisfiable { size }) => unsatisfied(size),
        Err(_) => response(StatusCode::SERVICE_UNAVAILABLE, "media is unavailable"),
    }
}

struct StreamQuery {
    presentation: Option<PresentationKey>,
    playback_ticket: Option<String>,
    api_key_present: bool,
}

fn stream_query(raw_query: Option<&str>) -> Result<StreamQuery, (StatusCode, &'static str)> {
    let mut query = auth::request_query(raw_query)
        .map_err(|()| (StatusCode::BAD_REQUEST, "invalid stream query"))?;
    let api_key_present = query.remove("ApiKey").is_some() || query.remove("api_key").is_some();
    let playback_ticket = query.remove("PlaybackTicket");
    if let Some(static_value) = take_alias(&mut query, "static", "Static")?
        && !static_value.eq_ignore_ascii_case("true")
        && !static_value.eq_ignore_ascii_case("false")
    {
        return Err((StatusCode::BAD_REQUEST, "invalid stream query"));
    }
    let presentation =
        take_alias(&mut query, "mediaSourceId", "MediaSourceId")?.filter(|value| !value.is_empty());
    for hint in STREAM_COMPATIBILITY_HINTS {
        query.remove(*hint);
    }
    if !query.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "invalid stream query"));
    }
    let presentation = presentation
        .map(|presentation| {
            Uuid::parse_str(&presentation)
                .map(PresentationKey::from_uuid)
                .map_err(|_| (StatusCode::NOT_FOUND, "media source was not found"))
        })
        .transpose()?;
    Ok(StreamQuery {
        presentation,
        playback_ticket,
        api_key_present,
    })
}

fn take_alias(
    query: &mut std::collections::HashMap<String, String>,
    canonical: &str,
    alias: &str,
) -> Result<Option<String>, (StatusCode, &'static str)> {
    let canonical = query.remove(canonical);
    let alias = query.remove(alias);
    if canonical.is_some() && alias.is_some() {
        return Err((StatusCode::BAD_REQUEST, "invalid stream query"));
    }
    Ok(canonical.or(alias))
}

fn requested_range(headers: &HeaderMap, size: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(value) = headers.get(header::RANGE) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| ())?;
    let value = value.strip_prefix("bytes=").ok_or(())?;
    if value.contains(',') {
        return Err(());
    }
    let (start, end) = value.split_once('-').ok_or(())?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        return Ok(Some((size.saturating_sub(suffix), size)));
    }
    let start = start.parse::<u64>().map_err(|_| ())?;
    if start >= size {
        return Err(());
    }
    let end_exclusive = if end.is_empty() {
        size
    } else {
        end.parse::<u64>()
            .map_err(|_| ())?
            .checked_add(1)
            .ok_or(())?
            .min(size)
    };
    if start >= end_exclusive {
        return Err(());
    }
    Ok(Some((start, end_exclusive)))
}

fn build_response(
    status: StatusCode,
    content_length: u64,
    etag: &str,
    content_type: &str,
    content_range: Option<&str>,
    body: Body,
) -> Response {
    let mut builder = Response::builder()
        .status(status)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, content_length)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "private, no-cache")
        .header(header::CONTENT_TYPE, content_type);
    if let Some(content_range) = content_range {
        builder = builder.header(header::CONTENT_RANGE, content_range);
    }
    builder
        .body(body)
        .unwrap_or_else(|_| response(StatusCode::INTERNAL_SERVER_ERROR, "response failed"))
}

fn unsatisfied(size: u64) -> Response {
    let mut response = response(
        StatusCode::RANGE_NOT_SATISFIABLE,
        "range is not satisfiable",
    );
    if let Ok(value) = HeaderValue::from_str(&format!("bytes */{size}")) {
        response.headers_mut().insert(header::CONTENT_RANGE, value);
    }
    response
}

fn response(status: StatusCode, message: &'static str) -> Response {
    (status, message).into_response()
}
