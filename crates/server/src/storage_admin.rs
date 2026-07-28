use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tjxy_common::LibraryId;
use tjxy_credentials::{CredentialCipher, CredentialCipherError};
use tjxy_db::{
    CreatedStorageBinding, CredentialRefreshState, StorageAccountRepository,
    StorageAccountRepositoryError, StorageBindingDraft, StorageBindingRepository,
    StorageBindingRepositoryError, StorageCredentialRepository,
};
use tjxy_storage::{BackendError, ObjectType, PageToken, StorageBackend, StorageObjectId};
use tjxy_storage_google_drive::{
    GoogleDriveBackend, GoogleDriveScope, GoogleOAuthClient, GoogleOAuthCredentials,
    GoogleOAuthWebClient, RefreshingAccessTokenProvider,
};
use tjxy_storage_onedrive::{
    MicrosoftCredentialStore, MicrosoftOAuthClient, MicrosoftOAuthCredentials,
    MicrosoftOAuthWebClient, MicrosoftPersonalDrive, OneDriveBackend, OneDriveScope,
    RefreshingMicrosoftAccessTokenProvider,
};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{AppState, auth, storage_admin_cursor::DirectoryPageCursorRegistry};

pub(crate) struct StorageAdminService {
    database: DatabaseConnection,
    cipher: Arc<CredentialCipher>,
    google_oauth: Option<Arc<GoogleDriveOAuthService>>,
    onedrive_oauth: Option<Arc<OneDriveOAuthService>>,
    storage_runtime: Arc<crate::runtime_storage::RuntimeStorageManager>,
}

impl StorageAdminService {
    pub fn new(
        database: DatabaseConnection,
        cipher: Arc<CredentialCipher>,
        google_oauth: Option<GoogleDriveOAuthConfiguration>,
        onedrive_oauth: Option<MicrosoftOneDriveOAuthConfiguration>,
        storage_runtime: Arc<crate::runtime_storage::RuntimeStorageManager>,
    ) -> Self {
        Self {
            database,
            cipher,
            google_oauth: google_oauth.map(GoogleDriveOAuthService::new).map(Arc::new),
            onedrive_oauth: onedrive_oauth.map(OneDriveOAuthService::new).map(Arc::new),
            storage_runtime,
        }
    }

    async fn bind_onedrive(
        &self,
        request: OneDriveBindingRequest,
        credentials: MicrosoftOAuthCredentials,
        oauth: &OneDriveOAuthService,
    ) -> Result<CreatedStorageBinding, StorageAdminError> {
        let credential_id = Uuid::new_v4();
        let payload = credentials.to_payload_json()?;
        let runtime_credentials = credentials.clone();
        let backend = oauth.onedrive_backend(credentials, &request.drive_id)?;
        let root = backend
            .get_object(&StorageObjectId::new("onedrive", request.root_object_id)?)
            .await?;
        let cursor = backend.latest_delta_cursor().await?;
        let envelope = self.cipher.seal(credential_id, "onedrive", &payload)?;
        let provider_drive_id = request.drive_id.clone();
        let draft = StorageBindingDraft::new(
            "onedrive",
            request.display_name,
            request.account_identity,
            credential_id,
            request.target_library_id,
            envelope,
            provider_drive_id.clone(),
            root.id().provider_object_id(),
            root.name(),
            cursor,
        )?;
        let created = StorageBindingRepository::new(&self.database)
            .create(&draft)
            .await?;
        let store = Arc::new(SqlMicrosoftCredentialStore {
            database: self.database.clone(),
            cipher: Arc::clone(&self.cipher),
            credential_id,
        });
        let runtime_backend = Arc::new(oauth.onedrive_backend_with_store(
            runtime_credentials,
            &provider_drive_id,
            store,
        )?);
        self.activate_provider(created.account_id(), provider_drive_id, runtime_backend)
            .await?;
        Ok(created)
    }

    async fn bind_google_drive(
        &self,
        request: GoogleDriveBindingRequest,
        credentials: GoogleOAuthCredentials,
        oauth: &GoogleDriveOAuthService,
    ) -> Result<CreatedStorageBinding, StorageAdminError> {
        let credential_id = Uuid::new_v4();
        let payload = credentials.to_payload_json()?;
        let scope = match request.scope {
            GoogleScope::MyDrive => GoogleDriveScope::MyDrive,
            GoogleScope::SharedDrive => GoogleDriveScope::SharedDrive(
                request
                    .shared_drive_id
                    .clone()
                    .ok_or(StorageAdminError::InvalidRequest)?,
            ),
        };
        let provider_drive_id = request
            .shared_drive_id
            .clone()
            .unwrap_or_else(|| "my-drive".to_owned());
        let requested_root = request.root_object_id.clone();
        let runtime_credentials = credentials.clone();
        let runtime_scope = scope.clone();
        let backend = oauth.google_backend(credentials, scope)?;
        let root = backend
            .get_object(&StorageObjectId::new("google-drive", requested_root)?)
            .await?;
        let cursor = backend.start_page_token().await?;
        let envelope = self.cipher.seal(credential_id, "google-drive", &payload)?;
        let draft = StorageBindingDraft::new(
            "google-drive",
            request.display_name,
            request.account_identity,
            credential_id,
            request.target_library_id,
            envelope,
            provider_drive_id.clone(),
            root.id().provider_object_id(),
            root.name(),
            cursor,
        )?;
        let created = StorageBindingRepository::new(&self.database)
            .create(&draft)
            .await?;
        let runtime_backend = Arc::new(oauth.google_backend(runtime_credentials, runtime_scope)?);
        self.activate_provider(created.account_id(), provider_drive_id, runtime_backend)
            .await?;
        Ok(created)
    }

