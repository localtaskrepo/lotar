//! DEV-21: configurable task completion policy — configuration contract.
//!
//! Covers the `issue.done_states` surface end to end:
//! - legacy behavior when the key is absent (inferred, after project
//!   resolution);
//! - explicit lists as the authoritative policy (mid-list terminal states,
//!   excluding conventionally-done names);
//! - project overrides, inheritance of explicit globals, empty-CSV clears,
//!   and the preserved explicit pin that equals the global value;
//! - canonical validation (empty list, duplicates, unknown statuses,
//!   subset-of-issue.states) across candidates, affected projects,
//!   dry runs, and force;
//! - the computed `config show` / `config inspect` fields.

mod common;

use std::collections::BTreeMap;

use lotar::services::config_service::ConfigService;
use lotar::workspace::{TasksDirectoryResolver, TasksDirectorySource};
use tempfile::TempDir;

fn resolver_for(path: &std::path::Path) -> TasksDirectoryResolver {
    TasksDirectoryResolver {
        path: path.to_path_buf(),
        source: TasksDirectorySource::CurrentDirectory,
    }
}

fn setup() -> (TempDir, std::path::PathBuf, TasksDirectoryResolver) {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let resolver = resolver_for(&tasks_dir);
    (tmp, tasks_dir, resolver)
}

fn set_fields(
    resolver: &TasksDirectoryResolver,
    values: &[(&str, &str)],
    global: bool,
    project: Option<&str>,
) -> Result<lotar::services::config_service::ConfigSetOutcome, String> {
    let map: BTreeMap<String, String> = values
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    ConfigService::set(resolver, &map, global, project).map_err(|e| e.to_string())
}

fn write_global_config(tasks_dir: &std::path::Path, body: &str) {
    std::fs::write(tasks_dir.join("config.yml"), body).unwrap();
}

#[test]
fn unset_done_states_keep_legacy_inferred_mode() {
    let (_tmp, tasks_dir, resolver) = setup();
    write_global_config(&tasks_dir, "issue:\n  states: [Todo, InProgress, Done]\n");

    let show = ConfigService::show(&resolver, None).unwrap();
    assert_eq!(show["done_states_mode"].as_str(), Some("inferred"));
    // Inferred set: the last state plus conventionally-named states.
    let effective = show["effective_done_states"]
        .as_array()
        .expect("effective_done_states array");
    let values: Vec<&str> = effective.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        values.iter().any(|v| v.eq_ignore_ascii_case("done")),
        "inferred set must include done: {values:?}"
    );
    // Raw resolved view keeps the optional field unset.
    assert!(show["issue_done_states"].is_null());

    let inspect = ConfigService::inspect(&resolver, None).unwrap();
    for view in [&inspect["effective"], &inspect["global_effective"]] {
        assert_eq!(view["done_states_mode"].as_str(), Some("inferred"));
        assert!(
            view["task_calendar_day"]
                .as_str()
                .is_some_and(|day| day.len() == 10 && day.as_bytes()[4] == b'-'),
            "task_calendar_day must be a live YYYY-MM-DD value"
        );
    }
    // The live calendar day equals today's server-local date.
    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    assert_eq!(
        inspect["global_effective"]["task_calendar_day"].as_str(),
        Some(today.as_str())
    );
}

