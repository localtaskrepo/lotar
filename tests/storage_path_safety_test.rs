use lotar::storage::operations::StorageOperations;
use lotar::storage::safety::{is_valid_project_prefix, validate_project_prefix};
use lotar::storage::task::Task;
use lotar::types::Priority;
use std::fs;

fn temp_tasks_root(name: &str) -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir()).unwrap_or_else(|_| {
        std::fs::create_dir_all(std::env::temp_dir().join(name)).unwrap();
        tempfile::TempDir::new().unwrap()
    })
}

fn sample_task(title: &str) -> Task {
    Task::new(
        std::path::PathBuf::from("."),
        title.to_string(),
        Priority::new("medium"),
    )
}

#[test]
fn add_rejects_absolute_project_paths() {
    let tmp = temp_tasks_root("lotar-abs-project");
    let outside = std::env::temp_dir().join(format!("lotar-pwn-{}", std::process::id()));
    let _ = fs::remove_dir_all(&outside);

    let result = StorageOperations::add(
        tmp.path(),
        &sample_task("pwned"),
        &outside.to_string_lossy(),
        None,
    );

    assert!(result.is_err(), "absolute project path must be rejected");
    assert!(
        !outside.join("1.yml").exists(),
        "no file may be written outside the tasks root"
    );
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn add_rejects_relative_traversal_projects() {
    let tmp = temp_tasks_root("lotar-traversal-project");
    for evil in [
        "../escape",
        "a/../..",
        "..\\windows",
        "sub/../../up",
        ".",
        "..",
    ] {
        let result = StorageOperations::add(tmp.path(), &sample_task("escape"), evil, None);
        assert!(result.is_err(), "project '{evil}' must be rejected");
        let escaped = tmp
            .path()
            .parent()
            .expect("temp dir always has a parent")
            .join("escape");
        assert!(
            !escaped.exists(),
            "traversal project '{evil}' must not create directories"
        );
    }
}

#[test]
fn add_accepts_valid_projects_and_allocates_sequential_ids() {
    let tmp = temp_tasks_root("lotar-valid-project");
    let first = StorageOperations::add(tmp.path(), &sample_task("one"), "DEMO", None).unwrap();
    let second = StorageOperations::add(tmp.path(), &sample_task("two"), "DEMO", None).unwrap();
    assert_eq!(first, "DEMO-1");
    assert_eq!(second, "DEMO-2");
}

#[test]
fn concurrent_task_creation_allocates_distinct_ids_without_lost_tasks() {
    let tmp = tempfile::tempdir().unwrap();
    let barrier = std::sync::Barrier::new(8);
    let ids = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let barrier = &barrier;
                let root = tmp.path();
                scope.spawn(move || {
                    barrier.wait();
                    let title = format!("Concurrent {i}");
                    let id =
                        StorageOperations::add(root, &sample_task(&title), "DEV", None).unwrap();
                    (id, title)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    let distinct: std::collections::HashSet<_> = ids.iter().map(|(id, _)| id).collect();
    assert_eq!(distinct.len(), 8);
    for (id, title) in ids {
        assert_eq!(
            StorageOperations::get(tmp.path(), &id, "DEV")
                .unwrap()
                .title,
            title
        );
    }
}

#[test]
fn yaml_extensions_survive_storage_and_service_mutations_without_dto_exposure() {
    use lotar::services::task_service::TaskService;
    use lotar::storage::manager::Storage;

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join(".tasks");
    let project = root.join("DEV");
    fs::create_dir_all(&project).unwrap();
    let path = project.join("1.yml");
    // Missing created exercises tolerant parsing before a real storage edit.
    let input = "title: Before\nx_scalar: '001'\nx_bool: true\nx_null: null\nx_list: [one, 2, false]\nx_nested: {inner: {values: [null, 3.5]}}\nreferences: [{link: 'https://example.com'}]\n";
    fs::write(&path, input).unwrap();
    let mut task = lotar::storage::task::parse_task_yaml_tolerant(input).unwrap();
    let expected = task.extra_fields.clone();
    task.title = "Edited".into();
    StorageOperations::edit(&root, "DEV-1", &task).unwrap();
    assert_eq!(
        StorageOperations::get(&root, "DEV-1", "DEV")
            .unwrap()
            .extra_fields,
        expected
    );

    let mut task = StorageOperations::get(&root, "DEV-1", "DEV").unwrap();
    task.description = Some("Edit through the read cache".into());
    StorageOperations::edit(&root, "DEV-1", &task).unwrap();

    let mut storage = Storage::new(&root);
    let dto = TaskService::add_comment(&mut storage, "DEV-1", "Keep the extensions").unwrap();
    let json = serde_json::to_value(dto).unwrap();
    assert!(json.get("extra_fields").is_none());
    for key in expected.keys() {
        assert!(json.get(key).is_none());
    }
    let saved: Task = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved.extra_fields, expected);
    assert_eq!(saved.comments.len(), 1);
    assert_eq!(saved.references.len(), 1);

    // A replacement constructed without extension data must preserve disk values.
    StorageOperations::edit(&root, "DEV-1", &sample_task("Replacement")).unwrap();
    let saved: Task = serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved.extra_fields, expected);
    assert_eq!(saved.title, "Replacement");
}

#[test]
fn malformed_task_edit_fails_without_overwriting_user_data() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("DEV");
    fs::create_dir_all(&project).unwrap();
    let path = project.join("1.yml");
    let input = "title: Legacy\ncomments: invalid\nx_keep: {nested: [1, 2]}\n";
    fs::write(&path, input).unwrap();
    let result = StorageOperations::edit(tmp.path(), "DEV-1", &sample_task("Replacement"));
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), input);
}

