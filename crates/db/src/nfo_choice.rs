use chrono::Utc;
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbErr, TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Order, Query},
};
use serde::{Deserialize, Serialize};
use tjxy_common::{CatalogItemId, StorageRootId};
use uuid::Uuid;

use crate::MetadataWorkRepository;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct NfoCandidateInfo {
    pub id: Uuid,
    pub name: String,
    pub digest: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct NfoChoiceInfo {
    pub item_id: Uuid,
    pub root_id: Uuid,
    pub fingerprint: String,
    pub candidates: Vec<NfoCandidateInfo>,
    pub conflict_fields: Vec<String>,
    pub selected_object_id: Option<Uuid>,
    pub status: String,
}

pub(crate) async fn saved_selection(
    connection: &impl ConnectionTrait,
    item: CatalogItemId,
    root: StorageRootId,
    fingerprint: &str,
) -> Result<Option<(Uuid, String)>, DbErr> {
    let row = connection
        .query_one(
            connection.get_database_backend().build(
                Query::select()
                    .columns([
                        Alias::new("selected_object_id"),
                        Alias::new("selected_digest"),
                    ])
                    .from(Alias::new("nfo_choices"))
                    .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item.as_uuid()))
                    .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                    .and_where(Expr::col(Alias::new("fingerprint")).eq(fingerprint))
                    .and_where(Expr::col(Alias::new("selected_object_id")).is_not_null()),
            ),
        )
        .await?;
    row.map(|row| {
        Ok((
            row.try_get("", "selected_object_id")?,
            row.try_get("", "selected_digest")?,
        ))
    })
    .transpose()
}

