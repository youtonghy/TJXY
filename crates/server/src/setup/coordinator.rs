use base64::{Engine as _, engine::general_purpose::STANDARD};
use sea_orm::{ConnectionTrait, DbBackend};
use std::sync::Arc;
use tjxy_application::{AuthService, SystemClock};
use tjxy_db::{
    InstallationRepository, SystemSettingsInput, SystemSettingsRepository, migrate_database,
};
use tokio::sync::{Mutex, Notify, broadcast};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    DatabaseDraft, InstallationConfigStore, InstallationProfile, NetworkConfiguration,
    PendingInstallation, SecretString, SetupError, SetupErrorCode, SetupValidator,
};

#[derive(Clone, Debug)]
pub struct CompleteSetupInput {
    site_title: String,
    site_subtitle: String,
    locale: String,
    logo_url: String,
    icon_url: String,
    database: DatabaseDraft,
    network: NetworkConfiguration,
    administrator_username: String,
    administrator_password: SecretString,
    installation_id: Option<Uuid>,
}

impl CompleteSetupInput {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        site_title: impl Into<String>,
        site_subtitle: impl Into<String>,
        locale: impl Into<String>,
        logo_url: impl Into<String>,
        icon_url: impl Into<String>,
        database: DatabaseDraft,
        network: NetworkConfiguration,
        administrator_username: impl Into<String>,
        administrator_password: impl Into<String>,
    ) -> Self {
        Self {
            site_title: site_title.into(),
            site_subtitle: site_subtitle.into(),
            locale: locale.into(),
            logo_url: logo_url.into(),
            icon_url: icon_url.into(),
            database,
            network,
            administrator_username: administrator_username.into(),
            administrator_password: SecretString::new(administrator_password),
            installation_id: None,
        }
    }

    #[must_use]
    pub fn with_installation_id(mut self, installation_id: Uuid) -> Self {
        self.installation_id = Some(installation_id);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupCompletion {
    destination_url: String,
}

impl SetupCompletion {
    #[must_use]
    pub fn destination_url(&self) -> &str {
        &self.destination_url
    }
}

#[derive(Clone, Debug)]
pub struct SetupCoordinator {
    config: InstallationConfigStore,
    validator: SetupValidator,
    completed: Arc<Notify>,
    active_installation: Arc<Mutex<Option<Uuid>>>,
    progress: broadcast::Sender<SetupProgress>,
    latest_progress: Arc<std::sync::Mutex<Option<SetupProgress>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupState {
    Unconfigured,
    Pending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetupStatus {
    state: SetupState,
    installation_id: Uuid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupProgressStage {
    Preparing,
    ConnectingDatabase,
    MigratingDatabase,
    CreatingAdministrator,
    SavingSettings,
    CompletingInstallation,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetupProgress {
    pub installation_id: Uuid,
    pub stage: SetupProgressStage,
}

impl SetupStatus {
    #[must_use]
    pub const fn state(self) -> SetupState {
        self.state
    }
    #[must_use]
    pub const fn installation_id(self) -> Uuid {
        self.installation_id
    }
}

impl SetupCoordinator {
    #[must_use]
    pub fn new(config: InstallationConfigStore, validator: SetupValidator) -> Self {
        let (progress, _) = broadcast::channel(32);
        Self {
            config,
            validator,
            completed: Arc::new(Notify::new()),
            active_installation: Arc::new(Mutex::new(None)),
            progress,
            latest_progress: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    #[must_use]
    pub fn subscribe_progress(&self) -> broadcast::Receiver<SetupProgress> {
        self.progress.subscribe()
    }

    #[must_use]
    pub fn configuration_writable(&self) -> bool {
        self.config.parent_is_writable()
    }

    #[must_use]
    pub fn latest_progress(&self, installation_id: Uuid) -> Option<SetupProgress> {
        self.latest_progress.lock().ok().and_then(|progress| {
            progress.filter(|update| update.installation_id == installation_id)
        })
    }

    fn emit_progress(&self, installation_id: Uuid, stage: SetupProgressStage) {
        let update = SetupProgress {
            installation_id,
            stage,
        };
        if let Ok(mut progress) = self.latest_progress.lock() {
            *progress = Some(update);
        }
        let _ = self.progress.send(update);
    }

    /// Returns the current setup state from the local installation manifest.
    ///
    /// # Errors
    ///
    /// Returns a stable configuration error when the manifest cannot be read.
    pub fn status(&self) -> Result<SetupStatus, SetupError> {
        match self
            .config
            .load()
            .map_err(|_| SetupError::new(SetupErrorCode::ConfigurationReadFailed))?
        {
            crate::InstallationState::Unconfigured => Ok(SetupStatus {
                state: SetupState::Unconfigured,
                installation_id: Uuid::new_v4(),
            }),
            crate::InstallationState::Pending(pending) => Ok(SetupStatus {
                state: SetupState::Pending,
                installation_id: pending.installation_id(),
            }),
            crate::InstallationState::Completed(_) => {
                Err(SetupError::new(SetupErrorCode::InstallationConflict))
            }
        }
    }

    pub async fn wait_until_completed(&self) {
        self.completed.notified().await;
    }

    /// Completes one new installation in dependency order.
    ///
    /// # Errors
    ///
    /// Returns a stable setup category and leaves completed state false after any partial failure.
    pub async fn complete(&self, input: CompleteSetupInput) -> Result<SetupCompletion, SetupError> {
        let installation_id = input.installation_id.unwrap_or_else(Uuid::new_v4);
        {
            let mut active = self.active_installation.lock().await;
            if active.is_some() {
                return Err(SetupError::new(SetupErrorCode::InstallationConflict));
            }
            *active = Some(installation_id);
        }
        let result = self.complete_inner(input, installation_id).await;
        if result.is_err() {
            self.emit_progress(installation_id, SetupProgressStage::Failed);
        }
        let mut active = self.active_installation.lock().await;
        if *active == Some(installation_id) {
            *active = None;
        }
        result
    }

    async fn complete_inner(
        &self,
        input: CompleteSetupInput,
        installation_id: Uuid,
    ) -> Result<SetupCompletion, SetupError> {
        let (_, database_configuration, _) =
            self.validator.prepare_configuration(&input.database)?;
        let server_id = Uuid::new_v4();
        let keyring = generated_keyring()?;
        let pending = PendingInstallation::new(
            installation_id,
            server_id,
            keyring,
            database_configuration,
            input.network.clone(),
            InstallationProfile::new(
                &input.site_title,
                &input.site_subtitle,
                &input.locale,
                &input.logo_url,
                &input.icon_url,
                &input.administrator_username,
            ),
        );
        self.config
            .write_pending(&pending)
            .map_err(|_| SetupError::new(SetupErrorCode::ConfigurationWriteFailed))?;
        self.resume_pending(pending, input.administrator_password.expose())
            .await
    }

    /// Resumes the exact pending installation after verifying its intended administrator.
    /// Resumes a pending installation after authenticating its original administrator.
    ///
    /// # Errors
    ///
    /// Returns a stable recovery or installation error without exposing database details.
    pub async fn recover(
        &self,
        installation_id: Uuid,
        administrator_username: &str,
        administrator_password: &str,
    ) -> Result<SetupCompletion, SetupError> {
        {
            let mut active = self.active_installation.lock().await;
            if active.is_some() {
                return Err(SetupError::new(SetupErrorCode::InstallationConflict));
            }
            *active = Some(installation_id);
        }
        let result = self
            .recover_inner(
                installation_id,
                administrator_username,
                administrator_password,
            )
            .await;
        if result.is_err() {
            self.emit_progress(installation_id, SetupProgressStage::Failed);
        }
        let mut active = self.active_installation.lock().await;
        if *active == Some(installation_id) {
            *active = None;
        }
        result
    }

    async fn recover_inner(
        &self,
        installation_id: Uuid,
        administrator_username: &str,
        administrator_password: &str,
    ) -> Result<SetupCompletion, SetupError> {
        let pending = match self
            .config
            .load()
            .map_err(|_| SetupError::new(SetupErrorCode::ConfigurationReadFailed))?
        {
            crate::InstallationState::Pending(pending)
                if pending.installation_id() == installation_id
                    && pending.profile().administrator_username() == administrator_username =>
            {
                pending
            }
            _ => {
                return Err(SetupError::new(
                    SetupErrorCode::RecoveryAuthenticationFailed,
                ));
            }
        };
        self.resume_pending(pending, administrator_password).await
    }

    #[allow(clippy::too_many_lines)]
    async fn resume_pending(
        &self,
        pending: PendingInstallation,
        administrator_password: &str,
    ) -> Result<SetupCompletion, SetupError> {
        let installation_id = pending.installation_id();
        self.emit_progress(installation_id, SetupProgressStage::ConnectingDatabase);
        let mut database = self
            .validator
            .connect_configuration(pending.database())
            .await?;
        self.emit_progress(
            pending.installation_id(),
            SetupProgressStage::MigratingDatabase,
        );
        migrate_database(&database)
            .await
            .map_err(|_| SetupError::new(SetupErrorCode::InstallationFailed))?;
        if database.get_database_backend() == DbBackend::MySql {
            database
                .close()
                .await
                .map_err(|_| SetupError::new(SetupErrorCode::InstallationFailed))?;
            database = self
                .validator
                .connect_configuration(pending.database())
                .await?;
        }
        let repository = InstallationRepository::new(&database);
        let mut installation = repository
            .begin(
                pending.installation_id(),
                pending.server_id(),
                chrono::Utc::now(),
            )
            .await
            .map_err(|_| SetupError::new(SetupErrorCode::InstallationConflict))?;
        let auth = AuthService::new(database.clone(), SystemClock, None, 2)
            .await
            .map_err(|_| SetupError::new(SetupErrorCode::InstallationFailed))?;
        if let Some(administrator_id) = installation.administrator_id() {
            let administrator = auth
                .verify_credentials(
                    pending.profile().administrator_username(),
                    administrator_password,
                )
                .await
                .map_err(|_| SetupError::new(SetupErrorCode::RecoveryAuthenticationFailed))?;
            if administrator.id() != administrator_id
                || !administrator.is_admin()
                || administrator.is_disabled()
            {
                return Err(SetupError::new(
                    SetupErrorCode::RecoveryAuthenticationFailed,
                ));
            }
        } else {
            self.emit_progress(
                pending.installation_id(),
                SetupProgressStage::CreatingAdministrator,
            );
            let administrator = auth
                .create_initial_admin(
                    pending.profile().administrator_username(),
                    administrator_password,
                )
                .await
                .map_err(|_| SetupError::new(SetupErrorCode::AdministratorInvalid))?
                .ok_or_else(|| SetupError::new(SetupErrorCode::AdministratorExists))?;
            installation = repository
                .attach_initial_admin(
                    pending.installation_id(),
                    administrator.id(),
                    installation.revision(),
                    chrono::Utc::now(),
                )
                .await
                .map_err(|_| SetupError::new(SetupErrorCode::InstallationConflict))?;
        }
        self.emit_progress(
            pending.installation_id(),
            SetupProgressStage::SavingSettings,
        );
        let settings = SystemSettingsRepository::new(&database);
        let expected_revision = settings
            .get()
            .await
            .map_err(|_| SetupError::new(SetupErrorCode::SystemSettingsInvalid))?
            .map(|settings| settings.revision());
        settings
            .put(
                &SystemSettingsInput {
                    media_browser_roots: Vec::new(),
                    locale: pending.profile().locale().to_owned(),
                    site_title: pending.profile().site_title().to_owned(),
                    site_subtitle: pending.profile().site_subtitle().to_owned(),
                    logo_url: pending.profile().logo_url().to_owned(),
                    icon_url: pending.profile().icon_url().to_owned(),
                    public_url: pending.network().public_url().map(str::to_owned),
                    listen_host: pending.network().listen_host().to_owned(),
                    port: pending.network().port(),
                    passkey_enabled: false,
                },
                expected_revision,
            )
            .await
            .map_err(|_| SetupError::new(SetupErrorCode::SystemSettingsInvalid))?;
        self.emit_progress(
            pending.installation_id(),
            SetupProgressStage::CompletingInstallation,
        );
        repository
            .complete(
                pending.installation_id(),
                installation.revision(),
                chrono::Utc::now(),
            )
            .await
            .map_err(|_| SetupError::new(SetupErrorCode::InstallationConflict))?;
        database
            .close()
            .await
            .map_err(|_| SetupError::new(SetupErrorCode::InstallationFailed))?;
        let destination_url = pending.network().admin_login_url();
        self.config
            .complete(&pending.complete())
            .map_err(|_| SetupError::new(SetupErrorCode::ConfigurationWriteFailed))?;
        self.emit_progress(installation_id, SetupProgressStage::Complete);
        self.completed.notify_waiters();
        Ok(SetupCompletion { destination_url })
    }
}

fn generated_keyring() -> Result<SecretString, SetupError> {
    let mut key = Zeroizing::new([0_u8; 32]);
    getrandom::fill(key.as_mut_slice())
        .map_err(|_| SetupError::new(SetupErrorCode::InstallationFailed))?;
    let encoded = Zeroizing::new(STANDARD.encode(key.as_slice()));
    Ok(SecretString::new(format!(
        r#"{{"active_version":1,"keys":{{"1":"{}"}}}}"#,
        encoded.as_str()
    )))
}
