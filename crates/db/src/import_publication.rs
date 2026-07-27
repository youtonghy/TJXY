use std::collections::{HashMap, HashSet};

use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::natural_key;

const MAX_ITEMS: usize = 100_000;
const MAX_TEXT_CHARS: usize = 32_768;
const MAX_ASSOCIATIONS_PER_ITEM: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportPublicationTarget {
    library_id: Uuid,
    user_id: Uuid,
}

impl ImportPublicationTarget {
    #[must_use]
    pub const fn new(library_id: Uuid, user_id: Uuid) -> Self {
        Self {
            library_id,
            user_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportPublicationReport {
    items: usize,
    replayed: bool,
}

impl ImportPublicationReport {
    #[must_use]
    pub const fn items(self) -> usize {
        self.items
    }

    #[must_use]
    pub const fn replayed(self) -> bool {
        self.replayed
    }
}

pub struct ImportPublicationRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> ImportPublicationRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Atomically publishes one sealed import generation into the live catalog.
    ///
    /// Replaying a completed generation is a read-only success. A failed transaction leaves the
    /// generation sealed and exposes none of its catalog, mapping, metadata, or user-data rows.
    ///
    /// # Errors
    ///
    /// Returns validation, state, foreign-key, uniqueness, or transaction failures.
    pub async fn publish(
        &self,
        job_id: Uuid,
        target: ImportPublicationTarget,
    ) -> Result<ImportPublicationReport, ImportPublicationError> {
        let transaction = self.database.begin().await?;
        let result = publish(&transaction, job_id, target).await;
        finish(transaction, result).await
    }
}

#[derive(Debug, Error)]
pub enum ImportPublicationError {
    #[error("import generation is not ready to publish")]
    NotReady,
    #[error("import staging generation is invalid")]
    InvalidStaging,
    #[error("import staging parent does not exist in the same generation")]
    MissingParent,
    #[error("import database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("import rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

#[derive(Deserialize)]
struct StagedPayload {
    version: u8,
    name: String,
    #[serde(default)]
    production_year: Option<i32>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    provider_ids: HashMap<String, String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    people: Vec<StagedPerson>,
    #[serde(default)]
    studios: Vec<StagedStudio>,
    #[serde(default)]
    user_data: StagedUserData,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StagedPerson {
    name: String,
    #[serde(rename = "Type")]
    person_type: String,
    #[serde(default)]
    role: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StagedStudio {
    name: String,
}

#[derive(Default, Deserialize)]
struct StagedUserData {
    #[serde(default)]
    playback_position_ticks: u64,
    #[serde(default)]
    played: bool,
    #[serde(default)]
    play_count: u64,
    #[serde(default)]
    is_favorite: bool,
}

struct StagedRow {
    entity_kind: String,
    legacy_item_id: String,
    parent_legacy_item_id: Option<String>,
    payload: StagedPayload,
    catalog_item_id: Uuid,
}

async fn publish(
    transaction: &DatabaseTransaction,
    job_id: Uuid,
    target: ImportPublicationTarget,
) -> Result<ImportPublicationReport, ImportPublicationError> {
    let (state, source_instance_id) = read_job(transaction, job_id).await?;
    if state == "Completed" {
        return Ok(ImportPublicationReport {
            items: mapping_count(transaction, job_id).await?,
            replayed: true,
        });
    }
    if state != "ReadyToPublish" {
        return Err(ImportPublicationError::NotReady);
    }
    let rows = read_staging(transaction, job_id).await?;
    validate_rows(&rows)?;
    for row in &rows {
        insert_catalog_item(transaction, row).await?;
    }
    let identities = rows
        .iter()
        .map(|row| (row.legacy_item_id.as_str(), row.catalog_item_id))
        .collect::<HashMap<_, _>>();
    for row in &rows {
        attach_parent(transaction, row, &identities).await?;
        attach_library(transaction, row.catalog_item_id, target.library_id).await?;
        insert_mapping(transaction, job_id, &source_instance_id, row).await?;
        insert_provider_ids(transaction, row).await?;
        insert_genres(transaction, row).await?;
        insert_people(transaction, row).await?;
        insert_studios(transaction, row).await?;
        insert_user_data(transaction, row, target.user_id).await?;
    }
    bump_user_revision(transaction, target.user_id).await?;
    crate::advance_catalog_generation(transaction).await?;
    let backend = transaction.get_database_backend();
    let complete = Query::update()
        .table(Alias::new("import_jobs"))
        .value(Alias::new("state"), "Completed")
        .value(Alias::new("updated_at"), Utc::now())
        .and_where(Expr::col(Alias::new("id")).eq(job_id))
        .and_where(Expr::col(Alias::new("state")).eq("ReadyToPublish"))
        .to_owned();
    if transaction
        .execute(backend.build(&complete))
        .await?
        .rows_affected()
        != 1
    {
        return Err(ImportPublicationError::NotReady);
    }
    Ok(ImportPublicationReport {
        items: rows.len(),
        replayed: false,
    })
}

async fn read_job(
    transaction: &DatabaseTransaction,
    job_id: Uuid,
) -> Result<(String, String), ImportPublicationError> {
    let query = Query::select()
        .columns([Alias::new("state"), Alias::new("source_instance_id")])
        .from(Alias::new("import_jobs"))
        .and_where(Expr::col(Alias::new("id")).eq(job_id))
        .limit(1)
        .to_owned();
    let backend = transaction.get_database_backend();
    let row = transaction
        .query_one(backend.build(&query))
        .await?
        .ok_or(ImportPublicationError::NotReady)?;
    Ok((
        row.try_get("", "state")?,
        row.try_get("", "source_instance_id")?,
    ))
}

async fn read_staging(
    transaction: &DatabaseTransaction,
    job_id: Uuid,
) -> Result<Vec<StagedRow>, ImportPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("entity_kind"),
            Alias::new("legacy_item_id"),
            Alias::new("identity_key"),
            Alias::new("parent_legacy_item_id"),
            Alias::new("payload"),
        ])
        .from(Alias::new("import_staging_items"))
        .and_where(Expr::col(Alias::new("import_job_id")).eq(job_id))
        .order_by(Alias::new("legacy_item_id"), sea_orm::sea_query::Order::Asc)
        .limit((MAX_ITEMS + 1) as u64)
        .to_owned();
    let backend = transaction.get_database_backend();
    let rows = transaction.query_all(backend.build(&query)).await?;
    if rows.len() > MAX_ITEMS {
        return Err(ImportPublicationError::InvalidStaging);
    }
    rows.iter().map(staged_row).collect()
}

fn staged_row(row: &QueryResult) -> Result<StagedRow, ImportPublicationError> {
    let payload: Value = row.try_get("", "payload")?;
    Ok(StagedRow {
        entity_kind: row.try_get("", "entity_kind")?,
        legacy_item_id: row.try_get("", "legacy_item_id")?,
        parent_legacy_item_id: row.try_get("", "parent_legacy_item_id")?,
        payload: serde_json::from_value(payload)
            .map_err(|_| ImportPublicationError::InvalidStaging)?,
        catalog_item_id: Uuid::new_v4(),
    })
}

fn validate_rows(rows: &[StagedRow]) -> Result<(), ImportPublicationError> {
    let mut identities = HashSet::with_capacity(rows.len());
    for row in rows {
        if !matches!(
            row.entity_kind.as_str(),
            "Movie" | "Series" | "Season" | "Episode"
        ) || row.payload.version != 1
            || !valid_text(&row.payload.name, 512)
            || row
                .payload
                .overview
                .as_deref()
                .is_some_and(|value| !valid_text(value, MAX_TEXT_CHARS))
            || !identities.insert(row.legacy_item_id.as_str())
            || row.payload.provider_ids.len() > MAX_ASSOCIATIONS_PER_ITEM
            || row.payload.genres.len() > MAX_ASSOCIATIONS_PER_ITEM
            || row.payload.people.len() > MAX_ASSOCIATIONS_PER_ITEM
            || row.payload.studios.len() > MAX_ASSOCIATIONS_PER_ITEM
            || row
                .payload
                .provider_ids
                .iter()
                .any(|(provider, id)| !valid_text(provider, 128) || !valid_text(id, 2048))
            || row
                .payload
                .genres
                .iter()
                .any(|genre| !valid_text(genre, 512))
            || row.payload.people.iter().any(|person| {
                !valid_text(&person.name, 512)
                    || !valid_text(&person.person_type, 128)
                    || person.role.chars().count() > 512
                    || person.role.chars().any(char::is_control)
            })
            || row
                .payload
                .studios
                .iter()
                .any(|studio| !valid_text(&studio.name, 512))
            || i64::try_from(row.payload.user_data.playback_position_ticks).is_err()
            || i32::try_from(row.payload.user_data.play_count).is_err()
        {
            return Err(ImportPublicationError::InvalidStaging);
        }
    }
    if rows.iter().any(|row| {
        row.parent_legacy_item_id
            .as_deref()
            .is_some_and(|parent| !identities.contains(parent))
    }) {
        return Err(ImportPublicationError::MissingParent);
    }
    Ok(())
}

async fn insert_catalog_item(
    transaction: &DatabaseTransaction,
    row: &StagedRow,
) -> Result<(), ImportPublicationError> {
    let insert = Query::insert()
        .into_table(Alias::new("catalog_items"))
        .columns([
            Alias::new("id"),
            Alias::new("item_type"),
            Alias::new("name"),
            Alias::new("sort_name"),
            Alias::new("production_year"),
            Alias::new("overview"),
            Alias::new("classification_state"),
            Alias::new("metadata_state"),
            Alias::new("structure_state"),
            Alias::new("source_state"),
            Alias::new("structure_expansion_revision"),
            Alias::new("source_index_revision"),
            Alias::new("is_present"),
        ])
        .values_panic([
            row.catalog_item_id.into(),
            row.entity_kind.clone().into(),
            row.payload.name.clone().into(),
            row.payload.name.to_lowercase().into(),
            row.payload.production_year.into(),
            row.payload.overview.clone().into(),
            "Imported".into(),
            "Ready".into(),
            "Imported".into(),
            "Unknown".into(),
            0_i64.into(),
            0_i64.into(),
            true.into(),
        ])
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&insert)).await?;
    Ok(())
}

