//! Crash-safe journal of in-flight remote creations, so a retried sync never
//! creates the same remote issue twice, plus task reference attachment.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct PendingScope {
    pub(super) provider: String,
    pub(super) endpoint_hash: String,
    pub(super) destination: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct PendingLink {
    pub(super) scope: PendingScope,
    pub(super) project: String,
    pub(super) task_id: Option<String>,
    pub(super) reference: Option<String>,
}

impl PendingLink {
    pub(super) fn canonicalize(&mut self, root: &Path) -> LoTaRResult<()> {
        if let Some(id) = self.task_id.as_deref() {
            self.task_id = Some(canonical_sync_task_id(root, &self.project, id)?);
        }
        self.project = canonical_sync_project(root, &self.project)?;
        Ok(())
    }
}

// The workspace-wide lock spans snapshot reads, creation and linking, across both
// directions and remote aliases. A crashed process releases it automatically.
pub(super) struct SyncJournal {
    pub(super) path: PathBuf,
    pub(super) _lock: Option<File>,
    pub(super) links: Vec<PendingLink>,
    pub(super) dry_run: bool,
}

impl SyncJournal {
    pub(super) fn open(root: &Path, dry_run: bool) -> LoTaRResult<Self> {
        let lock = if dry_run {
            None
        } else {
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(root.join(".sync-pending.lock"))?;
            file.try_lock_exclusive().map_err(|_| {
                LoTaRError::ValidationError(
                    "Another sync is running in this workspace; retry after it finishes".into(),
                )
            })?;
            Some(file)
        };
        let path = root.join(".sync-pending.json");
        let mut links: Vec<PendingLink> = match fs::read_to_string(&path) {
            Ok(payload) => serde_json::from_str(&payload).map_err(|err| LoTaRError::ValidationError(
                format!("Invalid sync recovery journal {}: {err}; do not delete it or retry creation before reconciling", path.display())))?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(err) => return Err(err.into()),
        };
        // Validate every stored identity before any scope/target filtering. An
        // unknown task must not turn into an unrelated entry that can be skipped.
        for link in &mut links {
            link.canonicalize(root).map_err(|err| LoTaRError::ValidationError(format!(
                "Invalid sync recovery identity in {}: {err}; reconcile the journal before retrying creation", path.display()
            )))?;
        }
        Ok(Self {
            path,
            _lock: lock,
            links,
            dry_run,
        })
    }

    pub(super) fn save(&self) -> LoTaRResult<()> {
        if self.dry_run {
            return Err(LoTaRError::ValidationError(
                "Dry run cannot write sync recovery state".into(),
            ));
        }
        #[cfg(test)]
        if self
            .links
            .iter()
            .any(|link| link.task_id.is_some() && link.reference.is_some())
            && FAIL_SYNC_RESULT_SAVE.replace(false)
        {
            return Err(std::io::Error::other("injected sync result journal failure").into());
        }
        let payload = serde_json::to_string_pretty(&self.links)
            .map_err(|err| LoTaRError::SerializationError(err.to_string()))?;
        // The shared writer must sync its temporary file before non-destructive
        // publication. On Unix, also persist the directory entry here. Portable
        // std::fs does not provide directory fsync on Windows.
        crate::storage::safety::atomic_write_file(&self.path, &payload)?;
        #[cfg(unix)]
        File::open(self.path.parent().expect("journal has a parent"))?.sync_all()?;
        Ok(())
    }

    pub(super) fn begin(&mut self, mut link: PendingLink) -> LoTaRResult<usize> {
        link.canonicalize(self.path.parent().expect("journal has a parent"))?;
        if self.links.iter().any(|pending| {
            pending.scope == link.scope
                && pending.project == link.project
                && ((link.task_id.is_some() && pending.task_id == link.task_id)
                    || (link.reference.is_some() && pending.reference == link.reference))
        }) {
            return Err(self.indeterminate());
        }
        self.links.push(link);
        self.save()?;
        Ok(self.links.len() - 1)
    }

    pub(super) fn save_result(&mut self, index: usize) -> LoTaRResult<()> {
        self.links[index].canonicalize(self.path.parent().expect("journal has a parent"))?;
        self.save().map_err(|err| {
            let link = &self.links[index];
            LoTaRError::ValidationError(format!(
                "Failed to journal returned identity task_id={:?}, reference={:?}: {err}. {}",
                link.task_id,
                link.reference,
                self.indeterminate()
            ))
        })
    }

