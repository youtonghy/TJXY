//! Bounded scan admission. Durable jobs own correctness; this controller only
//! decides how many background jobs may start. Running jobs are never aborted.
use std::{
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde::Serialize;

pub(crate) const MAX_SCAN_JOBS: usize = 8;
pub(crate) const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);
const COOLDOWN: Duration = Duration::from_secs(10);
const RESERVE_BYTES: u64 = 256 * 1024 * 1024;

/// Adaptive by default. A fixed bound provides a reproducible comparison and
/// a rollback switch without changing scan semantics or persisted job data.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScanConcurrency {
    fixed: Option<usize>,
}

impl FromStr for ScanConcurrency {
    type Err = InvalidScanConcurrency;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "auto" {
            return Ok(Self::default());
        }
        let count = value.parse::<usize>().map_err(|_| InvalidScanConcurrency)?;
        if !(1..=MAX_SCAN_JOBS).contains(&count) {
            return Err(InvalidScanConcurrency);
        }
        Ok(Self { fixed: Some(count) })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("scan concurrency must be auto or an integer from 1 to 8")]
pub struct InvalidScanConcurrency;

// Fixed-size latency histograms avoid storing individual request/query samples.
// The reported P95 is a bucket upper bound in milliseconds.
const LATENCY_BOUNDS_MS: [u64; 9] = [1, 5, 10, 25, 50, 100, 250, 1000, u64::MAX];

#[derive(Default)]
struct Latencies {
    counts: [AtomicU64; 9],
}

impl Latencies {
    fn record(&self, elapsed: Duration) {
        let millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        let bucket = LATENCY_BOUNDS_MS
            .iter()
            .position(|bound| millis <= *bound)
            .unwrap_or(8);
        self.counts[bucket].fetch_add(1, Ordering::Relaxed);
    }

    fn drain(&self) -> Option<u64> {
        let counts = self
            .counts
            .each_ref()
            .map(|count| count.swap(0, Ordering::Relaxed));
        let total: u64 = counts.iter().sum();
        if total == 0 {
            return None;
        }
        let rank = total.saturating_mul(95).div_ceil(100);
        let mut seen = 0;
        for (count, bound) in counts.into_iter().zip(LATENCY_BOUNDS_MS) {
            seen += count;
            if seen >= rank {
                // Keep serialized diagnostics finite while retaining pressure.
                return Some(bound.min(10_000));
            }
        }
        None
    }
}

#[derive(Default)]
pub(crate) struct ScanPressure {
    database: Latencies,
    claims: Latencies,
    foreground: Latencies,
    failures: AtomicU64,
}

