//! DEV-117 characterization of push orchestration for both providers.
//!
//! Each scenario runs `perform_push` against a scripted local HTTP remote and
//! renders a trace of report entries, warnings, the remote request sequence
//! and the pending-link journal. The expected traces were captured from the
//! provider-specific implementation before it was consolidated, so the shared
//! orchestration must reproduce them exactly.

use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

struct ScriptedRemote {
    address: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stopped: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ScriptedRemote {
    /// `failures`: (method, path substring) pairs answered with HTTP 500.
    fn new(provider: SyncProvider, failures: Vec<(&'static str, &'static str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let (log, stop) = (requests.clone(), stopped.clone());
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header == "\r\n" || header.is_empty() {
                        break;
                    }
                    if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:")
                    {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let parts: Vec<_> = first.split_whitespace().collect();
                let (method, path) = (parts[0].to_string(), parts[1].to_string());
                log.lock().unwrap().push(format!("{method} {path}"));

                let failing = failures
                    .iter()
                    .any(|(m, p)| *m == method && path.contains(p));
                let (status, payload) = if failing {
                    ("500 Internal Server Error", json!({"message": "boom"}))
                } else {
                    ("200 OK", respond(provider, &method, &path))
                };
                let payload = payload.to_string();
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                )
                .unwrap();
            }
        });
        Self {
            address,
            requests,
            stopped,
            thread: Some(thread),
        }
    }

    fn client(&self, provider: SyncProvider) -> SyncClient {
        SyncClient::new(AuthContext {
            provider,
            api_base: format!("http://{}", self.address),
            auth_header: None,
            user_agent: "sync-characterization-test".into(),
        })
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for ScriptedRemote {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn respond(provider: SyncProvider, method: &str, path: &str) -> JsonValue {
    match provider {
        SyncProvider::Jira => {
            if method == "POST" && path == "/rest/api/3/issue" {
                json!({"key": "ENG-9"})
            } else if path.contains("/transitions") {
                json!({"transitions": [{"id": "31", "to": {"name": "Done"}}]})
            } else if path.contains("/issuetype/project") {
                json!([{"name": "Task"}])
            } else if path.contains("/project/") {
                json!({"id": "1"})
            } else {
                json!({"key": "ENG-7", "fields": {"summary": "Remote title"}})
            }
        }
        SyncProvider::Github => {
            if method == "POST" && path == "/repos/org/repo/issues" {
                json!({"number": 9})
            } else {
                json!({"number": 7, "title": "Remote title", "state": "open", "labels": []})
            }
        }
    }
}

fn fixture(
    provider: SyncProvider,
) -> (tempfile::TempDir, TasksDirectoryResolver, SyncRemoteConfig) {
    let temp = tempfile::tempdir().unwrap();
    let resolver = TasksDirectoryResolver {
        path: temp.path().to_path_buf(),
        source: crate::workspace::TasksDirectorySource::CommandLineFlag,
    };
    fs::write(
        crate::utils::paths::global_config_path(temp.path()),
        "default.project: TEST\nauto.set_reporter: false\nauto.assign_on_status: false\n",
    )
    .unwrap();
    let remote = SyncRemoteConfig {
        provider,
        project: (provider == SyncProvider::Jira).then(|| "ENG".into()),
        repo: (provider == SyncProvider::Github).then(|| "org/repo".into()),
        filter: None,
        auth_profile: None,
        mapping: match provider {
            SyncProvider::Jira => HashMap::from([
                (
                    "title".to_string(),
                    SyncFieldMapping::Simple("summary".into()),
                ),
                (
                    "status".to_string(),
                    SyncFieldMapping::Simple("status".into()),
                ),
            ]),
            SyncProvider::Github => HashMap::from([(
                "title".to_string(),
                SyncFieldMapping::Simple("title".into()),
            )]),
        },
    };
    (temp, resolver, remote)
}

fn new_recorder(dry_run: bool) -> SyncReportRecorder {
    SyncReportRecorder::new(SyncRunContext {
        run_id: "characterization".into(),
        started_at: Utc::now().to_rfc3339(),
        direction: SyncDirection::Push,
        provider: "test".into(),
        remote: "test".into(),
        project: Some("TEST".into()),
        dry_run,
    })
}

fn task_with_reference(
    resolver: &TasksDirectoryResolver,
    title: &str,
    kind: &str,
    reference: Option<&str>,
) -> String {
    let mut storage = Storage::new(&resolver.path);
    let task = TaskService::create(
        &mut storage,
        TaskCreate {
            title: title.into(),
            project: Some("TEST".into()),
            ..Default::default()
        },
    )
    .unwrap();
    if let Some(reference) = reference {
        attach_sync_reference(&mut storage, &task.id, kind, reference).unwrap();
    }
    task.id
}

struct Outcome {
    entries: Vec<String>,
    warnings: Vec<String>,
    requests: Vec<String>,
    journal: String,
}

fn run_push(
    resolver: &TasksDirectoryResolver,
    remote: &SyncRemoteConfig,
    server: Option<&ScriptedRemote>,
    task_id: Option<&str>,
    dry_run: bool,
) -> Outcome {
    let client = server.map(|s| s.client(remote.provider));
    let mut journal = SyncJournal::open(&resolver.path, dry_run).unwrap();
    let mut recorder = new_recorder(dry_run);
    let mut warnings = Vec::new();
    perform_push(
        resolver,
        remote,
        Some("TEST"),
        task_id,
        dry_run,
        client.as_ref(),
        &mut recorder,
        &mut warnings,
        &mut journal,
    )
    .unwrap();
    let host = server.map(|s| s.address.to_string()).unwrap_or_default();
    let root = resolver.path.display().to_string();
    let scrub = |text: &str| {
        let text = text.replace(&root, "ROOT");
        if host.is_empty() {
            text
        } else {
            text.replace(&host, "HOST")
        }
    };
    let entries = recorder
        .entries
        .iter()
        .map(|entry| {
            scrub(&format!(
                "{} task={} ref={} msg={} fields={}",
                entry.status,
                entry.task_id.as_deref().unwrap_or("-"),
                entry.reference.as_deref().unwrap_or("-"),
                entry.message.as_deref().unwrap_or("-"),
                entry.fields.join(",")
            ))
        })
        .collect();
    let journal_state = format!(
        "pending={} refs=[{}]",
        journal.links.len(),
        journal
            .links
            .iter()
            .map(|link| link.reference.clone().unwrap_or_else(|| "-".into()))
            .collect::<Vec<_>>()
            .join(",")
    );
    Outcome {
        entries,
        warnings: warnings.iter().map(|w| scrub(w)).collect(),
        requests: server.map(ScriptedRemote::requests).unwrap_or_default(),
        journal: journal_state,
    }
}

fn reference_for(provider: SyncProvider, number: u32) -> (&'static str, String) {
    match provider {
        SyncProvider::Jira => ("jira", format!("ENG-{number}")),
        SyncProvider::Github => ("github", format!("org/repo#{number}")),
    }
}

fn stored_references(resolver: &TasksDirectoryResolver, id: &str) -> Vec<String> {
    TaskService::get(&Storage::new(&resolver.path), id, None)
        .unwrap()
        .references
        .iter()
        .filter_map(|r| r.jira.clone().or_else(|| r.github.clone()))
        .collect()
}

#[test]
fn push_update_with_changes_both_providers() {
    for provider in [SyncProvider::Jira, SyncProvider::Github] {
        let (_temp, resolver, remote) = fixture(provider);
        let (kind, reference) = reference_for(provider, 7);
        let id = task_with_reference(&resolver, "Local title", kind, Some(&reference));
        let server = ScriptedRemote::new(provider, vec![]);
        let out = run_push(&resolver, &remote, Some(&server), Some(&id), false);
        insta_like(provider, "update", &out);
    }
}

#[test]
fn push_update_skips_when_remote_already_matches() {
    for provider in [SyncProvider::Jira, SyncProvider::Github] {
        let (_temp, resolver, remote) = fixture(provider);
        let (kind, reference) = reference_for(provider, 7);
        let id = task_with_reference(&resolver, "Remote title", kind, Some(&reference));
        let server = ScriptedRemote::new(provider, vec![]);
        let out = run_push(&resolver, &remote, Some(&server), Some(&id), false);
        insta_like(provider, "update-no-diff", &out);
    }
}

#[test]
fn push_update_fetch_failure_warns_and_still_updates() {
    for provider in [SyncProvider::Jira, SyncProvider::Github] {
        let (_temp, resolver, remote) = fixture(provider);
        let (kind, reference) = reference_for(provider, 7);
        let id = task_with_reference(&resolver, "Local title", kind, Some(&reference));
        let fetch = match provider {
            SyncProvider::Jira => ("GET", "/rest/api/3/issue/ENG-7"),
            SyncProvider::Github => ("GET", "/repos/org/repo/issues/7"),
        };
        let server = ScriptedRemote::new(provider, vec![fetch]);
        let out = run_push(&resolver, &remote, Some(&server), Some(&id), false);
        insta_like(provider, "update-fetch-fails", &out);
    }
}

#[test]
fn push_update_failure_is_per_issue_and_later_tasks_continue() {
    for provider in [SyncProvider::Jira, SyncProvider::Github] {
        let (_temp, resolver, remote) = fixture(provider);
        let (kind, first) = reference_for(provider, 7);
        let (_, second) = reference_for(provider, 8);
        task_with_reference(&resolver, "First", kind, Some(&first));
        task_with_reference(&resolver, "Second", kind, Some(&second));
        let update = match provider {
            SyncProvider::Jira => ("PUT", "/rest/api/3/issue/ENG-7"),
            SyncProvider::Github => ("PATCH", "/repos/org/repo/issues/7"),
        };
        let server = ScriptedRemote::new(provider, vec![update]);
        let mut out = run_push(&resolver, &remote, Some(&server), None, false);
        // Task iteration order is not part of the contract; per-task results are.
        out.entries.sort();
        out.requests.sort();
        insta_like(provider, "update-partial-failure", &out);
    }
}

#[test]
fn push_dry_run_update_create_and_foreign_reference_make_no_requests() {
    for provider in [SyncProvider::Jira, SyncProvider::Github] {
        let (_temp, resolver, remote) = fixture(provider);
        let (kind, reference) = reference_for(provider, 7);
        let foreign = match provider {
            SyncProvider::Jira => "OTHER-3".to_string(),
            SyncProvider::Github => "other/repo#3".to_string(),
        };
        task_with_reference(&resolver, "Linked", kind, Some(&reference));
        task_with_reference(&resolver, "Unlinked", kind, None);
        task_with_reference(&resolver, "Foreign", kind, Some(&foreign));
        let mut out = run_push(&resolver, &remote, None, None, true);
        out.entries.sort();
        insta_like(provider, "dry-run", &out);
    }
}

#[test]
fn push_create_follows_journal_order_and_links_reference() {
    for provider in [SyncProvider::Jira, SyncProvider::Github] {
        let (_temp, resolver, remote) = fixture(provider);
        let id = task_with_reference(&resolver, "New work", "", None);
        let server = ScriptedRemote::new(provider, vec![]);
        let out = run_push(&resolver, &remote, Some(&server), Some(&id), false);
        insta_like(provider, "create", &out);
        let (_, expected) = reference_for(provider, 9);
        assert_eq!(stored_references(&resolver, &id), vec![expected]);
    }
}

#[test]
fn push_create_failure_leaves_indeterminate_pending_link() {
    for provider in [SyncProvider::Jira, SyncProvider::Github] {
        let (_temp, resolver, remote) = fixture(provider);
        let id = task_with_reference(&resolver, "New work", "", None);
        let create = match provider {
            SyncProvider::Jira => ("POST", "/rest/api/3/issue"),
            SyncProvider::Github => ("POST", "/repos/org/repo/issues"),
        };
        let server = ScriptedRemote::new(provider, vec![create]);
        let out = run_push(&resolver, &remote, Some(&server), Some(&id), false);
        insta_like(provider, "create-fails", &out);
        assert!(stored_references(&resolver, &id).is_empty());
    }
}

/// Compare a scenario trace with its recorded expectation.
fn insta_like(provider: SyncProvider, scenario: &str, out: &Outcome) {
    let mut actual = String::new();
    for line in &out.entries {
        actual.push_str(&format!("entry {line}\n"));
    }
    for line in &out.warnings {
        actual.push_str(&format!("warning {line}\n"));
    }
    for line in &out.requests {
        actual.push_str(&format!("request {line}\n"));
    }
    actual.push_str(&format!("journal {}\n", out.journal));
    let key = format!("{}:{scenario}", provider.as_str_for_tests());
    if let Ok(dir) = std::env::var("RECORD_SYNC_TRACES") {
        fs::write(format!("{dir}/{key}.trace"), &actual).unwrap();
        return;
    }
    let expected = expected_trace(&key);
    assert_eq!(actual, expected, "trace mismatch for {key}");
}

trait ProviderName {
    fn as_str_for_tests(&self) -> &'static str;
}

impl ProviderName for SyncProvider {
    fn as_str_for_tests(&self) -> &'static str {
        match self {
            SyncProvider::Jira => "jira",
            SyncProvider::Github => "github",
        }
    }
}

fn expected_trace(key: &str) -> String {
    let trace = match key {
        "github:create-fails" => {
            r#"entry failed task=TEST-1 ref=- msg=Failed to create GitHub issue: Validation error: Remote API error (HTTP 500): {"message":"boom"} fields=
warning Failed to create GitHub issue for TEST-1: Validation error: Remote API error (HTTP 500): {"message":"boom"}; Validation error: Indeterminate sync creation in ROOT/.sync-pending.json. Creation will not be retried. Inspect the remote issues and local tasks, fill the missing task_id/reference in this journal with the verified identity, then retry the same project/remote. Remove an intent only after verifying creation did not occur.
request POST /repos/org/repo/issues
journal pending=1 refs=[-]
"#
        }
        "github:create" => {
            r#"entry created task=TEST-1 ref=org/repo#9 msg=Created fields: title fields=
request POST /repos/org/repo/issues
journal pending=0 refs=[]
"#
        }
        "github:dry-run" => {
            r#"entry created task=TEST-2 ref=- msg=Dry run: would create title fields=
entry skipped task=TEST-3 ref=other/repo#3 msg=Reference does not match remote fields=
entry updated task=TEST-1 ref=org/repo#7 msg=Dry run: would update fields=title
journal pending=0 refs=[]
"#
        }
        "github:update-fetch-fails" => {
            r#"entry updated task=TEST-1 ref=org/repo#7 msg=- fields=title
warning GitHub issue org/repo#7 fetch failed; skipping diff: Validation error: Remote API error (HTTP 500): {"message":"boom"}
request GET /repos/org/repo/issues/7
request PATCH /repos/org/repo/issues/7
journal pending=0 refs=[]
"#
        }
        "github:update-no-diff" => {
            r#"entry skipped task=TEST-1 ref=org/repo#7 msg=No changes to push fields=
request GET /repos/org/repo/issues/7
journal pending=0 refs=[]
"#
        }
        "github:update-partial-failure" => {
            r#"entry failed task=TEST-1 ref=org/repo#7 msg=Failed to update GitHub issue: Validation error: Remote API error (HTTP 500): {"message":"boom"} fields=
entry updated task=TEST-2 ref=org/repo#8 msg=- fields=title
warning Failed to update GitHub issue org/repo#7: Validation error: Remote API error (HTTP 500): {"message":"boom"}
request GET /repos/org/repo/issues/7
request GET /repos/org/repo/issues/8
request PATCH /repos/org/repo/issues/7
request PATCH /repos/org/repo/issues/8
journal pending=0 refs=[]
"#
        }
        "github:update" => {
            r#"entry updated task=TEST-1 ref=org/repo#7 msg=- fields=title
request GET /repos/org/repo/issues/7
request PATCH /repos/org/repo/issues/7
journal pending=0 refs=[]
"#
        }
        "jira:create-fails" => {
            r#"entry failed task=TEST-1 ref=- msg=Failed to create Jira issue: Validation error: Remote API error (HTTP 500): {"message":"boom"} fields=
warning Failed to create Jira issue for TEST-1: Validation error: Remote API error (HTTP 500): {"message":"boom"}; Validation error: Indeterminate sync creation in ROOT/.sync-pending.json. Creation will not be retried. Inspect the remote issues and local tasks, fill the missing task_id/reference in this journal with the verified identity, then retry the same project/remote. Remove an intent only after verifying creation did not occur.
request GET /rest/api/3/project/ENG
request GET /rest/api/3/issuetype/project?projectId=1
request POST /rest/api/3/issue
journal pending=1 refs=[-]
"#
        }
        "jira:create" => {
            r#"entry created task=TEST-1 ref=ENG-9 msg=Created fields: issuetype, project, status, summary fields=
warning Jira status transition for ENG-9 failed: Validation error: No Jira transition named 'Todo'
request GET /rest/api/3/project/ENG
request GET /rest/api/3/issuetype/project?projectId=1
request POST /rest/api/3/issue
request GET /rest/api/3/issue/ENG-9/transitions
journal pending=0 refs=[]
"#
        }
        "jira:dry-run" => {
            r#"entry created task=TEST-2 ref=- msg=Dry run: would create issuetype, project, status, summary fields=
entry skipped task=TEST-3 ref=OTHER-3 msg=Reference does not match remote fields=
entry updated task=TEST-1 ref=ENG-7 msg=Dry run: would update fields=summary,status
journal pending=0 refs=[]
"#
        }
        "jira:update-fetch-fails" => {
            r#"entry updated task=TEST-1 ref=ENG-7 msg=- fields=summary,status
warning Jira issue ENG-7 fetch failed; skipping diff: Validation error: Remote API error (HTTP 500): {"message":"boom"}
warning Jira status transition for ENG-7 failed: Validation error: Remote API error (HTTP 500): {"message":"boom"}
request GET /rest/api/3/project/ENG
request GET /rest/api/3/issuetype/project?projectId=1
request GET /rest/api/3/issue/ENG-7?fields=summary%2Cstatus
request PUT /rest/api/3/issue/ENG-7
request GET /rest/api/3/issue/ENG-7/transitions
journal pending=0 refs=[]
"#
        }
        "jira:update-no-diff" => {
            r#"entry updated task=TEST-1 ref=ENG-7 msg=- fields=status
warning Jira status transition for ENG-7 failed: Validation error: No Jira transition named 'Todo'
request GET /rest/api/3/project/ENG
request GET /rest/api/3/issuetype/project?projectId=1
request GET /rest/api/3/issue/ENG-7?fields=summary%2Cstatus
request GET /rest/api/3/issue/ENG-7/transitions
journal pending=0 refs=[]
"#
        }
        "jira:update-partial-failure" => {
            r#"entry failed task=TEST-1 ref=ENG-7 msg=Failed to update Jira issue: Validation error: Remote API error (HTTP 500): {"message":"boom"} fields=
entry updated task=TEST-2 ref=ENG-8 msg=- fields=summary,status
warning Jira status transition for ENG-8 failed: Validation error: No Jira transition named 'Todo'
warning Failed to update Jira issue ENG-7: Validation error: Remote API error (HTTP 500): {"message":"boom"}
request GET /rest/api/3/issue/ENG-7?fields=summary%2Cstatus
request GET /rest/api/3/issue/ENG-8/transitions
request GET /rest/api/3/issue/ENG-8?fields=summary%2Cstatus
request GET /rest/api/3/issuetype/project?projectId=1
request GET /rest/api/3/project/ENG
request PUT /rest/api/3/issue/ENG-7
request PUT /rest/api/3/issue/ENG-8
journal pending=0 refs=[]
"#
        }
        "jira:update" => {
            r#"entry updated task=TEST-1 ref=ENG-7 msg=- fields=summary,status
warning Jira status transition for ENG-7 failed: Validation error: No Jira transition named 'Todo'
request GET /rest/api/3/project/ENG
request GET /rest/api/3/issuetype/project?projectId=1
request GET /rest/api/3/issue/ENG-7?fields=summary%2Cstatus
request PUT /rest/api/3/issue/ENG-7
request GET /rest/api/3/issue/ENG-7/transitions
journal pending=0 refs=[]
"#
        }
        _ => return format!("<no expectation recorded for {key}>"),
    };
    trace.to_string()
}
