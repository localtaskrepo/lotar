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

#[test]
fn sprint_summary_surfaces_blocked_tasks_and_capacity() {
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
            label: Some("Sprint A".to_string()),
            goal: Some("Deliver feature".to_string()),
            length: Some("1w".to_string()),
            capacity: Some(SprintCapacity {
                points: Some(20),
                hours: Some(40),
            }),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    let created = SprintService::create(&mut storage, sprint, None).expect("create sprint");
    let sprint_id = created.record.id;
    let sprint_id_arg = sprint_id.to_string();

    let mut sprint_doc = created.record.sprint.clone();
    sprint_doc.actual = Some(SprintActual {
        started_at: Some("2025-10-01T09:00:00Z".to_string()),
        ..SprintActual::default()
    });
    SprintService::update(&mut storage, sprint_id, sprint_doc).expect("set sprint actuals");

    let feature = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Complete onboarding flow".to_string(),
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
    .expect("create feature task");

    TaskService::update(
        &mut storage,
        &feature.id,
        TaskUpdate {
            status: Some(TaskStatus::from("Done")),
            effort: Some("5pt".to_string()),
            sprints: Some(vec![sprint_id]),
            ..TaskUpdate::default()
        },
    )
    .expect("update feature task");

    let blocker = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Fix API regression".to_string(),
            project: Some("TEST".to_string()),
            priority: Some(Priority::from("High")),
            task_type: Some(TaskType::from("Bug")),
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
    .expect("create blocked task");

    let blocker_id = blocker.id.clone();

    TaskService::update(
        &mut storage,
        &blocker.id,
        TaskUpdate {
            status: Some(TaskStatus::from("Blocked")),
            assignee: Some("alice@example.com".to_string()),
            effort: Some("4h".to_string()),
            sprints: Some(vec![sprint_id]),
            ..TaskUpdate::default()
        },
    )
    .expect("update blocked task");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "summary", sprint_id_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Sprint summary for #{sprint_id} (Sprint A)."
        )))
        .stdout(predicate::str::contains("Goal: Deliver feature"))
        .stdout(predicate::str::contains("Progress: 2 committed"))
        .stdout(predicate::str::contains("Status highlights:"))
        .stdout(predicate::str::contains(&blocker_id))
        .stdout(predicate::str::contains("Points: 5"))
        .stdout(predicate::str::contains("Hours: 4"))
        .stderr(predicate::str::contains(
            "blocked task(s) require attention",
        ))
        .stderr(predicate::str::contains("Overdue by"));

    let mut json_cmd = common::cargo_bin_in(&fixtures);
    let output = json_cmd
        .args([
            "--format",
            "json",
            "sprint",
            "summary",
            sprint_id_arg.as_str(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&output).expect("parse summary json");

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["metrics"]["blocked"].as_u64(), Some(1));
    let blocked = payload["blocked_tasks"].as_array().expect("blocked array");
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0]["id"].as_str(), Some(blocker_id.as_str()));
}
