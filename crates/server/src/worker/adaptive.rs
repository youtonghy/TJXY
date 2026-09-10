use std::{collections::HashMap, sync::Arc, time::Instant};

use sea_orm::DatabaseConnection;
use tjxy_application::{
    MetadataResolveService, ProbeService, SeriesExpandService, SourceIndexService,
};
use tjxy_db::{ClaimedWorkJob, WorkJobRepository, WorkTaskKind};
use tokio::task::{Id, JoinSet};
use uuid::Uuid;

use crate::scan_concurrency::{
    Controller, SAMPLE_INTERVAL, ScanConcurrency, ScanConcurrencySample, ScanObserver,
    ScanPressure, probe_ceiling, sample_host,
};

use super::{LEASE_DURATION, process_scan_job};

const SCAN_KINDS: [WorkTaskKind; 4] = [
    WorkTaskKind::ResolveMetadata,
    WorkTaskKind::IndexMediaSources,
    WorkTaskKind::ProbeMedia,
    WorkTaskKind::ExpandItem,
];
const BACKLOG_SAMPLE: u64 = 256;

pub(crate) struct ScanServices {
    pub(crate) probe: Arc<ProbeService>,
    pub(crate) metadata: Arc<MetadataResolveService>,
    pub(crate) sources: SourceIndexService,
    pub(crate) series: SeriesExpandService,
}

pub(crate) fn spawn_background_scan_worker(
    database: DatabaseConnection,
    probe: Arc<ProbeService>,
    metadata: Arc<MetadataResolveService>,
    concurrency: ScanConcurrency,
    pressure: Arc<ScanPressure>,
    observer: Option<ScanObserver>,
) {
    let services = Arc::new(ScanServices {
        sources: SourceIndexService::new(database.clone()),
        series: SeriesExpandService::new(database.clone()),
        probe,
        metadata,
    });
    tokio::spawn(run(database, services, concurrency, pressure, observer));
}

#[derive(Default)]
struct ActiveJobs {
    entries: HashMap<Id, (Uuid, WorkTaskKind)>,
    last_root: Option<Uuid>,
}

impl ActiveJobs {
    fn preferred_exclusions(&self) -> Vec<Uuid> {
        let mut roots = self
            .entries
            .values()
            .map(|entry| entry.0)
            .collect::<Vec<_>>();
        roots.extend(self.last_root);
        roots.sort_unstable();
        roots.dedup();
        roots.truncate(8);
        roots
    }

    fn hard_exclusions(&self, slots: usize) -> Vec<Uuid> {
        let mut counts = HashMap::<Uuid, usize>::new();
        for &(root, _) in self.entries.values() {
            *counts.entry(root).or_default() += 1;
        }
        if counts.len() < 2 {
            return Vec::new();
        }
        counts
            .into_iter()
            .filter_map(|(root, count)| (count >= slots.div_ceil(2)).then_some(root))
            .collect()
    }

    fn accepted_kinds(&self) -> Vec<WorkTaskKind> {
        let probes = self
            .entries
            .values()
            .filter(|entry| entry.1 == WorkTaskKind::ProbeMedia)
            .count();
        SCAN_KINDS
            .into_iter()
            .filter(|kind| *kind != WorkTaskKind::ProbeMedia || probes < probe_ceiling())
            .collect()
    }

    fn insert(&mut self, id: Id, job: &ClaimedWorkJob) {
        let root = job
            .job()
            .storage_root_affinity()
            .map_or(Uuid::nil(), tjxy_common::StorageRootId::as_uuid);
        self.last_root = Some(root);
        self.entries.insert(id, (root, job.job().task_kind()));
    }
}

