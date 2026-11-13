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
        Self { strip_attributes: true, ..Self::cli_fallback() }
    }
}

/// Resolve the shared scanner snapshot from a configuration manager, using
/// the project override when available and the resolved root config
/// otherwise. Pure lookup: selection order matches both historical surfaces.
pub(crate) fn scan_config_from_manager(manager: &crate::config::manager::ConfigManager, project: Option<&str>) -> ScanConfigSnapshot {
    let resolved = match project {
        Some(project_name) => manager.get_project_config(project_name).unwrap_or_else(|_| manager.get_resolved_config().clone()),
        None => manager.get_resolved_config().clone(),
    };
    let issue_type_words =
        if resolved.scan_enable_ticket_words { resolved.issue_types.values.iter().map(|t| t.to_string()).collect::<Vec<_>>() } else { Vec::new() };
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
pub(crate) fn merge_signal_words(signal_words: &[String], issue_type_words: &[String]) -> Vec<String> {
    let mut merged = signal_words.to_vec();
    for word in issue_type_words {
        if !merged.iter().any(|existing| existing.eq_ignore_ascii_case(word)) {
            merged.push(word.clone());
        }
    }
    merged
}

/// Build the quote-aware line scanner both surfaces use for comment ranges,
/// key extraction, and mention analysis. Signal words are applied only when
/// provided, matching the historical per-surface construction.
pub(crate) fn configured_line_scanner(ticket_patterns: Option<&[String]>, enable_ticket_words: bool, signal_words: &[String]) -> crate::scanner::Scanner {
    let mut scanner = crate::scanner::Scanner::new(std::path::PathBuf::from(".")).with_ticket_detection(ticket_patterns, enable_ticket_words);
    if !signal_words.is_empty() {
        scanner = scanner.with_signal_words(signal_words);
    }
    scanner
}

/// Replace a specific 1-based line in the file content and return the new
/// content. Only that line's text changes: every line keeps its own ending
/// (`\r\n` or `\n`), and a missing final newline stays missing (DEV-261).
pub(crate) fn replace_line_contents(contents: &str, line_number_1based: usize, new_line: &str) -> Option<String> {
    if line_number_1based == 0 {
        return None;
    }
    let mut out = String::with_capacity(contents.len() + new_line.len());
    let mut found = false;
    for (index, line) in contents.split_inclusive('\n').enumerate() {
        if index + 1 == line_number_1based {
            let ending = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            out.push_str(new_line);
            out.push_str(ending);
            found = true;
        } else {
            out.push_str(line);
        }
    }
    found.then_some(out)
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
                && key.trim().chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
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
    let file = code_ref.rsplit_once('#').map(|(file, _)| file).unwrap_or(code_ref);
    let mut occupied: std::collections::HashSet<String> = references.iter().filter_map(|reference| reference.code.clone()).collect();
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
            && !is_user_attached(history, code)
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
                            (path == file && !occupied.contains(candidate)).then_some((candidate, line))
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
    if !references.iter().any(|r| r.code.as_deref() == Some(code_ref)) {
        references.push(crate::types::ReferenceEntry { code: Some(code_ref.to_string()), ..Default::default() });
        changed = true;
    }
    if dedupe_code_references(references) {
        changed = true;
    }
    changed
}

/// Drop plain code references (nothing but `code`) that repeat an earlier
/// plain one. Entries that carry more data are never touched, nor is a
/// plain anchor that shares its line with such an entry. Returns whether
/// anything was removed.
pub(crate) fn dedupe_code_references(references: &mut Vec<crate::types::ReferenceEntry>) -> bool {
    let bare = |r: &crate::types::ReferenceEntry| crate::types::ReferenceEntry { code: None, ..r.clone() } == crate::types::ReferenceEntry::default();
    let mut seen = std::collections::HashSet::new();
    let before = references.len();
    references.retain(|r| match r.code.as_deref() {
        Some(code) if bare(r) => seen.insert(code.to_string()),
        _ => true,
    });
    references.len() != before
}

/// Whether a code reference was attached by a person rather than by scan.
/// Scan only ever writes single-line `path#N` anchors and records no history
/// for them, while explicit attachments (CLI, REST, MCP, UI) record a
/// `reference_added` entry. Path-only and ranged references (`path#A-B`)
/// therefore always count as user-attached.
pub(crate) fn is_user_attached(history: &[crate::types::TaskChangeLogEntry], code_ref: &str) -> bool {
    let Some((_, anchor)) = code_ref.rsplit_once('#') else {
        return true;
    };
    if anchor.parse::<usize>().is_err() {
        return true;
    }
    let recorded = format!("code:{code_ref}");
    history.iter().any(|entry| entry.changes.iter().any(|change| change.field == "reference_added" && change.new.as_deref() == Some(recorded.as_str())))
}

/// Validate without changing the source. Reject symlinks rather than replacing
/// their directory entry, and honor read-only source permissions even with rename.
pub fn validate_source_write(path: &std::path::Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_file() || metadata.permissions().readonly() {
        return Err(format!("Source is not a writable regular file: {}", path.display()));
    }
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    if std::fs::metadata(parent).map_err(|e| e.to_string())?.permissions().readonly() {
        return Err(format!("Source directory is read-only: {}", parent.display()));
    }
    std::fs::OpenOptions::new().write(true).open(path).map_err(|e| format!("Cannot prepare source write: {e}"))?;
    Ok(())
}

pub fn write_source(path: &std::path::Path, expected: &str, updated: &str) -> Result<(), String> {
    write_source_with_prepared(path, expected, updated, || Ok(()))
}

#[cfg(unix)]
#[doc(hidden)]
pub fn lock_source(path: &std::path::Path) -> Result<std::fs::File, String> {
    use std::os::unix::fs::MetadataExt;
    for _ in 0..8 {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| format!("Cannot open source inode for locking {}: {e}", path.display()))?;
        fs2::FileExt::lock_exclusive(&file).map_err(|e| format!("Cannot exclusively lock source inode {}: {e}", path.display()))?;
        let held = file.metadata().map_err(|e| e.to_string())?;
        let current = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if current.file_type().is_file() && held.dev() == current.dev() && held.ino() == current.ino() {
            return Ok(file);
        }
        // Another writer renamed while we waited on the old inode. Reopen and
        // lock the current inode; never compare/write under a stale inode lock.
    }
    Err("Source repeatedly replaced while acquiring its lock".to_string())
}

