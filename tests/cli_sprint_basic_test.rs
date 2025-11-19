use chrono::{Duration, Utc};
use lotar::config::types::SprintDefaultsConfig;
use lotar::services::sprint_service::SprintService;
use lotar::storage::TaskFilter;
use lotar::storage::manager::Storage;
use lotar::storage::sprint::{Sprint, SprintCapacity, SprintPlan, SprintTaskEntry};
use lotar::types::{Priority, custom_value_string};
use lotar::utils::paths;
use lotar::{Task, TaskStatus};
use predicates::prelude::*;
use serde_json::Value;

mod common;

fn append_sprint_tasks(storage: &mut Storage, sprint_id: u32, task_ids: &[String]) {
    let mut record = SprintService::get(storage, sprint_id).expect("sprint exists");
    let mut tasks = record.sprint.tasks;
    for id in task_ids {
        if tasks.iter().any(|entry| entry.id == *id) {
            continue;
        }
        tasks.push(SprintTaskEntry {
            id: id.clone(),
            order: None,
        });
    }
    record.sprint.tasks = tasks;
    SprintService::update(storage, sprint_id, record.sprint).expect("update sprint");
}

fn sprint_task_ids(storage: &Storage, sprint_id: u32) -> Vec<String> {
    SprintService::get(storage, sprint_id)
        .map(|record| {
            record
                .sprint
                .tasks
                .iter()
                .map(|entry| entry.id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn task_sprint_ids(storage: &Storage, task_id: &str) -> Vec<u32> {
    let mut memberships: Vec<(u32, usize)> = SprintService::list(storage)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|record| {
            record
                .sprint
                .tasks
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.id == task_id)
                .map(|(index, _)| (record.id, index))
        })
        .collect();
    memberships.sort_by(|(sprint_a, index_a), (sprint_b, index_b)| {
        sprint_a.cmp(sprint_b).then_with(|| index_a.cmp(index_b))
    });
    memberships
        .into_iter()
        .map(|(sprint_id, _)| sprint_id)
        .collect()
}

#[test]
fn sprint_list_reports_empty_state() {
    let fixtures = common::TestFixtures::new();

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No sprints found."));
}

#[test]
fn sprint_list_and_show_surface_created_sprint() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint 42".to_string()),
            goal: Some("Ship onboarding".to_string()),
            starts_at: Some("2030-10-10T09:00:00Z".to_string()),
            ends_at: Some("2030-10-24T17:00:00Z".to_string()),
            notes: Some("Kickoff Monday".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    let outcome = SprintService::create(&mut storage, sprint, None).expect("create sprint");
    assert_eq!(outcome.record.id, 1);

    // list should show the sprint with pending status and label
    let mut list_cmd = common::cargo_bin_in(&fixtures);
    list_cmd
        .args(["sprint", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sprint 42"))
        .stdout(predicate::str::contains("[pending]"));

    // show should print the detailed output including goal and notes
    let mut show_cmd = common::cargo_bin_in(&fixtures);
    show_cmd
        .args(["sprint", "show", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sprint 1 - Sprint 42 [pending]"))
        .stdout(predicate::str::contains("Goal: Ship onboarding"))
        .stdout(predicate::str::contains("notes:"));
}

#[test]
fn sprint_create_applies_defaults_from_global_config() {
    let fixtures = common::TestFixtures::new();

    let config_path = paths::global_config_path(&fixtures.tasks_root);
    std::fs::write(
        &config_path,
        r#"
default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug]
issue.priorities: [Low, Medium, High]
sprints:
  defaults:
    capacity_points: 30
    length: "2w"
"#,
    )
    .expect("write config");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "create", "--label", "Sprint 42"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created sprint #1 (Sprint 42)."))
        .stdout(predicate::str::contains("Applied sprint defaults:"))
        .stdout(predicate::str::contains("capacity_points"))
        .stdout(predicate::str::contains("length"));

    let sprint_file = fixtures.tasks_root.join("@sprints/1.yml");
    let contents = std::fs::read_to_string(&sprint_file).expect("sprint file written");
    assert!(contents.contains("points: 30"));
    assert!(contents.contains("length: 2w"));
}

#[test]
fn sprint_create_surfaces_conflicting_default_warning() {
    let fixtures = common::TestFixtures::new();

    let config_path = paths::global_config_path(&fixtures.tasks_root);
    std::fs::write(
        &config_path,
        r#"
default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug]
issue.priorities: [Low, Medium, High]
sprints:
  defaults:
    length: "2w"
"#,
    )
    .expect("write config");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args([
        "sprint",
        "create",
        "--label",
        "Sprint 99",
        "--ends-at",
        "2025-10-24T17:00:00Z",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Created sprint #1 (Sprint 99)."))
    .stderr(predicate::str::contains(
        "plan.length was ignored because plan.ends_at was provided.",
    ));

    let sprint_file = fixtures.tasks_root.join("@sprints/1.yml");
    let contents = std::fs::read_to_string(&sprint_file).expect("sprint file written");
    assert!(contents.contains("ends_at: 2025-10-24T17:00:00Z"));
    assert!(!contents.contains("length:"));
}

#[test]
fn sprint_create_respects_no_defaults_flag() {
    let fixtures = common::TestFixtures::new();

    let config_path = paths::global_config_path(&fixtures.tasks_root);
    std::fs::write(
        &config_path,
        r#"
default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug]
issue.priorities: [Low, Medium, High]
sprints:
  defaults:
    capacity_points: 12
    length: "1w"
"#,
    )
    .expect("write config");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "create", "--label", "Sprint 10", "--no-defaults"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created sprint #1 (Sprint 10)."))
        .stdout(predicate::str::contains("Applied sprint defaults:").not());

    let sprint_file = fixtures.tasks_root.join("@sprints/1.yml");
    let contents = std::fs::read_to_string(&sprint_file).expect("sprint file written");
    assert!(!contents.contains("points:"));
    assert!(!contents.contains("length:"));
}

#[test]
fn sprint_service_applies_defaults_for_direct_callers() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let defaults = SprintDefaultsConfig {
        capacity_points: Some(18),
        capacity_hours: None,
        length: Some("1w".to_string()),
        overdue_after: Some("8h".to_string()),
    };

    let outcome = SprintService::create(&mut storage, Sprint::default(), Some(&defaults))
        .expect("create sprint with defaults");

    assert_eq!(outcome.record.id, 1);
    assert_eq!(
        outcome.applied_defaults,
        vec![
            "capacity_points".to_string(),
            "length".to_string(),
            "overdue_after".to_string()
        ]
    );

    let sprint_file = fixtures.tasks_root.join("@sprints/1.yml");
    let contents = std::fs::read_to_string(&sprint_file).expect("sprint file written");
    assert!(contents.contains("points: 18"));
    assert!(contents.contains("length: 1w"));
    assert!(contents.contains("overdue_after: 8h"));
}

#[test]
fn sprint_update_modifies_plan_and_actual_fields() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint 7".to_string()),
            goal: Some("Initial goal".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    let outcome = SprintService::create(&mut storage, sprint, None).expect("create sprint");
    assert_eq!(outcome.record.id, 1);

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args([
        "sprint",
        "update",
        "1",
        "--goal",
        "Updated goal",
        "--actual-started-at",
        "2025-10-01T09:00:00Z",
        "--length",
        "2w",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Updated sprint #1 (Sprint 7)."))
    .stdout(predicate::str::contains("Computed end:"));

    let sprint_file = fixtures.tasks_root.join("@sprints/1.yml");
    let contents = std::fs::read_to_string(&sprint_file).expect("sprint file written");
    assert!(contents.contains("goal: Updated goal"));
    assert!(contents.contains("started_at: 2025-10-01T09:00:00Z"));
    assert!(contents.contains("length: 2w"));
}

#[test]
fn sprint_start_auto_selects_pending_sprint() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint_one = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Alpha".to_string()),
            starts_at: Some("2000-01-01T09:00:00Z".to_string()),
            length: Some("2w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    let sprint_two = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Beta".to_string()),
            starts_at: Some("2035-01-01T09:00:00Z".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint_one, None).expect("create sprint one");
    SprintService::create(&mut storage, sprint_two, None).expect("create sprint two");

    let start_at = "2025-10-01T09:00:00Z";

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "start", "--at", start_at])
        .assert()
        .success()
        .stdout(predicate::str::contains("Auto-selected sprint #1"))
        .stdout(predicate::str::contains(
            "Started sprint #1 (Sprint Alpha).",
        ))
        .stdout(predicate::str::contains(
            "Started at: 2025-10-01T09:00:00+00:00",
        ));

    let sprint_file = fixtures.tasks_root.join("@sprints/1.yml");
    let contents = std::fs::read_to_string(&sprint_file).expect("sprint file written");
    assert!(contents.contains("started_at: 2025-10-01T09:00:00+00:00"));

    let sprint_two_file = fixtures.tasks_root.join("@sprints/2.yml");
    let contents_two = std::fs::read_to_string(&sprint_two_file).expect("sprint file written");
    assert!(!contents_two.contains("started_at:"));
}

#[test]
fn sprint_close_defaults_to_last_active_sprint() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Gamma".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    let mut cmd_start = common::cargo_bin_in(&fixtures);
    cmd_start
        .args(["sprint", "start", "1", "--at", "2025-10-01T09:00:00Z"])
        .assert()
        .success();

    let mut cmd_close = common::cargo_bin_in(&fixtures);
    cmd_close
        .args(["sprint", "close", "--at", "2025-10-15T17:00:00Z"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Auto-selected sprint #1"))
        .stdout(predicate::str::contains("Closed sprint #1 (Sprint Gamma)."))
        .stdout(predicate::str::contains(
            "Closed at: 2025-10-15T17:00:00+00:00",
        ));

    let sprint_file = fixtures.tasks_root.join("@sprints/1.yml");
    let contents = std::fs::read_to_string(&sprint_file).expect("sprint file written");
    assert!(contents.contains("closed_at: 2025-10-15T17:00:00+00:00"));
}

#[test]
fn sprint_start_warns_about_parallel_active_sprints() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint_alpha = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Alpha".to_string()),
            length: Some("2w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    let sprint_beta = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Beta".to_string()),
            length: Some("2w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint_alpha, None).expect("create sprint alpha");
    SprintService::create(&mut storage, sprint_beta, None).expect("create sprint beta");

    let mut start_alpha = common::cargo_bin_in(&fixtures);
    start_alpha
        .args(["sprint", "start", "1", "--at", "2025-10-01T09:00:00Z"])
        .assert()
        .success();

    let mut start_beta = common::cargo_bin_in(&fixtures);
    start_beta
        .args(["sprint", "start", "2", "--at", "2025-10-05T09:00:00Z"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Started sprint #2 (Sprint Beta).",
        ))
        .stderr(predicate::str::contains(
            "Another sprint is still running: #1 (Sprint Alpha) is active. Close it before starting another sprint or pass --force if you intend to overlap.",
        ));
}

#[test]
fn sprint_close_warns_about_remaining_active_sprints() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint_alpha = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Alpha".to_string()),
            length: Some("2w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    let sprint_beta = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Beta".to_string()),
            length: Some("2w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint_alpha, None).expect("create sprint alpha");
    SprintService::create(&mut storage, sprint_beta, None).expect("create sprint beta");

    let mut start_alpha = common::cargo_bin_in(&fixtures);
    start_alpha
        .args(["sprint", "start", "1", "--at", "2025-10-01T09:00:00Z"])
        .assert()
        .success();

    let mut start_beta = common::cargo_bin_in(&fixtures);
    start_beta
        .args(["sprint", "start", "2", "--at", "2025-10-05T09:00:00Z"])
        .assert()
        .success();

    let mut close_alpha = common::cargo_bin_in(&fixtures);
    close_alpha
        .args(["sprint", "close", "1", "--at", "2025-10-10T17:00:00Z"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Closed sprint #1 (Sprint Alpha)."))
        .stderr(predicate::str::contains(
            "Additional sprints remain active: #2 (Sprint Beta) is active.",
        ));
}

#[test]
fn sprint_start_warns_when_start_time_far_in_future() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Delta".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    let future_start = (Utc::now() + Duration::hours(24)).to_rfc3339();

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "start", "1", "--at"])
        .arg(&future_start)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Started sprint #1 (Sprint Delta).",
        ))
        .stderr(predicate::str::contains("more than 12 hours in the future"));

    let later_start = (Utc::now() + Duration::hours(36)).to_rfc3339();

    let mut cmd_force = common::cargo_bin_in(&fixtures);
    cmd_force
        .args(["sprint", "start", "1", "--at"])
        .arg(&later_start)
        .arg("--force")
        .assert()
        .success()
        .stderr(predicate::str::contains("more than 12 hours in the future").not());
}

#[test]
fn sprint_start_warns_when_planned_start_is_overdue() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let planned_start = (Utc::now() - Duration::hours(36)).to_rfc3339();

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Overdue Start".to_string()),
            starts_at: Some(planned_start.clone()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "start", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Started sprint #1 (Sprint Overdue Start).",
        ))
        .stderr(predicate::str::contains("was scheduled to start at"))
        .stderr(predicate::str::contains(&planned_start));
}

