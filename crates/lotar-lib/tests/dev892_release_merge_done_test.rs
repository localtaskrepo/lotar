//! DEV-892: with lotar's own `.tasks/DEV/automation.yml`, a merge job that
//! completes sets an ordinary ticket Done, but hands a "Release vX.Y.Z" ticket
//! to @fix to tag and finish the release (STD-371): Done comes with step 7,
//! once the tag is on origin.

use lotar::services::automation_service::{AutomationEvent, AutomationService};

use crate::common;
use common::env_mutex::EnvVarGuard;

#[test]
fn merge_job_sets_done_except_on_a_release_ticket() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("DEV")).unwrap();
    // The rules name @fix and @merge: as agent profiles, not people.
    std::fs::write(tasks.join("config.yml"), "default:\n  project: DEV\nagents:\n  fix: &agent\n    runner: command\n    command: 'true'\n  merge: *agent\n")
        .unwrap();
    std::fs::write(tasks.join("DEV/config.yml"), "issue:\n  states: [Todo, InProgress, Merging, HelpNeeded, Blocked, Done, WontDo]\n").unwrap();
    std::fs::copy(std::path::Path::new(env!("REPO_ROOT")).join(".tasks/DEV/automation.yml"), tasks.join("DEV/automation.yml")).unwrap();
    for (id, title) in [(1, "Release v9.9.9"), (2, "Fix a bug")] {
        std::fs::write(
            tasks.join(format!("DEV/{id}.yml")),
            format!("title: {title}\nstatus: Merging\npriority: Medium\ntype: Chore\nassignee: '@merge'\ncreated: 2026-11-26T00:00:00Z\n"),
        )
        .unwrap();
    }
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::set("LOTAR_TRUST_ALL", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];

    let release = AutomationService::simulate(&tasks, "DEV-1", AutomationEvent::JobCompleted).unwrap();
    let names: Vec<&str> = release.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert!(names.contains(&"Release merged: finish the release"), "{names:?}");
    assert!(!names.contains(&"Merge agent lifecycle"), "{names:?}");
    let after = release.task_after.unwrap();
    assert_ne!(after.status.to_string(), "Done", "{names:?}");
    assert_eq!(after.assignee.as_deref(), Some("@fix"), "{names:?}");

    let other = AutomationService::simulate(&tasks, "DEV-2", AutomationEvent::JobCompleted).unwrap();
    let names: Vec<&str> = other.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert!(names.contains(&"Merge agent lifecycle"), "{names:?}");
    assert_eq!(other.task_after.unwrap().status.to_string(), "Done", "{names:?}");
}
