//! Google Drive v3 implementation of the provider-neutral storage contract.

use std::{fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use tjxy_storage::{
    BackendError, ByteRange, ByteStream, ChangeCursor, ChangePage, ObjectPage, PageToken,
    StorageBackend, StorageCapabilities, StorageObject, StorageObjectId,
};
use zeroize::Zeroizing;

const PROVIDER: &str = "google-drive";
const DEFAULT_API_BASE: &str = "https://www.googleapis.com/drive/v3";
const DEFAULT_AUTHORIZATION_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const DEFAULT_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const FOLDER_MIME_TYPE: &str = "application/vnd.google-apps.folder";
const MAX_CREDENTIAL_PAYLOAD_BYTES: usize = 128 * 1024;

#[async_trait]
pub trait AccessTokenProvider: Send + Sync {
    /// Returns a current OAuth access token, refreshing it when required.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] when credentials cannot produce a usable token.
    async fn access_token(&self) -> Result<String, BackendError>;
}

#[derive(Clone)]
pub struct GoogleOAuthCredentials {
    client_id: Zeroizing<String>,
    client_secret: Zeroizing<String>,
    refresh_token: Zeroizing<String>,
}

/// Server-side OAuth configuration for one Google Drive web authorization flow.
///
/// The client secret stays in this value and is never included in an authorization URL or DTO.
pub struct GoogleOAuthWebClient {
    client: Client,
    client_id: Zeroizing<String>,
    client_secret: Zeroizing<String>,
    redirect_uri: Url,
    authorization_endpoint: Url,
    token_endpoint: Url,
}

impl GoogleOAuthWebClient {
    /// Creates a web OAuth client using Google's production authorization and token endpoints.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for unsafe credentials, an invalid redirect URI, or
    /// an invalid HTTP client configuration.
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: impl AsRef<str>,
    ) -> Result<Self, BackendError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| invalid("Google OAuth HTTP client configuration is invalid"))?;
        Self::with_client(client, client_id, client_secret, redirect_uri)
    }

    fn with_client(
        client: Client,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: impl AsRef<str>,
    ) -> Result<Self, BackendError> {
        let redirect_uri = parse_redirect_uri(redirect_uri.as_ref())?;
        Ok(Self {
            client,
            client_id: Zeroizing::new(validate_secret("OAuth client ID", client_id.into())?),
            client_secret: Zeroizing::new(validate_secret(
                "OAuth client secret",
                client_secret.into(),
            )?),
            redirect_uri,
            authorization_endpoint: Url::parse(DEFAULT_AUTHORIZATION_ENDPOINT)
                .map_err(|_| invalid("Google authorization endpoint is invalid"))?,
            token_endpoint: Url::parse(DEFAULT_TOKEN_ENDPOINT)
                .map_err(|_| invalid("Google OAuth endpoint is invalid"))?,
        })
    }

    /// Overrides both Google OAuth endpoints for a loopback test server or an HTTPS proxy.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for unsafe endpoint URLs.
    pub fn with_endpoints(
        mut self,
        authorization_endpoint: impl AsRef<str>,
        token_endpoint: impl AsRef<str>,
    ) -> Result<Self, BackendError> {
        self.authorization_endpoint = parse_provider_endpoint(authorization_endpoint.as_ref())?;
        self.token_endpoint = parse_provider_endpoint(token_endpoint.as_ref())?;
        Ok(self)
    }

    /// Returns a fully encoded authorization URL for a one-time `state` and PKCE challenge.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for unsafe state or PKCE input.
    pub fn authorization_url(
        &self,
        state: &str,
        code_challenge: &str,
    ) -> Result<String, BackendError> {
        validate_secret("OAuth state", state.to_owned())?;
        validate_secret("OAuth PKCE challenge", code_challenge.to_owned())?;
        let mut url = self.authorization_endpoint.clone();
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", self.redirect_uri.as_str())
            .append_pair("scope", "https://www.googleapis.com/auth/drive.readonly")
            .append_pair("access_type", "offline")
            .append_pair("include_granted_scopes", "true")
            .append_pair("state", state)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }

    /// Exchanges one authorization code with its PKCE verifier and returns persistable credentials.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] if Google rejects the code, omits a refresh token, or returns an
    /// invalid response.
    pub async fn exchange_authorization_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<GoogleOAuthCredentials, BackendError> {
        validate_secret("OAuth authorization code", code.to_owned())?;
        validate_secret("OAuth PKCE verifier", code_verifier.to_owned())?;
        let response = self
            .client
            .post(self.token_endpoint.clone())
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("code_verifier", code_verifier),
            ])
            .send()
            .await
            .map_err(|_| temporary("Google OAuth authorization-code exchange failed"))?;
        if !response.status().is_success() {
            return Err(temporary(
                "Google OAuth authorization code was rejected or expired",
            ));
        }
        let token: AuthorizationCodeTokenResponse = response
            .json()
            .await
            .map_err(|_| invalid("Google OAuth returned malformed token JSON"))?;
        let refresh_token = token
            .refresh_token
            .ok_or_else(|| invalid("Google OAuth did not return a refresh token"))?;
        GoogleOAuthCredentials::new(
            self.client_id.to_string(),
            self.client_secret.to_string(),
            refresh_token,
        )
    }

    /// Returns the token endpoint used by refresh clients created for this authorization flow.
    #[must_use]
    pub fn token_endpoint(&self) -> &Url {
        &self.token_endpoint
    }
}