#[cfg(not(unix))]
#[doc(hidden)]
pub fn lock_source(path: &std::path::Path) -> Result<std::fs::File, String> {
    // Stable per-path locks avoid relying on unavailable portable inode IDs.
    // Never unlink locks because another process may already be waiting on one.
    let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let lock_dir = dirs::cache_dir().ok_or("No user cache directory for source locks")?.join("lotar/source-locks");
    std::fs::create_dir_all(&lock_dir).map_err(|e| e.to_string())?;
    let lock_path = lock_dir.join(format!("{}.lock", blake3::hash(canonical.as_os_str().as_encoded_bytes()).to_hex()));
    let lock = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path).map_err(|e| e.to_string())?;
    fs2::FileExt::lock_exclusive(&lock).map_err(|e| e.to_string())?;
    Ok(lock)
}

#[doc(hidden)]
pub fn write_source_with_prepared(path: &std::path::Path, expected: &str, updated: &str, prepared: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    validate_source_write(path)?;
    let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let permissions = std::fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    let staged = parent.join(format!(".lotar-scan-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)
        .map_err(|e| format!("Cannot reserve source replacement {}: {e}", staged.display()))?;
    let result = (|| -> Result<(), String> {
        crate::storage::safety::atomic_write_file(&staged, updated).map_err(|e| format!("Cannot prepare source replacement {}: {e}", staged.display()))?;
        std::fs::set_permissions(&staged, permissions).map_err(|e| format!("Cannot preserve source replacement permissions {}: {e}", staged.display()))?;
        prepared()?;
        let _source_lock = lock_source(path)?;
        // Keep the replacement inode locked across rename too, so writers
        // opening the new source cannot enter until this commit finishes.
        #[cfg(unix)]
        let _replacement_lock = lock_source(&staged)?;
        validate_source_write(path)?;
        if std::fs::canonicalize(path).map_err(|e| e.to_string())? != canonical || std::fs::read_to_string(path).map_err(|e| e.to_string())? != expected {
            return Err(format!("Source changed while scanning: {}", path.display()));
        }
        // Cooperating scans are serialized. Non-locking editors still have a
        // small race between this comparison and rename; this is not an OS CAS.
        std::fs::rename(&staged, path).map_err(|e| format!("Cannot publish source replacement {}: {e}", path.display()))
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
