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

fn ensure_tasks_dir(root: &std::path::Path) -> std::path::PathBuf {
    let tasks_dir = root.join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    tasks_dir
}

#[test]
fn show_and_inspect_redact_every_config_shape() {
    use lotar::config::{
        manager::ConfigManager,
        types::{GlobalConfig, ProjectConfig},
    };
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);
    let agents = serde_json::from_value(serde_json::json!({
        "worker": {"runner": "codex", "env": {"FAKE_KEY": "fake-agent-secret"}, "args": ["--quiet"]},
        "short": "codex"
    })).unwrap();
    let auth = serde_json::from_value(serde_json::json!({
        "example": {"provider": "github", "method": "token", "token_env": "fake-token-secret", "email_env": "fake-email-secret", "api_url": "https://example.invalid/api"}
    })).unwrap();
    let global = GlobalConfig {
        agents,
        auth_profiles: auth,
        ..GlobalConfig::default()
    };
    ConfigManager::save_global_config(&tasks_dir, &global).unwrap();
    let mut project = ProjectConfig::new("Example Project".into());
    project.agents = Some(global.agents.clone());
    project.auth_profiles = global.auth_profiles.clone();
    ConfigManager::save_project_config(&tasks_dir, "EX", &project).unwrap();
    for scope in [None, Some("EX")] {
        let show = ConfigService::show(&resolver, scope).unwrap();
        let inspect = ConfigService::inspect(&resolver, scope).unwrap();
        for response in [&show, &inspect] {
            let serialized = response.to_string();
            for secret in [
                "fake-agent-secret",
                "fake-token-secret",
                "fake-email-secret",
                "FAKE_KEY",
            ] {
                assert!(
                    !serialized.contains(secret),
                    "sensitive field was not redacted"
                );
            }
        }
        for effective in [&show, &inspect["effective"], &inspect["global_effective"]] {
            assert_eq!(effective["agent_profiles"]["worker"]["runner"], "codex");
            assert_eq!(effective["agent_profiles"]["worker"]["args"][0], "--quiet");
        }
        assert_eq!(inspect["auth_profiles"]["example"]["method"], "token");
        assert_eq!(
            inspect["global_raw"]["auth_profiles"]["example"]["api_url"],
            "https://example.invalid/api"
        );
    }
}

#[test]
fn inspect_reports_sources_for_global_scope() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut global_values = BTreeMap::new();
    global_values.insert("default_project".to_string(), "ACME".to_string());
    ConfigService::set(&resolver, &global_values, true, None).expect("set global prefix");

    let payload = ConfigService::inspect(&resolver, None).expect("inspect global scope");
    let effective = payload["effective"].as_object().expect("effective config");
    let sources = payload["sources"].as_object().expect("sources object");

    assert_eq!(effective["default_project"].as_str(), Some("ACME"));
    assert!(effective.get("default_prefix").is_none());
    assert_eq!(sources["default_project"].as_str(), Some("global"));
    let tags_source = sources["tags"].as_str();
    assert!(
        matches!(tags_source, Some("built_in") | Some("global")),
        "unexpected tags source: {tags_source:?}"
    );
}

#[test]
fn canonical_project_defaults_and_names_are_used_without_losing_settings() {
    use lotar::config::{
        manager::{ConfigManager, test_support},
        persistence,
    };
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    std::fs::create_dir(tasks_dir.join("EX")).unwrap();
    std::fs::write(
        tasks_dir.join("EX/config.yml"),
        "project:\n  name: Example Full Name\n",
    )
    .unwrap();
    let path = tasks_dir.join("config.yml");
    std::fs::write(
        &path,
        "default:\n  assignee: example\nscan:\n  signal_words: [FIXME]\n",
    )
    .unwrap();
    let mut manager = ConfigManager::new_manager_with_tasks_dir_readonly(&tasks_dir).unwrap();
    assert_eq!(manager.ensure_default_project(&tasks_dir).unwrap(), "EX");
    let config = persistence::load_global_config(Some(&tasks_dir)).unwrap();
    assert_eq!(config.default_project, "EX");
    assert_eq!(config.default_assignee.as_deref(), Some("example"));
    assert_eq!(config.scan_signal_words, vec!["FIXME".to_string()]);
    assert_eq!(
        lotar::project::get_effective_project_name(&resolver_for(&tasks_dir)),
        "EX"
    );
    assert_eq!(
        lotar::utils::project::resolve_project_input("Example Full Name", &tasks_dir),
        "EX"
    );
    std::fs::write(&path, "default: [broken").unwrap();
    let mut resolved = manager.get_resolved_config().clone();
    resolved.default_project.clear();
    let mut manager = test_support::from_resolved_config(resolved);
    assert!(manager.ensure_default_project(&tasks_dir).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "default: [broken");
}

#[test]
fn inspect_reports_project_overrides_with_shared_helpers() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut global_values = BTreeMap::new();
    global_values.insert("default_project".to_string(), "ACME".to_string());
    ConfigService::set(&resolver, &global_values, true, None).expect("set global prefix");

    ConfigService::create_project(&resolver, "Acme", Some("ACME"), None).expect("create project");

    let mut project_values = BTreeMap::new();
    project_values.insert("default_priority".to_string(), "High".to_string());
    ConfigService::set(&resolver, &project_values, false, Some("ACME"))
        .expect("set project override");

    let payload = ConfigService::inspect(&resolver, Some("ACME")).expect("inspect project scope");
    let sources = payload["sources"].as_object().expect("sources object");

    assert_eq!(sources["default_priority"].as_str(), Some("project"));
    assert_eq!(sources["default_project"].as_str(), Some("global"));
}
