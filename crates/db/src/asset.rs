use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use thiserror::Error;
use tjxy_common::{CatalogItemId, ImageType};
use uuid::Uuid;

const MAX_TEXT_CHARS: usize = 2048;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetPublication {
    item_id: CatalogItemId,
    image_type: ImageType,
    priority: i32,
    sha256: String,
    mime_type: String,
    width: i32,
    height: i32,
    byte_size: i64,
    local_relative_path: String,
    source_provider: String,
    source_reference: Option<String>,
}

impl AssetPublication {
    #[must_use]
    pub const fn image_type(&self) -> ImageType {
        self.image_type
    }

    /// Defines one validated content-addressed image reference.
    ///
    /// # Errors
    ///
    /// Returns [`AssetRepositoryError::InvalidPublication`] for invalid bounds or metadata.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        item_id: CatalogItemId,
        image_type: ImageType,
        priority: u32,
        sha256: impl Into<String>,
        mime_type: impl Into<String>,
        width: u32,
        height: u32,
        byte_size: u64,
        local_relative_path: impl Into<String>,
        source_provider: impl Into<String>,
        source_reference: Option<String>,
    ) -> Result<Self, AssetRepositoryError> {
        let publication = Self {
            item_id,
            image_type,
            priority: i32::try_from(priority)
                .map_err(|_| AssetRepositoryError::InvalidPublication)?,
            sha256: sha256.into(),
            mime_type: mime_type.into(),
            width: i32::try_from(width).map_err(|_| AssetRepositoryError::InvalidPublication)?,
            height: i32::try_from(height).map_err(|_| AssetRepositoryError::InvalidPublication)?,
            byte_size: i64::try_from(byte_size)
                .map_err(|_| AssetRepositoryError::InvalidPublication)?,
            local_relative_path: local_relative_path.into(),
            source_provider: source_provider.into(),
            source_reference,
        };
        if publication.sha256.len() != 64
            || !publication
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || publication.width <= 0
            || publication.height <= 0
            || publication.byte_size <= 0
            || !valid_text(&publication.mime_type)
            || !valid_text(&publication.local_relative_path)
            || !valid_text(&publication.source_provider)
            || publication
                .source_reference
                .as_deref()
                .is_some_and(|value| !valid_text(value))
        {
            return Err(AssetRepositoryError::InvalidPublication);
        }
        Ok(publication)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetPublicationReport {
    reused_blob: bool,
    reference_changed: bool,
}

impl AssetPublicationReport {
    #[must_use]
    pub const fn reused_blob(self) -> bool {
        self.reused_blob
    }

    #[must_use]
    pub const fn reference_changed(self) -> bool {
        self.reference_changed
    }
}

pub struct AssetRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> AssetRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Atomically reuses or creates a blob, updates the item role, and bumps generation on change.
    ///
    /// # Errors
    ///
    /// Returns stored metadata conflicts, foreign-key failures, or transaction failures.
    pub async fn publish(
        &self,
        publication: &AssetPublication,
    ) -> Result<AssetPublicationReport, AssetRepositoryError> {
        let transaction = self.database.begin().await?;
        let result = publish_in_transaction(&transaction, publication, true).await;
        finish(transaction, result).await
    }
}

