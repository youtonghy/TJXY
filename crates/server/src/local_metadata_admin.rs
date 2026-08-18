use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use axum::{
    Json,
    body::Bytes,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, TransactionTrait,
    sea_query::{Alias, Expr, Query},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tjxy_db::{AssetStorageError, AssetStorageRepository};
use uuid::Uuid;

use crate::{AppState, auth};

pub(crate) struct LocalMetadataAdminService {
    database: DatabaseConnection,
    current_path: PathBuf,
    source: &'static str,
}

impl LocalMetadataAdminService {
    pub(crate) fn new(
        database: DatabaseConnection,
        current_path: PathBuf,
        source: &'static str,
    ) -> Self {
        Self {
            database,
            current_path,
            source,
        }
    }

    async fn snapshot(&self) -> Result<LocalMetadataStorageDto, LocalMetadataAdminError> {
        let roots = AssetStorageRepository::new(&self.database).roots().await?;
        let pending_path = roots
            .iter()
            .find(|root| root.state() == "Pending")
            .map(|root| root.canonical_path().to_owned());
        let historical_locations = roots
            .iter()
            .filter(|root| root.state() == "History")
            .map(|root| root.canonical_path().to_owned())
            .collect();
        let rows = blob_rows(&self.database).await?;
        let root_paths: HashMap<Uuid, PathBuf> = roots
            .into_iter()
            .map(|root| (root.id(), PathBuf::from(root.canonical_path())))
            .collect();
        let current_path = self.current_path.clone();
        let scan_roots = root_paths
            .values()
            .cloned()
            .chain(std::iter::once(current_path.clone()))
            .collect::<HashSet<_>>();
        let files = tokio::task::spawn_blocking(move || scan_files(&scan_roots))
            .await
            .map_err(|_| LocalMetadataAdminError::ScanTask)??;
        let registered = rows
            .iter()
            .map(|row| (row.root_id, row.relative_path.clone()))
            .collect::<HashSet<_>>();
        let mut linked = Metric::default();
        let mut orphaned = Metric::default();
        let mut missing = Metric::default();
        for row in &rows {
            let root = row
                .root_id
                .and_then(|id| root_paths.get(&id))
                .unwrap_or(&current_path);
            if files.contains_key(&(root.clone(), row.relative_path.clone())) {
                if row.referenced {
                    linked.add(row.byte_size);
                } else {
                    orphaned.add(row.byte_size);
                }
            } else {
                missing.add(row.byte_size);
            }
        }
        let mut unregistered = Metric::default();
        for ((root, relative), bytes) in &files {
            let root_id = root_paths
                .iter()
                .find_map(|(id, path)| (path == root).then_some(*id));
            if !registered.contains(&(root_id, relative.clone())) {
                unregistered.add(*bytes);
            }
        }
        let total = linked + orphaned + missing + unregistered;
        let restart_required = pending_path.is_some();
        Ok(LocalMetadataStorageDto {
            current_path: self.current_path.to_string_lossy().into_owned(),
            pending_path,
            historical_locations,
            source: self.source,
            location_editable: self.source != "Environment",
            restart_required,
            checked_at: Utc::now(),
            statistics: Statistics {
                total,
                linked,
                orphaned,
                missing,
                unregistered,
            },
            cleanup_in_progress: false,
        })
    }

    async fn set_location(
        &self,
        path: &str,
    ) -> Result<LocalMetadataStorageDto, LocalMetadataAdminError> {
        if self.source == "Environment" {
            return Err(LocalMetadataAdminError::EnvironmentOverride);
        }
        let path = validate_location(path).await?;
        AssetStorageRepository::new(&self.database)
            .set_pending(&path.to_string_lossy())
            .await?;
        self.snapshot().await
    }

    async fn cleanup(&self) -> Result<CleanupDto, LocalMetadataAdminError> {
        let roots = AssetStorageRepository::new(&self.database).roots().await?;
        let root_paths: HashMap<Uuid, PathBuf> = roots
            .into_iter()
            .map(|root| (root.id(), PathBuf::from(root.canonical_path())))
            .collect();
        let rows = blob_rows(&self.database).await?;
        let registered = rows
            .iter()
            .map(|row| (row.root_id, row.relative_path.clone()))
            .collect::<HashSet<_>>();
        let mut deleted = Metric::default();
        let mut skipped_count = 0_u64;
        let mut failed_count = 0_u64;
        for row in rows.into_iter().filter(|row| !row.referenced) {
            let transaction = self.database.begin().await?;
            let backend = transaction.get_database_backend();
            let item_ref = asset_reference_exists("item_assets", row.id);
            let person_ref = asset_reference_exists("person_assets", row.id);
            let delete = Query::delete()
                .from_table(Alias::new("asset_blobs"))
                .and_where(Expr::col(Alias::new("id")).eq(row.id))
                .and_where(Expr::exists(item_ref).not())
                .and_where(Expr::exists(person_ref).not())
                .to_owned();
            let result = transaction.execute(backend.build(&delete)).await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                skipped_count += 1;
                continue;
            }
            transaction.commit().await?;
            let root = row
                .root_id
                .and_then(|id| root_paths.get(&id))
                .unwrap_or(&self.current_path);
            match tokio::fs::remove_file(root.join(&row.relative_path)).await {
                Ok(()) => deleted.add(row.byte_size),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    deleted.add(row.byte_size);
                }
                Err(_) => failed_count += 1,
            }
        }
        let current_path = self.current_path.clone();
        let scan_roots = root_paths
            .values()
            .cloned()
            .chain(std::iter::once(current_path))
            .collect::<HashSet<_>>();
        let files = tokio::task::spawn_blocking(move || scan_files(&scan_roots))
            .await
            .map_err(|_| LocalMetadataAdminError::ScanTask)??;
        for ((root, relative), bytes) in files {
            let root_id = root_paths
                .iter()
                .find_map(|(id, path)| (path == &root).then_some(*id));
            if registered.contains(&(root_id, relative.clone())) {
                continue;
            }
            match tokio::fs::remove_file(root.join(relative)).await {
                Ok(()) => deleted.add(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => skipped_count += 1,
                Err(_) => failed_count += 1,
            }
        }
        let storage = self.snapshot().await?;
        Ok(CleanupDto {
            deleted,
            skipped_count,
            failed_count,
            storage,
        })
    }
}

