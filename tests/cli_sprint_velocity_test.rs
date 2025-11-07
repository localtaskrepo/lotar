use lotar::api_types::{TaskCreate, TaskUpdate};
use lotar::services::sprint_service::SprintService;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::storage::sprint::{Sprint, SprintActual, SprintCapacity, SprintPlan};
use lotar::types::{Priority, TaskStatus, TaskType};
use lotar::utils::paths;
use predicates::prelude::*;
use serde_json::Value;

mod common;

fn create_task(
    storage: &mut Storage,
    title: &str,
    status: &str,
    effort: Option<&str>,
    sprint_id: u32,
) -> String {
    let task = TaskService::create(
        storage,
        TaskCreate {
            title: title.to_string(),
            project: Some("TEST".to_string()),
            priority: Some(Priority::from("Medium")),
            task_type: Some(TaskType::from("Feature")),
            reporter: None,
            assignee: None,
            due_date: None,
            effort: None,
            description: None,
            tags: Vec::new(),
            relationships: None,
            custom_fields: None,
            sprints: Vec::new(),
        },
    )
    .expect("create task");

    TaskService::update(
        storage,
        &task.id,
        TaskUpdate {
            status: Some(TaskStatus::from(status)),
            effort: effort.map(|value| value.to_string()),
            sprints: Some(vec![sprint_id]),
            ..TaskUpdate::default()
        },
    )
    .expect("update task");

    task.id
}

#[test]
fn sprint_velocity_reports_recent_completed_sprints() {
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

    // Older completed sprint
    let sprint_alpha = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Alpha".to_string()),
            starts_at: Some("2025-09-01T09:00:00Z".to_string()),
            ends_at: Some("2025-09-12T17:00:00Z".to_string()),
            capacity: Some(SprintCapacity {
                points: Some(30),
                hours: None,
            }),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    let alpha_created =
        SprintService::create(&mut storage, sprint_alpha, None).expect("create sprint alpha");
    let alpha_id = alpha_created.record.id;
    let mut alpha_doc = alpha_created.record.sprint.clone();
    alpha_doc.actual = Some(SprintActual {
        started_at: Some("2025-09-01T09:00:00Z".to_string()),
        closed_at: Some("2025-09-12T17:00:00Z".to_string()),
    });
    SprintService::update(&mut storage, alpha_id, alpha_doc).expect("close sprint alpha");

    create_task(
        &mut storage,
        "Ship alpha feature",
        "Done",
        Some("13pt"),
        alpha_id,
    );
    create_task(
        &mut storage,
        "Carry alpha bug",
        "Blocked",
        Some("5pt"),
        alpha_id,
    );

    // Recent completed sprint
    let sprint_beta = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Beta".to_string()),
            starts_at: Some("2025-10-01T09:00:00Z".to_string()),
            ends_at: Some("2025-10-14T17:00:00Z".to_string()),
            capacity: Some(SprintCapacity {
                points: Some(24),
                hours: Some(120),
            }),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    let beta_created =
        SprintService::create(&mut storage, sprint_beta, None).expect("create sprint beta");
    let beta_id = beta_created.record.id;
    let mut beta_doc = beta_created.record.sprint.clone();
    beta_doc.actual = Some(SprintActual {
        started_at: Some("2025-10-01T09:00:00Z".to_string()),
        closed_at: Some("2025-10-15T15:00:00Z".to_string()),
    });
    SprintService::update(&mut storage, beta_id, beta_doc).expect("close sprint beta");

    create_task(
        &mut storage,
        "Finish beta story",
        "Done",
        Some("8pt"),
        beta_id,
    );
    create_task(
        &mut storage,
        "Stretch beta story",
        "InProgress",
        Some("5pt"),
        beta_id,
    );

    // Active sprint (should be skipped by default)
    let sprint_gamma = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Gamma".to_string()),
            starts_at: Some("2025-10-20T09:00:00Z".to_string()),
            length: Some("2w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    let gamma_created =
        SprintService::create(&mut storage, sprint_gamma, None).expect("create sprint gamma");
    let gamma_id = gamma_created.record.id;
    let mut gamma_doc = gamma_created.record.sprint.clone();
    gamma_doc.actual = Some(SprintActual {
        started_at: Some("2025-10-20T09:00:00Z".to_string()),
        ..SprintActual::default()
    });
    SprintService::update(&mut storage, gamma_id, gamma_doc).expect("start sprint gamma");

    create_task(
        &mut storage,
        "Gamma story",
        "InProgress",
        Some("3pt"),
        gamma_id,
    );

    // JSON output defaults to points metric
    let mut json_cmd = common::cargo_bin_in(&fixtures);
    let output = json_cmd
        .args(["--format", "json", "sprint", "velocity"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&output).expect("parse velocity json");

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["metric"].as_str(), Some("points"));
    assert_eq!(payload["count"].as_u64(), Some(2));
    assert_eq!(payload["truncated"].as_bool(), Some(false));
    assert!(payload.get("include_active").is_none());
    assert_eq!(payload["skipped_incomplete"].as_bool(), Some(true));

    let entries = payload["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["summary"]["label"].as_str(), Some("Sprint Beta"));
    assert_eq!(entries[0]["completed"].as_f64(), Some(8.0));
    assert_eq!(
        entries[0]["capacity_consumed_ratio"].as_f64(),
        Some(8.0 / 24.0)
    );
    assert!(entries[0]["window"].as_str().unwrap().contains("->"));

    // Text mode with tasks metric and include active flag
    let mut text_cmd = common::cargo_bin_in(&fixtures);
    text_cmd
        .args([
            "sprint",
            "velocity",
            "--metric",
            "tasks",
            "--include-active",
            "--limit",
            "3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sprint velocity (tasks metric"))
        .stdout(predicate::str::contains("Sprint Beta"))
        .stdout(predicate::str::contains("Sprint Alpha"))
        .stdout(predicate::str::contains("Sprint Gamma"))
        .stderr(predicate::str::is_empty());
}
