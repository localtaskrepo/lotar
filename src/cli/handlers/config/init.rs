use super::ConfigHandler;
use crate::output::OutputRenderer;
use crate::utils::project::{
    generate_project_prefix, generate_unique_project_prefix, validate_explicit_prefix,
};
use crate::workspace::TasksDirectoryResolver;
use serde_yaml;
use std::fs;

impl ConfigHandler {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_config_init(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        template: String,
        prefix: Option<String>,
        project: Option<String>,
        copy_from: Option<String>,
        global: bool,
        dry_run: bool,
        force: bool,
    ) -> Result<(), String> {
        if dry_run {
            renderer.emit_info(format_args!(
                "DRY RUN: Would initialize config with template '{}'",
                template
            ));
            if let Some(ref prefix) = prefix {
                renderer.emit_raw_stdout(format_args!("  • Project prefix: {}", prefix));
                if let Some(ref project_name) = project {
                    if let Err(conflict) =
                        validate_explicit_prefix(prefix, project_name, &resolver.path)
                    {
                        renderer
                            .emit_raw_stdout(format_args!("  ❌ Conflict detected: {}", conflict));
                        return Err(conflict);
                    }
                    renderer.emit_raw_stdout(format_args!("  ✅ Prefix '{}' is available", prefix));
                }
            }
            if let Some(ref project) = project {
                renderer.emit_raw_stdout(format_args!("  • Project name: {}", project));
                if prefix.is_none() {
                    match generate_unique_project_prefix(project, &resolver.path) {
                        Ok(generated_prefix) => {
                            renderer.emit_raw_stdout(format_args!(
                                "  • Generated prefix: {} ✅",
                                generated_prefix
                            ));
                        }
                        Err(conflict) => {
                            renderer.emit_raw_stdout(format_args!(
                                "  • Generated prefix: {} ❌",
                                generate_project_prefix(project)
                            ));
                            renderer.emit_raw_stdout(format_args!(
                                "  ❌ Conflict detected: {}",
                                conflict
                            ));
                            return Err(conflict);
                        }
                    }
                }
            }
            if let Some(ref copy_from) = copy_from {
                renderer.emit_raw_stdout(format_args!("  • Copy settings from: {}", copy_from));
            }
            if global {
                renderer.emit_raw_stdout("  • Target: Global configuration (.tasks/config.yml)");
            } else {
                let project_name = project.as_deref().unwrap_or("DEFAULT");
                let project_prefix = if let Some(ref prefix) = prefix {
                    prefix.clone()
                } else {
                    match generate_unique_project_prefix(project_name, &resolver.path) {
                        Ok(prefix) => prefix,
                        Err(_) => generate_project_prefix(project_name),
                    }
                };
                renderer.emit_raw_stdout(format_args!(
                    "  • Target: Project configuration (.tasks/{}/config.yml)",
                    project_prefix
                ));
            }
            renderer.emit_success(
                "Dry run completed. Use the same command without --dry-run to apply.",
            );
            return Ok(());
        }

        renderer.emit_info(format_args!(
            "Initializing configuration with template '{}'",
            template
        ));

        let template_config = Self::load_template(&template)?;

        Self::apply_template_config(
            resolver,
            renderer,
            template_config,
            prefix,
            project,
            copy_from,
            global,
            force,
        )
    }

