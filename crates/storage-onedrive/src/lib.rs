//! Microsoft Graph `OneDrive` Personal storage backend.

use std::{collections::HashMap, fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use zeroize::Zeroizing;

const PROVIDER: &str = "onedrive";
const DEFAULT_API_BASE: &str = "https://graph.microsoft.com/v1.0/";
const DEFAULT_AUTHORIZATION_ENDPOINT: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const DEFAULT_TOKEN_ENDPOINT: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";

#[async_trait]
pub trait MicrosoftAccessTokenProvider: Send + Sync {
    async fn access_token(&self) -> Result<String, BackendError>;
}

#[derive(Clone)]
pub struct MicrosoftOAuthCredentials {
    client_id: Zeroizing<String>,
    client_secret: Option<Zeroizing<String>>,
    refresh_token: Zeroizing<String>,
}

/// Server-side OAuth configuration for a Microsoft-account authorization flow.
///
/// The optional confidential-client secret stays in this value and is never included in an
/// authorization URL or Admin DTO.
pub struct MicrosoftOAuthWebClient {
    client: Client,
    client_id: Zeroizing<String>,
    client_secret: Option<Zeroizing<String>>,
    redirect_uri: Url,
    authorization_endpoint: Url,
    token_endpoint: Url,
    graph_api_base: Url,
}

impl MicrosoftOAuthWebClient {
    /// Creates a Microsoft consumer-account OAuth client using production endpoints.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for unsafe client configuration or redirect URIs.
    pub fn new(
        client_id: impl Into<String>,
        client_secret: Option<String>,
        redirect_uri: impl AsRef<str>,
    ) -> Result<Self, BackendError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| invalid("Microsoft OAuth HTTP client configuration is invalid"))?;
        Ok(Self {
            client,
            client_id: Zeroizing::new(validate_secret(
                "Microsoft OAuth client ID",
                client_id.into(),
            )?),
            client_secret: client_secret
                .map(|value| {
                    validate_secret("Microsoft OAuth client secret", value).map(Zeroizing::new)
                })
                .transpose()?,
            redirect_uri: parse_redirect_uri(redirect_uri.as_ref())?,
            authorization_endpoint: Url::parse(DEFAULT_AUTHORIZATION_ENDPOINT)
                .map_err(|_| invalid("Microsoft authorization endpoint is invalid"))?,
            token_endpoint: Url::parse(DEFAULT_TOKEN_ENDPOINT)
                .map_err(|_| invalid("Microsoft OAuth endpoint is invalid"))?,
            graph_api_base: Url::parse(DEFAULT_API_BASE)
                .map_err(|_| invalid("Microsoft Graph API base URL is invalid"))?,
        })
    }

    /// Overrides endpoints for a loopback test server or an HTTPS-compatible proxy.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for unsafe endpoint URLs.
    pub fn with_endpoints(
        mut self,
        authorization_endpoint: impl AsRef<str>,
        token_endpoint: impl AsRef<str>,
        graph_api_base: impl AsRef<str>,
    ) -> Result<Self, BackendError> {
        self.authorization_endpoint = parse_provider_endpoint(authorization_endpoint.as_ref())?;
        self.token_endpoint = parse_provider_endpoint(token_endpoint.as_ref())?;
        self.graph_api_base = parse_graph_api_base(graph_api_base.as_ref())?;
        Ok(self)
    }

    /// Builds the encoded authorization URL for one-time `state` and S256 PKCE values.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for invalid state or challenge values.
    pub fn authorization_url(
        &self,
        state: &str,
        code_challenge: &str,
    ) -> Result<String, BackendError> {
        validate_secret("Microsoft OAuth state", state.to_owned())?;
        validate_secret("Microsoft OAuth PKCE challenge", code_challenge.to_owned())?;
        let mut url = self.authorization_endpoint.clone();
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", self.redirect_uri.as_str())
            .append_pair("response_mode", "query")
            .append_pair("scope", "offline_access User.Read Files.Read")
            .append_pair("state", state)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }

    /// Exchanges an authorization code for encrypted-at-rest credential material.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when the code exchange fails or omits a refresh token.
    pub async fn exchange_authorization_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<MicrosoftOAuthCredentials, BackendError> {
        validate_secret("Microsoft OAuth authorization code", code.to_owned())?;
        validate_secret("Microsoft OAuth PKCE verifier", code_verifier.to_owned())?;
        let mut form = vec![
            ("client_id", self.client_id.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("code_verifier", code_verifier),
            ("scope", "offline_access User.Read Files.Read"),
        ];
        if let Some(secret) = self.client_secret.as_deref() {
            form.push(("client_secret", secret));
        }
        let response = self
            .client
            .post(self.token_endpoint.clone())
            .form(&form)
            .send()
            .await
            .map_err(|_| temporary("Microsoft OAuth authorization-code exchange failed"))?;
        if !response.status().is_success() {
            return Err(temporary(
                "Microsoft OAuth authorization code was rejected or expired",
            ));
        }
        let token: MicrosoftAuthorizationCodeTokenResponse = response
            .json()
            .await
            .map_err(|_| invalid("Microsoft OAuth returned malformed token JSON"))?;
        let refresh_token = token
            .refresh_token
            .ok_or_else(|| invalid("Microsoft OAuth did not return a refresh token"))?;
        MicrosoftOAuthCredentials::new(
            self.client_id.to_string(),
            self.client_secret.as_deref().map(ToString::to_string),
            refresh_token,
        )
    }

    /// Obtains trusted identity, Personal drive ID, and root ID from Microsoft Graph.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when the account is not a Personal `OneDrive` or Graph is invalid.
    pub async fn discover_personal_drive(
        &self,
        credentials: MicrosoftOAuthCredentials,
    ) -> Result<MicrosoftPersonalDrive, BackendError> {
        let refresh_client =
            MicrosoftOAuthClient::new()?.with_token_endpoint(self.token_endpoint.as_str())?;
        let provider =
            RefreshingMicrosoftAccessTokenProvider::new(credentials, Arc::new(refresh_client));
        let access_token = provider.access_token().await?;
        let user: MicrosoftGraphUser = self
            .client
            .get(
                self.graph_api_base
                    .join("me?$select=id,displayName")
                    .map_err(|_| invalid("Microsoft Graph endpoint could not be constructed"))?,
            )
            .bearer_auth(&access_token)
            .send()
            .await
            .map_err(|_| temporary("Microsoft Graph identity request failed"))
            .and_then(checked)?
            .json()
            .await
            .map_err(|_| invalid("Microsoft Graph returned malformed user JSON"))?;
        let drive: MicrosoftGraphDrive = self
            .client
            .get(
                self.graph_api_base
                    .join("me/drive?$select=id,driveType")
                    .map_err(|_| invalid("Microsoft Graph endpoint could not be constructed"))?,
            )
            .bearer_auth(&access_token)
            .send()
            .await
            .map_err(|_| temporary("Microsoft Graph drive request failed"))
            .and_then(checked)?
            .json()
            .await
            .map_err(|_| invalid("Microsoft Graph returned malformed drive JSON"))?;
        if drive.drive_type != "personal" {
            return Err(invalid(
                "OneDrive Business and SharePoint are not supported in v1",
            ));
        }
        let root: MicrosoftGraphRoot = self
            .client
            .get(
                self.graph_api_base
                    .join("me/drive/root?$select=id")
                    .map_err(|_| invalid("Microsoft Graph endpoint could not be constructed"))?,
            )
            .bearer_auth(&access_token)
            .send()
            .await
            .map_err(|_| temporary("Microsoft Graph root request failed"))
            .and_then(checked)?
            .json()
            .await
            .map_err(|_| invalid("Microsoft Graph returned malformed root JSON"))?;
        validate_graph_id(&user.id)?;
        validate_graph_id(&drive.id)?;
        validate_graph_id(&root.id)?;
        let display_name = validate_secret("Microsoft Graph display name", user.display_name)?;
        Ok(MicrosoftPersonalDrive {
            account_identity: format!("{display_name} ({})", user.id),
            drive_id: drive.id,
            root_id: root.id,
        })
    }

    /// Returns the token endpoint used by refresh clients created for this authorization flow.
    #[must_use]
    pub fn token_endpoint(&self) -> &Url {
        &self.token_endpoint
    }

    /// Returns the Microsoft Graph base used for account discovery and binding validation.
    #[must_use]
    pub fn graph_api_base(&self) -> &Url {
        &self.graph_api_base
    }
}

