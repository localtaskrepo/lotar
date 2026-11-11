//! DEV-318: the t3 runner starts a T3 Code thread per job (fake `t3` here).
#![cfg(unix)]

use crate::common::TestFixtures;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_types::{AgentJob, TaskCreate};
use lotar::services::agent_job_service::{AgentJobService, AgentOrchestratorMode};
use lotar::services::finding_service::FindingService;
use lotar::services::task_service::TaskService;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// A fake `t3`: `thread start` records its arguments and stdin, prints
/// Electron's startup note and `Thread <id>` to stderr, the reply to stdout,
/// and exits with `FAKE_T3_EXIT`; `thread show` answers with the link and
/// `FAKE_T3_WORKTREE` as the thread's worktree.
fn fake_t3(dir: &Path) -> PathBuf {
    let path = dir.join("fake-t3");
    let script = format!(
        r#"#!/bin/sh
out="{out}"
if [ "$1" = "thread" ] && [ "$2" = "show" ]; then
  echo "$3" >> "$out/show.calls"
  if [ -n "$FAKE_T3_WORKTREE" ]; then wt="\"$FAKE_T3_WORKTREE\""; br='"agent/from-t3"'; else wt=null; br='"main"'; fi
  printf '{{"thread":{{"threadId":"%s","link":"https://app.t3.codes/threads/env/%s","worktreePath":%s,"branch":%s}},"messages":[]}}\n' "$3" "$3" "$wt" "$br"
  exit 0
fi
printf '%s\n' "$@" > "$out/start.args"
cat > "$out/start.stdin"
echo '[1111/090522.026791:ERROR:electron/shell/app/node_main.cc:154] Node.js environment variables are disabled because this process is invoked by other apps.' >&2
echo "Thread thread-$$" >&2
sleep 1
if [ "${{FAKE_T3_EXIT:-0}}" = "1" ]; then echo 'The run failed: provider quota exhausted' >&2; exit 1; fi
printf 'Fixed it.\nAll tests pass.\n'
exit "${{FAKE_T3_EXIT:-0}}"
"#,
        out = dir.display()
    );
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn wait_for_finished_job(ticket_id: &str) -> AgentJob {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        let finished = AgentJobService::list_jobs().into_iter().find(|job| {
            job.ticket_id == ticket_id && matches!(job.status.as_str(), "completed" | "failed")
        });
        if let Some(job) = finished {
            // Automation on the job's end runs right after the status change.
            sleep(Duration::from_millis(300));
            return job;
        }
        sleep(Duration::from_millis(50));
    }
    panic!(
        "no finished job for {ticket_id}: {:?}",
        AgentJobService::list_jobs()
    );
}

fn create(fixtures: &TestFixtures, title: &str, model: Option<(&str, &str)>) -> String {
    let mut storage = fixtures.create_storage();
    let custom_fields = model.map(|(provider, model)| {
        let mut fields = lotar::types::CustomFields::new();
        fields.insert(
            "provider".into(),
            lotar::types::custom_value_string(provider),
        );
        fields.insert("model".into(), lotar::types::custom_value_string(model));
        fields
    });
    // Reported as a finding, as the repository scanner does.
    let (outcome, _) = FindingService::report(
        &mut storage,
        TaskCreate {
            title: title.to_string(),
            project: Some("REPO".to_string()),
            reporter: Some("mallox".to_string()),
            custom_fields,
            ..Default::default()
        },
        &format!("test:{title}"),
    )
    .unwrap();
    assert!(outcome.created);
    outcome.id
}

const REPO_CONFIG: &str =
    "default:\n  project: REPO\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\n";

