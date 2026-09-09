use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, Order, Query},
};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

const MAX_LEASE_OWNER_CHARS: usize = 128;
const RETENTION_BATCH_SIZE: u64 = 100;
const CHILD_ROW_BATCH_SIZE: u64 = 500;
const LEGACY_ENROLL_LIMIT: u64 = 1_000;
const LEGACY_ENROLL_INSERT_BATCH_SIZE: usize = 200;
const TERMINAL_STATES: [&str; 2] = ["Completed", "Failed"];
const ACTIVE_STATES: [&str; 2] = ["Pending", "Running"];
const DEFERRED_REASON: &str = "dependency active";

pub struct WorkRetentionRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> WorkRetentionRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Processes one bounded batch whose forward retention period has elapsed.
    ///
    /// # Errors
    ///
    /// Returns validation, timestamp, database, or rollback failures.
    pub async fn run_once(
        &self,
        lease_owner: &str,
        retention: Duration,
        lease_duration: Duration,
    ) -> Result<WorkRetentionRun, WorkRetentionError> {
        validate(lease_owner, retention, lease_duration)?;
        let now = Utc::now();
        let cutoff = now
            .checked_sub_signed(retention)
            .ok_or(WorkRetentionError::TimestampOverflow)?;
        let lease_expires_at = now
            .checked_add_signed(lease_duration)
            .ok_or(WorkRetentionError::TimestampOverflow)?;
        let transaction = self.database.begin().await?;
        let claim =
            claim_next_batch(&transaction, lease_owner, now, cutoff, lease_expires_at).await;
        let claimed = finish(transaction, claim).await?;
        let Some(claimed) = claimed else {
            let transaction = self.database.begin().await?;
            let enrollment = enroll_legacy(&transaction, now, cutoff).await;
            let count = finish(transaction, enrollment).await?;
            if count != 0 {
                return Ok(WorkRetentionRun::EnrolledLegacy { count });
            }
            return Ok(WorkRetentionRun::Idle);
        };

        let transaction = self.database.begin().await?;
        let result = process_batch(&transaction, &claimed, now).await;
        finish(transaction, result).await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkRetentionRun {
    Idle,
    EnrolledLegacy {
        count: u64,
    },
    Processed {
        deleted: u64,
        compacted: u64,
        purged: u64,
        deferred: u64,
    },
}

struct RetentionClaimBatch {
    job_ids: Vec<Uuid>,
    lease_token: String,
}

struct RetentionClassification {
    compacted: Vec<Uuid>,
    deferred: Vec<Uuid>,
    deleted: Vec<Uuid>,
    /// Terminal jobs whose retired, unreferenced publication projection is
    /// deleted alongside the job instead of being compacted forever.
    purged: Vec<(Uuid, Uuid)>,
    missing: Vec<Uuid>,
}

async fn enroll_legacy(
    transaction: &DatabaseTransaction,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
) -> Result<u64, WorkRetentionError> {
    let entries = legacy_entries(transaction, now, cutoff).await?;
    Ok(enqueue_entries(transaction, &entries).await?)
}

async fn enqueue_entries(
    transaction: &DatabaseTransaction,
    entries: &[(Uuid, DateTime<Utc>)],
) -> Result<u64, DbErr> {
    let backend = transaction.get_database_backend();
    let conflict = if backend == sea_orm::DbBackend::MySql {
        sea_orm::sea_query::OnConflict::new()
            .update_column(Alias::new("job_id"))
            .to_owned()
    } else {
        sea_orm::sea_query::OnConflict::new()
            .do_nothing()
            .to_owned()
    };
    let mut enrolled = 0_u64;
    for entries in entries.chunks(LEGACY_ENROLL_INSERT_BATCH_SIZE) {
        let mut insert = Query::insert();
        insert
            .into_table(Alias::new("work_job_retention_queue"))
            .columns([
                Alias::new("job_id"),
                Alias::new("terminal_at"),
                Alias::new("attempt_count"),
            ])
            .on_conflict(conflict.clone());
        for (job_id, terminal_at) in entries {
            insert.values_panic([
                job_id.to_owned().into(),
                terminal_at.to_owned().into(),
                0_i32.into(),
            ]);
        }
        enrolled += transaction
            .execute(backend.build(&insert))
            .await?
            .rows_affected();
    }
    Ok(enrolled)
}

/// Re-enroll a compacted publication's terminal job as part of retirement.
/// Keep its original terminal time and preserve any existing queue lease.
pub(crate) async fn enqueue_retired_publication(
    transaction: &DatabaseTransaction,
    publication_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), DbErr> {
    let job = Alias::new("retired_job");
    let publication = Alias::new("retired_publication");
    let row = transaction
        .query_one(
            transaction.get_database_backend().build(
                Query::select()
                    .columns([
                        (job.clone(), Alias::new("id")),
                        (job.clone(), Alias::new("completed_at")),
                        (job.clone(), Alias::new("created_at")),
                    ])
                    .from_as(Alias::new("work_jobs"), job.clone())
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new("catalog_publications"),
                        publication.clone(),
                        Expr::col((publication.clone(), Alias::new("job_id")))
                            .equals((job.clone(), Alias::new("id"))),
                    )
                    .and_where(
                        Expr::col((publication.clone(), Alias::new("id"))).eq(publication_id),
                    )
                    .and_where(
                        Expr::col((publication, Alias::new("state")))
                            .eq(crate::catalog_publication::STATE_RETIRED),
                    )
                    .and_where(Expr::col((job, Alias::new("state"))).is_in(TERMINAL_STATES)),
            ),
        )
        .await?;
    if let Some(row) = row {
        let job_id = row.try_get::<Uuid>("", "id")?;
        let terminal_at = row
            .try_get::<Option<DateTime<Utc>>>("", "completed_at")?
            .or(row.try_get::<Option<DateTime<Utc>>>("", "created_at")?)
            .unwrap_or(now);
        enqueue_entries(transaction, &[(job_id, terminal_at)]).await?;
    }
    Ok(())
}

