//! Test-process environment for the unit tests of lotar-core and lotar-lib
//! (DEV-90), included by both via #[path]. The integration-test `common`
//! module (crates/lotar-lib/tests/common) performs the same redirect for its
//! binaries; keep the two in sync.

use std::sync::OnceLock;

static OWNED_SCRATCH: OnceLock<std::path::PathBuf> = OnceLock::new();

/// `StorageLocator::candidate_task_roots` scans visible sibling
/// directories of a workspace for extra `.tasks` roots, so fixtures
/// created directly under the shared session temp dir inherit every
/// leftover scratch workspace there. Redirect TMPDIR to a per-process
/// scratch named `<pid>-<nanos>` under a dot-prefixed (scanner-invisible)
/// base so lib-test fixtures are never siblings of unrelated workspaces;
/// nextest's process-per-test model isolates individual tests as well.
/// Cleanup mirrors `tests/common`: the destructor removes the scratch
/// and the empty base on regular exit, plus dead-owner sweeping for
/// processes killed before their destructors ran.
#[ctor::ctor(unsafe)]
unsafe fn isolate_tmpdir_under_owned_scratch() {
    scrub_repository_routing_env();
    scrub_config_override_env();
    // Ignore the developer's ~/.lotar (DEV-339), as `tests/common` does.
    // Manipulating process-wide env vars requires `unsafe`. Keep scope
    // tiny; the ctor runs before any test thread exists.
    unsafe {
        std::env::set_var("LOTAR_IGNORE_HOME_CONFIG", "1");
    }
    if OWNED_SCRATCH.get().is_some() {
        return;
    }
    let base = std::env::temp_dir().join(".lotar-test-scratch");
    sweep_scratches_of_dead_owners(&base);
    let scratch =
        base.join(format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0),));
    // Retry: a finishing process removes the base once it is empty,
    // which can happen between creating the base and the scratch.
    if !(0..3).any(|_| std::fs::create_dir_all(&scratch).is_ok()) {
        return; // keep the shared temp dir behavior on unusable filesystems
    }
    // Manipulating process-wide env vars requires `unsafe`. Keep scope
    // tiny; the ctor runs before any test thread exists.
    unsafe {
        std::env::set_var("TMPDIR", &scratch);
    }
    let _ = OWNED_SCRATCH.set(scratch);
}

#[dtor::dtor(unsafe)]
unsafe fn cleanup_owned_scratch() {
    if let Some(scratch) = OWNED_SCRATCH.get() {
        let _ = std::fs::remove_dir(scratch);
        if let Some(base) = scratch.parent() {
            let _ = std::fs::remove_dir(base); // only succeeds once no other process uses it
        }
    }
}

/// Remove inherited Git repository-routing variables once per lib-test
/// process (same baseline as `tests/common`; see that module for the
/// rationale). Shares the routing-key inventory with the production
/// `git_command` factory via `is_repository_routing_key`.
fn scrub_repository_routing_env() {
    let offenders: Vec<String> = std::env::vars().map(|(key, _)| key).filter(|key| crate::utils::git::is_repository_routing_key(key)).collect();
    for key in offenders {
        // Manipulating process-wide env vars requires `unsafe`. Keep
        // scope tiny; the ctor runs before any test thread exists.
        unsafe {
            std::env::remove_var(&key);
        }
    }
}

/// Remove inherited config overrides such as `LOTAR_DEFAULT_REPORTER`
/// (DEV-336): they beat the fixtures' own config. Same baseline as
/// `reset_lotar_test_environment` in `tests/common`.
fn scrub_config_override_env() {
    for var in crate::config::env_overrides::ENV_OVERRIDE_DEFINITIONS.iter().flat_map(|definition| definition.env_vars) {
        // Manipulating process-wide env vars requires `unsafe`. Keep
        // scope tiny; the ctor runs before any test thread exists.
        unsafe {
            std::env::remove_var(var);
        }
    }
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
        let Some(owner) = name.split('-').next().and_then(|pid| pid.parse::<u32>().ok()) else {
            continue;
        };
        if owner == own_pid {
            continue;
        }
        let dead = unsafe { libc::kill(owner as i32, 0) == -1 } && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        if dead {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(not(unix))]
fn sweep_scratches_of_dead_owners(_base: &std::path::Path) {}
