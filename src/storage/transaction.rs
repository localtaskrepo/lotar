//! Coordinated multi-file transactions for task and sprint mutations (DEV-55).
//!
//! A transaction stages every file write it intends to perform (task files,
//! sprint files, project config), captures the exact original bytes of each
//! target, and then publishes all writes atomically-by-rename under exclusive
//! advisory locks. Any publish failure rolls every file back to its original
//! bytes (or removes files the transaction created) before the error is
//! returned — unless the rollback itself fails, in which case the journal is
//! retained, participating mutations fail closed, and files may remain mixed
//! until a later recovery or manual reconciliation finishes the restore. A
//! write-ahead journal lets the next participating mutation recover a crashed
//! publish the same way before it proceeds; recovery fails closed when the
//! journal is corrupt, references unsafe paths, or a file was changed by
//! someone else.
//!
//! Bounded scope, deliberately: this is not a general-purpose transaction
//! framework and it does not make multi-file commits simultaneously visible to
//! readers. Limits:
//! - Crash recovery runs on the next participating mutation, not at mount or
//!   server start; until then, files may hold a partially published state.
//! - Rollback restores bytes, never directories: lock files and directories
//!   created while acquiring locks remain behind.
//! - Project config writes by non-participating commands are not lock
//!   coordinated (pre-existing behavior); a pending journal whose file was
//!   changed by one of them fails closed instead of clobbering the edit.
//! - No power-loss durability is claimed on ANY platform. The Unix fsyncs
//!   here are best-effort hardening, not a proven durable-commit protocol:
//!   after an acknowledged commit, power loss can in principle resurrect the
//!   unlinked journal and recovery would roll the acknowledged change back.
//!   Directory fsyncs are Unix-only (portable std::fs has none on Windows).

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::errors::{LoTaRError, LoTaRResult};
use crate::storage::safety::{acquire_storage_lock, atomic_write_bytes, atomic_write_file};

pub(crate) const JOURNAL_FILE_NAME: &str = ".txn-pending.json";
const JOURNAL_VERSION: u32 = 1;

/// Deterministic fault injection for tests. The hook lives under the
/// transaction primitive only; production code paths cannot trigger it.
#[cfg(test)]
pub(crate) mod fault {
    use std::cell::Cell;

    thread_local! {
        static FAIL_PUBLISH_AT: Cell<Option<usize>> = const { Cell::new(None) };
    }

    /// Fail the commit that publishes staged write `index` (0-based), after any
    /// earlier staged writes in the same commit have already been published.
    pub(crate) fn fail_publish_at(index: usize) {
        FAIL_PUBLISH_AT.with(|cell| cell.set(Some(index)));
    }

    pub(crate) fn take_fail_publish_at() -> Option<usize> {
        FAIL_PUBLISH_AT.with(|cell| cell.take())
    }

    thread_local! {
        static FAIL_PARENT_SYNC_ONCE: Cell<bool> = const { Cell::new(false) };
        static FAIL_JOURNAL_REMOVAL_TIMES: Cell<u32> = const { Cell::new(0) };
        static FAIL_JOURNAL_REMOVAL_FSYNC_ONCE: Cell<bool> = const { Cell::new(false) };
    }

    /// Make the next parent-directory fsync fail once (L2 propagation proof).
    pub(crate) fn fail_parent_sync_once() {
        FAIL_PARENT_SYNC_ONCE.with(|cell| cell.set(true));
    }

    pub(crate) fn take_fail_parent_sync_once() -> bool {
        FAIL_PARENT_SYNC_ONCE.with(|cell| cell.replace(false))
    }

    /// Make the next `n` journal-removal unlink attempts fail. A single
    /// failure proves the immediate-rollback invariant; two or more prove the
    /// retained-valid-journal path (review R1).
    pub(crate) fn fail_journal_removal_times(n: u32) {
        FAIL_JOURNAL_REMOVAL_TIMES.with(|cell| cell.set(n));
    }

    pub(crate) fn take_fail_journal_removal() -> bool {
        FAIL_JOURNAL_REMOVAL_TIMES.with(|cell| {
            let remaining = cell.get();
            if remaining > 0 {
                cell.set(remaining - 1);
                true
            } else {
                false
            }
        })
    }

    /// Make the fsync after the next successful journal-removal rename fail
    /// once; removal already succeeded, so the commit stays acknowledged.
    pub(crate) fn fail_journal_removal_fsync_once() {
        FAIL_JOURNAL_REMOVAL_FSYNC_ONCE.with(|cell| cell.set(true));
    }

    pub(crate) fn take_fail_journal_removal_fsync_once() -> bool {
        FAIL_JOURNAL_REMOVAL_FSYNC_ONCE.with(|cell| cell.replace(false))
    }
}