async fn legacy_entries(
    transaction: &DatabaseTransaction,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
) -> Result<Vec<(Uuid, DateTime<Utc>)>, WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let job = Alias::new("legacy_job");
    let queue = Alias::new("legacy_queue");
    let publication = Alias::new("legacy_publication");
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .columns([
                        (job.clone(), Alias::new("id")),
                        (job.clone(), Alias::new("completed_at")),
                        (job.clone(), Alias::new("created_at")),
                    ])
                    .from_as(Alias::new("work_jobs"), job.clone())
                    .join_as(
                        JoinType::LeftJoin,
                        Alias::new("work_job_retention_queue"),
                        queue.clone(),
                        Expr::col((queue.clone(), Alias::new("job_id")))
                            .equals((job.clone(), Alias::new("id"))),
                    )
                    .join_as(
                        JoinType::LeftJoin,
                        Alias::new("catalog_publications"),
                        publication.clone(),
                        Expr::col((publication.clone(), Alias::new("job_id")))
                            .equals((job.clone(), Alias::new("id"))),
                    )
                    .and_where(Expr::col((job.clone(), Alias::new("state"))).is_in(TERMINAL_STATES))
                    .cond_where(
                        Cond::any()
                            .add(Expr::col((job.clone(), Alias::new("completed_at"))).lte(cutoff))
                            .add(
                                Cond::all()
                                    .add(
                                        Expr::col((job.clone(), Alias::new("completed_at")))
                                            .is_null(),
                                    )
                                    .add(
                                        Cond::any()
                                            .add(
                                                Expr::col((job.clone(), Alias::new("created_at")))
                                                    .lte(cutoff),
                                            )
                                            .add(
                                                Expr::col((job.clone(), Alias::new("created_at")))
                                                    .is_null(),
                                            ),
                                    ),
                            ),
                    )
                    .and_where(Expr::col((queue, Alias::new("job_id"))).is_null())
                    .cond_where(legacy_publication_condition(&publication))
                    .order_by((job.clone(), Alias::new("completed_at")), Order::Asc)
                    .order_by((job, Alias::new("id")), Order::Asc)
                    .limit(LEGACY_ENROLL_LIMIT),
            ),
        )
        .await?;
    let entries = rows
        .iter()
        .map(|row| {
            Ok((
                row.try_get::<Uuid>("", "id")?,
                row.try_get::<Option<DateTime<Utc>>>("", "completed_at")?
                    .or(row.try_get::<Option<DateTime<Utc>>>("", "created_at")?)
                    .unwrap_or(now),
            ))
        })
        .collect::<Result<Vec<_>, DbErr>>()?;
    Ok(entries)
}

