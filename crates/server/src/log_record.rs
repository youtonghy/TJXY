//! Sanitized, bounded summaries for repeating failures. No unbounded message payloads are retained.
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::collections::HashMap;

const MAX_GROUPS: usize = 1024;
const SUMMARY_SECONDS: i64 = 60;

pub(crate) fn sanitize(value: &mut Value) {
    match value {
        Value::String(text) => *text = redact_urls(text),
        Value::Array(values) => {
            for value in values {
                sanitize(value);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                let key = key.to_ascii_lowercase();
                if [
                    "password",
                    "token",
                    "secret",
                    "authorization",
                    "cookie",
                    "credential",
                ]
                .iter()
                .any(|part| key.contains(part))
                {
                    *value = Value::String("[redacted]".to_owned());
                } else {
                    sanitize(value);
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn redact_urls(text: &str) -> String {
    let mut output = String::with_capacity(text.len().min(4096));
    for (index, word) in text.split_whitespace().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        let lower = word.to_ascii_lowercase();
        if lower.contains("://") || lower.starts_with("bearer") || lower.starts_with("basic") {
            // Free-form authorization messages cannot be reliably tokenized: retain the safe prefix only.
            if lower.starts_with("bearer") || lower.starts_with("basic") {
                output.push_str("[redacted credentials]");
                break;
            }
            output.push_str("[redacted URL]");
        } else if [
            "token=",
            "password=",
            "secret=",
            "api_key=",
            "apikey=",
            "authorization=",
        ]
        .iter()
        .any(|key| lower.contains(key))
        {
            output.push_str("[redacted credential]");
        } else {
            output.push_str(word);
        }
    }
    output
}

#[derive(Default)]
pub(crate) struct ErrorGroups {
    groups: HashMap<String, Group>,
}
struct Group {
    first: DateTime<Utc>,
    last: DateTime<Utc>,
    emitted_at: DateTime<Utc>,
    count: u64,
    pending: u64,
    terminal_jobs: u64,
    context: Value,
}
impl ErrorGroups {
    // Returns whether the original record is needed, plus summaries to write before it.
    pub(crate) fn record(&mut self, value: &Value, now: DateTime<Utc>) -> (bool, Vec<Value>) {
        if value.get("level").and_then(Value::as_str) != Some("ERROR") {
            return (true, Vec::new());
        }
        let fields = value.get("fields").unwrap_or(&Value::Null);
        let span = value.get("span").unwrap_or(&Value::Null);
        let mut context = serde_json::Map::new();
        for name in [
            "scope_id",
            "scope_type",
            "catalog_item_id",
            "media_source_id",
            "storage_object_id",
            "task_kind",
            "stage",
            "expected_revision",
            "error_code",
        ] {
            if let Some(value) = fields.get(name).or_else(|| span.get(name)) {
                context.insert(name.to_owned(), bounded_value(value));
            }
        }
        if let Some(target) = value.get("target") {
            context.insert("target".into(), bounded_value(target));
        }
        if !context.contains_key("error_code") {
            context.insert(
                "message".into(),
                bounded_value(fields.get("message").unwrap_or(&Value::Null)),
            );
        }
        let key = Value::Object(context.clone()).to_string();
        let mut emitted = Vec::new();
        if !self.groups.contains_key(&key) && self.groups.len() >= MAX_GROUPS {
            if let Some(oldest) = self
                .groups
                .iter()
                .min_by_key(|(_, group)| group.last)
                .map(|(key, _)| key.clone())
            {
                if let Some(group) = self.groups.remove(&oldest) {
                    if group.pending > 0 {
                        emitted.push(summary(&group));
                    }
                }
            }
        }
        let terminal = u64::from(fields.get("terminal").and_then(Value::as_bool) == Some(true));
        if let Some(group) = self.groups.get_mut(&key) {
            group.count = group.count.saturating_add(1);
            group.pending = group.pending.saturating_add(1);
            group.terminal_jobs = group.terminal_jobs.saturating_add(terminal);
            group.last = now;
            if (now - group.emitted_at).num_seconds() >= SUMMARY_SECONDS {
                emitted.push(summary(group));
                group.pending = 0;
                group.emitted_at = now;
            }
            return (false, emitted);
        }
        if let Some(job) = fields.get("job_id").or_else(|| span.get("job_id")) {
            context.insert("representative_job_id".into(), bounded_value(job));
        }
        self.groups.insert(
            key,
            Group {
                first: now,
                last: now,
                emitted_at: now,
                count: 1,
                pending: 0,
                terminal_jobs: terminal,
                context: Value::Object(context),
            },
        );
        (true, emitted)
    }
    pub(crate) fn flush(&mut self, now: DateTime<Utc>) -> Vec<Value> {
        let mut output = Vec::new();
        for group in self.groups.values_mut() {
            if group.pending > 0 && (now - group.emitted_at).num_seconds() >= SUMMARY_SECONDS {
                output.push(summary(group));
                group.pending = 0;
                group.emitted_at = now;
            }
        }
        self.groups
            .retain(|_, group| (now - group.last).num_seconds() < 1800 || group.pending > 0);
        output
    }
}
fn bounded_value(value: &Value) -> Value {
    if let Some(text) = value.as_str() {
        Value::String(text.chars().take(256).collect())
    } else if value.is_number() || value.is_boolean() || value.is_null() {
        value.clone()
    } else {
        Value::String("[structured context omitted]".into())
    }
}
fn summary(group: &Group) -> Value {
    json!({"timestamp": Utc::now(), "level":"ERROR", "target":"tjxy_server::error_summary", "fields": {
        "message":"repeated work failures", "first_at":group.first, "last_at":group.last,
        "execution_failures":group.count, "additional_failures":group.pending, "terminal_jobs":group.terminal_jobs,
        "context":group.context
    }})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_failures_flush_counts_and_container_is_bounded() {
        let now = Utc::now();
        let mut groups = ErrorGroups::default();
        let event = json!({"level":"ERROR","fields":{"scope_id":"one","stage":"metadata","expected_revision":1,"error_code":"nfo_conflict","terminal":true,"job_id":"job"}});
        assert!(groups.record(&event, now).0);
        for _ in 0..100 {
            assert!(!groups.record(&event, now).0);
        }
        let summary = groups.flush(now + chrono::Duration::seconds(61));
        assert_eq!(summary[0]["fields"]["execution_failures"], 101);
        assert_eq!(summary[0]["fields"]["terminal_jobs"], 101);
        assert!(groups.flush(now + chrono::Duration::seconds(62)).is_empty());
        for id in 0..2000 {
            groups.record(&json!({"level":"ERROR","fields":{"scope_id":id}}), now);
        }
        assert!(groups.groups.len() <= MAX_GROUPS);
    }
    #[test]
    fn urls_and_structured_credentials_are_removed() {
        let mut value = json!({"message":"failed https://user:password@example.invalid/movie?sig=secret end", "access_token":"secret", "nested":{"authorization":"Bearer private"}});
        sanitize(&mut value);
        let text = value.to_string();
        assert!(!text.contains("password@example"));
        assert!(!text.contains("sig=secret"));
        assert!(!text.contains("Bearer private"));
    }
}
