//! DEV-55 regression coverage: task and sprint membership mutations are
//! transactional at the public boundaries shared by CLI, REST, and MCP.
//! Invalid members/sprints or write failures must leave every affected file
//! unchanged; retries must not create duplicates; concurrent participating
//! writers must not lose updates.

use lotar::api_types::{TaskCreate, TaskUpdate};
use lotar::services::sprint_assignment;
use lotar::services::sprint_service::SprintService;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::storage::sprint::Sprint;
use lotar::utils::paths;
use std::path::{Path, PathBuf};
mod common;
use crate::common::env_mutex::EnvVarGuard;

fn write_global_config(tasks_dir: &Path, extra: &str) {
    let content = format!(
        "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n{extra}"
    );
    std::fs::create_dir_all(tasks_dir).unwrap();
    std::fs::write(paths::global_config_path(tasks_dir), content).unwrap();
}

fn workspace() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    (tmp, tasks_dir)
}

fn read_bytes(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

fn sprint_path(tasks_dir: &Path, id: u32) -> PathBuf {
    tasks_dir.join("@sprints").join(format!("{id}.yml"))
}

fn task_path(tasks_dir: &Path, id: u32) -> PathBuf {
    tasks_dir.join("TEST").join(format!("{id}.yml"))
}

fn labeled_sprint(label: &str) -> Sprint {
    use lotar::storage::sprint::SprintPlan;
    Sprint {
        plan: Some(SprintPlan {
            label: Some(label.to_string()),
            ..SprintPlan::default()
        }),
        ..Sprint::default()
    }
}

/// Lock-contention signature from `acquire_storage_lock`
/// (src/storage/safety.rs): the io `WouldBlock` error raised only after the
/// bounded 2-second retry loop expires. Both fragments are required: the
/// open-failure and lock-support-failure messages also promise
/// "mutation was not run" but are permanent, never retryable.
fn is_lock_contention_busy(error: &str) -> bool {
    error.contains("still busy after 2 seconds") && error.contains("mutation was not run")
}

/// Re-run a whole public mutation, but only when it failed with the lock
/// contention timeout above. That error is produced strictly before any
/// write ("mutation was not run"), so a retry re-runs the complete
/// operation and can neither resume nor duplicate a partial mutation;
/// every other error is returned unchanged and the bounded attempt count
/// keeps a genuinely stuck lock failing. Mirrors the DEV78
/// `begin_retrying_contention` precedent in src/storage/transaction.rs:
/// under heavy IO/fsync load one worker's multi-fsync commit can
/// legitimately outlast another worker's contention window — coordination
/// working as designed, not a lost serialization guarantee.
fn retry_lock_contention<T, E: std::fmt::Display>(
    mut operation: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    let mut busy_retries = 6u32;
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(err) if busy_retries > 0 && is_lock_contention_busy(&err.to_string()) => {
                busy_retries -= 1;
            }
            Err(err) => return Err(err),
        }
    }
}

#[test]
fn lock_contention_retry_classifier_matches_only_the_real_busy_contract() {
    // Live proof against the production generator: hold the sprints lock in
    // an isolated workspace, then let acquire_storage_lock exhaust its
    // 2-second contention window and classify the real error it returns.
    let (_tmp, tasks_dir) = workspace();
    let sprints_dir = tasks_dir.join("@sprints");
    std::fs::create_dir_all(&sprints_dir).unwrap();
    let held = lotar::storage::safety::acquire_storage_lock(
        &sprints_dir,
        lotar::storage::safety::sprint_lock_name(),
    )
    .unwrap();
    let busy = lotar::storage::safety::acquire_storage_lock(
        &sprints_dir,
        lotar::storage::safety::sprint_lock_name(),
    )
    .unwrap_err();
    assert_eq!(busy.kind(), std::io::ErrorKind::WouldBlock);
    let text = busy.to_string();
    assert!(
        is_lock_contention_busy(&text),
        "classifier must accept the live busy contract: {text}"
    );
    drop(held);

    // Permanent failures carry the same not-run promise but no busy marker.
    assert!(!is_lock_contention_busy(
        "Cannot open storage lock /w/.task.lock: No such file (os error 2); check the directory and permissions; mutation was not run"
    ));
    assert!(!is_lock_contention_busy(
        "Cannot acquire storage lock /w/.task.lock: Operation not supported; check filesystem locking support and permissions; mutation was not run"
    ));
    // The busy marker alone, without the nothing-ran guarantee, must not retry.
    assert!(!is_lock_contention_busy(
        "Storage lock /w/.sprints.lock is still busy after 2 seconds"
    ));
    // Unrelated mutation errors never retry.
    assert!(!is_lock_contention_busy(
        "A pending task/sprint transaction journal exists at /w/.txn-pending.json"
    ));
    assert!(!is_lock_contention_busy("Sprint not found: 999"));
    assert!(!is_lock_contention_busy(""));
}