fn legacy_publication_condition(publication: &Alias) -> Cond {
    let mut retired = Cond::all().add(
        Expr::col((publication.clone(), Alias::new("state")))
            .eq(crate::catalog_publication::STATE_RETIRED),
    );
    // Uncorrelated sets avoid scanning the catalog once per legacy job.
    // Exclude NULL explicitly so NOT IN does not hide unreferenced rows.
    for column in [
        "active_structure_publication_id",
        "active_source_publication_id",
    ] {
        let references = Query::select()
            .column(Alias::new(column))
            .from(Alias::new("catalog_items"))
            .and_where(Expr::col(Alias::new(column)).is_not_null())
            .to_owned();
        retired = retired.add(
            Expr::col((publication.clone(), Alias::new("id")))
                .in_subquery(references)
                .not(),
        );
    }
    Cond::any()
        .add(Expr::col((publication.clone(), Alias::new("job_id"))).is_null())
        .add(retired)
}

fn validate(
    lease_owner: &str,
    retention: Duration,
    lease_duration: Duration,
) -> Result<(), WorkRetentionError> {
    if lease_owner.trim().is_empty() {
        return Err(WorkRetentionError::EmptyLeaseOwner);
    }
    if lease_owner.chars().count() > MAX_LEASE_OWNER_CHARS {
        return Err(WorkRetentionError::LeaseOwnerTooLong);
    }
    if retention <= Duration::zero() {
        return Err(WorkRetentionError::InvalidRetention);
    }
    if lease_duration <= Duration::zero() {
        return Err(WorkRetentionError::InvalidLeaseDuration);
    }
    Ok(())
}

async fn claim_next_batch(
    transaction: &DatabaseTransaction,
    lease_owner: &str,
    now: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
) -> Result<Option<RetentionClaimBatch>, WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let condition = claimable(now);
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("job_id"))
                    .from(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("terminal_at")).lte(cutoff))
                    .cond_where(condition.clone())
                    .order_by(Alias::new("terminal_at"), Order::Asc)
                    .order_by(Alias::new("job_id"), Order::Asc)
                    .limit(RETENTION_BATCH_SIZE),
            ),
        )
        .await?;
    if rows.is_empty() {
        return Ok(None);
    }
    let lease_token = format!("{lease_owner}:{}", Uuid::new_v4());
    let candidate_ids = rows
        .iter()
        .map(|row| row.try_get("", "job_id"))
        .collect::<Result<Vec<Uuid>, DbErr>>()?;
    let update = Query::update()
        .table(Alias::new("work_job_retention_queue"))
        .value(Alias::new("lease_owner"), &lease_token)
        .value(Alias::new("lease_expires_at"), lease_expires_at)
        .and_where(Expr::col(Alias::new("job_id")).is_in(candidate_ids.iter().copied()))
        .and_where(Expr::col(Alias::new("terminal_at")).lte(cutoff))
        .cond_where(condition)
        .to_owned();
    transaction.execute(backend.build(&update)).await?;
    let job_ids = transaction
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("job_id"))
                    .from(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("job_id")).is_in(candidate_ids))
                    .and_where(Expr::col(Alias::new("lease_owner")).eq(&lease_token))
                    .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now)),
            ),
        )
        .await?
        .iter()
        .map(|row| row.try_get("", "job_id"))
        .collect::<Result<Vec<Uuid>, DbErr>>()?;
    Ok((!job_ids.is_empty()).then_some(RetentionClaimBatch {
        job_ids: sorted_ids(job_ids),
        lease_token,
    }))
}

fn claimable(now: DateTime<Utc>) -> Cond {
    Cond::all()
        .add(
            Cond::any()
                .add(Expr::col(Alias::new("available_at")).is_null())
                .add(Expr::col(Alias::new("available_at")).lte(now)),
        )
        .add(
            Cond::any()
                .add(Expr::col(Alias::new("lease_owner")).is_null())
                .add(Expr::col(Alias::new("lease_expires_at")).lte(now)),
        )
}

