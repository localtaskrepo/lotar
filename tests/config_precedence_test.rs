use std::collections::BTreeMap;

use lotar::config::resolution::{get_project_config, load_and_merge_configs};
use lotar::services::config_service::ConfigService;
use lotar::types::TaskStatus;
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

struct EnvVarGuard {
    key: String,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: tests serialize environment mutations.
        unsafe {
            std::env::set_var(key, value);
        }
        Self {
            key: key.to_string(),
            previous,
        }
    }
}

struct CliOverrideGuard;

impl CliOverrideGuard {
    fn set(pairs: &[(&str, &str)]) -> Self {
        let overrides = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<Vec<_>>();
        lotar::config::resolution::configure_cli_overrides(&overrides)
            .expect("configure CLI overrides");
        Self
    }
}

impl Drop for CliOverrideGuard {
    fn drop(&mut self) {
        lotar::config::resolution::clear_cli_overrides();
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(prev) = &self.previous {
            // SAFETY: tests serialize environment mutations.
            unsafe {
                std::env::set_var(&self.key, prev);
            }
        } else {
            // SAFETY: tests serialize environment mutations.
            unsafe {
                std::env::remove_var(&self.key);
            }
        }
    }
}

#[test]
fn project_config_overrides_environment_defaults() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut global_values = BTreeMap::new();
    global_values.insert("default_project".to_string(), "ENG".to_string());
    ConfigService::set(&resolver, &global_values, true, None).expect("seed global default prefix");

    let mut project_values = BTreeMap::new();
    project_values.insert("default_status".to_string(), "InProgress".to_string());
    ConfigService::set(&resolver, &project_values, false, Some("ENG"))
        .expect("apply project default status");

    let _guard = EnvVarGuard::set("LOTAR_DEFAULT_STATUS", "Done");

    let baseline = load_and_merge_configs(Some(tasks_dir.as_path())).expect("resolved base config");
    let resolved =
        get_project_config(&baseline, "ENG", tasks_dir.as_path()).expect("project config");

    assert_eq!(
        resolved.default_status,
        Some(TaskStatus::from("InProgress")),
        "project-specific default should win over env override"
    );
}

#[test]
fn environment_fills_gap_when_project_omits_value() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut global_values = BTreeMap::new();
    global_values.insert("default_project".to_string(), "OPS".to_string());
    ConfigService::set(&resolver, &global_values, true, None).expect("seed global default prefix");

    // Create a project config file without overriding default_status
    let mut project_values = BTreeMap::new();
    project_values.insert("project_name".to_string(), "Ops".to_string());
    ConfigService::set(&resolver, &project_values, false, Some("OPS"))
        .expect("create project config without overrides");

    let _guard = EnvVarGuard::set("LOTAR_DEFAULT_STATUS", "Done");

    let baseline = load_and_merge_configs(Some(tasks_dir.as_path())).expect("resolved base config");
    let resolved =
        get_project_config(&baseline, "OPS", tasks_dir.as_path()).expect("project config");

    assert_eq!(
        resolved.default_status,
        Some(TaskStatus::from("Done")),
        "env override should still apply when project does not set the field"
    );
}

#[test]
fn cli_overrides_have_highest_priority() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut global_values = BTreeMap::new();
    global_values.insert("default_project".to_string(), "CLI".to_string());
    ConfigService::set(&resolver, &global_values, true, None).expect("seed global default prefix");

    let mut project_values = BTreeMap::new();
    project_values.insert("project_name".to_string(), "Cli Demo".to_string());
    project_values.insert("default_status".to_string(), "InProgress".to_string());
    ConfigService::set(&resolver, &project_values, false, Some("CLI"))
        .expect("set project default status");

    let _env_guard = EnvVarGuard::set("LOTAR_DEFAULT_STATUS", "Done");
    let _cli_guard = CliOverrideGuard::set(&[("default_status", "Todo")]);

    let baseline = load_and_merge_configs(Some(tasks_dir.as_path())).expect("base config");
    let resolved =
        get_project_config(&baseline, "CLI", tasks_dir.as_path()).expect("project config");

    assert_eq!(
        resolved.default_status,
        Some(TaskStatus::from("Todo")),
        "CLI overrides should win even when project/env attempt to set the same field"
    );
}

#[test]
fn project_overrides_can_toggle_auto_codeowners() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = ensure_tasks_dir(tmp.path());
    let resolver = resolver_for(&tasks_dir);

    let mut global_values = BTreeMap::new();
    global_values.insert("default_project".to_string(), "AUTO".to_string());
    global_values.insert("auto_codeowners_assign".to_string(), "false".to_string());
    ConfigService::set(&resolver, &global_values, true, None).expect("seed global config");

    let mut project_values = BTreeMap::new();
    project_values.insert("project_name".to_string(), "Auto Demo".to_string());
    project_values.insert("auto_codeowners_assign".to_string(), "true".to_string());
    ConfigService::set(&resolver, &project_values, false, Some("AUTO"))
        .expect("set project auto toggles");

    let baseline = load_and_merge_configs(Some(tasks_dir.as_path())).expect("base config");
    assert!(
        !baseline.auto_codeowners_assign,
        "global scope should remain false"
    );

    let resolved =
        get_project_config(&baseline, "AUTO", tasks_dir.as_path()).expect("project config");
    assert!(
        resolved.auto_codeowners_assign,
        "project override should flip auto_codeowners_assign"
    );
}
