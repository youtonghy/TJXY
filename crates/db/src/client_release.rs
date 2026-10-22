use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, SqlErr,
    TransactionTrait,
    sea_query::{Alias, Expr, Order, Query, SelectStatement},
};
use thiserror::Error;
use tjxy_common::UserId;
use uuid::Uuid;

const MAX_VERSION_CHARS: usize = 64;
const MAX_FILE_NAME_CHARS: usize = 200;
const MAX_NOTES_CHARS: usize = 8_000;
const MAX_NOTES_BYTES: usize = 32 * 1024;

/// Operating system a client package installs on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientPlatform {
    Windows,
    Macos,
    Linux,
    Android,
}

impl ClientPlatform {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Linux => "linux",
            Self::Android => "android",
        }
    }

    /// Parses the wire and storage name of a platform.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "windows" => Some(Self::Windows),
            "macos" => Some(Self::Macos),
            "linux" => Some(Self::Linux),
            "android" => Some(Self::Android),
            _ => None,
        }
    }

    /// Lowercase file extensions accepted as installers for this platform.
    #[must_use]
    pub const fn package_extensions(self) -> &'static [&'static str] {
        match self {
            Self::Windows => &["exe", "msi"],
            Self::Macos => &["dmg", "zip"],
            Self::Linux => &["appimage", "deb", "rpm"],
            Self::Android => &["apk"],
        }
    }
}

/// CPU architecture of a client package. `Universal` packages serve every architecture of their
/// platform that has no dedicated release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientArch {
    X64,
    Arm64,
    Universal,
}

impl ClientArch {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
            Self::Universal => "universal",
        }
    }

    /// Parses the wire and storage name of an architecture.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "x64" => Some(Self::X64),
            "arm64" => Some(Self::Arm64),
            "universal" => Some(Self::Universal),
            _ => None,
        }
    }
}

/// A `SemVer` 2.0 version (`MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]`) compared by precedence, so
/// build metadata does not affect equality.
#[derive(Clone, Debug)]
pub struct ClientVersion {
    text: String,
    core: [u64; 3],
    prerelease: Vec<String>,
}

impl ClientVersion {
    /// Parses a `SemVer` string, accepting an optional leading `v`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let text = value.strip_prefix('v').unwrap_or(value);
        if text.is_empty() || text.chars().count() > MAX_VERSION_CHARS {
            return None;
        }
        let (without_build, build) = match text.split_once('+') {
            Some((head, build)) => (head, Some(build)),
            None => (text, None),
        };
        if build.is_some_and(|build| !valid_identifiers(build)) {
            return None;
        }
        let (core_text, prerelease) = match without_build.split_once('-') {
            Some((core, prerelease)) => (core, Some(prerelease)),
            None => (without_build, None),
        };
        if prerelease.is_some_and(|prerelease| !valid_identifiers(prerelease)) {
            return None;
        }
        let mut parts = core_text.split('.');
        let mut core = [0_u64; 3];
        for slot in &mut core {
            *slot = numeric_identifier(parts.next()?)?;
        }
        if parts.next().is_some() {
            return None;
        }
        let prerelease = match prerelease {
            Some(prerelease) => {
                let identifiers = prerelease.split('.').map(str::to_owned).collect::<Vec<_>>();
                if identifiers.iter().any(|identifier| {
                    identifier.len() > 1
                        && identifier.starts_with('0')
                        && identifier.bytes().all(|byte| byte.is_ascii_digit())
                }) {
                    return None;
                }
                identifiers
            }
            None => Vec::new(),
        };
        Some(Self {
            text: text.to_owned(),
            core,
            prerelease,
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl Ord for ClientVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core.cmp(&other.core).then_with(|| {
            match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => compare_prerelease(&self.prerelease, &other.prerelease),
            }
        })
    }
}

impl PartialEq for ClientVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for ClientVersion {}

impl PartialOrd for ClientVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn numeric_identifier(value: &str) -> Option<u64> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}

