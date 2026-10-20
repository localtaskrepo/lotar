use predicates::prelude::*;
use tempfile::TempDir;

mod common;
use crate::common::env_mutex::EnvVarGuard;

use lotar::api_types::TaskCreate;
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::utils::paths;
use serde_yaml_ng::Value;

fn write_minimal_config(tasks_dir: &std::path::Path, extra: &str) {
    // Provide minimal, valid lists so CLI validation passes where needed
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

#[test]
fn identity_refreshes_after_external_config_edits_and_removals() {
    use lotar::utils::identity::{resolve_current_user, resolve_current_user_explain};
    let temp = TempDir::new().unwrap();
    let _home = EnvVarGuard::set("HOME", temp.path().to_str().unwrap());
    let _reporter = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");
    let _user = EnvVarGuard::set("USER", "fallback-user");
    let tasks = temp.path().join("workspace/.tasks");
    std::fs::create_dir_all(tasks.join("TEST")).unwrap();
    write_minimal_config(&tasks, "default.reporter: alice\n");
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("alice"));

    let global = paths::global_config_path(&tasks);
    let before = std::fs::read_to_string(&global).unwrap();
    let stamp = std::fs::metadata(&global).unwrap().modified().unwrap();
    let changed = before.replace("alice", "bruce");
    assert_eq!(before.len(), changed.len());
    std::fs::write(&global, changed).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&global)
        .unwrap()
        .set_modified(stamp)
        .unwrap();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("bruce"));

    let project = tasks.join("TEST/config.yml");
    std::fs::write(&project, "default.reporter: bruce\n").unwrap();
    let found = resolve_current_user_explain(Some(&tasks)).unwrap();
    assert_eq!(found.user, "bruce");
    assert!(found.details.unwrap().contains("project TEST"));
    let stamp = std::fs::metadata(&project).unwrap().modified().unwrap();
    let replacement = tasks.join("TEST/replacement.yml");
    std::fs::write(&replacement, "default.reporter: carol\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_modified(stamp)
        .unwrap();
    assert_eq!(
        std::fs::metadata(&project).unwrap().len(),
        std::fs::metadata(&replacement).unwrap().len()
    );
    std::fs::rename(&replacement, &project).unwrap();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("carol"));
    std::fs::write(&project, "default: [\n").unwrap();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("bruce"));
    std::fs::write(&project, "default.reporter: david\n").unwrap();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("david"));
    std::fs::remove_file(&project).unwrap();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("bruce"));
    std::fs::create_dir_all(tasks.join("NEXT")).unwrap();
    std::fs::write(tasks.join("NEXT/config.yml"), "default.reporter: alice\n").unwrap();
    let before = std::fs::read_to_string(&global).unwrap();
    let stamp = std::fs::metadata(&global).unwrap().modified().unwrap();
    let changed = before.replace("default.project: TEST", "default.project: NEXT");
    assert_eq!(before.len(), changed.len());
    std::fs::write(&global, changed).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&global)
        .unwrap()
        .set_modified(stamp)
        .unwrap();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("alice"));
    std::fs::remove_file(&global).unwrap();
    assert_eq!(
        resolve_current_user(Some(&tasks)).as_deref(),
        Some("fallback-user")
    );
    assert!(
        !global.exists(),
        "identity lookup must not recreate configuration"
    );
}