impl fmt::Debug for GoogleOAuthWebClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleOAuthWebClient")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("redirect_uri", &self.redirect_uri)
            .field("authorization_endpoint", &self.authorization_endpoint)
            .field("token_endpoint", &self.token_endpoint)
            .finish_non_exhaustive()
    }
}

impl GoogleOAuthCredentials {
    /// Creates an in-memory credential value. Persist it only through a protected credential store.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for empty, unbounded, or control-containing fields.
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        refresh_token: impl Into<String>,
    ) -> Result<Self, BackendError> {
        Ok(Self {
            client_id: Zeroizing::new(validate_secret("OAuth client ID", client_id.into())?),
            client_secret: Zeroizing::new(validate_secret(
                "OAuth client secret",
                client_secret.into(),
            )?),
            refresh_token: Zeroizing::new(validate_secret(
                "OAuth refresh token",
                refresh_token.into(),
            )?),
        })
    }

    /// Encodes the versioned plaintext consumed by the encrypted credential store.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] if serialization unexpectedly exceeds its bound.
    pub fn to_payload_json(&self) -> Result<Zeroizing<Vec<u8>>, BackendError> {
        let encoded = serde_json::to_vec(&GoogleCredentialPayloadRef {
            version: 1,
            client_id: self.client_id.as_str(),
            client_secret: self.client_secret.as_str(),
            refresh_token: self.refresh_token.as_str(),
        })
        .map_err(|_| invalid("Google OAuth credential payload could not be encoded"))?;
        if encoded.len() > MAX_CREDENTIAL_PAYLOAD_BYTES {
            return Err(invalid("Google OAuth credential payload is too large"));
        }
        Ok(Zeroizing::new(encoded))
    }

    /// Decodes a versioned plaintext immediately after authenticated decryption.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for malformed, unsupported, or unbounded payloads.
    pub fn from_payload_json(payload: &[u8]) -> Result<Self, BackendError> {
        if payload.is_empty() || payload.len() > MAX_CREDENTIAL_PAYLOAD_BYTES {
            return Err(invalid("Google OAuth credential payload is invalid"));
        }
        let payload: GoogleCredentialPayload = serde_json::from_slice(payload)
            .map_err(|_| invalid("Google OAuth credential payload is malformed"))?;
        if payload.version != 1 {
            return Err(invalid(
                "Google OAuth credential payload version is unsupported",
            ));
        }
        Self::new(
            payload.client_id,
            payload.client_secret,
            payload.refresh_token,
        )
    }
}

#[derive(Serialize)]
struct GoogleCredentialPayloadRef<'value> {
    version: u8,
    client_id: &'value str,
    client_secret: &'value str,
    refresh_token: &'value str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GoogleCredentialPayload {
    version: u8,
    client_id: String,
    client_secret: String,
    refresh_token: String,
}

impl fmt::Debug for GoogleOAuthCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleOAuthCredentials")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .finish()
    }
}

pub struct OAuthRefreshRequest<'a> {
    credentials: &'a GoogleOAuthCredentials,
}

impl OAuthRefreshRequest<'_> {
    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.credentials.client_id
    }
    #[must_use]
    pub fn client_secret(&self) -> &str {
        &self.credentials.client_secret
    }
    #[must_use]
    pub fn refresh_token(&self) -> &str {
        &self.credentials.refresh_token
    }
}

pub struct OAuthAccessToken {
    token: Zeroizing<String>,
    expires_in: Duration,
}

