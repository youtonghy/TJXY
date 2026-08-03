use sea_orm::DatabaseConnection;
use serde_json::json;
use thiserror::Error;
use tjxy_common::{CatalogItemId, StorageRootId, UserId};
use tjxy_db::{
    CatalogItemType, CatalogPublicationError, CatalogPublicationRepository, CatalogQueryError,
    CatalogQueryRepository, ClaimedWorkJob, FullScanChildSubmission, FullScanPolicy,
    FullScanRepository, FullScanRepositoryError, LazyCatalogWorkTarget, MetadataRequirement,
    WorkJobRepository, WorkJobRepositoryError, WorkJobResult, WorkJobSpec, WorkJobState, WorkScope,
    WorkTaskKind,
};
use tjxy_domain::MetadataSourceMode;

const HYBRID_BACKGROUND_PRIORITY: i32 = 5;
const MAX_HYBRID_BACKGROUND_CANDIDATES: u64 = 20;

pub struct FullScanService {
    database: DatabaseConnection,
}

impl FullScanService {
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Advances one Full Media Scan orchestration pass without reading a storage backend.
    ///
    /// # Errors
    ///
    /// Returns [`FullScanError::ChildrenPending`] while durable child work remains, or a persistence error.
    pub async fn execute(&self, claimed: &ClaimedWorkJob) -> Result<FullScanResult, FullScanError> {
        let scans = FullScanRepository::new(&self.database);
        let policy = scans.policy(claimed).await?;
        let targets = scans.targets(claimed).await?;
        let storage_root_scope = scans.storage_root_scope(claimed).await?;
        let query = CatalogQueryRepository::new(&self.database);
        let jobs = WorkJobRepository::new(&self.database);
        let publications = CatalogPublicationRepository::new(&self.database);
        let principal = UserId::new();
        let mut scheduled = self
            .schedule_root_prerequisites(claimed, &jobs, &policy)
            .await?;
        if scheduled > 0 {
            return Err(FullScanError::ChildrenPending { scheduled });
        }
        for item in &targets {
            let target = scan_work_target(&query, principal, *item, storage_root_scope).await?;
            let Some(target) = target else {
                continue;
            };
            if let Some(requirement) = policy.metadata_requirement()?
                && target.needs_metadata_resolution(requirement)
            {
                let (natural_key, spec) = metadata_child_spec(
                    *item,
                    target,
                    requirement,
                    policy.metadata_source_mode(),
                    claimed.job().priority(),
                    storage_root_scope,
                )?;
                scheduled += self
                    .required_publication_child(claimed, &scans, &jobs, &natural_key, &spec)
                    .await?;
                continue;
            }
            if let Some(task) = eager_media_task(&policy, target) {
                let (natural_key, spec) = eager_child_spec(
                    *item,
                    task,
                    target,
                    claimed.job().priority(),
                    storage_root_scope,
                )?;
                scheduled += self
                    .required_publication_child(claimed, &scans, &jobs, &natural_key, &spec)
                    .await?;
                continue;
            }
            if policy.probes_eagerly() {
                for source in publications.active_sources(*item).await? {
                    if source.probe_state() == "NotProbed" || source.probe_state() == "Stale" {
                        let spec = WorkJobSpec::new(
                            WorkTaskKind::ProbeMedia,
                            WorkScope::MediaSource(source.id()),
                            source.probe_revision(),
                            claimed.job().priority(),
                        )?;
                        let natural_key =
                            format!("ProbeMedia:{}:{}", source.id(), source.probe_revision());
                        scheduled += self
                            .required_publication_child(claimed, &scans, &jobs, &natural_key, &spec)
                            .await?;
                    }
                }
            }
        }
        if policy.expands_in_background() && scheduled == 0 {
            scheduled += self
                .schedule_background_expansion(claimed, &scans, &query, &jobs, principal)
                .await?;
        }
        if scheduled > 0 {
            return Err(FullScanError::ChildrenPending { scheduled });
        }
        jobs.complete_full_scan(
            claimed,
            WorkJobResult::success(json!({"items": targets.len()}), Vec::new()),
        )
        .await?;
        Ok(FullScanResult { scheduled })
    }

