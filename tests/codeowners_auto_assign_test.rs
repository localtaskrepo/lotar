use assert_cmd::Command;
use tempfile::TempDir;

mod common;
use common::env_mutex::EnvVarGuard;

use lotar::api_types::TaskCreate;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::utils::paths;

fn write_minimal_config(tasks_dir: &std::path::Path, extra: &str) {
    let base = r#"default.project: TEST
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug, Chore]
issue.priorities: [Low, Medium, High]
"#;
    let mut content = String::from(base);
    if !extra.is_empty() {
        content.push_str(extra);
        if !extra.ends_with('\n') {
            content.push('\n');
        }
    }
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

fn init_fake_repo_with_codeowners(repo_root: &std::path::Path, codeowners: &str) {
    // minimal .git dir so find_repo_root detects
    std::fs::create_dir_all(repo_root.join(".git")).unwrap();
    std::fs::write(repo_root.join("CODEOWNERS"), codeowners).unwrap();
}

#[test]
fn codeowners_assigns_owner_on_first_status_change() {
    // EnvVarGuard will restore LOTAR_TASKS_DIR automatically

    let temp = TempDir::new().unwrap();
    let repo_root = temp.path();
    let tasks_dir = repo_root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    init_fake_repo_with_codeowners(repo_root, "* @global\n/src/** @alice\n");

    // Enable codeowners-based assignment
    write_minimal_config(&tasks_dir, "auto.codeowners_assign: true\n");

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    // Create a task with a custom field 'path' that matches CODEOWNERS
    let mut storage = Storage::new(tasks_dir.clone());
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Codeowners assign".to_string(),
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
            custom_fields: Some({
                let mut map = std::collections::HashMap::new();
                #[cfg(not(feature = "schema"))]
                {
                    map.insert(
                        "path".to_string(),
                        serde_yaml::Value::String("src/main.rs".to_string()),
                    );
                }
                #[cfg(feature = "schema")]
                {
                    map.insert(
                        "path".to_string(),
                        serde_json::Value::String("src/main.rs".to_string()),
                    );
                }
                map
            }),
        },
    )
    .expect("create task");

    // Move status via CLI to trigger handler with CODEOWNERS logic
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(repo_root)
        .args(["status", &created.id, "IN_PROGRESS"]) // first change away from default
        .assert()
        .success();

    // Read back and verify assignee is CODEOWNERS owner 'alice'
    let storage = Storage::new(tasks_dir.clone());
    let fetched = lotar::services::task_service::TaskService::get(&storage, &created.id, None)
        .expect("get task");
    // Path-based matching removed; default owner should be applied
    assert_eq!(fetched.assignee.as_deref(), Some("global"));

    // guard drops here
}

#[test]
fn codeowners_disabled_falls_back_to_identity() {
    // serialize via EnvVarGuard not needed; per-var mutex inside

    let temp = TempDir::new().unwrap();
    let repo_root = temp.path();
    let tasks_dir = repo_root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    init_fake_repo_with_codeowners(repo_root, "/src/** @alice\n");

    // Disable codeowners and set default.reporter so identity fallback is deterministic
    write_minimal_config(
        &tasks_dir,
        "auto.codeowners_assign: false\ndefault.reporter: bob\n",
    );

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let mut storage = Storage::new(tasks_dir.clone());
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Disabled codeowners".to_string(),
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
            custom_fields: Some({
                let mut map = std::collections::HashMap::new();
                #[cfg(not(feature = "schema"))]
                {
                    map.insert(
                        "path".to_string(),
                        serde_yaml::Value::String("src/lib.rs".to_string()),
                    );
                }
                #[cfg(feature = "schema")]
                {
                    map.insert(
                        "path".to_string(),
                        serde_json::Value::String("src/lib.rs".to_string()),
                    );
                }
                map
            }),
        },
    )
    .expect("create task");

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(repo_root)
        .args(["status", &created.id, "IN_PROGRESS"]) // first change
        .assert()
        .success();

    let storage = Storage::new(tasks_dir.clone());
    let fetched = lotar::services::task_service::TaskService::get(&storage, &created.id, None)
        .expect("get task");
    // Should fall back to identity (default.reporter 'bob')
    assert_eq!(fetched.assignee.as_deref(), Some("bob"));

    // guard drops here
}

#[test]
fn codeowners_default_multiple_owners_picks_first() {
    // use EnvVarGuard for LOTAR_TASKS_DIR

    let temp = TempDir::new().unwrap();
    let repo_root = temp.path();
    let tasks_dir = repo_root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    // Provide a catch-all with multiple owners; first should be chosen
    init_fake_repo_with_codeowners(repo_root, "* @alice @bob @carol\n");

    write_minimal_config(&tasks_dir, "auto.codeowners_assign: true\n");

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let mut storage = Storage::new(tasks_dir.clone());
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Multi owners".to_string(),
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
            custom_fields: Some({
                let mut map = std::collections::HashMap::new();
                #[cfg(not(feature = "schema"))]
                {
                    map.insert(
                        "path".to_string(),
                        serde_yaml::Value::String("src/file.rs".to_string()),
                    );
                }
                #[cfg(feature = "schema")]
                {
                    map.insert(
                        "path".to_string(),
                        serde_json::Value::String("src/file.rs".to_string()),
                    );
                }
                map
            }),
        },
    )
    .expect("create task");

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(repo_root)
        .args(["status", &created.id, "IN_PROGRESS"]) // first change
        .assert()
        .success();

    let storage = Storage::new(tasks_dir.clone());
    let fetched = lotar::services::task_service::TaskService::get(&storage, &created.id, None)
        .expect("get task");
    // Should pick the first owner from the default owner rule
    assert_eq!(fetched.assignee.as_deref(), Some("alice"));

    // guard drops here
}

#[test]
fn codeowners_no_match_and_no_default_falls_back_to_identity() {
    // use EnvVarGuard for LOTAR_TASKS_DIR

    let temp = TempDir::new().unwrap();
    let repo_root = temp.path();
    let tasks_dir = repo_root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    // Only docs rule; no default or star catch-all
    init_fake_repo_with_codeowners(repo_root, "/docs/** @docs\n");

    // Keep codeowners assign enabled; set default.reporter for deterministic fallback
    write_minimal_config(
        &tasks_dir,
        "auto.codeowners_assign: true\ndefault.reporter: bob\n",
    );

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", tasks_dir.to_string_lossy().as_ref());

    let mut storage = Storage::new(tasks_dir.clone());
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "No match".to_string(),
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
            custom_fields: Some({
                let mut map = std::collections::HashMap::new();
                #[cfg(not(feature = "schema"))]
                {
                    map.insert(
                        "path".to_string(),
                        serde_yaml::Value::String("src/lib.rs".to_string()),
                    );
                }
                #[cfg(feature = "schema")]
                {
                    map.insert(
                        "path".to_string(),
                        serde_json::Value::String("src/lib.rs".to_string()),
                    );
                }
                map
            }),
        },
    )
    .expect("create task");

    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(repo_root)
        .args(["status", &created.id, "IN_PROGRESS"]) // first change
        .assert()
        .success();

    let storage = Storage::new(tasks_dir.clone());
    let fetched = lotar::services::task_service::TaskService::get(&storage, &created.id, None)
        .expect("get task");
    // With no matching rule and no default owner, should fall back to identity (bob)
    assert_eq!(fetched.assignee.as_deref(), Some("bob"));

    // guard drops here
}
