use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use fs2::FileExt;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const SPRINT_LOCK_NAME: &str = "sprints";
const MAX_PROJECT_PREFIX_LEN: usize = 64;

static REPORTED_CORRUPT_FILES: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn is_valid_project_prefix(name: &str) -> bool {
    if name.is_empty() || name.len() > MAX_PROJECT_PREFIX_LEN {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_alphanumeric() => {}
        _ => return false,
    }
    name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

pub fn validate_project_prefix(name: &str) -> Result<(), String> {
    if is_valid_project_prefix(name) {
        Ok(())
    } else {
        Err(format!("Invalid project prefix '{name}': must be 1-64 alphanumeric characters (or '_'/'-'), without path separators"))
    }
}

pub fn warn_corrupt_once(path: &Path, reason: &str) {
    let should_report = REPORTED_CORRUPT_FILES.lock().map(|mut seen| seen.insert(path.to_path_buf())).unwrap_or(false);
    if should_report {
        OutputRenderer::new(OutputFormat::Text, LogLevel::Warn)
            .log_warn(format_args!("Skipping unreadable task file {} ({reason}); fix or remove it to restore the item", path.display()));
    }
}

/// Warn once per process that a task ID is stored in more than one file.
pub fn warn_duplicate_once(id: &str) {
    static REPORTED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
    let should_report = REPORTED.lock().map(|mut seen| seen.insert(id.to_string())).unwrap_or(false);
    if should_report {
        OutputRenderer::new(OutputFormat::Text, LogLevel::Warn).log_warn(format_args!(
            "Task {id} is stored in more than one file; listing one of them. `lotar show {id}` names the files; delete the extra copy or give it a new number with `lotar task renumber {id} --file <path>`. Changes to {id} are refused until then"
        ));
    }
}

pub fn sprint_lock_name() -> &'static str {
    SPRINT_LOCK_NAME
}

#[doc(hidden)]
pub fn is_lock_contention(error: &std::io::Error, native_code: Option<i32>) -> bool {
    error.kind() == std::io::ErrorKind::WouldBlock || native_code.is_some_and(|code| error.raw_os_error() == Some(code))
}

/// Overrides how long [`acquire_storage_lock`] waits for a busy lock
/// (milliseconds); used by tests.
pub const STORAGE_LOCK_WAIT_ENV: &str = "LOTAR_STORAGE_LOCK_WAIT_MS";
const DEFAULT_STORAGE_LOCK_WAIT: Duration = Duration::from_secs(2);

/// Acquire an exclusive advisory directory lock, retrying contention for up
/// to two seconds. The returned file owns the lock; dropping it releases the
/// lock on success, error, or panic. The lock file itself must not be removed.
/// Do not sweep atomic-write temp files here: other writers need not hold this
/// lock, so even an exclusive lock does not prove that their temp files are orphaned.
pub fn acquire_storage_lock(target_dir: &Path, lock_name: &str) -> std::io::Result<File> {
    acquire_storage_lock_within(target_dir, lock_name, crate::utils::time::env_millis(STORAGE_LOCK_WAIT_ENV, DEFAULT_STORAGE_LOCK_WAIT))
}

/// [`acquire_storage_lock`] with its own wait for a busy lock.
pub fn acquire_storage_lock_within(target_dir: &Path, lock_name: &str, wait: Duration) -> std::io::Result<File> {
    let lock_path = target_dir.join(format!(".{lock_name}.lock"));
    // Task, sprint and finding locks live one level below the tasks root (a
    // project folder, `@sprints`); give the root its runtime .gitignore (DEV-271).
    if matches!(lock_name, "task" | "finding" | SPRINT_LOCK_NAME)
        && let Some(root) = target_dir.parent()
        && root.join("config.yml").is_file()
    {
        crate::storage::manager::ensure_runtime_gitignore(root);
    }
    let file = File::options().write(true).create(true).truncate(false).open(&lock_path).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("Cannot open storage lock {}: {e}; check the directory and permissions; mutation was not run", lock_path.display()),
        )
    })?;
    let start = Instant::now();
    let contention_code = fs2::lock_contended_error().raw_os_error();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(e) if is_lock_contention(&e, contention_code) => {
                if start.elapsed() >= wait {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WouldBlock,
                        format!(
                            "Storage lock {} is still busy after {} seconds; retry after the other writer finishes; mutation was not run",
                            lock_path.display(),
                            wait.as_secs()
                        ),
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(std::io::Error::new(
                    e.kind(),
                    format!("Cannot acquire storage lock {}: {e}; check filesystem locking support and permissions; mutation was not run", lock_path.display()),
                ));
            }
        }
    }
}

