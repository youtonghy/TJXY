use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, Order, Query},
};
use serde_json::Value;
use thiserror::Error;
use tjxy_common::UserId;
use tjxy_credentials::{CredentialEnvelope, SealedCredential};
use uuid::Uuid;

pub const AI_PROVIDER_KEY: &str = "openai-compatible";
const MAX_MODELS: usize = 64;
const MAX_CONVERSATION_MESSAGES: u64 = 200;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AiReasoningEffort {
    #[default]
    Off,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl AiReasoningEffort {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "xhigh" => Some(Self::Xhigh),
            "max" => Some(Self::Max),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiModelInput {
    id: Uuid,
    upstream_id: String,
    display_name: String,
    reasoning_effort: AiReasoningEffort,
    is_visible: bool,
    is_default: bool,
    sort_order: i32,
}

impl AiModelInput {
    #[must_use]
    pub fn new(
        id: Uuid,
        upstream_id: impl Into<String>,
        display_name: impl Into<String>,
        is_visible: bool,
        is_default: bool,
        sort_order: i32,
    ) -> Self {
        Self {
            id,
            upstream_id: upstream_id.into(),
            display_name: display_name.into(),
            reasoning_effort: AiReasoningEffort::Off,
            is_visible,
            is_default,
            sort_order,
        }
    }
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub fn upstream_id(&self) -> &str {
        &self.upstream_id
    }
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    #[must_use]
    pub const fn reasoning_effort(&self) -> AiReasoningEffort {
        self.reasoning_effort
    }
    #[must_use]
    pub const fn with_reasoning_effort(mut self, reasoning_effort: AiReasoningEffort) -> Self {
        self.reasoning_effort = reasoning_effort;
        self
    }
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        self.is_visible
    }
    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.is_default
    }
    #[must_use]
    pub const fn sort_order(&self) -> i32 {
        self.sort_order
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiModelRecord(AiModelInput);

impl AiModelRecord {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.0.id()
    }
    #[must_use]
    pub fn upstream_id(&self) -> &str {
        self.0.upstream_id()
    }
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.0.display_name()
    }
    #[must_use]
    pub const fn reasoning_effort(&self) -> AiReasoningEffort {
        self.0.reasoning_effort()
    }
    #[must_use]
    pub const fn is_visible(&self) -> bool {
        self.0.is_visible()
    }
    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.0.is_default()
    }
    #[must_use]
    pub const fn sort_order(&self) -> i32 {
        self.0.sort_order()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiSettingsRecord {
    provider: String,
    enabled: bool,
    base_url: String,
    system_prompt: String,
    credential_id: Uuid,
    envelope: CredentialEnvelope,
    revision: i64,
    updated_at: DateTime<Utc>,
    models: Vec<AiModelRecord>,
}

impl AiSettingsRecord {
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    #[must_use]
    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }
    #[must_use]
    pub const fn credential_id(&self) -> Uuid {
        self.credential_id
    }
    #[must_use]
    pub const fn envelope(&self) -> &CredentialEnvelope {
        &self.envelope
    }
    #[must_use]
    pub const fn revision(&self) -> i64 {
        self.revision
    }
    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    #[must_use]
    pub fn models(&self) -> &[AiModelRecord] {
        &self.models
    }
    #[must_use]
    pub fn visible_models(&self) -> Vec<&AiModelRecord> {
        self.models
            .iter()
            .filter(|model| model.is_visible())
            .collect()
    }
    #[must_use]
    pub fn default_model(&self) -> Option<&AiModelRecord> {
        self.models
            .iter()
            .find(|model| model.is_visible() && model.is_default())
    }
    #[must_use]
    pub fn model(&self, id: Uuid) -> Option<&AiModelRecord> {
        self.models.iter().find(|model| model.id() == id)
    }
}

pub struct AiSettingsRepository<'a> {
    database: &'a DatabaseConnection,
}

impl<'a> AiSettingsRepository<'a> {
    #[must_use]
    pub const fn new(database: &'a DatabaseConnection) -> Self {
        Self { database }
    }

    /// Loads encrypted AI provider settings without decrypting the key.
    ///
    /// # Errors
    /// Returns malformed-envelope or database errors.
    pub async fn get(&self) -> Result<Option<AiSettingsRecord>, AiSettingsRepositoryError> {
        get_settings(self.database).await
    }