    async fn activate_provider(
        &self,
        account_id: Uuid,
        provider_drive_id: String,
        backend: Arc<dyn StorageBackend>,
    ) -> Result<(), StorageAdminError> {
        if let Err(error) =
            self.storage_runtime
                .activate_provider(account_id, provider_drive_id, backend)
        {
            StorageAccountRepository::new(&self.database)
                .disable_after_activation_failure(account_id)
                .await?;
            return Err(StorageAdminError::Runtime(error));
        }
        Ok(())
    }
}

pub(crate) struct SqlMicrosoftCredentialStore {
    pub(crate) database: DatabaseConnection,
    pub(crate) cipher: Arc<CredentialCipher>,
    pub(crate) credential_id: Uuid,
}

#[async_trait::async_trait]
impl MicrosoftCredentialStore for SqlMicrosoftCredentialStore {
    async fn persist(&self, credentials: &MicrosoftOAuthCredentials) -> Result<(), BackendError> {
        let payload = credentials.to_payload_json()?;
        let envelope = self
            .cipher
            .seal(self.credential_id, "onedrive", &payload)
            .map_err(|_| BackendError::TemporarilyUnavailable {
                message: "encrypted Microsoft credential rotation failed".into(),
            })?;
        StorageCredentialRepository::new(&self.database)
            .put(self.credential_id, &envelope, CredentialRefreshState::Ready)
            .await
            .map_err(|_| BackendError::TemporarilyUnavailable {
                message: "encrypted Microsoft credential persistence failed".into(),
            })
    }
}

const GOOGLE_OAUTH_SESSION_TTL: Duration = Duration::from_secs(10 * 60);

/// Server-only configuration for Google Drive OAuth. The secret is never serialized to Admin.
pub struct GoogleDriveOAuthConfiguration {
    client: GoogleOAuthWebClient,
    drive_api_base: String,
}

impl GoogleDriveOAuthConfiguration {
    /// Builds production Google Drive OAuth configuration.
    ///
    /// # Errors
    ///
    /// Returns a backend error when the client configuration or redirect URI is invalid.
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: impl AsRef<str>,
    ) -> Result<Self, BackendError> {
        Ok(Self {
            client: GoogleOAuthWebClient::new(client_id, client_secret, redirect_uri)?,
            drive_api_base: "https://www.googleapis.com/drive/v3".to_owned(),
        })
    }

    /// Overrides OAuth and Drive endpoints for a loopback test server or an HTTPS-compatible
    /// proxy. Production callers normally use Google's defaults.
    ///
    /// # Errors
    ///
    /// Returns a backend error when an OAuth endpoint is unsafe.
    pub fn with_endpoints(
        mut self,
        authorization_endpoint: impl AsRef<str>,
        token_endpoint: impl AsRef<str>,
        drive_api_base: impl Into<String>,
    ) -> Result<Self, BackendError> {
        self.client = self
            .client
            .with_endpoints(authorization_endpoint, token_endpoint)?;
        self.drive_api_base = drive_api_base.into();
        Ok(self)
    }
}

/// Server-only configuration for Microsoft Personal `OneDrive` OAuth.
pub struct MicrosoftOneDriveOAuthConfiguration {
    client: MicrosoftOAuthWebClient,
}

impl MicrosoftOneDriveOAuthConfiguration {
    /// Builds production Microsoft consumer-account OAuth configuration.
    ///
    /// # Errors
    ///
    /// Returns a backend error for an invalid client configuration or redirect URI.
    pub fn new(
        client_id: impl Into<String>,
        client_secret: Option<String>,
        redirect_uri: impl AsRef<str>,
    ) -> Result<Self, BackendError> {
        Ok(Self {
            client: MicrosoftOAuthWebClient::new(client_id, client_secret, redirect_uri)?,
        })
    }

    /// Overrides OAuth and Graph endpoints for a loopback test server or HTTPS proxy.
    ///
    /// # Errors
    ///
    /// Returns a backend error for unsafe endpoint URLs.
    pub fn with_endpoints(
        mut self,
        authorization_endpoint: impl AsRef<str>,
        token_endpoint: impl AsRef<str>,
        graph_api_base: impl AsRef<str>,
    ) -> Result<Self, BackendError> {
        self.client =
            self.client
                .with_endpoints(authorization_endpoint, token_endpoint, graph_api_base)?;
        Ok(self)
    }
}

struct GoogleDriveOAuthService {
    client: GoogleOAuthWebClient,
    drive_api_base: String,
    sessions: Arc<tokio::sync::Mutex<HashMap<Uuid, GoogleOAuthSession>>>,
}