async fn process_batch(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaimBatch,
    now: DateTime<Utc>,
) -> Result<WorkRetentionRun, WorkRetentionError> {
    ensure_live_claims(transaction, claimed, now).await?;
    let mut classification = classify_claims(transaction, claimed).await?;
    let cleanup_ids = sorted_ids(
        classification
            .deleted
            .iter()
            .chain(&classification.compacted)
            .chain(classification.purged.iter().map(|(job_id, _)| job_id))
            .copied()
            .collect(),
    );
    if !classification.deferred.is_empty() {
        defer_claims(
            transaction,
            claimed,
            &classification.deferred,
            now,
            Duration::hours(1),
            DEFERRED_REASON,
        )
        .await?;
    }
    if !cleanup_ids.is_empty() {
        clear_terminal_dependencies(transaction, &cleanup_ids).await?;
        delete_child_rows(transaction, &cleanup_ids).await?;
        let mut remaining = remaining_children(transaction, &cleanup_ids).await?;
        remaining.extend(trim_retired_projections(transaction, &classification.purged).await?);
        if !remaining.is_empty() {
            let remaining = remaining.into_iter().collect::<Vec<_>>();
            classification
                .compacted
                .retain(|id| !remaining.contains(id));
            classification.deleted.retain(|id| !remaining.contains(id));
            classification
                .purged
                .retain(|(id, _)| !remaining.contains(id));
            defer_claims(
                transaction,
                claimed,
                &remaining,
                now,
                Duration::milliseconds(100),
                "bounded child cleanup pending",
            )
            .await?;
            classification.deferred.extend(remaining);
        }
    }
    if !classification.purged.is_empty() {
        delete_retired_publications(transaction, &classification.purged).await?;
    }
    let fully_deleted = sorted_ids(
        classification
            .deleted
            .iter()
            .chain(classification.purged.iter().map(|(job_id, _)| job_id))
            .copied()
            .collect(),
    );
    if !fully_deleted.is_empty() {
        delete_jobs(transaction, &fully_deleted).await?;
    }
    // Queue rows for fully deleted jobs (deleted + purged) vanish through the
    // work_job_retention_queue -> work_jobs ON DELETE CASCADE foreign key, so
    // only compacted and missing jobs need an explicit queue-claim delete.
    let queue_ids = sorted_ids(
        classification
            .compacted
            .iter()
            .chain(&classification.missing)
            .copied()
            .collect(),
    );
    if !queue_ids.is_empty() {
        delete_queue_claims(transaction, claimed, &queue_ids, now).await?;
    }
    Ok(WorkRetentionRun::Processed {
        deleted: u64::try_from(
            classification.deleted.len()
                + classification.purged.len()
                + classification.missing.len(),
        )
        .expect("retention batch size fits u64"),
        compacted: u64::try_from(classification.compacted.len())
            .expect("retention batch size fits u64"),
        purged: u64::try_from(classification.purged.len()).expect("retention batch size fits u64"),
        deferred: u64::try_from(classification.deferred.len())
            .expect("retention batch size fits u64"),
    })
}

async fn ensure_live_claims(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaimBatch,
    now: DateTime<Utc>,
) -> Result<(), WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("job_id"))
                    .from(Alias::new("work_job_retention_queue"))
                    .and_where(
                        Expr::col(Alias::new("job_id")).is_in(claimed.job_ids.iter().copied()),
                    )
                    .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
                    .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now)),
            ),
        )
        .await?;
    if rows.len() != claimed.job_ids.len() {
        return Err(WorkRetentionError::LostLease);
    }
    Ok(())
}