#[derive(Debug)]
struct StagedWrite {
    path: PathBuf,
    relative: String,
    contents: String,
    /// Exact original bytes; `None` means the file must not exist before commit.
    original: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
struct JournalWrite {
    /// Normal `/`-separated path relative to the tasks root.
    path: String,
    /// Base64 of the original bytes; absent when the file did not exist.
    original: Option<String>,
    /// Base64 of the intended new bytes.
    new: String,
}

#[derive(Serialize, Deserialize)]
struct PendingJournal {
    version: u32,
    writes: Vec<JournalWrite>,
}

/// Coordinates task, sprint, and project-config file writes so they either all
/// land or all revert. Locks are acquired once in a deterministic order
/// (sprints lock first, then per-project task locks in sorted prefix order) and
/// are held until the transaction is committed or dropped.
#[derive(Debug)]
pub(crate) struct MultiFileTransaction {
    root: PathBuf,
    journal_path: PathBuf,
    /// Dropping these files releases the advisory locks (SyncJournal precedent).
    _locks: Vec<File>,
    /// Project prefixes whose task locks are held. Staged writes outside these
    /// projects (and outside `@sprints`) are rejected so publish and recovery
    /// never touch files whose locks this transaction does not own.
    locked_prefixes: BTreeSet<String>,
    staged: Vec<StagedWrite>,
    committed: bool,
}

impl MultiFileTransaction {
    /// Acquire the coordinated locks and recover any pending journal from a
    /// crashed earlier transaction. `project_prefixes` are the task project
    /// directories whose files this transaction may write.
    ///
    /// Lock order is deterministic: the sprints lock first, then per-project
    /// task locks in sorted prefix order. The acquired set is the union of the
    /// caller's request and every project referenced by a pending journal,
    /// derived while already holding the sprints lock (the only lock through
    /// which journals are created), so recovery itself never restores a file
    /// whose task lock this transaction does not hold (review M1).
    pub(crate) fn begin(root: &Path, project_prefixes: &[String]) -> LoTaRResult<Self> {
        let sprints_dir = crate::storage::sprint::Sprint::dir(root);
        fs::create_dir_all(&sprints_dir)?;
        let mut locks = vec![acquire_storage_lock(
            &sprints_dir,
            crate::storage::safety::sprint_lock_name(),
        )?];

        let journal_path = root.join(JOURNAL_FILE_NAME);
        let mut sorted: BTreeSet<String> = BTreeSet::new();
        for prefix in project_prefixes {
            crate::storage::safety::validate_project_prefix(prefix)
                .map_err(LoTaRError::ValidationError)?;
            sorted.insert(prefix.clone());
        }
        for prefix in peek_pending_journal_projects(root, &journal_path)? {
            sorted.insert(prefix);
        }
        for prefix in &sorted {
            let project_dir = root.join(prefix);
            fs::create_dir_all(&project_dir)?;
            locks.push(acquire_storage_lock(&project_dir, "task")?);
        }

        recover_pending_journal(root, &journal_path)?;

        Ok(Self {
            root: root.to_path_buf(),
            journal_path,
            _locks: locks,
            locked_prefixes: sorted,
            staged: Vec::new(),
            committed: false,
        })
    }

