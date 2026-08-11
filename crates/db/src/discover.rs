use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Cond, Expr, JoinType, OnConflict, Query},
};
use serde_json::{Value, json};
use thiserror::Error;
use tjxy_common::{
    CatalogItemId, LibraryRootBindingId, SortKey, StorageObjectRecordId, StorageRootId,
};
use tjxy_domain::MetadataSourceMode;
use uuid::Uuid;

use crate::{
    CatalogPublicationError, ClaimedWorkJob, MetadataRequirement, WorkJobRepository,
    WorkJobRepositoryError, WorkJobResult, WorkJobSubmission, WorkScope, WorkTaskKind,
};

const MAX_TITLES: u64 = 100_000;
const DISCOVERY_LIBRARY_KIND: &str = "DiscoverLibrary";

#[doc(hidden)]
pub async fn enqueue_after_root_sync(
    transaction: &DatabaseTransaction,
    root: StorageRootId,
    parent: StorageObjectRecordId,
    revision: i64,
    priority: i32,
    now: chrono::DateTime<Utc>,
) -> Result<(), WorkJobRepositoryError> {
    let libraries =
        eligible_library_scopes(transaction, root, revision, true, Some(parent)).await?;
    if !libraries.is_empty() {
        let submission = crate::work_job::enqueue_in_transaction(
            transaction,
            &crate::WorkJobSpec::new(
                WorkTaskKind::DiscoverTitles,
                WorkScope::StorageRoot(root),
                revision,
                priority,
            )?,
            now,
        )
        .await?;
        stage_discovery_libraries(transaction, submission.job().id(), &libraries).await?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DiscoveryLibraryScope {
    id: Uuid,
    profile_version: i32,
}

#[derive(Clone, Debug)]
pub struct DiscoverTitlesSnapshot {
    scope: WorkScope,
    root_id: StorageRootId,
    root_object_id: StorageObjectRecordId,
    libraries: Vec<DiscoveryLibraryScope>,
    titles: Vec<DiscoveredTitle>,
}

impl DiscoverTitlesSnapshot {
    #[must_use]
    pub fn title_count(&self) -> usize {
        self.titles.len()
    }
}

#[derive(Clone, Debug)]
pub struct DiscoveredTitle {
    item_id: CatalogItemId,
    library_id: Uuid,
    storage_object_id: StorageObjectRecordId,
    item_type: String,
    name: String,
    production_year: Option<i32>,
    metadata_requirement: Option<MetadataRequirement>,
    metadata_source_mode: MetadataSourceMode,
    children_indexed: bool,
    children_revision: i64,
}

pub struct DiscoverTitlesRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> DiscoverTitlesRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Enqueues explicit title discovery for one reconciled enabled Library root.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoverTitlesError`] when the root is unavailable, current, or invalid.
    pub async fn enqueue(
        &self,
        root: StorageRootId,
        priority: i32,
    ) -> Result<WorkJobSubmission, DiscoverTitlesError> {
        let storage_root = Alias::new("manual_discover_root");
        let relation = Alias::new("manual_discover_root_object");
        let query = Query::select()
            .expr_as(
                Expr::col((storage_root.clone(), Alias::new("reconciled_sync_revision"))),
                Alias::new("reconciled_sync_revision"),
            )
            .from_as(Alias::new("storage_roots"), storage_root.clone())
            .join_as(
                JoinType::InnerJoin,
                Alias::new("storage_root_objects"),
                relation.clone(),
                Expr::col((relation.clone(), Alias::new("storage_root_id")))
                    .equals((storage_root.clone(), Alias::new("id"))),
            )
            .and_where(Expr::col((storage_root, Alias::new("id"))).eq(root.as_uuid()))
            .and_where(
                Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))).is_null(),
            )
            .and_where(Expr::col((relation, Alias::new("children_indexed"))).eq(true))
            .limit(1)
            .to_owned();
        let transaction = self.database.begin().await?;
        let row = transaction
            .query_one(transaction.get_database_backend().build(&query))
            .await?
            .ok_or(DiscoverTitlesError::StaleRoot)?;
        let revision: i64 = row.try_get("", "reconciled_sync_revision")?;
        let libraries = eligible_library_scopes(&transaction, root, revision, false, None).await?;
        if libraries.is_empty() {
            transaction.rollback().await?;
            return Err(DiscoverTitlesError::AlreadyCurrent);
        }
        let result = async {
            let submission = crate::work_job::enqueue_in_transaction(
                &transaction,
                &crate::WorkJobSpec::new(
                    WorkTaskKind::DiscoverTitles,
                    WorkScope::StorageRoot(root),
                    revision,
                    priority,
                )?,
                Utc::now(),
            )
            .await?;
            stage_discovery_libraries(&transaction, submission.job().id(), &libraries).await?;
            Ok::<_, DiscoverTitlesError>(submission)
        }
        .await;
        match result {
            Ok(submission) => {
                transaction.commit().await?;
                Ok(submission)
            }
            Err(error) => {
                transaction.rollback().await?;
                Err(error)
            }
        }
    }

    /// Selects bounded title-layer candidates from one reconciled SQL root.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoverTitlesError`] for invalid claims, classifications, names, or SQL failures.
    #[allow(clippy::too_many_lines)] // Keeps title classification and storage fencing in one snapshot boundary.
    pub async fn snapshot(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<DiscoverTitlesSnapshot, DiscoverTitlesError> {
        if claimed.job().task_kind() != WorkTaskKind::DiscoverTitles {
            return Err(DiscoverTitlesError::InvalidClaim);
        }
        let libraries = staged_discovery_libraries(self.database, claimed).await?;
        let root = match claimed.job().scope() {
            WorkScope::StorageRoot(root) => {
                let eligible = eligible_library_scopes(
                    self.database,
                    root,
                    claimed.job().expected_revision(),
                    false,
                    None,
                )
                .await?
                .into_iter()
                .map(|scope| (scope.id, scope.profile_version))
                .collect::<HashMap<_, _>>();
                if libraries
                    .iter()
                    .any(|scope| eligible.get(&scope.id).copied() != Some(scope.profile_version))
                {
                    return Err(DiscoverTitlesError::StaleLibraryPolicy);
                }
                root
            }
            WorkScope::LibraryRootBinding(binding_id) => {
                let (root, eligible) = binding_discovery_scope(
                    self.database,
                    binding_id,
                    claimed.job().expected_revision(),
                )
                .await?;
                if libraries.as_slice() != [eligible] {
                    return Err(DiscoverTitlesError::StaleLibraryPolicy);
                }
                root
            }
            _ => return Err(DiscoverTitlesError::InvalidClaim),
        };
        let root_object =
            discovery_root_object(self.database, root, claimed.job().expected_revision()).await?;
        let library_ids = libraries.iter().map(|scope| scope.id).collect::<Vec<_>>();
        let rows = self
            .database
            .query_all(self.database.get_database_backend().build(&candidate_query(
                root,
                claimed.job().expected_revision(),
                &library_ids,
            )))
            .await?;
        let mut titles = Vec::with_capacity(rows.len());
        for row in rows {
            let object = StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?);
            let collection: String = row.try_get("", "collection_type")?;
            let raw_name: String = row.try_get("", "name")?;
            let (item_type, name, production_year) = match collection.as_str() {
                "movies" => {
                    let (name, year) = parse_title(&raw_name)?;
                    ("Movie", name, year)
                }
                "tvshows" | "shows" | "mixed" => {
                    let (name, year) = parse_title(&raw_name)?;
                    ("Series", name, year)
                }
                "music" => {
                    let Some(name) = parse_audio_title(&raw_name) else {
                        continue;
                    };
                    ("Audio", name, None)
                }
                _ => return Err(DiscoverTitlesError::UnsupportedCollection),
            };
            let metadata_requirement = match row.try_get::<String>("", "metadata_policy")?.as_str()
            {
                "full" => Some(MetadataRequirement::Full),
                "basic" => Some(MetadataRequirement::Basic),
                "none" => None,
                _ => return Err(DiscoverTitlesError::InvalidMetadataPolicy),
            };
            let metadata_source_mode =
                row.try_get::<String>("", "metadata_source_mode")?
                    .parse()
                    .map_err(|_| DiscoverTitlesError::InvalidMetadataSourceMode)?;
            titles.push(DiscoveredTitle {
                item_id: derived_item(object),
                library_id: row.try_get("", "library_id")?,
                storage_object_id: object,
                item_type: item_type.to_owned(),
                name,
                production_year,
                metadata_requirement,
                metadata_source_mode,
                children_indexed: row.try_get("", "children_indexed")?,
                children_revision: row.try_get("", "children_index_revision")?,
            });
        }
        if u64::try_from(titles.len()).unwrap_or(u64::MAX) > MAX_TITLES {
            return Err(DiscoverTitlesError::TitleLimit);
        }
        let mut storage_objects = Vec::with_capacity(titles.len() + 1);
        storage_objects.push(root_object);
        storage_objects.extend(titles.iter().map(|title| title.storage_object_id));
        ensure_discovery_storage_facts(self.database, root, &storage_objects).await?;
        Ok(DiscoverTitlesSnapshot {
            scope: claimed.job().scope(),
            root_id: root,
            root_object_id: root_object,
            libraries,
            titles,
        })
    }

    /// Atomically publishes a fenced title snapshot and completes its work job.
    ///
    /// # Errors
    ///
    /// Returns [`DiscoverTitlesError`] for stale roots, identity conflicts, or SQL failures.
    pub async fn publish(
        &self,
        claimed: &ClaimedWorkJob,
        snapshot: &DiscoverTitlesSnapshot,
    ) -> Result<i64, DiscoverTitlesError> {
        if claimed.job().scope() != snapshot.scope
            || claimed.job().task_kind() != WorkTaskKind::DiscoverTitles
        {
            return Err(DiscoverTitlesError::InvalidClaim);
        }
        let transaction = self.database.begin().await?;
        let result = async {
            crate::work_job::fence_live_claim(&transaction, claimed, Utc::now()).await?;
            fence_discovery_snapshot(&transaction, claimed, snapshot).await?;
            let mut storage_objects = Vec::with_capacity(snapshot.titles.len() + 1);
            storage_objects.push(snapshot.root_object_id);
            storage_objects.extend(snapshot.titles.iter().map(|title| title.storage_object_id));
            ensure_discovery_storage_facts(&transaction, snapshot.root_id, &storage_objects)
                .await?;
            for title in &snapshot.titles {
                let metadata_revision = upsert_title(&transaction, title).await?;
                if let Some(requirement) = title.metadata_requirement {
                    let spec = crate::WorkJobSpec::new(
                        WorkTaskKind::ResolveMetadata,
                        WorkScope::CatalogItem(title.item_id),
                        metadata_revision,
                        claimed.job().priority(),
                    )?
                    .with_metadata_requirement(requirement)?
                    .with_metadata_source_mode(title.metadata_source_mode)?
                    .with_input_sync_revision(if title.children_indexed {
                        title.children_revision
                    } else {
                        claimed.job().expected_revision()
                    })?;
                    crate::work_job::enqueue_in_transaction(&transaction, &spec, Utc::now())
                        .await?;
                }
            }
            advance_discovery_watermarks(&transaction, claimed, snapshot).await?;
            let generation = crate::catalog_publication::advance_generation(&transaction).await?;
            WorkJobRepository::new(self.database)
                .complete_in_transaction(
                    &transaction,
                    claimed,
                    WorkJobResult::success(
                        json!({
                            "discovered": snapshot.titles.len(),
                            "libraries": snapshot.libraries.len(),
                            "catalog_generation": generation
                        }),
                        Vec::new(),
                    ),
                )
                .await?;
            Ok(generation)
        }
        .await;
        match result {
            Ok(value) => {
                transaction.commit().await?;
                Ok(value)
            }
            Err(error) => {
                transaction.rollback().await?;
                Err(error)
            }
        }
    }
}

