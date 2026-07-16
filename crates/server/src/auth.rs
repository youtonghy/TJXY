use std::collections::HashMap;

use axum::{
    Json,
    body::Bytes,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tjxy_api::{AuthenticateUserByName, AuthenticationResult, SessionInfoDto, UserDto, UserPolicy};
use tjxy_application::{AuthError, ClientIdentity};
use tjxy_db::AuthUser;

use crate::AppState;

pub(crate) async fn authenticate_by_name(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let client = match client_identity(&headers, state.legacy_auth_enabled) {
        Ok(client) => client,
        Err(error) => return error.into_response(),
    };
    if !is_json_content_type(&headers) {
        return HttpAuthError::BadRequest.into_response();
    }
    let payload: AuthenticateUserByName = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(_) => return HttpAuthError::BadRequest.into_response(),
    };
    let Some(auth) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    let issued = match auth
        .authenticate(&payload.username, &payload.password, client)
        .await
    {
        Ok(issued) => issued,
        Err(error) => return HttpAuthError::from(error).into_response(),
    };
    let user = user_dto(issued.user(), state.identity.id);
    let session = SessionInfoDto::active(
        issued.session_id(),
        issued.user().id().as_uuid(),
        issued.user().name(),
        issued.client().client_name(),
        issued.client().device_id(),
        issued.client().device_name(),
        issued.client().client_version(),
        state.identity.id,
    );
    Json(AuthenticationResult::new(
        user,
        session,
        issued.access_token().expose_secret(),
        state.identity.id,
    ))
    .into_response()
}

pub(crate) async fn current_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let token = match access_token(&headers, query.as_deref(), state.legacy_auth_enabled) {
        Ok(token) => token,
        Err(error) => return error.into_response(),
    };
    let Some(auth) = state.auth.as_ref() else {
        return HttpAuthError::Unavailable.into_response();
    };
    match auth.authenticate_token(&token).await {
        Ok(principal) => Json(user_dto(principal.user(), state.identity.id)).into_response(),
        Err(error) => HttpAuthError::from(error).into_response(),
    }
}

fn user_dto(user: &AuthUser, server_id: uuid::Uuid) -> UserDto {
    UserDto::new(
        user.id().as_uuid(),
        user.name(),
        server_id,
        user.has_password(),
        UserPolicy::direct_play_only(user.is_admin()),
    )
}

fn client_identity(
    headers: &HeaderMap,
    legacy_enabled: bool,
) -> Result<ClientIdentity, HttpAuthError> {
    let value = if let Some(value) = headers.get(header::AUTHORIZATION) {
        value
    } else if legacy_enabled {
        headers
            .get("X-Emby-Authorization")
            .ok_or(HttpAuthError::BadRequest)?
    } else {
        return Err(HttpAuthError::BadRequest);
    };
    let parameters = parse_authorization(
        value.to_str().map_err(|_| HttpAuthError::BadRequest)?,
        legacy_enabled,
    )?;
    ClientIdentity::new(
        required(&parameters, "Client")?,
        required(&parameters, "Device")?,
        required(&parameters, "DeviceId")?,
        required(&parameters, "Version")?,
    )
    .map_err(|_| HttpAuthError::BadRequest)
}

fn access_token(
    headers: &HeaderMap,
    query: Option<&str>,
    legacy_enabled: bool,
) -> Result<String, HttpAuthError> {
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        let parameters = parse_authorization(
            value.to_str().map_err(|_| HttpAuthError::Unauthorized)?,
            legacy_enabled,
        )
        .map_err(|_| HttpAuthError::Unauthorized)?;
        if let Some(token) = parameters.get("Token").filter(|token| !token.is_empty()) {
            return valid_token(token);
        }
    }
    if legacy_enabled {
        for name in ["X-Emby-Token", "X-MediaBrowser-Token"] {
            if let Some(value) = headers.get(name) {
                return valid_token(value.to_str().map_err(|_| HttpAuthError::Unauthorized)?);
            }
        }
    }
    if let Some(query) = query {
        let parameters = parse_query(query)?;
        if let Some(token) = parameters.get("ApiKey") {
            return valid_token(token);
        }
        if legacy_enabled {
            if let Some(token) = parameters.get("api_key") {
                return valid_token(token);
            }
        }
    }
    Err(HttpAuthError::Unauthorized)
}

fn is_json_content_type(headers: &HeaderMap) -> bool {
    let Ok(value) = headers
        .get(header::CONTENT_TYPE)
        .ok_or(())
        .and_then(|value| value.to_str().map_err(|_| ()))
    else {
        return false;
    };
    let media_type = value.split(';').next().unwrap_or_default().trim();
    media_type.eq_ignore_ascii_case("application/json")
        || media_type.eq_ignore_ascii_case("text/json")
        || media_type.starts_with("application/") && media_type.ends_with("+json")
}

fn valid_token(value: &str) -> Result<String, HttpAuthError> {
    if value.is_empty() || value.len() > 1_024 || value.chars().any(char::is_control) {
        return Err(HttpAuthError::Unauthorized);
    }
    Ok(value.to_owned())
}