    async fn schedule_background_expansion(
        &self,
        claimed: &ClaimedWorkJob,
        scans: &FullScanRepository<'_>,
        query: &CatalogQueryRepository<'_>,
        jobs: &WorkJobRepository<'_>,
        principal: UserId,
    ) -> Result<usize, FullScanError> {
        let mut scheduled = 0;
        for item in scans
            .background_candidate_batch(claimed, MAX_HYBRID_BACKGROUND_CANDIDATES)
            .await?
        {
            let Some(target) = query.lazy_work_target(principal, item).await? else {
                continue;
            };
            if target.has_current_structure() {
                continue;
            }
            let Some(scope) = target.storage_scope() else {
                return Err(FullScanError::MissingStorageScope(item));
            };
            let (natural_key, spec) = if scope.is_ready() {
                (
                    format!("ExpandItem:{item}:{}", target.structure_revision()),
                    WorkJobSpec::new(
                        WorkTaskKind::ExpandItem,
                        WorkScope::CatalogItem(item),
                        target.structure_revision(),
                        HYBRID_BACKGROUND_PRIORITY,
                    )?
                    .with_input_sync_revision(scope.children_revision())?,
                )
            } else {
                (
                    format!(
                        "ScopedStorageSync:{}:{}",
                        scope.storage_object_id(),
                        scope.children_revision()
                    ),
                    WorkJobSpec::new(
                        WorkTaskKind::ScopedStorageSync,
                        WorkScope::StorageObject(scope.storage_object_id()),
                        scope.children_revision(),
                        HYBRID_BACKGROUND_PRIORITY,
                    )?
                    .with_storage_root_affinity(scope.storage_root_id())?,
                )
            };
            scheduled += self
                .required_publication_child(claimed, scans, jobs, &natural_key, &spec)
                .await?;
        }
        Ok(scheduled)
    }

    async fn schedule_root_prerequisites(
        &self,
        claimed: &ClaimedWorkJob,
        jobs: &WorkJobRepository<'_>,
        policy: &FullScanPolicy,
    ) -> Result<usize, FullScanError> {
        if !policy.selects_all_synced_objects() && !policy.selects_title_layer() {
            return Ok(0);
        }
        let mut scheduled = 0;
        let scans = FullScanRepository::new(&self.database);
        for root in scans.roots(claimed).await? {
            let recursive = policy.selects_all_synced_objects();
            let (natural_key, spec) = if recursive {
                (
                    format!("ValidateStorageRoot:{}", root.root_id()),
                    WorkJobSpec::new(
                        WorkTaskKind::ValidateStorageRoot,
                        WorkScope::StorageRoot(root.root_id()),
                        root.sync_revision(),
                        claimed.job().priority(),
                    )?
                    .with_storage_root_affinity(root.root_id())?,
                )
            } else {
                (
                    format!("ScopedStorageSync:{}", root.root_object_id()),
                    WorkJobSpec::new(
                        WorkTaskKind::ScopedStorageSync,
                        WorkScope::StorageObject(root.root_object_id()),
                        root.children_revision(),
                        claimed.job().priority(),
                    )?
                    .with_storage_root_affinity(root.root_id())?,
                )
            };
            match self
                .tracked_frozen_child_state(claimed, &scans, jobs, &natural_key, &spec)
                .await?
            {
                Some(WorkJobState::Pending | WorkJobState::Running) => {
                    scheduled += 1;
                    continue;
                }
                Some(WorkJobState::Failed) => {
                    return Err(if recursive {
                        FullScanError::ValidationFailed(root.root_id())
                    } else {
                        FullScanError::InventoryFailed(root.root_id())
                    });
                }
                None | Some(WorkJobState::Completed) => {}
            }
            let spec = if root.needs_discovery() {
                Some(WorkJobSpec::new(
                    WorkTaskKind::DiscoverTitles,
                    WorkScope::LibraryRootBinding(root.binding_id()),
                    root.reconciled_revision(),
                    claimed.job().priority(),
                )?)
            } else {
                None
            };
            if let Some(spec) = spec {
                let natural_key = format!(
                    "DiscoverTitles:{}:{}",
                    root.binding_id(),
                    root.reconciled_revision()
                );
                scheduled += self
                    .required_publication_child(claimed, &scans, jobs, &natural_key, &spec)
                    .await?;
            }
        }
        Ok(scheduled)
    }

    async fn tracked_child_state(
        &self,
        claimed: &ClaimedWorkJob,
        scans: &FullScanRepository<'_>,
        jobs: &WorkJobRepository<'_>,
        natural_key: &str,
        spec: &WorkJobSpec,
    ) -> Result<Option<WorkJobState>, FullScanError> {
        self.tracked_child_state_with_revision_check(claimed, scans, jobs, natural_key, spec, true)
            .await
    }

