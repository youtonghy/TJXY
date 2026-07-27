use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tjxy_application::{MetadataImportError, MetadataImportReport};
use tjxy_common::CatalogItemId;
use tjxy_db::MetadataPublicationError;
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) async fn import_nfo(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await
    {
        return response;
    }
    if !is_xml_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.metadata_import.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .import_nfo(CatalogItemId::from_uuid(item_id), &body, "admin:nfo")
        .await
    {
        Ok(report) => Json(MetadataImportDto::from(report)).into_response(),
        Err(error) => error_response(&error),
    }
}

fn is_xml_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .is_some_and(|value| matches!(value, "application/xml" | "text/xml"))
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct MetadataImportDto {
    changed: bool,
    state: &'static str,
}

impl From<MetadataImportReport> for MetadataImportDto {
    fn from(report: MetadataImportReport) -> Self {
        Self {
            changed: report.changed(),
            state: report.state().as_str(),
        }
    }
}

fn error_response(error: &MetadataImportError) -> Response {
    match error {
        MetadataImportError::Nfo(_) => StatusCode::BAD_REQUEST.into_response(),
        MetadataImportError::NfoKindMismatch
        | MetadataImportError::Publication(
            MetadataPublicationError::ItemKindMismatch
            | MetadataPublicationError::InvalidResolution,
        ) => StatusCode::CONFLICT.into_response(),
        MetadataImportError::Publication(MetadataPublicationError::ItemNotFound) => {
            StatusCode::NOT_FOUND.into_response()
        }
        MetadataImportError::Publication(
            MetadataPublicationError::Database(_) | MetadataPublicationError::RollbackFailed { .. },
        ) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