struct GoogleOAuthSession {
    owner_session_id: Uuid,
    target_library_id: LibraryId,
    expires_at: Instant,
    code_verifier: Zeroizing<String>,
    directory_cursors: DirectoryPageCursorRegistry<GoogleDirectoryPageContext>,
    status: GoogleOAuthSessionStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GoogleDirectoryPageContext {
    scope: GoogleDriveScope,
    parent_id: String,
}

enum GoogleOAuthSessionStatus {
    AwaitingCallback,
    Authorized {
        credentials: GoogleOAuthCredentials,
        account_identity: String,
    },
}

impl GoogleDriveOAuthService {
    fn new(configuration: GoogleDriveOAuthConfiguration) -> Self {
        let sessions = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        spawn_google_oauth_session_cleanup(Arc::downgrade(&sessions));
        Self {
            client: configuration.client,
            drive_api_base: configuration.drive_api_base,
            sessions,
        }
    }

    async fn begin(
        &self,
        owner_session_id: Uuid,
        target_library_id: LibraryId,
    ) -> Result<(Uuid, String), StorageAdminError> {
        let state = Uuid::new_v4();
        let code_verifier = Zeroizing::new(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ));
        let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        let authorization_url = self
            .client
            .authorization_url(&state.to_string(), &code_challenge)?;
        let mut sessions = self.sessions.lock().await;
        purge_expired(&mut sessions);
        sessions.insert(
            state,
            GoogleOAuthSession {
                owner_session_id,
                target_library_id,
                expires_at: Instant::now() + GOOGLE_OAUTH_SESSION_TTL,
                code_verifier,
                directory_cursors: DirectoryPageCursorRegistry::default(),
                status: GoogleOAuthSessionStatus::AwaitingCallback,
            },
        );
        Ok((state, authorization_url))
    }

    async fn complete_callback(
        &self,
        state: Uuid,
        code: Zeroizing<String>,
    ) -> Result<(), StorageAdminError> {
        let session = {
            let mut sessions = self.sessions.lock().await;
            purge_expired(&mut sessions);
            let session = sessions
                .remove(&state)
                .ok_or(StorageAdminError::InvalidRequest)?;
            if !matches!(&session.status, GoogleOAuthSessionStatus::AwaitingCallback) {
                sessions.insert(state, session);
                return Err(StorageAdminError::InvalidRequest);
            }
            session
        };
        let credentials = self
            .client
            .exchange_authorization_code(&code, &session.code_verifier)
            .await?;
        let backend = self.google_backend(credentials.clone(), GoogleDriveScope::MyDrive)?;
        let account_identity = backend.account_identity().await?;
        self.sessions.lock().await.insert(
            state,
            GoogleOAuthSession {
                status: GoogleOAuthSessionStatus::Authorized {
                    credentials,
                    account_identity,
                },
                ..session
            },
        );
        Ok(())
    }

    async fn with_authorized_session<ResultValue>(
        &self,
        state: Uuid,
        owner_session_id: Uuid,
        action: impl FnOnce(&GoogleOAuthCredentials, &str, LibraryId) -> ResultValue,
    ) -> Result<ResultValue, StorageAdminError> {
        let mut sessions = self.sessions.lock().await;
        purge_expired(&mut sessions);
        let session = sessions
            .get_mut(&state)
            .ok_or(StorageAdminError::InvalidRequest)?;
        if session.owner_session_id != owner_session_id {
            return Err(StorageAdminError::Forbidden);
        }
        let GoogleOAuthSessionStatus::Authorized {
            credentials,
            account_identity,
        } = &session.status
        else {
            return Err(StorageAdminError::Conflict);
        };
        Ok(action(
            credentials,
            account_identity,
            session.target_library_id,
        ))
    }

    async fn take_authorized_session(
        &self,
        state: Uuid,
        owner_session_id: Uuid,
    ) -> Result<(GoogleOAuthCredentials, String, LibraryId), StorageAdminError> {
        let session = {
            let mut sessions = self.sessions.lock().await;
            purge_expired(&mut sessions);
            sessions
                .remove(&state)
                .ok_or(StorageAdminError::InvalidRequest)?
        };
        if session.owner_session_id != owner_session_id {
            return Err(StorageAdminError::Forbidden);
        }
        let GoogleOAuthSessionStatus::Authorized {
            credentials,
            account_identity,
        } = session.status
        else {
            return Err(StorageAdminError::Conflict);
        };
        Ok((credentials, account_identity, session.target_library_id))
    }

    async fn prepare_directory_page(
        &self,
        state: Uuid,
        owner_session_id: Uuid,
        context: &GoogleDirectoryPageContext,
        cursor: Option<Uuid>,
    ) -> Result<(GoogleOAuthCredentials, Option<PageToken>), StorageAdminError> {
        let mut sessions = self.sessions.lock().await;
        purge_expired(&mut sessions);
        let session = sessions
            .get_mut(&state)
            .ok_or(StorageAdminError::InvalidRequest)?;
        if session.owner_session_id != owner_session_id {
            return Err(StorageAdminError::Forbidden);
        }
        let GoogleOAuthSessionStatus::Authorized { credentials, .. } = &session.status else {
            return Err(StorageAdminError::Conflict);
        };
        let provider_page = session
            .directory_cursors
            .resolve(cursor, context)
            .map_err(|_| StorageAdminError::InvalidRequest)?;
        Ok((credentials.clone(), provider_page))
    }

