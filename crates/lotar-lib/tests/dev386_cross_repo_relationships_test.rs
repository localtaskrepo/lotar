//! DEV-386: a ticket depends on a ticket in another repo's LoTaR, named
//! `repo:ID` through the `repos` setting.
use crate::common;
use lotar::services::relationships;
use lotar::storage::manager::Storage;
use serde_json::Value;
use std::path::Path;
use std::process::Output;

fn run(tasks: &Path, args: &[&str]) -> Output {
    common::lotar_cmd()
        .unwrap()
        .current_dir(tasks.parent().unwrap())
        .env("LOTAR_TEST_MODE", "1")
        .env_remove("LOTAR_TASKS_DIR")
        .arg("--tasks-dir")
        .arg(tasks)
        .args(args)
        .output()
        .unwrap()
}

fn ok(output: Output) -> String {
    assert!(output.status.success(), "stdout={} stderr={}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn failure(output: Output) -> String {
    assert!(!output.status.success(), "stdout={}", String::from_utf8_lossy(&output.stdout));
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// `<sandbox>/lotar/.tasks` (DEV, with a public URL) and
/// `<sandbox>/standards/.tasks` (STD), which registers the first as `lotar`.
fn repos(sandbox: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let lotar = sandbox.join("lotar/.tasks");
    let standards = sandbox.join("standards/.tasks");
    std::fs::create_dir_all(&lotar).unwrap();
    std::fs::create_dir_all(&standards).unwrap();
    std::fs::write(lotar.join("config.yml"), "default:\n  project: DEV\nserver:\n  public_url: https://lotar.example\n").unwrap();
    std::fs::write(standards.join("config.yml"), "default:\n  project: STD\nrepos:\n  lotar: ../lotar\n  gone: ../missing\n").unwrap();
    ok(run(&lotar, &["add", "Release v1.0.0"]));
    ok(run(&standards, &["add", "Use the release"]));
    (lotar, standards)
}

fn blocked(standards: &Path) -> Vec<String> {
    let storage = Storage::try_open(standards).unwrap();
    relationships::blocked_dependencies(&storage, "STD-1", &storage.get("STD-1", "STD").unwrap().relationships.depends_on).unwrap()
}

#[test]
fn a_ticket_depends_on_a_ticket_in_another_repo_and_is_unblocked_when_it_closes() {
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let (lotar, standards) = repos(sandbox.path());

    ok(run(&standards, &["task", "relationships", "add", "STD-1", "lotar:DEV-01", "--kind", "depends-on"]));
    let stored = std::fs::read_to_string(standards.join("STD/1.yml")).unwrap();
    assert!(stored.contains("- lotar:DEV-1"), "{stored}");
    // The other repo is never written.
    assert!(!std::fs::read_to_string(lotar.join("DEV/1.yml")).unwrap().contains("blocks"));

    let shown = ok(run(&standards, &["task", "relationships", "STD-1"]));
    assert!(shown.contains("lotar:DEV-1 [Todo] Release v1.0.0 https://lotar.example/task/DEV-1"), "{shown}");
    assert!(ok(run(&standards, &["show", "STD-1"])).contains("Depends on: lotar:DEV-1 [Todo] Release v1.0.0 https://lotar.example/task/DEV-1"));
    let json: Value = serde_json::from_str(&ok(run(&standards, &["--format", "json", "task", "relationships", "STD-1"]))).unwrap();
    assert_eq!(json["cross_repo"]["lotar:DEV-1"]["url"], "https://lotar.example/task/DEV-1");
    assert_eq!(json["cross_repo"]["lotar:DEV-1"]["done"], false);

    assert_eq!(blocked(&standards), ["lotar:DEV-1"]);
    ok(run(&lotar, &["status", "DEV-1", "Done"]));
    assert!(blocked(&standards).is_empty());
    assert!(ok(run(&standards, &["show", "STD-1"])).contains("lotar:DEV-1 [Done]"));

    ok(run(&standards, &["task", "relationships", "remove", "STD-1", "lotar:DEV-1", "--kind", "depends-on"]));
    assert!(!std::fs::read_to_string(standards.join("STD/1.yml")).unwrap().contains("- lotar:DEV-1"));
}

/// DEV-388: the other repo never learns of the dependent, so `lotar
/// automation run` fires its `unblocked` rule once the prerequisite is done.
#[test]
fn automation_run_fires_unblocked_once_a_prerequisite_in_another_repo_is_done() {
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let (lotar, standards) = repos(sandbox.path());
    std::fs::write(standards.join("config.yml"), "default:\n  project: STD\nissue:\n  states: [Todo, Blocked, InProgress, Done]\nrepos:\n  lotar: ../lotar\n")
        .unwrap();
    std::fs::write(
        standards.join("automation.yml"),
        "automation:\n  rules:\n    - name: Prerequisites done\n      when:\n        status: Blocked\n      on:\n        unblocked:\n          set:\n            status: InProgress\n          comment: \"Waited for ${{unblocked.by}}\"\n",
    )
    .unwrap();
    ok(run(&standards, &["task", "relationships", "add", "STD-1", "lotar:DEV-1", "--kind", "depends-on"]));
    ok(run(&standards, &["status", "STD-1", "Blocked"]));

    let pending = ok(run(&standards, &["automation", "run"]));
    assert!(pending.contains("against Prerequisites done: none match"), "{pending}");
    assert!(ok(run(&standards, &["automation", "run", "Prerequisites done"])).contains("none match"));

    ok(run(&lotar, &["status", "DEV-1", "Done"]));
    let dry = ok(run(&standards, &["automation", "run", "--dry-run"]));
    assert!(dry.contains("Would run 'Prerequisites done' on STD-1"), "{dry}");
    let ran = ok(run(&standards, &["automation", "run"]));
    assert!(ran.contains("Ran 'Prerequisites done' on STD-1"), "{ran}");
    let stored = std::fs::read_to_string(standards.join("STD/1.yml")).unwrap();
    assert!(stored.contains("status: InProgress"), "{stored}");
    assert!(stored.contains("Waited for lotar:DEV-1 (Release v1.0.0)"), "{stored}");

    // Out of Blocked, the rule's own condition keeps it from firing again.
    let again = ok(run(&standards, &["automation", "run"]));
    assert!(again.contains("none match"), "{again}");
    assert_eq!(std::fs::read_to_string(standards.join("STD/1.yml")).unwrap().matches("Waited for").count(), 1);
}

#[test]
fn unresolved_cross_repo_tasks_are_refused_on_write_and_stay_open() {
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let (_lotar, standards) = repos(sandbox.path());

    assert!(failure(run(&standards, &["task", "relationships", "add", "STD-1", "nope:DEV-1", "--kind", "depends-on"])).contains("not registered"));
    assert!(failure(run(&standards, &["task", "relationships", "add", "STD-1", "gone:DEV-1", "--kind", "depends-on"])).contains("no tasks folder"));
    assert!(failure(run(&standards, &["task", "relationships", "add", "STD-1", "lotar:DEV-9", "--kind", "depends-on"])).contains("does not exist"));
    assert!(failure(run(&standards, &["task", "relationships", "add", "STD-1", "lotar:DEV-1", "--kind", "blocks"])).contains("depends_on or related"));
    ok(run(&standards, &["task", "relationships", "add", "STD-1", "lotar:DEV-1", "--kind", "related"]));

    // Written by hand, or the other repo went away later: unresolved, so
    // the prerequisite stays open and check warns without failing.
    let path = standards.join("STD/1.yml");
    let task = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, task.replace("relationships:\n", "relationships:\n  depends_on:\n  - gone:DEV-1\n")).unwrap();
    assert_eq!(blocked(&standards), ["gone:DEV-1"]);
    let check: Value = serde_json::from_str(&ok(run(&standards, &["--format", "json", "task", "relationships", "check"]))).unwrap();
    let issues = check["issues"].as_array().unwrap();
    assert!(issues.iter().any(|issue| issue["code"] == "external_unresolved" && issue["severity"] == "warning"), "{check}");
    assert_eq!(check["complete"], true);
    assert!(ok(run(&standards, &["show", "STD-1"])).contains("gone:DEV-1 (unresolved: repo 'gone' has no tasks folder"));
    // Unrelated edits still work.
    ok(run(&standards, &["task", "edit", "STD-1", "--tag", "x"]));
}

#[test]
fn mentions_of_tasks_in_another_repo_are_not_local_links() {
    let ids = relationships::mentioned_ids("Waits for lotar:DEV-374, unlike DEV-12 and `x` STD-3.");
    assert_eq!(ids.into_iter().collect::<Vec<_>>(), ["DEV-12", "STD-3"]);
}

/// DEV-389: the web task panel reads the same status, title and link.
#[test]
fn rest_reads_tasks_in_another_repo_for_the_task_panel() {
    use lotar::api_server::{ApiServer, HttpRequest};
    let sandbox = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let (_lotar, standards) = repos(sandbox.path());
    let _env = common::env_mutex::EnvVarGuard::set("LOTAR_TASKS_DIR", standards.to_str().unwrap());
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let get = |refs: &str| {
        let response = api.handle_request(&HttpRequest {
            method: "GET".into(),
            path: "/api/relationships/cross-repo".into(),
            query: [("refs".to_string(), refs.to_string())].into_iter().collect(),
            headers: Default::default(),
            body: Vec::new(),
        });
        (response.status, serde_json::from_slice::<Value>(&response.body).unwrap())
    };

    let (status, body) = get("lotar:DEV-01, gone:DEV-1");
    assert_eq!(status, 200, "{body}");
    let spec = common::openapi_semantics::SpecValidator::load_published();
    for entry in body["data"].as_object().unwrap().values() {
        assert!(spec.validate(spec.component("CrossRepoTask"), entry).is_ok(), "{entry}");
    }
    let found = &body["data"]["lotar:DEV-01"];
    assert_eq!(found["reference"], "lotar:DEV-1");
    assert_eq!(found["title"], "Release v1.0.0");
    assert_eq!(found["status"], "Todo");
    assert_eq!(found["done"], false);
    assert_eq!(found["url"], "https://lotar.example/task/DEV-1");
    assert!(body["data"]["gone:DEV-1"]["unresolved"].as_str().unwrap().contains("no tasks folder"), "{body}");

    let many = vec!["lotar:DEV-1"; 101].join(",");
    assert_eq!(get(&many).0, 400);
}
