//! DEV-102 CLI blackbox no-op regressions: real `lotar` child processes in
//! isolated temporary workspaces prove that effective no-ops through the
//! CLI entrypoints preserve the task YAML bytes, filesystem mtime,
//! `modified`, and history, fire no automation hooks, and never escalate
//! into refusals that only genuine changes deserve — while genuine
//! changes, invalid patches, active-job ownership, and pending-journal
//! strictness keep behaving exactly as before.
//!
//! Owned scope: CLI entrypoints only. Sprint/config no-op CLI matrices are
//! owned by their DEV-102 workers (tests/dev102_sprint_noop_test.rs,
//! tests/dev102_config_noop_test.rs); the shared service boundary itself is
//! covered by tests/task_noop_update_test.rs.

mod common;

use common::TestFixtures;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::SystemTime;

const MARKER_UPDATED_HOOK: &str = "updated-hook";
const MARKER_COMMENTED_HOOK: &str = "commented-hook";

struct NoopCliEnv {
    fixtures: TestFixtures,
    _log_path: Option<PathBuf>,
}

impl NoopCliEnv {
    fn new(extra_config: &str) -> Self {
        let fixtures = TestFixtures::new();
        let content = format!(
            "default.project: NOP\ndefault.reporter: devoneoh\nissue.states: [Todo, InProgress, Done]\nissue.types: [Feature, Bug, Chore]\nissue.priorities: [Low, Medium, High]\nmembers: [devoneoh, alice, bob]\ncustom.fields: [product]\n{extra_config}"
        );
        std::fs::write(
            lotar::utils::paths::global_config_path(&fixtures.tasks_root),
            content,
        )
        .expect("write global config");
        Self {
            fixtures,
            _log_path: None,
        }
    }

    fn with_automation() -> Self {
        let env = Self::new("auto.populate_members: true\n");
        let log_path = env.fixtures.get_temp_path().join("automation-marker.log");
        let automation_yaml = format!(
            "automation:\n  rules:\n\
             \x20   - name: bare updated marker\n\
             \x20     on:\n\
             \x20       updated:\n\
             \x20         run: \"echo {MARKER_UPDATED_HOOK} >> {log}\"\n\
             \x20   - name: commented marker\n\
             \x20     on:\n\
             \x20       commented:\n\
             \x20         run: \"echo {MARKER_COMMENTED_HOOK} >> {log}\"\n",
            log = log_path.to_string_lossy(),
        );
        std::fs::write(
            env.fixtures.tasks_root.join("automation.yml"),
            automation_yaml,
        )
        .expect("write automation config");
        env
    }

    fn lotar(&self) -> assert_cmd::Command {
        let mut cmd = common::lotar_cmd().expect("lotar binary");
        cmd.env("LOTAR_TEST_SILENT", "1")
            .env("LOTAR_DEFAULT_REPORTER", "devoneoh")
            .current_dir(self.fixtures.get_temp_path())
            .arg("--tasks-dir")
            .arg(&self.fixtures.tasks_root);
        cmd
    }

