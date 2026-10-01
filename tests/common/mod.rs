use assert_cmd::Command;
use assert_cmd::cargo::{CargoError, cargo_bin_cmd};
use ctor::{ctor, dtor};
use lotar::Storage;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Re-export shared environment mutex for tests that mutate global env vars.
pub mod env_mutex;

// Minimal OpenAPI schema validator for semantic contract tests (DEV-82).
// Compiled into every test binary via `mod common`; only contract tests use it.
#[allow(dead_code)]
pub mod openapi_semantics;

/// Test fixtures to create isolated work directories per test.
pub struct TestFixtures {
    pub temp_dir: TempDir,
    pub tasks_root: PathBuf,
}

/// Build a `lotar` CLI child command with the test defaults applied
/// child-scoped (DEV-80): the child always runs with
/// `LOTAR_IGNORE_HOME_CONFIG=1` and without `RUST_TEST_THREADS`, independent
/// of the test process environment, which stays owned by guards and the
/// process constructor baseline.
#[allow(dead_code)]
pub fn lotar_cmd() -> Result<Command, CargoError> {
    let mut cmd = cargo_bin_cmd!("lotar");
    cmd.env_remove("RUST_TEST_THREADS");
    cmd.env("LOTAR_IGNORE_HOME_CONFIG", "1");
    Ok(cmd)
}

#[ctor]
unsafe fn init_lotar_test_environment() {
    reset_lotar_test_environment();
    scrub_repository_routing_env();
    isolate_tmpdir_under_owned_scratch();
}

/// Remove inherited Git repository-routing variables once per test process.
///
/// Raw fixture helpers that shell out to `Command::new("git")` inherit the
/// launching environment; an absolute `GIT_DIR`/`GIT_WORK_TREE`/injected
/// `GIT_CONFIG*` inherited from a wrapper process could direct fixture git
/// operations outside the test's owned temporary root before any assertion
/// runs. The ctor scrub gives every test process (npm runner and raw nextest
/// alike) a routing-clean baseline while preserving transport auth, SSH,
/// identity, and `PATH`. The key inventory is
/// [`lotar::utils::git::is_repository_routing_key`] — the same policy the
/// production `git_command` factory enforces per child. Case-supplied
/// `EnvVarGuard` scopes in negative tests still run AFTER this scrub and
/// keep working exactly as before.
pub fn scrub_repository_routing_env() {
    let offenders: Vec<String> = std::env::vars()
        .map(|(key, _)| key)
        .filter(|key| lotar::utils::git::is_repository_routing_key(key))
        .collect();
    for key in offenders {
        let _guard = env_mutex::lock_var(&key);
        // Manipulating process-wide env vars requires `unsafe`. Keep scope
        // tiny; the ctor runs before any test thread exists.
        unsafe {
            std::env::remove_var(&key);
        }
    }
}

/// Process-owned scratch directory that TMPDIR is redirected to (DEV-90).
static OWNED_SCRATCH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

const SCRATCH_BASE_NAME: &str = ".lotar-test-scratch";

/// Keep test fixtures from being siblings of unrelated workspaces (DEV-90).
///
/// `StorageLocator::candidate_task_roots` scans the visible sibling
/// directories of a workspace for extra `.tasks` roots (monorepo discovery).
/// Fixtures created directly under the shared session temp dir therefore sit
/// next to every leftover scratch workspace: a sibling with its own `.tasks`
/// and a colliding project prefix leaks phantom tasks into search results
/// (six deterministic sync-suite failures during DEV-55 verification).
///
/// Redirect TMPDIR to a per-process scratch named `<pid>-<nanos>` nested
/// under a dot-prefixed base — dot directories are invisible to the sibling
/// scan. Under nextest every test runs in its own process, so fixtures of
/// different tests stop being siblings of each other and of unrelated
/// leftovers entirely; multiple fixtures inside one test remain siblings,
/// exactly as before.
///
/// Cleanup: the `#[dtor]` removes this process's (normally empty) scratch on
/// a regular exit, and each new process sweeps base entries whose owning pid
/// is gone, because nextest kills finished test processes before their
/// destructors run. Leaked scratches are invisible to sibling scans either
/// way.
fn isolate_tmpdir_under_owned_scratch() {
    if OWNED_SCRATCH.get().is_some() {
        return;
    }
    let base = std::env::temp_dir().join(SCRATCH_BASE_NAME);
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
    // Manipulating process-wide env vars requires `unsafe`. Keep scope tiny;
    // the ctor runs before any test thread exists.
    unsafe {
        std::env::set_var("TMPDIR", &scratch);
    }
    let _ = OWNED_SCRATCH.set(scratch);
}