fn valid_identifiers(value: &str) -> bool {
    value.split('.').all(|identifier| {
        !identifier.is_empty()
            && identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn compare_prerelease(left: &[String], right: &[String]) -> Ordering {
    for (left, right) in left.iter().zip(right) {
        let ordering = match (left.parse::<u64>(), right.parse::<u64>()) {
            (Ok(left), Ok(right)) => left.cmp(&right),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => left.cmp(right),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

/// Validates an uploaded installer file name: a plain basename whose extension is an installer
/// format of `platform`. Returns the lowercase extension.
#[must_use]
pub fn client_package_extension(platform: ClientPlatform, file_name: &str) -> Option<String> {
    if file_name.is_empty()
        || file_name.chars().count() > MAX_FILE_NAME_CHARS
        || file_name.starts_with('.')
        || !file_name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '+' | ' ')
        })
    {
        return None;
    }
    let (_, extension) = file_name.rsplit_once('.')?;
    let extension = extension.to_ascii_lowercase();
    platform
        .package_extensions()
        .contains(&extension.as_str())
        .then_some(extension)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientReleaseInput {
    id: Uuid,
    platform: ClientPlatform,
    arch: ClientArch,
    version: ClientVersion,
    file_name: String,
    stored_name: String,
    size_bytes: u64,
    sha256: String,
    notes: String,
    created_by: Option<UserId>,
    mark_latest: bool,
}

impl ClientReleaseInput {
    /// Describes a package whose file has already been stored as `stored_name`.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        platform: ClientPlatform,
        arch: ClientArch,
        version: ClientVersion,
        file_name: impl Into<String>,
        stored_name: impl Into<String>,
        size_bytes: u64,
        sha256: impl Into<String>,
    ) -> Self {
        Self {
            id,
            platform,
            arch,
            version,
            file_name: file_name.into(),
            stored_name: stored_name.into(),
            size_bytes,
            sha256: sha256.into(),
            notes: String::new(),
            created_by: None,
            mark_latest: false,
        }
    }

    #[must_use]
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = notes.into();
        self
    }

    #[must_use]
    pub const fn with_creator(mut self, user_id: UserId) -> Self {
        self.created_by = Some(user_id);
        self
    }

    #[must_use]
    pub const fn marked_latest(mut self, mark_latest: bool) -> Self {
        self.mark_latest = mark_latest;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientReleaseRecord {
    id: Uuid,
    platform: ClientPlatform,
    arch: ClientArch,
    version: ClientVersion,
    file_name: String,
    stored_name: String,
    size_bytes: u64,
    sha256: String,
    notes: String,
    is_latest: bool,
    created_at: DateTime<Utc>,
}

impl ClientReleaseRecord {
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn platform(&self) -> ClientPlatform {
        self.platform
    }

    #[must_use]
    pub const fn arch(&self) -> ClientArch {
        self.arch
    }

    #[must_use]
    pub const fn version(&self) -> &ClientVersion {
        &self.version
    }

    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Name of the package file inside the release storage directory.
    #[must_use]
    pub fn stored_name(&self) -> &str {
        &self.stored_name
    }

    #[must_use]
    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn notes(&self) -> &str {
        &self.notes
    }

    #[must_use]
    pub const fn is_latest(&self) -> bool {
        self.is_latest
    }

    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

pub struct ClientReleaseRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> ClientReleaseRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Records an uploaded package, optionally making it the latest release of its target.
    ///
    /// # Errors
    ///
    /// Returns an input validation error for invalid values, a conflict when the target already
    /// has this version, or a database error when the transaction fails.
    pub async fn create(
        &self,
        input: &ClientReleaseInput,
    ) -> Result<ClientReleaseRecord, ClientReleaseRepositoryError> {
        validate_input(input)?;
        let transaction = self.database.begin().await?;
        let result = create_on(&transaction, input).await;
        finish(transaction, result).await
    }

    /// Lists every release, newest first within each platform and architecture.
    ///
    /// # Errors
    ///
    /// Returns a stored-value error for malformed persisted data or a database error when the
    /// query fails.
    pub async fn list(&self) -> Result<Vec<ClientReleaseRecord>, ClientReleaseRepositoryError> {
        let mut query = select_releases();
        query
            .order_by(Alias::new("platform"), Order::Asc)
            .order_by(Alias::new("arch"), Order::Asc)
            .order_by(Alias::new("created_at"), Order::Desc)
            .order_by(Alias::new("id"), Order::Desc);
        let rows = self
            .database
            .query_all(self.database.get_database_backend().build(&query))
            .await?;
        rows.iter().map(record_from_row).collect()
    }

    /// Loads one release.
    ///
    /// # Errors
    ///
    /// Returns [`ClientReleaseRepositoryError::NotFound`] for an unknown ID, a stored-value error
    /// for malformed persisted data, or a database error when the query fails.
    pub async fn get(&self, id: Uuid) -> Result<ClientReleaseRecord, ClientReleaseRepositoryError> {
        get_on(self.database, id)
            .await?
            .ok_or(ClientReleaseRepositoryError::NotFound)
    }

    /// Returns the release clients of `platform`/`arch` should run: the latest release built for
    /// that architecture, otherwise the latest universal package of the platform.
    ///
    /// # Errors
    ///
    /// Returns a stored-value error for malformed persisted data or a database error when the
    /// query fails.
    pub async fn latest_for(
        &self,
        platform: ClientPlatform,
        arch: ClientArch,
    ) -> Result<Option<ClientReleaseRecord>, ClientReleaseRepositoryError> {
        let mut query = select_releases();
        query
            .and_where(Expr::col(Alias::new("platform")).eq(platform.as_str()))
            .and_where(Expr::col(Alias::new("is_latest")).eq(true))
            .and_where(
                Expr::col(Alias::new("arch"))
                    .is_in([arch.as_str(), ClientArch::Universal.as_str()]),
            );
        let rows = self
            .database
            .query_all(self.database.get_database_backend().build(&query))
            .await?;
        let records = rows
            .iter()
            .map(record_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let dedicated = records
            .iter()
            .find(|record| record.arch == arch && arch != ClientArch::Universal);
        Ok(dedicated
            .or_else(|| {
                records
                    .iter()
                    .find(|record| record.arch == ClientArch::Universal)
            })
            .cloned())
    }

    /// Makes a release the latest of its platform and architecture, replacing the previous one.
    ///
    /// # Errors
    ///
    /// Returns [`ClientReleaseRepositoryError::NotFound`] for an unknown ID or a database error
    /// when the transaction fails.
    pub async fn mark_latest(
        &self,
        id: Uuid,
    ) -> Result<ClientReleaseRecord, ClientReleaseRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = mark_latest_on(&transaction, id).await;
        finish(transaction, result).await
    }

    /// Replaces the release notes of a release.
    ///
    /// # Errors
    ///
    /// Returns an input error for oversized notes, not found for an unknown ID, or a database
    /// error when the update fails.
    pub async fn update_notes(
        &self,
        id: Uuid,
        notes: &str,
    ) -> Result<ClientReleaseRecord, ClientReleaseRepositoryError> {
        validate_notes(notes)?;
        let statement = Query::update()
            .table(Alias::new("client_releases"))
            .value(Alias::new("notes"), notes)
            .and_where(Expr::col(Alias::new("id")).eq(id))
            .to_owned();
        let result = self
            .database
            .execute(self.database.get_database_backend().build(&statement))
            .await?;
        if result.rows_affected() == 0 {
            return Err(ClientReleaseRepositoryError::NotFound);
        }
        self.get(id).await
    }

    /// Deletes a release record and returns it so the caller can remove the stored file.
    ///
    /// # Errors
    ///
    /// Returns [`ClientReleaseRepositoryError::NotFound`] for an unknown ID or a database error
    /// when the transaction fails.
    pub async fn delete(
        &self,
        id: Uuid,
    ) -> Result<ClientReleaseRecord, ClientReleaseRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = delete_on(&transaction, id).await;
        finish(transaction, result).await
    }
}

async fn create_on(
    transaction: &DatabaseTransaction,
    input: &ClientReleaseInput,
) -> Result<ClientReleaseRecord, ClientReleaseRepositoryError> {
    if input.mark_latest {
        clear_latest_on(transaction, input.platform, input.arch).await?;
    }
    let size_bytes =
        i64::try_from(input.size_bytes).map_err(|_| ClientReleaseRepositoryError::InvalidInput)?;
    let statement = Query::insert()
        .into_table(Alias::new("client_releases"))
        .columns([
            Alias::new("id"),
            Alias::new("platform"),
            Alias::new("arch"),
            Alias::new("version"),
            Alias::new("file_name"),
            Alias::new("stored_name"),
            Alias::new("size_bytes"),
            Alias::new("sha256"),
            Alias::new("notes"),
            Alias::new("is_latest"),
            Alias::new("created_by"),
            Alias::new("created_at"),
        ])
        .values_panic([
            input.id.into(),
            input.platform.as_str().into(),
            input.arch.as_str().into(),
            input.version.as_str().into(),
            input.file_name.clone().into(),
            input.stored_name.clone().into(),
            size_bytes.into(),
            input.sha256.clone().into(),
            input.notes.clone().into(),
            input.mark_latest.into(),
            input.created_by.map(UserId::as_uuid).into(),
            Utc::now().into(),
        ])
        .to_owned();
    let backend = transaction.get_database_backend();
    let result = transaction.execute(backend.build(&statement)).await;
    if result
        .as_ref()
        .err()
        .and_then(DbErr::sql_err)
        .is_some_and(|error| matches!(error, SqlErr::UniqueConstraintViolation(_)))
    {
        return Err(ClientReleaseRepositoryError::VersionConflict);
    }
    result?;
    get_on(transaction, input.id)
        .await?
        .ok_or(ClientReleaseRepositoryError::NotFound)
}

async fn mark_latest_on(
    transaction: &DatabaseTransaction,
    id: Uuid,
) -> Result<ClientReleaseRecord, ClientReleaseRepositoryError> {
    let record = get_on(transaction, id)
        .await?
        .ok_or(ClientReleaseRepositoryError::NotFound)?;
    clear_latest_on(transaction, record.platform, record.arch).await?;
    let statement = Query::update()
        .table(Alias::new("client_releases"))
        .value(Alias::new("is_latest"), true)
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&statement))
        .await?;
    Ok(ClientReleaseRecord {
        is_latest: true,
        ..record
    })
}

async fn clear_latest_on(
    transaction: &DatabaseTransaction,
    platform: ClientPlatform,
    arch: ClientArch,
) -> Result<(), ClientReleaseRepositoryError> {
    let statement = Query::update()
        .table(Alias::new("client_releases"))
        .value(Alias::new("is_latest"), false)
        .and_where(Expr::col(Alias::new("platform")).eq(platform.as_str()))
        .and_where(Expr::col(Alias::new("arch")).eq(arch.as_str()))
        .and_where(Expr::col(Alias::new("is_latest")).eq(true))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&statement))
        .await?;
    Ok(())
}

