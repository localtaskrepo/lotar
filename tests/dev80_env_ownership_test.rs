//! DEV-80 Wave 2: environment ownership regressions for Rust test fixtures.
//!
//! Contract under test:
//! - Fixture construction (`TestFixtures::new`, `common::temp_dir`) never
//!   mutates the process environment, so values owned by a live
//!   `EnvVarGuard` survive fixture creation and guards restore the previous
//!   value on drop.
//! - Per-variable guard mutexes are exclusive: a second guard for the same
//!   variable cannot run concurrently. Nesting guards for the same variable
//!   on one thread is therefore forbidden (the mutex is not reentrant);
//!   guards for distinct variables nest freely and restore in LIFO order.
//! - Child commands inherit the guarded process environment, and the test
//!   defaults (`LOTAR_IGNORE_HOME_CONFIG`, `LOTAR_TEST_SILENT`) are applied
//!   child-scoped by `common::lotar_cmd`/`run_command` instead of through
//!   process-global mutation.

mod common;

use crate::common::TestFixtures;
use crate::common::env_mutex::EnvVarGuard;
use std::time::Duration;

/// Variables the process constructor baseline owns; fixtures must not touch.
const BASELINE_VARS: [&str; 5] = [
    "LOTAR_TASKS_DIR",
    "LOTAR_HOME",
    "LOTAR_TEST_SILENT",
    "LOTAR_IGNORE_ENV_TASKS_DIR",
    "LOTAR_IGNORE_HOME_CONFIG",
];

fn var_is(var: &str, expected: &str) -> bool {
    std::env::var_os(var).is_some_and(|value| value == expected)
}

#[test]
fn fixture_construction_preserves_guard_owned_values() {
    let scratch = tempfile::TempDir::new().expect("scratch temp dir");
    let owned_dir = scratch.path().join("owned_tasks");
    std::fs::create_dir_all(&owned_dir).expect("create owned tasks dir");
    let owned = owned_dir.to_string_lossy().to_string();

    let guard = EnvVarGuard::set("LOTAR_TASKS_DIR", &owned);
    assert!(
        var_is("LOTAR_TASKS_DIR", &owned),
        "guard must own the value before fixture construction"
    );

    let _fixtures = TestFixtures::new();
    assert!(
        var_is("LOTAR_TASKS_DIR", &owned),
        "TestFixtures::new must not clobber a live guard's value (DEV-80)"
    );

    let _second = crate::common::temp_dir();
    assert!(
        var_is("LOTAR_TASKS_DIR", &owned),
        "common::temp_dir must not clobber a live guard's value (DEV-80)"
    );

    drop(guard);
    assert_eq!(
        std::env::var_os("LOTAR_TASKS_DIR"),
        None,
        "guard must restore the previous (absent) value on drop"
    );
}

#[test]
fn fixture_construction_leaves_process_environment_untouched() {
    let before: Vec<(&str, Option<std::ffi::OsString>)> = BASELINE_VARS
        .iter()
        .map(|v| (*v, std::env::var_os(v)))
        .collect();

    let _fixtures = TestFixtures::new();
    let _extra = crate::common::temp_dir();

    for (var, before) in &before {
        assert_eq!(
            std::env::var_os(var),
            before.clone(),
            "fixture construction must not alter {var}"
        );
    }
}

#[test]
fn fixture_construction_preserves_guard_cleared_values() {
    // The process constructor baseline sets LOTAR_IGNORE_HOME_CONFIG=1; a
    // live clear guard must survive fixture construction (the old fixtures
    // re-set this variable unconditionally).
    assert_eq!(
        std::env::var("LOTAR_IGNORE_HOME_CONFIG").as_deref(),
        Ok("1"),
        "process constructor must own the deterministic baseline"
    );
    let _cleared = EnvVarGuard::clear("LOTAR_IGNORE_HOME_CONFIG");
    assert_eq!(std::env::var_os("LOTAR_IGNORE_HOME_CONFIG"), None);

    let _fixtures = TestFixtures::new();
    assert_eq!(
        std::env::var_os("LOTAR_IGNORE_HOME_CONFIG"),
        None,
        "TestFixtures::new must not re-set a variable cleared by a live guard (DEV-80)"
    );
}

