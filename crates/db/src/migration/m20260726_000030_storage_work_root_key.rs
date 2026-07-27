use std::collections::HashSet;

use chrono::Utc;
use sea_orm::{ConnectionTrait, DbBackend, QueryResult, TransactionTrait};
use sea_orm_migration::prelude::{
    Alias, ConditionalStatement, DbErr, DeriveMigrationName, Expr, Index, MigrationTrait, Query,
    SchemaManager, Table,
};
use uuid::Uuid;

const TABLE: &str = "work_jobs";
const COLUMN: &str = "natural_key_storage_root_id";
const AFFINITY: &str = "storage_root_affinity";
const INDEX: &str = "uq_work_jobs_active";
const ACTIVE_STATES: [&str; 2] = ["Pending", "Running"];
const STORAGE_TASKS: [&str; 3] = [
    "ScopedStorageSync",
    "RecoverStorageCursor",
    "ValidateStorageRoot",
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_active_index(manager).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .add_column(super::uuid_with_nil_default(
                        manager.get_connection().get_database_backend(),
                        Alias::new(COLUMN),
                    ))
                    .to_owned(),
            )
            .await?;
        normalize_work_root_columns(manager).await?;
        backfill_storage_roots(manager).await?;
        create_active_index(manager, true).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_active_index(manager).await?;
        retire_down_migration_conflicts(manager).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new(TABLE))
                    .drop_column(Alias::new(COLUMN))
                    .to_owned(),
            )
            .await?;
        create_active_index(manager, false).await
    }
}

async fn normalize_work_root_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let transaction = connection.begin().await?;
    let query = Query::select()
        .columns([Alias::new("id"), Alias::new(AFFINITY)])
        .from(Alias::new(TABLE))
        .to_owned();
    let backend = transaction.get_database_backend();
    for row in transaction.query_all(backend.build(&query)).await? {
        let update = Query::update()
            .table(Alias::new(TABLE))
            .value(Alias::new(AFFINITY), migration_uuid(&row, AFFINITY)?)
            .value(Alias::new(COLUMN), Uuid::nil())
            .and_where(Expr::col(Alias::new("id")).eq(migration_uuid(&row, "id")?))
            .to_owned();
        transaction.execute(backend.build(&update)).await?;
    }
    transaction.commit().await
}

async fn backfill_storage_roots(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let transaction = connection.begin().await?;
    let query = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("task_kind"),
            Alias::new("scope_type"),
            Alias::new("scope_id"),
            Alias::new(AFFINITY),
            Alias::new("state"),
        ])
        .from(Alias::new(TABLE))
        .and_where(Expr::col(Alias::new("task_kind")).is_in(STORAGE_TASKS))
        .to_owned();
    let backend = transaction.get_database_backend();
    for row in transaction.query_all(backend.build(&query)).await? {
        let affinity = migration_affinity(&transaction, &row).await?;
        if let Some(root_id) = affinity {
            let update = Query::update()
                .table(Alias::new(TABLE))
                .value(Alias::new(AFFINITY), root_id)
                .value(Alias::new(COLUMN), root_id)
                .and_where(Expr::col(Alias::new("id")).eq(migration_uuid(&row, "id")?))
                .to_owned();
            transaction.execute(backend.build(&update)).await?;
        } else if ACTIVE_STATES.contains(&row.try_get::<String>("", "state")?.as_str()) {
            retire_job(
                &transaction,
                migration_uuid(&row, "id")?,
                "storage root affinity could not be recovered during migration",
            )
            .await?;
        }
    }
    transaction.commit().await
}

async fn migration_affinity(
    connection: &impl ConnectionTrait,
    row: &QueryResult,
) -> Result<Option<Uuid>, DbErr> {
    let stored = migration_uuid(row, AFFINITY)?;
    if !stored.is_nil() {
        return Ok(Some(stored));
    }
    let scope_type: String = row.try_get("", "scope_type")?;
    let scope_id = migration_uuid(row, "scope_id")?;
    if scope_type == "StorageRoot" {
        return Ok(Some(scope_id));
    }
    if scope_type != "StorageObject" {
        return Ok(None);
    }
    let query = Query::select()
        .column(Alias::new("storage_root_id"))
        .from(Alias::new("storage_root_objects"))
        .and_where(Expr::col(Alias::new("storage_object_id")).eq(scope_id))
        .order_by(
            Alias::new("storage_root_id"),
            sea_orm::sea_query::Order::Asc,
        )
        .limit(2)
        .to_owned();
    let rows = connection
        .query_all(connection.get_database_backend().build(&query))
        .await?;
    (rows.len() == 1)
        .then(|| rows[0].try_get("", "storage_root_id"))
        .transpose()
}

