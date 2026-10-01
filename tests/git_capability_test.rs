//! Runtime Git capability probe wrapper tests (DEV-79).
//!
//! The probe performs a real `git init` in an isolated directory, so these
//! tests drive it with owned fake executables where the platform allows
//! scripts and with cross-platform outcomes (missing binary, cached verdict,
//! fail-closed contract) everywhere else.

mod common;

#[test]
fn probe_reports_missing_git_binary() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("git");
    let probe = common::probe_git_with_binary(Some(&missing));
    assert!(!probe.available);
    assert!(
        probe.reason.contains("git"),
        "reason should mention git: {}",
        probe.reason
    );
}

#[test]
fn probe_verdict_is_deterministic_and_cached() {
    let cached = common::git_capability();
    let fresh = common::probe_git_with_binary(None);
    assert_eq!(common::git_available(), cached.available);
    assert_eq!(fresh.available, cached.available);
    if !cached.available {
        assert!(
            !fresh.reason.is_empty(),
            "unavailable verdicts carry a reason"
        );
    }
}

#[test]
fn require_git_fails_closed_when_the_probe_fails() {
    let probe = common::git_capability();
    if probe.available {
        common::require_git();
    } else {
        let panicked = std::panic::catch_unwind(common::require_git).is_err();
        assert!(
            panicked,
            "require_git must fail closed when the probe fails, reason: {}",
            probe.reason
        );
    }
}

#[cfg(unix)]
mod fake_git_probes {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn write_fake_git(script: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let git = dir.path().join("git");
        std::fs::write(&git, script).unwrap();
        let mut permissions = std::fs::metadata(&git).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&git, permissions).unwrap();
        (dir, git)
    }

    #[test]
    fn probe_rejects_git_that_exits_zero_without_artifact() {
        let (_dir, git) = write_fake_git("#!/bin/sh\nexit 0\n");
        let probe = super::common::probe_git_with_binary(Some(&git));
        assert!(!probe.available);
        assert!(
            probe.reason.contains(".git"),
            "reason should mention the missing artifact: {}",
            probe.reason
        );
    }

    #[test]
    fn probe_surfaces_git_failure_diagnostics() {
        let (_dir, git) = write_fake_git("#!/bin/sh\necho probe-boom >&2\nexit 3\n");
        let probe = super::common::probe_git_with_binary(Some(&git));
        assert!(!probe.available);
        assert!(
            probe.reason.contains("probe-boom"),
            "reason should surface stderr: {}",
            probe.reason
        );
    }

    /// The probe child must run with a fresh environment: routing variables
    /// set in the test process (as a wrapper harness would) must not reach
    /// the probe's git invocation. The fake records its received environment
    /// inside the owned probe root and satisfies the artifact contract.
    #[test]
    fn probe_child_environment_has_no_inherited_git_routing() {
        use super::common::env_mutex::EnvVarGuard;
        let outside = tempfile::tempdir().unwrap();
        let _guard_dir =
            EnvVarGuard::set("GIT_DIR", outside.path().join("escaped").to_str().unwrap());
        let _guard_lower = EnvVarGuard::set(
            "git_dir",
            outside.path().join("escaped-lower").to_str().unwrap(),
        );
        let _guard_worktree = EnvVarGuard::set("GIT_WORK_TREE", outside.path().to_str().unwrap());

        let (_dir, git) =
            write_fake_git("#!/bin/sh\nenv > probe-env-capture.txt\nmkdir -p repo/.git\nexit 0\n");
        let root = tempfile::tempdir().unwrap();
        let probe = super::common::probe_git_in(root.path(), Some(&git));
        // The availability verdict depends on `.git`-named creation being
        // permitted (denied in some sandboxes); the routing-isolation
        // assertions below hold in every environment.
        let _ = probe;

        let captured = std::fs::read_to_string(root.path().join("probe-env-capture.txt")).unwrap();
        for line in captured.lines() {
            let upper = line.to_ascii_uppercase();
            assert!(
                !(upper.starts_with("GIT_DIR=")
                    || upper.starts_with("GIT_WORK_TREE=")
                    || upper.starts_with("GIT_CONFIG_GLOBAL=/")
                        && !line.contains(root.path().to_str().unwrap())),
                "probe child inherited a routing variable: {line}"
            );
        }
        assert!(
            captured.contains("GIT_CONFIG_NOSYSTEM=1"),
            "probe child must disable system config"
        );
        let owned_prefix = root.path().to_str().unwrap();
        assert!(
            captured
                .lines()
                .any(|l| l.starts_with("HOME=") && l.contains(owned_prefix)),
            "probe child HOME must be the owned root"
        );
        assert!(
            captured.lines().any(|l| l.starts_with("PATH=")),
            "probe child keeps PATH to resolve the git binary"
        );
    }

    /// Write-integrity control: with the fix, an inherited absolute GIT_DIR
    /// never reaches the probe child, so nothing can create or reinitialize
    /// a repository outside the owned probe root. The fake writes a marker
    /// into $GIT_DIR iff it leaks through - the old inherited-env probe
    /// fails this test.
    #[test]
    fn inherited_absolute_git_dir_cannot_escape_the_owned_probe_root() {
        use super::common::env_mutex::EnvVarGuard;
        let outside = tempfile::tempdir().unwrap();
        let escape_target = outside.path().join("escaped-repo");
        std::fs::create_dir_all(&escape_target).unwrap();
        let _guard = EnvVarGuard::set("GIT_DIR", escape_target.to_str().unwrap());

        let (_dir, git) = write_fake_git(
            "#!/bin/sh\nif [ -n \"${GIT_DIR:-}\" ]; then touch \"$GIT_DIR/escaped-marker\" 2>/dev/null; fi\nmkdir -p repo/.git\nexit 0\n",
        );
        let root = tempfile::tempdir().unwrap();
        let probe = super::common::probe_git_in(root.path(), Some(&git));
        assert!(
            !escape_target.join("escaped-marker").exists(),
            "probe child saw GIT_DIR and wrote outside the owned root"
        );
        if probe.available {
            assert!(
                root.path().join("repo").join(".git").exists(),
                "the artifact must be created inside the owned root"
            );
        }
    }

    /// Real-git variant of the escape control: when real Git is available,
    /// an inherited absolute GIT_DIR must not redirect the actual `git init`.
    #[test]
    fn real_probe_ignores_inherited_git_dir_when_git_is_available() {
        use super::common::env_mutex::EnvVarGuard;
        if !super::common::git_capability().available {
            return; // environment without Git: covered by the fake controls above
        }
        let outside = tempfile::tempdir().unwrap();
        let escape_target = outside.path().join("escaped-repo");
        std::fs::create_dir_all(&escape_target).unwrap();
        let _guard = EnvVarGuard::set("GIT_DIR", escape_target.to_str().unwrap());

        let root = tempfile::tempdir().unwrap();
        let probe = super::common::probe_git_in(root.path(), None);
        assert!(
            probe.available,
            "real git must satisfy the probe: {probe:?}"
        );
        assert!(
            !escape_target.join(".git").exists(),
            "real git init wrote into the inherited GIT_DIR outside the owned root"
        );
        assert!(root.path().join("repo").join(".git").exists());
    }
}

