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
    MetadataError, MetadataItemKind, MetadataLookup, MetadataProvider, MetadataProviderError,
    ReloadableMetadataProvider, TmdbCatalogClient, TmdbProvider,
};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{AppState, auth};

pub(crate) const TMDB_PROVIDER_KEY: &str = "tmdb";
pub(crate) const THEAUDIODB_PROVIDER_KEY: &str = "theaudiodb";
pub(crate) const MUSICBRAINZ_PROVIDER_KEY: &str = "musicbrainz";
pub(crate) const DEFAULT_TMDB_LANGUAGE: &str = "zh-CN";
const DEFAULT_MUSIC_LANGUAGE: &str = "und";

pub(crate) type TmdbProviderFactory =
    dyn Fn(&str, &str) -> Result<TmdbProvider, MetadataError> + Send + Sync;
pub(crate) type MusicProviderFactory =
    dyn Fn(&str) -> Result<Arc<dyn MetadataProvider>, MetadataError> + Send + Sync;

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

#[derive(Clone)]
pub(crate) struct MusicProviderEnvironmentFallback {
    provider: Arc<dyn MetadataProvider>,
    display_value: Option<String>,
}

impl MusicProviderEnvironmentFallback {
    pub(crate) fn new(provider: Arc<dyn MetadataProvider>, display_value: Option<String>) -> Self {
        Self {
            provider,
            display_value,
        }
    }
}

struct MusicProviderAdmin {
    key: &'static str,
    name: &'static str,
    runtime: Arc<ReloadableMetadataProvider>,
    environment_fallback: Option<MusicProviderEnvironmentFallback>,
    provider_factory: Arc<MusicProviderFactory>,
    exposes_value: bool,
}

pub(crate) struct MetadataSettingsAdminService {
    database: sea_orm::DatabaseConnection,
    cipher: Option<Arc<CredentialCipher>>,
    runtime: Arc<ReloadableMetadataProvider>,
    environment_fallback: Option<TmdbEnvironmentFallback>,
    provider_factory: Arc<TmdbProviderFactory>,
    the_audio_db: MusicProviderAdmin,
    musicbrainz: MusicProviderAdmin,
}

impl MetadataSettingsAdminService {
    pub(crate) fn new(
        database: sea_orm::DatabaseConnection,
        cipher: Option<Arc<CredentialCipher>>,
        runtime: Arc<ReloadableMetadataProvider>,
        environment_fallback: Option<TmdbEnvironmentFallback>,
        provider_factory: Arc<TmdbProviderFactory>,
        the_audio_db_runtime: Arc<ReloadableMetadataProvider>,
        the_audio_db_environment_fallback: Option<MusicProviderEnvironmentFallback>,
        the_audio_db_provider_factory: Arc<MusicProviderFactory>,
        musicbrainz_runtime: Arc<ReloadableMetadataProvider>,
        musicbrainz_environment_fallback: Option<MusicProviderEnvironmentFallback>,
        musicbrainz_provider_factory: Arc<MusicProviderFactory>,
    ) -> Self {
        Self {
            database,
            cipher,
            runtime,
            environment_fallback,
            provider_factory,
            the_audio_db: MusicProviderAdmin {
                key: THEAUDIODB_PROVIDER_KEY,
                name: "TheAudioDB",
                runtime: the_audio_db_runtime,
                environment_fallback: the_audio_db_environment_fallback,
                provider_factory: the_audio_db_provider_factory,
                exposes_value: false,
            },
            musicbrainz: MusicProviderAdmin {
                key: MUSICBRAINZ_PROVIDER_KEY,
                name: "MusicBrainz",
                runtime: musicbrainz_runtime,
                environment_fallback: musicbrainz_environment_fallback,
                provider_factory: musicbrainz_provider_factory,
                exposes_value: true,
            },
        }
    }