#[test]
fn lock_contention_retry_helper_retries_busy_and_passes_through_other_errors() {
    use std::cell::Cell;
    let busy_text = "IO error: Storage lock /w/.sprints/.sprints.lock is still busy after 2 seconds; retry after the other writer finishes; mutation was not run";

    // Busy failures are retried until the operation succeeds.
    let attempts = Cell::new(0u32);
    let outcome: Result<u32, String> = retry_lock_contention(|| {
        attempts.set(attempts.get() + 1);
        if attempts.get() < 3 {
            Err(busy_text.to_string())
        } else {
            Ok(attempts.get())
        }
    });
    assert_eq!(outcome.unwrap(), 3);
    assert_eq!(attempts.get(), 3);

    // Any other error is returned verbatim after exactly one attempt.
    let attempts = Cell::new(0u32);
    let outcome: Result<u32, String> = retry_lock_contention(|| {
        attempts.set(attempts.get() + 1);
        Err::<u32, _>("Sprint not found: 999".to_string())
    });
    assert_eq!(outcome.unwrap_err(), "Sprint not found: 999");
    assert_eq!(attempts.get(), 1, "non-busy errors must not be retried");

    // Persistent contention still fails: one attempt plus six retries.
    let attempts = Cell::new(0u32);
    let outcome: Result<(), String> = retry_lock_contention(|| {
        attempts.set(attempts.get() + 1);
        Err(busy_text.to_string())
    });
    assert!(outcome.unwrap_err().contains("still busy after 2 seconds"));
    assert_eq!(attempts.get(), 7, "retry budget must stay bounded");
}

#[test]
fn invalid_sprint_create_leaves_every_file_unchanged() {
    let (_tmp, tasks_dir) = workspace();
    write_global_config(&tasks_dir, "auto.populate_members: true\n");
    let mut storage = Storage::new(&tasks_dir);
    SprintService::create(&mut storage, labeled_sprint("Alpha"), None).unwrap();

    let sprint_before = read_bytes(&sprint_path(&tasks_dir, 1)).unwrap();
    let config_before = read_bytes(&tasks_dir.join("TEST").join("config.yml"));

    let err = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "No partial state".to_string(),
            project: Some("TEST".to_string()),
            // A fresh member makes auto-population stage a config write, and
            // the missing sprint must block the whole transaction including
            // that config mutation.
            assignee: Some("fresh-member".to_string()),
            sprints: vec![999],
            ..TaskCreate::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("Sprint not found: 999"), "{err}");

    assert!(!task_path(&tasks_dir, 1).exists());
    assert_eq!(
        read_bytes(&sprint_path(&tasks_dir, 1)).unwrap(),
        sprint_before
    );
    assert_eq!(
        read_bytes(&tasks_dir.join("TEST").join("config.yml")),
        config_before,
        "auto-populated config must not be mutated by a failed create"
    );
}

#[test]
fn invalid_member_update_leaves_task_and_sprint_bytes_unchanged() {
    let (_tmp, tasks_dir) = workspace();
    write_global_config(
        &tasks_dir,
        "strict_members: true\nauto.populate_members: false\nmembers: [alice]\n",
    );
    let mut storage = Storage::new(&tasks_dir);
    SprintService::create(&mut storage, labeled_sprint("Alpha"), None).unwrap();
    let task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Guarded".to_string(),
            project: Some("TEST".to_string()),
            reporter: Some("alice".to_string()),
            assignee: Some("alice".to_string()),
            ..TaskCreate::default()
        },
    )
    .unwrap();

    let task_before = read_bytes(&task_path(&tasks_dir, 1)).unwrap();
    let sprint_before = read_bytes(&sprint_path(&tasks_dir, 1)).unwrap();

    let err = TaskService::update(
        &mut storage,
        &task.id,
        TaskUpdate {
            assignee: Some("mallory".to_string()),
            sprints: Some(vec![1]),
            ..TaskUpdate::default()
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("not in configured members"),
        "{err}"
    );

    assert_eq!(read_bytes(&task_path(&tasks_dir, 1)).unwrap(), task_before);
    assert_eq!(
        read_bytes(&sprint_path(&tasks_dir, 1)).unwrap(),
        sprint_before
    );
    let dto = TaskService::get(&storage, &task.id, None).unwrap();
    assert!(dto.sprints.is_empty());
}

