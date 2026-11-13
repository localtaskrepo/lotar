//! DEV-383: `comment: { author: ... }` conditions and `${{comment.author}}`
//! let a `commented` rule tell who wrote the comment.

use lotar::services::automation_service::{AutomationEvent, AutomationService};
use lotar::services::task_service::{CommentContext, TaskService};
use lotar::storage::manager::Storage;

use crate::common;
use common::env_mutex::EnvVarGuard;

/// A workspace with SC-1 (holding `task_extra`) and one rule.
fn workspace(when: &str, hook: &str, task_extra: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("SC")).unwrap();
    std::fs::write(tasks.join("config.yml"), "default:\n  project: SC\nissue:\n  states: [Todo, InProgress, Done]\n").unwrap();
    std::fs::write(tasks.join("SC/1.yml"), format!("title: One\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-07-01T00:00:00Z\n{task_extra}"))
        .unwrap();
    std::fs::write(
        tasks.join("automation.yml"),
        format!(
            "automation:\n  rules:\n    - name: Rule\n      when: {when}\n      on:\n        {hook}:\n          comment: \"answer from ${{{{comment.author}}}}: ${{{{comment.text}}}}\"\n"
        ),
    )
    .unwrap();
    tmp
}

/// The comments on SC-1 after `author` comments "yes".
fn fire(when: &str, author: &str) -> Vec<String> {
    let tmp = workspace(when, "commented", "");
    let tasks = tmp.path().join(".tasks");
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];
    let mut storage = Storage::new(&tasks);
    let context = CommentContext { actor_override: Some(author.to_string()), ..CommentContext::default() };
    TaskService::add_comment_with_context(&mut storage, "SC-1", "yes", context).unwrap();
    TaskService::get(&storage, "SC-1", None).unwrap().comments.into_iter().map(|comment| comment.text).collect()
}

#[test]
fn author_condition_fires_on_that_persons_comment_only() {
    assert_eq!(fire("{ comment: { author: mallox } }", "mallox"), ["yes", "answer from mallox: yes"]);
    assert_eq!(fire("{ comment: { author: mallox } }", "claude"), ["yes"]);
    assert_eq!(fire("{ comment: { author: mallox } }", "Mallox").len(), 2, "names match case-insensitively, as assignees do");
}

#[test]
fn simulate_checks_the_latest_comment() {
    let by = |author: &str| format!("comments:\n- date: 2026-07-02T00:00:00Z\n  author: {author}\n  text: yes\n");
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];

    let tmp = workspace("{ comment: { author: mallox } }", "commented", &by("mallox"));
    let simulated = AutomationService::simulate(&tmp.path().join(".tasks"), "SC-1", AutomationEvent::Commented).unwrap();
    assert!(simulated.matched);
    assert!(simulated.warnings.iter().any(|w| w == "Checked against the latest comment, by mallox."), "{:?}", simulated.warnings);
    assert!(simulated.warnings.iter().any(|w| w.contains("${{comment.author}}") && w.contains("unexpanded")), "{:?}", simulated.warnings);

    let tmp = workspace("{ comment: { author: mallox } }", "commented", &by("claude"));
    assert!(!AutomationService::simulate(&tmp.path().join(".tasks"), "SC-1", AutomationEvent::Commented).unwrap().matched);
}

#[test]
fn author_condition_works_with_not_any_and_operators() {
    assert_eq!(fire("{ not: { comment: { author: mallox } } }", "claude").len(), 2);
    assert_eq!(fire("{ not: { comment: { author: mallox } } }", "mallox").len(), 1);
    assert_eq!(fire("{ any: [ {comment: {author: mallox}}, {comment: {author: ravi}} ] }", "ravi").len(), 2);
    assert_eq!(fire("{ comment: { author: [mallox, ravi] } }", "ravi").len(), 2);
    assert_eq!(fire("{ comment: { author: { starts_with: mal } } }", "mallox").len(), 2);
    assert_eq!(fire("{ all: [ {current: {status: Todo}}, {comment: {author: mallox}} ] }", "mallox").len(), 2);
}

#[test]
fn comment_condition_never_matches_other_events() {
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];
    let tmp = workspace("{ comment: { author: mallox } }", "manual", "comments:\n- date: 2026-07-02T00:00:00Z\n  author: mallox\n  text: yes\n");
    assert!(!AutomationService::simulate(&tmp.path().join(".tasks"), "SC-1", AutomationEvent::Manual).unwrap().matched);
}

#[test]
fn unknown_comment_condition_key_is_rejected() {
    let parsed: Result<lotar::automation::types::AutomationConditionGroup, _> = serde_yaml_ng::from_str("comment: { text: hi }");
    assert!(parsed.is_err());
}