    fn add_task(&self, title: &str) -> String {
        let output = self
            .lotar()
            .arg("add")
            .arg(title)
            .arg("--project=NOP")
            .output()
            .expect("spawn lotar add");
        assert!(
            output.status.success(),
            "lotar add failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        common::extract_task_id_from_output(&stdout)
            .unwrap_or_else(|| panic!("no task id in add output: {stdout}"))
    }

    fn run(&self, args: &[&str]) -> Output {
        self.lotar().args(args).output().expect("spawn lotar")
    }

    fn run_ok(&self, args: &[&str]) -> Output {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "lotar {args:?} failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        output
    }

    fn run_fail(&self, args: &[&str]) -> Output {
        let output = self.run(args);
        assert!(
            !output.status.success(),
            "lotar {args:?} unexpectedly succeeded: {}",
            String::from_utf8_lossy(&output.stdout),
        );
        output
    }

    fn combined(output: &Output) -> String {
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }

    fn task_path(&self, task_id: &str) -> PathBuf {
        let project = task_id.rsplit_once('-').map(|(p, _)| p).unwrap_or("NOP");
        let numeric = task_id.rsplit_once('-').map(|(_, n)| n).unwrap_or(task_id);
        self.fixtures
            .tasks_root
            .join(project)
            .join(format!("{numeric}.yml"))
    }

    fn marker_count(&self, marker: &str) -> usize {
        let contents =
            std::fs::read_to_string(self.fixtures.get_temp_path().join("automation-marker.log"))
                .unwrap_or_default();
        contents
            .lines()
            .filter(|line| line.trim() == marker)
            .count()
    }

    fn reset_markers(&self) {
        std::fs::write(
            self.fixtures.get_temp_path().join("automation-marker.log"),
            b"",
        )
        .expect("reset automation marker log");
    }

    /// Register a FAKE agent profile in the NOP project config: the name is
    /// a recognized directive for normalization/comparison, but no real
    /// model, runner binary, or repository exists and assigning it never
    /// launches anything (assignment only stores the directive; no
    /// automation is configured in this fixture).
    fn register_fake_agent_profile(&self, name: &str) {
        let project_dir = self.fixtures.tasks_root.join("NOP");
        std::fs::create_dir_all(&project_dir).expect("create project dir");
        std::fs::write(
            project_dir.join("config.yml"),
            format!("agents:\n  {name}: fake-runner\n"),
        )
        .expect("write project agent profile");
    }
}

struct FrozenFile {
    bytes: Vec<u8>,
    mtime: SystemTime,
}

fn freeze(path: &Path) -> FrozenFile {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or_else(|e| panic!("stat {}: {e}", path.display()));
    FrozenFile { bytes, mtime }
}

fn assert_frozen(path: &Path, before: &FrozenFile, label: &str) {
    let after = freeze(path);
    assert_eq!(
        after.bytes, before.bytes,
        "{label}: task YAML bytes must be preserved"
    );
    assert_eq!(
        after.mtime, before.mtime,
        "{label}: task YAML mtime must be untouched"
    );
}

/// (modified, history-length) snapshot parsed from the raw task YAML.
fn yaml_snapshot(path: &Path) -> (Option<String>, usize) {
    let raw = std::fs::read_to_string(path).unwrap();
    let value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&raw).unwrap_or_else(|e| panic!("parse task YAML {e}: {raw}"));
    (
        value
            .get("modified")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        value
            .get("history")
            .and_then(|v| v.as_sequence())
            .map(|s| s.len())
            .unwrap_or(0),
    )
}