/// The ctor baseline must leave no repository-routing variable in the test
/// process environment: raw `Command::new("git")` fixture helpers inherit it.
#[test]
fn process_baseline_has_no_repository_routing_env() {
    for (key, _) in std::env::vars() {
        assert!(
            !lotar::utils::git::is_repository_routing_key(&key),
            "routing variable {key} survived the test-environment baseline"
        );
    }
}

/// Shared routing-key inventory: case-insensitive routing keys and injected
/// `GIT_CONFIG*` count; transport auth, identity, and plain platform vars do
/// not (they must survive sanitization for Cargo and normal git use).
#[test]
fn routing_key_matcher_covers_case_and_config_variants() {
    for key in [
        "GIT_DIR",
        "git_dir",
        "Git_Work_Tree",
        "GIT_INDEX_FILE",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_GLOBAL",
        "git_config_system",
    ] {
        assert!(
            lotar::utils::git::is_repository_routing_key(key),
            "{key} must be a routing key"
        );
    }
    for key in [
        "GIT_SSH_COMMAND",
        "GIT_SSH_VARIANT",
        "GIT_ASKPASS",
        "GIT_AUTHOR_NAME",
        "GIT_COMMITTER_EMAIL",
        "GIT_EDITOR",
        "PATH",
        "HOME",
        "USERPROFILE",
    ] {
        assert!(
            !lotar::utils::git::is_repository_routing_key(key),
            "{key} must NOT be a routing key"
        );
    }
}
