//! DEV-59 regression coverage: automation actions compose against the
//! latest persisted task state, so sequential rules/hooks in one dispatch
//! keep earlier collection updates instead of overwriting them with the
//! event-time snapshot. Conditions, templates, and comment/job dispatch
//! keep the event's own values, and automation writes never re-dispatch.

use lotar::api_types::{TaskCreate, TaskDTO, TaskUpdate};
use lotar::services::automation_service::{AutomationEvent, AutomationService};
use lotar::services::task_service::TaskService;
use lotar::storage::manager::Storage;
use lotar::types::TaskRelationships;
use lotar::utils::paths;
use std::path::{Path, PathBuf};

fn workspace() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::write(
        paths::global_config_path(&tasks_dir),
        "default.project: TEST\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\n",
    )
    .unwrap();
    (tmp, tasks_dir)
}

fn set_automation(tasks_dir: &Path, rules: &str) {
    let yaml = format!("automation:\n  rules:\n{rules}");
    AutomationService::set(tasks_dir, Some("TEST"), &yaml).expect("set automation");
}

fn create_task(storage: &mut Storage, title: &str, tags: &[&str]) -> TaskDTO {
    TaskService::create(
        storage,
        TaskCreate {
            title: title.to_string(),
            project: Some("TEST".to_string()),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..TaskCreate::default()
        },
    )
    .expect("create task")
}

fn start_in_progress(storage: &mut Storage, id: &str) -> TaskDTO {
    TaskService::update(
        storage,
        id,
        TaskUpdate {
            status: Some("InProgress".to_string()),
            ..TaskUpdate::default()
        },
    )
    .expect("update task")
}

fn get_task(storage: &Storage, id: &str) -> TaskDTO {
    TaskService::get(storage, id, None).expect("get task")
}

fn tag_count(task: &TaskDTO, tag: &str) -> usize {
    task.tags.iter().filter(|t| t == &tag).count()
}

#[test]
fn sequential_rules_tag_additions_compose() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Compose tags", &[]);
    // `rule-one-only` is added by the first rule and never repeated by the
    // second: a second action cloned from the stale event snapshot would
    // drop it. `alpha` is the overlap member proving dedup.
    set_automation(
        &tasks_dir,
        "    - name: add-alpha\n      when: {status: InProgress}\n      on: {updated: {add: {tags: [alpha, rule-one-only]}}}\n    - name: add-beta\n      when: {status: InProgress}\n      on: {updated: {add: {tags: [alpha, beta]}}}\n",
    );

    start_in_progress(&mut storage, &task.id);

    let after = get_task(&storage, &task.id);
    assert_eq!(
        after.tags,
        vec![
            "alpha".to_string(),
            "rule-one-only".to_string(),
            "beta".to_string()
        ],
        "{:?}",
        after.tags
    );
    // The duplicate add in the second rule must not duplicate the tag.
    assert_eq!(tag_count(&after, "alpha"), 1, "{:?}", after.tags);
}

#[test]
fn later_removals_keep_earlier_additions() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Add then remove", &["keep", "drop"]);
    set_automation(
        &tasks_dir,
        "    - name: add-fresh\n      when: {status: InProgress}\n      on: {updated: {add: {tags: [fresh]}}}\n    - name: remove-drop\n      when: {status: InProgress}\n      on: {updated: {remove: {tags: [drop]}}}\n",
    );

    start_in_progress(&mut storage, &task.id);

    let after = get_task(&storage, &task.id);
    assert_eq!(
        after.tags,
        vec!["keep".to_string(), "fresh".to_string()],
        "{:?}",
        after.tags
    );
}

#[test]
fn custom_fields_merge_across_rules_with_last_write_per_key() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Compose fields", &[]);
    set_automation(
        &tasks_dir,
        "    - name: fields-one\n      when: {status: InProgress}\n      on: {updated: {set: {custom_fields: {team: core, layer: backend}}}}\n    - name: fields-two\n      when: {status: InProgress}\n      on: {updated: {set: {custom_fields: {team: platform}}}}\n",
    );

    start_in_progress(&mut storage, &task.id);

    let after = get_task(&storage, &task.id);
    assert!(
        after
            .custom_fields
            .get("team")
            .is_some_and(|v| v.as_str().is_some_and(|s| s.contains("platform"))),
        "{:?}",
        after.custom_fields
    );
    assert!(
        after
            .custom_fields
            .get("layer")
            .is_some_and(|v| v.as_str().is_some_and(|s| s.contains("backend"))),
        "{:?}",
        after.custom_fields
    );
}