    /// Atomically replaces AI provider settings and its model catalog.
    ///
    /// # Errors
    /// Returns validation, revision, credential identity, or database errors.
    pub async fn put(
        &self,
        sealed: &SealedCredential,
        enabled: bool,
        base_url: &str,
        system_prompt: &str,
        models: &[AiModelInput],
        expected_revision: Option<i64>,
    ) -> Result<AiSettingsRecord, AiSettingsRepositoryError> {
        validate_settings(sealed, base_url, system_prompt, models, expected_revision)?;
        let transaction = self.database.begin().await?;
        let result = put_settings(
            &transaction,
            sealed,
            enabled,
            base_url,
            system_prompt,
            models,
            expected_revision,
        )
        .await;
        finish_settings(transaction, result).await
    }

    /// Deletes settings using an optional revision fence.
    ///
    /// # Errors
    /// Returns revision or database errors.
    pub async fn delete(
        &self,
        expected_revision: Option<i64>,
    ) -> Result<bool, AiSettingsRepositoryError> {
        if expected_revision.is_some_and(|revision| revision <= 0) {
            return Err(AiSettingsRepositoryError::InvalidRevision);
        }
        let mut statement = Query::delete();
        statement
            .from_table(Alias::new("ai_provider_settings"))
            .and_where(Expr::col(Alias::new("provider")).eq(AI_PROVIDER_KEY));
        if let Some(revision) = expected_revision {
            statement.and_where(Expr::col(Alias::new("revision")).eq(revision));
        }
        let affected = self
            .database
            .execute(self.database.get_database_backend().build(&statement))
            .await?
            .rows_affected();
        if affected == 1 || expected_revision.is_none() {
            Ok(affected == 1)
        } else {
            Err(AiSettingsRepositoryError::RevisionConflict)
        }
    }
}

#[derive(Debug, Error)]
pub enum AiSettingsRepositoryError {
    #[error("AI provider setting is invalid")]
    InvalidSettings,
    #[error("AI model settings are invalid")]
    InvalidModels,
    #[error("AI provider settings revision is invalid")]
    InvalidRevision,
    #[error("AI provider settings changed since they were read")]
    RevisionConflict,
    #[error("AI provider credential identity changed during rotation")]
    CredentialIdentityConflict,
    #[error("stored AI provider credential envelope is malformed")]
    InvalidStoredEnvelope,
    #[error("AI provider settings database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("AI provider settings rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

#[derive(Clone, Debug, PartialEq)]
pub struct AiMessageRecord {
    id: Uuid,
    role: String,
    content: String,
    metadata: Value,
    created_at: DateTime<Utc>,
}

impl AiMessageRecord {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub fn role(&self) -> &str {
        &self.role
    }
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
    #[must_use]
    pub const fn metadata(&self) -> &Value {
        &self.metadata
    }
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AiConversationRecord {
    id: Uuid,
    user_id: UserId,
    model_id: Uuid,
    title: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    messages: Vec<AiMessageRecord>,
}

impl AiConversationRecord {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
    #[must_use]
    pub const fn model_id(&self) -> Uuid {
        self.model_id
    }
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    #[must_use]
    pub fn messages(&self) -> &[AiMessageRecord] {
        &self.messages
    }
}

pub struct AiConversationRepository<'a> {
    database: &'a DatabaseConnection,
}

impl<'a> AiConversationRepository<'a> {
    #[must_use]
    pub const fn new(database: &'a DatabaseConnection) -> Self {
        Self { database }
    }

    /// Creates one user-owned conversation and its first exchange atomically.
    ///
    /// # Errors
    /// Returns validation or database errors.
    pub async fn create_with_exchange(
        &self,
        user_id: UserId,
        model_id: Uuid,
        title: &str,
        user_content: &str,
        assistant_content: &str,
        metadata: &Value,
    ) -> Result<Uuid, AiConversationRepositoryError> {
        self.create_with_exchange_at(
            Uuid::new_v4(),
            user_id,
            model_id,
            title,
            user_content,
            assistant_content,
            metadata,
        )
        .await
    }

