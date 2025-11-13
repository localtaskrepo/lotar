use super::ConfigHandler;
use super::render::{YamlRenderOptions, emit_config_yaml};
use crate::config::ConfigManager;
use crate::config::source_labels::{build_global_source_labels, build_project_source_labels};
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;
use std::io::IsTerminal;

impl ConfigHandler {
    pub(super) fn handle_config_show(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        project: Option<String>,
        explain: bool,
        full: bool,
    ) -> Result<(), String> {
        let effective_read_root = resolver.path.clone();

        let config_manager =
            ConfigManager::new_manager_with_tasks_dir_readonly(&effective_read_root)
                .map_err(|e| format!("Failed to load config: {}", e))?;

        let colorize_comments =
            std::env::var("NO_COLOR").is_err() && std::io::stdout().is_terminal();

        if !matches!(renderer.format, crate::output::OutputFormat::Json) {
            let mut message = format!("Tasks directory: {}", effective_read_root.display());
            if effective_read_root.is_relative()
                && let Ok(resolved) = std::fs::canonicalize(&effective_read_root)
            {
                message.push_str(" (resolved: ");
                message.push_str(&resolved.display().to_string());
                message.push(')');
            }
            renderer.emit_info(&message);
        }

        if let Some(project_name) = project {
            let project_prefix = crate::utils::project::resolve_project_input(
                &project_name,
                resolver.path.as_path(),
            );
            let resolved_project = config_manager
                .get_project_config(&project_prefix)
                .map_err(|e| format!("Failed to load project config: {}", e))?;

            let project_cfg_raw = crate::config::persistence::load_project_config_from_dir(
                &project_prefix,
                &effective_read_root,
            )
            .ok();
            let project_label = crate::utils::project::format_project_label(
                &project_prefix,
                project_cfg_raw.as_ref().and_then(|cfg| {
                    let name = cfg.project_name.trim();
                    if name.is_empty() { None } else { Some(name) }
                }),
            );
            let global_cfg =
                crate::config::persistence::load_global_config(Some(&effective_read_root)).ok();
            let home_cfg = crate::config::persistence::load_home_config().ok();
            let base_config = config_manager.get_resolved_config().clone();
            let project_sources = build_project_source_labels(
                &resolved_project,
                &base_config,
                project_cfg_raw.as_ref(),
                &global_cfg,
                &home_cfg,
            );

            const PROJECT_ONLY_SOURCES: &[&str] = &["project"];
            let options = YamlRenderOptions {
                include_defaults: full,
                include_comments: explain,
                colorize_comments,
                allowed_sources: if full {
                    None
                } else {
                    Some(PROJECT_ONLY_SOURCES)
                },
            };

            emit_config_yaml(
                renderer,
                "project",
                Some(project_label.as_str()),
                &resolved_project,
                &project_sources,
                &options,
            );
        } else {
            let resolved_config = config_manager.get_resolved_config();
            let global_cfg =
                crate::config::persistence::load_global_config(Some(&effective_read_root)).ok();
            let home_cfg = crate::config::persistence::load_home_config().ok();
            let sources = build_global_source_labels(resolved_config, &global_cfg, &home_cfg);

            const GLOBAL_SOURCES: &[&str] = &["env", "home", "global"];
            let options = YamlRenderOptions {
                include_defaults: full,
                include_comments: explain,
                colorize_comments,
                allowed_sources: if full { None } else { Some(GLOBAL_SOURCES) },
            };

            emit_config_yaml(
                renderer,
                "global",
                None,
                resolved_config,
                &sources,
                &options,
            );
        }

        Ok(())
    }
}
