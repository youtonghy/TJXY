use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use tjxy_application::MediaReadError;
use tjxy_common::{CatalogItemId, PresentationKey};
use tjxy_storage::ByteRange;
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) async fn get(
    State(state): State<AppState>,
    Path((item_id, presentation, delivery_index, stream)): Path<(Uuid, Uuid, i32, String)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    serve(
        state,
        item_id,
        presentation,
        delivery_index,
        None,
        &stream,
        headers,
        raw_query,
    )
    .await
}

pub(crate) async fn get_with_offset(
    State(state): State<AppState>,
    Path((item_id, presentation, delivery_index, start_position_ticks, stream)): Path<(
        Uuid,
        Uuid,
        i32,
        i64,
        String,
    )>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    serve(
        state,
        item_id,
        presentation,
        delivery_index,
        Some(start_position_ticks),
        &stream,
        headers,
        raw_query,
    )
    .await
}

#[allow(clippy::too_many_arguments)] // The arguments are the two Jellyfin route shapes plus request state.
async fn serve(
    state: AppState,
    item_id: Uuid,
    presentation: Uuid,
    delivery_index: i32,
    start_position_ticks: Option<i64>,
    stream: &str,
    headers: HeaderMap,
    raw_query: Option<String>,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !valid_query(raw_query.as_deref()) {
        return error(StatusCode::BAD_REQUEST, "unsupported subtitle query");
    }
    if start_position_ticks.is_some_and(|ticks| ticks != 0) {
        return error(
            StatusCode::BAD_REQUEST,
            "subtitle time offsets are unsupported",
        );
    }
    let Some(format) = stream
        .strip_prefix("Stream.")
        .filter(|format| valid_format(format))
    else {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported subtitle format",
        );
    };
    let Some(media) = state.media.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "media backend is unavailable",
        );
    };
    let resolved = match media
        .resolve_subtitle(
            principal.user().id(),
            CatalogItemId::from_uuid(item_id),
            PresentationKey::from_uuid(presentation),
            delivery_index,
        )
        .await
    {
        Ok(Some(resolved)) => resolved,
        Ok(None) => return error(StatusCode::NOT_FOUND, "subtitle was not found"),
        Err(MediaReadError::BackendUnavailable) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "media backend is unavailable",
            );
        }
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "subtitle is unavailable"),
    };
    if resolved.format() != format {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "subtitle format conversion is unsupported",
        );
    }
    let resolved = resolved.into_media();
    let size = resolved.size();
    let etag = resolved.etag().to_owned();
    let body = if size == 0 {
        Body::empty()
    } else {
        let Ok(range) = ByteRange::new(0, size) else {
            return error(StatusCode::SERVICE_UNAVAILABLE, "subtitle is unavailable");
        };
        match resolved.open_range(range).await {
            Ok(opened) => Body::from_stream(opened.into_stream()),
            Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "subtitle is unavailable"),
        }
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type(format))
        .header(header::CONTENT_LENGTH, size)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "private, no-cache")
        .body(body)
        .unwrap_or_else(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "response failed"))
}

fn valid_query(raw_query: Option<&str>) -> bool {
    let Ok(mut query) = auth::request_query(raw_query) else {
        return false;
    };
    query.remove("ApiKey");
    query.remove("api_key");
    for hint in [
        "deviceId",
        "DeviceId",
        "playSessionId",
        "PlaySessionId",
        "tag",
        "Tag",
    ] {
        query.remove(hint);
    }
    query.is_empty()
}

fn valid_format(format: &str) -> bool {
    !format.is_empty()
        && format.len() <= 32
        && format
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn content_type(format: &str) -> &'static str {
    match format {
        "srt" => "application/x-subrip",
        "vtt" => "text/vtt; charset=utf-8",
        "ass" | "ssa" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, message).into_response()
}