#[test]
fn held_task_lock_prevents_add_edit_and_delete() {
    use fs2::FileExt;

    let tmp = tempfile::tempdir().unwrap();
    let id = StorageOperations::add(tmp.path(), &sample_task("Original"), "DEV", None).unwrap();
    let project = tmp.path().join("DEV");
    let path = project.join("1.yml");
    let original = fs::read_to_string(&path).unwrap();
    let held = fs::File::open(project.join(".task.lock")).unwrap();
    held.lock_exclusive().unwrap();
    assert!(StorageOperations::add(tmp.path(), &sample_task("New"), "DEV", None).is_err());
    assert!(StorageOperations::edit(tmp.path(), &id, &sample_task("Edited")).is_err());
    assert!(StorageOperations::delete(tmp.path(), &id, "DEV").is_err());
    assert!(!project.join("2.yml").exists());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    drop(held);
    StorageOperations::edit(tmp.path(), &id, &sample_task("Released")).unwrap();
    assert_eq!(
        StorageOperations::get(tmp.path(), &id, "DEV")
            .unwrap()
            .title,
        "Released"
    );
}

#[test]
fn task_ids_with_unsafe_prefixes_do_not_resolve() {
    let tmp = temp_tasks_root("lotar-id-prefix");
    assert!(StorageOperations::get(tmp.path(), "/abs/path-1", "/abs/path").is_none());
    assert!(StorageOperations::get(tmp.path(), "../evil-1", "../evil").is_none());
    assert!(StorageOperations::edit(tmp.path(), "../../x-1", &sample_task("t")).is_err());
    assert!(StorageOperations::delete(tmp.path(), "OK-1", "../../evil").is_err());
}

#[test]
fn corrupt_yaml_is_skipped_by_search_not_propagated() {
    use lotar::storage::filter::TaskFilter;
    use lotar::storage::search::StorageSearch;

    let tmp = temp_tasks_root("lotar-corrupt-search");
    let project = tmp.path().join("COR");
    fs::create_dir_all(&project).unwrap();
    let good = serde_yaml_ng::to_string(&sample_task("fine")).unwrap();
    fs::write(project.join("1.yml"), good).unwrap();
    fs::write(project.join("2.yml"), "{{{ not yaml: [").unwrap();

    let results = StorageSearch::search(tmp.path(), &TaskFilter::default());
    assert_eq!(results.len(), 1, "only the valid task is returned");
    assert_eq!(results[0].0, "COR-1");
}

#[test]
fn sprint_list_skips_corrupt_files_instead_of_failing() {
    use lotar::services::sprint_service::SprintService;
    use lotar::storage::manager::Storage;
    use lotar::storage::sprint::Sprint;

    let tmp = temp_tasks_root("lotar-corrupt-sprint");
    let root = tmp.path().join(".tasks");
    fs::create_dir_all(root.join("@sprints")).unwrap();
    fs::write(root.join("@sprints/1.yml"), "not: [valid\n").unwrap();

    let mut storage = Storage::new(&root);
    let records = SprintService::list(&storage).expect("list must degrade, not fail");
    assert!(records.is_empty());

    let outcome = SprintService::create(&mut storage, Sprint::default(), None);
    assert!(outcome.is_ok(), "creating after a corrupt file still works");
}

#[test]
fn project_prefix_validator_rules() {
    assert!(validate_project_prefix("DEV").is_ok());
    assert!(validate_project_prefix("My_Project-2").is_ok());
    assert!(validate_project_prefix("/tmp/abs").is_err());
    assert!(validate_project_prefix("../rel").is_err());
    assert!(validate_project_prefix("").is_err());
    assert!(is_valid_project_prefix("A"));
}

#[test]
fn task_service_create_rejects_traversal_projects_end_to_end() {
    use lotar::api_types::TaskCreate;
    use lotar::services::task_service::TaskService;
    use lotar::storage::manager::Storage;

    let tmp = temp_tasks_root("lotar-create-traversal");
    let root = tmp.path().join(".tasks");
    fs::create_dir_all(&root).unwrap();
    let mut storage = Storage::new(&root);

    let outside_root = tempfile::tempdir_in(std::env::temp_dir()).unwrap();
    let evil = outside_root.path().join("pwn");

    for bad in [
        evil.to_string_lossy().to_string(),
        "../../sibling".to_string(),
    ] {
        let result = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "nope".to_string(),
                project: Some(bad.clone()),
                ..Default::default()
            },
        );
        assert!(result.is_err(), "project '{bad}' must be rejected");
        assert!(
            !evil.join("config.yml").exists() && !root.join("../../sibling").exists(),
            "traversal project '{bad}' must not be created"
        );
    }
}
