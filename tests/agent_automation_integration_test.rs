#![cfg(unix)]

mod common;

use common::TestFixtures;
use lotar::api_types::{AgentJobCreateRequest, TaskCreate, TaskUpdate};
use lotar::services::agent_job_service::{AgentJobService, AgentOrchestratorMode};
use lotar::services::automation_service::{
    AutomationEvent, AutomationJobContext, AutomationService,
};
use lotar::services::sprint_service::SprintService;
use lotar::services::task_service::{TaskService, TaskUpdateContext};
use lotar::storage::sprint::{Sprint, SprintActual};
use lotar::types::TaskStatus;
use serde_yaml::{Mapping as YamlMapping, Value as YamlValue};
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::sync::{LazyLock, Mutex, MutexGuard};
#[cfg(unix)]
use std::thread::sleep;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
fn write_stub_agent_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).expect("write stub agent script");
    let mut perms = fs::metadata(&path)
        .expect("stat stub agent script")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub agent script");
    path
}

#[cfg(unix)]
static AGENT_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[cfg(unix)]
fn lock_agent_tests() -> MutexGuard<'static, ()> {
    AGENT_TEST_LOCK.lock().expect("lock agent tests")
}

fn enable_server_mode() {
    AgentJobService::set_orchestrator_mode(AgentOrchestratorMode::Server);
}

#[cfg(unix)]
fn init_git_repository(root: &Path) {
    fs::write(root.join("README.md"), "test repo\n").expect("write repo file");

    let status = Command::new("git")
        .arg("init")
        .current_dir(root)
        .status()
        .expect("git init");
    assert!(status.success(), "git init should succeed");

    for args in [
        &["config", "user.name", "Agent Tests"][..],
        &["config", "user.email", "agent-tests@example.com"][..],
        &["config", "commit.gpgsign", "false"][..],
    ] {
        let status = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git config");
        assert!(status.success(), "git config should succeed: {:?}", args);
    }

    let add_status = Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .status()
        .expect("git add");
    assert!(add_status.success(), "git add should succeed");

    let commit_status = Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(root)
        .status()
        .expect("git commit");
    assert!(commit_status.success(), "git commit should succeed");
}

#[cfg(unix)]
const AGENT_PIPELINE_TEMPLATE: &str = include_str!("../src/config/templates/agent-pipeline.yml");

#[cfg(unix)]
const AGENT_REVIEWED_TEMPLATE: &str = include_str!("../src/config/templates/agent-reviewed.yml");

#[cfg(unix)]
fn template_config_and_automation(template: &str, project_name: &str) -> (String, String) {
    let template_doc: YamlValue = serde_yaml::from_str(template).expect("parse template yaml");
    let template_map = template_doc
        .as_mapping()
        .expect("template should be a mapping");

    let mut config = template_map
        .get(YamlValue::String("config".to_string()))
        .cloned()
        .expect("template config section");
    if let Some(config_map) = config.as_mapping_mut()
        && let Some(project_value) = config_map.get_mut(YamlValue::String("project".to_string()))
        && let Some(project_map) = project_value.as_mapping_mut()
    {
        project_map.insert(
            YamlValue::String("name".to_string()),
            YamlValue::String(project_name.to_string()),
        );
    }

    let automation = template_map
        .get(YamlValue::String("automation".to_string()))
        .cloned()
        .expect("template automation section");

    let config_yaml = serde_yaml::to_string(&config).expect("serialize config section");
    let mut automation_root = YamlMapping::new();
    automation_root.insert(YamlValue::String("automation".to_string()), automation);
    let automation_yaml = serde_yaml::to_string(&YamlValue::Mapping(automation_root))
        .expect("serialize automation section");

    (config_yaml, automation_yaml)
}

#[cfg(unix)]
fn install_template_workflow(
    fixtures: &TestFixtures,
    prefix: &str,
    project_name: &str,
    template: &str,
    agent_commands: &[(&str, &Path)],
) {
    let (mut config_yaml, automation_yaml) = template_config_and_automation(template, project_name);

    for (agent_name, command_path) in agent_commands {
        let old = format!("{agent_name}:\n    runner: copilot");
        let new = format!(
            "{agent_name}:\n    runner: claude\n    command: \"{}\"",
            command_path.to_string_lossy()
        );
        config_yaml = config_yaml.replace(&old, &new);
    }

    let project_dir = fixtures.tasks_root.join(prefix);
    fs::create_dir_all(&project_dir).expect("create project config dir");
    fixtures.create_config_in_dir(&project_dir, &config_yaml);
    fs::write(fixtures.tasks_root.join("automation.yml"), automation_yaml)
        .expect("write automation config");
}

#[cfg(unix)]
fn automation_yaml_for(agent: &str) -> String {
    format!(
        "automation:\n  rules:\n    - name: Agent lifecycle\n      when:\n        assignee: \"@{agent}\"\n      on:\n        job_start:\n          set:\n            status: InProgress\n        complete:\n          set:\n            status: NeedsReview\n            assignee: \"@assignee_or_reporter\"\n          add:\n            tags: [agent-success]\n        error:\n          set:\n            assignee: \"@reporter\"\n          add:\n            tags: [agent-error]\n        cancel:\n          set:\n            assignee: \"@reporter\"\n          add:\n            tags: [agent-cancel]\n"
    )
}

#[cfg(unix)]
fn job_for_ticket(ticket_id: &str) -> lotar::api_types::AgentJob {
    AgentJobService::list_jobs()
        .into_iter()
        .find(|job| job.ticket_id == ticket_id)
        .expect("job queued")
}

#[cfg(unix)]
fn wait_for_job_status(job_id: &str, status: &str, timeout_ms: u64) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        if let Some(job) = AgentJobService::get_job(job_id)
            && job.status == status
        {
            return true;
        }
        sleep(Duration::from_millis(50));
    }
    false
}

#[cfg(unix)]
fn wait_for_completed_jobs(
    ticket_id: &str,
    expected: usize,
    timeout_ms: u64,
) -> Vec<lotar::api_types::AgentJob> {
    let start = Instant::now();
    let mut latest = Vec::new();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        let jobs: Vec<_> = AgentJobService::list_jobs()
            .into_iter()
            .filter(|job| job.ticket_id == ticket_id)
            .collect();
        if jobs.len() == expected && jobs.iter().all(|job| job.status == "completed") {
            return jobs;
        }
        latest = jobs;
        sleep(Duration::from_millis(50));
    }
    latest
}

#[cfg(unix)]
fn wait_for_ticket_jobs<F>(
    ticket_id: &str,
    timeout_ms: u64,
    predicate: F,
) -> Vec<lotar::api_types::AgentJob>
where
    F: Fn(&[lotar::api_types::AgentJob]) -> bool,
{
    let start = Instant::now();
    let mut latest = Vec::new();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        let jobs: Vec<_> = AgentJobService::list_jobs()
            .into_iter()
            .filter(|job| job.ticket_id == ticket_id)
            .collect();
        if predicate(&jobs) {
            return jobs;
        }
        latest = jobs;
        sleep(Duration::from_millis(50));
    }
    latest
}

#[cfg(unix)]
fn wait_for_task_status(
    storage: &lotar::Storage,
    ticket_id: &str,
    status: &str,
    timeout_ms: u64,
) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        if let Ok(task) = TaskService::get(storage, ticket_id, None)
            && task.status.as_str() == status
        {
            return true;
        }
        sleep(Duration::from_millis(50));
    }
    false
}

#[cfg(unix)]
fn wait_for_task_state(
    storage: &lotar::Storage,
    ticket_id: &str,
    status: &str,
    assignee: Option<&str>,
    timeout_ms: u64,
) -> Option<lotar::api_types::TaskDTO> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        if let Ok(task) = TaskService::get(storage, ticket_id, None)
            && task.status.as_str() == status
            && assignee.is_none_or(|value| task.assignee.as_deref() == Some(value))
        {
            return Some(task);
        }
        sleep(Duration::from_millis(50));
    }
    None
}

