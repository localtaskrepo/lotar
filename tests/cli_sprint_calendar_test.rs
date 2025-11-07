use lotar::services::sprint_service::SprintService;
use lotar::storage::manager::Storage;
use lotar::storage::sprint::{Sprint, SprintActual, SprintPlan};
use lotar::utils::paths;
use predicates::prelude::*;
use serde_json::Value;

mod common;

#[test]
fn sprint_calendar_json_reports_timeline() {
    let fixtures = common::TestFixtures::new();

    let config_path = paths::global_config_path(&fixtures.tasks_root);
    std::fs::write(
        &config_path,
        r#"
default.project: TEST
issue.states: [Todo, InProgress, Blocked, Done]
issue.types: [Feature, Bug]
issue.priorities: [Low, Medium, High]
"#,
    )
    .expect("write config");

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    // Upcoming sprint
    let upcoming = Sprint {
        plan: Some(SprintPlan {
            label: Some("Upcoming Sprint".to_string()),
            goal: Some("Ship new analytics panel".to_string()),
            starts_at: Some("2025-11-10T09:00:00Z".to_string()),
            ends_at: Some("2025-11-21T17:00:00Z".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    SprintService::create(&mut storage, upcoming, None).expect("create upcoming sprint");

    // Active but overdue sprint
    let active = Sprint {
        plan: Some(SprintPlan {
            label: Some("Current Sprint".to_string()),
            goal: Some("Burn down backlog".to_string()),
            starts_at: Some("2025-10-10T09:00:00Z".to_string()),
            length: Some("2w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    let active_created =
        SprintService::create(&mut storage, active, None).expect("create active sprint");
    let active_id = active_created.record.id;
    let mut active_doc = active_created.record.sprint.clone();
    active_doc.actual = Some(SprintActual {
        started_at: Some("2025-10-10T09:00:00Z".to_string()),
        ..SprintActual::default()
    });
    SprintService::update(&mut storage, active_id, active_doc).expect("activate sprint");

    // Completed sprint
    let completed = Sprint {
        plan: Some(SprintPlan {
            label: Some("September Sprint".to_string()),
            goal: Some("Stabilize release".to_string()),
            starts_at: Some("2025-09-01T09:00:00Z".to_string()),
            ends_at: Some("2025-09-14T17:00:00Z".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    let completed_created =
        SprintService::create(&mut storage, completed, None).expect("create completed sprint");
    let completed_id = completed_created.record.id;
    let mut completed_doc = completed_created.record.sprint.clone();
    completed_doc.actual = Some(SprintActual {
        started_at: Some("2025-09-01T09:00:00Z".to_string()),
        closed_at: Some("2025-09-14T17:00:00Z".to_string()),
    });
    SprintService::update(&mut storage, completed_id, completed_doc).expect("close sprint");

    let broken = Sprint {
        plan: Some(SprintPlan {
            label: Some("Broken Sprint".to_string()),
            goal: None,
            starts_at: Some("not-a-timestamp".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    SprintService::create(&mut storage, broken, None).expect("create broken sprint");

    let mut cmd = common::cargo_bin_in(&fixtures);
    let output = cmd
        .args([
            "--format",
            "json",
            "sprint",
            "calendar",
            "--include-complete",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&output).expect("parse calendar json");

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["count"].as_u64(), Some(4));
    assert_eq!(payload["truncated"].as_bool(), Some(false));
    assert!(payload.get("skipped_complete").is_none());

    let entries = payload["sprints"].as_array().expect("sprints array");
    assert_eq!(entries.len(), 4);

    let upcoming_entry = entries
        .iter()
        .find(|entry| entry["summary"]["label"].as_str() == Some("Upcoming Sprint"))
        .expect("find upcoming sprint");
    assert!(
        upcoming_entry["relative"]
            .as_str()
            .unwrap()
            .contains("starts"),
        "expected upcoming sprint relative status to mention start"
    );

    let current_entry = entries
        .iter()
        .find(|entry| entry["summary"]["label"].as_str() == Some("Current Sprint"))
        .expect("find current sprint");
    assert!(
        current_entry["relative"]
            .as_str()
            .unwrap()
            .contains("overdue")
            || current_entry["relative"].as_str().unwrap().contains("ends"),
        "expected active sprint relative status to mention progress"
    );

    let completed_entry = entries
        .iter()
        .find(|entry| entry["summary"]["label"].as_str() == Some("September Sprint"))
        .expect("find completed sprint");
    assert!(
        completed_entry["relative"]
            .as_str()
            .unwrap()
            .contains("ended"),
        "expected completed sprint relative status to mention completion"
    );

    let broken_entry = entries
        .iter()
        .find(|entry| entry["summary"]["label"].as_str() == Some("Broken Sprint"))
        .expect("find broken sprint");
    assert_eq!(
        broken_entry["summary"]["has_warnings"].as_bool(),
        Some(true)
    );
    assert_eq!(broken_entry["has_warnings"].as_bool(), Some(true));
    let warnings = broken_entry["status_warnings"]
        .as_array()
        .expect("warnings array");
    assert!(
        warnings
            .iter()
            .any(|warning| warning["code"].as_str() == Some("unparseable_timestamp")),
        "expected unparseable timestamp warning"
    );
    assert!(broken_entry["window"].as_str().is_some());
}

#[test]
fn sprint_calendar_text_highlights_relative_states() {
    let fixtures = common::TestFixtures::new();

    let config_path = paths::global_config_path(&fixtures.tasks_root);
    std::fs::write(
        &config_path,
        r#"
default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature]
issue.priorities: [Low, High]
"#,
    )
    .expect("write config");

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let pending = Sprint {
        plan: Some(SprintPlan {
            label: Some("Calendar Sprint".to_string()),
            starts_at: Some("2025-12-01T09:00:00Z".to_string()),
            ends_at: Some("2025-12-12T17:00:00Z".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    SprintService::create(&mut storage, pending, None).expect("create pending sprint");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "calendar"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sprint calendar:"))
        .stdout(predicate::str::contains("Calendar Sprint"))
        .stdout(predicate::str::contains("starts"));
}