    pub(crate) async fn tmdb_catalog_client(
        &self,
    ) -> Result<TmdbCatalogClient, MetadataSettingsAdminError> {
        let cipher = self
            .cipher
            .as_ref()
            .ok_or(MetadataSettingsAdminError::CipherUnavailable)?;
        let stored = MetadataProviderSettingsRepository::new(&self.database)
            .get(TMDB_PROVIDER_KEY)
            .await?
            .filter(MetadataProviderSettingRecord::enabled)
            .ok_or(MetadataSettingsAdminError::CredentialUnavailable)?;
        let plaintext =
            cipher.open(stored.credential_id(), stored.provider(), stored.envelope())?;
        let access_token = str::from_utf8(&plaintext)
            .map_err(|_| MetadataSettingsAdminError::InvalidCredential)?;
        TmdbCatalogClient::new(access_token, stored.language())
            .map_err(|_| MetadataSettingsAdminError::InvalidConfiguration)
    }

    async fn settings(&self) -> Result<MetadataSettingsDto, MetadataSettingsAdminError> {
        let stored = MetadataProviderSettingsRepository::new(&self.database)
            .get(TMDB_PROVIDER_KEY)
            .await?;
        Ok(self.settings_dto(stored.as_ref()))
    }

    async fn music_settings(
        &self,
        provider: &str,
    ) -> Result<ProviderSettingsDto, MetadataSettingsAdminError> {
        let admin = self.music_provider(provider)?;
        let stored = MetadataProviderSettingsRepository::new(&self.database)
            .get(admin.key)
            .await?;
        self.music_settings_dto(admin, stored.as_ref()).await
    }