impl OAuthAccessToken {
    /// Creates a validated short-lived OAuth access token.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for an unsafe token or zero lifetime.
    pub fn new(token: impl Into<String>, expires_in: Duration) -> Result<Self, BackendError> {
        if expires_in.is_zero() {
            return Err(invalid("OAuth access token lifetime must be positive"));
        }
        Ok(Self {
            token: Zeroizing::new(validate_secret("OAuth access token", token.into())?),
            expires_in,
        })
    }
}

#[async_trait]
pub trait OAuthRefreshClient: Send + Sync {
    async fn refresh(
        &self,
        request: &OAuthRefreshRequest<'_>,
    ) -> Result<OAuthAccessToken, BackendError>;
}

pub struct GoogleOAuthClient {
    client: Client,
    token_endpoint: Url,
}

impl GoogleOAuthClient {
    /// Creates the production Google OAuth refresh client.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] if the bounded HTTP client cannot be built.
    pub fn new() -> Result<Self, BackendError> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .read_timeout(Duration::from_secs(20))
                .build()
                .map_err(|_| invalid("Google OAuth HTTP client configuration is invalid"))?,
            token_endpoint: Url::parse("https://oauth2.googleapis.com/token")
                .map_err(|_| invalid("Google OAuth endpoint is invalid"))?,
        })
    }

    /// Overrides the OAuth token endpoint for a loopback test server or an HTTPS proxy.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for unsafe endpoint URLs.
    pub fn with_token_endpoint(mut self, value: impl AsRef<str>) -> Result<Self, BackendError> {
        self.token_endpoint = parse_provider_endpoint(value.as_ref())?;
        Ok(self)
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct AuthorizationCodeTokenResponse {
    refresh_token: Option<String>,
}

#[async_trait]
impl OAuthRefreshClient for GoogleOAuthClient {
    async fn refresh(
        &self,
        request: &OAuthRefreshRequest<'_>,
    ) -> Result<OAuthAccessToken, BackendError> {
        let response = self
            .client
            .post(self.token_endpoint.clone())
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", request.client_id()),
                ("client_secret", request.client_secret()),
                ("refresh_token", request.refresh_token()),
            ])
            .send()
            .await
            .map_err(|_| temporary("Google OAuth refresh request failed"))?;
        if !response.status().is_success() {
            return Err(temporary(
                "Google OAuth refresh was rejected; reauthorization may be required",
            ));
        }
        let token: TokenResponse = response
            .json()
            .await
            .map_err(|_| invalid("Google OAuth returned malformed token JSON"))?;
        OAuthAccessToken::new(token.access_token, Duration::from_secs(token.expires_in))
    }
}

struct CachedAccessToken {
    token: Zeroizing<String>,
    refresh_after: tokio::time::Instant,
}

pub struct RefreshingAccessTokenProvider {
    credentials: GoogleOAuthCredentials,
    client: Arc<dyn OAuthRefreshClient>,
    cached: tokio::sync::Mutex<Option<CachedAccessToken>>,
}

impl RefreshingAccessTokenProvider {
    #[must_use]
    pub fn new(
        credentials: GoogleOAuthCredentials,
        client: Arc<impl OAuthRefreshClient + 'static>,
    ) -> Self {
        Self {
            credentials,
            client,
            cached: tokio::sync::Mutex::new(None),
        }
    }
}

#[async_trait]
impl AccessTokenProvider for RefreshingAccessTokenProvider {
    async fn access_token(&self) -> Result<String, BackendError> {
        let mut cached = self.cached.lock().await;
        let now = tokio::time::Instant::now();
        if let Some(value) = cached.as_ref().filter(|value| value.refresh_after > now) {
            return Ok(value.token.to_string());
        }
        let refreshed = self
            .client
            .refresh(&OAuthRefreshRequest {
                credentials: &self.credentials,
            })
            .await?;
        let skew = Duration::from_secs(60).min(refreshed.expires_in / 2);
        let usable_for = refreshed.expires_in.saturating_sub(skew);
        let refresh_after = now
            .checked_add(usable_for)
            .ok_or_else(|| invalid("OAuth access token expiry is outside the supported range"))?;
        let token = refreshed.token;
        let exposed = token.to_string();
        *cached = Some(CachedAccessToken {
            token,
            refresh_after,
        });
        Ok(exposed)
    }
}

#[async_trait]
pub trait GoogleDriveTransport: Send + Sync {
    async fn get_json(
        &self,
        api_base: &Url,
        path: &str,
        query: &[(String, String)],
        access_token: &str,
    ) -> Result<serde_json::Value, BackendError>;

