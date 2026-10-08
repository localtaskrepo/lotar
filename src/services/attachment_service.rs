use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::reference_service::{ReferenceMutationOutcome, ReferenceService};
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
        #[cfg(test)]
        STORE_LOCKS_HELD.with(|cell| cell.set(cell.get() + 1));
        Ok(AttachmentStoreGuard { _file: file })
    }

    /// Test-only count of attachment store locks currently held on this
    /// thread. `ReferenceService::dispatch_post_commit` asserts it is zero
    /// so reference automation provably runs only after store locks drop.
    #[cfg(test)]
    pub fn store_locks_held() -> usize {
        STORE_LOCKS_HELD.with(|cell| cell.get())
    }

    /// Take the store coordination lock for the configured attachments
    /// store so managed attachment reference attach/detach from any
    /// surface (MCP, CLI) serializes with blob creation, dedup, and
    /// reclamation exactly like the REST upload/remove flows. The lock is
    /// taken unconditionally: managed operations always target the store
    /// (DEV-61 keeps store blobs reachable only through the typed
    /// `attachment` kind), unlike repository `file` references which can
    /// never resolve inside the store.
    pub fn lock_store_for_attachments(
        tasks_root: &Path,
        config: &crate::config::types::ResolvedConfig,
    ) -> LoTaRResult<AttachmentStoreGuard> {
        let store_root = Self::resolve_attachments_root(tasks_root, config)?;
        Self::lock_store(&store_root)
    }

    /// Detach a MANAGED attachment reference (typed `attachment` entry).
    /// Returns whether this task actually carried the reference so callers
    /// can gate blob cleanup on typed membership: a task that never held
    /// the managed reference (or only holds a same-named repository `file`
    /// entry) must never trigger store reclamation. Repository `file`
    /// entries are never touched by this call.
    pub fn detach_managed_reference(
        storage: &mut Storage,
        task_id: &str,
        stored_name: &str,
    ) -> LoTaRResult<ReferenceMutationOutcome> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }
        let name = Self::validate_managed_name(stored_name)?.to_string();

        // Reference-only detach: works for missing/stale blobs (idempotent
        // changed=false); blob cleanup stays with the caller, gated on the
        // typed-membership signal in the returned outcome.
        ReferenceService::commit_reference_change(
            storage,
            task_id,
            "attachment",
            false,
            move |task, changed_values| {
                let before_len = task.references.len();
                task.references
                    .retain(|r| r.attachment.as_deref() != Some(name.as_str()));
                if task.references.len() == before_len {
                    return false;
                }
                changed_values.push(name.clone());
                true
            },
        )
    }

    /// Managed attachment values are bare store leaf names. Reject path
    /// shapes (separators, `.`/`..`, absolute prefixes) so a managed value
    /// can never be smuggled in as a path into — or out of — the store.
    pub fn validate_managed_name(stored: &str) -> LoTaRResult<&str> {
        let trimmed = stored.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Missing attachment name".to_string(),
            ));
        }
        if trimmed.len() > 4096 {
            return Err(LoTaRError::ValidationError(
                "Attachment name is too long (max 4096 characters)".to_string(),
            ));
        }
        let path = Path::new(trimmed);
        if trimmed.contains(['/', '\\'])
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(LoTaRError::ValidationError(
                "Attachment reference must be a stored file name, not a path".to_string(),
            ));
        }
        Ok(trimmed)
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

    /// Typed refcount over MANAGED `attachment` entries that belong to the
    /// SAME attachments store as `store_root`. Repository `file` entries
    /// never keep a blob alive, and neither do same-hash references whose
    /// tasks resolve to a different project-configured store: content-hash
    /// names are only unique within one store, so a blob re-created in
    /// another store must not block reclamation here (and vice versa).
    /// Tasks sharing one configured store root still protect each other.
    /// Soft-deleted tombstones COUNT as live references (DEV-92): their
    /// blobs must survive until the task is hard-deleted or restored and
    /// the reference removed. Any resolution failure (unparseable id,
    /// ambiguous location, config or store error) counts conservatively as
    /// a live reference so a blob is never deleted on uncertain grounds.
    pub fn is_hash_referenced(storage: &Storage, store_root: &Path, hash_tag: &str) -> bool {
        let target = hash_tag.trim();
        if target.len() != 32 || !target.bytes().all(|b: u8| b.is_ascii_hexdigit()) {
            return false;
        }
        let target = target.to_ascii_lowercase();
        let store_canonical = store_root
            .canonicalize()
            .unwrap_or_else(|_| store_root.to_path_buf());
        let all = storage.search(&TaskFilter {
            deletion: crate::storage::DeletionFilter::All,
            ..Default::default()
        });
        for (id, task) in all {
            let has_match = task.references.iter().any(|reference| {
                reference
                    .attachment
                    .as_deref()
                    .and_then(Self::extract_hash_tag)
                    .is_some_and(|hash| hash == target)
            });
            if !has_match {
                continue;
            }
            // Which store does this task's attachment belong to? Failures
            // conservatively count as referencing THIS store's blob.
            let Ok(parsed) = crate::storage::TaskId::parse(&id) else {
                return true;
            };
            let Ok(location) = storage.resolve_task_location(&id) else {
                return true;
            };
            let Ok(cfg) = crate::config::resolution::config_for_project(
                &location.root,
                Some(&parsed.project),
            ) else {
                return true;
            };
            let Ok(root) = Self::resolve_attachments_root(&location.root, &cfg) else {
                return true;
            };
            let canonical = root.canonicalize().unwrap_or(root);
            if canonical == store_canonical {
                return true;
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

    /// Attach a MANAGED attachment reference (typed `attachment` entry
    /// holding the stored blob name). Dedup is exact-string over the typed
    /// key: a repository `file` entry with the same text never counts as
    /// membership and is never conflated with the managed reference.
    pub fn attach_managed_reference(
        storage: &mut Storage,
        task_id: &str,
        stored_name: &str,
    ) -> LoTaRResult<ReferenceMutationOutcome> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }
        let name = Self::validate_managed_name(stored_name)?.to_string();

        ReferenceService::commit_reference_change(
            storage,
            task_id,
            "attachment",
            true,
            move |task, changed_values| {
                let already = task
                    .references
                    .iter()
                    .any(|r| r.attachment.as_deref() == Some(name.as_str()));
                if already {
                    return false;
                }
                task.references.push(ReferenceEntry {
                    attachment: Some(name.clone()),
                    ..Default::default()
                });
                changed_values.push(name.clone());
                true
            },
        )
    }

    pub fn resolve_attachment_path(root: &Path, rel_path: &str) -> Result<PathBuf, String> {
        Self::resolve_attachment_path_typed(root, rel_path).map_err(|e| e.to_string())
    }

    /// Typed variant of [`Self::resolve_attachment_path`] (DEV-58): REST
    /// routes classify the failure by variant instead of matching on
    /// message text. `Invalid` keeps the path-safety validation guards a
    /// client error, `Missing` means no stored blob exists under the root,
    /// and `RootUnavailable` is a server-side store-resolution failure.
    /// The string-returning wrapper above stays for CLI/MCP callers and
    /// produces identical messages.
    pub fn resolve_attachment_path_typed(
        root: &Path,
        rel_path: &str,
    ) -> Result<PathBuf, AttachmentPathError> {
        let rel = Path::new(rel_path);
        if rel_path.trim().is_empty() {
            return Err(AttachmentPathError::Invalid("Missing path"));
        }
        if rel.is_absolute() {
            return Err(AttachmentPathError::Invalid("Invalid attachment path"));
        }
        if rel.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(AttachmentPathError::Invalid("Invalid attachment path"));
        }

        let root_canon = fs::canonicalize(root)
            .map_err(|e| AttachmentPathError::RootUnavailable(e.to_string()))?;
        let joined = root.join(rel);
        let joined_canon = fs::canonicalize(&joined).map_err(|_| AttachmentPathError::Missing)?;
        if !joined_canon.starts_with(&root_canon) {
            return Err(AttachmentPathError::Invalid("Invalid attachment path"));
        }
        Ok(joined_canon)
    }
}

/// Typed resolution failure for a stored attachment path (DEV-58). Display
/// strings match the legacy string errors exactly.
#[derive(Debug)]
pub enum AttachmentPathError {
    /// Path-syntax or safety-guard rejection (client fault).
    Invalid(&'static str),
    /// No stored blob exists under the attachments root.
    Missing,
    /// The attachments root itself could not be resolved (server fault).
    RootUnavailable(String),
}

impl std::fmt::Display for AttachmentPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachmentPathError::Invalid(msg) => f.write_str(msg),
            AttachmentPathError::Missing => f.write_str("Attachment not found"),
            AttachmentPathError::RootUnavailable(msg) => f.write_str(msg),
        }
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

impl Drop for AttachmentStoreGuard {
    fn drop(&mut self) {
        #[cfg(test)]
        STORE_LOCKS_HELD.with(|cell| cell.set(cell.get().saturating_sub(1)));
    }
}

// Test-only count of attachment store locks currently held on this thread.
// ReferenceService::dispatch_post_commit asserts it is zero so reference
// automation provably runs only after store locks drop.
#[cfg(test)]
thread_local! {
    static STORE_LOCKS_HELD: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
