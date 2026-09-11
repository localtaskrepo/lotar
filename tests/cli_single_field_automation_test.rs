//! DEV-28 regression coverage: the single-field CLI commands (status,
//! priority, assignee, duedate, effort) must persist through
//! `TaskService::update` with agent automation enabled, so `on.updated`
//! automation rules observe the field change. Each test drives the real
//! `lotar` binary in an isolated temporary workspace with deterministic
//! automation side effects (synchronous `run` actions appending to a log
//! file); no agents or Git are involved.

mod common;

use common::TestFixtures;
use std::path::PathBuf;

const MARKER_PRIORITY_HIGH: &str = "priority-high";
const MARKER_DUE_SET: &str = "due-set";
const MARKER_EFFORT_SET: &str = "effort-set";
const MARKER_EFFORT_CLEARED: &str = "effort-cleared";
const MARKER_STATUS_DONE: &str = "status-done";
const MARKER_ASSIGNEE_ALICE: &str = "assignee-alice";

struct AutomationCliEnv {
    fixtures: TestFixtures,
    log_path: PathBuf,
}

impl AutomationCliEnv {
    fn new() -> Self {
        let fixtures = TestFixtures::new();
        let log_path = fixtures.get_temp_path().join("automation-marker.log");
        let automation_yaml = format!(
            "automation:\n  rules:\n\
             \x20   - name: priority raised to high\n\
             \x20     when:\n\
             \x20       changes:\n\
             \x20         priority:\n\
             \x20           to: High\n\
             \x20     on:\n\
             \x20       updated:\n\
             \x20         run: \"echo {MARKER_PRIORITY_HIGH} >> {log}\"\n\
             \x20   - name: due date set\n\
             \x20     when:\n\
             \x20       changes:\n\
             \x20         due_date:\n\
             \x20           to:\n\
             \x20             exists: true\n\
             \x20     on:\n\
             \x20       updated:\n\
             \x20         run: \"echo {MARKER_DUE_SET} >> {log}\"\n\
             \x20   - name: effort set\n\
             \x20     when:\n\
             \x20       changes:\n\
             \x20         effort:\n\
             \x20           to:\n\
             \x20             exists: true\n\
             \x20     on:\n\
             \x20       updated:\n\
             \x20         run: \"echo {MARKER_EFFORT_SET} >> {log}\"\n\
             \x20   - name: effort cleared\n\
             \x20     when:\n\
             \x20       changes:\n\
             \x20         effort:\n\
             \x20           to:\n\
             \x20             exists: false\n\
             \x20     on:\n\
             \x20       updated:\n\
             \x20         run: \"echo {MARKER_EFFORT_CLEARED} >> {log}\"\n\
             \x20   - name: status done\n\
             \x20     when:\n\
             \x20       changes:\n\
             \x20         status:\n\
             \x20           to: Done\n\
             \x20     on:\n\
             \x20       updated:\n\
             \x20         run: \"echo {MARKER_STATUS_DONE} >> {log}\"\n\
             \x20   - name: assignee alice\n\
             \x20     when:\n\
             \x20       changes:\n\
             \x20         assignee:\n\
             \x20           to: alice\n\
             \x20     on:\n\
             \x20       updated:\n\
             \x20         run: \"echo {MARKER_ASSIGNEE_ALICE} >> {log}\"\n",
            log = log_path.to_string_lossy(),
        );
        std::fs::write(fixtures.tasks_root.join("automation.yml"), automation_yaml)
            .expect("write automation config");

        Self { fixtures, log_path }
    }

    fn lotar(&self) -> assert_cmd::Command {
        let mut cmd = common::lotar_cmd().expect("lotar binary");
        cmd.env("LOTAR_TEST_SILENT", "1")
            .arg("--tasks-dir")
            .arg(&self.fixtures.tasks_root);
        cmd
    }

