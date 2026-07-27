use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, IsolationLevel, QueryResult,
    TransactionTrait,
    sea_query::{Alias, Expr, Order, Query},
};
use thiserror::Error;
use tjxy_common::{CatalogItemId, MediaSourceId, StorageObjectRecordId, StorageRootId};
use uuid::Uuid;

use crate::{
    CatalogPublicationError, WorkJobRepositoryError, WorkJobSpec, WorkJobSubmission, WorkScope,
    WorkTaskKind, catalog_query::lock_catalog_item_visibility,
    source_publication::effective_source_publication, work_job::enqueue_in_transaction,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManualProbeSubmission {
    media_source_id: MediaSourceId,
    submission: WorkJobSubmission,
}

impl ManualProbeSubmission {
    #[must_use]
    pub const fn media_source_id(&self) -> MediaSourceId {
        self.media_source_id
    }

    #[must_use]
    pub const fn submission(&self) -> &WorkJobSubmission {
        &self.submission
    }
}

#[derive(Debug, Error)]
pub enum ManualProbeError {
    #[error("catalog item is missing or unavailable to enabled libraries")]
    ItemUnavailable,
    #[error("catalog item has no active media sources; run source indexing first")]
    NoActiveMediaSources,
    #[error("catalog item has no active media source with an available location")]
    NoAvailableMediaSources,
    #[error("catalog item exceeds the explicit Probe source batch limit")]
    TooManyMediaSources,
    #[error("active source publication changed during manual Probe submission")]
    StalePublication,
    #[error("manual Probe source limit must be positive")]
    InvalidSourceLimit,
    #[error("active media source query failed: {0}")]
    Publication(#[from] CatalogPublicationError),
    #[error("durable Probe enqueue failed: {0}")]
    Work(#[from] WorkJobRepositoryError),
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}

pub struct ManualProbeRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> ManualProbeRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Selects available sources and atomically enqueues or joins their Probe jobs.
    ///
    /// Selection reads at most `max_sources + 1` lightweight rows. The visibility,
    /// effective publication, candidate revisions, and durable jobs share one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`ManualProbeError`] when the item is unavailable, no source can be
    /// probed, the bounded source limit is exceeded, or the transaction fails.
    pub async fn enqueue_item(
        &self,
        item_id: CatalogItemId,
        priority: i32,
        max_sources: usize,
    ) -> Result<Vec<ManualProbeSubmission>, ManualProbeError> {
        if max_sources == 0 {
            return Err(ManualProbeError::InvalidSourceLimit);
        }
        let row_limit = u64::try_from(max_sources)
            .ok()
            .and_then(|limit| limit.checked_add(1))
            .ok_or(ManualProbeError::InvalidSourceLimit)?;
        let transaction = self
            .database
            .begin_with_config(Some(IsolationLevel::Serializable), None)
            .await?;
        let result = enqueue_item(&transaction, item_id, priority, max_sources, row_limit).await;
        finish(transaction, result).await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProbeCandidate {
    media_source_id: MediaSourceId,
    probe_revision: i64,
    storage_root_id: StorageRootId,
}

async fn enqueue_item(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    priority: i32,
    max_sources: usize,
    row_limit: u64,
) -> Result<Vec<ManualProbeSubmission>, ManualProbeError> {
    if !lock_catalog_item(transaction, item_id).await? {
        return Err(ManualProbeError::ItemUnavailable);
    }
    lock_structure_owner(transaction, item_id).await?;
    if !lock_catalog_item_visibility(transaction, item_id).await? {
        return Err(ManualProbeError::ItemUnavailable);
    }
    let Some(publication_id) = effective_source_publication(transaction, item_id).await? else {
        return Err(ManualProbeError::NoActiveMediaSources);
    };
    if !active_source_exists(transaction, item_id, publication_id).await? {
        return Err(ManualProbeError::NoActiveMediaSources);
    }

    let initial = available_candidates(transaction, item_id, publication_id, row_limit).await?;
    validate_candidates(&initial, max_sources)?;
    lock_candidate_state(transaction, publication_id, &initial).await?;

    // Re-read after taking source/location locks so revisions and availability
    // describe the same transaction snapshot used for durable enqueue.
    let candidates = available_candidates(transaction, item_id, publication_id, row_limit).await?;
    validate_candidates(&candidates, max_sources)?;
    lock_candidate_state(transaction, publication_id, &candidates).await?;
    if effective_source_publication(transaction, item_id).await? != Some(publication_id) {
        return Err(ManualProbeError::StalePublication);
    }

    let now = Utc::now();
    let mut submissions = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let spec = WorkJobSpec::new(
            WorkTaskKind::ProbeMedia,
            WorkScope::MediaSource(candidate.media_source_id),
            candidate.probe_revision,
            priority,
        )?
        .with_storage_root_affinity(candidate.storage_root_id)?;
        let submission = enqueue_in_transaction(transaction, &spec, now).await?;
        submissions.push(ManualProbeSubmission {
            media_source_id: candidate.media_source_id,
            submission,
        });
    }
    Ok(submissions)
}

async fn lock_structure_owner(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
) -> Result<(), ManualProbeError> {
    let item = Alias::new("manual_probe_item");
    let query = Query::select()
        .column((item.clone(), Alias::new("structure_owner_item_id")))
        .from_as(Alias::new("catalog_items"), item.clone())
        .and_where(Expr::col((item, Alias::new("id"))).eq(item_id.as_uuid()))
        .to_owned();
    let owner = transaction
        .query_one(transaction.get_database_backend().build(&query))
        .await?
        .ok_or(ManualProbeError::ItemUnavailable)?
        .try_get::<Option<Uuid>>("", "structure_owner_item_id")?
        .map(CatalogItemId::from_uuid);
    if let Some(owner) = owner
        && owner != item_id
        && !lock_catalog_item(transaction, owner).await?
    {
        return Err(ManualProbeError::ItemUnavailable);
    }
    Ok(())
}

async fn lock_catalog_item(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
) -> Result<bool, DbErr> {
    let item = Alias::new("catalog_items");
    let update = Query::update()
        .table(item.clone())
        .value(Alias::new("is_present"), true)
        .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item, Alias::new("classification_state"))).eq("Matched"))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&update))
        .await
        .map(|result| result.rows_affected() > 0)
}