fn migration_uuid(row: &QueryResult, column: &str) -> Result<Uuid, DbErr> {
    match row.try_get::<Uuid>("", column) {
        Ok(value) => Ok(value),
        Err(uuid_error) => row
            .try_get::<String>("", column)
            .ok()
            .and_then(|value| Uuid::parse_str(&value).ok())
            .ok_or(uuid_error),
    }
}

async fn retire_down_migration_conflicts(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let transaction = connection.begin().await?;
    let query = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("scope_type"),
            Alias::new("scope_id"),
            Alias::new("task_kind"),
            Alias::new("expected_revision"),
        ])
        .from(Alias::new(TABLE))
        .and_where(Expr::col(Alias::new("state")).is_in(ACTIVE_STATES))
        .order_by(Alias::new("created_at"), sea_orm::sea_query::Order::Asc)
        .order_by(Alias::new("id"), sea_orm::sea_query::Order::Asc)
        .to_owned();
    let backend = transaction.get_database_backend();
    let mut seen = HashSet::new();
    for row in transaction.query_all(backend.build(&query)).await? {
        let key = (
            row.try_get::<String>("", "scope_type")?,
            row.try_get::<Uuid>("", "scope_id")?,
            row.try_get::<String>("", "task_kind")?,
            row.try_get::<i64>("", "expected_revision")?,
        );
        if !seen.insert(key) {
            retire_job(
                &transaction,
                row.try_get("", "id")?,
                "storage-root-specific work retired during migration rollback",
            )
            .await?;
        }
    }
    transaction.commit().await
}

async fn retire_job(
    connection: &impl ConnectionTrait,
    job_id: Uuid,
    reason: &str,
) -> Result<(), DbErr> {
    let backend = connection.get_database_backend();
    let mut update = Query::update();
    update
        .table(Alias::new(TABLE))
        .value(Alias::new("state"), "Failed")
        .value(Alias::new("completed_at"), Utc::now())
        .value(Alias::new("lease_owner"), Option::<String>::None)
        .value(
            Alias::new("lease_expires_at"),
            Option::<chrono::DateTime<Utc>>::None,
        )
        .value(Alias::new("last_error"), reason)
        .and_where(Expr::col(Alias::new("id")).eq(job_id));
    if backend == DbBackend::MySql {
        update.value(Alias::new("active_slot"), Option::<String>::None);
    }
    connection.execute(backend.build(&update)).await?;
    Ok(())
}

async fn drop_active_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .drop_index(
            Index::drop()
                .name(INDEX)
                .table(Alias::new(TABLE))
                .to_owned(),
        )
        .await
}

async fn create_active_index(
    manager: &SchemaManager<'_>,
    include_storage_root: bool,
) -> Result<(), DbErr> {
    let backend = manager.get_connection().get_database_backend();
    let mut index = Index::create();
    index.name(INDEX).table(Alias::new(TABLE));
    if backend == DbBackend::MySql {
        index.col((Alias::new("scope_type"), 32));
    } else {
        index.col(Alias::new("scope_type"));
    }
    index.col(Alias::new("scope_id"));
    if backend == DbBackend::MySql {
        index.col((Alias::new("task_kind"), 32));
    } else {
        index.col(Alias::new("task_kind"));
    }
    index.col(Alias::new("expected_revision"));
    if include_storage_root {
        index.col(Alias::new(COLUMN));
    }
    if backend == DbBackend::MySql {
        index.col((Alias::new("active_slot"), 16));
    } else {
        index.and_where(Expr::col(Alias::new("state")).is_in(ACTIVE_STATES));
    }
    manager.create_index(index.unique().to_owned()).await
}