/// Run a mutation only after acquiring an exclusive advisory directory lock.
/// Contention is retried for up to two seconds. Dropping the file releases the
/// lock on success, error, or panic; the lock file itself must not be removed.
/// Do not sweep atomic-write temp files here: other writers need not hold this
/// lock, so even an exclusive lock does not prove that their temp files are orphaned.
pub fn with_storage_lock<T, E: From<std::io::Error>>(target_dir: &Path, lock_name: &str, f: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    let _file = acquire_storage_lock(target_dir, lock_name)?;
    refuse_under_pending_transaction(target_dir)?;
    f()
}

/// Fail closed when a pending multi-file transaction journal exists for the
/// workspace this directory belongs to (DEV-55 review L3). Checked after the
/// advisory lock is acquired, never before, so a journal cannot appear or be
/// recovered around the check: journals are only created by writers that hold
/// this same lock, and recovery runs while holding it. Refusing here keeps
/// non-participating writers (task comments, direct sprint edits, scan
/// creation) from mutating files a pending rollback would need to restore.
/// Lock-less config writes remain outside this guard and are documented as
/// such.
fn refuse_under_pending_transaction(target_dir: &Path) -> std::io::Result<()> {
    let Some(root) = target_dir.parent() else {
        return Ok(());
    };
    let journal = root.join(crate::storage::transaction::JOURNAL_FILE_NAME);
    if journal.is_file() {
        return Err(std::io::Error::other(PendingJournalRefusal(format!(
            "A pending task/sprint transaction journal exists at {}; a crashed multi-file mutation is waiting to be rolled back. Run any task create/update, sprint assignment, or sprint cleanup operation to trigger recovery, then retry this change",
            journal.display()
        ))));
    }
    Ok(())
}

/// Typed marker wrapping the pending-transaction refusal message (DEV-58):
/// REST error classification recognizes this client-correctable
/// coordination state by downcasting the `io::Error` payload instead of
/// matching on message text. Display is exactly the refusal message, so
/// existing diagnostics are unchanged.
#[derive(Debug)]
pub(crate) struct PendingJournalRefusal(pub(crate) String);

impl std::fmt::Display for PendingJournalRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PendingJournalRefusal {}

/// Sync the new file before atomically publishing it. Callers requiring durable
/// directory entries must also sync the parent directory after this succeeds.
/// Existing destination permissions are applied before writing any temp contents.
pub fn atomic_write_file(path: &Path, contents: &str) -> std::io::Result<()> {
    atomic_write_file_with_io(
        path,
        |file| {
            file.write_all(contents.as_bytes())?;
            file.sync_all()
        },
        |temp, target| fs::rename(temp, target),
    )
}

/// Byte-oriented variant of [`atomic_write_file`] for rollback payloads that
/// must reproduce exact original bytes (including non-UTF-8 content).
pub(crate) fn atomic_write_bytes(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    atomic_write_file_with_io(
        path,
        |file| {
            file.write_all(contents)?;
            file.sync_all()
        },
        |temp, target| fs::rename(temp, target),
    )
}

// Keep failure injection local so tests never depend on global hooks or privileges.
// Public only for tests/storage_safety_test.rs.
#[doc(hidden)]
pub fn atomic_write_file_with_io(
    path: &Path,
    prepare: impl FnOnce(&mut File) -> std::io::Result<()>,
    publish: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("target path has no parent directory"))?;
    let file_name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| std::io::Error::other("target path has no file name"))?;
    let permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let unique = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = dir.join(format!(".{}.tmp-{}-{}", file_name, std::process::id(), unique));
    // Exclusive creation means failure cleanup can only remove our own temp.
    let mut options = File::options();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if let Some(permissions) = &permissions {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        // Avoid even an empty temp being opened with broader access before chmod.
        options.mode(permissions.mode());
    }
    let mut file = options.open(&tmp_path)?;
    let prepared = permissions.map_or(Ok(()), |permissions| file.set_permissions(permissions)).and_then(|()| prepare(&mut file));
    drop(file);
    let result = prepared.and_then(|()| publish(&tmp_path, path));
    if result.is_err() {
        // Never unlink the destination or bypass its permissions to retry.
        let _ = fs::remove_file(&tmp_path);
    }
    result
}
