/// Scan-related helpers shared by CLI and API surfaces.

#[derive(Default, Debug, Clone)]
pub struct InlineAttrs {
    pub assignee: Option<String>,
    pub priority: Option<String>,
    pub task_type: Option<String>,
    pub effort: Option<String>,
    pub due: Option<String>,
    pub tags: Vec<String>,
    pub fields: Vec<(String, String)>,
}

/// Pure snapshot of the scanner-relevant configuration shared by the CLI and
/// API scan surfaces. Each surface keeps its own fallback when no
/// configuration can be read at all.
#[derive(Debug, Clone)]
pub(crate) struct ScanConfigSnapshot {
    pub(crate) signal_words: Vec<String>,
    pub(crate) ticket_patterns: Option<Vec<String>>,
    pub(crate) enable_ticket_words: bool,
    pub(crate) enable_mentions: bool,
    pub(crate) strip_attributes: bool,
    pub(crate) issue_type_words: Vec<String>,
}

impl ScanConfigSnapshot {
    /// CLI fallback when no configuration can be read: scanner defaults with
    /// mention tracking left enabled. `strip_attributes` is unused on this
    /// path; the CLI resolves stripping through its own project lookup.
    pub(crate) fn cli_fallback() -> Self {
        Self {
            signal_words: Vec::new(),
            ticket_patterns: None,
            enable_ticket_words: false,
            enable_mentions: true,
            strip_attributes: false,
            issue_type_words: Vec::new(),
        }
    }

    /// API service fallback when no configuration can be read: identical to
    /// the CLI fallback except attribute stripping defaults to on.
    pub(crate) fn service_fallback() -> Self {
        Self {
            strip_attributes: true,
            ..Self::cli_fallback()
        }
    }
}

/// Resolve the shared scanner snapshot from a configuration manager, using
/// the project override when available and the resolved root config
/// otherwise. Pure lookup: selection order matches both historical surfaces.
pub(crate) fn scan_config_from_manager(
    manager: &crate::config::manager::ConfigManager,
    project: Option<&str>,
) -> ScanConfigSnapshot {
    let resolved = match project {
        Some(project_name) => manager
            .get_project_config(project_name)
            .unwrap_or_else(|_| manager.get_resolved_config().clone()),
        None => manager.get_resolved_config().clone(),
    };
    let issue_type_words = if resolved.scan_enable_ticket_words {
        resolved
            .issue_types
            .values
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    ScanConfigSnapshot {
        signal_words: resolved.scan_signal_words,
        ticket_patterns: resolved.scan_ticket_patterns,
        enable_ticket_words: resolved.scan_enable_ticket_words,
        enable_mentions: resolved.scan_enable_mentions,
        strip_attributes: resolved.scan_strip_attributes,
        issue_type_words,
    }
}

/// Merge regular signal words with issue-type words, skipping
/// case-insensitive duplicates. Shared by the CLI and API scan surfaces.
pub(crate) fn merge_signal_words(
    signal_words: &[String],
    issue_type_words: &[String],
) -> Vec<String> {
    let mut merged = signal_words.to_vec();
    for word in issue_type_words {
        if !merged
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(word))
        {
            merged.push(word.clone());
        }
    }
    merged
}

/// Build the quote-aware line scanner both surfaces use for comment ranges,
/// key extraction, and mention analysis. Signal words are applied only when
/// provided, matching the historical per-surface construction.
pub(crate) fn configured_line_scanner(
    ticket_patterns: Option<&[String]>,
    enable_ticket_words: bool,
    signal_words: &[String],
) -> crate::scanner::Scanner {
    let mut scanner = crate::scanner::Scanner::new(std::path::PathBuf::from("."))
        .with_ticket_detection(ticket_patterns, enable_ticket_words);
    if !signal_words.is_empty() {
        scanner = scanner.with_signal_words(signal_words);
    }
    scanner
}