#[test]
fn identity_refreshes_cached_misses_and_detector_toggle_changes() {
    use lotar::utils::identity::resolve_current_user;
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("workspace");
    let tasks = root.join(".tasks");
    let _home = EnvVarGuard::set("HOME", temp.path().to_str().unwrap());
    let _reporter = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");
    let _user = EnvVarGuard::clear("USER");
    let _username = EnvVarGuard::clear("USERNAME");
    std::fs::create_dir_all(tasks.join("TEST")).unwrap();
    std::fs::write(root.join("package.json"), r#"{"author":"manifest-user"}"#).unwrap();
    write_minimal_config(&tasks, "auto.identity: false\n");
    assert!(resolve_current_user(Some(&tasks)).is_none());
    write_minimal_config(&tasks, "auto.identity: true\nauto.identity_git: false\n");
    assert_eq!(
        resolve_current_user(Some(&tasks)).as_deref(),
        Some("manifest-user")
    );
    write_minimal_config(&tasks, "auto.identity: true\nauto.identity_git: true\n");
    assert_eq!(
        resolve_current_user(Some(&tasks)).as_deref(),
        Some("manifest-user")
    );
    std::fs::write(tasks.join("TEST/config.yml"), "auto.identity: false\n").unwrap();
    assert!(resolve_current_user(Some(&tasks)).is_none());
    std::fs::remove_file(tasks.join("TEST/config.yml")).unwrap();
    assert_eq!(
        resolve_current_user(Some(&tasks)).as_deref(),
        Some("manifest-user")
    );
}

#[test]
fn identity_refreshes_after_environment_detector_toggle_changes() {
    use lotar::utils::identity::resolve_current_user;
    let temp = TempDir::new().unwrap();
    let _home = EnvVarGuard::set("HOME", temp.path().to_str().unwrap());
    let _reporter = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");
    let _user = EnvVarGuard::clear("USER");
    let _username = EnvVarGuard::clear("USERNAME");
    let _identity = EnvVarGuard::set("LOTAR_AUTO_IDENTITY", "false");
    let root = temp.path().join("workspace");
    let tasks = root.join(".tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    write_minimal_config(&tasks, "auto.identity_git: false\n");
    std::fs::write(root.join("package.json"), r#"{"author":"manifest-user"}"#).unwrap();
    assert!(resolve_current_user(Some(&tasks)).is_none());
    // EnvVarGuard holds the variable's mutex and restores its original value.
    unsafe {
        std::env::set_var("LOTAR_AUTO_IDENTITY", "true");
    }
    assert_eq!(
        resolve_current_user(Some(&tasks)).as_deref(),
        Some("manifest-user")
    );
}

#[test]
fn identity_refreshes_after_cli_override_changes_without_manual_invalidation() {
    use lotar::utils::identity::resolve_current_user;
    struct ClearOverrides;
    impl Drop for ClearOverrides {
        fn drop(&mut self) {
            lotar::config::resolution::clear_cli_overrides();
        }
    }
    let _clear = ClearOverrides;
    let temp = TempDir::new().unwrap();
    let _home = EnvVarGuard::set("HOME", temp.path().to_str().unwrap());
    let _reporter = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");
    let tasks = temp.path().join("workspace/.tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    write_minimal_config(&tasks, "default.reporter: alice\n");
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("alice"));
    lotar::config::resolution::configure_cli_overrides(&[(
        "default.reporter".into(),
        "bruce".into(),
    )])
    .unwrap();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("bruce"));
    lotar::config::resolution::clear_cli_overrides();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("alice"));
}

#[cfg(unix)]
#[test]
fn identity_refreshes_after_home_config_edits() {
    use lotar::utils::identity::resolve_current_user;
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let _home = EnvVarGuard::set("HOME", temp.path().to_str().unwrap());
    let _reporter = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");
    let _threads = EnvVarGuard::clear("RUST_TEST_THREADS");
    let _mode = EnvVarGuard::set("LOTAR_TEST_MODE", "0");
    let _ignore = EnvVarGuard::clear("LOTAR_IGNORE_HOME_CONFIG");
    assert_eq!(dirs::home_dir().unwrap(), temp.path());
    let tasks = temp.path().join("workspace/.tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    write_minimal_config(&tasks, "default.reporter: alice\n");
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("alice"));
    std::fs::write(temp.path().join(".lotar"), "default.reporter: bruce\n").unwrap();
    std::fs::set_permissions(
        temp.path().join(".lotar"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("bruce"));
    std::fs::write(temp.path().join(".lotar"), "default.reporter: carol\n").unwrap();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("carol"));
    // The guard above keeps this variable serialized and restores it on drop.
    unsafe {
        std::env::set_var("LOTAR_IGNORE_HOME_CONFIG", "1");
    }
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("alice"));
    unsafe {
        std::env::remove_var("LOTAR_IGNORE_HOME_CONFIG");
    }
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("carol"));
    std::fs::remove_file(temp.path().join(".lotar")).unwrap();
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("alice"));
}

