use chrono::{TimeZone, Utc};
use lotar::services::sprint_status::{SprintLifecycleState, SprintStatusWarning, derive_status};
use lotar::storage::sprint::{Sprint, SprintActual, SprintPlan};

#[test]
fn pending_before_start_uses_planned_start() {
    let plan = SprintPlan {
        label: Some("Sprint 1".to_string()),
        starts_at: Some("2025-10-10T09:00:00Z".to_string()),
        ..SprintPlan::default()
    };
    let sprint = Sprint {
        plan: Some(plan),
        ..Sprint::default()
    };

    let now = Utc.with_ymd_and_hms(2025, 10, 9, 12, 0, 0).unwrap();
    let status = derive_status(&sprint, now);

    assert_eq!(status.state, SprintLifecycleState::Pending);
    assert_eq!(
        status.planned_start,
        Some(Utc.with_ymd_and_hms(2025, 10, 10, 9, 0, 0).unwrap())
    );
    assert!(status.warnings.is_empty());
}

#[test]
fn active_sprint_computes_end_from_length() {
    let plan = SprintPlan {
        length: Some("2w".to_string()),
        ..SprintPlan::default()
    };
    let actual = SprintActual {
        started_at: Some("2025-10-01T09:00:00Z".to_string()),
        ..SprintActual::default()
    };
    let sprint = Sprint {
        plan: Some(plan),
        actual: Some(actual),
        ..Sprint::default()
    };

    let now = Utc.with_ymd_and_hms(2025, 10, 5, 12, 0, 0).unwrap();
    let status = derive_status(&sprint, now);

    assert_eq!(status.state, SprintLifecycleState::Active);
    let expected_end = Utc.with_ymd_and_hms(2025, 10, 15, 9, 0, 0).unwrap();
    assert_eq!(status.computed_end, Some(expected_end));
}

#[test]
fn overdue_when_after_computed_end() {
    let plan = SprintPlan {
        length: Some("1w".to_string()),
        ..SprintPlan::default()
    };
    let actual = SprintActual {
        started_at: Some("2025-09-01T09:00:00Z".to_string()),
        ..SprintActual::default()
    };
    let sprint = Sprint {
        plan: Some(plan),
        actual: Some(actual),
        ..Sprint::default()
    };

    let now = Utc.with_ymd_and_hms(2025, 9, 10, 12, 0, 0).unwrap();
    let status = derive_status(&sprint, now);

    assert_eq!(status.state, SprintLifecycleState::Overdue);
    let expected_end = Utc.with_ymd_and_hms(2025, 9, 8, 9, 0, 0).unwrap();
    assert_eq!(status.computed_end, Some(expected_end));
}

#[test]
fn complete_when_closed() {
    let plan = SprintPlan {
        ends_at: Some("2025-09-08T09:00:00Z".to_string()),
        ..SprintPlan::default()
    };
    let actual = SprintActual {
        started_at: Some("2025-09-01T09:00:00Z".to_string()),
        closed_at: Some("2025-09-07T18:00:00Z".to_string()),
    };
    let sprint = Sprint {
        plan: Some(plan),
        actual: Some(actual),
        ..Sprint::default()
    };

    let now = Utc.with_ymd_and_hms(2025, 9, 7, 20, 0, 0).unwrap();
    let status = derive_status(&sprint, now);

    assert_eq!(status.state, SprintLifecycleState::Complete);
    assert!(status.computed_end.is_some());
}

#[test]
fn warnings_emitted_for_invalid_inputs() {
    let plan = SprintPlan {
        starts_at: Some("not-a-date".to_string()),
        length: Some("abc".to_string()),
        ..SprintPlan::default()
    };
    let sprint = Sprint {
        plan: Some(plan),
        ..Sprint::default()
    };

    let now = Utc.with_ymd_and_hms(2025, 9, 1, 0, 0, 0).unwrap();
    let status = derive_status(&sprint, now);

    assert_eq!(status.state, SprintLifecycleState::Pending);
    assert!(status.computed_end.is_none());
    assert_eq!(status.warnings.len(), 2);
    assert!(
        status
            .warnings
            .contains(&SprintStatusWarning::UnparseableTimestamp {
                field: "plan.starts_at",
                value: "not-a-date".to_string(),
            })
    );
    assert!(
        status
            .warnings
            .contains(&SprintStatusWarning::UnparseableLength {
                field: "plan.length",
                value: "abc".to_string(),
            })
    );
}