fn validate_candidates(
    candidates: &[ProbeCandidate],
    max_sources: usize,
) -> Result<(), ManualProbeError> {
    if candidates.is_empty() {
        Err(ManualProbeError::NoAvailableMediaSources)
    } else if candidates.len() > max_sources {
        Err(ManualProbeError::TooManyMediaSources)
    } else {
        Ok(())
    }
}

async fn active_source_exists(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    publication_id: Uuid,
) -> Result<bool, DbErr> {
    let source = Alias::new("manual_probe_active_source");
    let query = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("publication_media_sources"), source.clone())
        .and_where(Expr::col((source.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(Expr::col((source, Alias::new("catalog_item_id"))).eq(item_id.as_uuid()))
        .limit(1)
        .to_owned();
    transaction
        .query_one(transaction.get_database_backend().build(&query))
        .await
        .map(|row| row.is_some())
}

async fn available_candidates(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    publication_id: Uuid,
    row_limit: u64,
) -> Result<Vec<ProbeCandidate>, DbErr> {
    let projected = Alias::new("manual_probe_projected_source");
    let source = Alias::new("manual_probe_source");
    let location = Alias::new("manual_probe_projected_location");
    let canonical_location = Alias::new("manual_probe_location");
    let available_location = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("publication_media_locations"), location.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("media_locations"),
            canonical_location.clone(),
            Expr::col((canonical_location.clone(), Alias::new("id")))
                .equals((location.clone(), Alias::new("media_location_id"))),
        )
        .and_where(Expr::col((location.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(
            Expr::col((location, Alias::new("media_source_id")))
                .equals((projected.clone(), Alias::new("media_source_id"))),
        )
        .and_where(
            Expr::col((canonical_location, Alias::new("availability_state"))).eq("Available"),
        )
        .to_owned();
    let query = Query::select()
        .expr_as(
            Expr::col((projected.clone(), Alias::new("media_source_id"))),
            Alias::new("media_source_id"),
        )
        .expr_as(
            Expr::col((source.clone(), Alias::new("probe_revision"))),
            Alias::new("probe_revision"),
        )
        .from_as(Alias::new("publication_media_sources"), projected.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("media_sources"),
            source.clone(),
            Expr::col((source, Alias::new("id")))
                .equals((projected.clone(), Alias::new("media_source_id"))),
        )
        .and_where(Expr::col((projected.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(
            Expr::col((projected.clone(), Alias::new("catalog_item_id"))).eq(item_id.as_uuid()),
        )
        .and_where(Expr::exists(available_location))
        .order_by((projected, Alias::new("presentation_key")), Order::Asc)
        .limit(row_limit)
        .to_owned();
    let candidates = transaction
        .query_all(transaction.get_database_backend().build(&query))
        .await?
        .iter()
        .map(candidate_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let mut authorized = Vec::with_capacity(candidates.len());
    for mut candidate in candidates {
        let Some(root_id) = authorized_probe_root(
            transaction,
            item_id,
            publication_id,
            candidate.media_source_id,
        )
        .await?
        else {
            continue;
        };
        candidate.storage_root_id = root_id;
        authorized.push(candidate);
    }
    Ok(authorized)
}

fn candidate_from_row(row: &QueryResult) -> Result<ProbeCandidate, DbErr> {
    Ok(ProbeCandidate {
        media_source_id: MediaSourceId::from_uuid(row.try_get("", "media_source_id")?),
        probe_revision: row.try_get("", "probe_revision")?,
        storage_root_id: StorageRootId::from_uuid(Uuid::nil()),
    })
}

#[allow(clippy::too_many_lines)] // Keeps source, publication-root, and library-root authorization in one query.
async fn authorized_probe_root(
    transaction: &DatabaseTransaction,
    item_id: CatalogItemId,
    publication_id: Uuid,
    source_id: MediaSourceId,
) -> Result<Option<StorageRootId>, DbErr> {
    let projected = Alias::new("manual_probe_authorized_location");
    let location = Alias::new("manual_probe_authorized_canonical_location");
    let object = Alias::new("manual_probe_authorized_object");
    let relation = Alias::new("manual_probe_authorized_relation");
    let root = Alias::new("manual_probe_authorized_root");
    let account = Alias::new("manual_probe_authorized_account");
    let publication = Alias::new("manual_probe_authorized_publication");
    let publication_job = Alias::new("manual_probe_authorized_publication_job");
    let item = Alias::new("manual_probe_authorized_item");
    let membership = Alias::new("manual_probe_authorized_membership");
    let library_root = Alias::new("manual_probe_authorized_library_root");
    let library = Alias::new("manual_probe_authorized_library");
    let query = Query::select()
        .expr_as(
            Expr::col((relation.clone(), Alias::new("storage_root_id"))),
            Alias::new("storage_root_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("id"))),
            Alias::new("storage_object_id"),
        )
        .from_as(Alias::new("publication_media_locations"), projected.clone())
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("media_locations"),
            location.clone(),
            Expr::col((location.clone(), Alias::new("id")))
                .equals((projected.clone(), Alias::new("media_location_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((projected.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            relation.clone(),
            Expr::col((relation.clone(), Alias::new("storage_object_id")))
                .equals((object.clone(), Alias::new("id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((root.clone(), Alias::new("id")))
                        .equals((relation.clone(), Alias::new("storage_root_id"))),
                )
                .add(
                    Expr::col((root.clone(), Alias::new("storage_account_id")))
                        .equals((object.clone(), Alias::new("storage_account_id"))),
                ),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            account.clone(),
            Expr::col((account.clone(), Alias::new("id")))
                .equals((root.clone(), Alias::new("storage_account_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id"))).eq(publication_id),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("work_jobs"),
            publication_job.clone(),
            Expr::col((publication_job.clone(), Alias::new("id")))
                .equals((publication.clone(), Alias::new("job_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id"))).eq(item_id.as_uuid()),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            library_root.clone(),
            Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                .equals((relation.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            sea_orm::sea_query::Cond::all()
                .add(
                    Expr::col((membership.clone(), Alias::new("library_id")))
                        .equals((library_root.clone(), Alias::new("library_id"))),
                )
                .add(
                    sea_orm::sea_query::Cond::any()
                        .add(
                            Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                                .equals((item.clone(), Alias::new("id"))),
                        )
                        .add(
                            Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                                .equals((item.clone(), Alias::new("structure_owner_item_id"))),
                        ),
                ),
        )
        .join_as(
            sea_orm::sea_query::JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((library_root.clone(), Alias::new("library_id"))),
        )
        .and_where(Expr::col((projected.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(
            Expr::col((projected.clone(), Alias::new("media_source_id"))).eq(source_id.as_uuid()),
        )
        .and_where(Expr::col((location, Alias::new("availability_state"))).eq("Available"))
        .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((relation.clone(), Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((account, Alias::new("status"))).is_in(["Active", "Ready"]))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .cond_where(
            sea_orm::sea_query::Cond::any()
                .add(
                    Expr::col((publication_job.clone(), Alias::new("storage_root_affinity")))
                        .eq(Uuid::nil()),
                )
                .add(
                    Expr::col((publication_job, Alias::new("storage_root_affinity")))
                        .equals((relation.clone(), Alias::new("storage_root_id"))),
                ),
        )
        .order_by((projected, Alias::new("priority")), Order::Desc)
        .order_by((relation, Alias::new("storage_root_id")), Order::Asc)
        .to_owned();
    let backend = transaction.get_database_backend();
    for row in transaction.query_all(backend.build(&query)).await? {
        let root_id = StorageRootId::from_uuid(row.try_get("", "storage_root_id")?);
        let object_id = StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?);
        if crate::storage_path_authorization::storage_path_is_authorized(
            transaction,
            root_id,
            object_id,
            crate::storage_path_authorization::StoragePathAvailability::Present,
        )
        .await?
        {
            return Ok(Some(root_id));
        }
    }
    Ok(None)
}

async fn lock_candidate_state(
    transaction: &DatabaseTransaction,
    publication_id: Uuid,
    candidates: &[ProbeCandidate],
) -> Result<(), DbErr> {
    lock_candidate_sources(transaction, candidates).await?;
    if candidates.is_empty() {
        return Ok(());
    }
    let ids = candidates
        .iter()
        .map(|candidate| candidate.media_source_id.as_uuid())
        .collect::<Vec<_>>();
    let projected = Alias::new("manual_probe_lock_projected_location");
    let locations = Query::select()
        .column((projected.clone(), Alias::new("media_location_id")))
        .from_as(Alias::new("publication_media_locations"), projected.clone())
        .and_where(Expr::col((projected.clone(), Alias::new("publication_id"))).eq(publication_id))
        .and_where(Expr::col((projected, Alias::new("media_source_id"))).is_in(ids))
        .to_owned();
    let location = Alias::new("media_locations");
    let update = Query::update()
        .table(location.clone())
        .value(Alias::new("availability_state"), "Available")
        .and_where(Expr::col((location.clone(), Alias::new("id"))).in_subquery(locations))
        .and_where(Expr::col((location, Alias::new("availability_state"))).eq("Available"))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&update))
        .await?;
    Ok(())
}

async fn lock_candidate_sources(
    transaction: &DatabaseTransaction,
    candidates: &[ProbeCandidate],
) -> Result<(), DbErr> {
    if candidates.is_empty() {
        return Ok(());
    }
    let ids = candidates
        .iter()
        .map(|candidate| candidate.media_source_id.as_uuid())
        .collect::<Vec<_>>();
    let source = Alias::new("media_sources");
    let update = Query::update()
        .table(source.clone())
        .value(
            Alias::new("probe_revision"),
            Expr::col((source.clone(), Alias::new("probe_revision"))),
        )
        .and_where(Expr::col((source, Alias::new("id"))).is_in(ids))
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&update))
        .await?;
    Ok(())
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, ManualProbeError>,
) -> Result<T, ManualProbeError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(ManualProbeError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}
