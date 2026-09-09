use sea_orm::ConnectionTrait;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Default, Serialize)]
struct Counter {
    statements: u64,
    elapsed_us: u128,
    failures: u64,
}
#[derive(Default)]
pub struct SqlMeasurements {
    counters: Mutex<BTreeMap<&'static str, Counter>>,
    claim: Mutex<Option<sea_orm::Statement>>,
}
impl SqlMeasurements {
    pub fn observe(&self, info: &sea_orm::metric::Info<'_>) {
        let sql = &info.statement.sql;
        let category = if sql.starts_with("SELECT") && sql.contains("\"work_jobs\" AS \"job\"") {
            let mut claim = self.claim.lock().unwrap();
            if claim.is_none() {
                *claim = Some(info.statement.clone());
            }
            "queue_claim"
        } else if sql.contains("work_maintenance_state") {
            "queue_maintenance_lease"
        } else if sql.contains("work_jobs") {
            "work_jobs_other"
        } else if sql.contains("catalog_publications") {
            "publications"
        } else if sql.contains("storage_root_objects") {
            "storage_inventory"
        } else if sql.contains("invalidation") || sql.contains("outbox") {
            "outbox"
        } else {
            "other"
        };
        let mut counters = self.counters.lock().unwrap();
        let counter = counters.entry(category).or_default();
        counter.statements += 1;
        counter.elapsed_us += info.elapsed.as_micros();
        counter.failures += u64::from(info.failed);
    }
    pub fn spawn(self: Arc<Self>, directory: PathBuf, database: sea_orm::DatabaseConnection) {
        tokio::spawn(async move {
            let mut plan_written = false;
            loop {
                let bytes = serde_json::to_vec_pretty(&*self.counters.lock().unwrap()).unwrap();
                let temporary = directory.join("metrics.next.json");
                if std::fs::write(&temporary, bytes).is_ok() {
                    let _ = std::fs::rename(temporary, directory.join("metrics.json"));
                }
                if !plan_written {
                    let statement = self.claim.lock().unwrap().clone();
                    if let Some(mut statement) = statement {
                        statement.sql = format!("EXPLAIN QUERY PLAN {}", statement.sql);
                        if let Ok(rows) = database.query_all(statement).await {
                            let details = rows
                                .iter()
                                .filter_map(|row| row.try_get::<String>("", "detail").ok())
                                .collect::<Vec<_>>();
                            let _ = std::fs::write(
                                directory.join("claim-plan.json"),
                                serde_json::to_vec_pretty(&details).unwrap(),
                            );
                        }
                        plan_written = true;
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }
}
