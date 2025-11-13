use predicates::prelude::*;
use std::fs;
mod common;

// Helper to run lotar with args in a given cwd
fn run_lotar(cwd: &std::path::Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(cwd);
    cmd.args(args);
    cmd.assert()
}

#[test]
fn normalize_outputs_canonical_yaml_in_dry_run() {
    let tmp = crate::common::temp_dir();
    let tasks = tmp.path().join(".tasks");
    fs::create_dir_all(&tasks).unwrap();

    // Write a global config using dotted keys and flat fields
    let cfg_path = tasks.join("config.yml");
    let yaml = r#"
server.port: 9090
default.project: DEMO
default.assignee: alice@example.com
issue.states: [Todo, InProgress, Done]
issue.types: [Feature, Bug]
issue.priorities: [Low, Medium, High]
scan.signal_words: [TODO, FIXME]
auto.identity: true
auto.identity_git: false
"#;
    fs::write(&cfg_path, yaml).unwrap();

    // Dry run normalize should print canonical nested YAML and not modify the file
    run_lotar(tmp.path(), &["config", "normalize"]) // no --write
        .success()
        .stdout(
            predicate::str::contains("server:\n")
                .and(predicate::str::contains("default:\n"))
                .and(predicate::str::contains("issue:\n"))
                .and(predicate::str::contains("scan:\n"))
                .and(predicate::str::contains("auto:\n")),
        );

    // Original file should remain unchanged
    let current = fs::read_to_string(&cfg_path).unwrap();
    assert!(current.contains("server.port: 9090"));
}

#[test]
fn normalize_writes_when_write_flag_is_set() {
    let tmp = crate::common::temp_dir();
    let tasks = tmp.path().join(".tasks");
    fs::create_dir_all(&tasks).unwrap();

    let cfg_path = tasks.join("config.yml");
    fs::write(&cfg_path, "default.project: DEMO\n").unwrap();

    run_lotar(tmp.path(), &["config", "normalize", "--write"]) // global
        .success();

    let new_contents = fs::read_to_string(&cfg_path).unwrap();
    assert!(new_contents.contains("default:"));
    assert!(new_contents.contains("project:"));
}

#[test]
fn validation_reports_invalid_and_ambiguous_ticket_patterns_global() {
    let tmp = crate::common::temp_dir();
    let tasks = tmp.path().join(".tasks");
    fs::create_dir_all(&tasks).unwrap();

    // Global config with invalid regex and overlapping patterns
    let cfg_path = tasks.join("config.yml");
    let yaml = r#"
scan:
  ticket_patterns:
    - "[A-Z+"        # invalid regex
    - "[A-Z]{2,}-\\d+" # typical key pattern
    - ".+"            # overlaps everything
issue_states: [Todo]
issue_types: [Feature]
issue_priorities: [Low, Medium]
"#;
    fs::write(&cfg_path, yaml).unwrap();

    // Run validate --global and expect failure due to invalid regex, with a warning for overlap
    run_lotar(tmp.path(), &["config", "validate", "--global"]) // errors cause non-zero in our handler
        .failure()
        .stdout(
            predicate::str::contains("Invalid regex").and(
                predicate::str::contains("overlap")
                    .or(predicate::str::contains("Multiple patterns match")),
            ),
        );
}

#[test]
fn validation_reports_invalid_ticket_patterns_project() {
    let tmp = crate::common::temp_dir();
    let tasks = tmp.path().join(".tasks");
    fs::create_dir_all(&tasks).unwrap();

    // Create minimal valid global to appease defaults
    fs::write(
        tasks.join("config.yml"),
        "issue_states: [Todo]\nissue_types: [Feature]\nissue_priorities: [Low, Medium]\n",
    )
    .unwrap();

    // Project config
    let proj = tasks.join("DEMO");
    fs::create_dir_all(&proj).unwrap();
    let proj_cfg = proj.join("config.yml");
    let yaml = r#"
project:
    name: DEMO
scan:
  ticket_patterns:
    - "(unclosed"  # invalid regex
"#;
    fs::write(&proj_cfg, yaml).unwrap();

    // Validate project and expect failure due to invalid regex
    run_lotar(tmp.path(), &["config", "validate", "--project", "DEMO"]) // project scope
        .failure()
        .stdout(predicate::str::contains("Invalid regex"));
}
