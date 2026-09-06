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
) -> bool {
    let file = code_ref
        .rsplit_once('#')
        .map(|(file, _)| file)
        .unwrap_or(code_ref);
    let mut changed = false;
    for reference in references.iter_mut() {
        if let Some(code) = reference.code.as_ref()
            && code
                .rsplit_once('#')
                .is_some_and(|(path, anchor)| path == file && anchor.parse::<usize>().is_ok())
            && code != code_ref
            && !live_refs.contains(code)
        {
            reference.code = Some(code_ref.to_string());
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
mod scan_utils_tests {
    use super::{parse_inline_attributes, strip_bracket_attributes};

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
