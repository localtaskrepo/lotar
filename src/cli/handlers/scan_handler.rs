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
    ScanConfigSnapshot, configured_line_scanner, merge_signal_words, parse_inline_attributes,
    refresh_code_reference, replace_line_contents, scan_config_from_manager, validate_source_write,
    write_source,
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
    let LoadedTask { full_id, .. } = load_task(&mut ctx, task_id, project_hint)?;
    ctx.storage
        .mutate_task(&full_id, |fresh| {
            let changed = mutator(fresh);
            if changed {
                fresh.modified = chrono::Utc::now().to_rfc3339();
            }
            Ok(changed)
        })
        .map_err(TaskStorageAction::Update.map_err(&full_id))?;
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

        // DEV-119: anchor maintenance only touches anchors under explicitly
        // scanned paths; a scan without paths covers the whole workspace.
        let anchor_scope: Vec<PathBuf> = if args.paths.is_empty() {
            Vec::new()
        } else {
            roots.clone()
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

        // Load config (global or project-specific) to obtain scan settings.
        // When no configuration can be read at all, fall back to scanner
        // defaults with mention tracking left enabled (CLI behavior).
        let scan_cfg =
            match crate::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
                _resolver.path.as_path(),
            )
            .ok()
            {
                Some(mgr) => {
                    scan_config_from_manager(&mgr, effective_project_for_config.as_deref())
                }
                None => ScanConfigSnapshot::cli_fallback(),
            };
        let final_words = merge_signal_words(&scan_cfg.signal_words, &scan_cfg.issue_type_words);
        let line_scanner = configured_line_scanner(
            scan_cfg.ticket_patterns.as_deref(),
            scan_cfg.enable_ticket_words,
            &final_words,
        );

        let mut all_results = Vec::new();
        let mut all_mentions = Vec::new();
        for root in roots {
            renderer.log_debug(format_args!("scan: scanning path={}", root.display()));
            let mut scanner = scanner::Scanner::new(root)
                .with_include_ext(&args.include)
                .with_exclude_ext(&args.exclude)
                .with_modified_only(args.modified_only)
                .with_mention_tracking(scan_cfg.enable_mentions);
            if !final_words.is_empty() {
                scanner = scanner.with_signal_words(&final_words);
            }
            scanner = scanner.with_ticket_detection(
                scan_cfg.ticket_patterns.as_deref(),
                scan_cfg.enable_ticket_words,
            );
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
            Self::maintain_anchors(_resolver, renderer, &args, &anchor_scope)?;
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
                        let range = line_scanner
                            .matched_source_comment_range(
                                &entry.file_path,
                                &contents,
                                entry.line_number,
                            )
                            .ok_or("Cannot establish a safe scan comment boundary")?;
                        let existing_key =
                            line_scanner.extract_ticket_key_from_line(&orig_line[range.clone()]);
                        if existing_key.is_none() {
                            if line_scanner
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
                            let new_line = line_scanner
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
                                    replace_line_contents(&contents, entry.line_number, &new_line)
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
                        } else if !args.dry_run && scan_cfg.enable_mentions {
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
                                let live_refs: Vec<String> = line_scanner
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
            // Do not mutate existing tasks before new source edits have succeeded.
            Self::maintain_anchors(_resolver, renderer, &args, &anchor_scope)?;
            if !args.dry_run && applied > 0 && !json {
                renderer.emit_success(format_args!("Applied {} update(s).", applied));
            }
        }

        // Safe known-ticket mention path: bare known-ID comment mentions never
        // create tasks and never edit source; they only anchor existing tasks.
        // Runs after the create loop so tasks created in this run are anchored too.
        if !args.dry_run && scan_cfg.enable_mentions && !all_mentions.is_empty() {
            Self::refresh_bare_mentions(
                _resolver,
                renderer,
                scan_cfg.ticket_patterns.as_deref(),
                scan_cfg.enable_ticket_words,
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
        let line_scanner = configured_line_scanner(ticket_patterns, enable_ticket_words, &[]);
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

    /// Anchor maintenance after a scan (DEV-119): relocation runs on every
    /// non-dry-run scan; `--reanchor` additionally removes dead anchors and
    /// also previews its changes during `--dry-run`.
    fn maintain_anchors(
        resolver: &crate::workspace::TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
        args: &ScanArgs,
        scope: &[PathBuf],
    ) -> Result<(), String> {
        if args.dry_run && !args.reanchor {
            return Ok(());
        }
        Self::reanchor_with_before_apply(
            resolver,
            renderer,
            scope,
            args.reanchor,
            args.dry_run,
            |_| Ok(()),
        )
    }

    /// Same anchor pass with a seam fired after planning and immediately
    /// before each task write: the callback can simulate a concurrent editor
    /// landing between the search snapshot and the updated references.
    fn reanchor_with_before_apply(
        resolver: &crate::workspace::TasksDirectoryResolver,
        renderer: &crate::output::OutputRenderer,
        scope: &[PathBuf],
        prune_dead: bool,
        dry_run: bool,
        before_apply: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        use crate::services::scan_anchor_service::{
            AnchorPassOptions, DEFAULT_RELOCATION_WINDOW, run_anchor_pass,
        };
        let mut ctx = match TaskCommandContext::new_read_only(resolver, None, None) {
            Ok(ctx) => ctx,
            Err(_) => return Ok(()),
        };
        let line_scanner = configured_line_scanner(
            ctx.config.scan_ticket_patterns.as_deref(),
            ctx.config.scan_enable_ticket_words,
            &[],
        );
        let repo_root = std::env::current_dir()
            .ok()
            .and_then(|cwd| crate::utils::git::find_repo_root(&cwd));
        let report = run_anchor_pass(
            &mut ctx.storage,
            &line_scanner,
            repo_root.as_deref(),
            &AnchorPassOptions {
                scope,
                prune_dead,
                dry_run,
                window: DEFAULT_RELOCATION_WINDOW,
            },
            before_apply,
        )
        .map_err(|err| err.to_string())?;
        for (task, old, new) in &report.relocated {
            renderer.log_debug(format_args!("reanchor: {task} {old} -> {new}"));
        }
        for message in report.messages(dry_run) {
            renderer.emit_info(format_args!("{}", message));
        }
        Ok(())
    }
}

#[cfg(test)]
mod reanchor_tests {
    use super::ScanHandler;
    use crate::output::{LogLevel, OutputFormat, OutputRenderer};
    use crate::types::ReferenceEntry;
    use crate::workspace::TasksDirectoryResolver;
    use std::path::PathBuf;

    struct ReanchorFixture {
        temp: tempfile::TempDir,
        resolver: TasksDirectoryResolver,
        task_id: String,
    }

    impl ReanchorFixture {
        /// Task SAFE-1 anchored at `main.rs#1` while the key now lives on a
        /// later line, so the re-anchor pass must relocate the anchor.
        fn new(source: &str) -> Self {
            let temp = tempfile::tempdir().unwrap();
            let tasks_dir = temp.path().join(".tasks");
            std::fs::create_dir(&tasks_dir).unwrap();
            std::fs::write(temp.path().join("main.rs"), source).unwrap();
            let resolver =
                TasksDirectoryResolver::resolve(Some(tasks_dir.to_str().unwrap()), None).unwrap();
            let mut storage = crate::storage::manager::Storage::new(&tasks_dir);
            let task = crate::storage::task::Task::new(
                tasks_dir.clone(),
                "reanchor target".into(),
                crate::types::Priority::new("medium"),
            );
            let task_id = storage.add(&task, "SAFE", None).unwrap();
            let mut task = storage.get(&task_id, "SAFE").unwrap();
            task.references = vec![ReferenceEntry {
                code: Some("main.rs#1".into()),
                ..Default::default()
            }];
            storage.edit(&task_id, &task).unwrap();
            Self {
                temp,
                resolver,
                task_id,
            }
        }

        fn tasks_dir(&self) -> PathBuf {
            self.temp.path().join(".tasks")
        }

        fn run<F>(&self, before_apply: F) -> Result<(), String>
        where
            F: FnMut(&str) -> Result<(), String>,
        {
            let original_cwd = std::env::current_dir().unwrap();
            std::env::set_current_dir(self.temp.path()).unwrap();
            let renderer = OutputRenderer::new(OutputFormat::Text, LogLevel::Error);
            let result = ScanHandler::reanchor_with_before_apply(
                &self.resolver,
                &renderer,
                &[],
                false,
                false,
                before_apply,
            );
            std::env::set_current_dir(&original_cwd).unwrap();
            result
        }
    }

    #[test]
    fn reanchor_preserves_concurrent_task_edits() {
        let fixture = ReanchorFixture::new("// filler\n// filler two\n// SAFE-1: moved\n");
        let tasks_dir = fixture.tasks_dir();
        fixture
            .run(|task_id| {
                // Deterministic concurrent editor: lands between the search
                // snapshot and the task write, adding a comment, a code
                // reference, and a custom field that must all survive.
                let mut storage = crate::storage::manager::Storage::new(&tasks_dir);
                let mut fresh = storage.get(task_id, "SAFE").ok_or("task missing")?;
                fresh.comments.push(crate::types::TaskComment {
                    date: "2026-10-13T00:00:00Z".into(),
                    text: "concurrent note".into(),
                });
                fresh.references.push(ReferenceEntry {
                    code: Some("other.rs#7".into()),
                    ..Default::default()
                });
                fresh
                    .custom_fields
                    .insert("severity".into(), crate::types::custom_value_string("high"));
                storage.edit(task_id, &fresh).map_err(|e| e.to_string())?;
                Ok(())
            })
            .unwrap();

        let verify = crate::storage::manager::Storage::new(&fixture.tasks_dir());
        let updated = verify.get(&fixture.task_id, "SAFE").unwrap();
        assert!(
            updated
                .references
                .iter()
                .any(|r| r.code.as_deref() == Some("main.rs#3")),
            "re-anchor did not relocate: {:?}",
            updated.references
        );
        assert!(
            updated
                .references
                .iter()
                .any(|r| r.code.as_deref() == Some("other.rs#7")),
            "concurrent reference lost: {:?}",
            updated.references
        );
        assert!(
            updated.comments.iter().any(|c| c.text == "concurrent note"),
            "concurrent comment lost: {:?}",
            updated.comments
        );
        assert!(
            updated.custom_fields.contains_key("severity"),
            "concurrent custom field lost: {:?}",
            updated.custom_fields
        );
    }

    #[test]
    fn reanchor_without_changes_never_rewrites_task_files() {
        let fixture = ReanchorFixture::new("// SAFE-1: still here\n");
        let task_path = fixture.tasks_dir().join("SAFE/1.yml");
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800);
        std::fs::File::options()
            .write(true)
            .open(&task_path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let before = std::fs::read(&task_path).unwrap();
        fixture
            .run(|_| Err("no-op re-anchor must not apply any task write".to_string()))
            .unwrap();
        assert_eq!(std::fs::read(&task_path).unwrap(), before);
        assert_eq!(
            std::fs::metadata(&task_path).unwrap().modified().unwrap(),
            old
        );
    }
}