async fn discovery_root_object(
    connection: &impl ConnectionTrait,
    root_id: StorageRootId,
    revision: i64,
) -> Result<StorageObjectRecordId, DiscoverTitlesError> {
    let relation = Alias::new("discover_snapshot_root_relation");
    let root = Alias::new("discover_snapshot_root");
    let query = Query::select()
        .expr_as(
            Expr::col((relation.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .from_as(Alias::new("storage_root_objects"), relation.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("id")))
                .equals((relation.clone(), Alias::new("storage_root_id"))),
        )
        .and_where(Expr::col((root.clone(), Alias::new("id"))).eq(root_id.as_uuid()))
        .and_where(Expr::col((root, Alias::new("reconciled_sync_revision"))).eq(revision))
        .and_where(Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))).is_null())
        .and_where(Expr::col((relation.clone(), Alias::new("children_indexed"))).eq(true))
        .and_where(Expr::col((relation, Alias::new("presence_state"))).eq("Present"))
        .limit(2)
        .to_owned();
    let rows = connection
        .query_all(connection.get_database_backend().build(&query))
        .await?;
    if rows.len() != 1 {
        return Err(DiscoverTitlesError::StaleRoot);
    }
    Ok(StorageObjectRecordId::from_uuid(
        rows[0].try_get("", "storage_object_id")?,
    ))
}