/// Replace a specific 1-based line in the file content and return the new
/// content, preserving the trailing-newline shape of the input.
pub(crate) fn replace_line_contents(
    contents: &str,
    line_number_1based: usize,
    new_line: &str,
) -> Option<String> {
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

/// Strip parsed metadata from a comment segment, never from the surrounding source.
pub fn strip_bracket_attributes(line: &str) -> String {
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
                        // Keep malformed blocks and nested source expressions verbatim.
                        if is_metadata_block(&buf) {
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

fn is_metadata_block(value: &str) -> bool {
    let mut tags = false;
    value.split(',').all(|part| {
        if let Some((key, value)) = part.split_once('=') {
            tags = key.trim().eq_ignore_ascii_case("tags");
            !key.trim().is_empty()
                && key
                    .trim()
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                && !value.trim().is_empty()
                && !value.contains(['[', ']'])
        } else {
            tags && !part.trim().is_empty() && !part.contains(['[', ']'])
        }
    })
}

/// Refresh only stale anchors in this source file. Preserve live repeated hits and
/// all metadata on mixed reference entries, including when anchors converge.
pub fn refresh_code_reference(
    references: &mut Vec<crate::types::ReferenceEntry>,
    code_ref: &str,
    live_refs: &[String],
    history: &[crate::types::TaskChangeLogEntry],
) -> bool {
    let file = code_ref
        .rsplit_once('#')
        .map(|(file, _)| file)
        .unwrap_or(code_ref);
    let mut occupied: std::collections::HashSet<String> = references
        .iter()
        .filter_map(|reference| reference.code.clone())
        .collect();
    let mut replacements = std::collections::HashMap::new();
    let mut changed = false;
    for reference in references.iter_mut() {
        if let Some(code) = reference.code.as_ref()
            && let Some((path, anchor)) = code.rsplit_once('#')
            && path == file
            && let Ok(old_line) = anchor.parse::<usize>()
            && code != code_ref
            && !live_refs.contains(code)
            // References a person attached on purpose stay where they point.
            && !crate::services::scan_anchor_service::is_user_attached(history, code)
        {
            // Do not collapse a moved mention into an already-live anchor.
            // Mixed entries sharing one old anchor must relocate together.
            let replacement = replacements
                .entry(code.clone())
                .or_insert_with(|| {
                    live_refs
                        .iter()
                        .filter_map(|candidate| {
                            let (path, anchor) = candidate.rsplit_once('#')?;
                            let line = anchor.parse::<usize>().ok()?;
                            (path == file && !occupied.contains(candidate))
                                .then_some((candidate, line))
                        })
                        .min_by_key(|(_, line)| line.abs_diff(old_line))
                        .map(|(candidate, _)| candidate.clone())
                        .unwrap_or_else(|| code_ref.to_string())
                })
                .clone();
            occupied.insert(replacement.clone());
            reference.code = Some(replacement);
            changed = true;
        }
    }
    if !references
        .iter()
        .any(|r| r.code.as_deref() == Some(code_ref))
    {
        references.push(crate::types::ReferenceEntry {
            code: Some(code_ref.to_string()),
            ..Default::default()
        });
        changed = true;
    }
    changed
}

/// Validate without changing the source. Reject symlinks rather than replacing
/// their directory entry, and honor read-only source permissions even with rename.
pub fn validate_source_write(path: &std::path::Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() || metadata.permissions().readonly() {
        return Err(format!(
            "Source is not a writable regular file: {}",
            path.display()
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    if std::fs::metadata(parent)
        .map_err(|e| e.to_string())?
        .permissions()
        .readonly()
    {
        return Err(format!(
            "Source directory is read-only: {}",
            parent.display()
        ));
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| format!("Cannot prepare source write: {e}"))?;
    Ok(())
}

pub fn write_source(path: &std::path::Path, expected: &str, updated: &str) -> Result<(), String> {
    write_source_with_prepared(path, expected, updated, || Ok(()))
}

#[cfg(unix)]
fn lock_source(path: &std::path::Path) -> Result<std::fs::File, String> {
    use std::os::unix::fs::MetadataExt;
    for _ in 0..8 {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| {
                format!(
                    "Cannot open source inode for locking {}: {e}",
                    path.display()
                )
            })?;
        fs2::FileExt::lock_exclusive(&file).map_err(|e| {
            format!(
                "Cannot exclusively lock source inode {}: {e}",
                path.display()
            )
        })?;
        let held = file.metadata().map_err(|e| e.to_string())?;
        let current = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if current.file_type().is_file()
            && held.dev() == current.dev()
            && held.ino() == current.ino()
        {
            return Ok(file);
        }
        // Another writer renamed while we waited on the old inode. Reopen and
        // lock the current inode; never compare/write under a stale inode lock.
    }
    Err("Source repeatedly replaced while acquiring its lock".to_string())
}

#[cfg(not(unix))]
fn lock_source(path: &std::path::Path) -> Result<std::fs::File, String> {
    // Stable per-path locks avoid relying on unavailable portable inode IDs.
    // Never unlink locks because another process may already be waiting on one.
    let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let lock_dir = dirs::cache_dir()
        .ok_or("No user cache directory for source locks")?
        .join("lotar/source-locks");
    std::fs::create_dir_all(&lock_dir).map_err(|e| e.to_string())?;
    let lock_path = lock_dir.join(format!(
        "{}.lock",
        blake3::hash(canonical.as_os_str().as_encoded_bytes()).to_hex()
    ));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    fs2::FileExt::lock_exclusive(&lock).map_err(|e| e.to_string())?;
    Ok(lock)
}

pub(crate) fn write_source_with_prepared(
    path: &std::path::Path,
    expected: &str,
    updated: &str,
    prepared: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    validate_source_write(path)?;
    let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let permissions = std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .permissions();
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let staged = parent.join(format!(
        ".lotar-scan-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)
        .map_err(|e| {
            format!(
                "Cannot reserve source replacement {}: {e}",
                staged.display()
            )
        })?;
    let result = (|| -> Result<(), String> {
        crate::storage::safety::atomic_write_file(&staged, updated).map_err(|e| {
            format!(
                "Cannot prepare source replacement {}: {e}",
                staged.display()
            )
        })?;
        std::fs::set_permissions(&staged, permissions).map_err(|e| {
            format!(
                "Cannot preserve source replacement permissions {}: {e}",
                staged.display()
            )
        })?;
        prepared()?;
        let _source_lock = lock_source(path)?;
        // Keep the replacement inode locked across rename too, so writers
        // opening the new source cannot enter until this commit finishes.
        #[cfg(unix)]
        let _replacement_lock = lock_source(&staged)?;
        validate_source_write(path)?;
        if std::fs::canonicalize(path).map_err(|e| e.to_string())? != canonical
            || std::fs::read_to_string(path).map_err(|e| e.to_string())? != expected
        {
            return Err(format!("Source changed while scanning: {}", path.display()));
        }
        // Cooperating scans are serialized. Non-locking editors still have a
        // small race between this comparison and rename; this is not an OS CAS.
        std::fs::rename(&staged, path)
            .map_err(|e| format!("Cannot publish source replacement {}: {e}", path.display()))
    })();
    let _ = std::fs::remove_file(&staged);
    result
}

/// Parse inline bracket attributes like [key=value] and map them to AddArgs fields.
/// Recognized keys (case-insensitive): assignee, priority, tags|tag, due|due_date,
/// type, effort. Unknown keys go into fields Vec.
pub fn parse_inline_attributes(line: &str) -> InlineAttrs {
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
                        if is_metadata_block(&current) {
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

#[cfg(test)]
mod reference_mutation_tests {
    use crate::api_types::ScanRequest;
    use crate::cli::ScanArgs;
    use crate::cli::handlers::{CommandHandler, ScanHandler};
    use crate::output::{LogLevel, OutputFormat, OutputRenderer};
    use crate::services::scan_service::ScanService;
    use crate::storage::manager::Storage;
    use crate::storage::operations::before_task_write;
    use crate::storage::task::Task;
    use crate::types::{Priority, ReferenceEntry, TaskComment, custom_value_string};
    use crate::workspace::TasksDirectoryResolver;
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::rc::Rc;

    #[derive(Clone, Copy)]
    enum Surface {
        Cli,
        Service,
    }

    fn run(surface: Surface, resolver: &TasksDirectoryResolver, source: &std::path::Path) {
        let paths = vec![source.to_string_lossy().to_string()];
        match surface {
            Surface::Cli => {
                ScanHandler::execute(
                    ScanArgs {
                        paths,
                        include: Vec::new(),
                        exclude: Vec::new(),
                        detailed: false,
                        context: 0,
                        dry_run: false,
                        strip_attributes: None,
                        reanchor: false,
                        modified_only: false,
                    },
                    Some("SAFE"),
                    resolver,
                    &OutputRenderer::new(OutputFormat::Json, LogLevel::Error),
                )
                .unwrap();
            }
            Surface::Service => {
                let response = ScanService::run(
                    resolver,
                    ScanRequest {
                        paths,
                        project: Some("SAFE".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(response.summary.failed, 0);
                assert_eq!(response.entries.len(), 1);
                assert_eq!(response.entries[0].task_id.as_deref(), Some("SAFE-1"));
                assert_eq!(response.summary.created + response.summary.updated, 1);
            }
        }
    }

    fn preserves_concurrent_edits(surface: Surface, source: &str, create: bool) {
        for already_anchored in [false, true] {
            // A private parent prevents lookup from discovering sibling test workspaces.
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("workspace");
            let tasks = root.join(".tasks");
            fs::create_dir_all(&tasks).unwrap();
            fs::write(
                tasks.join("config.yml"),
                "default:\n  project: SAFE\n  reporter: tester\nauto:\n  identity: false\n",
            )
            .unwrap();
            let source_path = root.join("main.rs");
            fs::write(&source_path, source).unwrap();
            let resolver =
                TasksDirectoryResolver::resolve(Some(tasks.to_str().unwrap()), None).unwrap();
            if !create {
                let mut storage = Storage::new(&tasks);
                let task = Task::new(
                    tasks.clone(),
                    "existing task".into(),
                    Priority::new("medium"),
                );
                assert_eq!(storage.add(&task, "SAFE", None).unwrap(), "SAFE-1");
            }

            let edited = Rc::new(Cell::new(false));
            let was_edited = Rc::clone(&edited);
            let committed_bytes = Rc::new(RefCell::new(None));
            let editor_bytes = Rc::clone(&committed_bytes);
            let editor_tasks = tasks.clone();
            let desired_anchor = format!("{}#1", source_path.display());
            let _guard = before_task_write::install(&tasks, "SAFE-1", move || {
                let mut editor = Storage::new(&editor_tasks);
                editor
                    .mutate_task("SAFE-1", |fresh| {
                        fresh.comments.push(TaskComment {
                            date: "2026-10-20T00:00:00Z".into(),
                            text: "concurrent comment".into(),
                        });
                        fresh.references.push(ReferenceEntry {
                            code: Some("other.rs#7".into()),
                            ..Default::default()
                        });
                        if already_anchored {
                            fresh.references.push(ReferenceEntry {
                                code: Some(desired_anchor),
                                ..Default::default()
                            });
                        }
                        fresh.modified = "2000-01-01T00:00:00Z".into();
                        fresh
                            .custom_fields
                            .insert("severity".into(), custom_value_string("high"));
                        fresh.extra_fields.insert(
                            "user_meta".into(),
                            serde_yaml_ng::from_str("nested: {enabled: true, labels: [one, two]}")
                                .unwrap(),
                        );
                        Ok(true)
                    })
                    .unwrap();
                let task_path = editor_tasks.join("SAFE/1.yml");
                let formatted = format!(
                    "# Concurrent editor formatting\n{}",
                    fs::read_to_string(&task_path).unwrap()
                );
                fs::write(&task_path, formatted).unwrap();
                *editor_bytes.borrow_mut() = Some(fs::read(&task_path).unwrap());
                was_edited.set(true);
            });

            run(surface, &resolver, &source_path);
            assert!(edited.get(), "concurrent editor was not exercised");
            let updated = Storage::new(&tasks).get("SAFE-1", "SAFE").unwrap();
            if already_anchored {
                assert_eq!(updated.modified, "2000-01-01T00:00:00Z");
                assert_eq!(
                    fs::read(tasks.join("SAFE/1.yml")).unwrap(),
                    *committed_bytes.borrow().as_ref().unwrap()
                );
            } else {
                assert_ne!(updated.modified, "2000-01-01T00:00:00Z");
            }
            assert_eq!(updated.comments.len(), 1, "concurrent comment was lost");
            assert_eq!(updated.comments[0].text, "concurrent comment");
            assert_eq!(
                updated.custom_fields.get("severity"),
                Some(&custom_value_string("high"))
            );
            assert_eq!(
                updated.extra_fields.get("user_meta"),
                Some(
                    &serde_yaml_ng::from_str("nested: {enabled: true, labels: [one, two]}")
                        .unwrap()
                )
            );
            assert_eq!(updated.references.len(), 2, "concurrent reference was lost");
            assert!(
                updated
                    .references
                    .iter()
                    .any(|r| r.code.as_deref() == Some("other.rs#7"))
            );
            assert!(updated.references.iter().any(|r| {
                r.code
                    .as_deref()
                    .is_some_and(|code| code.ends_with("main.rs#1"))
            }));
            let expected_source = if create {
                "// TODO (SAFE-1): create task\n"
            } else {
                source
            };
            assert_eq!(fs::read_to_string(&source_path).unwrap(), expected_source);

            // An unchanged repeat must preserve user formatting and both file mtimes.
            let task_path = tasks.join("SAFE/1.yml");
            let formatted = format!(
                "# User-owned formatting\n{}user_layout: {{flag: 'yes'}}\n",
                fs::read_to_string(&task_path).unwrap()
            );
            fs::write(&task_path, &formatted).unwrap();
            let old =
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800);
            for path in [&task_path, &source_path] {
                fs::File::options()
                    .write(true)
                    .open(path)
                    .unwrap()
                    .set_modified(old)
                    .unwrap();
            }
            run(surface, &resolver, &source_path);
            assert_eq!(fs::read_to_string(&task_path).unwrap(), formatted);
            assert_eq!(fs::read_to_string(&source_path).unwrap(), expected_source);
            for path in [&task_path, &source_path] {
                assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), old);
            }
        }
    }

    #[test]
    fn cli_create_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Cli, "// TODO: create task\n", true);
    }

    #[test]
    fn cli_known_todo_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Cli, "// TODO (SAFE-1): existing task\n", false);
    }

    #[test]
    fn cli_bare_mention_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Cli, "// SAFE-1 bare mention\n", false);
    }

    #[test]
    fn service_create_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Service, "// TODO: create task\n", true);
    }

    #[test]
    fn service_known_todo_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Service, "// TODO (SAFE-1): existing task\n", false);
    }

    #[test]
    fn service_bare_mention_preserves_concurrent_edits() {
        preserves_concurrent_edits(Surface::Service, "// SAFE-1 bare mention\n", false);
    }
}