async fn classify_claims(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaimBatch,
) -> Result<RetentionClassification, WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .columns([Alias::new("id"), Alias::new("state")])
                    .from(Alias::new("work_jobs"))
                    .and_where(Expr::col(Alias::new("id")).is_in(claimed.job_ids.iter().copied())),
            ),
        )
        .await?;
    let mut states = HashMap::with_capacity(rows.len());
    for row in rows {
        states.insert(
            row.try_get::<Uuid>("", "id")?,
            row.try_get::<String>("", "state")?,
        );
    }
    let existing_ids = states.keys().copied().collect::<Vec<_>>();
    let mut protected_ids = active_dependency_ids(transaction, &existing_ids).await?;
    protected_ids.extend(recovery_cursor_ids(transaction, &existing_ids).await?);
    protected_ids.extend(active_full_scan_child_ids(transaction, &existing_ids).await?);
    let publications = publication_ownership(transaction, &existing_ids).await?;
    let referenced_publications = active_publication_references(transaction, &publications).await?;
    let playing = active_playback_publications(transaction, &publications).await?;
    protected_ids.extend(
        publications
            .iter()
            .filter(|(_, (id, _))| playing.contains(id))
            .map(|(job, _)| *job),
    );
    let mut compacted_ids = Vec::new();
    let mut deferred_ids = Vec::new();
    let mut deleted_ids = Vec::new();
    let mut purged_ids = Vec::new();
    let mut missing_ids = Vec::new();
    for job_id in &claimed.job_ids {
        let Some(state) = states.get(job_id) else {
            missing_ids.push(*job_id);
            continue;
        };
        if !TERMINAL_STATES.contains(&state.as_str()) || protected_ids.contains(job_id) {
            deferred_ids.push(*job_id);
        } else if let Some((publication_id, publication_state)) = publications.get(job_id) {
            if publication_state == crate::catalog_publication::STATE_RETIRED
                && !referenced_publications.contains(publication_id)
            {
                purged_ids.push((*job_id, *publication_id));
            } else {
                compacted_ids.push(*job_id);
            }
        } else {
            deleted_ids.push(*job_id);
        }
    }
    Ok(RetentionClassification {
        compacted: sorted_ids(compacted_ids),
        deferred: sorted_ids(deferred_ids),
        deleted: sorted_ids(deleted_ids),
        purged: sorted_purges(purged_ids),
        missing: sorted_ids(missing_ids),
    })
}

/// Maps each claimed job to its publication identity and state. Publications
/// are unique per job, so at most one entry exists per job id.
async fn publication_ownership(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<HashMap<Uuid, (Uuid, String)>, DbErr> {
    if job_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let backend = transaction.get_database_backend();
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .columns([Alias::new("job_id"), Alias::new("id"), Alias::new("state")])
                    .from(Alias::new("catalog_publications"))
                    .and_where(Expr::col(Alias::new("job_id")).is_in(job_ids.iter().copied())),
            ),
        )
        .await?;
    let mut ownership = HashMap::with_capacity(rows.len());
    for row in rows {
        ownership.insert(
            row.try_get::<Uuid>("", "job_id")?,
            (
                row.try_get::<Uuid>("", "id")?,
                row.try_get::<String>("", "state")?,
            ),
        );
    }
    Ok(ownership)
}

/// Collects publication ids still referenced by a catalog item's active
/// structure or source pointer. Retired publications held by these pointers
/// must stay queryable and therefore cannot be purged.
async fn active_publication_references(
    transaction: &DatabaseTransaction,
    publications: &HashMap<Uuid, (Uuid, String)>,
) -> Result<HashSet<Uuid>, DbErr> {
    let publication_ids = publications
        .values()
        .map(|(publication_id, _)| *publication_id)
        .collect::<Vec<_>>();
    if publication_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let backend = transaction.get_database_backend();
    let mut referenced = HashSet::new();
    for column in [
        "active_structure_publication_id",
        "active_source_publication_id",
    ] {
        let rows = transaction
            .query_all(
                backend.build(
                    Query::select()
                        .column(Alias::new(column))
                        .from(Alias::new("catalog_items"))
                        .and_where(
                            Expr::col(Alias::new(column)).is_in(publication_ids.iter().copied()),
                        ),
                ),
            )
            .await?;
        for row in rows {
            if let Ok(publication_id) = row.try_get::<Uuid>("", column) {
                referenced.insert(publication_id);
            }
        }
    }
    Ok(referenced)
}

