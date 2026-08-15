use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use tjxy_application::{AssetReadError, DirectMetadataReadError, OpenedAsset, OpenedDirectImage};
use tjxy_common::{CatalogItemId, ImageType};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) async fn get_original(
    State(state): State<AppState>,
    Path((item_id, image_type)): Path<(Uuid, String)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    original(
        state,
        item_id,
        &image_type,
        &headers,
        raw_query.as_deref(),
        false,
    )
    .await
}

pub(crate) async fn head_original(
    State(state): State<AppState>,
    Path((item_id, image_type)): Path<(Uuid, String)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    original(
        state,
        item_id,
        &image_type,
        &headers,
        raw_query.as_deref(),
        true,
    )
    .await
}

async fn original(
    state: AppState,
    item_id: Uuid,
    image_type: &str,
    headers: &HeaderMap,
    raw_query: Option<&str>,
    head_only: bool,
) -> Response {
    if let Err(response) = auth::authenticated_principal(&state, headers, raw_query).await {
        return response;
    }
    let Ok(image_type) = image_type.parse::<ImageType>() else {
        return error(StatusCode::BAD_REQUEST, "unsupported image type");
    };
    if !valid_query(raw_query) {
        return error(StatusCode::BAD_REQUEST, "unsupported image query");
    }
    let Some(assets) = state.assets.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "asset service is unavailable",
        );
    };
    let asset = match assets
        .original(CatalogItemId::from_uuid(item_id), image_type, 0)
        .await
    {
        Ok(Some(asset)) => asset,
        Ok(None) => {
            let Some(direct) = state.direct_metadata.as_ref() else {
                return error(StatusCode::NOT_FOUND, "image was not found");
            };
            return match direct
                .image(CatalogItemId::from_uuid(item_id), image_type, 0)
                .await
            {
                Ok(Some(image)) => direct_image_response(image, headers, head_only),
                Ok(None) => error(StatusCode::NOT_FOUND, "image was not found"),
                Err(DirectMetadataReadError::Query(_))
                | Err(DirectMetadataReadError::BackendUnavailable) => error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "image service is unavailable",
                ),
                Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "image data is invalid"),
            };
        }
        Err(AssetReadError::Query(_)) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "asset service is unavailable",
            );
        }
        Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "asset data is invalid"),
    };
    asset_response(asset, headers, head_only)
}

fn direct_image_response(
    image: OpenedDirectImage,
    request_headers: &HeaderMap,
    head_only: bool,
) -> Response {
    let etag = format!("\"{}\"", image.etag());
    if etag_matches(request_headers, &etag) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, etag)
            .header(header::CACHE_CONTROL, "private, max-age=0, must-revalidate")
            .body(Body::empty())
            .unwrap_or_else(|_| {
                error(StatusCode::INTERNAL_SERVER_ERROR, "invalid image metadata")
            });
    }
    let mime_type = image.mime_type();
    let size = image.size();
    let body = if head_only {
        Body::empty()
    } else {
        Body::from_stream(image.into_stream())
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_LENGTH, size)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "private, max-age=0, must-revalidate")
        .body(body)
        .unwrap_or_else(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "invalid image metadata"))
}

fn valid_query(raw_query: Option<&str>) -> bool {
    let Ok(mut parameters) = auth::request_query(raw_query) else {
        return false;
    };
    parameters.remove("ApiKey");
    parameters.remove("api_key");
    parameters.remove("tag");
    parameters.is_empty()
}

fn asset_response(asset: OpenedAsset, request_headers: &HeaderMap, head_only: bool) -> Response {
    let etag = format!("\"{}\"", asset.sha256());
    if etag_matches(request_headers, &etag) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, etag)
            .header(header::CACHE_CONTROL, "private, max-age=0, must-revalidate")
            .body(Body::empty())
            .unwrap_or_else(|_| {
                error(StatusCode::INTERNAL_SERVER_ERROR, "invalid asset metadata")
            });
    }
    let mime_type = asset.mime_type().to_owned();
    let byte_size = asset.byte_size();
    let body = if head_only {
        Body::empty()
    } else {
        let mut file = asset.into_file();
        Body::from_stream(async_stream::stream! {
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                match file.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(read) => yield Ok::<Bytes, std::io::Error>(Bytes::copy_from_slice(&buffer[..read])),
                    Err(error) => {
                        yield Err::<Bytes, std::io::Error>(error);
                        break;
                    }
                }
            }
        })
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_LENGTH, byte_size)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "private, max-age=0, must-revalidate")
        .body(body)
        .unwrap_or_else(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "invalid asset metadata"))
}

fn etag_matches(headers: &HeaderMap, current: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.split(',').any(|candidate| {
                let candidate = candidate.trim();
                candidate == "*"
                    || candidate == current
                    || candidate.strip_prefix("W/") == Some(current)
            })
        })
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, message).into_response()
}