impl fmt::Debug for MicrosoftOAuthWebClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MicrosoftOAuthWebClient")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("redirect_uri", &self.redirect_uri)
            .field("authorization_endpoint", &self.authorization_endpoint)
            .field("token_endpoint", &self.token_endpoint)
            .field("graph_api_base", &self.graph_api_base)
            .finish_non_exhaustive()
    }
}

/// Server-derived identifiers for one Microsoft Personal `OneDrive` account.
pub struct MicrosoftPersonalDrive {
    account_identity: String,
    drive_id: String,
    root_id: String,
}

impl MicrosoftPersonalDrive {
    #[must_use]
    pub fn account_identity(&self) -> &str {
        &self.account_identity
    }

    #[must_use]
    pub fn drive_id(&self) -> &str {
        &self.drive_id
    }

    #[must_use]
    pub fn root_id(&self) -> &str {
        &self.root_id
    }
}

impl MicrosoftOAuthCredentials {
    /// Creates validated `OneDrive` Personal OAuth credentials.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for empty, unbounded, or control-containing fields.
    pub fn new(
        client_id: impl Into<String>,
        client_secret: Option<String>,
        refresh_token: impl Into<String>,
    ) -> Result<Self, BackendError> {
        Ok(Self {
            client_id: Zeroizing::new(validate_secret(
                "Microsoft OAuth client ID",
                client_id.into(),
            )?),
            client_secret: client_secret
                .map(|value| {
                    validate_secret("Microsoft OAuth client secret", value).map(Zeroizing::new)
                })
                .transpose()?,
            refresh_token: Zeroizing::new(validate_secret(
                "Microsoft OAuth refresh token",
                refresh_token.into(),
            )?),
        })
    }