#[cfg(unix)]
#[test]
fn auto_queue_assignment_starts_job_and_updates_status() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();
    let agent_name = "stub-success";
    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "stub-agent-success.sh",
        "#!/bin/sh\n\
echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"stub-1\"}'\n\
echo '{\"type\":\"assistant\",\"message\":{\"content\":[{\"text\":\"stubbed success\"}]},\"session_id\":\"stub-1\"}'\n\
exit 0\n",
    );
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "agents:\n  {agent_name}:\n    runner: claude\n    command: \"{}\"\n",
            script.to_string_lossy()
        ),
    );
    // Automation rules define lifecycle actions - agent job is queued implicitly on assignment
    let automation_yaml = automation_yaml_for(agent_name);
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Auto agent run".to_string(),
            project: Some("AUTO".to_string()),
            reporter: Some("sam".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    let _updated = TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{}", agent_name)),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let job = job_for_ticket(&created.id);
    assert!(
        wait_for_job_status(&job.id, "completed", 2000),
        "job did not complete in time"
    );

    let completed = AgentJobService::get_job(&job.id).expect("job still exists");
    assert!(
        completed
            .summary
            .as_deref()
            .is_some_and(|v| v.contains("stubbed success")),
        "expected summary from stub agent"
    );

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert_eq!(refreshed.status.as_str(), "NeedsReview");
    assert_eq!(refreshed.assignee.as_deref(), Some("sam"));
    assert!(
        refreshed
            .tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case("agent-success"))
    );
}

#[cfg(unix)]
#[test]
fn auto_queue_assignment_handles_failure_action() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();
    let agent_name = "stub-failure";
    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "stub-agent-fail.sh",
        "#!/bin/sh\n\
echo '{\"type\":\"assistant\",\"message\":{\"content\":[{\"text\":\"stubbed failure\"}]}}'\n\
exit 1\n",
    );
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "agents:\n  {agent_name}:\n    runner: claude\n    command: \"{}\"\n",
            script.to_string_lossy()
        ),
    );
    let automation_yaml = automation_yaml_for(agent_name);
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Auto agent fail".to_string(),
            project: Some("FAIL".to_string()),
            reporter: Some("sam".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{}", agent_name)),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let job = job_for_ticket(&created.id);
    assert!(
        wait_for_job_status(&job.id, "failed", 2000),
        "job did not fail in time"
    );

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert_eq!(refreshed.assignee.as_deref(), Some("sam"));
    assert!(
        refreshed
            .tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case("agent-error"))
    );
}

#[cfg(unix)]
#[test]
fn auto_queue_assignment_handles_cancel_action() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();
    let agent_name = "stub-cancel";
    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "stub-agent-cancel.sh",
        "#!/bin/sh\n\
echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"stub-cancel\"}'\n\
sleep 5\n",
    );
    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "agents:\n  {agent_name}:\n    runner: claude\n    command: \"{}\"\n",
            script.to_string_lossy()
        ),
    );
    let automation_yaml = automation_yaml_for(agent_name);
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Auto agent cancel".to_string(),
            project: Some("CANCEL".to_string()),
            reporter: Some("sam".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{}", agent_name)),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let job = job_for_ticket(&created.id);
    let cancelled = AgentJobService::cancel_job(&job.id)
        .expect("cancel job")
        .expect("job exists");
    assert_eq!(cancelled.status, "cancelled");
    let job_after = AgentJobService::get_job(&job.id).expect("job still exists");
    assert_eq!(job_after.status, "cancelled");

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert_eq!(refreshed.assignee.as_deref(), Some("sam"));
    assert!(
        refreshed
            .tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case("agent-cancel"))
    );
}

#[cfg(unix)]
#[test]
fn auto_queue_assignment_chains_agents_and_runs_commands() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let implement_agent = "implement";
    let test_agent = "test";

    let implement_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "stub-agent-implement.sh",
        "#!/bin/sh\n\
echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"impl-1\"}'\n\
echo '{\"type\":\"assistant\",\"message\":{\"content\":[{\"text\":\"implement done\"}]},\"session_id\":\"impl-1\"}'\n\
exit 0\n",
    );
    let test_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "stub-agent-test.sh",
        "#!/bin/sh\n\
echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"test-1\"}'\n\
echo '{\"type\":\"assistant\",\"message\":{\"content\":[{\"text\":\"test done\"}]},\"session_id\":\"test-1\"}'\n\
exit 0\n",
    );

    let run_log = fixtures.get_temp_path().join("automation-run.log");
    let run_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "automation-run.sh",
        &format!(
            "#!/bin/sh\n\
printf '%s|%s' \"$LOTAR_TICKET_ID\" \"$LOTAR_AGENT_PROFILE\" > \"{}\"\n",
            run_log.to_string_lossy()
        ),
    );

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "issue:\n  states:\n    - Todo\n    - InProgress\n    - Testing\n    - Done\nagents:\n  {implement_agent}:\n    runner: claude\n    command: \"{}\"\n  {test_agent}:\n    runner: claude\n    command: \"{}\"\n",
            implement_script.to_string_lossy(),
            test_script.to_string_lossy()
        ),
    );

    let automation_yaml = format!(
        "automation:\n  rules:\n    - name: Implement phase\n      when:\n        assignee: \"@{implement_agent}\"\n      on:\n        job_start:\n          set:\n            status: InProgress\n        complete:\n          set:\n            status: Testing\n            assignee: \"@{test_agent}\"\n          run:\n            command: \"{}\"\n    - name: Test phase\n      when:\n        assignee: \"@{test_agent}\"\n      on:\n        job_start:\n          set:\n            status: Testing\n        complete:\n          set:\n            status: Done\n            assignee: \"@reporter\"\n",
        run_script.to_string_lossy()
    );
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Auto agent pipeline".to_string(),
            project: Some("PIPE".to_string()),
            reporter: Some("sam".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{}", implement_agent)),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let jobs = wait_for_completed_jobs(&created.id, 2, 4000);
    assert_eq!(jobs.len(), 2, "expected two completed jobs");

    let implement_job = jobs
        .iter()
        .find(|job| job.agent.as_deref() == Some(implement_agent))
        .expect("implement job recorded");
    assert_eq!(implement_job.status, "completed");

    let test_job = jobs
        .iter()
        .find(|job| job.agent.as_deref() == Some(test_agent))
        .expect("test job recorded");
    assert_eq!(test_job.status, "completed");

    let run_output = fs::read_to_string(&run_log).expect("read run log");
    assert!(run_output.contains(&created.id));
    assert!(run_output.contains(implement_agent));

    assert!(
        wait_for_task_status(&storage, &created.id, "Done", 2000),
        "task did not reach Done after both jobs completed"
    );

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert_eq!(refreshed.status.as_str(), "Done");
    assert_eq!(refreshed.assignee.as_deref(), Some("sam"));
}

#[cfg(unix)]
#[test]
fn shipped_agent_pipeline_template_runs_end_to_end() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();
    init_git_repository(fixtures.get_temp_path());

    let run_log = fixtures.get_temp_path().join("template-pipeline-run.log");
    let agent_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "template-pipeline-agent.sh",
        &format!(
            r#"#!/bin/sh
profile="${{LOTAR_AGENT_PROFILE:-unknown}}"
printf '%s|%s\n' "$LOTAR_TICKET_ID" "$profile" >> "{}"
sid="${{profile}}-$$"
printf '{{"type":"system","subtype":"init","session_id":"%s"}}\n' "$sid"
printf '{{"type":"assistant","message":{{"content":[{{"text":"%s complete"}}]}},"session_id":"%s"}}\n' "$profile" "$sid"
exit 0
"#,
            run_log.to_string_lossy()
        ),
    );

    install_template_workflow(
        &fixtures,
        "PIP",
        "Pipeline Template",
        AGENT_PIPELINE_TEMPLATE,
        &[
            ("implement", agent_script.as_path()),
            ("test", agent_script.as_path()),
            ("merge", agent_script.as_path()),
            ("merge-retry", agent_script.as_path()),
        ],
    );

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Shipped pipeline template".to_string(),
            project: Some("PIP".to_string()),
            reporter: Some("Agent Tests".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("@implement".to_string()),
            ..Default::default()
        },
    )
    .expect("assign implementation agent");

    let jobs = wait_for_completed_jobs(&created.id, 3, 6000);
    assert_eq!(jobs.len(), 3, "expected implement, test, and merge jobs");

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get final task");
    assert_eq!(refreshed.status.as_str(), "Done");
    assert_eq!(refreshed.assignee.as_deref(), Some("Agent Tests"));

    let run_output = fs::read_to_string(&run_log).expect("read pipeline run log");
    let phases: Vec<_> = run_output.lines().collect();
    assert_eq!(phases, vec!["PIP-1|implement", "PIP-1|test", "PIP-1|merge"]);
}

