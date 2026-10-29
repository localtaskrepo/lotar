//! Pull: apply remote issues to local tasks.

use super::*;

pub(super) fn perform_pull(
    target: SyncTarget<'_>,
    project: &str,
    client: &SyncClient,
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

    if let Some(task_id) = task_id {
        let trimmed = task_id.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Task id cannot be empty".to_string(),
            ));
        }

        let storage = Storage::new(&resolver.path.clone());
        let mut existing = TaskService::get(&storage, trimmed, Some(project))?;
        existing.id = canonical_sync_task_id(&resolver.path, project, trimmed)?;
        let trimmed = existing.id.as_str();
        let reference = match determine_reference_state(remote, &existing) {
            ReferenceState::Matching(value) => value,
            ReferenceState::ProviderOnly(_) => {
                return Err(LoTaRError::ValidationError(
                    "Task reference does not match this remote".to_string(),
                ));
            }
            ReferenceState::None => {
                let pending_reference = journal
                    .links
                    .iter()
                    .find(|link| {
                        dry_run
                            && link.project == project
                            && link.task_id.as_deref() == Some(trimmed)
                            && link.scope == pending_scope(remote, client)
                    })
                    .and_then(|link| link.reference.clone());
                pending_reference.ok_or_else(|| {
                    LoTaRError::ValidationError("Task is missing a remote reference".to_string())
                })?
            }
        };

        let issue = match remote.provider {
            SyncProvider::Jira => {
                let fields = jira_pull_field_names(remote);
                jira_fetch_issue(client, &reference, &fields)?
            }
            SyncProvider::Github => {
                let (repo, number) = parse_github_reference(&reference, remote.repo.as_deref())
                    .ok_or_else(|| {
                        LoTaRError::ValidationError(format!(
                            "Invalid GitHub reference '{}'",
                            reference
                        ))
                    })?;
                github_fetch_issue(client, &repo, number)?
            }
        };

        let update = build_task_update_from_issue(remote.provider, remote, &issue, Some(&existing));
        if task_update_is_empty(&update) {
            recorder.record(
                SyncEntryStatus::Skipped,
                make_entry(
                    SyncEntryStatus::Skipped,
                    Some(existing.id.clone()),
                    Some(reference.clone()),
                    Some(existing.title.clone()),
                    Some("No changes to apply".to_string()),
                ),
            );
            return Ok(());
        }

        let update_fields = task_update_field_names(&update);
        if dry_run {
            recorder.record(
                SyncEntryStatus::Updated,
                make_entry_with_fields(
                    SyncEntryStatus::Updated,
                    Some(existing.id.clone()),
                    Some(reference.clone()),
                    Some(existing.title.clone()),
                    update_fields.clone(),
                    Some("Dry run: would update".to_string()),
                ),
            );
            return Ok(());
        }

        let mut storage = Storage::new(&resolver.path.clone());
        if let Err(err) = TaskService::update(&mut storage, &existing.id, update) {
            recorder.record(
                SyncEntryStatus::Failed,
                make_entry(
                    SyncEntryStatus::Failed,
                    Some(existing.id.clone()),
                    Some(reference.clone()),
                    Some(existing.title.clone()),
                    Some(format!("Failed to update local task: {}", err)),
                ),
            );
            return Err(LoTaRError::ValidationError(format!(
                "Failed to update local task {}: {}",
                existing.id, err
            )));
        }

        recorder.record(
            SyncEntryStatus::Updated,
            make_entry_with_fields(
                SyncEntryStatus::Updated,
                Some(existing.id.clone()),
                Some(reference.clone()),
                Some(existing.title.clone()),
                update_fields,
                None,
            ),
        );
        return Ok(());
    }

    let issues = match remote.provider {
        SyncProvider::Jira => jira_search_issues(client, remote)?,
        SyncProvider::Github => github_list_issues(client, remote, warnings)?,
    };

    let storage = Storage::new(&resolver.path.clone());

    // Pull index includes tombstones (DEV-92): a remote issue whose local
    // task is soft-deleted must MATCH its existing task instead of being
    // re-imported as a duplicate. Deleted matches are then skipped below.
    let filter = TaskListFilter {
        project: Some(project.to_string()),
        deletion: crate::api_types::DeletionFilter::All,
        ..Default::default()
    };
    // Move stubs (DEV-183) count too: an issue whose task moved to another
    // project matches the deleted stub here and is skipped, rather than being
    // imported again or synced into the other project.
    let mut tasks = TaskService::list_with_moved(&storage, &filter);
    for (id, task) in &mut tasks {
        task.id = canonical_sync_task_id(&resolver.path, project, &task.id)?;
        if !matches!(
            crate::storage::moves::follow(&resolver.path, &task.id),
            Ok(Some(_))
        ) {
            *task = TaskService::get_including_deleted(&storage, &task.id, None)?;
        }
        id.clone_from(&task.id);
    }
    let mut tasks_by_id: HashMap<String, TaskDTO> = HashMap::new();
    for (_id, task) in &tasks {
        tasks_by_id.insert(task.id.clone(), task.clone());
    }
    let mut reference_index = build_reference_index(remote, &tasks);
    if dry_run {
        for link in &journal.links {
            if link.project == project
                && link.scope == pending_scope(remote, client)
                && let (Some(id), Some(reference)) = (&link.task_id, &link.reference)
            {
                reference_index
                    .entry(reference.clone())
                    .or_insert_with(|| id.clone());
            }
        }
    }

    let mut failures = Vec::new();

    let mut storage = Storage::new(&resolver.path.clone());

    for issue in issues {
        let issue_title = default_title_from_issue(remote.provider, &issue);
        let reference = match issue_reference_for_remote(remote, &issue) {
            Some(reference) => reference,
            None => {
                recorder.record(
                    SyncEntryStatus::Skipped,
                    make_entry(
                        SyncEntryStatus::Skipped,
                        None,
                        None,
                        issue_title.clone(),
                        Some("Missing reference".to_string()),
                    ),
                );
                continue;
            }
        };

        if let Some(task_id) = reference_index.get(&reference) {
            let existing = tasks_by_id.get(task_id).cloned();
            // Deleted matches are skipped, never updated or re-imported
            // (DEV-92). The journal-based dry-run reference index above
            // keeps its explicit recovery semantics unchanged.
            if existing
                .as_ref()
                .is_some_and(|task| task.deleted_at.is_some())
            {
                recorder.record(
                    SyncEntryStatus::Skipped,
                    make_entry(
                        SyncEntryStatus::Skipped,
                        Some(task_id.clone()),
                        Some(reference.clone()),
                        existing.as_ref().map(|task| task.title.clone()),
                        Some("Local task is deleted; skipped".to_string()),
                    ),
                );
                continue;
            }
            let update =
                build_task_update_from_issue(remote.provider, remote, &issue, existing.as_ref());
            if task_update_is_empty(&update) {
                recorder.record(
                    SyncEntryStatus::Skipped,
                    make_entry(
                        SyncEntryStatus::Skipped,
                        Some(task_id.clone()),
                        Some(reference.clone()),
                        existing.as_ref().map(|task| task.title.clone()),
                        Some("No changes to apply".to_string()),
                    ),
                );
                continue;
            }
            let update_fields = task_update_field_names(&update);
            if dry_run {
                recorder.record(
                    SyncEntryStatus::Updated,
                    make_entry_with_fields(
                        SyncEntryStatus::Updated,
                        Some(task_id.clone()),
                        Some(reference.clone()),
                        existing.as_ref().map(|task| task.title.clone()),
                        update_fields.clone(),
                        Some("Dry run: would update".to_string()),
                    ),
                );
                continue;
            }
            if let Err(err) = TaskService::update(&mut storage, task_id, update) {
                failures.push(format!("Failed to update local task {}: {}", task_id, err));
                recorder.record(
                    SyncEntryStatus::Failed,
                    make_entry(
                        SyncEntryStatus::Failed,
                        Some(task_id.clone()),
                        Some(reference.clone()),
                        existing.as_ref().map(|task| task.title.clone()),
                        Some(format!("Failed to update local task: {}", err)),
                    ),
                );
                continue;
            }
            recorder.record(
                SyncEntryStatus::Updated,
                make_entry_with_fields(
                    SyncEntryStatus::Updated,
                    Some(task_id.clone()),
                    Some(reference.clone()),
                    existing.as_ref().map(|task| task.title.clone()),
                    update_fields,
                    None,
                ),
            );
        } else {
            let create = build_task_create_from_issue(remote.provider, remote, &issue, project)?;
            if dry_run {
                recorder.record(
                    SyncEntryStatus::Created,
                    make_entry(
                        SyncEntryStatus::Created,
                        None,
                        Some(reference.clone()),
                        Some(create.title.clone()),
                        Some("Dry run".to_string()),
                    ),
                );
                continue;
            }
            crate::storage::safety::validate_project_prefix(project)
                .map_err(LoTaRError::ValidationError)?;
            let project_path = resolver.path.join(project);
            if project_path.exists()
                && !project_path
                    .canonicalize()?
                    .starts_with(resolver.path.canonicalize()?)
            {
                return Err(LoTaRError::ValidationError(
                    "Sync project must be inside the locked tasks workspace".into(),
                ));
            }
            let pending = journal.begin(PendingLink {
                scope: pending_scope(remote, client),
                project: project.into(),
                task_id: None,
                reference: Some(reference.clone()),
            })?;
            let created_task = match TaskService::create(&mut storage, create) {
                Ok(task) => task,
                Err(err) => {
                    failures.push(format!(
                        "Failed to create local task: {}; {}",
                        err,
                        journal.indeterminate()
                    ));
                    recorder.record(
                        SyncEntryStatus::Failed,
                        make_entry(
                            SyncEntryStatus::Failed,
                            None,
                            Some(reference.clone()),
                            issue_title.clone(),
                            Some(format!("Failed to create local task: {}", err)),
                        ),
                    );
                    continue;
                }
            };
            journal.links[pending].task_id = Some(created_task.id.clone());
            journal.save_result(pending)?;
            if let Err(err) = attach_sync_reference(
                &mut storage,
                &created_task.id,
                provider_label(&remote.provider),
                &reference,
            ) {
                failures.push(format!(
                    "Failed to attach reference for {}: {}",
                    created_task.id, err
                ));
                recorder.record(
                    SyncEntryStatus::Failed,
                    make_entry(
                        SyncEntryStatus::Failed,
                        Some(created_task.id.clone()),
                        Some(reference.clone()),
                        Some(created_task.title.clone()),
                        Some(format!("Failed to attach reference: {}", err)),
                    ),
                );
                continue;
            }
            journal.finish(pending, resolver)?;
            reference_index.insert(reference.clone(), created_task.id.clone());
            tasks_by_id.insert(created_task.id.clone(), created_task.clone());
            recorder.record(
                SyncEntryStatus::Created,
                make_entry(
                    SyncEntryStatus::Created,
                    Some(created_task.id.clone()),
                    Some(reference.clone()),
                    Some(created_task.title.clone()),
                    None,
                ),
            );
        }
    }

    append_failure_warnings(&failures, warnings);
    Ok(())
}