    /// Fail closed unless every listed project's task lock is already held.
    /// Callers that discover additional projects after `begin` (from state
    /// read under the coordinated locks) use this to abort and retry with a
    /// wider lock set instead of writing into unlocked projects (review L1).
    pub(crate) fn ensure_covers(&self, project_prefixes: &[String]) -> LoTaRResult<()> {
        if let Some(missing) = project_prefixes
            .iter()
            .find(|prefix| !self.locked_prefixes.contains(*prefix))
        {
            return Err(LoTaRError::ValidationError(format!(
                "Transaction holds task locks for [{}] only; refusing writes for unlocked project '{missing}'",
                self.locked_prefixes
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        Ok(())
    }

    /// Stage `contents` for the file at `path`, which must live inside the
    /// transaction root, under a lock this transaction holds (a sprint file or
    /// a file inside a locked project), and must not traverse or be a symlink.
    /// The file's current bytes are captured as the rollback image; staging a
    /// path twice or staging a no-op write is rejected.
    pub(crate) fn stage(&mut self, path: &Path, contents: String) -> LoTaRResult<()> {
        if path == self.journal_path {
            return Err(LoTaRError::ValidationError(
                "Transaction cannot stage the recovery journal itself".to_string(),
            ));
        }
        let relative = relative_under_root(&self.root, path)?;
        // Same symlink discipline as journal recovery: staged backups and
        // publishes must target real directories the locks actually cover.
        validate_components_not_symlinks(&self.root, &relative)?;
        if fs::symlink_metadata(path).is_ok_and(|meta| meta.is_symlink()) {
            return Err(LoTaRError::ValidationError(format!(
                "Transaction cannot stage symlink '{}'",
                path.display()
            )));
        }
        let owner = relative.split('/').next().unwrap_or_default();
        if owner != "@sprints" && !self.locked_prefixes.contains(owner) {
            return Err(LoTaRError::ValidationError(format!(
                "Transaction cannot stage '{}': project '{}' has no held task lock",
                path.display(),
                owner
            )));
        }
        if self.staged.iter().any(|w| w.relative == relative) {
            return Err(LoTaRError::ValidationError(format!(
                "Transaction cannot stage '{}' twice",
                path.display()
            )));
        }
        let original = match fs::read(path) {
            Ok(bytes) => {
                if bytes == contents.as_bytes() {
                    // Writing identical bytes would churn the journal for nothing.
                    return Ok(());
                }
                Some(bytes)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => return Err(err.into()),
        };
        self.staged.push(StagedWrite {
            path: path.to_path_buf(),
            relative,
            contents,
            original,
        });
        Ok(())
    }

    /// Durably journal the staged writes, publish them all, then remove the
    /// journal. On any publish failure, roll back every published file to its
    /// exact original bytes (removing files this transaction created) and only
    /// then remove the journal.
    pub(crate) fn commit(mut self) -> LoTaRResult<()> {
        let result = self.publish();
        self.committed = result.is_ok();
        result
    }

    fn publish(&mut self) -> LoTaRResult<()> {
        if self.staged.is_empty() {
            return Ok(());
        }
        let journal = PendingJournal {
            version: JOURNAL_VERSION,
            writes: self
                .staged
                .iter()
                .map(|w| JournalWrite {
                    path: w.relative.clone(),
                    original: w
                        .original
                        .as_ref()
                        .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)),
                    new: base64::engine::general_purpose::STANDARD.encode(w.contents.as_bytes()),
                })
                .collect(),
        };
        write_journal(&self.journal_path, &journal)?;

        #[cfg(test)]
        let fail_at = fault::take_fail_publish_at();
        let outcome = (|| -> LoTaRResult<()> {
            for (write_index, write) in self.staged.iter().enumerate() {
                // Index only drives the test-only injection below.
                let _ = write_index;
                #[cfg(test)]
                if fail_at == Some(write_index) {
                    return Err(LoTaRError::IoError(std::io::Error::other(
                        "injected publish failure",
                    )));
                }
                atomic_write_file(&write.path, &write.contents)?;
                // Persist the directory entry of every completed rename before
                // continuing; an fsync failure here is a publish failure and
                // triggers rollback (review L2).
                sync_parent_dir(&write.path)?;
            }
            Ok(())
        })();

        if let Err(err) = outcome {
            let rollback = self.rollback_published();
            return match rollback {
                Ok(()) => match remove_journal(&self.journal_path) {
                    Ok(()) => Err(err),
                    Err(cleanup_err) => Err(LoTaRError::ValidationError(format!(
                        "{err}; every affected file was restored, but cleaning up the recovery journal failed ({cleanup_err}); the journal at {} is still valid (recovery verifies and retries its removal) and blocks participating mutations until then",
                        self.journal_path.display()
                    ))),
                },
                Err(rollback_err) => Err(LoTaRError::ValidationError(format!(
                    "{err}; automatic rollback also failed ({rollback_err}); recovery journal {} was retained and blocks further participating mutations until reconciled",
                    self.journal_path.display()
                ))),
            };
        }

        match remove_journal(&self.journal_path) {
            Ok(()) => Ok(()),
            Err(removal_err) => {
                // All writes were published, but nothing is acknowledged while
                // the journal still exists: restore every file under the locks
                // this transaction still holds so an error implies unchanged
                // state immediately, not just after the next recovery
                // (review R1).
                match self.rollback_published() {
                    Ok(()) => match remove_journal(&self.journal_path) {
                        Ok(()) => Err(removal_err),
                        Err(cleanup_err) => Err(LoTaRError::ValidationError(format!(
                            "{removal_err}; every affected file was restored; cleaning up the recovery journal still failed ({cleanup_err}); the retained journal at {} matches that state and recovery will verify and retry its removal",
                            self.journal_path.display()
                        ))),
                    },
                    Err(rollback_err) => Err(LoTaRError::ValidationError(format!(
                        "{removal_err}; rolling the published writes back also failed ({rollback_err}); recovery journal {} was retained and the next participating mutation will finish the rollback",
                        self.journal_path.display()
                    ))),
                }
            }
        }
    }

    /// Restore every staged file that no longer holds its original bytes.
    /// Unpublished files still match their originals and are skipped naturally.
    /// Deterministic reverse order.
    fn rollback_published(&self) -> LoTaRResult<()> {
        for write in self.staged.iter().rev() {
            match &write.original {
                Some(original) => {
                    match fs::read(&write.path) {
                        Ok(current) if current == *original => continue,
                        Ok(_) => {}
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            return Err(LoTaRError::ValidationError(format!(
                                "cannot roll back '{}': file disappeared",
                                write.path.display()
                            )));
                        }
                        Err(err) => {
                            return Err(LoTaRError::ValidationError(format!(
                                "cannot roll back '{}': {err}",
                                write.path.display()
                            )));
                        }
                    }
                    atomic_write_bytes(&write.path, original)?;
                    sync_parent_dir(&write.path)?;
                }
                None => match fs::symlink_metadata(&write.path) {
                    Ok(_) => {
                        fs::remove_file(&write.path)?;
                        sync_parent_dir(&write.path)?;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => return Err(err.into()),
                },
            }
        }
        Ok(())
    }
}

/// Restore a workspace that has a pending (crashed) transaction journal.
///
/// Every journaled file must currently hold either its original or its
/// intended-new bytes; anything else is an external conflicting edit and fails
/// closed so backups never clobber it.
fn recover_pending_journal(root: &Path, journal_path: &Path) -> LoTaRResult<()> {
    let raw = match fs::read_to_string(journal_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    let journal: PendingJournal = serde_json::from_str(&raw).map_err(|err| {
        LoTaRError::ValidationError(format!(
            "Corrupt pending transaction journal {}: {err}; do not delete it blindly; reconcile the affected task/sprint files manually or restore them from backups, then remove the journal",
            journal_path.display()
        ))
    })?;
    if journal.version != JOURNAL_VERSION {
        return Err(LoTaRError::ValidationError(format!(
            "Unsupported pending transaction journal version {} at {}; reconcile manually",
            journal.version,
            journal_path.display()
        )));
    }

    // Validate every path before touching any file so a hostile journal cannot
    // trigger partial recovery writes.
    let mut resolved: Vec<(PathBuf, JournalWrite)> = Vec::with_capacity(journal.writes.len());
    for write in journal.writes {
        let path = resolve_journal_relative_path(root, journal_path, &write.path)?;
        resolved.push((path, write));
    }

    for (path, write) in &resolved {
        let original = write
            .original
            .as_ref()
            .map(|encoded| {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|err| {
                        LoTaRError::ValidationError(format!(
                            "Invalid original-bytes payload for '{}' in {}: {err}",
                            write.path,
                            journal_path.display()
                        ))
                    })
            })
            .transpose()?;
        let new = base64::engine::general_purpose::STANDARD
            .decode(&write.new)
            .map_err(|err| {
                LoTaRError::ValidationError(format!(
                    "Invalid new-bytes payload for '{}' in {}: {err}",
                    write.path,
                    journal_path.display()
                ))
            })?;
        let current = fs::read(path);
        let state_ok = match (&original, &current) {
            (Some(original), Ok(current)) => *current == *original || *current == new,
            (Some(_), Err(_)) => false,
            (None, Err(err)) => err.kind() == std::io::ErrorKind::NotFound,
            (None, Ok(current)) => *current == new,
        };
        if !state_ok {
            return Err(LoTaRError::ValidationError(format!(
                "Pending transaction journal {} cannot be applied safely: '{}' no longer holds its original or intended bytes; reconcile the file manually, then remove the journal",
                journal_path.display(),
                path.display()
            )));
        }
        match &original {
            Some(original) => {
                if current
                    .as_ref()
                    .is_ok_and(|current| current == original.as_slice())
                {
                    continue;
                }
                atomic_write_bytes(path, original)?;
                sync_parent_dir(path)?;
            }
            None => {
                if current.is_ok() {
                    fs::remove_file(path)?;
                    sync_parent_dir(path)?;
                }
            }
        }
    }

    remove_journal(journal_path)
}

fn write_journal(journal_path: &Path, journal: &PendingJournal) -> LoTaRResult<()> {
    let payload = serde_json::to_string(journal).map_err(|err| {
        LoTaRError::SerializationError(format!("Failed to serialize transaction journal: {err}"))
    })?;
    atomic_write_file(journal_path, &payload)?;
    #[cfg(unix)]
    if let Some(parent) = journal_path.parent()
        && let Ok(dir) = File::open(parent)
    {
        dir.sync_all()?;
    }
    Ok(())
}

/// Unlink the journal. The successful unlink is the commit's linearization
/// point: once it has happened the operation is acknowledged, because
/// retrying a committed operation is what duplicates work (review M2).
/// - If the unlink fails after a successful publish, the caller of
///   [`MultiFileTransaction::commit`] immediately rolls back under its held
///   locks before the error surfaces, so "error means unchanged" holds without
///   waiting for a later recovery (review R1).
/// - If the unlink succeeds but the root-dir fsync fails, the removal is still
///   acknowledged and `Ok` is returned: the in-memory state machine is
///   complete and returning an error here would drive exactly the
///   duplicate-retry the acknowledgement point exists to prevent.
///
/// Durability caveat (review R2): no power-loss durability is claimed on any
/// platform. The Unix fsyncs in this module are best-effort hardening, not a
/// proven durable-commit protocol: after an acknowledged commit, power loss
/// can in principle resurrect the unlinked journal entry, and recovery would
/// then roll the acknowledged change back. After a rollback or recovery the
/// same window is harmless (files already match their originals, so a
/// resurrected journal verifies as a no-op).
fn remove_journal(journal_path: &Path) -> LoTaRResult<()> {
    #[cfg(test)]
    if fault::take_fail_journal_removal() {
        return Err(std::io::Error::other("injected journal removal failure").into());
    }
    match fs::remove_file(journal_path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    }
    #[cfg(unix)]
    if let Some(parent) = journal_path.parent()
        && let Ok(dir) = File::open(parent)
    {
        #[cfg(test)]
        if fault::take_fail_journal_removal_fsync_once() {
            return Ok(());
        }
        let _ = dir.sync_all();
    }
    Ok(())
}

/// fsync a file's parent directory so a completed rename is durable. Errors
/// propagate so callers treat lost durability as a failed write (review L2).
fn sync_parent_dir(path: &Path) -> LoTaRResult<()> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        #[cfg(test)]
        if fault::take_fail_parent_sync_once() {
            return Err(LoTaRError::IoError(std::io::Error::other(
                "injected parent directory sync failure",
            )));
        }
        let dir = File::open(parent).map_err(|err| {
            LoTaRError::ValidationError(format!(
                "cannot sync parent directory of '{}': {err}",
                path.display()
            ))
        })?;
        dir.sync_all().map_err(|err| {
            LoTaRError::ValidationError(format!(
                "failed to sync parent directory of '{}': {err}",
                path.display()
            ))
        })?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Read a pending journal and derive the set of project prefixes it touches,
/// validating every path exactly like recovery would. Runs under the sprints
/// lock, where journals are created, so the answer cannot change before the
/// derived task locks are acquired (review M1).
fn peek_pending_journal_projects(root: &Path, journal_path: &Path) -> LoTaRResult<Vec<String>> {
    let raw = match fs::read_to_string(journal_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };
    let journal: PendingJournal = serde_json::from_str(&raw).map_err(|err| {
        LoTaRError::ValidationError(format!(
            "Corrupt pending transaction journal {}: {err}; do not delete it blindly; reconcile the affected task/sprint files manually or restore them from backups, then remove the journal",
            journal_path.display()
        ))
    })?;
    if journal.version != JOURNAL_VERSION {
        return Err(LoTaRError::ValidationError(format!(
            "Unsupported pending transaction journal version {} at {}; reconcile manually",
            journal.version,
            journal_path.display()
        )));
    }
    let mut projects: BTreeSet<String> = BTreeSet::new();
    for write in &journal.writes {
        // Full validation (traversal, symlinks, self-target) before trusting
        // any prefix, so a hostile journal cannot influence lock acquisition
        // toward paths recovery would later refuse.
        resolve_journal_relative_path(root, journal_path, &write.path)?;
        if let Some(owner) = write.path.split('/').next()
            && owner != "@sprints"
        {
            crate::storage::safety::validate_project_prefix(owner)
                .map_err(LoTaRError::ValidationError)?;
            projects.insert(owner.to_string());
        }
    }
    Ok(projects.into_iter().collect())
}

/// Walk the path components below `root` and refuse any symlinked component,
/// mirroring journal path validation for production staging (review L3b).
fn validate_components_not_symlinks(root: &Path, relative: &str) -> LoTaRResult<()> {
    let mut path = root.to_path_buf();
    for component in relative.split('/') {
        path.push(component);
        if fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_symlink()) {
            return Err(LoTaRError::ValidationError(format!(
                "Transaction path '{}' passes through symlink '{}'; refusing to stage",
                relative,
                path.display()
            )));
        }
    }
    Ok(())
}

/// Path of `path` relative to `root`, as a normal `/`-separated string. The
/// path must be inside `root` without traversal components.
fn relative_under_root(root: &Path, path: &Path) -> LoTaRResult<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| {
            LoTaRError::ValidationError(format!(
                "Transaction path '{}' is outside the tasks root '{}'",
                path.display(),
                root.display()
            ))
        })?
        .to_string_lossy()
        .replace('\\', "/");
    validate_relative_path(&relative)?;
    if relative.is_empty() {
        return Err(LoTaRError::ValidationError(
            "Transaction path cannot be the tasks root itself".to_string(),
        ));
    }
    Ok(relative)
}

/// Resolve a journal-relative path against `root`, refusing absolute paths,
/// traversal, dot components, the journal itself, and any symlink component.
fn resolve_journal_relative_path(
    root: &Path,
    journal_path: &Path,
    relative: &str,
) -> LoTaRResult<PathBuf> {
    validate_relative_path(relative)?;
    let mut path = root.to_path_buf();
    for component in relative.split('/') {
        path.push(component);
        let is_symlink = fs::symlink_metadata(&path)
            .map(|meta| meta.is_symlink())
            .unwrap_or(false);
        if is_symlink {
            return Err(LoTaRError::ValidationError(format!(
                "Pending transaction journal path '{}' passes through symlink '{}'; reconcile manually",
                relative,
                path.display()
            )));
        }
    }
    if path == journal_path {
        return Err(LoTaRError::ValidationError(
            "Pending transaction journal cannot target itself".to_string(),
        ));
    }
    Ok(path)
}

fn validate_relative_path(relative: &str) -> LoTaRResult<()> {
    let mut components = Path::new(relative).components();
    let valid = components
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        && Path::new(relative)
            .components()
            .any(|component| matches!(component, Component::Normal(_)))
        && !relative.starts_with('/');
    if valid {
        Ok(())
    } else {
        Err(LoTaRError::ValidationError(format!(
            "Unsafe transaction path '{relative}': expected a relative path without traversal"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn workspace() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(".tasks");
        fs::create_dir_all(&root).unwrap();
        (tmp, root)
    }

    fn stage_all(txn: &mut MultiFileTransaction, writes: &[(&Path, &str)]) {
        for (path, contents) in writes {
            txn.stage(path, contents.to_string()).unwrap();
        }
    }

    #[test]
    fn commit_publishes_all_staged_writes_and_removes_journal() {
        let (_tmp, root) = workspace();
        let sprint = root.join("@sprints").join("1.yml");
        let task = root.join("DEV").join("1.yml");
        fs::create_dir_all(sprint.parent().unwrap()).unwrap();
        fs::create_dir_all(task.parent().unwrap()).unwrap();
        fs::write(&sprint, "original sprint").unwrap();

        let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        stage_all(&mut txn, &[(&sprint, "new sprint"), (&task, "new task")]);
        txn.commit().unwrap();

        assert_eq!(fs::read_to_string(&sprint).unwrap(), "new sprint");
        assert_eq!(fs::read_to_string(&task).unwrap(), "new task");
        assert!(!root.join(JOURNAL_FILE_NAME).exists());
    }

    #[test]
    fn identical_bytes_are_not_staged_or_journalled() {
        let (_tmp, root) = workspace();
        let file = root.join("DEV").join("1.yml");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "same").unwrap();

        let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        stage_all(&mut txn, &[(&file, "same")]);
        txn.commit().unwrap();
        assert!(!root.join(JOURNAL_FILE_NAME).exists());
    }

    #[test]
    fn midpublish_failure_restores_exact_original_bytes_and_removes_created_files() {
        let (_tmp, root) = workspace();
        let first = root.join("@sprints").join("1.yml");
        let second = root.join("@sprints").join("2.yml");
        let created = root.join("DEV").join("7.yml");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::create_dir_all(created.parent().unwrap()).unwrap();
        fs::write(&first, "first original").unwrap();
        fs::write(&second, "second original").unwrap();

        fault::fail_publish_at(1);
        let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        stage_all(
            &mut txn,
            &[
                (&first, "first new"),
                (&second, "second new"),
                (&created, "created"),
            ],
        );
        let err = txn.commit().unwrap_err();
        assert!(err.to_string().contains("injected publish failure"));

        assert_eq!(fs::read_to_string(&first).unwrap(), "first original");
        assert_eq!(fs::read_to_string(&second).unwrap(), "second original");
        assert!(!created.exists());
        assert!(!root.join(JOURNAL_FILE_NAME).exists());
    }

    #[cfg(unix)]
    #[test]
    fn midpublish_rollback_preserves_original_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let (_tmp, root) = workspace();
        let file = root.join("DEV").join("1.yml");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "private original").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();

        fault::fail_publish_at(0);
        let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        stage_all(&mut txn, &[(&file, "replacement")]);
        txn.commit().unwrap_err();

        assert_eq!(fs::read_to_string(&file).unwrap(), "private original");
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn journal_write_failure_leaves_every_file_unchanged() {
        let (_tmp, root) = workspace();
        let file = root.join("DEV").join("1.yml");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "original").unwrap();

        let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        // A directory in place of the journal makes the durable intent write fail.
        fs::create_dir_all(root.join(JOURNAL_FILE_NAME)).unwrap();
        stage_all(&mut txn, &[(&file, "new")]);
        assert!(txn.commit().is_err());

        assert_eq!(fs::read_to_string(&file).unwrap(), "original");
    }

    #[test]
    fn crashed_publish_is_recovered_by_the_next_begin() {
        let (_tmp, root) = workspace();
        let overwritten = root.join("@sprints").join("1.yml");
        let created = root.join("DEV").join("3.yml");
        fs::create_dir_all(overwritten.parent().unwrap()).unwrap();
        fs::create_dir_all(created.parent().unwrap()).unwrap();
        fs::write(&overwritten, "sprint original").unwrap();

        {
            let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
            stage_all(
                &mut txn,
                &[(&overwritten, "sprint new"), (&created, "created new")],
            );
            // Simulate a crash after the journal and the first file landed.
            let journal = PendingJournal {
                version: JOURNAL_VERSION,
                writes: vec![
                    JournalWrite {
                        path: "@sprints/1.yml".to_string(),
                        original: Some(
                            base64::engine::general_purpose::STANDARD.encode(b"sprint original"),
                        ),
                        new: base64::engine::general_purpose::STANDARD.encode(b"sprint new"),
                    },
                    JournalWrite {
                        path: "DEV/3.yml".to_string(),
                        original: None,
                        new: base64::engine::general_purpose::STANDARD.encode(b"created new"),
                    },
                ],
            };
            write_journal(&root.join(JOURNAL_FILE_NAME), &journal).unwrap();
            fs::write(&overwritten, "sprint new").unwrap();
            fs::write(&created, "created new").unwrap();
            // Deliberately drop without commit or rollback.
        }

        MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        assert_eq!(fs::read_to_string(&overwritten).unwrap(), "sprint original");
        assert!(!created.exists());
        assert!(!root.join(JOURNAL_FILE_NAME).exists());
    }

    #[test]
    fn corrupt_journal_fails_closed_and_is_kept() {
        let (_tmp, root) = workspace();
        let journal_path = root.join(JOURNAL_FILE_NAME);
        fs::write(&journal_path, "{not json").unwrap();

        let err = MultiFileTransaction::begin(&root, &[]).unwrap_err();
        assert!(
            err.to_string()
                .contains("Corrupt pending transaction journal")
        );
        assert!(journal_path.exists());
    }

    #[test]
    fn journal_with_path_traversal_fails_closed_without_touching_files() {
        let (_tmp, root) = workspace();
        let outside_target = root.parent().unwrap().join("escape.yml");
        let journal_path = root.join(JOURNAL_FILE_NAME);
        let journal = PendingJournal {
            version: JOURNAL_VERSION,
            writes: vec![JournalWrite {
                path: "../escape.yml".to_string(),
                original: None,
                new: base64::engine::general_purpose::STANDARD.encode(b"evil"),
            }],
        };
        fs::write(&journal_path, serde_json::to_string(&journal).unwrap()).unwrap();

        let err = MultiFileTransaction::begin(&root, &[]).unwrap_err();
        assert!(err.to_string().contains("Unsafe transaction path"));
        assert!(journal_path.exists());
        assert!(!outside_target.exists());
    }

    #[test]
    fn journal_with_symlinked_target_fails_closed() {
        let (_tmp, root) = workspace();
        let real = root.join("DEV").join("1.yml");
        fs::create_dir_all(root.join("DEV")).unwrap();
        fs::write(&real, "real").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, root.join("DEV").join("link.yml")).unwrap();

        let journal_path = root.join(JOURNAL_FILE_NAME);
        let journal = PendingJournal {
            version: JOURNAL_VERSION,
            writes: vec![JournalWrite {
                path: "DEV/link.yml".to_string(),
                original: None,
                new: base64::engine::general_purpose::STANDARD.encode(b"new"),
            }],
        };
        fs::write(&journal_path, serde_json::to_string(&journal).unwrap()).unwrap();

        let err = MultiFileTransaction::begin(&root, &[]).unwrap_err();
        assert!(err.to_string().contains("symlink"));
        assert_eq!(fs::read_to_string(&real).unwrap(), "real");
        assert!(journal_path.exists());
    }