#[cfg(unix)]
#[test]
fn shipped_agent_reviewed_template_handles_failure_review_and_merge() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();
    init_git_repository(fixtures.get_temp_path());

    let run_log = fixtures.get_temp_path().join("template-reviewed-run.log");
    let fail_once_marker = fixtures.get_temp_path().join("test-fails-once.marker");
    fs::write(&fail_once_marker, "fail once\n").expect("write fail-once marker");

    let implement_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "template-reviewed-implement.sh",
        &format!(
            r#"#!/bin/sh
profile="${{LOTAR_AGENT_PROFILE:-unknown}}"
printf '%s|%s\n' "$LOTAR_TICKET_ID" "$profile" >> "{}"
sid="${{profile}}-$$"
printf '{{"type":"system","subtype":"init","session_id":"%s"}}\n' "$sid"
printf '{{"type":"assistant","message":{{"content":[{{"text":"%s complete"}}]}},"session_id":"%s"}}\n' "$profile" "$sid"
exit 0
"#,
            run_log.to_string_lossy()
        ),
    );
    let test_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "template-reviewed-test.sh",
        &format!(
            r#"#!/bin/sh
profile="${{LOTAR_AGENT_PROFILE:-unknown}}"
printf '%s|%s\n' "$LOTAR_TICKET_ID" "$profile" >> "{}"
sid="${{profile}}-$$"
printf '{{"type":"system","subtype":"init","session_id":"%s"}}\n' "$sid"
if [ -f "{}" ]; then
  rm "{}"
  printf '{{"type":"assistant","message":{{"content":[{{"text":"%s failed"}}]}},"session_id":"%s"}}\n' "$profile" "$sid"
  exit 1
fi
printf '{{"type":"assistant","message":{{"content":[{{"text":"%s complete"}}]}},"session_id":"%s"}}\n' "$profile" "$sid"
exit 0
"#,
            run_log.to_string_lossy(),
            fail_once_marker.to_string_lossy(),
            fail_once_marker.to_string_lossy()
        ),
    );
    let merge_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "template-reviewed-merge.sh",
        &format!(
            r#"#!/bin/sh
profile="${{LOTAR_AGENT_PROFILE:-unknown}}"
printf '%s|%s\n' "$LOTAR_TICKET_ID" "$profile" >> "{}"
sid="${{profile}}-$$"
printf '{{"type":"system","subtype":"init","session_id":"%s"}}\n' "$sid"
printf '{{"type":"assistant","message":{{"content":[{{"text":"%s complete"}}]}},"session_id":"%s"}}\n' "$profile" "$sid"
exit 0
"#,
            run_log.to_string_lossy()
        ),
    );

    install_template_workflow(
        &fixtures,
        "REV",
        "Reviewed Template",
        AGENT_REVIEWED_TEMPLATE,
        &[
            ("implement", implement_script.as_path()),
            ("test", test_script.as_path()),
            ("merge", merge_script.as_path()),
            ("merge-retry", merge_script.as_path()),
        ],
    );
    lotar::utils::identity::invalidate_identity_cache(Some(fixtures.tasks_root.as_path()));

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Shipped reviewed template".to_string(),
            project: Some("REV".to_string()),
            reporter: Some("Agent Tests".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("@implement".to_string()),
            ..Default::default()
        },
    )
    .expect("assign implementation agent");

    let failed_cycle_jobs = wait_for_ticket_jobs(&created.id, 6000, |jobs| {
        jobs.len() >= 2
            && jobs
                .iter()
                .any(|job| job.agent.as_deref() == Some("implement") && job.status == "completed")
            && jobs
                .iter()
                .any(|job| job.agent.as_deref() == Some("test") && job.status == "failed")
    });
    assert!(
        failed_cycle_jobs
            .iter()
            .any(|job| job.agent.as_deref() == Some("implement") && job.status == "completed"),
        "expected completed implementation job in failed cycle"
    );
    assert!(
        failed_cycle_jobs
            .iter()
            .any(|job| job.agent.as_deref() == Some("test") && job.status == "failed"),
        "expected failed test job in failed cycle"
    );

    let review_task =
        wait_for_task_state(&storage, &created.id, "Review", Some("Agent Tests"), 6000)
            .expect("task should hand back to reporter for review");
    assert!(review_task.tags.iter().any(|tag| tag == "ready-for-review"));
    assert!(review_task.tags.iter().any(|tag| tag == "test-failure"));
    assert!(review_task.history.iter().any(|entry| {
        let saw_status_bounce = entry.changes.iter().any(|change| {
            change.field == "status"
                && change.old.as_deref() == Some("Testing")
                && change.new.as_deref() == Some("InProgress")
        });
        let saw_reassignment = entry.changes.iter().any(|change| {
            change.field == "assignee"
                && change.old.as_deref() == Some("@test")
                && change.new.as_deref() == Some("@implement")
        });
        saw_status_bounce && saw_reassignment
    }));

    let advanced = TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("@merge".to_string()),
            ..Default::default()
        },
    )
    .expect("reporter should be allowed to send reviewed task to merge");
    assert_eq!(advanced.assignee.as_deref(), Some("@merge"));

    let final_task = wait_for_task_state(&storage, &created.id, "Done", Some("Agent Tests"), 6000)
        .expect("task should complete after merge");
    assert!(!final_task.tags.iter().any(|tag| tag == "ready-for-review"));

    let final_jobs = wait_for_ticket_jobs(&created.id, 6000, |jobs| {
        jobs.len() >= 5
            && jobs
                .iter()
                .any(|job| job.agent.as_deref() == Some("merge") && job.status == "completed")
    });
    assert_eq!(
        final_jobs.len(),
        5,
        "expected implement/test retry plus merge jobs"
    );

    let run_output = fs::read_to_string(&run_log).expect("read reviewed run log");
    let phases: Vec<_> = run_output.lines().collect();
    assert_eq!(
        phases,
        vec![
            "REV-1|implement",
            "REV-1|test",
            "REV-1|implement",
            "REV-1|test",
            "REV-1|merge",
        ]
    );
}

// ---------- New feature tests ----------

/// Command runner: `runner: command` runs arbitrary scripts with full job lifecycle.
#[cfg(unix)]
#[test]
fn command_runner_captures_output_and_completes() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let agent_name = "lint-cmd";
    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "cmd-runner.sh",
        "#!/bin/sh\necho 'lint passed'\nexit 0\n",
    );

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "agents:\n  {agent_name}:\n    runner: command\n    command: \"{}\"\n",
            script.to_string_lossy()
        ),
    );
    let automation_yaml = format!(
        "automation:\n  rules:\n    - name: Command runner lifecycle\n      when:\n        assignee: \"@{agent_name}\"\n      on:\n        job_completed:\n          set:\n            status: Done\n            assignee: \"@reporter\"\n"
    );
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Command runner test".to_string(),
            project: Some("CMD".to_string()),
            reporter: Some("alice".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{agent_name}")),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let job = job_for_ticket(&created.id);
    assert_eq!(job.runner, "command");
    assert!(
        wait_for_job_status(&job.id, "completed", 3000),
        "command runner job did not complete in time"
    );

    let completed = AgentJobService::get_job(&job.id).expect("job exists");
    assert_eq!(completed.exit_code, Some(0));

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert_eq!(refreshed.status.as_str(), "Done");
    assert_eq!(refreshed.assignee.as_deref(), Some("alice"));
}

/// Command runner: failure (non-zero exit) fires `job_failed` hook.
#[cfg(unix)]
#[test]
fn command_runner_failure_fires_job_failed() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let agent_name = "fail-cmd";
    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "cmd-fail.sh",
        "#!/bin/sh\necho 'build failed'\nexit 1\n",
    );

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "agents:\n  {agent_name}:\n    runner: command\n    command: \"{}\"\n",
            script.to_string_lossy()
        ),
    );
    let automation_yaml = format!(
        "automation:\n  rules:\n    - name: Fail lifecycle\n      when:\n        assignee: \"@{agent_name}\"\n      on:\n        job_failed:\n          set:\n            assignee: \"@reporter\"\n          add:\n            tags: [cmd-error]\n"
    );
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Cmd fail test".to_string(),
            project: Some("CMDF".to_string()),
            reporter: Some("bob".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{agent_name}")),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let job = job_for_ticket(&created.id);
    assert!(
        wait_for_job_status(&job.id, "failed", 3000),
        "command runner job did not fail in time"
    );

    // Wait for the automation job_failed hook to reset assignee + add tag.
    let start = Instant::now();
    let refreshed = loop {
        let task = TaskService::get(&storage, &created.id, None).expect("get task");
        if task.assignee.as_deref() == Some("bob") && task.tags.iter().any(|t| t == "cmd-error") {
            break task;
        }
        if start.elapsed() > Duration::from_millis(3000) {
            panic!(
                "automation job_failed hook did not update task in time: assignee={:?} tags={:?}",
                task.assignee, task.tags
            );
        }
        sleep(Duration::from_millis(50));
    };
    assert_eq!(refreshed.assignee.as_deref(), Some("bob"));
    assert!(refreshed.tags.iter().any(|t| t == "cmd-error"));
}