    /// Creates one user-owned conversation with a caller-provided id and its first exchange
    /// atomically.
    ///
    /// # Errors
    /// Returns validation or database errors.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_with_exchange_at(
        &self,
        id: Uuid,
        user_id: UserId,
        model_id: Uuid,
        title: &str,
        user_content: &str,
        assistant_content: &str,
        metadata: &Value,
    ) -> Result<Uuid, AiConversationRepositoryError> {
        if !valid_text(title, 160) {
            return Err(AiConversationRepositoryError::InvalidTitle);
        }
        let metadata_json = validate_exchange(user_content, assistant_content, metadata)?;
        let now = Utc::now();
        let transaction = self.database.begin().await?;
        let statement = Query::insert()
            .into_table(Alias::new("ai_conversations"))
            .columns([
                Alias::new("id"),
                Alias::new("user_id"),
                Alias::new("model_id"),
                Alias::new("title"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                id.into(),
                user_id.as_uuid().into(),
                model_id.into(),
                title.into(),
                now.into(),
                now.into(),
            ])
            .to_owned();
        let result = async {
            transaction
                .execute(transaction.get_database_backend().build(&statement))
                .await?;
            append_exchange(
                &transaction,
                user_id,
                id,
                user_content,
                assistant_content,
                &metadata_json,
            )
            .await?;
            Ok(id)
        }
        .await;
        finish_conversation(transaction, result).await
    }

    /// Lists recent conversations for one user.
    ///
    /// # Errors
    /// Returns invalid limit or database errors.
    pub async fn list(
        &self,
        user_id: UserId,
        limit: u64,
    ) -> Result<Vec<AiConversationRecord>, AiConversationRepositoryError> {
        if limit == 0 || limit > 100 {
            return Err(AiConversationRepositoryError::InvalidLimit);
        }
        let statement = conversation_select()
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
            .order_by(Alias::new("updated_at"), Order::Desc)
            .limit(limit)
            .to_owned();
        self.database
            .query_all(self.database.get_database_backend().build(&statement))
            .await?
            .iter()
            .map(conversation_from_row)
            .collect()
    }

    /// Loads one conversation only for its owner.
    ///
    /// # Errors
    /// Returns malformed metadata or database errors.
    pub async fn get(
        &self,
        user_id: UserId,
        conversation_id: Uuid,
    ) -> Result<Option<AiConversationRecord>, AiConversationRepositoryError> {
        let statement = conversation_select()
            .and_where(Expr::col(Alias::new("id")).eq(conversation_id))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
            .to_owned();
        let Some(row) = self
            .database
            .query_one(self.database.get_database_backend().build(&statement))
            .await?
        else {
            return Ok(None);
        };
        let mut conversation = conversation_from_row(&row)?;
        conversation.messages = load_messages(self.database, conversation_id).await?;
        Ok(Some(conversation))
    }

    /// Atomically appends a user and assistant message.
    ///
    /// # Errors
    /// Returns ownership, validation, metadata, or database errors.
    pub async fn append_exchange(
        &self,
        user_id: UserId,
        conversation_id: Uuid,
        user_content: &str,
        assistant_content: &str,
        metadata: &Value,
    ) -> Result<(), AiConversationRepositoryError> {
        let metadata_json = validate_exchange(user_content, assistant_content, metadata)?;
        let transaction = self.database.begin().await?;
        let result = append_exchange(
            &transaction,
            user_id,
            conversation_id,
            user_content,
            assistant_content,
            &metadata_json,
        )
        .await;
        finish_conversation(transaction, result).await
    }

    /// Deletes one conversation only for its owner.
    ///
    /// # Errors
    /// Returns database errors.
    pub async fn delete(
        &self,
        user_id: UserId,
        conversation_id: Uuid,
    ) -> Result<bool, AiConversationRepositoryError> {
        let statement = Query::delete()
            .from_table(Alias::new("ai_conversations"))
            .and_where(Expr::col(Alias::new("id")).eq(conversation_id))
            .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
            .to_owned();
        Ok(self
            .database
            .execute(self.database.get_database_backend().build(&statement))
            .await?
            .rows_affected()
            == 1)
    }
}

#[derive(Debug, Error)]
pub enum AiConversationRepositoryError {
    #[error("AI conversation title is invalid")]
    InvalidTitle,
    #[error("AI conversation message is invalid")]
    InvalidMessage,
    #[error("AI conversation metadata is invalid")]
    InvalidMetadata,
    #[error("AI conversation page size is invalid")]
    InvalidLimit,
    #[error("AI conversation was not found for this user")]
    NotFound,
    #[error("AI conversation database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("AI conversation rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

fn validate_settings(
    sealed: &SealedCredential,
    base_url: &str,
    prompt: &str,
    models: &[AiModelInput],
    revision: Option<i64>,
) -> Result<(), AiSettingsRepositoryError> {
    if sealed.provider() != AI_PROVIDER_KEY
        || !valid_text(base_url, 2_048)
        || !valid_text(prompt, 16_000)
    {
        return Err(AiSettingsRepositoryError::InvalidSettings);
    }
    if revision.is_some_and(|value| value <= 0) {
        return Err(AiSettingsRepositoryError::InvalidRevision);
    }
    if models.is_empty() || models.len() > MAX_MODELS {
        return Err(AiSettingsRepositoryError::InvalidModels);
    }
    let mut ids = HashSet::new();
    let mut upstream = HashSet::new();
    let mut default_count = 0;
    for model in models {
        if !ids.insert(model.id())
            || !upstream.insert(model.upstream_id())
            || !valid_text(model.upstream_id(), 255)
            || !valid_text(model.display_name(), 128)
            || model.sort_order() < 0
            || (model.is_default() && !model.is_visible())
        {
            return Err(AiSettingsRepositoryError::InvalidModels);
        }
        default_count += usize::from(model.is_default());
    }
    if default_count != 1 {
        return Err(AiSettingsRepositoryError::InvalidModels);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Keeps the revision-fenced settings and model replacement atomic.
async fn put_settings(
    transaction: &DatabaseTransaction,
    sealed: &SealedCredential,
    enabled: bool,
    base_url: &str,
    prompt: &str,
    models: &[AiModelInput],
    revision: Option<i64>,
) -> Result<AiSettingsRecord, AiSettingsRepositoryError> {
    let now = Utc::now();
    let backend = transaction.get_database_backend();
    match revision {
        None => {
            if get_settings(transaction).await?.is_some() {
                return Err(AiSettingsRepositoryError::RevisionConflict);
            }
            let statement = Query::insert()
                .into_table(Alias::new("ai_provider_settings"))
                .columns([
                    Alias::new("provider"),
                    Alias::new("enabled"),
                    Alias::new("base_url"),
                    Alias::new("system_prompt"),
                    Alias::new("credential_id"),
                    Alias::new("encrypted_payload"),
                    Alias::new("key_version"),
                    Alias::new("revision"),
                    Alias::new("created_at"),
                    Alias::new("updated_at"),
                ])
                .values_panic([
                    AI_PROVIDER_KEY.into(),
                    enabled.into(),
                    base_url.into(),
                    prompt.into(),
                    sealed.credential_id().into(),
                    sealed.envelope().payload().to_vec().into(),
                    sealed.envelope().key_version().into(),
                    1_i64.into(),
                    now.into(),
                    now.into(),
                ])
                .to_owned();
            transaction.execute(backend.build(&statement)).await?;
        }
        Some(current_revision) => {
            let current = get_settings(transaction)
                .await?
                .ok_or(AiSettingsRepositoryError::RevisionConflict)?;
            if current.revision() != current_revision {
                return Err(AiSettingsRepositoryError::RevisionConflict);
            }
            if current.credential_id() != sealed.credential_id() {
                return Err(AiSettingsRepositoryError::CredentialIdentityConflict);
            }
            let next = current_revision
                .checked_add(1)
                .ok_or(AiSettingsRepositoryError::InvalidRevision)?;
            let statement = Query::update()
                .table(Alias::new("ai_provider_settings"))
                .value(Alias::new("enabled"), enabled)
                .value(Alias::new("base_url"), base_url)
                .value(Alias::new("system_prompt"), prompt)
                .value(
                    Alias::new("encrypted_payload"),
                    sealed.envelope().payload().to_vec(),
                )
                .value(Alias::new("key_version"), sealed.envelope().key_version())
                .value(Alias::new("revision"), next)
                .value(Alias::new("updated_at"), now)
                .and_where(Expr::col(Alias::new("provider")).eq(AI_PROVIDER_KEY))
                .and_where(Expr::col(Alias::new("revision")).eq(current_revision))
                .to_owned();
            if transaction
                .execute(backend.build(&statement))
                .await?
                .rows_affected()
                != 1
            {
                return Err(AiSettingsRepositoryError::RevisionConflict);
            }
        }
    }
    transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("ai_models"))
                    .and_where(Expr::col(Alias::new("provider")).eq(AI_PROVIDER_KEY)),
            ),
        )
        .await?;
    for model in models {
        let statement = Query::insert()
            .into_table(Alias::new("ai_models"))
            .columns([
                Alias::new("id"),
                Alias::new("provider"),
                Alias::new("upstream_id"),
                Alias::new("display_name"),
                Alias::new("reasoning_effort"),
                Alias::new("is_visible"),
                Alias::new("is_default"),
                Alias::new("sort_order"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
            ])
            .values_panic([
                model.id().into(),
                AI_PROVIDER_KEY.into(),
                model.upstream_id().into(),
                model.display_name().into(),
                model.reasoning_effort().as_str().into(),
                model.is_visible().into(),
                model.is_default().into(),
                model.sort_order().into(),
                now.into(),
                now.into(),
            ])
            .to_owned();
        transaction.execute(backend.build(&statement)).await?;
    }
    get_settings(transaction)
        .await?
        .ok_or(AiSettingsRepositoryError::RevisionConflict)
}

async fn get_settings<C: ConnectionTrait>(
    connection: &C,
) -> Result<Option<AiSettingsRecord>, AiSettingsRepositoryError> {
    let statement = Query::select()
        .columns([
            Alias::new("provider"),
            Alias::new("enabled"),
            Alias::new("base_url"),
            Alias::new("system_prompt"),
            Alias::new("credential_id"),
            Alias::new("encrypted_payload"),
            Alias::new("key_version"),
            Alias::new("revision"),
            Alias::new("updated_at"),
        ])
        .from(Alias::new("ai_provider_settings"))
        .and_where(Expr::col(Alias::new("provider")).eq(AI_PROVIDER_KEY))
        .to_owned();
    let Some(row) = connection
        .query_one(connection.get_database_backend().build(&statement))
        .await?
    else {
        return Ok(None);
    };
    let envelope = CredentialEnvelope::from_parts(
        row.try_get("", "key_version")?,
        row.try_get("", "encrypted_payload")?,
    )
    .map_err(|_| AiSettingsRepositoryError::InvalidStoredEnvelope)?;
    Ok(Some(AiSettingsRecord {
        provider: row.try_get("", "provider")?,
        enabled: row.try_get("", "enabled")?,
        base_url: row.try_get("", "base_url")?,
        system_prompt: row.try_get("", "system_prompt")?,
        credential_id: row.try_get("", "credential_id")?,
        envelope,
        revision: row.try_get("", "revision")?,
        updated_at: row.try_get("", "updated_at")?,
        models: load_models(connection).await?,
    }))
}

async fn load_models<C: ConnectionTrait>(
    connection: &C,
) -> Result<Vec<AiModelRecord>, AiSettingsRepositoryError> {
    let statement = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("upstream_id"),
            Alias::new("display_name"),
            Alias::new("reasoning_effort"),
            Alias::new("is_visible"),
            Alias::new("is_default"),
            Alias::new("sort_order"),
        ])
        .from(Alias::new("ai_models"))
        .and_where(Expr::col(Alias::new("provider")).eq(AI_PROVIDER_KEY))
        .order_by(Alias::new("sort_order"), Order::Asc)
        .to_owned();
    connection
        .query_all(connection.get_database_backend().build(&statement))
        .await?
        .iter()
        .map(|row| {
            let reasoning_effort = row.try_get::<String>("", "reasoning_effort")?;
            Ok(AiModelRecord(AiModelInput {
                id: row.try_get("", "id")?,
                upstream_id: row.try_get("", "upstream_id")?,
                display_name: row.try_get("", "display_name")?,
                reasoning_effort: AiReasoningEffort::parse(&reasoning_effort)
                    .ok_or(AiSettingsRepositoryError::InvalidModels)?,
                is_visible: row.try_get("", "is_visible")?,
                is_default: row.try_get("", "is_default")?,
                sort_order: row.try_get("", "sort_order")?,
            }))
        })
        .collect()
}

