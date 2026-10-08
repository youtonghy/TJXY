use chrono::{Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, Order, Query},
};
use tjxy_common::WorkJobId;

use crate::{WorkJobRepository, WorkJobRepositoryError, WorkTaskKind};

const BATCH_SIZE: u64 = 100;
/// Superseded metadata jobs removed per maintenance pass. Pending jobs own no history, so the
/// batch is larger than the cancellation batch while still bounded within one transaction.
const SUPERSEDED_BATCH_SIZE: u64 = 5_000;
pub(crate) const INACTIVE_SCOPE_REASON: &str =
    "cancelled: storage account disabled or root unbound";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkMaintenanceReport {
    pub acquired: bool,
    pub reclaimed: u64,
    pub cancelled: u64,
    pub superseded: u64,
}

impl<Clock: crate::WorkJobClock> WorkJobRepository<'_, Clock> {
    /// Runs bounded global maintenance under a database-wide scheduling lease.
    ///
    /// # Errors
    /// Returns database failures. The lease and transitions roll back together on failure.
    pub async fn maintain_queue(&self) -> Result<WorkMaintenanceReport, WorkJobRepositoryError> {
        let database = self.connection();
        let now = self.now();
        let transaction = database.begin().await?;
        let backend = transaction.get_database_backend();
        let acquired = transaction
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("work_maintenance_state"))
                        .value(Alias::new("available_at"), now + Duration::seconds(5))
                        .and_where(Expr::col(Alias::new("id")).eq(1_i32))
                        .and_where(Expr::col(Alias::new("available_at")).lte(now)),
                ),
            )
            .await?
            .rows_affected()
            == 1;
        if !acquired {
            transaction.commit().await?;
            return Ok(WorkMaintenanceReport::default());
        }
        let cancelled = cancel_disabled_storage_work(&transaction, now).await?;
        let superseded = purge_superseded_metadata_jobs(&transaction).await?;
        let reclaimed = crate::work_job::reclaim_expired_leases(&transaction, now).await?;
        crate::work_job::fail_terminal_dependents(
            &transaction,
            &[
                WorkTaskKind::ScopedStorageSync,
                WorkTaskKind::RecoverStorageCursor,
                WorkTaskKind::ValidateStorageRoot,
                WorkTaskKind::DiscoverTitles,
                WorkTaskKind::ExpandItem,
                WorkTaskKind::IndexMediaSources,
                WorkTaskKind::ResolveMetadata,
                WorkTaskKind::ProbeMedia,
                WorkTaskKind::FullMediaScan,
                WorkTaskKind::FullLibraryRootScan,
            ],
            now,
        )
        .await?;
        if cancelled + reclaimed > 0 {
            crate::work_queue::commit_and_notify(transaction).await?;
        } else {
            transaction.commit().await?;
        }
        Ok(WorkMaintenanceReport {
            acquired,
            reclaimed,
            cancelled,
            superseded,
        })
    }
}