/// The repository only assigns tickets; the profile lives in the home
/// config, so the repository needs no `lotar trust`. Done on job_completed.
const REPO_RULES: &str = "automation:\n  rules:\n    - name: Fix new findings\n      when:\n        custom_fields: { finding: { exists: true } }\n      on:\n        created:\n          set:\n            assignee: \"@fix\"\n          agent:\n            provider: \"${{ticket.field:provider}}\"\n            model: \"${{ticket.field:model}}\"\n    - name: Done when the fix finished\n      when:\n        assignee: \"@fix\"\n      on:\n        job_completed:\n          set:\n            status: Done\n            assignee: \"@reporter\"\n";

struct Home {
    _env: Vec<EnvVarGuard>,
    out: PathBuf,
}

/// A home config with the `fix` profile and an untrusted repository.
fn home_with_fix_profile(fixtures: &TestFixtures, profile_extra: &str) -> Home {
    let home = fixtures.get_temp_path().join("home");
    let out = fixtures.get_temp_path().join("t3-out");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&out).unwrap();
    let t3 = fake_t3(&out);
    fs::write(
        home.join(".lotar"),
        format!(
            "agents:\n  fix:\n    runner: t3\n    command: \"{}\"\n    provider: claudeAgent\n    model: claude-opus-5-5\n{profile_extra}",
            t3.display()
        ),
    )
    .unwrap();
    fs::set_permissions(home.join(".lotar"), fs::Permissions::from_mode(0o600)).unwrap();
    fixtures.create_config_in_dir(&fixtures.tasks_root, REPO_CONFIG);
    fs::write(fixtures.tasks_root.join("automation.yml"), REPO_RULES).unwrap();
    let env = vec![
        EnvVarGuard::set("HOME", home.to_str().unwrap()),
        EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "0"),
        EnvVarGuard::clear("LOTAR_TRUST_ALL"),
        EnvVarGuard::set(
            "LOTAR_TRUST_FILE",
            fixtures.get_temp_path().join("trust.yml").to_str().unwrap(),
        ),
        EnvVarGuard::set("LOTAR_TASKS_DIR", fixtures.tasks_root.to_str().unwrap()),
    ];
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    Home { _env: env, out }
}

fn read_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn home_profile_and_repo_rule_run_a_t3_thread_and_set_done() {
    let fixtures = TestFixtures::new();
    let home = home_with_fix_profile(&fixtures, "");
    assert!(
        !lotar::services::trust_service::TrustService::allows_commands(&fixtures.tasks_root),
        "the repository is not trusted"
    );

    let id = create(&fixtures, "Leftover worktrees", None);
    let job = wait_for_finished_job(&id);
    assert_eq!(job.status, "completed", "{job:?}");
    assert_eq!(job.runner, "t3");
    assert_eq!(job.summary.as_deref(), Some("Fixed it.\nAll tests pass."));

    // The profile's provider and model: the ticket has no fields, so the
    // rule's templates expand to nothing.
    let args = read_lines(&home.out.join("start.args"));
    assert_eq!(
        args,
        [
            "thread",
            "start",
            ".",
            "-",
            "--title",
            &format!("{id} Leftover worktrees"),
            "--provider",
            "claudeAgent",
            "--model",
            "claude-opus-5-5",
            "--wait"
        ]
    );
    let prompt = fs::read_to_string(home.out.join("start.stdin")).unwrap();
    assert!(prompt.contains(&id), "{prompt}");
    let tasks_dir = fs::canonicalize(&fixtures.tasks_root).unwrap();
    assert!(
        prompt.contains(&format!("--tasks-dir \"{}\"", tasks_dir.display())),
        "{prompt}"
    );

    let storage = fixtures.create_storage();
    let task = TaskService::get(&storage, &id, None).unwrap();
    assert_eq!(task.status.as_str(), "Done");
    assert_eq!(task.assignee.as_deref(), Some("mallox"));
    let thread = job.session_id.clone().unwrap();
    let link = format!("https://app.t3.codes/threads/env/{thread}");
    assert!(
        task.references
            .iter()
            .any(|r| r.link.as_deref() == Some(link.as_str())),
        "{:?}",
        task.references
    );
    let events = AgentJobService::events_for(&job.id);
    let messages: Vec<_> = events.iter().filter_map(|e| e.message.as_deref()).collect();
    assert!(
        messages.contains(&format!("T3 Code thread: {link}").as_str()),
        "{messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("electron")),
        "{messages:?}"
    );

    // A ticket can pick its own provider and model.
    let picked = create(&fixtures, "CI is red", Some(("codex", "gpt-5.5")));
    let job = wait_for_finished_job(&picked);
    assert_eq!(job.status, "completed", "{job:?}");
    let args = read_lines(&home.out.join("start.args"));
    let joined = args.join(" ");
    assert!(
        joined.contains("--provider codex --model gpt-5.5"),
        "{joined}"
    );
}

