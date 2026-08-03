use std::collections::BTreeMap;

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, QueryResult, TransactionTrait,
    sea_query::{Alias, Expr, JoinType, OnConflict, Order, Query},
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::{CatalogItemId, SortKey};
use tjxy_metadata::{MetadataItemKind, MetadataLookup, MetadataResolution};
use uuid::Uuid;

const MAX_TITLE_CHARS: usize = 512;
const MAX_OVERVIEW_CHARS: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataPublicationReport {
    changed: bool,
}

impl MetadataPublicationReport {
    #[must_use]
    pub const fn changed(self) -> bool {
        self.changed
    }
}

pub struct MetadataPublicationRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> MetadataPublicationRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Reads the current stable naming evidence for metadata resolution.
    ///
    /// # Errors
    ///
    /// Returns [`MetadataPublicationError`] for missing, invalid, or unsupported items.
    pub async fn lookup(
        &self,
        item_id: CatalogItemId,
    ) -> Result<MetadataLookup, MetadataPublicationError> {
        let current = current_item(self.database, item_id).await?;
        MetadataLookup::new(
            parse_item_kind(&current.item_kind)?,
            current.name,
            current.production_year,
        )
        .map_err(|_| MetadataPublicationError::InvalidResolution)
    }

    /// Atomically publishes resolved fields, provider identities, provenance, and generation.
    ///
    /// Exact replays perform no writes and do not advance the catalog generation.
    ///
    /// # Errors
    ///
    /// Returns validation, identity-conflict, database, commit, or rollback failures.
    pub async fn publish(
        &self,
        item_id: CatalogItemId,
        resolution: &MetadataResolution,
    ) -> Result<MetadataPublicationReport, MetadataPublicationError> {
        validate_resolution(resolution)?;
        let transaction = self.database.begin().await?;
        let result = async {
            let revision = lock_metadata_item(&transaction, item_id).await?;
            let report = publish_in_transaction(
                &transaction,
                item_id,
                resolution,
                crate::MetadataRequirement::Full,
            )
            .await?;
            if report.changed {
                advance_direct_metadata_revision(&transaction, item_id, revision).await?;
            }
            Ok(report)
        }
        .await;
        finish(transaction, result).await
    }
}

