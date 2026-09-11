use crate::api_types::TaskDTO;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::task_service::TaskService;
use crate::storage::TaskFilter;
use crate::storage::manager::Storage;
use crate::types::ReferenceEntry;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub struct AttachmentService;

impl AttachmentService {
    pub fn compute_attachments_root(
        tasks_dir: &Path,
        config: &crate::config::types::ResolvedConfig,
    ) -> LoTaRResult<PathBuf> {
        let raw = config.attachments_dir.trim();
        if raw.is_empty() {
            return Err(LoTaRError::ValidationError(
                "attachments_dir cannot be empty".to_string(),
            ));
        }

        let configured = Path::new(raw);
        let root = if configured.is_absolute() {
            configured.to_path_buf()
        } else {
            if configured
                .components()
                .any(|c| matches!(c, Component::ParentDir))
            {
                return Err(LoTaRError::ValidationError(
                    "attachments_dir cannot contain '..'".to_string(),
                ));
            }
            tasks_dir.join(configured)
        };
        Ok(root)
    }

    pub fn resolve_attachments_root(
        tasks_dir: &Path,
        config: &crate::config::types::ResolvedConfig,
    ) -> LoTaRResult<PathBuf> {
        let root = Self::compute_attachments_root(tasks_dir, config)?;
        fs::create_dir_all(&root)?;
        Ok(root)
    }

