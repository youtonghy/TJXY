use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
// Jellyfin clients may include legacy or client-specific fields alongside Username/Pw.
#[serde(rename_all = "PascalCase")]
pub struct AuthenticateUserByName {
    pub username: String,
    #[serde(rename = "Pw", default, deserialize_with = "null_as_default")]
    pub password: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct CreateUserByName {
    pub name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub password: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateUserName {
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateUserPassword {
    #[serde(rename = "NewPw", default, deserialize_with = "null_as_default")]
    pub new_password: String,
    #[serde(default)]
    pub reset_password: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateUserPolicy {
    pub is_administrator: bool,
    pub is_disabled: bool,
    #[serde(default)]
    pub authentication_provider_id: Option<String>,
    #[serde(default)]
    pub password_reset_provider_id: Option<String>,
}

fn null_as_default<'de, Deserializer>(
    deserializer: Deserializer,
) -> Result<String, Deserializer::Error>
where
    Deserializer: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticationResult {
    pub user: UserDto,
    pub session_info: SessionInfoDto,
    pub access_token: String,
    pub server_id: Uuid,
}

impl AuthenticationResult {
    #[must_use]
    pub fn new(
        user: UserDto,
        session_info: SessionInfoDto,
        access_token: impl Into<String>,
        server_id: Uuid,
    ) -> Self {
        Self {
            user,
            session_info,
            access_token: access_token.into(),
            server_id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserDto {
    pub name: String,
    pub server_id: Uuid,
    pub id: Uuid,
    pub has_password: bool,
    pub has_configured_password: bool,
    pub configuration: UserConfiguration,
    pub policy: UserPolicy,
}

impl UserDto {
    #[must_use]
    pub fn new(
        id: Uuid,
        name: impl Into<String>,
        server_id: Uuid,
        has_password: bool,
        policy: UserPolicy,
    ) -> Self {
        Self {
            name: name.into(),
            server_id,
            id,
            has_password,
            has_configured_password: has_password,
            configuration: UserConfiguration {},
            policy,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UserConfiguration {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)] // These booleans are part of the pinned client contract.
pub struct UserPolicy {
    pub is_administrator: bool,
    pub is_disabled: bool,
    pub enable_media_playback: bool,
    pub enable_audio_playback_transcoding: bool,
    pub enable_video_playback_transcoding: bool,
    pub enable_playback_remuxing: bool,
    pub authentication_provider_id: String,
    pub password_reset_provider_id: String,
}

impl UserPolicy {
    #[must_use]
    pub fn direct_play_only(is_administrator: bool) -> Self {
        Self {
            is_administrator,
            is_disabled: false,
            enable_media_playback: true,
            enable_audio_playback_transcoding: false,
            enable_video_playback_transcoding: false,
            enable_playback_remuxing: false,
            authentication_provider_id: "TJXY.LocalAuthentication".to_owned(),
            password_reset_provider_id: "TJXY.LocalPasswordReset".to_owned(),
        }
    }

    #[must_use]
    pub const fn with_disabled(mut self, is_disabled: bool) -> Self {
        self.is_disabled = is_disabled;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SessionInfoDto {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_name: String,
    pub client: String,
    pub device_id: String,
    pub device_name: String,
    pub application_version: String,
    pub server_id: Uuid,
    pub is_active: bool,
    pub playable_media_types: Vec<String>,
    pub supported_commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_media_control: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_remote_control: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<SessionCapabilitiesDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SessionCapabilitiesDto {
    pub playable_media_types: Vec<String>,
    pub supported_commands: Vec<String>,
    pub supports_media_control: bool,
    pub supports_persistent_identifier: bool,
}

impl SessionInfoDto {
    #[must_use]
    #[allow(clippy::too_many_arguments)] // The fields mirror Jellyfin's session identity tuple.
    pub fn active(
        id: Uuid,
        user_id: Uuid,
        user_name: impl Into<String>,
        client: impl Into<String>,
        device_id: impl Into<String>,
        device_name: impl Into<String>,
        application_version: impl Into<String>,
        server_id: Uuid,
    ) -> Self {
        Self {
            id,
            user_id,
            user_name: user_name.into(),
            client: client.into(),
            device_id: device_id.into(),
            device_name: device_name.into(),
            application_version: application_version.into(),
            server_id,
            is_active: true,
            playable_media_types: Vec::new(),
            supported_commands: Vec::new(),
            last_activity_date: None,
            supports_media_control: None,
            supports_remote_control: None,
            capabilities: None,
        }
    }

    #[must_use]
    #[allow(clippy::too_many_arguments)] // Mirrors Jellyfin's session identity and capability tuple.
    pub fn listed(
        id: Uuid,
        user_id: Uuid,
        user_name: impl Into<String>,
        client: impl Into<String>,
        device_id: impl Into<String>,
        device_name: impl Into<String>,
        application_version: impl Into<String>,
        server_id: Uuid,
        last_activity_date: DateTime<Utc>,
        playable_media_types: Vec<String>,
        supported_commands: Vec<String>,
        supports_media_control: bool,
        supports_persistent_identifier: bool,
    ) -> Self {
        Self {
            id,
            user_id,
            user_name: user_name.into(),
            client: client.into(),
            device_id: device_id.into(),
            device_name: device_name.into(),
            application_version: application_version.into(),
            server_id,
            is_active: true,
            playable_media_types: playable_media_types.clone(),
            supported_commands: supported_commands.clone(),
            last_activity_date: Some(last_activity_date),
            supports_media_control: Some(supports_media_control),
            supports_remote_control: Some(supports_media_control),
            capabilities: Some(SessionCapabilitiesDto {
                playable_media_types,
                supported_commands,
                supports_media_control,
                supports_persistent_identifier,
            }),
        }
    }
}
