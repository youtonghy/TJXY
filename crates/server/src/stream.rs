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
    let user_id = if headers.contains_key(header::AUTHORIZATION)
        || query.api_key_present
        || headers.contains_key("X-Emby-Token")
        || headers.contains_key("X-MediaBrowser-Token")
    {
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal.user().id(),
            Err(response) => return response,
        }
    } else if let Some(ticket) = query.playback_ticket.as_deref() {
        let Some(service) = state.playback_tickets.as_ref() else {
            return response(
                StatusCode::SERVICE_UNAVAILABLE,
                "playback tickets are unavailable",
            );
        };
        match service
            .authorize(
                ticket,
                CatalogItemId::from_uuid(item_id),
                query.presentation,
            )
            .await
        {
            Ok(Some(grant)) => grant.user_id(),
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
        .resolve(
            user_id,
            CatalogItemId::from_uuid(item_id),
            query.presentation,
        )
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
    presentation: PresentationKey,
    playback_ticket: Option<String>,
    api_key_present: bool,
}

fn stream_query(raw_query: Option<&str>) -> Result<StreamQuery, (StatusCode, &'static str)> {
    let mut query = auth::request_query(raw_query)
        .map_err(|()| (StatusCode::BAD_REQUEST, "invalid stream query"))?;
    let api_key_present = query.remove("ApiKey").is_some() || query.remove("api_key").is_some();
    let playback_ticket = query.remove("PlaybackTicket");
    if query.remove("static").as_deref() != Some("true") {
        return Err((StatusCode::BAD_REQUEST, "invalid stream query"));
    }
    let presentation = query
        .remove("mediaSourceId")
        .ok_or((StatusCode::BAD_REQUEST, "invalid stream query"))?;
    if !query.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "invalid stream query"));
    }
    let presentation = Uuid::parse_str(&presentation)
        .map(PresentationKey::from_uuid)
        .map_err(|_| (StatusCode::NOT_FOUND, "media source was not found"))?;
    Ok(StreamQuery {
        presentation,
        playback_ticket,
        api_key_present,
    })
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
