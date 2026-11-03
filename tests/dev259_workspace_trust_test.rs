//! DEV-259: commands from repository files (automation `run`, agent
//! profiles in repo config) only run after the workspace is trusted.
#![cfg(unix)]

mod common;

use common::TestFixtures;
use common::env_mutex::EnvVarGuard;
use predicates::prelude::*;

struct TrustEnv {
    fixtures: TestFixtures,
    trust_file: std::path::PathBuf,
    _trust_all: EnvVarGuard,
    _trust_file: EnvVarGuard,
    _tasks: EnvVarGuard,
}

impl TrustEnv {
    fn new() -> Self {
        let fixtures = TestFixtures::new();
        let trust_file = fixtures.get_temp_path().join("home-trust.yml");
        let _trust_all = EnvVarGuard::clear("LOTAR_TRUST_ALL");
        let _trust_file = EnvVarGuard::set("LOTAR_TRUST_FILE", &trust_file.to_string_lossy());
        let _tasks = EnvVarGuard::set(
            "LOTAR_TASKS_DIR",
            fixtures.tasks_root.to_string_lossy().as_ref(),
        );
        Self {
            fixtures,
            trust_file,
            _trust_all,
            _trust_file,
            _tasks,
        }
    }

    fn root(&self) -> &std::path::Path {
        self.fixtures.get_temp_path()
    }

    fn lotar(&self) -> assert_cmd::Command {
        let mut cmd = common::lotar_cmd().unwrap();
        cmd.current_dir(self.root())
            .env_remove("LOTAR_TRUST_ALL")
            .env("LOTAR_TRUST_FILE", &self.trust_file)
            .env("LOTAR_TEST_SILENT", "1");
        cmd
    }

    fn write_rule(&self, marker: &std::path::Path) {
        std::fs::write(
            self.fixtures.tasks_root.join("automation.yml"),
            format!(
                "automation:\n  rules:\n    - name: mark\n      on:\n        created:\n          run: \"touch {}\"\n",
                marker.display()
            ),
        )
        .unwrap();
    }
}

#[test]
fn repository_run_commands_wait_for_trust() {
    let env = TrustEnv::new();
    let marker = env.root().join("RAN");
    env.write_rule(&marker);

    env.lotar()
        .args(["add", "untrusted", "--project=TRU"])
        .assert()
        .success()
        .stderr(predicate::str::contains("lotar trust"));
    assert!(!marker.exists(), "an untrusted rule must not run");

    env.lotar()
        .args(["trust", "--status"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rule 'mark' on created runs touch",
        ));
    env.lotar().arg("trust").assert().success();
    env.lotar()
        .args(["add", "trusted", "--project=TRU"])
        .assert()
        .success();
    assert!(marker.exists(), "a trusted rule runs");
    std::fs::remove_file(&marker).unwrap();

    // A change from outside LoTaR (a pull, a merge) needs trust again.
    let other = env.root().join("OTHER");
    env.write_rule(&other);
    env.lotar()
        .args(["add", "changed", "--project=TRU"])
        .assert()
        .success()
        .stderr(predicate::str::contains("changed since you last trusted"));
    assert!(
        !other.exists(),
        "a changed rule must not run before re-trust"
    );

    env.lotar().args(["trust", "--revoke"]).assert().success();
    env.lotar()
        .args(["--format", "json", "trust", "--status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"state\":\"untrusted\""));
}

#[test]
fn lotar_writes_keep_a_trusted_workspace_trusted() {
    let env = TrustEnv::new();
    let marker = env.root().join("RAN");
    env.write_rule(&marker);
    let tasks = env.fixtures.tasks_root.clone();
    use lotar::services::trust_service::{TrustService, TrustState};

    // An untrusted workspace stays untrusted through LoTaR's own writes.
    let yaml = format!(
        "automation:\n  rules:\n    - name: mark2\n      on:\n        created:\n          run: \"touch {}\"\n",
        marker.display()
    );
    lotar::services::automation_service::AutomationService::set(&tasks, None, &yaml).unwrap();
    assert_eq!(TrustService::state(&tasks), TrustState::Untrusted);

    TrustService::trust(&tasks).unwrap();
    let yaml = yaml.replace("mark2", "mark3");
    lotar::services::automation_service::AutomationService::set(&tasks, None, &yaml).unwrap();
    assert_eq!(
        TrustService::state(&tasks),
        TrustState::Trusted,
        "edits through LoTaR keep the trust"
    );
}

#[test]
fn repository_agent_profiles_wait_for_trust() {
    let env = TrustEnv::new();
    let marker = env.root().join("AGENT_RAN");
    env.fixtures.create_config_in_dir(
        &env.fixtures.tasks_root,
        &format!(
            "default:\n  project: TEST\nagents:\n  stub:\n    runner: command\n    command: sh\n    args: ['-c', 'touch \"{}\"']\n",
            marker.display()
        ),
    );
    env.lotar().args(["add", "agent task"]).assert().success();

    env.lotar()
        .args(["agent", "run", "TEST-1", "go", "--agent", "stub", "--wait"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("agent profile 'stub'"));
    assert!(!marker.exists());

    env.lotar().arg("trust").assert().success();
    env.lotar()
        .args(["agent", "run", "TEST-1", "go", "--agent", "stub", "--wait"])
        .assert()
        .success();
    assert!(marker.exists(), "a trusted profile runs");
}