/// Template expansion in automation `run` commands — `${{ticket.id}}` and `${{ticket.title}}`.
#[cfg(unix)]
#[test]
fn template_expansion_in_run_action() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let agent_name = "tmpl-agent";
    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "stub-tmpl.sh",
        "#!/bin/sh\n\
echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"t-1\"}'\n\
echo '{\"type\":\"assistant\",\"message\":{\"content\":[{\"text\":\"done\"}]},\"session_id\":\"t-1\"}'\n\
exit 0\n",
    );

    let run_log = fixtures.get_temp_path().join("tmpl-run.log");
    let run_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "tmpl-run.sh",
        &format!(
            "#!/bin/sh\necho \"$1|$2\" > \"{}\"\n",
            run_log.to_string_lossy()
        ),
    );

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "agents:\n  {agent_name}:\n    runner: claude\n    command: \"{}\"\n",
            script.to_string_lossy()
        ),
    );
    let automation_yaml = format!(
        "automation:\n  rules:\n    - name: Template test\n      when:\n        assignee: \"@{agent_name}\"\n      on:\n        job_completed:\n          run:\n            command: \"{}\"\n            args:\n              - \"${{{{ticket.id}}}}\"\n              - \"${{{{ticket.title}}}}\"\n",
        run_script.to_string_lossy()
    );
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Template expansion test".to_string(),
            project: Some("TMPL".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{agent_name}")),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let job = job_for_ticket(&created.id);
    assert!(
        wait_for_job_status(&job.id, "completed", 5000),
        "job did not complete in time"
    );

    // Wait for the automation run command to create the log file (async after job completion)
    let deadline = Instant::now() + Duration::from_millis(5000);
    while !run_log.exists() && Instant::now() < deadline {
        sleep(Duration::from_millis(50));
    }

    let run_output = fs::read_to_string(&run_log).expect("read template run log");
    assert!(
        run_output.contains(&created.id),
        "expected ticket ID in run output, got: {}",
        run_output
    );
    assert!(
        run_output.contains("Template expansion test"),
        "expected ticket title in run output, got: {}",
        run_output
    );
}

/// Event hooks: `on.assigned` fires when assignee changes, `on.start` (legacy) also fires.
#[cfg(unix)]
#[test]
fn event_hooks_assigned_and_legacy_start_both_fire() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let assigned_log = fixtures.get_temp_path().join("assigned.log");
    let start_log = fixtures.get_temp_path().join("start.log");

    let assigned_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "hook-assigned.sh",
        &format!(
            "#!/bin/sh\necho assigned > \"{}\"\n",
            assigned_log.to_string_lossy()
        ),
    );
    let start_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "hook-start.sh",
        &format!(
            "#!/bin/sh\necho start > \"{}\"\n",
            start_log.to_string_lossy()
        ),
    );

    fixtures.create_config_in_dir(&fixtures.tasks_root, "");
    let automation_yaml = format!(
        "automation:\n  rules:\n    - name: Dual hook test\n      when:\n        assignee: alice\n      on:\n        assigned:\n          run: \"{}\"\n        start:\n          run: \"{}\"\n",
        assigned_script.to_string_lossy(),
        start_script.to_string_lossy()
    );
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Hook test".to_string(),
            project: Some("HOOK".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("alice".to_string()),
            ..Default::default()
        },
    )
    .expect("update assignee");

    // Both hooks should have fired
    assert!(
        assigned_log.exists(),
        "assigned hook did not fire — log file not created"
    );
    assert!(
        start_log.exists(),
        "legacy start hook did not fire — log file not created"
    );
}

/// Event hooks: `on.created` fires only on task creation, not on update.
#[cfg(unix)]
#[test]
fn event_hook_created_fires_only_on_create() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let created_log = fixtures.get_temp_path().join("created-hook.log");
    let created_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "hook-created.sh",
        &format!(
            "#!/bin/sh\necho created >> \"{}\"\n",
            created_log.to_string_lossy()
        ),
    );

    fixtures.create_config_in_dir(&fixtures.tasks_root, "");
    let automation_yaml = format!(
        "automation:\n  rules:\n    - name: Created hook\n      on:\n        created:\n          run: \"{}\"\n",
        created_script.to_string_lossy()
    );
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();

    // TaskService::create already calls apply_task_update(None, task) internally,
    // which fires the Created event and automation hooks.
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Created hook test".to_string(),
            project: Some("CRHK".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    assert!(
        created_log.exists(),
        "created hook did not fire on task creation"
    );
    let content = fs::read_to_string(&created_log).expect("read created log");
    let fire_count = content.lines().filter(|l| l.trim() == "created").count();
    assert_eq!(fire_count, 1, "created hook should fire exactly once");

    // Now update the task — `TaskService::update` also calls apply_task_update
    // internally with previous=Some(...), producing an Updated event.
    // The created hook should NOT fire again.
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("bob".to_string()),
            ..Default::default()
        },
    )
    .expect("update task");

    // Re-read the file — should still have only 1 "created" line
    let content_after = fs::read_to_string(&created_log).expect("read created log again");
    let fire_count_after = content_after
        .lines()
        .filter(|l| l.trim() == "created")
        .count();
    assert_eq!(
        fire_count_after, 1,
        "created hook should NOT fire on update"
    );
}

/// Template expansion: `${{previous.assignee}}` is available on `updated` event.
#[cfg(unix)]
#[test]
fn previous_template_vars_in_updated_hook() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let prev_log = fixtures.get_temp_path().join("prev.log");
    let prev_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "prev-hook.sh",
        &format!(
            "#!/bin/sh\necho \"$1\" > \"{}\"\n",
            prev_log.to_string_lossy()
        ),
    );

    fixtures.create_config_in_dir(&fixtures.tasks_root, "");
    let automation_yaml = format!(
        "automation:\n  rules:\n    - name: Previous vars test\n      on:\n        updated:\n          run:\n            command: \"{}\"\n            args:\n              - \"${{{{previous.assignee}}}}\"\n",
        prev_script.to_string_lossy()
    );
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Previous vars test".to_string(),
            project: Some("PREV".to_string()),
            assignee: Some("alice".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    // Update assignee from alice to bob — TaskService::update calls
    // apply_task_update(Some(prev), new) internally, which fires the Updated
    // event with ${{previous.assignee}} = alice.
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("bob".to_string()),
            ..Default::default()
        },
    )
    .expect("update assignee");

    assert!(prev_log.exists(), "updated hook did not fire");
    let content = fs::read_to_string(&prev_log).expect("read prev log");
    assert_eq!(
        content.trim(),
        "alice",
        "expected previous.assignee=alice, got: {}",
        content.trim()
    );
}

/// LOTAR_* env vars: command runner receives ticket context via env vars.
#[cfg(unix)]
#[test]
fn command_runner_receives_lotar_env_vars() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let agent_name = "env-cmd";
    let env_log = fixtures.get_temp_path().join("env.log");
    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "cmd-env.sh",
        &format!(
            "#!/bin/sh\necho \"id=$LOTAR_TICKET_ID|title=$LOTAR_TICKET_TITLE|status=$LOTAR_TICKET_STATUS|runner=$LOTAR_AGENT_RUNNER\" > \"{}\"\n",
            env_log.to_string_lossy()
        ),
    );

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "agents:\n  {agent_name}:\n    runner: command\n    command: \"{}\"\n",
            script.to_string_lossy()
        ),
    );
    // No automation rules needed — we're testing that the runner process itself gets LOTAR_* vars
    AutomationService::set(&fixtures.tasks_root, None, "automation:\n  rules: []\n")
        .expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Env vars test".to_string(),
            project: Some("ENV".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{agent_name}")),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let job = job_for_ticket(&created.id);
    assert!(
        wait_for_job_status(&job.id, "completed", 3000),
        "job did not complete in time"
    );

    assert!(env_log.exists(), "env log file not created");
    let content = fs::read_to_string(&env_log).expect("read env log");
    assert!(
        content.contains(&format!("id={}", created.id)),
        "LOTAR_TICKET_ID missing in env output: {}",
        content
    );
    assert!(
        content.contains("title=Env vars test"),
        "LOTAR_TICKET_TITLE missing in env output: {}",
        content
    );
    assert!(
        content.contains("runner=command"),
        "LOTAR_AGENT_RUNNER missing in env output: {}",
        content
    );
}