async fn ensure_discovery_storage_facts(
    connection: &impl ConnectionTrait,
    root_id: StorageRootId,
    storage_objects: &[StorageObjectRecordId],
) -> Result<(), DiscoverTitlesError> {
    if !crate::catalog_storage_scope::storage_objects_are_reconciled(
        connection,
        storage_objects,
        Some(root_id),
    )
    .await?
    {
        return Err(DiscoverTitlesError::StorageInputPending);
    }
    Ok(())
}

async fn binding_discovery_scope<Connection>(
    connection: &Connection,
    binding_id: LibraryRootBindingId,
    revision: i64,
) -> Result<(StorageRootId, DiscoveryLibraryScope), DiscoverTitlesError>
where
    Connection: ConnectionTrait,
{
    let binding = Alias::new("binding_discover_binding");
    let library = Alias::new("binding_discover_library");
    let root = Alias::new("binding_discover_root");
    let relation = Alias::new("binding_discover_root_object");
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
                        root.clone(),
                        Expr::col((root.clone(), Alias::new("id")))
                            .equals((binding.clone(), Alias::new("storage_root_id"))),
                    )
                    .join_as(
                        JoinType::InnerJoin,
                        Alias::new("storage_root_objects"),
                        relation.clone(),
                        Expr::col((relation.clone(), Alias::new("storage_root_id")))
                            .equals((root.clone(), Alias::new("id"))),
                    )
                    .and_where(
                        Expr::col((binding.clone(), Alias::new("id"))).eq(binding_id.as_uuid()),
                    )
                    .and_where(
                        Expr::col((binding, Alias::new("discovered_sync_revision"))).lt(revision),
                    )
                    .and_where(
                        Expr::col((root, Alias::new("reconciled_sync_revision"))).eq(revision),
                    )
                    .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
                    .and_where(
                        Expr::col((relation.clone(), Alias::new("parent_storage_object_id")))
                            .is_null(),
                    )
                    .and_where(
                        Expr::col((relation.clone(), Alias::new("children_indexed"))).eq(true),
                    )
                    .and_where(Expr::col((relation, Alias::new("presence_state"))).eq("Present"))
                    .limit(1)
                    .to_owned(),
            ),
        )
        .await?
        .ok_or(DiscoverTitlesError::StaleRoot)?;
    Ok((
        StorageRootId::from_uuid(row.try_get("", "storage_root_id")?),
        DiscoveryLibraryScope {
            id: row.try_get("", "library_id")?,
            profile_version: row.try_get("", "profile_version")?,
        },
    ))
}

