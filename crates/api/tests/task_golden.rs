use chrono::{TimeZone, Utc};
use serde_json::json;
use tjxy_api::{AdminTaskJobInfo, AdminTaskJobStatus};
use uuid::Uuid;

#[test]
fn admin_task_job_exposes_only_safe_observation_fields() {
    let id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11").unwrap();
    let scope_id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12").unwrap();
    let created_at = Utc.with_ymd_and_hms(2026, 7, 24, 1, 2, 3).unwrap();
    let dto = AdminTaskJobInfo::new(
        id,
        "ProbeMedia",
        "MediaSource",
        scope_id,
        AdminTaskJobStatus::Retrying,
        100,
        2,
        Some(created_at),
        None,
        None,
        None,
    );

    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        json!({
            "Id": id,
            "TaskKind": "ProbeMedia",
            "ScopeType": "MediaSource",
            "ScopeId": scope_id,
            "Status": "Retrying",
            "Priority": 100,
            "AttemptCount": 2,
            "CreatedAt": "2026-07-24T01:02:03Z",
            "StartedAt": null,
            "CompletedAt": null,
            "Outcome": null
        })
    );
}