    #[test]
    fn journal_with_conflicting_external_edit_fails_closed() {
        let (_tmp, root) = workspace();
        let file = root.join("DEV").join("1.yml");
        fs::create_dir_all(root.join("DEV")).unwrap();
        fs::write(&file, "original").unwrap();
        let journal_path = root.join(JOURNAL_FILE_NAME);
        let journal = PendingJournal {
            version: JOURNAL_VERSION,
            writes: vec![JournalWrite {
                path: "DEV/1.yml".to_string(),
                original: Some(base64::engine::general_purpose::STANDARD.encode(b"original")),
                new: base64::engine::general_purpose::STANDARD.encode(b"new"),
            }],
        };
        fs::write(&journal_path, serde_json::to_string(&journal).unwrap()).unwrap();
        // An external editor changed the file after the crash.
        fs::write(&file, "external edit").unwrap();

        let err = MultiFileTransaction::begin(&root, &[]).unwrap_err();
        assert!(err.to_string().contains("no longer holds its original"));
        assert_eq!(fs::read_to_string(&file).unwrap(), "external edit");
        assert!(journal_path.exists());
    }

    #[test]
    fn staging_path_outside_root_or_twice_is_rejected() {
        let (_tmp, root) = workspace();
        let file = root.join("DEV").join("1.yml");
        fs::create_dir_all(root.join("DEV")).unwrap();
        let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        assert!(
            txn.stage(&root.parent().unwrap().join("outside.yml"), "x".into())
                .is_err()
        );
        txn.stage(&file, "first".to_string()).unwrap();
        assert!(txn.stage(&file, "second".to_string()).is_err());
    }

