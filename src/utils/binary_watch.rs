//! Noticing that the `lotar` binary was replaced (an upgrade or reinstall),
//! for long-running processes that exit so whatever started them runs the
//! new version: `lotar mcp` by default, `lotar serve --exit-on-upgrade`
//! (DEV-305).
//!
//! The watched file is the path lotar was started from, such as the
//! `/opt/homebrew/bin/lotar` symlink, not the resolved executable. Homebrew
//! installs every version under its own Cellar path and usually deletes the
//! old one, so the resolved file never changes; the symlink moves to the
//! new version.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Overrides the poll interval (milliseconds); used by tests.
pub const INTERVAL_ENV: &str = "LOTAR_BINARY_WATCH_INTERVAL_MS";
const DEFAULT_INTERVAL: Duration = Duration::from_secs(2);

/// The file a path leads to, with its size and modification time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryIdentity {
    target: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

impl BinaryIdentity {
    /// `None` when the path leads to no regular file (for example while an
    /// upgrade replaces it, or after an uninstall).
    pub fn of(path: &Path) -> Option<Self> {
        let target = std::fs::canonicalize(path).ok()?;
        let meta = std::fs::metadata(&target).ok()?;
        if !meta.is_file() {
            return None;
        }
        Some(Self {
            target,
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

/// The path this process was started as, when it leads to the running
/// binary: `argv[0]` if it names a path, else its match on `PATH`. Falls
/// back to the running executable.
pub fn watched_path() -> Option<PathBuf> {
    let running = std::env::current_exe().ok()?;
    let running_target = std::fs::canonicalize(&running).ok();
    let launched = std::env::args_os().next().and_then(|argv0| {
        let path = PathBuf::from(&argv0);
        if path.components().count() > 1 {
            return std::path::absolute(&path).ok();
        }
        let search = std::env::var_os("PATH")?;
        std::env::split_paths(&search).find_map(|dir| {
            let candidate = dir.join(&path);
            if candidate.is_file() {
                return Some(candidate);
            }
            let exe = candidate.with_extension(std::env::consts::EXE_EXTENSION);
            (!std::env::consts::EXE_EXTENSION.is_empty() && exe.is_file()).then_some(exe)
        })
    });
    match launched {
        Some(path)
            if running_target.is_some() && std::fs::canonicalize(&path).ok() == running_target =>
        {
            Some(path)
        }
        _ => Some(running),
    }
}

fn interval() -> Duration {
    std::env::var(INTERVAL_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_INTERVAL)
}

/// Run `on_change` once, on a background thread, after the binary at
/// [`watched_path`] changes: another file, size or modification time. The
/// new binary must stay the same for one more interval first, so a file
/// still being written is not run. A missing path is not a change. Returns
/// false when there is nothing to watch.
pub fn spawn_watcher(on_change: impl FnOnce() + Send + 'static) -> bool {
    let Some(path) = watched_path() else {
        return false;
    };
    spawn_watcher_for(path, interval(), on_change)
}

/// The one upgrade reaction for long-running commands (`lotar mcp`, `lotar
/// serve --exit-on-upgrade`): once the binary is replaced, stop running
/// agent jobs and exit with status 0, so whatever started this process runs
/// the new version. The note goes to stderr; stdout may carry a protocol.
/// Returns false when there is nothing to watch.
pub fn exit_when_replaced() -> bool {
    spawn_watcher(|| {
        eprintln!("The lotar binary changed; exiting so the new version can start");
        crate::services::agent_job_service::AgentJobService::shutdown_jobs(Duration::from_secs(10));
        std::process::exit(0);
    })
}

/// [`spawn_watcher`] for a given path and interval.
pub fn spawn_watcher_for(
    path: PathBuf,
    interval: Duration,
    on_change: impl FnOnce() + Send + 'static,
) -> bool {
    let Some(initial) = BinaryIdentity::of(&path) else {
        return false;
    };
    std::thread::spawn(move || {
        let mut candidate: Option<BinaryIdentity> = None;
        loop {
            std::thread::sleep(interval);
            match BinaryIdentity::of(&path) {
                Some(now) if now != initial => {
                    if candidate.as_ref() == Some(&now) {
                        on_change();
                        return;
                    }
                    candidate = Some(now);
                }
                _ => candidate = None,
            }
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const FAST: Duration = Duration::from_millis(20);

    fn watch(path: &Path) -> mpsc::Receiver<()> {
        let (tx, rx) = mpsc::channel();
        assert!(spawn_watcher_for(path.to_path_buf(), FAST, move || {
            let _ = tx.send(());
        }));
        rx
    }

    #[test]
    fn replacing_the_file_is_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("lotar");
        std::fs::write(&bin, b"old").unwrap();
        let fired = watch(&bin);
        std::thread::sleep(FAST * 3);
        assert!(fired.try_recv().is_err());
        let next = dir.path().join("lotar.new");
        std::fs::write(&next, b"new binary").unwrap();
        std::fs::rename(&next, &bin).unwrap();
        assert!(fired.recv_timeout(Duration::from_secs(5)).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_retargeted_symlink_is_a_change_and_a_missing_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("0.1.0");
        let new = dir.path().join("0.2.0");
        std::fs::write(&old, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        let link = dir.path().join("lotar");
        std::os::unix::fs::symlink(&old, &link).unwrap();
        let fired = watch(&link);

        // Mid-upgrade: the link is gone for a while, then points at the new
        // version. Only the second state is a change.
        std::fs::remove_file(&link).unwrap();
        std::thread::sleep(FAST * 5);
        assert!(fired.try_recv().is_err());
        std::os::unix::fs::symlink(&new, &link).unwrap();
        assert!(fired.recv_timeout(Duration::from_secs(5)).is_ok());
    }

    #[test]
    fn nothing_to_watch_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!spawn_watcher_for(dir.path().join("missing"), FAST, || {}));
    }
}
