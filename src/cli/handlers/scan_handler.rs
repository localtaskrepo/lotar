use crate::cli::ScanArgs;
use crate::cli::handlers::CommandHandler;
use crate::output::OutputRenderer;
use crate::project;
use crate::scanner;
use crate::workspace::TasksDirectoryResolver;
use std::path::PathBuf;

/// Handler for scan command
pub struct ScanHandler;

impl CommandHandler for ScanHandler {
    type Args = ScanArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        _project: Option<&str>,
        _resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        renderer.log_info("scan: begin");
        let default_root = project::get_project_path().unwrap_or_else(|| PathBuf::from("."));
        let roots: Vec<PathBuf> = if args.paths.is_empty() {
            vec![default_root]
        } else {
            args.paths.iter().map(PathBuf::from).collect()
        };

        // Validate all roots exist
        for root in &roots {
            if !root.exists() {
                return Err(format!("Path '{}' does not exist", root.display()));
            }
        }

        if !matches!(renderer.format, crate::output::OutputFormat::Json) {
            if roots.len() == 1 {
                renderer.emit_info(&format!(
                    "Scanning {} for TODO comments...",
                    roots[0].display()
                ));
            } else {
                let joined = roots
                    .iter()
                    .map(|r| r.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                renderer.emit_info(&format!("Scanning multiple paths: {}", joined));
            }
        }

        // Load config (global or project-specific) to obtain scan signal words
        let cfg_words: Vec<String> = {
            let mgr = crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
                _resolver.path.as_path(),
            )
            .ok();
            if let Some(mgr) = mgr {
                if let Some(project_name) = _project {
                    if let Ok(resolved) = mgr.get_project_config(project_name) {
                        resolved.scan_signal_words
                    } else {
                        mgr.get_resolved_config().scan_signal_words.clone()
                    }
                } else {
                    mgr.get_resolved_config().scan_signal_words.clone()
                }
            } else {
                // Fallback: use scanner defaults by returning empty to skip override
                Vec::new()
            }
        };

        let mut all_results = Vec::new();
        for root in roots {
            renderer.log_debug(&format!("scan: scanning path={}", root.display()));
            let mut scanner = scanner::Scanner::new(root)
                .with_include_ext(&args.include)
                .with_exclude_ext(&args.exclude);
            if !cfg_words.is_empty() {
                scanner = scanner.with_signal_words(&cfg_words);
            }
            let mut results = scanner.scan();
            all_results.append(&mut results);
        }

        // Ensure deterministic ordering across roots
        all_results.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.line_number.cmp(&b.line_number))
        });

        if matches!(renderer.format, crate::output::OutputFormat::Json) {
            let items: Vec<serde_json::Value> = all_results
                .iter()
                .map(|entry| {
                    serde_json::json!({
                        "file": entry.file_path,
                        "line": entry.line_number,
                        "title": entry.title,
                        "annotation": entry.annotation
                    })
                })
                .collect();
            renderer.emit_raw_stdout(&serde_json::to_string(&items).unwrap());
        } else if all_results.is_empty() {
            renderer.emit_success("No TODO comments found.");
        } else {
            renderer.emit_info(&format!("Found {} TODO comment(s):", all_results.len()));
            for entry in all_results {
                if args.detailed {
                    renderer.emit_raw_stdout(&format!("  📄 {}", entry.file_path.display()));
                    renderer.emit_raw_stdout(&format!(
                        "    Line {}: {}",
                        entry.line_number,
                        entry.title.trim()
                    ));
                    if !entry.annotation.is_empty() {
                        renderer.emit_raw_stdout(&format!("    Note: {}", entry.annotation));
                    }
                    renderer.emit_raw_stdout("");
                } else {
                    renderer.emit_raw_stdout(&format!(
                        "  {}:{} - {}",
                        entry.file_path.display(),
                        entry.line_number,
                        entry.title.trim()
                    ));
                }
            }
        }

        Ok(())
    }
}
