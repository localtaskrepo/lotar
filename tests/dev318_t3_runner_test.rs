//! DEV-318: the t3 runner starts a T3 Code thread per job (fake `t3` here).
#![cfg(unix)]

use crate::common::TestFixtures;
use crate::common::env_mutex::EnvVarGuard;
use lotar::api_types::{AgentJob, TaskCreate};
use lotar::services::agent_active_jobs;
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
/// `FAKE_T3_WORKTREE` as the thread's worktree, on branch `agent/from-t3`
/// until the run renames it to `FAKE_T3_RENAME`, with the run's status
/// from the `status` file (default `completed`), or fails with
/// `FAKE_T3_GONE`, on claudeAgent's claude-opus-5-5; `thread send` records its arguments, folder and stdin
/// and replies (DEV-368). `thread start` runs while a `hold` file exists.
fn fake_t3(dir: &Path) -> PathBuf {
    let path = dir.join("fake-t3");
    let script = format!(
        r#"#!/bin/sh
out="{out}"
if [ "$1" = "thread" ] && [ "$2" = "show" ]; then
  echo "$3" >> "$out/show.calls"
  if [ -n "$FAKE_T3_GONE" ]; then echo "Thread $3 not found" >&2; exit 1; fi
  if [ -n "$FAKE_T3_WORKTREE" ]; then wt="\"$FAKE_T3_WORKTREE\""; br="\"$(cat "$out/branch" 2>/dev/null || echo agent/from-t3)\""; else wt=null; br='"main"'; fi
  st="$(cat "$out/status" 2>/dev/null || echo completed)"; if [ "$st" = running ]; then run='"run:1"'; else run=null; fi
  printf '{{"thread":{{"threadId":"%s","link":"https://app.t3.codes/threads/env/%s","worktreePath":%s,"branch":%s,"status":"%s","activeRunId":%s,"pendingRequestCount":0,"providerInstanceId":"claudeAgent","model":"claude-opus-5-5"}},"messages":[{{"role":"assistant","text":"Fixed it."}}]}}\n' "$3" "$3" "$wt" "$br" "$st" "$run"
  exit 0
fi
if [ "$1" = "thread" ] && [ "$2" = "send" ]; then
  printf '%s\n' "$@" > "$out/send.args"
  pwd -P > "$out/send.cwd"
  cat > "$out/send.stdin"
  printf 'Merged it.\n'
  exit "${{FAKE_T3_EXIT:-0}}"
fi
printf '%s\n' "$@" > "$out/start.args"
echo start >> "$out/start.calls"
cat > "$out/start.stdin"
echo '[1111/090522.026791:ERROR:electron/shell/app/node_main.cc:154] Node.js environment variables are disabled because this process is invoked by other apps.' >&2
echo "Thread thread-$$" >&2
sleep 1
while [ -f "$out/hold" ]; do sleep 0.1; done
if [ -n "$FAKE_T3_RENAME" ]; then echo "$FAKE_T3_RENAME" > "$out/branch"; fi
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
    let tasks_dir = PathBuf::from(std::env::var("LOTAR_TASKS_DIR").unwrap());
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        let finished = AgentJobService::list_jobs().into_iter().find(|job| job.ticket_id == ticket_id && matches!(job.status.as_str(), "completed" | "failed"));
        // The status changes before the end rules run; the job's active
        // record goes once they have (DEV-356).
        if let Some(job) = finished.filter(|job| agent_active_jobs::find(&tasks_dir, &job.id).is_none()) {
            return job;
        }
        sleep(Duration::from_millis(50));
    }
    panic!("no finished job for {ticket_id}: {:?}", AgentJobService::list_jobs());
}

fn create(fixtures: &TestFixtures, title: &str, model: Option<(&str, &str)>) -> String {
    let mut storage = fixtures.create_storage();
    let custom_fields = model.map(|(provider, model)| {
        let mut fields = lotar::types::CustomFields::new();
        fields.insert("provider".into(), lotar::types::custom_value_string(provider));
        fields.insert("model".into(), lotar::types::custom_value_string(model));
        fields
    });
    // Reported as a finding, as the repository scanner does.
    let (outcome, _) = FindingService::report(
        &mut storage,
        TaskCreate { title: title.to_string(), project: Some("REPO".to_string()), reporter: Some("mallox".to_string()), custom_fields, ..Default::default() },
        &format!("test:{title}"),
    )
    .unwrap();
    assert!(outcome.created);
    outcome.id
}

const REPO_CONFIG: &str = "default:\n  project: REPO\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\n";

/// The repository only assigns tickets; the profile lives in the home
/// config, so the repository needs no `lotar trust`. Done on job_completed.
const REPO_RULES: &str = "automation:\n  rules:\n    - name: Fix new findings\n      when:\n        custom_fields: { finding: { exists: true } }\n      on:\n        created:\n          set:\n            assignee: \"@fix\"\n          agent:\n            provider: \"${{ticket.field:provider}}\"\n            model: \"${{ticket.field:model}}\"\n    - name: Done when the fix finished\n      when:\n        assignee: \"@fix\"\n      on:\n        job_completed:\n          set:\n            status: Done\n            assignee: \"@reporter\"\n";

struct Home {
    _env: Vec<EnvVarGuard>,
    out: PathBuf,
}