    /// Store `bytes` under `root`, deduplicated by content hash. Returns the
    /// stored filename and whether this call created the file (false when an
    /// identical file already existed), so callers can clean up orphaned
    /// writes without deleting content other tasks still reference.
    pub fn store_bytes(
        root: &Path,
        original_filename: &str,
        bytes: &[u8],
    ) -> LoTaRResult<(String, bool)> {
        fs::create_dir_all(root)?;

        let safe_original = sanitize_original_filename(original_filename);
        let (stem, ext) = split_stem_ext(&safe_original);

        let hash = blake3::hash(bytes);
        let hash_hex = hash.to_hex().to_string();
        let hash_tag = &hash_hex[..32];
        let dedupe_suffix_dot = format!(".{hash_tag}");
        let dedupe_suffix_dash = format!("-{hash_tag}");

        let mut existing_dot: Option<(PathBuf, String)> = None;
        let mut existing_dash: Option<(PathBuf, String)> = None;

        // Prefer dedupe: if a file with this hash already exists, reuse it.
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let file_stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("");
                let name = match path.file_name().and_then(OsStr::to_str) {
                    Some(n) => n.to_string(),
                    None => continue,
                };

                if file_stem.ends_with(&dedupe_suffix_dot) {
                    existing_dot = Some((path, name));
                } else if file_stem.ends_with(&dedupe_suffix_dash) {
                    existing_dash = Some((path, name));
                }
            }
        }

        if let Some((_path, name)) = existing_dot {
            return Ok((name, false));
        }

        if let Some((dash_path, dash_name)) = existing_dash {
            let _ = dash_path;
            return Ok((dash_name, false));
        }

        let base_stem = if stem.is_empty() {
            "file"
        } else {
            stem.as_str()
        };
        let base_stem = truncate_component(base_stem, 80);

        let mut filename = format!("{}.{}", base_stem, hash_tag);
        if let Some(ext) = ext {
            filename.push('.');
            filename.push_str(&truncate_component(&ext, 10));
        }

        let target = root.join(&filename);
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)
        {
            Ok(mut file) => {
                file.write_all(bytes)?;
                Ok((filename, true))
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok((filename, false)),
            Err(err) => Err(LoTaRError::IoError(err)),
        }
    }

    /// Remove a stored attachment file this process just created. Only safe
    /// for files reported as newly created by [`Self::store_bytes`]; deduped
    /// pre-existing content must be preserved.
    pub fn remove_created_file(root: &Path, stored_name: &str) {
        let sanitized = sanitize_original_filename(stored_name);
        let _ = fs::remove_file(root.join(sanitized));
    }

    /// Acquire the cross-process coordination lock for one attachments
    /// store (root), backed by the same fs2 advisory locking as the task
    /// storage locks (`.attachments-store.lock` inside the store directory,
    /// bounded 2s contention retry, fail closed).
    ///
    /// Upload handlers hold it across `store_bytes` (dedup/create), the
    /// reference attach, and failure cleanup; remove handlers hold it across
    /// detach, reference re-check, and blob deletion — always in
    /// store-lock -> task-lock order. A concurrent upload can never
    /// dedupe-and-attach a blob that a failing request is still rolling
    /// back, and a concurrent remove cannot delete a blob that an in-flight
    /// attach is about to reference. Works across API processes because the
    /// lock lives on the filesystem, not in process memory.
    ///
    /// The store directory is created if absent (idempotent; `store_bytes`
    /// requires it too) so the lock target always exists; the lock file
    /// itself is never removed.
    pub fn lock_store(root: &Path) -> LoTaRResult<AttachmentStoreGuard> {
        fs::create_dir_all(root)?;
        let file = crate::storage::safety::acquire_storage_lock(root, "attachments-store")
            .map_err(|err| LoTaRError::ValidationError(err.to_string()))?;
        Ok(AttachmentStoreGuard { _file: file })
    }

    /// Take the store coordination lock only when a file reference value
    /// resolves INSIDE the attachments store, so attaching or detaching a
    /// store blob from any surface (MCP, CLI) serializes with blob creation
    /// and reclamation exactly like the REST upload/remove flows. Paths
    /// outside the store return `None` and stay unlocked; unresolvable
    /// values return `None` (the attach/detach call itself will reject them).
    pub fn lock_store_for_repo_path(
        tasks_root: &Path,
        config: &crate::config::types::ResolvedConfig,
        repo_root: &Path,
        value: &str,
    ) -> LoTaRResult<Option<AttachmentStoreGuard>> {
        let Ok(store_root) = Self::resolve_attachments_root(tasks_root, config) else {
            return Ok(None);
        };
        let repo_canonical = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        let Ok(resolved) = crate::services::reference_service::ReferenceService::resolve_path(
            &repo_canonical,
            value.trim(),
        ) else {
            return Ok(None);
        };
        let store_canonical = store_root
            .canonicalize()
            .unwrap_or_else(|_| store_root.clone());
        if resolved.starts_with(&store_canonical) {
            Ok(Some(Self::lock_store(&store_root)?))
        } else {
            Ok(None)
        }
    }

    pub fn detach_file_reference(
        storage: &mut Storage,
        task_id: &str,
        file_rel: &str,
    ) -> LoTaRResult<TaskDTO> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let project = derived.to_string();
        let mut task = storage
            .get(task_id, &project)
            .ok_or_else(|| LoTaRError::TaskNotFound(task_id.to_string()))?;

        let before_len = task.references.len();
        task.references
            .retain(|r| r.file.as_deref() != Some(file_rel));

        if task.references.len() != before_len {
            task.modified = chrono::Utc::now().to_rfc3339();
            storage.edit(task_id, &task)?;
        }

        TaskService::get(storage, task_id, Some(&derived))
    }

    pub fn extract_hash_tag(file_rel: &str) -> Option<String> {
        let leaf = Path::new(file_rel)
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .trim();
        if leaf.is_empty() {
            return None;
        }

        // Handle `name.<hash>` (no extension): last segment is the hash.
        if let Some((_left, right)) = leaf.rsplit_once('.')
            && right.len() == 32
            && right.bytes().all(|b: u8| b.is_ascii_hexdigit())
        {
            return Some(right.to_ascii_lowercase());
        }

        // Handle `name.<hash>.ext` and `name-<hash>.ext`.
        let (stem, _ext) = match leaf.rsplit_once('.') {
            Some((left, right)) if !left.is_empty() && !right.is_empty() => (left, Some(right)),
            _ => (leaf, None),
        };
        if stem.len() < 33 {
            return None;
        }
        let delim = stem.as_bytes()[stem.len() - 33];
        if delim != b'.' && delim != b'-' {
            return None;
        }
        let hash = &stem[stem.len() - 32..];
        if !hash.bytes().all(|b: u8| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(hash.to_ascii_lowercase())
    }

    pub fn is_hash_referenced(storage: &Storage, hash_tag: &str) -> bool {
        let target = hash_tag.trim();
        if target.len() != 32 || !target.bytes().all(|b: u8| b.is_ascii_hexdigit()) {
            return false;
        }
        let target = target.to_ascii_lowercase();
        let all = storage.search(&TaskFilter::default());
        for (_id, task) in all {
            for reference in &task.references {
                if let Some(file_rel) = reference.file.as_deref()
                    && let Some(hash) = Self::extract_hash_tag(file_rel)
                    && hash == target
                {
                    return true;
                }
            }
        }
        false
    }

    pub fn delete_all_by_hash(root: &Path, hash_tag: &str) -> usize {
        if hash_tag.len() != 32 || !hash_tag.bytes().all(|b: u8| b.is_ascii_hexdigit()) {
            return 0;
        }
        let dot_suffix = format!(".{hash_tag}");
        let dash_suffix = format!("-{hash_tag}");
        let mut deleted = 0;
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let file_stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("");
                if (file_stem.ends_with(&dot_suffix) || file_stem.ends_with(&dash_suffix))
                    && std::fs::remove_file(&path).is_ok()
                {
                    deleted += 1;
                }
            }
        }
        deleted
    }

    pub fn find_attachment_by_hash(root: &Path, hash_tag: &str) -> Option<PathBuf> {
        if hash_tag.len() != 32 || !hash_tag.bytes().all(|b: u8| b.is_ascii_hexdigit()) {
            return None;
        }

        let dot_suffix = format!(".{hash_tag}");
        let dash_suffix = format!("-{hash_tag}");
        let mut dot_match: Option<PathBuf> = None;
        let mut dash_match: Option<PathBuf> = None;

        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let file_stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("");
                if file_stem.ends_with(&dot_suffix) {
                    dot_match = Some(path);
                } else if file_stem.ends_with(&dash_suffix) {
                    dash_match = Some(path);
                }
            }
        }

        dot_match.or(dash_match)
    }

    pub fn download_filename(stored_filename: &str) -> String {
        // `stored_filename` is expected to be a leaf like `name.<hash>.ext`.
        // This strips the hash suffix and keeps the original extension.
        let raw = stored_filename.trim();
        if raw.is_empty() {
            return "attachment".to_string();
        }

        let (stem, ext) = match raw.rsplit_once('.') {
            Some((left, right)) if !left.is_empty() && !right.is_empty() => {
                let looks_like_hash =
                    right.len() == 32 && right.bytes().all(|b: u8| b.is_ascii_hexdigit());
                if looks_like_hash {
                    // `name.<hash>` (no extension)
                    (raw, None)
                } else {
                    // `name.<hash>.ext` or `name-hash.ext`
                    (left, Some(right))
                }
            }
            _ => (raw, None),
        };

        let display_stem = strip_hash_suffix(stem).unwrap_or(stem);
        let display_stem = if display_stem.trim().is_empty() {
            "attachment"
        } else {
            display_stem
        };

        match ext {
            Some(ext) => format!("{}.{}", display_stem, ext),
            None => display_stem.to_string(),
        }
    }

    pub fn attach_file_reference(
        storage: &mut Storage,
        task_id: &str,
        file_rel: &str,
    ) -> LoTaRResult<(TaskDTO, bool)> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let project = derived.to_string();
        let mut task = storage
            .get(task_id, &project)
            .ok_or_else(|| LoTaRError::TaskNotFound(task_id.to_string()))?;

        let already = task
            .references
            .iter()
            .any(|r| r.file.as_deref() == Some(file_rel));

        let mut attached = false;
        if !already {
            task.references.push(ReferenceEntry {
                file: Some(file_rel.to_string()),
                ..Default::default()
            });
            task.modified = chrono::Utc::now().to_rfc3339();
            storage.edit(task_id, &task)?;
            attached = true;
        }

        Ok((
            TaskService::get(storage, task_id, Some(&derived))?,
            attached,
        ))
    }

    pub fn resolve_attachment_path(root: &Path, rel_path: &str) -> Result<PathBuf, String> {
        let rel = Path::new(rel_path);
        if rel_path.trim().is_empty() {
            return Err("Missing path".to_string());
        }
        if rel.is_absolute() {
            return Err("Invalid attachment path".to_string());
        }
        if rel.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err("Invalid attachment path".to_string());
        }

        let root_canon = fs::canonicalize(root).map_err(|e| e.to_string())?;
        let joined = root.join(rel);
        let joined_canon =
            fs::canonicalize(&joined).map_err(|_| "Attachment not found".to_string())?;
        if !joined_canon.starts_with(&root_canon) {
            return Err("Invalid attachment path".to_string());
        }
        Ok(joined_canon)
    }
}

