use crate::cli::ScanArgs;
use crate::cli::handlers::AddHandler;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::output::OutputRenderer;
use crate::project;
use crate::scanner;
use crate::workspace::TasksDirectoryResolver;
use std::path::PathBuf;
// feature-aware custom fields manipulation helpers are implemented below

fn edit_task_with_context<F>(
    resolver: &TasksDirectoryResolver,
    task_id: &str,
    project_hint: Option<&str>,
    mutator: F,
) -> Result<(), String>
where
    F: FnOnce(&mut crate::storage::task::Task) -> bool,
{
    let mut ctx = TaskCommandContext::new(resolver, project_hint, Some(task_id))?;
    let LoadedTask {
        full_id, mut task, ..
    } = load_task(&mut ctx, task_id, project_hint)?;
    let changed = mutator(&mut task);
    if changed {
        task.modified = chrono::Utc::now().to_rfc3339();
        ctx.storage
            .edit(&full_id, &task)
            .map_err(TaskStorageAction::Update.map_err(&full_id))?;
    }
    Ok(())
}

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
                renderer.emit_info(format_args!(
                    "Scanning {} for TODO comments...",
                    roots[0].display()
                ));
            } else {
                let joined = roots
                    .iter()
                    .map(|r| r.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                renderer.emit_info(format_args!("Scanning multiple paths: {}", joined));
            }
        }

        // Resolve effective project early for config purposes (so project overrides apply)
        let mut project_resolver = crate::cli::project::ProjectResolver::new(_resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;
        let effective_project_for_config = match project_resolver.resolve_project("", _project) {
            Ok(project) => {
                if project.is_empty() {
                    None
                } else {
                    Some(project)
                }
            }
            Err(_) => None,
        };

        // Load config (global or project-specific) to obtain scan settings
        let (
            cfg_words,
            cfg_ticket_patterns,
            cfg_enable_ticket_words,
            cfg_enable_mentions,
            issue_type_words,
        ): (Vec<String>, Option<Vec<String>>, bool, bool, Vec<String>) = {
            let mgr = crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
                _resolver.path.as_path(),
            )
            .ok();
            if let Some(mgr) = mgr {
                if let Some(project_name) = effective_project_for_config.as_deref() {
                    if let Ok(resolved) = mgr.get_project_config(project_name) {
                        let mut type_words: Vec<String> = Vec::new();
                        if resolved.scan_enable_ticket_words {
                            type_words = resolved
                                .issue_types
                                .values
                                .iter()
                                .map(|t| t.to_string())
                                .collect();
                        }
                        (
                            resolved.scan_signal_words,
                            resolved.scan_ticket_patterns,
                            resolved.scan_enable_ticket_words,
                            resolved.scan_enable_mentions,
                            type_words,
                        )
                    } else {
                        let r = mgr.get_resolved_config();
                        let mut type_words: Vec<String> = Vec::new();
                        if r.scan_enable_ticket_words {
                            type_words =
                                r.issue_types.values.iter().map(|t| t.to_string()).collect();
                        }
                        (
                            r.scan_signal_words.clone(),
                            r.scan_ticket_patterns.clone(),
                            r.scan_enable_ticket_words,
                            r.scan_enable_mentions,
                            type_words,
                        )
                    }
                } else {
                    let r = mgr.get_resolved_config();
                    let mut type_words: Vec<String> = Vec::new();
                    if r.scan_enable_ticket_words {
                        type_words = r.issue_types.values.iter().map(|t| t.to_string()).collect();
                    }
                    (
                        r.scan_signal_words.clone(),
                        r.scan_ticket_patterns.clone(),
                        r.scan_enable_ticket_words,
                        r.scan_enable_mentions,
                        type_words,
                    )
                }
            } else {
                // Fallback: use scanner defaults by returning empty to skip override
                (Vec::new(), None, false, true, Vec::new())
            }
        };

        let mut all_results = Vec::new();
        for root in roots {
            renderer.log_debug(format_args!("scan: scanning path={}", root.display()));
            let mut scanner = scanner::Scanner::new(root)
                .with_include_ext(&args.include)
                .with_exclude_ext(&args.exclude)
                .with_modified_only(args.modified_only);
            // Merge regular signal words with issue-type words if enabled
            let mut final_words = cfg_words.clone();
            for w in &issue_type_words {
                if !final_words.iter().any(|v| v.eq_ignore_ascii_case(w)) {
                    final_words.push(w.clone());
                }
            }
            if !final_words.is_empty() {
                scanner = scanner.with_signal_words(&final_words);
            }
            scanner = scanner
                .with_ticket_detection(cfg_ticket_patterns.as_deref(), cfg_enable_ticket_words);
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
                        "uuid": entry.uuid,
                        "annotation": entry.annotation
                    })
                })
                .collect();
            match serde_json::to_string(&items) {
                Ok(s) => renderer.emit_raw_stdout(s),
                Err(e) => renderer.emit_raw_stdout(
                    serde_json::json!({"status":"error","message":format!("scan serialization failed: {}", e)}).to_string(),
                ),
            }
        } else if all_results.is_empty() {
            renderer.emit_success("No TODO comments found.");
            // Even if we didn't find TODO comments, we can still try to relocate anchors for existing tasks
            if !args.dry_run {
                Self::reanchor_existing_references(_resolver, renderer, None)?;
            }
        } else {
            renderer.emit_info(format_args!("Found {} TODO comment(s):", all_results.len()));
            // When applying, we'll need a storage context for creating tasks
            let effective_project = match project_resolver.resolve_project("", _project) {
                Ok(project) => {
                    if project.is_empty() {
                        None
                    } else {
                        Some(project)
                    }
                }
                Err(e) => return Err(e),
            };

            // Resolve config to decide attribute stripping policy if not overridden
            let cfg_strip = if let Some(p) = effective_project_for_config.as_deref() {
                project_resolver
                    .get_project_config(p)
                    .map_err(|e| format!("Failed to resolve project config: {}", e))?
                    .scan_strip_attributes
            } else {
                project_resolver.get_config().scan_strip_attributes
            };
            let strip_attributes = args.strip_attributes.unwrap_or(cfg_strip);

            // Before applying insertions, attempt to re-anchor any existing references that drifted.
            if !args.dry_run {
                // Provide a small window for proximity search
                Self::reanchor_existing_references(_resolver, renderer, Some(7))?;
            }

            let mut applied = 0usize;
            for entry in all_results {
                // If detailed flag is set, emit a per-file header line
                if args.detailed {
                    renderer.emit_raw_stdout(format_args!("  📄 {}", entry.file_path.display()));
                }
                // Default behavior: apply changes (unless --dry-run)
                // Read file and target line
                if let Ok(contents) = std::fs::read_to_string(&entry.file_path) {
                    let all_lines: Vec<&str> = contents.lines().collect();
                    if let Some(orig_line) = all_lines.get(entry.line_number - 1).copied() {
                        let tmp_scanner = scanner::Scanner::new(PathBuf::from("."))
                            .with_ticket_detection(
                                cfg_ticket_patterns.as_deref(),
                                cfg_enable_ticket_words,
                            );
                        let existing_key = tmp_scanner.extract_ticket_key_from_line(orig_line);
                        if existing_key.is_none() {
                            // Parse inline attributes from the original line before any stripping
                            let inline_attrs = parse_inline_attributes(orig_line);
                            // Create task title from entry.title
                            // Reuse AddHandler with smart defaults
                            let cli_add_args = crate::cli::AddArgs {
                                title: entry.title.clone(),
                                task_type: inline_attrs.task_type,
                                priority: inline_attrs.priority,
                                reporter: None,
                                assignee: inline_attrs.assignee,
                                effort: inline_attrs.effort,
                                due: inline_attrs.due,
                                description: None,
                                tags: inline_attrs.tags,
                                fields: inline_attrs.fields,
                                bug: false,
                                epic: false,
                                critical: false,
                                high: false,
                                dry_run: false,
                                explain: false,
                            };
                            let task_id = if args.dry_run {
                                // Simulate an ID for preview purposes; we can use a placeholder
                                // Use project prefix if known; else DEFAULT
                                let effective = match &effective_project {
                                    Some(p) => p.clone(),
                                    None => crate::project::get_effective_project_name(_resolver),
                                };
                                format!("{}-NEW", effective)
                            } else {
                                AddHandler::execute(
                                    cli_add_args,
                                    effective_project.as_deref(),
                                    _resolver,
                                    renderer,
                                )?
                            };

                            // If not dry-run, persist a reverse link in the created task (bi-directional reference)
                            if !args.dry_run {
                                let rel =
                                    crate::utils::paths::repo_relative_display(&entry.file_path);
                                let line_number = entry.line_number;
                                let reanchor = args.reanchor;
                                if let Err(err) =
                                    edit_task_with_context(_resolver, &task_id, None, |task| {
                                        let code_ref = format!("{}#L{}", rel, line_number);
                                        let has_exact = task
                                            .references
                                            .iter()
                                            .any(|r| r.code.as_deref() == Some(code_ref.as_str()));

                                        if reanchor {
                                            let before_len = task.references.len();
                                            task.references.retain(|r| {
                                                r.code.as_deref() == Some(code_ref.as_str())
                                            });
                                            let mut changed = before_len != task.references.len();
                                            if !has_exact {
                                                task.references.push(
                                                    crate::types::ReferenceEntry {
                                                        code: Some(code_ref),
                                                        link: None,
                                                    },
                                                );
                                                changed = true;
                                            }
                                            return changed;
                                        }

                                        if has_exact {
                                            return false;
                                        }

                                        let file_key = rel.as_str();
                                        task.references.retain(|r| {
                                            if let Some(code) = &r.code
                                                && let Some((file_part, _)) = code.split_once("#L")
                                                && file_part == file_key
                                                && code != code_ref.as_str()
                                            {
                                                return false;
                                            }
                                            true
                                        });
                                        task.references.push(crate::types::ReferenceEntry {
                                            code: Some(code_ref),
                                            link: None,
                                        });
                                        true
                                    })
                                {
                                    renderer.log_debug(format_args!(
                                        "scan: unable to update references for {}: {}",
                                        task_id, err
                                    ));
                                }
                            }

                            // Insert (KEY) after signal word; optionally strip bracket attributes
                            let mut new_line =
                                match tmp_scanner.suggest_insertion_for_line(orig_line, &task_id) {
                                    Some(l) => l,
                                    None => orig_line.to_string(),
                                };
                            if strip_attributes {
                                new_line = strip_bracket_attributes(&new_line);
                            }

                            // Optional: emit context snippet when requested
                            if args.detailed && args.context > 0 {
                                let start = entry.line_number.saturating_sub(args.context);
                                // clamp to at least 1
                                let start = if start == 0 { 1 } else { start };
                                let end = (entry.line_number + args.context).min(all_lines.len());
                                for ln in start..=end {
                                    if ln == entry.line_number {
                                        continue;
                                    }
                                    if let Some(ctx) = all_lines.get(ln - 1) {
                                        renderer.emit_raw_stdout(format_args!(
                                            "    {}:{}",
                                            entry.file_path.display(),
                                            ln
                                        ));
                                        renderer.emit_raw_stdout(format_args!("      {}", ctx));
                                    }
                                }
                            }

                            // Only emit and write when a real change occurs
                            if new_line != orig_line
                                && let Some(updated) =
                                    replace_line(&contents, entry.line_number, &new_line)
                            {
                                if args.dry_run {
                                    renderer.emit_raw_stdout(format_args!(
                                        "  📄 {}:{}\n    - {}\n    + {}",
                                        entry.file_path.display(),
                                        entry.line_number,
                                        orig_line,
                                        new_line
                                    ));
                                } else if let Err(e) = std::fs::write(&entry.file_path, updated) {
                                    renderer.log_error(format_args!(
                                        "Failed to write changes to {}: {}",
                                        entry.file_path.display(),
                                        e
                                    ));
                                } else {
                                    renderer.emit_raw_stdout(format_args!(
                                        "  📄 {}:{}\n    - {}\n    + {}",
                                        entry.file_path.display(),
                                        entry.line_number,
                                        orig_line,
                                        new_line
                                    ));
                                    applied += 1;
                                }
                            }
                        } else if !args.dry_run && cfg_enable_mentions {
                            // Movement/relocation resilience: if an existing key is present, ensure
                            // the corresponding task has a code reference for this file+line.
                            if let Some(task_id) = existing_key {
                                let rel =
                                    crate::utils::paths::repo_relative_display(&entry.file_path);
                                let line_number = entry.line_number;
                                let reanchor = args.reanchor;
                                if let Err(err) =
                                    edit_task_with_context(_resolver, &task_id, None, |task| {
                                        let code_ref = format!("{}#L{}", rel, line_number);
                                        let has_exact = task
                                            .references
                                            .iter()
                                            .any(|r| r.code.as_deref() == Some(code_ref.as_str()));

                                        if reanchor {
                                            let before_len = task.references.len();
                                            task.references.retain(|r| {
                                                r.code.as_deref() == Some(code_ref.as_str())
                                            });
                                            let mut changed = before_len != task.references.len();
                                            if !has_exact {
                                                task.references.push(
                                                    crate::types::ReferenceEntry {
                                                        code: Some(code_ref),
                                                        link: None,
                                                    },
                                                );
                                                changed = true;
                                            }
                                            return changed;
                                        }

                                        if has_exact {
                                            return false;
                                        }

                                        let file_key = rel.as_str();
                                        task.references.retain(|r| {
                                            if let Some(code) = &r.code
                                                && let Some((file_part, _)) = code.split_once("#L")
                                                && file_part == file_key
                                                && code != code_ref.as_str()
                                            {
                                                return false;
                                            }
                                            true
                                        });
                                        task.references.push(crate::types::ReferenceEntry {
                                            code: Some(code_ref),
                                            link: None,
                                        });
                                        true
                                    })
                                {
                                    renderer.log_debug(format_args!(
                                        "scan: unable to refresh anchor for {}: {}",
                                        task_id, err
                                    ));
                                }
                            }
                        }
                    }
                }
                // Apply path (and dry-run preview) prints patch above when changes occur; lines with existing keys are left untouched
            }
            if !args.dry_run && applied > 0 {
                renderer.emit_success(format_args!("Applied {} update(s).", applied));
            }
        }

        Ok(())
    }
}