/// A home config with the `fix` profile and an untrusted repository.
/// `profile_extra` can add more profiles; `{T3}` in it is the fake `t3`.
fn home_with_fix_profile(fixtures: &TestFixtures, profile_extra: &str) -> Home {
    let home = fixtures.get_temp_path().join("home");
    let out = fixtures.get_temp_path().join("t3-out");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&out).unwrap();
    let t3 = fake_t3(&out);
    let profile_extra = profile_extra.replace("{T3}", t3.to_str().unwrap());
    fs::write(
        home.join(".lotar"),
        format!("agents:\n  fix:\n    runner: t3\n    command: \"{}\"\n    provider: claudeAgent\n    model: claude-opus-5-5\n{profile_extra}", t3.display()),
    )
    .unwrap();
    fs::set_permissions(home.join(".lotar"), fs::Permissions::from_mode(0o600)).unwrap();
    fixtures.create_config_in_dir(&fixtures.tasks_root, REPO_CONFIG);
    fs::write(fixtures.tasks_root.join("automation.yml"), REPO_RULES).unwrap();
    let env = vec![
        EnvVarGuard::set("HOME", home.to_str().unwrap()),
        EnvVarGuard::set("LOTAR_IGNORE_HOME_CONFIG", "0"),
        EnvVarGuard::clear("LOTAR_TRUST_ALL"),
        EnvVarGuard::set("LOTAR_TRUST_FILE", fixtures.get_temp_path().join("trust.yml").to_str().unwrap()),
        EnvVarGuard::set("LOTAR_TASKS_DIR", fixtures.tasks_root.to_str().unwrap()),
    ];
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
    Home { _env: env, out }
}

fn read_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path).unwrap_or_default().lines().map(str::to_string).collect()
}

#[test]
fn home_profile_and_repo_rule_run_a_t3_thread_and_set_done() {
    let fixtures = TestFixtures::new();
    let home = home_with_fix_profile(&fixtures, "");
    assert!(!lotar::services::trust_service::TrustService::allows_commands(&fixtures.tasks_root), "the repository is not trusted");

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
        ["thread", "start", ".", "-", "--title", &format!("{id} Leftover worktrees"), "--provider", "claudeAgent", "--model", "claude-opus-5-5", "--wait"]
    );
    let prompt = fs::read_to_string(home.out.join("start.stdin")).unwrap();
    assert!(prompt.contains(&id), "{prompt}");
    let tasks_dir = fs::canonicalize(&fixtures.tasks_root).unwrap();
    assert!(prompt.contains(&format!("--tasks-dir \"{}\"", tasks_dir.display())), "{prompt}");
    // `--wait` returns at the turn's end, not when awaited background work ends (DEV-335).
    assert!(prompt.contains("The job ends when your turn ends"), "{prompt}");

    let storage = fixtures.create_storage();
    let task = TaskService::get(&storage, &id, None).unwrap();
    assert_eq!(task.status.as_str(), "Done");
    assert_eq!(task.assignee.as_deref(), Some("mallox"));
    let thread = job.session_id.clone().unwrap();
    let link = format!("https://app.t3.codes/threads/env/{thread}");
    assert!(task.references.iter().any(|r| r.link.as_deref() == Some(link.as_str())), "{:?}", task.references);
    let events = AgentJobService::events_for(&job.id);
    let messages: Vec<_> = events.iter().filter_map(|e| e.message.as_deref()).collect();
    assert!(messages.contains(&format!("T3 Code thread: {link}").as_str()), "{messages:?}");
    assert!(!messages.iter().any(|m| m.contains("electron")), "{messages:?}");

    // A ticket can pick its own provider and model.
    let picked = create(&fixtures, "CI is red", Some(("codex", "gpt-5.5")));
    let job = wait_for_finished_job(&picked);
    assert_eq!(job.status, "completed", "{job:?}");
    let args = read_lines(&home.out.join("start.args"));
    let joined = args.join(" ");
    assert!(joined.contains("--provider codex --model gpt-5.5"), "{joined}");
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
    assert_eq!(job.summary.as_deref(), Some(format!("The T3 Code thread waits for an answer: {link}").as_str()));

    let storage = fixtures.create_storage();
    let task = TaskService::get(&storage, &id, None).unwrap();
    assert_eq!(task.status.as_str(), "HelpNeeded", "not Done");
    assert_eq!(task.assignee.as_deref(), Some("mallox"));
    assert!(task.comments.iter().any(|c| c.text.contains(&link)), "{:?}", task.comments);
}

#[test]
fn a_failed_run_fails_the_job_with_t3s_error() {
    let fixtures = TestFixtures::new();
    let _home = home_with_fix_profile(&fixtures, "    env:\n      FAKE_T3_EXIT: \"1\"\n");
    let id = create(&fixtures, "Quota", None);
    let job = wait_for_finished_job(&id);
    assert_eq!(job.status, "failed", "{job:?}");
    assert_eq!(job.exit_code, Some(1));
    assert_eq!(job.summary.as_deref(), Some("The run failed: provider quota exhausted"));
    let failed = AgentJobService::events_for(&job.id).into_iter().find(|event| event.kind == "agent_job_failed").unwrap();
    assert_eq!(failed.message.as_deref(), Some("The t3 runner exited with code 1."), "DEV-401: every failure says why");
    let storage = fixtures.create_storage();
    let task = TaskService::get(&storage, &id, None).unwrap();
    assert_eq!(task.status.as_str(), "Todo", "the Done rule did not run");
}