async fn lock_metadata_item(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
) -> Result<i64, MetadataPublicationError> {
    let table = Alias::new("catalog_items");
    let lock = Query::update()
        .table(table.clone())
        .value(
            Alias::new("metadata_revision"),
            Expr::col((table.clone(), Alias::new("metadata_revision"))),
        )
        .and_where(Expr::col((table, Alias::new("id"))).eq(item_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction.execute(backend.build(&lock)).await?;
    let row = transaction
        .query_one(
            backend.build(
                &Query::select()
                    .column(Alias::new("metadata_revision"))
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
                    .limit(1)
                    .to_owned(),
            ),
        )
        .await?
        .ok_or(MetadataPublicationError::ItemNotFound)?;
    Ok(row.try_get("", "metadata_revision")?)
}

async fn advance_direct_metadata_revision(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    previous_revision: i64,
) -> Result<(), MetadataPublicationError> {
    let next_revision = previous_revision
        .checked_add(1)
        .ok_or(MetadataPublicationError::InvalidResolution)?;
    let update = Query::update()
        .table(Alias::new("catalog_items"))
        .value(Alias::new("metadata_revision"), next_revision)
        .value(Alias::new("metadata_resolved_revision"), next_revision)
        .value(
            Alias::new("metadata_resolved_requirement"),
            crate::MetadataRequirement::Full.as_i32(),
        )
        .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
        .and_where(Expr::col(Alias::new("metadata_revision")).eq(previous_revision))
        .to_owned();
    if transaction
        .execute(transaction.get_database_backend().build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(MetadataPublicationError::ItemNotFound);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum MetadataPublicationError {
    #[error("catalog item was not found")]
    ItemNotFound,
    #[error("resolved metadata does not match the catalog item kind")]
    ItemKindMismatch,
    #[error("resolved metadata is invalid")]
    InvalidResolution,
    #[error("metadata publication database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("metadata publication rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

pub(crate) async fn publish_in_transaction(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    resolution: &MetadataResolution,
    requirement: crate::MetadataRequirement,
) -> Result<MetadataPublicationReport, MetadataPublicationError> {
    let current = current_item(transaction, item_id).await?;
    if current.item_kind != item_kind(resolution.item_kind()) {
        return Err(MetadataPublicationError::ItemKindMismatch);
    }
    let current_provider_ids = read_provider_ids(transaction, item_id).await?;
    let effective = EffectiveMetadata::new(&current, &current_provider_ids, resolution);
    let desired_provenance = desired_provenance(resolution)?;
    let associations_changed = requirement == crate::MetadataRequirement::Full
        && associations_changed(transaction, item_id, resolution).await?;
    let changed = current.name != effective.title
        || current.original_title != effective.original_title
        || current.production_year != effective.production_year
        || current.overview != effective.overview
        || current.metadata_state != effective.metadata_state
        || current_provider_ids != effective.provider_ids
        || !provenance_matches(transaction, item_id, &desired_provenance).await?
        || associations_changed;
    if !changed {
        return Ok(MetadataPublicationReport { changed: false });
    }
    update_item(transaction, item_id, &effective).await?;
    publish_provider_ids(transaction, item_id, resolution.provider_ids()).await?;
    publish_provenance(transaction, item_id, &desired_provenance).await?;
    if requirement == crate::MetadataRequirement::Full {
        publish_associations(transaction, item_id, resolution).await?;
    }
    bump_generation(transaction).await?;
    Ok(MetadataPublicationReport { changed: true })
}

struct CurrentItem {
    item_kind: String,
    name: String,
    original_title: Option<String>,
    production_year: Option<i32>,
    overview: Option<String>,
    metadata_state: String,
}

struct EffectiveMetadata {
    title: String,
    original_title: Option<String>,
    production_year: Option<i32>,
    overview: Option<String>,
    provider_ids: BTreeMap<String, String>,
    metadata_state: String,
}

impl EffectiveMetadata {
    fn new(
        current: &CurrentItem,
        current_provider_ids: &BTreeMap<String, String>,
        resolution: &MetadataResolution,
    ) -> Self {
        let mut provider_ids = current_provider_ids.clone();
        provider_ids.extend(
            resolution
                .provider_ids()
                .iter()
                .map(|(provider, id)| (provider.clone(), id.clone())),
        );
        let production_year = resolution.production_year().or(current.production_year);
        let overview = resolution
            .overview()
            .map(str::to_owned)
            .or_else(|| current.overview.clone());
        let metadata_state = if production_year.is_some()
            && overview.as_deref().is_some_and(|value| !value.is_empty())
            && !provider_ids.is_empty()
        {
            "Ready"
        } else {
            "Partial"
        };
        Self {
            title: resolution.title().to_owned(),
            original_title: resolution
                .original_title()
                .map(str::to_owned)
                .or_else(|| current.original_title.clone()),
            production_year,
            overview,
            provider_ids,
            metadata_state: metadata_state.to_owned(),
        }
    }
}

async fn current_item<Connection: ConnectionTrait>(
    connection: &Connection,
    item_id: CatalogItemId,
) -> Result<CurrentItem, MetadataPublicationError> {
    let query = Query::select()
        .columns([
            Alias::new("item_type"),
            Alias::new("name"),
            Alias::new("original_title"),
            Alias::new("production_year"),
            Alias::new("overview"),
            Alias::new("metadata_state"),
        ])
        .from(Alias::new("catalog_items"))
        .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
        .limit(1)
        .to_owned();
    let backend = connection.get_database_backend();
    let row = connection
        .query_one(backend.build(&query))
        .await?
        .ok_or(MetadataPublicationError::ItemNotFound)?;
    Ok(CurrentItem {
        item_kind: row.try_get("", "item_type")?,
        name: row.try_get("", "name")?,
        original_title: row.try_get("", "original_title")?,
        production_year: row.try_get("", "production_year")?,
        overview: row.try_get("", "overview")?,
        metadata_state: row.try_get("", "metadata_state")?,
    })
}

async fn update_item(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    metadata: &EffectiveMetadata,
) -> Result<(), MetadataPublicationError> {
    let update = Query::update()
        .table(Alias::new("catalog_items"))
        .value(Alias::new("name"), metadata.title.as_str())
        .value(
            Alias::new("original_title"),
            metadata.original_title.as_deref(),
        )
        .value(Alias::new("sort_name"), metadata.title.to_lowercase())
        .value(
            Alias::new("sort_key"),
            SortKey::from_text(&metadata.title).into_bytes(),
        )
        .value(Alias::new("production_year"), metadata.production_year)
        .value(Alias::new("overview"), metadata.overview.as_deref())
        .value(
            Alias::new("metadata_state"),
            metadata.metadata_state.as_str(),
        )
        .value(Alias::new("last_error"), Option::<String>::None)
        .and_where(Expr::col(Alias::new("id")).eq(item_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    if transaction
        .execute(backend.build(&update))
        .await?
        .rows_affected()
        != 1
    {
        return Err(MetadataPublicationError::ItemNotFound);
    }
    Ok(())
}

async fn read_provider_ids(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
) -> Result<BTreeMap<String, String>, DbErr> {
    let query = Query::select()
        .columns([Alias::new("provider"), Alias::new("provider_item_id")])
        .from(Alias::new("provider_ids"))
        .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    let rows = transaction.query_all(backend.build(&query)).await?;
    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String>("", "provider")?,
                row.try_get::<String>("", "provider_item_id")?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, DbErr>>()
}

async fn publish_provider_ids(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    provider_ids: &BTreeMap<String, String>,
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    for (provider, provider_item_id) in provider_ids {
        let delete = Query::delete()
            .from_table(Alias::new("provider_ids"))
            .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid()))
            .and_where(Expr::col(Alias::new("provider")).eq(provider.as_str()))
            .to_owned();
        transaction.execute(backend.build(&delete)).await?;
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
                item_id.as_uuid().into(),
                provider.as_str().into(),
                provider_item_id.as_str().into(),
            ])
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
    }
    Ok(())
}

struct DesiredProvenance {
    field: String,
    provider: String,
    reference: Option<String>,
    value_hash: String,
}

fn desired_provenance(
    resolution: &MetadataResolution,
) -> Result<Vec<DesiredProvenance>, MetadataPublicationError> {
    resolution
        .provenance_entries()
        .map(|(field, provenance)| {
            let value = field_value(resolution, field)
                .ok_or(MetadataPublicationError::InvalidResolution)?;
            Ok(DesiredProvenance {
                field: field.to_owned(),
                provider: provenance.provider().to_owned(),
                reference: provenance.reference().map(str::to_owned),
                value_hash: format!("{:x}", Sha256::digest(value.as_bytes())),
            })
        })
        .collect()
}

fn field_value(resolution: &MetadataResolution, field: &str) -> Option<String> {
    match field {
        "title" => Some(resolution.title().to_owned()),
        "original_title" => resolution.original_title().map(str::to_owned),
        "overview" => resolution.overview().map(str::to_owned),
        "production_year" => resolution.production_year().map(|year| year.to_string()),
        _ => field
            .strip_prefix("provider_id:")
            .and_then(|provider| resolution.provider_ids().get(provider).cloned()),
    }
}

async fn provenance_matches(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    desired: &[DesiredProvenance],
) -> Result<bool, DbErr> {
    let query = Query::select()
        .columns([
            Alias::new("field_name"),
            Alias::new("source_provider"),
            Alias::new("source_reference"),
            Alias::new("value_hash"),
        ])
        .from(Alias::new("metadata_provenance"))
        .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid()))
        .to_owned();
    let backend = transaction.get_database_backend();
    let rows = transaction.query_all(backend.build(&query)).await?;
    Ok(desired.iter().all(|expected| {
        rows.iter()
            .filter(|row| {
                row.try_get::<String>("", "field_name").ok().as_deref()
                    == Some(expected.field.as_str())
            })
            .count()
            == 1
            && rows.iter().any(|row| provenance_row_matches(row, expected))
    }))
}

fn provenance_row_matches(row: &QueryResult, desired: &DesiredProvenance) -> bool {
    row.try_get::<String>("", "field_name").ok().as_deref() == Some(&desired.field)
        && row.try_get::<String>("", "source_provider").ok().as_deref() == Some(&desired.provider)
        && row
            .try_get::<Option<String>>("", "source_reference")
            .ok()
            .flatten()
            == desired.reference
        && row.try_get::<String>("", "value_hash").ok().as_deref() == Some(&desired.value_hash)
}

async fn publish_provenance(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    provenance: &[DesiredProvenance],
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    for entry in provenance {
        let delete = Query::delete()
            .from_table(Alias::new("metadata_provenance"))
            .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid()))
            .and_where(Expr::col(Alias::new("field_name")).eq(entry.field.as_str()))
            .to_owned();
        transaction.execute(backend.build(&delete)).await?;
        let conflict = if backend == sea_orm::DbBackend::MySql {
            OnConflict::new().update_column(Alias::new("id")).to_owned()
        } else {
            OnConflict::columns([
                Alias::new("catalog_item_id"),
                Alias::new("field_name"),
                Alias::new("source_provider"),
            ])
            .do_nothing()
            .to_owned()
        };
        let insert = Query::insert()
            .into_table(Alias::new("metadata_provenance"))
            .columns([
                Alias::new("id"),
                Alias::new("catalog_item_id"),
                Alias::new("field_name"),
                Alias::new("source_provider"),
                Alias::new("source_reference"),
                Alias::new("value_hash"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                item_id.as_uuid().into(),
                entry.field.as_str().into(),
                entry.provider.as_str().into(),
                entry.reference.as_deref().into(),
                entry.value_hash.as_str().into(),
            ])
            .on_conflict(conflict)
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
    }
    Ok(())
}