impl ScanPressure {
    pub(crate) fn database(&self, elapsed: Duration, failed: bool) {
        self.database.record(elapsed);
        if failed {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn claim(&self, elapsed: Duration, failed: bool) {
        self.claims.record(elapsed);
        if failed {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn foreground(&self, elapsed: Duration) {
        self.foreground.record(elapsed);
    }

    pub(crate) fn drain(&self) -> PressureSample {
        PressureSample {
            // A transaction waiting for the pool must not be diluted by the
            // many fast statements executed after it finally acquires a slot.
            database_p95_ms: self.database.drain().max(self.claims.drain()),
            foreground_p95_ms: self.foreground.drain(),
            database_failures: self.failures.swap(0, Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PressureSample {
    pub(crate) database_p95_ms: Option<u64>,
    pub(crate) foreground_p95_ms: Option<u64>,
    pub(crate) database_failures: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HostSample {
    pub(crate) cpu_percent: f32,
    pub(crate) available_bytes: u64,
    pub(crate) total_bytes: u64,
    pub(crate) sampled_at: Instant,
}

/// Start one CPU/memory sampler per server, never a process or disk inventory.
/// Native sampling runs off the async executor. An absent/stale sample prevents
/// expansion, including unsupported systems and a stalled native refresh.
pub(crate) fn sample_host(
    mut activity: tokio::sync::watch::Receiver<bool>,
) -> tokio::sync::watch::Receiver<Option<HostSample>> {
    let (sender, receiver) = tokio::sync::watch::channel(None);
    tokio::spawn(async move {
        let mut system = sysinfo::System::new();
        let mut primed = false;
        loop {
            if sender.is_closed() {
                return;
            }
            if !*activity.borrow() {
                primed = false;
                sender.send_replace(None);
                if activity.wait_for(|active| *active).await.is_err() {
                    return;
                }
            }
            let refresh = tokio::task::spawn_blocking(move || {
                system.refresh_cpu_usage();
                system.refresh_memory();
                let process = sysinfo::get_current_pid().ok();
                if let Some(pid) = process {
                    system.refresh_processes_specifics(
                        sysinfo::ProcessesToUpdate::Some(&[pid]),
                        true,
                        sysinfo::ProcessRefreshKind::nothing().with_cpu(),
                    );
                }
                let cores =
                    std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
                let cores = f32::from(u16::try_from(cores).unwrap_or(u16::MAX));
                let process_cpu = process
                    .and_then(|pid| system.process(pid))
                    .map_or(0.0, |process| process.cpu_usage() / cores);
                let mut total = system.total_memory();
                let mut available = system.available_memory();
                if let Some(cgroup) = system.cgroup_limits() {
                    total = total.min(cgroup.total_memory);
                    available = available.min(cgroup.free_memory);
                }
                let sample =
                    (sysinfo::IS_SUPPORTED_SYSTEM && total > 0 && !system.cpus().is_empty())
                        .then_some(HostSample {
                            cpu_percent: system.global_cpu_usage().max(process_cpu),
                            available_bytes: available,
                            total_bytes: total,
                            sampled_at: Instant::now(),
                        });
                (system, sample)
            })
            .await;
            let Ok((refreshed, sample)) = refresh else {
                sender.send_replace(None);
                return;
            };
            system = refreshed;
            // CPU usage requires two refreshes separated by the sample interval.
            sender.send_replace(if primed { sample } else { None });
            primed = true;
            tokio::select! {
                () = tokio::time::sleep(SAMPLE_INTERVAL) => {}
                changed = activity.changed() => { if changed.is_err() { return; } }
            }
        }
    });
    receiver
}

/// Aggregate only; no job IDs, paths, credentials or raw database statements.
#[derive(Clone, Debug, Serialize)]
pub struct ScanConcurrencySample {
    pub elapsed_seconds: f64,
    pub mode: &'static str,
    pub slots: usize,
    pub active: usize,
    pub pending_sample: usize,
    pub pending_sample_capped: bool,
    pub completed: u64,
    pub cpu_percent: Option<f32>,
    pub available_memory_bytes: Option<u64>,
    pub database_p95_ms_upper_bound: Option<u64>,
    pub foreground_p95_ms_upper_bound: Option<u64>,
    pub reason: &'static str,
}

pub(crate) type ScanObserver = Arc<dyn Fn(&ScanConcurrencySample) + Send + Sync>;

pub(crate) struct Controller {
    config: ScanConcurrency,
    slots: usize,
    healthy: u8,
    cooldown_until: Instant,
    cpu_ema: Option<f32>,
    rate_ema: Option<f64>,
    expansion_rate: Option<f64>,
    evaluation_at: Instant,
}

impl Controller {
    pub(crate) fn new(config: ScanConcurrency, now: Instant) -> Self {
        Self {
            config,
            slots: config.fixed.unwrap_or(1),
            healthy: 0,
            cooldown_until: now,
            cpu_ema: None,
            rate_ema: None,
            expansion_rate: None,
            evaluation_at: now,
        }
    }

    pub(crate) const fn slots(&self) -> usize {
        self.slots
    }

    pub(crate) const fn mode(&self) -> &'static str {
        if self.config.fixed.is_some() {
            "fixed"
        } else {
            "auto"
        }
    }

    pub(crate) fn update(
        &mut self,
        now: Instant,
        host: Option<HostSample>,
        pressure: PressureSample,
        pending: usize,
        completions_per_second: f64,
    ) -> &'static str {
        let valid_host = host.filter(|sample| {
            now.saturating_duration_since(sample.sampled_at) <= SAMPLE_INTERVAL * 3
                && sample.cpu_percent.is_finite()
                && sample.total_bytes > 0
        });
        let Some(host) = valid_host else {
            self.shrink(now, 1);
            return "host_sample_unavailable";
        };
        let cpu = self.cpu_ema.map_or(host.cpu_percent, |previous| {
            previous * 0.7 + host.cpu_percent * 0.3
        });
        self.cpu_ema = Some(cpu);
        let rate = self.rate_ema.map_or(completions_per_second, |previous| {
            previous * 0.5 + completions_per_second * 0.5
        });
        self.rate_ema = Some(rate);
        let memory_reserve = RESERVE_BYTES.max(host.total_bytes / 10);
        if host.available_bytes < memory_reserve {
            self.shrink(now, (self.slots / 2).max(1));
            return "memory_pressure";
        }
        if host.cpu_percent >= 90.0 || cpu >= 85.0 {
            self.shrink(now, (self.slots / 2).max(1));
            return "cpu_pressure";
        }
        if pressure.database_failures > 0
            || pressure
                .database_p95_ms
                .is_some_and(|latency| latency >= 100)
        {
            self.shrink(now, (self.slots / 2).max(1));
            return "database_pressure";
        }
        if pressure
            .foreground_p95_ms
            .is_some_and(|latency| latency >= 250)
        {
            self.shrink(now, (self.slots / 2).max(1));
            return "foreground_pressure";
        }
        if let Some(fixed) = self.config.fixed {
            if now >= self.cooldown_until {
                self.slots = fixed;
            }
            return "fixed_bound";
        }
        // A small tail gains little from starting extra workers. Bound the
        // estimate so counting a million-row history is never required.
        let demand = pending.div_ceil(32).clamp(1, MAX_SCAN_JOBS);
        if demand < self.slots {
            self.slots = demand;
            self.expansion_rate = None;
            self.healthy = 0;
            return "small_backlog";
        }
        if let Some(previous_rate) = self.expansion_rate
            && now >= self.evaluation_at
        {
            self.expansion_rate = None;
            if previous_rate > 0.0 && rate < previous_rate * 1.05 {
                self.shrink(now, self.slots.saturating_sub(1).max(1));
                // Do not repeatedly probe a serial database or saturated backend
                // when extra admission did not produce measurable throughput.
                self.cooldown_until = now + COOLDOWN * 3;
                return if rate < previous_rate * 0.9 {
                    "throughput_regressed"
                } else {
                    "no_throughput_gain"
                };
            }
        }
        if now < self.cooldown_until
            || cpu >= 70.0
            || pressure.database_p95_ms.is_some_and(|latency| latency > 25)
        {
            self.healthy = 0;
            return "holding";
        }
        self.healthy = self.healthy.saturating_add(1);
        if self.healthy >= 3 && self.slots < demand {
            self.slots += 1;
            self.healthy = 0;
            self.expansion_rate = Some(rate);
            self.evaluation_at = now + COOLDOWN;
            self.cooldown_until = now + COOLDOWN;
            return "healthy_expansion";
        }
        "stable"
    }

    fn shrink(&mut self, now: Instant, slots: usize) {
        self.slots = slots;
        self.healthy = 0;
        self.expansion_rate = None;
        self.cooldown_until = now + COOLDOWN;
    }
}

pub(crate) fn probe_ceiling() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |count| count.get().saturating_sub(1).clamp(1, 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(now: Instant) -> HostSample {
        HostSample {
            cpu_percent: 20.0,
            available_bytes: 4 << 30,
            total_bytes: 8 << 30,
            sampled_at: now,
        }
    }

    #[test]
    fn healthy_backlog_expands_slowly_and_respects_cooldown() {
        let start = Instant::now();
        let mut controller = Controller::new(ScanConcurrency::default(), start);
        for seconds in [0, 2] {
            let now = start + Duration::from_secs(seconds);
            controller.update(now, Some(host(now)), PressureSample::default(), 256, 10.0);
            assert_eq!(controller.slots(), 1);
        }
        let now = start + Duration::from_secs(4);
        assert_eq!(
            controller.update(now, Some(host(now)), PressureSample::default(), 256, 10.0),
            "healthy_expansion"
        );
        assert_eq!(controller.slots(), 2);
        for seconds in [6, 8, 10, 12] {
            let now = start + Duration::from_secs(seconds);
            controller.update(now, Some(host(now)), PressureSample::default(), 256, 15.0);
            assert_eq!(controller.slots(), 2);
        }
    }

    #[test]
    fn load_database_and_foreground_pressure_each_reduce_admission() {
        let now = Instant::now();
        for (cpu, available, database_ms, foreground_ms, expected) in [
            (95.0, 4 << 30, None, None, "cpu_pressure"),
            (20.0, 64 << 20, None, None, "memory_pressure"),
            (20.0, 4 << 30, Some(100), None, "database_pressure"),
            (20.0, 4 << 30, None, Some(250), "foreground_pressure"),
        ] {
            let mut controller = Controller::new("8".parse().unwrap(), now);
            let mut sample = host(now);
            sample.cpu_percent = cpu;
            sample.available_bytes = available;
            let pressure = PressureSample {
                database_p95_ms: database_ms,
                foreground_p95_ms: foreground_ms,
                ..PressureSample::default()
            };
            assert_eq!(
                controller.update(now, Some(sample), pressure, 256, 10.0),
                expected
            );
            assert_eq!(controller.slots(), 4);
            // Healthy samples during cooldown cannot immediately undo shrinking.
            controller.update(
                now + Duration::from_secs(2),
                Some(host(now)),
                PressureSample::default(),
                256,
                10.0,
            );
            assert_eq!(controller.slots(), 4);
        }
    }

    #[test]
    fn unknown_or_stale_host_data_falls_back_to_one_and_later_recovers() {
        let now = Instant::now();
        let mut controller = Controller::new("4".parse().unwrap(), now);
        controller.update(
            now + Duration::from_secs(7),
            Some(host(now)),
            PressureSample::default(),
            256,
            5.0,
        );
        assert_eq!(controller.slots(), 1);
        let recovered = now + Duration::from_secs(20);
        controller.update(
            recovered,
            Some(host(recovered)),
            PressureSample::default(),
            256,
            5.0,
        );
        assert_eq!(controller.slots(), 4);
    }

    #[test]
    fn small_backlogs_do_not_expand_and_large_backlogs_cannot_exceed_eight() {
        let start = Instant::now();
        let mut controller = Controller::new(ScanConcurrency::default(), start);
        for seconds in 0..100 {
            let now = start + Duration::from_secs(seconds * 2);
            controller.update(now, Some(host(now)), PressureSample::default(), 32, 10.0);
        }
        assert_eq!(controller.slots(), 1);
        for seconds in 100..300 {
            let now = start + Duration::from_secs(seconds * 2);
            let rate = 10.0 * f64::from(u32::try_from(controller.slots()).unwrap());
            controller.update(
                now,
                Some(host(now)),
                PressureSample::default(),
                usize::MAX,
                rate,
            );
        }
        assert_eq!(controller.slots(), MAX_SCAN_JOBS);
    }

    #[test]
    fn throughput_regression_undoes_a_probe_expansion() {
        let start = Instant::now();
        let mut controller = Controller::new(ScanConcurrency::default(), start);
        for seconds in [0, 2, 4] {
            let now = start + Duration::from_secs(seconds);
            controller.update(now, Some(host(now)), PressureSample::default(), 256, 20.0);
        }
        assert_eq!(controller.slots(), 2);
        let now = start + Duration::from_secs(14);
        assert_eq!(
            controller.update(now, Some(host(now)), PressureSample::default(), 256, 1.0),
            "throughput_regressed"
        );
        assert_eq!(controller.slots(), 1);
    }

    #[test]
    fn flat_throughput_returns_capacity_and_waits_before_trying_again() {
        let start = Instant::now();
        let mut controller = Controller::new(ScanConcurrency::default(), start);
        for seconds in [0, 2, 4] {
            let now = start + Duration::from_secs(seconds);
            controller.update(now, Some(host(now)), PressureSample::default(), 256, 20.0);
        }
        let now = start + Duration::from_secs(14);
        assert_eq!(
            controller.update(now, Some(host(now)), PressureSample::default(), 256, 20.0),
            "no_throughput_gain"
        );
        assert_eq!(controller.slots(), 1);
        for seconds in (16..44).step_by(2) {
            let now = start + Duration::from_secs(seconds);
            controller.update(now, Some(host(now)), PressureSample::default(), 256, 20.0);
            assert_eq!(controller.slots(), 1);
        }
    }

    #[test]
    fn queued_claims_are_not_hidden_by_fast_sql_samples() {
        let pressure = ScanPressure::default();
        for _ in 0..1000 {
            pressure.database(Duration::from_millis(1), false);
        }
        pressure.claim(Duration::from_millis(200), false);
        pressure.claim(Duration::from_millis(200), false);
        let sample = pressure.drain();
        assert_eq!(sample.database_p95_ms, Some(250));
        let now = Instant::now();
        let mut controller = Controller::new("4".parse().unwrap(), now);
        assert_eq!(
            controller.update(now, Some(host(now)), sample, 256, 10.0),
            "database_pressure"
        );
        assert_eq!(controller.slots(), 2);
    }

    #[test]
    fn latency_histogram_is_bounded_and_each_window_is_drained() {
        let samples = Latencies::default();
        for _ in 0..95 {
            samples.record(Duration::from_millis(5));
        }
        for _ in 0..5 {
            samples.record(Duration::from_secs(5));
        }
        assert_eq!(samples.drain(), Some(5));
        assert_eq!(samples.drain(), None);
        assert!("9".parse::<ScanConcurrency>().is_err());
        assert!("0".parse::<ScanConcurrency>().is_err());
    }
}
