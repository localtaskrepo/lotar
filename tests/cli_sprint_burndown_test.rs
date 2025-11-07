use lotar::api_types::{TaskCreate, TaskUpdate};
use lotar::services::sprint_service::SprintService;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::storage::sprint::{Sprint, SprintActual, SprintPlan};
use lotar::types::{Priority, TaskStatus, TaskType};
use lotar::utils::paths;
use predicates::prelude::*;
use serde_json::Value;

mod common;

#[test]
fn sprint_burndown_json_reports_totals_and_series() {
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

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Analytics Sprint".to_string()),
            starts_at: Some("2024-05-01T09:00:00Z".to_string()),
            ends_at: Some("2024-05-08T17:00:00Z".to_string()),
            goal: Some("Generate burndown insights".to_string()),
            ..SprintPlan::default()
        }),
        actual: Some(SprintActual {
            started_at: Some("2024-05-01T09:00:00Z".to_string()),
            closed_at: Some("2024-05-08T17:00:00Z".to_string()),
        }),
        ..Sprint::default()
    };

    let created = SprintService::create(&mut storage, sprint, None).expect("create sprint");
    let sprint_id = created.record.id;
    let sprint_id_arg = sprint_id.to_string();

    let _ = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Model velocity".to_string(),
            project: Some("TEST".to_string()),
            priority: Some(Priority::from("Medium")),
            task_type: Some(TaskType::from("Feature")),
            effort: Some("5pt".to_string()),
            sprints: vec![sprint_id],
            ..TaskCreate::default()
        },
    )
    .expect("create task");

    let _ = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Wire burndown dashboard".to_string(),
            project: Some("TEST".to_string()),
            priority: Some(Priority::from("High")),
            task_type: Some(TaskType::from("Feature")),
            effort: Some("3pt".to_string()),
            sprints: vec![sprint_id],
            ..TaskCreate::default()
        },
    )
    .expect("create task");

    let hours_task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Pair on automation".to_string(),
            project: Some("TEST".to_string()),
            priority: Some(Priority::from("Low")),
            task_type: Some(TaskType::from("Bug")),
            effort: Some("4h".to_string()),
            sprints: vec![sprint_id],
            ..TaskCreate::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &hours_task.id,
        TaskUpdate {
            status: Some(TaskStatus::from("InProgress")),
            ..TaskUpdate::default()
        },
    )
    .expect("update task status");

    let mut json_cmd = common::cargo_bin_in(&fixtures);
    let output = json_cmd
        .args([
            "--format",
            "json",
            "sprint",
            "burndown",
            sprint_id_arg.as_str(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&output).expect("parse burndown json");

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["totals"]["tasks"].as_u64(), Some(3));
    assert_eq!(payload["totals"]["points"].as_f64(), Some(8.0));
    assert_eq!(payload["totals"]["hours"].as_f64(), Some(4.0));

    let series = payload["series"].as_array().expect("series array");
    assert!(!series.is_empty());

    let first = &series[0];
    assert_eq!(first["remaining_tasks"].as_u64(), Some(3));
    assert!((first["ideal_tasks"].as_f64().unwrap() - 3.0).abs() < 1e-6);
    assert!((first["remaining_points"].as_f64().unwrap() - 8.0).abs() < 1e-6);
    assert!((first["remaining_hours"].as_f64().unwrap() - 4.0).abs() < 1e-6);

    let last = series.last().expect("last point");
    assert!(last["ideal_tasks"].as_f64().unwrap().abs() <= 1e-6);
    assert!(last["ideal_points"].as_f64().unwrap().abs() <= 1e-6);
    assert!(last["ideal_hours"].as_f64().unwrap().abs() <= 1e-6);
}

#[test]
fn sprint_burndown_text_falls_back_when_no_effort() {
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

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Fallback Sprint".to_string()),
            starts_at: Some("2024-06-01T09:00:00Z".to_string()),
            ends_at: Some("2024-06-08T17:00:00Z".to_string()),
            ..SprintPlan::default()
        }),
        actual: Some(SprintActual {
            started_at: Some("2024-06-01T09:00:00Z".to_string()),
            closed_at: Some("2024-06-08T17:00:00Z".to_string()),
        }),
        ..Sprint::default()
    };

    let created = SprintService::create(&mut storage, sprint, None).expect("create sprint");
    let sprint_id = created.record.id;
    let sprint_id_arg = sprint_id.to_string();

    let task_one = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Sync with stakeholders".to_string(),
            project: Some("TEST".to_string()),
            priority: Some(Priority::from("High")),
            task_type: Some(TaskType::from("Feature")),
            sprints: vec![sprint_id],
            ..TaskCreate::default()
        },
    )
    .expect("create task");

    let task_two = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Collect feedback".to_string(),
            project: Some("TEST".to_string()),
            priority: Some(Priority::from("Low")),
            task_type: Some(TaskType::from("Feature")),
            sprints: vec![sprint_id],
            ..TaskCreate::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &task_one.id,
        TaskUpdate {
            status: Some(TaskStatus::from("InProgress")),
            ..TaskUpdate::default()
        },
    )
    .expect("update task status");

    TaskService::update(
        &mut storage,
        &task_two.id,
        TaskUpdate {
            status: Some(TaskStatus::from("Todo")),
            ..TaskUpdate::default()
        },
    )
    .expect("update task status");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args([
        "sprint",
        "burndown",
        "--metric",
        "points",
        sprint_id_arg.as_str(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(format!(
        "Sprint burndown for #{sprint_id} (Fallback Sprint)."
    )))
    .stdout(predicate::str::contains("Total tasks: 2"))
    .stdout(predicate::str::contains("Ideal horizon"))
    .stderr(predicate::str::contains(
        "No point estimates recorded for this sprint; falling back to tasks.",
    ));
}
