//! Tests for entry-point validation, auth setup, the Jira/GitHub HTTP helpers,
//! and single-task pulls (HTTP paths run against a local mock API).

use super::test_support::{MockApi, RecordedRequest, recorder, workspace};
use super::*;

fn ok(payload: JsonValue) -> (u16, JsonValue) {
    (200, payload)
}

fn query_param<'a>(request: &'a RecordedRequest, key: &str) -> Option<&'a str> {
    let (_, query) = request.path.split_once('?')?;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
}

mod auth {
    use super::*;

    fn remote(provider: SyncProvider) -> SyncRemoteConfig {
        SyncRemoteConfig {
            provider,
            project: None,
            repo: None,
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        }
    }

    fn build(
        provider: SyncProvider,
        profile: &SyncAuthProfile,
    ) -> (LoTaRResult<AuthContext>, Vec<String>) {
        let mut warnings = Vec::new();
        let result = build_auth_context(&remote(provider), "profile", profile, &mut warnings);
        (result, warnings)
    }

    fn error(provider: SyncProvider, profile: &SyncAuthProfile) -> String {
        match build(provider, profile).0 {
            Ok(_) => panic!("expected an auth error"),
            Err(err) => err.to_string(),
        }
    }

    #[test]
    fn github_defaults_to_a_bearer_token_against_the_public_api() {
        let (auth, warnings) = build(
            SyncProvider::Github,
            &SyncAuthProfile {
                token_env: Some("literal-token".into()),
                ..Default::default()
            },
        );
        let auth = auth.unwrap();
        assert_eq!(auth.api_base, "https://api.github.com");
        assert_eq!(auth.auth_header.as_deref(), Some("Bearer literal-token"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn jira_defaults_to_basic_auth_against_the_profile_url() {
        let (auth, _) = build(
            SyncProvider::Jira,
            &SyncAuthProfile {
                email_env: Some("me@example.com".into()),
                token_env: Some("secret".into()),
                base_url: Some("https://jira.example.com/".into()),
                ..Default::default()
            },
        );
        let auth = auth.unwrap();
        assert_eq!(auth.api_base, "https://jira.example.com");
        let expected = BASE64_STANDARD.encode("me@example.com:secret");
        assert_eq!(auth.auth_header, Some(format!("Basic {expected}")));
    }

    #[test]
    fn rejects_incomplete_or_unknown_settings() {
        let jira_basic = SyncAuthProfile {
            email_env: Some("me@example.com".into()),
            token_env: Some("secret".into()),
            ..Default::default()
        };
        assert!(error(SyncProvider::Jira, &jira_basic).contains("must set api_url or base_url"));

        let with_url = SyncAuthProfile {
            api_url: Some("https://jira.example.com".into()),
            ..jira_basic
        };
        let missing_email = SyncAuthProfile {
            email_env: None,
            ..with_url.clone()
        };
        assert!(error(SyncProvider::Jira, &missing_email).contains("must set email_env"));
        let missing_token = SyncAuthProfile {
            method: Some("token".into()),
            token_env: None,
            ..with_url.clone()
        };
        assert!(error(SyncProvider::Jira, &missing_token).contains("must set token_env"));
        let unknown = SyncAuthProfile {
            method: Some("OAuth".into()),
            ..with_url.clone()
        };
        assert!(error(SyncProvider::Jira, &unknown).contains("Unsupported auth method 'oauth'"));
        let blank = SyncAuthProfile {
            token_env: Some("  ".into()),
            ..with_url
        };
        assert!(error(SyncProvider::Jira, &blank).contains("token value is empty"));
    }

    #[test]
    fn unset_env_style_names_fall_back_to_the_literal_with_a_warning() {
        let (auth, warnings) = build(
            SyncProvider::Github,
            &SyncAuthProfile {
                token_env: Some("LOTAR_TEST_UNSET_TOKEN_9F2C".into()),
                ..Default::default()
            },
        );
        assert_eq!(
            auth.unwrap().auth_header.as_deref(),
            Some("Bearer LOTAR_TEST_UNSET_TOKEN_9F2C")
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("env var is not set"));
    }

    #[test]
    fn a_profile_for_another_provider_wins_with_a_warning() {
        let (auth, warnings) = build(
            SyncProvider::Github,
            &SyncAuthProfile {
                provider: Some(SyncProvider::Jira),
                email_env: Some("me@example.com".into()),
                token_env: Some("secret".into()),
                api_url: Some("https://jira.example.com".into()),
                ..Default::default()
            },
        );
        let auth = auth.unwrap();
        assert_eq!(auth.provider, SyncProvider::Jira);
        assert!(auth.auth_header.unwrap().starts_with("Basic "));
        assert!(warnings[0].contains("does not match remote provider"));
    }
}

mod http {
    use super::*;

    #[test]
    fn error_responses_carry_the_status_and_body() {
        let api = MockApi::start(|_| (404, json!({"message": "Not Found"})));
        let client = api.client(SyncProvider::Github);
        let err = github_fetch_issue(&client, "org/repo", 1).unwrap_err();
        assert!(err.to_string().contains("HTTP 404"));
        assert!(err.to_string().contains("Not Found"));
    }

    #[test]
    fn github_search_pages_skips_pull_requests_and_falls_back_on_fetch_errors() {
        let api = MockApi::start(|request| {
            if request.path.starts_with("/search/issues") {
                let page = query_param(request, "page").unwrap_or("1");
                return ok(if page == "1" {
                    // A full page forces a second request.
                    let items: Vec<_> = (1..=100)
                        .map(|n| {
                            if n == 3 {
                                json!({"number": n, "pull_request": {}})
                            } else {
                                json!({"number": n, "title": "search"})
                            }
                        })
                        .collect();
                    json!({ "items": items })
                } else {
                    json!({"items": [{"number": 101, "title": "search"}]})
                });
            }
            if request.path == "/repos/org/repo/issues/5" {
                return (500, json!({"message": "boom"}));
            }
            ok(json!({"title": "detailed"}))
        });
        let client = api.client(SyncProvider::Github);
        let mut warnings = Vec::new();

        let issues = github_search_issues(&client, "org/repo", "label:bug", &mut warnings).unwrap();

        assert_eq!(
            issues.len(),
            100,
            "99 issues from page 1 plus 1 from page 2"
        );
        assert_eq!(
            issues.iter().filter(|i| i["title"] == "search").count(),
            1,
            "only the failed fetch keeps the search payload"
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("GitHub issue 5 fetch failed"));
        let searches: Vec<_> = api
            .requests()
            .into_iter()
            .filter(|r| r.path.starts_with("/search/issues"))
            .collect();
        assert_eq!(searches.len(), 2);
        assert!(
            query_param(&searches[0], "q")
                .unwrap()
                .contains("repo%3Aorg%2Frepo")
        );
    }

    #[test]
    fn github_validate_search_warns_about_search_limits() {
        let api = MockApi::start(|_| ok(json!({"items": []})));
        let mut warnings = Vec::new();
        github_validate_search(
            &api.client(SyncProvider::Github),
            "org/repo",
            "is:open",
            &mut warnings,
        )
        .unwrap();
        assert!(warnings[0].contains("search API"));
        assert_eq!(query_param(&api.requests()[0], "per_page"), Some("1"));
    }

    #[test]
    fn jira_transition_posts_the_matching_transition_id() {
        let api = MockApi::start(|request| {
            if request.method == "GET" {
                ok(json!({"transitions": [
                    {"id": "11", "name": "In Progress"},
                    {"id": "31", "name": "Done"},
                ]}))
            } else {
                ok(json!({}))
            }
        });
        let client = api.client(SyncProvider::Jira);

        jira_transition_issue(&client, "ENG-1", "done").unwrap();
        let err = jira_transition_issue(&client, "ENG-1", "Archived").unwrap_err();

        assert!(
            err.to_string()
                .contains("No Jira transition named 'Archived'")
        );
        let posts: Vec<_> = api
            .requests()
            .into_iter()
            .filter(|r| r.method == "POST")
            .collect();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].path, "/rest/api/3/issue/ENG-1/transitions");
        let body: JsonValue = serde_json::from_str(&posts[0].body).unwrap();
        assert_eq!(body, json!({"transition": {"id": "31"}}));
    }

    #[test]
    fn jira_user_lookup_resolves_once_and_reports_failures() {
        let api = MockApi::start(|request| match query_param(request, "query") {
            Some("ada") => ok(json!([{"accountId": "557058:ada"}])),
            Some("nobody") => ok(json!([])),
            _ => (500, json!({"message": "down"})),
        });
        let client = api.client(SyncProvider::Jira);
        let mut cache = JiraLookupCache::default();
        let mut warnings = Vec::new();
        let mut lookup = |value: &str, warnings: &mut Vec<String>| {
            jira_user_value(value, "assignee", Some(&client), &mut cache, warnings)
        };

        let expected = Some(json!({"accountId": "557058:ada"}));
        assert_eq!(lookup("ada", &mut warnings), expected);
        assert_eq!(
            lookup("ADA", &mut warnings),
            expected,
            "cached, case-insensitive"
        );
        assert_eq!(
            lookup("accountId:5b10ac8d82e05b22cc7d4ef5", &mut warnings),
            Some(json!({"accountId": "5b10ac8d82e05b22cc7d4ef5"})),
            "account ids skip the lookup"
        );
        assert_eq!(lookup("nobody", &mut warnings), None);
        assert_eq!(lookup("broken", &mut warnings), None);
        assert_eq!(lookup("  ", &mut warnings), None);

        assert_eq!(api.requests().len(), 3, "ada, nobody, broken");
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("Failed to resolve Jira user 'broken'"))
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("value 'nobody' is not an accountId"))
        );
    }
}

