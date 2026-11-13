//! DEV-477: `lotar automation simulate` lists every rule an event fires, in
//! file order, and previews the ticket after all of them, as a real event
//! applies them.

use lotar::api_types::TaskCreate;
use lotar::services::automation_service::{AutomationEvent, AutomationService};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;

use crate::common;
use common::env_mutex::EnvVarGuard;

const RULES: &str = r#"automation:
  rules:
    - name: Features start high
      when: { type: Feature }
      on:
        created:
          set: { priority: High }
          add: { tags: [first] }
    - name: Bugs only
      when: { type: Bug }
      on:
        created:
          add: { tags: [bug] }
    - when: { type: Feature }
      on:
        created:
          set: { priority: Low }
          add: { tags: [last] }
"#;

#[test]
fn simulate_lists_every_matching_rule_and_applies_them_all() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("SC")).unwrap();
    std::fs::write(tasks.join("config.yml"), "default:\n  project: SC\nissue:\n  states: [Todo, InProgress, Done]\n").unwrap();
    std::fs::write(tasks.join("SC/1.yml"), "title: Feature\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-07-01T00:00:00Z\n").unwrap();
    std::fs::write(tasks.join("automation.yml"), RULES).unwrap();
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];

    let result = AutomationService::simulate(&tasks, "SC-1", AutomationEvent::Created).unwrap();
    assert!(result.matched);
    let names: Vec<&str> = result.rules.iter().map(|rule| rule.name.as_str()).collect();
    assert_eq!(names, vec!["Features start high", "Rule 3"]);
    let last: Vec<&str> = result.rules[1].actions.iter().map(|action| action.action.as_str()).collect();
    assert_eq!(last, vec!["set_priority", "add_tags"]);
    assert_eq!(result.actions.len(), 4, "all actions of both rules: {:?}", result.actions);
    let after = result.task_after.unwrap();
    assert_eq!(after.priority.to_string(), "Low", "the last rule in file order wins");
    assert_eq!(after.tags, vec!["first".to_string(), "last".to_string()]);

    // A real event ends the same way.
    let mut storage = Storage::new(&tasks);
    let created = TaskService::create(
        &mut storage,
        TaskCreate { title: "Another feature".into(), project: Some("SC".into()), task_type: Some("Feature".into()), ..Default::default() },
    )
    .unwrap();
    let real = TaskService::get(&storage, &created.id, None).unwrap();
    assert_eq!(real.priority, after.priority);
    assert_eq!(real.tags, after.tags);
}

// A later rule's `@assignee` names the assignee the event fired with, not the
// one an earlier rule sets, as a real event resolves it (DEV-477 review).
const ALIAS_RULES: &str = r#"automation:
  rules:
    - name: Hand to bob
      on:
        created:
          set: { assignee: bob }
    - name: Reporter is the assignee
      on:
        created:
          set: { reporter: "@assignee" }
"#;

#[test]
fn simulate_resolves_aliases_against_the_ticket_the_event_fired_on() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tmp.path().join(".tasks");
    std::fs::create_dir_all(tasks.join("SC")).unwrap();
    std::fs::write(tasks.join("config.yml"), "default:\n  project: SC\nissue:\n  states: [Todo, InProgress, Done]\n").unwrap();
    std::fs::write(
        tasks.join("SC/1.yml"),
        "title: Feature\nstatus: Todo\npriority: Medium\ntype: Feature\nassignee: alice\nreporter: carol\ncreated: 2026-07-01T00:00:00Z\n",
    )
    .unwrap();
    std::fs::write(tasks.join("automation.yml"), ALIAS_RULES).unwrap();
    let _env = [EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "1"), EnvVarGuard::clear("LOTAR_PROJECT")];

    let after = AutomationService::simulate(&tasks, "SC-1", AutomationEvent::Created).unwrap().task_after.unwrap();
    assert_eq!(after.assignee.as_deref(), Some("bob"));
    assert_eq!(after.reporter.as_deref(), Some("alice"));

    let mut storage = Storage::new(&tasks);
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Another".into(),
            project: Some("SC".into()),
            assignee: Some("alice".into()),
            reporter: Some("carol".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let real = TaskService::get(&storage, &created.id, None).unwrap();
    assert_eq!(real.assignee, after.assignee);
    assert_eq!(real.reporter, after.reporter);
}
