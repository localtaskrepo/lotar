//! Push: create or update remote issues from local tasks.

use super::*;

pub(super) fn perform_push(
    target: SyncTarget<'_>,
    project: Option<&str>,
    client: Option<&SyncClient>,
    sinks: SyncSinks<'_>,
) -> LoTaRResult<()> {
    let SyncTarget {
        resolver,
        remote,
        task_id,
        dry_run,
    } = target;
    let SyncSinks {
        recorder,
        warnings,
        journal,
    } = sinks;
    if remote.provider == SyncProvider::Github
        && remote.repo.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(LoTaRError::ValidationError(
            "GitHub remote must define repo".to_string(),
        ));
    }

    let storage = Storage::new(&resolver.path.clone());
    let tasks = if let Some(task_id) = task_id {
        let trimmed = task_id.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Task id cannot be empty".to_string(),
            ));
        }
        let task = TaskService::get(&storage, trimmed, project)?;
        vec![(trimmed.to_string(), task)]
    } else {
        let filter = TaskListFilter {
            project: project.map(|p| p.to_string()),
            ..Default::default()
        };
        TaskService::list(&storage, &filter)
    };

    let mut failures = Vec::new();
    let mut jira_lookup = JiraLookupCache::default();
    let mut selected = HashSet::new();
    let label = provider_display_name(remote.provider);

    for (_id, mut task) in tasks {
        let prefix = crate::storage::operations::StorageOperations::get_project_for_task(&task.id)
            .ok_or_else(|| LoTaRError::InvalidTaskId(task.id.clone()))?;
        task.id = canonical_sync_task_id(&resolver.path, &prefix, &task.id)?;
        if !selected.insert(task.id.clone()) {
            continue;
        }
        let task = TaskService::get(&storage, &task.id, None)?;
        let skipped = |recorder: &mut SyncReportRecorder, reference: &str, message: &str| {
            recorder.record(
                SyncEntryStatus::Skipped,
                make_entry(
                    SyncEntryStatus::Skipped,
                    Some(task.id.clone()),
                    Some(reference.to_string()),
                    Some(task.title.clone()),
                    Some(message.to_string()),
                ),
            );
        };
        match determine_reference_state(remote, &task) {
            ReferenceState::Matching(reference) => {
                if remote.provider == SyncProvider::Jira && !dry_run {
                    ensure_jira_issue_types(&mut jira_lookup, client, remote, warnings);
                }
                let mut payload = PushPayload::build(
                    remote,
                    &task,
                    SyncOperation::Update,
                    client,
                    &mut jira_lookup,
                    warnings,
                );
                let mut changed_fields = payload.field_names();
                if payload.is_empty() {
                    skipped(recorder, &reference, "No changes to push");
                    continue;
                }
                if dry_run {
                    recorder.record(
                        SyncEntryStatus::Updated,
                        make_entry_with_fields(
                            SyncEntryStatus::Updated,
                            Some(task.id.clone()),
                            Some(reference.clone()),
                            Some(task.title.clone()),
                            changed_fields,
                            Some("Dry run: would update".to_string()),
                        ),
                    );
                    continue;
                }
                let client = require_sync_client(client)?;
                let target = PushTarget::for_reference(remote, &reference)?;
                match payload.diff_against_remote(client, &target, &changed_fields) {
                    Ok(fields) => changed_fields = fields,
                    Err(err) => warnings.push(format!(
                        "{} issue {} fetch failed; skipping diff: {}",
                        label, reference, err
                    )),
                }
                if payload.is_empty() {
                    skipped(recorder, &reference, "No changes to push");
                    continue;
                }
                if let Err(err) = payload.send_update(client, &target) {
                    failures.push(format!(
                        "Failed to update {} issue {}: {}",
                        label, reference, err
                    ));
                    recorder.record(
                        SyncEntryStatus::Failed,
                        make_entry(
                            SyncEntryStatus::Failed,
                            Some(task.id.clone()),
                            Some(reference.clone()),
                            Some(task.title.clone()),
                            Some(format!("Failed to update {} issue: {}", label, err)),
                        ),
                    );
                    continue;
                }
                payload.apply_remote_state(client, &target, &reference, false, warnings);
                recorder.record(
                    SyncEntryStatus::Updated,
                    make_entry_with_fields(
                        SyncEntryStatus::Updated,
                        Some(task.id.clone()),
                        Some(reference),
                        Some(task.title.clone()),
                        changed_fields,
                        None,
                    ),
                );
            }
            ReferenceState::ProviderOnly(reference) => {
                skipped(recorder, &reference, "Reference does not match remote");
            }
            ReferenceState::None => {
                if dry_run {
                    let payload = PushPayload::build(
                        remote,
                        &task,
                        SyncOperation::Create,
                        client,
                        &mut jira_lookup,
                        warnings,
                    );
                    let message = format_field_list(&payload.field_names())
                        .map(|fields| format!("Dry run: would create {}", fields));
                    recorder.record(
                        SyncEntryStatus::Created,
                        make_entry(
                            SyncEntryStatus::Created,
                            Some(task.id.clone()),
                            None,
                            Some(task.title.clone()),
                            message.or_else(|| Some("Dry run".to_string())),
                        ),
                    );
                    continue;
                }
                let client = require_sync_client(client)?;
                let mut storage = Storage::new(&resolver.path.clone());
                // Use the same identity prerequisites as the reference writer before
                // issuing a non-idempotent remote request.
                let local_project = crate::storage::TaskId::parse(&task.id)
                    .map_err(|err| LoTaRError::InvalidTaskId(format!("{}: {err}", task.id)))?
                    .project;
                crate::storage::safety::validate_project_prefix(&local_project)
                    .map_err(LoTaRError::ValidationError)?;
                TaskService::get(&storage, &task.id, Some(&local_project))?;
                let task_path = sync_task_path(resolver, &local_project, &task.id)?;
                OpenOptions::new().write(true).open(&task_path)?;
                if task_path
                    .parent()
                    .expect("task has parent")
                    .metadata()?
                    .permissions()
                    .readonly()
                {
                    return Err(LoTaRError::ValidationError(
                        "Local task directory is read-only; cannot attach sync reference".into(),
                    ));
                }
                if remote.provider == SyncProvider::Jira {
                    ensure_jira_issue_types(&mut jira_lookup, Some(client), remote, warnings);
                }
                let payload = PushPayload::build(
                    remote,
                    &task,
                    SyncOperation::Create,
                    Some(client),
                    &mut jira_lookup,
                    warnings,
                );
                let destination = create_destination(remote)?;
                // Journal order is the recovery contract: begin before the
                // non-idempotent create, save_result once the remote identity is
                // known, attach the local reference, then finish.
                let pending = journal.begin(PendingLink {
                    scope: pending_scope(remote, client),
                    project: local_project,
                    task_id: Some(task.id.clone()),
                    reference: None,
                })?;
                let (reference, target) = match payload.create(client, destination) {
                    Ok(created) => created,
                    Err(err) => {
                        failures.push(format!(
                            "Failed to create {} issue for {}: {}; {}",
                            label,
                            task.id,
                            err,
                            journal.indeterminate()
                        ));
                        recorder.record(
                            SyncEntryStatus::Failed,
                            make_entry(
                                SyncEntryStatus::Failed,
                                Some(task.id.clone()),
                                None,
                                Some(task.title.clone()),
                                Some(format!("Failed to create {} issue: {}", label, err)),
                            ),
                        );
                        continue;
                    }
                };
                journal.links[pending].reference = Some(reference.clone());
                journal.save_result(pending)?;
                if let Err(err) = attach_sync_reference(
                    &mut storage,
                    &task.id,
                    provider_label(&remote.provider),
                    &reference,
                ) {
                    failures.push(format!(
                        "Failed to attach {} reference for {}: {}",
                        label, task.id, err
                    ));
                    recorder.record(
                        SyncEntryStatus::Failed,
                        make_entry(
                            SyncEntryStatus::Failed,
                            Some(task.id.clone()),
                            Some(reference.clone()),
                            Some(task.title.clone()),
                            Some(format!("Failed to attach {} reference: {}", label, err)),
                        ),
                    );
                    continue;
                }
                journal.finish(pending, resolver)?;
                payload.apply_remote_state(client, &target, &reference, true, warnings);
                recorder.record(
                    SyncEntryStatus::Created,
                    make_entry(
                        SyncEntryStatus::Created,
                        Some(task.id.clone()),
                        Some(reference),
                        Some(task.title.clone()),
                        format_field_list(&payload.field_names())
                            .map(|fields| format!("Created fields: {}", fields)),
                    ),
                );
            }
        }
    }

    append_failure_warnings(&failures, warnings);
    Ok(())
}