#[derive(Default)]
struct BlobRow {
    id: Uuid,
    root_id: Option<Uuid>,
    relative_path: String,
    byte_size: u64,
    referenced: bool,
}

async fn blob_rows(database: &DatabaseConnection) -> Result<Vec<BlobRow>, LocalMetadataAdminError> {
    let backend = database.get_database_backend();
    let item_ref = Query::select()
        .expr(Expr::val(1))
        .from(Alias::new("item_assets"))
        .and_where(
            Expr::col((Alias::new("item_assets"), Alias::new("asset_blob_id")))
                .equals((Alias::new("asset_blobs"), Alias::new("id"))),
        )
        .to_owned();
    let person_ref = Query::select()
        .expr(Expr::val(1))
        .from(Alias::new("person_assets"))
        .and_where(
            Expr::col((Alias::new("person_assets"), Alias::new("asset_blob_id")))
                .equals((Alias::new("asset_blobs"), Alias::new("id"))),
        )
        .to_owned();
    let query = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("storage_root_id"),
            Alias::new("local_relative_path"),
            Alias::new("byte_size"),
        ])
        .expr_as(
            Expr::exists(item_ref).or(Expr::exists(person_ref)),
            Alias::new("referenced"),
        )
        .from(Alias::new("asset_blobs"))
        .to_owned();
    database
        .query_all(backend.build(&query))
        .await?
        .into_iter()
        .map(|row| {
            let size: i64 = row.try_get("", "byte_size")?;
            Ok(BlobRow {
                id: row.try_get("", "id")?,
                root_id: row.try_get("", "storage_root_id")?,
                relative_path: row.try_get("", "local_relative_path")?,
                byte_size: u64::try_from(size)
                    .map_err(|_| LocalMetadataAdminError::InvalidStoredData)?,
                referenced: row.try_get("", "referenced")?,
            })
        })
        .collect()
}

fn asset_reference_exists(table: &str, id: Uuid) -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .expr(Expr::val(1))
        .from(Alias::new(table))
        .and_where(Expr::col(Alias::new("asset_blob_id")).eq(id))
        .to_owned()
}

fn scan_files(roots: &HashSet<PathBuf>) -> Result<HashMap<(PathBuf, String), u64>, std::io::Error> {
    let mut files = HashMap::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let mut stack = vec![root.clone()];
        while let Some(directory) = stack.pop() {
            for entry in std::fs::read_dir(directory)? {
                let entry = entry?;
                let ty = entry.file_type()?;
                if ty.is_symlink() {
                    continue;
                }
                if ty.is_dir() {
                    if entry.file_name() != "branding" {
                        stack.push(entry.path());
                    }
                    continue;
                }
                if ty.is_file() {
                    let relative = entry
                        .path()
                        .strip_prefix(root)
                        .map_err(std::io::Error::other)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    if content_addressed_path(&relative) {
                        files.insert((root.clone(), relative), entry.metadata()?.len());
                    }
                }
            }
        }
    }
    Ok(files)
}