#[test]
fn a_thread_that_waits_for_an_answer_goes_back_to_the_reporter() {
    let fixtures = TestFixtures::new();
    let _home = home_with_fix_profile(&fixtures, "    env:\n      FAKE_T3_EXIT: \"2\"\n");
    let id = create(&fixtures, "Needs a decision", None);
    let job = wait_for_finished_job(&id);
    assert_eq!(job.status, "failed", "{job:?}");
    assert_eq!(job.exit_code, Some(2));
    let thread = job.session_id.clone().unwrap();
    let link = format!("https://app.t3.codes/threads/env/{thread}");
    assert_eq!(
        job.summary.as_deref(),
        Some(format!("The T3 Code thread waits for an answer: {link}").as_str())
    );

    let storage = fixtures.create_storage();
    let task = TaskService::get(&storage, &id, None).unwrap();
    assert_eq!(task.status.as_str(), "HelpNeeded", "not Done");
    assert_eq!(task.assignee.as_deref(), Some("mallox"));
    assert!(
        task.comments.iter().any(|c| c.text.contains(&link)),
        "{:?}",
        task.comments
    );
}

#[test]
fn a_failed_run_fails_the_job_with_t3s_error() {
    let fixtures = TestFixtures::new();
    let _home = home_with_fix_profile(&fixtures, "    env:\n      FAKE_T3_EXIT: \"1\"\n");
    let id = create(&fixtures, "Quota", None);
    let job = wait_for_finished_job(&id);
    assert_eq!(job.status, "failed", "{job:?}");
    assert_eq!(job.exit_code, Some(1));
    assert_eq!(
        job.summary.as_deref(),
        Some("The run failed: provider quota exhausted")
    );
    let storage = fixtures.create_storage();
    let task = TaskService::get(&storage, &id, None).unwrap();
    assert_eq!(task.status.as_str(), "Todo", "the Done rule did not run");
}

#[test]
fn lotar_worktrees_become_t3_worktrees() {
    let fixtures = TestFixtures::new();
    let repo = fixtures.get_temp_path();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(status.status.success(), "git {args:?}: {status:?}");
    };
    fs::write(repo.join(".gitignore"), "home/\nt3-out/\nwt/\n.tasks/\n").unwrap();
    git(&["init", "-q", "-b", "main"]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@example.com",
        "add",
        ".",
    ]);
    git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@example.com",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-qm",
        "init",
    ]);
    let worktree = repo.join("wt");
    let home = home_with_fix_profile(
        &fixtures,
        &format!(
            "    env:\n      FAKE_T3_WORKTREE: \"{}\"\n",
            worktree.display()
        ),
    );
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!("{REPO_CONFIG}agent:\n  worktree:\n    enabled: true\n"),
    );

    let id = create(&fixtures, "In a worktree", None);
    let job = wait_for_finished_job(&id);
    assert_eq!(job.status, "completed", "{job:?}");
    let args = read_lines(&home.out.join("start.args")).join(" ");
    assert!(
        args.contains(&format!("--worktree main --branch agent/{id}")),
        "{args}"
    );
    assert_eq!(job.worktree_path.as_deref(), worktree.to_str());
    assert_eq!(job.worktree_branch.as_deref(), Some("agent/from-t3"));
}