/// Backward compatibility: old YAML keys (job_start, complete, error, cancel) still work.
#[cfg(unix)]
#[test]
fn backward_compat_old_yaml_keys_still_work() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let agent_name = "compat-agent";
    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "compat-agent.sh",
        "#!/bin/sh\n\
echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"c-1\"}'\n\
echo '{\"type\":\"assistant\",\"message\":{\"content\":[{\"text\":\"compat ok\"}]},\"session_id\":\"c-1\"}'\n\
exit 0\n",
    );

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "agents:\n  {agent_name}:\n    runner: claude\n    command: \"{}\"\n",
            script.to_string_lossy()
        ),
    );
    // Use OLD YAML key names: job_start, complete (not job_started, job_completed)
    let automation_yaml = format!(
        "automation:\n  rules:\n    - name: Backward compat\n      when:\n        assignee: \"@{agent_name}\"\n      on:\n        job_start:\n          set:\n            status: InProgress\n        complete:\n          set:\n            status: Done\n            assignee: \"@reporter\"\n          add:\n            tags: [compat-success]\n"
    );
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Backward compat test".to_string(),
            project: Some("COMPAT".to_string()),
            reporter: Some("reporter-user".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{agent_name}")),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let job = job_for_ticket(&created.id);
    assert!(
        wait_for_job_status(&job.id, "completed", 3000),
        "job did not complete in time (backward compat)"
    );

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert_eq!(refreshed.status.as_str(), "Done");
    assert_eq!(refreshed.assignee.as_deref(), Some("reporter-user"));
    assert!(
        refreshed.tags.iter().any(|t| t == "compat-success"),
        "expected compat-success tag, got tags: {:?}",
        refreshed.tags
    );
}

/// Comment action appends a template-expanded comment to the task.
#[test]
fn comment_action_appends_template_expanded_comment() {
    let fixtures = TestFixtures::new();
    let automation_yaml = "\
automation:
  rules:
    - name: Comment on assign
      when:
        assignee: { exists: true }
      on:
        assigned:
          comment: \"Assigned to ${{ticket.assignee}}, was ${{previous.assignee}}\"
";
    AutomationService::set(&fixtures.tasks_root, None, automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Comment test".to_string(),
            project: Some("CMT".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("alice".to_string()),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert!(
        !refreshed.comments.is_empty(),
        "expected at least one comment, got none"
    );
    let comment = &refreshed.comments[0];
    assert!(
        comment.text.contains("alice"),
        "expected 'alice' in comment, got: {}",
        comment.text
    );
    // previous.assignee should be empty since there was no prior assignee
    assert!(
        comment.text.contains("Assigned to alice"),
        "expected 'Assigned to alice' in comment, got: {}",
        comment.text
    );
}

/// Max-iterations safety net: after reaching the limit, no more jobs are queued
/// and the ticket is tagged `automation-limit-reached`.
#[cfg(unix)]
#[test]
fn max_iterations_stops_after_limit() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let agent_name = "iter-agent";
    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "iter-stub.sh",
        "#!/bin/sh\n\
echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"i-1\"}'\n\
echo '{\"type\":\"assistant\",\"message\":{\"content\":[{\"text\":\"done\"}]},\"session_id\":\"i-1\"}'\n\
exit 0\n",
    );

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "agents:\n  {agent_name}:\n    runner: claude\n    command: \"{}\"\n",
            script.to_string_lossy()
        ),
    );
    // max_iterations: 1 — after first completed job, further assignments should be blocked
    let automation_yaml = format!(
        "automation:\n  max_iterations: 1\n  rules:\n    - name: Iter lifecycle\n      when:\n        assignee: \"@{agent_name}\"\n      on:\n        job_completed:\n          set:\n            status: NeedsReview\n"
    );
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Max iterations test".to_string(),
            project: Some("ITER".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    // First assignment → first job
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{agent_name}")),
            ..Default::default()
        },
    )
    .expect("update assignee");

    let job = job_for_ticket(&created.id);
    assert!(
        wait_for_job_status(&job.id, "completed", 3000),
        "first job did not complete in time"
    );
    sleep(Duration::from_millis(200));

    // Unassign, then reassign to trigger a second job attempt
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("nobody".to_string()),
            ..Default::default()
        },
    )
    .expect("unassign");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some(format!("@{agent_name}")),
            ..Default::default()
        },
    )
    .expect("reassign to agent");

    sleep(Duration::from_millis(300));

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert!(
        refreshed
            .tags
            .iter()
            .any(|t| t == "automation-limit-reached"),
        "expected automation-limit-reached tag, got: {:?}",
        refreshed.tags
    );

    // Verify only 1 job was created (second was blocked by max_iterations)
    let jobs: Vec<_> = AgentJobService::list_jobs()
        .into_iter()
        .filter(|j| j.ticket_id == created.id)
        .collect();
    assert_eq!(jobs.len(), 1, "expected exactly 1 job, got {}", jobs.len());
}

/// Verify that a rule with `cooldown` only fires once within the cooldown window.
#[test]
fn cooldown_skips_rule_within_window() {
    let fixtures = TestFixtures::new();

    // Reset cooldown state from any prior test
    lotar::services::automation_service::cooldown_reset();

    let yaml = r#"
automation:
  rules:
    - name: Tag on update
      cooldown: "60s"
      on:
        updated:
          add:
            tags: [auto-tagged]
"#;
    AutomationService::set(&fixtures.tasks_root, None, yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Cooldown test".into(),
            project: Some("COOL".into()),
            ..Default::default()
        },
    )
    .expect("create task");

    // First update — rule should fire
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            priority: Some("High".into()),
            ..Default::default()
        },
    )
    .expect("update 1");
    let after1 = TaskService::get(&storage, &created.id, None).expect("get after update 1");
    assert!(
        after1.tags.iter().any(|t| t == "auto-tagged"),
        "first update should add tag, got: {:?}",
        after1.tags
    );

    // Remove the tag so we can detect if the rule fires again
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            tags: Some(vec![]),
            ..Default::default()
        },
    )
    .expect("update 2 - remove tag");
    let after2 = TaskService::get(&storage, &created.id, None).expect("get after update 2");
    assert!(
        !after2.tags.iter().any(|t| t == "auto-tagged"),
        "tag should be removed"
    );

    // Second update within cooldown — rule should NOT fire
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            priority: Some("Critical".into()),
            ..Default::default()
        },
    )
    .expect("update 3");
    let after3 = TaskService::get(&storage, &created.id, None).expect("get after update 3");
    assert!(
        !after3.tags.iter().any(|t| t == "auto-tagged"),
        "second update within cooldown should NOT add tag, got: {:?}",
        after3.tags
    );
}

/// Verify that cooldown expires and the rule fires again after the window.
#[test]
fn cooldown_allows_after_expiry() {
    let fixtures = TestFixtures::new();

    // Reset cooldown state from any prior test
    lotar::services::automation_service::cooldown_reset();

    let yaml = r#"
automation:
  rules:
    - name: Tag on update short cooldown
      cooldown: "1s"
      on:
        updated:
          add:
            tags: [short-cd]
"#;
    AutomationService::set(&fixtures.tasks_root, None, yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Short cooldown test".into(),
            project: Some("COOL2".into()),
            ..Default::default()
        },
    )
    .expect("create task");

    // First update — rule should fire
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            priority: Some("High".into()),
            ..Default::default()
        },
    )
    .expect("update 1");
    let after1 = TaskService::get(&storage, &created.id, None).expect("get after update 1");
    assert!(
        after1.tags.iter().any(|t| t == "short-cd"),
        "first update should add tag"
    );

    // Remove tag
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            tags: Some(vec![]),
            ..Default::default()
        },
    )
    .expect("remove tag");

    // Wait for cooldown to expire
    sleep(Duration::from_millis(1100));

    // Third update — rule should fire again
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            priority: Some("Critical".into()),
            ..Default::default()
        },
    )
    .expect("update 3 after cooldown");
    let after3 = TaskService::get(&storage, &created.id, None).expect("get after update 3");
    assert!(
        after3.tags.iter().any(|t| t == "short-cd"),
        "update after cooldown expiry should add tag again, got: {:?}",
        after3.tags
    );
}