// One dispatcher claims serially and supervises a bounded JoinSet. Every job's
// execution/lease renewal runs independently, including while SQL claims wait.
#[allow(clippy::too_many_lines)]
async fn run(
    database: DatabaseConnection,
    services: Arc<ScanServices>,
    config: ScanConcurrency,
    pressure: Arc<ScanPressure>,
    observer: Option<ScanObserver>,
) {
    let started = Instant::now();
    let mut controller = Controller::new(config, started);
    let mut active = ActiveJobs::default();
    let mut tasks = JoinSet::new();
    let mut idle = tjxy_db::WorkQueueWaiter::default();
    let (sampling, activity) = tokio::sync::watch::channel(false);
    let host = sample_host(activity);
    let mut tick = tokio::time::interval(SAMPLE_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let owner = format!("adaptive-scan-{}", Uuid::new_v4());
    let mut empty = false;
    let mut completed = 0_u32;
    let mut last_sample = started;
    loop {
        while tasks.len() < controller.slots() && !empty {
            let jobs = WorkJobRepository::new(&database);
            let kinds = active.accepted_kinds();
            let preferred = active.preferred_exclusions();
            let before = Instant::now();
            let mut result = jobs
                .claim_next_background(&kinds, &preferred, &owner, LEASE_DURATION)
                .await;
            if matches!(result, Ok(None)) && !preferred.is_empty() {
                result = jobs
                    .claim_next_background(
                        &kinds,
                        &active.hard_exclusions(controller.slots()),
                        &owner,
                        LEASE_DURATION,
                    )
                    .await;
            }
            // This includes pool acquisition and the entire claim transaction,
            // whereas the driver callback observes SQL execution only.
            pressure.claim(before.elapsed(), result.is_err());
            match result {
                Ok(Some(claimed)) => {
                    if !*sampling.borrow() {
                        sampling.send_replace(true);
                    }
                    idle.reset();
                    let task_database = database.clone();
                    let task_services = Arc::clone(&services);
                    let tracked = claimed.clone();
                    let handle = tasks.spawn(async move {
                        process_scan_job(&task_database, &task_services, &claimed).await
                    });
                    active.insert(handle.id(), &tracked);
                }
                Ok(None) => empty = true,
                Err(error) => {
                    tracing::warn!(%error, "background scan claim failed; bounded polling will retry");
                    empty = true;
                }
            }
        }
        if empty && tasks.is_empty() && *sampling.borrow() {
            sampling.send_replace(false);
        }
        tokio::select! {
            outcome = tasks.join_next_with_id(), if !tasks.is_empty() => {
                match outcome {
                    Some(Ok((id, succeeded))) => {
                        active.entries.remove(&id);
                        completed = completed.saturating_add(u32::from(succeeded));
                    }
                    Some(Err(error)) => {
                        active.entries.remove(&error.id());
                        pressure.database(std::time::Duration::ZERO, true);
                        tracing::error!(%error, "scan execution stopped; durable lease recovery remains enabled");
                    }
                    None => {}
                }
                empty = false;
            }
            _ = tick.tick(), if !empty || !tasks.is_empty() => {
                let before = Instant::now();
                let pending = WorkJobRepository::new(&database).pending_background_roots(&SCAN_KINDS, BACKLOG_SAMPLE).await;
                pressure.database(before.elapsed(), pending.is_err());
                let pending = pending.unwrap_or_default();
                let now = Instant::now();
                let sample = *host.borrow();
                let measured = pressure.drain();
                let previous = controller.slots();
                let reason = controller.update(now, sample, measured, pending.len() + tasks.len(),
                    f64::from(completed) / now.duration_since(last_sample).as_secs_f64().max(0.001));
                if controller.slots() != previous {
                    tracing::info!(previous, slots = controller.slots(), reason, "background scan concurrency adjusted");
                }
                if let Some(observer) = &observer {
                    observer(&ScanConcurrencySample {
                        elapsed_seconds: now.duration_since(started).as_secs_f64(),
                        mode: controller.mode(), slots: controller.slots(), active: tasks.len(),
                        pending_sample: pending.len(), pending_sample_capped: pending.len() == 256,
                        completed: u64::from(completed), cpu_percent: sample.map(|sample| sample.cpu_percent),
                        available_memory_bytes: sample.map(|sample| sample.available_bytes),
                        database_p95_ms_upper_bound: measured.database_p95_ms,
                        foreground_p95_ms_upper_bound: measured.foreground_p95_ms,
                        reason,
                    });
                }
                completed = 0;
                last_sample = now;
                empty = false;
            }
            () = idle.wait(), if empty => empty = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[tokio::test]
    async fn competing_roots_are_capped_and_new_roots_are_preferred() {
        let mut tasks = JoinSet::new();
        let mut active = ActiveJobs::default();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        for root in [first, first, second] {
            let task = tasks.spawn(std::future::pending::<()>());
            active
                .entries
                .insert(task.id(), (root, WorkTaskKind::ResolveMetadata));
        }
        assert_eq!(active.hard_exclusions(4), vec![first]);
        let preferred = active
            .preferred_exclusions()
            .into_iter()
            .collect::<HashSet<_>>();
        assert_eq!(preferred, HashSet::from([first, second]));
        // A sole root may use otherwise idle capacity.
        active.entries.retain(|_, (root, _)| *root == first);
        assert!(active.hard_exclusions(4).is_empty());
        tasks.shutdown().await;
    }

    #[tokio::test]
    async fn shrinking_keeps_running_tasks_and_only_blocks_new_admission() {
        let now = Instant::now();
        let mut controller = Controller::new("4".parse().unwrap(), now);
        let mut tasks = JoinSet::new();
        for _ in 0..4 {
            tasks.spawn(std::future::pending::<()>());
        }
        controller.update(
            now,
            None,
            crate::scan_concurrency::PressureSample::default(),
            256,
            0.0,
        );
        assert_eq!(controller.slots(), 1);
        assert_eq!(tasks.len(), 4);
        assert!(tasks.len() >= controller.slots());
        tasks.shutdown().await;
    }
}