/// Remove scratch directories whose creating process no longer exists. A
/// scratch is owned by exactly one short-lived test process; a live owner (or
/// a recycled pid) only makes the sweep skip an entry, never removes a dir
/// still in use. Unix-only, like the fs2 locking these tests exercise.
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
        // Signal 0 probes existence only; ESRCH means the owner is gone.
        let dead = unsafe { libc::kill(owner as i32, 0) == -1 }
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        if dead {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(not(unix))]
fn sweep_scratches_of_dead_owners(_base: &std::path::Path) {}

#[dtor]
unsafe fn cleanup_owned_scratch() {
    // Best effort: normally empty because every fixture temp dir removes its
    // own tree on drop. Failures leave only an invisible dot-nested dir that
    // a later process sweeps once this pid is gone.
    if let Some(scratch) = OWNED_SCRATCH.get() {
        let _ = std::fs::remove_dir(scratch);
    }
}

/// Remove shared LOTAR environment variables and ignore home config for deterministic tests.
///
/// DEV-80 ownership contract:
/// - The process constructor calls this once before any test thread exists;
///   it is the only environment writer besides `EnvVarGuard` scopes.
/// - Test fixtures (`TestFixtures::new`, [`temp_dir`]) never mutate the
///   process environment; mid-test scopes must use `EnvVarGuard` so previous
///   values are restored.
/// - Each variable's mutex is held only around the mutation itself: a
///   concurrent guard holder is serialized with this reset, never silently
///   clobbered by it. The per-variable mutexes are not reentrant, so callers
///   must not already hold an `EnvVarGuard`/`lock_var` for any listed
///   variable.
pub fn reset_lotar_test_environment() {
    for var in [
        "LOTAR_TASKS_DIR",
        "LOTAR_HOME",
        "LOTAR_TEST_SILENT",
        "LOTAR_IGNORE_ENV_TASKS_DIR",
    ] {
        let _guard = env_mutex::lock_var(var);
        // Manipulating process-wide env vars requires `unsafe`. Keep scope tiny.
        unsafe {
            std::env::remove_var(var);
        }
    }
    let _guard = env_mutex::lock_var("LOTAR_IGNORE_HOME_CONFIG");
    unsafe {
        std::env::set_var("LOTAR_IGNORE_HOME_CONFIG", "1");
    }
}

/// Create an isolated temp directory without mutating the process
/// environment (DEV-80): live `EnvVarGuard` values survive this call.
#[allow(dead_code)]
pub fn temp_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp directory")
}

impl TestFixtures {
    /// Create isolated workspace fixtures.
    ///
    /// DEV-80: fixture construction owns no environment state. Values owned
    /// by live `EnvVarGuard`s survive fixture creation, and the deterministic
    /// baseline (cleared LOTAR vars, `LOTAR_IGNORE_HOME_CONFIG=1`) is owned
    /// by the process constructor plus the child-scoped defaults in
    /// [`lotar_cmd`].
    #[allow(dead_code)]
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let tasks_root = temp_dir.path().join(".tasks");
        fs::create_dir_all(&tasks_root).expect("Failed to create tasks directory");