#[cfg(test)]
mod scan_utils_tests {
    use super::{parse_inline_attributes, strip_bracket_attributes};

    #[test]
    fn refresh_relocates_stale_anchors_to_unoccupied_live_mentions_and_keeps_metadata() {
        use crate::types::ReferenceEntry;
        let mut references = vec![
            ReferenceEntry {
                code: Some("main.rs#1".into()),
                ..Default::default()
            },
            ReferenceEntry {
                code: Some("main.rs#2".into()),
                link: Some("https://example.invalid/a".into()),
                ..Default::default()
            },
            ReferenceEntry {
                code: Some("main.rs#2".into()),
                github: Some("example/repo#1".into()),
                ..Default::default()
            },
        ];
        let live = vec!["main.rs#1".into(), "main.rs#4".into()];
        assert!(super::refresh_code_reference(
            &mut references,
            "main.rs#1",
            &live,
            &[]
        ));
        assert_eq!(references.len(), 3);
        assert_eq!(references[0].code.as_deref(), Some("main.rs#1"));
        assert_eq!(references[1].code.as_deref(), Some("main.rs#4"));
        assert_eq!(
            references[1].link.as_deref(),
            Some("https://example.invalid/a")
        );
        assert_eq!(references[2].code.as_deref(), Some("main.rs#4"));
        assert_eq!(references[2].github.as_deref(), Some("example/repo#1"));
        let before = references.clone();
        assert!(!super::refresh_code_reference(
            &mut references,
            "main.rs#4",
            &live,
            &[]
        ));
        assert_eq!(references, before);
    }