async fn delete_on(
    transaction: &DatabaseTransaction,
    id: Uuid,
) -> Result<ClientReleaseRecord, ClientReleaseRepositoryError> {
    let record = get_on(transaction, id)
        .await?
        .ok_or(ClientReleaseRepositoryError::NotFound)?;
    let statement = Query::delete()
        .from_table(Alias::new("client_releases"))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&statement))
        .await?;
    Ok(record)
}

fn select_releases() -> SelectStatement {
    Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("platform"),
            Alias::new("arch"),
            Alias::new("version"),
            Alias::new("file_name"),
            Alias::new("stored_name"),
            Alias::new("size_bytes"),
            Alias::new("sha256"),
            Alias::new("notes"),
            Alias::new("is_latest"),
            Alias::new("created_at"),
        ])
        .from(Alias::new("client_releases"))
        .to_owned()
}

async fn get_on<C: ConnectionTrait>(
    connection: &C,
    id: Uuid,
) -> Result<Option<ClientReleaseRecord>, ClientReleaseRepositoryError> {
    let mut statement = select_releases();
    statement.and_where(Expr::col(Alias::new("id")).eq(id));
    let row = connection
        .query_one(connection.get_database_backend().build(&statement))
        .await?;
    row.map(|value| record_from_row(&value)).transpose()
}

