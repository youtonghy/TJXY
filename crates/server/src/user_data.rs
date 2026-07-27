use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tjxy_api::{UpdateUserItemDataDto, UserItemDataDto};
use tjxy_application::UserDataServiceError;
use tjxy_common::{CatalogItemId, UserId};
use tjxy_db::{UserDataPatch, UserDataRecord, UserDataRepositoryError};
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) async fn get(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Ok(requested_user) = requested_user(raw_query.as_deref()) else {
        return error(StatusCode::BAD_REQUEST, "invalid user data query");
    };
    let Some(service) = state.user_data.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "user data is unavailable");
    };
    match service
        .get(
            principal.user().id(),
            requested_user,
            CatalogItemId::from_uuid(item_id),
        )
        .await
    {
        Ok(Some(data)) => Json(dto(&data)).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "item was not found"),
        Err(error_value) => service_error(&error_value),
    }
}

pub(crate) async fn post(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Ok(requested_user) = requested_user(raw_query.as_deref()) else {
        return error(StatusCode::BAD_REQUEST, "invalid user data query");
    };
    if !auth::is_json_content_type(&headers) {
        return error(StatusCode::BAD_REQUEST, "invalid user data request");
    }
    let update: UpdateUserItemDataDto = match serde_json::from_slice(&body) {
        Ok(update) => update,
        Err(_) => return error(StatusCode::BAD_REQUEST, "invalid user data request"),
    };
    let patch = patch(&update);
    let Some(service) = state.user_data.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "user data is unavailable");
    };
    match service
        .commit(
            principal.user().id(),
            requested_user,
            CatalogItemId::from_uuid(item_id),
            patch,
        )
        .await
    {
        Ok(Some(commit)) => {
            state
                .realtime_events()
                .publish_user_data_changed(principal.user().id(), commit.user_revision);
            Json(dto(&commit.data)).into_response()
        }
        Ok(None) => error(StatusCode::NOT_FOUND, "item was not found"),
        Err(error_value) => service_error(&error_value),
    }
}

pub(crate) async fn favorite(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    command(
        state,
        user_id,
        item_id,
        headers,
        raw_query,
        UserDataPatch::favorite(true),
    )
    .await
}

pub(crate) async fn unfavorite(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    command(
        state,
        user_id,
        item_id,
        headers,
        raw_query,
        UserDataPatch::favorite(false),
    )
    .await
}

pub(crate) async fn played(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    command(
        state,
        user_id,
        item_id,
        headers,
        raw_query,
        UserDataPatch::default().with_played(true),
    )
    .await
}

pub(crate) async fn unplayed(
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    command(
        state,
        user_id,
        item_id,
        headers,
        raw_query,
        UserDataPatch::default().with_played(false),
    )
    .await
}

async fn command(
    state: AppState,
    requested_user: Uuid,
    item_id: Uuid,
    headers: HeaderMap,
    raw_query: Option<String>,
    patch: UserDataPatch,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    if !valid_auth_only_query(raw_query.as_deref()) {
        return error(StatusCode::BAD_REQUEST, "invalid user data query");
    }
    let Some(service) = state.user_data.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "user data is unavailable");
    };
    match service
        .commit(
            principal.user().id(),
            Some(UserId::from_uuid(requested_user)),
            CatalogItemId::from_uuid(item_id),
            patch,
        )
        .await
    {
        Ok(Some(commit)) => {
            state
                .realtime_events()
                .publish_user_data_changed(principal.user().id(), commit.user_revision);
            Json(dto(&commit.data)).into_response()
        }
        Ok(None) => error(StatusCode::NOT_FOUND, "item was not found"),
        Err(error_value) => service_error(&error_value),
    }
}

fn requested_user(raw_query: Option<&str>) -> Result<Option<UserId>, ()> {
    let mut query = auth::request_query(raw_query)?;
    query.remove("ApiKey");
    query.remove("api_key");
    let lower = query.remove("userId");
    let upper = query.remove("UserId");
    if !query.is_empty() || (lower.is_some() && upper.is_some()) {
        return Err(());
    }
    lower
        .or(upper)
        .map(|value| {
            Uuid::parse_str(&value)
                .map(UserId::from_uuid)
                .map_err(|_| ())
        })
        .transpose()
}

fn valid_auth_only_query(raw_query: Option<&str>) -> bool {
    let Ok(mut query) = auth::request_query(raw_query) else {
        return false;
    };
    query.remove("ApiKey");
    query.remove("api_key");
    query.is_empty()
}

fn patch(update: &UpdateUserItemDataDto) -> UserDataPatch {
    let mut patch = UserDataPatch::default();
    if let Some(value) = update.is_favorite {
        patch = patch.with_favorite(value);
    }
    if let Some(value) = update.played {
        patch = patch.with_played(value);
    }
    if let Some(value) = update.play_count {
        patch = patch.with_play_count(value);
    }
    if let Some(value) = update.playback_position_ticks {
        patch = patch.with_playback_position_ticks(value);
    }
    patch
}

fn dto(data: &UserDataRecord) -> UserItemDataDto {
    UserItemDataDto::new(
        data.catalog_item_id.as_uuid(),
        data.is_favorite,
        data.is_played,
        data.play_count,
        data.playback_position_ticks,
    )
    .with_last_played_date(data.last_played_at)
}

fn service_error(error_value: &UserDataServiceError) -> Response {
    match error_value {
        UserDataServiceError::UnauthorizedUser => {
            error(StatusCode::FORBIDDEN, "requested user is not authorized")
        }
        UserDataServiceError::Repository(
            UserDataRepositoryError::EmptyPatch
            | UserDataRepositoryError::NegativePlaybackPosition
            | UserDataRepositoryError::InvalidPlayCount
            | UserDataRepositoryError::InvalidPlayCountDelta,
        ) => error(StatusCode::BAD_REQUEST, "invalid user data request"),
        UserDataServiceError::Repository(_) => {
            error(StatusCode::SERVICE_UNAVAILABLE, "user data is unavailable")
        }
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, message).into_response()
}
