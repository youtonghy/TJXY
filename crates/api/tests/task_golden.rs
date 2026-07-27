use chrono::{TimeZone, Utc};
use serde_json::json;
use tjxy_api::{
    AdminHybridCandidateInfo, AdminHybridCandidatePage, AdminTaskJobInfo, AdminTaskJobStatus,
};
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
            "CompletedAt": null
        })
    );
}

#[test]
fn hybrid_candidate_page_uses_safe_pascal_case_fields() {
    let item_id = Uuid::parse_str("018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12").unwrap();
    let selected_at = Utc.with_ymd_and_hms(2026, 7, 25, 2, 3, 4).unwrap();
    let dto = AdminHybridCandidatePage::new(
        vec![AdminHybridCandidateInfo::new(
            item_id,
            "Pinned Series",
            Some(2026),
            "NotExpanded",
            selected_at,
        )],
        1,
        0,
    );

    assert_eq!(
        serde_json::to_value(dto).unwrap(),
        json!({
            "Items": [{
                "Id": item_id,
                "Name": "Pinned Series",
                "ProductionYear": 2026,
                "StructureState": "NotExpanded",
                "SelectedAt": "2026-07-25T02:03:04Z"
            }],
            "TotalRecordCount": 1,
            "StartIndex": 0
        })
    );
}
