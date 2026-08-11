use axum::{
    Json,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tjxy_api::{AdminTaskJobInfo, AdminTaskJobOutcome, AdminTaskJobStatus, ScheduledTaskInfo};
use tjxy_application::TaskServiceError;
use tjxy_common::{CatalogItemId, LibraryId, StorageRootId};
use tjxy_db::{
    DiscoverTitlesError, FullScanRepositoryError, ManualProbeError, MetadataWorkError,
    StorageSyncRepositoryError, WorkJobAdminOutcome, WorkJobAdminRecord, WorkJobAdminStatus,
};
use uuid::Uuid;

use crate::{AppState, auth};

const FULL_MEDIA_SCAN_TASK_ID: Uuid = Uuid::from_u128(0x05e4_277b_c267_4c27_81b3_2c18_1db4_9279);
const DEFAULT_RECENT_JOB_LIMIT: u64 = 50;

pub(crate) async fn recent_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    let Ok(mut query) = auth::request_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    query.remove("ApiKey");
    query.remove("api_key");
    let limit = match query.remove("Limit") {
        Some(limit) => match limit.parse::<u64>() {
            Ok(limit) => limit,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        },
        None => DEFAULT_RECENT_JOB_LIMIT,
    };
    if !query.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks.recent_jobs(limit).await {
        Ok(jobs) => Json(jobs.iter().map(admin_job_info).collect::<Vec<_>>()).into_response(),
        Err(TaskServiceError::Repository(
            tjxy_db::WorkJobRepositoryError::InvalidObservationLimit,
        )) => StatusCode::BAD_REQUEST.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn scheduled_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    match full_media_scan(&state).await {
        Ok(task) => Json(vec![task]).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn scheduled_task(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    if task_id != FULL_MEDIA_SCAN_TASK_ID {
        return StatusCode::NOT_FOUND.into_response();
    }
    match full_media_scan(&state).await {
        Ok(task) => Json(task).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn start_scheduled_task(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    if task_id != FULL_MEDIA_SCAN_TASK_ID {
        return StatusCode::NOT_FOUND.into_response();
    }
    refresh(&state).await
}

pub(crate) async fn cancel_scheduled_task(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    if task_id != FULL_MEDIA_SCAN_TASK_ID {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks.cancel_full_media_scan().await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn refresh_library(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    refresh(&state).await
}

pub(crate) async fn discover_titles(
    State(state): State<AppState>,
    Path(root_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .discover_titles(StorageRootId::from_uuid(root_id))
        .await
    {
        Ok(submission) => (
            StatusCode::ACCEPTED,
            Json(ManualTaskSubmission {
                job_id: submission.job().id().as_uuid(),
            }),
        )
            .into_response(),
        Err(error) => manual_task_error(&error),
    }
}

pub(crate) async fn validate_storage(
    State(state): State<AppState>,
    Path(root_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .validate_storage(StorageRootId::from_uuid(root_id))
        .await
    {
        Ok(submission) => (
            StatusCode::ACCEPTED,
            Json(ManualTaskSubmission {
                job_id: submission.job().id().as_uuid(),
            }),
        )
            .into_response(),
        Err(error) => manual_task_error(&error),
    }
}

pub(crate) async fn full_scan_root(
    State(state): State<AppState>,
    Path((library_id, root_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .full_scan_root(
            LibraryId::from_uuid(library_id),
            StorageRootId::from_uuid(root_id),
        )
        .await
    {
        Ok(submission) => accepted_task(&submission),
        Err(error) => manual_task_error(&error),
    }
}

pub(crate) async fn resolve_metadata(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .resolve_metadata(CatalogItemId::from_uuid(item_id))
        .await
    {
        Ok(submission) => (
            StatusCode::ACCEPTED,
            Json(ManualTaskSubmission {
                job_id: submission.job().id().as_uuid(),
            }),
        )
            .into_response(),
        Err(error) => manual_task_error(&error),
    }
}

pub(crate) async fn probe_media(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks.probe_media(CatalogItemId::from_uuid(item_id)).await {
        Ok(submissions) => (
            StatusCode::ACCEPTED,
            Json(ManualProbeSubmission {
                jobs: submissions
                    .iter()
                    .map(|submission| ManualProbeJob {
                        media_source_id: submission.media_source_id().as_uuid(),
                        job_id: submission.submission().job().id().as_uuid(),
                        created: submission.submission().created(),
                    })
                    .collect(),
            }),
        )
            .into_response(),
        Err(error) => manual_task_error(&error),
    }
}

pub(crate) async fn expand_item(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal = match administrator(&state, &headers, raw_query.as_deref()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .expand_item(principal.user().id(), CatalogItemId::from_uuid(item_id))
        .await
    {
        Ok(submission) => accepted_task(&submission),
        Err(error) => manual_task_error(&error),
    }
}

pub(crate) async fn index_media_sources(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal = match administrator(&state, &headers, raw_query.as_deref()).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks
        .index_media_sources(principal.user().id(), CatalogItemId::from_uuid(item_id))
        .await
    {
        Ok(submission) => accepted_task(&submission),
        Err(error) => manual_task_error(&error),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ManualTaskSubmission {
    job_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ManualProbeSubmission {
    jobs: Vec<ManualProbeJob>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ManualProbeJob {
    media_source_id: Uuid,
    job_id: Uuid,
    created: bool,
}

fn accepted_task(submission: &tjxy_db::WorkJobSubmission) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(ManualTaskSubmission {
            job_id: submission.job().id().as_uuid(),
        }),
    )
        .into_response()
}

fn manual_task_error(error: &TaskServiceError) -> Response {
    match error {
        TaskServiceError::Probe(
            ManualProbeError::NoActiveMediaSources
            | ManualProbeError::NoAvailableMediaSources
            | ManualProbeError::TooManyMediaSources,
        )
        | TaskServiceError::Discover(DiscoverTitlesError::AlreadyCurrent) => {
            StatusCode::CONFLICT.into_response()
        }
        TaskServiceError::InvalidManualMediaItemType => StatusCode::CONFLICT.into_response(),
        TaskServiceError::ManualMediaItemUnavailable
        | TaskServiceError::FullScan(FullScanRepositoryError::UnavailableLibraryRoot)
        | TaskServiceError::Probe(ManualProbeError::ItemUnavailable)
        | TaskServiceError::Discover(
            DiscoverTitlesError::StaleRoot
            | DiscoverTitlesError::InvalidClaim
            | DiscoverTitlesError::UnsupportedCollection
            | DiscoverTitlesError::InvalidTitle
            | DiscoverTitlesError::IdentityConflict
            | DiscoverTitlesError::TitleLimit,
        )
        | TaskServiceError::Validation(StorageSyncRepositoryError::MissingScope)
        | TaskServiceError::Metadata(
            MetadataWorkError::AmbiguousStorageScope
            | MetadataWorkError::StaleOrUnavailable
            | MetadataWorkError::InvalidClaim
            | MetadataWorkError::MissingSyncRevision
            | MetadataWorkError::TooManySidecars
            | MetadataWorkError::TooManyImages
            | MetadataWorkError::AmbiguousSidecars
            | MetadataWorkError::InvalidSidecarSize
            | MetadataWorkError::InvalidStoredMetadata,
        ) => StatusCode::NOT_FOUND.into_response(),
        TaskServiceError::FullScan(FullScanRepositoryError::InvalidCandidateLimit) => {
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        TaskServiceError::Catalog(_)
        | TaskServiceError::Repository(_)
        | TaskServiceError::FullScan(
            FullScanRepositoryError::InvalidClaim
            | FullScanRepositoryError::StaleLibrary
            | FullScanRepositoryError::InvalidStoredPolicy
            | FullScanRepositoryError::CorruptHybridCandidateBatch
            | FullScanRepositoryError::CorruptRootDependency { .. }
            | FullScanRepositoryError::Database(_)
            | FullScanRepositoryError::Work(_)
            | FullScanRepositoryError::RollbackFailed { .. },
        )
        | TaskServiceError::Probe(
            ManualProbeError::InvalidSourceLimit
            | ManualProbeError::StalePublication
            | ManualProbeError::Publication(_)
            | ManualProbeError::Work(_)
            | ManualProbeError::Database(_)
            | ManualProbeError::RollbackFailed { .. },
        )
        | TaskServiceError::Validation(_)
        | TaskServiceError::Discover(
            DiscoverTitlesError::Database(_)
            | DiscoverTitlesError::Publication(_)
            | DiscoverTitlesError::Work(_)
            | DiscoverTitlesError::InvalidMetadataPolicy
            | DiscoverTitlesError::InvalidMetadataSourceMode
            | DiscoverTitlesError::MissingLibraryScope
            | DiscoverTitlesError::InvalidLibraryScope
            | DiscoverTitlesError::StaleLibraryPolicy
            | DiscoverTitlesError::StorageInputPending,
        )
        | TaskServiceError::Metadata(
            MetadataWorkError::Database(_)
            | MetadataWorkError::SourcePublication(_)
            | MetadataWorkError::Publication(_)
            | MetadataWorkError::Asset(_)
            | MetadataWorkError::Work(_)
            | MetadataWorkError::RequirementUpgraded,
        ) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

async fn administrator(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<tjxy_application::AuthenticatedPrincipal, Response> {
    auth::authenticated_administrator(state, headers, query).await
}

async fn full_media_scan(state: &AppState) -> Result<ScheduledTaskInfo, Response> {
    let Some(tasks) = state.tasks.as_ref() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    tasks
        .full_media_scan_active()
        .await
        .map(|active| ScheduledTaskInfo::full_media_scan(FULL_MEDIA_SCAN_TASK_ID, active))
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE.into_response())
}

async fn refresh(state: &AppState) -> Response {
    let Some(tasks) = state.tasks.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match tasks.refresh_libraries().await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn admin_job_info(record: &WorkJobAdminRecord) -> AdminTaskJobInfo {
    let job = record.job();
    AdminTaskJobInfo::new(
        job.id().as_uuid(),
        job.task_kind().as_str(),
        job.scope().scope_type(),
        job.scope().id(),
        match record.admin_status() {
            WorkJobAdminStatus::Pending => AdminTaskJobStatus::Pending,
            WorkJobAdminStatus::Retrying => AdminTaskJobStatus::Retrying,
            WorkJobAdminStatus::Running => AdminTaskJobStatus::Running,
            WorkJobAdminStatus::Completed => AdminTaskJobStatus::Completed,
            WorkJobAdminStatus::Cancelled => AdminTaskJobStatus::Cancelled,
            WorkJobAdminStatus::Failed => AdminTaskJobStatus::Failed,
        },
        job.priority(),
        job.attempt_count(),
        record.created_at(),
        record.started_at(),
        record.completed_at(),
        record.outcome().map(|outcome| match outcome {
            WorkJobAdminOutcome::NoMetadataMatch => AdminTaskJobOutcome::NoMetadataMatch,
            WorkJobAdminOutcome::CompletedWithWarnings => {
                AdminTaskJobOutcome::CompletedWithWarnings
            }
        }),
    )
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use tjxy_application::TaskServiceError;
    use tjxy_db::DiscoverTitlesError;

    use super::manual_task_error;

    #[test]
    fn pending_discovery_storage_input_is_retryable() {
        let response = manual_task_error(&TaskServiceError::Discover(
            DiscoverTitlesError::StorageInputPending,
        ));

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