    pub(super) fn indeterminate(&self) -> LoTaRError {
        LoTaRError::ValidationError(format!(
            "Indeterminate sync creation in {}. Creation will not be retried. Inspect the remote issues and local tasks, fill the missing task_id/reference in this journal with the verified identity, then retry the same project/remote. Remove an intent only after verifying creation did not occur.",
            self.path.display()
        ))
    }

    pub(super) fn reconcile(
        &mut self,
        resolver: &TasksDirectoryResolver,
        scope: &PendingScope,
        project: Option<&str>,
        task_id: Option<&str>,
        remote: &SyncRemoteConfig,
    ) -> LoTaRResult<()> {
        let project = project
            .map(|project| canonical_sync_project(&resolver.path, project))
            .transpose()?;
        let project = project.as_deref();
        let task_id = task_id
            .map(|id| {
                let prefix =
                    crate::storage::operations::StorageOperations::get_project_for_task(id.trim())
                        .ok_or_else(|| LoTaRError::InvalidTaskId(id.to_string()))?;
                canonical_sync_task_id(&resolver.path, &prefix, id.trim())
            })
            .transpose()?;
        let task_id = task_id.as_deref();
        let mut storage = Storage::new(&resolver.path);
        let mut index = 0;
        while index < self.links.len() {
            let link = &self.links[index];
            if project.is_some_and(|p| p != link.project)
                || task_id.is_some_and(|id| {
                    link.task_id
                        .as_deref()
                        .is_some_and(|known| known != id.trim())
                })
            {
                index += 1;
                continue;
            }
            // Never reinterpret a pending identity after switching servers or repos.
            if &link.scope != scope {
                return Err(LoTaRError::ValidationError(format!(
                    "Pending sync belongs to a different remote identity; reconcile {} with its original project/remote first",
                    self.path.display()
                )));
            }
            let (Some(id), Some(reference)) = (&link.task_id, &link.reference) else {
                return Err(self.indeterminate());
            };
            crate::storage::safety::validate_project_prefix(&link.project)
                .map_err(LoTaRError::ValidationError)?;
            let canonical_prefix_matches = crate::storage::TaskId::parse(id)
                .is_ok_and(|parsed| parsed.project == link.project);
            if !canonical_prefix_matches
                || normalize_reference_for_remote(remote, reference).as_deref() != Some(reference)
            {
                return Err(LoTaRError::ValidationError(
                    "Pending sync identity does not match its project/remote".into(),
                ));
            }
            let existing = TaskService::get(&storage, id, Some(&link.project))?;
            sync_task_path(resolver, &link.project, id)?;
            match determine_reference_state(remote, &existing) {
                ReferenceState::None => {}
                ReferenceState::Matching(value) if value == *reference => {}
                _ => {
                    return Err(LoTaRError::ValidationError(format!(
                        "Pending sync reference conflicts with task {id}; reconcile {} manually",
                        self.path.display()
                    )));
                }
            }
            if self.dry_run {
                index += 1;
                continue;
            }
            attach_sync_reference(&mut storage, id, &scope.provider, reference)?;
            self.finish(index, resolver)?;
        }
        Ok(())
    }

    pub(super) fn finish(
        &mut self,
        index: usize,
        resolver: &TasksDirectoryResolver,
    ) -> LoTaRResult<()> {
        let link = &self.links[index];
        let id = link
            .task_id
            .as_deref()
            .ok_or_else(|| self.indeterminate())?;
        // Make the link durable before forgetting its recovery identity.
        let path = sync_task_path(resolver, &link.project, id)?;
        OpenOptions::new().write(true).open(&path)?.sync_all()?;
        #[cfg(unix)]
        File::open(path.parent().expect("task has parent"))?.sync_all()?;
        self.links.remove(index);
        self.save()
    }
}

pub(super) fn pending_scope(remote: &SyncRemoteConfig, client: &SyncClient) -> PendingScope {
    PendingScope {
        provider: provider_label(&remote.provider).into(),
        endpoint_hash: blake3::hash(client.auth.api_base.trim_end_matches('/').as_bytes())
            .to_hex()
            .to_string(),
        destination: match remote.provider {
            SyncProvider::Github => normalize_github_repo(remote.repo.as_deref().unwrap_or("")),
            SyncProvider::Jira => remote
                .project
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_ascii_uppercase(),
        },
    }
}

