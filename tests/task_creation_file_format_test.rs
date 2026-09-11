use lotar::api_types::TaskCreate;
use lotar::services::task_service::TaskService;
use serde_yaml_ng::Value as YamlValue;

mod common;

#[test]
fn new_task_file_omits_modified_and_history() {
    let fixtures = common::TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let request = TaskCreate {
        title: "Fresh Task".to_string(),
        project: Some("TEST".to_string()),
        ..TaskCreate::default()
    };

    let created = TaskService::create(&mut storage, request).expect("task creation succeeds");
    assert!(created.id.starts_with("TEST-"));

    let task_file = fixtures.tasks_root.join("TEST").join("1.yml");
    let contents = std::fs::read_to_string(&task_file).expect("task file exists");

    assert!(
        contents.contains("type: Feature"),
        "type key should be present"
    );
    assert!(
        !contents.contains("task_type:"),
        "legacy task_type key should be omitted"
    );
    assert!(
        contents.contains("created:"),
        "created timestamp should be present"
    );
    assert!(
        !contents.contains("modified:"),
        "modified should be omitted until a change occurs"
    );
    assert!(
        !contents.contains("history:"),
        "history should be empty and omitted on creation"
    );
}

/// DEV-23: a default-valued creation must persist the resolved defaults as
/// explicit YAML keys (status, priority, type) instead of relying on implicit
/// configuration, and a reload of the same file must agree with them.
#[test]
fn default_valued_creation_persists_explicit_status_priority_and_type_keys() {
    let fixtures = common::TestFixtures::new();
    let mut storage = fixtures.create_storage();

    let request = TaskCreate {
        title: "Defaults Only".to_string(),
        project: Some("DFLT".to_string()),
        status: None,
        priority: None,
        task_type: None,
        ..TaskCreate::default()
    };

    let created = TaskService::create(&mut storage, request)
        .expect("creation with omitted enum fields succeeds");
    assert_eq!(created.id, "DFLT-1");

    let task_file = fixtures.tasks_root.join("DFLT").join("1.yml");
    let contents = std::fs::read_to_string(&task_file).expect("task file exists");
    let persisted = task_yaml_mapping(&contents);

    // Built-in defaults: first state (Todo), configured default priority
    // (Medium, deliberately not the first list entry), and first type (Feature).
    assert_eq!(
        yaml_str(&persisted, "status"),
        "Todo",
        "resolved default status must be an explicit YAML key"
    );
    assert_eq!(
        yaml_str(&persisted, "priority"),
        "Medium",
        "resolved default priority must be an explicit YAML key"
    );
    assert_eq!(
        yaml_str(&persisted, "type"),
        "Feature",
        "resolved default task type must be an explicit YAML key"
    );

    // Reload through the service (fresh storage handle) and require the
    // persisted values to survive a full round-trip without config inference.
    let reloaded_storage = fixtures.create_storage();
    let reloaded = TaskService::get(&reloaded_storage, "DFLT-1", None)
        .expect("reload of default-valued task succeeds");
    assert_eq!(reloaded.status.as_str(), "Todo");
    assert_eq!(reloaded.priority.as_str(), "Medium");
    assert_eq!(reloaded.task_type.as_str(), "Feature");
}

/// DEV-23: the same contract must hold when a project configures defaults that
/// are not the first entries of their lists (default.status / default.priority
/// pointing at non-first values, and a first issue type that differs from the
/// global Feature default). Creation must serialize those configured defaults
/// explicitly and reload must agree.
#[test]
fn default_valued_creation_persists_configured_non_first_project_defaults() {
    let fixtures = common::TestFixtures::new();

    let project_dir = fixtures.tasks_root.join("ALT");
    std::fs::create_dir_all(&project_dir).expect("create project dir");
    std::fs::write(
        project_dir.join("config.yml"),
        "default:\n  status: InProgress\n  priority: High\nissue:\n  states: [Todo, InProgress, Done]\n  priorities: [Low, Medium, High]\n  types: [Bug, Feature, Chore]\n",
    )
    .expect("write project config");

    let mut storage = fixtures.create_storage();
    let request = TaskCreate {
        title: "Configured Defaults".to_string(),
        project: Some("ALT".to_string()),
        status: None,
        priority: None,
        task_type: None,
        ..TaskCreate::default()
    };

    let created = TaskService::create(&mut storage, request)
        .expect("creation against configured project succeeds");
    assert_eq!(created.id, "ALT-1");
    assert_eq!(created.status.as_str(), "InProgress");
    assert_eq!(created.priority.as_str(), "High");
    assert_eq!(created.task_type.as_str(), "Bug");

    let task_file = fixtures.tasks_root.join("ALT").join("1.yml");
    let contents = std::fs::read_to_string(&task_file).expect("task file exists");
    let persisted = task_yaml_mapping(&contents);

    // Non-first configured defaults must be explicit in the file, proving the
    // serialized values come from project configuration rather than the
    // global first-entry fallbacks (Todo / Medium / Feature).
    assert_eq!(
        yaml_str(&persisted, "status"),
        "InProgress",
        "configured non-first default status must be an explicit YAML key"
    );
    assert_eq!(
        yaml_str(&persisted, "priority"),
        "High",
        "configured non-first default priority must be an explicit YAML key"
    );
    assert_eq!(
        yaml_str(&persisted, "type"),
        "Bug",
        "configured project-first default type must be an explicit YAML key"
    );

    // Reload from disk with a fresh storage handle and require agreement.
    let reloaded_storage = fixtures.create_storage();
    let reloaded = TaskService::get(&reloaded_storage, "ALT-1", None)
        .expect("reload of configured-default task succeeds");
    assert_eq!(reloaded.status.as_str(), "InProgress");
    assert_eq!(reloaded.priority.as_str(), "High");
    assert_eq!(reloaded.task_type.as_str(), "Bug");
}

fn task_yaml_mapping(contents: &str) -> serde_yaml_ng::Mapping {
    let doc: YamlValue = serde_yaml_ng::from_str(contents).expect("task YAML parses");
    doc.as_mapping()
        .expect("task YAML root is a mapping")
        .clone()
}

fn yaml_str<'a>(map: &'a serde_yaml_ng::Mapping, key: &str) -> &'a str {
    map.get(YamlValue::String(key.to_string()))
        .and_then(YamlValue::as_str)
        .unwrap_or_else(|| panic!("expected explicit '{key}' key with a string value"))
}
