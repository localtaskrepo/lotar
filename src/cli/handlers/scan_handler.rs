use crate::cli::ScanArgs;
use crate::cli::handlers::AddHandler;
use crate::cli::handlers::CommandHandler;
use crate::cli::handlers::task::context::TaskCommandContext;
use crate::cli::handlers::task::errors::TaskStorageAction;
use crate::cli::handlers::task::mutation::{LoadedTask, load_task};
use crate::output::OutputRenderer;
use crate::project;
use crate::scanner;
use crate::utils::scan::{
    parse_inline_attributes, refresh_code_reference, validate_source_write, write_source,
};
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
        let mut all_mentions = Vec::new();
        for root in roots {
            renderer.log_debug(format_args!("scan: scanning path={}", root.display()));
            let mut scanner = scanner::Scanner::new(root)
                .with_include_ext(&args.include)
                .with_exclude_ext(&args.exclude)
                .with_modified_only(args.modified_only)
                .with_mention_tracking(cfg_enable_mentions);
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
            let (mut results, mut mentions) = scanner.scan_with_mentions();
            all_results.append(&mut results);
            all_mentions.append(&mut mentions);
        }

        // Ensure deterministic ordering across roots
        all_results.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.line_number.cmp(&b.line_number))
        });
        all_mentions.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.line_number.cmp(&b.line_number))
        });

        let json = matches!(renderer.format, crate::output::OutputFormat::Json);
        let json_items: Vec<serde_json::Value> = all_results
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
        if all_results.is_empty() {
            if !json {
                renderer.emit_success("No TODO comments found.");
            }
            // Even if we didn't find TODO comments, we can still try to relocate anchors for existing tasks
            if !args.dry_run {
                Self::reanchor_existing_references(_resolver, renderer, None)?;
            }
        } else {
            renderer.emit_info(format_args!("Found {} TODO comment(s):", all_results.len()));
            // When applying, we'll need a storage context for creating tasks
            let resolved_project = project_resolver.resolve_project("", _project)?;
            let effective_project = (!resolved_project.is_empty()).then_some(resolved_project);

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

            let mut applied = 0usize;
            for entry in all_results {
                // If detailed flag is set, emit a per-file header line
                if args.detailed && !json {
                    renderer.emit_raw_stdout(format_args!("  📄 {}", entry.file_path.display()));
                }
                // Default behavior: apply changes (unless --dry-run)
                // Read file and target line
                {
                    let contents = std::fs::read_to_string(&entry.file_path).map_err(|e| {
                        format!("Failed to read {}: {e}", entry.file_path.display())
                    })?;
                    let all_lines: Vec<&str> = contents.lines().collect();
                    let orig_line =
                        all_lines
                            .get(entry.line_number - 1)
                            .copied()
                            .ok_or_else(|| {
                                format!(
                                    "Source line disappeared: {}:{}",
                                    entry.file_path.display(),
                                    entry.line_number
                                )
                            })?;
                    {
                        let mut tmp_scanner = scanner::Scanner::new(PathBuf::from("."))
                            .with_ticket_detection(
                                cfg_ticket_patterns.as_deref(),
                                cfg_enable_ticket_words,
                            );
                        let mut words = cfg_words.clone();
                        words.extend(issue_type_words.iter().cloned());
                        if !words.is_empty() {
                            tmp_scanner = tmp_scanner.with_signal_words(&words);
                        }
                        let range = tmp_scanner
                            .matched_source_comment_range(
                                &entry.file_path,
                                &contents,
                                entry.line_number,
                            )
                            .ok_or("Cannot establish a safe scan comment boundary")?;
                        let existing_key =
                            tmp_scanner.extract_ticket_key_from_line(&orig_line[range.clone()]);
                        if existing_key.is_none() {
                            if tmp_scanner
                                .suggest_source_insertion(
                                    &entry.file_path,
                                    &contents,
                                    entry.line_number,
                                    "SCAN-NEW",
                                    strip_attributes,
                                )
                                .is_none()
                            {
                                return Err(format!(
                                    "Cannot safely insert a task key at {}:{}",
                                    entry.file_path.display(),
                                    entry.line_number
                                ));
                            }
                            if !args.dry_run {
                                validate_source_write(&entry.file_path)?;
                            }
                            // Parse inline attributes from the original line before any stripping
                            let inline_attrs = parse_inline_attributes(&orig_line[range]);
                            // Create task title from entry.title
                            // Reuse AddHandler with smart defaults
                            let cli_add_args = crate::cli::AddArgs {
                                title: entry.title.clone(),
                                task_type: inline_attrs.task_type,
                                priority: inline_attrs.priority,
                                status: None,
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
                                if let Err(err) =
                                    edit_task_with_context(_resolver, &task_id, None, |task| {
                                        let code_ref = format!("{}#{}", rel, line_number);
                                        refresh_code_reference(&mut task.references, &code_ref, &[])
                                    })
                                {
                                    return Err(rollback_created_task(_resolver, &task_id, err));
                                }
                            }

                            // Insert (KEY) after signal word; optionally strip bracket attributes
                            let new_line = tmp_scanner
                                .suggest_source_insertion(
                                    &entry.file_path,
                                    &contents,
                                    entry.line_number,
                                    &task_id,
                                    strip_attributes,
                                )
                                .ok_or("Cannot safely insert task key")?;

                            // Optional: emit context snippet when requested
                            if args.detailed && args.context > 0 && !json {
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
                                if args.dry_run && !json {
                                    renderer.emit_raw_stdout(format_args!(
                                        "  📄 {}:{}\n    - {}\n    + {}",
                                        entry.file_path.display(),
                                        entry.line_number,
                                        orig_line,
                                        new_line
                                    ));
                                } else if !args.dry_run {
                                    if let Err(e) =
                                        write_source(&entry.file_path, &contents, &updated)
                                    {
                                        return Err(rollback_created_task(
                                            _resolver,
                                            &task_id,
                                            format!(
                                                "Failed to write {}: {e}",
                                                entry.file_path.display()
                                            ),
                                        ));
                                    }
                                    if !json {
                                        renderer.emit_raw_stdout(format_args!(
                                            "  📄 {}:{}\n    - {}\n    + {}",
                                            entry.file_path.display(),
                                            entry.line_number,
                                            orig_line,
                                            new_line
                                        ));
                                    }
                                    applied += 1;
                                }
                            }
                        } else if !args.dry_run && cfg_enable_mentions {
                            // Movement/relocation resilience: if an existing key is present, ensure
                            // the corresponding task has a code reference for this file+line.
                            if let Some(task_id) = existing_key {
                                let prefix = crate::storage::TaskId::parse(&task_id)
                                    .map(|parsed| parsed.project)
                                    .unwrap_or_default();
                                if crate::storage::manager::Storage::try_open(&_resolver.path)
                                    .and_then(|storage| storage.get(&task_id, &prefix))
                                    .is_none()
                                {
                                    // External ticket mentions do not imply a local task exists.
                                    continue;
                                }
                                let rel =
                                    crate::utils::paths::repo_relative_display(&entry.file_path);
                                let line_number = entry.line_number;
                                let live_refs: Vec<String> = tmp_scanner
                                    .key_mention_lines(&entry.file_path, &contents, &task_id)
                                    .iter()
                                    .map(|line| format!("{}#{}", rel, line))
                                    .collect();
                                edit_task_with_context(_resolver, &task_id, None, |task| {
                                    let code_ref = format!("{}#{}", rel, line_number);
                                    refresh_code_reference(
                                        &mut task.references,
                                        &code_ref,
                                        &live_refs,
                                    )
                                })?;
                            }
                        }
                    }
                }
                // Apply path (and dry-run preview) prints patch above when changes occur; lines with existing keys are left untouched
            }
            if !args.dry_run {
                // Do not mutate existing tasks before new source edits have succeeded.
                Self::reanchor_existing_references(_resolver, renderer, Some(7))?;
            }
            if !args.dry_run && applied > 0 && !json {
                renderer.emit_success(format_args!("Applied {} update(s).", applied));
            }
        }

        // Safe known-ticket mention path: bare known-ID comment mentions never
        // create tasks and never edit source; they only anchor existing tasks.
        // Runs after the create loop so tasks created in this run are anchored too.
        if !args.dry_run && cfg_enable_mentions && !all_mentions.is_empty() {
            Self::refresh_bare_mentions(
                _resolver,
                renderer,
                cfg_ticket_patterns.as_deref(),
                cfg_enable_ticket_words,
                &all_mentions,
            )?;
        }

        if json {
            renderer.emit_json(&json_items);
        }
        Ok(())
    }
}