async fn eligible_library_scopes<Connection>(
    connection: &Connection,
    root_id: StorageRootId,
    revision: i64,
    automatic: bool,
    parent: Option<StorageObjectRecordId>,
) -> Result<Vec<DiscoveryLibraryScope>, DbErr>
where
    Connection: ConnectionTrait,
{
    let root = Alias::new("eligible_discover_root");
    let binding = Alias::new("eligible_discover_binding");
    let library = Alias::new("eligible_discover_library");
    let relation = Alias::new("eligible_discover_root_object");
    let mut query = Query::select();
    query
        .expr_as(
            Expr::col((library.clone(), Alias::new("id"))),
            Alias::new("library_id"),
        )
        .expr_as(
            Expr::col((library.clone(), Alias::new("profile_version"))),
            Alias::new("profile_version"),
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
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((binding.clone(), Alias::new("library_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            relation.clone(),
            Expr::col((relation.clone(), Alias::new("storage_root_id")))
                .equals((root.clone(), Alias::new("id"))),
        )
        .and_where(Expr::col((root.clone(), Alias::new("id"))).eq(root_id.as_uuid()))
        .and_where(Expr::col((root, Alias::new("reconciled_sync_revision"))).eq(revision))
        .and_where(Expr::col((binding, Alias::new("discovered_sync_revision"))).lt(revision))
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true))
        .and_where(Expr::col((relation.clone(), Alias::new("parent_storage_object_id"))).is_null())
        .and_where(Expr::col((relation.clone(), Alias::new("children_indexed"))).eq(true))
        .and_where(Expr::col((relation.clone(), Alias::new("presence_state"))).eq("Present"))
        .order_by(
            (library.clone(), Alias::new("id")),
            sea_orm::sea_query::Order::Asc,
        );
    if automatic {
        query.and_where(
            Expr::col((library, Alias::new("object_selection_scope"))).ne("library_roots"),
        );
    }
    if let Some(parent) = parent {
        query
            .and_where(Expr::col((relation, Alias::new("storage_object_id"))).eq(parent.as_uuid()));
    }
    connection
        .query_all(connection.get_database_backend().build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok(DiscoveryLibraryScope {
                id: row.try_get("", "library_id")?,
                profile_version: row.try_get("", "profile_version")?,
            })
        })
        .collect()
}

