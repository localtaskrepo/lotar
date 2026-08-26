use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use fs2::FileExt;
use std::collections::HashSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

const SPRINT_LOCK_NAME: &str = "sprints";
const MAX_PROJECT_PREFIX_LEN: usize = 64;
const TEMP_FILE_MARKER: &str = ".yml.tmp-";

static REPORTED_CORRUPT_FILES: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
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
    name.chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

pub fn validate_project_prefix(name: &str) -> Result<(), String> {
    if is_valid_project_prefix(name) {
        Ok(())
    } else {
        Err(format!(
            "Invalid project prefix '{name}': must be 1-64 alphanumeric characters (or '_'/'-'), without path separators"
        ))
    }
}

pub fn warn_corrupt_once(path: &Path, reason: &str) {
    let should_report = REPORTED_CORRUPT_FILES
        .lock()
        .map(|mut seen| seen.insert(path.to_path_buf()))
        .unwrap_or(false);
    if should_report {
        OutputRenderer::new(OutputFormat::Text, LogLevel::Warn).log_warn(format_args!(
            "Skipping unreadable task file {} ({reason}); fix or remove it to restore the item",
            path.display()
        ));
    }
}

pub fn sprint_lock_name() -> &'static str {
    SPRINT_LOCK_NAME
}

/// Run `f` while holding an exclusive advisory lock on `target_dir`.
///
/// The lock is a dot-file (`<target_dir>/.lock`) so it never appears in project
/// listings or YAML scans. If the directory or lock file cannot be prepared,
/// `f` runs without cross-process exclusion.
/// Remove orphaned atomic-write temp files from `target_dir`.
///
/// Only call while holding the directory's exclusive lock: every legitimate
/// writer creates its temp file under that same lock, so anything matching
/// the temp pattern observed here belongs to a process that died mid-write.
fn sweep_orphan_temp_files(target_dir: &Path) {
    let Ok(entries) = fs::read_dir(target_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_temp = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.starts_with('.') && name.contains(TEMP_FILE_MARKER));
        if is_temp {
            let _ = fs::remove_file(&path);
        }
    }
}

pub fn with_storage_lock<T>(target_dir: &Path, lock_name: &str, f: impl FnOnce() -> T) -> T {
    if !target_dir.is_dir() {
        return f();
    }
    let lock_path = target_dir.join(format!(".{lock_name}.lock"));
    let handle = File::create(&lock_path);
    match handle {
        Ok(file) => match file.try_lock_exclusive() {
            Ok(_) => {
                sweep_orphan_temp_files(target_dir);
                let result = f();
                let _ = file.unlock();
                result
            }
            Err(_) => {
                OutputRenderer::new(OutputFormat::Text, LogLevel::Warn).log_warn(format_args!(
                    "Storage lock {} busy; proceeding without cross-process locking",
                    lock_path.display()
                ));
                f()
            }
        },
        Err(e) => {
            OutputRenderer::new(OutputFormat::Text, LogLevel::Warn).log_warn(format_args!(
                "Could not open storage lock file {}: {e}; proceeding without cross-process locking",
                lock_path.display()
            ));
            f()
        }
    }
}

pub fn atomic_write_file(path: &Path, contents: &str) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("target path has no parent directory"))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| std::io::Error::other("target path has no file name"))?;
    let unique = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = dir.join(format!(
        ".{}.tmp-{}-{}",
        file_name,
        std::process::id(),
        unique
    ));
    fs::write(&tmp_path, contents)?;
    match fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(rename_err) => {
            #[cfg(windows)]
            {
                if path.exists() {
                    let removed = fs::remove_file(path);
                    if removed.is_ok() {
                        if fs::rename(&tmp_path, path).is_ok() {
                            return Ok(());
                        }
                    }
                }
            }
            let _ = fs::remove_file(&tmp_path);
            Err(rename_err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_realistic_project_prefixes() {
        for ok in ["DEV", "DEMO", "AUTH", "TP", "My_Project-2", "ÜBER"] {
            assert!(is_valid_project_prefix(ok), "expected valid: {ok}");
        }
    }

    #[test]
    fn rejects_traversal_and_reserved_prefixes() {
        for bad in [
            "..",
            ".",
            "../evil",
            "../../etc",
            "/tmp/abs",
            "/abs",
            "a/b",
            "a\\b",
            "a b",
            "@sprints",
            ".hidden",
            "-flag",
            "",
            "a..b",
        ] {
            assert!(!is_valid_project_prefix(bad), "expected invalid: {bad}");
        }
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp_files() {
        let dir = std::env::temp_dir().join(format!("lotar-safety-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("5.yml");
        atomic_write_file(&target, "first").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "first");
        atomic_write_file(&target, "second").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "second");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.contains(".tmp-"))
                    .unwrap_or(false)
            })
            .collect();
        assert!(leftovers.is_empty(), "temp files left behind");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn storage_lock_runs_closure_and_creates_lock_file() {
        let dir = std::env::temp_dir().join(format!("lotar-lock-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let result = with_storage_lock(&dir, "task", || 41 + 1);
        assert_eq!(result, 42);
        assert!(dir.join(".task.lock").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lock_acquisition_sweeps_orphaned_temp_files_only() {
        let dir = std::env::temp_dir().join(format!("lotar-sweep-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".7.yml.tmp-999-0"), "orphaned partial write").unwrap();
        fs::write(dir.join("7.yml"), "complete task").unwrap();
        fs::write(dir.join(".gitkeep"), "").unwrap();

        let ran = with_storage_lock(&dir, "task", || true);
        assert!(ran);

        assert!(!dir.join(".7.yml.tmp-999-0").exists(), "orphan swept");
        assert!(dir.join("7.yml").exists(), "real task untouched");
        assert!(dir.join(".gitkeep").exists(), "unrelated dotfile untouched");
        let _ = fs::remove_dir_all(&dir);
    }
}