        Self {
            temp_dir,
            tasks_root,
        }
    }

    #[allow(dead_code)] // Used across multiple test modules
    pub fn create_storage(&self) -> Storage {
        Storage::new(&self.tasks_root.clone())
    }

    #[allow(dead_code)] // Used across multiple test modules
    pub fn get_temp_path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    /// Run a lotar command with the given arguments and return the output
    #[allow(dead_code)] // Used by output format consistency tests
    pub fn run_command(&self, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        // Create a basic Cargo.toml if it doesn't exist for project detection
        let cargo_toml_path = self.temp_dir.path().join("Cargo.toml");
        if !cargo_toml_path.exists() {
            std::fs::write(
                &cargo_toml_path,
                "[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2021\"",
            )?;

            // Create src directory and main.rs for a valid Rust project
            let src_dir = self.temp_dir.path().join("src");
            std::fs::create_dir_all(&src_dir)?;
            std::fs::write(
                src_dir.join("main.rs"),
                "fn main() { println!(\"Hello, world!\"); }",
            )?;
        }

        let mut cmd = lotar_cmd().map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
        let output = cmd
            .env("LOTAR_TEST_SILENT", "1")
            .args(args)
            .current_dir(self.temp_dir.path())
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(format!(
                "Command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into())
        }
    }

    /// Create a config file in the specified directory
    #[allow(dead_code)]
    pub fn create_config_in_dir(&self, dir: &std::path::Path, content: &str) {
        let config_path = dir.join("config.yml");
        std::fs::write(&config_path, content).expect("Failed to create config file");
    }
}

/// Extract a task identifier from CLI output (text or JSON).
#[allow(dead_code)]
pub fn extract_task_id_from_output(output: &str) -> Option<String> {
    // JSON payloads may nest the task ID within "task" or expose "task_id" directly.
    if let Ok(json) = serde_json::from_str::<Value>(output) {
        if let Some(id) = json
            .get("task")
            .and_then(|task| task.get("id").and_then(Value::as_str))
        {
            return Some(id.to_string());
        }
        if let Some(id) = json.get("task_id").and_then(Value::as_str) {
            return Some(id.to_string());
        }
    }

    // Prefer explicit creation messages when present.
    for line in output.lines() {
        if let Some(id) = line
            .split("Created task: ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
        {
            return Some(id.to_string());
        }
    }

    // Fallback heuristic: find the first token resembling PREFIX-123.
    output
        .split_whitespace()
        .find(|token| token.contains('-') && token.chars().any(|c| c.is_ascii_digit()))
        .map(|token| token.trim_end_matches(':').to_string())
}

/// Convenience wrapper to parse IDs from binary stdout captures.
#[allow(dead_code)]
pub fn extract_task_id_from_bytes(output: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(output);
    extract_task_id_from_output(&text)
}

/// Command helpers shared across CLI tests
#[allow(dead_code)]
pub fn cargo_bin_silent() -> Command {
    // Spawn lotar with LOTAR_TEST_SILENT=1 to suppress non-essential warnings in tests
    let mut cmd = lotar_cmd().expect("binary 'lotar' not found");
    cmd.env("LOTAR_TEST_SILENT", "1");
    cmd
}

/// Spawn the CLI with LOTAR_TEST_SILENT and cwd set to the fixture dir.
#[allow(dead_code)]
pub fn cargo_bin_in(fixtures: &TestFixtures) -> Command {
    let mut cmd = cargo_bin_silent();
    cmd.current_dir(fixtures.get_temp_path());
    cmd
}

// Note: Test functions for common utilities have been removed to prevent duplication
// across all test files that import this module. Each test file should test its own functionality.

/// Result of the runtime Git capability probe.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitProbe {
    pub available: bool,
    pub reason: String,
}

/// Whether git-dependent tests can run in this environment.
///
/// Some sandboxed runtimes forbid creating anything named `.git` outside the
/// workspace, which makes every test that runs `git init` fail with
/// "Operation not permitted". Git-dependent tests are always compiled
/// (DEV-79); they live in source-local `git_required` modules that the
/// gitless nextest profile excludes when this probe fails, and guarded tests
/// call `require_git()` to fail closed if selected anyway. This runtime probe
/// performs what the tests actually need — a real `git init` inside an
/// isolated temporary directory — and verifies the `.git` artifact exists
/// afterwards.
///
/// Environment-variable overrides are deliberately not provided; the
/// designated-runner environment contract is tracked separately (DEV-80).
#[allow(dead_code)]
pub fn git_available() -> bool {
    git_capability().available
}

/// Cached outcome of the runtime Git capability probe.
#[allow(dead_code)]
pub fn git_capability() -> &'static GitProbe {
    static CAPABILITY: std::sync::OnceLock<GitProbe> = std::sync::OnceLock::new();
    CAPABILITY.get_or_init(|| probe_git_with_binary(None))
}