async fn associations_changed(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    resolution: &MetadataResolution,
) -> Result<bool, DbErr> {
    if let Some(genres) = resolution.genres()
        && read_association_names(transaction, item_id, "item_genres", "genres", "genre_id").await?
            != sorted_names(genres)
    {
        return Ok(true);
    }
    if let Some(studios) = resolution.studios()
        && read_association_names(transaction, item_id, "item_studios", "studios", "studio_id")
            .await?
            != sorted_names(studios)
    {
        return Ok(true);
    }
    if let Some(people) = resolution.people() {
        let desired = people
            .iter()
            .enumerate()
            .map(|(index, person)| {
                Ok((
                    person.name().to_owned(),
                    person.role().unwrap_or_default().to_owned(),
                    person
                        .order()
                        .map_or_else(|| i32::try_from(index), i32::try_from)?,
                ))
            })
            .collect::<Result<Vec<_>, std::num::TryFromIntError>>()
            .map_err(|error| DbErr::Custom(error.to_string()))?;
        if read_people(transaction, item_id).await? != desired {
            return Ok(true);
        }
    }
    Ok(false)
}

fn sorted_names(values: &[String]) -> Vec<String> {
    let mut values = values.to_vec();
    values.sort();
    values
}

async fn read_association_names(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    link_table: &str,
    value_table: &str,
    value_id_column: &str,
) -> Result<Vec<String>, DbErr> {
    let link = Alias::new("metadata_association_link");
    let value = Alias::new("metadata_association_value");
    let query = Query::select()
        .expr_as(
            Expr::col((value.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .from_as(Alias::new(link_table), link.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new(value_table),
            value.clone(),
            Expr::col((value.clone(), Alias::new("id")))
                .equals((link.clone(), Alias::new(value_id_column))),
        )
        .and_where(Expr::col((link, Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
        .order_by((value, Alias::new("name")), Order::Asc)
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| row.try_get("", "name"))
        .collect()
}

async fn read_people(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
) -> Result<Vec<(String, String, i32)>, DbErr> {
    let link = Alias::new("metadata_person_link");
    let person = Alias::new("metadata_person");
    let query = Query::select()
        .expr_as(
            Expr::col((person.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((link.clone(), Alias::new("role"))),
            Alias::new("role"),
        )
        .expr_as(
            Expr::col((link.clone(), Alias::new("sort_order"))),
            Alias::new("sort_order"),
        )
        .from_as(Alias::new("item_people"), link.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("people"),
            person,
            Expr::col((Alias::new("metadata_person"), Alias::new("id")))
                .equals((link.clone(), Alias::new("person_id"))),
        )
        .and_where(Expr::col((link.clone(), Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
        .order_by((link, Alias::new("sort_order")), Order::Asc)
        .to_owned();
    let backend = transaction.get_database_backend();
    transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok((
                row.try_get("", "name")?,
                row.try_get("", "role")?,
                row.try_get("", "sort_order")?,
            ))
        })
        .collect()
}

async fn publish_associations(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    resolution: &MetadataResolution,
) -> Result<(), DbErr> {
    if let Some(genres) = resolution.genres() {
        replace_named_associations(
            transaction,
            item_id,
            "item_genres",
            "genres",
            "genre_id",
            genres,
        )
        .await?;
    }
    if let Some(studios) = resolution.studios() {
        replace_named_associations(
            transaction,
            item_id,
            "item_studios",
            "studios",
            "studio_id",
            studios,
        )
        .await?;
    }
    if let Some(people) = resolution.people() {
        replace_people(transaction, item_id, people).await?;
    }
    Ok(())
}

async fn replace_named_associations(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    link_table: &str,
    value_table: &str,
    value_id_column: &str,
    names: &[String],
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new(link_table))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid())),
            ),
        )
        .await?;
    for name in names {
        let conflict = if backend == sea_orm::DbBackend::MySql {
            OnConflict::column(Alias::new("name"))
                .update_column(Alias::new("name"))
                .to_owned()
        } else {
            OnConflict::column(Alias::new("name"))
                .do_nothing()
                .to_owned()
        };
        transaction
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new(value_table))
                        .columns([Alias::new("id"), Alias::new("name")])
                        .values_panic([Uuid::new_v4().into(), name.as_str().into()])
                        .on_conflict(conflict),
                ),
            )
            .await?;
        let value_id: Uuid = transaction
            .query_one(
                backend.build(
                    Query::select()
                        .column(Alias::new("id"))
                        .from(Alias::new(value_table))
                        .and_where(Expr::col(Alias::new("name")).eq(name.as_str()))
                        .limit(1),
                ),
            )
            .await?
            .ok_or_else(|| DbErr::Custom("metadata association value is missing".to_owned()))?
            .try_get("", "id")?;
        transaction
            .execute(
                backend.build(
                    Query::insert()
                        .into_table(Alias::new(link_table))
                        .columns([
                            Alias::new("id"),
                            Alias::new("catalog_item_id"),
                            Alias::new(value_id_column),
                        ])
                        .values_panic([
                            Uuid::new_v4().into(),
                            item_id.as_uuid().into(),
                            value_id.into(),
                        ]),
                ),
            )
            .await?;
    }
    Ok(())
}