fn rollback_created_task(
    resolver: &TasksDirectoryResolver,
    task_id: &str,
    error: String,
) -> String {
    let result = task_id
        .rsplit_once('-')
        .ok_or_else(|| "Invalid created task ID".to_string())
        .and_then(|(project, _)| {
            crate::storage::manager::Storage::new(&resolver.path.clone())
                .delete(task_id, project)
                .map_err(|e| e.to_string())
                .and_then(|deleted| {
                    if deleted {
                        Ok(())
                    } else {
                        Err("Created task was not removed".to_string())
                    }
                })
        });
    match result {
        Ok(()) => error,
        Err(rollback) => format!("{error}; rollback of {task_id} failed: {rollback}"),
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

// custom_fields-based source_refs helper removed; we now use Task.references with code anchors

impl ScanHandler {
    /// Anchor bare known-ticket mentions: comment lines carrying an existing
    /// ticket key without a signal word. Never creates tasks (unknown or
    /// external keys are skipped) and never edits source text.
    fn refresh_bare_mentions(
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
        ticket_patterns: Option<&[String]>,
        enable_ticket_words: bool,
        mentions: &[scanner::Reference],
    ) -> Result<(), String> {
        let line_scanner = scanner::Scanner::new(PathBuf::from("."))
            .with_ticket_detection(ticket_patterns, enable_ticket_words);
        let mut anchored = 0usize;
        for entry in mentions {
            let task_id = entry.uuid.as_str();
            if task_id.is_empty() {
                continue;
            }
            // Unknown or external ticket mentions do not imply a local task exists.
            let prefix = match crate::storage::TaskId::parse(task_id) {
                Ok(parsed) => parsed.project,
                Err(_) => continue,
            };
            if crate::storage::manager::Storage::try_open(&resolver.path)
                .and_then(|storage| storage.get(task_id, &prefix))
                .is_none()
            {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&entry.file_path) else {
                continue;
            };
            let live_lines = line_scanner.key_mention_lines(&entry.file_path, &contents, task_id);
            if !live_lines.contains(&entry.line_number) {
                // Source changed since discovery; only live mentions are anchored.
                continue;
            }
            let rel = crate::utils::paths::repo_relative_display(&entry.file_path);
            let code_ref = format!("{}#{}", rel, entry.line_number);
            let live_refs: Vec<String> = live_lines
                .iter()
                .map(|line| format!("{}#{}", rel, line))
                .collect();
            edit_task_with_context(resolver, task_id, None, |task| {
                refresh_code_reference(&mut task.references, &code_ref, &live_refs)
            })?;
            anchored += 1;
        }
        if anchored > 0 {
            renderer.emit_info(format_args!("Anchored {} bare mention(s).", anchored));
        }
        Ok(())
    }

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
        let line_scanner = scanner::Scanner::new(PathBuf::from(".")).with_ticket_detection(
            ctx.config.scan_ticket_patterns.as_deref(),
            ctx.config.scan_enable_ticket_words,
        );
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
                let candidates = line_scanner.key_mention_lines(&abs_path, &content, &key);

                // If the original line still contains the key marker, nothing to do
                if let Some(orig_line) = orig_line_opt
                    && candidates.contains(&orig_line)
                {
                    // Ensure canonical formatting of code ref
                    let rel = crate::utils::paths::repo_relative_display(&abs_path);
                    let new_code = format!("{}#{}", rel, orig_line);
                    if r.code.as_deref() != Some(&new_code) {
                        r.code = Some(new_code.clone());
                        changed = true;
                    }
                    continue;
                }

                let best_line = if let Some(orig_line) = orig_line_opt {
                    candidates
                        .iter()
                        .copied()
                        .filter(|line| line.abs_diff(orig_line) <= window)
                        .min_by_key(|line| line.abs_diff(orig_line))
                        .or_else(|| {
                            candidates
                                .iter()
                                .copied()
                                .min_by_key(|line| line.abs_diff(orig_line))
                        })
                } else {
                    candidates.first().copied()
                };

                if let Some(new_line) = best_line {
                    let rel = crate::utils::paths::repo_relative_display(&abs_path);
                    let new_code = format!("{}#{}", rel, new_line);
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
        if let Some((path, anchor)) = code.split_once('#') {
            let mut buffer = String::new();
            let mut found = false;
            for ch in anchor.chars() {
                if ch.is_ascii_digit() {
                    buffer.push(ch);
                    found = true;
                    continue;
                }
                if found {
                    break;
                }
            }
            let line = buffer.parse::<usize>().ok();
            (path.to_string(), line)
        } else {
            (code.to_string(), None)
        }
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