#[test]
fn sprint_close_warns_when_past_planned_end() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let planned_start = Utc::now() - Duration::days(14);
    let planned_start_str = planned_start.to_rfc3339();
    let computed_end = (planned_start + Duration::days(7)).to_rfc3339();

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Overdue Close".to_string()),
            starts_at: Some(planned_start_str.clone()),
            length: Some("1w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    let mut cmd_start = common::cargo_bin_in(&fixtures);
    cmd_start
        .args(["sprint", "start", "1", "--at", &planned_start_str])
        .assert()
        .success();

    let close_at = Utc::now().to_rfc3339();

    let mut cmd_close = common::cargo_bin_in(&fixtures);
    cmd_close
        .args(["sprint", "close", "1", "--at"])
        .arg(&close_at)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Closed sprint #1 (Sprint Overdue Close).",
        ))
        .stderr(predicate::str::contains("was scheduled to end by"))
        .stderr(predicate::str::contains(&computed_end));
}

#[test]
fn sprint_parallel_warnings_can_be_disabled_via_config() {
    let fixtures = common::TestFixtures::new();

    let config_path = paths::global_config_path(&fixtures.tasks_root);
    std::fs::write(
        &config_path,
        r#"
default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug]
issue.priorities: [Low, Medium, High]
sprints:
  notifications:
    enabled: false
"#,
    )
    .expect("write config");

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint_alpha = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Alpha".to_string()),
            length: Some("2w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    let sprint_beta = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Beta".to_string()),
            length: Some("2w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint_alpha, None).expect("create sprint alpha");
    SprintService::create(&mut storage, sprint_beta, None).expect("create sprint beta");

    let mut start_alpha = common::cargo_bin_in(&fixtures);
    start_alpha
        .args(["sprint", "start", "1", "--at", "2025-10-01T09:00:00Z"])
        .assert()
        .success();

    let mut start_beta = common::cargo_bin_in(&fixtures);
    start_beta
        .args(["sprint", "start", "2", "--at", "2025-10-05T09:00:00Z"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Started sprint #2 (Sprint Beta)."))
        .stderr(predicate::str::contains("Another sprint is still running").not());
}

#[test]
fn sprint_no_warn_suppresses_lifecycle_warnings() {
    let fixtures = common::TestFixtures::new();
    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint_alpha = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Alpha".to_string()),
            length: Some("1w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    let sprint_beta = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Beta".to_string()),
            length: Some("1w".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint_alpha, None).expect("create sprint alpha");
    SprintService::create(&mut storage, sprint_beta, None).expect("create sprint beta");

    let start_alpha_at = Utc::now() - Duration::hours(1);
    let mut start_alpha = common::cargo_bin_in(&fixtures);
    start_alpha
        .args(["sprint", "start", "1", "--at"])
        .arg(start_alpha_at.to_rfc3339())
        .assert()
        .success();

    let future_start = (Utc::now() + Duration::hours(24)).to_rfc3339();

    let mut start_beta = common::cargo_bin_in(&fixtures);
    start_beta
        .args(["sprint", "start", "2", "--at"])
        .arg(&future_start)
        .arg("--no-warn")
        .assert()
        .success()
        .stdout(predicate::str::contains("Started sprint #2 (Sprint Beta)."))
        .stderr(predicate::str::contains("Another sprint").not())
        .stderr(predicate::str::contains("more than 12 hours in the future").not());

    let close_time = (Utc::now() + Duration::days(14)).to_rfc3339();

    let mut close_beta = common::cargo_bin_in(&fixtures);
    close_beta
        .args(["sprint", "close", "2", "--at"])
        .arg(&close_time)
        .arg("--no-warn")
        .assert()
        .success()
        .stderr(predicate::str::contains("was scheduled to end by").not())
        .stderr(predicate::str::contains("Additional sprints remain active").not());
}

#[test]
fn sprint_review_reports_remaining_work_and_status_breakdown() {
    let fixtures = common::TestFixtures::new();

    let config_path = paths::global_config_path(&fixtures.tasks_root);
    std::fs::write(
        &config_path,
        r#"
default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature]
issue.priorities: [Medium]
"#,
    )
    .expect("write config");

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Review".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    let mut start_cmd = common::cargo_bin_in(&fixtures);
    start_cmd
        .args(["sprint", "start", "1", "--at", "2025-01-01T09:00:00Z"])
        .assert()
        .success();

    let mut close_cmd = common::cargo_bin_in(&fixtures);
    close_cmd
        .args(["sprint", "close", "1", "--at", "2025-01-14T17:00:00Z"])
        .assert()
        .success();

    let mut task_done = Task::new(
        fixtures.tasks_root.clone(),
        "Finish release notes".to_string(),
        Priority::from("Medium"),
    );
    task_done.status = TaskStatus::from("Done");
    let task_done_id = storage
        .add(&task_done, "TEST", Some("Test Project"))
        .expect("failed to add done task");

    let mut task_in_progress = Task::new(
        fixtures.tasks_root.clone(),
        "Implement feature".to_string(),
        Priority::from("Medium"),
    );
    task_in_progress.status = TaskStatus::from("InProgress");
    let task_in_progress_id = storage
        .add(&task_in_progress, "TEST", Some("Test Project"))
        .expect("failed to add in-progress task");

    let mut task_todo = Task::new(
        fixtures.tasks_root.clone(),
        "Backlog refinement".to_string(),
        Priority::from("Medium"),
    );
    task_todo.status = TaskStatus::from("Todo");
    let task_todo_id = storage
        .add(&task_todo, "TEST", Some("Test Project"))
        .expect("failed to add todo task");

    let task_ids = vec![task_done_id, task_in_progress_id, task_todo_id];
    append_sprint_tasks(&mut storage, 1, &task_ids);

    let mut review_cmd = common::cargo_bin_in(&fixtures);
    review_cmd
        .args(["sprint", "review", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Sprint review for #1 (Sprint Review).",
        ))
        .stdout(predicate::str::contains(
            "Tasks: 3 total • 1 done • 2 remaining",
        ))
        .stdout(predicate::str::contains("Status breakdown:"))
        .stdout(predicate::str::contains("Implement feature [InProgress]"))
        .stdout(predicate::str::contains("Backlog refinement [Todo]"));
}

#[test]
fn sprint_stats_reports_capacity_and_timeline() {
    let fixtures = common::TestFixtures::new();

    let config_path = paths::global_config_path(&fixtures.tasks_root);
    std::fs::write(
        &config_path,
        r#"
default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature]
issue.priorities: [Medium]
"#,
    )
    .expect("write config");

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Stats".to_string()),
            starts_at: Some("2025-01-01T09:00:00Z".to_string()),
            ends_at: Some("2025-01-08T09:00:00Z".to_string()),
            capacity: Some(SprintCapacity {
                points: Some(30),
                hours: Some(80),
            }),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    drop(storage);

    let mut start_cmd = common::cargo_bin_in(&fixtures);
    start_cmd
        .args(["sprint", "start", "1", "--at", "2025-01-01T09:00:00Z"])
        .assert()
        .success();

    let mut close_cmd = common::cargo_bin_in(&fixtures);
    close_cmd
        .args(["sprint", "close", "1", "--at", "2025-01-08T09:00:00Z"])
        .assert()
        .success();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let mut done_hours_task = Task::new(
        fixtures.tasks_root.clone(),
        "Complete API work".to_string(),
        Priority::from("Medium"),
    );
    done_hours_task.status = TaskStatus::from("Done");
    done_hours_task.effort = Some("8h".to_string());
    let done_hours_id = storage
        .add(&done_hours_task, "TEST", Some("Test Project"))
        .expect("failed to add done hours task");

    let mut remaining_hours_task = Task::new(
        fixtures.tasks_root.clone(),
        "Continue UI polish".to_string(),
        Priority::from("Medium"),
    );
    remaining_hours_task.status = TaskStatus::from("InProgress");
    remaining_hours_task.effort = Some("4h".to_string());
    let remaining_hours_id = storage
        .add(&remaining_hours_task, "TEST", Some("Test Project"))
        .expect("failed to add remaining hours task");

    let mut done_points_task = Task::new(
        fixtures.tasks_root.clone(),
        "Deploy landing page".to_string(),
        Priority::from("Medium"),
    );
    done_points_task.status = TaskStatus::from("Done");
    done_points_task.effort = Some("3pt".to_string());
    let done_points_id = storage
        .add(&done_points_task, "TEST", Some("Test Project"))
        .expect("failed to add done points task");

    let mut remaining_points_task = Task::new(
        fixtures.tasks_root.clone(),
        "Write documentation".to_string(),
        Priority::from("Medium"),
    );
    remaining_points_task.status = TaskStatus::from("Todo");
    remaining_points_task.effort = Some("5pt".to_string());
    let remaining_points_id = storage
        .add(&remaining_points_task, "TEST", Some("Test Project"))
        .expect("failed to add remaining points task");

    let task_ids = vec![
        done_hours_id,
        remaining_hours_id,
        done_points_id,
        remaining_points_id,
    ];
    append_sprint_tasks(&mut storage, 1, &task_ids);

    drop(storage);

    let mut stats_cmd = common::cargo_bin_in(&fixtures);
    stats_cmd
        .args(["sprint", "stats", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Sprint stats for #1 (Sprint Stats).",
        ))
        .stdout(predicate::str::contains(
            "Tasks: 4 committed • 2 done • 2 remaining (50.0% complete)",
        ))
        .stdout(predicate::str::contains(
            "Points: 8 committed • 3 done • 5 remaining (37.5% complete)",
        ))
        .stdout(predicate::str::contains(
            "  Capacity: 30 planned • 26.7% committed • 10.0% consumed",
        ))
        .stdout(predicate::str::contains(
            "Hours: 12 committed • 8 done • 4 remaining (66.7% complete)",
        ))
        .stdout(predicate::str::contains(
            "  Capacity: 80 planned • 15.0% committed • 10.0% consumed",
        ))
        .stdout(predicate::str::contains("Timeline:"))
        .stdout(predicate::str::contains(
            "  Planned start: 2025-01-01T09:00:00+00:00",
        ))
        .stdout(predicate::str::contains(
            "  Actual end: 2025-01-08T09:00:00+00:00",
        ))
        .stdout(predicate::str::contains("  Actual duration: 7d"));
}