    async fn put_music(
        &self,
        provider: &str,
        request: MusicPutSettingsRequest,
    ) -> Result<ProviderSettingsDto, MetadataSettingsAdminError> {
        let admin = self.music_provider(provider)?;
        let cipher = self
            .cipher
            .as_ref()
            .ok_or(MetadataSettingsAdminError::CipherUnavailable)?;
        let repository = MetadataProviderSettingsRepository::new(&self.database);
        let current = repository.get(admin.key).await?;
        let credential_id = current
            .as_ref()
            .map_or_else(Uuid::new_v4, MetadataProviderSettingRecord::credential_id);
        let plaintext = if let Some(value) = request.value {
            value
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
        let value = str::from_utf8(&plaintext)
            .map_err(|_| MetadataSettingsAdminError::InvalidCredential)?;
        validate_music_value(admin.key, value)?;
        let provider = (admin.provider_factory)(value)
            .map_err(|_| MetadataSettingsAdminError::InvalidConfiguration)?;
        let sealed = cipher.seal_bound(credential_id, admin.key, &plaintext)?;
        let stored = repository
            .put(
                &sealed,
                request.enabled,
                if admin.key == MUSICBRAINZ_PROVIDER_KEY {
                    DEFAULT_MUSIC_LANGUAGE
                } else {
                    "en-US"
                },
                request.revision,
            )
            .await?;
        admin.runtime.replace(request.enabled.then_some(provider));
        self.music_settings_dto(admin, Some(&stored)).await
    }

    async fn delete_music(&self, provider: &str) -> Result<(), MetadataSettingsAdminError> {
        let admin = self.music_provider(provider)?;
        self.cipher
            .as_ref()
            .ok_or(MetadataSettingsAdminError::CipherUnavailable)?;
        MetadataProviderSettingsRepository::new(&self.database)
            .delete(admin.key, None)
            .await?;
        admin.runtime.replace(
            admin
                .environment_fallback
                .as_ref()
                .map(|fallback| Arc::clone(&fallback.provider)),
        );
        Ok(())
    }

    async fn test_music(
        &self,
        provider: &str,
        value: Option<Zeroizing<String>>,
    ) -> Result<TestMetadataSettingsDto, MetadataSettingsAdminError> {
        let admin = self.music_provider(provider)?;
        let provider = if let Some(value) = value {
            validate_music_value(admin.key, &value)?;
            (admin.provider_factory)(&value)
                .map_err(|_| MetadataSettingsAdminError::InvalidConfiguration)?
        } else if let Some(stored) = MetadataProviderSettingsRepository::new(&self.database)
            .get(admin.key)
            .await?
        {
            let cipher = self
                .cipher
                .as_ref()
                .ok_or(MetadataSettingsAdminError::CipherUnavailable)?;
            let plaintext =
                cipher.open(stored.credential_id(), stored.provider(), stored.envelope())?;
            let value = str::from_utf8(&plaintext)
                .map_err(|_| MetadataSettingsAdminError::InvalidCredential)?;
            (admin.provider_factory)(value)
                .map_err(|_| MetadataSettingsAdminError::InvalidConfiguration)?
        } else {
            Arc::clone(
                &admin
                    .environment_fallback
                    .as_ref()
                    .ok_or(MetadataSettingsAdminError::CredentialUnavailable)?
                    .provider,
            )
        };
        let lookup = MetadataLookup::new(MetadataItemKind::Audio, "Artist - Track", None)
            .map_err(|_| MetadataSettingsAdminError::InvalidConfiguration)?;
        provider.resolve(&lookup).await?;
        Ok(TestMetadataSettingsDto { status: "Success" })
    }

    fn music_provider(
        &self,
        provider: &str,
    ) -> Result<&MusicProviderAdmin, MetadataSettingsAdminError> {
        match provider {
            THEAUDIODB_PROVIDER_KEY => Ok(&self.the_audio_db),
            MUSICBRAINZ_PROVIDER_KEY => Ok(&self.musicbrainz),
            _ => Err(MetadataSettingsAdminError::InvalidConfiguration),
        }
    }

    async fn music_settings_dto(
        &self,
        admin: &MusicProviderAdmin,
        stored: Option<&MetadataProviderSettingRecord>,
    ) -> Result<ProviderSettingsDto, MetadataSettingsAdminError> {
        let (configured, enabled, revision, source, value) = if let Some(stored) = stored {
            let value = if admin.exposes_value {
                let cipher = self
                    .cipher
                    .as_ref()
                    .ok_or(MetadataSettingsAdminError::CipherUnavailable)?;
                let plaintext =
                    cipher.open(stored.credential_id(), stored.provider(), stored.envelope())?;
                Some(
                    str::from_utf8(&plaintext)
                        .map_err(|_| MetadataSettingsAdminError::InvalidCredential)?
                        .to_owned(),
                )
            } else {
                None
            };
            (
                true,
                stored.enabled(),
                Some(stored.revision()),
                "Database",
                value,
            )
        } else if let Some(fallback) = admin.environment_fallback.as_ref() {
            (
                true,
                true,
                None,
                "Environment",
                fallback.display_value.clone(),
            )
        } else {
            (false, false, None, "None", None)
        };
        if admin.exposes_value {
            Ok(ProviderSettingsDto::MusicBrainz(MusicBrainzSettingsDto {
                provider: admin.name,
                configured,
                enabled,
                user_agent: value.unwrap_or_default(),
                revision,
                source,
                encryption_available: self.cipher.is_some(),
            }))
        } else {
            Ok(ProviderSettingsDto::TheAudioDb(MusicSecretSettingsDto {
                provider: admin.name,
                configured,
                enabled,
                revision,
                source,
                encryption_available: self.cipher.is_some(),
            }))
        }
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

struct MusicPutSettingsRequest {
    enabled: bool,
    value: Option<Zeroizing<Vec<u8>>>,
    revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct PutAudioDbSettingsRequest {
    enabled: bool,
    api_key: Option<Zeroizing<String>>,
    revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct PutMusicBrainzSettingsRequest {
    enabled: bool,
    user_agent: Option<Zeroizing<String>>,
    revision: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct TestAudioDbSettingsRequest {
    api_key: Option<Zeroizing<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct TestMusicBrainzSettingsRequest {
    user_agent: Option<Zeroizing<String>>,
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
#[serde(untagged)]
enum ProviderSettingsDto {
    TheAudioDb(MusicSecretSettingsDto),
    MusicBrainz(MusicBrainzSettingsDto),
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct MusicSecretSettingsDto {
    provider: &'static str,
    configured: bool,
    enabled: bool,
    revision: Option<i64>,
    source: &'static str,
    encryption_available: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct MusicBrainzSettingsDto {
    provider: &'static str,
    configured: bool,
    enabled: bool,
    user_agent: String,
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

pub(crate) async fn get_the_audio_db(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    get_music(state, headers, raw_query, THEAUDIODB_PROVIDER_KEY).await
}

pub(crate) async fn get_musicbrainz(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    get_music(state, headers, raw_query, MUSICBRAINZ_PROVIDER_KEY).await
}

async fn get_music(
    state: AppState,
    headers: HeaderMap,
    raw_query: Option<String>,
    provider: &'static str,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return no_store(response);
    }
    let Some(service) = state.metadata_settings_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.music_settings(provider).await {
        Ok(settings) => no_store(Json(settings).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn put_the_audio_db(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return no_store(response);
    }
    let request = match json_request::<PutAudioDbSettingsRequest>(&headers, &body) {
        Ok(request) => request,
        Err(response) => return no_store(response),
    };
    put_music_route(
        state,
        THEAUDIODB_PROVIDER_KEY,
        MusicPutSettingsRequest {
            enabled: request.enabled,
            value: request
                .api_key
                .map(|value| Zeroizing::new(value.as_bytes().to_vec())),
            revision: request.revision,
        },
    )
    .await
}

pub(crate) async fn put_musicbrainz(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return no_store(response);
    }
    let request = match json_request::<PutMusicBrainzSettingsRequest>(&headers, &body) {
        Ok(request) => request,
        Err(response) => return no_store(response),
    };
    put_music_route(
        state,
        MUSICBRAINZ_PROVIDER_KEY,
        MusicPutSettingsRequest {
            enabled: request.enabled,
            value: request
                .user_agent
                .map(|value| Zeroizing::new(value.as_bytes().to_vec())),
            revision: request.revision,
        },
    )
    .await
}

async fn put_music_route(
    state: AppState,
    provider: &'static str,
    request: MusicPutSettingsRequest,
) -> Response {
    let Some(service) = state.metadata_settings_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.put_music(provider, request).await {
        Ok(settings) => no_store(Json(settings).into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn delete_the_audio_db(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    delete_music_route(state, headers, raw_query, THEAUDIODB_PROVIDER_KEY).await
}

pub(crate) async fn delete_musicbrainz(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Response {
    delete_music_route(state, headers, raw_query, MUSICBRAINZ_PROVIDER_KEY).await
}

async fn delete_music_route(
    state: AppState,
    headers: HeaderMap,
    raw_query: Option<String>,
    provider: &'static str,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return no_store(response);
    }
    let Some(service) = state.metadata_settings_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.delete_music(provider).await {
        Ok(()) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(error) => no_store(error_response(&error)),
    }
}

pub(crate) async fn test_the_audio_db(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return no_store(response);
    }
    let request = match json_request::<TestAudioDbSettingsRequest>(&headers, &body) {
        Ok(request) => request,
        Err(response) => return no_store(response),
    };
    test_music_route(state, THEAUDIODB_PROVIDER_KEY, request.api_key).await
}

pub(crate) async fn test_musicbrainz(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = administrator(&state, &headers, raw_query.as_deref()).await {
        return no_store(response);
    }
    let request = match json_request::<TestMusicBrainzSettingsRequest>(&headers, &body) {
        Ok(request) => request,
        Err(response) => return no_store(response),
    };
    test_music_route(state, MUSICBRAINZ_PROVIDER_KEY, request.user_agent).await
}

async fn test_music_route(
    state: AppState,
    provider: &'static str,
    value: Option<Zeroizing<String>>,
) -> Response {
    let Some(service) = state.metadata_settings_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    match service.test_music(provider, value).await {
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

fn validate_music_value(provider: &str, value: &str) -> Result<(), MetadataSettingsAdminError> {
    let valid = match provider {
        THEAUDIODB_PROVIDER_KEY => {
            !value.is_empty()
                && value.len() <= 256
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                })
        }
        MUSICBRAINZ_PROVIDER_KEY => {
            value.trim() == value
                && !value.is_empty()
                && value.len() <= 512
                && !value.chars().any(char::is_control)
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(MetadataSettingsAdminError::InvalidConfiguration)
    }
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
pub(crate) enum MetadataSettingsAdminError {
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