async fn stage_discovery_libraries(
    transaction: &DatabaseTransaction,
    job_id: tjxy_common::WorkJobId,
    libraries: &[DiscoveryLibraryScope],
) -> Result<(), WorkJobRepositoryError> {
    let backend = transaction.get_database_backend();
    for library in libraries {
        let insert = Query::insert()
            .into_table(Alias::new("work_staging_rows"))
            .columns([
                Alias::new("id"),
                Alias::new("job_id"),
                Alias::new("publication_id"),
                Alias::new("entity_kind"),
                Alias::new("natural_key"),
                Alias::new("payload"),
                Alias::new("validation_state"),
            ])
            .values_panic([
                Uuid::new_v4().into(),
                job_id.as_uuid().into(),
                job_id.as_uuid().into(),
                DISCOVERY_LIBRARY_KIND.into(),
                library.id.to_string().into(),
                json!({"profile_version": library.profile_version}).into(),
                "Required".into(),
            ])
            .on_conflict(
                OnConflict::columns([
                    Alias::new("job_id"),
                    Alias::new("publication_id"),
                    Alias::new("entity_kind"),
                    Alias::new("natural_key"),
                ])
                .update_columns([Alias::new("payload"), Alias::new("validation_state")])
                .to_owned(),
            )
            .to_owned();
        transaction.execute(backend.build(&insert)).await?;
    }
    Ok(())
}