#[test]
fn sprint_review_auto_selects_latest_completed_when_id_omitted() {
    let fixtures = common::TestFixtures::new();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint_one = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint One".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    let sprint_two = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Two".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint_one, None).expect("create sprint one");
    SprintService::create(&mut storage, sprint_two, None).expect("create sprint two");

    let mut start_first = common::cargo_bin_in(&fixtures);
    start_first
        .args(["sprint", "start", "1", "--at", "2024-01-01T09:00:00Z"])
        .assert()
        .success();

    let mut close_first = common::cargo_bin_in(&fixtures);
    close_first
        .args(["sprint", "close", "1", "--at", "2024-01-07T17:00:00Z"])
        .assert()
        .success();

    let mut start_second = common::cargo_bin_in(&fixtures);
    start_second
        .args(["sprint", "start", "2", "--at", "2024-02-01T09:00:00Z"])
        .assert()
        .success();

    let mut close_second = common::cargo_bin_in(&fixtures);
    close_second
        .args(["sprint", "close", "2", "--at", "2024-02-14T17:00:00Z"])
        .assert()
        .success();

    let mut review_cmd = common::cargo_bin_in(&fixtures);
    review_cmd
        .args(["sprint", "review"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Auto-selected sprint #2 for review.",
        ))
        .stdout(predicate::str::contains(
            "Sprint review for #2 (Sprint Two).",
        ))
        .stdout(predicate::str::contains(
            "No tasks are linked to this sprint.",
        ));
}