fn strip_hash_suffix(stem: &str) -> Option<&str> {
    // Supports both legacy `name-<32hex>` and new `name.<32hex>` formats.
    // Stored attachment names are sanitized to ASCII, so byte slicing is safe.
    if stem.len() < 33 {
        return None;
    }
    let bytes = stem.as_bytes();
    let delim = bytes[stem.len() - 33];
    if delim != b'-' && delim != b'.' {
        return None;
    }

    let hash = &stem[stem.len() - 32..];
    if !hash.bytes().all(|b: u8| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(&stem[..stem.len() - 33])
}

fn sanitize_original_filename(input: &str) -> String {
    let leaf = Path::new(input)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("");
    let trimmed = leaf.trim();
    if trimmed.is_empty() {
        return "file".to_string();
    }

    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }

    let out = out.trim_matches('-').trim_matches('.').to_string();
    if out.is_empty() {
        "file".to_string()
    } else {
        out
    }
}

fn split_stem_ext(filename: &str) -> (String, Option<String>) {
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_string();
    let ext = path
        .extension()
        .and_then(OsStr::to_str)
        .map(|s| s.to_ascii_lowercase());
    (stem, ext)
}

fn truncate_component(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_string();
    }
    value.chars().take(max_len).collect::<String>()
}

/// Guard for the per-store attachments coordination lock. Dropping it
/// releases the underlying fs2 lock; the lock file itself stays behind.
pub struct AttachmentStoreGuard {
    _file: std::fs::File,
}