/// Replace a specific 1-based line in the file content and return the new content
fn replace_line(contents: &str, line_number_1based: usize, new_line: &str) -> Option<String> {
    let mut lines: Vec<&str> = contents.lines().collect();
    if line_number_1based == 0 || line_number_1based > lines.len() {
        return None;
    }
    lines[line_number_1based - 1] = new_line;
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        out.push_str(line);
        if i + 1 < lines.len() || contents.ends_with('\n') {
            out.push('\n');
        }
    }
    Some(out)
}

/// Strip inline attribute blocks like [key=value] without altering any other whitespace.
/// This preserves all spacing/alignment and only removes bracket sections that contain
/// an equals sign, which distinguishes them from language generics (e.g., Vec<String>)
/// or indexers (e.g., arr[0]).
fn strip_bracket_attributes(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth = 0usize;
    let mut buf = String::new(); // collect content when inside brackets

    for ch in line.chars() {
        match ch {
            '[' => {
                if depth == 0 {
                    // starting a new top-level bracket; reset buffer and decide later
                    buf.clear();
                } else {
                    // nested bracket content
                    buf.push('[');
                }
                depth += 1;
            }
            ']' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        // End of a top-level bracket. Decide whether to drop it.
                        // If the inner content contains an '=', treat it as an attribute and drop.
                        // Otherwise, keep the original bracket with its content.
                        if buf.contains('=') {
                            // drop entire [ ... ] including its content; do not write anything
                        } else {
                            out.push('[');
                            out.push_str(&buf);
                            out.push(']');
                        }
                        buf.clear();
                        continue;
                    } else {
                        // closing a nested level inside top-level; record literal
                        buf.push(']');
                        continue;
                    }
                }
                // Unbalanced ']' outside any bracket: write through
                out.push(']');
            }
            c => {
                if depth == 0 {
                    out.push(c);
                } else {
                    buf.push(c);
                }
            }
        }
    }
    // If brackets are unbalanced and we're still inside, write them back literally
    if depth > 0 {
        out.push('[');
        out.push_str(&buf);
    }
    out
}

