use axum::{
    Json,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tjxy_common::{CatalogItemId, PresentationKey};
use tjxy_db::{SourcePlaybackPolicy, SourcePlaybackPolicyError};
use uuid::Uuid;

use crate::{AppState, auth};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct PlaybackPolicyRequest {
    admin_priority: i32,
    is_default: bool,
    is_hidden: bool,
}

pub(crate) async fn update(
    State(state): State<AppState>,
    Path((item_id, presentation_key)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    Json(request): Json<PlaybackPolicyRequest>,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if !empty_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(catalog) = state.catalog.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let policy = SourcePlaybackPolicy::new(
        request.admin_priority,
        request.is_default,
        request.is_hidden,
    );
    match catalog
        .set_source_playback_policy(
            CatalogItemId::from_uuid(item_id),
            PresentationKey::from_uuid(presentation_key),
            policy,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(SourcePlaybackPolicyError::HiddenDefault) => StatusCode::BAD_REQUEST.into_response(),
        Err(SourcePlaybackPolicyError::SourceUnavailable) => StatusCode::NOT_FOUND.into_response(),
        Err(
            SourcePlaybackPolicyError::Database(_)
            | SourcePlaybackPolicyError::RollbackFailed { .. },
        ) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn empty_query(raw_query: Option<&str>) -> bool {
    let Ok(mut query) = auth::request_query(raw_query) else {
        return false;
    };
    query.remove("ApiKey");
    query.remove("api_key");
    query.is_empty()
}
