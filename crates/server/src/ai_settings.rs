use axum::{
    Json,
    body::Bytes,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tjxy_db::{
    AiModelInput, AiModelRecord, AiReasoningEffort, AiSettingsRecord, AiSettingsRepositoryError,
    AiUsageAnalytics, AiUsageDaily, AiUsageFailure, AiUsageModel, AiUsageSummary, AiUsageUser,
};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{AppState, ai::AiServiceError, auth};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct PutAiSettingsRequest {
    enabled: bool,
    base_url: String,
    api_key: Option<Zeroizing<String>>,
    system_prompt: String,
    daily_total_token_limit: u64,
    daily_user_token_limit: u64,
    revision: Option<i64>,
    models: Vec<AiModelRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct AiModelRequest {
    id: Uuid,
    upstream_id: String,
    display_name: String,
    #[serde(default)]
    reasoning_effort: AiReasoningEffort,
    is_visible: bool,
    is_default: bool,
    sort_order: i32,
}

impl From<AiModelRequest> for AiModelInput {
    fn from(value: AiModelRequest) -> Self {
        Self::new(
            value.id,
            value.upstream_id,
            value.display_name,
            value.is_visible,
            value.is_default,
            value.sort_order,
        )
        .with_reasoning_effort(value.reasoning_effort)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct TestAiSettingsRequest {
    base_url: Option<String>,
    api_key: Option<Zeroizing<String>>,
    upstream_model: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct DiscoverAiModelsRequest {
    base_url: Option<String>,
    api_key: Option<Zeroizing<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct DeleteAiSettingsRequest {
    revision: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AiSettingsDto {
    provider: &'static str,
    configured: bool,
    enabled: bool,
    base_url: Option<String>,
    system_prompt: String,
    daily_total_token_limit: u64,
    daily_user_token_limit: u64,
    revision: Option<i64>,
    encryption_available: bool,
    models: Vec<AiModelDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AiModelDto {
    id: Uuid,
    upstream_id: String,
    display_name: String,
    reasoning_effort: AiReasoningEffort,
    is_visible: bool,
    is_default: bool,
    sort_order: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PublicAiModelsDto {
    items: Vec<PublicAiModelDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct PublicAiModelDto {
    id: Uuid,
    display_name: String,
    is_default: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct TestResultDto {
    status: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct DiscoveredAiModelsDto {
    items: Vec<DiscoveredAiModelDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct DiscoveredAiModelDto {
    id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AiAnalyticsDto {
    window: AiAnalyticsWindowDto,
    summary: AiUsageSummaryDto,
    daily: Vec<AiUsageDailyDto>,
    users: Vec<AiUsageUserDto>,
    models: Vec<AiUsageModelDto>,
    recent_failures: Vec<AiUsageFailureDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AiAnalyticsWindowDto {
    today: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    time_zone: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AiUsageSummaryDto {
    total_requests: u64,
    active_users: u64,
    successful_requests: u64,
    failed_requests: u64,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    known_token_requests: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AiUsageDailyDto {
    day: String,
    total_requests: u64,
    successful_requests: u64,
    failed_requests: u64,
    total_tokens: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AiUsageUserDto {
    user_id: Uuid,
    username: String,
    total_requests: u64,
    successful_requests: u64,
    total_tokens: Option<u64>,
    last_used_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AiUsageModelDto {
    model_id: Uuid,
    display_name: String,
    upstream_model_id: String,
    total_requests: u64,
    successful_requests: u64,
    total_tokens: Option<u64>,
    last_used_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AiUsageFailureDto {
    id: Uuid,
    user_id: Uuid,
    username: String,
    model_id: Uuid,
    model_display_name: String,
    outcome: &'static str,
    elapsed_ms: u64,
    started_at: DateTime<Utc>,
}

pub(crate) async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return no_store(response);
    }
    let Some(service) = state.ai.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.settings().await {
        Ok(settings) => no_store(
            Json(settings_dto(
                settings.as_ref(),
                state.ai_encryption_available,
            ))
            .into_response(),
        ),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn put(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return no_store(response);
    }
    let request: PutAiSettingsRequest = match json_body(&headers, &body) {
        Ok(value) => value,
        Err(status) => return no_store(status.into_response()),
    };
    let Some(service) = state.ai.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let models = request
        .models
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
    match service
        .save_settings(
            request.enabled,
            &request.base_url,
            request.api_key,
            &request.system_prompt,
            request.daily_total_token_limit,
            request.daily_user_token_limit,
            &models,
            request.revision,
        )
        .await
    {
        Ok(settings) => no_store(
            Json(settings_dto(Some(&settings), state.ai_encryption_available)).into_response(),
        ),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return no_store(response);
    }
    let request: DeleteAiSettingsRequest = match json_body(&headers, &body) {
        Ok(value) => value,
        Err(status) => return no_store(status.into_response()),
    };
    let Some(service) = state.ai.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.delete_settings(Some(request.revision)).await {
        Ok(_) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn test(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return no_store(response);
    }
    let request: TestAiSettingsRequest = match json_body(&headers, &body) {
        Ok(value) => value,
        Err(status) => return no_store(status.into_response()),
    };
    let Some(service) = state.ai.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service
        .test_configuration(
            request.base_url.as_deref(),
            request.api_key,
            request.upstream_model.as_deref(),
        )
        .await
    {
        Ok(()) => no_store(Json(TestResultDto { status: "Success" }).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn discover_models(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return no_store(response);
    }
    let request: DiscoverAiModelsRequest = match json_body(&headers, &body) {
        Ok(value) => value,
        Err(status) => return no_store(status.into_response()),
    };
    let Some(service) = state.ai.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service
        .discover_models(request.base_url.as_deref(), request.api_key)
        .await
    {
        Ok(models) => no_store(
            Json(DiscoveredAiModelsDto {
                items: models
                    .into_iter()
                    .map(|id| DiscoveredAiModelDto { id })
                    .collect(),
            })
            .into_response(),
        ),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn models(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_principal(&state, &headers, query.as_deref()).await {
        return no_store(response);
    }
    if !state.ai_encryption_available {
        return no_store(Json(PublicAiModelsDto { items: Vec::new() }).into_response());
    }
    let Some(service) = state.ai.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.settings().await {
        Ok(Some(settings)) if settings.enabled() => {
            let items = settings
                .visible_models()
                .into_iter()
                .map(|model| PublicAiModelDto {
                    id: model.id(),
                    display_name: model.display_name().to_owned(),
                    is_default: model.is_default(),
                })
                .collect();
            no_store(Json(PublicAiModelsDto { items }).into_response())
        }
        Ok(_) => no_store(Json(PublicAiModelsDto { items: Vec::new() }).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn analytics(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) =
        auth::authenticated_administrator(&state, &headers, query.as_deref()).await
    {
        return no_store(response);
    }
    let Some(service) = state.ai.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let now = Local::now();
    let today = now.date_naive();
    let tomorrow = today + Duration::days(1);
    let Some(starts_at) = local_midnight(today) else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let Some(ends_at) = local_midnight(tomorrow) else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let trend_start = today - Duration::days(13);
    match service
        .usage_analytics(
            &today.to_string(),
            &trend_start.to_string(),
            &today.to_string(),
            20,
        )
        .await
    {
        Ok(usage) => no_store(
            Json(analytics_dto(
                usage,
                today,
                starts_at,
                ends_at,
                &now.offset().to_string(),
            ))
            .into_response(),
        ),
        Err(error) => {
            tracing::error!("AI analytics failed: {error}");
            no_store(error_response(&error))
        }
    }
}

fn local_midnight(day: NaiveDate) -> Option<DateTime<Utc>> {
    day.and_hms_opt(0, 0, 0)?
        .and_local_timezone(Local)
        .earliest()
        .map(|value| value.with_timezone(&Utc))
}

fn analytics_dto(
    usage: AiUsageAnalytics,
    today: NaiveDate,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    time_zone: &str,
) -> AiAnalyticsDto {
    AiAnalyticsDto {
        window: AiAnalyticsWindowDto {
            today: today.to_string(),
            starts_at,
            ends_at,
            time_zone: format!("server-local {time_zone}"),
        },
        summary: summary_dto(&usage.summary),
        daily: usage.daily.into_iter().map(daily_dto).collect(),
        users: usage.users.into_iter().map(user_dto).collect(),
        models: usage.models.into_iter().map(model_usage_dto).collect(),
        recent_failures: usage.recent_failures.into_iter().map(failure_dto).collect(),
    }
}

fn summary_dto(value: &AiUsageSummary) -> AiUsageSummaryDto {
    AiUsageSummaryDto {
        total_requests: value.total_requests,
        active_users: value.active_users,
        successful_requests: value.successful_requests,
        failed_requests: value.failed_requests,
        prompt_tokens: value.prompt_tokens,
        completion_tokens: value.completion_tokens,
        total_tokens: value.total_tokens,
        known_token_requests: value.known_token_requests,
    }
}

fn daily_dto(value: AiUsageDaily) -> AiUsageDailyDto {
    AiUsageDailyDto {
        day: value.day,
        total_requests: value.total_requests,
        successful_requests: value.successful_requests,
        failed_requests: value.failed_requests,
        total_tokens: value.total_tokens,
    }
}

fn user_dto(value: AiUsageUser) -> AiUsageUserDto {
    AiUsageUserDto {
        user_id: value.user_id,
        username: value.username,
        total_requests: value.total_requests,
        successful_requests: value.successful_requests,
        total_tokens: value.total_tokens,
        last_used_at: value.last_used_at,
    }
}

fn model_usage_dto(value: AiUsageModel) -> AiUsageModelDto {
    AiUsageModelDto {
        model_id: value.model_id,
        display_name: value.display_name,
        upstream_model_id: value.upstream_model_id,
        total_requests: value.total_requests,
        successful_requests: value.successful_requests,
        total_tokens: value.total_tokens,
        last_used_at: value.last_used_at,
    }
}

fn failure_dto(value: AiUsageFailure) -> AiUsageFailureDto {
    AiUsageFailureDto {
        id: value.id,
        user_id: value.user_id,
        username: value.username,
        model_id: value.model_id,
        model_display_name: value.model_display_name,
        outcome: value.outcome.as_str(),
        elapsed_ms: value.elapsed_ms,
        started_at: value.started_at,
    }
}

fn settings_dto(settings: Option<&AiSettingsRecord>, encryption_available: bool) -> AiSettingsDto {
    let Some(settings) = settings else {
        return AiSettingsDto {
            provider: "OpenAiCompatible",
            configured: false,
            enabled: false,
            base_url: None,
            system_prompt: default_system_prompt().to_owned(),
            daily_total_token_limit: 0,
            daily_user_token_limit: 0,
            revision: None,
            encryption_available,
            models: Vec::new(),
        };
    };
    AiSettingsDto {
        provider: "OpenAiCompatible",
        configured: true,
        enabled: settings.enabled(),
        base_url: Some(settings.base_url().to_owned()),
        system_prompt: settings.system_prompt().to_owned(),
        daily_total_token_limit: settings.daily_total_token_limit(),
        daily_user_token_limit: settings.daily_user_token_limit(),
        revision: Some(settings.revision()),
        encryption_available,
        models: settings.models().iter().map(model_dto).collect(),
    }
}

fn model_dto(model: &AiModelRecord) -> AiModelDto {
    AiModelDto {
        id: model.id(),
        upstream_id: model.upstream_id().to_owned(),
        display_name: model.display_name().to_owned(),
        reasoning_effort: model.reasoning_effort(),
        is_visible: model.is_visible(),
        is_default: model.is_default(),
        sort_order: model.sort_order(),
    }
}

pub(crate) const fn default_system_prompt() -> &'static str {
    "你是 TJXY 影视助手。只讨论电影、剧集、音乐、演职员、媒体资料、用户的观影记录与个性化推荐。遇到无关问题时简短说明能力边界，并引导用户回到影视话题。推荐必须优先依据工具返回的本地媒体库和用户上下文，不得编造媒体库中不存在的条目。"
}

fn json_body<T: serde::de::DeserializeOwned>(
    headers: &HeaderMap,
    body: &[u8],
) -> Result<T, StatusCode> {
    let is_json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
    if !is_json {
        return Err(StatusCode::BAD_REQUEST);
    }
    serde_json::from_slice(body).map_err(|_| StatusCode::BAD_REQUEST)
}

fn error_response(error: &AiServiceError) -> Response {
    match error {
        AiServiceError::InvalidBaseUrl
        | AiServiceError::InvalidCredential
        | AiServiceError::CredentialRequiredForOriginChange
        | AiServiceError::Settings(
            AiSettingsRepositoryError::InvalidSettings
            | AiSettingsRepositoryError::InvalidModels
            | AiSettingsRepositoryError::InvalidRevision,
        ) => StatusCode::BAD_REQUEST.into_response(),
        AiServiceError::Settings(
            AiSettingsRepositoryError::RevisionConflict
            | AiSettingsRepositoryError::CredentialIdentityConflict,
        ) => StatusCode::CONFLICT.into_response(),
        AiServiceError::UpstreamRejected | AiServiceError::InvalidUpstreamResponse => {
            StatusCode::BAD_GATEWAY.into_response()
        }
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
