//! Recoverable legacy media-server import adapters.

use std::{fmt, net::IpAddr, sync::Arc, time::Duration};

use async_trait::async_trait;
use reqwest::{Client, Url};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use tjxy_db::{ClaimedImportJob, ImportJobRepository, ImportStagingItem};
use zeroize::Zeroizing;

const PAGE_SIZE: u64 = 200;
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_ID_CHARS: usize = 2048;
const MAX_CREDENTIAL_PAYLOAD_BYTES: usize = 128 * 1024;

#[derive(Clone)]
pub struct EmbyApiCredentials {
    base_url: Url,
    user_id: String,
    api_key: Zeroizing<String>,
}

impl EmbyApiCredentials {
    /// Creates bounded credentials for an Emby API source.
    ///
    /// Plain HTTP is accepted only for loopback hosts so local server migrations remain usable.
    ///
    /// # Errors
    ///
    /// Returns [`EmbyImportError::InvalidConfiguration`] for unsafe URLs or invalid fields.
    pub fn new(
        base_url: &str,
        user_id: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, EmbyImportError> {
        let mut base_url = Url::parse(base_url)
            .map_err(|_| EmbyImportError::InvalidConfiguration("Emby base URL is invalid"))?;
        if base_url.cannot_be_a_base()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
            || base_url.username() != ""
            || base_url.password().is_some()
            || !matches!(base_url.scheme(), "https" | "http")
            || base_url.scheme() == "http" && !base_url.host_str().is_some_and(is_loopback_host)
        {
            return Err(EmbyImportError::InvalidConfiguration(
                "Emby base URL must be HTTPS or loopback HTTP without credentials, query, or fragment",
            ));
        }
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }
        let user_id = user_id.into();
        let api_key = api_key.into();
        if !valid_secret(&user_id) || !valid_secret(&api_key) {
            return Err(EmbyImportError::InvalidConfiguration(
                "Emby user ID or API key is invalid",
            ));
        }
        Ok(Self {
            base_url,
            user_id,
            api_key: Zeroizing::new(api_key),
        })
    }

    /// Encodes the versioned plaintext consumed only by the authenticated credential store.
    ///
    /// # Errors
    ///
    /// Returns [`EmbyImportError::InvalidConfiguration`] if encoding exceeds its bound.
    pub fn to_payload_json(&self) -> Result<Zeroizing<Vec<u8>>, EmbyImportError> {
        let encoded = serde_json::to_vec(&EmbyCredentialPayloadRef {
            version: 1,
            base_url: self.base_url.as_str(),
            user_id: &self.user_id,
            api_key: &self.api_key,
        })
        .map_err(|_| {
            EmbyImportError::InvalidConfiguration("Emby credential payload could not be encoded")
        })?;
        if encoded.len() > MAX_CREDENTIAL_PAYLOAD_BYTES {
            return Err(EmbyImportError::InvalidConfiguration(
                "Emby credential payload is too large",
            ));
        }
        Ok(Zeroizing::new(encoded))
    }

    /// Decodes one authenticated, versioned credential payload.
    ///
    /// # Errors
    ///
    /// Returns [`EmbyImportError::InvalidConfiguration`] for malformed or unsupported payloads.
    pub fn from_payload_json(payload: &[u8]) -> Result<Self, EmbyImportError> {
        if payload.is_empty() || payload.len() > MAX_CREDENTIAL_PAYLOAD_BYTES {
            return Err(EmbyImportError::InvalidConfiguration(
                "Emby credential payload is invalid",
            ));
        }
        let payload: EmbyCredentialPayload = serde_json::from_slice(payload).map_err(|_| {
            EmbyImportError::InvalidConfiguration("Emby credential payload is malformed")
        })?;
        if payload.version != 1 {
            return Err(EmbyImportError::InvalidConfiguration(
                "Emby credential payload version is unsupported",
            ));
        }
        Self::new(
            &payload.base_url,
            payload.user_id,
            payload.api_key.to_string(),
        )
    }
}

#[derive(Serialize)]
struct EmbyCredentialPayloadRef<'value> {
    version: u8,
    base_url: &'value str,
    user_id: &'value str,
    api_key: &'value str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmbyCredentialPayload {
    version: u8,
    base_url: String,
    user_id: String,
    api_key: Zeroizing<String>,
}

impl fmt::Debug for EmbyApiCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbyApiCredentials")
            .field("base_url", &self.base_url)
            .field("user_id", &self.user_id)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