/// DEV-401: `t3 thread start --wait` interrupted (exit 130) while the
/// thread works on: the job waits for the run instead of failing.
#[test]
fn a_t3_that_exits_while_its_thread_runs_waits_for_the_run() {
    let fixtures = TestFixtures::new();
    let home = home_with_fix_profile(&fixtures, "    env:\n      FAKE_T3_EXIT: \"130\"\n");
    fs::write(home.out.join("status"), "running").unwrap();
    let id = create(&fixtures, "Interrupted", None);
    let start = Instant::now();
    while !messages_for_ticket(&id).iter().any(|m| m.contains("before its thread's run ended")) {
        assert!(start.elapsed() < Duration::from_secs(10), "{:?}", AgentJobService::list_jobs());
        sleep(Duration::from_millis(50));
    }
    sleep(Duration::from_secs(1));
    let running = AgentJobService::list_jobs().into_iter().find(|job| job.ticket_id == id).unwrap();
    assert_eq!(running.status, "running", "waits while the thread runs");
    fs::write(home.out.join("status"), "completed").unwrap();

    let job = wait_for_finished_job(&id);
    assert_eq!(job.status, "completed", "{job:?}: {:?}", messages(&job));
    assert_eq!(job.exit_code, Some(0));
    assert_eq!(job.summary.as_deref(), Some("Fixed it."));
    let task = TaskService::get(&fixtures.create_storage(), &id, None).unwrap();
    assert_eq!(task.status.as_str(), "Done", "the completed rules ran, not the failed ones");
}

fn messages_for_ticket(ticket_id: &str) -> Vec<String> {
    AgentJobService::list_jobs().iter().filter(|job| job.ticket_id == ticket_id).flat_map(messages).collect()
}