#[test]
fn whoami_honors_default_and_explicit_project_identity() {
    let temp = TempDir::new().unwrap();
    let tasks = temp.path().join("workspace/.tasks");
    for project in ["TEST", "OTHER"] {
        std::fs::create_dir_all(tasks.join(project)).unwrap();
    }
    write_minimal_config(&tasks, "");
    std::fs::write(
        tasks.join("TEST/config.yml"),
        "project.name: Test Project\ndefault.reporter: alice\nauto.identity: false\n",
    )
    .unwrap();
    std::fs::write(
        tasks.join("OTHER/config.yml"),
        "default.reporter: bob\nauto.identity_git: false\n",
    )
    .unwrap();
    for (project, expected, identity, git) in [
        (None, "alice", false, true),
        (Some("Test Project"), "alice", false, true),
        (Some("OTHER"), "bob", true, false),
    ] {
        let mut cmd = crate::common::lotar_cmd().unwrap();
        cmd.current_dir(temp.path())
            .env("HOME", temp.path())
            .env("USER", "fallback-user")
            .env_remove("LOTAR_DEFAULT_REPORTER")
            .args([
                "--tasks-dir",
                tasks.to_str().unwrap(),
                "whoami",
                "--explain",
                "--format",
                "json",
            ]);
        if let Some(project) = project {
            cmd.args(["--project", project]);
        }
        let output = cmd.assert().success().get_output().stdout.clone();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["user"], expected);
        assert_eq!(value["source"], "config.default_reporter");
        assert_eq!(value["auto_identity"], identity);
        assert_eq!(value["auto_identity_git"], git);
        assert!(
            value["details"]
                .as_str()
                .unwrap()
                .contains(if expected == "alice" {
                    "project TEST"
                } else {
                    "project OTHER"
                })
        );
    }
}

#[test]
fn project_identity_cache_is_scoped_and_precedence_is_preserved() {
    use lotar::utils::identity::{
        resolve_current_user, resolve_current_user_explain_for_project,
        resolve_current_user_for_project,
    };
    let temp = TempDir::new().unwrap();
    let _home = EnvVarGuard::set("HOME", temp.path().to_str().unwrap());
    let _env = EnvVarGuard::set("LOTAR_DEFAULT_REPORTER", "environment-user");
    let _user = EnvVarGuard::set("USER", "fallback-user");
    let tasks = temp.path().join("workspace/.tasks");
    for (project, reporter) in [
        ("TEST", "alice"),
        ("OTHER", "bob"),
        ("INHERIT", ""),
        ("SELF", "\"@me\""),
    ] {
        std::fs::create_dir_all(tasks.join(project)).unwrap();
        std::fs::write(
            tasks.join(project).join("config.yml"),
            if reporter.is_empty() {
                String::new()
            } else {
                format!("default.reporter: {reporter}\nauto.identity: false\n")
            },
        )
        .unwrap();
    }
    write_minimal_config(&tasks, "default.reporter: workspace-user\n");
    assert_eq!(resolve_current_user(Some(&tasks)).as_deref(), Some("alice"));
    for (project, expected) in [
        ("OTHER", "bob"),
        ("TEST", "alice"),
        ("INHERIT", "environment-user"),
        ("SELF", "fallback-user"),
        ("OTHER", "bob"),
    ] {
        assert_eq!(
            resolve_current_user_for_project(Some(&tasks), Some(project)).as_deref(),
            Some(expected)
        );
    }
    let second = temp.path().join("second/.tasks");
    std::fs::create_dir_all(second.join("OTHER")).unwrap();
    std::fs::write(second.join("OTHER/config.yml"), "default.reporter: carol\n").unwrap();
    assert_eq!(
        resolve_current_user_for_project(Some(&second), Some("OTHER")).as_deref(),
        Some("carol")
    );

    lotar::config::resolution::configure_cli_overrides(&[(
        "default.reporter".into(),
        "cli-user".into(),
    )])
    .unwrap();
    let found = resolve_current_user_explain_for_project(Some(&tasks), Some("OTHER"));
    lotar::config::resolution::clear_cli_overrides();
    let found = found.unwrap();
    assert_eq!(found.user, "cli-user");
    assert!(found.details.unwrap().contains("CLI configuration"));
}

#[test]
fn whoami_uses_config_default_reporter() {
    // EnvVarGuard serializes the env var safely

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Configure a deterministic reporter identity
    write_minimal_config(&tasks_dir, "default.reporter: alice@example.com\n");

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp.path())
        .args(["whoami"]) // text mode
        .assert()
        .success()
        .stdout(predicate::str::contains("alice@example.com"));

    // restored by _guard drop
}

#[test]
fn whoami_explain_text_includes_source_and_confidence() {
    // EnvVarGuard serializes the env var safely

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Deterministic reporter identity
    write_minimal_config(&tasks_dir, "default.reporter: carol\n");

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp.path())
        .args(["whoami", "--explain"]) // text mode
        .assert()
        .success()
        .stdout(predicate::str::contains("carol"))
        .stdout(predicate::str::contains("source:"))
        .stdout(predicate::str::contains("confidence:"));

    // restored by _guard drop
}