#[async_trait]
pub trait EmbyApiTransport: Send + Sync {
    /// Fetches one raw Emby item page. Implementations must not place `api_key` in a URL.
    async fn fetch_items(
        &self,
        base_url: &Url,
        user_id: &str,
        api_key: &str,
        start_index: u64,
        limit: u64,
    ) -> Result<Value, EmbyImportError>;
}

struct ReqwestEmbyApiTransport {
    client: Client,
}

impl ReqwestEmbyApiTransport {
    fn new() -> Result<Self, EmbyImportError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| EmbyImportError::InvalidConfiguration("Emby HTTP client is invalid"))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl EmbyApiTransport for ReqwestEmbyApiTransport {
    async fn fetch_items(
        &self,
        base_url: &Url,
        user_id: &str,
        api_key: &str,
        start_index: u64,
        limit: u64,
    ) -> Result<Value, EmbyImportError> {
        let endpoint = base_url
            .join(&format!("Users/{user_id}/Items"))
            .map_err(|_| EmbyImportError::InvalidConfiguration("Emby endpoint is invalid"))?;
        let response = self
            .client
            .get(endpoint)
            .header("X-Emby-Token", api_key)
            .query(&[
                ("StartIndex", start_index.to_string()),
                ("Limit", limit.to_string()),
                ("Recursive", "true".to_owned()),
                (
                    "IncludeItemTypes",
                    "Movie,Series,Season,Episode".to_owned(),
                ),
                (
                    "Fields",
                    "Overview,Path,ProviderIds,Genres,People,Studios,ParentId,SeriesId,SeasonId,IndexNumber,UserData,ImageTags,BackdropImageTags"
                        .to_owned(),
                ),
            ])
            .send()
            .await
            .map_err(|_| EmbyImportError::Transport("Emby item request failed"))?;
        if !response.status().is_success() {
            return Err(EmbyImportError::Transport("Emby item request was rejected"));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(EmbyImportError::InvalidResponse(
                "Emby item response is too large",
            ));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| EmbyImportError::Transport("Emby item response body failed"))?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(EmbyImportError::InvalidResponse(
                "Emby item response is too large",
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| EmbyImportError::InvalidResponse("Emby item response is malformed"))
    }
}

pub struct EmbyApiImporter {
    database: DatabaseConnection,
    credentials: EmbyApiCredentials,
    transport: Arc<dyn EmbyApiTransport>,
}

impl EmbyApiImporter {
    /// Creates the production Emby API staging importer.
    ///
    /// # Errors
    ///
    /// Returns [`EmbyImportError`] when the bounded HTTP client cannot be configured.
    pub fn new(
        database: DatabaseConnection,
        credentials: EmbyApiCredentials,
    ) -> Result<Self, EmbyImportError> {
        Ok(Self {
            database,
            credentials,
            transport: Arc::new(ReqwestEmbyApiTransport::new()?),
        })
    }

    #[must_use]
    pub fn with_transport(mut self, transport: Arc<impl EmbyApiTransport + 'static>) -> Self {
        self.transport = transport;
        self
    }

    /// Resumes a claimed import from its durable page checkpoint and stages every returned item.
    ///
    /// Dry runs finish after staging. Non-dry runs deliberately stop before catalog publication;
    /// identity resolution and the generation transaction own that later boundary.
    ///
    /// # Errors
    ///
    /// Returns transport, response-validation, lease, or staging failures.
    pub async fn run_claimed(
        &self,
        claimed: &ClaimedImportJob,
    ) -> Result<EmbyImportReport, EmbyImportError> {
        let repository = ImportJobRepository::new(&self.database);
        let mut start_index = checkpoint_start(claimed.job().checkpoint())?;
        let mut imported = start_index;
        loop {
            let raw = self
                .transport
                .fetch_items(
                    &self.credentials.base_url,
                    &self.credentials.user_id,
                    &self.credentials.api_key,
                    start_index,
                    PAGE_SIZE,
                )
                .await?;
            let page: EmbyItemsPage = serde_json::from_value(raw).map_err(|_| {
                EmbyImportError::InvalidResponse("Emby item page has an invalid shape")
            })?;
            let page_limit = usize::try_from(PAGE_SIZE)
                .map_err(|_| EmbyImportError::InvalidResponse("Emby page limit is unsupported"))?;
            if page.items.len() > page_limit {
                return Err(EmbyImportError::InvalidResponse(
                    "Emby item page exceeds the requested limit",
                ));
            }
            if page.items.is_empty() {
                if start_index < page.total_record_count {
                    return Err(EmbyImportError::InvalidResponse(
                        "Emby pagination ended before the reported total",
                    ));
                }
                break;
            }
            let page_len = u64::try_from(page.items.len()).map_err(|_| {
                EmbyImportError::InvalidResponse("Emby item page length is invalid")
            })?;
            for raw_item in page.items {
                let item = raw_item.into_staging_item()?;
                repository.stage_item(claimed, &item).await?;
            }
            start_index =
                start_index
                    .checked_add(page_len)
                    .ok_or(EmbyImportError::InvalidResponse(
                        "Emby pagination index overflowed",
                    ))?;
            imported = imported
                .checked_add(page_len)
                .ok_or(EmbyImportError::InvalidResponse(
                    "Emby imported item count overflowed",
                ))?;
            repository
                .save_checkpoint(claimed, json!({"start_index": start_index}))
                .await?;
            if start_index >= page.total_record_count {
                break;
            }
        }
        let report = EmbyImportReport { items: imported };
        if claimed.job().dry_run() {
            repository
                .complete_dry_run(
                    claimed,
                    json!({"items": imported, "conflicts": 0, "errors": 0}),
                )
                .await?;
            Ok(report)
        } else {
            repository
                .seal_for_publication(
                    claimed,
                    json!({"items": imported, "conflicts": 0, "errors": 0}),
                )
                .await?;
            Ok(report)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbyImportReport {
    items: u64,
}

impl EmbyImportReport {
    #[must_use]
    pub const fn items(self) -> u64 {
        self.items
    }
}

#[derive(Debug, Error)]
pub enum EmbyImportError {
    #[error("invalid import configuration: {0}")]
    InvalidConfiguration(&'static str),
    #[error("legacy server transport failed: {0}")]
    Transport(&'static str),
    #[error("legacy server returned an invalid response: {0}")]
    InvalidResponse(&'static str),
    #[error("import staging failed: {0}")]
    Staging(#[from] tjxy_db::ImportStagingRepositoryError),
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EmbyItemsPage {
    #[serde(default)]
    items: Vec<EmbyItem>,
    total_record_count: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EmbyItem {
    id: String,
    name: String,
    #[serde(rename = "Type")]
    entity_kind: String,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    series_id: Option<String>,
    #[serde(default)]
    season_id: Option<String>,
    #[serde(default)]
    index_number: Option<i64>,
    #[serde(default)]
    production_year: Option<i32>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    provider_ids: Value,
    #[serde(default)]
    genres: Value,
    #[serde(default)]
    people: Value,
    #[serde(default)]
    studios: Value,
    #[serde(default)]
    user_data: EmbyUserData,
    #[serde(default)]
    image_tags: Value,
    #[serde(default)]
    backdrop_image_tags: Value,
}

impl EmbyItem {
    fn into_staging_item(self) -> Result<ImportStagingItem, EmbyImportError> {
        if !valid_identity(&self.id)
            || !valid_identity(&self.name)
            || !valid_identity(&self.entity_kind)
            || self
                .parent_id
                .as_deref()
                .is_some_and(|id| !valid_identity(id))
        {
            return Err(EmbyImportError::InvalidResponse(
                "Emby item identity is invalid",
            ));
        }
        let payload = json!({
            "version": 1,
            "name": self.name,
            "production_year": self.production_year,
            "overview": self.overview,
            "legacy_path": self.path,
            "provider_ids": self.provider_ids,
            "genres": self.genres,
            "people": self.people,
            "studios": self.studios,
            "series_legacy_id": self.series_id,
            "season_legacy_id": self.season_id,
            "index_number": self.index_number,
            "user_data": {
                "is_favorite": self.user_data.is_favorite,
                "played": self.user_data.played,
                "play_count": self.user_data.play_count,
                "playback_position_ticks": self.user_data.playback_position_ticks,
            },
            "image_tags": self.image_tags,
            "backdrop_image_tags": self.backdrop_image_tags,
        });
        Ok(ImportStagingItem::new(
            self.entity_kind,
            self.id,
            self.parent_id,
            payload,
        )?)
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EmbyUserData {
    #[serde(default)]
    is_favorite: bool,
    #[serde(default)]
    played: bool,
    #[serde(default)]
    play_count: u64,
    #[serde(default)]
    playback_position_ticks: u64,
}

fn checkpoint_start(checkpoint: &Value) -> Result<u64, EmbyImportError> {
    match checkpoint.get("start_index") {
        None => Ok(0),
        Some(value) => value.as_u64().ok_or(EmbyImportError::InvalidResponse(
            "Emby import checkpoint is invalid",
        )),
    }
}

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_ID_CHARS
        && !value.chars().any(char::is_control)
}

fn valid_secret(value: &str) -> bool {
    valid_identity(value) && value.len() <= 16 * 1024
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}
