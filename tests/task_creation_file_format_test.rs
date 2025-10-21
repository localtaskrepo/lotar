use lotar::api_types::TaskCreate;
use lotar::services::task_service::TaskService;

mod common;

#[test]
fn new_task_file_omits_modified_and_history() {
    let fixtures = common::TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let request = TaskCreate {
        title: "Fresh Task".to_string(),
        project: Some("TEST".to_string()),
        priority: None,
        task_type: None,
        reporter: None,
        assignee: None,
        due_date: None,
        effort: None,
        description: None,
        tags: vec![],
        relationships: None,
        custom_fields: None,
    };

    let created = TaskService::create(&mut storage, request).expect("task creation succeeds");
    assert!(created.id.starts_with("TEST-"));

    let task_file = fixtures.tasks_root.join("TEST").join("1.yml");
    let contents = std::fs::read_to_string(&task_file).expect("task file exists");

    assert!(
        contents.contains("type: Feature"),
        "type key should be present"
    );
    assert!(
        !contents.contains("task_type:"),
        "legacy task_type key should be omitted"
    );
    assert!(
        contents.contains("created:"),
        "created timestamp should be present"
    );
    assert!(
        !contents.contains("modified:"),
        "modified should be omitted until a change occurs"
    );
    assert!(
        !contents.contains("history:"),
        "history should be empty and omitted on creation"
    );
}