fn content_addressed_path(relative: &str) -> bool {
    let Some((prefix, file)) = relative.split_once('/') else {
        return false;
    };
    let Some((digest, extension)) = file.rsplit_once('.') else {
        return false;
    };
    prefix.len() == 2
        && digest.len() == 64
        && digest.starts_with(prefix)
        && digest
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && matches!(extension, "jpg" | "png" | "gif" | "webp" | "avif" | "bmp")
}

async fn validate_location(value: &str) -> Result<PathBuf, LocalMetadataAdminError> {
    let value = value.trim();
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 4096
        || value.chars().any(char::is_control)
        || !path.is_absolute()
    {
        return Err(LocalMetadataAdminError::InvalidLocation);
    }
    tokio::fs::create_dir_all(path).await?;
    let canonical = tokio::fs::canonicalize(path).await?;
    let probe = canonical.join(format!(".tjxy-write-test-{}", Uuid::new_v4()));
    tokio::fs::write(&probe, b"").await?;
    tokio::fs::remove_file(probe).await?;
    Ok(canonical)
}

#[derive(Clone, Copy, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Metric {
    count: u64,
    bytes: u64,
}
impl Metric {
    fn add(&mut self, bytes: u64) {
        self.count += 1;
        self.bytes = self.bytes.saturating_add(bytes);
    }
}
impl std::ops::Add for Metric {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            count: self.count.saturating_add(rhs.count),
            bytes: self.bytes.saturating_add(rhs.bytes),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Statistics {
    total: Metric,
    linked: Metric,
    orphaned: Metric,
    missing: Metric,
    unregistered: Metric,
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LocalMetadataStorageDto {
    current_path: String,
    pending_path: Option<String>,
    historical_locations: Vec<String>,
    source: &'static str,
    location_editable: bool,
    restart_required: bool,
    checked_at: DateTime<Utc>,
    statistics: Statistics,
    cleanup_in_progress: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CleanupDto {
    deleted: Metric,
    skipped_count: u64,
    failed_count: u64,
    storage: LocalMetadataStorageDto,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct LocationRequest {
    path: String,
}

pub(crate) async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) = authorized(&state, &headers, query.as_deref()).await {
        return no_store(response);
    }
    let Some(service) = state.local_metadata_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    no_store(async_response(service.snapshot().await))
}
pub(crate) async fn put_location(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Response {
    if let Err(response) = authorized(&state, &headers, query.as_deref()).await {
        return no_store(response);
    }
    if !auth::is_json_content_type(&headers) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Ok(request) = serde_json::from_slice::<LocationRequest>(&body) else {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    };
    let Some(service) = state.local_metadata_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    no_store(async_response(service.set_location(&request.path).await))
}
pub(crate) async fn cleanup(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    if let Err(response) = authorized(&state, &headers, query.as_deref()).await {
        return no_store(response);
    }
    let Some(service) = state.local_metadata_admin.as_ref() else {
        return no_store(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    no_store(async_response(service.cleanup().await))
}

async fn authorized(
    state: &AppState,
    headers: &HeaderMap,
    raw: Option<&str>,
) -> Result<(), Response> {
    let Ok(mut query) = auth::request_query(raw) else {
        return Err(StatusCode::BAD_REQUEST.into_response());
    };
    query.remove("ApiKey");
    query.remove("api_key");
    if !query.is_empty() {
        return Err(StatusCode::BAD_REQUEST.into_response());
    }
    auth::authenticated_administrator(state, headers, raw)
        .await
        .map(|_| ())
}
fn async_response<T: Serialize>(result: Result<T, LocalMetadataAdminError>) -> Response {
    match result {
        Ok(dto) => Json(dto).into_response(),
        Err(LocalMetadataAdminError::InvalidLocation) => StatusCode::BAD_REQUEST.into_response(),
        Err(LocalMetadataAdminError::EnvironmentOverride) => StatusCode::CONFLICT.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}
fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[derive(Debug, Error)]
enum LocalMetadataAdminError {
    #[error("invalid metadata storage location")]
    InvalidLocation,
    #[error("metadata storage location is controlled by the environment")]
    EnvironmentOverride,
    #[error("stored asset metadata is invalid")]
    InvalidStoredData,
    #[error("metadata file scan task failed")]
    ScanTask,
    #[error("metadata file operation failed: {0}")]
    File(#[from] std::io::Error),
    #[error("metadata database operation failed: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("metadata storage operation failed: {0}")]
    Storage(#[from] AssetStorageError),
}