#[derive(Debug, Error)]
pub enum AssetRepositoryError {
    #[error("asset publication is invalid")]
    InvalidPublication,
    #[error("stored asset metadata conflicts with its content digest")]
    StoredBlobConflict,
    #[error("catalog generation state is missing")]
    MissingCatalogState,
    #[error("asset database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("asset rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

pub(crate) async fn publish_in_transaction(
    transaction: &DatabaseTransaction,
    publication: &AssetPublication,
    bump_on_change: bool,
) -> Result<AssetPublicationReport, AssetRepositoryError> {
    let (blob_id, reused_blob) = find_or_insert_blob(transaction, publication).await?;
    let current = current_reference(transaction, publication).await?;
    let reference_changed = current
        .as_ref()
        .is_none_or(|(current_blob, provider, reference)| {
            *current_blob != blob_id
                || provider != &publication.source_provider
                || reference != &publication.source_reference
        });
    if reference_changed {
        upsert_reference(transaction, publication, blob_id).await?;
        if bump_on_change {
            bump_generation(transaction).await?;
        }
    }
    Ok(AssetPublicationReport {
        reused_blob,
        reference_changed,
    })
}

async fn find_or_insert_blob(
    transaction: &DatabaseTransaction,
    publication: &AssetPublication,
) -> Result<(Uuid, bool), AssetRepositoryError> {
    let proposed_id = Uuid::new_v4();
    let backend = transaction.get_database_backend();
    let query = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("mime_type"),
            Alias::new("width"),
            Alias::new("height"),
            Alias::new("byte_size"),
            Alias::new("local_relative_path"),
        ])
        .from(Alias::new("asset_blobs"))
        .and_where(Expr::col(Alias::new("sha256")).eq(&publication.sha256))
        .limit(1)
        .to_owned();
    let existing_before = transaction.query_one(backend.build(&query)).await?;
    let conflict = if backend == sea_orm::DbBackend::MySql {
        OnConflict::new()
            .update_column(Alias::new("sha256"))
            .to_owned()
    } else {
        OnConflict::column(Alias::new("sha256"))
            .do_nothing()
            .to_owned()
    };
    let insert = Query::insert()
        .into_table(Alias::new("asset_blobs"))
        .columns([
            Alias::new("id"),
            Alias::new("sha256"),
            Alias::new("mime_type"),
            Alias::new("width"),
            Alias::new("height"),
            Alias::new("byte_size"),
            Alias::new("local_relative_path"),
            Alias::new("created_at"),
        ])
        .values_panic([
            proposed_id.into(),
            publication.sha256.clone().into(),
            publication.mime_type.clone().into(),
            publication.width.into(),
            publication.height.into(),
            publication.byte_size.into(),
            publication.local_relative_path.clone().into(),
            Utc::now().into(),
        ])
        .on_conflict(conflict)
        .to_owned();
    transaction.execute(backend.build(&insert)).await?;
    let inserted = existing_before.is_none();
    let row = match existing_before {
        Some(row) => row,
        None => transaction
            .query_one(backend.build(&query))
            .await?
            .ok_or(AssetRepositoryError::StoredBlobConflict)?,
    };
    let matches = row.try_get::<String>("", "mime_type")? == publication.mime_type
        && row.try_get::<Option<i32>>("", "width")? == Some(publication.width)
        && row.try_get::<Option<i32>>("", "height")? == Some(publication.height)
        && row.try_get::<i64>("", "byte_size")? == publication.byte_size
        && row.try_get::<String>("", "local_relative_path")? == publication.local_relative_path;
    if !matches {
        return Err(AssetRepositoryError::StoredBlobConflict);
    }
    Ok((row.try_get("", "id")?, !inserted))
}

async fn current_reference(
    transaction: &DatabaseTransaction,
    publication: &AssetPublication,
) -> Result<Option<(Uuid, String, Option<String>)>, AssetRepositoryError> {
    let query = Query::select()
        .columns([
            Alias::new("asset_blob_id"),
            Alias::new("source_provider"),
            Alias::new("source_reference"),
        ])
        .from(Alias::new("item_assets"))
        .and_where(Expr::col(Alias::new("item_id")).eq(publication.item_id.as_uuid()))
        .and_where(Expr::col(Alias::new("image_type")).eq(publication.image_type.as_str()))
        .and_where(Expr::col(Alias::new("priority")).eq(publication.priority))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_one(backend.build(&query))
        .await?
        .map(|row| {
            Ok((
                row.try_get("", "asset_blob_id")?,
                row.try_get("", "source_provider")?,
                row.try_get("", "source_reference")?,
            ))
        })
        .transpose()
}

async fn upsert_reference(
    transaction: &DatabaseTransaction,
    publication: &AssetPublication,
    blob_id: Uuid,
) -> Result<(), AssetRepositoryError> {
    let insert = Query::insert()
        .into_table(Alias::new("item_assets"))
        .columns([
            Alias::new("id"),
            Alias::new("item_id"),
            Alias::new("asset_blob_id"),
            Alias::new("image_type"),
            Alias::new("priority"),
            Alias::new("source_provider"),
            Alias::new("source_reference"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            publication.item_id.as_uuid().into(),
            blob_id.into(),
            publication.image_type.as_str().into(),
            publication.priority.into(),
            publication.source_provider.clone().into(),
            publication.source_reference.clone().into(),
        ])
        .on_conflict(
            OnConflict::columns([
                Alias::new("item_id"),
                Alias::new("image_type"),
                Alias::new("priority"),
            ])
            .update_columns([
                Alias::new("asset_blob_id"),
                Alias::new("source_provider"),
                Alias::new("source_reference"),
            ])
            .to_owned(),
        )
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&insert)).await?;
    Ok(())
}

async fn bump_generation(transaction: &DatabaseTransaction) -> Result<(), AssetRepositoryError> {
    crate::advance_catalog_generation(transaction).await?;
    Ok(())
}

fn valid_text(value: &str) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= MAX_TEXT_CHARS
        && !value.chars().any(char::is_control)
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, AssetRepositoryError>,
) -> Result<T, AssetRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(AssetRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