/// Fail closed for tests that require Git.
///
/// Guarded tests call this instead of returning silently when Git is
/// unavailable: a selected test that cannot exercise its Git-dependent path
/// must fail with a diagnostic rather than report a pass (DEV-79).
#[allow(dead_code)]
#[track_caller]
pub fn require_git() {
    let probe = git_capability();
    let reason = if probe.reason.is_empty() {
        "unknown reason"
    } else {
        &probe.reason
    };
    assert!(
        probe.available,
        "this test requires Git, but the runtime capability probe failed: {reason}. Failing closed instead of passing silently (DEV-79)"
    );
}

/// Probe Git with the environment-resolved `git` binary in a fresh temp dir.
#[allow(dead_code)]
pub fn probe_git_with_binary(git_binary: Option<&std::path::Path>) -> GitProbe {
    match tempfile::tempdir() {
        Ok(tmp) => probe_git_in(tmp.path(), git_binary),
        Err(err) => GitProbe {
            available: false,
            reason: format!("cannot create probe directory: {err}"),
        },
    }
}

/// Pass through the Windows bootstrap variables a child process requires
/// (`SystemRoot` et al.) without leaking any parent Git configuration. HOME
/// bases (`HOME`/`USERPROFILE`/`XDG_CONFIG_HOME`) are always overridden with
/// the owned root by the caller.
#[cfg(windows)]
fn apply_windows_bootstrap_env(command: &mut std::process::Command, owned_root: &std::path::Path) {
    const BOOTSTRAP_KEYS: &[&str] = &["SystemRoot", "windir", "TEMP", "TMP", "COMSPEC"];
    for key in BOOTSTRAP_KEYS {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.env("USERPROFILE", owned_root);
}

#[cfg(not(windows))]
fn apply_windows_bootstrap_env(
    _command: &mut std::process::Command,
    _owned_root: &std::path::Path,
) {
}

/// Run the real Git capability probe inside `probe_root`.
///
/// `git_binary` is `None` to resolve `git` from the environment (what the
/// cached capability uses) or a path to a controlled executable, which lets
/// the probe's own tests drive failure and missing-binary outcomes
/// deterministically on any platform.
#[allow(dead_code)]
pub fn probe_git_in(
    probe_root: &std::path::Path,
    git_binary: Option<&std::path::Path>,
) -> GitProbe {
    let repo = probe_root.join("repo");
    if let Err(err) = fs::create_dir(&repo) {
        return GitProbe {
            available: false,
            reason: format!("cannot prepare probe repository directory: {err}"),
        };
    }
    let gitconfig = probe_root.join("gitconfig");
    if let Err(err) = fs::write(&gitconfig, "") {
        return GitProbe {
            available: false,
            reason: format!("cannot prepare probe gitconfig: {err}"),
        };
    }
    let program = git_binary.unwrap_or_else(|| std::path::Path::new("git"));
    let mut command = std::process::Command::new(program);
    command
        .arg("init")
        .arg("--quiet")
        .arg(&repo)
        // Fresh child environment (DEV-79 review): an inherited absolute
        // GIT_DIR/GIT_WORK_TREE could direct `git init` outside this owned
        // probe root BEFORE the artifact check runs. Only PATH (to resolve
        // the binary) and owned HOME/config variables survive; Windows adds
        // the process bootstrap variables it requires.
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", probe_root)
        .env("USERPROFILE", probe_root)
        .env("XDG_CONFIG_HOME", probe_root.join(".config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", &gitconfig)
        .current_dir(probe_root)
        .stdin(std::process::Stdio::null());
    apply_windows_bootstrap_env(&mut command, probe_root);
    let output = command.output();
    match output {
        Ok(output) if output.status.success() => {
            if repo.join(".git").exists() {
                GitProbe {
                    available: true,
                    reason: String::new(),
                }
            } else {
                GitProbe {
                    available: false,
                    reason: format!(
                        "`git init` exited successfully but left no .git artifact in {}",
                        repo.display()
                    ),
                }
            }
        }
        Ok(output) => GitProbe {
            available: false,
            reason: format!(
                "`git init` failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        },
        Err(err) => GitProbe {
            available: false,
            reason: format!("cannot run git: {err}"),
        },
    }
}
