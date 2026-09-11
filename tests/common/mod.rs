use assert_cmd::Command;
use assert_cmd::cargo::{CargoError, cargo_bin_cmd};
use ctor::{ctor, dtor};
use lotar::types::Priority;
use lotar::{Storage, Task};
use serde_json::Value;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

// Re-export shared environment mutex for tests that mutate global env vars.
pub mod env_mutex;

/// Test fixtures to create isolated work directories per test.
pub struct TestFixtures {
    pub temp_dir: TempDir,
    pub tasks_root: PathBuf,
}

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
    isolate_tmpdir_under_owned_scratch();
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
pub fn reset_lotar_test_environment() {
    // Manipulating process-wide env vars requires `unsafe`. Keep scope tiny.
    unsafe {
        std::env::remove_var("LOTAR_TASKS_DIR");
        std::env::remove_var("LOTAR_HOME");
        std::env::remove_var("LOTAR_TEST_SILENT");
        std::env::remove_var("LOTAR_IGNORE_ENV_TASKS_DIR");
        std::env::set_var("LOTAR_IGNORE_HOME_CONFIG", "1");
    }
}

#[allow(dead_code)]
pub fn temp_dir() -> TempDir {
    reset_lotar_test_environment();
    TempDir::new().expect("Failed to create temp directory")
}

impl TestFixtures {
    #[allow(dead_code)]
    pub fn new() -> Self {
        reset_lotar_test_environment();

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let tasks_root = temp_dir.path().join(".tasks");
        fs::create_dir_all(&tasks_root).expect("Failed to create tasks directory");

        Self {
            temp_dir,
            tasks_root,
        }
    }

    // Clean up after test
    #[allow(dead_code)]
    pub fn cleanup(&self) {
        // Temporary directory is automatically cleaned up when TestFixtures is dropped
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

    /// Create test files with TODO comments for scanner testing
    #[allow(dead_code)] // Used by scanner tests
    pub fn create_test_source_files(&self) -> Vec<String> {
        let mut files = Vec::new();

        // Create Rust file with TODO containing UUID
        let rust_file_path = self.temp_dir.path().join("test.rs");
        let mut rust_file = File::create(&rust_file_path).unwrap();
        writeln!(rust_file, "fn main() {{").unwrap();
        writeln!(rust_file, "    // TODO (uuid-1234): Test Rust with UUID").unwrap();
        writeln!(rust_file, "    // TODO: Implement main functionality").unwrap();
        writeln!(rust_file, "}}").unwrap();
        files.push(rust_file_path.to_string_lossy().to_string());

        // Create JavaScript file with TODO
        let js_file_path = self.temp_dir.path().join("test.js");
        let mut js_file = File::create(&js_file_path).unwrap();
        writeln!(js_file, "function test() {{").unwrap();
        writeln!(js_file, "    // TODO: Test JavaScript").unwrap();
        writeln!(js_file, "}}").unwrap();
        files.push(js_file_path.to_string_lossy().to_string());

        // Create Python file with TODO
        let py_file_path = self.temp_dir.path().join("test.py");
        let mut py_file = File::create(&py_file_path).unwrap();
        writeln!(py_file, "def test():").unwrap();
        writeln!(py_file, "    # TODO: Test Python").unwrap();
        writeln!(py_file, "    pass").unwrap();
        files.push(py_file_path.to_string_lossy().to_string());

        files
    }

    /// Create a sample task for testing
    #[allow(dead_code)]
    pub fn create_sample_task(&self, _project: &str) -> Task {
        Task::new(
            self.tasks_root.clone(),
            "Sample Test Task".to_string(),
            Priority::from("Medium"),
        )
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

/// Test utility functions
pub mod utils {
    /// Extract project prefix from task ID (e.g., "PROJ-123" -> "PROJ")
    #[allow(dead_code)] // Used across multiple test modules
    pub fn get_project_for_task(task_id: &str) -> Option<String> {
        task_id.split('-').next().map(|s| s.to_string())
    }
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

/// Assertion helpers for testing
pub mod assertions {
    use std::path::Path;

    #[allow(dead_code)] // Used in storage_crud_test.rs
    pub fn assert_task_exists(tasks_root: &Path, project: &str, _task_id: &str) {
        // Look for .yml files since we changed the extension
        let task_files = std::fs::read_dir(tasks_root.join(project))
            .expect("Project directory should exist")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yml"))
            .collect::<Vec<_>>();

        assert!(
            !task_files.is_empty(),
            "Should have at least one task file in project {project}"
        );
    }

    #[allow(dead_code)] // Used in storage_crud_test.rs
    pub fn assert_metadata_updated(
        tasks_root: &Path,
        project: &str,
        task_count: u64,
        current_id: u64,
    ) {
        // Removed metadata file existence check since we've eliminated metadata.yml files
        // With the new filesystem-based approach, we verify the data by counting files and finding max ID
        let project_path = tasks_root.join(project);

        // Count actual task files in the directory (exclude config.yml)
        let actual_task_count = if let Ok(entries) = std::fs::read_dir(&project_path) {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    let path = entry.path();
                    let file_name = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("");
                    path.is_file()
                        && path.extension().is_some_and(|ext| ext == "yml")
                        && file_name != "config.yml" // Exclude config files from task count
                })
                .count() as u64
        } else {
            0
        };

        // Find the highest numbered file to verify current_id
        let actual_current_id = if let Ok(entries) = std::fs::read_dir(&project_path) {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let file_name = entry.file_name();
                    let name_str = file_name.to_string_lossy();
                    if name_str.ends_with(".yml") {
                        name_str.strip_suffix(".yml")?.parse::<u64>().ok()
                    } else {
                        None
                    }
                })
                .max()
                .unwrap_or(0)
        } else {
            0
        };

        assert_eq!(
            actual_task_count, task_count,
            "Task count mismatch for project {project}"
        );
        assert_eq!(
            actual_current_id, current_id,
            "Current ID mismatch for project {project}"
        );
    }
}

// Note: Test functions for common utilities have been removed to prevent duplication
// across all test files that import this module. Each test file should test its own functionality.

/// Whether git-dependent tests can run in this environment.
///
/// Some sandboxed runtimes forbid creating anything named `.git` anywhere
/// except the workspace, which makes every test that runs `git init` fail
/// with "Operation not permitted". The build-time `no_git_tests` cfg compiles
/// those tests out when detected at build time, but cargo redirects `TMPDIR`
/// for build scripts, which can fool that probe. This runtime probe checks
/// what the tests actually do — creating a `.git` directory inside a real
/// tempfile — and lets each gated test skip instead of failing.
#[allow(dead_code)]
pub fn git_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let Ok(tmp) = tempfile::tempdir() else {
            return false;
        };
        let probe = tmp.path().join(".git");
        match fs::create_dir(&probe) {
            Ok(()) => {
                let _ = fs::remove_dir(&probe);
                true
            }
            Err(_) => false,
        }
    })
}