async fn replace_people(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    people: &[tjxy_metadata::MetadataPerson],
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("item_people"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item_id.as_uuid())),
            ),
        )
        .await?;
    for (index, person) in people.iter().enumerate() {
        let select = Query::select()
            .column(Alias::new("id"))
            .from(Alias::new("people"))
            .and_where(Expr::col(Alias::new("name")).eq(person.name()))
            .order_by(Alias::new("id"), Order::Asc)
            .limit(1)
            .to_owned();
        let person_id = if let Some(row) = transaction.query_one(backend.build(&select)).await? {
            row.try_get("", "id")?
        } else {
            let person_id = Uuid::new_v4();
            transaction
                .execute(
                    backend.build(
                        Query::insert()
                            .into_table(Alias::new("people"))
                            .columns([
                                Alias::new("id"),
                                Alias::new("name"),
                                Alias::new("sort_name"),
                            ])
                            .values_panic([
                                person_id.into(),
                                person.name().into(),
                                person.name().to_lowercase().into(),
                            ]),
                    ),
                )
                .await?;
            person_id
        };
        let sort_order = person
            .order()
            .map_or_else(|| i32::try_from(index), i32::try_from)
            .map_err(|error| DbErr::Custom(error.to_string()))?;
        transaction
            .execute(
                backend.build(
                    Query::insert()
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
                            item_id.as_uuid().into(),
                            person_id.into(),
                            person.role().unwrap_or_default().into(),
                            sort_order.into(),
                        ]),
                ),
            )
            .await?;
    }
    Ok(())
}