pub(crate) async fn stage_discovery_binding(
    transaction: &DatabaseTransaction,
    job_id: tjxy_common::WorkJobId,
    binding_id: LibraryRootBindingId,
    profile_version: i64,
) -> Result<(), WorkJobRepositoryError> {
    let profile_version = i32::try_from(profile_version)
        .map_err(|_| WorkJobRepositoryError::InvalidChildReference)?;
    let binding = Alias::new("staged_discover_binding");
    let library = Alias::new("staged_discover_library");
    let query = Query::select()
        .expr_as(
            Expr::col((binding.clone(), Alias::new("library_id"))),
            Alias::new("library_id"),
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
        .and_where(Expr::col((library.clone(), Alias::new("profile_version"))).eq(profile_version))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .limit(1)
        .to_owned();
    let row = transaction
        .query_one(transaction.get_database_backend().build(&query))
        .await?
        .ok_or(WorkJobRepositoryError::StaleParentPolicy)?;
    stage_discovery_libraries(
        transaction,
        job_id,
        &[DiscoveryLibraryScope {
            id: row.try_get("", "library_id")?,
            profile_version,
        }],
    )
    .await
}

async fn staged_discovery_libraries(
    database: &DatabaseConnection,
    claimed: &ClaimedWorkJob,
) -> Result<Vec<DiscoveryLibraryScope>, DiscoverTitlesError> {
    let query = Query::select()
        .columns([Alias::new("natural_key"), Alias::new("payload")])
        .from(Alias::new("work_staging_rows"))
        .and_where(Expr::col(Alias::new("job_id")).eq(claimed.id().as_uuid()))
        .and_where(Expr::col(Alias::new("publication_id")).eq(claimed.id().as_uuid()))
        .and_where(Expr::col(Alias::new("entity_kind")).eq(DISCOVERY_LIBRARY_KIND))
        .and_where(Expr::col(Alias::new("validation_state")).eq("Required"))
        .order_by(Alias::new("natural_key"), sea_orm::sea_query::Order::Asc)
        .to_owned();
    let mut libraries = Vec::new();
    for row in database
        .query_all(database.get_database_backend().build(&query))
        .await?
    {
        let id = row
            .try_get::<String>("", "natural_key")?
            .parse::<Uuid>()
            .map_err(|_| DiscoverTitlesError::InvalidLibraryScope)?;
        let payload: Value = row.try_get("", "payload")?;
        let profile_version = payload
            .get("profile_version")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .filter(|value| *value >= 0)
            .ok_or(DiscoverTitlesError::InvalidLibraryScope)?;
        libraries.push(DiscoveryLibraryScope {
            id,
            profile_version,
        });
    }
    if libraries.is_empty() {
        return Err(DiscoverTitlesError::MissingLibraryScope);
    }
    Ok(libraries)
}

async fn fence_discovery_snapshot(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    snapshot: &DiscoverTitlesSnapshot,
) -> Result<(), DiscoverTitlesError> {
    let backend = transaction.get_database_backend();
    let root_fence = Query::update()
        .table(Alias::new("storage_roots"))
        .value(
            Alias::new("reconciled_sync_revision"),
            Expr::col(Alias::new("reconciled_sync_revision")),
        )
        .and_where(Expr::col(Alias::new("id")).eq(snapshot.root_id.as_uuid()))
        .and_where(
            Expr::col(Alias::new("reconciled_sync_revision")).eq(claimed.job().expected_revision()),
        )
        .to_owned();
    if transaction
        .execute(backend.build(&root_fence))
        .await?
        .rows_affected()
        != 1
    {
        return Err(DiscoverTitlesError::StaleRoot);
    }
    if let WorkScope::LibraryRootBinding(binding_id) = snapshot.scope {
        let binding_fence = Query::update()
            .table(Alias::new("library_storage_roots"))
            .value(
                Alias::new("storage_root_id"),
                Expr::col(Alias::new("storage_root_id")),
            )
            .and_where(Expr::col(Alias::new("id")).eq(binding_id.as_uuid()))
            .and_where(Expr::col(Alias::new("storage_root_id")).eq(snapshot.root_id.as_uuid()))
            .to_owned();
        if transaction
            .execute(backend.build(&binding_fence))
            .await?
            .rows_affected()
            != 1
        {
            return Err(DiscoverTitlesError::StaleRoot);
        }
    }
    for library in &snapshot.libraries {
        let fence = Query::update()
            .table(Alias::new("libraries"))
            .value(
                Alias::new("profile_version"),
                Expr::col(Alias::new("profile_version")),
            )
            .and_where(Expr::col(Alias::new("id")).eq(library.id))
            .and_where(Expr::col(Alias::new("profile_version")).eq(library.profile_version))
            .and_where(Expr::col(Alias::new("is_enabled")).eq(true))
            .to_owned();
        if transaction
            .execute(backend.build(&fence))
            .await?
            .rows_affected()
            != 1
        {
            return Err(DiscoverTitlesError::StaleLibraryPolicy);
        }
    }
    Ok(())
}

async fn advance_discovery_watermarks(
    transaction: &DatabaseTransaction,
    claimed: &ClaimedWorkJob,
    snapshot: &DiscoverTitlesSnapshot,
) -> Result<(), DiscoverTitlesError> {
    let backend = transaction.get_database_backend();
    for library in &snapshot.libraries {
        let mut update = Query::update();
        update
            .table(Alias::new("library_storage_roots"))
            .value(
                Alias::new("discovered_sync_revision"),
                claimed.job().expected_revision(),
            )
            .and_where(Expr::col(Alias::new("library_id")).eq(library.id))
            .and_where(Expr::col(Alias::new("storage_root_id")).eq(snapshot.root_id.as_uuid()))
            .and_where(
                Expr::col(Alias::new("discovered_sync_revision"))
                    .lt(claimed.job().expected_revision()),
            );
        if let WorkScope::LibraryRootBinding(binding_id) = snapshot.scope {
            update.and_where(Expr::col(Alias::new("id")).eq(binding_id.as_uuid()));
        }
        if transaction
            .execute(backend.build(&update))
            .await?
            .rows_affected()
            != 1
        {
            return Err(DiscoverTitlesError::StaleRoot);
        }
    }
    let binding = Alias::new("remaining_discover_binding");
    let library = Alias::new("remaining_discover_library");
    let remaining = Query::select()
        .expr_as(
            Expr::col((binding.clone(), Alias::new("id"))).count(),
            Alias::new("count"),
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
            Expr::col((binding.clone(), Alias::new("storage_root_id")))
                .eq(snapshot.root_id.as_uuid()),
        )
        .and_where(
            Expr::col((binding, Alias::new("discovered_sync_revision")))
                .lt(claimed.job().expected_revision()),
        )
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .to_owned();
    let count = transaction
        .query_one(backend.build(&remaining))
        .await?
        .ok_or(DiscoverTitlesError::StaleRoot)?
        .try_get::<i64>("", "count")?;
    if count == 0 {
        let update = Query::update()
            .table(Alias::new("storage_roots"))
            .value(
                Alias::new("discovered_sync_revision"),
                claimed.job().expected_revision(),
            )
            .and_where(Expr::col(Alias::new("id")).eq(snapshot.root_id.as_uuid()))
            .and_where(
                Expr::col(Alias::new("reconciled_sync_revision"))
                    .eq(claimed.job().expected_revision()),
            )
            .and_where(
                Expr::col(Alias::new("discovered_sync_revision"))
                    .lt(claimed.job().expected_revision()),
            )
            .to_owned();
        transaction.execute(backend.build(&update)).await?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Keeps the three identity upserts in one audited transaction helper.
async fn upsert_title(
    transaction: &sea_orm::DatabaseTransaction,
    title: &DiscoveredTitle,
) -> Result<i64, DiscoverTitlesError> {
    let backend = transaction.get_database_backend();
    let structure_state = if title.item_type == "Series" {
        "NotExpanded"
    } else {
        "NotApplicable"
    };
    let source_state = if matches!(title.item_type.as_str(), "Movie" | "Audio") {
        "NotIndexed"
    } else {
        "Unknown"
    };
    let insert = Query::insert()
        .into_table(Alias::new("catalog_items"))
        .columns([
            Alias::new("id"),
            Alias::new("item_type"),
            Alias::new("name"),
            Alias::new("sort_name"),
            Alias::new("sort_key"),
            Alias::new("production_year"),
            Alias::new("classification_state"),
            Alias::new("metadata_state"),
            Alias::new("structure_state"),
            Alias::new("source_state"),
            Alias::new("structure_expansion_revision"),
            Alias::new("source_index_revision"),
            Alias::new("is_present"),
        ])
        .values_panic([
            title.item_id.as_uuid().into(),
            title.item_type.clone().into(),
            title.name.clone().into(),
            title.name.to_lowercase().into(),
            SortKey::from_text(&title.name).into_bytes().into(),
            title.production_year.into(),
            "Matched".into(),
            "Partial".into(),
            structure_state.into(),
            source_state.into(),
            0_i64.into(),
            0_i64.into(),
            true.into(),
        ])
        .on_conflict(idempotent_conflict(backend, "id"))
        .to_owned();
    transaction.execute(backend.build(&insert)).await?;
    let row = transaction
        .query_one(
            backend.build(
                &Query::select()
                    .columns([Alias::new("item_type"), Alias::new("metadata_revision")])
                    .from(Alias::new("catalog_items"))
                    .and_where(Expr::col(Alias::new("id")).eq(title.item_id.as_uuid()))
                    .to_owned(),
            ),
        )
        .await?
        .ok_or(DiscoverTitlesError::IdentityConflict)?;
    if row.try_get::<String>("", "item_type")? != title.item_type {
        return Err(DiscoverTitlesError::IdentityConflict);
    }
    transaction
        .execute(
            backend.build(
                &Query::insert()
                    .into_table(Alias::new("library_catalog_items"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("library_id"),
                        Alias::new("catalog_item_id"),
                    ])
                    .values_panic([
                        Uuid::new_v4().into(),
                        title.library_id.into(),
                        title.item_id.as_uuid().into(),
                    ])
                    .on_conflict(idempotent_conflict(backend, "catalog_item_id"))
                    .to_owned(),
            ),
        )
        .await?;
    transaction
        .execute(
            backend.build(
                &Query::insert()
                    .into_table(Alias::new("identity_matches"))
                    .columns([
                        Alias::new("id"),
                        Alias::new("storage_object_id"),
                        Alias::new("candidate_catalog_item_id"),
                        Alias::new("confidence"),
                        Alias::new("state"),
                        Alias::new("evidence"),
                    ])
                    .values_panic([
                        derived_identity(title.storage_object_id).into(),
                        title.storage_object_id.as_uuid().into(),
                        title.item_id.as_uuid().into(),
                        1.0.into(),
                        "Matched".into(),
                        json!({"source":"title-layer"}).into(),
                    ])
                    .on_conflict(idempotent_conflict(backend, "id"))
                    .to_owned(),
            ),
        )
        .await?;
    Ok(row.try_get("", "metadata_revision")?)
}

fn idempotent_conflict(backend: sea_orm::DbBackend, column: &'static str) -> OnConflict {
    if backend == sea_orm::DbBackend::MySql {
        OnConflict::new()
            .update_column(Alias::new(column))
            .to_owned()
    } else {
        OnConflict::new().do_nothing().to_owned()
    }
}

#[allow(clippy::too_many_lines)] // Keeps cross-backend music and video discovery in one fenced query.
fn candidate_query(
    root_id: StorageRootId,
    revision: i64,
    library_ids: &[Uuid],
) -> sea_orm::sea_query::SelectStatement {
    let root = Alias::new("discover_root");
    let root_object = Alias::new("discover_root_object");
    let child = Alias::new("discover_child");
    let object = Alias::new("discover_object");
    let binding = Alias::new("discover_binding");
    let library = Alias::new("discover_library");
    Query::select()
        .distinct()
        .expr_as(
            Expr::col((library.clone(), Alias::new("id"))),
            Alias::new("library_id"),
        )
        .expr_as(
            Expr::col((library.clone(), Alias::new("collection_type"))),
            Alias::new("collection_type"),
        )
        .expr_as(
            Expr::col((library.clone(), Alias::new("metadata_policy"))),
            Alias::new("metadata_policy"),
        )
        .expr_as(
            Expr::col((library.clone(), Alias::new("metadata_source_mode"))),
            Alias::new("metadata_source_mode"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((child.clone(), Alias::new("children_indexed"))),
            Alias::new("children_indexed"),
        )
        .expr_as(
            Expr::col((child.clone(), Alias::new("children_index_revision"))),
            Alias::new("children_index_revision"),
        )
        .from_as(Alias::new("storage_roots"), root.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            root_object.clone(),
            Cond::all()
                .add(
                    Expr::col((root_object.clone(), Alias::new("storage_root_id")))
                        .equals((root.clone(), Alias::new("id"))),
                )
                .add(
                    Expr::col((root_object.clone(), Alias::new("parent_storage_object_id")))
                        .is_null(),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            child.clone(),
            Expr::col((child.clone(), Alias::new("storage_root_id")))
                .equals((root.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_objects"),
            object.clone(),
            Expr::col((object.clone(), Alias::new("id")))
                .equals((child.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            binding.clone(),
            Expr::col((binding.clone(), Alias::new("storage_root_id")))
                .equals((root.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((binding.clone(), Alias::new("library_id"))),
        )
        .and_where(Expr::col((root.clone(), Alias::new("id"))).eq(root_id.as_uuid()))
        .and_where(Expr::col((root.clone(), Alias::new("reconciled_sync_revision"))).eq(revision))
        .and_where(Expr::col((binding, Alias::new("discovered_sync_revision"))).lt(revision))
        .and_where(Expr::col((library.clone(), Alias::new("is_enabled"))).eq(true))
        .and_where(
            Expr::col((library.clone(), Alias::new("id"))).is_in(library_ids.iter().copied()),
        )
        .cond_where(
            Cond::any()
                .add(
                    Cond::all()
                        .add(
                            Expr::col((library.clone(), Alias::new("collection_type"))).eq("music"),
                        )
                        .add(Expr::col((object.clone(), Alias::new("object_type"))).eq("File"))
                        .add(supported_audio_name_condition(&object)),
                )
                .add(
                    Cond::all()
                        .add(Expr::col((library, Alias::new("collection_type"))).ne("music"))
                        .add(
                            Expr::col((child.clone(), Alias::new("parent_storage_object_id")))
                                .equals((root_object, Alias::new("storage_object_id"))),
                        ),
                ),
        )
        .and_where(Expr::col((child.clone(), Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((child, Alias::new("observed_sync_revision"))).lte(revision))
        .and_where(Expr::col((object.clone(), Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((object, Alias::new("observed_sync_revision"))).lte(revision))
        .limit(MAX_TITLES + 1)
        .to_owned()
}

fn supported_audio_name_condition(object: &Alias) -> sea_orm::sea_query::Condition {
    [
        "aac", "flac", "m4a", "mp3", "oga", "ogg", "opus", "wav", "wave", "webm",
    ]
    .into_iter()
    .fold(Cond::any(), |condition, extension| {
        condition.add(
            Expr::col((object.clone(), Alias::new("normalized_name")))
                .like(format!("%.{extension}")),
        )
    })
}

fn parse_title(value: &str) -> Result<(String, Option<i32>), DiscoverTitlesError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 512 {
        return Err(DiscoverTitlesError::InvalidTitle);
    }
    if let Some((name, year)) = crate::title_year::split_title_year(trimmed) {
        return Ok((name.to_owned(), Some(year)));
    }
    Ok((trimmed.to_owned(), None))
}

fn parse_audio_title(value: &str) -> Option<String> {
    let path = std::path::Path::new(value);
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "aac" | "flac" | "m4a" | "mp3" | "oga" | "ogg" | "opus" | "wav" | "wave" | "webm"
    ) {
        return None;
    }
    let title = path.file_stem()?.to_str()?.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

fn derived_item(object: StorageObjectRecordId) -> CatalogItemId {
    CatalogItemId::from_uuid(Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("tjxy-title:{}", object.as_uuid()).as_bytes(),
    ))
}

fn derived_identity(object: StorageObjectRecordId) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("tjxy-title-identity:{}", object.as_uuid()).as_bytes(),
    )
}

#[derive(Debug, Error)]
pub enum DiscoverTitlesError {
    #[error("claimed work is not title discovery for a storage root")]
    InvalidClaim,
    #[error("title discovery root revision changed or was already published")]
    StaleRoot,
    #[error("title discovery storage facts are ambiguous or not yet reconciled")]
    StorageInputPending,
    #[error("title discovery is already current for this root")]
    AlreadyCurrent,
    #[error("title discovery job has no persisted Library scope")]
    MissingLibraryScope,
    #[error("title discovery job has a corrupt persisted Library scope")]
    InvalidLibraryScope,
    #[error("title discovery Library policy changed after scheduling")]
    StaleLibraryPolicy,
    #[error("title discovery exceeded its bounded title count")]
    TitleLimit,
    #[error("library collection type is not supported for title discovery")]
    UnsupportedCollection,
    #[error("library metadata policy is invalid")]
    InvalidMetadataPolicy,
    #[error("library metadata source mode is invalid")]
    InvalidMetadataSourceMode,
    #[error("title-layer object has an invalid name")]
    InvalidTitle,
    #[error("title identity conflicts with an existing catalog item")]
    IdentityConflict,
    #[error("title discovery database operation failed: {0}")]
    Database(#[from] DbErr),
    #[error("title discovery catalog generation failed: {0}")]
    Publication(#[from] CatalogPublicationError),
    #[error("title discovery work operation failed: {0}")]
    Work(#[from] WorkJobRepositoryError),
}
