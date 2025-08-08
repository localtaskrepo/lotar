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
        let path = if let Some(scan_path) = args.path {
            PathBuf::from(scan_path)
        } else {
            project::get_project_path().unwrap_or_else(|| {
                renderer.emit_warning("No path specified. Using current directory.");
                PathBuf::from(".")
            })
        };

        if !path.exists() {
            return Err(format!("Path '{}' does not exist", path.display()));
        }

        if !matches!(renderer.format, crate::output::OutputFormat::Json) {
            renderer.emit_info(&format!("Scanning {} for TODO comments...", path.display()));
        }

        renderer.log_debug(&format!("scan: scanning path={}", path.display()));
        let mut scanner = scanner::Scanner::new(path);
        let results = scanner.scan();

        if matches!(renderer.format, crate::output::OutputFormat::Json) {
            let items: Vec<serde_json::Value> = results
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
        } else if results.is_empty() {
            renderer.emit_success("No TODO comments found.");
        } else {
            renderer.emit_info(&format!("Found {} TODO comment(s):", results.len()));
            for entry in results {
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