    async fn register_directory_page(
        &self,
        state: Uuid,
        owner_session_id: Uuid,
        context: GoogleDirectoryPageContext,
        provider_token: Option<PageToken>,
    ) -> Result<Option<Uuid>, StorageAdminError> {
        let mut sessions = self.sessions.lock().await;
        purge_expired(&mut sessions);
        let session = sessions
            .get_mut(&state)
            .ok_or(StorageAdminError::InvalidRequest)?;
        if session.owner_session_id != owner_session_id {
            return Err(StorageAdminError::Forbidden);
        }
        if !matches!(session.status, GoogleOAuthSessionStatus::Authorized { .. }) {
            return Err(StorageAdminError::Conflict);
        }
        Ok(session.directory_cursors.register(context, provider_token))
    }

    fn google_backend(
        &self,
        credentials: GoogleOAuthCredentials,
        scope: GoogleDriveScope,
    ) -> Result<GoogleDriveBackend, BackendError> {
        let refresh_client =
            GoogleOAuthClient::new()?.with_token_endpoint(self.client.token_endpoint().as_str())?;
        let provider = RefreshingAccessTokenProvider::new(credentials, Arc::new(refresh_client));
        GoogleDriveBackend::new(provider, scope)?.with_api_base(&self.drive_api_base)
    }
}

fn purge_expired(sessions: &mut HashMap<Uuid, GoogleOAuthSession>) {
    let now = Instant::now();
    sessions.retain(|_, session| session.expires_at > now);
}

fn spawn_google_oauth_session_cleanup(
    sessions: std::sync::Weak<tokio::sync::Mutex<HashMap<Uuid, GoogleOAuthSession>>>,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let Some(sessions) = sessions.upgrade() else {
                break;
            };
            let mut sessions = sessions.lock().await;
            purge_expired(&mut sessions);
        }
    });
}

struct OneDriveOAuthService {
    client: MicrosoftOAuthWebClient,
    sessions: Arc<tokio::sync::Mutex<HashMap<Uuid, OneDriveOAuthSession>>>,
}

struct OneDriveOAuthSession {
    owner_session_id: Uuid,
    target_library_id: LibraryId,
    expires_at: Instant,
    code_verifier: Zeroizing<String>,
    directory_cursors: DirectoryPageCursorRegistry<OneDriveDirectoryPageContext>,
    status: OneDriveOAuthSessionStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OneDriveDirectoryPageContext {
    drive_id: String,
    parent_id: String,
}

struct PreparedOneDriveDirectoryPage {
    credentials: MicrosoftOAuthCredentials,
    drive_id: String,
    parent: StorageObjectId,
    context: OneDriveDirectoryPageContext,
    provider_page: Option<PageToken>,
}

enum OneDriveOAuthSessionStatus {
    AwaitingCallback,
    Authorized {
        credentials: MicrosoftOAuthCredentials,
        personal_drive: MicrosoftPersonalDrive,
    },
}

impl OneDriveOAuthService {
    fn new(configuration: MicrosoftOneDriveOAuthConfiguration) -> Self {
        let sessions = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        spawn_onedrive_oauth_session_cleanup(Arc::downgrade(&sessions));
        Self {
            client: configuration.client,
            sessions,
        }
    }

