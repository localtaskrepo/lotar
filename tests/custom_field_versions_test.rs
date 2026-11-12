//! Version custom fields (DEV-373): writes are checked and stored as
//! strings, queries understand version ranges, sorting follows version order.

use crate::common::env_mutex::EnvVarGuard;
use lotar::api_server::{ApiServer, HttpRequest};
use lotar::routes;
use serde_json::{Value, json};
use std::collections::HashMap;
use tempfile::TempDir;

const CONFIG: &str =
    "default:\n  project: DEMO\ncustom:\n  fields:\n    - product\n    - { name: affects, type: version }\n    - { name: fixed_in, type: version }\n";

fn workspace() -> TempDir {
    let temp = crate::common::temp_dir();
    std::fs::create_dir_all(temp.path().join(".tasks")).unwrap();
    std::fs::write(temp.path().join(".tasks/config.yml"), CONFIG).unwrap();
    temp
}

fn lotar(temp: &TempDir, args: &[&str]) -> std::process::Output {
    crate::common::lotar_cmd().unwrap().current_dir(temp.path()).env("LOTAR_TEST_SILENT", "1").args(args).output().unwrap()
}

fn add(temp: &TempDir, title: &str, fields: &[&str]) -> String {
    let mut args = vec!["add", title, "--format", "json"];
    for field in fields {
        args.push("--field");
        args.push(field);
    }
    let out = lotar(temp, &args);
    assert!(out.status.success(), "add {title}: {}", String::from_utf8_lossy(&out.stderr));
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    value["task"]["id"].as_str().unwrap().to_string()
}

