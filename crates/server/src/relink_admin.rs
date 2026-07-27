use axum::{
    Json,
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use tjxy_db::{
    StorageRelinkCandidate, StorageRelinkDecision, StorageRelinkDecisionReport,
    StorageRelinkRepository, StorageRelinkRepositoryError,
};
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) struct RelinkAdminService {
    database: DatabaseConnection,
}

impl RelinkAdminService {
    pub(crate) const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    async fn pending(
        &self,
        limit: u64,
    ) -> Result<Vec<StorageRelinkCandidate>, StorageRelinkRepositoryError> {
        StorageRelinkRepository::new(&self.database)
            .pending(limit)
            .await
    }

    async fn decide(
        &self,
        id: Uuid,
        decision: StorageRelinkDecision,
    ) -> Result<StorageRelinkDecisionReport, StorageRelinkRepositoryError> {
        StorageRelinkRepository::new(&self.database)
            .decide(id, decision)
            .await
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct PendingQuery {
    #[serde(default = "default_limit")]
    limit: u64,
}

const fn default_limit() -> u64 {
    50
}

pub(crate) async fn pending(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    Query(query): Query<PendingQuery>,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    let Some(service) = state.relink_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.pending(query.limit).await {
        Ok(items) => Json(StorageRelinkPageDto {
            items: items.into_iter().map(Into::into).collect(),
        })
        .into_response(),
        Err(error) => error_response(&error),
    }
}

pub(crate) async fn confirm(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    decide(
        &state,
        &headers,
        raw_query.as_deref(),
        id,
        StorageRelinkDecision::Confirm,
    )
    .await
}

pub(crate) async fn reject(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    decide(
        &state,
        &headers,
        raw_query.as_deref(),
        id,
        StorageRelinkDecision::Reject,
    )
    .await
}

async fn decide(
    state: &AppState,
    headers: &HeaderMap,
    raw_query: Option<&str>,
    id: Uuid,
    decision: StorageRelinkDecision,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(state, headers, raw_query).await {
        return response;
    }
    let Some(service) = state.relink_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.decide(id, decision).await {
        Ok(report) => Json(StorageRelinkDecisionDto {
            changed: report.changed(),
            state: report.state(),
        })
        .into_response(),
        Err(error) => error_response(&error),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct StorageRelinkPageDto {
    items: Vec<StorageRelinkCandidateDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct StorageRelinkCandidateDto {
    id: Uuid,
    storage_root_id: Uuid,
    previous_storage_object_id: Uuid,
    replacement_storage_object_id: Uuid,
    previous_name: String,
    replacement_name: String,
    confidence: f64,
    evidence: serde_json::Value,
    state: String,
}

impl From<StorageRelinkCandidate> for StorageRelinkCandidateDto {
    fn from(candidate: StorageRelinkCandidate) -> Self {
        Self {
            id: candidate.id(),
            storage_root_id: candidate.root_id(),
            previous_storage_object_id: candidate.previous_object_id(),
            replacement_storage_object_id: candidate.replacement_object_id(),
            previous_name: candidate.previous_name().to_owned(),
            replacement_name: candidate.replacement_name().to_owned(),
            confidence: candidate.confidence(),
            evidence: candidate.evidence().clone(),
            state: candidate.state().to_owned(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct StorageRelinkDecisionDto {
    changed: bool,
    state: &'static str,
}

fn error_response(error: &StorageRelinkRepositoryError) -> Response {
    match error {
        StorageRelinkRepositoryError::InvalidLimit => StatusCode::BAD_REQUEST.into_response(),
        StorageRelinkRepositoryError::NotFound => StatusCode::NOT_FOUND.into_response(),
        StorageRelinkRepositoryError::DecisionConflict
        | StorageRelinkRepositoryError::StaleCandidate
        | StorageRelinkRepositoryError::IdentityConflict => StatusCode::CONFLICT.into_response(),
        StorageRelinkRepositoryError::Database(_)
        | StorageRelinkRepositoryError::RollbackFailed { .. } => {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}