    async fn get_range(
        &self,
        api_base: &Url,
        path: &str,
        query: &[(String, String)],
        access_token: &str,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError>;
}

struct ReqwestTransport {
    client: Client,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GoogleDriveScope {
    MyDrive,
    SharedDrive(String),
}

pub struct GoogleDriveBackend {
    transport: Arc<dyn GoogleDriveTransport>,
    token_provider: Arc<dyn AccessTokenProvider>,
    scope: GoogleDriveScope,
    api_base: Url,
}

/// One Shared Drive visible to the authenticated account.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoogleSharedDrive {
    id: String,
    name: String,
}

impl GoogleSharedDrive {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// One page of Shared Drives from the authenticated account.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoogleSharedDrivePage {
    drives: Vec<GoogleSharedDrive>,
    next_page: Option<PageToken>,
}

impl GoogleSharedDrivePage {
    #[must_use]
    pub fn drives(&self) -> &[GoogleSharedDrive] {
        &self.drives
    }

    #[must_use]
    pub fn next_page(&self) -> Option<&PageToken> {
        self.next_page.as_ref()
    }
}

impl GoogleDriveBackend {
    /// Creates a Google Drive backend with bounded connect and read timeouts.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for an unsafe Shared Drive ID or client setup.
    pub fn new(
        token_provider: impl AccessTokenProvider + 'static,
        scope: GoogleDriveScope,
    ) -> Result<Self, BackendError> {
        if let GoogleDriveScope::SharedDrive(id) = &scope {
            validate_drive_id(id)?;
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| invalid("Google Drive HTTP client configuration is invalid"))?;
        Ok(Self {
            transport: Arc::new(ReqwestTransport { client }),
            token_provider: Arc::new(token_provider),
            scope,
            api_base: Url::parse(DEFAULT_API_BASE)
                .map_err(|_| invalid("Google Drive API base URL is invalid"))?,
        })
    }

    /// Overrides the API endpoint for a loopback test server or an HTTPS-compatible proxy.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError::InvalidValue`] for non-HTTPS non-loopback endpoints.
    pub fn with_api_base(mut self, value: impl AsRef<str>) -> Result<Self, BackendError> {
        let url = Url::parse(value.as_ref()).map_err(|_| invalid("API base URL is invalid"))?;
        let secure = url.scheme() == "https";
        let loopback = url
            .host_str()
            .and_then(|host| host.parse::<std::net::IpAddr>().ok())
            .is_some_and(|address| address.is_loopback());
        if !secure && !loopback {
            return Err(invalid("API base URL must use HTTPS or loopback HTTP"));
        }
        self.api_base = url;
        Ok(self)
    }

    #[must_use]
    pub fn with_transport(mut self, transport: Arc<dyn GoogleDriveTransport>) -> Self {
        self.transport = transport;
        self
    }

    /// Requests the opaque cursor used to begin polling Drive changes.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for authentication, transport, or malformed responses.
    pub async fn start_page_token(&self) -> Result<ChangeCursor, BackendError> {
        let token = self.token().await?;
        let mut query = vec![("supportsAllDrives".to_owned(), "true".to_owned())];
        if let GoogleDriveScope::SharedDrive(drive_id) = &self.scope {
            query.push(("driveId".to_owned(), drive_id.clone()));
        }
        let value = self
            .transport
            .get_json(&self.api_base, "changes/startPageToken", &query, &token)
            .await?;
        let response: StartPageToken = serde_json::from_value(value)
            .map_err(|_| invalid("Google Drive returned malformed start-page-token JSON"))?;
        ChangeCursor::new(response.start_page_token)
    }

    /// Returns the authenticated Google account's stable display identity.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for authentication, transport, or malformed account metadata.
    pub async fn account_identity(&self) -> Result<String, BackendError> {
        let token = self.token().await?;
        let value = self
            .transport
            .get_json(
                &self.api_base,
                "about",
                &[(
                    "fields".to_owned(),
                    "user(emailAddress,displayName)".to_owned(),
                )],
                &token,
            )
            .await?;
        let response: AboutResponse = serde_json::from_value(value)
            .map_err(|_| invalid("Google Drive returned malformed account metadata JSON"))?;
        let identity = response
            .user
            .and_then(|user| user.email_address.or(user.display_name))
            .ok_or_else(|| invalid("Google Drive account metadata omitted its identity"))?;
        validate_secret("Google account identity", identity)
    }