#[test]
fn relationships_compose_across_rules_without_duplicates() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let overlap = create_task(&mut storage, "Overlap", &[]);
    let first_only = create_task(&mut storage, "First only", &[]);
    let seeded = create_task(&mut storage, "Seeded", &[]);
    let task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Compose links".to_string(),
            project: Some("TEST".to_string()),
            relationships: Some(TaskRelationships {
                depends_on: vec![seeded.id.clone()],
                ..TaskRelationships::default()
            }),
            ..TaskCreate::default()
        },
    )
    .expect("create task");
    // The first rule contributes `first_only` to both collections without
    // the second rule repeating it, so a second action cloned from the
    // stale event snapshot would drop it. The second rule re-adds
    // `overlap` (dedup) and removes the seeded member.
    set_automation(
        &tasks_dir,
        &format!(
            "    - name: link-one\n      when: {{status: InProgress}}\n      on: {{updated: {{add: {{depends_on: [{overlap}, {first_only}], blocks: [{first_only}]}}}}}}\n    - name: link-two\n      when: {{status: InProgress}}\n      on:\n        updated:\n          add: {{depends_on: [{overlap}]}}\n          remove: {{depends_on: [{seeded}]}}\n",
            overlap = overlap.id,
            first_only = first_only.id,
            seeded = seeded.id
        ),
    );

    start_in_progress(&mut storage, &task.id);

    let after = get_task(&storage, &task.id);
    assert_eq!(
        after.relationships.depends_on,
        vec![overlap.id.clone(), first_only.id.clone()],
        "{:?}",
        after.relationships
    );
    assert_eq!(
        after.relationships.blocks,
        vec![first_only.id.clone()],
        "{:?}",
        after.relationships
    );
}

#[test]
fn same_rule_multiple_hooks_and_legacy_start_compose() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Hook fan-out", &[]);
    set_automation(
        &tasks_dir,
        "    - name: fan-out\n      when: {status: InProgress}\n      on:\n        updated: {add: {tags: [evt-updated]}}\n        assigned: {add: {tags: [evt-assigned]}}\n        start: {add: {tags: [legacy-start]}}\n",
    );

    TaskService::update(
        &mut storage,
        &task.id,
        TaskUpdate {
            status: Some("InProgress".to_string()),
            assignee: Some("alice".to_string()),
            ..TaskUpdate::default()
        },
    )
    .expect("update task");

    let after = get_task(&storage, &task.id);
    assert_eq!(
        after.tags,
        vec![
            "evt-updated".to_string(),
            "evt-assigned".to_string(),
            "legacy-start".to_string()
        ],
        "{:?}",
        after.tags
    );
}

#[test]
fn set_replaces_collections_and_scalars_last_writer_wins() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Replacement order", &["original"]);
    set_automation(
        &tasks_dir,
        "    - name: replace-then-add\n      when: {status: InProgress}\n      on:\n        updated:\n          set: {tags: [set-first], priority: Low}\n          add: {tags: [add-after]}\n    - name: later-writer\n      when: {status: InProgress}\n      on:\n        updated:\n          set: {priority: High}\n          add: {tags: [cross-rule]}\n",
    );

    start_in_progress(&mut storage, &task.id);

    let after = get_task(&storage, &task.id);
    assert_eq!(
        after.tags,
        vec![
            "set-first".to_string(),
            "add-after".to_string(),
            "cross-rule".to_string()
        ],
        "{:?}",
        after.tags
    );
    // set.tags replaced the seeded tag; the later rule's scalar write wins.
    assert!(!after.tags.contains(&"original".to_string()));
    assert!(after.priority.to_string().contains("High"));
}