/// Typed live `references` entries (never history text) containing a URL.
fn live_reference_urls(path: &Path) -> Vec<String> {
    let raw = std::fs::read_to_string(path).unwrap();
    let value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&raw).unwrap_or_else(|e| panic!("parse task YAML {e}: {raw}"));
    value
        .get("references")
        .and_then(|v| v.as_sequence())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("link").and_then(|l| l.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Parsed live assignee scalar (never history text; YAML-quoted directives
/// like `'@copilot'` come back unquoted).
fn stored_assignee(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).unwrap();
    let value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&raw).unwrap_or_else(|e| panic!("parse task YAML {e}: {raw}"));
    value
        .get("assignee")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// The CLI edit path routes every no-op flavor (all-None patch, same
/// scalars, normalized effort, duplicate tag merge, identical custom-field
/// map, clearing an already-absent assignee) through the shared effective
/// change boundary: exit 0, byte-identical YAML, untouched mtime,
/// `modified`, and history. A genuine edit still writes and records
/// exactly one history entry.
#[test]
fn cli_edit_noop_matrix_freezes_task_state() {
    let env = NoopCliEnv::new("");
    let id = env.add_task("Edit no-op target");

    // Give the task a stable populated state first (real change).
    env.run_ok(&[
        "task",
        "edit",
        &id,
        "--effort",
        "3h",
        "--due",
        "2031-01-15",
        "--tag",
        "svc",
        "--field",
        "product=Core",
        "--description",
        "Keep me",
    ]);
    let yaml = std::fs::read_to_string(env.task_path(&id)).unwrap();
    assert!(yaml.contains("effort: 3.00h"), "canonical effort: {yaml}");

    let frozen = freeze(&env.task_path(&id));
    let before = yaml_snapshot(&env.task_path(&id));

    // All-None patch: no edit flags at all.
    env.run_ok(&["task", "edit", &id]);
    // Same scalars, same tag merge, same custom-field map.
    env.run_ok(&[
        "task",
        "edit",
        &id,
        "--title",
        "Edit no-op target",
        "--description",
        "Keep me",
        "--tag",
        "svc",
        "--field",
        "product=Core",
        "--due",
        "2031-01-15",
    ]);
    // Normalized effort: 180m canonicalizes to the stored 3.00h.
    env.run_ok(&["task", "edit", &id, "--effort", "180m"]);
    // Clearing an already-absent assignee (null-clear of a missing value).
    env.run_ok(&["task", "edit", &id, "--assignee", ""]);

    assert_frozen(&env.task_path(&id), &frozen, "no-op edit matrix");
    assert_eq!(
        yaml_snapshot(&env.task_path(&id)),
        before,
        "no-op edit matrix must not touch modified or history"
    );

    // Genuine change: bytes differ, modified moves, history grows by one.
    env.run_ok(&["task", "edit", &id, "--title", "Edited for real"]);
    let after = freeze(&env.task_path(&id));
    assert_ne!(after.bytes, frozen.bytes, "genuine edit must write");
    let (modified, history) = yaml_snapshot(&env.task_path(&id));
    assert_ne!(modified, before.0, "genuine edit must bump modified");
    assert_eq!(history, before.1 + 1, "genuine edit adds one history entry");
}

/// Assignee same-value assignments resolve identity and member semantics
/// (@me -> current identity, case-insensitive members, human `@`-aliases,
/// padded task-id aliases) and report the PropertyNoop contract — success,
/// "unchanged", and no mutation metadata — without touching the file. A
/// genuinely different assignee still writes.
#[test]
fn cli_assignee_same_value_aliases_report_property_noop() {
    let env = NoopCliEnv::new("auto.populate_members: true\n");
    let id = env.add_task("Assignee no-op target");

    env.run_ok(&["assignee", &id, "devoneoh"]);
    let frozen = freeze(&env.task_path(&id));

    // @me resolves to the configured identity: property no-op, zero
    // mutation metadata in the JSON payload.
    let output = env.run_ok(&["-f", "json", "assignee", &id, "@me"]);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let payload: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("assignee noop must emit one JSON object: {e}\n{stdout}"));
    assert_eq!(payload["status"], "success", "noop payload: {payload}");
    assert_eq!(payload["task_id"], id, "noop payload: {payload}");
    assert_eq!(payload["assignee"], "devoneoh", "noop payload: {payload}");
    assert!(
        payload["message"]
            .as_str()
            .unwrap_or("")
            .contains("unchanged"),
        "noop message must report unchanged: {payload}"
    );
    assert!(
        !payload.as_object().unwrap().contains_key("modified"),
        "noop payload must carry zero mutation metadata: {payload}"
    );

    // Case-insensitive member equality and human `@`-alias stripping.
    env.run_ok(&["assignee", &id, "DevOneOh"]);
    env.run_ok(&["assignee", &id, "@devoneoh"]);

    // Padded task-id alias addresses the same task canonically.
    let project = id.rsplit_once('-').map(|(p, _)| p).unwrap_or("NOP");
    let padded = format!(
        "{project}-00{}",
        id.rsplit_once('-').map(|(_, n)| n).unwrap_or("1")
    );
    env.run_ok(&["assignee", &padded, "@me"]);

    assert_frozen(&env.task_path(&id), &frozen, "assignee noop aliases");

    // Genuine change still writes and reports the transition.
    env.run_ok(&["assignee", &id, "alice"]);
    let after = freeze(&env.task_path(&id));
    assert_ne!(
        after.bytes, frozen.bytes,
        "genuine assignee change must write"
    );
    let yaml = std::fs::read_to_string(env.task_path(&id)).unwrap();
    assert!(
        yaml.contains("assignee: alice"),
        "persisted assignee: {yaml}"
    );
}