/// Deletes Pending metadata resolutions whose item has moved on to a newer metadata revision.
///
/// Enqueueing a newer revision already removes its predecessors; this drains the backlog created
/// before that rule existed and any revision bump that happened outside an enqueue. A job whose
/// revision is behind the item's can only fail as stale, so removing it changes no outcome.
async fn purge_superseded_metadata_jobs(
    transaction: &DatabaseTransaction,
) -> Result<u64, WorkJobRepositoryError> {
    let backend = transaction.get_database_backend();
    let job = Alias::new("stale_job");
    let item = Alias::new("stale_item");
    let stale = Query::select()
        .column((job.clone(), Alias::new("id")))
        .from_as(Alias::new("work_jobs"), job.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id")))
                .equals((job.clone(), Alias::new("scope_id"))),
        )
        .and_where(Expr::col((job.clone(), Alias::new("state"))).eq("Pending"))
        .and_where(
            Expr::col((job.clone(), Alias::new("task_kind")))
                .eq(WorkTaskKind::ResolveMetadata.as_str()),
        )
        .and_where(Expr::col((job.clone(), Alias::new("scope_type"))).eq("CatalogItem"))
        .and_where(
            Expr::col((job.clone(), Alias::new("expected_revision")))
                .lt(Expr::col((item, Alias::new("metadata_revision")))),
        )
        .and_where(crate::work_job::lacks_owned_rows(&job, "work_staging_rows"))
        .and_where(crate::work_job::lacks_owned_rows(
            &job,
            "catalog_publications",
        ))
        .and_where(crate::work_job::lacks_owned_rows(&job, "work_results"))
        .and_where(crate::work_job::lacks_owned_rows(
            &job,
            "storage_sync_pages",
        ))
        .limit(SUPERSEDED_BATCH_SIZE)
        .to_owned();
    let ids = transaction
        .query_all(backend.build(&stale))
        .await?
        .iter()
        .map(|row| row.try_get::<uuid::Uuid>("", "id"))
        .collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() {
        return Ok(0);
    }
    let delete = Query::delete()
        .from_table(Alias::new("work_jobs"))
        .and_where(Expr::col(Alias::new("id")).is_in(ids))
        .and_where(Expr::col(Alias::new("state")).eq("Pending"))
        .to_owned();
    Ok(transaction
        .execute(backend.build(&delete))
        .await?
        .rows_affected())
}

