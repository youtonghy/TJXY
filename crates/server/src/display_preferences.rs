use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use md5::{Digest, Md5};
use tjxy_api::DisplayPreferencesDto;
use tjxy_application::DisplayPreferencesServiceError;
use tjxy_common::UserId;
use tjxy_db::DisplayPreferencesRepositoryError;
use uuid::Uuid;

use crate::{AppState, auth};

const MAX_DISPLAY_ID_CHARS: usize = 256;
const MAX_CLIENT_CHARS: usize = 256;
const MAX_OPTION_CHARS: usize = 256;
const MAX_CUSTOM_PREFERENCES: usize = 128;
const MAX_CUSTOM_KEY_CHARS: usize = 128;
const MAX_CUSTOM_VALUE_CHARS: usize = 2_048;
const MAX_IMAGE_DIMENSION: i32 = 8_192;

pub(crate) async fn get(
    State(state): State<AppState>,
    Path(display_preferences_id): Path<String>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Ok(query) = preferences_query(raw_query.as_deref()) else {
        return error(StatusCode::BAD_REQUEST, "invalid display preferences query");
    };
    let Ok(normalized_id) = normalize_id(&display_preferences_id) else {
        return error(StatusCode::BAD_REQUEST, "invalid display preferences id");
    };
    let Some(service) = state.display_preferences.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "display preferences are unavailable",
        );
    };
    let document = match service
        .get(
            principal.user().id(),
            query.requested_user,
            normalized_id,
            &query.client,
        )
        .await
    {
        Ok(document) => document,
        Err(error_value) => return service_error(&error_value),
    };
    let mut preferences = match document {
        Some(document) => match serde_json::from_value::<DisplayPreferencesDto>(document) {
            Ok(preferences) => preferences,
            Err(_) => {
                return error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "display preferences are unavailable",
                );
            }
        },
        None => DisplayPreferencesDto::default(),
    };
    preferences.id = Some(normalized_id.to_string());
    preferences.client = Some(query.client);
    Json(preferences).into_response()
}

pub(crate) async fn post(
    State(state): State<AppState>,
    Path(display_preferences_id): Path<String>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let principal =
        match auth::authenticated_principal(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Ok(query) = preferences_query(raw_query.as_deref()) else {
        return error(StatusCode::BAD_REQUEST, "invalid display preferences query");
    };
    let Ok(normalized_id) = normalize_id(&display_preferences_id) else {
        return error(StatusCode::BAD_REQUEST, "invalid display preferences id");
    };
    if !auth::is_json_content_type(&headers) {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid display preferences request",
        );
    }
    let mut preferences: DisplayPreferencesDto = match serde_json::from_slice(&body) {
        Ok(preferences) => preferences,
        Err(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                "invalid display preferences request",
            );
        }
    };
    if !valid_preferences(&preferences) {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid display preferences request",
        );
    }
    preferences.id = Some(normalized_id.to_string());
    preferences.client = Some(query.client.clone());
    let Ok(document) = serde_json::to_value(preferences) else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid display preferences request",
        );
    };
    let Some(service) = state.display_preferences.as_ref() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "display preferences are unavailable",
        );
    };
    match service
        .replace(
            principal.user().id(),
            query.requested_user,
            normalized_id,
            &query.client,
            &document,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error_value) => service_error(&error_value),
    }
}

struct PreferencesQuery {
    client: String,
    requested_user: Option<UserId>,
}

fn preferences_query(raw_query: Option<&str>) -> Result<PreferencesQuery, ()> {
    let mut query = auth::request_query(raw_query)?;
    query.remove("ApiKey");
    query.remove("api_key");
    let client = query.remove("client").ok_or(())?;
    let lower = query.remove("userId");
    let upper = query.remove("UserId");
    if !query.is_empty()
        || lower.is_some() && upper.is_some()
        || !valid_bounded_text(&client, MAX_CLIENT_CHARS, true)
    {
        return Err(());
    }
    let requested_user = lower
        .or(upper)
        .map(|value| {
            Uuid::parse_str(&value)
                .map(UserId::from_uuid)
                .map_err(|_| ())
        })
        .transpose()?;
    Ok(PreferencesQuery {
        client,
        requested_user,
    })
}

fn normalize_id(value: &str) -> Result<Uuid, ()> {
    if !valid_bounded_text(value, MAX_DISPLAY_ID_CHARS, true) {
        return Err(());
    }
    if let Ok(id) = Uuid::parse_str(value) {
        return Ok(id);
    }
    let utf16le = value
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let digest: [u8; 16] = Md5::digest(utf16le).into();
    Ok(Uuid::from_bytes_le(digest))
}

fn valid_preferences(preferences: &DisplayPreferencesDto) -> bool {
    (0..=MAX_IMAGE_DIMENSION).contains(&preferences.primary_image_height)
        && (0..=MAX_IMAGE_DIMENSION).contains(&preferences.primary_image_width)
        && [
            preferences.view_type.as_deref(),
            preferences.sort_by.as_deref(),
            preferences.index_by.as_deref(),
        ]
        .into_iter()
        .flatten()
        .all(|value| valid_bounded_text(value, MAX_OPTION_CHARS, false))
        && preferences.custom_prefs.len() <= MAX_CUSTOM_PREFERENCES
        && preferences.custom_prefs.iter().all(|(key, value)| {
            valid_bounded_text(key, MAX_CUSTOM_KEY_CHARS, true)
                && value.as_deref().is_none_or(|value| {
                    value.chars().count() <= MAX_CUSTOM_VALUE_CHARS
                        && !value.chars().any(char::is_control)
                })
        })
}

fn valid_bounded_text(value: &str, maximum: usize, require_nonempty: bool) -> bool {
    (!require_nonempty || !value.is_empty())
        && value.trim() == value
        && value.chars().count() <= maximum
        && !value.chars().any(char::is_control)
}

fn service_error(error_value: &DisplayPreferencesServiceError) -> Response {
    match error_value {
        DisplayPreferencesServiceError::UnauthorizedUser => {
            error(StatusCode::FORBIDDEN, "requested user is not authorized")
        }
        DisplayPreferencesServiceError::Repository(
            DisplayPreferencesRepositoryError::InvalidClient
            | DisplayPreferencesRepositoryError::InvalidDocument,
        ) => error(
            StatusCode::BAD_REQUEST,
            "invalid display preferences request",
        ),
        DisplayPreferencesServiceError::Repository(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "display preferences are unavailable",
        ),
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, message).into_response()
}