/// A registered agent directive and a same-named human are DIFFERENT
/// assignees: replacing bare human `copilot` with the registered
/// `@copilot` directive — and the reverse — are genuine changes that
/// persist the exact directive/human spelling, write bytes, bump
/// `modified`, and record one history entry each. Unregistered-human
/// spellings (`@alice` vs `alice`, case variants) remain no-ops. The
/// agent profile is a fake fixture (`agents: {copilot: fake-runner}`):
/// assignment only stores the directive, so no agent process is launched
/// and nothing needs cleanup.
#[test]
fn cli_assignee_agent_directive_vs_same_name_human_is_genuine_both_directions() {
    let env = NoopCliEnv::new("auto.populate_members: true\n");
    env.register_fake_agent_profile("copilot");
    let id = env.add_task("Agent directive target");

    // Human baseline.
    env.run_ok(&["assignee", &id, "copilot"]);
    assert_eq!(
        stored_assignee(&env.task_path(&id)),
        Some("copilot".to_string()),
        "bare human spelling must persist first"
    );

    // Human -> registered agent directive: GENUINE, exact spelling kept.
    let frozen = freeze(&env.task_path(&id));
    let before = yaml_snapshot(&env.task_path(&id));
    let output = env.run_ok(&["assignee", &id, "@copilot"]);
    assert!(
        !NoopCliEnv::combined(&output).contains("already"),
        "a registered agent directive must never be reported as a no-op"
    );
    assert_eq!(
        stored_assignee(&env.task_path(&id)),
        Some("@copilot".to_string()),
        "directive spelling must persist"
    );
    assert_ne!(
        freeze(&env.task_path(&id)).bytes,
        frozen.bytes,
        "human -> agent directive must write"
    );
    let (modified, history) = yaml_snapshot(&env.task_path(&id));
    assert_ne!(
        modified, before.0,
        "human -> agent directive bumps modified"
    );
    assert_eq!(
        history,
        before.1 + 1,
        "human -> agent directive records one history entry"
    );

    // Registered agent directive -> same-named human: GENUINE again.
    let frozen_back = freeze(&env.task_path(&id));
    let before_back = yaml_snapshot(&env.task_path(&id));
    env.run_ok(&["assignee", &id, "copilot"]);
    assert_eq!(
        stored_assignee(&env.task_path(&id)),
        Some("copilot".to_string()),
        "bare human spelling must persist on the way back"
    );
    assert_ne!(
        freeze(&env.task_path(&id)).bytes,
        frozen_back.bytes,
        "agent directive -> human must write"
    );
    let (modified_back, history_back) = yaml_snapshot(&env.task_path(&id));
    assert_ne!(
        modified_back, before_back.0,
        "agent directive -> human bumps modified"
    );
    assert_eq!(
        history_back,
        before_back.1 + 1,
        "agent directive -> human records one history entry"
    );

    // Unregistered-human spellings stay no-ops: legacy `@alice` and case.
    env.run_ok(&["assignee", &id, "alice"]);
    assert_eq!(
        stored_assignee(&env.task_path(&id)),
        Some("alice".to_string())
    );
    let frozen_human = freeze(&env.task_path(&id));
    env.run_ok(&["assignee", &id, "@alice"]);
    env.run_ok(&["assignee", &id, "Alice"]);
    assert_frozen(
        &env.task_path(&id),
        &frozen_human,
        "unregistered human @-alias and case variants",
    );
}

// A real `lotar-agent-wrapper` process with ticket metadata would arm the
// CLI's active-job refusal, but the process-table scan cannot attribute
// tickets on macOS (sysinfo exposes no argv there, verified: `lotar agent
// list-running` shows the wrapper pid with `job_id`/`ticket` as `-`), so
// the same-@me-under-active-job ordering cannot be blackbox-armed on this
// platform. ACTUAL coverage today: the same-value PropertyNoop contract
// itself is covered below, and NO active-job assignment carveout case
// exists anywhere yet — the DEV-102 core worker
// (tests/task_noop_update_test.rs owner) is coordinated to add the
// service-level guard case.