    fn load_template(template_name: &str) -> Result<serde_yaml::Value, String> {
        let template_content = match template_name {
            "default" => include_str!("../../../config/templates/default.yml"),
            "agile" => include_str!("../../../config/templates/agile.yml"),
            "kanban" => include_str!("../../../config/templates/kanban.yml"),
            "jira" => include_str!("../../../config/templates/jira.yml"),
            "github" => include_str!("../../../config/templates/github.yml"),
            "jira-github" => include_str!("../../../config/templates/jira-github.yml"),
            _ => return Err(format!("Unknown template: {}", template_name)),
        };

        serde_yaml::from_str(template_content)
            .map_err(|e| format!("Failed to parse template '{}': {}", template_name, e))
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_template_config(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        template: serde_yaml::Value,
        prefix: Option<String>,
        project: Option<String>,
        copy_from: Option<String>,
        global: bool,
        force: bool,
    ) -> Result<(), String> {
        let template_map = template.as_mapping().ok_or("Invalid template format")?;

        let config_section = template_map
            .get(serde_yaml::Value::String("config".to_string()))
            .ok_or("Template missing 'config' section")?;

        let mut config = config_section.clone();

        if let Some(config_map) = config.as_mapping_mut() {
            Self::normalize_template_config(config_map);
        }

        if let Some(config_map) = config.as_mapping_mut() {
            if let Some(source_project) = copy_from.as_deref() {
                Self::merge_config_from_project(config_map, resolver, renderer, source_project)?;
                Self::normalize_template_config(config_map);
            }

            if let Some(project_name) = project.as_ref() {
                Self::set_nested_value(
                    config_map,
                    &["project", "name"],
                    serde_yaml::Value::String(project_name.clone()),
                );
            }
        }

        let config_path = if global {
            fs::create_dir_all(&resolver.path)
                .map_err(|e| format!("Failed to create tasks directory: {}", e))?;
            crate::utils::paths::global_config_path(&resolver.path)
        } else {
            let detected_project_name = Self::extract_project_name(&config);
            let project_name_owned = project
                .as_deref()
                .map(|s| s.to_string())
                .or_else(|| detected_project_name.clone())
                .unwrap_or_else(|| "DEFAULT".to_string());

            let project_prefix = if let Some(explicit_prefix) = &prefix {
                validate_explicit_prefix(explicit_prefix, &project_name_owned, &resolver.path)?;
                explicit_prefix.clone()
            } else {
                generate_unique_project_prefix(&project_name_owned, &resolver.path)?
            };

            let project_dir = crate::utils::paths::project_dir(&resolver.path, &project_prefix);
            fs::create_dir_all(&project_dir)
                .map_err(|e| format!("Failed to create project directory: {}", e))?;
            crate::utils::paths::project_config_path(&resolver.path, &project_prefix)
        };

        if config_path.exists() && !force {
            return Err(format!(
                "Configuration already exists at {}. Use --force to overwrite.",
                config_path.display()
            ));
        }

        let tmp_yaml = serde_yaml::to_string(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        let canonical = if config_path.parent() == Some(&resolver.path) {
            let parsed = crate::config::normalization::parse_global_from_yaml_str(&tmp_yaml)
                .map_err(|e| format!("Failed to parse config for canonicalization: {}", e))?;
            Self::validate_generated_global_config(resolver, renderer, &parsed)?;
            crate::config::normalization::to_canonical_global_yaml(&parsed)
        } else {
            let prefix = config_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let detected_project_name = Self::extract_project_name(&config);
            let project_name_value = project
                .as_deref()
                .or(detected_project_name.as_deref())
                .unwrap_or(prefix);
            let parsed = crate::config::normalization::parse_project_from_yaml_str(
                project_name_value,
                &tmp_yaml,
            )
            .map_err(|e| format!("Failed to parse project config for canonicalization: {}", e))?;
            Self::validate_generated_project_config(resolver, renderer, &parsed)?;
            crate::config::normalization::to_canonical_project_yaml(&parsed)
        };

        fs::write(&config_path, canonical)
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        renderer.emit_success(format_args!(
            "Configuration initialized at: {}",
            config_path.display()
        ));
        Ok(())
    }

    fn merge_config_from_project(
        target_config: &mut serde_yaml::Mapping,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        source_project: &str,
    ) -> Result<(), String> {
        let source_config_path =
            crate::utils::paths::project_config_path(&resolver.path, source_project);

        if !source_config_path.exists() {
            return Err(format!(
                "Source project '{}' does not exist",
                source_project
            ));
        }

        let source_content = fs::read_to_string(&source_config_path)
            .map_err(|e| format!("Failed to read source config: {}", e))?;

        let source_config: serde_yaml::Value = serde_yaml::from_str(&source_content)
            .map_err(|e| format!("Failed to parse source config: {}", e))?;

        if let Some(source_map) = source_config.as_mapping() {
            for (key, value) in source_map {
                if let Some(key_str) = key.as_str()
                    && key_str != "project_name"
                    && key_str != "prefix"
                    && key_str != "project"
                {
                    target_config.insert(key.clone(), value.clone());
                }
            }
        }

        renderer.emit_info(format_args!(
            "Copied settings from project '{}'",
            source_project
        ));
        Ok(())
    }

    fn normalize_template_config(config_map: &mut serde_yaml::Mapping) {
        use serde_yaml::Value as Y;

        if let Some(project_name_value) = config_map.remove(Y::String("project_name".into())) {
            Self::set_nested_value(config_map, &["project", "name"], project_name_value);
        }

        config_map.remove(Y::String("prefix".into()));

        if let Some(value) = config_map.remove(Y::String("issue_states".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "states"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("issue_types".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "types"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("issue_priorities".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "priorities"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("tags".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "tags"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("categories".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["issue", "categories"], normalized);
        }
        if let Some(value) = config_map.remove(Y::String("custom_fields".into())) {
            let normalized = Self::unwrap_template_values(value);
            Self::set_nested_value(config_map, &["custom", "fields"], normalized);
        }
    }

    fn unwrap_template_values(value: serde_yaml::Value) -> serde_yaml::Value {
        use serde_yaml::Value as Y;
        match value {
            Y::Mapping(mut map) => {
                let values_key = Y::String("values".into());
                if let Some(inner) = map.remove(&values_key) {
                    return inner;
                }
                let primitive_key = Y::String("primitive".into());
                if let Some(inner) = map.remove(&primitive_key) {
                    return inner;
                }
                Y::Mapping(map)
            }
            other => other,
        }
    }

    fn set_nested_value(map: &mut serde_yaml::Mapping, path: &[&str], value: serde_yaml::Value) {
        use serde_yaml::Value as Y;

        if path.is_empty() {
            return;
        }

        let key = Y::String(path[0].to_string());
        if path.len() == 1 {
            map.insert(key, value);
            return;
        }

        let mut child = match map.remove(&key) {
            Some(Y::Mapping(existing)) => existing,
            _ => serde_yaml::Mapping::new(),
        };

        Self::set_nested_value(&mut child, &path[1..], value);
        map.insert(key, Y::Mapping(child));
    }

    fn extract_project_name(config: &serde_yaml::Value) -> Option<String> {
        use serde_yaml::Value as Y;

        let map = config.as_mapping()?;
        if let Some(project_value) = map.get(Y::String("project".into()))
            && let Some(project_map) = project_value.as_mapping()
            && let Some(name_value) = project_map.get(Y::String("name".into()))
            && let Some(name_str) = name_value.as_str()
        {
            let trimmed = name_str.trim();
            if trimmed.is_empty() || trimmed.contains("{{") {
                return None;
            }
            return Some(trimmed.to_string());
        }

        if let Some(legacy_name) = map.get(Y::String("project_name".into()))
            && let Some(name_str) = legacy_name.as_str()
        {
            let trimmed = name_str.trim();
            if trimmed.is_empty() || trimmed.contains("{{") {
                return None;
            }
            return Some(trimmed.to_string());
        }

        None
    }

    fn validate_generated_project_config(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        config: &crate::config::types::ProjectConfig,
    ) -> Result<(), String> {
        let validator = crate::config::validation::ConfigValidator::new(&resolver.path);
        let result = validator.validate_project_config(config);

        for warning in &result.warnings {
            renderer.emit_warning(warning.to_string());
        }

        if result.has_errors() {
            for error in &result.errors {
                renderer.emit_error(error.to_string());
            }
            return Err("Generated project configuration failed validation".to_string());
        }

        Ok(())
    }

    fn validate_generated_global_config(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        config: &crate::config::types::GlobalConfig,
    ) -> Result<(), String> {
        let validator = crate::config::validation::ConfigValidator::new(&resolver.path);
        let result = validator.validate_global_config(config);

        for warning in &result.warnings {
            renderer.emit_warning(warning.to_string());
        }

        if result.has_errors() {
            for error in &result.errors {
                renderer.emit_error(error.to_string());
            }
            return Err("Generated global configuration failed validation".to_string());
        }

        Ok(())
    }
}