    async fn tracked_frozen_child_state(
        &self,
        claimed: &ClaimedWorkJob,
        scans: &FullScanRepository<'_>,
        jobs: &WorkJobRepository<'_>,
        natural_key: &str,
        spec: &WorkJobSpec,
    ) -> Result<Option<WorkJobState>, FullScanError> {
        self.tracked_child_state_with_revision_check(claimed, scans, jobs, natural_key, spec, false)
            .await
    }

    async fn tracked_child_state_with_revision_check(
        &self,
        claimed: &ClaimedWorkJob,
        scans: &FullScanRepository<'_>,
        jobs: &WorkJobRepository<'_>,
        natural_key: &str,
        spec: &WorkJobSpec,
        check_expected_revision: bool,
    ) -> Result<Option<WorkJobState>, FullScanError> {
        let child = if let Some(job_id) = scans.child_dependency(claimed, natural_key).await? {
            jobs.get(job_id)
                .await?
                .ok_or_else(|| FullScanError::MissingChildDependency(natural_key.to_owned()))?
        } else {
            match jobs
                .enqueue_full_scan_child(claimed, natural_key, spec)
                .await?
            {
                FullScanChildSubmission::Current => return Ok(None),
                FullScanChildSubmission::Stale => {
                    return Err(FullScanError::ChildTargetChanged {
                        task: spec.task_kind(),
                        scope: spec.scope(),
                    });
                }
                FullScanChildSubmission::Job(submission) => submission.job().clone(),
            }
        };
        if child.task_kind() != spec.task_kind()
            || child.scope() != spec.scope()
            || (check_expected_revision && child.expected_revision() != spec.expected_revision())
            || child.storage_root_affinity() != spec.storage_root_affinity()
        {
            return Err(FullScanError::CorruptChildDependency(
                natural_key.to_owned(),
            ));
        }
        Ok(Some(child.state()))
    }

    async fn required_publication_child(
        &self,
        claimed: &ClaimedWorkJob,
        scans: &FullScanRepository<'_>,
        jobs: &WorkJobRepository<'_>,
        natural_key: &str,
        spec: &WorkJobSpec,
    ) -> Result<usize, FullScanError> {
        match self
            .tracked_child_state(claimed, scans, jobs, natural_key, spec)
            .await?
        {
            None => Ok(0),
            Some(WorkJobState::Pending | WorkJobState::Running) => Ok(1),
            Some(WorkJobState::Failed) => Err(FullScanError::ChildFailed {
                task: spec.task_kind(),
                scope: spec.scope(),
            }),
            Some(WorkJobState::Completed) => Err(FullScanError::ChildCompletedWithoutPublication {
                task: spec.task_kind(),
                scope: spec.scope(),
            }),
        }
    }
}

fn eager_media_task(
    policy: &FullScanPolicy,
    target: LazyCatalogWorkTarget,
) -> Option<WorkTaskKind> {
    if !policy.expands_eagerly() {
        return None;
    }
    match target.item_type() {
        CatalogItemType::Series if !target.has_current_structure() => {
            Some(WorkTaskKind::ExpandItem)
        }
        CatalogItemType::Movie | CatalogItemType::Episode | CatalogItemType::Audio
            if !target.has_current_sources() =>
        {
            Some(WorkTaskKind::IndexMediaSources)
        }
        _ => None,
    }
}

fn eager_child_spec(
    item: CatalogItemId,
    task: WorkTaskKind,
    target: LazyCatalogWorkTarget,
    priority: i32,
    storage_root: Option<StorageRootId>,
) -> Result<(String, WorkJobSpec), FullScanError> {
    let revision = match task {
        WorkTaskKind::ExpandItem => target.structure_revision(),
        _ => target.source_revision(),
    };
    let Some(scope) = target.storage_scope() else {
        return Err(FullScanError::MissingStorageScope(item));
    };
    let direct_audio =
        task == WorkTaskKind::IndexMediaSources && target.item_type() == CatalogItemType::Audio;
    if scope.is_ready() || (direct_audio && scope.is_ready_for_direct_source()) {
        let spec = root_affine_spec(
            WorkJobSpec::new(task, WorkScope::CatalogItem(item), revision, priority)?
                .with_input_sync_revision(if direct_audio {
                    scope.metadata_input_revision()
                } else {
                    scope.children_revision()
                })?,
            storage_root.or(Some(scope.storage_root_id())),
        )?;
        Ok((format!("{}:{item}:{revision}", task.as_str()), spec))
    } else {
        Ok((
            format!(
                "ScopedStorageSync:{}:{}",
                scope.storage_object_id(),
                scope.children_revision()
            ),
            WorkJobSpec::new(
                WorkTaskKind::ScopedStorageSync,
                WorkScope::StorageObject(scope.storage_object_id()),
                scope.children_revision(),
                priority,
            )?
            .with_storage_root_affinity(scope.storage_root_id())?,
        ))
    }
}

