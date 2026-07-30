use std::{str, sync::Arc};

use axum::{
    Json,
    body::Bytes,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tjxy_credentials::{CredentialCipher, CredentialCipherError};
use tjxy_db::{
    MetadataProviderSettingRecord, MetadataProviderSettingsRepository,
    MetadataProviderSettingsRepositoryError,
};
use tjxy_metadata::{
    MetadataError, MetadataProviderError, ReloadableMetadataProvider, TmdbProvider,
};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{AppState, auth};

pub(crate) const TMDB_PROVIDER_KEY: &str = "tmdb";
pub(crate) const DEFAULT_TMDB_LANGUAGE: &str = "zh-CN";

pub(crate) type TmdbProviderFactory =
    dyn Fn(&str, &str) -> Result<TmdbProvider, MetadataError> + Send + Sync;

#[derive(Clone)]
pub(crate) struct TmdbEnvironmentFallback {
    provider: Arc<TmdbProvider>,
    language: String,
}

impl TmdbEnvironmentFallback {
    pub(crate) fn new(provider: Arc<TmdbProvider>, language: String) -> Self {
        Self { provider, language }
    }
}

pub(crate) struct MetadataSettingsAdminService {
    database: sea_orm::DatabaseConnection,
    cipher: Option<Arc<CredentialCipher>>,
    runtime: Arc<ReloadableMetadataProvider>,
    environment_fallback: Option<TmdbEnvironmentFallback>,
    provider_factory: Arc<TmdbProviderFactory>,
}

impl MetadataSettingsAdminService {
    pub(crate) fn new(
        database: sea_orm::DatabaseConnection,
        cipher: Option<Arc<CredentialCipher>>,
        runtime: Arc<ReloadableMetadataProvider>,
        environment_fallback: Option<TmdbEnvironmentFallback>,
        provider_factory: Arc<TmdbProviderFactory>,
    ) -> Self {
        Self {
            database,
            cipher,
            runtime,
            environment_fallback,
            provider_factory,
        }
    }

    async fn settings(&self) -> Result<MetadataSettingsDto, MetadataSettingsAdminError> {
        let stored = MetadataProviderSettingsRepository::new(&self.database)
            .get(TMDB_PROVIDER_KEY)
            .await?;
        Ok(self.settings_dto(stored.as_ref()))
    }

    async fn put(
        &self,
        request: PutMetadataSettingsRequest,
    ) -> Result<MetadataSettingsDto, MetadataSettingsAdminError> {
        let cipher = self
            .cipher
            .as_ref()
            .ok_or(MetadataSettingsAdminError::CipherUnavailable)?;
        let repository = MetadataProviderSettingsRepository::new(&self.database);
        let current = repository.get(TMDB_PROVIDER_KEY).await?;
        let credential_id = current
            .as_ref()
            .map_or_else(Uuid::new_v4, MetadataProviderSettingRecord::credential_id);
        let plaintext = if let Some(access_token) = request.access_token {
            Zeroizing::new(access_token.as_bytes().to_vec())
        } else {
            let current = current
                .as_ref()
                .ok_or(MetadataSettingsAdminError::CredentialUnavailable)?;
            cipher.open(
                current.credential_id(),
                current.provider(),
                current.envelope(),
            )?
        };
        let provider = self.build_provider(&plaintext, &request.language)?;
        let sealed = cipher.seal_bound(credential_id, TMDB_PROVIDER_KEY, &plaintext)?;
        let stored = repository
            .put(
                &sealed,
                request.enabled,
                &request.language,
                request.revision,
            )
            .await?;
        self.runtime
            .replace(stored.enabled().then_some(provider as Arc<_>));
        Ok(self.settings_dto(Some(&stored)))
    }

    async fn delete(&self) -> Result<(), MetadataSettingsAdminError> {
        self.cipher
            .as_ref()
            .ok_or(MetadataSettingsAdminError::CipherUnavailable)?;
        MetadataProviderSettingsRepository::new(&self.database)
            .delete(TMDB_PROVIDER_KEY, None)
            .await?;
        self.restore_environment_fallback();
        Ok(())
    }

    async fn test(
        &self,
        request: TestMetadataSettingsRequest,
    ) -> Result<TestMetadataSettingsDto, MetadataSettingsAdminError> {
        let provider = if let Some(access_token) = request.access_token {
            let language = request.language.as_deref().unwrap_or(DEFAULT_TMDB_LANGUAGE);
            self.build_provider(access_token.as_bytes(), language)?
        } else {
            let cipher = self
                .cipher
                .as_ref()
                .ok_or(MetadataSettingsAdminError::CipherUnavailable)?;
            let stored = MetadataProviderSettingsRepository::new(&self.database)
                .get(TMDB_PROVIDER_KEY)
                .await?;
            if let Some(stored) = stored {
                let plaintext =
                    cipher.open(stored.credential_id(), stored.provider(), stored.envelope())?;
                let language = request.language.as_deref().unwrap_or(stored.language());
                self.build_provider(&plaintext, language)?
            } else {
                let fallback = self
                    .environment_fallback
                    .as_ref()
                    .ok_or(MetadataSettingsAdminError::CredentialUnavailable)?;
                if request
                    .language
                    .as_deref()
                    .is_some_and(|language| language != fallback.language)
                {
                    return Err(MetadataSettingsAdminError::InvalidConfiguration);
                }
                Arc::clone(&fallback.provider)
            }
        };
        provider.validate_connection().await?;
        Ok(TestMetadataSettingsDto { status: "Success" })
    }

    fn settings_dto(&self, stored: Option<&MetadataProviderSettingRecord>) -> MetadataSettingsDto {
        if let Some(stored) = stored {
            return MetadataSettingsDto {
                provider: "Tmdb",
                configured: true,
                enabled: stored.enabled(),
                language: stored.language().to_owned(),
                revision: Some(stored.revision()),
                source: "Database",
                encryption_available: self.cipher.is_some(),
            };
        }
        if let Some(fallback) = self.environment_fallback.as_ref() {
            return MetadataSettingsDto {
                provider: "Tmdb",
                configured: true,
                enabled: true,
                language: fallback.language.clone(),
                revision: None,
                source: "Environment",
                encryption_available: self.cipher.is_some(),
            };
        }
        MetadataSettingsDto {
            provider: "Tmdb",
            configured: false,
            enabled: false,
            language: DEFAULT_TMDB_LANGUAGE.to_owned(),
            revision: None,
            source: "None",
            encryption_available: self.cipher.is_some(),
        }
    }

    fn build_provider(
        &self,
        plaintext: &[u8],
        language: &str,
    ) -> Result<Arc<TmdbProvider>, MetadataSettingsAdminError> {
        let access_token =
            str::from_utf8(plaintext).map_err(|_| MetadataSettingsAdminError::InvalidCredential)?;
        (self.provider_factory)(access_token, language)
            .map(Arc::new)
            .map_err(|_| MetadataSettingsAdminError::InvalidConfiguration)
    }

    fn restore_environment_fallback(&self) {
        self.runtime.replace(
            self.environment_fallback
                .as_ref()
                .map(|fallback| Arc::clone(&fallback.provider) as Arc<_>),
        );
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct PutMetadataSettingsRequest {
    enabled: bool,
    language: String,
    access_token: Option<Zeroizing<String>>,
    revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct TestMetadataSettingsRequest {
    access_token: Option<Zeroizing<String>>,
    language: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct MetadataSettingsDto {
    provider: &'static str,
    configured: bool,
    enabled: bool,
    language: String,
    revision: Option<i64>,
    source: &'static str,
    encryption_available: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct TestMetadataSettingsDto {
    status: &'static str,
}

pub(crate) async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return no_store(response);
    }
    let Some(service) = state.metadata_settings_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.settings().await {
        Ok(settings) => no_store(Json(settings).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn put(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return no_store(response);
    }
    let request = match json_request::<PutMetadataSettingsRequest>(&headers, &body) {
        Ok(request) => request,
        Err(response) => return no_store(response),
    };
    let Some(service) = state.metadata_settings_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.put(request).await {
        Ok(settings) => no_store(Json(settings).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return no_store(response);
    }
    let Some(service) = state.metadata_settings_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.delete().await {
        Ok(()) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn test(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return no_store(response);
    }
    let request = match json_request::<TestMetadataSettingsRequest>(&headers, &body) {
        Ok(request) => request,
        Err(response) => return no_store(response),
    };
    let Some(service) = state.metadata_settings_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.test(request).await {
        Ok(status) => no_store(Json(status).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

async fn administrator(
    state: &AppState,
    headers: &HeaderMap,
    raw_query: Option<&str>,
) -> Result<(), Response> {
    if !auth_only_query(raw_query) {
        return Err(StatusCode::BAD_REQUEST.into_response());
    }
    auth::authenticated_administrator(state, headers, raw_query)
        .await
        .map(|_| ())
}

fn auth_only_query(raw_query: Option<&str>) -> bool {
    let Ok(mut query) = auth::request_query(raw_query) else {
        return false;
    };
    query.remove("ApiKey");
    query.remove("api_key");
    query.is_empty()
}

#[allow(clippy::result_large_err)] // Route parsing returns the ready-to-send Axum response.
fn json_request<Request>(headers: &HeaderMap, body: &[u8]) -> Result<Request, Response>
where
    Request: serde::de::DeserializeOwned,
{
    if !auth::is_json_content_type(headers) {
        return Err(StatusCode::BAD_REQUEST.into_response());
    }
    serde_json::from_slice(body).map_err(|_| StatusCode::BAD_REQUEST.into_response())
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn error_response(error: &MetadataSettingsAdminError) -> Response {
    match error {
        MetadataSettingsAdminError::CipherUnavailable
        | MetadataSettingsAdminError::Cipher(_)
        | MetadataSettingsAdminError::Repository(
            MetadataProviderSettingsRepositoryError::Database(_)
            | MetadataProviderSettingsRepositoryError::RollbackFailed { .. }
            | MetadataProviderSettingsRepositoryError::InvalidStoredEnvelope,
        ) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        MetadataSettingsAdminError::Repository(
            MetadataProviderSettingsRepositoryError::RevisionConflict
            | MetadataProviderSettingsRepositoryError::CredentialIdentityConflict,
        ) => StatusCode::CONFLICT.into_response(),
        MetadataSettingsAdminError::Provider(MetadataProviderError::Rejected) => {
            StatusCode::UNPROCESSABLE_ENTITY.into_response()
        }
        MetadataSettingsAdminError::Provider(
            MetadataProviderError::TemporarilyUnavailable | MetadataProviderError::InvalidResponse,
        ) => StatusCode::BAD_GATEWAY.into_response(),
        MetadataSettingsAdminError::CredentialUnavailable
        | MetadataSettingsAdminError::InvalidCredential
        | MetadataSettingsAdminError::InvalidConfiguration
        | MetadataSettingsAdminError::Repository(
            MetadataProviderSettingsRepositoryError::InvalidProvider
            | MetadataProviderSettingsRepositoryError::InvalidLanguage
            | MetadataProviderSettingsRepositoryError::InvalidRevision,
        ) => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[derive(Debug, Error)]
enum MetadataSettingsAdminError {
    #[error("metadata provider credential encryption is unavailable")]
    CipherUnavailable,
    #[error("metadata provider credential is unavailable")]
    CredentialUnavailable,
    #[error("metadata provider credential is invalid")]
    InvalidCredential,
    #[error("metadata provider configuration is invalid")]
    InvalidConfiguration,
    #[error("metadata provider credential operation failed")]
    Cipher(#[from] CredentialCipherError),
    #[error("metadata provider settings persistence failed")]
    Repository(#[from] MetadataProviderSettingsRepositoryError),
    #[error("metadata provider connection test failed")]
    Provider(#[from] MetadataProviderError),
}