async fn active_playback_publications(
    transaction: &DatabaseTransaction,
    publications: &HashMap<Uuid, (Uuid, String)>,
) -> Result<HashSet<Uuid>, DbErr> {
    let publication_ids = publications.values().map(|(id, _)| *id).collect::<Vec<_>>();
    if publication_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let mut referenced = HashSet::new();
    let projection = Alias::new("retention_source_projection");
    let ticket = Alias::new("retention_ticket");
    let tickets = Query::select()
        .distinct()
        .column((projection.clone(), Alias::new("publication_id")))
        .from_as(Alias::new("publication_media_sources"), projection.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("playback_tickets"),
            ticket.clone(),
            Expr::col((ticket.clone(), Alias::new("media_source_id")))
                .equals((projection.clone(), Alias::new("media_source_id"))),
        )
        .and_where(
            Expr::col((projection, Alias::new("publication_id")))
                .is_in(publication_ids.iter().copied()),
        )
        .and_where(Expr::col((ticket.clone(), Alias::new("revoked_at"))).is_null())
        .and_where(Expr::col((ticket, Alias::new("expires_at"))).gt(Utc::now()))
        .to_owned();
    referenced.extend(selected_ids(transaction, tickets, "publication_id").await?);
    let publication = Alias::new("retention_live_publication");
    let session = Alias::new("retention_playback_session");
    let sessions = Query::select()
        .distinct()
        .expr_as(
            Expr::col((publication.clone(), Alias::new("id"))),
            Alias::new("publication_id"),
        )
        .from_as(Alias::new("catalog_publications"), publication.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("playback_sessions"),
            session.clone(),
            Expr::col((session.clone(), Alias::new("catalog_item_id")))
                .equals((publication.clone(), Alias::new("owner_catalog_item_id"))),
        )
        .and_where(Expr::col((publication, Alias::new("id"))).is_in(publication_ids))
        .and_where(Expr::col((session.clone(), Alias::new("stopped_at"))).is_null())
        .and_where(
            Expr::col((session, Alias::new("last_event_at"))).gt(Utc::now() - Duration::hours(24)),
        )
        .to_owned();
    referenced.extend(selected_ids(transaction, sessions, "publication_id").await?);
    Ok(referenced)
}

