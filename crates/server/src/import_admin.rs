use std::sync::Arc;

use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tjxy_credentials::{CredentialCipher, CredentialCipherError};
use tjxy_db::{
    CreatedImportRuntime, ImportJobRecord, ImportJobRepository, ImportPublicationError,
    ImportPublicationRepository, ImportPublicationTarget, ImportRuntimeDraft,
    ImportRuntimeRepository, ImportRuntimeRepositoryError, ImportStagingRepositoryError,
};
use tjxy_import::{EmbyApiCredentials, EmbyImportError};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{AppState, auth};

pub(crate) struct ImportAdminService {
    database: DatabaseConnection,
    cipher: Arc<CredentialCipher>,
}

impl ImportAdminService {
    pub(crate) const fn new(database: DatabaseConnection, cipher: Arc<CredentialCipher>) -> Self {
        Self { database, cipher }
    }

    async fn create_emby(
        &self,
        request: CreateEmbyImportRequest,
    ) -> Result<CreatedImportRuntime, ImportAdminError> {
        let credentials = EmbyApiCredentials::new(
            &request.base_url,
            request.emby_user_id,
            request.api_key.to_string(),
        )?;
        let payload = credentials.to_payload_json()?;
        let source_id = Uuid::new_v4();
        let envelope = self.cipher.seal(source_id, "emby-import", &payload)?;
        let draft = ImportRuntimeDraft::new(
            source_id,
            request.source_instance_id,
            request.dry_run,
            envelope,
            request.target_library_id,
            request.target_user_id,
        )?;
        Ok(ImportRuntimeRepository::new(&self.database)
            .create_emby(&draft)
            .await?)
    }

    async fn status(&self, job_id: Uuid) -> Result<ImportJobRecord, ImportAdminError> {
        ImportJobRepository::new(&self.database)
            .get(job_id)
            .await?
            .ok_or(ImportAdminError::NotFound)
    }

    async fn pause(&self, job_id: Uuid) -> Result<(), ImportAdminError> {
        Ok(ImportJobRepository::new(&self.database)
            .pause(job_id)
            .await?)
    }

    async fn resume(&self, job_id: Uuid) -> Result<(), ImportAdminError> {
        Ok(ImportJobRepository::new(&self.database)
            .resume(job_id)
            .await?)
    }

    async fn publish(
        &self,
        job_id: Uuid,
    ) -> Result<tjxy_db::ImportPublicationReport, ImportAdminError> {
        let source = ImportRuntimeRepository::new(&self.database)
            .source_for_job(job_id)
            .await?
            .ok_or(ImportAdminError::NotFound)?;
        Ok(ImportPublicationRepository::new(&self.database)
            .publish(
                job_id,
                ImportPublicationTarget::new(source.target_library_id(), source.target_user_id()),
            )
            .await?)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct CreateEmbyImportDto {
    base_url: String,
    emby_user_id: String,
    api_key: Zeroizing<String>,
    source_instance_id: String,
    dry_run: bool,
    target_library_id: Uuid,
    target_user_id: Uuid,
}

struct CreateEmbyImportRequest {
    base_url: String,
    emby_user_id: String,
    api_key: Zeroizing<String>,
    source_instance_id: String,
    dry_run: bool,
    target_library_id: Uuid,
    target_user_id: Uuid,
}

pub(crate) async fn create_emby(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: CreateEmbyImportDto = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.import_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let request = CreateEmbyImportRequest {
        base_url: request.base_url,
        emby_user_id: request.emby_user_id,
        api_key: request.api_key,
        source_instance_id: request.source_instance_id,
        dry_run: request.dry_run,
        target_library_id: request.target_library_id,
        target_user_id: request.target_user_id,
    };
    match service.create_emby(request).await {
        Ok(created) => (
            StatusCode::ACCEPTED,
            Json(CreatedImportDto {
                job_id: created.job_id(),
                source_id: created.source_id(),
            }),
        )
            .into_response(),
        Err(error) => error.into_response(),
    }
}

pub(crate) async fn status(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    let Some(service) = state.import_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.status(job_id).await {
        Ok(job) => Json(ImportStatusDto::from(&job)).into_response(),
        Err(error) => error.into_response(),
    }
}

pub(crate) async fn pause(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    command_response(&state, &headers, raw_query.as_deref(), |service| {
        Box::pin(service.pause(job_id))
    })
    .await
}

pub(crate) async fn resume(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    command_response(&state, &headers, raw_query.as_deref(), |service| {
        Box::pin(service.resume(job_id))
    })
    .await
}

pub(crate) async fn publish(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return response;
    }
    let Some(service) = state.import_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.publish(job_id).await {
        Ok(report) => Json(PublishedImportDto {
            items: report.items(),
            replayed: report.replayed(),
        })
        .into_response(),
        Err(error) => error.into_response(),
    }
}

async fn administrator(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<(), Response> {
    auth::authenticated_administrator(state, headers, query)
        .await
        .map(|_| ())
}

async fn command_response<'service>(
    state: &'service AppState,
    headers: &HeaderMap,
    query: Option<&str>,
    command: impl FnOnce(
        &'service ImportAdminService,
    ) -> std::pin::Pin<
        Box<dyn Future<Output = Result<(), ImportAdminError>> + Send + 'service>,
    >,
) -> Response {
    if let Err(response) = administrator(state, headers, query).await {
        return response;
    }
    let Some(service) = state.import_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match command(service).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error.into_response(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CreatedImportDto {
    job_id: Uuid,
    source_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ImportStatusDto {
    id: Uuid,
    adapter_kind: String,
    source_instance_id: String,
    state: String,
    dry_run: bool,
    checkpoint: Value,
    counters: Value,
    attempt_count: i32,
}

impl From<&ImportJobRecord> for ImportStatusDto {
    fn from(job: &ImportJobRecord) -> Self {
        Self {
            id: job.id(),
            adapter_kind: job.adapter_kind().to_owned(),
            source_instance_id: job.source_instance_id().to_owned(),
            state: job.state().as_str().to_owned(),
            dry_run: job.dry_run(),
            checkpoint: job.checkpoint().clone(),
            counters: job.counters().clone(),
            attempt_count: job.attempt_count(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PublishedImportDto {
    items: usize,
    replayed: bool,
}

#[derive(Debug, Error)]
enum ImportAdminError {
    #[error("import job was not found")]
    NotFound,
    #[error("Emby import request is invalid: {0}")]
    Import(#[from] EmbyImportError),
    #[error("import credential encryption failed: {0}")]
    Cipher(#[from] CredentialCipherError),
    #[error("import runtime persistence failed: {0}")]
    Runtime(#[from] ImportRuntimeRepositoryError),
    #[error("import job state update failed: {0}")]
    Job(#[from] ImportStagingRepositoryError),
    #[error("import publication failed: {0}")]
    Publication(#[from] ImportPublicationError),
}

impl IntoResponse for ImportAdminError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
            Self::Import(_) | Self::Runtime(ImportRuntimeRepositoryError::InvalidDraft) => {
                StatusCode::BAD_REQUEST.into_response()
            }
            Self::Job(ImportStagingRepositoryError::InvalidTransition)
            | Self::Publication(
                ImportPublicationError::NotReady
                | ImportPublicationError::InvalidStaging
                | ImportPublicationError::MissingParent,
            ) => StatusCode::CONFLICT.into_response(),
            Self::Cipher(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            Self::Runtime(_)
            | Self::Job(_)
            | Self::Publication(
                ImportPublicationError::Database(_) | ImportPublicationError::RollbackFailed { .. },
            ) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        }
    }
}