#[cfg(test)]
mod scan_handler_tests {
    use super::{parse_inline_attributes, strip_bracket_attributes};

    #[test]
    fn strip_preserves_leading_indentation() {
        let input = "\t    // TODO: Do it [assignee=me]  [priority=high]";
        let out = strip_bracket_attributes(input);
        assert!(out.starts_with("\t    // TODO: Do it"));
        // Ensure no brackets remain
        assert!(!out.contains('[') && !out.contains(']'));
        // Ensure indentation didn't collapse
        assert_eq!(&out[..5], "\t    ");
    }

    #[test]
    fn strip_does_not_collapse_spacing_or_alignments() {
        let input = "    signal_words: Vec<String>,                     // TODO handle words [tag=scan]  [due=2025-12-31]";
        let out = strip_bracket_attributes(input);
        // Leading spaces preserved
        assert!(out.starts_with(
            "    signal_words: Vec<String>,                     // TODO handle words"
        ));
        // No brackets remain
        assert!(!out.contains('[') && !out.contains(']'));
        // The run of spaces before the comment should still be long (>= 5)
        let after_comma = out.split("Vec<String>,").nth(1).unwrap_or("");
        // Expect at least 5 spaces before the // comment after the comma
        assert!(after_comma.starts_with("     "));
    }

