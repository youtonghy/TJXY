//! A single-writer log directory with capacity checked before every write.
use crate::logging_runtime::log_file_date;
use chrono::{NaiveDate, Utc};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
use tjxy_db::LogBudget;

pub(crate) struct BoundedLog {
    directory: PathBuf,
    files: VecDeque<(PathBuf, u64)>,
    active: Option<File>,
    day: NaiveDate,
    sequence: u32,
    budget: LogBudget,
    errors: crate::log_record::ErrorGroups,
    // Prevent two processes from independently spending the same directory budget.
    _lock: File,
}
impl BoundedLog {
    pub(crate) fn new(directory: &Path) -> io::Result<Self> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(directory.join(".tjxy-writer.lock"))?;
        #[cfg(unix)]
        rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
        let mut files = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && log_file_date(&entry.file_name().to_string_lossy()).is_some()
            {
                files.push((entry.path(), entry.metadata()?.len()));
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut writer = Self {
            directory: directory.to_owned(),
            files: files.into(),
            active: None,
            day: Utc::now().date_naive(),
            sequence: 0,
            budget: LogBudget::default(),
            errors: crate::log_record::ErrorGroups::default(),
            _lock: lock,
        };
        writer.sequence = writer
            .files
            .iter()
            .filter_map(|(path, _)| {
                let name = path.file_name()?.to_str()?;
                let key = name
                    .strip_prefix(&format!("tjxy.{}.", writer.day))?
                    .strip_suffix(".log")?;
                key.parse::<u32>().ok().map(|value| value + 1)
            })
            .max()
            .unwrap_or(0);
        writer.set_budget(LogBudget::default())?;
        Ok(writer)
    }
    pub(crate) fn set_budget(&mut self, budget: LogBudget) -> io::Result<()> {
        budget.validate().map_err(io::Error::other)?;
        self.budget = budget;
        self.active = None;
        // Existing oversized files cannot satisfy a lowered per-file budget.
        let oversized: Vec<_> = self
            .files
            .iter()
            .filter(|(_, size)| *size > budget.max_file_bytes)
            .map(|(path, _)| path.clone())
            .collect();
        for path in oversized {
            remove_if_present(&path)?;
            self.files.retain(|(p, _)| *p != path);
        }
        self.enforce(0)
    }
    pub(crate) fn cleanup(&mut self, days: u16) -> io::Result<()> {
        self.active = None;
        crate::logging_runtime::cleanup_directory(&self.directory, days, Utc::now().date_naive())
            .map_err(io::Error::other)?;
        self.files.retain(|(path, _)| path.exists());
        self.enforce(0)
    }
    pub(crate) fn flush_summaries(&mut self) -> io::Result<()> {
        for summary in self.errors.flush(Utc::now()) {
            self.write_record(&json_line(&summary)?)?;
        }
        Ok(())
    }
    fn write_record(&mut self, payload: &[u8]) -> io::Result<()> {
        let payload = if payload.len() as u64 > self.budget.max_file_bytes {
            b"{\"level\":\"WARN\",\"fields\":{\"message\":\"oversized log event omitted\"}}\n"
                .as_slice()
        } else {
            payload
        };
        let length = payload.len() as u64;
        self.enforce(length)?;
        let active_size = self.files.back().map_or(0, |(_, size)| *size);
        if self.active.is_none()
            || self.day != Utc::now().date_naive()
            || active_size + length > self.budget.max_file_bytes
        {
            self.rotate()?;
        }
        if let Some((_, size)) = self.files.back_mut() {
            *size += length;
        }
        self.active
            .as_mut()
            .ok_or_else(|| io::Error::other("missing active log"))?
            .write_all(payload)
    }
    fn enforce(&mut self, incoming: u64) -> io::Result<()> {
        let mut total: u64 = self.files.iter().map(|(_, size)| size).sum();
        while total.saturating_add(incoming) > self.budget.max_directory_bytes {
            let Some((path, size)) = self.files.front().cloned() else {
                break;
            };
            if self.files.len() == 1 {
                self.active = None;
            }
            remove_if_present(&path)?;
            self.files.pop_front();
            total = total.saturating_sub(size);
        }
        Ok(())
    }
    fn rotate(&mut self) -> io::Result<()> {
        self.active = None;
        let today = Utc::now().date_naive();
        if today != self.day {
            self.day = today;
            self.sequence = 0;
        }
        loop {
            if self.sequence > 999_999 {
                return Err(io::Error::other("daily log rotation limit reached"));
            }
            let path = self
                .directory
                .join(format!("tjxy.{}.{:06}.log", self.day, self.sequence));
            self.sequence += 1;
            match OpenOptions::new().create_new(true).write(true).open(&path) {
                Ok(file) => {
                    self.files.push_back((path, 0));
                    self.active = Some(file);
                    return Ok(());
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }
}
impl Write for BoundedLog {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // The nonblocking tracing writer supplies one complete event per buffer.
        // Keep a valid diagnostic record when a single event exceeds its budget.
        let marker =
            b"{\"level\":\"WARN\",\"fields\":{\"message\":\"oversized log event omitted\"}}\n";
        let payload = if bytes.len() as u64 > self.budget.max_file_bytes {
            marker.as_slice()
        } else {
            bytes
        };
        if let Ok(mut record) = serde_json::from_slice::<serde_json::Value>(payload) {
            crate::log_record::sanitize(&mut record);
            let (original, summaries) = self.errors.record(&record, Utc::now());
            for summary in summaries {
                self.write_record(&json_line(&summary)?)?;
            }
            if original {
                let sanitized = json_line(&record)?;
                self.write_record(if sanitized.len() as u64 > self.budget.max_file_bytes {
                    marker
                } else {
                    &sanitized
                })?;
            }
        } else {
            // Never copy unstructured records containing arbitrary backend error strings.
            self.write_record(b"{\"level\":\"WARN\",\"fields\":{\"message\":\"invalid JSON log event omitted\"}}\n")?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.active.as_mut().map_or(Ok(()), Write::flush)
    }
}
fn json_line(value: &serde_json::Value) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_includes_active_file_and_oversized_records() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("notes.txt"), b"keep").unwrap();
        let mut writer = BoundedLog::new(dir.path()).unwrap();
        writer.budget = LogBudget {
            max_file_bytes: 128,
            max_directory_bytes: 256,
        };
        for _ in 0..100 {
            writer
                .write_all(b"{\"message\":\"repeated failure\"}\n")
                .unwrap();
        }
        writer.write_all(&vec![b'x'; 1024]).unwrap();
        writer.flush().unwrap();
        let mut total = 0;
        for entry in fs::read_dir(dir.path()).unwrap().flatten() {
            if log_file_date(&entry.file_name().to_string_lossy()).is_none() {
                continue;
            }
            let content = fs::read_to_string(entry.path()).unwrap();
            assert!(content.len() <= 128);
            total += content.len();
            for line in content.lines() {
                serde_json::from_str::<serde_json::Value>(line).unwrap();
            }
        }
        assert!(total <= 256);
        assert!(dir.path().join("notes.txt").exists());
    }
    #[test]
    fn restart_respects_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut writer = BoundedLog::new(dir.path()).unwrap();
            writer.write_all(b"{}\n").unwrap();
        }
        let mut writer = BoundedLog::new(dir.path()).unwrap();
        writer.write_all(b"{}\n").unwrap();
        assert_eq!(writer.files.len(), 2);
    }
}