    /// Encodes the versioned plaintext consumed by the encrypted credential store.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] if serialization fails or exceeds its bound.
    pub fn to_payload_json(&self) -> Result<Zeroizing<Vec<u8>>, BackendError> {
        let encoded = serde_json::to_vec(&MicrosoftCredentialPayloadRef {
            version: 1,
            account_type: "Personal",
            client_id: self.client_id.as_str(),
            client_secret: self.client_secret.as_deref().map(String::as_str),
            refresh_token: self.refresh_token.as_str(),
        })
        .map_err(|_| invalid("Microsoft OAuth credential payload could not be encoded"))?;
        if encoded.len() > 128 * 1024 {
            return Err(invalid("Microsoft OAuth credential payload is too large"));
        }
        Ok(Zeroizing::new(encoded))
    }

    /// Decodes an authenticated `OneDrive` Personal credential payload.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for malformed, unsupported, or non-Personal payloads.
    pub fn from_payload_json(payload: &[u8]) -> Result<Self, BackendError> {
        if payload.is_empty() || payload.len() > 128 * 1024 {
            return Err(invalid("Microsoft OAuth credential payload is invalid"));
        }
        let payload: MicrosoftCredentialPayload = serde_json::from_slice(payload)
            .map_err(|_| invalid("Microsoft OAuth credential payload is malformed"))?;
        if payload.version != 1 || payload.account_type != "Personal" {
            return Err(invalid(
                "Microsoft OAuth credential payload is not a supported Personal account",
            ));
        }
        Self::new(
            payload.client_id,
            payload.client_secret,
            payload.refresh_token,
        )
    }

    fn with_refresh_token(&self, refresh_token: String) -> Result<Self, BackendError> {
        Self::new(
            self.client_id.to_string(),
            self.client_secret
                .as_ref()
                .map(|value| value.as_str().to_owned()),
            refresh_token,
        )
    }
}

impl fmt::Debug for MicrosoftOAuthCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MicrosoftOAuthCredentials")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .finish()
    }
}

#[derive(Serialize)]
struct MicrosoftCredentialPayloadRef<'value> {
    version: u8,
    account_type: &'value str,
    client_id: &'value str,
    client_secret: Option<&'value str>,
    refresh_token: &'value str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MicrosoftCredentialPayload {
    version: u8,
    account_type: String,
    client_id: String,
    client_secret: Option<String>,
    refresh_token: String,
}

pub struct MicrosoftOAuthRefreshRequest<'value> {
    credentials: &'value MicrosoftOAuthCredentials,
}

impl MicrosoftOAuthRefreshRequest<'_> {
    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.credentials.client_id
    }

    #[must_use]
    pub fn client_secret(&self) -> Option<&str> {
        self.credentials
            .client_secret
            .as_deref()
            .map(String::as_str)
    }

    #[must_use]
    pub fn refresh_token(&self) -> &str {
        &self.credentials.refresh_token
    }
}

pub struct MicrosoftAccessToken {
    token: Zeroizing<String>,
    expires_in: Duration,
    refresh_token: Option<Zeroizing<String>>,
}

impl MicrosoftAccessToken {
    /// Defines one successful Microsoft OAuth refresh response.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for invalid token values or a zero lifetime.
    pub fn new(
        token: impl Into<String>,
        expires_in: Duration,
        refresh_token: Option<String>,
    ) -> Result<Self, BackendError> {
        if expires_in.is_zero() {
            return Err(invalid(
                "Microsoft OAuth access token lifetime must be positive",
            ));
        }
        Ok(Self {
            token: Zeroizing::new(validate_secret(
                "Microsoft OAuth access token",
                token.into(),
            )?),
            expires_in,
            refresh_token: refresh_token
                .map(|value| {
                    validate_secret("Microsoft OAuth refresh token", value).map(Zeroizing::new)
                })
                .transpose()?,
        })
    }
}

#[async_trait]
pub trait MicrosoftOAuthRefreshClient: Send + Sync {
    async fn refresh(
        &self,
        request: &MicrosoftOAuthRefreshRequest<'_>,
    ) -> Result<MicrosoftAccessToken, BackendError>;
}

#[async_trait]
pub trait MicrosoftCredentialStore: Send + Sync {
    async fn persist(&self, credentials: &MicrosoftOAuthCredentials) -> Result<(), BackendError>;
}

struct NoopCredentialStore;

#[async_trait]
impl MicrosoftCredentialStore for NoopCredentialStore {
    async fn persist(&self, _credentials: &MicrosoftOAuthCredentials) -> Result<(), BackendError> {
        Ok(())
    }
}

struct CachedMicrosoftAccessToken {
    token: Zeroizing<String>,
    refresh_after: tokio::time::Instant,
}

struct MicrosoftCredentialState {
    credentials: MicrosoftOAuthCredentials,
    cached: Option<CachedMicrosoftAccessToken>,
}

