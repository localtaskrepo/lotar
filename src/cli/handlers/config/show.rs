use super::ConfigHandler;
use super::render::{YamlRenderOptions, emit_config_yaml};
use crate::config::ConfigManager;
use crate::config::source_labels::{build_global_source_labels, build_project_source_labels};
use crate::output::OutputRenderer;
use crate::services::attachment_service::AttachmentService;
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

        let resolved_tasks_dir = if effective_read_root.is_relative() {
            std::fs::canonicalize(&effective_read_root).ok()
        } else {
            None
        };

        let tasks_dir_for_paths = resolved_tasks_dir
            .as_ref()
            .unwrap_or(&effective_read_root)
            .clone();

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

            let attachments_root = AttachmentService::compute_attachments_root(
                &tasks_dir_for_paths,
                &resolved_project,
            );
            let (uploads_mode, uploads_limit_bytes) =
                describe_upload_policy(resolved_project.attachments_max_upload_mb);

            if !matches!(renderer.format, crate::output::OutputFormat::Json) {
                if let Ok(root) = &attachments_root {
                    renderer.emit_info(format!("Attachments root: {}", root.display()));
                }
                renderer.emit_info(format!("Attachment uploads: {uploads_mode}"));
            }

            let mut json_meta = serde_json::json!({
                "paths": {
                    "tasks_dir": tasks_dir_for_paths.display().to_string(),
                },
                "attachments": {
                    "uploads": uploads_mode,
                    "max_upload_bytes": uploads_limit_bytes,
                }
            });
            if let Some(resolved) = resolved_tasks_dir.as_ref() {
                json_meta["paths"]["tasks_dir_resolved"] =
                    serde_json::Value::String(resolved.display().to_string());
            }
            match attachments_root {
                Ok(root) => {
                    json_meta["paths"]["attachments_root"] =
                        serde_json::Value::String(root.display().to_string());
                }
                Err(err) => {
                    json_meta["paths"]["attachments_root_error"] =
                        serde_json::Value::String(err.to_string());
                }
            }

            emit_config_yaml(
                renderer,
                "project",
                Some(project_label.as_str()),
                &resolved_project,
                &project_sources,
                &options,
                Some(json_meta),
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

            let attachments_root =
                AttachmentService::compute_attachments_root(&tasks_dir_for_paths, resolved_config);
            let (uploads_mode, uploads_limit_bytes) =
                describe_upload_policy(resolved_config.attachments_max_upload_mb);

            if !matches!(renderer.format, crate::output::OutputFormat::Json) {
                if let Ok(root) = &attachments_root {
                    renderer.emit_info(format!("Attachments root: {}", root.display()));
                }
                renderer.emit_info(format!("Attachment uploads: {uploads_mode}"));
            }

            let mut json_meta = serde_json::json!({
                "paths": {
                    "tasks_dir": tasks_dir_for_paths.display().to_string(),
                },
                "attachments": {
                    "uploads": uploads_mode,
                    "max_upload_bytes": uploads_limit_bytes,
                }
            });
            if let Some(resolved) = resolved_tasks_dir.as_ref() {
                json_meta["paths"]["tasks_dir_resolved"] =
                    serde_json::Value::String(resolved.display().to_string());
            }
            match attachments_root {
                Ok(root) => {
                    json_meta["paths"]["attachments_root"] =
                        serde_json::Value::String(root.display().to_string());
                }
                Err(err) => {
                    json_meta["paths"]["attachments_root_error"] =
                        serde_json::Value::String(err.to_string());
                }
            }

            emit_config_yaml(
                renderer,
                "global",
                None,
                resolved_config,
                &sources,
                &options,
                Some(json_meta),
            );
        }

        Ok(())
    }
}

fn describe_upload_policy(max_upload_mb: i64) -> (String, Option<u64>) {
    match max_upload_mb {
        0 => ("disabled".to_string(), None),
        -1 => ("unlimited".to_string(), None),
        n if n > 0 => {
            let bytes = u64::try_from(n)
                .ok()
                .and_then(|v| v.checked_mul(1024 * 1024));
            (format!("limited ({n} MiB)"), bytes)
        }
        other => (format!("invalid ({other})"), None),
    }
}