pub(super) fn provider_display_name(provider: SyncProvider) -> &'static str {
    match provider {
        SyncProvider::Jira => "Jira",
        SyncProvider::Github => "GitHub",
    }
}

pub(super) fn require_sync_client(client: Option<&SyncClient>) -> LoTaRResult<&SyncClient> {
    client.ok_or_else(|| {
        LoTaRError::ValidationError("Auth profile is required for sync operations".to_string())
    })
}

/// Remote container new issues are created in (Jira project key or GitHub repo).
pub(super) fn create_destination(remote: &SyncRemoteConfig) -> LoTaRResult<&str> {
    match remote.provider {
        SyncProvider::Jira => remote.project.as_deref().ok_or_else(|| {
            LoTaRError::ValidationError("Jira remote must define project".to_string())
        }),
        SyncProvider::Github => remote.repo.as_deref().ok_or_else(|| {
            LoTaRError::ValidationError("GitHub remote must define repo".to_string())
        }),
    }
}

/// Remote issue addressed by a sync reference.
pub(super) enum PushTarget {
    Jira { key: String },
    Github { repo: String, number: u64 },
}

impl PushTarget {
    pub(super) fn for_reference(remote: &SyncRemoteConfig, reference: &str) -> LoTaRResult<Self> {
        match remote.provider {
            SyncProvider::Jira => Ok(PushTarget::Jira {
                key: reference.to_string(),
            }),
            SyncProvider::Github => parse_github_reference(reference, remote.repo.as_deref())
                .map(|(repo, number)| PushTarget::Github { repo, number })
                .ok_or_else(|| {
                    LoTaRError::ValidationError(format!("Invalid GitHub reference '{}'", reference))
                }),
        }
    }
}

