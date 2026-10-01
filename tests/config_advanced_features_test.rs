mod common;

use crate::common::cargo_bin_silent;
use common::TestFixtures;
use std::fs;

/// Config command advanced features: dry-run previews, force, copy-from,
/// template validation, and config show output. Every test asserts the exact
/// exit status plus the observable file effects of the documented behavior.

#[test]
fn config_init_dry_run_previews_without_writing() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    cargo_bin_silent()
        .current_dir(temp_dir)
        .args(["config", "init", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("DRY RUN"))
        .stdout(predicates::str::contains("workflow 'default'"));

    let tasks_dir = temp_dir.join(".tasks");
    assert!(
        !tasks_dir.join("config.yml").exists(),
        "dry-run init must not create the global config"
    );
    assert!(
        fs::read_dir(&tasks_dir).unwrap().count() == 0,
        "dry-run init must not create any project files"
    );

    cargo_bin_silent()
        .current_dir(temp_dir)
        .args(["config", "init", "--dry-run", "--template=agile"])
        .assert()
        .success()
        .stdout(predicates::str::contains("DRY RUN"))
        .stdout(predicates::str::contains("workflow 'agile'"));

    assert!(
        !tasks_dir.join("config.yml").exists(),
        "dry-run init with a template must not create the global config"
    );
    assert!(
        fs::read_dir(&tasks_dir).unwrap().count() == 0,
        "dry-run init with a template must not create any project files"
    );
}

#[test]
fn config_set_dry_run_previews_without_modifying_config() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    cargo_bin_silent()
        .current_dir(temp_dir)
        .args(["config", "init", "--template=default"])
        .assert()
        .success();

    cargo_bin_silent()
        .current_dir(temp_dir)
        .args([
            "config",
            "set",
            "project_name",
            "test-project-dry-run",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "DRY RUN: Would set project_name = test-project-dry-run",
        ));

    let config_path = temp_dir.join(".tasks").join("config.yml");
    assert!(config_path.exists(), "init should have created the config");
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(
        !content.contains("test-project-dry-run"),
        "dry-run set must not modify the config: {content}"
    );
}

#[test]
fn config_force_overwrites_project_config_with_agile_workflow() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "init", "--template=default"])
        .assert()
        .success();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "init", "--force", "--template=agile"])
        .assert()
        .success();

    let tasks_dir = temp_dir.join(".tasks");
    assert!(
        tasks_dir.join("config.yml").exists(),
        "force init must leave the global config in place"
    );
    let project_configs: Vec<String> = fs::read_dir(&tasks_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.path().join("config.yml"))
        .filter(|path| path.exists())
        .map(|path| fs::read_to_string(&path).unwrap_or_default())
        .collect();
    assert!(
        project_configs
            .iter()
            .any(|content| content.contains("Verify") || content.contains("InProgress")),
        "force init must overwrite the project config with the agile workflow: {project_configs:?}"
    );
}

#[test]
fn config_force_does_not_bypass_field_validation() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "set", "invalid_field", "invalid_value", "--force"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Invalid project config field"));
}

#[test]
fn config_copy_from_merges_source_project_settings() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "init", "--template=agile"])
        .assert()
        .success();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "set", "default_project", "DEMO", "--global"])
        .assert()
        .success();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "set", "default_priority", "High"])
        .assert()
        .success();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "init", "--project=frontend", "--copy-from=DEMO"])
        .assert()
        .success();

    let copied: Vec<String> = fs::read_dir(temp_dir.join(".tasks"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.path().join("config.yml"))
        .filter(|path| path.exists())
        .map(|path| fs::read_to_string(&path).unwrap_or_default())
        .filter(|content| content.contains("name: frontend"))
        .collect();
    assert_eq!(
        copied.len(),
        1,
        "exactly one frontend project config must exist: {copied:?}"
    );
    assert!(
        copied[0].contains("priority: High"),
        "copy-from must merge non-identity settings from the source project: {}",
        copied[0]
    );
}

#[test]
fn config_set_rejects_invalid_and_unknown_fields() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "init", "--template=default"])
        .assert()
        .success();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args([
            "config",
            "set",
            "issue_prefix",
            "invalid-prefix-with-dashes",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Invalid project config field: 'issue_prefix'",
        ));

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "set", "unknown_field", "some_value"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Invalid project config field: 'unknown_field'",
        ));
}

#[test]
fn config_set_persists_project_name_in_project_config() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "init", "--template=default"])
        .assert()
        .success();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "set", "default_project", "DEMO", "--global"])
        .assert()
        .success();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "set", "project_name", "project-specific"])
        .assert()
        .success();

    let project_config =
        fs::read_to_string(temp_dir.join(".tasks").join("DEMO").join("config.yml"))
            .expect("project config must exist after set");
    assert!(
        project_config.contains("name: project-specific"),
        "project_name must be persisted in the project config: {project_config}"
    );
}

#[test]
fn config_show_json_reports_resolved_defaults_and_sources() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "init", "--template=default"])
        .assert()
        .success();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "set", "default_project", "DEMO", "--global"])
        .assert()
        .success();

    let output = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "show", "--format=json"])
        .output()
        .expect("run config show");
    assert!(
        output.status.success(),
        "config show failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "config show --format=json must emit valid JSON: {e}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(
        payload["config"]["default_project"], "DEMO",
        "resolved config must report the global default project: {payload}"
    );
    assert_eq!(
        payload["sources"]["default.project"], "global",
        "the default project must be attributed to the global source: {payload}"
    );
}

#[test]
fn config_init_accepts_documented_templates() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    for template in [
        "default",
        "agile",
        "kanban",
        "jira",
        "github",
        "jira-github",
    ] {
        crate::common::lotar_cmd()
            .unwrap()
            .current_dir(temp_dir)
            .args([
                "config",
                "init",
                format!("--template={template}").as_str(),
                "--force",
            ])
            .assert()
            .success();
    }

    assert!(
        temp_dir.join(".tasks").join("config.yml").exists(),
        "template init must leave the global config in place"
    );
}

#[test]
fn config_init_rejects_unknown_template() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["config", "init", "--template=nonexistent"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Unknown workflow/template"));
}

#[test]
fn config_help_documents_advanced_options() {
    let fixtures = TestFixtures::new();
    let temp_dir = fixtures.temp_dir.path();

    let output = crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp_dir)
        .args(["help", "config"])
        .output()
        .expect("run help config");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--dry-run"), "help must document --dry-run");
    assert!(stdout.contains("--force"), "help must document --force");
    assert!(
        stdout.contains("--copy-from"),
        "help must document --copy-from"
    );
}