#[test]
fn whoami_json_compact_and_explain_fields() {
    // EnvVarGuard serializes the env var safely

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    write_minimal_config(&tasks_dir, "default.reporter: dave\n");

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    // Compact JSON (no extra fields)
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp.path())
        .args(["whoami", "--format=json"]) // json mode
        .assert()
        .success()
        .stdout(predicate::str::contains("{\"user\":\"dave\"}"));

    // Explain JSON includes source/confidence/details
    let mut cmd2 = crate::common::lotar_cmd().unwrap();
    cmd2.current_dir(temp.path())
        .args(["whoami", "--format=json", "--explain"]) // json + explain
        .assert()
        .success()
        .stdout(predicate::str::contains("\"user\":"))
        .stdout(predicate::str::contains("\"source\":"))
        .stdout(predicate::str::contains("\"confidence\":"));

    // restored by _guard drop
}

#[test]
fn status_dry_run_explain_previews_and_does_not_write() {
    // EnvVarGuard serializes the env var safely

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Default reporter used for auto-assign preview
    write_minimal_config(&tasks_dir, "default.reporter: bob\n");

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    // Create a task via service to get a known ID
    let mut storage = Storage::new(&tasks_dir);
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Preview status change".to_string(),
            project: Some("TEST".to_string()),
            ..TaskCreate::default()
        },
    )
    .expect("create task");

    // Dry-run with explain should preview change and show auto-assign effect
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp.path())
        .args(["status", &created.id, "Done", "--dry-run", "--explain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN: Would change"))
        .stdout(predicate::str::contains("assignee"))
        .stdout(predicate::str::contains("bob"))
        .stdout(predicate::str::contains("Explanation:"));

    // Verify status wasn't changed (get path prints current status)
    let mut cmd2 = crate::common::lotar_cmd().unwrap();
    cmd2.current_dir(temp.path())
        .args(["status", &created.id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task "))
        .stdout(predicate::str::contains(" status: "))
        .stdout(predicate::str::contains("Todo"));

    // restored by _guard drop
}

#[test]
fn add_dry_run_explain_creates_no_task_files() {
    // EnvVarGuard serializes the env var safely

    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    write_minimal_config(&tasks_dir, "");

    let _guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());

    // Snapshot YAML files before
    let before_count = count_task_yaml_files(&tasks_dir);

    // Run add in dry-run mode
    let mut cmd = crate::common::lotar_cmd().unwrap();
    cmd.current_dir(temp.path())
        .args([
            "add",
            "Dry run task",
            "--project=TEST",
            "--dry-run",
            "--explain",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN: Would create task"))
        .stdout(predicate::str::contains("Explanation:"));

    // No new YAML files should be written
    let after_count = count_task_yaml_files(&tasks_dir);
    assert_eq!(
        before_count, after_count,
        "dry-run should not write task files"
    );

    // restored by _guard drop
}

#[test]
fn add_sets_reporter_via_me_alias_default() {
    let temp = TempDir::new().unwrap();
    let tasks_dir = temp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    // Ensure default reporter is @me so identity resolution is required
    write_minimal_config(&tasks_dir, "default.reporter: \"@me\"\n");

    let _guard_tasks = EnvVarGuard::set("LOTAR_TASKS_DIR", &tasks_dir.to_string_lossy());
    let _guard_user = EnvVarGuard::set("USER", "alias-user");
    let _guard_username = EnvVarGuard::set("USERNAME", "alias-user");
    let _guard_env_default = EnvVarGuard::clear("LOTAR_DEFAULT_REPORTER");

    lotar::utils::identity::invalidate_identity_cache(Some(&tasks_dir));

    crate::common::lotar_cmd()
        .unwrap()
        .current_dir(temp.path())
        .args(["add", "Alias reporter task", "--project=TEST"])
        .assert()
        .success();

    let task_path = tasks_dir.join("TEST").join("1.yml");
    let contents = std::fs::read_to_string(task_path).unwrap();
    let parsed: Value = serde_yaml_ng::from_str(&contents).unwrap();
    assert_eq!(parsed["reporter"].as_str(), Some("alias-user"));
}

fn count_task_yaml_files(tasks_dir: &std::path::Path) -> usize {
    // Count all project task YAML files (exclude top-level config.yml)
    let mut count = 0usize;
    if let Ok(read) = std::fs::read_dir(tasks_dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // inside project dir
                if let Ok(inner) = std::fs::read_dir(&path) {
                    for f in inner.flatten() {
                        let fp = f.path();
                        if fp.is_file() {
                            let name = fp.file_name().and_then(|s| s.to_str()).unwrap_or("");
                            let ext = fp.extension().and_then(|s| s.to_str()).unwrap_or("");
                            if ext.eq_ignore_ascii_case("yml") && name != "config.yml" {
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    count
}
