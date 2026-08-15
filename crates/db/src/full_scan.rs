use std::collections::HashSet;

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, IsolationLevel,
    TransactionTrait,
    sea_query::{
        Alias, CaseStatement, Condition, Expr, JoinType, Order, Query, SelectStatement, SimpleExpr,
    },
};
use thiserror::Error;
use tjxy_common::{
    CatalogItemId, LibraryId, LibraryRootBindingId, StorageObjectRecordId, StorageRootId, WorkJobId,
};
use tjxy_domain::{LocalMetadataAccessMode, MetadataSourceMode};

use crate::{
    ClaimedWorkJob, WorkJobRepository, WorkJobSpec, WorkJobSubmission, WorkScope, WorkStagingRow,
    WorkTaskKind, work_job::enqueue_in_transaction,
};

const HYBRID_BATCH_ENTITY_KIND: &str = "FullScanHybridCandidate";
const HYBRID_BATCH_MARKER_ENTITY_KIND: &str = "FullScanHybridBatch";

pub struct FullScanRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> FullScanRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Enqueues or joins a Manual Full scan for one enabled Library-root binding.
    ///
    /// The parent captures the Library profile revision but deliberately does not
    /// capture a root sync revision: its validation child advances that watermark.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError`] when the requested binding is unavailable
    /// or the serializable submission transaction cannot be committed.
    pub async fn enqueue_root_scan(
        &self,
        library_id: LibraryId,
        root_id: StorageRootId,
        priority: i32,
    ) -> Result<WorkJobSubmission, FullScanRepositoryError> {
        let transaction = self
            .database
            .begin_with_config(Some(IsolationLevel::Serializable), None)
            .await?;
        let result = async {
            let binding = root_binding_context(&transaction, library_id, root_id).await?;
            fence_root_submission(&transaction, binding).await?;
            let spec = WorkJobSpec::new(
                WorkTaskKind::FullLibraryRootScan,
                WorkScope::LibraryRootBinding(binding.binding_id),
                i64::from(binding.profile_version),
                priority,
            )?;
            enqueue_in_transaction(&transaction, &spec, chrono::Utc::now())
                .await
                .map_err(Into::into)
        }
        .await;
        finish(transaction, result).await
    }

    /// Reads the effective policy captured by one fenced Library scan.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError`] for invalid work, stale policy, corrupt values, or SQL failure.
    pub async fn policy(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<FullScanPolicy, FullScanRepositoryError> {
        Ok(scan_context(self.database, claimed).await?.policy)
    }

    /// Returns the selected storage root for a root-scoped Full scan.
    ///
    /// Library-wide scans return `None` because their child work is not root-affine.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError`] for invalid work or a stale Library binding.
    pub async fn storage_root_scope(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<Option<StorageRootId>, FullScanRepositoryError> {
        Ok(scan_context(self.database, claimed).await?.root_id)
    }

    /// Lists the items selected by the fenced Library policy.
    ///
    /// Title-layer scans include only explicit Library members. All-object scans also
    /// include children from active Structure publications.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError`] for invalid work, stale library policy, or SQL failure.
    pub async fn targets(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<Vec<CatalogItemId>, FullScanRepositoryError> {
        let context = scan_context(self.database, claimed).await?;
        let backend = self.database.get_database_backend();
        let explicit = explicit_targets(context.library_id.as_uuid(), context.root_id);
        let mut ids = self
            .database
            .query_all(backend.build(&explicit))
            .await?
            .iter()
            .map(|row| {
                row.try_get("", "catalog_item_id")
                    .map(CatalogItemId::from_uuid)
            })
            .collect::<Result<HashSet<_>, DbErr>>()?;
        if context.policy.selects_all_synced_objects() {
            let projection = projected_targets(context.library_id.as_uuid(), context.root_id);
            for row in self.database.query_all(backend.build(&projection)).await? {
                ids.insert(CatalogItemId::from_uuid(
                    row.try_get("", "catalog_item_id")?,
                ));
            }
        }
        let mut ids = ids.into_iter().collect::<Vec<_>>();
        ids.sort_unstable_by_key(|id| id.as_uuid());
        Ok(ids)
    }

    /// Returns a bounded, deterministic set of Series eligible for background expansion.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError`] for an invalid limit, stale scan policy, or SQL failure.
    pub async fn background_candidates(
        &self,
        claimed: &ClaimedWorkJob,
        limit: u64,
    ) -> Result<Vec<CatalogItemId>, FullScanRepositoryError> {
        if limit == 0 || limit > 64 {
            return Err(FullScanRepositoryError::InvalidCandidateLimit);
        }
        let context = scan_context(self.database, claimed).await?;
        if !context.policy.expands_in_background() {
            return Ok(Vec::new());
        }
        let query = background_candidate_query(context.library_id, limit);
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(|row| {
                row.try_get("", "catalog_item_id")
                    .map(CatalogItemId::from_uuid)
            })
            .collect::<Result<Vec<_>, DbErr>>()
            .map_err(Into::into)
    }

    /// Returns the durable, bounded Hybrid candidate batch selected for this Full scan.
    ///
    /// The first call stores the ranked item IDs under the parent claim. Retries reuse that
    /// selection so one refresh cannot advance through successive batches after children publish.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError`] for invalid work, corrupt staging, or SQL failure.
    pub async fn background_candidate_batch(
        &self,
        claimed: &ClaimedWorkJob,
        limit: u64,
    ) -> Result<Vec<CatalogItemId>, FullScanRepositoryError> {
        if limit == 0 || limit > 64 {
            return Err(FullScanRepositoryError::InvalidCandidateLimit);
        }
        let context = scan_context(self.database, claimed).await?;
        if !context.policy.expands_in_background() {
            return Ok(Vec::new());
        }
        if let Some(staged) = staged_background_candidates(self.database, claimed).await? {
            return Ok(staged);
        }
        let selected = self.background_candidates(claimed, limit).await?;
        let mut rows = vec![WorkStagingRow::new(
            HYBRID_BATCH_MARKER_ENTITY_KIND,
            "selected",
            serde_json::json!({"limit": limit}),
            "Selected",
        )?];
        rows.extend(
            selected
                .iter()
                .enumerate()
                .map(|(rank, item)| {
                    WorkStagingRow::new(
                        HYBRID_BATCH_ENTITY_KIND,
                        format!("{rank:03}:{item}"),
                        serde_json::json!({}),
                        "Selected",
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        WorkJobRepository::new(self.database)
            .stage_batch(claimed, claimed.id().as_uuid(), &rows)
            .await?;
        Ok(selected)
    }

    /// Lists root inventory/discovery watermarks for one fenced Full scan.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError`] for invalid claims or SQL failures.
    #[allow(clippy::too_many_lines)] // Keeps the root/account/object authorization boundary in one auditable query.
    pub async fn roots(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<Vec<FullScanRoot>, FullScanRepositoryError> {
        let context = scan_context(self.database, claimed).await?;
        let root = Alias::new("full_root");
        let binding = Alias::new("full_root_binding");
        let relation = Alias::new("full_root_object");
        let object = Alias::new("full_root_storage_object");
        let account = Alias::new("full_root_account");
        let mut query = Query::select()
            .expr_as(
                Expr::col((root.clone(), Alias::new("id"))),
                Alias::new("root_id"),
            )
            .expr_as(
                Expr::col((root.clone(), Alias::new("sync_revision"))),
                Alias::new("sync_revision"),
            )
            .expr_as(
                Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))),
                Alias::new("reconciled_sync_revision"),
            )
            .expr_as(
                Expr::col((binding.clone(), Alias::new("discovered_sync_revision"))),
                Alias::new("discovered_sync_revision"),
            )
            .expr_as(
                Expr::col((binding.clone(), Alias::new("id"))),
                Alias::new("binding_id"),
            )
            .expr_as(
                Expr::col((binding.clone(), Alias::new("library_id"))),
                Alias::new("library_id"),
            )
            .expr_as(
                Expr::col((relation.clone(), Alias::new("storage_object_id"))),
                Alias::new("root_object_id"),
            )
            .expr_as(
                Expr::col((relation.clone(), Alias::new("children_indexed"))),
                Alias::new("children_indexed"),
            )
            .expr_as(
                Expr::col((relation.clone(), Alias::new("children_index_revision"))),
                Alias::new("children_index_revision"),
            )
            .from_as(Alias::new("storage_roots"), root.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("library_storage_roots"),
                binding.clone(),
                Expr::col((binding.clone(), Alias::new("storage_root_id")))
                    .equals((root.clone(), Alias::new("id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_root_objects"),
                relation.clone(),
                Expr::col((relation.clone(), Alias::new("storage_root_id")))
                    .equals((root.clone(), Alias::new("id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_objects"),
                object.clone(),
                Expr::col((object.clone(), Alias::new("id")))
                    .equals((relation.clone(), Alias::new("storage_object_id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_accounts"),
                account.clone(),
                Expr::col((account.clone(), Alias::new("id")))
                    .equals((object.clone(), Alias::new("storage_account_id"))),
            )
            .and_where(
                Expr::col((binding.clone(), Alias::new("library_id")))
                    .eq(context.library_id.as_uuid()),
            )
            .and_where(
                Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))).is_null(),
            )
            .and_where(
                Expr::col((relation, Alias::new("presence_state")))
                    .is_in(["Present", "TemporarilyUnavailable"]),
            )
            .and_where(Expr::col((object.clone(), Alias::new("object_type"))).eq("Directory"))
            .and_where(
                Expr::col((object.clone(), Alias::new("storage_account_id")))
                    .equals((root, Alias::new("storage_account_id"))),
            )
            .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
            .and_where(Expr::col((account, Alias::new("status"))).eq("Active"))
            .to_owned();
        if let Some(binding_id) = context.binding_id {
            query
                .and_where(Expr::col((binding.clone(), Alias::new("id"))).eq(binding_id.as_uuid()));
        }
        self.database
            .query_all(self.database.get_database_backend().build(&query))
            .await?
            .iter()
            .map(|row| {
                Ok(FullScanRoot {
                    binding_id: LibraryRootBindingId::from_uuid(row.try_get("", "binding_id")?),
                    library_id: LibraryId::from_uuid(row.try_get("", "library_id")?),
                    root_id: StorageRootId::from_uuid(row.try_get("", "root_id")?),
                    root_object_id: StorageObjectRecordId::from_uuid(
                        row.try_get("", "root_object_id")?,
                    ),
                    sync_revision: row.try_get("", "sync_revision")?,
                    reconciled_revision: row.try_get("", "reconciled_sync_revision")?,
                    discovered_revision: row.try_get("", "discovered_sync_revision")?,
                    children_indexed: row.try_get("", "children_indexed")?,
                    children_revision: row.try_get("", "children_index_revision")?,
                })
            })
            .collect::<Result<Vec<_>, DbErr>>()
            .map_err(Into::into)
    }

    /// Returns the durable validation child recorded for one root in this Full scan.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError`] for invalid work, corrupt staging, or SQL failure.
    pub async fn validation_dependency(
        &self,
        claimed: &ClaimedWorkJob,
        root_id: StorageRootId,
    ) -> Result<Option<WorkJobId>, FullScanRepositoryError> {
        self.root_dependency(claimed, root_id, "FullScanValidation")
            .await
    }

    /// Returns the durable title-layer inventory child recorded for one root in this scan.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError`] for invalid work, corrupt staging, or SQL failure.
    pub async fn inventory_dependency(
        &self,
        claimed: &ClaimedWorkJob,
        root_id: StorageRootId,
    ) -> Result<Option<WorkJobId>, FullScanRepositoryError> {
        self.root_dependency(claimed, root_id, "FullScanInventory")
            .await
    }

    /// Returns one durable child recorded under this Full Scan natural key.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError`] for invalid work, corrupt staging, or SQL failure.
    pub async fn child_dependency(
        &self,
        claimed: &ClaimedWorkJob,
        natural_key: &str,
    ) -> Result<Option<WorkJobId>, FullScanRepositoryError> {
        self.root_dependency(claimed, natural_key, "FullScanChild")
            .await
    }

    async fn root_dependency(
        &self,
        claimed: &ClaimedWorkJob,
        natural_key: impl ToString,
        entity_kind: &'static str,
    ) -> Result<Option<WorkJobId>, FullScanRepositoryError> {
        if !matches!(
            claimed.job().task_kind(),
            WorkTaskKind::FullMediaScan | WorkTaskKind::FullLibraryRootScan
        ) {
            return Err(FullScanRepositoryError::InvalidClaim);
        }
        let query = Query::select()
            .column(Alias::new("payload"))
            .from(Alias::new("work_staging_rows"))
            .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid()))
            .and_where(Expr::col(Alias::new("publication_id")).eq(claimed.id().as_uuid()))
            .and_where(Expr::col(Alias::new("entity_kind")).eq(entity_kind))
            .and_where(Expr::col(Alias::new("natural_key")).eq(natural_key.to_string()))
            .limit(1)
            .to_owned();
        let Some(row) = self
            .database
            .query_one(self.database.get_database_backend().build(&query))
            .await?
        else {
            return Ok(None);
        };
        let payload: serde_json::Value = row.try_get("", "payload")?;
        let job_id = payload
            .get("job_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            .map(WorkJobId::from_uuid)
            .ok_or(FullScanRepositoryError::CorruptRootDependency { entity_kind })?;
        Ok(Some(job_id))
    }
}

fn background_candidate_query(library_id: LibraryId, limit: u64) -> SelectStatement {
    let item = Alias::new("background_series");
    let membership = Alias::new("background_membership");
    let publication = Alias::new("background_structure_publication");
    let current_structure = Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_publications"), publication.clone())
        .and_where(
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((item.clone(), Alias::new("active_structure_publication_id"))),
        )
        .and_where(
            Expr::col((publication.clone(), Alias::new("owner_catalog_item_id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication.clone(), Alias::new("state"))).eq("Active"))
        .and_where(
            Expr::col((publication, Alias::new("expected_revision")))
                .equals((item.clone(), Alias::new("structure_expansion_revision"))),
        )
        .limit(1)
        .to_owned();
    Query::select()
        .expr_as(
            Expr::col((item.clone(), Alias::new("id"))),
            Alias::new("catalog_item_id"),
        )
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .and_where(
            Expr::col((membership.clone(), Alias::new("library_id"))).eq(library_id.as_uuid()),
        )
        .and_where(Expr::col((item.clone(), Alias::new("item_type"))).eq("Series"))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item.clone(), Alias::new("classification_state"))).eq("Matched"))
        .and_where(Expr::exists(current_structure).not())
        .order_by_expr(
            CaseStatement::new()
                .case(
                    Expr::col((membership.clone(), Alias::new("hybrid_admin_selected_at")))
                        .is_not_null(),
                    0,
                )
                .finally(1)
                .into(),
            Order::Asc,
        )
        .order_by(
            (membership.clone(), Alias::new("hybrid_admin_selected_at")),
            Order::Asc,
        )
        .order_by_expr(
            background_signal_rank(background_user_signal(
                &item,
                BackgroundUserSignal::Watching,
            )),
            Order::Asc,
        )
        .order_by_expr(
            background_signal_rank(background_engaged_next_up_signal(&item)),
            Order::Asc,
        )
        .order_by_expr(
            background_signal_rank(background_user_signal(
                &item,
                BackgroundUserSignal::Favorite,
            )),
            Order::Asc,
        )
        .order_by((item.clone(), Alias::new("date_created")), Order::Desc)
        .order_by((item, Alias::new("id")), Order::Asc)
        .limit(limit)
        .to_owned()
}

async fn staged_background_candidates(
    database: &DatabaseConnection,
    claimed: &ClaimedWorkJob,
) -> Result<Option<Vec<CatalogItemId>>, FullScanRepositoryError> {
    let marker = Query::select()
        .expr(Expr::val(1_i32))
        .from(Alias::new("work_staging_rows"))
        .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid()))
        .and_where(Expr::col(Alias::new("publication_id")).eq(claimed.id().as_uuid()))
        .and_where(Expr::col(Alias::new("entity_kind")).eq(HYBRID_BATCH_MARKER_ENTITY_KIND))
        .limit(1)
        .to_owned();
    if database
        .query_one(database.get_database_backend().build(&marker))
        .await?
        .is_none()
    {
        return Ok(None);
    }
    let query = Query::select()
        .column(Alias::new("natural_key"))
        .from(Alias::new("work_staging_rows"))
        .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid()))
        .and_where(Expr::col(Alias::new("publication_id")).eq(claimed.id().as_uuid()))
        .and_where(Expr::col(Alias::new("entity_kind")).eq(HYBRID_BATCH_ENTITY_KIND))
        .order_by(Alias::new("natural_key"), Order::Asc)
        .to_owned();
    let candidates = database
        .query_all(database.get_database_backend().build(&query))
        .await?
        .iter()
        .map(|row| {
            let natural_key = row.try_get::<String>("", "natural_key")?;
            let (_, item_id) = natural_key
                .split_once(':')
                .ok_or(FullScanRepositoryError::CorruptHybridCandidateBatch)?;
            uuid::Uuid::parse_str(item_id)
                .map(CatalogItemId::from_uuid)
                .map_err(|_| FullScanRepositoryError::CorruptHybridCandidateBatch)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(candidates))
}

#[derive(Clone, Copy)]
enum BackgroundUserSignal {
    Watching,
    Favorite,
}

fn background_signal_rank(signal: SelectStatement) -> SimpleExpr {
    CaseStatement::new()
        .case(Expr::exists(signal), 0)
        .finally(1)
        .into()
}

fn background_user_signal(candidate: &Alias, signal: BackgroundUserSignal) -> SelectStatement {
    let item = Alias::new("background_signal_item");
    let data = Alias::new("background_signal_user_data");
    let predicate = match signal {
        BackgroundUserSignal::Watching => Condition::all()
            .add(Expr::col((data.clone(), Alias::new("playback_position_ticks"))).gt(0_i64))
            .add(Expr::col((data.clone(), Alias::new("is_played"))).eq(false)),
        BackgroundUserSignal::Favorite => {
            Condition::all().add(Expr::col((data.clone(), Alias::new("is_favorite"))).eq(true))
        }
    };
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("user_data"),
            data.clone(),
            Expr::col((data, Alias::new("catalog_item_id")))
                .equals((item.clone(), Alias::new("id"))),
        )
        .and_where(
            Condition::any()
                .add(
                    Expr::col((item.clone(), Alias::new("id")))
                        .equals((candidate.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((item, Alias::new("structure_owner_item_id")))
                        .equals((candidate.clone(), Alias::new("id"))),
                )
                .into(),
        )
        .and_where(predicate.into())
        .limit(1)
        .to_owned()
}

fn background_engaged_next_up_signal(candidate: &Alias) -> SelectStatement {
    let user = Alias::new("background_home_user");
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("users"), user.clone())
        .and_where(Expr::exists(background_played_episode_signal(
            candidate, &user,
        )))
        .and_where(Expr::exists(background_unstarted_episode_signal(
            candidate, &user,
        )))
        .limit(1)
        .to_owned()
}

fn background_played_episode_signal(candidate: &Alias, user: &Alias) -> SelectStatement {
    let publication = Alias::new("background_home_played_publication");
    let member = Alias::new("background_home_played_member");
    let item = Alias::new("background_home_played_item");
    let data = Alias::new("background_home_played_data");
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_publications"), publication.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("publication_catalog_items"),
            member.clone(),
            Expr::col((member.clone(), Alias::new("publication_id")))
                .equals((publication.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id")))
                .equals((member, Alias::new("catalog_item_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("user_data"),
            data.clone(),
            Condition::all()
                .add(
                    Expr::col((data.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((data.clone(), Alias::new("user_id")))
                        .equals((user.clone(), Alias::new("id"))),
                ),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("id"))).equals((
            candidate.clone(),
            Alias::new("active_structure_publication_id"),
        )))
        .and_where(
            Expr::col((publication.clone(), Alias::new("owner_catalog_item_id")))
                .equals((candidate.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"))
        .and_where(Expr::col((item.clone(), Alias::new("item_type"))).eq("Episode"))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item.clone(), Alias::new("classification_state"))).eq("Matched"))
        .and_where(
            Expr::col((item, Alias::new("structure_owner_item_id")))
                .equals((candidate.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((data, Alias::new("is_played"))).eq(true))
        .limit(1)
        .to_owned()
}

fn background_unstarted_episode_signal(candidate: &Alias, user: &Alias) -> SelectStatement {
    let publication = Alias::new("background_home_next_publication");
    let member = Alias::new("background_home_next_member");
    let item = Alias::new("background_home_next_item");
    let data = Alias::new("background_home_next_data");
    Query::select()
        .expr(Expr::val(1_i32))
        .from_as(Alias::new("catalog_publications"), publication.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("publication_catalog_items"),
            member.clone(),
            Expr::col((member.clone(), Alias::new("publication_id")))
                .equals((publication.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id")))
                .equals((member, Alias::new("catalog_item_id"))),
        )
        .join_as(
            JoinType::LeftJoin,
            Alias::new("user_data"),
            data.clone(),
            Condition::all()
                .add(
                    Expr::col((data.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((data.clone(), Alias::new("user_id")))
                        .equals((user.clone(), Alias::new("id"))),
                ),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("id"))).equals((
            candidate.clone(),
            Alias::new("active_structure_publication_id"),
        )))
        .and_where(
            Expr::col((publication.clone(), Alias::new("owner_catalog_item_id")))
                .equals((candidate.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((publication.clone(), Alias::new("publication_kind"))).eq("Structure"))
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"))
        .and_where(Expr::col((item.clone(), Alias::new("item_type"))).eq("Episode"))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item.clone(), Alias::new("classification_state"))).eq("Matched"))
        .and_where(
            Expr::col((item, Alias::new("structure_owner_item_id")))
                .equals((candidate.clone(), Alias::new("id"))),
        )
        .cond_where(
            Condition::any()
                .add(Expr::col((data.clone(), Alias::new("is_played"))).is_null())
                .add(Expr::col((data.clone(), Alias::new("is_played"))).eq(false)),
        )
        .cond_where(
            Condition::any()
                .add(Expr::col((data.clone(), Alias::new("playback_position_ticks"))).is_null())
                .add(Expr::col((data, Alias::new("playback_position_ticks"))).eq(0_i64)),
        )
        .limit(1)
        .to_owned()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullScanPolicy {
    scan_profile: String,
    object_selection_scope: String,
    metadata_policy: String,
    metadata_source_mode: MetadataSourceMode,
    local_metadata_access_mode: LocalMetadataAccessMode,
    expansion_policy: String,
    probe_policy: String,
}

impl FullScanPolicy {
    #[must_use]
    pub fn scan_profile(&self) -> &str {
        &self.scan_profile
    }

    #[must_use]
    pub const fn metadata_source_mode(&self) -> MetadataSourceMode {
        self.metadata_source_mode
    }

    #[must_use]
    pub const fn local_metadata_access_mode(&self) -> LocalMetadataAccessMode {
        self.local_metadata_access_mode
    }

    #[must_use]
    pub fn selects_all_synced_objects(&self) -> bool {
        self.object_selection_scope == "all_synced_objects"
    }

    #[must_use]
    pub fn selects_title_layer(&self) -> bool {
        self.object_selection_scope == "title_layer"
    }

    /// Returns the metadata depth requested by the persisted effective policy.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanRepositoryError::InvalidStoredPolicy`] for an invalid policy value.
    pub fn metadata_requirement(
        &self,
    ) -> Result<Option<crate::MetadataRequirement>, FullScanRepositoryError> {
        let requirement = match self.metadata_policy.as_str() {
            "none" => Ok(None),
            "basic" => Ok(Some(crate::MetadataRequirement::Basic)),
            "full" => Ok(Some(crate::MetadataRequirement::Full)),
            _ => Err(FullScanRepositoryError::InvalidStoredPolicy),
        }?;
        if self.scan_profile == "Lazy" {
            Ok(None)
        } else {
            Ok(requirement)
        }
    }

    #[must_use]
    pub fn resolves_metadata(&self) -> bool {
        self.metadata_policy != "none"
    }

    #[must_use]
    pub fn expands_eagerly(&self) -> bool {
        self.expansion_policy == "eager"
    }

    #[must_use]
    pub fn expands_in_background(&self) -> bool {
        self.expansion_policy == "background"
    }

    #[must_use]
    pub fn probes_eagerly(&self) -> bool {
        self.probe_policy == "eager"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FullScanRoot {
    binding_id: LibraryRootBindingId,
    library_id: LibraryId,
    root_id: StorageRootId,
    root_object_id: StorageObjectRecordId,
    sync_revision: i64,
    reconciled_revision: i64,
    discovered_revision: i64,
    children_indexed: bool,
    children_revision: i64,
}

impl FullScanRoot {
    #[must_use]
    pub const fn binding_id(self) -> LibraryRootBindingId {
        self.binding_id
    }
    #[must_use]
    pub const fn library_id(self) -> LibraryId {
        self.library_id
    }
    #[must_use]
    pub const fn root_id(self) -> StorageRootId {
        self.root_id
    }
    #[must_use]
    pub const fn root_object_id(self) -> StorageObjectRecordId {
        self.root_object_id
    }
    #[must_use]
    pub const fn reconciled_revision(self) -> i64 {
        self.reconciled_revision
    }
    #[must_use]
    pub const fn sync_revision(self) -> i64 {
        self.sync_revision
    }
    #[must_use]
    pub const fn needs_inventory(self) -> bool {
        !self.children_indexed
    }
    #[must_use]
    pub const fn needs_discovery(self) -> bool {
        self.children_indexed && self.discovered_revision < self.reconciled_revision
    }
    #[must_use]
    pub const fn children_revision(self) -> i64 {
        self.children_revision
    }
}

async fn fence_root_submission(
    transaction: &DatabaseTransaction,
    binding: RootBindingContext,
) -> Result<(), FullScanRepositoryError> {
    let backend = transaction.get_database_backend();
    let binding_fence = Query::update()
        .table(Alias::new("library_storage_roots"))
        .value(Alias::new("id"), Expr::col(Alias::new("id")))
        .and_where(Expr::col(Alias::new("id")).eq(binding.binding_id.as_uuid()))
        .and_where(Expr::col(Alias::new("library_id")).eq(binding.library_id.as_uuid()))
        .and_where(Expr::col(Alias::new("storage_root_id")).eq(binding.root_id.as_uuid()))
        .to_owned();
    let library_fence = Query::update()
        .table(Alias::new("libraries"))
        .value(
            Alias::new("profile_version"),
            Expr::col(Alias::new("profile_version")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(binding.library_id.as_uuid()))
        .and_where(Expr::col(Alias::new("profile_version")).eq(binding.profile_version))
        .and_where(Expr::col(Alias::new("is_enabled")).eq(true))
        .to_owned();
    if transaction
        .execute(backend.build(&binding_fence))
        .await?
        .rows_affected()
        != 1
        || transaction
            .execute(backend.build(&library_fence))
            .await?
            .rows_affected()
            != 1
    {
        return Err(FullScanRepositoryError::UnavailableLibraryRoot);
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct FullScanContext {
    library_id: LibraryId,
    binding_id: Option<LibraryRootBindingId>,
    root_id: Option<StorageRootId>,
    policy: FullScanPolicy,
}

#[derive(Clone, Copy, Debug)]
struct RootBindingContext {
    binding_id: LibraryRootBindingId,
    library_id: LibraryId,
    root_id: StorageRootId,
    profile_version: i32,
    metadata_source_mode: MetadataSourceMode,
    local_metadata_access_mode: LocalMetadataAccessMode,
}

async fn scan_context(
    connection: &DatabaseConnection,
    claimed: &ClaimedWorkJob,
) -> Result<FullScanContext, FullScanRepositoryError> {
    match (claimed.job().task_kind(), claimed.job().scope()) {
        (WorkTaskKind::FullMediaScan, WorkScope::Library(library_id)) => {
            let row = connection
                .query_one(
                    connection.get_database_backend().build(
                        &Query::select()
                            .columns([
                                Alias::new("scan_profile"),
                                Alias::new("object_selection_scope"),
                                Alias::new("metadata_policy"),
                                Alias::new("metadata_source_mode"),
                                Alias::new("local_metadata_access_mode"),
                                Alias::new("expansion_policy"),
                                Alias::new("probe_policy"),
                            ])
                            .from(Alias::new("libraries"))
                            .and_where(Expr::col(Alias::new("id")).eq(library_id.as_uuid()))
                            .and_where(Expr::col(Alias::new("is_enabled")).eq(true))
                            .and_where(
                                Expr::col(Alias::new("profile_version"))
                                    .eq(claimed.job().expected_revision()),
                            )
                            .limit(1)
                            .to_owned(),
                    ),
                )
                .await?
                .ok_or(FullScanRepositoryError::StaleLibrary)?;
            let policy = FullScanPolicy {
                scan_profile: row.try_get("", "scan_profile")?,
                object_selection_scope: row.try_get("", "object_selection_scope")?,
                metadata_policy: row.try_get("", "metadata_policy")?,
                metadata_source_mode: row
                    .try_get::<String>("", "metadata_source_mode")?
                    .parse()
                    .map_err(|_| FullScanRepositoryError::InvalidStoredPolicy)?,
                local_metadata_access_mode: row
                    .try_get::<String>("", "local_metadata_access_mode")?
                    .parse()
                    .map_err(|_| FullScanRepositoryError::InvalidStoredPolicy)?,
                expansion_policy: row.try_get("", "expansion_policy")?,
                probe_policy: row.try_get("", "probe_policy")?,
            };
            crate::library::validate_effective_policy_values(
                &policy.object_selection_scope,
                &policy.metadata_policy,
                &policy.expansion_policy,
                &policy.probe_policy,
            )
            .map_err(|_| FullScanRepositoryError::InvalidStoredPolicy)?;
            Ok(FullScanContext {
                library_id,
                binding_id: None,
                root_id: None,
                policy,
            })
        }
        (WorkTaskKind::FullLibraryRootScan, WorkScope::LibraryRootBinding(binding_id)) => {
            let binding = binding_context(connection, binding_id).await?;
            if i64::from(binding.profile_version) != claimed.job().expected_revision() {
                return Err(FullScanRepositoryError::StaleLibrary);
            }
            Ok(FullScanContext {
                library_id: binding.library_id,
                binding_id: Some(binding.binding_id),
                root_id: Some(binding.root_id),
                policy: FullScanPolicy {
                    scan_profile: "Full".to_owned(),
                    object_selection_scope: "all_synced_objects".to_owned(),
                    metadata_policy: "full".to_owned(),
                    metadata_source_mode: binding.metadata_source_mode,
                    local_metadata_access_mode: binding.local_metadata_access_mode,
                    expansion_policy: "eager".to_owned(),
                    probe_policy: "eager".to_owned(),
                },
            })
        }
        _ => Err(FullScanRepositoryError::InvalidClaim),
    }
}

async fn root_binding_context(
    transaction: &DatabaseTransaction,
    library_id: LibraryId,
    root_id: StorageRootId,
) -> Result<RootBindingContext, FullScanRepositoryError> {
    let binding = Alias::new("manual_full_binding");
    let library = Alias::new("manual_full_library");
    let row = transaction
        .query_one(
            transaction.get_database_backend().build(
                &Query::select()
                    .expr_as(
                        Expr::col((binding.clone(), Alias::new("id"))),
                        Alias::new("binding_id"),
                    )
                    .expr_as(
                        Expr::col((library.clone(), Alias::new("profile_version"))),
                        Alias::new("profile_version"),
                    )
                    .expr_as(
                        Expr::col((library.clone(), Alias::new("metadata_source_mode"))),
                        Alias::new("metadata_source_mode"),
                    )
                    .expr_as(
                        Expr::col((library.clone(), Alias::new("local_metadata_access_mode"))),
                        Alias::new("local_metadata_access_mode"),
                    )
                    .from_as(Alias::new("library_storage_roots"), binding.clone())
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new("libraries"),
                        library.clone(),
                        Expr::col((library.clone(), Alias::new("id")))
                            .equals((binding.clone(), Alias::new("library_id"))),
                    )
                    .and_where(
                        Expr::col((binding.clone(), Alias::new("library_id")))
                            .eq(library_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col((binding, Alias::new("storage_root_id"))).eq(root_id.as_uuid()),
                    )
                    .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
                    .limit(1)
                    .to_owned(),
            ),
        )
        .await?
        .ok_or(FullScanRepositoryError::UnavailableLibraryRoot)?;
    Ok(RootBindingContext {
        binding_id: LibraryRootBindingId::from_uuid(row.try_get("", "binding_id")?),
        library_id,
        root_id,
        profile_version: row.try_get("", "profile_version")?,
        metadata_source_mode: row
            .try_get::<String>("", "metadata_source_mode")?
            .parse()
            .map_err(|_| FullScanRepositoryError::InvalidStoredPolicy)?,
        local_metadata_access_mode: row
            .try_get::<String>("", "local_metadata_access_mode")?
            .parse()
            .map_err(|_| FullScanRepositoryError::InvalidStoredPolicy)?,
    })
}

async fn binding_context<Connection>(
    connection: &Connection,
    binding_id: LibraryRootBindingId,
) -> Result<RootBindingContext, FullScanRepositoryError>
where
    Connection: ConnectionTrait,
{
    let binding = Alias::new("full_binding_context");
    let library = Alias::new("full_binding_library");
    let row = connection
        .query_one(
            connection.get_database_backend().build(
                &Query::select()
                    .expr_as(
                        Expr::col((binding.clone(), Alias::new("library_id"))),
                        Alias::new("library_id"),
                    )
                    .expr_as(
                        Expr::col((binding.clone(), Alias::new("storage_root_id"))),
                        Alias::new("storage_root_id"),
                    )
                    .expr_as(
                        Expr::col((library.clone(), Alias::new("profile_version"))),
                        Alias::new("profile_version"),
                    )
                    .expr_as(
                        Expr::col((library.clone(), Alias::new("metadata_source_mode"))),
                        Alias::new("metadata_source_mode"),
                    )
                    .expr_as(
                        Expr::col((library.clone(), Alias::new("local_metadata_access_mode"))),
                        Alias::new("local_metadata_access_mode"),
                    )
                    .from_as(Alias::new("library_storage_roots"), binding.clone())
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new("libraries"),
                        library.clone(),
                        Expr::col((library.clone(), Alias::new("id")))
                            .equals((binding.clone(), Alias::new("library_id"))),
                    )
                    .and_where(Expr::col((binding, Alias::new("id"))).eq(binding_id.as_uuid()))
                    .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
                    .limit(1)
                    .to_owned(),
            ),
        )
        .await?
        .ok_or(FullScanRepositoryError::StaleLibrary)?;
    Ok(RootBindingContext {
        binding_id,
        library_id: LibraryId::from_uuid(row.try_get("", "library_id")?),
        root_id: StorageRootId::from_uuid(row.try_get("", "storage_root_id")?),
        profile_version: row.try_get("", "profile_version")?,
        metadata_source_mode: row
            .try_get::<String>("", "metadata_source_mode")?
            .parse()
            .map_err(|_| FullScanRepositoryError::InvalidStoredPolicy)?,
        local_metadata_access_mode: row
            .try_get::<String>("", "local_metadata_access_mode")?
            .parse()
            .map_err(|_| FullScanRepositoryError::InvalidStoredPolicy)?,
    })
}

fn explicit_targets(
    library: uuid::Uuid,
    root_id: Option<StorageRootId>,
) -> sea_orm::sea_query::SelectStatement {
    let membership = Alias::new("scan_membership");
    let item = Alias::new("scan_item");
    let identity = Alias::new("scan_identity");
    let relation = Alias::new("scan_identity_root");
    let mut query = Query::select();
    query
        .expr_as(
            Expr::col((item.clone(), Alias::new("id"))),
            Alias::new("catalog_item_id"),
        )
        .from_as(Alias::new("library_catalog_items"), membership.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            item.clone(),
            Expr::col((item.clone(), Alias::new("id")))
                .equals((membership.clone(), Alias::new("catalog_item_id"))),
        )
        .and_where(Expr::col((membership, Alias::new("library_id"))).eq(library))
        .and_where(Expr::col((item.clone(), Alias::new("is_present"))).eq(true))
        .and_where(Expr::col((item.clone(), Alias::new("classification_state"))).eq("Matched"));
    if let Some(root_id) = root_id {
        query
            .join_as(
                JoinType::InnerJoin,
                Alias::new("identity_matches"),
                identity.clone(),
                Expr::col((identity.clone(), Alias::new("candidate_catalog_item_id")))
                    .equals((item, Alias::new("id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_root_objects"),
                relation.clone(),
                Expr::col((relation.clone(), Alias::new("storage_object_id")))
                    .equals((identity.clone(), Alias::new("storage_object_id"))),
            )
            .and_where(Expr::col((identity, Alias::new("state"))).eq("Matched"))
            .and_where(Expr::col((relation, Alias::new("storage_root_id"))).eq(root_id.as_uuid()));
    }
    query
}

fn projected_targets(
    library: uuid::Uuid,
    root_id: Option<StorageRootId>,
) -> sea_orm::sea_query::SelectStatement {
    let owner = Alias::new("scan_owner");
    let membership = Alias::new("scan_owner_membership");
    let publication = Alias::new("scan_publication");
    let projected = Alias::new("scan_projected");
    let identity = Alias::new("scan_owner_identity");
    let relation = Alias::new("scan_owner_root");
    let mut query = Query::select();
    query
        .expr_as(
            Expr::col((projected.clone(), Alias::new("catalog_item_id"))),
            Alias::new("catalog_item_id"),
        )
        .from_as(Alias::new("library_catalog_items"), membership.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_items"),
            owner.clone(),
            Expr::col((owner.clone(), Alias::new("id")))
                .equals((membership.clone(), Alias::new("catalog_item_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("catalog_publications"),
            publication.clone(),
            Expr::col((publication.clone(), Alias::new("id")))
                .equals((owner.clone(), Alias::new("active_structure_publication_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("publication_catalog_items"),
            projected.clone(),
            Expr::col((projected.clone(), Alias::new("publication_id")))
                .equals((publication.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((membership, Alias::new("library_id"))).eq(library))
        .and_where(Expr::col((publication, Alias::new("state"))).eq("Active"));
    if let Some(root_id) = root_id {
        query
            .join_as(
                JoinType::InnerJoin,
                Alias::new("identity_matches"),
                identity.clone(),
                Expr::col((identity.clone(), Alias::new("candidate_catalog_item_id")))
                    .equals((owner, Alias::new("id"))),
            )
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_root_objects"),
                relation.clone(),
                Expr::col((relation.clone(), Alias::new("storage_object_id")))
                    .equals((identity.clone(), Alias::new("storage_object_id"))),
            )
            .and_where(Expr::col((identity, Alias::new("state"))).eq("Matched"))
            .and_where(Expr::col((relation, Alias::new("storage_root_id"))).eq(root_id.as_uuid()))
            .and_where(Expr::col((projected, Alias::new("storage_root_id"))).eq(root_id.as_uuid()));
    }
    query
}

async fn finish<T>(
    transaction: DatabaseTransaction,
    result: Result<T, FullScanRepositoryError>,
) -> Result<T, FullScanRepositoryError> {
    match result {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(original) => match transaction.rollback().await {
            Ok(()) => Err(original),
            Err(rollback) => Err(FullScanRepositoryError::RollbackFailed {
                original: original.to_string(),
                rollback,
            }),
        },
    }
}

#[derive(Debug, Error)]
pub enum FullScanRepositoryError {
    #[error("claimed work is not a library-wide or library-root Full Media Scan")]
    InvalidClaim,
    #[error("library is disabled, missing, or its profile version changed")]
    StaleLibrary,
    #[error("library root binding is missing or its library is disabled")]
    UnavailableLibraryRoot,
    #[error("library has an invalid persisted effective scan policy")]
    InvalidStoredPolicy,
    #[error("background expansion candidate limit must be between 1 and 64")]
    InvalidCandidateLimit,
    #[error("full scan {entity_kind} dependency is corrupt")]
    CorruptRootDependency { entity_kind: &'static str },
    #[error("full scan Hybrid candidate batch is corrupt")]
    CorruptHybridCandidateBatch,
    #[error("full scan target query failed: {0}")]
    Database(#[from] DbErr),
    #[error("full scan durable work operation failed: {0}")]
    Work(#[from] crate::WorkJobRepositoryError),
    #[error("rollback failed after {original}: {rollback}")]
    RollbackFailed { original: String, rollback: DbErr },
}