    #[test]
    fn dropped_uncommitted_transaction_writes_nothing() {
        let (_tmp, root) = workspace();
        let file = root.join("DEV").join("1.yml");
        fs::create_dir_all(root.join("DEV")).unwrap();
        fs::write(&file, "original").unwrap();
        let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        stage_all(&mut txn, &[(&file, "new")]);
        drop(txn);
        assert_eq!(fs::read_to_string(&file).unwrap(), "original");
        assert!(!root.join(JOURNAL_FILE_NAME).exists());
    }

    #[test]
    fn recovery_waits_for_the_journal_projects_task_lock() {
        let (_tmp, root) = workspace();
        let auth_task = root.join("AUTH").join("5.yml");
        fs::create_dir_all(root.join("AUTH")).unwrap();
        fs::write(&auth_task, "auth original").unwrap();
        let journal_path = root.join(JOURNAL_FILE_NAME);
        let journal = PendingJournal {
            version: JOURNAL_VERSION,
            writes: vec![JournalWrite {
                path: "AUTH/5.yml".to_string(),
                original: Some(base64::engine::general_purpose::STANDARD.encode(b"auth original")),
                new: base64::engine::general_purpose::STANDARD.encode(b"auth new"),
            }],
        };
        fs::write(&journal_path, serde_json::to_string(&journal).unwrap()).unwrap();
        fs::write(&auth_task, "auth new").unwrap();

        // An external writer holds the AUTH task lock, exactly like a
        // concurrent `lotar comment add` would. Recovery must not restore the
        // AUTH file without that lock: begin blocks on it and fails closed
        // after the storage-lock timeout instead of clobbering the writer.
        let held = File::options()
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("AUTH").join(".task.lock"))
            .unwrap();
        use fs2::FileExt;
        held.try_lock_exclusive().unwrap();