pub(super) fn resolve_pull_project_prefix(
    project_prefix: Option<String>,
    remote: &SyncRemoteConfig,
    warnings: &mut Vec<String>,
) -> LoTaRResult<String> {
    if let Some(prefix) = project_prefix {
        return Ok(prefix);
    }

    match remote.provider {
        SyncProvider::Jira => {
            if let Some(project) = remote.project.as_ref() {
                let trimmed = project.trim();
                if !trimmed.is_empty() {
                    warnings.push(format!(
                        "Pulling without --project; using Jira project '{}' as local prefix",
                        trimmed
                    ));
                    return Ok(trimmed.to_string());
                }
            }
        }
        SyncProvider::Github => {
            return Err(LoTaRError::ValidationError(
                "Pull sync for GitHub requires --project or default_project".to_string(),
            ));
        }
    }

    Err(LoTaRError::ValidationError(
        "Pull sync requires --project or default_project".to_string(),
    ))
}

pub(super) fn build_task_update_from_issue(
    provider: SyncProvider,
    remote: &SyncRemoteConfig,
    issue: &JsonValue,
    existing: Option<&TaskDTO>,
) -> TaskUpdate {
    let local = map_issue_to_local_fields(provider, remote, issue, existing);
    let custom_fields = local
        .custom_fields
        .map(|fields| merge_custom_fields(existing.map(|task| &task.custom_fields), fields));
    let mut update = TaskUpdate {
        title: local.title,
        status: local.status.map(|status| status.to_string()),
        priority: local.priority.map(|priority| priority.to_string()),
        task_type: local.task_type.map(|task_type| task_type.to_string()),
        reporter: local.reporter,
        assignee: local.assignee,
        due_date: None,
        effort: None,
        description: local.description,
        tags: local.tags,
        acceptance_criteria: None,
        relationships: None,
        custom_fields,
        sprints: None,
    };

    if let Some(existing) = existing {
        if update.title.as_deref() == Some(existing.title.as_str()) {
            update.title = None;
        }
        if update.description.as_deref() == existing.description.as_deref() {
            update.description = None;
        }
        if update.status.as_deref() == Some(existing.status.as_str()) {
            update.status = None;
        }
        if update.priority.as_deref() == Some(existing.priority.as_str()) {
            update.priority = None;
        }
        if update.task_type.as_deref() == Some(existing.task_type.as_str()) {
            update.task_type = None;
        }
        if update.assignee.as_deref() == existing.assignee.as_deref() {
            update.assignee = None;
        }
        if update.reporter.as_deref() == existing.reporter.as_deref() {
            update.reporter = None;
        }
        if let Some(tags) = update.tags.as_ref()
            && tags == &existing.tags
        {
            update.tags = None;
        }
        if update.custom_fields.as_ref() == Some(&existing.custom_fields) {
            update.custom_fields = None;
        }
    }

    update
}

pub(super) fn build_task_create_from_issue(
    provider: SyncProvider,
    remote: &SyncRemoteConfig,
    issue: &JsonValue,
    project: &str,
) -> LoTaRResult<TaskCreate> {
    let local = map_issue_to_local_fields(provider, remote, issue, None);
    let title = local
        .title
        .or_else(|| default_title_from_issue(provider, issue));
    let Some(title) = title else {
        return Err(LoTaRError::ValidationError(
            "Remote issue missing title".to_string(),
        ));
    };
    let create = TaskCreate {
        title,
        project: Some(project.to_string()),
        status: local.status.map(|status| status.to_string()),
        priority: local.priority.map(|priority| priority.to_string()),
        task_type: local.task_type.map(|task_type| task_type.to_string()),
        reporter: local.reporter,
        assignee: local.assignee,
        due_date: None,
        effort: None,
        description: local.description,
        tags: local.tags.unwrap_or_default(),
        acceptance_criteria: Vec::new(),
        relationships: None,
        custom_fields: local.custom_fields,
        sprints: Vec::new(),
    };
    Ok(create)
}