async fn attach_parent(
    transaction: &DatabaseTransaction,
    row: &StagedRow,
    identities: &HashMap<&str, Uuid>,
) -> Result<(), ImportPublicationError> {
    let Some(parent) = row.parent_legacy_item_id.as_deref() else {
        return Ok(());
    };
    let parent_id = identities
        .get(parent)
        .ok_or(ImportPublicationError::MissingParent)?;
    let update = Query::update()
        .table(Alias::new("catalog_items"))
        .value(Alias::new("parent_id"), *parent_id)
        .and_where(Expr::col(Alias::new("id")).eq(row.catalog_item_id))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&update)).await?;
    Ok(())
}

async fn attach_library(
    transaction: &DatabaseTransaction,
    catalog_item_id: Uuid,
    library_id: Uuid,
) -> Result<(), ImportPublicationError> {
    let insert = Query::insert()
        .into_table(Alias::new("library_catalog_items"))
        .columns([
            Alias::new("id"),
            Alias::new("library_id"),
            Alias::new("catalog_item_id"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            library_id.into(),
            catalog_item_id.into(),
        ])
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&insert)).await?;
    Ok(())
}

async fn insert_mapping(
    transaction: &DatabaseTransaction,
    job_id: Uuid,
    source_instance_id: &str,
    row: &StagedRow,
) -> Result<(), ImportPublicationError> {
    let insert = Query::insert()
        .into_table(Alias::new("legacy_item_mappings"))
        .columns([
            Alias::new("id"),
            Alias::new("import_job_id"),
            Alias::new("source_instance_id"),
            Alias::new("legacy_item_id"),
            Alias::new("identity_key"),
            Alias::new("catalog_item_id"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            job_id.into(),
            source_instance_id.into(),
            row.legacy_item_id.clone().into(),
            natural_key::hash(&[source_instance_id, &row.legacy_item_id]).into(),
            row.catalog_item_id.into(),
        ])
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&insert)).await?;
    Ok(())
}

