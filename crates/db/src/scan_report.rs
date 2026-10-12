use sea_orm::{
    ConnectionTrait, DbErr,
    sea_query::{Alias, Expr, Order, Query},
};
use serde::{Deserialize, Serialize};
use tjxy_common::{CatalogItemId, WorkJobId};

use crate::{
    ClaimedWorkJob, FullScanRepository, WorkJobRepository, WorkJobRepositoryError, WorkStagingRow,
};

const ISSUE_KIND: &str = "FullScanItemIssue";
/// Marks an item that received child work in a scan; keyed by the bare item id.
pub(crate) const TOUCHED_KIND: &str = "FullScanTouched";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ScanItemIssue {
    pub item_id: uuid::Uuid,
    pub child_job_id: uuid::Uuid,
    pub task_kind: String,
    pub scope_type: String,
    pub scope_id: uuid::Uuid,
    pub needs_selection: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ScanReportPage {
    pub counters: Option<serde_json::Value>,
    pub issues: Vec<ScanItemIssue>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ScanHistoryEntry {
    pub id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub state: String,
}

impl FullScanRepository<'_> {
    /// Lists scan parents independently of the much larger child-task history.
    /// # Errors
    /// Returns database failures.
    pub async fn history_page(&self, offset: u64) -> Result<Vec<ScanHistoryEntry>, DbErr> {
        let database = self.connection();
        database
            .query_all(
                database.get_database_backend().build(
                    Query::select()
                        .columns([
                            Alias::new("id"),
                            Alias::new("created_at"),
                            Alias::new("state"),
                        ])
                        .from(Alias::new("work_jobs"))
                        .and_where(
                            Expr::col(Alias::new("task_kind"))
                                .is_in(["FullMediaScan", "FullLibraryRootScan"]),
                        )
                        .order_by(Alias::new("created_at"), Order::Desc)
                        .order_by(Alias::new("id"), Order::Asc)
                        .offset(offset)
                        .limit(50),
                ),
            )
            .await?
            .into_iter()
            .map(|row| {
                Ok(ScanHistoryEntry {
                    id: row.try_get("", "id")?,
                    created_at: row.try_get("", "created_at")?,
                    state: row.try_get("", "state")?,
                })
            })
            .collect()
    }

    /// Records one isolated item failure under the parent's lease and retention lifecycle.
    ///
    /// # Errors
    /// Returns serialization, lease or database failures.
    pub async fn record_item_issue(
        &self,
        claimed: &ClaimedWorkJob,
        issue: &ScanItemIssue,
    ) -> Result<(), WorkJobRepositoryError> {
        let payload =
            serde_json::to_value(issue).map_err(|_| WorkJobRepositoryError::InvalidStagingRow)?;
        WorkJobRepository::new(self.connection())
            .stage_batch(
                claimed,
                claimed.id().as_uuid(),
                &[WorkStagingRow::new(
                    ISSUE_KIND,
                    issue.item_id.to_string(),
                    payload,
                    "Warning",
                )?],
            )
            .await
    }

    /// Reads the durable outcome of one item, including after orchestration restarts.
    ///
    /// # Errors
    /// Returns database or payload failures.
    pub async fn item_issue(
        &self,
        job: WorkJobId,
        item: CatalogItemId,
    ) -> Result<Option<ScanItemIssue>, DbErr> {
        let database = self.connection();
        let row = database
            .query_one(
                database.get_database_backend().build(
                    Query::select()
                        .column(Alias::new("payload"))
                        .from(Alias::new("work_staging_rows"))
                        .and_where(Expr::col(Alias::new("job_id")).eq(job.as_uuid()))
                        .and_where(Expr::col(Alias::new("entity_kind")).eq(ISSUE_KIND))
                        .and_where(Expr::col(Alias::new("natural_key")).eq(item.to_string())),
                ),
            )
            .await?;
        row.map(|row| {
            serde_json::from_value(row.try_get("", "payload")?)
                .map_err(|_| DbErr::Custom("invalid scan item report".to_owned()))
        })
        .transpose()
    }

    /// Reads durable issues for one bounded target page.
    /// # Errors
    /// Returns an invalid page size, database or payload failure.
    pub async fn item_issues(
        &self,
        job: WorkJobId,
        items: &[CatalogItemId],
    ) -> Result<std::collections::HashMap<CatalogItemId, ScanItemIssue>, DbErr> {
        if items.len() > 128 {
            return Err(DbErr::Custom(
                "scan report page exceeds 128 items".to_owned(),
            ));
        }
        if items.is_empty() {
            return Ok(std::collections::HashMap::default());
        }
        let database = self.connection();
        database
            .query_all(
                database.get_database_backend().build(
                    Query::select()
                        .column(Alias::new("payload"))
                        .from(Alias::new("work_staging_rows"))
                        .and_where(Expr::col(Alias::new("job_id")).eq(job.as_uuid()))
                        .and_where(Expr::col(Alias::new("entity_kind")).eq(ISSUE_KIND))
                        .and_where(
                            Expr::col(Alias::new("natural_key"))
                                .is_in(items.iter().map(ToString::to_string)),
                        ),
                ),
            )
            .await?
            .into_iter()
            .map(|row| {
                let issue: ScanItemIssue = serde_json::from_value(row.try_get("", "payload")?)
                    .map_err(|_| DbErr::Custom("invalid scan item report".to_owned()))?;
                Ok((CatalogItemId::from_uuid(issue.item_id), issue))
            })
            .collect()
    }

    /// Returns the page items that received child work in this scan, so a no-change scan reports
    /// skipped items accurately. One indexed lookup per target page.
    ///
    /// # Errors
    /// Returns an invalid page size or database failures.
    pub async fn items_had_work(
        &self,
        job: WorkJobId,
        items: &[CatalogItemId],
    ) -> Result<std::collections::HashSet<CatalogItemId>, DbErr> {
        if items.len() > 128 {
            return Err(DbErr::Custom(
                "scan report page exceeds 128 items".to_owned(),
            ));
        }
        if items.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let database = self.connection();
        database
            .query_all(
                database.get_database_backend().build(
                    Query::select()
                        .column(Alias::new("natural_key"))
                        .from(Alias::new("work_staging_rows"))
                        .and_where(Expr::col(Alias::new("job_id")).eq(job.as_uuid()))
                        .and_where(Expr::col(Alias::new("publication_id")).eq(job.as_uuid()))
                        .and_where(Expr::col(Alias::new("entity_kind")).eq(TOUCHED_KIND))
                        .and_where(
                            Expr::col(Alias::new("natural_key"))
                                .is_in(items.iter().map(ToString::to_string)),
                        ),
                ),
            )
            .await?
            .into_iter()
            .map(|row| {
                let key: String = row.try_get("", "natural_key")?;
                uuid::Uuid::parse_str(&key)
                    .map(CatalogItemId::from_uuid)
                    .map_err(|_| DbErr::Custom("invalid scan touched item".to_owned()))
            })
            .collect()
    }

    /// Returns one bounded diagnostic page; legacy scans have no detailed counters.
    ///
    /// # Errors
    /// Returns database or payload failures.
    pub async fn report_page(&self, job: WorkJobId, offset: u64) -> Result<ScanReportPage, DbErr> {
        let database = self.connection();
        let result = database
            .query_one(
                database.get_database_backend().build(
                    Query::select()
                        .column(Alias::new("counters"))
                        .from(Alias::new("work_results"))
                        .and_where(Expr::col(Alias::new("job_id")).eq(job.as_uuid())),
                ),
            )
            .await?;
        let counters = result
            .map(|row| row.try_get::<serde_json::Value>("", "counters"))
            .transpose()?
            .filter(|value| value.get("needs_selection").is_some());
        let rows = database
            .query_all(
                database.get_database_backend().build(
                    Query::select()
                        .column(Alias::new("payload"))
                        .from(Alias::new("work_staging_rows"))
                        .and_where(Expr::col(Alias::new("job_id")).eq(job.as_uuid()))
                        .and_where(Expr::col(Alias::new("entity_kind")).eq(ISSUE_KIND))
                        .order_by(Alias::new("natural_key"), Order::Asc)
                        .offset(offset)
                        .limit(51),
                ),
            )
            .await?;
        let has_more = rows.len() > 50;
        let issues = rows
            .into_iter()
            .take(50)
            .map(|row| {
                serde_json::from_value(row.try_get("", "payload")?)
                    .map_err(|_| DbErr::Custom("invalid scan item report".to_owned()))
            })
            .collect::<Result<_, _>>()?;
        Ok(ScanReportPage {
            counters,
            issues,
            has_more,
        })
    }
}
