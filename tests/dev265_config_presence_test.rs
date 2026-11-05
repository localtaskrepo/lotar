//! DEV-265: a higher config layer that sets a value back to its built-in
//! default must still win over a lower layer's non-default value.

use crate::common;
use common::TestFixtures;
use predicates::prelude::*;

fn workspace() -> TestFixtures {
    let fixtures = TestFixtures::new();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  project: ENV\n  priority: High\nauto:\n  set_reporter: false\n",
    )
    .unwrap();
    fixtures
}

fn add(fixtures: &TestFixtures, args: &[&str], env: &[(&str, &str)]) -> String {
    let mut cmd = common::lotar_cmd().unwrap();
    cmd.current_dir(fixtures.get_temp_path())
        .env("LOTAR_DEFAULT_REPORTER", "tester");
    for (key, value) in env {
        cmd.env(key, value);
    }
    let output = cmd.args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn cli_and_env_can_set_values_back_to_the_default() {
    let fixtures = workspace();

    let plain = add(&fixtures, &["add", "plain"], &[]);
    assert!(plain.contains("Priority: High"), "{plain}");
    assert!(!plain.contains("Reporter:"), "{plain}");

    let cli = add(
        &fixtures,
        &[
            "--config",
            "auto.set_reporter=true",
            "--config",
            "default.priority=Medium",
            "add",
            "cli",
        ],
        &[],
    );
    assert!(cli.contains("Priority: Medium"), "{cli}");
    assert!(cli.contains("Reporter: tester"), "{cli}");

    let env = add(
        &fixtures,
        &["add", "env"],
        &[
            ("LOTAR_DEFAULT_PRIORITY", "Medium"),
            ("LOTAR_AUTO_SET_REPORTER", "true"),
        ],
    );
    assert!(env.contains("Priority: Medium"), "{env}");
    assert!(env.contains("Reporter: tester"), "{env}");

    common::lotar_cmd()
        .unwrap()
        .current_dir(fixtures.get_temp_path())
        .env("LOTAR_AUTO_SET_REPORTER", "true")
        .args(["config", "show", "--explain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("set-reporter: true # (env)"));
}

#[test]
fn home_config_can_set_values_back_to_the_default() {
    let fixtures = workspace();
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(".lotar"), "default:\n  priority: Medium\n").unwrap();
    let out = add(
        &fixtures,
        &["add", "home"],
        &[
            ("HOME", home.path().to_str().unwrap()),
            ("LOTAR_IGNORE_HOME_CONFIG", "0"),
        ],
    );
    assert!(out.contains("Priority: Medium"), "{out}");
}

/// A config file that exists but does not parse must not make REST/MCP
/// writes validate against built-in defaults (DEV-265 finding 2).
#[test]
fn broken_config_fails_writes_closed() {
    use common::env_mutex::EnvVarGuard;
    use lotar::api_server::{ApiServer, HttpRequest};
    use serde_json::json;

    let fixtures = TestFixtures::new();
    let tasks = fixtures.tasks_root.clone();
    std::fs::write(
        tasks.join("config.yml"),
        "default:\n  project: ENV\nissue:\n  states: [Open, Closed]\n",
    )
    .unwrap();
    let _tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks.to_string_lossy());
    let mut api = ApiServer::new();
    lotar::routes::initialize(&mut api);
    let post = |api: &ApiServer, path: &str, body: serde_json::Value| {
        api.handle_request(&HttpRequest {
            method: "POST".into(),
            path: path.into(),
            query: Default::default(),
            headers: Default::default(),
            body: serde_json::to_vec(&body).unwrap(),
        })
    };
    let created = post(
        &api,
        "/api/tasks/add",
        json!({"title": "t", "project": "ENV"}),
    );
    assert_eq!(
        created.status,
        201,
        "{}",
        String::from_utf8_lossy(&created.body)
    );

    std::fs::write(
        tasks.join("config.yml"),
        "default:\n  project: ENV\nissue:\n  states: [Open, Closed\n",
    )
    .unwrap();
    let updated = post(
        &api,
        "/api/tasks/update",
        json!({"id": "ENV-1", "status": "Done"}),
    );
    assert_ne!(
        updated.status,
        200,
        "{}",
        String::from_utf8_lossy(&updated.body)
    );
    let stored = std::fs::read_to_string(tasks.join("ENV").join("1.yml")).unwrap();
    assert!(stored.contains("status: Open"), "{stored}");
    let created = post(
        &api,
        "/api/tasks/add",
        json!({"title": "u", "project": "ENV", "status": "Done"}),
    );
    assert_ne!(created.status, 201);
}