/// Provider payload for one push; the orchestration in `perform_push` is
/// shared and these adapters only translate to provider requests (DEV-117).
pub(super) enum PushPayload {
    Jira(JiraIssuePayload),
    Github(GithubIssuePayload),
}

impl PushPayload {
    pub(super) fn build(
        remote: &SyncRemoteConfig,
        task: &TaskDTO,
        op: SyncOperation,
        client: Option<&SyncClient>,
        lookup: &mut JiraLookupCache,
        warnings: &mut Vec<String>,
    ) -> Self {
        match remote.provider {
            SyncProvider::Jira => PushPayload::Jira(build_jira_payload(
                remote, task, op, client, lookup, warnings,
            )),
            SyncProvider::Github => PushPayload::Github(build_github_payload(remote, task, op)),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        match self {
            PushPayload::Jira(payload) => payload.is_empty(),
            PushPayload::Github(payload) => payload.is_empty(true),
        }
    }

    pub(super) fn field_names(&self) -> Vec<String> {
        match self {
            PushPayload::Jira(payload) => jira_payload_field_names(payload),
            PushPayload::Github(payload) => github_payload_field_names(payload),
        }
    }

    /// Drop fields that already match the remote issue; returns the fields
    /// still to push.
    pub(super) fn diff_against_remote(
        &mut self,
        client: &SyncClient,
        target: &PushTarget,
        changed_fields: &[String],
    ) -> LoTaRResult<Vec<String>> {
        match (self, target) {
            (PushPayload::Jira(payload), PushTarget::Jira { key }) => {
                let issue = jira_fetch_issue(client, key, changed_fields)?;
                Ok(filter_jira_payload_against_issue(&issue, payload))
            }
            (PushPayload::Github(payload), PushTarget::Github { repo, number }) => {
                let issue = github_fetch_issue(client, repo, *number)?;
                Ok(filter_github_payload_against_issue(&issue, payload))
            }
            _ => unreachable!("payload and target are built for the same provider"),
        }
    }

    pub(super) fn send_update(&self, client: &SyncClient, target: &PushTarget) -> LoTaRResult<()> {
        match (self, target) {
            // A status-only Jira change is applied by the transition alone.
            (PushPayload::Jira(payload), PushTarget::Jira { key }) => {
                if payload.fields.is_empty() {
                    Ok(())
                } else {
                    jira_update_issue(client, key, payload)
                }
            }
            (PushPayload::Github(payload), PushTarget::Github { repo, number }) => {
                github_update_issue(client, repo, *number, payload)
            }
            _ => unreachable!("payload and target are built for the same provider"),
        }
    }

    /// Create the remote issue; returns the canonical reference and target.
    pub(super) fn create(
        &self,
        client: &SyncClient,
        destination: &str,
    ) -> LoTaRResult<(String, PushTarget)> {
        match self {
            PushPayload::Jira(payload) => {
                let key = jira_create_issue(client, destination, payload)?;
                let reference = normalize_jira_reference(&key).unwrap_or(key);
                Ok((reference.clone(), PushTarget::Jira { key: reference }))
            }
            PushPayload::Github(payload) => {
                let number = github_create_issue(client, destination, payload)?;
                let reference = format!("{}#{}", normalize_github_repo(destination), number);
                Ok((
                    reference,
                    PushTarget::Github {
                        repo: destination.to_string(),
                        number,
                    },
                ))
            }
        }
    }

    /// Workflow state that needs a separate request after the main write:
    /// Jira status transitions (update and create) and closing a newly
    /// created GitHub issue. Failures are warnings, never entry failures.
    pub(super) fn apply_remote_state(
        &self,
        client: &SyncClient,
        target: &PushTarget,
        reference: &str,
        created: bool,
        warnings: &mut Vec<String>,
    ) {
        match (self, target) {
            (PushPayload::Jira(payload), PushTarget::Jira { key }) => {
                if let Some(status) = payload.desired_status.as_deref()
                    && let Err(err) = jira_transition_issue(client, key, status)
                {
                    warnings.push(format!(
                        "Jira status transition for {} failed: {}",
                        reference, err
                    ));
                }
            }
            (PushPayload::Github(payload), PushTarget::Github { repo, number }) => {
                if created && payload.state.as_deref() == Some("closed") {
                    let close_payload = GithubIssuePayload {
                        state: Some("closed".to_string()),
                        ..Default::default()
                    };
                    if let Err(err) = github_update_issue(client, repo, *number, &close_payload) {
                        warnings.push(format!(
                            "Failed to close GitHub issue {}: {}",
                            reference, err
                        ));
                    }
                }
            }
            _ => unreachable!("payload and target are built for the same provider"),
        }
    }
}