pub struct RefreshingMicrosoftAccessTokenProvider {
    state: tokio::sync::Mutex<MicrosoftCredentialState>,
    client: Arc<dyn MicrosoftOAuthRefreshClient>,
    store: Arc<dyn MicrosoftCredentialStore>,
}

impl RefreshingMicrosoftAccessTokenProvider {
    #[must_use]
    pub fn new(
        credentials: MicrosoftOAuthCredentials,
        client: Arc<impl MicrosoftOAuthRefreshClient + 'static>,
    ) -> Self {
        Self {
            state: tokio::sync::Mutex::new(MicrosoftCredentialState {
                credentials,
                cached: None,
            }),
            client,
            store: Arc::new(NoopCredentialStore),
        }
    }

    #[must_use]
    pub fn with_credential_store(
        mut self,
        store: Arc<impl MicrosoftCredentialStore + 'static>,
    ) -> Self {
        self.store = store;
        self
    }
}

#[async_trait]
impl MicrosoftAccessTokenProvider for RefreshingMicrosoftAccessTokenProvider {
    async fn access_token(&self) -> Result<String, BackendError> {
        let mut state = self.state.lock().await;
        let now = tokio::time::Instant::now();
        if let Some(cached) = state
            .cached
            .as_ref()
            .filter(|cached| cached.refresh_after > now)
        {
            return Ok(cached.token.to_string());
        }
        let refreshed = self
            .client
            .refresh(&MicrosoftOAuthRefreshRequest {
                credentials: &state.credentials,
            })
            .await?;
        if let Some(refresh_token) = refreshed.refresh_token.as_ref() {
            let updated = state
                .credentials
                .with_refresh_token(refresh_token.to_string())?;
            self.store.persist(&updated).await?;
            state.credentials = updated;
        }
        let skew = Duration::from_secs(60).min(refreshed.expires_in / 2);
        let refresh_after = now
            .checked_add(refreshed.expires_in.saturating_sub(skew))
            .ok_or_else(|| invalid("Microsoft OAuth expiry is outside supported range"))?;
        let token = refreshed.token;
        let exposed = token.to_string();
        state.cached = Some(CachedMicrosoftAccessToken {
            token,
            refresh_after,
        });
        Ok(exposed)
    }
}

pub struct MicrosoftOAuthClient {
    client: Client,
    token_endpoint: Url,
}

impl MicrosoftOAuthClient {
    /// Creates the production Personal Microsoft-account OAuth client.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] if the bounded HTTP client cannot be configured.
    pub fn new() -> Result<Self, BackendError> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(20))
                .build()
                .map_err(|_| invalid("Microsoft OAuth HTTP client configuration is invalid"))?,
            token_endpoint: Url::parse(DEFAULT_TOKEN_ENDPOINT)
                .map_err(|_| invalid("Microsoft OAuth endpoint is invalid"))?,
        })
    }

    /// Overrides the token endpoint for a loopback test server or an HTTPS-compatible proxy.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for unsafe endpoint URLs.
    pub fn with_token_endpoint(mut self, value: impl AsRef<str>) -> Result<Self, BackendError> {
        self.token_endpoint = parse_provider_endpoint(value.as_ref())?;
        Ok(self)
    }
}