#[test]
fn create_failure_retry_creates_single_task_without_duplicates() {
    let (_tmp, tasks_dir) = workspace();
    write_global_config(&tasks_dir, "");
    let mut storage = Storage::new(&tasks_dir);
    SprintService::create(&mut storage, labeled_sprint("Alpha"), None).unwrap();

    let failed = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "First attempt".to_string(),
            project: Some("TEST".to_string()),
            sprints: vec![424242],
            ..TaskCreate::default()
        },
    );
    assert!(failed.is_err());

    let dto = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Retry".to_string(),
            project: Some("TEST".to_string()),
            sprints: vec![1],
            ..TaskCreate::default()
        },
    )
    .unwrap();

    // The failed attempt left no orphan; the retry reused its identifier.
    assert_eq!(dto.id, "TEST-1");
    let mut count = 0usize;
    for entry in std::fs::read_dir(tasks_dir.join("TEST")).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".yml") && name != "config.yml" {
            count += 1;
        }
    }
    assert_eq!(count, 1);

    let sprint: Sprint =
        serde_yaml_ng::from_str(&std::fs::read_to_string(sprint_path(&tasks_dir, 1)).unwrap())
            .unwrap();
    let memberships = sprint
        .tasks
        .iter()
        .filter(|entry| entry.id == "TEST-1")
        .count();
    assert_eq!(memberships, 1, "retry must not duplicate the membership");
}

#[test]
fn malformed_task_yaml_update_failure_leaves_sprint_bytes() {
    let (_tmp, tasks_dir) = workspace();
    write_global_config(&tasks_dir, "");
    let mut storage = Storage::new(&tasks_dir);
    SprintService::create(&mut storage, labeled_sprint("Alpha"), None).unwrap();
    let task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Will corrupt".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        },
    )
    .unwrap();

    // Simulate external corruption that tolerant parsing refuses to edit.
    let corrupted = "title: [unclosed\n";
    std::fs::write(task_path(&tasks_dir, 1), corrupted).unwrap();
    let sprint_before = read_bytes(&sprint_path(&tasks_dir, 1)).unwrap();

    let err = TaskService::update(
        &mut storage,
        &task.id,
        TaskUpdate {
            sprints: Some(vec![1]),
            ..TaskUpdate::default()
        },
    )
    .unwrap_err();
    // The unreadable task fails the operation before any sprint write.
    assert!(err.to_string().contains("Task not found"), "{err}");

    assert_eq!(
        std::fs::read_to_string(task_path(&tasks_dir, 1)).unwrap(),
        corrupted
    );
    assert_eq!(
        read_bytes(&sprint_path(&tasks_dir, 1)).unwrap(),
        sprint_before
    );
}

#[test]
fn successful_create_preserves_unknown_sprint_fields_and_member_order() {
    let (_tmp, tasks_dir) = workspace();
    write_global_config(&tasks_dir, "");
    let mut storage = Storage::new(&tasks_dir);
    TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Existing".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        },
    )
    .unwrap();
    drop(storage);

    // Handwritten sprint YAML with user-owned extension and a preset member.
    std::fs::write(
        sprint_path(&tasks_dir, 1),
        "plan:\n  label: Alpha\ntasks:\n  - TEST-1\nx_team_extension:\n  board: core\n",
    )
    .unwrap();

    let mut storage = Storage::new(&tasks_dir);
    let second = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Joined later".to_string(),
            project: Some("TEST".to_string()),
            sprints: vec![1],
            ..TaskCreate::default()
        },
    )
    .unwrap();
    assert_eq!(second.id, "TEST-2");

    let raw = std::fs::read_to_string(sprint_path(&tasks_dir, 1)).unwrap();
    let sprint: Sprint = serde_yaml_ng::from_str(&raw).unwrap();
    assert_eq!(
        sprint
            .extra_fields
            .get("x_team_extension")
            .and_then(|value| value.get("board")),
        Some(&serde_yaml_ng::Value::String("core".to_string())),
        "unknown sprint fields must survive membership writes: {raw}"
    );
    let order: Vec<&str> = sprint.tasks.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(order, vec!["TEST-1", "TEST-2"], "member order must persist");
}