mod jira_values {
    use super::*;

    #[test]
    fn adf_round_trips_plain_text() {
        let adf = jira_text_to_adf("Hello world");
        assert_eq!(jira_adf_to_text(&adf), "Hello world");
        assert_eq!(jira_text_to_adf("   "), JsonValue::Null);

        let nested = json!({"content": [
            {"content": [{"text": "First "}, {"content": [{"text": "line"}]}]},
            {"content": [{"text": "  "}]},
            {"content": [{"text": "Second"}]},
        ]});
        assert_eq!(jira_adf_to_text(&nested), "First line\nSecond");
    }

    #[test]
    fn account_ids_need_a_plausible_shape() {
        assert_eq!(
            normalize_jira_account_id(" accountId: 712020:abc-def "),
            Some("712020:abc-def".into())
        );
        assert_eq!(normalize_jira_account_id("short"), None);
        assert_eq!(normalize_jira_account_id("someone@example.com"), None);
        assert_eq!(normalize_jira_account_id(""), None);
    }

    #[test]
    fn issue_types_match_case_insensitively_then_fall_back() {
        let mut warnings = Vec::new();
        let unknown = JiraLookupCache::default();
        assert_eq!(
            resolve_jira_issue_type("Bug", &unknown, &mut warnings),
            "Bug"
        );

        let known = JiraLookupCache {
            issue_types: Some(vec!["Story".into(), "Task".into(), "Bug".into()]),
            ..Default::default()
        };
        assert_eq!(resolve_jira_issue_type("bug", &known, &mut warnings), "Bug");
        assert!(warnings.is_empty());
        assert_eq!(
            resolve_jira_issue_type("Epic", &known, &mut warnings),
            "Task"
        );
        assert!(warnings[0].contains("falling back to 'Task'"));

        let no_task = JiraLookupCache {
            issue_types: Some(vec!["Story".into()]),
            ..Default::default()
        };
        assert_eq!(
            resolve_jira_issue_type("Epic", &no_task, &mut warnings),
            "Story"
        );
    }
}