#[allow(clippy::too_many_arguments)] // Persist the claim revisions alongside the candidate snapshot atomically.
pub(crate) async fn record_conflict(
    transaction: &DatabaseTransaction,
    item: CatalogItemId,
    root: StorageRootId,
    fingerprint: &str,
    candidates: &[NfoCandidateInfo],
    fields: &[String],
    metadata_revision: i64,
    input_sync_revision: i64,
) -> Result<(), DbErr> {
    let candidates = serde_json::to_value(candidates)
        .map_err(|_| DbErr::Custom("invalid NFO candidate diagnostics".to_owned()))?;
    let fields = serde_json::to_value(fields)
        .map_err(|_| DbErr::Custom("invalid NFO conflict diagnostics".to_owned()))?;
    let query = Query::insert()
        .into_table(Alias::new("nfo_choices"))
        .columns([
            Alias::new("catalog_item_id"),
            Alias::new("storage_root_id"),
            Alias::new("fingerprint"),
            Alias::new("metadata_revision"),
            Alias::new("input_sync_revision"),
            Alias::new("candidates"),
            Alias::new("conflict_fields"),
            Alias::new("selected_object_id"),
            Alias::new("selected_digest"),
            Alias::new("status"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            item.as_uuid().into(),
            root.as_uuid().into(),
            fingerprint.into(),
            metadata_revision.into(),
            input_sync_revision.into(),
            candidates.into(),
            fields.into(),
            Option::<Uuid>::None.into(),
            Option::<String>::None.into(),
            "NeedsSelection".into(),
            Utc::now().into(),
        ])
        .on_conflict(
            OnConflict::columns([Alias::new("catalog_item_id"), Alias::new("storage_root_id")])
                .update_columns([
                    Alias::new("fingerprint"),
                    Alias::new("metadata_revision"),
                    Alias::new("input_sync_revision"),
                    Alias::new("candidates"),
                    Alias::new("conflict_fields"),
                    Alias::new("selected_object_id"),
                    Alias::new("selected_digest"),
                    Alias::new("status"),
                    Alias::new("updated_at"),
                ])
                .to_owned(),
        )
        .to_owned();
    transaction
        .execute(transaction.get_database_backend().build(&query))
        .await?;
    Ok(())
}

impl MetadataWorkRepository<'_> {
    /// Returns a bounded page of current NFO choices for administrator diagnostics.
    ///
    /// # Errors
    /// Returns database or invalid-payload failures.
    pub async fn nfo_choices(&self, offset: u64) -> Result<Vec<NfoChoiceInfo>, DbErr> {
        let database = self.connection();
        let rows = database
            .query_all(
                database.get_database_backend().build(
                    Query::select()
                        .columns([
                            Alias::new("catalog_item_id"),
                            Alias::new("storage_root_id"),
                            Alias::new("fingerprint"),
                            Alias::new("metadata_revision"),
                            Alias::new("input_sync_revision"),
                            Alias::new("candidates"),
                            Alias::new("conflict_fields"),
                            Alias::new("selected_object_id"),
                            Alias::new("status"),
                        ])
                        .from(Alias::new("nfo_choices"))
                        .and_where(Expr::col(Alias::new("status")).ne("Resolved"))
                        .order_by(Alias::new("updated_at"), Order::Desc)
                        .order_by(Alias::new("catalog_item_id"), Order::Asc)
                        .offset(offset)
                        .limit(50),
                ),
            )
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(NfoChoiceInfo {
                    item_id: row.try_get("", "catalog_item_id")?,
                    root_id: row.try_get("", "storage_root_id")?,
                    fingerprint: row.try_get("", "fingerprint")?,
                    candidates: serde_json::from_value(row.try_get("", "candidates")?)
                        .map_err(|_| DbErr::Custom("invalid NFO candidates".to_owned()))?,
                    conflict_fields: serde_json::from_value(row.try_get("", "conflict_fields")?)
                        .map_err(|_| DbErr::Custom("invalid NFO fields".to_owned()))?,
                    selected_object_id: row.try_get("", "selected_object_id")?,
                    status: row.try_get("", "status")?,
                })
            })
            .collect()
    }

    /// Saves a choice and schedules resolution in the same transaction.
    /// Publication rechecks both inventory identity and selected content digest.
    ///
    /// # Errors
    /// Returns stale-choice or database failures.
    pub async fn choose_nfo(
        &self,
        item: CatalogItemId,
        root: StorageRootId,
        candidate: Uuid,
        fingerprint: &str,
    ) -> Result<crate::WorkJobSubmission, DbErr> {
        let database = self.connection();
        let transaction = database.begin().await?;
        let backend = database.get_database_backend();
        transaction
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("catalog_items"))
                        .value(
                            Alias::new("metadata_revision"),
                            Expr::col(Alias::new("metadata_revision")),
                        )
                        .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
                ),
            )
            .await?;
        let row = transaction
            .query_one(
                backend.build(
                    Query::select()
                        .columns([
                            Alias::new("candidates"),
                            Alias::new("selected_object_id"),
                            Alias::new("selected_digest"),
                        ])
                        .from(Alias::new("nfo_choices"))
                        .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item.as_uuid()))
                        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                        .and_where(Expr::col(Alias::new("fingerprint")).eq(fingerprint)),
                ),
            )
            .await?
            .ok_or_else(|| DbErr::Custom("NFO choice is stale".to_owned()))?;
        let candidates: Vec<NfoCandidateInfo> =
            serde_json::from_value(row.try_get("", "candidates")?)
                .map_err(|_| DbErr::Custom("invalid NFO choices".to_owned()))?;
        let selected = candidates
            .iter()
            .find(|entry| entry.id == candidate)
            .ok_or_else(|| DbErr::Custom("NFO candidate is unavailable".to_owned()))?;
        if selected.digest.len() != 64 {
            return Err(DbErr::Custom("NFO candidate was not fully read".to_owned()));
        }
        let same_choice = row.try_get::<Option<Uuid>>("", "selected_object_id")? == Some(candidate)
            && row
                .try_get::<Option<String>>("", "selected_digest")?
                .as_deref()
                == Some(&selected.digest);
        let updated = transaction
            .execute(
                backend.build(
                    Query::update()
                        .table(Alias::new("nfo_choices"))
                        .value(Alias::new("selected_object_id"), candidate)
                        .value(Alias::new("selected_digest"), &selected.digest)
                        .value(Alias::new("status"), "Selected")
                        .value(Alias::new("updated_at"), Utc::now())
                        .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item.as_uuid()))
                        .and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()))
                        .and_where(Expr::col(Alias::new("fingerprint")).eq(fingerprint)),
                ),
            )
            .await?;
        if updated.rows_affected() != 1 {
            return Err(DbErr::Custom("NFO choice is stale".to_owned()));
        }
        if !same_choice {
            transaction
                .execute(
                    backend.build(
                        Query::update()
                            .table(Alias::new("catalog_items"))
                            .value(
                                Alias::new("metadata_revision"),
                                Expr::col(Alias::new("metadata_revision")).add(1_i64),
                            )
                            .and_where(Expr::col(Alias::new("id")).eq(item.as_uuid())),
                    ),
                )
                .await?;
        }
        let submission = enqueue_selection_resolution(&transaction, item, root).await?;
        crate::work_queue::commit_and_notify(transaction).await?;
        Ok(submission)
    }
}

