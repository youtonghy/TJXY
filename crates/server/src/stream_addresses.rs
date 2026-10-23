//! Alternate stream addresses: the client-facing address list and the
//! cross-origin policy that lets a page served from one configured address
//! load images from another.

use std::{collections::HashSet, env, sync::Arc};

use axum::{
    Json,
    extract::{RawQuery, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tjxy_db::{StreamAddress, SystemSettingsRecord};
use tokio::sync::RwLock;
use url::Url;

use crate::{AppState, auth};

const PREFLIGHT_MAX_AGE_SECONDS: &str = "600";

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct StreamAddressesDto {
    addresses: Vec<StreamAddressDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct StreamAddressDto {
    name: String,
    url: String,
}

/// Lists the configured stream addresses for any signed-in user.
pub(crate) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) = auth::authenticated_principal(&state, &headers, query.as_deref()).await {
        return response;
    }
    let Some(service) = state.system_settings.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Ok(record) = service.get().await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let addresses = record
        .as_ref()
        .map_or(&[][..], SystemSettingsRecord::stream_addresses)
        .iter()
        .map(|address| StreamAddressDto {
            name: address.name.clone(),
            url: address.url.clone(),
        })
        .collect();
    let mut response = Json(StreamAddressesDto { addresses }).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Origins that may read authenticated resources cross-origin: every
/// configured stream address plus the public address. Loaded lazily and
/// dropped whenever system settings change.
#[derive(Default)]
pub(crate) struct CrossOriginPolicy {
    origins: RwLock<Option<Arc<HashSet<String>>>>,
}

impl CrossOriginPolicy {
    pub(crate) async fn invalidate(&self) {
        *self.origins.write().await = None;
    }

    async fn allows(&self, state: &AppState, origin: &str) -> bool {
        let Some(origin) = origin_of(origin) else {
            return false;
        };
        if let Some(origins) = self.origins.read().await.as_ref() {
            return origins.contains(&origin);
        }
        // Loading under the write lock orders the reload after any concurrent
        // `invalidate`, so a stale snapshot can never outlive a settings change.
        let mut cached = self.origins.write().await;
        if let Some(origins) = cached.as_ref() {
            return origins.contains(&origin);
        }
        let Some(service) = state.system_settings.as_ref() else {
            return false;
        };
        let record = match service.get().await {
            Ok(record) => record,
            Err(error) => {
                tracing::warn!(%error, "cross-origin policy could not load system settings");
                return false;
            }
        };
        let origins = Arc::new(allowed_origins(
            record.as_ref(),
            env::var("TJXY_PUBLIC_ADDRESS").ok().as_deref(),
        ));
        let allowed = origins.contains(&origin);
        *cached = Some(origins);
        allowed
    }
}

fn allowed_origins(
    record: Option<&SystemSettingsRecord>,
    public_override: Option<&str>,
) -> HashSet<String> {
    let public_url = public_override.or_else(|| record.and_then(SystemSettingsRecord::public_url));
    record
        .map_or(&[][..], SystemSettingsRecord::stream_addresses)
        .iter()
        .map(|address: &StreamAddress| address.url.as_str())
        .chain(public_url)
        .filter_map(origin_of)
        .collect()
}

fn origin_of(value: &str) -> Option<String> {
    let url = Url::parse(value.trim()).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.origin().ascii_serialization())
}

/// Answers CORS preflights and tags responses for allowed origins. Only the
/// bearer `Authorization` header is accepted; credentials (cookies) are never
/// allowed cross-origin.
pub(crate) async fn cross_origin(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let allowed = match origin.as_deref() {
        Some(origin) => state.cross_origin.allows(&state, origin).await,
        None => false,
    };
    let preflight = request.method() == Method::OPTIONS;
    let mut response = if preflight {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
    if let (true, Some(origin)) = (allowed, origin)
        && let Ok(value) = HeaderValue::from_str(&origin)
    {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
        if preflight {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, HEAD"),
            );
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("Authorization, Accept"),
            );
            headers.insert(
                header::ACCESS_CONTROL_MAX_AGE,
                HeaderValue::from_static(PREFLIGHT_MAX_AGE_SECONDS),
            );
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::origin_of;

    #[test]
    fn origins_compare_by_normalized_scheme_host_and_port() {
        assert_eq!(
            origin_of("HTTPS://Media.Example.com:443").as_deref(),
            Some("https://media.example.com")
        );
        assert_eq!(
            origin_of("http://10.0.0.2:8096/").as_deref(),
            Some("http://10.0.0.2:8096")
        );
        assert_eq!(origin_of("null"), None);
        assert_eq!(origin_of("file:///tmp"), None);
    }
}
