use axum::{
    Json,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tjxy_api::{AdminHybridCandidateInfo, AdminHybridCandidatePage};
use tjxy_application::TaskServiceError;
use tjxy_common::{CatalogItemId, LibraryId};
use tjxy_db::HybridCandidateError;
use uuid::Uuid;

use crate::{AppState, auth};

const DEFAULT_LIMIT: u64 = 50;

pub(crate) async fn list(
    State(state): State<AppState>,
    Path(library_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    let Some((start_index, limit)) = page_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .hybrid_candidates(LibraryId::from_uuid(library_id), start_index, limit)
        .await
    {
        Ok(page) => Json(AdminHybridCandidatePage::new(
            page.items()
                .iter()
                .map(|item| {
                    AdminHybridCandidateInfo::new(
                        item.catalog_item_id().as_uuid(),
                        item.name(),
                        item.production_year(),
                        item.structure_state(),
                        item.selected_at(),
                    )
                })
                .collect(),
            page.total_record_count(),
            page.start_index(),
        ))
        .into_response(),
        Err(error) => candidate_error(&error),
    }
}

pub(crate) async fn pin(
    State(state): State<AppState>,
    Path((library_id, item_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    if !empty_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .pin_hybrid_candidate(
            LibraryId::from_uuid(library_id),
            CatalogItemId::from_uuid(item_id),
        )
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => candidate_error(&error),
    }
}

pub(crate) async fn unpin(
    State(state): State<AppState>,
    Path((library_id, item_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    if !empty_query(raw_query.as_deref()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .unpin_hybrid_candidate(
            LibraryId::from_uuid(library_id),
            CatalogItemId::from_uuid(item_id),
        )
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => candidate_error(&error),
    }
}

fn page_query(raw_query: Option<&str>) -> Option<(u64, u64)> {
    let mut query = auth::request_query(raw_query).ok()?;
    query.remove("ApiKey");
    query.remove("api_key");
    let start_index = query
        .remove("StartIndex")
        .map_or(Some(0), |value| value.parse().ok())?;
    let limit = query
        .remove("Limit")
        .map_or(Some(DEFAULT_LIMIT), |value| value.parse().ok())?;
    query.is_empty().then_some((start_index, limit))
}

fn empty_query(raw_query: Option<&str>) -> bool {
    let Ok(mut query) = auth::request_query(raw_query) else {
        return false;
    };
    query.remove("ApiKey");
    query.remove("api_key");
    query.is_empty()
}

fn candidate_error(error: &TaskServiceError) -> Response {
    match error {
        TaskServiceError::HybridCandidate(HybridCandidateError::InvalidPage) => {
            StatusCode::BAD_REQUEST.into_response()
        }
        TaskServiceError::HybridCandidate(
            HybridCandidateError::LibraryUnavailable | HybridCandidateError::ItemUnavailable,
        ) => StatusCode::NOT_FOUND.into_response(),
        TaskServiceError::HybridCandidate(HybridCandidateError::LibraryNotBackground) => {
            StatusCode::CONFLICT.into_response()
        }
        TaskServiceError::HybridCandidate(
            HybridCandidateError::DatabaseInvariant
            | HybridCandidateError::Database(_)
            | HybridCandidateError::RollbackFailed { .. },
        )
        | TaskServiceError::Catalog(_)
        | TaskServiceError::Probe(_)
        | TaskServiceError::Repository(_)
        | TaskServiceError::Discover(_)
        | TaskServiceError::Validation(_)
        | TaskServiceError::Metadata(_)
        | TaskServiceError::FullScan(_)
        | TaskServiceError::ManualMediaItemUnavailable
        | TaskServiceError::InvalidManualMediaItemType => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

async fn administrator(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<tjxy_application::AuthenticatedPrincipal, Response> {
    auth::authenticated_administrator(state, headers, query).await
}