#[test]
fn concurrent_creates_and_assignments_lose_no_memberships() {
    let (_tmp, tasks_dir) = workspace();
    write_global_config(&tasks_dir, "");
    let mut storage = Storage::new(&tasks_dir);
    SprintService::create(&mut storage, labeled_sprint("Alpha"), None).unwrap();

    let preexisting: Vec<String> = (0..6)
        .map(|i| {
            TaskService::create(
                &mut storage,
                TaskCreate {
                    title: format!("Existing {i}"),
                    project: Some("TEST".to_string()),
                    ..TaskCreate::default()
                },
            )
            .unwrap()
            .id
        })
        .collect();

    let root = tasks_dir.clone();
    std::thread::scope(|scope| {
        for worker in 0..3u32 {
            let root = root.clone();
            scope.spawn(move || {
                let mut storage = Storage::new(&root);
                for round in 0..4u32 {
                    retry_lock_contention(|| {
                        TaskService::create(
                            &mut storage,
                            TaskCreate {
                                title: format!("Created {worker}-{round}"),
                                project: Some("TEST".to_string()),
                                sprints: vec![1],
                                ..TaskCreate::default()
                            },
                        )
                    })
                    .expect("created task under coordinated contention");
                }
            });
        }
        for chunk in preexisting.chunks(2) {
            let root = root.clone();
            let ids: Vec<String> = chunk.to_vec();
            scope.spawn(move || {
                let mut storage = Storage::new(&root);
                let mut records = SprintService::list(&storage).unwrap();
                retry_lock_contention(|| {
                    sprint_assignment::assign_tasks(
                        &mut storage,
                        &mut records,
                        &ids,
                        Some("1"),
                        false,
                        false,
                    )
                })
                .expect("assignment under coordinated contention");
            });
        }
    });

    // Every create landed exactly once with a unique identifier.
    let mut numeric_ids: Vec<u32> = Vec::new();
    for entry in std::fs::read_dir(tasks_dir.join("TEST")).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(stem) = name.strip_suffix(".yml").filter(|stem| *stem != "config") {
            numeric_ids.push(stem.parse().unwrap());
        }
    }
    numeric_ids.sort_unstable();
    let expected: Vec<u32> = (1..=(6 + 12)).collect();
    assert_eq!(numeric_ids, expected, "no lost or duplicated task files");

    // The sprint holds every task exactly once: 6 preexisting + 12 created.
    let sprint: Sprint =
        serde_yaml_ng::from_str(&std::fs::read_to_string(sprint_path(&tasks_dir, 1)).unwrap())
            .unwrap();
    let mut members: Vec<&str> = sprint.tasks.iter().map(|entry| entry.id.as_str()).collect();
    members.sort_unstable();
    members.dedup();
    assert_eq!(members.len(), 18, "all memberships must survive");

    let verify = Storage::new(&tasks_dir);
    for numeric in &expected {
        let id = format!("TEST-{numeric}");
        let dto = TaskService::get(&verify, &id, None).unwrap();
        assert_eq!(dto.sprints, vec![1], "{id} must report its membership");
    }
}

mod api_boundary {
    use super::*;
    use lotar::api_server::{ApiServer, HttpRequest};
    use lotar::routes;
    use serde_json::{Value, json};
    use std::collections::HashMap;

    fn mk_req(method: &str, path: &str, body: Value) -> HttpRequest {
        HttpRequest {
            method: method.to_string(),
            path: path.to_string(),
            query: HashMap::new(),
            headers: HashMap::new(),
            body: serde_json::to_vec(&body).unwrap(),
        }
    }

    fn server() -> ApiServer {
        let mut api = ApiServer::new();
        routes::initialize(&mut api);
        api
    }

    // The REST boundary routes through the same TaskService transaction, so a
    // rejected sprint must not leave partial files behind.
    #[test]
    fn rest_create_with_invalid_sprint_rejects_and_writes_nothing() {
        let (_tmp, tasks_dir) = workspace();
        write_global_config(&tasks_dir, "auto.populate_members: true\n");
        let mut storage = Storage::new(&tasks_dir);
        SprintService::create(&mut storage, labeled_sprint("Alpha"), None).unwrap();
        drop(storage);

        let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
        let api = server();

        let resp = api.handle_request(&mk_req(
            "POST",
            "/api/tasks/add",
            json!({
                "title": "REST rollback",
                "project": "TEST",
                "assignee": "rest-member",
                "sprints": [31337],
            }),
        ));
        assert_eq!(resp.status, 404, "invalid sprint must be rejected");
        let body: Value = serde_json::from_slice(&resp.body).unwrap();
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Sprint not found: 31337")
        );

        assert!(!task_path(&tasks_dir, 1).exists());
        assert!(read_bytes(&tasks_dir.join("TEST").join("config.yml")).is_none());
        assert!(!tasks_dir.join(".txn-pending.json").exists());
    }
}
