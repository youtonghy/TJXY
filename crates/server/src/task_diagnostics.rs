use axum::{
    Json,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tjxy_application::TaskServiceError;
use tjxy_common::{CatalogItemId, StorageRootId, WorkJobId};
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) async fn nfo_choices(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return response;
    }
    let Ok(offset) = page_offset(raw.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(tasks) = &state.tasks else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks.nfo_choices(offset).await {
        Ok(choices) => Json(choices).into_response(),
        Err(error) => diagnostic_error(&error),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub(crate) struct NfoChoiceRequest {
    item_id: Uuid,
    root_id: Uuid,
    candidate_id: Uuid,
    fingerprint: String,
}

pub(crate) async fn choose_nfo(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    Json(body): Json<NfoChoiceRequest>,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return response;
    }
    if body.fingerprint.len() != 64
        || !body
            .fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(tasks) = &state.tasks else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .choose_nfo(
            CatalogItemId::from_uuid(body.item_id),
            StorageRootId::from_uuid(body.root_id),
            body.candidate_id,
            &body.fingerprint,
        )
        .await
    {
        Ok(job) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({"JobId": job.job().id().as_uuid()})),
        )
            .into_response(),
        Err(error) => diagnostic_error(&error),
    }
}

pub(crate) async fn scan_report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return response;
    }
    let Ok(offset) = page_offset(raw.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(tasks) = &state.tasks else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks.scan_report(WorkJobId::from_uuid(id), offset).await {
        Ok(report) => Json(report).into_response(),
        Err(error) => diagnostic_error(&error),
    }
}

pub(crate) async fn retry_scan_issues(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return response;
    }
    let Ok(offset) = page_offset(raw.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(tasks) = &state.tasks else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .retry_scan_issues(WorkJobId::from_uuid(id), offset)
        .await
    {
        Ok(jobs) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({"JobIds": jobs})),
        )
            .into_response(),
        Err(error) => diagnostic_error(&error),
    }
}

fn page_offset(raw: Option<&str>) -> Result<u64, ()> {
    let mut query = auth::request_query(raw)?;
    query.remove("api_key");
    query.remove("ApiKey");
    let offset = query
        .remove("Offset")
        .map_or(Ok(0), |value| value.parse::<u64>().map_err(|_| ()))?;
    if !query.is_empty() || offset > 1_000_000 {
        return Err(());
    }
    Ok(offset)
}

fn diagnostic_error(error: &TaskServiceError) -> Response {
    match error {
        TaskServiceError::StaleDiagnostic => StatusCode::CONFLICT.into_response(),
        TaskServiceError::ManualMediaItemUnavailable => StatusCode::NOT_FOUND.into_response(),
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn work_health(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return response;
    }
    let Some(tasks) = &state.tasks else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks.work_health().await {
        Ok(health) => Json(serde_json::json!({ "Health": health, "Cleanup": crate::worker::retention_observation() })).into_response(),
        Err(error) => diagnostic_error(&error),
    }
}

pub(crate) async fn scan_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return response;
    }
    let Ok(offset) = page_offset(raw.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(tasks) = &state.tasks else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks.scan_history(offset).await {
        Ok(scans) => Json(scans).into_response(),
        Err(error) => diagnostic_error(&error),
    }
}