/// Simulation returns matching rule and actions without modifying the task.
#[test]
fn simulate_returns_actions_without_side_effects() {
    use lotar::services::automation_service::AutomationEvent;

    let fixtures = TestFixtures::new();
    let yaml = r#"
automation:
  rules:
    - name: Auto-tag bugs
      when:
        type: Bug
      on:
        updated:
          set:
            priority: High
          add:
            tags: [triaged]
"#;
    AutomationService::set(&fixtures.tasks_root, None, yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Simulate test".into(),
            project: Some("SIM".into()),
            task_type: Some("Bug".into()),
            ..Default::default()
        },
    )
    .expect("create task");

    let result =
        AutomationService::simulate(&fixtures.tasks_root, &created.id, AutomationEvent::Updated)
            .expect("simulate");

    assert!(result.matched, "rule should match");
    assert_eq!(result.rule_name.as_deref(), Some("Auto-tag bugs"));
    assert!(
        result.actions.iter().any(|a| a.action == "set_priority"),
        "expected set_priority action"
    );
    assert!(
        result.actions.iter().any(|a| a.action == "add_tags"),
        "expected add_tags action"
    );

    // Task should be unchanged
    let unchanged = TaskService::get(&storage, &created.id, None).expect("get task");
    assert!(unchanged.tags.is_empty(), "simulate should not modify tags");
}

/// Agent profile names like `@implement` must NOT leak into the project member list
/// when auto_populate_members is enabled.
#[cfg(unix)]
#[test]
fn agent_assignee_does_not_leak_into_members() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    fixtures.create_config_in_dir(&fixtures.tasks_root, "auto:\n  populate_members: true\nagents:\n  implement:\n    runner: claude\n    command: echo\n");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Member leak test".to_string(),
            project: Some("LEAK".to_string()),
            reporter: Some("alice".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    // Assign to an @-prefixed agent profile name
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("@implement".to_string()),
            ..Default::default()
        },
    )
    .expect("update assignee");

    // Read the config back and verify the agent profile name is NOT in the members list
    let cfg_mgr = lotar::config::manager::ConfigManager::new_manager_with_tasks_dir_readonly(
        &fixtures.tasks_root,
    )
    .expect("load config");
    let config = cfg_mgr
        .get_project_config("LEAK")
        .unwrap_or_else(|_| cfg_mgr.get_resolved_config().clone());
    let lower_members: Vec<String> = config
        .members
        .iter()
        .map(|m| m.to_ascii_lowercase())
        .collect();
    assert!(
        !lower_members.contains(&"implement".to_string()),
        "agent profile name '@implement' should not appear in members: {:?}",
        config.members
    );
    assert!(
        !lower_members.iter().any(|m| m.starts_with('@')),
        "no @-prefixed names should appear in members: {:?}",
        config.members
    );
    // alice (the reporter) SHOULD be in members since she's a real person
    assert!(
        lower_members.contains(&"alice".to_string()),
        "real reporter 'alice' should be auto-populated: {:?}",
        config.members
    );
}

/// Async `run` with `wait: false` should spawn in the background and let the update
/// return immediately. The command writes a file that we poll for.
#[cfg(unix)]
#[test]
fn async_run_action_does_not_block() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let marker = fixtures.get_temp_path().join("async_marker.txt");
    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "async-write.sh",
        &format!(
            "#!/bin/sh\nsleep 0.1\necho done > \"{}\"\n",
            marker.to_string_lossy()
        ),
    );

    fixtures.create_config_in_dir(&fixtures.tasks_root, "");
    let automation_yaml = format!(
        "automation:\n  rules:\n    - name: Async run\n      on:\n        updated:\n          run:\n            command: \"{}\"\n            wait: false\n",
        script.to_string_lossy()
    );
    AutomationService::set(&fixtures.tasks_root, None, &automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Async run test".to_string(),
            project: Some("ASYN".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    // Update returns immediately (before the script's 100ms sleep finishes)
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("bob".to_string()),
            ..Default::default()
        },
    )
    .expect("update should not block");

    // Marker should NOT exist yet (script sleeps 100ms)
    assert!(
        !marker.exists(),
        "async run should not block — marker file appeared too early"
    );

    // Poll until the script completes (max 2s)
    let start = std::time::Instant::now();
    while !marker.exists() && start.elapsed() < std::time::Duration::from_secs(2) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        marker.exists(),
        "async run command should eventually write the marker file"
    );
}

/// Date condition operators: `before`, `within`, `older_than` on due_date field.
#[cfg(unix)]
#[test]
fn date_condition_before_today_matches_overdue_task() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    fixtures.create_config_in_dir(&fixtures.tasks_root, "");
    // Rule: tag overdue tasks (due_date before today)
    let automation_yaml = r#"
automation:
  rules:
    - name: Tag overdue
      when:
        due_date: { before: "today", exists: true }
      on:
        updated:
          add:
            tags: [overdue]
"#;
    AutomationService::set(&fixtures.tasks_root, None, automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    // Create task with a past due date
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Overdue task".to_string(),
            project: Some("DATE".to_string()),
            due_date: Some("2020-01-01".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    // Update to trigger automation
    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("bob".to_string()),
            ..Default::default()
        },
    )
    .expect("update");

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert!(
        refreshed.tags.iter().any(|t| t == "overdue"),
        "overdue task should be tagged: {:?}",
        refreshed.tags
    );

    // Create task with a future due date — should NOT match
    let future_task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Future task".to_string(),
            project: Some("DATE".to_string()),
            due_date: Some("2099-12-31".to_string()),
            ..Default::default()
        },
    )
    .expect("create future task");

    TaskService::update(
        &mut storage,
        &future_task.id,
        TaskUpdate {
            assignee: Some("alice".to_string()),
            ..Default::default()
        },
    )
    .expect("update future");

    let future_refreshed = TaskService::get(&storage, &future_task.id, None).expect("get task");
    assert!(
        !future_refreshed.tags.iter().any(|t| t == "overdue"),
        "future task should NOT be tagged overdue: {:?}",
        future_refreshed.tags
    );
}

/// Date condition: `older_than` matches tasks created more than N days ago.
#[cfg(unix)]
#[test]
fn date_condition_older_than_matches_stale_created() {
    use lotar::services::automation_service::AutomationEvent;

    let fixtures = TestFixtures::new();
    fixtures.create_config_in_dir(&fixtures.tasks_root, "");
    let automation_yaml = r#"
automation:
  rules:
    - name: Stale created
      when:
        created: { older_than: "30d" }
      on:
        updated:
          add:
            tags: [stale]
"#;
    AutomationService::set(&fixtures.tasks_root, None, automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    // Create a task (it will have created=now, which is NOT older than 30d)
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Recent task".to_string(),
            project: Some("STALE".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    // Simulate: rule should NOT match (task created just now)
    let result =
        AutomationService::simulate(&fixtures.tasks_root, &created.id, AutomationEvent::Updated)
            .expect("simulate");
    assert!(
        !result.matched,
        "recent task should not match older_than:30d"
    );
}

/// Assignment strategies: `@round_robin` rotates through project members.
#[cfg(unix)]
#[test]
fn assignment_strategy_round_robin() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default:\n  members:\n    - alice\n    - bob\n    - carol\n",
    );
    let automation_yaml = r#"
automation:
  rules:
    - name: Round robin assign
      when:
        assignee:
          exists: false
      on:
        created:
          set:
            assignee: "@round_robin"
"#;
    AutomationService::set(&fixtures.tasks_root, None, automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let mut assignees = Vec::new();
    for i in 0..6 {
        let created = TaskService::create(
            &mut storage,
            TaskCreate {
                title: format!("Round robin {i}"),
                project: Some("RR".to_string()),
                ..Default::default()
            },
        )
        .expect("create task");
        // Re-read: automation changes happen inside create but aren't reflected in the return value
        let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
        if let Some(a) = refreshed.assignee.as_deref() {
            assignees.push(a.to_string());
        }
    }

    // All three members should appear at least once in 6 assignments
    assert!(
        assignees.iter().any(|a| a == "alice"),
        "alice should appear: {:?}",
        assignees
    );
    assert!(
        assignees.iter().any(|a| a == "bob"),
        "bob should appear: {:?}",
        assignees
    );
    assert!(
        assignees.iter().any(|a| a == "carol"),
        "carol should appear: {:?}",
        assignees
    );
}

/// Assignment strategy: `@least_busy` picks member with fewest active tasks.
#[cfg(unix)]
#[test]
fn assignment_strategy_least_busy() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default:\n  members:\n    - alice\n    - bob\n",
    );
    let automation_yaml = r#"
automation:
  rules:
    - name: Least busy assign
      when:
        assignee: { exists: false }
      on:
        created:
          set:
            assignee: "@least_busy"