/// A bare `on.updated` automation hook (no `when`) stays silent across
/// every no-op entrypoint and fires exactly once for a genuine change;
/// duplicate explicit comment appends are intentional and still both
/// persist and fire `on.commented` per append.
#[test]
fn cli_bare_updated_automation_silent_on_noops_fires_once_on_change() {
    let env = NoopCliEnv::with_automation();
    let id = env.add_task("Automation no-op target");

    // Real change first (also proves the hook wiring works).
    env.run_ok(&["assignee", &id, "devoneoh"]);
    assert_eq!(env.marker_count(MARKER_UPDATED_HOOK), 1);
    env.reset_markers();

    let frozen = freeze(&env.task_path(&id));

    // No-op edit and no-op assignee: zero hooks.
    env.run_ok(&["task", "edit", &id, "--title", "Automation no-op target"]);
    env.run_ok(&["assignee", &id, "@me"]);
    assert_eq!(
        env.marker_count(MARKER_UPDATED_HOOK),
        0,
        "bare on.updated must stay silent on no-ops"
    );
    assert_eq!(
        env.marker_count(MARKER_COMMENTED_HOOK),
        0,
        "no comments happened yet"
    );
    assert_frozen(&env.task_path(&id), &frozen, "automation no-op round");

    // Genuine change: exactly one bare-hook fire.
    env.run_ok(&["task", "edit", &id, "--title", "Automation real change"]);
    assert_eq!(
        env.marker_count(MARKER_UPDATED_HOOK),
        1,
        "genuine change fires the bare hook exactly once"
    );

    // Duplicate explicit comment appends are intentional: both persist
    // (no dedup) and each fires on.commented once.
    env.run_ok(&["comment", &id, "duplicate body"]);
    env.run_ok(&["comment", &id, "duplicate body"]);
    assert_eq!(env.marker_count(MARKER_COMMENTED_HOOK), 2);
    assert_eq!(
        env.marker_count(MARKER_UPDATED_HOOK),
        1,
        "comment appends must not fire on.updated"
    );
    let yaml = std::fs::read_to_string(env.task_path(&id)).unwrap();
    assert_eq!(
        yaml.matches("text: duplicate body").count(),
        2,
        "explicit duplicate appends must both persist: {yaml}"
    );
}

/// Duplicate reference adds and absent reference removes are no-ops that
/// preserve the typed reference entries; a real remove still detaches.
#[test]
fn cli_reference_duplicate_add_and_absent_remove_preserve_typed_state() {
    let env = NoopCliEnv::new("");
    let id = env.add_task("Reference no-op target");

    let added = env.run_ok(&[
        "task",
        "reference",
        "add",
        "link",
        &id,
        "https://example.com/docs",
    ]);
    assert!(
        NoopCliEnv::combined(&added).contains("reference updated"),
        "first link add is a real change"
    );

    let frozen = freeze(&env.task_path(&id));

    let duplicate = env.run_ok(&[
        "task",
        "reference",
        "add",
        "link",
        &id,
        "https://example.com/docs",
    ]);
    assert!(
        NoopCliEnv::combined(&duplicate).contains("already in desired state"),
        "duplicate link add must report the desired-state no-op"
    );

    env.run_ok(&[
        "task",
        "reference",
        "remove",
        "link",
        &id,
        "https://example.com/absent",
    ]);

    assert_frozen(
        &env.task_path(&id),
        &frozen,
        "duplicate add / absent remove",
    );
    assert_eq!(
        live_reference_urls(&env.task_path(&id)),
        vec!["https://example.com/docs".to_string()],
        "typed live link reference must be preserved verbatim"
    );

    // Real remove detaches and writes.
    env.run_ok(&[
        "task",
        "reference",
        "remove",
        "link",
        &id,
        "https://example.com/docs",
    ]);
    let after = freeze(&env.task_path(&id));
    assert_ne!(after.bytes, frozen.bytes, "real remove must write");
    assert_eq!(
        live_reference_urls(&env.task_path(&id)),
        Vec::<String>::new(),
        "removed link must be gone from the typed references block"
    );
}

