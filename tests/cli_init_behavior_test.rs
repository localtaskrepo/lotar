use std::path::Path;
use std::process::Output;

use tempfile::TempDir;

mod common;

fn run(args: &[&str], dir: &Path) -> Output {
    let mut cmd = common::lotar_cmd().expect("lotar cmd");
    cmd.current_dir(dir)
        .env("LOTAR_TEST_SILENT", "1")
        .args(args)
        .output()
        .expect("run lotar")
}

fn run_ok(args: &[&str], dir: &Path) -> String {
    let out = run(args, dir);
    assert!(
        out.status.success(),
        "lotar {:?} failed.\nstdout: {}\nstderr: {}",
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {}", p.display(), e))
}

#[test]
fn validate_after_init_succeeds() {
    // Regression: with the "always create global with default_project" behavior,
    // `config validate --project=<PREFIX>` used to fail because the prefix
    // collided with itself. It should now succeed.
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(&["init", "--yes", "--project=App"], tmp.path());
    let stdout = run_ok(&["config", "validate", "--project=APP"], tmp.path());
    assert!(
        stdout.contains("All configurations are valid"),
        "validate output should say valid:\n{}",
        stdout
    );
    let stdout = run_ok(&["config", "validate", "--global"], tmp.path());
    assert!(
        stdout.contains("All configurations are valid"),
        "global validate should pass:\n{}",
        stdout
    );
}

#[test]
fn rerun_with_force_overwrites_project() {
    // Regression: `--force` must bypass the prefix-collision check so a project
    // can be reinitialized in place (e.g. to switch workflow presets).
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(&["init", "--yes", "--project=App"], tmp.path());
    let before = read(&tmp.path().join(".tasks/APP/config.yml"));
    assert!(
        !before.contains("Verify"),
        "default workflow should not contain Verify state"
    );
    run_ok(
        &[
            "init",
            "--yes",
            "--project=App",
            "--workflow=agile",
            "--force",
        ],
        tmp.path(),
    );
    let after = read(&tmp.path().join(".tasks/APP/config.yml"));
    assert!(
        after.contains("Verify"),
        "agile workflow should overwrite states:\n{}",
        after
    );
}

#[test]
fn bare_init_writes_global_and_minimal_project() {
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(&["init", "--yes", "--project=MyApp"], tmp.path());

    let global = tmp.path().join(".tasks/config.yml");
    let proj = tmp.path().join(".tasks/MYAP/config.yml");
    assert!(global.exists(), "global config should exist");
    assert!(proj.exists(), "project config should exist");

    let g = read(&global);
    assert!(g.contains("default:"), "global should set default");
    assert!(
        g.contains("project: MYAP"),
        "global should carry default.project"
    );

    let p = read(&proj);
    assert!(
        p.contains("project:"),
        "project config should have project block"
    );
    assert!(
        p.contains("name: MyApp"),
        "project.name should be the human name"
    );
    assert!(
        !p.contains("{{project_name}}"),
        "project config must not contain placeholder, got:\n{}",
        p
    );
}

#[test]
fn init_never_writes_placeholder_literal() {
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    // No --project: uses detected folder name.
    run_ok(&["init", "--yes"], tmp.path());
    for entry in walkdir(&tmp.path().join(".tasks")) {
        if entry.extension().and_then(|s| s.to_str()) == Some("yml") {
            let text = read(&entry);
            assert!(
                !text.contains("{{project_name}}"),
                "{} contains placeholder:\n{}",
                entry.display(),
                text
            );
        }
    }
}

#[test]
fn workflow_agile_writes_expected_states() {
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(
        &["init", "--yes", "--project=App", "--workflow=agile"],
        tmp.path(),
    );
    let cfg = read(&tmp.path().join(".tasks/APP/config.yml"));
    for s in ["Todo", "InProgress", "Verify", "Done"] {
        assert!(
            cfg.contains(s),
            "agile states should include {}:\n{}",
            s,
            cfg
        );
    }
}

#[test]
fn with_automation_scaffold_writes_file() {
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(
        &["init", "--yes", "--project=App", "--with=automation"],
        tmp.path(),
    );
    let path = tmp.path().join(".tasks/APP/automation.yml");
    assert!(path.exists(), "automation.yml should be written");
    let text = read(&path);
    assert!(
        text.contains("automation:"),
        "automation.yml should have automation key"
    );
}

#[test]
fn with_agents_pipeline_scaffold_has_phase_keys() {
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(
        &["init", "--yes", "--project=App", "--with=agents:pipeline"],
        tmp.path(),
    );
    let path = tmp.path().join(".tasks/APP/agents.yml");
    assert!(path.exists(), "agents.yml should be written");
    let text = read(&path);
    for key in ["implement:", "test:", "merge:"] {
        assert!(
            text.contains(key),
            "agents.yml should have {}:\n{}",
            key,
            text
        );
    }
}

#[test]
fn rerun_without_force_errors() {
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(&["init", "--yes", "--project=App"], tmp.path());
    let out = run(&["init", "--yes", "--project=App"], tmp.path());
    assert!(
        !out.status.success(),
        "re-running init without --force should fail.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn dry_run_writes_no_files() {
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(&["init", "--yes", "--project=App", "--dry-run"], tmp.path());
    let tasks = tmp.path().join(".tasks");
    if tasks.exists() {
        let has_files = walkdir(&tasks).into_iter().any(|p| p.is_file());
        assert!(!has_files, "dry-run must not write any files");
    }
}

#[test]
fn legacy_agent_pipeline_template_writes_scaffolds() {
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(
        &[
            "init",
            "--yes",
            "--project=Pipe",
            "--template=agent-pipeline",
        ],
        tmp.path(),
    );
    assert!(tmp.path().join(".tasks/PIPE/automation.yml").exists());
    assert!(tmp.path().join(".tasks/PIPE/agents.yml").exists());
    let agents = read(&tmp.path().join(".tasks/PIPE/agents.yml"));
    assert!(agents.contains("implement:"));
}

#[test]
fn override_default_priority_lands_in_config() {
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(
        &[
            "init",
            "--yes",
            "--project=App",
            "--default-priority=High",
            "--default-assignee=@me",
        ],
        tmp.path(),
    );
    let cfg = read(&tmp.path().join(".tasks/APP/config.yml"));
    assert!(
        cfg.contains("priority: High") || cfg.contains("priority: HIGH"),
        "expected default.priority set:\n{}",
        cfg
    );
    assert!(
        cfg.contains("'@me'") || cfg.contains("\"@me\"") || cfg.contains("@me"),
        "expected default.assignee set:\n{}",
        cfg
    );
}

#[test]
fn copy_from_merges_but_replaces_project_name() {
    common::reset_lotar_test_environment();
    let tmp = TempDir::new().unwrap();
    run_ok(
        &[
            "init",
            "--yes",
            "--project=Source",
            "--default-priority=High",
        ],
        tmp.path(),
    );
    run_ok(
        &["init", "--yes", "--project=Target", "--copy-from=SOUR"],
        tmp.path(),
    );
    let cfg = read(&tmp.path().join(".tasks/TARG/config.yml"));
    assert!(
        cfg.contains("name: Target"),
        "target keeps its own name:\n{}",
        cfg
    );
    assert!(
        cfg.contains("High") || cfg.contains("HIGH"),
        "copied default priority should be present:\n{}",
        cfg
    );
}

// Tiny walker — avoid adding a dev-dep just for tests.
fn walkdir(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                for e in rd.flatten() {
                    stack.push(e.path());
                }
            }
        } else {
            out.push(p);
        }
    }
    out
}