mod targeted_pull {
    use super::*;

    fn github_issue_route(request: &RecordedRequest) -> (u16, JsonValue) {
        if request.path == "/repos/org/repo/issues/7" {
            ok(json!({"number": 7, "title": "Remote title", "state": "open"}))
        } else {
            (404, json!({"message": "unexpected"}))
        }
    }

    /// GitHub remote that maps the issue title onto the task title.
    fn titled_remote(mut remote: SyncRemoteConfig) -> SyncRemoteConfig {
        remote
            .mapping
            .insert("title".into(), SyncFieldMapping::Simple("title".into()));
        remote
    }

    fn task_with_reference(resolver: &TasksDirectoryResolver, reference: Option<&str>) -> TaskDTO {
        let mut storage = Storage::new(&resolver.path);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Local title".into(),
                project: Some("TEST".into()),
                ..Default::default()
            },
        )
        .unwrap();
        match reference {
            Some(reference) => {
                attach_sync_reference(&mut storage, &task.id, "github", reference)
                    .unwrap()
                    .0
            }
            None => task,
        }
    }

    fn pull_one(
        resolver: &TasksDirectoryResolver,
        remote: &SyncRemoteConfig,
        api: &MockApi,
        task_id: &str,
        dry_run: bool,
    ) -> (LoTaRResult<()>, SyncReportRecorder) {
        let mut recorder = recorder(SyncDirection::Pull, dry_run);
        let mut journal = SyncJournal::open(&resolver.path, dry_run).unwrap();
        let result = perform_pull(
            SyncTarget {
                resolver,
                remote,
                task_id: Some(task_id),
                dry_run,
            },
            "TEST",
            &api.client(SyncProvider::Github),
            SyncSinks {
                recorder: &mut recorder,
                warnings: &mut Vec::new(),
                journal: &mut journal,
            },
        );
        (result, recorder)
    }

    fn title(resolver: &TasksDirectoryResolver, id: &str) -> String {
        TaskService::get(&Storage::new(&resolver.path), id, None)
            .unwrap()
            .title
    }

    #[test]
    fn dry_run_reports_the_update_without_writing() {
        let (_temp, resolver, remote) = workspace(SyncProvider::Github);
        let remote = titled_remote(remote);
        let task = task_with_reference(&resolver, Some("org/repo#7"));
        let api = MockApi::start(github_issue_route);

        let (result, recorder) = pull_one(&resolver, &remote, &api, &task.id, true);

        result.unwrap();
        assert_eq!(recorder.summary.updated, 1);
        let entry = &recorder.entries[0];
        assert_eq!(entry.reference.as_deref(), Some("org/repo#7"));
        assert!(entry.fields.contains(&"title".to_string()));
        assert_eq!(entry.message.as_deref(), Some("Dry run: would update"));
        assert_eq!(title(&resolver, &task.id), "Local title");
    }

    #[test]
    fn applies_the_remote_issue_and_then_has_nothing_to_do() {
        let (_temp, resolver, remote) = workspace(SyncProvider::Github);
        let remote = titled_remote(remote);
        let task = task_with_reference(&resolver, Some("org/repo#7"));
        let api = MockApi::start(github_issue_route);

        pull_one(&resolver, &remote, &api, &task.id, false)
            .0
            .unwrap();
        assert_eq!(title(&resolver, &task.id), "Remote title");

        let (result, recorder) = pull_one(&resolver, &remote, &api, &task.id, false);
        result.unwrap();
        assert_eq!(recorder.summary.skipped, 1);
        assert_eq!(
            recorder.entries[0].message.as_deref(),
            Some("No changes to apply")
        );
    }

    #[test]
    fn rejects_tasks_without_a_usable_reference() {
        let (_temp, resolver, remote) = workspace(SyncProvider::Github);
        let remote = titled_remote(remote);
        let api = MockApi::start(github_issue_route);

        let unlinked = task_with_reference(&resolver, None);
        let err = pull_one(&resolver, &remote, &api, &unlinked.id, false)
            .0
            .unwrap_err();
        assert!(err.to_string().contains("missing a remote reference"));

        let other_repo = task_with_reference(&resolver, Some("other/repo#7"));
        let err = pull_one(&resolver, &remote, &api, &other_repo.id, false)
            .0
            .unwrap_err();
        assert!(err.to_string().contains("does not match this remote"));

        let err = pull_one(&resolver, &remote, &api, "  ", false)
            .0
            .unwrap_err();
        assert!(err.to_string().contains("Task id cannot be empty"));
        assert!(api.requests().is_empty());
    }
}