fn titles(temp: &TempDir, args: &[&str]) -> Vec<String> {
    let mut all = vec!["list", "--format", "json"];
    all.extend_from_slice(args);
    let out = lotar(temp, &all);
    assert!(out.status.success(), "list {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    value["tasks"].as_array().unwrap().iter().map(|task| task["title"].as_str().unwrap().to_string()).collect()
}

fn sorted(mut list: Vec<String>) -> Vec<String> {
    list.sort();
    list
}

fn seed(temp: &TempDir) {
    add(temp, "old", &["affects=0.12.4"]);
    add(temp, "whole minor", &["affects=0.13"]);
    add(temp, "patch", &["affects=v0.13.3", "fixed_in=0.14.0"]);
    add(temp, "several", &["affects=0.13.1,0.14.0-rc.1"]);
    add(temp, "new", &["affects=1.0.0"]);
    add(temp, "none", &["product=web"]);
}

#[test]
fn cli_writes_check_and_normalise_versions() {
    let temp = workspace();
    let id = add(&temp, "patch", &["affects=v0.13.3"]);
    let several = add(&temp, "several", &["affects=0.13.1, 0.13.2"]);
    let file = std::fs::read_to_string(temp.path().join(".tasks/DEMO/1.yml")).unwrap();
    assert!(file.contains("affects: 0.13.3") || file.contains("affects: '0.13.3'"), "{file}");
    let task: serde_yaml_ng::Value = serde_yaml_ng::from_str(&file).unwrap();
    assert!(task["custom_fields"]["affects"].is_string() || task["affects"].is_string(), "stored as a string: {file}");
    let file = std::fs::read_to_string(temp.path().join(".tasks/DEMO/2.yml")).unwrap();
    assert!(file.contains("- 0.13.1") && file.contains("- 0.13.2"), "a list: {file}");
    assert_eq!(several, "DEMO-2");

    let out = lotar(&temp, &["add", "bad", "--field", "affects=soon"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("'soon' is not a version"), "{}", String::from_utf8_lossy(&out.stderr));

    let out = lotar(&temp, &["task", "edit", &id, "--field", "fixed_in=next"]);
    assert!(!out.status.success(), "edit rejects an unreadable version");
    let out = lotar(&temp, &["task", "edit", &id, "--field", "fixed_in=0.14", "--field", "product=web"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // Text fields take anything.
    let out = lotar(&temp, &["task", "edit", &id, "--field", "product=soon"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn hand_written_values_that_are_not_versions_do_not_block_other_edits() {
    let temp = workspace();
    let id = add(&temp, "old", &["product=web"]);
    let path = temp.path().join(".tasks/DEMO/1.yml");
    let file = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, file.replace("product: web", "product: web\n  affects: someday")).unwrap();
    let out = lotar(&temp, &["task", "edit", &id, "--field", "product=api"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn cli_queries_understand_version_ranges() {
    let temp = workspace();
    seed(&temp);
    // `0.13` is every 0.13.x; a list matches when one of its versions does.
    assert_eq!(sorted(titles(&temp, &["affects>=0.13", "affects<0.14"])), ["patch", "several", "whole minor"]);
    assert_eq!(sorted(titles(&temp, &["affects=0.13"])), ["patch", "several", "whole minor"]);
    assert_eq!(sorted(titles(&temp, &["affects>0.13.2"])), ["new", "patch", "several", "whole minor"]);
    assert_eq!(sorted(titles(&temp, &["affects<=1.0"])), ["new", "old", "patch", "several", "whole minor"]);
    assert_eq!(sorted(titles(&temp, &["affects=^0.13"])), ["patch", "several", "whole minor"]);
    assert_eq!(sorted(titles(&temp, &["affects=~0.13.2"])), ["patch", "whole minor"]);
    assert_eq!(sorted(titles(&temp, &["affects=0.12.4,1.0.0"])), ["new", "old"]);
    assert_eq!(sorted(titles(&temp, &["-affects>=0.14", "affects>=0.12"])), ["old", "patch", "whole minor"]);
    assert_eq!(titles(&temp, &["fixed_in<0.15"]), ["patch"]);
    // An assignment can hold a range too.
    assert_eq!(sorted(titles(&temp, &["affects=>=1"])), ["new"]);

    let out = lotar(&temp, &["list", "affects>=soon"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("'soon' is not a version"), "{}", String::from_utf8_lossy(&out.stderr));
    let out = lotar(&temp, &["list", "product>=1"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a version field"), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn sorting_by_a_version_field_follows_version_order() {
    let temp = workspace();
    add(&temp, "ten", &["affects=0.10.0"]);
    add(&temp, "nine", &["affects=0.9.1"]);
    add(&temp, "rc", &["affects=1.0.0-rc.1"]);
    add(&temp, "final", &["affects=1.0.0"]);
    add(&temp, "none", &["product=web"]);
    assert_eq!(titles(&temp, &["--sort-by", "affects", "order=asc"]), ["none", "nine", "ten", "rc", "final"]);
    assert_eq!(titles(&temp, &["--sort-by", "field:affects", "order=desc"]), ["final", "rc", "ten", "nine", "none"]);
}

fn mk_req(method: &str, path: &str, query: &[(&str, &str)], body: Value) -> HttpRequest {
    let query: HashMap<String, String> = query.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    HttpRequest { method: method.to_string(), path: path.to_string(), query, headers: HashMap::new(), body: serde_json::to_vec(&body).unwrap() }
}

#[test]
fn rest_writes_and_queries_use_versions() {
    let temp = workspace();
    let tasks_dir = temp.path().join(".tasks");
    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    let mut api = ApiServer::new();
    routes::initialize(&mut api);

    let create = |title: &str, fields: Value| json!({"title": title, "project": "DEMO", "custom_fields": fields});
    let created = api.handle_request(&mk_req("POST", "/api/tasks/add", &[], create("float", json!({"affects": 0.13}))));
    assert_eq!(created.status, 201, "{}", String::from_utf8_lossy(&created.body));
    let created: Value = serde_json::from_slice(&created.body).unwrap();
    assert_eq!(created["data"]["custom_fields"]["affects"], json!("0.13"), "stored as a string");
    let listed = api.handle_request(&mk_req("POST", "/api/tasks/add", &[], create("list", json!({"affects": ["v0.13.2", "0.14.1"]}))));
    assert_eq!(listed.status, 201, "{}", String::from_utf8_lossy(&listed.body));
    let listed: Value = serde_json::from_slice(&listed.body).unwrap();
    assert_eq!(listed["data"]["custom_fields"]["affects"], json!(["0.13.2", "0.14.1"]));
    let rejected = api.handle_request(&mk_req("POST", "/api/tasks/add", &[], create("bad", json!({"affects": "later"}))));
    assert_eq!(rejected.status, 400, "{}", String::from_utf8_lossy(&rejected.body));
    let id = created["data"]["id"].as_str().unwrap().to_string();
    let rejected = api.handle_request(&mk_req("POST", "/api/tasks/update", &[], json!({"id": id, "custom_fields": {"affects": "0.13", "fixed_in": "x.y"}})));
    assert_eq!(rejected.status, 400, "{}", String::from_utf8_lossy(&rejected.body));

    let total = |query: &[(&str, &str)]| -> Vec<String> {
        let resp = api.handle_request(&mk_req("GET", "/api/tasks/list", query, json!({})));
        assert_eq!(resp.status, 200, "{query:?}: {}", String::from_utf8_lossy(&resp.body));
        let body: Value = serde_json::from_slice(&resp.body).unwrap();
        let mut names: Vec<String> = body["data"]["tasks"].as_array().unwrap().iter().map(|t| t["title"].as_str().unwrap().to_string()).collect();
        names.sort();
        names
    };
    assert_eq!(total(&[("query", "affects>=0.14")]), ["list"]);
    assert_eq!(total(&[("query", "affects>=0.13 affects<0.14")]), ["float", "list"]);
    assert_eq!(total(&[("affects", ">0.13.5 <0.15")]), ["float", "list"]);
    assert_eq!(total(&[("affects", ">=0.14 <0.15")]), ["list"]);
    assert_eq!(total(&[("affects", "0.13.2")]), ["float", "list"]);

    let bad = api.handle_request(&mk_req("GET", "/api/tasks/list", &[("query", "affects>=nope")], json!({})));
    assert_eq!(bad.status, 400);
}