#[test]
fn env_var_guards_restore_previous_values() {
    // Existing previous value, owned by the process constructor baseline.
    assert_eq!(
        std::env::var("LOTAR_IGNORE_HOME_CONFIG").as_deref(),
        Ok("1")
    );
    {
        let _cleared = EnvVarGuard::clear("LOTAR_IGNORE_HOME_CONFIG");
        assert_eq!(std::env::var_os("LOTAR_IGNORE_HOME_CONFIG"), None);
    }
    assert_eq!(
        std::env::var("LOTAR_IGNORE_HOME_CONFIG").as_deref(),
        Ok("1"),
        "clear guard must restore the previous value on drop"
    );
    {
        let _overridden = EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "0");
        assert!(var_is("LOTAR_IGNORE_HOME_CONFIG", "0"));
    }
    assert_eq!(
        std::env::var("LOTAR_IGNORE_HOME_CONFIG").as_deref(),
        Ok("1"),
        "set guard must restore the previous value on drop"
    );

    // Absent previous value.
    {
        let _set = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "1");
        assert!(var_is("LOTAR_TEST_FAST_IO", "1"));
    }
    assert_eq!(
        std::env::var_os("LOTAR_TEST_FAST_IO"),
        None,
        "guard must remove a variable that was absent before"
    );
}

#[test]
fn nested_guards_on_distinct_variables_restore_in_lifo_order() {
    // Distinct variables must nest freely (same-variable nesting on one
    // thread is forbidden: the per-variable mutex is not reentrant).
    let outer = EnvVarGuard::set("LOTAR_TEST_FAST_IO", "outer-value");
    let inner = EnvVarGuard::clear("LOTAR_IGNORE_HOME_CONFIG");
    assert!(var_is("LOTAR_TEST_FAST_IO", "outer-value"));
    assert_eq!(std::env::var_os("LOTAR_IGNORE_HOME_CONFIG"), None);

    drop(inner);
    assert!(
        var_is("LOTAR_TEST_FAST_IO", "outer-value"),
        "outer guard must be unaffected by the inner guard's restore"
    );
    assert_eq!(
        std::env::var("LOTAR_IGNORE_HOME_CONFIG").as_deref(),
        Ok("1"),
        "inner guard must restore its previous value first"
    );

    drop(outer);
    assert_eq!(
        std::env::var_os("LOTAR_TEST_FAST_IO"),
        None,
        "outer guard must restore its previous value last"
    );
}

#[test]
fn same_variable_guards_are_exclusive_across_threads() {
    let outer = EnvVarGuard::set("LOTAR_TASKS_DIR", "outer-owned-value");
    let (acquired_tx, acquired_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        let inner = EnvVarGuard::set("LOTAR_TASKS_DIR", "inner-owned-value");
        acquired_tx.send(()).expect("notify acquisition");
        release_rx.recv().expect("receive release");
        drop(inner);
    });

    assert!(
        acquired_rx
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "a second guard for the same variable must not acquire while the outer guard holds the per-variable lock"
    );
    drop(outer);
    acquired_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("inner guard acquires after the outer guard drops");
    assert!(var_is("LOTAR_TASKS_DIR", "inner-owned-value"));
    release_tx.send(()).expect("release worker");
    worker.join().expect("worker must not panic");
    assert_eq!(
        std::env::var_os("LOTAR_TASKS_DIR"),
        None,
        "inner guard restores the post-outer (absent) value on drop"
    );
}

#[test]
fn child_commands_reflect_guard_scoped_environment() {
    let fixtures = TestFixtures::new();
    let env_tasks_dir = fixtures.temp_dir.path().join("dev80_env_tasks");
    std::fs::create_dir_all(&env_tasks_dir).expect("create env tasks dir");

    {
        let _guard = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            env_tasks_dir.to_str().expect("utf-8 path"),
        );
        let output = fixtures
            .run_command(&["config", "show"])
            .expect("config show under guard");
        assert!(
            output
                .lines()
                .any(|line| line.contains("Tasks directory:") && line.contains("dev80_env_tasks")),
            "child command must inherit the guard-owned value: {output}"
        );
    }

    let output = fixtures
        .run_command(&["config", "show"])
        .expect("config show after guard drop");
    assert!(
        output
            .lines()
            .any(|line| line.contains("Tasks directory:") && !line.contains("dev80_env_tasks")),
        "child command must see the restored default after the guard drops: {output}"
    );
}

#[test]
fn child_commands_apply_explicit_child_scoped_defaults() {
    let fixtures = TestFixtures::new();

    // run_command children receive LOTAR_IGNORE_HOME_CONFIG=1 from
    // common::lotar_cmd, not from the process environment: clearing the
    // process-level value must not change child output. (Local detection is
    // weak when no real home config exists; the pin is that output never
    // depends on the process-level variable.)
    let cleared_output = {
        let _cleared = EnvVarGuard::clear("LOTAR_IGNORE_HOME_CONFIG");
        fixtures
            .run_command(&["config", "show"])
            .expect("config show with cleared process default")
    };
    let baseline_output = fixtures
        .run_command(&["config", "show"])
        .expect("config show at baseline");
    assert_eq!(
        cleared_output, baseline_output,
        "child output must not depend on the process-level LOTAR_IGNORE_HOME_CONFIG (DEV-80 child-scoped defaults)"
    );
}