#[derive(Deserialize)]
struct MicrosoftTokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct MicrosoftAuthorizationCodeTokenResponse {
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MicrosoftGraphUser {
    id: String,
    display_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MicrosoftGraphDrive {
    id: String,
    drive_type: String,
}

#[derive(Deserialize)]
struct MicrosoftGraphRoot {
    id: String,
}

#[async_trait]
impl MicrosoftOAuthRefreshClient for MicrosoftOAuthClient {
    async fn refresh(
        &self,
        request: &MicrosoftOAuthRefreshRequest<'_>,
    ) -> Result<MicrosoftAccessToken, BackendError> {
        let mut form = vec![
            ("client_id", request.client_id()),
            ("grant_type", "refresh_token"),
            ("refresh_token", request.refresh_token()),
            ("scope", "offline_access User.Read Files.Read"),
        ];
        if let Some(secret) = request.client_secret() {
            form.push(("client_secret", secret));
        }
        let response = self
            .client
            .post(self.token_endpoint.clone())
            .form(&form)
            .send()
            .await
            .map_err(|_| temporary("Microsoft OAuth refresh request failed"))?;
        if !response.status().is_success() {
            return Err(temporary(
                "Microsoft OAuth refresh was rejected; reauthorization may be required",
            ));
        }
        let response: MicrosoftTokenResponse = response
            .json()
            .await
            .map_err(|_| invalid("Microsoft OAuth returned malformed token JSON"))?;
        MicrosoftAccessToken::new(
            response.access_token,
            Duration::from_secs(response.expires_in),
            response.refresh_token,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OneDriveScope {
    Personal,
    Business,
    SharePoint,
}

#[async_trait]
pub trait OneDriveTransport: Send + Sync {
    async fn get_json(
        &self,
        api_base: &Url,
        target: &str,
        access_token: &str,
    ) -> Result<serde_json::Value, BackendError>;

    async fn get_range(
        &self,
        api_base: &Url,
        target: &str,
        access_token: &str,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError>;
}

struct ReqwestTransport {
    client: Client,
}

pub struct OneDriveBackend {
    transport: Arc<dyn OneDriveTransport>,
    token_provider: Arc<dyn MicrosoftAccessTokenProvider>,
    drive_id: String,
    api_base: Url,
}

impl OneDriveBackend {
    /// Creates a `OneDrive` Personal backend. Business and `SharePoint` are excluded from v1.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for unsupported scopes, invalid drive IDs, or client setup.
    pub fn new(
        token_provider: impl MicrosoftAccessTokenProvider + 'static,
        scope: OneDriveScope,
        drive_id: impl Into<String>,
    ) -> Result<Self, BackendError> {
        if scope != OneDriveScope::Personal {
            return Err(invalid(
                "OneDrive Business and SharePoint are not supported in v1",
            ));
        }
        let drive_id = drive_id.into();
        validate_graph_id(&drive_id)?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .build()
            .map_err(|_| invalid("OneDrive HTTP client configuration is invalid"))?;
        Ok(Self {
            transport: Arc::new(ReqwestTransport { client }),
            token_provider: Arc::new(token_provider),
            drive_id,
            api_base: Url::parse(DEFAULT_API_BASE)
                .map_err(|_| invalid("Microsoft Graph API base URL is invalid"))?,
        })
    }

    /// Overrides the Graph endpoint for an HTTPS-compatible proxy or loopback test server.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for non-HTTPS, non-loopback endpoints.
    pub fn with_api_base(mut self, value: impl AsRef<str>) -> Result<Self, BackendError> {
        let mut url = Url::parse(value.as_ref())
            .map_err(|_| invalid("Microsoft Graph API base URL is invalid"))?;
        if !secure_or_loopback(&url) {
            return Err(invalid(
                "Microsoft Graph API base must use HTTPS or loopback HTTP",
            ));
        }
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        self.api_base = url;
        Ok(self)
    }

    #[must_use]
    pub fn with_transport(mut self, transport: Arc<dyn OneDriveTransport>) -> Self {
        self.transport = transport;
        self
    }

    /// Requests the latest Delta cursor without enumerating the existing hierarchy.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for authentication, transport, or malformed links.
    pub async fn latest_delta_cursor(&self) -> Result<ChangeCursor, BackendError> {
        let token = self.token().await?;
        let target = format!("drives/{}/root/delta?token=latest", self.drive_id);
        let value = self
            .transport
            .get_json(&self.api_base, &target, &token)
            .await?;
        let response: DeltaPage = serde_json::from_value(value)
            .map_err(|_| invalid("OneDrive returned malformed Delta JSON"))?;
        let link = response
            .delta_link
            .ok_or_else(|| invalid("OneDrive latest Delta response omitted deltaLink"))?;
        validate_graph_continuation(&link)?;
        ChangeCursor::new(link)
    }

    async fn token(&self) -> Result<String, BackendError> {
        let token = self.token_provider.access_token().await?;
        if token.trim().is_empty() || token.chars().any(char::is_control) {
            return Err(invalid(
                "Microsoft OAuth provider returned an invalid access token",
            ));
        }
        Ok(token)
    }
}

#[async_trait]
impl StorageBackend for OneDriveBackend {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        let item_id = validate_object_id(id)?;
        let token = self.token().await?;
        let target = item_target(&self.drive_id, item_id, None);
        let value = self
            .transport
            .get_json(&self.api_base, &target, &token)
            .await?;
        let item: DriveItem = serde_json::from_value(value)
            .map_err(|_| invalid("OneDrive returned malformed driveItem JSON"))?;
        item.into_storage_object(&self.drive_id)?
            .ok_or(BackendError::NotFound)
    }

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        let parent_id = validate_object_id(parent)?;
        let target = if let Some(page) = page {
            validate_graph_continuation(page.as_str())?;
            page.as_str().to_owned()
        } else {
            item_target(&self.drive_id, parent_id, Some("children"))
        };
        let token = self.token().await?;
        let value = self
            .transport
            .get_json(&self.api_base, &target, &token)
            .await?;
        let response: ItemPage = serde_json::from_value(value)
            .map_err(|_| invalid("OneDrive returned malformed children JSON"))?;
        let objects = response
            .value
            .into_iter()
            .filter_map(|item| match item.into_storage_object(&self.drive_id) {
                Ok(Some(object)) => Some(Ok(object)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next_page = response
            .next_link
            .map(|link| {
                validate_graph_continuation(&link)?;
                PageToken::new(link)
            })
            .transpose()?;
        Ok(ObjectPage { objects, next_page })
    }

    async fn list_changes(&self, cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        validate_graph_continuation(cursor.as_str())?;
        let token = self.token().await?;
        let value = self
            .transport
            .get_json(&self.api_base, cursor.as_str(), &token)
            .await?;
        let response: DeltaPage = serde_json::from_value(value)
            .map_err(|_| invalid("OneDrive returned malformed Delta JSON"))?;
        let changes = retain_last_delta_occurrences(response.value)
            .into_iter()
            .map(|item| item.into_storage_change(&self.drive_id))
            .collect::<Result<Vec<_>, _>>()?;
        match (response.next_link, response.delta_link) {
            (Some(link), _) => {
                validate_graph_continuation(&link)?;
                Ok(ChangePage::continuation(changes, ChangeCursor::new(link)?))
            }
            (None, Some(link)) => {
                validate_graph_continuation(&link)?;
                Ok(ChangePage::new(changes, ChangeCursor::new(link)?))
            }
            (None, None) => Err(invalid("OneDrive Delta response omitted continuation link")),
        }
    }

    async fn latest_change_cursor(&self) -> Result<ChangeCursor, BackendError> {
        self.latest_delta_cursor().await
    }

    async fn open_range(
        &self,
        id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        let item_id = validate_object_id(id)?;
        let token = self.token().await?;
        let target = format!("drives/{}/items/{item_id}/content", self.drive_id);
        self.transport
            .get_range(&self.api_base, &target, &token, range)
            .await
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new()
            .with_changes(true)
            .with_range_reads(true)
    }
}

fn item_target(drive_id: &str, item_id: &str, child: Option<&str>) -> String {
    const SELECT: &str =
        "id,name,size,file,folder,deleted,parentReference,lastModifiedDateTime,eTag,cTag";
    child.map_or_else(
        || format!("drives/{drive_id}/items/{item_id}?$select={SELECT}"),
        |value| format!("drives/{drive_id}/items/{item_id}/{value}?$top=200&$select={SELECT}"),
    )
}

#[derive(Deserialize)]
struct ItemPage {
    value: Vec<DriveItem>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
}

#[derive(Deserialize)]
struct DeltaPage {
    #[serde(default)]
    value: Vec<DriveItem>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink")]
    delta_link: Option<String>,
}

fn retain_last_delta_occurrences(items: Vec<DriveItem>) -> Vec<DriveItem> {
    let mut latest = HashMap::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        latest.insert(item.id.clone(), (index, item));
    }
    let mut retained = latest.into_values().collect::<Vec<_>>();
    retained.sort_unstable_by_key(|(index, _)| *index);
    retained.into_iter().map(|(_, item)| item).collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveItem {
    id: String,
    #[serde(default)]
    name: String,
    size: Option<u64>,
    file: Option<FileFacet>,
    folder: Option<serde_json::Value>,
    deleted: Option<serde_json::Value>,
    parent_reference: Option<ParentReference>,
    last_modified_date_time: Option<DateTime<Utc>>,
    e_tag: Option<String>,
    c_tag: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileFacet {
    mime_type: Option<String>,
    hashes: Option<Hashes>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hashes {
    quick_xor_hash: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParentReference {
    id: Option<String>,
    drive_id: Option<String>,
}

impl DriveItem {
    fn into_storage_change(
        self,
        drive_id: &str,
    ) -> Result<tjxy_storage::StorageChange, BackendError> {
        if self.deleted.is_some() {
            return Ok(tjxy_storage::StorageChange::Removed(StorageObjectId::new(
                PROVIDER, self.id,
            )?));
        }
        self.into_storage_object(drive_id)?
            .map(tjxy_storage::StorageChange::Upsert)
            .ok_or_else(|| invalid("OneDrive Delta item omitted file or folder metadata"))
    }

    fn into_storage_object(self, drive_id: &str) -> Result<Option<StorageObject>, BackendError> {
        if self.deleted.is_some() {
            return Ok(None);
        }
        validate_graph_id(&self.id)?;
        if self.name.is_empty() {
            return Err(invalid("OneDrive driveItem omitted its name"));
        }
        let id = StorageObjectId::new(PROVIDER, self.id)?;
        let mut object = if self.folder.is_some() {
            StorageObject::directory(id, self.name)
        } else if let Some(file) = self.file {
            let mut object = StorageObject::file(
                id,
                self.name,
                self.size
                    .ok_or_else(|| invalid("OneDrive file omitted its size"))?,
            );
            if let Some(mime_type) = file.mime_type {
                object = object.with_mime_type(mime_type)?;
            }
            if let Some(checksum) = file.hashes.and_then(|hashes| hashes.quick_xor_hash) {
                object = object.with_checksum(checksum)?;
            }
            object
        } else {
            return Ok(None);
        };
        if let Some(parent) = self.parent_reference {
            if parent
                .drive_id
                .as_deref()
                .is_some_and(|value| value != drive_id)
            {
                return Err(invalid("OneDrive driveItem crossed configured drive scope"));
            }
            if let Some(parent_id) = parent.id {
                object = object.with_parents(vec![StorageObjectId::new(PROVIDER, parent_id)?])?;
            }
        }
        if let Some(etag) = self.e_tag.or(self.c_tag) {
            object = object.with_etag(etag.clone())?.with_remote_revision(etag)?;
        }
        if let Some(modified) = self.last_modified_date_time {
            object = object.with_remote_modified_at(modified);
        }
        Ok(Some(object))
    }
}

#[async_trait]
impl OneDriveTransport for ReqwestTransport {
    async fn get_json(
        &self,
        api_base: &Url,
        target: &str,
        access_token: &str,
    ) -> Result<serde_json::Value, BackendError> {
        let response = self
            .client
            .get(endpoint(api_base, target)?)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|error| http_error(&error))?;
        checked(response)?
            .json()
            .await
            .map_err(|_| invalid("OneDrive returned malformed JSON"))
    }

    async fn get_range(
        &self,
        api_base: &Url,
        target: &str,
        access_token: &str,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        let graph_url = endpoint(api_base, target)?;
        let mut response = send_range(&self.client, graph_url, Some(access_token), range).await?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| invalid("OneDrive content redirect omitted Location"))?;
            let download = Url::parse(location)
                .map_err(|_| invalid("OneDrive content redirect URL is invalid"))?;
            if !secure_or_loopback(&download) {
                return Err(invalid("OneDrive content redirect must use HTTPS"));
            }
            response = send_range(&self.client, download, None, range).await?;
        }
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
            let size = unsatisfied_size(response.headers())
                .ok_or_else(|| invalid("OneDrive 416 response omitted object size"))?;
            return Err(BackendError::RangeNotSatisfiable { size });
        }
        if response.status() != StatusCode::PARTIAL_CONTENT {
            return match checked(response) {
                Ok(_) => Err(temporary("OneDrive ignored the bounded Range request")),
                Err(error) => Err(error),
            };
        }
        validate_content_range(response.headers(), range)?;
        let mut upstream = response.bytes_stream();
        let expected = range.end_exclusive() - range.start();
        Ok(Box::pin(async_stream::stream! {
            use futures_util::StreamExt;
            let mut remaining = expected;
            while let Some(chunk) = upstream.next().await {
                if let Ok(bytes) = chunk {
                    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                    if length > remaining {
                        yield Err(invalid("OneDrive Range body exceeded Content-Range"));
                        return;
                    }
                    remaining -= length;
                    yield Ok(bytes);
                } else {
                    yield Err(temporary("OneDrive Range stream failed"));
                    return;
                }
            }
            if remaining != 0 {
                yield Err(temporary("OneDrive Range stream ended early"));
            }
        }))
    }
}

async fn send_range(
    client: &Client,
    url: Url,
    access_token: Option<&str>,
    range: ByteRange,
) -> Result<reqwest::Response, BackendError> {
    let end = range
        .end_exclusive()
        .checked_sub(1)
        .ok_or_else(|| invalid("OneDrive byte range is invalid"))?;
    let mut request = client.get(url).header(
        reqwest::header::RANGE,
        format!("bytes={}-{}", range.start(), end),
    );
    if let Some(token) = access_token {
        request = request.bearer_auth(token);
    }
    request.send().await.map_err(|error| http_error(&error))
}

fn endpoint(api_base: &Url, target: &str) -> Result<Url, BackendError> {
    if target.starts_with("https://") {
        validate_graph_continuation(target)?;
        return Url::parse(target).map_err(|_| invalid("Microsoft Graph URL is invalid"));
    }
    if target.is_empty() || target.starts_with('/') || target.contains("..") {
        return Err(invalid("Microsoft Graph relative target is invalid"));
    }
    api_base
        .join(target)
        .map_err(|_| invalid("Microsoft Graph endpoint could not be constructed"))
}

fn validate_graph_continuation(value: &str) -> Result<(), BackendError> {
    let url = Url::parse(value).map_err(|_| invalid("Microsoft Graph continuation is invalid"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("graph.microsoft.com")
        || url.port_or_known_default() != Some(443)
        || !url.path().starts_with("/v1.0/")
        || url.username() != ""
        || url.password().is_some()
    {
        return Err(invalid("Microsoft Graph continuation has an unsafe origin"));
    }
    Ok(())
}

fn parse_redirect_uri(value: &str) -> Result<Url, BackendError> {
    let url = Url::parse(value).map_err(|_| invalid("Microsoft OAuth redirect URI is invalid"))?;
    if url.query().is_some()
        || url.fragment().is_some()
        || url.username() != ""
        || url.password().is_some()
        || !secure_or_loopback(&url)
    {
        return Err(invalid(
            "Microsoft OAuth redirect URI must use HTTPS or loopback HTTP",
        ));
    }
    Ok(url)
}

fn parse_provider_endpoint(value: &str) -> Result<Url, BackendError> {
    let url = Url::parse(value).map_err(|_| invalid("Microsoft OAuth endpoint is invalid"))?;
    if url.query().is_some()
        || url.fragment().is_some()
        || url.username() != ""
        || url.password().is_some()
        || !secure_or_loopback(&url)
    {
        return Err(invalid(
            "Microsoft OAuth endpoint must use HTTPS or loopback HTTP",
        ));
    }
    Ok(url)
}

fn parse_graph_api_base(value: &str) -> Result<Url, BackendError> {
    let mut url = parse_provider_endpoint(value)?;
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}

fn secure_or_loopback(url: &Url) -> bool {
    url.scheme() == "https"
        || (url.scheme() == "http"
            && url
                .host_str()
                .and_then(|host| host.parse::<std::net::IpAddr>().ok())
                .is_some_and(|address| address.is_loopback()))
}

fn checked(response: reqwest::Response) -> Result<reqwest::Response, BackendError> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(classify_http_failure(response.status(), response.headers()))
    }
}

fn classify_http_failure(status: StatusCode, headers: &reqwest::header::HeaderMap) -> BackendError {
    match status {
        StatusCode::GONE => BackendError::ChangeCursorInvalid,
        StatusCode::NOT_FOUND => BackendError::NotFound,
        StatusCode::TOO_MANY_REQUESTS => BackendError::RateLimited {
            retry_after: parse_retry_after(headers),
        },
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            temporary("Microsoft Graph rejected the request")
        }
        status if status.is_server_error() => temporary("Microsoft Graph is unavailable"),
        _ => invalid("Microsoft Graph rejected an invalid request"),
    }
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    httpdate::parse_http_date(value)
        .ok()?
        .duration_since(std::time::SystemTime::now())
        .ok()
}

fn validate_content_range(
    headers: &reqwest::header::HeaderMap,
    expected: ByteRange,
) -> Result<(), BackendError> {
    let value = headers
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("bytes "))
        .ok_or_else(|| invalid("OneDrive 206 response omitted Content-Range"))?;
    let (interval, _) = value
        .split_once('/')
        .ok_or_else(|| invalid("OneDrive Content-Range is malformed"))?;
    let (start, end) = interval
        .split_once('-')
        .ok_or_else(|| invalid("OneDrive Content-Range is malformed"))?;
    let start = start
        .parse::<u64>()
        .map_err(|_| invalid("OneDrive Content-Range is malformed"))?;
    let end = end
        .parse::<u64>()
        .map_err(|_| invalid("OneDrive Content-Range is malformed"))?;
    if start != expected.start() || end.checked_add(1) != Some(expected.end_exclusive()) {
        return Err(invalid("OneDrive returned the wrong byte interval"));
    }
    Ok(())
}

fn unsatisfied_size(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?
        .strip_prefix("bytes */")?
        .parse()
        .ok()
}

fn validate_object_id(id: &StorageObjectId) -> Result<&str, BackendError> {
    if id.provider() != PROVIDER {
        return Err(BackendError::NotFound);
    }
    validate_graph_id(id.provider_object_id())?;
    Ok(id.provider_object_id())
}

fn validate_graph_id(value: &str) -> Result<(), BackendError> {
    if value.is_empty()
        || value.len() > 2048
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'!'))
    {
        return Err(invalid("OneDrive stable ID is invalid"));
    }
    Ok(())
}

fn http_error(error: &reqwest::Error) -> BackendError {
    let category = if error.is_timeout() {
        "timed out"
    } else if error.is_connect() {
        "could not connect"
    } else if error.is_request() {
        "request could not be sent"
    } else if error.is_body() {
        "response body failed"
    } else {
        "request failed"
    };
    temporary(&format!("OneDrive {category}"))
}

fn temporary(message: &str) -> BackendError {
    BackendError::TemporarilyUnavailable {
        message: message.to_owned(),
    }
}

fn invalid(message: &str) -> BackendError {
    BackendError::InvalidValue {
        message: message.to_owned(),
    }
}

fn validate_secret(name: &str, value: String) -> Result<String, BackendError> {
    if value.trim().is_empty() || value.len() > 16 * 1024 || value.chars().any(char::is_control) {
        return Err(invalid(&format!("{name} is invalid")));
    }
    Ok(value)
}

impl fmt::Debug for OneDriveBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OneDriveBackend")
            .field("drive_id", &self.drive_id)
            .field("api_base", &self.api_base)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use reqwest::{StatusCode, Url, header::HeaderMap};
    use tjxy_storage::BackendError;

    use super::{MicrosoftOAuthWebClient, classify_http_failure};

    #[test]
    fn gone_response_marks_the_delta_cursor_invalid() {
        assert_eq!(
            classify_http_failure(StatusCode::GONE, &HeaderMap::new()),
            BackendError::ChangeCursorInvalid
        );
    }

    #[test]
    fn web_oauth_authorization_url_uses_pkce_without_client_secret() {
        let client = MicrosoftOAuthWebClient::new(
            "client-id",
            Some("client-secret-never-exposed".to_owned()),
            "http://127.0.0.1:8096/Admin/Storage/OAuth/OneDrive/Callback",
        )
        .unwrap();
        let parsed = Url::parse(
            &client
                .authorization_url("state-value", "challenge-value")
                .unwrap(),
        )
        .unwrap();
        let query: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(query["state"], "state-value");
        assert_eq!(query["code_challenge"], "challenge-value");
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["scope"], "offline_access User.Read Files.Read");
        assert!(!query.contains_key("client_secret"));
    }
}