    #[test]
    fn parse_inline_attributes_preserves_custom_field_key() {
        let attrs = parse_inline_attributes("// TODO tidy [product=Platform]");
        assert_eq!(
            attrs.fields,
            vec![("product".to_string(), "Platform".to_string())]
        );
    }
}

/// Parse inline bracket attributes like [key=value] and map them to AddArgs fields.
/// Recognized keys (case-insensitive): assignee, priority, tags|tag, due|due_date,
/// type, effort. Unknown keys go into fields Vec.
fn parse_inline_attributes(line: &str) -> InlineAttrs {
    let mut attrs = Vec::new();
    // Collect top-level bracket contents
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in line.chars() {
        match ch {
            '[' => {
                depth += 1;
                if depth == 1 {
                    current.clear();
                } else {
                    // nested, include the bracket content but we'll ignore for parsing simplicity
                    current.push('[');
                }
            }
            ']' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        if !current.trim().is_empty() {
                            attrs.push(current.trim().to_string());
                        }
                        current.clear();
                        continue;
                    } else {
                        current.push(']');
                    }
                }
            }
            c => {
                if depth > 0 {
                    current.push(c);
                }
            }
        }
    }

    let mut out = InlineAttrs::default();
    for a in attrs {
        // Allow comma-separated pairs within a single [ ... ]
        for part in a.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (k, v) = if let Some((k, v)) = part.split_once('=') {
                (k.trim().to_lowercase(), v.trim().to_string())
            } else {
                // Single flag form [tag=foo] is preferred; if bare token present, skip
                continue;
            };
            match k.as_str() {
                "assignee" | "assign" => out.assignee = Some(v),
                "priority" => out.priority = Some(v),
                "type" => out.task_type = Some(v),
                "effort" => out.effort = Some(v),
                "due" | "due_date" => out.due = Some(v),
                "tag" => out.tags.push(v),
                "tags" => {
                    // Split on commas or whitespace
                    for t in v.split(|c: char| c == ',' || c.is_whitespace()) {
                        let t = t.trim();
                        if !t.is_empty() {
                            out.tags.push(t.to_string());
                        }
                    }
                }
                // ticket indicates an existing key; ignore here (handled elsewhere)
                "ticket" => {}
                _ => out.fields.push((k.to_string(), v)),
            }
        }
    }
    out
}