async fn bump_generation(
    transaction: &DatabaseTransaction,
) -> Result<(), MetadataPublicationError> {
    crate::advance_catalog_generation(transaction).await?;
    Ok(())
}

fn validate_resolution(resolution: &MetadataResolution) -> Result<(), MetadataPublicationError> {
    if !valid_text(resolution.title(), MAX_TITLE_CHARS)
        || resolution
            .original_title()
            .is_some_and(|value| !valid_text(value, MAX_TITLE_CHARS))
        || resolution
            .overview()
            .is_some_and(|value| !valid_text(value, MAX_OVERVIEW_CHARS))
        || resolution
            .production_year()
            .is_some_and(|year| !(1..=9999).contains(&year))
        || resolution
            .provider_ids()
            .iter()
            .any(|(provider, id)| !valid_text(provider, 128) || !valid_text(id, 2048))
    {
        return Err(MetadataPublicationError::InvalidResolution);
    }
    Ok(())
}

fn item_kind(kind: MetadataItemKind) -> &'static str {
    match kind {
        MetadataItemKind::Audio => "Audio",
        MetadataItemKind::Movie => "Movie",
        MetadataItemKind::Series => "Series",
        MetadataItemKind::Season => "Season",
        MetadataItemKind::Episode => "Episode",
    }
}

fn parse_item_kind(kind: &str) -> Result<MetadataItemKind, MetadataPublicationError> {
    match kind {
        "Audio" => Ok(MetadataItemKind::Audio),
        "Movie" => Ok(MetadataItemKind::Movie),
        "Series" => Ok(MetadataItemKind::Series),
        "Season" => Ok(MetadataItemKind::Season),
        "Episode" => Ok(MetadataItemKind::Episode),
        _ => Err(MetadataPublicationError::ItemKindMismatch),
    }
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, MetadataPublicationError>,
) -> Result<T, MetadataPublicationError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(MetadataPublicationError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
