use super::ConfigHandler;
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

impl ConfigHandler {
    pub(super) fn handle_config_normalize(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        global: bool,
        project: Option<String>,
        write: bool,
    ) -> Result<(), String> {
        use crate::utils::paths;
        use std::path::PathBuf;

        let mut targets: Vec<(String, PathBuf)> = Vec::new();

        if global || project.is_none() {
            let path = paths::global_config_path(&resolver.path);
            if path.exists() {
                targets.push(("global".to_string(), path));
            }
        }

        if let Some(proj) = project {
            let path = paths::project_config_path(&resolver.path, &proj);
            if !path.exists() {
                return Err(format!("Project config not found: {}", path.display()));
            }
            targets.push((format!("project:{}", proj), path));
        } else if !global {
            // If neither --global nor --project specified, normalize all project configs
            for (prefix, dir) in crate::utils::filesystem::list_visible_subdirs(&resolver.path) {
                let cfg = dir.join("config.yml");
                if cfg.exists() {
                    targets.push((format!("project:{}", prefix), cfg));
                }
            }
        }

        if targets.is_empty() {
            renderer.emit_info("No configuration files found to normalize");
            return Ok(());
        }

        let mut changed = 0usize;
        for (label, path) in targets {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

            // Attempt normalization via round-trip through our tolerant parsers,
            // then emit in canonical nested style.
            let canonical = if label == "global" {
                let parsed = crate::config::normalization::parse_global_from_yaml_str(&content)
                    .map_err(|e| e.to_string())?;
                crate::config::normalization::to_canonical_global_yaml(&parsed)
            } else {
                // label is project:<prefix>
                let proj = label.split_once(':').map(|(_, p)| p).unwrap_or("");
                let parsed =
                    crate::config::normalization::parse_project_from_yaml_str(proj, &content)
                        .map_err(|e| e.to_string())?;
                crate::config::normalization::to_canonical_project_yaml(&parsed)
            };

            if write {
                std::fs::write(&path, canonical.as_bytes())
                    .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
                renderer.emit_success(format_args!("Normalized {} -> {}", label, path.display()));
                changed += 1;
            } else {
                renderer.emit_info(format_args!(
                    "Would normalize {} -> {}",
                    label,
                    path.display()
                ));
                renderer.emit_raw_stdout(&canonical);
            }
        }

        if write {
            renderer.emit_success(format_args!(
                "Normalization complete ({} file(s) updated)",
                changed
            ));
        }
        Ok(())
    }
}
