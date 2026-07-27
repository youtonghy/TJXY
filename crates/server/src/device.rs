use axum::{
    Json,
    body::Bytes,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tjxy_api::{ClientCapabilitiesDto, DeviceInfoDto, DeviceInfoDtoQueryResult, DeviceOptionsDto};
use tjxy_application::{AuthError, DeviceRecord};
use tjxy_common::UserId;
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Ok(user_id) = list_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.devices(&principal, user_id).await {
        Ok(devices) => Json(DeviceInfoDtoQueryResult::new(
            devices.iter().map(device_dto).collect(),
        ))
        .into_response(),
        Err(error) => device_error_response(&error),
    }
}

pub(crate) async fn info(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Ok(device_id) = singular_id_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.device(&principal, &device_id).await {
        Ok(Some(device)) => Json(device_dto(&device)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => device_error_response(&error),
    }
}

pub(crate) async fn options(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Ok(device_id) = singular_id_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.device_options(&principal, &device_id).await {
        Ok(Some(options)) => {
            let Ok(id) = i32::try_from(options.id()) else {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            };
            Json(DeviceOptionsDto::new(
                id,
                options.device_id(),
                options.custom_name().map(str::to_owned),
            ))
            .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => device_error_response(&error),
    }
}

pub(crate) async fn update_options(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Ok(device_id) = singular_id_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let payload: DeviceOptionsDto = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service
        .update_device_options(&principal, &device_id, payload.custom_name.as_deref())
        .await
    {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => device_error_response(&error),
    }
}

pub(crate) async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let Ok(device_ids) = delete_query(raw_query.as_deref()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.auth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match service.delete_devices(&principal, &device_ids).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) | Err(AuthError::InvalidDeviceRequest) => StatusCode::BAD_REQUEST.into_response(),
        Err(error) => device_error_response(&error),
    }
}

fn device_dto(device: &DeviceRecord) -> DeviceInfoDto {
    let capabilities = ClientCapabilitiesDto {
        playable_media_types: device.playable_media_types().to_vec(),
        supported_commands: device.supported_commands().to_vec(),
        supports_media_control: device.supports_media_control(),
        supports_persistent_identifier: device.supports_persistent_identifier(),
        device_profile: device.device_profile().cloned(),
        app_store_url: device.app_store_url().map(str::to_owned),
        icon_url: device.icon_url().map(str::to_owned),
    };
    DeviceInfoDto::new(
        device.device_name(),
        device.custom_name().map(str::to_owned),
        device.device_id(),
        device.user_name(),
        device.client_name(),
        device.client_version(),
        device.user_id().as_uuid(),
        device.last_activity_at(),
        capabilities,
        device.icon_url().map(str::to_owned),
    )
}

fn list_query(raw_query: Option<&str>) -> Result<Option<UserId>, ()> {
    let mut parameters = auth_parameters(raw_query)?;
    let user_id = take_parameter(&mut parameters, "userId")?
        .map(|user_id| Uuid::parse_str(&user_id).map(UserId::from_uuid))
        .transpose()
        .map_err(|_| ())?;
    parameters.is_empty().then_some(user_id).ok_or(())
}

fn singular_id_query(raw_query: Option<&str>) -> Result<String, ()> {
    let mut parameters = auth_parameters(raw_query)?;
    let device_id =
        take_parameter(&mut parameters, "id")?.filter(|device_id| !device_id.is_empty());
    if parameters.is_empty() {
        device_id.ok_or(())
    } else {
        Err(())
    }
}

fn delete_query(raw_query: Option<&str>) -> Result<Vec<String>, ()> {
    let mut device_ids = Vec::new();
    for (name, value) in auth::request_query_pairs(raw_query)? {
        if name.eq_ignore_ascii_case("id") && !value.is_empty() {
            device_ids.push(value);
        } else if name == "ApiKey" || name == "api_key" {
        } else {
            return Err(());
        }
    }
    (!device_ids.is_empty()).then_some(device_ids).ok_or(())
}

fn auth_parameters(
    raw_query: Option<&str>,
) -> Result<std::collections::HashMap<String, String>, ()> {
    let mut parameters = auth::request_query(raw_query)?;
    parameters.remove("ApiKey");
    parameters.remove("api_key");
    Ok(parameters)
}

fn take_parameter(
    parameters: &mut std::collections::HashMap<String, String>,
    expected: &str,
) -> Result<Option<String>, ()> {
    let keys = parameters
        .keys()
        .filter(|key| key.eq_ignore_ascii_case(expected))
        .cloned()
        .collect::<Vec<_>>();
    if keys.len() > 1 {
        return Err(());
    }
    Ok(keys.first().and_then(|key| parameters.remove(key)))
}

fn device_error_response(error: &AuthError) -> Response {
    match error {
        AuthError::InvalidDeviceRequest => StatusCode::BAD_REQUEST.into_response(),
        AuthError::Repository(tjxy_db::AuthRepositoryError::UserNotFound) => {
            StatusCode::NOT_FOUND.into_response()
        }
        AuthError::Forbidden => StatusCode::FORBIDDEN.into_response(),
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