#[test]
fn sprint_cleanup_refs_prunes_missing_memberships() {
    let fixtures = common::TestFixtures::new();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Present".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };

    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    let mut task = Task::new(
        fixtures.tasks_root.clone(),
        "Task with dangling sprint".to_string(),
        Priority::from("Medium"),
    );
    task.status = TaskStatus::from("Todo");
    task.sprints = vec![1, 99];
    let task_id = storage
        .add(&task, "TEST", Some("Test Project"))
        .expect("failed to add sprint cleanup task");

    drop(storage);

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "cleanup-refs"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Removed 1 sprint reference(s) across 1 task(s).",
        ))
        .stdout(predicate::str::contains(
            "Sprint #99: removed 1 reference(s).",
        ));

    let storage_after = Storage::new(fixtures.tasks_root.clone());
    let tasks = storage_after.search(&TaskFilter::default());
    let (_, updated_task) = tasks
        .into_iter()
        .find(|(id, _)| id == &task_id)
        .expect("task still exists");

    assert!(updated_task.sprints.is_empty());
    let memberships = task_sprint_ids(&storage_after, &task_id);
    assert_eq!(memberships, vec![1]);
}

#[test]
fn sprint_cleanup_refs_targets_requested_sprint() {
    let fixtures = common::TestFixtures::new();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let mut task = Task::new(
        fixtures.tasks_root.clone(),
        "Task with multiple missing sprints".to_string(),
        Priority::from("Medium"),
    );
    task.status = TaskStatus::from("Todo");
    task.sprints = vec![5, 6];
    let task_id = storage
        .add(&task, "TEST", Some("Test Project"))
        .expect("failed to add cleanup target");

    drop(storage);

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "cleanup-refs", "5"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Removed 2 sprint reference(s) across 1 task(s).",
        ))
        .stdout(predicate::str::contains(
            "Sprint #5: removed 1 reference(s).",
        ))
        .stdout(predicate::str::contains(
            "Sprint #6: removed 1 reference(s).",
        ));

    let storage_after = Storage::new(fixtures.tasks_root.clone());
    let tasks = storage_after.search(&TaskFilter::default());
    let (_, updated_task) = tasks
        .into_iter()
        .find(|(id, _)| id == &task_id)
        .expect("task still exists");

    assert!(updated_task.sprints.is_empty());
    let memberships = task_sprint_ids(&storage_after, &task_id);
    assert!(memberships.is_empty());
}

