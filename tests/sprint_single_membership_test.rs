use lotar::api_types::TaskCreate;
use lotar::services::sprint_assignment;
use lotar::services::sprint_service::SprintService;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::storage::sprint::{Sprint, SprintPlan};
use lotar::types::{Priority, TaskType};
use lotar::utils::paths;

fn write_minimal_config(tasks_dir: &std::path::Path) {
    let content = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
issue.tags: [*]
"#;
    std::fs::create_dir_all(tasks_dir).unwrap();
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

#[test]
fn sprint_assignment_requires_force_to_replace_membership() {
    let temp_dir = tempfile::tempdir().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    write_minimal_config(&tasks_dir);

    let mut storage = Storage::new(tasks_dir.clone());

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

    let task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Sample work".to_string(),
            project: Some("TEST".to_string()),
            priority: Some(Priority::from("Medium")),
            task_type: Some(TaskType::from("Feature")),
            ..TaskCreate::default()
        },
    )
    .expect("create task");

    let mut records = SprintService::list(&storage).expect("list sprints");

    let outcome = sprint_assignment::assign_tasks(
        &mut storage,
        &records,
        std::slice::from_ref(&task.id),
        Some("1"),
        false,
        false,
    )
    .expect("initial assignment succeeds");
    assert_eq!(outcome.modified, vec![task.id.clone()]);

    records = SprintService::list(&storage).expect("refresh sprints after first assignment");

    let second = sprint_assignment::assign_tasks(
        &mut storage,
        &records,
        std::slice::from_ref(&task.id),
        Some("2"),
        false,
        false,
    )
    .expect("reassignment without force should add membership");
    assert_eq!(second.modified, vec![task.id.clone()]);
    assert!(second.replaced.is_empty());

    let dto = TaskService::get(&storage, &task.id, None).expect("fetch task after copy");
    assert_eq!(dto.sprints, vec![1, 2]);

    records = SprintService::list(&storage).expect("refresh sprints before force");

    let forced = sprint_assignment::assign_tasks(
        &mut storage,
        &records,
        std::slice::from_ref(&task.id),
        Some("2"),
        false,
        true,
    )
    .expect("forced reassignment succeeds");
    assert_eq!(forced.modified, vec![task.id.clone()]);
    assert_eq!(forced.replaced.len(), 1);
    assert_eq!(forced.replaced[0].task_id, task.id);
    assert_eq!(forced.replaced[0].previous, vec![1]);

    let updated = TaskService::get(&storage, &task.id, None).expect("fetch task after force");
    assert_eq!(updated.sprints, vec![2]);
}
