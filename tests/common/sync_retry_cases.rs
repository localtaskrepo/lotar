use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, atomic::AtomicBool};

struct MockRemote {
    address: std::net::SocketAddr,
    creates: Arc<AtomicU64>,
    writes: Arc<AtomicU64>,
    stopped: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MockRemote {
    fn new(provider: SyncProvider, ambiguous: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let creates = Arc::new(AtomicU64::new(0));
        let writes = Arc::new(AtomicU64::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let (count, mutations, stop) = (creates.clone(), writes.clone(), stopped.clone());
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
                reader.read_exact(&mut vec![0; length]).unwrap();
                let parts: Vec<_> = first.split_whitespace().collect();
                let method = parts[0];
                let path = parts[1];
                if method != "GET" {
                    mutations.fetch_add(1, Ordering::SeqCst);
                }
                let is_create = method == "POST"
                    && (path == "/rest/api/3/issue" || path == "/repos/org/repo/issues");
                if is_create {
                    count.fetch_add(1, Ordering::SeqCst);
                    if ambiguous {
                        continue;
                    } // Remote committed, response connection lost.
                }
                let issue = match provider {
                    SyncProvider::Jira => {
                        json!({"key":"ENG-7", "fields":{"summary":"Sync safety"}})
                    }
                    SyncProvider::Github => {
                        json!({"number":7, "title":"Sync safety", "state":"open"})
                    }
                };
                let payload = if is_create {
                    issue
                } else if path.contains("/search/jql") {
                    json!({"issues":[issue], "total":1})
                } else if path.starts_with("/repos/org/repo/issues?") {
                    json!([issue])
                } else if path.contains("/issuetype/project") {
                    json!([{"name":"Task"}])
                } else if path.contains("/project/") {
                    json!({"id":"1"})
                } else {
                    issue
                }
                .to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", payload.len(), payload).unwrap();
            }
        });
        Self {
            address,
            creates,
            writes,
            stopped,
            thread: Some(thread),
        }
    }

    fn client(&self, provider: SyncProvider) -> SyncClient {
        SyncClient::new(AuthContext {
            provider,
            api_base: format!("http://{}", self.address),
            auth_header: None,
            user_agent: "sync-safety-test".into(),
        })
    }
}

impl Drop for MockRemote {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        self.thread.take().unwrap().join().unwrap();
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
        "default.project: TEST\nauto.set.reporter: false\nauto.assign: false\n",
    )
    .unwrap();
    let remote = SyncRemoteConfig {
        provider,
        project: (provider == SyncProvider::Jira).then(|| "ENG".into()),
        repo: (provider == SyncProvider::Github).then(|| "org/repo".into()),
        filter: None,
        auth_profile: None,
        mapping: HashMap::new(),
    };
    (temp, resolver, remote)
}

fn recorder(direction: SyncDirection, dry_run: bool) -> SyncReportRecorder {
    SyncReportRecorder::new(SyncRunContext {
        run_id: "test".into(),
        started_at: Utc::now().to_rfc3339(),
        direction,
        provider: "test".into(),
        remote: "test".into(),
        project: Some("TEST".into()),
        dry_run,
    })
}