fn conversation_select() -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("user_id"),
            Alias::new("model_id"),
            Alias::new("title"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
        ])
        .from(Alias::new("ai_conversations"))
        .to_owned()
}

fn conversation_from_row(
    row: &QueryResult,
) -> Result<AiConversationRecord, AiConversationRepositoryError> {
    Ok(AiConversationRecord {
        id: row.try_get("", "id")?,
        user_id: UserId::from_uuid(row.try_get::<Uuid>("", "user_id")?),
        model_id: row.try_get("", "model_id")?,
        title: row.try_get("", "title")?,
        created_at: row.try_get("", "created_at")?,
        updated_at: row.try_get("", "updated_at")?,
        messages: Vec::new(),
    })
}

async fn append_exchange(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    conversation_id: Uuid,
    user_content: &str,
    assistant_content: &str,
    metadata_json: &str,
) -> Result<(), AiConversationRepositoryError> {
    let owned = Query::select()
        .expr(Expr::val(1_i32))
        .from(Alias::new("ai_conversations"))
        .and_where(Expr::col(Alias::new("id")).eq(conversation_id))
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
        .limit(1)
        .to_owned();
    if transaction
        .query_one(transaction.get_database_backend().build(&owned))
        .await?
        .is_none()
    {
        return Err(AiConversationRepositoryError::NotFound);
    }
    let now = Utc::now();
    for (role, content, metadata, created_at) in [
        ("user", user_content, "{}", now),
        (
            "assistant",
            assistant_content,
            metadata_json,
            now + Duration::microseconds(1),
        ),
    ] {
        let statement = Query::insert()
            .into_table(Alias::new("ai_messages"))
            .columns([
                Alias::new("id"),
                Alias::new("conversation_id"),
                Alias::new("role"),
                Alias::new("content"),
                Alias::new("metadata_json"),
                Alias::new("created_at"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                conversation_id.into(),
                role.into(),
                content.into(),
                metadata.into(),
                created_at.into(),
            ])
            .to_owned();
        transaction
            .execute(transaction.get_database_backend().build(&statement))
            .await?;
    }
    let statement = Query::update()
        .table(Alias::new("ai_conversations"))
        .value(Alias::new("updated_at"), now)
        .and_where(Expr::col(Alias::new("id")).eq(conversation_id))
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id.as_uuid()))
        .to_owned();
    if transaction
        .execute(transaction.get_database_backend().build(&statement))
        .await?
        .rows_affected()
        != 1
    {
        return Err(AiConversationRepositoryError::NotFound);
    }
    Ok(())
}