    async fn begin(
        &self,
        owner_session_id: Uuid,
        target_library_id: LibraryId,
    ) -> Result<(Uuid, String), StorageAdminError> {
        let state = Uuid::new_v4();
        let code_verifier = Zeroizing::new(format!(
            "{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        ));
        let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        let authorization_url = self
            .client
            .authorization_url(&state.to_string(), &code_challenge)?;
        let mut sessions = self.sessions.lock().await;
        purge_expired_onedrive(&mut sessions);
        sessions.insert(
            state,
            OneDriveOAuthSession {
                owner_session_id,
                target_library_id,
                expires_at: Instant::now() + GOOGLE_OAUTH_SESSION_TTL,
                code_verifier,
                directory_cursors: DirectoryPageCursorRegistry::default(),
                status: OneDriveOAuthSessionStatus::AwaitingCallback,
            },
        );
        Ok((state, authorization_url))
    }

    async fn complete_callback(
        &self,
        state: Uuid,
        code: Zeroizing<String>,
    ) -> Result<(), StorageAdminError> {
        let session = {
            let mut sessions = self.sessions.lock().await;
            purge_expired_onedrive(&mut sessions);
            let session = sessions
                .remove(&state)
                .ok_or(StorageAdminError::InvalidRequest)?;
            if !matches!(
                &session.status,
                OneDriveOAuthSessionStatus::AwaitingCallback
            ) {
                sessions.insert(state, session);
                return Err(StorageAdminError::InvalidRequest);
            }
            session
        };
        let credentials = self
            .client
            .exchange_authorization_code(&code, &session.code_verifier)
            .await?;
        let personal_drive = self
            .client
            .discover_personal_drive(credentials.clone())
            .await?;
        self.sessions.lock().await.insert(
            state,
            OneDriveOAuthSession {
                status: OneDriveOAuthSessionStatus::Authorized {
                    credentials,
                    personal_drive,
                },
                ..session
            },
        );
        Ok(())
    }

    async fn take_authorized_session(
        &self,
        state: Uuid,
        owner_session_id: Uuid,
    ) -> Result<(MicrosoftOAuthCredentials, MicrosoftPersonalDrive, LibraryId), StorageAdminError>
    {
        let session = {
            let mut sessions = self.sessions.lock().await;
            purge_expired_onedrive(&mut sessions);
            sessions
                .remove(&state)
                .ok_or(StorageAdminError::InvalidRequest)?
        };
        if session.owner_session_id != owner_session_id {
            return Err(StorageAdminError::Forbidden);
        }
        let OneDriveOAuthSessionStatus::Authorized {
            credentials,
            personal_drive,
        } = session.status
        else {
            return Err(StorageAdminError::Conflict);
        };
        Ok((credentials, personal_drive, session.target_library_id))
    }

    async fn prepare_directory_page(
        &self,
        state: Uuid,
        owner_session_id: Uuid,
        parent_id: Option<String>,
        cursor: Option<Uuid>,
    ) -> Result<PreparedOneDriveDirectoryPage, StorageAdminError> {
        let mut sessions = self.sessions.lock().await;
        purge_expired_onedrive(&mut sessions);
        let session = sessions
            .get_mut(&state)
            .ok_or(StorageAdminError::InvalidRequest)?;
        if session.owner_session_id != owner_session_id {
            return Err(StorageAdminError::Forbidden);
        }
        let OneDriveOAuthSessionStatus::Authorized {
            credentials,
            personal_drive,
        } = &session.status
        else {
            return Err(StorageAdminError::Conflict);
        };
        let drive_id = personal_drive.drive_id().to_owned();
        let parent = StorageObjectId::new(
            "onedrive",
            parent_id.unwrap_or_else(|| personal_drive.root_id().to_owned()),
        )?;
        let context = OneDriveDirectoryPageContext {
            drive_id: drive_id.clone(),
            parent_id: parent.provider_object_id().to_owned(),
        };
        let provider_page = session
            .directory_cursors
            .resolve(cursor, &context)
            .map_err(|_| StorageAdminError::InvalidRequest)?;
        Ok(PreparedOneDriveDirectoryPage {
            credentials: credentials.clone(),
            drive_id,
            parent,
            context,
            provider_page,
        })
    }

    async fn register_directory_page(
        &self,
        state: Uuid,
        owner_session_id: Uuid,
        context: OneDriveDirectoryPageContext,
        provider_token: Option<PageToken>,
    ) -> Result<Option<Uuid>, StorageAdminError> {
        let mut sessions = self.sessions.lock().await;
        purge_expired_onedrive(&mut sessions);
        let session = sessions
            .get_mut(&state)
            .ok_or(StorageAdminError::InvalidRequest)?;
        if session.owner_session_id != owner_session_id {
            return Err(StorageAdminError::Forbidden);
        }
        if !matches!(
            session.status,
            OneDriveOAuthSessionStatus::Authorized { .. }
        ) {
            return Err(StorageAdminError::Conflict);
        }
        Ok(session.directory_cursors.register(context, provider_token))
    }

    fn onedrive_backend(
        &self,
        credentials: MicrosoftOAuthCredentials,
        drive_id: &str,
    ) -> Result<OneDriveBackend, BackendError> {
        let refresh_client = MicrosoftOAuthClient::new()?
            .with_token_endpoint(self.client.token_endpoint().as_str())?;
        let provider =
            RefreshingMicrosoftAccessTokenProvider::new(credentials, Arc::new(refresh_client));
        OneDriveBackend::new(provider, OneDriveScope::Personal, drive_id)?
            .with_api_base(self.client.graph_api_base().as_str())
    }

    fn onedrive_backend_with_store(
        &self,
        credentials: MicrosoftOAuthCredentials,
        drive_id: &str,
        store: Arc<SqlMicrosoftCredentialStore>,
    ) -> Result<OneDriveBackend, BackendError> {
        let refresh_client = MicrosoftOAuthClient::new()?
            .with_token_endpoint(self.client.token_endpoint().as_str())?;
        let provider =
            RefreshingMicrosoftAccessTokenProvider::new(credentials, Arc::new(refresh_client))
                .with_credential_store(store);
        OneDriveBackend::new(provider, OneDriveScope::Personal, drive_id)?
            .with_api_base(self.client.graph_api_base().as_str())
    }
}

fn purge_expired_onedrive(sessions: &mut HashMap<Uuid, OneDriveOAuthSession>) {
    let now = Instant::now();
    sessions.retain(|_, session| session.expires_at > now);
}

fn spawn_onedrive_oauth_session_cleanup(
    sessions: std::sync::Weak<tokio::sync::Mutex<HashMap<Uuid, OneDriveOAuthSession>>>,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let Some(sessions) = sessions.upgrade() else {
                break;
            };
            let mut sessions = sessions.lock().await;
            purge_expired_onedrive(&mut sessions);
        }
    });
}