fn record_from_row(row: &QueryResult) -> Result<ClientReleaseRecord, ClientReleaseRepositoryError> {
    let invalid = || ClientReleaseRepositoryError::InvalidStoredValue;
    let size_bytes: i64 = row.try_get("", "size_bytes")?;
    Ok(ClientReleaseRecord {
        id: row.try_get("", "id")?,
        platform: ClientPlatform::parse(&row.try_get::<String>("", "platform")?)
            .ok_or_else(invalid)?,
        arch: ClientArch::parse(&row.try_get::<String>("", "arch")?).ok_or_else(invalid)?,
        version: ClientVersion::parse(&row.try_get::<String>("", "version")?)
            .ok_or_else(invalid)?,
        file_name: row.try_get("", "file_name")?,
        stored_name: row.try_get("", "stored_name")?,
        size_bytes: u64::try_from(size_bytes).map_err(|_| invalid())?,
        sha256: row.try_get("", "sha256")?,
        notes: row.try_get("", "notes")?,
        is_latest: row.try_get("", "is_latest")?,
        created_at: row.try_get("", "created_at")?,
    })
}

fn validate_input(input: &ClientReleaseInput) -> Result<(), ClientReleaseRepositoryError> {
    let valid_hash = input.sha256.len() == 64
        && input
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if client_package_extension(input.platform, &input.file_name).is_none()
        || input.stored_name.is_empty()
        || input.stored_name.contains(['/', '\\'])
        || input.size_bytes == 0
        || !valid_hash
    {
        return Err(ClientReleaseRepositoryError::InvalidInput);
    }
    validate_notes(&input.notes)
}

