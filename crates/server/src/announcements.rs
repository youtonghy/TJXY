use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tjxy_db::{
    AnnouncementDraftInput, AnnouncementKind, AnnouncementRecord, AnnouncementRepository,
    AnnouncementRepositoryError, AnnouncementStatus, AnnouncementView,
};
use uuid::Uuid;

use crate::{AppState, auth};

const DEFAULT_LIMIT: u64 = 20;
const MAX_LIMIT: u64 = 100;
const MAX_OFFSET: u64 = 100_000;

pub(crate) struct AnnouncementService {
    database: sea_orm::DatabaseConnection,
}

impl AnnouncementService {
    #[must_use]
    pub const fn new(database: sea_orm::DatabaseConnection) -> Self {
        Self { database }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct AnnouncementContentRequest {
    title: String,
    body_markdown: String,
    kind: AnnouncementKindDto,
    revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct RevisionRequest {
    revision: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct AcknowledgeRequest {
    content_version: i64,
}

#[derive(Clone, Copy, Deserialize)]
enum AnnouncementKindDto {
    Popup,
    Standard,
}

impl From<AnnouncementKindDto> for AnnouncementKind {
    fn from(value: AnnouncementKindDto) -> Self {
        match value {
            AnnouncementKindDto::Popup => Self::Popup,
            AnnouncementKindDto::Standard => Self::Standard,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AdminAnnouncementDto {
    id: Uuid,
    title: String,
    body_markdown: String,
    kind: &'static str,
    status: &'static str,
    content_version: i64,
    revision: i64,
    published_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ClientAnnouncementDto {
    id: Uuid,
    title: String,
    body_markdown: String,
    kind: &'static str,
    content_version: i64,
    published_at: DateTime<Utc>,
    is_read: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PageDto<T> {
    items: Vec<T>,
    total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ClientPageDto {
    items: Vec<ClientAnnouncementDto>,
    total: u64,
    unread_count: u64,
}

pub(crate) async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return no_store(response);
    }
    let Some(service) = state.announcements.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let Some(query) = parse_admin_query(raw.as_deref()) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    match AnnouncementRepository::new(&service.database)
        .admin_page(query.status, query.kind, query.offset, query.limit)
        .await
    {
        Ok(page) => no_store(
            Json(PageDto {
                items: page.items().iter().map(admin_dto).collect(),
                total: page.total(),
            })
            .into_response(),
        ),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn admin_create(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return no_store(response);
    }
    if !query_is_auth_only(raw.as_deref()) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Ok(request) = serde_json::from_slice::<AnnouncementContentRequest>(&body) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    if request.revision.is_some() {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Some(service) = state.announcements.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let input =
        AnnouncementDraftInput::new(request.title, request.body_markdown, request.kind.into());
    match AnnouncementRepository::new(&service.database)
        .create_draft(&input)
        .await
    {
        Ok(record) => no_store((StatusCode::CREATED, Json(admin_dto(&record))).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn admin_update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return no_store(response);
    }
    if !query_is_auth_only(raw.as_deref()) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Ok(request) = serde_json::from_slice::<AnnouncementContentRequest>(&body) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(revision) = request.revision else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(service) = state.announcements.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let input =
        AnnouncementDraftInput::new(request.title, request.body_markdown, request.kind.into());
    match AnnouncementRepository::new(&service.database)
        .update(id, &input, revision)
        .await
    {
        Ok(record) => no_store(Json(admin_dto(&record)).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn admin_publish(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    body: Bytes,
) -> Response {
    admin_transition(state, id, headers, raw, body, AnnouncementStatus::Published).await
}

pub(crate) async fn admin_archive(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    body: Bytes,
) -> Response {
    admin_transition(state, id, headers, raw, body, AnnouncementStatus::Archived).await
}

async fn admin_transition(
    state: AppState,
    id: Uuid,
    headers: HeaderMap,
    raw: Option<String>,
    body: Bytes,
    target: AnnouncementStatus,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return no_store(response);
    }
    if !query_is_auth_only(raw.as_deref()) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Ok(request) = serde_json::from_slice::<RevisionRequest>(&body) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(service) = state.announcements.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let repository = AnnouncementRepository::new(&service.database);
    let result = if target == AnnouncementStatus::Published {
        repository.publish(id, request.revision).await
    } else {
        repository.archive(id, request.revision).await
    };
    match result {
        Ok(record) => no_store(Json(admin_dto(&record)).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn admin_delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = auth::authenticated_administrator(&state, &headers, raw.as_deref()).await
    {
        return no_store(response);
    }
    if !query_is_auth_only(raw.as_deref()) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Ok(request) = serde_json::from_slice::<RevisionRequest>(&body) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(service) = state.announcements.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match AnnouncementRepository::new(&service.database)
        .delete(id, request.revision)
        .await
    {
        Ok(()) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn client_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let principal = match auth::authenticated_principal(&state, &headers, raw.as_deref()).await {
        Ok(value) => value,
        Err(response) => return no_store(response),
    };
    let Some(query) = parse_client_query(raw.as_deref()) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(service) = state.announcements.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match AnnouncementRepository::new(&service.database)
        .visible_page(principal.user().id(), query.limit, query.offset)
        .await
    {
        Ok(page) => no_store(
            Json(ClientPageDto {
                items: page.items().iter().filter_map(client_dto).collect(),
                total: page.total(),
                unread_count: page.unread_count(),
            })
            .into_response(),
        ),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn next_popup(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let principal = match auth::authenticated_principal(&state, &headers, raw.as_deref()).await {
        Ok(value) => value,
        Err(response) => return no_store(response),
    };
    if !query_is_auth_only(raw.as_deref()) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Some(service) = state.announcements.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match AnnouncementRepository::new(&service.database)
        .next_popup(principal.user().id())
        .await
    {
        Ok(Some(view)) => match client_dto(&view) {
            Some(value) => no_store(Json(value).into_response()),
            None => no_store(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        },
        Ok(None) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn acknowledge(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
    body: Bytes,
) -> Response {
    let principal = match auth::authenticated_principal(&state, &headers, raw.as_deref()).await {
        Ok(value) => value,
        Err(response) => return no_store(response),
    };
    if !query_is_auth_only(raw.as_deref()) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Ok(request) = serde_json::from_slice::<AcknowledgeRequest>(&body) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(service) = state.announcements.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match AnnouncementRepository::new(&service.database)
        .acknowledge(principal.user().id(), id, request.content_version)
        .await
    {
        Ok(()) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

fn admin_dto(value: &AnnouncementRecord) -> AdminAnnouncementDto {
    AdminAnnouncementDto {
        id: value.id(),
        title: value.title().to_owned(),
        body_markdown: value.body_markdown().to_owned(),
        kind: kind_dto(value.kind()),
        status: status_dto(value.status()),
        content_version: value.content_version(),
        revision: value.revision(),
        published_at: value.published_at(),
        created_at: value.created_at(),
        updated_at: value.updated_at(),
    }
}

fn client_dto(value: &AnnouncementView) -> Option<ClientAnnouncementDto> {
    let record = value.record();
    Some(ClientAnnouncementDto {
        id: record.id(),
        title: record.title().to_owned(),
        body_markdown: record.body_markdown().to_owned(),
        kind: kind_dto(record.kind()),
        content_version: record.content_version(),
        published_at: record.published_at()?,
        is_read: value.is_read(),
    })
}

const fn kind_dto(value: AnnouncementKind) -> &'static str {
    match value {
        AnnouncementKind::Popup => "Popup",
        AnnouncementKind::Standard => "Standard",
    }
}
const fn status_dto(value: AnnouncementStatus) -> &'static str {
    match value {
        AnnouncementStatus::Draft => "Draft",
        AnnouncementStatus::Published => "Published",
        AnnouncementStatus::Archived => "Archived",
    }
}

struct PageQuery {
    offset: u64,
    limit: u64,
}
struct AdminQuery {
    offset: u64,
    limit: u64,
    status: Option<AnnouncementStatus>,
    kind: Option<AnnouncementKind>,
}

fn parse_client_query(raw: Option<&str>) -> Option<PageQuery> {
    let mut query = clean_query(raw)?;
    let offset = query
        .remove("startIndex")
        .map_or(Some(0), |value| value.parse().ok())?;
    let limit = query
        .remove("limit")
        .map_or(Some(DEFAULT_LIMIT), |value| value.parse().ok())?;
    (query.is_empty() && offset <= MAX_OFFSET && limit > 0 && limit <= MAX_LIMIT)
        .then_some(PageQuery { offset, limit })
}

fn parse_admin_query(raw: Option<&str>) -> Option<AdminQuery> {
    let mut query = clean_query(raw)?;
    let offset = query
        .remove("startIndex")
        .map_or(Some(0), |value| value.parse().ok())?;
    let limit = query
        .remove("limit")
        .map_or(Some(DEFAULT_LIMIT), |value| value.parse().ok())?;
    let status = match query.remove("status").as_deref() {
        None => None,
        Some("Draft") => Some(AnnouncementStatus::Draft),
        Some("Published") => Some(AnnouncementStatus::Published),
        Some("Archived") => Some(AnnouncementStatus::Archived),
        Some(_) => return None,
    };
    let kind = match query.remove("kind").as_deref() {
        None => None,
        Some("Popup") => Some(AnnouncementKind::Popup),
        Some("Standard") => Some(AnnouncementKind::Standard),
        Some(_) => return None,
    };
    (query.is_empty() && offset <= MAX_OFFSET && limit > 0 && limit <= MAX_LIMIT).then_some(
        AdminQuery {
            offset,
            limit,
            status,
            kind,
        },
    )
}

fn clean_query(raw: Option<&str>) -> Option<std::collections::HashMap<String, String>> {
    let mut query = auth::request_query(raw).ok()?;
    query.remove("ApiKey");
    query.remove("api_key");
    Some(query)
}

fn query_is_auth_only(raw: Option<&str>) -> bool {
    clean_query(raw).is_some_and(|value| value.is_empty())
}

fn error_response(error: &AnnouncementRepositoryError) -> Response {
    match error {
        AnnouncementRepositoryError::InvalidInput
        | AnnouncementRepositoryError::InvalidRevision
        | AnnouncementRepositoryError::InvalidTransition => StatusCode::BAD_REQUEST.into_response(),
        AnnouncementRepositoryError::NotFound => StatusCode::NOT_FOUND.into_response(),
        AnnouncementRepositoryError::RevisionConflict
        | AnnouncementRepositoryError::StaleVersion => StatusCode::CONFLICT.into_response(),
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