"#;
    AutomationService::set(&fixtures.tasks_root, None, automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();

    // Pre-assign 3 tasks to alice (makes her busier)
    for i in 0..3 {
        let t = TaskService::create(
            &mut storage,
            TaskCreate {
                title: format!("Alice task {i}"),
                project: Some("LB".to_string()),
                assignee: Some("alice".to_string()),
                ..Default::default()
            },
        )
        .expect("create alice task");
        // Ensure they're active — set status to InProgress
        TaskService::update(
            &mut storage,
            &t.id,
            TaskUpdate {
                status: Some("InProgress".into()),
                ..Default::default()
            },
        )
        .expect("set in-progress");
    }

    // Create a task without assignee — automation should pick bob (least busy)
    let new_task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Least busy test".to_string(),
            project: Some("LB".to_string()),
            ..Default::default()
        },
    )
    .expect("create unassigned task");

    let refreshed = TaskService::get(&storage, &new_task.id, None).expect("get task");
    assert_eq!(
        refreshed.assignee.as_deref(),
        Some("bob"),
        "should assign to least busy member (bob has 0 active tasks, alice has 3)"
    );
}

/// Sprint condition: `sprint: { exists: false }` matches tasks not in any sprint.
/// Sprint action: `add: { sprint: "@active" }` adds the task to the active sprint.
#[cfg(unix)]
#[test]
fn sprint_condition_and_add_to_active() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    fixtures.create_config_in_dir(&fixtures.tasks_root, "default:\n  members:\n    - alice\n");
    let automation_yaml = r#"
automation:
  rules:
    - name: Auto-assign to sprint
      when:
        sprint:
          exists: false
      on:
        created:
          add:
            sprint: "@active"
"#;
    AutomationService::set(&fixtures.tasks_root, None, automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();

    // Create an active sprint
    let sprint = Sprint {
        actual: Some(SprintActual {
            started_at: Some(chrono::Utc::now().to_rfc3339()),
            closed_at: None,
        }),
        ..Default::default()
    };
    let outcome = SprintService::create(&mut storage, sprint, None).expect("create sprint");
    let sprint_id = outcome.record.id;

    // Create a task — automation should add it to the active sprint
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Sprint auto-add".to_string(),
            project: Some("SP".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    // Re-read task to see automation effects
    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert!(
        refreshed.sprints.contains(&sprint_id),
        "task should be in sprint #{}: sprints={:?}",
        sprint_id,
        refreshed.sprints
    );

    // Create a second task that's already in a sprint — rule should NOT match
    let second = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Already in sprint".to_string(),
            project: Some("SP".to_string()),
            sprints: vec![sprint_id],
            ..Default::default()
        },
    )
    .expect("create second task");

    let refreshed2 = TaskService::get(&storage, &second.id, None).expect("get second task");
    // Should still be in the sprint (was already there)
    assert!(
        refreshed2.sprints.contains(&sprint_id),
        "second task should keep sprint membership"
    );
}

#[test]
fn relationship_action_add_and_remove() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    fixtures.create_config_in_dir(&fixtures.tasks_root, "default:\n  members:\n    - alice\n");

    // Automation: on created, add depends_on and related relationships
    let yaml = r#"
automation:
  rules:
    - name: add-rels
      on:
        created:
          add:
            depends_on: "RL-1"
            related: ["RL-1"]
"#;
    AutomationService::set(&fixtures.tasks_root, Some("RL"), yaml).expect("set automation");
    let mut storage = fixtures.create_storage();

    // Create first task (RL-1) — this one will be the target of relationships
    let _t1 = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "First task".to_string(),
            project: Some("RL".to_string()),
            ..Default::default()
        },
    )
    .expect("create first task");

    // Create second task (RL-2) — automation should add relationships
    let t2 = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Second task".to_string(),
            project: Some("RL".to_string()),
            ..Default::default()
        },
    )
    .expect("create second task");

    let refreshed = TaskService::get(&storage, &t2.id, None).expect("get second task");
    assert!(
        refreshed
            .relationships
            .depends_on
            .contains(&"RL-1".to_string()),
        "should have depends_on: RL-1, got {:?}",
        refreshed.relationships.depends_on
    );
    assert!(
        refreshed
            .relationships
            .related
            .contains(&"RL-1".to_string()),
        "should have related: RL-1, got {:?}",
        refreshed.relationships.related
    );

    // Now update automation to remove the depends_on on update
    let yaml2 = r#"
automation:
  rules:
    - name: remove-rels
      when:
        status: InProgress
      on:
        updated:
          remove:
            depends_on: "RL-1"
"#;
    AutomationService::set(&fixtures.tasks_root, Some("RL"), yaml2).expect("set automation 2");

    // Update the second task status to InProgress
    let _updated = TaskService::update(
        &mut storage,
        &t2.id,
        TaskUpdate {
            status: Some(lotar::types::TaskStatus::new("InProgress")),
            ..Default::default()
        },
    )
    .expect("update task");

    let refreshed2 = TaskService::get(&storage, &t2.id, None).expect("get second task after");
    assert!(
        !refreshed2
            .relationships
            .depends_on
            .contains(&"RL-1".to_string()),
        "depends_on should be removed, got {:?}",
        refreshed2.relationships.depends_on
    );
    // related was not removed so should still be there
    assert!(
        refreshed2
            .relationships
            .related
            .contains(&"RL-1".to_string()),
        "related should still be there, got {:?}",
        refreshed2.relationships.related
    );
}

#[cfg(unix)]
#[test]
fn review_transitions_require_reporter_identity() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default:\n  reporter: reviewer\nissue:\n  states: [Todo, InProgress, Review, Done]\n  priorities: [Low, Medium]\n  types: [Feature]\n",
    );
    lotar::utils::identity::invalidate_identity_cache(Some(fixtures.tasks_root.as_path()));

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Human review gate".to_string(),
            project: Some("REV".to_string()),
            reporter: Some("sam".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update_with_context(
        &mut storage,
        &created.id,
        TaskUpdate {
            status: Some(TaskStatus::new("Review")),
            assignee: Some("sam".to_string()),
            ..Default::default()
        },
        TaskUpdateContext::automation_disabled(),
    )
    .expect("set task to review");

    let err = TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            status: Some(TaskStatus::new("InProgress")),
            assignee: Some("implement".to_string()),
            ..Default::default()
        },
    )
    .expect_err("non-reporter should be blocked");

    assert!(
        err.to_string().contains("Only reporter 'sam'"),
        "unexpected error: {err}"
    );
}

#[cfg(unix)]
#[test]
fn reporter_can_advance_review_ticket() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default:\n  reporter: sam\nissue:\n  states: [Todo, InProgress, Review, Done]\n  priorities: [Low, Medium]\n  types: [Feature]\n",
    );
    lotar::utils::identity::invalidate_identity_cache(Some(fixtures.tasks_root.as_path()));

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Reporter review approval".to_string(),
            project: Some("REV".to_string()),
            reporter: Some("sam".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update_with_context(
        &mut storage,
        &created.id,
        TaskUpdate {
            status: Some(TaskStatus::new("Review")),
            assignee: Some("sam".to_string()),
            ..Default::default()
        },
        TaskUpdateContext::automation_disabled(),
    )
    .expect("set task to review");

    let updated = TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            status: Some(TaskStatus::new("InProgress")),
            assignee: Some("implement".to_string()),
            ..Default::default()
        },
    )
    .expect("reporter should be allowed to advance review");

    assert_eq!(updated.status.as_str(), "InProgress");
    assert_eq!(updated.assignee.as_deref(), Some("implement"));
}