async fn active_dependency_ids(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<HashSet<Uuid>, DbErr> {
    selected_ids(
        transaction,
        Query::select()
            .column(Alias::new("required_sync_job_id"))
            .from(Alias::new("work_jobs"))
            .and_where(Expr::col(Alias::new("required_sync_job_id")).is_in(job_ids.iter().copied()))
            .and_where(Expr::col(Alias::new("state")).is_in(ACTIVE_STATES))
            .to_owned(),
        "required_sync_job_id",
    )
    .await
}

async fn recovery_cursor_ids(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<HashSet<Uuid>, DbErr> {
    selected_ids(
        transaction,
        Query::select()
            .column(Alias::new("recovery_job_id"))
            .from(Alias::new("storage_sync_cursors"))
            .and_where(Expr::col(Alias::new("recovery_job_id")).is_in(job_ids.iter().copied()))
            .to_owned(),
        "recovery_job_id",
    )
    .await
}

/// Deletes retired publication projections with their change events, children,
/// and the publication row itself. Children are removed before the parent so
/// no foreign key is left dangling across the supported SQL dialects.
async fn delete_retired_publications(
    transaction: &DatabaseTransaction,
    purged: &[(Uuid, Uuid)],
) -> Result<(), WorkRetentionError> {
    let publication_ids = purged
        .iter()
        .map(|(_, publication_id)| *publication_id)
        .collect::<Vec<_>>();
    let backend = transaction.get_database_backend();
    let deleted = transaction
        .execute(
            backend.build(
                &Query::delete()
                    .from_table(Alias::new("catalog_publications"))
                    .and_where(Expr::col(Alias::new("id")).is_in(publication_ids.iter().copied()))
                    .and_where(Expr::col(Alias::new("state")).eq("Retired"))
                    .to_owned(),
            ),
        )
        .await?
        .rows_affected();
    ensure_affected(deleted, purged.len())
}

async fn active_full_scan_child_ids(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<HashSet<Uuid>, DbErr> {
    if job_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let staging = Alias::new("retention_active_staging");
    let parent = Alias::new("retention_active_parent");
    let query = Query::select()
        .column((staging.clone(), Alias::new("payload")))
        .from_as(Alias::new("work_staging_rows"), staging.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("work_jobs"),
            parent.clone(),
            Expr::col((parent.clone(), Alias::new("id")))
                .equals((staging.clone(), Alias::new("job_id"))),
        )
        .and_where(Expr::col((parent.clone(), Alias::new("state"))).is_in(ACTIVE_STATES))
        .and_where(
            Expr::col((parent, Alias::new("task_kind")))
                .is_in(["FullMediaScan", "FullLibraryRootScan"]),
        )
        .to_owned();
    let requested = job_ids.iter().copied().collect::<HashSet<_>>();
    let backend = transaction.get_database_backend();
    let mut protected = HashSet::new();
    for row in transaction.query_all(backend.build(&query)).await? {
        let payload: Value = row.try_get("", "payload")?;
        if let Some(job_id) = payload
            .get("job_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .filter(|job_id| requested.contains(job_id))
        {
            protected.insert(job_id);
        }
    }
    Ok(protected)
}

async fn selected_ids(
    transaction: &DatabaseTransaction,
    query: sea_orm::sea_query::SelectStatement,
    column: &str,
) -> Result<HashSet<Uuid>, DbErr> {
    let backend = transaction.get_database_backend();
    transaction
        .query_all(backend.build(&query))
        .await?
        .iter()
        .map(|row| row.try_get("", column))
        .collect()
}

async fn defer_claims(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaimBatch,
    job_ids: &[Uuid],
    now: DateTime<Utc>,
    delay: Duration,
    reason: &str,
) -> Result<(), WorkRetentionError> {
    let available_at = now
        .checked_add_signed(delay)
        .ok_or(WorkRetentionError::TimestampOverflow)?;
    let backend = transaction.get_database_backend();
    let updated = transaction
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("work_job_retention_queue"))
                    .value(Alias::new("lease_owner"), Option::<String>::None)
                    .value(
                        Alias::new("lease_expires_at"),
                        Option::<DateTime<Utc>>::None,
                    )
                    .value(Alias::new("available_at"), available_at)
                    .value(Alias::new("last_error"), reason)
                    .and_where(Expr::col(Alias::new("job_id")).is_in(job_ids.iter().copied()))
                    .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
                    .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now)),
            ),
        )
        .await?
        .rows_affected();
    ensure_affected(updated, job_ids.len())
}

async fn clear_terminal_dependencies(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    transaction
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("work_jobs"))
                    .value(Alias::new("required_sync_job_id"), Option::<Uuid>::None)
                    .and_where(
                        Expr::col(Alias::new("required_sync_job_id"))
                            .is_in(job_ids.iter().copied()),
                    )
                    .and_where(Expr::col(Alias::new("state")).is_in(TERMINAL_STATES)),
            ),
        )
        .await?;
    Ok(())
}