#[test]
fn conditions_and_templates_keep_event_values_after_earlier_mutations() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Event snapshot", &[]);
    set_automation(
        &tasks_dir,
        "    - name: mutator\n      when: {status: InProgress}\n      on: {updated: {set: {status: Done}, add: {tags: [mutated]}}}\n    - name: observer\n      when: {status: InProgress}\n      on:\n        updated:\n          add: {tags: [observed]}\n          comment: \"Was ${{previous.status}}, event ${{ticket.status}}\"\n",
    );

    start_in_progress(&mut storage, &task.id);

    let after = get_task(&storage, &task.id);
    // Both rules matched the event (status changed to InProgress) even though
    // the first rule had already persisted status Done when the second ran.
    assert!(
        after.tags.contains(&"mutated".to_string()),
        "{:?}",
        after.tags
    );
    assert!(
        after.tags.contains(&"observed".to_string()),
        "{:?}",
        after.tags
    );
    assert!(after.status.to_string().contains("Done"));
    // Templates resolved event-time values, not the mutated persisted state.
    assert!(
        after
            .comments
            .iter()
            .any(|c| c.text.contains("Was Todo, event InProgress")),
        "{:?}",
        after.comments
    );
}

#[test]
fn comment_event_dispatch_composes_across_rules() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Comment dispatch", &[]);
    set_automation(
        &tasks_dir,
        "    - name: comment-one\n      on:\n        commented:\n          add: {tags: [comment-1]}\n          comment: \"Saw ${{comment.text}}\"\n    - name: comment-two\n      on: {commented: {add: {tags: [comment-2]}}}\n",
    );

    TaskService::add_comment(&mut storage, &task.id, "hello world").expect("add comment");

    let after = get_task(&storage, &task.id);
    assert_eq!(
        after.tags,
        vec!["comment-1".to_string(), "comment-2".to_string()],
        "{:?}",
        after.tags
    );
    assert!(
        after
            .comments
            .iter()
            .any(|c| c.text.contains("Saw hello world")),
        "{:?}",
        after.comments
    );
}

#[test]
fn job_event_dispatch_composes_across_rules() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Job dispatch", &[]);
    set_automation(
        &tasks_dir,
        "    - name: job-one\n      on: {complete: {add: {tags: [job-a]}}}\n    - name: job-two\n      on: {complete: {add: {tags: [job-b]}}}\n",
    );

    AutomationService::apply_job_event(&tasks_dir, &task.id, AutomationEvent::JobCompleted, None)
        .expect("apply job event");

    let after = get_task(&storage, &task.id);
    assert_eq!(
        after.tags,
        vec!["job-a".to_string(), "job-b".to_string()],
        "{:?}",
        after.tags
    );
}

#[test]
fn comment_only_rule_keeps_earlier_collection_updates() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Comment only", &[]);
    set_automation(
        &tasks_dir,
        "    - name: collector\n      when: {status: InProgress}\n      on: {updated: {add: {tags: [kept]}}}\n    - name: notifier\n      when: {status: InProgress}\n      on: {updated: {comment: noted by automation}}\n",
    );

    start_in_progress(&mut storage, &task.id);

    let after = get_task(&storage, &task.id);
    assert_eq!(after.tags, vec!["kept".to_string()], "{:?}", after.tags);
    assert!(
        after
            .comments
            .iter()
            .any(|c| c.text.contains("noted by automation")),
        "{:?}",
        after.comments
    );
}

#[test]
fn automation_writes_do_not_recurse() {
    let (_tmp, tasks_dir) = workspace();
    let mut storage = Storage::new(&tasks_dir);
    let task = create_task(&mut storage, "Ping pong guard", &[]);
    set_automation(
        &tasks_dir,
        "    - name: to-done\n      when: {status: InProgress}\n      on: {updated: {set: {status: Done}, add: {tags: [guard-applied]}}}\n    - name: back-to-in-progress\n      when: {status: Done}\n      on: {updated: {set: {status: InProgress}, add: {tags: [pong]}}}\n",
    );

    start_in_progress(&mut storage, &task.id);

    let after = get_task(&storage, &task.id);
    // The first rule's automation write must not re-dispatch: the second
    // rule (which would match a user-driven change to Done) never fires.
    assert!(after.status.to_string().contains("Done"));
    assert_eq!(tag_count(&after, "guard-applied"), 1, "{:?}", after.tags);
    assert!(
        !after.tags.contains(&"pong".to_string()),
        "{:?}",
        after.tags
    );
}