#[allow(clippy::too_many_lines)] // Keep the bounded cancellation query and its authorization predicates together.
async fn cancel_disabled_storage_work(
    transaction: &DatabaseTransaction,
    now: chrono::DateTime<Utc>,
) -> Result<u64, WorkJobRepositoryError> {
    let job = Alias::new("inactive_job");
    let mut invalid = Cond::any();
    for (scope, table, account_column) in [
        ("StorageObject", "storage_objects", "storage_account_id"),
        ("StorageRoot", "storage_roots", "storage_account_id"),
    ] {
        let scoped = Alias::new("inactive_scope");
        let account = Alias::new("inactive_account");
        let disabled = Query::select()
            .expr(Expr::val(1_i32))
            .from_as(Alias::new(table), scoped.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_accounts"),
                account.clone(),
                Expr::col((account.clone(), Alias::new("id")))
                    .equals((scoped.clone(), Alias::new(account_column))),
            )
            .and_where(
                Expr::col((scoped, Alias::new("id"))).equals((job.clone(), Alias::new("scope_id"))),
            )
            .and_where(Expr::col((account, Alias::new("status"))).eq("Disabled"))
            .to_owned();
        invalid = invalid.add(
            Cond::all()
                .add(Expr::col((job.clone(), Alias::new("scope_type"))).eq(scope))
                .add(Expr::exists(disabled)),
        );
    }
    // An explicit root affinity must still have an enabled library binding. Shared
    // roots are retained while any enabled library continues to reference them.
    let binding = Alias::new("active_binding");
    let library = Alias::new("active_library");
    let active_root = Alias::new("active_root");
    let active_account = Alias::new("active_account");
    let active = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("library_storage_roots"), binding.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((binding.clone(), Alias::new("library_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_roots"),
            active_root.clone(),
            Expr::col((active_root.clone(), Alias::new("id")))
                .equals((binding.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_accounts"),
            active_account.clone(),
            Expr::col((active_account.clone(), Alias::new("id")))
                .equals((active_root, Alias::new("storage_account_id"))),
        )
        .and_where(Expr::col((active_account, Alias::new("status"))).is_in(["Active", "Ready"]))
        .and_where(
            Expr::col((binding, Alias::new("storage_root_id")))
                .equals((job.clone(), Alias::new("storage_root_affinity"))),
        )
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .to_owned();
    invalid = invalid.add(
        Cond::all()
            .add(
                Expr::col((job.clone(), Alias::new("storage_root_affinity"))).ne(uuid::Uuid::nil()),
            )
            .add(Expr::exists(active).not()),
    );
    let bound_library = Alias::new("valid_bound_library");
    let scope_binding = Alias::new("valid_scope_binding");
    let valid_binding = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("library_storage_roots"), scope_binding.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            bound_library.clone(),
            Expr::col((bound_library.clone(), Alias::new("id")))
                .equals((scope_binding.clone(), Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((scope_binding, Alias::new("id")))
                .equals((job.clone(), Alias::new("scope_id"))),
        )
        .and_where(Expr::col((bound_library, Alias::new("is_enabled"))).eq(true))
        .to_owned();
    invalid = invalid.add(
        Cond::all()
            .add(Expr::col((job.clone(), Alias::new("scope_type"))).eq("LibraryRootBinding"))
            .add(Expr::exists(valid_binding).not()),
    );
    let disabled_library = Query::select()
        .expr(Expr::val(1_i32))
        .from(Alias::new("libraries"))
        .and_where(
            Expr::col((Alias::new("libraries"), Alias::new("id")))
                .equals((job.clone(), Alias::new("scope_id"))),
        )
        .and_where(Expr::col(Alias::new("is_enabled")).eq(false))
        .to_owned();
    invalid = invalid.add(
        Cond::all()
            .add(Expr::col((job.clone(), Alias::new("scope_type"))).eq("Library"))
            .add(Expr::exists(disabled_library)),
    );
    let query = Query::select()
        .column((job.clone(), Alias::new("id")))
        .from_as(Alias::new("work_jobs"), job.clone())
        .and_where(Expr::col((job.clone(), Alias::new("state"))).is_in(["Pending", "Running"]))
        .cond_where(invalid)
        .order_by((job.clone(), Alias::new("created_at")), Order::Asc)
        .order_by((job, Alias::new("id")), Order::Asc)
        .limit(BATCH_SIZE)
        .to_owned();
    let rows = transaction
        .query_all(transaction.get_database_backend().build(&query))
        .await?;
    let mut cancelled = 0;
    for row in rows {
        cancelled += u64::from(
            crate::work_job::cancel_job(
                transaction,
                WorkJobId::from_uuid(row.try_get("", "id")?),
                INACTIVE_SCOPE_REASON,
                now,
                false,
            )
            .await?,
        );
    }
    Ok(cancelled)
}

/// Lock an existing scoped account until publication commits, ordering it with account disable.
/// Legacy unscoped jobs still use their publication-specific authorization fences.
pub(crate) async fn fence_storage_account(
    transaction: &DatabaseTransaction,
    claimed: &crate::ClaimedWorkJob,
) -> Result<(), WorkJobRepositoryError> {
    let mut query = Query::select();
    if let Some(root) = claimed.job().storage_root_affinity() {
        query
            .column(Alias::new("storage_account_id"))
            .from(Alias::new("storage_roots"))
            .and_where(Expr::col(Alias::new("id")).eq(root.as_uuid()));
    } else {
        let (table, id) = match claimed.job().scope() {
            crate::WorkScope::StorageRoot(id) => ("storage_roots", id.as_uuid()),
            crate::WorkScope::StorageObject(id) => ("storage_objects", id.as_uuid()),
            _ => return Ok(()),
        };
        query
            .column(Alias::new("storage_account_id"))
            .from(Alias::new(table))
            .and_where(Expr::col(Alias::new("id")).eq(id));
    }
    let backend = transaction.get_database_backend();
    // WorkJobRepository also supports standalone queue records; absence is handled by each domain's snapshot fence.
    let Some(row) = transaction.query_one(backend.build(&query)).await? else {
        return Ok(());
    };
    let account: uuid::Uuid = row.try_get("", "storage_account_id")?;
    let updated = transaction
        .execute(
            backend.build(
                Query::update()
                    .table(Alias::new("storage_accounts"))
                    .value(Alias::new("status"), Expr::col(Alias::new("status")))
                    .and_where(Expr::col(Alias::new("id")).eq(account))
                    .and_where(Expr::col(Alias::new("status")).is_in(["Active", "Ready"])),
            ),
        )
        .await?;
    if updated.rows_affected() != 1 {
        return Err(WorkJobRepositoryError::LostLease);
    }
    Ok(())
}