#[cfg(unix)]
#[test]
fn clarification_handoff_to_reporter_survives_job_failure() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        "default:\n  reporter: reviewer\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\n  priorities: [Low, Medium]\n  types: [Feature]\nagents:\n  implement:\n    runner: command\n    command: /usr/bin/true\n",
    );

    let automation_yaml = "automation:\n  rules:\n    - name: Implement failure fallback\n      when:\n        assignee: \"@implement\"\n      on:\n        error:\n          set:\n            status: HelpNeeded\n            assignee: \"@me\"\n          add:\n            tags: [agent-error]\n";
    AutomationService::set(&fixtures.tasks_root, None, automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Clarification handoff".to_string(),
            project: Some("ASK".to_string()),
            reporter: Some("sam".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("@implement".to_string()),
            ..Default::default()
        },
    )
    .expect("assign implement agent");

    TaskService::update_with_context(
        &mut storage,
        &created.id,
        TaskUpdate {
            status: Some(TaskStatus::new("HelpNeeded")),
            assignee: Some("sam".to_string()),
            ..Default::default()
        },
        TaskUpdateContext::automation_disabled(),
    )
    .expect("agent hands ticket back to reporter");

    AutomationService::apply_job_event(
        fixtures.tasks_root.as_path(),
        &created.id,
        AutomationEvent::JobFailed,
        Some(AutomationJobContext {
            job_id: "job-clarify".to_string(),
            runner: "command".to_string(),
            agent: Some("implement".to_string()),
            worktree_path: None,
            worktree_branch: None,
        }),
    )
    .expect("apply job failure");

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert_eq!(refreshed.status.as_str(), "HelpNeeded");
    assert_eq!(refreshed.assignee.as_deref(), Some("sam"));
    assert!(
        !refreshed.tags.iter().any(|tag| tag == "agent-error"),
        "clarification handoff should prevent fallback error automation: {:?}",
        refreshed.tags
    );
}

#[cfg(unix)]
#[test]
fn running_agent_can_hand_ticket_back_for_clarification() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();

    let script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "clarify-agent.sh",
        &format!(
            "#!/bin/sh\n\
set -eu\n\
\"{}\" --tasks-dir \"$LOTAR_TASKS_DIR\" comment \"$LOTAR_TICKET_ID\" \"Need clarification: should this keep the old behavior or switch now?\"\n\
\"{}\" --tasks-dir \"$LOTAR_TASKS_DIR\" status \"$LOTAR_TICKET_ID\" HelpNeeded\n\
\"{}\" --tasks-dir \"$LOTAR_TASKS_DIR\" assignee \"$LOTAR_TICKET_ID\" sam@example.com\n\
exit 1\n",
            env!("CARGO_BIN_EXE_lotar"),
            env!("CARGO_BIN_EXE_lotar"),
            env!("CARGO_BIN_EXE_lotar"),
        ),
    );

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "default:\n  reporter: reviewer\nissue:\n  states: [Todo, InProgress, HelpNeeded, Done]\n  priorities: [Low, Medium]\n  types: [Feature]\nagents:\n  implement:\n    runner: command\n    command: /bin/sh\n    args:\n      - \"{}\"\n",
            script.to_string_lossy()
        ),
    );

    let automation_yaml = "automation:\n  rules:\n    - name: Implement failure fallback\n      when:\n        assignee: \"@implement\"\n      on:\n        job_start:\n          set:\n            status: InProgress\n        error:\n          set:\n            status: HelpNeeded\n            assignee: \"@me\"\n          add:\n            tags: [agent-error]\n";
    AutomationService::set(&fixtures.tasks_root, None, automation_yaml).expect("set automation");

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Clarification via running agent".to_string(),
            project: Some("ASK".to_string()),
            reporter: Some("sam@example.com".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update(
        &mut storage,
        &created.id,
        TaskUpdate {
            assignee: Some("@implement".to_string()),
            ..Default::default()
        },
    )
    .expect("assign implement agent");

    let job = job_for_ticket(&created.id);
    assert!(
        wait_for_job_status(&job.id, "failed", 3000),
        "clarification job did not fail in time"
    );

    let refreshed = TaskService::get(&storage, &created.id, None).expect("get task");
    assert_eq!(refreshed.status.as_str(), "HelpNeeded");
    assert_eq!(refreshed.assignee.as_deref(), Some("sam@example.com"));
    assert!(
        refreshed
            .comments
            .iter()
            .any(|comment| comment.text.contains("Need clarification:")),
        "expected clarification comment, got {:?}",
        refreshed.comments
    );
    assert!(
        !refreshed.tags.iter().any(|tag| tag == "agent-error"),
        "self-handoff should avoid fallback error automation: {:?}",
        refreshed.tags
    );
}

#[cfg(unix)]
#[test]
fn merge_jobs_require_worktrees() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();
    let merge_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "stub-agent-merge.sh",
        "#!/bin/sh\n\
echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"merge-1\"}'\n\
echo '{\"type\":\"assistant\",\"message\":{\"content\":[{\"text\":\"merge done\"}]},\"session_id\":\"merge-1\"}'\n\
exit 0\n",
    );

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "default:\n  project: MRG\nissue:\n  states: [Todo, Merging, Done]\n  priorities: [Low]\n  types: [Feature]\nagent:\n  worktree:\n    enabled: false\nagents:\n  merge:\n    runner: claude\n    command: \"{}\"\n",
            merge_script.to_string_lossy()
        ),
    );

    let mut storage = fixtures.create_storage();
    let created = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Worktree gated merge".to_string(),
            project: Some("MRG".to_string()),
            reporter: Some("sam".to_string()),
            ..Default::default()
        },
    )
    .expect("create task");

    TaskService::update_with_context(
        &mut storage,
        &created.id,
        TaskUpdate {
            status: Some(TaskStatus::new("Merging")),
            assignee: Some("merge".to_string()),
            ..Default::default()
        },
        TaskUpdateContext::automation_disabled(),
    )
    .expect("set task to merging");

    let err = AgentJobService::start_job_with_tasks_dir(
        AgentJobCreateRequest {
            ticket_id: created.id.clone(),
            prompt: "merge this ticket".to_string(),
            runner: None,
            agent: Some("merge".to_string()),
        },
        fixtures.tasks_root.as_path(),
    )
    .expect_err("merge job should require worktrees");

    assert!(
        err.to_string().contains("agent.worktree.enabled=true"),
        "unexpected error: {err}"
    );
}

#[cfg(unix)]
#[test]
fn merge_jobs_are_serialized_even_with_parallel_slots() {
    let _guard = lock_agent_tests();
    enable_server_mode();
    let fixtures = TestFixtures::new();
    init_git_repository(fixtures.get_temp_path());

    let merge_script = write_stub_agent_script(
        fixtures.get_temp_path(),
        "stub-agent-merge-serial.sh",
        "#!/bin/sh\n\
echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"merge-serial\"}'\n\
sleep 1\n\
echo '{\"type\":\"assistant\",\"message\":{\"content\":[{\"text\":\"serialized merge done\"}]},\"session_id\":\"merge-serial\"}'\n\
exit 0\n",
    );

    fixtures.create_config_in_dir(
        &fixtures.tasks_root,
        &format!(
            "default:\n  project: MRG\nissue:\n  states: [Todo, Merging, Done]\n  priorities: [Low]\n  types: [Feature]\nagent:\n  worktree:\n    enabled: true\n    max_parallel_jobs: 4\n    cleanup_on_done: true\nagents:\n  merge:\n    runner: claude\n    command: \"{}\"\n",
            merge_script.to_string_lossy()
        ),
    );

    let mut storage = fixtures.create_storage();
    let first = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Serialized merge one".to_string(),
            project: Some("MRG".to_string()),
            reporter: Some("sam".to_string()),
            ..Default::default()
        },
    )
    .expect("create first task");
    let second = TaskService::create(
        &mut storage,
        TaskCreate {
            title: "Serialized merge two".to_string(),
            project: Some("MRG".to_string()),
            reporter: Some("sam".to_string()),
            ..Default::default()
        },
    )
    .expect("create second task");

    for ticket_id in [&first.id, &second.id] {
        TaskService::update_with_context(
            &mut storage,
            ticket_id,
            TaskUpdate {
                status: Some(TaskStatus::new("Merging")),
                assignee: Some("merge".to_string()),
                ..Default::default()
            },
            TaskUpdateContext::automation_disabled(),
        )
        .expect("set task to merging");
    }

    let first_job = AgentJobService::start_job_with_tasks_dir(
        AgentJobCreateRequest {
            ticket_id: first.id.clone(),
            prompt: "merge first".to_string(),
            runner: None,
            agent: Some("merge".to_string()),
        },
        fixtures.tasks_root.as_path(),
    )
    .expect("start first merge job");
    let second_job = AgentJobService::start_job_with_tasks_dir(
        AgentJobCreateRequest {
            ticket_id: second.id.clone(),
            prompt: "merge second".to_string(),
            runner: None,
            agent: Some("merge".to_string()),
        },
        fixtures.tasks_root.as_path(),
    )
    .expect("start second merge job");

    assert!(wait_for_job_status(&first_job.id, "running", 1500));
    assert!(wait_for_job_status(&second_job.id, "queued", 500));

    assert!(wait_for_job_status(&first_job.id, "completed", 4000));
    assert!(wait_for_job_status(&second_job.id, "completed", 5000));
}