    #[cfg(unix)]
    #[test]
    fn locked_inode_rename_preserves_handles_on_unix() {
        use std::io::Read;
        use std::os::unix::fs::MetadataExt;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.rs");
        let replacement = temp.path().join("replacement.rs");
        std::fs::write(&source, "original").unwrap();
        std::fs::write(&replacement, "replacement").unwrap();
        let mut old = super::lock_source(&source).unwrap();
        let mut new = super::lock_source(&replacement).unwrap();
        std::fs::rename(&replacement, &source)
            .expect("rename while both read/write inodes are locked");
        let published = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&source)
            .unwrap();
        assert_eq!(
            published.metadata().unwrap().ino(),
            new.metadata().unwrap().ino()
        );
        assert_ne!(
            published.metadata().unwrap().ino(),
            old.metadata().unwrap().ino()
        );
        assert_eq!(
            fs2::FileExt::try_lock_exclusive(&published)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::WouldBlock
        );
        let mut old_bytes = String::new();
        let mut new_bytes = String::new();
        old.read_to_string(&mut old_bytes).unwrap();
        new.read_to_string(&mut new_bytes).unwrap();
        assert_eq!(old_bytes, "original");
        assert_eq!(new_bytes, "replacement");
        drop(new);
        fs2::FileExt::try_lock_exclusive(&published).unwrap();
    }

    #[test]
    fn cooperating_source_writers_compare_after_prepare_and_rename() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("main.rs");
        std::fs::write(&path, "original").unwrap();
        let barrier = std::sync::Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                super::write_source_with_prepared(&path, "original", "first", || {
                    barrier.wait();
                    Ok(())
                })
            });
            let second = scope.spawn(|| {
                super::write_source_with_prepared(&path, "original", "second", || {
                    barrier.wait();
                    Ok(())
                })
            });
            [first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .all(|error| error.contains("Source changed"))
        );
        let winner = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            winner,
            if results[0].is_ok() {
                "first"
            } else {
                "second"
            }
        );
        assert!(super::write_source(&path, "original", "stale").is_err());
        super::write_source(&path, &winner, "next inode").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "next inode");
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

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