struct OneDriveBindingRequest {
    target_library_id: LibraryId,
    display_name: String,
    account_identity: String,
    drive_id: String,
    root_object_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct OneDriveOAuthStartDto {
    target_library_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub(crate) struct OneDriveDirectoryQuery {
    parent_id: Option<String>,
    page_token: Option<Uuid>,
}

#[derive(Deserialize)]
pub(crate) struct OneDriveCallbackQuery {
    #[serde(rename = "state")]
    state: Uuid,
    #[serde(rename = "code")]
    code: Option<Zeroizing<String>>,
    #[serde(rename = "error")]
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct OneDriveBindingDto {
    display_name: String,
    root_object_id: String,
}

#[derive(Clone, Copy, Deserialize)]
enum GoogleScope {
    MyDrive,
    SharedDrive,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct GoogleDriveOAuthStartDto {
    target_library_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub(crate) struct GoogleDriveDirectoryQuery {
    scope: GoogleScope,
    shared_drive_id: Option<String>,
    parent_id: Option<String>,
    page_token: Option<Uuid>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub(crate) struct GoogleSharedDriveQuery {
    page_token: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct GoogleDriveCallbackQuery {
    #[serde(rename = "state")]
    state: Uuid,
    #[serde(rename = "code")]
    code: Option<Zeroizing<String>>,
    #[serde(rename = "error")]
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct GoogleDriveBindingDto {
    scope: GoogleScope,
    display_name: String,
    shared_drive_id: Option<String>,
    root_object_id: String,
}

struct GoogleDriveBindingRequest {
    target_library_id: LibraryId,
    scope: GoogleScope,
    display_name: String,
    account_identity: String,
    shared_drive_id: Option<String>,
    root_object_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct GoogleDriveOAuthStartResponse {
    state: Uuid,
    authorization_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct GoogleDriveDirectoryResponse {
    items: Vec<GoogleDriveDirectoryDto>,
    next_page_token: Option<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct GoogleDriveDirectoryDto {
    id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct GoogleSharedDriveResponse {
    items: Vec<GoogleDriveDirectoryDto>,
    next_page_token: Option<String>,
}

pub(crate) async fn start_onedrive_oauth(
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
    let session_id = match auth::authenticated_session_id(&principal) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: OneDriveOAuthStartDto = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.storage_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(oauth) = service.onedrive_oauth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match oauth
        .begin(session_id, LibraryId::from_uuid(request.target_library_id))
        .await
    {
        Ok((state, authorization_url)) => Json(GoogleDriveOAuthStartResponse {
            state,
            authorization_url,
        })
        .into_response(),
        Err(error) => oauth_response(&error),
    }
}

pub(crate) async fn onedrive_oauth_callback(
    State(state): State<AppState>,
    Query(query): Query<OneDriveCallbackQuery>,
) -> Response {
    if query.error.is_some() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(code) = query.code else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.storage_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(oauth) = service.onedrive_oauth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match oauth.complete_callback(query.state, code).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => oauth_response(&error),
    }
}

pub(crate) async fn onedrive_directories(
    State(state): State<AppState>,
    Path(oauth_state): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    Query(query): Query<OneDriveDirectoryQuery>,
) -> Response {
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let session_id = match auth::authenticated_session_id(&principal) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };
    let Some(service) = state.storage_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(oauth) = service.onedrive_oauth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let prepared = match oauth
        .prepare_directory_page(oauth_state, session_id, query.parent_id, query.page_token)
        .await
    {
        Ok(value) => value,
        Err(error) => return oauth_response(&error),
    };
    let backend = match oauth.onedrive_backend(prepared.credentials, &prepared.drive_id) {
        Ok(backend) => backend,
        Err(error) => return oauth_response(&StorageAdminError::Backend(error)),
    };
    match backend
        .list_children(&prepared.parent, prepared.provider_page)
        .await
    {
        Ok(page) => {
            let tjxy_storage::ObjectPage { objects, next_page } = page;
            let next_page_token = match oauth
                .register_directory_page(oauth_state, session_id, prepared.context, next_page)
                .await
            {
                Ok(cursor) => cursor,
                Err(error) => return oauth_response(&error),
            };
            Json(GoogleDriveDirectoryResponse {
                items: objects
                    .into_iter()
                    .filter(|item| item.object_type() == ObjectType::Directory)
                    .map(|item| GoogleDriveDirectoryDto {
                        id: item.id().provider_object_id().to_owned(),
                        name: item.name().to_owned(),
                    })
                    .collect(),
                next_page_token,
            })
            .into_response()
        }
        Err(error) => oauth_response(&StorageAdminError::Backend(error)),
    }
}

pub(crate) async fn bind_onedrive(
    State(state): State<AppState>,
    Path(oauth_state): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let session_id = match auth::authenticated_session_id(&principal) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: OneDriveBindingDto = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.storage_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(oauth) = service.onedrive_oauth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let (credentials, personal_drive, target_library_id) =
        match oauth.take_authorized_session(oauth_state, session_id).await {
            Ok(value) => value,
            Err(error) => return oauth_response(&error),
        };
    let request = OneDriveBindingRequest {
        target_library_id,
        display_name: request.display_name,
        account_identity: personal_drive.account_identity().to_owned(),
        drive_id: personal_drive.drive_id().to_owned(),
        root_object_id: if request.root_object_id == "root" {
            personal_drive.root_id().to_owned()
        } else {
            request.root_object_id
        },
    };
    binding_response(&service.bind_onedrive(request, credentials, oauth).await)
}

pub(crate) async fn start_google_drive_oauth(
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
    let session_id = match auth::authenticated_session_id(&principal) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: GoogleDriveOAuthStartDto = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(service) = state.storage_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(oauth) = service.google_oauth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match oauth
        .begin(session_id, LibraryId::from_uuid(request.target_library_id))
        .await
    {
        Ok((state, authorization_url)) => Json(GoogleDriveOAuthStartResponse {
            state,
            authorization_url,
        })
        .into_response(),
        Err(error) => oauth_response(&error),
    }
}

pub(crate) async fn google_drive_oauth_callback(
    State(state): State<AppState>,
    Query(query): Query<GoogleDriveCallbackQuery>,
) -> Response {
    if query.error.is_some() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(code) = query.code else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(service) = state.storage_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(oauth) = service.google_oauth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match oauth.complete_callback(query.state, code).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => oauth_response(&error),
    }
}

pub(crate) async fn google_drive_directories(
    State(state): State<AppState>,
    Path(oauth_state): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    Query(query): Query<GoogleDriveDirectoryQuery>,
) -> Response {
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let session_id = match auth::authenticated_session_id(&principal) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };
    let Some(service) = state.storage_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(oauth) = service.google_oauth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let scope = match google_scope(query.scope, query.shared_drive_id.as_deref()) {
        Ok(scope) => scope,
        Err(error) => return oauth_response(&error),
    };
    let parent_id = query.parent_id.unwrap_or_else(|| match &scope {
        GoogleDriveScope::MyDrive => "root".to_owned(),
        GoogleDriveScope::SharedDrive(id) => id.clone(),
    });
    let parent = match StorageObjectId::new("google-drive", parent_id) {
        Ok(parent) => parent,
        Err(error) => return oauth_response(&StorageAdminError::Backend(error)),
    };
    let context = GoogleDirectoryPageContext {
        scope: scope.clone(),
        parent_id: parent.provider_object_id().to_owned(),
    };
    let (credentials, provider_page) = match oauth
        .prepare_directory_page(oauth_state, session_id, &context, query.page_token)
        .await
    {
        Ok(value) => value,
        Err(error) => return oauth_response(&error),
    };
    let backend = match oauth.google_backend(credentials, scope) {
        Ok(backend) => backend,
        Err(error) => return oauth_response(&StorageAdminError::Backend(error)),
    };
    match backend.list_children(&parent, provider_page).await {
        Ok(page) => {
            let tjxy_storage::ObjectPage { objects, next_page } = page;
            let next_page_token = match oauth
                .register_directory_page(oauth_state, session_id, context, next_page)
                .await
            {
                Ok(cursor) => cursor,
                Err(error) => return oauth_response(&error),
            };
            Json(GoogleDriveDirectoryResponse {
                items: objects
                    .into_iter()
                    .filter(|object| object.object_type() == ObjectType::Directory)
                    .map(|object| GoogleDriveDirectoryDto {
                        id: object.id().provider_object_id().to_owned(),
                        name: object.name().to_owned(),
                    })
                    .collect(),
                next_page_token,
            })
            .into_response()
        }
        Err(error) => oauth_response(&StorageAdminError::Backend(error)),
    }
}

pub(crate) async fn google_shared_drives(
    State(state): State<AppState>,
    Path(oauth_state): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    Query(query): Query<GoogleSharedDriveQuery>,
) -> Response {
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let session_id = match auth::authenticated_session_id(&principal) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };
    let page = match query.page_token.map(PageToken::new).transpose() {
        Ok(page) => page,
        Err(error) => return oauth_response(&StorageAdminError::Backend(error)),
    };
    let Some(service) = state.storage_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(oauth) = service.google_oauth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let credentials = match oauth
        .with_authorized_session(oauth_state, session_id, |credentials, _, _| {
            credentials.clone()
        })
        .await
    {
        Ok(credentials) => credentials,
        Err(error) => return oauth_response(&error),
    };
    let backend = match oauth.google_backend(credentials, GoogleDriveScope::MyDrive) {
        Ok(backend) => backend,
        Err(error) => return oauth_response(&StorageAdminError::Backend(error)),
    };
    match backend.list_shared_drives(page).await {
        Ok(page) => Json(GoogleSharedDriveResponse {
            items: page
                .drives()
                .iter()
                .map(|drive| GoogleDriveDirectoryDto {
                    id: drive.id().to_owned(),
                    name: drive.name().to_owned(),
                })
                .collect(),
            next_page_token: page.next_page().map(|page| page.as_str().to_owned()),
        })
        .into_response(),
        Err(error) => oauth_response(&StorageAdminError::Backend(error)),
    }
}

pub(crate) async fn bind_google_drive(
    State(state): State<AppState>,
    Path(oauth_state): Path<Uuid>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
    body: Bytes,
) -> Response {
    let principal =
        match auth::authenticated_administrator(&state, &headers, raw_query.as_deref()).await {
            Ok(principal) => principal,
            Err(response) => return response,
        };
    let session_id = match auth::authenticated_session_id(&principal) {
        Ok(session_id) => session_id,
        Err(response) => return response,
    };
    if !auth::is_json_content_type(&headers) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let request: GoogleDriveBindingDto = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if google_scope(request.scope, request.shared_drive_id.as_deref()).is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(service) = state.storage_admin.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(oauth) = service.google_oauth.as_ref() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let (credentials, account_identity, target_library_id) =
        match oauth.take_authorized_session(oauth_state, session_id).await {
            Ok(value) => value,
            Err(error) => return oauth_response(&error),
        };
    let request = GoogleDriveBindingRequest {
        target_library_id,
        scope: request.scope,
        display_name: request.display_name,
        account_identity,
        shared_drive_id: request.shared_drive_id,
        root_object_id: request.root_object_id,
    };
    let result = service.bind_google_drive(request, credentials, oauth).await;
    binding_response(&result)
}

fn google_scope(
    scope: GoogleScope,
    shared_drive_id: Option<&str>,
) -> Result<GoogleDriveScope, StorageAdminError> {
    match (scope, shared_drive_id) {
        (GoogleScope::MyDrive, None) => Ok(GoogleDriveScope::MyDrive),
        (GoogleScope::SharedDrive, Some(id)) if !id.is_empty() => {
            Ok(GoogleDriveScope::SharedDrive(id.to_owned()))
        }
        (GoogleScope::MyDrive, Some(_)) | (GoogleScope::SharedDrive, _) => {
            Err(StorageAdminError::InvalidRequest)
        }
    }
}

fn binding_response(result: &Result<CreatedStorageBinding, StorageAdminError>) -> Response {
    match result {
        Ok(binding) => (
            StatusCode::CREATED,
            Json(StorageBindingDto {
                account_id: binding.account_id(),
                root_id: binding.root_id(),
                initial_sync_job_id: binding.initial_sync_job_id().as_uuid(),
                restart_required: false,
            }),
        )
            .into_response(),
        Err(
            StorageAdminError::InvalidRequest
            | StorageAdminError::Backend(
                BackendError::UnsupportedCapability { .. }
                | BackendError::InvalidValue { .. }
                | BackendError::NotFound
                | BackendError::RangeNotSatisfiable { .. },
            ),
        ) => StatusCode::BAD_REQUEST.into_response(),
        Err(StorageAdminError::Backend(
            BackendError::TemporarilyUnavailable { .. }
            | BackendError::RateLimited { .. }
            | BackendError::ChangeCursorInvalid,
        )) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        Err(StorageAdminError::Repository(_) | StorageAdminError::Conflict) => {
            StatusCode::CONFLICT.into_response()
        }
        Err(StorageAdminError::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(
            StorageAdminError::Cipher(_)
            | StorageAdminError::Runtime(_)
            | StorageAdminError::Account(_),
        ) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn oauth_response(error: &StorageAdminError) -> Response {
    match error {
        StorageAdminError::InvalidRequest
        | StorageAdminError::Backend(
            BackendError::UnsupportedCapability { .. }
            | BackendError::InvalidValue { .. }
            | BackendError::NotFound
            | BackendError::RangeNotSatisfiable { .. },
        ) => StatusCode::BAD_REQUEST.into_response(),
        StorageAdminError::Forbidden => StatusCode::FORBIDDEN.into_response(),
        StorageAdminError::Conflict | StorageAdminError::Repository(_) => {
            StatusCode::CONFLICT.into_response()
        }
        StorageAdminError::Backend(
            BackendError::TemporarilyUnavailable { .. }
            | BackendError::RateLimited { .. }
            | BackendError::ChangeCursorInvalid,
        ) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        StorageAdminError::Cipher(_)
        | StorageAdminError::Runtime(_)
        | StorageAdminError::Account(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct StorageBindingDto {
    account_id: Uuid,
    root_id: Uuid,
    initial_sync_job_id: Uuid,
    restart_required: bool,
}

#[derive(Debug, Error)]
enum StorageAdminError {
    #[error("storage binding request is invalid")]
    InvalidRequest,
    #[error("storage OAuth session does not belong to the authenticated administrator")]
    Forbidden,
    #[error("storage OAuth callback has not completed")]
    Conflict,
    #[error("storage provider validation failed: {0}")]
    Backend(#[from] BackendError),
    #[error("storage credential encryption failed: {0}")]
    Cipher(#[from] CredentialCipherError),
    #[error("storage binding persistence failed: {0}")]
    Repository(#[from] StorageBindingRepositoryError),
    #[error("storage account status update failed: {0}")]
    Account(#[from] StorageAccountRepositoryError),
    #[error("storage backend runtime activation failed: {0}")]
    Runtime(#[from] crate::runtime_storage::RuntimeStorageError),
}