#[derive(Default)]
struct InlineAttrs {
    assignee: Option<String>,
    priority: Option<String>,
    task_type: Option<String>,
    effort: Option<String>,
    due: Option<String>,
    tags: Vec<String>,
    fields: Vec<(String, String)>,
}

// custom_fields-based source_refs helper removed; we now use Task.references with code anchors

impl ScanHandler {
    /// Re-anchor existing task references by searching for their key occurrences
    /// near the previous anchor line, with a fallback to a full-file search.
    /// If `window_hint` is None, a default small window will be used.
    fn reanchor_existing_references(
        _resolver: &crate::workspace::TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
        window_hint: Option<usize>,
    ) -> Result<(), String> {
        let window = window_hint.unwrap_or(7);
        let mut ctx = match TaskCommandContext::new_read_only(_resolver, None, None) {
            Ok(ctx) => ctx,
            Err(_) => return Ok(()),
        };

        // Load all tasks (all projects)
        let filter = crate::storage::TaskFilter::default();
        let mut tasks = ctx.storage.search(&filter);
        if tasks.is_empty() {
            return Ok(());
        }

        let mut updates = 0usize;
        // Compute repo root once for this pass
        let repo_root = std::env::current_dir()
            .ok()
            .and_then(|cwd| crate::utils::git::find_repo_root(&cwd));
        // Best-effort git rename map if inside a repo
        let rename_map = if let Some(root) = &repo_root {
            Self::git_rename_map(root)
        } else {
            std::collections::HashMap::new()
        };

        for (task_id, mut task) in tasks.drain(..) {
            // Skip tasks without references
            if task.references.is_empty() {
                continue;
            }

            // Track if this task changed
            let mut changed = false;

            for r in task.references.iter_mut() {
                // Avoid borrowing r.code across mutations by cloning it first
                let code_ref_opt = r.code.clone();
                let Some(code_ref) = code_ref_opt else {
                    continue;
                };
                let (path_str, orig_line_opt) = Self::parse_code_ref(&code_ref);
                if path_str.is_empty() {
                    continue;
                }

                // Resolve absolute file path if possible
                let abs_path = if let Some(root) = &repo_root {
                    root.join(&path_str)
                } else {
                    std::path::PathBuf::from(&path_str)
                };

                let abs_path = if abs_path.exists() {
                    abs_path
                } else {
                    // File may have been renamed; try git rename map using repo-relative key
                    let rel_key = &path_str;
                    if let Some(new_rel) = rename_map.get(rel_key) {
                        if let Some(root) = &repo_root {
                            let candidate = root.join(new_rel);
                            if candidate.exists() {
                                candidate
                            } else {
                                // Fall back to skipping if new file also doesn't exist
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                };

                // Determine the search key token for this task
                let key = task_id.clone();

                // Load file and gather lines
                let Ok(content) = std::fs::read_to_string(&abs_path) else {
                    continue;
                };
                let lines: Vec<&str> = content.lines().collect();

                // If the original line still contains the key marker, nothing to do
                if let Some(orig_line) = orig_line_opt
                    && let Some(existing) = lines.get(orig_line.saturating_sub(1))
                    && Self::line_contains_key(existing, &key)
                {
                    // Ensure canonical formatting of code ref
                    let rel = crate::utils::paths::repo_relative_display(&abs_path);
                    let new_code = format!("{}#L{}", rel, orig_line);
                    if r.code.as_deref() != Some(&new_code) {
                        r.code = Some(new_code.clone());
                        changed = true;
                    }
                    continue;
                }

                // Proximity search window around original line (if known)
                let mut best_line: Option<usize> = None;
                if let Some(orig_line) = orig_line_opt {
                    let start = orig_line.saturating_sub(window);
                    let end = (orig_line + window).min(lines.len());
                    for ln in start..=end {
                        if ln == 0 || ln > lines.len() {
                            continue;
                        }
                        let text = lines[ln - 1];
                        if Self::line_contains_key(text, &key) {
                            best_line = Some(ln);
                            break;
                        }
                    }
                }

                // Fallback: full-file search for the exact key token markers
                if best_line.is_none() {
                    // Try to find all candidate lines and pick the closest to original, else the first
                    let mut candidates: Vec<usize> = Vec::new();
                    for (idx, text) in lines.iter().enumerate() {
                        if Self::line_contains_key(text, &key) {
                            candidates.push(idx + 1);
                        }
                    }
                    if !candidates.is_empty() {
                        if let Some(orig_line) = orig_line_opt {
                            // Choose nearest to original
                            candidates.sort_by_key(|&ln| ln.abs_diff(orig_line));
                            best_line = candidates.first().copied();
                        } else {
                            best_line = candidates.first().copied();
                        }
                    }
                }

                if let Some(new_line) = best_line {
                    let rel = crate::utils::paths::repo_relative_display(&abs_path);
                    let new_code = format!("{}#L{}", rel, new_line);
                    if r.code.as_deref() != Some(&new_code) {
                        r.code = Some(new_code.clone());
                        changed = true;
                        renderer.log_debug(format_args!("reanchor: {} -> {}", code_ref, new_code));
                    }
                }
            }

            if changed {
                task.modified = chrono::Utc::now().to_rfc3339();
                ctx.storage
                    .edit(&task_id, &task)
                    .map_err(TaskStorageAction::Update.map_err(&task_id))?;
                updates += 1;
            }
        }

        if updates > 0 {
            renderer.emit_info(format_args!("Re-anchored {} task(s).", updates));
        }
        Ok(())
    }

    fn parse_code_ref(code: &str) -> (String, Option<usize>) {
        if let Some((path, line_part)) = code.split_once("#L") {
            let line = line_part
                .split(['-', ':', '#'])
                .next()
                .and_then(|s| s.parse::<usize>().ok());
            (path.to_string(), line)
        } else {
            (code.to_string(), None)
        }
    }

    fn line_contains_key(line: &str, key: &str) -> bool {
        if line.contains(&format!("({})", key)) {
            return true;
        }
        // Accept [ticket=KEY] with optional spaces and case-insensitive ticket
        let pattern = format!(r"(?i)\[\s*ticket\s*=\s*{}\s*\]", regex::escape(key));
        if regex::Regex::new(&pattern)
            .map(|re| re.is_match(line))
            .unwrap_or(false)
        {
            return true;
        }
        false
    }

    fn git_rename_map(repo_root: &std::path::Path) -> std::collections::HashMap<String, String> {
        use std::process::Command;
        let mut map = std::collections::HashMap::new();
        let out = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("status")
            .arg("--porcelain")
            .output();
        let Ok(o) = out else {
            return map;
        };
        if !o.status.success() {
            return map;
        }
        let s = String::from_utf8_lossy(&o.stdout);
        for line in s.lines() {
            if line.len() < 4 {
                continue;
            }
            let status = &line[..2];
            if status.contains('R')
                && let Some(pos) = line.find(" -> ")
            {
                let old = line[3..pos].trim().to_string();
                let newp = line[pos + 4..].trim().to_string();
                map.insert(old, newp);
            }
        }
        map
    }
}