fn local_task(resolver: &TasksDirectoryResolver) -> TaskDTO {
    TaskService::create(
        &mut Storage::new(&resolver.path),
        TaskCreate {
            title: "Sync safety".into(),
            project: Some("TEST".into()),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn sync_creation_link_failure_recovers_once_both_providers_and_directions() {
    for provider in [SyncProvider::Github, SyncProvider::Jira] {
        for direction in [SyncDirection::Push, SyncDirection::Pull] {
            let (_temp, resolver, remote) = fixture(provider);
            let server = MockRemote::new(provider, false);
            let client = server.client(provider);
            let task = (direction == SyncDirection::Push).then(|| local_task(&resolver));
            let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
            let mut first = recorder(direction, false);
            FAIL_SYNC_LINK.set(true);
            match direction {
                SyncDirection::Push => perform_push(
                    &resolver,
                    &remote,
                    Some("TEST"),
                    task.as_ref().map(|t| t.id.as_str()),
                    false,
                    Some(&client),
                    &mut first,
                    &mut vec![],
                    &mut journal,
                )
                .unwrap(),
                SyncDirection::Pull => perform_pull(
                    &resolver,
                    &remote,
                    "TEST",
                    None,
                    false,
                    &client,
                    &mut first,
                    &mut vec![],
                    &mut journal,
                )
                .unwrap(),
            }
            assert_eq!(first.summary.failed, 1);
            assert_eq!(journal.links.len(), 1);
            let id = journal.links[0].task_id.clone().unwrap();
            assert!(journal.links[0].reference.is_some());
            drop(journal);
            let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
            // Targeted retry must recover the link before task lookup/reference indexing.
            assert_eq!(id, "TEST-1");
            let retry_alias = "TEST-01";
            journal
                .reconcile(
                    &resolver,
                    &pending_scope(&remote, &client),
                    Some("TEST"),
                    Some(retry_alias),
                    &remote,
                )
                .unwrap();
            let mut second = recorder(direction, false);
            match direction {
                SyncDirection::Push => perform_push(
                    &resolver,
                    &remote,
                    Some("TEST"),
                    Some(retry_alias),
                    false,
                    Some(&client),
                    &mut second,
                    &mut vec![],
                    &mut journal,
                )
                .unwrap(),
                SyncDirection::Pull => {
                    perform_pull(
                        &resolver,
                        &remote,
                        "TEST",
                        Some(retry_alias),
                        false,
                        &client,
                        &mut second,
                        &mut vec![],
                        &mut journal,
                    )
                    .unwrap();
                    perform_pull(
                        &resolver,
                        &remote,
                        "TEST",
                        None,
                        false,
                        &client,
                        &mut second,
                        &mut vec![],
                        &mut journal,
                    )
                    .unwrap();
                }
            }
            assert!(journal.links.is_empty());
            assert!(
                second
                    .entries
                    .iter()
                    .filter_map(|entry| entry.task_id.as_deref())
                    .all(|id| id == "TEST-1")
            );
            assert_eq!(
                server.creates.load(Ordering::SeqCst),
                u64::from(direction == SyncDirection::Push)
            );
            let tasks = TaskService::list(
                &Storage::new(&resolver.path),
                &TaskListFilter {
                    project: Some("TEST".into()),
                    ..Default::default()
                },
            );
            assert_eq!(tasks.len(), 1);
            assert!(matches!(
                determine_reference_state(&remote, &tasks[0].1),
                ReferenceState::Matching(_)
            ));
        }
    }
}

#[test]
fn sync_ambiguous_remote_success_is_persisted_and_fails_closed() {
    for provider in [SyncProvider::Github, SyncProvider::Jira] {
        let (_temp, resolver, remote) = fixture(provider);
        let server = MockRemote::new(provider, true);
        let client = server.client(provider);
        let task = local_task(&resolver);
        let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
        let mut warnings = vec![];
        perform_push(
            &resolver,
            &remote,
            Some("TEST"),
            Some(&task.id),
            false,
            Some(&client),
            &mut recorder(SyncDirection::Push, false),
            &mut warnings,
            &mut journal,
        )
        .unwrap();
        assert!(warnings.iter().any(|s| s.contains("Indeterminate")));
        drop(journal);
        let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
        for alias in ["TEST-01", "TEST-+1", "TEST-1-extra", "TEST-+0001-extra"] {
            assert!(Storage::new(&resolver.path).get(alias, "TEST").is_some());
            let error = journal
                .reconcile(
                    &resolver,
                    &pending_scope(&remote, &client),
                    Some("TEST"),
                    Some(alias),
                    &remote,
                )
                .unwrap_err();
            assert!(error.to_string().contains("Creation will not be retried"));
        }
        assert_eq!(server.creates.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn sync_dry_run_never_writes_journal_tasks_or_remote() {
    for provider in [SyncProvider::Github, SyncProvider::Jira] {
        for direction in [SyncDirection::Push, SyncDirection::Pull] {
            let (_temp, resolver, remote) = fixture(provider);
            let server = MockRemote::new(provider, false);
            let client = server.client(provider);
            if direction == SyncDirection::Push {
                local_task(&resolver);
            }
            let before =
                TaskService::list(&Storage::new(&resolver.path), &TaskListFilter::default()).len();
            let mut journal = SyncJournal::open(&resolver.path, true).unwrap();
            let mut report = recorder(direction, true);
            match direction {
                SyncDirection::Push => perform_push(
                    &resolver,
                    &remote,
                    Some("TEST"),
                    None,
                    true,
                    None,
                    &mut report,
                    &mut vec![],
                    &mut journal,
                )
                .unwrap(),
                SyncDirection::Pull => perform_pull(
                    &resolver,
                    &remote,
                    "TEST",
                    None,
                    true,
                    &client,
                    &mut report,
                    &mut vec![],
                    &mut journal,
                )
                .unwrap(),
            }
            assert!(!resolver.path.join(".sync-pending.json").exists());
            assert!(!resolver.path.join(".sync-pending.lock").exists());
            assert_eq!(server.writes.load(Ordering::SeqCst), 0);
            assert_eq!(
                TaskService::list(&Storage::new(&resolver.path), &TaskListFilter::default()).len(),
                before
            );
        }
    }
}

#[test]
fn sync_returned_identity_journal_failure_never_recreates() {
    for provider in [SyncProvider::Github, SyncProvider::Jira] {
        for direction in [SyncDirection::Push, SyncDirection::Pull] {
            let (_temp, resolver, remote) = fixture(provider);
            let server = MockRemote::new(provider, false);
            let client = server.client(provider);
            if direction == SyncDirection::Push {
                local_task(&resolver);
            }
            let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
            FAIL_SYNC_RESULT_SAVE.set(true);
            let result = match direction {
                SyncDirection::Push => perform_push(
                    &resolver,
                    &remote,
                    Some("TEST"),
                    None,
                    false,
                    Some(&client),
                    &mut recorder(direction, false),
                    &mut vec![],
                    &mut journal,
                ),
                SyncDirection::Pull => perform_pull(
                    &resolver,
                    &remote,
                    "TEST",
                    None,
                    false,
                    &client,
                    &mut recorder(direction, false),
                    &mut vec![],
                    &mut journal,
                ),
            };
            assert!(result.unwrap_err().to_string().contains("journal failure"));
            drop(journal);
            let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
            assert!(
                journal
                    .reconcile(
                        &resolver,
                        &pending_scope(&remote, &client),
                        Some("TEST"),
                        None,
                        &remote
                    )
                    .unwrap_err()
                    .to_string()
                    .contains("Indeterminate")
            );
            assert_eq!(
                server.creates.load(Ordering::SeqCst),
                u64::from(direction == SyncDirection::Push)
            );
            assert_eq!(
                TaskService::list(&Storage::new(&resolver.path), &TaskListFilter::default()).len(),
                1
            );
        }
    }
}

#[test]
fn sync_lock_scope_corruption_and_unreturned_local_id_fail_closed() {
    let (_temp, resolver, remote) = fixture(SyncProvider::Github);
    let server = MockRemote::new(SyncProvider::Github, false);
    let client = server.client(SyncProvider::Github);
    let scope = pending_scope(&remote, &client);
    let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
    assert!(SyncJournal::open(&resolver.path, false).is_err());
    let pending = PendingLink {
        scope: scope.clone(),
        project: "TEST".into(),
        task_id: None,
        reference: Some("org/repo#7".into()),
    };
    journal.begin(pending).unwrap();
    // Models a local create that committed but failed before returning/journaling its ID.
    local_task(&resolver);
    assert!(
        journal
            .reconcile(&resolver, &scope, Some("TEST"), None, &remote)
            .unwrap_err()
            .to_string()
            .contains("Indeterminate")
    );
    let mut other = scope.clone();
    other.endpoint_hash.push_str("another-server");
    assert!(
        journal
            .reconcile(&resolver, &other, Some("TEST"), None, &remote)
            .unwrap_err()
            .to_string()
            .contains("different remote identity")
    );
    journal
        .reconcile(&resolver, &scope, Some("OTHER"), None, &remote)
        .unwrap();
    assert_eq!(journal.links.len(), 1);
    drop(journal);
    fs::write(resolver.path.join(".sync-pending.json"), "broken").unwrap();
    assert!(SyncJournal::open(&resolver.path, false).is_err());
}

#[test]
fn sync_dry_recovery_preserves_journal_and_rejects_cross_scope_linking() {
    let (_temp, resolver, remote) = fixture(SyncProvider::Github);
    let server = MockRemote::new(SyncProvider::Github, false);
    let client = server.client(SyncProvider::Github);
    let scope = pending_scope(&remote, &client);
    let task = local_task(&resolver);
    let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
    journal
        .begin(PendingLink {
            scope: scope.clone(),
            project: "TEST".into(),
            task_id: Some(task.id.clone()),
            reference: Some("org/repo#7".into()),
        })
        .unwrap();
    let before = fs::read(&journal.path).unwrap();
    drop(journal);
    let mut journal = SyncJournal::open(&resolver.path, true).unwrap();
    journal
        .reconcile(&resolver, &scope, Some("TEST"), Some(&task.id), &remote)
        .unwrap();
    perform_pull(
        &resolver,
        &remote,
        "TEST",
        Some(&task.id),
        true,
        &client,
        &mut recorder(SyncDirection::Pull, true),
        &mut vec![],
        &mut journal,
    )
    .unwrap();
    let mut preview = recorder(SyncDirection::Pull, true);
    perform_pull(
        &resolver,
        &remote,
        "TEST",
        None,
        true,
        &client,
        &mut preview,
        &mut vec![],
        &mut journal,
    )
    .unwrap();
    assert_eq!(preview.summary.created, 0);
    assert_eq!(fs::read(&journal.path).unwrap(), before);
    let mut other = scope.clone();
    other.destination = "org/other".into();
    assert!(
        journal
            .reconcile(&resolver, &other, Some("TEST"), Some(&task.id), &remote)
            .is_err()
    );
    let local = TaskService::get(&Storage::new(&resolver.path), &task.id, Some("TEST")).unwrap();
    assert!(matches!(
        determine_reference_state(&remote, &local),
        ReferenceState::None
    ));
    assert_eq!(server.writes.load(Ordering::SeqCst), 0);
}

#[test]
fn sync_loaded_journal_and_new_intents_use_storage_identity() {
    for provider in [SyncProvider::Github, SyncProvider::Jira] {
        let (_temp, resolver, remote) = fixture(provider);
        let server = MockRemote::new(provider, true);
        let client = server.client(provider);
        let task = local_task(&resolver);
        let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
        let alias = "TEST-+0001-extra";
        assert_eq!(
            TaskService::get(&Storage::new(&resolver.path), alias, Some("TEST"))
                .unwrap()
                .id,
            alias
        );
        perform_push(
            &resolver,
            &remote,
            Some("TEST"),
            Some(alias),
            false,
            Some(&client),
            &mut recorder(SyncDirection::Push, false),
            &mut vec![],
            &mut journal,
        )
        .unwrap();
        assert_eq!(journal.links[0].task_id.as_deref(), Some(task.id.as_str()));
        // Reproduce an older journal which persisted a storage alias verbatim.
        journal.links[0].task_id = Some(alias.into());
        journal.save().unwrap();
        drop(journal);
        let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
        assert_eq!(journal.links[0].task_id.as_deref(), Some("TEST-1"));
        let mut duplicate = journal.links[0].clone();
        duplicate.task_id = Some("TEST-01".into());
        assert!(
            journal
                .begin(duplicate)
                .unwrap_err()
                .to_string()
                .contains("Indeterminate")
        );
        assert_eq!(journal.links.len(), 1);
        assert!(
            journal
                .reconcile(
                    &resolver,
                    &pending_scope(&remote, &client),
                    Some("TEST"),
                    Some("TEST-01"),
                    &remote
                )
                .is_err()
        );
        assert_eq!(server.creates.load(Ordering::SeqCst), 1);

        for invalid in ["TEST-nope", "TEST-999", "OTHER-1", "../TEST-1"] {
            journal.links[0].task_id = Some(invalid.into());
            journal.save().unwrap();
            drop(journal);
            let error = match SyncJournal::open(&resolver.path, false) {
                Ok(_) => panic!("invalid journal identity was accepted: {invalid}"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("Invalid sync recovery identity"));
            // Restore the fixture explicitly; production never drops invalid entries.
            let mut links: Vec<PendingLink> = serde_json::from_slice(
                &fs::read(resolver.path.join(".sync-pending.json")).unwrap(),
            )
            .unwrap();
            links[0].task_id = Some(task.id.clone());
            fs::write(
                resolver.path.join(".sync-pending.json"),
                serde_json::to_vec(&links).unwrap(),
            )
            .unwrap();
            journal = SyncJournal::open(&resolver.path, false).unwrap();
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn sync_replacement_io_failure_preserves_all_pending_intents() {
    use std::os::unix::ffi::OsStrExt;

    struct ImmutableJournal(std::ffi::CString);
    impl ImmutableJournal {
        fn new(path: &Path) -> Self {
            let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
            // A user-owned immutable destination permits temp creation but forces
            // the real rename(2) publication to fail, without mocking the writer.
            assert_eq!(
                unsafe { libc::chflags(path.as_ptr(), libc::UF_IMMUTABLE) },
                0,
                "{}",
                std::io::Error::last_os_error()
            );
            Self(path)
        }
    }
    impl Drop for ImmutableJournal {
        fn drop(&mut self) {
            assert_eq!(unsafe { libc::chflags(self.0.as_ptr(), 0) }, 0);
        }
    }

    for provider in [SyncProvider::Github, SyncProvider::Jira] {
        let (_temp, resolver, remote) = fixture(provider);
        let server = MockRemote::new(provider, true);
        let client = server.client(provider);
        local_task(&resolver);
        local_task(&resolver);
        let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
        perform_push(
            &resolver,
            &remote,
            Some("TEST"),
            None,
            false,
            Some(&client),
            &mut recorder(SyncDirection::Push, false),
            &mut vec![],
            &mut journal,
        )
        .unwrap();
        assert_eq!(journal.links.len(), 2);
        assert_eq!(server.creates.load(Ordering::SeqCst), 2);
        let before = fs::read(&journal.path).unwrap();
        let immutable = ImmutableJournal::new(&journal.path);
        journal.links[0].reference = Some(match provider {
            SyncProvider::Github => "org/repo#7".into(),
            SyncProvider::Jira => "ENG-7".into(),
        });
        let error = journal.save_result(0).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Failed to journal returned identity")
        );
        assert!(!error.to_string().contains("injected"));
        assert_eq!(fs::read(&journal.path).unwrap(), before);
        drop(immutable);
        drop(journal);
        let mut journal = SyncJournal::open(&resolver.path, false).unwrap();
        assert_eq!(journal.links.len(), 2);
        assert!(journal.links.iter().all(|link| link.reference.is_none()));
        for alias in ["TEST-01", "TEST-02"] {
            assert!(
                journal
                    .reconcile(
                        &resolver,
                        &pending_scope(&remote, &client),
                        Some("TEST"),
                        Some(alias),
                        &remote
                    )
                    .unwrap_err()
                    .to_string()
                    .contains("Indeterminate")
            );
        }
        assert_eq!(server.creates.load(Ordering::SeqCst), 2);
    }
}