async fn enqueue_selection_resolution(
    transaction: &DatabaseTransaction,
    item: CatalogItemId,
    root: StorageRootId,
) -> Result<crate::WorkJobSubmission, DbErr> {
    let backend = transaction.get_database_backend();
    let scope = crate::metadata_work::metadata_storage_scope(transaction, item, Some(root))
        .await
        .map_err(|_| DbErr::Custom("NFO storage scope is unavailable".into()))?;
    let policy = crate::source_publication::metadata_policy_for_item(transaction, item, true)
        .await
        .map_err(|_| DbErr::Custom("NFO metadata policy is unavailable".into()))?
        .ok_or_else(|| DbErr::Custom("NFO metadata policy is unavailable".into()))?;
    let revision = transaction
        .query_one(backend.build(&crate::metadata_work::metadata_schedule_query(item)))
        .await?
        .ok_or_else(|| DbErr::Custom("NFO item is unavailable".into()))?
        .try_get::<i64>("", "metadata_revision")?;
    let spec = crate::WorkJobSpec::new(
        crate::WorkTaskKind::ResolveMetadata,
        crate::WorkScope::CatalogItem(item),
        revision,
        20,
    )
    .and_then(|spec| spec.with_metadata_requirement(crate::MetadataRequirement::Full))
    .and_then(|spec| spec.with_metadata_source_mode(policy.source_mode))
    .and_then(|spec| spec.with_local_metadata_access_mode(policy.access_mode))
    .and_then(|spec| spec.with_storage_root_affinity(root))
    .and_then(|spec| spec.with_input_sync_revision(scope.metadata_input_revision()))
    .map_err(|_| DbErr::Custom("NFO work specification is unavailable".into()))?;
    let submission = crate::work_job::enqueue_in_transaction(transaction, &spec, Utc::now())
        .await
        .map_err(|_| DbErr::Custom("NFO resolution could not be enqueued".into()))?;
    Ok(submission)
}

pub(crate) async fn awaiting_selection(
    connection: &impl ConnectionTrait,
    spec: &crate::WorkJobSpec,
    now: chrono::DateTime<Utc>,
) -> Result<bool, DbErr> {
    if spec.task_kind() != crate::WorkTaskKind::ResolveMetadata {
        return Ok(false);
    }
    let crate::WorkScope::CatalogItem(item) = spec.scope() else {
        return Ok(false);
    };
    let Some(input_revision) = spec.input_sync_revision() else {
        return Ok(false);
    };
    let mut query = Query::select();
    query
        .expr(Expr::val(1_i32))
        .from(Alias::new("nfo_choices"))
        .and_where(Expr::col(Alias::new("catalog_item_id")).eq(item.as_uuid()))
        .and_where(Expr::col(Alias::new("metadata_revision")).eq(spec.expected_revision()))
        .and_where(Expr::col(Alias::new("input_sync_revision")).eq(input_revision))
        .and_where(Expr::col(Alias::new("status")).eq("NeedsSelection"))
        .and_where(Expr::col(Alias::new("updated_at")).gt(now - chrono::Duration::minutes(5)))
        .limit(1);
    if let Some(root) = spec.storage_root_affinity() {
        query.and_where(Expr::col(Alias::new("storage_root_id")).eq(root.as_uuid()));
    }
    Ok(connection
        .query_one(connection.get_database_backend().build(&query))
        .await?
        .is_some())
}