mod entry_points {
    use super::*;

    /// Workspace whose global config defines `origin` (GitHub) and, when
    /// `with_remotes` is false, no remotes at all.
    fn configured(with_remotes: bool) -> (tempfile::TempDir, TasksDirectoryResolver) {
        let (temp, resolver, _) = workspace(SyncProvider::Github);
        if with_remotes {
            let path = crate::utils::paths::global_config_path(temp.path());
            let mut config = fs::read_to_string(&path).unwrap();
            config.push_str("remotes:\n  origin:\n    provider: github\n    repo: org/repo\n");
            fs::write(path, config).unwrap();
        }
        (temp, resolver)
    }

    fn run_error(
        resolver: &TasksDirectoryResolver,
        direction: SyncDirection,
        options: SyncOptions<'_>,
    ) -> String {
        SyncService::run(resolver, direction, options)
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn run_rejects_missing_unknown_and_unconfigured_remotes() {
        let (_temp, resolver) = configured(true);
        let blank = SyncOptions {
            remote: "  ",
            ..Default::default()
        };
        assert!(
            run_error(&resolver, SyncDirection::Push, blank).contains("remote name is required")
        );
        let unknown = SyncOptions {
            remote: "upstream",
            ..Default::default()
        };
        assert!(
            run_error(&resolver, SyncDirection::Push, unknown)
                .contains("Unknown sync remote 'upstream'. Available: origin")
        );

        let (_temp, bare) = configured(false);
        let origin = SyncOptions {
            remote: "origin",
            ..Default::default()
        };
        assert!(
            run_error(&bare, SyncDirection::Push, origin).contains("No remotes are configured")
        );
    }

    #[test]
    fn run_keeps_targeted_tasks_inside_the_project() {
        let (_temp, resolver) = configured(true);
        let options = SyncOptions {
            remote: "origin",
            project: Some("TEST"),
            task_id: Some("OTHER-1"),
            dry_run: true,
            ..Default::default()
        };
        assert!(
            run_error(&resolver, SyncDirection::Push, options)
                .contains("Task 'OTHER-1' does not belong to project 'TEST'")
        );
    }

    #[test]
    fn pull_needs_an_auth_profile_even_when_dry() {
        let (_temp, resolver) = configured(true);
        let options = SyncOptions {
            remote: "origin",
            dry_run: true,
            ..Default::default()
        };
        assert!(
            run_error(&resolver, SyncDirection::Pull, options)
                .contains("Auth profile is required for sync operations")
        );
    }

    #[test]
    fn validate_requires_a_remote_and_an_auth_profile() {
        let (_temp, resolver) = configured(true);
        let err = |result: LoTaRResult<SyncValidateResponse>| result.unwrap_err().to_string();

        assert!(
            err(SyncService::validate(&resolver, None, None, None, None))
                .contains("remote name is required")
        );
        assert!(
            err(SyncService::validate(
                &resolver,
                None,
                Some("upstream"),
                None,
                None
            ))
            .contains("Unknown sync remote 'upstream'")
        );
        let override_config = SyncRemoteConfig {
            provider: SyncProvider::Github,
            project: None,
            repo: Some("org/repo".into()),
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        let response = SyncService::validate(&resolver, None, None, Some(override_config), None);
        assert!(err(response).contains("Auth profile is required for sync validation"));
    }
}
