//! DEV-152: `agent.automation` never changed anything and was retired. Agent
//! job lifecycle is automation rules (`on.job_started` and friends). Config
//! files that still have the block keep loading and keep it as written, but
//! validation warns and shows the equivalent rule; the `agent_on_*` keys of
//! `config set` are gone with a hint.

use std::path::{Path, PathBuf};

mod common;

const GLOBAL: &str = "default:\n  project: AB\nagent:\n  automation:\n    on_start:\n      set_status: InProgress\n    on_success:\n      set_status: NeedsReview\n      reassign_to: assignee_or_reporter\n";
const PROJECT: &str =
    "project:\n  name: AB\nagent:\n  automation:\n    on_failure:\n      set_status: Blocked\n";

fn workspace() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("AB")).unwrap();
    std::fs::write(tasks.join("config.yml"), GLOBAL).unwrap();
    std::fs::write(tasks.join("AB/config.yml"), PROJECT).unwrap();
    (tmp, tasks)
}

fn run(tasks: &Path, args: &[&str]) -> (bool, String) {
    let mut cmd = common::cargo_bin_silent();
    cmd.env("LOTAR_IGNORE_HOME_CONFIG", "1");
    cmd.current_dir(tasks.parent().unwrap());
    cmd.arg("--tasks-dir").arg(tasks);
    let out = cmd.args(args).output().unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn old_blocks_load_and_validate_with_a_warning_showing_the_rule() {
    let (_tmp, tasks) = workspace();

    let (ok, out) = run(&tasks, &["config", "validate"]);
    assert!(ok, "a warning is not an error: {out}");
    assert!(out.contains("agent.automation is no longer used"), "{out}");
    assert!(out.contains("job_started:"), "{out}");
    assert!(out.contains("status: InProgress"), "{out}");
    assert!(out.contains("job_completed:"), "{out}");
    assert!(out.contains("assignee: \"@assignee_or_reporter\""), "{out}");

    let (ok, out) = run(&tasks, &["config", "validate", "--project", "AB"]);
    assert!(ok, "{out}");
    assert!(out.contains("job_failed:"), "{out}");
    assert!(out.contains("status: Blocked"), "{out}");

    // Tasks still work, and nothing else in the workspace minds the block.
    let (ok, out) = run(&tasks, &["add", "Still works"]);
    assert!(ok, "{out}");
}

#[test]
fn saving_other_settings_keeps_the_old_block_as_written() {
    let (_tmp, tasks) = workspace();
    let (ok, out) = run(
        &tasks,
        &["config", "set", "default_priority", "High", "--global"],
    );
    assert!(ok, "{out}");
    let global = std::fs::read_to_string(tasks.join("config.yml")).unwrap();
    assert!(global.contains("priority: High"), "{global}");
    assert!(
        global.contains("reassign_to: assignee_or_reporter"),
        "{global}"
    );

    let (ok, out) = run(
        &tasks,
        &[
            "config",
            "set",
            "default_priority",
            "Low",
            "--project",
            "AB",
        ],
    );
    assert!(ok, "{out}");
    let project = std::fs::read_to_string(tasks.join("AB/config.yml")).unwrap();
    assert!(project.contains("set_status: Blocked"), "{project}");
}

#[test]
fn the_old_config_set_keys_are_removed_with_a_hint() {
    let (_tmp, tasks) = workspace();
    for key in [
        "agent_on_start_status",
        "agent_on_success_reassign_to",
        "agent.automation.on_cancel.set_status",
    ] {
        let (ok, out) = run(&tasks, &["config", "set", key, "Todo", "--global"]);
        assert!(!ok, "{key}: {out}");
        assert!(out.contains(&format!("'{key}' was removed")), "{out}");
        assert!(out.contains("job_started"), "{out}");
        assert!(
            !out.contains("Valid flat fields"),
            "the hint replaces the list: {out}"
        );
    }
}

#[test]
fn the_suggested_rule_skips_empty_actions() {
    let legacy: lotar::types::CustomFieldValue =
        serde_yaml_ng::from_str("on_start: {set_status: ''}\non_cancel: {reassign_to: bob}\n")
            .unwrap();
    let rule = lotar::config::validation::validator::legacy_agent_automation_rule(&legacy);
    assert!(!rule.contains("job_started"), "{rule}");
    assert!(
        rule.contains("job_cancelled:\n          set:\n            assignee: \"bob\""),
        "{rule}"
    );

    let empty: lotar::types::CustomFieldValue = serde_yaml_ng::from_str("{}").unwrap();
    let rule = lotar::config::validation::validator::legacy_agent_automation_rule(&empty);
    assert!(rule.contains("no rule is needed"), "{rule}");
}