fn validate_notes(notes: &str) -> Result<(), ClientReleaseRepositoryError> {
    if notes.len() > MAX_NOTES_BYTES || notes.chars().count() > MAX_NOTES_CHARS {
        return Err(ClientReleaseRepositoryError::InvalidInput);
    }
    Ok(())
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, ClientReleaseRepositoryError>,
) -> Result<T, ClientReleaseRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(ClientReleaseRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

#[derive(Debug, Error)]
pub enum ClientReleaseRepositoryError {
    #[error("client release input is invalid")]
    InvalidInput,
    #[error("client release version already exists for this target")]
    VersionConflict,
    #[error("client release was not found")]
    NotFound,
    #[error("stored client release value is invalid")]
    InvalidStoredValue,
    #[error("client release rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
    #[error("client release database operation failed: {0}")]
    Database(#[from] DbErr),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(value: &str) -> ClientVersion {
        ClientVersion::parse(value).unwrap()
    }

    #[test]
    fn versions_follow_semver_precedence() {
        let ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.10.0",
            "2.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(version(pair[0]) < version(pair[1]), "{pair:?}");
        }
        assert_eq!(version("1.2.3+build.5"), version("v1.2.3"));
    }

    #[test]
    fn malformed_versions_are_rejected() {
        for value in [
            "", "1", "1.2", "1.2.3.4", "01.2.3", "1.2.x", "1.2.3-", "1.2.3-01", "1.2.3+",
        ] {
            assert!(ClientVersion::parse(value).is_none(), "{value}");
        }
    }

    #[test]
    fn package_names_must_match_the_platform() {
        assert_eq!(
            client_package_extension(ClientPlatform::Windows, "TJXY-1.0.0-setup-x64.EXE"),
            Some("exe".to_owned())
        );
        assert_eq!(
            client_package_extension(ClientPlatform::Linux, "TJXY-1.0.0.AppImage"),
            Some("appimage".to_owned())
        );
        for (platform, name) in [
            (ClientPlatform::Android, "TJXY.exe"),
            (ClientPlatform::Windows, "../TJXY.exe"),
            (ClientPlatform::Windows, ".exe"),
            (ClientPlatform::Macos, "TJXY"),
        ] {
            assert!(client_package_extension(platform, name).is_none(), "{name}");
        }
    }
}