fn required<'value>(
    parameters: &'value HashMap<String, String>,
    name: &str,
) -> Result<&'value str, HttpAuthError> {
    parameters
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(HttpAuthError::BadRequest)
}

fn parse_authorization(
    value: &str,
    legacy_enabled: bool,
) -> Result<HashMap<String, String>, HttpAuthError> {
    let (scheme, parameters) = value
        .split_once(char::is_whitespace)
        .ok_or(HttpAuthError::BadRequest)?;
    if !(scheme.eq_ignore_ascii_case("MediaBrowser")
        || legacy_enabled && scheme.eq_ignore_ascii_case("Emby"))
    {
        return Err(HttpAuthError::BadRequest);
    }
    parse_parameters(parameters)
}

fn parse_parameters(mut input: &str) -> Result<HashMap<String, String>, HttpAuthError> {
    let mut output = HashMap::new();
    loop {
        input = input
            .trim_start_matches(|character: char| character.is_whitespace() || character == ',');
        if input.is_empty() {
            return Ok(output);
        }
        let equals = input.find('=').ok_or(HttpAuthError::BadRequest)?;
        let key = input[..equals].trim();
        if key.is_empty() || key.contains(',') {
            return Err(HttpAuthError::BadRequest);
        }
        input = &input[equals + 1..];
        let (value, remaining) = if let Some(quoted) = input.strip_prefix('"') {
            parse_quoted(quoted)?
        } else {
            let comma = input.find(',').unwrap_or(input.len());
            (input[..comma].trim().to_owned(), &input[comma..])
        };
        let value = percent_decode(&value, false)?;
        if output.insert(key.to_owned(), value).is_some() {
            return Err(HttpAuthError::BadRequest);
        }
        input = remaining;
    }
}

fn parse_query(input: &str) -> Result<HashMap<String, String>, HttpAuthError> {
    let mut output = HashMap::new();
    for pair in input.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = percent_decode(key, true)?;
        let value = percent_decode(value, true)?;
        output.entry(key).or_insert(value);
    }
    Ok(output)
}

fn percent_decode(value: &str, plus_as_space: bool) -> Result<String, HttpAuthError> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = hex_digit(bytes[index + 1]).ok_or(HttpAuthError::BadRequest)?;
                let low = hex_digit(bytes[index + 2]).ok_or(HttpAuthError::BadRequest)?;
                output.push((high << 4) | low);
                index += 3;
            }
            b'%' => return Err(HttpAuthError::BadRequest),
            b'+' if plus_as_space => {
                output.push(b' ');
                index += 1;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).map_err(|_| HttpAuthError::BadRequest)
}

const fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn parse_quoted(input: &str) -> Result<(String, &str), HttpAuthError> {
    let mut output = String::new();
    let mut escaped = false;
    for (index, character) in input.char_indices() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            let remaining = &input[index + character.len_utf8()..];
            if !remaining.trim_start().is_empty() && !remaining.trim_start().starts_with(',') {
                return Err(HttpAuthError::BadRequest);
            }
            return Ok((output, remaining));
        } else {
            output.push(character);
        }
    }
    Err(HttpAuthError::BadRequest)
}

#[derive(Clone, Copy, Debug)]
enum HttpAuthError {
    BadRequest,
    Unauthorized,
    Forbidden,
    Unavailable,
    TooManyRequests,
}

impl From<AuthError> for HttpAuthError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidCredentials | AuthError::InvalidToken => Self::Unauthorized,
            AuthError::Forbidden => Self::Forbidden,
            AuthError::InvalidUsername
            | AuthError::InvalidPassword
            | AuthError::PasswordRequired
            | AuthError::InvalidClientIdentity
            | AuthError::InvalidSessionLifetime
            | AuthError::InvalidPasswordConcurrency => Self::BadRequest,
            AuthError::TimestampOverflow
            | AuthError::PasswordEngine
            | AuthError::PasswordWorker
            | AuthError::Repository(_) => Self::Unavailable,
            AuthError::Busy => Self::TooManyRequests,
        }
    }
}

impl IntoResponse for HttpAuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest => (StatusCode::BAD_REQUEST, "invalid authentication request"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "invalid credentials or token"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "authentication is not permitted"),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "authentication is unavailable",
            ),
            Self::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "authentication capacity is busy",
            ),
        };
        let mut response = (status, Json(ErrorBody { message })).into_response();
        if matches!(self, Self::TooManyRequests) {
            response.headers_mut().insert(
                header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("1"),
            );
        }
        response
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ErrorBody {
    message: &'static str,
}

#[cfg(test)]
mod tests {
    use super::parse_parameters;

    #[test]
    fn parser_handles_quoted_commas_and_rejects_duplicates() {
        let parsed = parse_parameters(r#"Client="A, B", Device="Phone""#).unwrap();
        assert_eq!(parsed["Client"], "A, B");
        assert!(parse_parameters(r#"Client="A", Client="B""#).is_err());
    }
}