    /// Lists Shared Drives visible to the authenticated account.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] for authentication, transport, malformed pagination, or invalid
    /// Shared Drive identity data.
    pub async fn list_shared_drives(
        &self,
        page: Option<PageToken>,
    ) -> Result<GoogleSharedDrivePage, BackendError> {
        let token = self.token().await?;
        let mut query = vec![
            ("pageSize".to_owned(), "100".to_owned()),
            (
                "fields".to_owned(),
                "nextPageToken,drives(id,name)".to_owned(),
            ),
        ];
        if let Some(page) = page {
            query.push(("pageToken".to_owned(), page.as_str().to_owned()));
        }
        let value = self
            .transport
            .get_json(&self.api_base, "drives", &query, &token)
            .await?;
        let page: SharedDriveList = serde_json::from_value(value)
            .map_err(|_| invalid("Google Drive returned malformed Shared Drive JSON"))?;
        let drives = page
            .drives
            .into_iter()
            .map(|drive| {
                validate_drive_id(&drive.id)?;
                Ok(GoogleSharedDrive {
                    id: drive.id,
                    name: validate_secret("Google Shared Drive name", drive.name)?,
                })
            })
            .collect::<Result<Vec<_>, BackendError>>()?;
        Ok(GoogleSharedDrivePage {
            drives,
            next_page: page.next_page_token.map(PageToken::new).transpose()?,
        })
    }

    async fn token(&self) -> Result<String, BackendError> {
        let token = self.token_provider.access_token().await?;
        if token.trim().is_empty() || token.chars().any(char::is_control) {
            return Err(invalid("OAuth provider returned an invalid access token"));
        }
        Ok(token)
    }
}

#[async_trait]
impl StorageBackend for GoogleDriveBackend {
    async fn get_object(&self, id: &StorageObjectId) -> Result<StorageObject, BackendError> {
        let file_id = validate_object_id(id)?;
        let token = self.token().await?;
        let value = self
            .transport
            .get_json(
                &self.api_base,
                &format!("files/{file_id}"),
                &[
                    ("supportsAllDrives".to_owned(), "true".to_owned()),
                    (
                        "fields".to_owned(),
                        "id,name,mimeType,size,md5Checksum,modifiedTime,version,trashed".to_owned(),
                    ),
                ],
                &token,
            )
            .await?;
        let file: DriveFile = serde_json::from_value(value)
            .map_err(|_| invalid("Google Drive returned malformed file JSON"))?;
        file.into_storage_object()?.ok_or(BackendError::NotFound)
    }