pub(super) fn sync_task_path(
    resolver: &TasksDirectoryResolver,
    project: &str,
    id: &str,
) -> LoTaRResult<PathBuf> {
    let folders =
        crate::storage::locator::StorageLocator::project_folders_for_name(&resolver.path, project);
    let path = folders
        .iter()
        .find_map(|folder| {
            crate::storage::operations::StorageOperations::get_file_path_for_id(
                &resolver.path.join(folder),
                id,
            )
        })
        .ok_or_else(|| LoTaRError::TaskNotFound(id.to_string()))?;
    if !path
        .canonicalize()?
        .starts_with(resolver.path.canonicalize()?)
    {
        return Err(LoTaRError::ValidationError(
            "Sync task must be inside the locked tasks workspace".into(),
        ));
    }
    Ok(path)
}

pub(super) fn canonical_sync_project(root: &Path, project: &str) -> LoTaRResult<String> {
    crate::storage::safety::validate_project_prefix(project)
        .map_err(LoTaRError::ValidationError)?;
    let path = root.join(project);
    if !path.exists() {
        return Ok(project.to_string());
    }
    let path = path.canonicalize()?;
    if path.parent() != Some(root.canonicalize()?.as_path()) || !path.is_dir() {
        return Err(LoTaRError::ValidationError(
            "Sync project must be directly inside the locked tasks workspace".into(),
        ));
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| LoTaRError::ValidationError("Invalid sync project directory".into()))
}

pub(super) fn canonical_sync_task_id(root: &Path, project: &str, id: &str) -> LoTaRResult<String> {
    use crate::storage::operations::StorageOperations;
    // Canonical ID parse (final numeric suffix; padded aliases like TP-001
    // collapse to TP-1) plus a single-root existence check: sync
    // canonicalization stays locked to the provided tasks workspace and never
    // adopts a sibling-root task.
    if StorageOperations::get_project_for_task(id).as_deref() != Some(project)
        || StorageOperations::get_in_root(root, id, project).is_none()
    {
        return Err(LoTaRError::InvalidTaskId(id.to_string()));
    }
    let project = canonical_sync_project(root, project)?;
    let path = StorageOperations::get_file_path_for_id(&root.join(&project), id)
        .ok_or_else(|| LoTaRError::TaskNotFound(id.to_string()))?
        .canonicalize()?;
    // Any folder inside the project counts (DEV-4); outside it never does.
    if !path.starts_with(root.join(&project).canonicalize()?) || !path.is_file() {
        return Err(LoTaRError::ValidationError(
            "Sync task identity crosses its project boundary".into(),
        ));
    }
    let number = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.parse::<u64>().ok())
        .ok_or_else(|| LoTaRError::InvalidTaskId(id.to_string()))?;
    let canonical = format!("{project}-{number}");
    let canonical_path = StorageOperations::get_file_path_for_id(&root.join(&project), &canonical)
        .ok_or_else(|| LoTaRError::InvalidTaskId(id.to_string()))?
        .canonicalize()?;
    if canonical_path != path {
        return Err(LoTaRError::InvalidTaskId(id.to_string()));
    }
    Ok(canonical)
}

#[cfg(test)]
thread_local! {
    pub(super) static FAIL_SYNC_LINK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static FAIL_SYNC_RESULT_SAVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn attach_sync_reference(
    storage: &mut Storage,
    id: &str,
    provider: &str,
    reference: &str,
) -> LoTaRResult<(TaskDTO, bool)> {
    #[cfg(test)]
    if FAIL_SYNC_LINK.replace(false) {
        return Err(std::io::Error::other("injected sync link failure").into());
    }
    let outcome = ReferenceService::attach_platform_reference(storage, id, provider, reference)?;
    // Sync holds no attachment store locks for platform references, so
    // post-commit hooks run here with the same semantics as every other
    // surface (consistent with task creation during sync already firing
    // automation).
    ReferenceService::dispatch_post_commit(storage, &outcome);
    Ok((outcome.task, outcome.changed))
}