async fn insert_provider_ids(
    transaction: &DatabaseTransaction,
    row: &StagedRow,
) -> Result<(), ImportPublicationError> {
    let backend = transaction.get_database_backend();
    for (provider, provider_item_id) in &row.payload.provider_ids {
        let insert = Query::insert()
            .into_table(Alias::new("provider_ids"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("provider"),
                Alias::new("provider_item_id"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                row.catalog_item_id.into(),
                provider.into(),
                provider_item_id.into(),
            ])
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
    }
    Ok(())
}

async fn insert_genres(
    transaction: &DatabaseTransaction,
    row: &StagedRow,
) -> Result<(), ImportPublicationError> {
    let backend = transaction.get_database_backend();
    for genre in &row.payload.genres {
        let proposed_id = Uuid::new_v4();
        let conflict = if backend == sea_orm::DbBackend::MySql {
            OnConflict::column(Alias::new("name"))
                .update_column(Alias::new("name"))
                .to_owned()
        } else {
            OnConflict::column(Alias::new("name"))
                .do_nothing()
                .to_owned()
        };
        let insert = Query::insert()
            .into_table(Alias::new("genres"))
            .columns([Alias::new("id"), Alias::new("name")])
            .values_panic([proposed_id.into(), genre.into()])
            .on_conflict(conflict)
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
        let select = Query::select()
            .column(Alias::new("id"))
            .from(Alias::new("genres"))
            .and_where(Expr::col(Alias::new("name")).eq(genre))
            .limit(1)
            .to_owned();
        let genre_id: Uuid = transaction
            .query_one(backend.build(&select))
            .await?
            .ok_or(ImportPublicationError::InvalidStaging)?
            .try_get("", "id")?;
        let link = Query::insert()
            .into_table(Alias::new("item_genres"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("genre_id"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                row.catalog_item_id.into(),
                genre_id.into(),
            ])
            .to_owned();
        transaction.execute(backend.build(&link)).await?;
    }
    Ok(())
}

async fn insert_user_data(
    transaction: &DatabaseTransaction,
    row: &StagedRow,
    user_id: Uuid,
) -> Result<(), ImportPublicationError> {
    let playback_position = i64::try_from(row.payload.user_data.playback_position_ticks)
        .map_err(|_| ImportPublicationError::InvalidStaging)?;
    let play_count = i32::try_from(row.payload.user_data.play_count)
        .map_err(|_| ImportPublicationError::InvalidStaging)?;
    let insert = Query::insert()
        .into_table(Alias::new("user_data"))
        .columns([
            Alias::new("id"),
            Alias::new("user_id"),
            Alias::new("catalog_item_id"),
            Alias::new("playback_position_ticks"),
            Alias::new("is_played"),
            Alias::new("play_count"),
            Alias::new("is_favorite"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            user_id.into(),
            row.catalog_item_id.into(),
            playback_position.into(),
            row.payload.user_data.played.into(),
            play_count.into(),
            row.payload.user_data.is_favorite.into(),
            Utc::now().into(),
        ])
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&insert)).await?;
    Ok(())
}

async fn insert_people(
    transaction: &DatabaseTransaction,
    row: &StagedRow,
) -> Result<(), ImportPublicationError> {
    let backend = transaction.get_database_backend();
    for (sort_order, person) in row.payload.people.iter().enumerate() {
        let person_id = Uuid::new_v4();
        let insert_person = Query::insert()
            .into_table(Alias::new("people"))
            .columns([
                Alias::new("id"),
                Alias::new("name"),
                Alias::new("sort_name"),
            ])
            .values_panic([
                person_id.into(),
                person.name.clone().into(),
                person.name.to_lowercase().into(),
            ])
            .to_owned();
        transaction.execute(backend.build(&insert_person)).await?;
        let role = if person.role.trim().is_empty() {
            &person.person_type
        } else {
            &person.role
        };
        let sort_order =
            i32::try_from(sort_order).map_err(|_| ImportPublicationError::InvalidStaging)?;
        let link = Query::insert()
            .into_table(Alias::new("item_people"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("person_id"),
                Alias::new("role"),
                Alias::new("sort_order"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                row.catalog_item_id.into(),
                person_id.into(),
                role.into(),
                sort_order.into(),
            ])
            .to_owned();
        transaction.execute(backend.build(&link)).await?;
    }
    Ok(())
}

async fn insert_studios(
    transaction: &DatabaseTransaction,
    row: &StagedRow,
) -> Result<(), ImportPublicationError> {
    let backend = transaction.get_database_backend();
    for studio in &row.payload.studios {
        let proposed_id = Uuid::new_v4();
        let conflict = if backend == sea_orm::DbBackend::MySql {
            OnConflict::column(Alias::new("name"))
                .update_column(Alias::new("name"))
                .to_owned()
        } else {
            OnConflict::column(Alias::new("name"))
                .do_nothing()
                .to_owned()
        };
        let insert = Query::insert()
            .into_table(Alias::new("studios"))
            .columns([Alias::new("id"), Alias::new("name")])
            .values_panic([proposed_id.into(), studio.name.clone().into()])
            .on_conflict(conflict)
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
        let select = Query::select()
            .column(Alias::new("id"))
            .from(Alias::new("studios"))
            .and_where(Expr::col(Alias::new("name")).eq(&studio.name))
            .limit(1)
            .to_owned();
        let studio_id: Uuid = transaction
            .query_one(backend.build(&select))
            .await?
            .ok_or(ImportPublicationError::InvalidStaging)?
            .try_get("", "id")?;
        let link = Query::insert()
            .into_table(Alias::new("item_studios"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("studio_id"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                row.catalog_item_id.into(),
                studio_id.into(),
            ])
            .to_owned();
        transaction.execute(backend.build(&link)).await?;
    }
    Ok(())
}

async fn bump_user_revision(
    transaction: &DatabaseTransaction,
    user_id: Uuid,
) -> Result<(), ImportPublicationError> {
    let now = Utc::now();
    let backend = transaction.get_database_backend();
    let conflict = if backend == sea_orm::DbBackend::MySql {
        OnConflict::column(Alias::new("user_id"))
            .update_column(Alias::new("user_id"))
            .to_owned()
    } else {
        OnConflict::column(Alias::new("user_id"))
            .do_nothing()
            .to_owned()
    };
    let insert = Query::insert()
        .into_table(Alias::new("user_catalog_state"))
        .columns([
            Alias::new("id"),
            Alias::new("user_id"),
            Alias::new("revision"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            Uuid::new_v4().into(),
            user_id.into(),
            0_i64.into(),
            now.into(),
        ])
        .on_conflict(conflict)
        .to_owned();
    transaction.execute(backend.build(&insert)).await?;
    let update = Query::update()
        .table(Alias::new("user_catalog_state"))
        .value(
            Alias::new("revision"),
            Expr::col(Alias::new("revision")).add(1),
        )
        .value(Alias::new("updated_at"), now)
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id))
        .to_owned();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(ImportPublicationError::InvalidStaging);
    }
    Ok(())
}

async fn mapping_count(
    transaction: &DatabaseTransaction,
    job_id: Uuid,
) -> Result<usize, ImportPublicationError> {
    let query = Query::select()
        .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
        .from(Alias::new("legacy_item_mappings"))
        .and_where(Expr::col(Alias::new("import_job_id")).eq(job_id))
        .to_owned();
    let backend = transaction.get_database_backend();
    let count: i64 = transaction
        .query_one(backend.build(&query))
        .await?
        .ok_or(ImportPublicationError::InvalidStaging)?
        .try_get("", "count")?;
    usize::try_from(count).map_err(|_| ImportPublicationError::InvalidStaging)
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, ImportPublicationError>,
) -> Result<T, ImportPublicationError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(ImportPublicationError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