    fn add_task(&self, title: &str) -> String {
        let output = self
            .lotar()
            .arg("add")
            .arg(title)
            .arg("--project=DEV28")
            .output()
            .expect("spawn lotar add");
        assert!(
            output.status.success(),
            "lotar add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        common::extract_task_id_from_output(&stdout)
            .unwrap_or_else(|| panic!("no task id in add output: {stdout}"))
    }

    fn run_ok(&self, args: &[&str]) {
        let output = self.lotar().args(args).output().expect("spawn lotar");
        assert!(
            output.status.success(),
            "lotar {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn stdout_of(&self, args: &[&str]) -> String {
        let output = self.lotar().args(args).output().expect("spawn lotar");
        assert!(
            output.status.success(),
            "lotar {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn marker_count(&self, marker: &str) -> usize {
        let contents = std::fs::read_to_string(&self.log_path).unwrap_or_default();
        contents
            .lines()
            .filter(|line| line.trim() == marker)
            .count()
    }

    fn task_yaml(&self, task_id: &str) -> String {
        let project = task_id.split('-').next().unwrap_or_default();
        let numeric = task_id.rsplit('-').next().unwrap_or_default();
        std::fs::read_to_string(
            self.fixtures
                .tasks_root
                .join(project)
                .join(format!("{numeric}.yml")),
        )
        .unwrap_or_else(|_| panic!("task file for {task_id} should exist"))
    }
}

#[test]
fn cli_priority_set_routes_through_automation_and_persists() {
    let env = AutomationCliEnv::new();
    let task_id = env.add_task("Priority automation target");

    env.run_ok(&["priority", &task_id, "High"]);

    assert_eq!(env.marker_count(MARKER_PRIORITY_HIGH), 1);
    assert!(
        env.task_yaml(&task_id).contains("priority: High"),
        "priority value must be persisted in the task file"
    );
    let current = env.stdout_of(&["priority", &task_id]);
    assert!(
        current.contains("priority: High"),
        "getter must reload the persisted priority: {current}"
    );
}

#[test]
fn cli_priority_repeat_value_is_noop_without_automation() {
    let env = AutomationCliEnv::new();
    let task_id = env.add_task("Priority noop target");

    env.run_ok(&["priority", &task_id, "High"]);
    assert_eq!(env.marker_count(MARKER_PRIORITY_HIGH), 1);

    env.run_ok(&["priority", &task_id, "High"]);
    assert_eq!(
        env.marker_count(MARKER_PRIORITY_HIGH),
        1,
        "a no-op priority command must not fire update automation"
    );
}

#[test]
fn cli_duedate_set_routes_through_automation_and_persists() {
    let env = AutomationCliEnv::new();
    let task_id = env.add_task("Due date automation target");

    env.run_ok(&["due-date", &task_id, "2030-01-15"]);

    assert_eq!(env.marker_count(MARKER_DUE_SET), 1);
    assert!(
        env.task_yaml(&task_id).contains("due_date: 2030-01-15"),
        "normalized due date must be persisted in the task file"
    );
    let current = env.stdout_of(&["due-date", &task_id]);
    assert!(
        current.contains("due date: 2030-01-15"),
        "getter must reload the persisted due date: {current}"
    );
}

#[test]
fn cli_effort_set_and_clear_route_through_automation() {
    let env = AutomationCliEnv::new();
    let task_id = env.add_task("Effort automation target");

    env.run_ok(&["effort", &task_id, "3h"]);
    assert_eq!(env.marker_count(MARKER_EFFORT_SET), 1);
    assert_eq!(env.marker_count(MARKER_EFFORT_CLEARED), 0);
    assert!(
        env.task_yaml(&task_id).contains("effort: 3.00h"),
        "normalized effort must be persisted in the task file"
    );

    env.run_ok(&["effort", &task_id, "--clear"]);
    assert_eq!(env.marker_count(MARKER_EFFORT_CLEARED), 1);
    let yaml = env.task_yaml(&task_id);
    assert!(
        !yaml.lines().any(|line| line.starts_with("effort:")),
        "cleared effort must be removed from the task file: {yaml}"
    );
    let current = env.stdout_of(&["effort", &task_id]);
    assert!(
        current.contains("effort: -"),
        "getter must show no effort after clear: {current}"
    );

    env.run_ok(&["priority", &task_id, "High"]);
    assert_eq!(
        env.marker_count(MARKER_EFFORT_SET),
        1,
        "an unrelated field update must not re-fire effort rules"
    );
    assert_eq!(env.marker_count(MARKER_EFFORT_CLEARED), 1);
}

#[test]
fn cli_effort_clear_on_effortless_task_is_noop() {
    let env = AutomationCliEnv::new();
    let task_id = env.add_task("Effort clear noop target");

    env.run_ok(&["effort", &task_id, "--clear"]);

    assert_eq!(
        env.marker_count(MARKER_EFFORT_CLEARED),
        0,
        "clearing an already-empty effort must not fire update automation"
    );
}

#[test]
fn cli_status_and_assignee_set_route_through_automation() {
    let env = AutomationCliEnv::new();
    let task_id = env.add_task("Status and assignee automation target");

    env.run_ok(&["assignee", &task_id, "alice"]);
    assert_eq!(env.marker_count(MARKER_ASSIGNEE_ALICE), 1);
    assert!(
        env.task_yaml(&task_id).contains("assignee: alice"),
        "assignee must be persisted in the task file"
    );

    env.run_ok(&["status", &task_id, "Done"]);
    assert_eq!(env.marker_count(MARKER_STATUS_DONE), 1);
    assert!(
        env.task_yaml(&task_id).contains("status: Done"),
        "status must be persisted in the task file"
    );

    let assignee = env.stdout_of(&["assignee", &task_id]);
    assert!(
        assignee.contains("assignee: alice"),
        "getter must reload the persisted assignee: {assignee}"
    );
    let status = env.stdout_of(&["status", &task_id]);
    assert!(
        status.contains("status: Done"),
        "getter must reload the persisted status: {status}"
    );
}

#[test]
fn nested_task_subcommands_delegate_to_automated_update_path() {
    let env = AutomationCliEnv::new();
    let status_task = env.add_task("Nested status target");
    let assignee_task = env.add_task("Nested assignee target");
    let priority_task = env.add_task("Nested priority target");
    let duedate_task = env.add_task("Nested duedate target");
    let effort_task = env.add_task("Nested effort target");

    env.run_ok(&["task", "status", &status_task, "Done"]);
    env.run_ok(&["task", "assignee", &assignee_task, "alice"]);
    env.run_ok(&["task", "priority", &priority_task, "High"]);
    env.run_ok(&["task", "due-date", &duedate_task, "2030-02-20"]);
    env.run_ok(&["task", "effort", &effort_task, "90m"]);
    env.run_ok(&["task", "effort", &effort_task, "--clear"]);

    assert_eq!(env.marker_count(MARKER_STATUS_DONE), 1);
    assert_eq!(env.marker_count(MARKER_ASSIGNEE_ALICE), 1);
    assert_eq!(env.marker_count(MARKER_PRIORITY_HIGH), 1);
    assert_eq!(env.marker_count(MARKER_DUE_SET), 1);
    assert_eq!(env.marker_count(MARKER_EFFORT_SET), 1);
    assert_eq!(env.marker_count(MARKER_EFFORT_CLEARED), 1);

    assert!(env.task_yaml(&status_task).contains("status: Done"));
    assert!(env.task_yaml(&assignee_task).contains("assignee: alice"));
    assert!(env.task_yaml(&priority_task).contains("priority: High"));
    assert!(
        env.task_yaml(&duedate_task)
            .contains("due_date: 2030-02-20")
    );
    assert!(
        !env.task_yaml(&effort_task)
            .lines()
            .any(|line| line.starts_with("effort:")),
        "nested effort clear must remove the effort value"
    );

    let effort_get = env.stdout_of(&["effort", &effort_task]);
    assert!(
        effort_get.contains("effort: -"),
        "getter must show no effort after nested clear: {effort_get}"
    );
}
