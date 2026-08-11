use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum ScheduledTaskState {
    Idle,
    Running,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ScheduledTaskInfo {
    name: String,
    state: ScheduledTaskState,
    id: Uuid,
    triggers: Vec<ScheduledTaskTrigger>,
    description: String,
    category: String,
    is_hidden: bool,
    key: String,
}

impl ScheduledTaskInfo {
    #[must_use]
    pub fn full_media_scan(id: Uuid, active: bool) -> Self {
        Self {
            name: "Scan Media Library".to_owned(),
            state: if active {
                ScheduledTaskState::Running
            } else {
                ScheduledTaskState::Idle
            },
            id,
            triggers: Vec::new(),
            description: "Scans all enabled media libraries".to_owned(),
            category: "Library".to_owned(),
            is_hidden: false,
            key: "FullMediaScan".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ScheduledTaskTrigger {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum AdminTaskJobStatus {
    Pending,
    Retrying,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum AdminTaskJobOutcome {
    NoMetadataMatch,
    CompletedWithWarnings,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AdminTaskJobInfo {
    id: Uuid,
    task_kind: String,
    scope_type: String,
    scope_id: Uuid,
    status: AdminTaskJobStatus,
    priority: i32,
    attempt_count: i32,
    created_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    outcome: Option<AdminTaskJobOutcome>,
}

impl AdminTaskJobInfo {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        id: Uuid,
        task_kind: impl Into<String>,
        scope_type: impl Into<String>,
        scope_id: Uuid,
        status: AdminTaskJobStatus,
        priority: i32,
        attempt_count: i32,
        created_at: Option<DateTime<Utc>>,
        started_at: Option<DateTime<Utc>>,
        completed_at: Option<DateTime<Utc>>,
        outcome: Option<AdminTaskJobOutcome>,
    ) -> Self {
        Self {
            id,
            task_kind: task_kind.into(),
            scope_type: scope_type.into(),
            scope_id,
            status,
            priority,
            attempt_count,
            created_at,
            started_at,
            completed_at,
            outcome,
        }
    }
}
