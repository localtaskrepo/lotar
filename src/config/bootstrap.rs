use crate::config::types::GlobalConfig;
use crate::utils::project::generate_unique_project_prefix;
use std::fs;
use std::path::Path;

/// Ensure the global configuration exists for the provided tasks root.
///
/// When no configuration is present we create one with a best-effort
/// default project prefix derived from the explicit `project_context`,
/// the surrounding repository name, or existing project folders.
pub fn ensure_global_config(
    root_path: &Path,
    project_context: Option<&str>,
) -> std::io::Result<()> {
    let global_config_path = crate::utils::paths::global_config_path(root_path);

    if global_config_path.exists() {
        return Ok(());
    }

    if let Some(parent) = global_config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut global_config = GlobalConfig::default();

    if let Some(prefix) = determine_smart_default_project(root_path, project_context) {
        global_config.default_project = prefix;
    }

    let config_yaml = crate::config::normalization::to_canonical_global_yaml(&global_config);
    fs::write(&global_config_path, config_yaml)
}

fn determine_smart_default_project(
    root_path: &Path,
    project_context: Option<&str>,
) -> Option<String> {
    if let Some(project_name) = project_context
        && let Ok(prefix) = generate_unique_project_prefix(project_name, root_path)
    {
        return Some(prefix);
    }

    if let Some(auto_detected) = crate::project::detect_project_name()
        && let Ok(prefix) = generate_unique_project_prefix(&auto_detected, root_path)
    {
        return Some(prefix);
    }

    crate::utils::filesystem::list_visible_subdirs(root_path)
        .into_iter()
        .map(|(dir_name, _)| dir_name)
        .next()
}