async fn load_messages<C: ConnectionTrait>(
    connection: &C,
    conversation_id: Uuid,
) -> Result<Vec<AiMessageRecord>, AiConversationRepositoryError> {
    let statement = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("role"),
            Alias::new("content"),
            Alias::new("metadata_json"),
            Alias::new("created_at"),
        ])
        .from(Alias::new("ai_messages"))
        .and_where(Expr::col(Alias::new("conversation_id")).eq(conversation_id))
        .order_by(Alias::new("created_at"), Order::Desc)
        .limit(MAX_CONVERSATION_MESSAGES)
        .to_owned();
    let mut messages = connection
        .query_all(connection.get_database_backend().build(&statement))
        .await?
        .iter()
        .map(|row| {
            let metadata: String = row.try_get("", "metadata_json")?;
            Ok(AiMessageRecord {
                id: row.try_get("", "id")?,
                role: row.try_get("", "role")?,
                content: row.try_get("", "content")?,
                metadata: serde_json::from_str(&metadata)
                    .map_err(|_| AiConversationRepositoryError::InvalidMetadata)?,
                created_at: row.try_get("", "created_at")?,
            })
        })
        .collect::<Result<Vec<_>, AiConversationRepositoryError>>()?;
    messages.reverse();
    Ok(messages)
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(|character| {
            let code = u32::from(character);
            code < 0x20 && character != '\n' && character != '\t'
        })
}