/// Test-only fault injection for the upload store/attach/cleanup sequence.
/// Thread-local, like the DEV-55 transaction fault hooks: production code
/// paths can never trigger it, and it is compiled out of release builds.
#[cfg(test)]
pub(crate) mod upload_fault {
    use std::cell::{Cell, RefCell};
    use std::sync::mpsc::{Receiver, Sender};

    thread_local! {
        static FAIL_NEXT_ATTACH: Cell<bool> = const { Cell::new(false) };
        static PARK_AFTER_STORE: RefCell<Option<(Sender<()>, Receiver<()>)>> =
            const { RefCell::new(None) };
    }

    /// Make the next attach on this thread fail after the blob was stored.
    pub(crate) fn fail_next_attach() {
        FAIL_NEXT_ATTACH.with(|cell| cell.set(true));
    }

    pub(crate) fn take_fail_next_attach() -> bool {
        FAIL_NEXT_ATTACH.with(|cell| cell.replace(false))
    }

    /// Park the next store->attach transition on this thread: signal
    /// `stored` once the blob exists, then block until `release`.
    pub(crate) fn arm_park_after_store(stored: Sender<()>, release: Receiver<()>) {
        PARK_AFTER_STORE.with(|slot| *slot.borrow_mut() = Some((stored, release)));
    }

    pub(crate) fn park_after_store_if_armed() {
        PARK_AFTER_STORE.with(|slot| {
            if let Some((stored, release)) = slot.borrow_mut().take() {
                let _ = stored.send(());
                // Bounded by the test, which always sends release or drops
                // the sender (recv then errors and the park ends).
                let _ = release.recv();
            }
        });
    }
}