#[test]
fn lotar_worktrees_become_t3_worktrees() {
    let fixtures = TestFixtures::new();
    let repo = fixtures.get_temp_path();
    let git = |args: &[&str]| {
        let status = Command::new("git").args(args).current_dir(repo).env("GIT_CONFIG_GLOBAL", "/dev/null").output().unwrap();
        assert!(status.status.success(), "git {args:?}: {status:?}");
    };
    fs::write(repo.join(".gitignore"), "home/\nt3-out/\nwt/\n.tasks/\n").unwrap();
    git(&["init", "-q", "-b", "main"]);
    git(&["-c", "user.name=T", "-c", "user.email=t@example.com", "add", "."]);
    git(&["-c", "user.name=T", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", "commit", "-qm", "init"]);
    let worktree = repo.join("wt");
    let home =
        home_with_fix_profile(&fixtures, &format!("    env:\n      FAKE_T3_WORKTREE: \"{}\"\n      FAKE_T3_RENAME: t3/in-a-worktree\n", worktree.display()));
    fixtures.create_config_in_dir(&fixtures.tasks_root, &format!("{REPO_CONFIG}agent:\n  worktree:\n    enabled: true\n"));

    let id = create(&fixtures, "In a worktree", None);
    let job = wait_for_finished_job(&id);
    assert_eq!(job.status, "completed", "{job:?}");
    let args = read_lines(&home.out.join("start.args")).join(" ");
    assert!(args.contains(&format!("--worktree main --branch agent/{id}")), "{args}");
    assert_eq!(job.worktree_path.as_deref(), worktree.to_str());
    // T3 Code renamed the thread's branch while it worked (DEV-342).
    assert_eq!(job.worktree_branch.as_deref(), Some("t3/in-a-worktree"));
}

/// Rules that hand a finding to `fix`, then to `follow_up` in the fix
/// thread (`session: continue`), then set Done (DEV-368).
fn home_with_follow_up(fixtures: &TestFixtures, fix_extra: &str, follow_up: &str, follow_up_extra: &str) -> Home {
    let profile = format!("  {follow_up}:\n    runner: t3\n    command: \"{{T3}}\"\n    provider: codex\n    model: gpt-5.5\n{follow_up_extra}");
    let home = home_with_fix_profile(fixtures, &format!("{fix_extra}{profile}"));
    let rules = format!(
        "automation:\n  rules:\n    - name: Fix new findings\n      when:\n        custom_fields: {{ finding: {{ exists: true }} }}\n      on:\n        created:\n          set:\n            assignee: \"@fix\"\n    - name: Follow up in the fix thread\n      when:\n        assignee: \"@fix\"\n      on:\n        job_completed:\n          set:\n            assignee: \"@{follow_up}\"\n          agent:\n            session: continue\n    - name: Done after the follow-up\n      when:\n        assignee: \"@{follow_up}\"\n      on:\n        job_completed:\n          set:\n            status: Done\n            assignee: \"@reporter\"\n"
    );
    fs::write(fixtures.tasks_root.join("automation.yml"), rules).unwrap();
    home
}

fn wait_for_finished_agent_job(ticket_id: &str, agent: &str) -> AgentJob {
    let tasks_dir = PathBuf::from(std::env::var("LOTAR_TASKS_DIR").unwrap());
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        let finished = AgentJobService::list_jobs()
            .into_iter()
            .find(|job| job.ticket_id == ticket_id && job.agent.as_deref() == Some(agent) && matches!(job.status.as_str(), "completed" | "failed"));
        if let Some(job) = finished.filter(|job| agent_active_jobs::find(&tasks_dir, &job.id).is_none()) {
            return job;
        }
        sleep(Duration::from_millis(50));
    }
    panic!("no finished {agent} job for {ticket_id}: {:?}", AgentJobService::list_jobs());
}

fn messages(job: &AgentJob) -> Vec<String> {
    AgentJobService::events_for(&job.id).into_iter().filter_map(|event| event.message).collect()
}

#[test]
fn a_merge_job_runs_as_the_next_turn_of_the_fix_thread_in_its_worktree() {
    let fixtures = TestFixtures::new();
    let repo = fixtures.get_temp_path().to_path_buf();
    let git = |dir: &Path, args: &[&str]| {
        let output = Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
    };
    fs::write(repo.join(".gitignore"), "home/\nt3-out/\nwt/\n.tasks/\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "init"]);
    // The worktree T3 Code made for the fix thread, with the fix committed.
    let worktree = repo.join("wt");
    git(&repo, &["worktree", "add", "-q", "-b", "agent/from-t3", worktree.to_str().unwrap()]);
    fs::write(worktree.join("fix.txt"), "fixed\n").unwrap();
    git(&worktree, &["add", "fix.txt"]);
    git(&worktree, &["commit", "-qm", "fix"]);

    let env = format!("    env:\n      FAKE_T3_WORKTREE: \"{}\"\n", worktree.display());
    let home = home_with_follow_up(&fixtures, &env, "merge", &env);
    fixtures.create_config_in_dir(&fixtures.tasks_root, &format!("{REPO_CONFIG}agent:\n  worktree:\n    enabled: true\n"));

    let id = create(&fixtures, "Merge in the fix thread", None);
    let fix = wait_for_finished_agent_job(&id, "fix");
    assert_eq!(fix.status, "completed", "{fix:?}");
    let merge = wait_for_finished_agent_job(&id, "merge");
    assert_eq!(merge.status, "completed", "{merge:?}: {:?}", messages(&merge));

    // One thread: the merge job is its next turn, in its worktree.
    assert_eq!(read_lines(&home.out.join("start.calls")).len(), 1, "no second thread");
    let thread = fix.session_id.clone().unwrap();
    assert_eq!(read_lines(&home.out.join("send.args")), ["thread", "send", thread.as_str(), "-", "--wait"]);
    assert_eq!(read_lines(&home.out.join("send.cwd")), [fs::canonicalize(&worktree).unwrap().to_str().unwrap()]);
    assert!(fs::read_to_string(home.out.join("send.stdin")).unwrap().contains(&id));
    assert_eq!(merge.session_id.as_deref(), Some(thread.as_str()));
    assert_eq!(merge.worktree_path.as_deref(), worktree.to_str());
    assert_eq!(merge.worktree_branch.as_deref(), Some("agent/from-t3"));
    assert_eq!(merge.summary.as_deref(), Some("Merged it."));

    let messages = messages(&merge);
    let link = format!("https://app.t3.codes/threads/env/{thread}");
    assert!(messages.contains(&format!("Continuing the T3 Code thread: {link}")), "{messages:?}");
    assert!(messages.iter().any(|m| m.contains("keeps its model")), "{messages:?}");
    // The merge-job check: the branch is in the main checkout.
    assert!(messages.contains(&"Fast-forwarded the main checkout to agent/from-t3".to_string()), "{messages:?}");
    assert!(repo.join("fix.txt").exists());
    let task = TaskService::get(&fixtures.create_storage(), &id, None).unwrap();
    assert_eq!(task.status.as_str(), "Done");
}

#[test]
fn a_follow_up_without_an_earlier_thread_starts_a_new_one() {
    let fixtures = TestFixtures::new();
    let home = home_with_fix_profile(&fixtures, "    session: continue\n");
    let id = create(&fixtures, "First job", None);
    let job = wait_for_finished_agent_job(&id, "fix");
    assert_eq!(job.status, "completed", "{job:?}");
    assert_eq!(read_lines(&home.out.join("start.calls")).len(), 1);
    assert!(!home.out.join("send.args").exists());
    let reason = format!("No earlier T3 Code thread on {id}; starting a new thread.");
    assert!(messages(&job).contains(&reason), "{:?}", messages(&job));
    let log = fs::read_to_string(fixtures.tasks_root.join("@agent-logs").join(format!("{}.jsonl", job.id))).unwrap();
    assert!(log.contains(&reason), "{log}");
}

#[test]
fn a_follow_up_on_a_gone_thread_starts_a_new_one() {
    let fixtures = TestFixtures::new();
    let home = home_with_follow_up(&fixtures, "", "follow", "    env:\n      FAKE_T3_GONE: \"1\"\n");
    let id = create(&fixtures, "Gone thread", None);
    let fix = wait_for_finished_agent_job(&id, "fix");
    let follow = wait_for_finished_agent_job(&id, "follow");
    assert_eq!(follow.status, "completed", "{follow:?}");
    assert_eq!(read_lines(&home.out.join("start.calls")).len(), 2);
    assert!(!home.out.join("send.args").exists());
    let thread = fix.session_id.unwrap();
    let gone = format!("The T3 Code thread {thread} is gone; starting a new thread.");
    assert!(messages(&follow).contains(&gone), "{:?}", messages(&follow));
    assert_ne!(follow.session_id.as_deref(), Some(thread.as_str()));
}

#[test]
fn a_follow_up_finds_the_thread_in_the_job_logs() {
    let fixtures = TestFixtures::new();
    let home = home_with_fix_profile(&fixtures, "    session: continue\n");
    // An earlier job of another process: only its log is left.
    let logs = fixtures.tasks_root.join("@agent-logs");
    fs::create_dir_all(&logs).unwrap();
    let log = concat!(
        r#"{"type":"header","job_id":"job-20260101T000000.000Z-0","ticket_id":"REPO-1","runner":"t3","created_at":"2026-01-01T00:00:00+00:00"}"#,
        "\n",
        r#"{"type":"status","status":"completed","at":"2026-01-01T00:10:00+00:00","session_id":"thread-old"}"#,
        "\n"
    );
    fs::write(logs.join("job-20260101T000000.000Z-0.jsonl"), log).unwrap();

    let id = create(&fixtures, "Logged thread", None);
    assert_eq!(id, "REPO-1");
    let job = wait_for_finished_agent_job(&id, "fix");
    assert_eq!(job.status, "completed", "{job:?}: {:?}", messages(&job));
    assert!(!home.out.join("start.calls").exists(), "no new thread");
    assert_eq!(read_lines(&home.out.join("send.args")), ["thread", "send", "thread-old", "-", "--wait"]);
    assert_eq!(job.session_id.as_deref(), Some("thread-old"));
}

fn wait_for_job(job_id: &str) -> AgentJob {
    let tasks_dir = PathBuf::from(std::env::var("LOTAR_TASKS_DIR").unwrap());
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        let job = AgentJobService::get_job(job_id).filter(|job| matches!(job.status.as_str(), "completed" | "failed"));
        if let Some(job) = job.filter(|job| agent_active_jobs::find(&tasks_dir, &job.id).is_none()) {
            return job;
        }
        sleep(Duration::from_millis(50));
    }
    panic!("job {job_id} did not finish: {:?}", AgentJobService::get_job(job_id));
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository whose first thread's worktree `wt` holds a fix on
/// `agent/from-t3` that main doesn't have yet, and a ticket the `fix`
/// profile worked on there. `profile_extra` is as for
/// `home_with_fix_profile`; `{WT}` in it is the worktree.
fn ticket_with_unmerged_work(fixtures: &TestFixtures, profile_extra: &str) -> (Home, PathBuf, String) {
    let repo = fixtures.get_temp_path().to_path_buf();
    fs::write(repo.join(".gitignore"), "home/\nt3-out/\nwt/\n.tasks/\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "init"]);
    let worktree = repo.join("wt");
    git(&repo, &["worktree", "add", "-q", "-b", "agent/from-t3", worktree.to_str().unwrap()]);
    fs::write(worktree.join("fix.txt"), "fixed\n").unwrap();
    git(&worktree, &["add", "fix.txt"]);
    git(&worktree, &["commit", "-qm", "fix"]);

    let profile_extra = format!("    env:\n      FAKE_T3_WORKTREE: \"{{WT}}\"\n{profile_extra}").replace("{WT}", worktree.to_str().unwrap());
    let home = home_with_fix_profile(fixtures, &profile_extra);
    fixtures.create_config_in_dir(&fixtures.tasks_root, &format!("{REPO_CONFIG}agent:\n  worktree:\n    enabled: true\n"));
    // Only the assignment: the ticket stays open between jobs, as after a failed merge.
    let rules = "automation:\n  rules:\n    - name: Fix new findings\n      when:\n        custom_fields: { finding: { exists: true } }\n      on:\n        created:\n          set:\n            assignee: \"@fix\"\n";
    fs::write(fixtures.tasks_root.join("automation.yml"), rules).unwrap();

    let id = create(fixtures, "Retried fix", None);
    let first = wait_for_finished_agent_job(&id, "fix");
    assert_eq!(first.status, "completed", "{first:?}");
    (home, worktree, id)
}

fn run_agent(fixtures: &TestFixtures, id: &str, agent: &str, prompt: &str) -> AgentJob {
    let req =
        lotar::api_types::AgentJobCreateRequest { ticket_id: id.to_string(), prompt: prompt.to_string(), agent: Some(agent.into()), ..Default::default() };
    wait_for_job(&AgentJobService::start_job_with_tasks_dir(req, &fixtures.tasks_root).unwrap().id)
}

#[test]
fn a_second_job_continues_the_thread_with_unmerged_work_instead_of_a_new_one() {
    let fixtures = TestFixtures::new();
    let repo = fixtures.get_temp_path().to_path_buf();
    let (home, _, id) = ticket_with_unmerged_work(&fixtures, "");
    let thread = AgentJobService::list_jobs().into_iter().find(|job| job.ticket_id == id).and_then(|job| job.session_id).unwrap();
    let retry = |prompt: &str| run_agent(&fixtures, &id, "fix", prompt);

    // A retry with the same profile while the branch is unmerged: the same thread and branch.
    let second = retry("Try again");
    assert_eq!(second.status, "completed", "{second:?}: {:?}", messages(&second));
    assert_eq!(read_lines(&home.out.join("start.calls")).len(), 1, "no second thread");
    assert_eq!(read_lines(&home.out.join("send.args")), ["thread", "send", thread.as_str(), "-", "--wait"]);
    assert_eq!(second.session_id.as_deref(), Some(thread.as_str()));
    assert_eq!(second.worktree_branch.as_deref(), Some("agent/from-t3"));
    let link = format!("https://app.t3.codes/threads/env/{thread}");
    let reason =
        format!("The ticket's earlier T3 Code thread {link} has unmerged work on agent/from-t3; this job continues it instead of starting a new thread.");
    assert!(messages(&second).contains(&reason), "{:?}", messages(&second));
    let log = fs::read_to_string(fixtures.tasks_root.join("@agent-logs").join(format!("{}.jsonl", second.id))).unwrap();
    assert!(log.contains(&reason), "{log}");

    // Once the fix landed, the next job starts its own thread.
    git(&repo, &["merge", "-q", "--ff-only", "agent/from-t3"]);
    let third = retry("Something new");
    assert_eq!(third.status, "completed", "{third:?}");
    assert_eq!(read_lines(&home.out.join("start.calls")).len(), 2);
    assert!(!messages(&third).iter().any(|m| m.contains("unmerged work")), "{:?}", messages(&third));
}

#[test]
fn a_job_on_another_model_starts_a_new_thread_on_the_unmerged_branch() {
    let fixtures = TestFixtures::new();
    let codex = "  fix-codex:\n    runner: t3\n    command: \"{T3}\"\n    provider: codex\n    model: gpt-5.5\n    env:\n      FAKE_T3_WORKTREE: \"{WT}\"\n";
    let (home, worktree, id) = ticket_with_unmerged_work(&fixtures, codex);
    let earlier = AgentJobService::list_jobs().into_iter().find(|job| job.ticket_id == id).and_then(|job| job.session_id).unwrap();
    // The failed job left work uncommitted.
    fs::write(worktree.join("half.txt"), "half done\n").unwrap();

    let retry = run_agent(&fixtures, &id, "fix-codex", "Try again");
    assert_eq!(retry.status, "completed", "{retry:?}: {:?}", messages(&retry));
    assert_eq!(read_lines(&home.out.join("start.calls")).len(), 2, "a new thread");
    assert!(!home.out.join("send.args").exists(), "the earlier thread isn't continued");
    let args = read_lines(&home.out.join("start.args"));
    let pairs: Vec<_> = args.windows(2).map(|pair| (pair[0].as_str(), pair[1].as_str())).collect();
    for expected in [("--provider", "codex"), ("--model", "gpt-5.5"), ("--worktree", "agent/from-t3")] {
        assert!(pairs.contains(&expected), "{expected:?} in {args:?}");
    }
    assert!(!args.contains(&"--branch".to_string()), "{args:?}");
    let link = format!("https://app.t3.codes/threads/env/{earlier}");
    let stdin = fs::read_to_string(home.out.join("start.stdin")).unwrap();
    assert!(stdin.contains(&format!("The ticket's earlier T3 Code thread {link} worked on branch `agent/from-t3`")), "{stdin}");
    assert_ne!(retry.session_id.as_deref(), Some(earlier.as_str()));
    let reason = format!(
        "The ticket's earlier T3 Code thread {link} has unmerged work on agent/from-t3 and runs claudeAgent/claude-opus-5-5, not this job's provider and model; this job starts a new thread on agent/from-t3. Committed what the earlier thread left uncommitted: {id}: uncommitted changes from the earlier thread"
    );
    assert!(messages(&retry).contains(&reason), "{:?}", messages(&retry));

    // The earlier work is on the branch, and its worktree no longer holds it.
    assert_eq!(git(&worktree, &["status", "--porcelain"]), "");
    assert!(git(&worktree, &["ls-tree", "--name-only", "agent/from-t3"]).lines().any(|file| file == "half.txt"));
    let output = Command::new("git").args(["symbolic-ref", "-q", "HEAD"]).current_dir(&worktree).output().unwrap();
    assert!(!output.status.success(), "the earlier worktree is detached");
}

#[test]
fn a_fresh_session_starts_a_new_thread_on_the_unmerged_branch_on_the_same_model() {
    let fixtures = TestFixtures::new();
    let review =
        "  review:\n    runner: t3\n    command: \"{T3}\"\n    provider: claudeAgent\n    model: claude-opus-5-5\n    env:\n      FAKE_T3_WORKTREE: \"{WT}\"\n";
    let (home, worktree, id) = ticket_with_unmerged_work(&fixtures, review);
    let earlier = AgentJobService::list_jobs().into_iter().find(|job| job.ticket_id == id).and_then(|job| job.session_id).unwrap();
    // The fix's reviewer runs on the fix's provider and model, in a thread of its own.
    let rules = "automation:\n  rules:\n    - name: Review the fix\n      when:\n        assignee: \"@fix\"\n      on:\n        job_completed:\n          set:\n            assignee: \"@review\"\n          agent:\n            session: fresh\n";
    fs::write(fixtures.tasks_root.join("automation.yml"), rules).unwrap();

    // session: new (the default) still continues the thread with unmerged work.
    let fix = run_agent(&fixtures, &id, "fix", "Try again");
    assert_eq!(fix.status, "completed", "{fix:?}: {:?}", messages(&fix));
    assert_eq!(fix.session_id.as_deref(), Some(earlier.as_str()));
    let review = wait_for_finished_agent_job(&id, "review");
    assert_eq!(review.status, "completed", "{review:?}: {:?}", messages(&review));

    assert_eq!(read_lines(&home.out.join("start.calls")).len(), 2, "a new thread for the review");
    let args = read_lines(&home.out.join("start.args"));
    let pairs: Vec<_> = args.windows(2).map(|pair| (pair[0].as_str(), pair[1].as_str())).collect();
    assert!(pairs.contains(&("--worktree", "agent/from-t3")), "{args:?}");
    assert!(!args.contains(&"--branch".to_string()), "{args:?}");
    assert_ne!(review.session_id.as_deref(), Some(earlier.as_str()));
    let link = format!("https://app.t3.codes/threads/env/{earlier}");
    let stdin = fs::read_to_string(home.out.join("start.stdin")).unwrap();
    assert!(stdin.contains(&format!("The ticket's earlier T3 Code thread {link} worked on branch `agent/from-t3`")), "{stdin}");
    let reason = format!(
        "The ticket's earlier T3 Code thread {link} has unmerged work on agent/from-t3 and the job asks for a fresh thread (session: fresh); this job starts a new thread on agent/from-t3."
    );
    assert!(messages(&review).contains(&reason), "{:?}", messages(&review));
    let output = Command::new("git").args(["symbolic-ref", "-q", "HEAD"]).current_dir(&worktree).output().unwrap();
    assert!(!output.status.success(), "the earlier worktree is detached");
}

/// A job whose process died while its thread ran (DEV-378): a pid that no
/// longer exists owns its record, and its log has no end.
fn orphaned_t3_job(fixtures: &TestFixtures, ticket_id: &str, job_id: &str) {
    let mut gone = Command::new("true").spawn().unwrap();
    let owner_pid = gone.id();
    gone.wait().unwrap();
    let job = agent_active_jobs::ActiveJob {
        job_id: job_id.into(),
        ticket_id: ticket_id.into(),
        runner: "t3".into(),
        agent: Some("fix".into()),
        owner_pid,
        child_pid: owner_pid,
        started_at: "2026-11-12T18:36:42+00:00".into(),
        session_id: Some("thread-orphan".into()),
    };
    agent_active_jobs::register(&fixtures.tasks_root, &job);
    let logs = fixtures.tasks_root.join("@agent-logs");
    fs::create_dir_all(&logs).unwrap();
    let log = format!(
        "{}\n{}\n",
        format_args!(r#"{{"type":"header","job_id":"{job_id}","ticket_id":"{ticket_id}","runner":"t3","created_at":"2026-11-12T18:36:42+00:00"}}"#),
        r#"{"type":"event","kind":"agent_job_started","at":"2026-11-12T18:36:42+00:00"}"#
    );
    fs::write(logs.join(format!("{job_id}.jsonl")), log).unwrap();
}

const END_RULES: &str = "automation:\n  rules:\n    - name: Ends\n      on:\n        job_completed:\n          set: { status: Done }\n          comment: \"completed ${{agent.job_id}}\"\n        job_failed:\n          set: { status: HelpNeeded }\n";

#[test]
fn a_job_whose_worker_died_ends_from_its_thread_on_the_next_start() {
    let fixtures = TestFixtures::new();
    let home = home_with_fix_profile(&fixtures, "");
    let _queue = EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", fixtures.get_temp_path().join("queue").to_str().unwrap());
    fs::write(fixtures.tasks_root.join("automation.yml"), END_RULES).unwrap();
    let mut storage = fixtures.create_storage();
    let id = TaskService::create(&mut storage, TaskCreate { title: "Orphan".into(), project: Some("REPO".into()), ..Default::default() }).unwrap().id;
    let job_id = "job-20261112T183642.565Z-1";
    orphaned_t3_job(&fixtures, &id, job_id);
    // A status set while the job ran is still its outcome.
    let tasks_dir = fixtures.tasks_root.clone();
    assert!(agent_active_jobs::list(&tasks_dir).is_empty(), "a dead process's job is not running");
    agent_active_jobs::record_outcome(
        &tasks_dir,
        &lotar::api_types::JobOutcome { job_id: job_id.into(), assignee: Some("mallox".into()), ..Default::default() },
    )
    .unwrap();
    fs::write(home.out.join("status"), "running").unwrap();

    AgentJobService::recover_orphaned_jobs(&tasks_dir);
    AgentJobService::recover_orphaned_jobs(&tasks_dir);
    let running = AgentJobService::get_job(job_id).expect("taken over");
    assert_eq!(running.status, "running");
    assert!(agent_active_jobs::find(&tasks_dir, job_id).is_some_and(|job| job.owner_pid == std::process::id()));
    sleep(Duration::from_secs(1));
    assert_eq!(AgentJobService::get_job(job_id).unwrap().status, "running", "waits while the thread runs");
    fs::write(home.out.join("status"), "completed").unwrap();

    let job = wait_for_finished_job(&id);
    assert_eq!(job.id, job_id);
    assert_eq!(job.status, "completed", "{job:?}: {:?}", messages(&job));
    assert_eq!(job.summary.as_deref(), Some("Fixed it."));
    let task = TaskService::get(&fixtures.create_storage(), &id, None).unwrap();
    assert_eq!(task.status.as_str(), "Done");
    assert_eq!(task.assignee.as_deref(), Some("mallox"), "the recorded outcome applies");
    assert_eq!(task.comments.iter().filter(|c| c.text.contains(job_id)).count(), 1, "end rules ran once: {:?}", task.comments);
    let status = lotar::services::agent_log_service::AgentLogService::load_status(
        fixtures.tasks_root.parent().unwrap(),
        fixtures.tasks_root.join("@agent-logs").to_str().unwrap(),
        job_id,
    )
    .unwrap()
    .expect("the log has its end");
    assert_eq!((status.status.as_str(), status.session_id.as_deref()), ("completed", Some("thread-orphan")));
    assert!(messages(&job).iter().any(|m| m.contains("exited before it ended")), "{:?}", messages(&job));

    AgentJobService::recover_orphaned_jobs(&tasks_dir);
    assert!(agent_active_jobs::orphaned(&tasks_dir).is_empty());
}

#[test]
fn a_job_whose_thread_is_unknown_fails_with_the_reason() {
    let fixtures = TestFixtures::new();
    let _home = home_with_fix_profile(&fixtures, "");
    let _queue = EnvVarGuard::set("LOTAR_AGENT_QUEUE_DIR", fixtures.get_temp_path().join("queue").to_str().unwrap());
    fs::write(fixtures.tasks_root.join("automation.yml"), END_RULES).unwrap();
    let mut storage = fixtures.create_storage();
    let id = TaskService::create(&mut storage, TaskCreate { title: "Orphan".into(), project: Some("REPO".into()), ..Default::default() }).unwrap().id;
    orphaned_t3_job(&fixtures, &id, "job-20261112T183643.341Z-2");
    let _gone = EnvVarGuard::set("FAKE_T3_GONE", "1");

    AgentJobService::recover_orphaned_jobs(&fixtures.tasks_root);
    let job = wait_for_finished_job(&id);
    assert_eq!(job.status, "failed");
    assert!(job.summary.as_deref().is_some_and(|s| s.contains("Could not read the job's T3 Code thread thread-orphan")), "{job:?}");
    let task = TaskService::get(&fixtures.create_storage(), &id, None).unwrap();
    assert_eq!(task.status.as_str(), "HelpNeeded");
}

/// DEV-384: the owner's comments reach the ticket's agent through a
/// `tell_agent` rule; other people's don't.
const TELL_RULE: &str = "    - name: The owner's comments reach the agent\n      when:\n        comment: { author: mallox }\n      on:\n        commented:\n          tell_agent:\n            message: \"${{comment.author}} commented: ${{comment.text}}\"\n            agent: fix\n";

fn comment_as(fixtures: &TestFixtures, id: &str, author: &str, text: &str) {
    let mut storage = fixtures.create_storage();
    let context = lotar::services::task_service::CommentContext { actor_override: Some(author.to_string()), fire_commented: true };
    TaskService::add_comment_with_context(&mut storage, id, text, context).unwrap();
}

fn finished_jobs(ticket_id: &str, count: usize) -> Vec<AgentJob> {
    let tasks_dir = PathBuf::from(std::env::var("LOTAR_TASKS_DIR").unwrap());
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        let jobs: Vec<_> = AgentJobService::list_jobs()
            .into_iter()
            .filter(|job| job.ticket_id == ticket_id && matches!(job.status.as_str(), "completed" | "failed"))
            .filter(|job| agent_active_jobs::find(&tasks_dir, &job.id).is_none())
            .collect();
        if jobs.len() >= count {
            return jobs;
        }
        sleep(Duration::from_millis(50));
    }
    panic!("not {count} finished jobs for {ticket_id}: {:?}", AgentJobService::list_jobs());
}

#[test]
fn the_owners_comment_reaches_the_running_job_as_its_next_turn() {
    let fixtures = TestFixtures::new();
    let home = home_with_fix_profile(&fixtures, "");
    fs::write(fixtures.tasks_root.join("automation.yml"), format!("{REPO_RULES}{TELL_RULE}")).unwrap();
    fs::write(home.out.join("hold"), "").unwrap();

    let id = create(&fixtures, "Flaky smoke test", None);
    let tasks_dir = fixtures.tasks_root.clone();
    let start = Instant::now();
    while agent_active_jobs::for_ticket(&tasks_dir, &id).and_then(|job| job.session_id).is_none() {
        assert!(start.elapsed() < Duration::from_secs(20), "the job never got its thread");
        sleep(Duration::from_millis(50));
    }
    comment_as(&fixtures, &id, "claude", "claude: still working");
    comment_as(&fixtures, &id, "mallox", "Please also check the retry count.");
    fs::remove_file(home.out.join("hold")).unwrap();

    let jobs = finished_jobs(&id, 1);
    assert_eq!(jobs.len(), 1, "the comment runs in the same job: {jobs:?}");
    let job = &jobs[0];
    assert_eq!(job.status, "completed", "{job:?}: {:?}", messages(job));
    let thread = job.session_id.clone().unwrap();
    assert_eq!(read_lines(&home.out.join("send.args")), ["thread", "send", thread.as_str(), "-", "--wait"]);
    assert_eq!(fs::read_to_string(home.out.join("send.stdin")).unwrap(), "mallox commented: Please also check the retry count.");
    assert_eq!(job.summary.as_deref(), Some("Merged it."), "the queued turn's reply");
    assert!(messages(job).contains(&"mallox commented: Please also check the retry count.".to_string()));
    // The end rules ran after the queued turn.
    let task = TaskService::get(&fixtures.create_storage(), &id, None).unwrap();
    assert_eq!(task.status.as_str(), "Done");
}

#[test]
fn the_owners_comment_on_an_idle_ticket_continues_its_agents_thread() {
    let fixtures = TestFixtures::new();
    let home = home_with_fix_profile(&fixtures, "");
    fs::write(fixtures.tasks_root.join("automation.yml"), format!("{REPO_RULES}{TELL_RULE}")).unwrap();
    let id = create(&fixtures, "Old warning", None);
    let first = wait_for_finished_job(&id);
    assert_eq!(first.status, "completed", "{first:?}");
    let task = TaskService::get(&fixtures.create_storage(), &id, None).unwrap();
    assert_eq!(task.assignee.as_deref(), Some("mallox"), "handed back");

    comment_as(&fixtures, &id, "someone", "Not the owner.");
    sleep(Duration::from_millis(300));
    assert_eq!(finished_jobs(&id, 1).len(), 1, "only the owner's comments start a job");
    assert!(agent_active_jobs::for_ticket(&fixtures.tasks_root, &id).is_none());

    comment_as(&fixtures, &id, "mallox", "Why did you skip the docs?");
    let jobs = finished_jobs(&id, 2);
    let second = jobs.iter().find(|job| job.id != first.id).unwrap();
    assert_eq!(second.status, "completed", "{second:?}: {:?}", messages(second));
    assert_eq!(second.agent.as_deref(), Some("fix"), "the ticket's last agent");
    let thread = first.session_id.clone().unwrap();
    assert_eq!(second.session_id.as_deref(), Some(thread.as_str()), "the next turn of its thread");
    assert_eq!(read_lines(&home.out.join("start.calls")).len(), 1, "no second thread");
    assert!(fs::read_to_string(home.out.join("send.stdin")).unwrap().contains("mallox commented: Why did you skip the docs?"));
}

#[test]
fn the_owners_comment_on_a_ticket_without_an_agent_starts_the_rules_agent() {
    let fixtures = TestFixtures::new();
    let home = home_with_fix_profile(&fixtures, "");
    fs::write(fixtures.tasks_root.join("automation.yml"), format!("{REPO_RULES}{TELL_RULE}")).unwrap();
    let mut storage = fixtures.create_storage();
    let task = TaskService::create(&mut storage, TaskCreate { title: "An idea".into(), project: Some("REPO".into()), ..Default::default() }).unwrap();
    assert!(task.assignee.is_none());

    comment_as(&fixtures, &task.id, "mallox", "Could we try this?");
    let job = wait_for_finished_job(&task.id);
    assert_eq!(job.status, "completed", "{job:?}: {:?}", messages(&job));
    assert_eq!(job.agent.as_deref(), Some("fix"));
    assert!(fs::read_to_string(home.out.join("start.stdin")).unwrap().contains("mallox commented: Could we try this?"));
}