        let started = std::time::Instant::now();
        let err = MultiFileTransaction::begin(&root, &[]).unwrap_err();
        assert!(
            err.to_string().contains("busy") || err.to_string().contains("WouldBlock"),
            "expected lock timeout, got: {err}"
        );
        assert!(
            started.elapsed() >= Duration::from_secs(2),
            "begin must have waited on the AUTH task lock"
        );
        // The journaled file was left untouched while its lock was unavailable.
        assert_eq!(fs::read_to_string(&auth_task).unwrap(), "auth new");
        assert!(journal_path.exists());
        drop(held);

        // Once the lock is free, begin recovers normally.
        MultiFileTransaction::begin(&root, &[]).unwrap();
        assert_eq!(fs::read_to_string(&auth_task).unwrap(), "auth original");
        assert!(!journal_path.exists());
    }

    #[test]
    fn begin_unions_requested_and_journal_project_locks() {
        let (_tmp, root) = workspace();
        let dev_task = root.join("DEV").join("1.yml");
        let auth_task = root.join("AUTH").join("1.yml");
        for dir in ["DEV", "AUTH"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(&dev_task, "dev original").unwrap();
        fs::write(&auth_task, "auth original").unwrap();
        let journal_path = root.join(JOURNAL_FILE_NAME);
        let journal = PendingJournal {
            version: JOURNAL_VERSION,
            writes: vec![JournalWrite {
                path: "AUTH/1.yml".to_string(),
                original: Some(base64::engine::general_purpose::STANDARD.encode(b"auth original")),
                new: base64::engine::general_purpose::STANDARD.encode(b"auth new"),
            }],
        };
        fs::write(&journal_path, serde_json::to_string(&journal).unwrap()).unwrap();
        fs::write(&auth_task, "auth new").unwrap();

        // Requested DEV, journal says AUTH: both must be locked before the
        // staged write below can proceed and recovery must have restored AUTH.
        let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        assert_eq!(fs::read_to_string(&auth_task).unwrap(), "auth original");
        // Staging an unlocked project is refused even after a successful begin.
        let other = root.join("OTHER").join("2.yml");
        assert!(txn.stage(&other, "nope".to_string()).is_err());
        txn.stage(&dev_task, "dev new".to_string()).unwrap();
        txn.commit().unwrap();
        assert_eq!(fs::read_to_string(&dev_task).unwrap(), "dev new");
    }

    #[test]
    fn ensure_covers_fails_for_projects_without_a_held_lock() {
        let (_tmp, root) = workspace();
        let txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        assert!(txn.ensure_covers(&["DEV".to_string()]).is_ok());
        let err = txn
            .ensure_covers(&["DEV".to_string(), "AUTH".to_string()])
            .unwrap_err();
        assert!(err.to_string().contains("unlocked project 'AUTH'"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_symlinked_parent_components() {
        let (_tmp, root) = workspace();
        fs::create_dir_all(root.join("real")).unwrap();
        fs::create_dir_all(root.join("DEV")).unwrap();
        std::os::unix::fs::symlink("../real", root.join("DEV").join("linked")).unwrap();
        let target = root.join("DEV").join("linked").join("1.yml");
        let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        let err = txn.stage(&target, "x".to_string()).unwrap_err();
        assert!(err.to_string().contains("symlink"), "{err}");
    }

    #[test]
    fn staging_rejects_the_journal_itself() {
        let (_tmp, root) = workspace();
        let mut txn = MultiFileTransaction::begin(&root, &[]).unwrap();
        let err = txn
            .stage(&root.join(JOURNAL_FILE_NAME), "x".to_string())
            .unwrap_err();
        assert!(err.to_string().contains("journal"), "{err}");
    }

    #[test]
    fn parent_sync_failure_rolls_back_and_removes_the_journal() {
        let (_tmp, root) = workspace();
        let first = root.join("@sprints").join("1.yml");
        let second = root.join("@sprints").join("2.yml");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::write(&first, "first original").unwrap();
        fs::write(&second, "second original").unwrap();

        fault::fail_parent_sync_once();
        let mut txn = MultiFileTransaction::begin(&root, &[]).unwrap();
        stage_all(&mut txn, &[(&first, "first new"), (&second, "second new")]);
        let err = txn.commit().unwrap_err();
        assert!(
            err.to_string()
                .contains("injected parent directory sync failure"),
            "{err}"
        );
        assert_eq!(fs::read_to_string(&first).unwrap(), "first original");
        assert_eq!(fs::read_to_string(&second).unwrap(), "second original");
        assert!(!root.join(JOURNAL_FILE_NAME).exists());
    }

    #[test]
    fn journal_removal_failure_rolls_back_immediately() {
        let (_tmp, root) = workspace();
        let file = root.join("@sprints").join("1.yml");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "original").unwrap();

        // All writes published, but the journal unlink failed once. The error
        // must surface together with an already-restored workspace — not a
        // published-but-pending state waiting for the next recovery (R1) —
        // and the cleanup retry removes the journal.
        fault::fail_journal_removal_times(1);
        let mut txn = MultiFileTransaction::begin(&root, &[]).unwrap();
        stage_all(&mut txn, &[(&file, "new value")]);
        let err = txn.commit().unwrap_err();
        assert!(
            err.to_string().contains("injected journal removal failure"),
            "{err}"
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "original",
            "files must be unchanged the moment the error is returned"
        );
        assert!(!root.join(JOURNAL_FILE_NAME).exists());
    }

    #[test]
    fn persistent_journal_removal_failure_retains_a_valid_journal() {
        let (_tmp, root) = workspace();
        let file = root.join("@sprints").join("1.yml");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "original").unwrap();

        // Removal fails on both the publish path and the post-rollback retry:
        // files are still restored immediately and the retained journal
        // matches that state, so recovery is a verify-no-op plus a removal
        // retry rather than a partial-state repair.
        fault::fail_journal_removal_times(3);
        let mut txn = MultiFileTransaction::begin(&root, &[]).unwrap();
        stage_all(&mut txn, &[(&file, "new value")]);
        let err = txn.commit().unwrap_err();
        assert!(
            err.to_string().contains("injected journal removal failure"),
            "{err}"
        );
        assert!(
            err.to_string().contains("every affected file was restored"),
            "{err}"
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "original",
            "files must be unchanged the moment the error is returned"
        );
        assert!(root.join(JOURNAL_FILE_NAME).exists());

        // Third failure: even recovery cannot clean up yet, so begin fails
        // closed while leaving the (already restored) files untouched.
        let err = MultiFileTransaction::begin(&root, &[]).unwrap_err();
        assert!(
            err.to_string().contains("injected journal removal failure"),
            "{err}"
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "original");

        // Counter exhausted: the next begin verifies the no-op state, removes
        // the journal, and a fresh commit succeeds.
        MultiFileTransaction::begin(&root, &[]).unwrap();
        assert!(!root.join(JOURNAL_FILE_NAME).exists());
        let mut txn = MultiFileTransaction::begin(&root, &[]).unwrap();
        stage_all(&mut txn, &[(&file, "new value")]);
        txn.commit().unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new value");
    }

    #[test]
    fn journal_removal_fsync_failure_still_acknowledges_the_commit() {
        let (_tmp, root) = workspace();
        let file = root.join("@sprints").join("1.yml");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "original").unwrap();

        fault::fail_journal_removal_fsync_once();
        let mut txn = MultiFileTransaction::begin(&root, &[]).unwrap();
        stage_all(&mut txn, &[(&file, "acknowledged")]);
        txn.commit()
            .expect("successful journal removal rename acknowledges the commit");
        assert_eq!(fs::read_to_string(&file).unwrap(), "acknowledged");
        assert!(!root.join(JOURNAL_FILE_NAME).exists());
    }

    #[test]
    fn readers_never_observe_torn_files_during_repeated_commits() {
        let (_tmp, root) = workspace();
        let sprint = root.join("@sprints").join("1.yml");
        let task = root.join("DEV").join("1.yml");
        fs::create_dir_all(sprint.parent().unwrap()).unwrap();
        fs::create_dir_all(root.join("DEV")).unwrap();
        fs::write(&sprint, "tasks:\n- DEV-1\n").unwrap();
        fs::write(&task, "title: seed\n").unwrap();
        MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();

        let root_clone = root.clone();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                for round in 0..40u32 {
                    let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
                    txn.stage(&sprint, format!("tasks:\n- DEV-{round}\n"))
                        .unwrap();
                    txn.stage(&task, format!("title: round-{round}\n")).unwrap();
                    txn.commit().unwrap();
                }
            });
            for _ in 0..2 {
                let root = root_clone.clone();
                scope.spawn(move || {
                    for _ in 0..200u32 {
                        // Atomic renames mean readers always parse fully.
                        if let Ok(text) = fs::read_to_string(root.join("@sprints").join("1.yml"))
                            && serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&text).is_err()
                        {
                            panic!("torn sprint read: {text}");
                        }
                        if let Ok(text) = fs::read_to_string(root.join("DEV").join("1.yml"))
                            && serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&text).is_err()
                        {
                            panic!("torn task read: {text}");
                        }
                    }
                });
            }
        });
    }

    /// `begin` for the concurrency test only: retry the bounded 2s
    /// lock-contention timeout (io `WouldBlock`, "still busy after 2
    /// seconds") a few extra times. Under heavy IO/fsync load one worker's
    /// commit (multiple fsyncs) can legitimately outlast the other worker's
    /// contention window; that is coordination working as designed, not a
    /// lost serialization guarantee. Retrying only that error keeps the
    /// read-back-your-own-write assertion meaningful while removing
    /// wall-clock sensitivity; every other failure surfaces unchanged, and
    /// the dedicated lock-timeout test below keeps asserting the 2s bound.
    fn begin_retrying_contention(
        root: &Path,
        prefixes: &[String],
    ) -> LoTaRResult<MultiFileTransaction> {
        let mut busy_retries = 6u32;
        loop {
            match MultiFileTransaction::begin(root, prefixes) {
                Ok(txn) => return Ok(txn),
                Err(LoTaRError::IoError(io_err))
                    if io_err.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    if busy_retries == 0 {
                        return Err(LoTaRError::IoError(io_err));
                    }
                    busy_retries -= 1;
                }
                Err(err) => return Err(err),
            }
        }
    }

    #[test]
    fn concurrent_transactions_serialize_on_the_coordinated_locks() {
        let (_tmp, root) = workspace();
        let file = root.join("DEV").join("1.yml");
        fs::create_dir_all(root.join("DEV")).unwrap();
        fs::write(&file, "0").unwrap();

        // Pre-create the lock files so both workers can open them immediately.
        MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();

        let root_clone = root.clone();
        std::thread::scope(|scope| {
            for worker in 0..2u32 {
                let root = root_clone.clone();
                let file = file.clone();
                scope.spawn(move || {
                    for round in 0..25u32 {
                        let mut txn = begin_retrying_contention(&root, &["DEV".to_string()])
                            .expect("begin under contention");
                        let next = format!("{}-{}", worker, round);
                        txn.stage(&file, next.clone()).unwrap();
                        txn.commit().unwrap();
                        assert_eq!(fs::read_to_string(&file).unwrap(), next);
                    }
                });
            }
        });
    }

    #[test]
    fn midpublish_rollback_restores_exact_binary_original_bytes() {
        let (_tmp, root) = workspace();
        let file = root.join("DEV").join("bin.yml");
        fs::create_dir_all(root.join("DEV")).unwrap();
        let original: Vec<u8> = vec![0x74, 0x69, 0x74, 0x6c, 0x65, 0x3a, 0x20, 0xff, 0xfe, 0x0a];
        fs::write(&file, &original).unwrap();

        fault::fail_publish_at(0);
        let mut txn = MultiFileTransaction::begin(&root, &["DEV".to_string()]).unwrap();
        stage_all(&mut txn, &[(&file, "replacement text")]);
        txn.commit().unwrap_err();

        assert_eq!(fs::read(&file).unwrap(), original);
        assert!(!root.join(JOURNAL_FILE_NAME).exists());
    }
}