/// Repeated soft deletes and restores of an already-active task are no-ops
/// that keep the tombstone/active bytes and mtime frozen; genuine
/// lifecycle transitions still write while preserving `modified`
/// (lifecycle exception contract).
#[test]
fn cli_repeated_soft_delete_and_active_restore_are_noops() {
    let env = NoopCliEnv::new("");
    let id = env.add_task("Lifecycle no-op target");
    let active = freeze(&env.task_path(&id));
    let (modified_active, _) = yaml_snapshot(&env.task_path(&id));

    // Genuine soft delete.
    env.run_ok(&["task", "delete", &id, "--yes"]);
    let tombstone = freeze(&env.task_path(&id));
    assert_ne!(tombstone.bytes, active.bytes, "soft delete must write");
    assert!(
        std::fs::read_to_string(env.task_path(&id))
            .unwrap()
            .contains("deleted_at:"),
        "soft delete must stamp deleted_at"
    );

    // Repeated soft delete: state-after semantics keep reporting success
    // (`deleted: true`), but the service boundary performs no write — the
    // tombstone bytes, mtime, modified, and history stay frozen.
    env.run_ok(&["task", "delete", &id, "--yes"]);
    assert_frozen(&env.task_path(&id), &tombstone, "repeated soft delete");

    // Genuine restore.
    env.run_ok(&["task", "restore", &id]);
    let restored = freeze(&env.task_path(&id));
    assert_ne!(restored.bytes, tombstone.bytes, "restore must write");
    assert!(
        !std::fs::read_to_string(env.task_path(&id))
            .unwrap()
            .contains("deleted_at:"),
        "restore must clear deleted_at"
    );

    // Restore of an active task: idempotent no-op.
    env.run_ok(&["task", "restore", &id]);
    assert_frozen(&env.task_path(&id), &restored, "active restore");

    let (modified_final, _) = yaml_snapshot(&env.task_path(&id));
    assert_eq!(
        modified_final, modified_active,
        "lifecycle transitions preserve modified per the lifecycle exception"
    );
}

/// Invalid patches keep failing closed without writes, and a pending
/// transaction journal refuses mutations strictly — including patches that
/// would otherwise be effective no-ops (the no-op boundary never bypasses
/// journal recovery).
#[test]
fn cli_invalid_patches_and_pending_journal_still_fail_closed() {
    let env = NoopCliEnv::new("");
    let id = env.add_task("Refusal no-op target");
    let frozen = freeze(&env.task_path(&id));

    let bad_priority = env.run_fail(&["task", "edit", &id, "--priority", "Bogus"]);
    assert!(
        NoopCliEnv::combined(&bad_priority).contains("validation failed")
            || NoopCliEnv::combined(&bad_priority).contains("not allowed"),
        "invalid priority must fail with a validation error: {}",
        NoopCliEnv::combined(&bad_priority)
    );
    let bad_type = env.run_fail(&["task", "edit", &id, "--type", "AlsoBogus"]);
    assert!(
        NoopCliEnv::combined(&bad_type).contains("validation failed")
            || NoopCliEnv::combined(&bad_type).contains("not allowed"),
        "invalid type must fail with a validation error"
    );
    assert_frozen(&env.task_path(&id), &frozen, "invalid patches");

    // Corrupt pending journal: every mutation — even an effective no-op —
    // is refused strictly, mirroring the genuine-change contract.
    let journal = env.fixtures.tasks_root.join(".txn-pending.json");
    std::fs::write(&journal, "not a journal").unwrap();

    let refused = env.run_fail(&["task", "edit", &id, "--title", "Refusal no-op target"]);
    let refusal = NoopCliEnv::combined(&refused);
    assert!(
        refusal.to_lowercase().contains("pending transaction"),
        "no-op edit must still hit the pending-journal refusal: {refusal}"
    );
    assert!(
        journal.exists(),
        "the strict refusal must not consume the journal"
    );
    assert_frozen(
        &env.task_path(&id),
        &frozen,
        "no-op edit under pending journal",
    );
}