#[test]
fn sprint_add_assigns_tasks_to_sprint() {
    let fixtures = common::TestFixtures::new();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Attach".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    let mut task_a = Task::new(
        fixtures.tasks_root.clone(),
        "Attach Alpha".to_string(),
        Priority::from("Medium"),
    );
    task_a.status = TaskStatus::from("Todo");
    let mut task_b = Task::new(
        fixtures.tasks_root.clone(),
        "Attach Beta".to_string(),
        Priority::from("Medium"),
    );
    task_b.status = TaskStatus::from("Todo");

    let id_a = storage
        .add(&task_a, "TEST", Some("Test Project"))
        .expect("failed to add task a");
    let id_b = storage
        .add(&task_b, "TEST", Some("Test Project"))
        .expect("failed to add task b");
    drop(storage);

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "add", "--sprint", "1", &id_a, &id_b])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Attached sprint #1 (Sprint Attach) to 2 task(s).",
        ));

    let storage_after = Storage::new(fixtures.tasks_root.clone());
    let task_a_after = storage_after
        .get(&id_a, "TEST".to_string())
        .expect("task a exists");
    assert!(task_a_after.sprints.is_empty());
    let task_b_after = storage_after
        .get(&id_b, "TEST".to_string())
        .expect("task b exists");
    assert!(task_b_after.sprints.is_empty());
    let sprint_members = sprint_task_ids(&storage_after, 1);
    assert!(sprint_members.contains(&id_a));
    assert!(sprint_members.contains(&id_b));
}

