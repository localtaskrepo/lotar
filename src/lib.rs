// Note: clippy allows for `uninlined_format_args` and `collapsible_if` were
// previously added to suppress new lints when upgrading the stable toolchain.
// We'll remove them temporarily in CI so we can evaluate whether the warnings
// still occur and address them individually. If necessary we can re-add a more
// targeted allow later.
#![warn(clippy::needless_pass_by_value)]

pub mod api_events;
pub mod api_server;
pub mod api_types;
pub mod automation;
pub mod cli;
pub mod config;
pub mod errors;
pub mod help;
pub mod mcp;
pub mod output;
pub mod project;
pub mod routes;
pub mod scanner;
pub mod services;
pub mod storage;
pub mod types;
pub mod utils;
pub mod web_server;
pub mod workspace;

pub use errors::{LoTaRError, LoTaRResult};
pub use storage::{TaskFilter, manager::Storage, task::Task};
pub use types::TaskStatus;
pub use workspace::TasksDirectoryResolver;

/// Test-process environment for the lib's own unit tests (DEV-90). The
/// integration-test `common` module performs the same redirect for its
/// binaries; keep the two in sync. Never compiled into non-test builds.
#[cfg(test)]
mod test_environment {
    use std::sync::OnceLock;

    static OWNED_SCRATCH: OnceLock<std::path::PathBuf> = OnceLock::new();

    /// `StorageLocator::candidate_task_roots` scans visible sibling
    /// directories of a workspace for extra `.tasks` roots, so fixtures
    /// created directly under the shared session temp dir inherit every
    /// leftover scratch workspace there. Redirect TMPDIR to a per-process
    /// scratch named `<pid>-<nanos>` under a dot-prefixed (scanner-invisible)
    /// base so lib-test fixtures are never siblings of unrelated workspaces;
    /// nextest's process-per-test model isolates individual tests as well.
    /// Cleanup mirrors `tests/common`: destructors on regular exit, plus
    /// dead-owner sweeping because nextest kills finished test processes
    /// before their destructors run.
    #[ctor::ctor]
    unsafe fn isolate_tmpdir_under_owned_scratch() {
        if OWNED_SCRATCH.get().is_some() {
            return;
        }
        let base = std::env::temp_dir().join(".lotar-test-scratch");
        sweep_scratches_of_dead_owners(&base);
        let scratch = base.join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        if std::fs::create_dir_all(&scratch).is_err() {
            return; // keep the shared temp dir behavior on unusable filesystems
        }
        // Manipulating process-wide env vars requires `unsafe`. Keep scope
        // tiny; the ctor runs before any test thread exists.
        unsafe {
            std::env::set_var("TMPDIR", &scratch);
        }
        let _ = OWNED_SCRATCH.set(scratch);
    }

    /// Remove scratch directories whose creating process no longer exists;
    /// see the equivalent helper in `tests/common/mod.rs`.
    #[cfg(unix)]
    fn sweep_scratches_of_dead_owners(base: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(base) else {
            return;
        };
        let own_pid = std::process::id();
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let Some(owner) = name
                .split('-')
                .next()
                .and_then(|pid| pid.parse::<u32>().ok())
            else {
                continue;
            };
            if owner == own_pid {
                continue;
            }
            let dead = unsafe { libc::kill(owner as i32, 0) == -1 }
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
            if dead {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }

    #[cfg(not(unix))]
    fn sweep_scratches_of_dead_owners(_base: &std::path::Path) {}
}