fn validate_exchange(
    user_content: &str,
    assistant_content: &str,
    metadata: &Value,
) -> Result<String, AiConversationRepositoryError> {
    if !valid_text(user_content, 32_000) || !valid_text(assistant_content, 32_000) {
        return Err(AiConversationRepositoryError::InvalidMessage);
    }
    let metadata_json = serde_json::to_string(metadata)
        .map_err(|_| AiConversationRepositoryError::InvalidMetadata)?;
    if metadata_json.len() > 64 * 1024 {
        return Err(AiConversationRepositoryError::InvalidMetadata);
    }
    Ok(metadata_json)
}

async fn finish_settings<T>(
    transaction: DatabaseTransaction,
    result: Result<T, AiSettingsRepositoryError>,
) -> Result<T, AiSettingsRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(rollback) => Err(AiSettingsRepositoryError::RollbackFailed {
                original: error.to_string(),
                rollback,
            }),
        },
    }
}
async fn finish_conversation<T>(
    transaction: DatabaseTransaction,
    result: Result<T, AiConversationRepositoryError>,
) -> Result<T, AiConversationRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(error) => match transaction.rollback().await {
            Ok(()) => Err(error),
            Err(rollback) => Err(AiConversationRepositoryError::RollbackFailed {
                original: error.to_string(),
                rollback,
            }),
        },
    }
}
