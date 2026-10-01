//! Explicit default-port pin persistence (round 4).
//!
//! An explicit `server.port: 8080` is a strict pin (no fallback). These
//! tests prove the pin survives config mutations, that only an explicit
//! port request ever writes the key, and that labels credit the file tier
//! truthfully. All work happens in isolated child workspaces; no ports are
//! bound.

mod common;

use common::TestFixtures;
use std::process::Command;

fn run_cli(cwd: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_lotar"))
        .current_dir(cwd)
        .env_remove("LOTAR_TASKS_DIR")
        .env_remove("LOTAR_PORT")
        .env_remove("LOTAR_SERVER_PORT")
        .env("LOTAR_IGNORE_HOME_CONFIG", "1")
        .env("LOTAR_TEST_SILENT", "1")
        .args(args)
        .output()
        .expect("run lotar");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn global_config(fixtures: &TestFixtures) -> String {
    std::fs::read_to_string(fixtures.tasks_root.join("config.yml")).unwrap_or_default()
}

#[test]
fn explicit_default_port_pin_survives_unrelated_config_set() {
    let fixtures = TestFixtures::new();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "server:\n  port: 8080\ndefault:\n  tags: [alpha]\n",
    )
    .unwrap();

    let (ok, _, stderr) = run_cli(
        fixtures.temp_dir.path(),
        &["config", "set", "--global", "default.tags", "beta"],
    );
    assert!(ok, "unrelated set must succeed: {stderr}");

    let after = global_config(&fixtures);
    assert!(
        after.contains("port: 8080"),
        "explicit default-port pin must survive an unrelated mutation: {after}"
    );
    assert!(
        after.contains("tags") && after.contains("beta"),
        "the unrelated change must still apply: {after}"
    );
}

#[test]
fn explicit_config_set_of_the_default_port_writes_the_key() {
    let fixtures = TestFixtures::new();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  tags: [alpha]\n",
    )
    .unwrap();

    let (ok, _, stderr) = run_cli(
        fixtures.temp_dir.path(),
        &["config", "set", "server.port", "8080"],
    );
    assert!(ok, "explicit default-port set must succeed: {stderr}");

    let after = global_config(&fixtures);
    assert!(
        after.contains("port: 8080"),
        "config set server.port 8080 must persist the pin: {after}"
    );
}

#[test]
fn unrelated_set_on_implicit_config_never_introduces_the_port_key() {
    let fixtures = TestFixtures::new();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  tags: [alpha]\n",
    )
    .unwrap();

    let (ok, _, stderr) = run_cli(
        fixtures.temp_dir.path(),
        &["config", "set", "--global", "default.tags", "beta"],
    );
    assert!(ok, "unrelated set must succeed: {stderr}");

    let after = global_config(&fixtures);
    assert!(
        !after.contains("port"),
        "implicit configs must not gain a port key (fallback stays eligible): {after}"
    );
}

#[test]
fn config_override_does_not_persist_the_port() {
    let fixtures = TestFixtures::new();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  tags: [alpha]\n",
    )
    .unwrap();

    let (ok, _, stderr) = run_cli(
        fixtures.temp_dir.path(),
        &[
            "--config",
            "server.port=8080",
            "config",
            "set",
            "--global",
            "default.tags",
            "beta",
        ],
    );
    assert!(ok, "set with a --config override must succeed: {stderr}");

    let after = global_config(&fixtures);
    assert!(
        !after.contains("port"),
        "--config server.port is invocation-scoped and must never persist: {after}"
    );
}

#[test]
fn dry_run_leaves_config_bytes_untouched() {
    let fixtures = TestFixtures::new();
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "server:\n  port: 8080\ndefault:\n  tags: [alpha]\n",
    )
    .unwrap();
    let before = global_config(&fixtures);

    let (ok, _, stderr) = run_cli(
        fixtures.temp_dir.path(),
        &[
            "config",
            "set",
            "--global",
            "default.tags",
            "beta",
            "--dry-run",
        ],
    );
    assert!(ok, "dry run must succeed: {stderr}");

    assert_eq!(
        global_config(&fixtures),
        before,
        "dry-run must preserve the file byte-for-byte, pin included"
    );
}

#[test]
fn config_show_labels_explicit_default_port_as_global_not_default() {
    let fixtures = TestFixtures::new();

    // Explicit pin: credited to the file tier (truthful — the port is
    // configured and serve will treat it strictly).
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "server:\n  port: 8080\n",
    )
    .unwrap();
    let (ok, stdout, stderr) = run_cli(
        fixtures.temp_dir.path(),
        &["config", "show", "--format", "json"],
    );
    assert!(ok, "config show must succeed: {stderr}");
    let payload: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");
    assert_eq!(payload["config"]["server_port"], 8080);
    assert_eq!(
        payload["sources"]["server.port"], "global",
        "explicit file pin must be credited to global: {payload}"
    );

    // Absent: no source credited (default is filtered), value is default.
    std::fs::write(
        fixtures.tasks_root.join("config.yml"),
        "default:\n  tags: [a]\n",
    )
    .unwrap();
    let (ok, stdout, stderr) = run_cli(
        fixtures.temp_dir.path(),
        &["config", "show", "--format", "json"],
    );
    assert!(ok, "config show must succeed: {stderr}");
    let payload: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");
    assert_eq!(payload["config"]["server_port"], 8080);
    assert!(
        payload["sources"].get("server.port").is_none(),
        "absent port must not claim a source: {payload}"
    );
}

#[test]
fn serializer_emits_port_only_with_explicit_intent_or_non_default_value() {
    use lotar::config::normalization::{GlobalYamlOptions, to_canonical_global_yaml_with};

    fn cfg_with_port(port: u16) -> lotar::config::GlobalConfig {
        lotar::config::GlobalConfig {
            server_port: port,
            ..lotar::config::GlobalConfig::default()
        }
    }

    let implicit = to_canonical_global_yaml_with(
        &cfg_with_port(8080),
        GlobalYamlOptions {
            explicit_server_port: false,
        },
    );
    assert!(
        !implicit.contains("port"),
        "default stays implicit: {implicit}"
    );

    let pinned = to_canonical_global_yaml_with(
        &cfg_with_port(8080),
        GlobalYamlOptions {
            explicit_server_port: true,
        },
    );
    assert!(
        pinned.contains("port: 8080"),
        "explicit intent emits the pin: {pinned}"
    );

    let non_default = to_canonical_global_yaml_with(
        &cfg_with_port(9000),
        GlobalYamlOptions {
            explicit_server_port: false,
        },
    );
    assert!(
        non_default.contains("port: 9000"),
        "non-default values always emit: {non_default}"
    );

    // The rendered pin parses back to the pinned value (presence detection
    // itself is covered by the in-crate persistence unit tests and the
    // real-binary regressions above).
    let parsed = lotar::config::normalization::parse_global_from_yaml_str(&pinned).unwrap();
    assert_eq!(parsed.server_port, 8080);
}