#[test]
fn explicit_global_done_states_are_authoritative() {
    let (_tmp, tasks_dir, resolver) = setup();
    write_global_config(
        &tasks_dir,
        "issue:\n  states: [Backlog, Todo, InProgress, Closed, Shipped, Archived]\n  done_states: [Closed, Shipped]\n",
    );

    let show = ConfigService::show(&resolver, None).unwrap();
    assert_eq!(show["done_states_mode"].as_str(), Some("explicit"));
    let values: Vec<&str> = show["effective_done_states"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    // Mid-list terminal states are honored in the configured order.
    assert_eq!(values, vec!["Closed", "Shipped"]);
    // The raw canonical file round-trips the explicit list.
    let raw = std::fs::read_to_string(tasks_dir.join("config.yml")).unwrap();
    assert!(raw.contains("done_states"));
}

#[test]
fn explicit_done_states_can_exclude_conventional_done() {
    let (_tmp, tasks_dir, resolver) = setup();
    write_global_config(
        &tasks_dir,
        "issue:\n  states: [Todo, InProgress, Done, Shipped]\n  done_states: [Shipped]\n",
    );
    // A Done task must NOT classify as done under this policy; verified via
    // the shared completion module directly (runtime surfaces are covered
    // by the runtime regression file).
    let config = lotar::config::resolution::load_and_merge_configs(Some(&tasks_dir)).unwrap();
    let done = lotar::services::completion::effective_done_statuses(&config);
    assert!(done.contains("shipped"));
    assert!(!done.contains("done"), "explicit list excludes Done");
    let _ = resolver; // resolver unused beyond setup shape
}

#[test]
fn project_overrides_inherit_and_clear() {
    let (_tmp, tasks_dir, resolver) = setup();
    write_global_config(
        &tasks_dir,
        "issue:\n  states: [Todo, InProgress, Done, Shipped]\n  done_states: [Shipped]\n",
    );
    std::fs::create_dir_all(tasks_dir.join("EX")).unwrap();
    std::fs::write(
        tasks_dir.join("EX/config.yml"),
        "project:\n  name: Example\nissue:\n  done_states: [Done]\n",
    )
    .unwrap();

    // Project override wins over the global explicit list.
    let show = ConfigService::show(&resolver, Some("EX")).unwrap();
    assert_eq!(show["done_states_mode"].as_str(), Some("explicit"));
    let values: Vec<&str> = show["effective_done_states"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(values, vec!["Done"]);

    // A project without its own override inherits the explicit global.
    std::fs::create_dir_all(tasks_dir.join("WH")).unwrap();
    let show = ConfigService::show(&resolver, Some("WH")).unwrap();
    assert_eq!(show["done_states_mode"].as_str(), Some("explicit"));
    let values: Vec<&str> = show["effective_done_states"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(values, vec!["Shipped"]);

    // Clearing the project override with an empty CSV returns to inherit.
    set_fields(&resolver, &[("issue_done_states", "")], false, Some("EX")).unwrap();
    let show = ConfigService::show(&resolver, Some("EX")).unwrap();
    let values: Vec<&str> = show["effective_done_states"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(values, vec!["Shipped"]);
}

#[test]
fn explicit_project_pin_equal_to_global_is_preserved() {
    let (_tmp, tasks_dir, resolver) = setup();
    write_global_config(
        &tasks_dir,
        "issue:\n  states: [Todo, InProgress, Done, Shipped]\n  done_states: [Shipped]\n",
    );
    std::fs::create_dir_all(tasks_dir.join("EX")).unwrap();
    std::fs::write(
        tasks_dir.join("EX/config.yml"),
        "project:\n  name: Example\n",
    )
    .unwrap();

    // Setting a project value equal to the global must NOT be deduplicated
    // into a clear: the pin expresses intent and must survive.
    set_fields(
        &resolver,
        &[("issue_done_states", "Shipped")],
        false,
        Some("EX"),
    )
    .unwrap();
    let raw = std::fs::read_to_string(tasks_dir.join("EX/config.yml")).unwrap();
    assert!(
        raw.contains("done_states"),
        "explicit project pin must be persisted: {raw}"
    );

    // ...and a later global change must not silently re-point the project.
    set_fields(
        &resolver,
        &[("issue_done_states", "Done, Shipped")],
        true,
        None,
    )
    .unwrap();
    let show = ConfigService::show(&resolver, Some("EX")).unwrap();
    let values: Vec<&str> = show["effective_done_states"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(
        values,
        vec!["Shipped"],
        "project pin survives global change"
    );
}

#[test]
fn canonical_validation_rejects_empty_duplicates_and_unknowns() {
    let (_tmp, _tasks_dir, resolver) = setup();

    // Literal empty list is invalid.
    let err = set_fields(&resolver, &[("issue_done_states", "[]")], true, None).unwrap_err();
    assert!(err.to_lowercase().contains("at least one"), "{err}");

    // Duplicates are rejected.
    let err = set_fields(
        &resolver,
        &[("issue_done_states", "Done, Done")],
        true,
        None,
    )
    .unwrap_err();
    assert!(err.to_lowercase().contains("duplicate"), "{err}");

    // Unknown statuses are rejected by the token parser.
    let err = set_fields(
        &resolver,
        &[("issue_done_states", "NotAStatus")],
        true,
        None,
    )
    .unwrap_err();
    assert!(!err.is_empty());

    // Values outside issue_states fail resolved-config validation.
    write_global_config(&_tasks_dir, "issue:\n  states: [Todo, Done]\n");
    let err = set_fields(&resolver, &[("issue_done_states", "Shipped")], true, None).unwrap_err();
    assert!(
        err.to_lowercase().contains("issue_done_states"),
        "subset violation must name the field: {err}"
    );
}

#[test]
fn global_candidate_revalidates_affected_projects_and_dry_run_writes_nothing() {
    let (_tmp, tasks_dir, resolver) = setup();
    write_global_config(
        &tasks_dir,
        "issue:\n  states: [Todo, InProgress, Done, Shipped]\n",
    );
    // PX narrows its states without its own done override, so it inherits
    // the global done policy and IS affected by global done-state changes.
    std::fs::create_dir_all(tasks_dir.join("PX")).unwrap();
    std::fs::write(
        tasks_dir.join("PX/config.yml"),
        "project:\n  name: Narrow\nissue:\n  states: [Todo, InProgress]\n",
    )
    .unwrap();

    // Global done list referencing a status PX does not have must be
    // blocked as a newly-introduced resolved error for the project.
    let err = set_fields(&resolver, &[("issue_done_states", "Shipped")], true, None).unwrap_err();
    assert!(
        err.contains("PX") && err.to_lowercase().contains("resolved"),
        "affected project must block the candidate: {err}"
    );

    // Dry run performs the same validation without writing.
    use lotar::config::candidate::{self, ConfigScope, ConfigSetRequest};
    let result = candidate::apply_config_set(
        &tasks_dir,
        &ConfigScope::Global,
        &ConfigSetRequest {
            entries: vec![("issue_done_states".to_string(), "Shipped".to_string())],
            force: false,
            dry_run: true,
        },
    );
    assert!(result.is_err(), "dry run must validate identically");
    let raw = std::fs::read_to_string(tasks_dir.join("config.yml")).unwrap();
    assert!(!raw.contains("done_states"), "dry run must not write");

    // A global candidate valid for every affected project applies, and PX
    // inherits it explicitly (subset of both the global list and PX's own
    // narrowed states).
    set_fields(
        &resolver,
        &[("issue_done_states", "InProgress")],
        true,
        None,
    )
    .unwrap();
    let show = ConfigService::show(&resolver, Some("PX")).unwrap();
    assert_eq!(show["done_states_mode"].as_str(), Some("explicit"));
    let values: Vec<&str> = show["effective_done_states"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(values, vec!["InProgress"]);
}

#[test]
fn global_unset_restores_inferred_mode() {
    let (_tmp, tasks_dir, resolver) = setup();
    set_fields(&resolver, &[("issue_done_states", "Done")], true, None).unwrap();
    let show = ConfigService::show(&resolver, None).unwrap();
    assert_eq!(show["done_states_mode"].as_str(), Some("explicit"));

    // Empty global CSV unsets the explicit list and restores inference.
    set_fields(&resolver, &[("issue_done_states", "")], true, None).unwrap();
    let show = ConfigService::show(&resolver, None).unwrap();
    assert_eq!(show["done_states_mode"].as_str(), Some("inferred"));
    let raw = std::fs::read_to_string(tasks_dir.join("config.yml")).unwrap();
    assert!(!raw.contains("done_states"), "unset key is not written");
}
