use std::collections::HashMap;

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbErr,
    sea_query::{Alias, Cond, Expr, JoinType, Query},
};
use thiserror::Error;
use tjxy_common::{
    CatalogItemId, MediaLocationId, MediaSourceId, PresentationKey, StorageObjectRecordId,
    StorageRootId, SubtitleId,
};
use uuid::Uuid;

use crate::{ClaimedWorkJob, WorkScope, WorkTaskKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceIndexObject {
    id: StorageObjectRecordId,
    name: String,
    checksum: Option<String>,
    stable_source: Option<(MediaSourceId, PresentationKey, MediaLocationId)>,
    stable_subtitle: Option<(SubtitleId, MediaSourceId)>,
}

impl SourceIndexObject {
    #[must_use]
    pub const fn id(&self) -> StorageObjectRecordId {
        self.id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn checksum(&self) -> Option<&str> {
        self.checksum.as_deref()
    }
    #[must_use]
    pub const fn stable_source(&self) -> Option<(MediaSourceId, PresentationKey, MediaLocationId)> {
        self.stable_source
    }
    #[must_use]
    pub const fn stable_subtitle(&self) -> Option<(SubtitleId, MediaSourceId)> {
        self.stable_subtitle
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceIndexSnapshot {
    owner: CatalogItemId,
    objects: Vec<SourceIndexObject>,
    restrict_to_stable_sources: bool,
}

impl SourceIndexSnapshot {
    #[must_use]
    pub const fn owner(&self) -> CatalogItemId {
        self.owner
    }
    #[must_use]
    pub fn objects(&self) -> &[SourceIndexObject] {
        &self.objects
    }

    #[must_use]
    pub const fn restrict_to_stable_sources(&self) -> bool {
        self.restrict_to_stable_sources
    }
}

pub struct SourceIndexRepository<'connection> {
    database: &'connection DatabaseConnection,
}

impl<'connection> SourceIndexRepository<'connection> {
    #[must_use]
    pub const fn new(database: &'connection DatabaseConnection) -> Self {
        Self { database }
    }

    /// Reads one fenced source-index input exclusively from reconciled SQL inventory.
    ///
    /// # Errors
    ///
    /// Returns [`SourceIndexRepositoryError`] for invalid work, missing/ambiguous scope, or SQL corruption.
    pub async fn snapshot(
        &self,
        claimed: &ClaimedWorkJob,
    ) -> Result<SourceIndexSnapshot, SourceIndexRepositoryError> {
        let WorkScope::CatalogItem(owner) = claimed.job().scope() else {
            return Err(SourceIndexRepositoryError::InvalidClaim);
        };
        if claimed.job().task_kind() != WorkTaskKind::IndexMediaSources {
            return Err(SourceIndexRepositoryError::InvalidClaim);
        }
        let revision = claimed
            .job()
            .input_sync_revision()
            .ok_or(SourceIndexRepositoryError::MissingSyncRevision)?;
        let scope = crate::catalog_storage_scope::resolve_catalog_storage_scope(
            self.database,
            owner,
            claimed.job().storage_root_affinity(),
        )
        .await
        .map_err(|error| match error {
            crate::catalog_storage_scope::CatalogStorageScopeError::Ambiguous => {
                SourceIndexRepositoryError::MissingScope
            }
            crate::catalog_storage_scope::CatalogStorageScopeError::Database(error) => {
                SourceIndexRepositoryError::Database(error)
            }
        })?
        .ok_or(SourceIndexRepositoryError::MissingScope)?;
        if !scope.children_indexed()
            || !scope.accepts_metadata_input(revision)
            || !crate::catalog_storage_scope::storage_scope_is_reconciled(
                self.database,
                scope,
                true,
            )
            .await?
        {
            return Err(SourceIndexRepositoryError::StorageInputPending);
        }
        let mut rows = self
            .database
            .query_all(self.database.get_database_backend().build(&candidate_query(
                owner,
                revision,
                claimed.job().storage_root_affinity(),
            )))
            .await?;
        let mut restrict_to_stable_sources = false;
        if rows.is_empty()
            && let Some(publication_id) =
                crate::source_publication::effective_source_publication(self.database, owner)
                    .await?
        {
            rows =
                self.database
                    .query_all(self.database.get_database_backend().build(
                        &projected_candidate_query(
                            owner,
                            publication_id,
                            revision,
                            claimed.job().storage_root_affinity(),
                        ),
                    ))
                    .await?;
            restrict_to_stable_sources = true;
        }
        if rows.is_empty() {
            return Err(SourceIndexRepositoryError::MissingScope);
        }
        let mut objects = rows
            .iter()
            .map(|row| {
                Ok(SourceIndexObject {
                    id: StorageObjectRecordId::from_uuid(row.try_get("", "storage_object_id")?),
                    name: row.try_get("", "name")?,
                    checksum: row.try_get("", "checksum")?,
                    stable_source: None,
                    stable_subtitle: None,
                })
            })
            .collect::<Result<Vec<_>, DbErr>>()?;
        let ids = objects
            .iter()
            .map(|object| object.id.as_uuid())
            .collect::<Vec<_>>();
        let sources = stable_sources(self.database, owner, &ids).await?;
        let subtitles = stable_subtitles(self.database, owner, &ids).await?;
        for object in &mut objects {
            object.stable_source = sources.get(&object.id.as_uuid()).copied();
            object.stable_subtitle = subtitles.get(&object.id.as_uuid()).copied();
        }
        Ok(SourceIndexSnapshot {
            owner,
            objects,
            restrict_to_stable_sources,
        })
    }
}

#[derive(Debug, Error)]
pub enum SourceIndexRepositoryError {
    #[error("claimed work is not a source-index job")]
    InvalidClaim,
    #[error("source-index job does not record its synchronized input revision")]
    MissingSyncRevision,
    #[error("matched source directory is missing, ambiguous, or not reconciled")]
    MissingScope,
    #[error("source-index storage input contains facts not yet reconciled")]
    StorageInputPending,
    #[error("source-index publication lookup failed: {0}")]
    Publication(#[from] crate::CatalogPublicationError),
    #[error("source-index SQL query failed: {0}")]
    Database(#[from] DbErr),
}

#[allow(clippy::too_many_lines)] // Keeps the complete direct root authorization and inventory fence in one query.
fn candidate_query(
    owner: CatalogItemId,
    revision: i64,
    storage_root: Option<StorageRootId>,
) -> sea_orm::sea_query::SelectStatement {
    let identity = Alias::new("source_index_identity");
    let parent = Alias::new("source_index_parent");
    let child = Alias::new("source_index_child");
    let object = Alias::new("source_index_object");
    let root = Alias::new("source_index_root");
    let library_root = Alias::new("source_index_library_root");
    let membership = Alias::new("source_index_membership");
    let library = Alias::new("source_index_library");
    Query::select()
        .distinct()
        .expr_as(
            Expr::col((object.clone(), Alias::new("id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("checksum"))),
            Alias::new("checksum"),
        )
        .from_as(Alias::new("identity_matches"), identity.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            parent.clone(),
            Expr::col((parent.clone(), Alias::new("storage_object_id")))
                .equals((identity.clone(), Alias::new("storage_object_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            child.clone(),
            Cond::all()
                .add(
                    Expr::col((child.clone(), Alias::new("storage_root_id")))
                        .equals((parent.clone(), Alias::new("storage_root_id"))),
                )
                .add(
                    Expr::col((child.clone(), Alias::new("parent_storage_object_id")))
                        .equals((parent.clone(), Alias::new("storage_object_id"))),
                ),
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
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("id")))
                .equals((parent.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            library_root.clone(),
            Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                .equals((root.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Cond::all()
                .add(
                    Expr::col((membership.clone(), Alias::new("library_id")))
                        .equals((library_root, Alias::new("library_id"))),
                )
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .eq(owner.as_uuid()),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(
            Expr::col((identity.clone(), Alias::new("candidate_catalog_item_id")))
                .eq(owner.as_uuid()),
        )
        .and_where(Expr::col((identity, Alias::new("state"))).eq("Matched"))
        .cond_where(storage_root.map_or_else(Cond::all, |storage_root| {
            Cond::all().add(
                Expr::col((parent.clone(), Alias::new("storage_root_id")))
                    .eq(storage_root.as_uuid()),
            )
        }))
        .and_where(Expr::col((parent.clone(), Alias::new("children_indexed"))).eq(true))
        .and_where(Expr::col((parent, Alias::new("children_index_revision"))).gte(revision))
        .and_where(Expr::col((child, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((object.clone(), Alias::new("object_type"))).eq("File"))
        .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((root, Alias::new("reconciled_sync_revision"))).gte(revision))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .to_owned()
}

#[allow(clippy::too_many_lines)] // The fallback must fence publication, lineage, root, and inventory in one query.
fn projected_candidate_query(
    owner: CatalogItemId,
    publication_id: Uuid,
    revision: i64,
    storage_root: Option<StorageRootId>,
) -> sea_orm::sea_query::SelectStatement {
    let item = Alias::new("projected_source_item");
    let source = Alias::new("projected_source_seed");
    let location = Alias::new("projected_source_location");
    let seed = Alias::new("projected_source_object");
    let parent = Alias::new("projected_source_parent");
    let child = Alias::new("projected_source_child");
    let object = Alias::new("projected_source_candidate");
    let root = Alias::new("projected_source_root");
    let library_root = Alias::new("projected_source_library_root");
    let membership = Alias::new("projected_source_membership");
    let library = Alias::new("projected_source_library");
    Query::select()
        .distinct()
        .expr_as(
            Expr::col((object.clone(), Alias::new("id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("name"))),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((object.clone(), Alias::new("checksum"))),
            Alias::new("checksum"),
        )
        .from_as(Alias::new("catalog_items"), item.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("publication_media_sources"),
            source.clone(),
            Cond::all()
                .add(Expr::col((source.clone(), Alias::new("publication_id"))).eq(publication_id))
                .add(
                    Expr::col((source.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("publication_media_locations"),
            location.clone(),
            Cond::all()
                .add(
                    Expr::col((location.clone(), Alias::new("publication_id")))
                        .equals((source.clone(), Alias::new("publication_id"))),
                )
                .add(
                    Expr::col((location.clone(), Alias::new("media_source_id")))
                        .equals((source, Alias::new("media_source_id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            seed.clone(),
            Expr::col((seed.clone(), Alias::new("storage_object_id")))
                .equals((location, Alias::new("storage_object_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            parent.clone(),
            Cond::all()
                .add(
                    Expr::col((parent.clone(), Alias::new("storage_root_id")))
                        .equals((seed.clone(), Alias::new("storage_root_id"))),
                )
                .add(
                    Expr::col((parent.clone(), Alias::new("storage_object_id")))
                        .equals((seed, Alias::new("parent_storage_object_id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("storage_root_objects"),
            child.clone(),
            Cond::all()
                .add(
                    Expr::col((child.clone(), Alias::new("storage_root_id")))
                        .equals((parent.clone(), Alias::new("storage_root_id"))),
                )
                .add(
                    Expr::col((child.clone(), Alias::new("parent_storage_object_id")))
                        .equals((parent.clone(), Alias::new("storage_object_id"))),
                ),
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
            Alias::new("storage_roots"),
            root.clone(),
            Expr::col((root.clone(), Alias::new("id")))
                .equals((parent.clone(), Alias::new("storage_root_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_storage_roots"),
            library_root.clone(),
            Expr::col((library_root.clone(), Alias::new("storage_root_id")))
                .equals((root.clone(), Alias::new("id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("library_catalog_items"),
            membership.clone(),
            Cond::all()
                .add(
                    Expr::col((membership.clone(), Alias::new("library_id")))
                        .equals((library_root, Alias::new("library_id"))),
                )
                .add(
                    Expr::col((membership.clone(), Alias::new("catalog_item_id")))
                        .equals((item.clone(), Alias::new("structure_owner_item_id"))),
                ),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("libraries"),
            library.clone(),
            Expr::col((library.clone(), Alias::new("id")))
                .equals((membership, Alias::new("library_id"))),
        )
        .and_where(Expr::col((item.clone(), Alias::new("id"))).eq(owner.as_uuid()))
        .and_where(Expr::col((item, Alias::new("structure_owner_item_id"))).is_not_null())
        .cond_where(storage_root.map_or_else(Cond::all, |storage_root| {
            Cond::all().add(
                Expr::col((parent.clone(), Alias::new("storage_root_id")))
                    .eq(storage_root.as_uuid()),
            )
        }))
        .and_where(Expr::col((parent.clone(), Alias::new("children_indexed"))).eq(true))
        .and_where(Expr::col((parent, Alias::new("children_index_revision"))).gte(revision))
        .and_where(Expr::col((child, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((object.clone(), Alias::new("object_type"))).eq("File"))
        .and_where(Expr::col((object, Alias::new("presence_state"))).eq("Present"))
        .and_where(Expr::col((root, Alias::new("reconciled_sync_revision"))).gte(revision))
        .and_where(Expr::col((library, Alias::new("is_enabled"))).eq(true))
        .and_where(Expr::exists(
            crate::source_publication::effective_source_publication_visible(owner, publication_id),
        ))
        .to_owned()
}

async fn stable_sources(
    database: &DatabaseConnection,
    owner: CatalogItemId,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, (MediaSourceId, PresentationKey, MediaLocationId)>, DbErr> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let location = Alias::new("stable_location");
    let source = Alias::new("stable_source");
    let query = Query::select()
        .expr_as(
            Expr::col((location.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((location.clone(), Alias::new("id"))),
            Alias::new("location_id"),
        )
        .expr_as(
            Expr::col((source.clone(), Alias::new("id"))),
            Alias::new("source_id"),
        )
        .expr_as(
            Expr::col((source.clone(), Alias::new("presentation_key"))),
            Alias::new("presentation_key"),
        )
        .from_as(Alias::new("media_locations"), location.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("media_sources"),
            source.clone(),
            Expr::col((source.clone(), Alias::new("id")))
                .equals((location.clone(), Alias::new("media_source_id"))),
        )
        .and_where(Expr::col((source, Alias::new("catalog_item_id"))).eq(owner.as_uuid()))
        .and_where(
            Expr::col((location, Alias::new("storage_object_id"))).is_in(ids.iter().copied()),
        )
        .to_owned();
    let mut stable = database
        .query_all(database.get_database_backend().build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok((
                row.try_get("", "storage_object_id")?,
                (
                    MediaSourceId::from_uuid(row.try_get("", "source_id")?),
                    PresentationKey::from_uuid(row.try_get("", "presentation_key")?),
                    MediaLocationId::from_uuid(row.try_get("", "location_id")?),
                ),
            ))
        })
        .collect::<Result<HashMap<_, _>, DbErr>>()?;
    merge_confirmed_relink_aliases(database, owner, ids, &mut stable).await?;
    Ok(stable)
}

async fn merge_confirmed_relink_aliases(
    database: &DatabaseConnection,
    owner: CatalogItemId,
    ids: &[Uuid],
    stable: &mut HashMap<Uuid, (MediaSourceId, PresentationKey, MediaLocationId)>,
) -> Result<(), DbErr> {
    let candidate = Alias::new("stable_relink_candidate");
    let previous_location = Alias::new("stable_relink_location");
    let previous_source = Alias::new("stable_relink_source");
    let alias_query = Query::select()
        .expr_as(
            Expr::col((
                candidate.clone(),
                Alias::new("replacement_storage_object_id"),
            )),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((previous_location.clone(), Alias::new("id"))),
            Alias::new("location_id"),
        )
        .expr_as(
            Expr::col((previous_source.clone(), Alias::new("id"))),
            Alias::new("source_id"),
        )
        .expr_as(
            Expr::col((previous_source.clone(), Alias::new("presentation_key"))),
            Alias::new("presentation_key"),
        )
        .from_as(Alias::new("storage_relink_candidates"), candidate.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("media_locations"),
            previous_location.clone(),
            Expr::col((previous_location.clone(), Alias::new("storage_object_id")))
                .equals((candidate.clone(), Alias::new("previous_storage_object_id"))),
        )
        .join_as(
            JoinType::InnerJoin,
            Alias::new("media_sources"),
            previous_source.clone(),
            Expr::col((previous_source.clone(), Alias::new("id")))
                .equals((previous_location.clone(), Alias::new("media_source_id"))),
        )
        .and_where(Expr::col((candidate.clone(), Alias::new("state"))).eq("Confirmed"))
        .and_where(
            Expr::col((candidate, Alias::new("replacement_storage_object_id")))
                .is_in(ids.iter().copied()),
        )
        .and_where(Expr::col((previous_source, Alias::new("catalog_item_id"))).eq(owner.as_uuid()))
        .to_owned();
    for row in database
        .query_all(database.get_database_backend().build(&alias_query))
        .await?
    {
        let object_id: Uuid = row.try_get("", "storage_object_id")?;
        let previous_location_id: Uuid = row.try_get("", "location_id")?;
        let alias = (
            MediaSourceId::from_uuid(row.try_get("", "source_id")?),
            PresentationKey::from_uuid(row.try_get("", "presentation_key")?),
            MediaLocationId::from_uuid(Uuid::new_v5(&previous_location_id, object_id.as_bytes())),
        );
        if stable
            .get(&object_id)
            .is_some_and(|existing| *existing != alias)
        {
            return Err(DbErr::Custom(
                "confirmed storage relink aliases resolve to conflicting media sources".to_owned(),
            ));
        }
        stable.insert(object_id, alias);
    }
    Ok(())
}

async fn stable_subtitles(
    database: &DatabaseConnection,
    owner: CatalogItemId,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, (SubtitleId, MediaSourceId)>, DbErr> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let subtitle = Alias::new("stable_subtitle");
    let source = Alias::new("subtitle_source");
    let query = Query::select()
        .expr_as(
            Expr::col((subtitle.clone(), Alias::new("storage_object_id"))),
            Alias::new("storage_object_id"),
        )
        .expr_as(
            Expr::col((subtitle.clone(), Alias::new("id"))),
            Alias::new("subtitle_id"),
        )
        .expr_as(
            Expr::col((subtitle.clone(), Alias::new("media_source_id"))),
            Alias::new("media_source_id"),
        )
        .from_as(Alias::new("subtitles"), subtitle.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("media_sources"),
            source.clone(),
            Expr::col((source.clone(), Alias::new("id")))
                .equals((subtitle.clone(), Alias::new("media_source_id"))),
        )
        .and_where(Expr::col((source, Alias::new("catalog_item_id"))).eq(owner.as_uuid()))
        .and_where(
            Expr::col((subtitle, Alias::new("storage_object_id"))).is_in(ids.iter().copied()),
        )
        .to_owned();
    database
        .query_all(database.get_database_backend().build(&query))
        .await?
        .iter()
        .map(|row| {
            Ok((
                row.try_get("", "storage_object_id")?,
                (
                    SubtitleId::from_uuid(row.try_get("", "subtitle_id")?),
                    MediaSourceId::from_uuid(row.try_get("", "media_source_id")?),
                ),
            ))
        })
        .collect()
}