    async fn list_children(
        &self,
        parent: &StorageObjectId,
        page: Option<PageToken>,
    ) -> Result<ObjectPage, BackendError> {
        let parent_id = validate_object_id(parent)?;
        let token = self.token().await?;
        let mut query = vec![
            ("q".to_owned(), format!("'{parent_id}' in parents and trashed = false")),
            ("spaces".to_owned(), "drive".to_owned()),
            ("pageSize".to_owned(), "1000".to_owned()),
            ("supportsAllDrives".to_owned(), "true".to_owned()),
            ("includeItemsFromAllDrives".to_owned(), "true".to_owned()),
            ("fields".to_owned(), "nextPageToken,files(id,name,mimeType,size,md5Checksum,modifiedTime,version,trashed)".to_owned()),
        ];
        if let Some(page) = page {
            query.push(("pageToken".to_owned(), page.as_str().to_owned()));
        }
        if let GoogleDriveScope::SharedDrive(drive_id) = &self.scope {
            query.push(("driveId".to_owned(), drive_id.clone()));
            query.push(("corpora".to_owned(), "drive".to_owned()));
        }
        let value = self
            .transport
            .get_json(&self.api_base, "files", &query, &token)
            .await?;
        let page: FileList = serde_json::from_value(value)
            .map_err(|_| invalid("Google Drive returned malformed file-list JSON"))?;
        let objects = page
            .files
            .into_iter()
            .filter_map(|file| match file.into_storage_object() {
                Ok(Some(object)) => Some(Ok(object)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ObjectPage {
            objects,
            next_page: page.next_page_token.map(PageToken::new).transpose()?,
        })
    }

    async fn list_changes(&self, cursor: ChangeCursor) -> Result<ChangePage, BackendError> {
        let token = self.token().await?;
        let mut query = vec![
            ("pageToken".to_owned(), cursor.as_str().to_owned()),
            ("spaces".to_owned(), "drive".to_owned()),
            ("pageSize".to_owned(), "1000".to_owned()),
            ("supportsAllDrives".to_owned(), "true".to_owned()),
            ("includeItemsFromAllDrives".to_owned(), "true".to_owned()),
            ("fields".to_owned(), "nextPageToken,newStartPageToken,changes(fileId,removed,file(id,parents,name,mimeType,size,md5Checksum,modifiedTime,version,trashed))".to_owned()),
        ];
        if let GoogleDriveScope::SharedDrive(drive_id) = &self.scope {
            query.push(("driveId".to_owned(), drive_id.clone()));
        }
        let value = self
            .transport
            .get_json(&self.api_base, "changes", &query, &token)
            .await?;
        let page: ChangeList = serde_json::from_value(value)
            .map_err(|_| invalid("Google Drive returned malformed change-list JSON"))?;
        let (next_cursor, has_more) = match (page.next_page_token, page.new_start_page_token) {
            (Some(cursor), _) => (cursor, true),
            (None, Some(cursor)) => (cursor, false),
            (None, None) => {
                return Err(invalid("Google Drive change page omitted its next cursor"));
            }
        };
        let changes = page
            .changes
            .into_iter()
            .map(DriveChange::into_storage_change)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = ChangeCursor::new(next_cursor)?;
        Ok(if has_more {
            ChangePage::continuation(changes, next_cursor)
        } else {
            ChangePage::new(changes, next_cursor)
        })
    }

    async fn latest_change_cursor(&self) -> Result<ChangeCursor, BackendError> {
        self.start_page_token().await
    }

    async fn open_range(
        &self,
        id: &StorageObjectId,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        let file_id = validate_object_id(id)?;
        let token = self.token().await?;
        self.transport
            .get_range(
                &self.api_base,
                &format!("files/{file_id}"),
                &[("alt".to_owned(), "media".to_owned())],
                &token,
                range,
            )
            .await
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities::new()
            .with_changes(true)
            .with_range_reads(true)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileList {
    files: Vec<DriveFile>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedDriveList {
    #[serde(default)]
    drives: Vec<SharedDrive>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedDrive {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartPageToken {
    start_page_token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AboutResponse {
    user: Option<AboutUser>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AboutUser {
    email_address: Option<String>,
    display_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangeList {
    changes: Vec<DriveChange>,
    next_page_token: Option<String>,
    new_start_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveChange {
    file_id: String,
    #[serde(default)]
    removed: bool,
    file: Option<DriveFile>,
}

impl DriveChange {
    fn into_storage_change(self) -> Result<tjxy_storage::StorageChange, BackendError> {
        validate_drive_id(&self.file_id)?;
        if self.removed {
            return Ok(tjxy_storage::StorageChange::Removed(StorageObjectId::new(
                PROVIDER,
                self.file_id,
            )?));
        }
        let file = self
            .file
            .ok_or_else(|| invalid("Google Drive upsert change omitted file metadata"))?;
        match file.into_storage_object()? {
            Some(object) => Ok(tjxy_storage::StorageChange::Upsert(object)),
            None => Ok(tjxy_storage::StorageChange::Removed(StorageObjectId::new(
                PROVIDER,
                self.file_id,
            )?)),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFile {
    id: String,
    #[serde(default)]
    parents: Vec<String>,
    name: String,
    mime_type: String,
    size: Option<String>,
    md5_checksum: Option<String>,
    modified_time: Option<DateTime<Utc>>,
    version: Option<String>,
    trashed: Option<bool>,
}

impl DriveFile {
    fn into_storage_object(self) -> Result<Option<StorageObject>, BackendError> {
        if self.trashed.unwrap_or(false) {
            return Ok(None);
        }
        let id = StorageObjectId::new(PROVIDER, self.id)?;
        let mut object = if self.mime_type == FOLDER_MIME_TYPE {
            StorageObject::directory(id, self.name)
        } else {
            let Some(size) = self.size else {
                return Ok(None);
            };
            StorageObject::file(
                id,
                self.name,
                size.parse()
                    .map_err(|_| invalid("Google Drive file size is invalid"))?,
            )
        };
        object = object.with_parents(
            self.parents
                .into_iter()
                .map(|parent| StorageObjectId::new(PROVIDER, parent))
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        object = object.with_mime_type(self.mime_type)?;
        if let Some(checksum) = self.md5_checksum {
            object = object.with_checksum(checksum)?;
        }
        if let Some(version) = self.version {
            object = object.with_remote_revision(version)?;
        }
        if let Some(modified) = self.modified_time {
            object = object.with_remote_modified_at(modified);
        }
        Ok(Some(object))
    }
}

#[async_trait]
impl GoogleDriveTransport for ReqwestTransport {
    async fn get_json(
        &self,
        api_base: &Url,
        path: &str,
        query: &[(String, String)],
        access_token: &str,
    ) -> Result<serde_json::Value, BackendError> {
        let response = self
            .client
            .get(endpoint(api_base, path)?)
            .bearer_auth(access_token)
            .query(query)
            .send()
            .await
            .map_err(http_error)?;
        checked(response)?
            .json()
            .await
            .map_err(|_| invalid("Google Drive returned malformed JSON"))
    }

    async fn get_range(
        &self,
        api_base: &Url,
        path: &str,
        query: &[(String, String)],
        access_token: &str,
        range: ByteRange,
    ) -> Result<ByteStream, BackendError> {
        let end_inclusive = range
            .end_exclusive()
            .checked_sub(1)
            .ok_or_else(|| invalid("Google Drive byte range is invalid"))?;
        let response = self
            .client
            .get(endpoint(api_base, path)?)
            .bearer_auth(access_token)
            .query(query)
            .header(
                reqwest::header::RANGE,
                format!("bytes={}-{}", range.start(), end_inclusive),
            )
            .send()
            .await
            .map_err(http_error)?;
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
            let size = unsatisfied_size(response.headers())
                .ok_or_else(|| invalid("Google Drive 416 response omitted object size"))?;
            return Err(BackendError::RangeNotSatisfiable { size });
        }
        if response.status() != StatusCode::PARTIAL_CONTENT {
            return match checked(response) {
                Ok(_) => Err(temporary("Google Drive ignored the bounded Range request")),
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
                        yield Err(invalid("Google Drive Range body exceeded Content-Range"));
                        return;
                    }
                    remaining -= length;
                    yield Ok(bytes);
                } else {
                    yield Err(temporary("Google Drive Range stream failed"));
                    return;
                }
            }
            if remaining != 0 {
                yield Err(temporary("Google Drive Range stream ended early"));
            }
        }))
    }
}

fn endpoint(api_base: &Url, path: &str) -> Result<Url, BackendError> {
    let mut base = api_base.clone();
    if !base.path().ends_with('/') {
        base.set_path(&format!("{}/", base.path()));
    }
    base.join(path)
        .map_err(|_| invalid("Google Drive endpoint could not be constructed"))
}

fn validate_content_range(
    headers: &reqwest::header::HeaderMap,
    expected: ByteRange,
) -> Result<(), BackendError> {
    let value = headers
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| invalid("Google Drive 206 response omitted Content-Range"))?;
    let Some(value) = value.strip_prefix("bytes ") else {
        return Err(invalid("Google Drive Content-Range is malformed"));
    };
    let Some((interval, _size)) = value.split_once('/') else {
        return Err(invalid("Google Drive Content-Range is malformed"));
    };
    let Some((start, end)) = interval.split_once('-') else {
        return Err(invalid("Google Drive Content-Range is malformed"));
    };
    let start = start
        .parse::<u64>()
        .map_err(|_| invalid("Google Drive Content-Range is malformed"))?;
    let end = end
        .parse::<u64>()
        .map_err(|_| invalid("Google Drive Content-Range is malformed"))?;
    if start != expected.start() || end.checked_add(1) != Some(expected.end_exclusive()) {
        return Err(invalid("Google Drive returned the wrong byte interval"));
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

fn checked(response: reqwest::Response) -> Result<reqwest::Response, BackendError> {
    match response.status() {
        status if status.is_success() => Ok(response),
        StatusCode::NOT_FOUND => Err(BackendError::NotFound),
        status => Err(classify_http_failure(
            status,
            response.headers(),
            std::time::SystemTime::now(),
        )),
    }
}

fn classify_http_failure(
    status: StatusCode,
    headers: &reqwest::header::HeaderMap,
    now: std::time::SystemTime,
) -> BackendError {
    match status {
        StatusCode::GONE => BackendError::ChangeCursorInvalid,
        StatusCode::TOO_MANY_REQUESTS => BackendError::RateLimited {
            retry_after: parse_retry_after(headers, now),
        },
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            temporary("Google Drive rejected the request")
        }
        status if status.is_server_error() => temporary("Google Drive is temporarily unavailable"),
        _ => invalid("Google Drive rejected an invalid request"),
    }
}

fn parse_retry_after(
    headers: &reqwest::header::HeaderMap,
    now: std::time::SystemTime,
) -> Option<std::time::Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(std::time::Duration::from_secs(seconds));
    }
    httpdate::parse_http_date(value)
        .ok()?
        .duration_since(now)
        .ok()
}

fn validate_object_id(id: &StorageObjectId) -> Result<&str, BackendError> {
    if id.provider() != PROVIDER {
        return Err(BackendError::NotFound);
    }
    validate_drive_id(id.provider_object_id())?;
    Ok(id.provider_object_id())
}

fn validate_drive_id(value: &str) -> Result<(), BackendError> {
    if value.is_empty()
        || value.len() > 2048
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(invalid("Google Drive ID is invalid"));
    }
    Ok(())
}

fn parse_redirect_uri(value: &str) -> Result<Url, BackendError> {
    let url = Url::parse(value).map_err(|_| invalid("OAuth redirect URI is invalid"))?;
    if url.query().is_some()
        || url.fragment().is_some()
        || url.username() != ""
        || url.password().is_some()
        || !secure_or_loopback(&url)
    {
        return Err(invalid("OAuth redirect URI must be HTTPS or loopback HTTP"));
    }
    Ok(url)
}

fn parse_provider_endpoint(value: &str) -> Result<Url, BackendError> {
    let url = Url::parse(value).map_err(|_| invalid("OAuth endpoint is invalid"))?;
    if url.query().is_some()
        || url.fragment().is_some()
        || url.username() != ""
        || url.password().is_some()
        || !secure_or_loopback(&url)
    {
        return Err(invalid("OAuth endpoint must be HTTPS or loopback HTTP"));
    }
    Ok(url)
}

fn secure_or_loopback(url: &Url) -> bool {
    if url.scheme() == "https" {
        return true;
    }
    url.scheme() == "http"
        && url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        })
}

fn http_error(_error: reqwest::Error) -> BackendError {
    temporary("Google Drive request failed")
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use reqwest::{StatusCode, Url, header};
    use tjxy_storage::BackendError;

    use super::classify_http_failure;

    #[test]
    fn web_oauth_authorization_url_contains_pkce_without_client_secret() {
        let client = super::GoogleOAuthWebClient::new(
            "client-id",
            "client-secret-never-exposed",
            "http://127.0.0.1:8096/oauth/callback",
        )
        .unwrap();
        let url = client
            .authorization_url("state-value", "challenge-value")
            .unwrap();
        let parsed = Url::parse(&url).unwrap();
        let query: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(query["state"], "state-value");
        assert_eq!(query["code_challenge"], "challenge-value");
        assert_eq!(query["code_challenge_method"], "S256");
        assert!(!url.contains("client-secret-never-exposed"));
    }

    #[test]
    fn web_oauth_rejects_non_loopback_http_redirects() {
        assert!(
            super::GoogleOAuthWebClient::new(
                "client-id",
                "client-secret",
                "http://example.invalid/oauth/callback",
            )
            .is_err()
        );
    }

    #[test]
    fn rate_limit_preserves_retry_after_delta_seconds() {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::RETRY_AFTER, header::HeaderValue::from_static("17"));

        assert_eq!(
            classify_http_failure(
                StatusCode::TOO_MANY_REQUESTS,
                &headers,
                SystemTime::UNIX_EPOCH
            ),
            BackendError::RateLimited {
                retry_after: Some(Duration::from_secs(17)),
            }
        );
    }

    #[test]
    fn gone_response_marks_the_change_cursor_invalid() {
        assert_eq!(
            classify_http_failure(
                StatusCode::GONE,
                &header::HeaderMap::new(),
                SystemTime::UNIX_EPOCH,
            ),
            BackendError::ChangeCursorInvalid
        );
    }

    #[test]
    fn rate_limit_accepts_http_date_and_ignores_past_dates() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let future = now + Duration::from_secs(23);
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::RETRY_AFTER,
            header::HeaderValue::from_str(&httpdate::fmt_http_date(future)).unwrap(),
        );
        assert_eq!(
            classify_http_failure(StatusCode::TOO_MANY_REQUESTS, &headers, now),
            BackendError::RateLimited {
                retry_after: Some(Duration::from_secs(23)),
            }
        );

        headers.insert(
            header::RETRY_AFTER,
            header::HeaderValue::from_str(&httpdate::fmt_http_date(now - Duration::from_secs(1)))
                .unwrap(),
        );
        assert_eq!(
            classify_http_failure(StatusCode::TOO_MANY_REQUESTS, &headers, now),
            BackendError::RateLimited { retry_after: None }
        );
    }
}