async fn delete_child_rows(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<(), DbErr> {
    for table in ["work_staging_rows", "storage_sync_pages", "work_results"] {
        delete_bounded_rows(transaction, table, "job_id", job_ids).await?;
    }
    Ok(())
}
async fn delete_bounded_rows(
    transaction: &DatabaseTransaction,
    table: &str,
    foreign_key: &str,
    parents: &[Uuid],
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    let rows = transaction
        .query_all(
            backend.build(
                Query::select()
                    .column(Alias::new("id"))
                    .from(Alias::new(table))
                    .and_where(Expr::col(Alias::new(foreign_key)).is_in(parents.iter().copied()))
                    .order_by(Alias::new("id"), Order::Asc)
                    .limit(CHILD_ROW_BATCH_SIZE),
            ),
        )
        .await?;
    let ids = rows
        .iter()
        .map(|row| row.try_get::<Uuid>("", "id"))
        .collect::<Result<Vec<_>, _>>()?;
    if !ids.is_empty() {
        transaction
            .execute(
                backend.build(
                    Query::delete()
                        .from_table(Alias::new(table))
                        .and_where(Expr::col(Alias::new("id")).is_in(ids)),
                ),
            )
            .await?;
    }
    Ok(())
}
async fn remaining_children(
    transaction: &DatabaseTransaction,
    parents: &[Uuid],
) -> Result<HashSet<Uuid>, DbErr> {
    let mut remaining = HashSet::new();
    for table in ["work_staging_rows", "storage_sync_pages", "work_results"] {
        remaining.extend(
            selected_ids(
                transaction,
                Query::select()
                    .distinct()
                    .column(Alias::new("job_id"))
                    .from(Alias::new(table))
                    .and_where(Expr::col(Alias::new("job_id")).is_in(parents.iter().copied()))
                    .to_owned(),
                "job_id",
            )
            .await?,
        );
    }
    Ok(remaining)
}
async fn trim_retired_projections(
    transaction: &DatabaseTransaction,
    purged: &[(Uuid, Uuid)],
) -> Result<HashSet<Uuid>, DbErr> {
    if purged.is_empty() {
        return Ok(HashSet::new());
    }
    let publications = purged.iter().map(|(_, id)| *id).collect::<Vec<_>>();
    let mut remaining = HashSet::new();
    for table in [
        "catalog_change_outbox",
        "publication_catalog_items",
        "publication_media_sources",
        "publication_media_locations",
        "publication_subtitles",
    ] {
        delete_bounded_rows(transaction, table, "publication_id", &publications).await?;
        let ids = selected_ids(
            transaction,
            Query::select()
                .distinct()
                .column(Alias::new("publication_id"))
                .from(Alias::new(table))
                .and_where(
                    Expr::col(Alias::new("publication_id")).is_in(publications.iter().copied()),
                )
                .to_owned(),
            "publication_id",
        )
        .await?;
        remaining.extend(
            purged
                .iter()
                .filter(|(_, id)| ids.contains(id))
                .map(|(job, _)| *job),
        );
    }
    Ok(remaining)
}

async fn delete_jobs(
    transaction: &DatabaseTransaction,
    job_ids: &[Uuid],
) -> Result<(), WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let deleted = transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("work_jobs"))
                    .and_where(Expr::col(Alias::new("id")).is_in(job_ids.iter().copied()))
                    .and_where(Expr::col(Alias::new("state")).is_in(TERMINAL_STATES)),
            ),
        )
        .await?
        .rows_affected();
    ensure_affected(deleted, job_ids.len())
}

async fn delete_queue_claims(
    transaction: &DatabaseTransaction,
    claimed: &RetentionClaimBatch,
    job_ids: &[Uuid],
    now: DateTime<Utc>,
) -> Result<(), WorkRetentionError> {
    let backend = transaction.get_database_backend();
    let deleted = transaction
        .execute(
            backend.build(
                Query::delete()
                    .from_table(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("job_id")).is_in(job_ids.iter().copied()))
                    .and_where(Expr::col(Alias::new("lease_owner")).eq(&claimed.lease_token))
                    .and_where(Expr::col(Alias::new("lease_expires_at")).gt(now)),
            ),
        )
        .await?
        .rows_affected();
    ensure_affected(deleted, job_ids.len())
}

fn ensure_affected(actual: u64, expected: usize) -> Result<(), WorkRetentionError> {
    (actual == u64::try_from(expected).expect("retention batch size fits u64"))
        .then_some(())
        .ok_or(WorkRetentionError::LostLease)
}

fn sorted_ids(mut job_ids: Vec<Uuid>) -> Vec<Uuid> {
    job_ids.sort_unstable();
    job_ids.dedup();
    job_ids
}

fn sorted_purges(mut purges: Vec<(Uuid, Uuid)>) -> Vec<(Uuid, Uuid)> {
    purges.sort_unstable();
    purges.dedup();
    purges
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, WorkRetentionError>,
) -> Result<T, WorkRetentionError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(WorkRetentionError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

#[derive(Debug, Error)]
pub enum WorkRetentionError {
    #[error("lease owner must not be empty")]
    EmptyLeaseOwner,
    #[error("lease owner must not exceed 128 characters")]
    LeaseOwnerTooLong,
    #[error("work retention duration must be positive")]
    InvalidRetention,
    #[error("lease duration must be positive")]
    InvalidLeaseDuration,
    #[error("retention timestamp is outside the supported range")]
    TimestampOverflow,
    #[error("work retention lease is expired or no longer owned")]
    LostLease,
    #[error("database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}
