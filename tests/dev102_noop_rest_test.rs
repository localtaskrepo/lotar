//! No-op REST mutations preserve stored bytes/clocks and stay off the event bus.
use lotar::api_events::{self, ApiEvent};
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

mod common;
use common::env_mutex::EnvVarGuard;

struct Fixture {
    _temp: tempfile::TempDir,
    _env: Vec<EnvVarGuard>,
    root: PathBuf,
    api: ApiServer,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(".tasks");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("config.yml"), "default.project: NP\nissue.states: [Todo, InProgress, Done]\nissue.priorities: [Low, Medium, High]\nissue.types: [Feature, Bug, Chore]\nauto.populate_members: false\nauto.set_reporter: false\nauto.assign_on_status: false\nauto.identity: false\nauto.identity_git: false\n").unwrap();
        let env = vec![
            EnvVarGuard::set("LOTAR_TASKS_DIR", root.to_str().unwrap()),
            EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"),
            EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1"),
            EnvVarGuard::clear("LOTAR_PROJECT"),
            EnvVarGuard::clear("LOTAR_DEFAULT_ASSIGNEE"),
            EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER"),
        ];
        let mut api = ApiServer::new();
        routes::initialize(&mut api);
        Self {
            _temp: temp,
            _env: env,
            root,
            api,
        }
    }

    fn post(&self, route: &str, body: Value) -> Value {
        let response = self.api.handle_request(&HttpRequest {
            method: "POST".into(),
            path: route.into(),
            query: HashMap::new(),
            headers: HashMap::new(),
            body: serde_json::to_vec(&body).unwrap(),
        });
        assert!(
            (200..300).contains(&response.status),
            "{route}: {}",
            String::from_utf8_lossy(&response.body)
        );
        serde_json::from_slice::<Value>(&response.body).unwrap()["data"].clone()
    }

    fn create(&self) -> (String, PathBuf) {
        let task = self.post(
            "/api/tasks/add",
            json!({"project":"NP","title":"No-op REST fixture"}),
        );
        let id = task["id"].as_str().unwrap().to_string();
        let file = self
            .root
            .join("NP")
            .join(format!("{}.yml", id.rsplit('-').next().unwrap()));
        (id, file)
    }
}

fn pin_file(path: &Path) -> (Vec<u8>, SystemTime) {
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_650_000_000);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(time))
        .unwrap();
    (
        std::fs::read(path).unwrap(),
        std::fs::metadata(path).unwrap().modified().unwrap(),
    )
}

fn assert_stable(path: &Path, before: &(Vec<u8>, SystemTime)) {
    assert_eq!(std::fs::read(path).unwrap(), before.0);
    assert_eq!(
        std::fs::metadata(path).unwrap().modified().unwrap(),
        before.1
    );
}

fn task_events(receiver: &mpsc::Receiver<ApiEvent>) -> Vec<ApiEvent> {
    receiver
        .try_iter()
        .filter(|event| event.kind.starts_with("task_"))
        .collect()
}

#[test]
fn rest_empty_repeated_clear_and_same_status_patches_are_silent() {
    let fixture = Fixture::new();
    let (id, file) = fixture.create();
    let before = pin_file(&file);
    let events = api_events::subscribe();
    for patch in [
        json!({"id":id}),
        json!({"id":id,"title":"No-op REST fixture","status":"todo","priority":"medium","task_type":"feature"}),
        json!({"id":id,"description":null,"assignee":null,"reporter":null,"effort":null,"due_date":null,"tags":[],"acceptance_criteria":[],"custom_fields":{},"relationships":{},"sprints":[]}),
    ] {
        let response = fixture.post("/api/tasks/update", patch);
        assert_eq!(response["id"], id);
        assert_stable(&file, &before);
        assert!(task_events(&events).is_empty());
    }
    fixture.post("/api/tasks/status", json!({"id":id,"status":"Todo"}));
    assert_stable(&file, &before);
    assert!(task_events(&events).is_empty());
    fixture.post("/api/tasks/status", json!({"id":id,"status":"InProgress"}));
    assert_ne!(std::fs::read(&file).unwrap(), before.0);
    let emitted = task_events(&events);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].kind, "task_updated");
    assert_eq!(emitted[0].data["status"], "InProgress");
}

#[test]
fn rest_same_comment_edit_is_silent_but_duplicate_append_is_real() {
    let fixture = Fixture::new();
    let (id, file) = fixture.create();
    fixture.post(
        "/api/tasks/comment",
        json!({"id":id,"text":"Same explicit comment"}),
    );
    let before = pin_file(&file);
    let events = api_events::subscribe();
    fixture.post(
        "/api/tasks/comment/update",
        json!({"id":id,"index":0,"text":"Same explicit comment"}),
    );
    assert_stable(&file, &before);
    assert!(task_events(&events).is_empty());
    let after = fixture.post(
        "/api/tasks/comment",
        json!({"id":id,"text":"Same explicit comment"}),
    );
    assert_eq!(after["comments"].as_array().unwrap().len(), 2);
    assert_ne!(std::fs::read(&file).unwrap(), before.0);
    assert_eq!(task_events(&events).len(), 1);
}

#[test]
fn rest_lifecycle_repeats_are_silent_and_real_transitions_preserve_modified() {
    let fixture = Fixture::new();
    let (id, file) = fixture.create();
    let before = pin_file(&file);
    let events = api_events::subscribe();
    let restored = fixture.post("/api/tasks/restore", json!({"id":id}));
    let modified = restored["modified"].clone();
    assert_stable(&file, &before);
    assert!(task_events(&events).is_empty());
    fixture.post("/api/tasks/delete", json!({"id":id}));
    let deleted = pin_file(&file);
    assert_eq!(task_events(&events).len(), 1);
    fixture.post("/api/tasks/delete", json!({"id":id}));
    assert_stable(&file, &deleted);
    assert!(task_events(&events).is_empty());
    let restored = fixture.post("/api/tasks/restore", json!({"id":id}));
    assert_eq!(restored["modified"], modified);
    assert_eq!(task_events(&events).len(), 1);
    let active = pin_file(&file);
    fixture.post("/api/tasks/restore", json!({"id":id}));
    assert_stable(&file, &active);
    assert!(task_events(&events).is_empty());
}
