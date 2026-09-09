//! Low-frequency diagnostics; no counters are updated on request or task hot paths.
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement,
    sea_query::{Alias, Expr, Query},
};
use serde::Serialize;

const TABLES: &[&str] = &[
    "catalog_items",
    "item_assets",
    "asset_blobs",
    "person_assets",
    "direct_metadata_refs",
    "work_jobs",
    "work_results",
    "work_staging_rows",
    "storage_sync_pages",
    "work_job_retention_queue",
    "catalog_publications",
    "publication_catalog_items",
    "publication_media_sources",
    "publication_media_locations",
    "nfo_choices",
    "catalog_change_outbox",
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WorkHealth {
    pub sampled_at: DateTime<Utc>,
    pub backend: &'static str,
    pub tables: Vec<TableHealth>,
    pub pending_jobs: u64,
    pub oldest_pending_at: Option<DateTime<Utc>>,
    pub retention_candidates: Option<u64>,
    pub retention_days: Option<u64>,
    pub allocated_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub wal_bytes: Option<u64>,
    pub space_is_estimated: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TableHealth {
    pub name: &'static str,
    pub rows: u64,
    pub data_bytes: Option<u64>,
    pub index_bytes: Option<u64>,
    pub allocated_bytes: Option<u64>,
}

/// Samples exact row counts and backend-specific space accounting. Call at most once per five minutes.
/// # Errors
/// Returns database failures; unsupported space introspection is represented by null values.
pub async fn sample_work_health(
    database: &DatabaseConnection,
    retention: Option<Duration>,
) -> Result<WorkHealth, DbErr> {
    let backend = database.get_database_backend();
    let mut tables = Vec::new();
    for &name in TABLES {
        let rows = count(
            database,
            Query::select()
                .expr_as(Expr::cust("COUNT(*)"), Alias::new("value"))
                .from(Alias::new(name))
                .to_owned(),
        )
        .await?;
        let (data_bytes, index_bytes, allocated_bytes) = table_space(database, name)
            .await
            .unwrap_or((None, None, None));
        tables.push(TableHealth {
            name,
            rows,
            data_bytes,
            index_bytes,
            allocated_bytes,
        });
    }
    let pending = database
        .query_one(
            backend.build(
                Query::select()
                    .expr_as(Expr::cust("COUNT(*)"), Alias::new("value"))
                    .expr_as(
                        Expr::col(Alias::new("created_at")).min(),
                        Alias::new("oldest"),
                    )
                    .from(Alias::new("work_jobs"))
                    .and_where(Expr::col(Alias::new("state")).eq("Pending")),
            ),
        )
        .await?
        .ok_or_else(|| DbErr::Custom("missing queue diagnostics".into()))?;
    let retention_candidates = if let Some(retention) = retention {
        Some(
            count(
                database,
                Query::select()
                    .expr_as(Expr::cust("COUNT(*)"), Alias::new("value"))
                    .from(Alias::new("work_job_retention_queue"))
                    .and_where(Expr::col(Alias::new("terminal_at")).lte(Utc::now() - retention))
                    .to_owned(),
            )
            .await?,
        )
    } else {
        None
    };
    let (allocated_bytes, free_bytes, wal_bytes) =
        database_space(database).await.unwrap_or((None, None, None));
    Ok(WorkHealth {
        sampled_at: Utc::now(),
        backend: match backend {
            DbBackend::Sqlite => "SQLite",
            DbBackend::Postgres => "PostgreSQL",
            DbBackend::MySql => "MySQL",
        },
        tables,
        pending_jobs: unsigned(pending.try_get("", "value")?)?,
        oldest_pending_at: pending.try_get("", "oldest")?,
        retention_candidates,
        retention_days: retention.map(|value| value.num_days().unsigned_abs()),
        allocated_bytes,
        free_bytes,
        wal_bytes,
        space_is_estimated: backend == DbBackend::MySql,
    })
}
async fn count(
    database: &DatabaseConnection,
    query: sea_orm::sea_query::SelectStatement,
) -> Result<u64, DbErr> {
    let row = database
        .query_one(database.get_database_backend().build(&query))
        .await?
        .ok_or_else(|| DbErr::Custom("missing count".into()))?;
    unsigned(row.try_get("", "value")?)
}
fn unsigned(value: i64) -> Result<u64, DbErr> {
    u64::try_from(value).map_err(|_| DbErr::Custom("negative diagnostic value".into()))
}
async fn table_space(
    database: &DatabaseConnection,
    name: &str,
) -> Result<(Option<u64>, Option<u64>, Option<u64>), DbErr> {
    let backend = database.get_database_backend();
    let query = match backend {
        DbBackend::Postgres => Statement::from_sql_and_values(
            backend,
            "SELECT pg_table_size($1::regclass)::bigint AS data, pg_indexes_size($1::regclass)::bigint AS indexes, pg_total_relation_size($1::regclass)::bigint AS allocated",
            [name.into()],
        ),
        DbBackend::MySql => Statement::from_sql_and_values(
            backend,
            "SELECT CAST(DATA_LENGTH AS SIGNED) AS data, CAST(INDEX_LENGTH AS SIGNED) AS indexes, CAST(DATA_LENGTH + INDEX_LENGTH + DATA_FREE AS SIGNED) AS allocated FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?",
            [name.into()],
        ),
        DbBackend::Sqlite => Statement::from_sql_and_values(
            backend,
            "SELECT COALESCE(SUM(CASE WHEN name = ?1 THEN payload ELSE 0 END),0) AS data, COALESCE(SUM(CASE WHEN name != ?1 THEN pgsize ELSE 0 END),0) AS indexes, COALESCE(SUM(pgsize),0) AS allocated FROM dbstat WHERE name = ?1 OR name IN (SELECT name FROM sqlite_master WHERE type='index' AND tbl_name = ?1)",
            [name.into()],
        ),
    };
    let Some(row) = database.query_one(query).await? else {
        return Ok((None, None, None));
    };
    Ok((
        Some(unsigned(row.try_get("", "data")?)?),
        Some(unsigned(row.try_get("", "indexes")?)?),
        Some(unsigned(row.try_get("", "allocated")?)?),
    ))
}
async fn database_space(
    database: &DatabaseConnection,
) -> Result<(Option<u64>, Option<u64>, Option<u64>), DbErr> {
    let backend = database.get_database_backend();
    if backend == DbBackend::Sqlite {
        let mut values = Vec::new();
        for pragma in ["page_count", "page_size", "freelist_count"] {
            let row = database
                .query_one(Statement::from_string(backend, format!("PRAGMA {pragma}")))
                .await?
                .ok_or_else(|| DbErr::Custom("missing SQLite page statistics".into()))?;
            values.push(unsigned(row.try_get("", pragma)?)?);
        }
        return Ok((
            Some(values[0].saturating_mul(values[1])),
            Some(values[2].saturating_mul(values[1])),
            None,
        ));
    }
    if backend == DbBackend::Postgres {
        let row = database
            .query_one(Statement::from_string(
                backend,
                "SELECT pg_database_size(current_database())::bigint AS size",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("missing PostgreSQL size".into()))?;
        return Ok((Some(unsigned(row.try_get("", "size")?)?), None, None));
    }
    Ok((None, None, None))
}