async fn scan_work_target(
    query: &CatalogQueryRepository<'_>,
    principal: UserId,
    item: CatalogItemId,
    storage_root: Option<StorageRootId>,
) -> Result<Option<LazyCatalogWorkTarget>, CatalogQueryError> {
    if let Some(storage_root) = storage_root {
        query
            .lazy_work_target_in_storage_root(principal, item, storage_root)
            .await
    } else {
        query.lazy_work_target(principal, item).await
    }
}

fn root_affine_spec(
    spec: WorkJobSpec,
    storage_root: Option<StorageRootId>,
) -> Result<WorkJobSpec, WorkJobRepositoryError> {
    match storage_root {
        Some(storage_root) => spec.with_storage_root_affinity(storage_root),
        None => Ok(spec),
    }
}

fn metadata_child_spec(
    item: CatalogItemId,
    target: LazyCatalogWorkTarget,
    requirement: MetadataRequirement,
    metadata_source_mode: MetadataSourceMode,
    priority: i32,
    storage_root_scope: Option<StorageRootId>,
) -> Result<(String, WorkJobSpec), FullScanError> {
    let Some(scope) = target.storage_scope() else {
        return Err(FullScanError::MissingStorageScope(item));
    };
    let spec = WorkJobSpec::new(
        WorkTaskKind::ResolveMetadata,
        WorkScope::CatalogItem(item),
        target.metadata_revision(),
        priority,
    )?
    .with_metadata_requirement(requirement)?
    .with_metadata_source_mode(metadata_source_mode)?
    .with_input_sync_revision(scope.metadata_input_revision())?;
    let spec = root_affine_spec(spec, storage_root_scope.or(Some(scope.storage_root_id())))?;
    Ok((
        format!(
            "ResolveMetadata:{item}:{}:{}:{}",
            target.metadata_revision(),
            requirement.as_i32(),
            metadata_source_mode.as_str()
        ),
        spec,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FullScanResult {
    scheduled: usize,
}
impl FullScanResult {
    #[must_use]
    pub const fn scheduled(self) -> usize {
        self.scheduled
    }
}

#[derive(Debug, Error)]
pub enum FullScanError {
    #[error("full scan scheduled {scheduled} child jobs")]
    ChildrenPending { scheduled: usize },
    #[error("full scan item {0} has no matched storage scope")]
    MissingStorageScope(tjxy_common::CatalogItemId),
    #[error("full scan validation dependency for root {0} is missing")]
    MissingValidationDependency(tjxy_common::StorageRootId),
    #[error("full scan title-layer inventory dependency for root {0} is missing")]
    MissingInventoryDependency(tjxy_common::StorageRootId),
    #[error("full scan validation failed for root {0}")]
    ValidationFailed(tjxy_common::StorageRootId),
    #[error("full scan title-layer inventory failed for root {0}")]
    InventoryFailed(tjxy_common::StorageRootId),
    #[error("full scan child dependency {0} is missing")]
    MissingChildDependency(String),
    #[error("full scan child dependency {0} is corrupt")]
    CorruptChildDependency(String),
    #[error("full scan child {task:?} for {scope:?} failed")]
    ChildFailed {
        task: WorkTaskKind,
        scope: WorkScope,
    },
    #[error("full scan child {task:?} for {scope:?} completed without publishing its state")]
    ChildCompletedWithoutPublication {
        task: WorkTaskKind,
        scope: WorkScope,
    },
    #[error("full scan child {task:?} for {scope:?} changed while it was being scheduled")]
    ChildTargetChanged {
        task: WorkTaskKind,
        scope: WorkScope,
    },
    #[error("full scan target query failed: {0}")]
    Repository(#[from] FullScanRepositoryError),
    #[error("full scan catalog query failed: {0}")]
    Query(#[from] CatalogQueryError),
    #[error("full scan publication query failed: {0}")]
    Publication(#[from] CatalogPublicationError),
    #[error("full scan work operation failed: {0}")]
    Work(#[from] WorkJobRepositoryError),
}
