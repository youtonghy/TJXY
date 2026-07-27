use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AdminHybridCandidateInfo {
    id: Uuid,
    name: String,
    production_year: Option<i32>,
    structure_state: String,
    selected_at: DateTime<Utc>,
}

impl AdminHybridCandidateInfo {
    #[must_use]
    pub fn new(
        id: Uuid,
        name: impl Into<String>,
        production_year: Option<i32>,
        structure_state: impl Into<String>,
        selected_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            production_year,
            structure_state: structure_state.into(),
            selected_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AdminHybridCandidatePage {
    items: Vec<AdminHybridCandidateInfo>,
    total_record_count: u64,
    start_index: u64,
}

impl AdminHybridCandidatePage {
    #[must_use]
    pub const fn new(
        items: Vec<AdminHybridCandidateInfo>,
        total_record_count: u64,
        start_index: u64,
    ) -> Self {
        Self {
            items,
            total_record_count,
            start_index,
        }
    }
}

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
        }
    }
}
