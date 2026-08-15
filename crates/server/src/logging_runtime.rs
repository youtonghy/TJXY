use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
    time::Duration,
};

use chrono::{Days, NaiveDate, Utc};
use thiserror::Error;
use tjxy_db::LogMode;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    Layer, Registry,
    filter::{LevelFilter, Targets},
    layer::SubscriberExt,
    reload,
    util::SubscriberInitExt,
};

const FILE_PREFIX: &str = "tjxy.";
const FILE_SUFFIX: &str = ".log";

#[derive(Clone)]
pub struct LoggingRuntime {
    directory: PathBuf,
    filter: reload::Handle<Targets, Registry>,
    retention_days: Arc<AtomicU16>,
}

impl LoggingRuntime {
    /// Initializes the process-wide JSON file subscriber in Error mode.
    ///
    /// # Errors
    /// Returns an error when the directory, appender, or global subscriber cannot be initialized.
    pub fn initialize(directory: PathBuf) -> Result<(Self, WorkerGuard), LoggingRuntimeError> {
        fs::create_dir_all(&directory)?;
        let appender = tracing_appender::rolling::RollingFileAppender::builder()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("tjxy")
            .filename_suffix("log")
            .build(&directory)
            .map_err(|error| LoggingRuntimeError::Appender(error.to_string()))?;
        let (writer, guard) = tracing_appender::non_blocking(appender);
        let (filter, handle) = reload::Layer::new(filter_for_mode(LogMode::Error));
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_writer(writer),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_filter(LevelFilter::ERROR),
            )
            .try_init()
            .map_err(|error| LoggingRuntimeError::Subscriber(error.to_string()))?;
        Ok((
            Self {
                directory,
                filter: handle,
                retention_days: Arc::new(AtomicU16::new(tjxy_db::DEFAULT_LOG_RETENTION_DAYS)),
            },
            guard,
        ))
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Applies a new runtime level immediately.
    ///
    /// # Errors
    /// Returns an error if the subscriber has already shut down.
    pub fn set_mode(&self, mode: LogMode) -> Result<(), LoggingRuntimeError> {
        self.filter
            .reload(filter_for_mode(mode))
            .map_err(|error| LoggingRuntimeError::Reload(error.to_string()))
    }

    /// Removes TJXY daily log files older than the configured UTC retention window.
    ///
    /// # Errors
    /// Returns the first directory or deletion error. Unrecognized files are never removed.
    pub fn cleanup(&self, retention_days: u16) -> Result<(), LoggingRuntimeError> {
        if !(1..=365).contains(&retention_days) {
            return Err(LoggingRuntimeError::InvalidRetention);
        }
        self.retention_days.store(retention_days, Ordering::Relaxed);
        cleanup_directory(&self.directory, retention_days, Utc::now().date_naive())
    }

    /// Reapplies the current retention policy while the server remains running.
    pub async fn run_retention_scheduler(self: Arc<Self>) {
        let mut schedule = tokio::time::interval(Duration::from_secs(60 * 60));
        schedule.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        schedule.tick().await;
        loop {
            schedule.tick().await;
            let days = self.retention_days.load(Ordering::Relaxed);
            if let Err(error) = self.cleanup(days) {
                tracing::error!(error = %error, "scheduled log retention cleanup failed");
            }
        }
    }
}

fn filter_for_mode(mode: LogMode) -> Targets {
    let application_level = match mode {
        LogMode::Error => LevelFilter::ERROR,
        LogMode::Debug => LevelFilter::DEBUG,
    };
    Targets::new()
        .with_default(LevelFilter::ERROR)
        .with_target("tjxy_", application_level)
}

pub(crate) fn cleanup_directory(
    directory: &Path,
    retention_days: u16,
    today: NaiveDate,
) -> Result<(), LoggingRuntimeError> {
    let cutoff = today
        .checked_sub_days(Days::new(u64::from(retention_days.saturating_sub(1))))
        .ok_or(LoggingRuntimeError::InvalidRetention)?;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Some(date) = log_file_date(&entry.file_name().to_string_lossy()) else {
            continue;
        };
        if date < cutoff {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

pub(crate) fn log_file_date(name: &str) -> Option<NaiveDate> {
    let date = name.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?;
    if date.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

#[derive(Debug, Error)]
pub enum LoggingRuntimeError {
    #[error("log file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("logging subscriber initialization failed: {0}")]
    Subscriber(String),
    #[error("logging appender initialization failed: {0}")]
    Appender(String),
    #[error("logging filter reload failed: {0}")]
    Reload(String),
    #[error("logging retention window is invalid")]
    InvalidRetention,
}

#[cfg(test)]
mod tests {
    use super::{cleanup_directory, filter_for_mode, log_file_date};
    use chrono::NaiveDate;
    use tjxy_db::LogMode;
    use tracing::Level;

    #[test]
    fn debug_mode_keeps_dependency_noise_at_error() {
        let filter = filter_for_mode(LogMode::Debug);
        assert!(filter.would_enable("tjxy_server::worker", &Level::DEBUG));
        assert!(!filter.would_enable("sqlx::query", &Level::INFO));
        assert!(filter.would_enable("sqlx::query", &Level::ERROR));
    }

    #[test]
    fn only_strict_daily_names_are_recognized() {
        assert_eq!(
            log_file_date("tjxy.2026-08-13.log"),
            NaiveDate::from_ymd_opt(2026, 8, 13)
        );
        assert_eq!(log_file_date("../tjxy.2026-08-13.log"), None);
        assert_eq!(log_file_date("tjxy.2026-8-13.log"), None);
        assert_eq!(log_file_date("server.2026-08-13.log"), None);
    }

    #[test]
    fn cleanup_preserves_window_and_unrecognized_files() {
        let directory = tempfile::tempdir().unwrap();
        for name in [
            "tjxy.2026-08-09.log",
            "tjxy.2026-08-10.log",
            "tjxy.2026-08-13.log",
            "notes.txt",
        ] {
            std::fs::write(directory.path().join(name), name).unwrap();
        }
        cleanup_directory(
            directory.path(),
            4,
            NaiveDate::from_ymd_opt(2026, 8, 13).unwrap(),
        )
        .unwrap();
        assert!(!directory.path().join("tjxy.2026-08-09.log").exists());
        assert!(directory.path().join("tjxy.2026-08-10.log").exists());
        assert!(directory.path().join("notes.txt").exists());
    }
}