#[test]
fn sprint_add_requires_force_for_existing_membership() {
    let fixtures = common::TestFixtures::new();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint_one = Sprint {
        plan: Some(SprintPlan {
            label: Some("Alpha".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    let sprint_two = Sprint {
        plan: Some(SprintPlan {
            label: Some("Beta".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    SprintService::create(&mut storage, sprint_one, None).expect("create sprint #1");
    SprintService::create(&mut storage, sprint_two, None).expect("create sprint #2");

    let mut task = Task::new(
        fixtures.tasks_root.clone(),
        "Already Assigned".to_string(),
        Priority::from("Medium"),
    );
    task.status = TaskStatus::from("Todo");
    let task_id = storage
        .add(&task, "TEST", Some("Test Project"))
        .expect("failed to add sprint-assigned task");
    let mut sprint_one_record = SprintService::get(&storage, 1).expect("load sprint #1");
    sprint_one_record.sprint.tasks.push(SprintTaskEntry {
        id: task_id.clone(),
        order: None,
    });
    SprintService::update(&mut storage, sprint_one_record.id, sprint_one_record.sprint)
        .expect("persist sprint membership");
    drop(storage);

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "add", "--sprint", "2", &task_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Attached sprint #2 (Beta) to 1 task(s).",
        ));

    let storage_after = Storage::new(fixtures.tasks_root.clone());
    let fetched = storage_after
        .get(&task_id, "TEST".to_string())
        .expect("task still exists");
    assert!(fetched.sprints.is_empty());
    let memberships = task_sprint_ids(&storage_after, &task_id);
    assert_eq!(memberships, vec![1, 2]);
    drop(storage_after);

    let mut cmd_force = common::cargo_bin_in(&fixtures);
    cmd_force
        .args(["sprint", "add", "--force", "--sprint", "2", &task_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Attached sprint #2 (Beta) to 1 task(s).",
        ))
        .stdout(predicate::str::contains("moved from sprint(s) #1"));

    let storage_final = Storage::new(fixtures.tasks_root.clone());
    let updated = storage_final
        .get(&task_id, "TEST".to_string())
        .expect("task updated");
    assert!(updated.sprints.is_empty());
    let memberships = task_sprint_ids(&storage_final, &task_id);
    assert_eq!(memberships, vec![2]);
}

#[test]
fn sprint_move_reassigns_memberships() {
    let fixtures = common::TestFixtures::new();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint_one = Sprint {
        plan: Some(SprintPlan {
            label: Some("Alpha".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    let sprint_two = Sprint {
        plan: Some(SprintPlan {
            label: Some("Release".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    SprintService::create(&mut storage, sprint_one, None).expect("create sprint #1");
    SprintService::create(&mut storage, sprint_two, None).expect("create sprint #2");

    let mut task = Task::new(
        fixtures.tasks_root.clone(),
        "Move Me".to_string(),
        Priority::from("Medium"),
    );
    task.status = TaskStatus::from("Todo");
    let task_id = storage
        .add(&task, "TEST", Some("Test Project"))
        .expect("failed to add task for sprint move");
    let mut sprint_one_record = SprintService::get(&storage, 1).expect("load sprint #1");
    sprint_one_record.sprint.tasks.push(SprintTaskEntry {
        id: task_id.clone(),
        order: None,
    });
    SprintService::update(&mut storage, sprint_one_record.id, sprint_one_record.sprint)
        .expect("persist sprint membership");
    drop(storage);

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "move", "--sprint", "2", &task_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Moved 1 task(s) to sprint #2 (Release).",
        ))
        .stdout(predicate::str::contains("moved from sprint(s) #1"));

    let storage_after = Storage::new(fixtures.tasks_root.clone());
    let updated = storage_after
        .get(&task_id, "TEST".to_string())
        .expect("task updated");
    assert!(updated.sprints.is_empty());
    let memberships = task_sprint_ids(&storage_after, &task_id);
    assert_eq!(memberships, vec![2]);
}

#[test]
fn sprint_move_json_reports_reassignment_messages() {
    let fixtures = common::TestFixtures::new();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint_one = Sprint {
        plan: Some(SprintPlan {
            label: Some("Alpha".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    let sprint_two = Sprint {
        plan: Some(SprintPlan {
            label: Some("Release".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    SprintService::create(&mut storage, sprint_one, None).expect("create sprint #1");
    SprintService::create(&mut storage, sprint_two, None).expect("create sprint #2");

    let mut task = Task::new(
        fixtures.tasks_root.clone(),
        "Move Me".to_string(),
        Priority::from("Medium"),
    );
    task.status = TaskStatus::from("Todo");
    let task_id = storage
        .add(&task, "TEST", Some("Test Project"))
        .expect("failed to add task for sprint move json");
    let mut sprint_one_record = SprintService::get(&storage, 1).expect("load sprint #1");
    sprint_one_record.sprint.tasks.push(SprintTaskEntry {
        id: task_id.clone(),
        order: None,
    });
    SprintService::update(&mut storage, sprint_one_record.id, sprint_one_record.sprint)
        .expect("persist sprint membership");
    drop(storage);

    let mut cmd = common::cargo_bin_in(&fixtures);
    let assert = cmd
        .args([
            "--format", "json", "sprint", "move", "--sprint", "2", &task_id,
        ])
        .assert()
        .success();
    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    let payload: Value = serde_json::from_str(output.trim()).expect("valid json payload");

    let messages = payload
        .get("messages")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(messages.iter().any(|message| {
        message
            .as_str()
            .map(|text| text.contains("moved from sprint(s) #1"))
            .unwrap_or(false)
    }));
}

#[test]
fn sprint_add_appends_select_filter_results() {
    let fixtures = common::TestFixtures::new();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Select".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    let mut manual = Task::new(
        fixtures.tasks_root.clone(),
        "Manual".to_string(),
        Priority::from("Medium"),
    );
    manual.status = TaskStatus::from("Todo");
    let manual_id = storage
        .add(&manual, "TEST", Some("Test Project"))
        .expect("add manual task");

    let mut selected = Task::new(
        fixtures.tasks_root.clone(),
        "Selected".to_string(),
        Priority::from("Medium"),
    );
    selected.status = TaskStatus::from("Todo");
    selected
        .custom_fields
        .insert("iteration".to_string(), custom_value_string("beta"));
    let selected_id = storage
        .add(&selected, "TEST", Some("Test Project"))
        .expect("add selected task");
    drop(storage);

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args([
        "sprint",
        "add",
        "--sprint",
        "1",
        &manual_id,
        "--select-where",
        "field:iteration=beta",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("2 task(s)"));

    let storage_after = Storage::new(fixtures.tasks_root.clone());
    let record = SprintService::get(&storage_after, 1).expect("sprint #1 exists");
    let assigned: Vec<String> = record
        .sprint
        .tasks
        .iter()
        .map(|entry| entry.id.clone())
        .collect();
    assert!(assigned.contains(&manual_id));
    assert!(assigned.contains(&selected_id));
}

#[test]
fn sprint_remove_detaches_tasks() {
    let fixtures = common::TestFixtures::new();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Detach".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    let mut task = Task::new(
        fixtures.tasks_root.clone(),
        "Detach Me".to_string(),
        Priority::from("Medium"),
    );
    task.status = TaskStatus::from("Todo");
    let task_id = storage
        .add(&task, "TEST", Some("Test Project"))
        .expect("failed to add task for sprint remove");
    let mut sprint_record = SprintService::get(&storage, 1).expect("load sprint #1");
    sprint_record.sprint.tasks.push(SprintTaskEntry {
        id: task_id.clone(),
        order: None,
    });
    SprintService::update(&mut storage, sprint_record.id, sprint_record.sprint)
        .expect("persist sprint membership");
    drop(storage);

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "remove", "--sprint", "1", &task_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Removed sprint #1 (Sprint Detach) from 1 task(s).",
        ));

    let storage_after = Storage::new(fixtures.tasks_root.clone());
    let task_after = storage_after
        .get(&task_id, "TEST".to_string())
        .expect("task exists");
    assert!(task_after.sprints.is_empty());
    let memberships = task_sprint_ids(&storage_after, &task_id);
    assert!(memberships.is_empty());
}

#[test]
fn sprint_backlog_lists_unassigned_tasks() {
    let fixtures = common::TestFixtures::new();

    let mut storage = Storage::new(fixtures.tasks_root.clone());

    let sprint = Sprint {
        plan: Some(SprintPlan {
            label: Some("Sprint Active".to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    };
    SprintService::create(&mut storage, sprint, None).expect("create sprint");

    let mut backlog_task = Task::new(
        fixtures.tasks_root.clone(),
        "Backlog Candidate".to_string(),
        Priority::from("Medium"),
    );
    backlog_task.status = TaskStatus::from("Todo");
    let backlog_id = storage
        .add(&backlog_task, "TEST", Some("Test Project"))
        .expect("failed to add backlog task");

    let mut assigned_task = Task::new(
        fixtures.tasks_root.clone(),
        "Assigned".to_string(),
        Priority::from("Medium"),
    );
    assigned_task.status = TaskStatus::from("InProgress");
    let assigned_id = storage
        .add(&assigned_task, "TEST", Some("Test Project"))
        .expect("failed to add assigned task");
    append_sprint_tasks(&mut storage, 1, std::slice::from_ref(&assigned_id));
    drop(storage);

    let mut cmd = common::cargo_bin_in(&fixtures);
    let output = cmd
        .args(["sprint", "backlog", "--project", "TEST", "--limit", "5"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains(&backlog_id));
    assert!(!stdout.contains("Assigned"));
}

#[test]
fn sprint_normalize_detects_non_canonical_yaml_in_check_mode() {
    let fixtures = common::TestFixtures::new();

    let sprint_dir = fixtures.tasks_root.join("@sprints");
    std::fs::create_dir_all(&sprint_dir).expect("create sprints dir");
    std::fs::write(
        sprint_dir.join("1.yml"),
        r#"plan:
  label: Sprint Dirty
  ends_at: 2030-01-15T17:00:00Z
  length: 2w
"#,
    )
    .expect("write sprint file");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "normalize"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Sprint normalization required for: #1",
        ))
        .stderr(predicate::str::contains(
            "Sprint #1 canonicalization notice: plan.length was ignored because plan.ends_at was provided.",
        ));

    let contents = std::fs::read_to_string(sprint_dir.join("1.yml")).expect("read sprint file");
    assert!(contents.contains("length: 2w"));
}

#[test]
fn sprint_normalize_rewrites_yaml_when_requested() {
    let fixtures = common::TestFixtures::new();

    let sprint_dir = fixtures.tasks_root.join("@sprints");
    std::fs::create_dir_all(&sprint_dir).expect("create sprints dir");
    std::fs::write(
        sprint_dir.join("1.yml"),
        r#"plan:
  label: Sprint Dirty
  ends_at: 2030-01-15T17:00:00Z
  length: 2w
"#,
    )
    .expect("write sprint file");

    let mut cmd = common::cargo_bin_in(&fixtures);
    cmd.args(["sprint", "normalize", "--write"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Normalized sprint #1."))
        .stdout(predicate::str::contains(
            "Normalization complete (1 sprint(s) processed, 1 updated).",
        ));

    let contents = std::fs::read_to_string(sprint_dir.join("1.yml")).expect("read sprint file");
    assert!(!contents.contains("length: 2w"));

    let parsed: Sprint = serde_yaml::from_str(&contents).expect("parse canonical sprint");
    let plan = parsed.plan.expect("plan retained");
    assert!(plan.length.is_none());
    assert_eq!(plan.ends_at.as_deref(), Some("2030-01-15T17:00:00Z"));
}
