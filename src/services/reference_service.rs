use crate::api_types::{ReferenceSnippetDTO, ReferenceSnippetLineDTO, TaskDTO};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::automation_service::AutomationService;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::storage::task::Task;
use crate::types::{ReferenceEntry, TaskChange, TaskChangeLogEntry};
use ignore::WalkBuilder;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ReferenceService;

/// Result of a single reference or managed-attachment mutation, carrying
/// everything callers need to run post-commit hooks exactly once for
/// changed operations. Dropping the outcome silently would skip those
/// hooks, so it is `#[must_use]`.
#[must_use]
#[derive(Debug, Clone)]
pub struct ReferenceMutationOutcome {
    /// Task after the mutation attempt (equals the previous state when
    /// `changed` is false).
    pub task: TaskDTO,
    /// Whether a reference was actually added or removed.
    pub changed: bool,
    /// Task state before the mutation; present only when `changed` is true.
    pub previous: Option<TaskDTO>,
}

#[cfg(test)]
#[path = "../../tests/common/dev60_lock_order_cases.rs"]
mod dev60_lock_order_cases;

impl ReferenceService {
    /// Run post-commit hooks for a reference mutation: changed operations
    /// dispatch automation with the previous/current task pair, firing the
    /// generic `updated` event (plus the legacy `start` catch-all).
    /// Reference values are not part of the automation condition
    /// vocabulary, so rules with change conditions correctly stay silent
    /// while bare `on.updated` rules fire exactly once. Unchanged
    /// operations do nothing.
    ///
    /// Callers must invoke this only after every lock they hold has been
    /// released — in particular the cross-process attachment store lock —
    /// because automation re-enters task storage and may mutate further.
    pub fn dispatch_post_commit(storage: &mut Storage, outcome: &ReferenceMutationOutcome) {
        #[cfg(test)]
        assert_eq!(
            crate::services::attachment_service::AttachmentService::store_locks_held(),
            0,
            "reference post-commit hooks must run after the attachment store lock is released"
        );
        let Some(previous) = outcome.previous.as_ref() else {
            return;
        };
        let Ok(parsed) = crate::storage::TaskId::parse(&outcome.task.id) else {
            return;
        };
        let config =
            TaskService::resolve_config_for_project(storage.root_path.as_path(), &parsed.project);
        let _ =
            AutomationService::apply_task_update(storage, Some(previous), &outcome.task, &config);
    }

    /// Shared locked commit for reference mutations: parses the canonical
    /// id, refuses non-local targets, then applies the typed mutation to
    /// the freshest task state under the project task lock (concurrent
    /// writers to other fields can never be lost). Changed operations
    /// record exactly one `reference_added`/`reference_removed` history
    /// entry (old/new carry `kind:value` displays) and bump `modified`
    /// once; unchanged operations leave the file bytes identical and skip
    /// history, timestamps, and post-commit hooks.
    pub(crate) fn commit_reference_change<F>(
        storage: &mut Storage,
        task_id: &str,
        kind: &str,
        added: bool,
        apply: F,
    ) -> LoTaRResult<ReferenceMutationOutcome>
    where
        F: FnOnce(&mut Task, &mut Vec<String>) -> bool,
    {
        let parsed = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?;
        if parsed.project.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }
        let canonical = parsed.canonical();
        TaskService::ensure_local_task(storage, task_id)?;

        let root_path = storage.root_path.clone();
        let history_field = if added {
            "reference_added"
        } else {
            "reference_removed"
        };
        let canonical_guard = canonical.clone();
        let outcome = storage.mutate_task(&canonical, move |task| {
            // Lifecycle guard (DEV-92): reference and attachment mutations
            // refuse tombstones until the task is restored.
            TaskService::ensure_not_deleted(task, &canonical_guard)?;
            let mut changed_values = Vec::new();
            if !apply(task, &mut changed_values) {
                return Ok(false);
            }
            let display = changed_values
                .iter()
                .map(|value| format!("{kind}:{value}"))
                .collect::<Vec<_>>()
                .join(", ");
            let (old, new) = if added {
                (None, Some(display))
            } else {
                (Some(display), None)
            };
            let now = chrono::Utc::now().to_rfc3339();
            task.history.push(TaskChangeLogEntry {
                at: now.clone(),
                actor: crate::utils::identity::resolve_current_user(Some(root_path.as_path())),
                changes: vec![TaskChange {
                    field: history_field.into(),
                    old,
                    new,
                }],
            });
            task.modified = now;
            Ok(true)
        })?;

        let previous = outcome
            .previous()
            .map(|before| TaskService::dto_from_task(storage, &canonical, before.clone()));
        let task = TaskService::dto_from_task(storage, &canonical, outcome.after.clone());
        Ok(ReferenceMutationOutcome {
            task,
            changed: outcome.changed,
            previous,
        })
    }

    pub fn suggest_repo_files(repo_root: &Path, query: &str, limit: usize) -> Vec<String> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut results = Vec::new();
        let mut builder = WalkBuilder::new(repo_root);
        builder.filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !(name == ".git" || name == ".tasks" || name == "target" || name == "node_modules")
        });

        for entry in builder.build() {
            if results.len() >= limit {
                break;
            }
            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            if !entry.file_type().is_some_and(|ty| ty.is_file()) {
                continue;
            }
            let path = entry.path();
            let rel = match path.strip_prefix(repo_root) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let rel_display = Self::normalize_path_for_display(rel);
            if rel_display.to_ascii_lowercase().contains(&needle) {
                results.push(rel_display);
            }
        }

        results
    }

    pub fn attach_link_reference(
        storage: &mut Storage,
        task_id: &str,
        url: &str,
    ) -> LoTaRResult<ReferenceMutationOutcome> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError("Missing url".to_string()));
        }
        if trimmed.len() > 4096 {
            return Err(LoTaRError::ValidationError(
                "Link reference is too long (max 4096 characters)".to_string(),
            ));
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("javascript:")
            || lower.starts_with("data:")
            || lower.starts_with("vbscript:")
        {
            return Err(LoTaRError::ValidationError(
                "Link reference protocol is not allowed".to_string(),
            ));
        }

        let target = trimmed.to_string();
        Self::commit_reference_change(
            storage,
            task_id,
            "link",
            true,
            move |task, changed_values| {
                let already = task
                    .references
                    .iter()
                    .any(|r| r.link.as_deref() == Some(target.as_str()));
                if already {
                    return false;
                }
                task.references.push(ReferenceEntry {
                    link: Some(target.clone()),
                    ..Default::default()
                });
                changed_values.push(target.clone());
                true
            },
        )
    }

    pub fn detach_link_reference(
        storage: &mut Storage,
        task_id: &str,
        url: &str,
    ) -> LoTaRResult<ReferenceMutationOutcome> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError("Missing url".to_string()));
        }

        let target = trimmed.to_string();
        Self::commit_reference_change(
            storage,
            task_id,
            "link",
            false,
            move |task, changed_values| {
                let before_len = task.references.len();
                task.references
                    .retain(|r| r.link.as_deref() != Some(target.as_str()));
                if task.references.len() == before_len {
                    return false;
                }
                changed_values.push(target.clone());
                true
            },
        )
    }

    pub fn attach_code_reference(
        storage: &mut Storage,
        repo_root: &Path,
        task_id: &str,
        code: &str,
    ) -> LoTaRResult<ReferenceMutationOutcome> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Missing code reference".to_string(),
            ));
        }
        if trimmed.len() > 4096 {
            return Err(LoTaRError::ValidationError(
                "Code reference is too long (max 4096 characters)".to_string(),
            ));
        }

        let (raw_path, start_line, end_line) = Self::split_reference(trimmed);
        if raw_path.trim().is_empty() {
            return Err(LoTaRError::ValidationError(
                "Reference path is empty".to_string(),
            ));
        }
        if let (Some(start), Some(end)) = (start_line, end_line)
            && end < start
        {
            return Err(LoTaRError::ValidationError(
                "End line must be greater than or equal to start line".to_string(),
            ));
        }

        // Store targets are rejected like `file` references: unresolvable
        // paths fall through to the snippet reader, which reports the
        // missing file; existing store blobs fail closed here.
        let repo_root_canonical = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        if let Ok(resolved) = Self::resolve_path(&repo_root_canonical, &raw_path) {
            Self::ensure_outside_attachments_store(&storage.root_path, &resolved)
                .map_err(LoTaRError::ValidationError)?;
        }

        let snippet = Self::snippet_for_code(repo_root, trimmed, 0, 0)
            .map_err(LoTaRError::ValidationError)?;

        let normalized = if start_line.is_some() {
            if end_line.is_some() && snippet.highlight_end != snippet.highlight_start {
                format!(
                    "{}#{}-{}",
                    snippet.path, snippet.highlight_start, snippet.highlight_end
                )
            } else {
                format!("{}#{}", snippet.path, snippet.highlight_start)
            }
        } else {
            snippet.path.clone()
        };

        Self::commit_reference_change(
            storage,
            task_id,
            "code",
            true,
            move |task, changed_values| {
                let already = task
                    .references
                    .iter()
                    .any(|r| r.code.as_deref() == Some(normalized.as_str()));
                if already {
                    return false;
                }
                task.references.push(ReferenceEntry {
                    code: Some(normalized.clone()),
                    ..Default::default()
                });
                changed_values.push(normalized.clone());
                true
            },
        )
    }

    pub fn detach_code_reference(
        storage: &mut Storage,
        task_id: &str,
        code: &str,
    ) -> LoTaRResult<ReferenceMutationOutcome> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Missing code reference".to_string(),
            ));
        }

        let mut candidates = vec![trimmed.to_string()];
        if let Some((path_part, anchor_part)) = trimmed.split_once('#') {
            let numbers = Self::extract_numbers(anchor_part);
            if let Some(start) = numbers.first().copied() {
                let end = numbers.get(1).copied();
                let canonical_no_l = if let Some(end) = end {
                    if end != start {
                        format!("{}#{}-{}", path_part.trim(), start, end)
                    } else {
                        format!("{}#{}", path_part.trim(), start)
                    }
                } else {
                    format!("{}#{}", path_part.trim(), start)
                };

                let canonical_with_l = if let Some(end) = end {
                    if end != start {
                        format!("{}#L{}-L{}", path_part.trim(), start, end)
                    } else {
                        format!("{}#L{}", path_part.trim(), start)
                    }
                } else {
                    format!("{}#L{}", path_part.trim(), start)
                };

                candidates.push(canonical_no_l);
                candidates.push(canonical_with_l);
            }
        }

        candidates.sort();
        candidates.dedup();

        Self::commit_reference_change(
            storage,
            task_id,
            "code",
            false,
            move |task, changed_values| {
                let before_len = task.references.len();
                task.references.retain(|r| {
                    let Some(stored) = r.code.as_deref() else {
                        return true;
                    };
                    if candidates.iter().any(|candidate| candidate == stored) {
                        changed_values.push(stored.to_string());
                        return false;
                    }
                    true
                });
                task.references.len() != before_len
            },
        )
    }

    pub fn attach_platform_reference(
        storage: &mut Storage,
        task_id: &str,
        kind: &str,
        value: &str,
    ) -> LoTaRResult<ReferenceMutationOutcome> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Missing reference value".to_string(),
            ));
        }
        if trimmed.len() > 4096 {
            return Err(LoTaRError::ValidationError(
                "Reference value is too long (max 4096 characters)".to_string(),
            ));
        }

        let normalized = match kind.trim().to_ascii_lowercase().as_str() {
            "jira" => normalize_jira_reference(trimmed),
            "github" => normalize_github_reference(trimmed),
            other => {
                return Err(LoTaRError::ValidationError(format!(
                    "Unsupported reference kind: {}",
                    other
                )));
            }
        };

        let field_kind = kind.trim().to_ascii_lowercase();
        let kind_label = field_kind.clone();
        Self::commit_reference_change(
            storage,
            task_id,
            &kind_label,
            true,
            move |task, changed_values| {
                let already = if field_kind == "jira" {
                    task.references
                        .iter()
                        .any(|r| r.jira.as_deref() == Some(normalized.as_str()))
                } else {
                    task.references
                        .iter()
                        .any(|r| r.github.as_deref() == Some(normalized.as_str()))
                };
                if already {
                    return false;
                }
                let entry = if field_kind == "jira" {
                    ReferenceEntry {
                        jira: Some(normalized.clone()),
                        ..Default::default()
                    }
                } else {
                    ReferenceEntry {
                        github: Some(normalized.clone()),
                        ..Default::default()
                    }
                };
                task.references.push(entry);
                changed_values.push(normalized.clone());
                true
            },
        )
    }

    pub fn detach_platform_reference(
        storage: &mut Storage,
        task_id: &str,
        kind: &str,
        value: &str,
    ) -> LoTaRResult<ReferenceMutationOutcome> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Missing reference value".to_string(),
            ));
        }

        let normalized = match kind.trim().to_ascii_lowercase().as_str() {
            "jira" => normalize_jira_reference(trimmed),
            "github" => normalize_github_reference(trimmed),
            other => {
                return Err(LoTaRError::ValidationError(format!(
                    "Unsupported reference kind: {}",
                    other
                )));
            }
        };

        let field_kind = kind.trim().to_ascii_lowercase();
        let kind_label = field_kind.clone();
        Self::commit_reference_change(
            storage,
            task_id,
            &kind_label,
            false,
            move |task, changed_values| {
                let before_len = task.references.len();
                if field_kind == "jira" {
                    task.references
                        .retain(|r| r.jira.as_deref() != Some(normalized.as_str()));
                } else {
                    task.references
                        .retain(|r| r.github.as_deref() != Some(normalized.as_str()));
                }
                if task.references.len() == before_len {
                    return false;
                }
                changed_values.push(normalized.clone());
                true
            },
        )
    }

    pub fn attach_file_reference(
        storage: &mut Storage,
        repo_root: &Path,
        task_id: &str,
        file: &str,
    ) -> LoTaRResult<ReferenceMutationOutcome> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let trimmed = file.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Missing file reference".to_string(),
            ));
        }
        if trimmed.len() > 4096 {
            return Err(LoTaRError::ValidationError(
                "File reference is too long (max 4096 characters)".to_string(),
            ));
        }

        let repo_root_canonical = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        let resolved = Self::resolve_path(&repo_root_canonical, trimmed)
            .map_err(LoTaRError::ValidationError)?;
        Self::ensure_outside_attachments_store(&storage.root_path, &resolved)
            .map_err(LoTaRError::ValidationError)?;
        let rel = resolved
            .strip_prefix(&repo_root_canonical)
            .unwrap_or(&resolved);
        let normalized = Self::normalize_path_for_display(rel);

        Self::commit_reference_change(
            storage,
            task_id,
            "file",
            true,
            move |task, changed_values| {
                let already = task
                    .references
                    .iter()
                    .any(|r| r.file.as_deref() == Some(normalized.as_str()));
                if already {
                    return false;
                }
                task.references.push(ReferenceEntry {
                    file: Some(normalized.clone()),
                    ..Default::default()
                });
                changed_values.push(normalized.clone());
                true
            },
        )
    }

    pub fn detach_file_reference(
        storage: &mut Storage,
        repo_root: &Path,
        task_id: &str,
        file: &str,
    ) -> LoTaRResult<ReferenceMutationOutcome> {
        let derived = crate::storage::TaskId::parse(task_id)
            .map_err(|err| LoTaRError::InvalidTaskId(format!("{task_id}: {err}")))?
            .project;
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let trimmed = file.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Missing file reference".to_string(),
            ));
        }

        let repo_root_canonical = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        let normalized = match Self::resolve_path(&repo_root_canonical, trimmed) {
            Ok(resolved) => {
                Self::ensure_outside_attachments_store(&storage.root_path, &resolved)
                    .map_err(LoTaRError::ValidationError)?;
                let rel = resolved
                    .strip_prefix(&repo_root_canonical)
                    .unwrap_or(&resolved);
                Self::normalize_path_for_display(rel)
            }
            Err(_) => trimmed.to_string(),
        };

        Self::commit_reference_change(
            storage,
            task_id,
            "file",
            false,
            move |task, changed_values| {
                let before_len = task.references.len();
                task.references
                    .retain(|r| r.file.as_deref() != Some(normalized.as_str()));
                if task.references.len() == before_len {
                    return false;
                }
                changed_values.push(normalized.clone());
                true
            },
        )
    }

    /// Repository `file` references must never point into the configured
    /// attachments store (DEV-61): managed blobs are only reachable as
    /// typed `attachment` references. The check resolves the task project's
    /// configured store (custom `attachments_dir`, absolute or relative,
    /// symlinked entries included via canonicalization on both sides) and
    /// fails closed when the resolved target lands inside it.
    /// Repository-path reference kinds (`file`, `code`) must never point
    /// into a managed attachments store (DEV-61): managed blobs are only
    /// reachable as typed `attachment` references, and aliasing them as
    /// repo paths would dangle after blob reclamation. The check covers
    /// EVERY store configured under this tasks root - the base store plus
    /// each project's `attachments.dir` override (absolute or relative,
    /// symlinked targets included via canonicalization) - so a task cannot
    /// reach around its own store by targeting another project's.
    fn ensure_outside_attachments_store(tasks_root: &Path, resolved: &Path) -> Result<(), String> {
        if Self::path_inside_managed_stores(tasks_root, resolved)? {
            return Err(
                "Reference target is inside the managed attachments store; use attachment references for stored blobs instead of file references"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// True when `resolved` (an existing, canonicalized repository path)
    /// lies inside any managed attachments store configured under
    /// `tasks_root`. Nonexistent store directories cannot contain files,
    /// so unresolvable roots are simply skipped.
    pub(crate) fn path_inside_managed_stores(
        tasks_root: &Path,
        resolved: &Path,
    ) -> Result<bool, String> {
        Ok(Self::managed_store_roots(tasks_root)?
            .iter()
            .any(|store| resolved.starts_with(store)))
    }

    /// All distinct managed attachment store roots configured under
    /// `tasks_root`: the base store plus every project directory's
    /// overridden store, canonicalized when present and deduped in stable
    /// discovery order. FAILS CLOSED: any configuration load/parse error
    /// (global or per-project) or an invalid `attachments.dir` value is
    /// propagated so callers deny the reference instead of silently
    /// operating with an unverified store set — an unknown custom store
    /// might contain the very blob being aliased. A store directory that
    /// does not exist yet cannot hold blobs, so its un-canonicalized path
    /// is kept without error.
    pub(crate) fn managed_store_roots(tasks_root: &Path) -> Result<Vec<PathBuf>, String> {
        // The merged chain deliberately swallows unreadable global config
        // layers, but this guard must fail closed: an existing-but-invalid
        // `.tasks/config.yml` could hide a custom attachments.dir, so its
        // parse failure denies the reference. A genuinely missing file is
        // the standard default and is fine.
        let global_path = crate::utils::paths::global_config_path(tasks_root);
        if global_path.exists() {
            crate::config::persistence::load_global_config(Some(tasks_root)).map_err(|e| {
                format!("Failed to load task configuration for attachment store guard: {e}")
            })?;
        }
        let base =
            crate::config::resolution::load_and_merge_configs(Some(tasks_root)).map_err(|e| {
                format!("Failed to load task configuration for attachment store guard: {e}")
            })?;
        let mut roots: Vec<PathBuf> = Vec::new();
        let mut push_store = |cfg: &crate::config::types::ResolvedConfig| -> Result<(), String> {
            let root =
                crate::services::attachment_service::AttachmentService::compute_attachments_root(
                    tasks_root, cfg,
                )
                .map_err(|e| format!("Invalid attachments store configuration: {e}"))?;
            let canonical = root.canonicalize().unwrap_or(root);
            if !roots.contains(&canonical) {
                roots.push(canonical);
            }
            Ok(())
        };
        push_store(&base)?;
        let entries = fs::read_dir(tasks_root)
            .map_err(|e| format!("Failed to enumerate projects for attachment store guard: {e}"))?;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with('@') || !entry.path().is_dir() {
                continue;
            }
            let cfg = crate::config::resolution::get_project_config(&base, name, tasks_root)
                .map_err(|e| {
                    format!("Failed to load project '{name}' configuration for attachment store guard: {e}")
                })?;
            push_store(&cfg)?;
        }
        Ok(roots)
    }

    /// Snippet preview with the managed-store guard applied: store blobs
    /// are never previewed as repository code (they render through the
    /// attachment download routes instead), so a `code` value that
    /// resolves into any configured store fails closed. Unresolvable
    /// paths delegate to [`Self::snippet_for_code`]'s own errors.
    pub fn snippet_for_code_guarded(
        tasks_root: &Path,
        repo_root: &Path,
        code: &str,
        context_before: usize,
        context_after: usize,
    ) -> Result<ReferenceSnippetDTO, String> {
        let (raw_path, _start, _end) = Self::split_reference(code);
        let repo_root_canonical = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        if let Ok(resolved) = Self::resolve_path(&repo_root_canonical, &raw_path)
            && Self::path_inside_managed_stores(tasks_root, &resolved)?
        {
            return Err(
                "Reference target is inside the managed attachments store; use attachment download routes for stored blobs instead of code references"
                    .to_string(),
            );
        }
        Self::snippet_for_code(repo_root, code, context_before, context_after)
    }

    pub fn snippet_for_code(
        repo_root: &Path,
        code: &str,
        context_before: usize,
        context_after: usize,
    ) -> Result<ReferenceSnippetDTO, String> {
        let (raw_path, start_line, end_line) = Self::split_reference(code);
        if raw_path.is_empty() {
            return Err("Reference path is empty".into());
        }

        let repo_root_canonical = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        let resolved = Self::resolve_path(&repo_root_canonical, &raw_path)?;
        let contents = fs::read_to_string(&resolved).map_err(|e| {
            format!(
                "Failed to read reference target {}: {}",
                resolved.display(),
                e
            )
        })?;

        let lines: Vec<&str> = contents.lines().collect();
        if lines.is_empty() {
            return Err("Referenced file is empty".into());
        }

        let total_lines = lines.len();
        let highlight_start = start_line.unwrap_or(1).max(1);
        if highlight_start > total_lines {
            return Err(format!(
                "Reference line {} exceeds file length {}",
                highlight_start, total_lines
            ));
        }

        let highlight_end_raw = end_line.unwrap_or(highlight_start).max(highlight_start);
        let highlight_end = highlight_end_raw.min(total_lines);

        let usable_before = context_before.min(highlight_start.saturating_sub(1));
        let usable_after = context_after.min(total_lines.saturating_sub(highlight_end));
        let start_line_inclusive = highlight_start - usable_before;
        let end_line_inclusive = (highlight_end + usable_after).min(total_lines);

        let mut snippet_lines = Vec::with_capacity(end_line_inclusive - start_line_inclusive + 1);
        for number in start_line_inclusive..=end_line_inclusive {
            if let Some(text) = lines.get(number - 1) {
                snippet_lines.push(ReferenceSnippetLineDTO {
                    number,
                    text: text.to_string(),
                });
            }
        }

        let path_display = resolved
            .strip_prefix(&repo_root_canonical)
            .unwrap_or(&resolved);
        let path_display = Self::normalize_path_for_display(path_display);

        let has_more_before = start_line_inclusive > 1;
        let has_more_after = end_line_inclusive < total_lines;

        Ok(ReferenceSnippetDTO {
            path: path_display,
            start_line: start_line_inclusive,
            end_line: end_line_inclusive,
            highlight_start,
            highlight_end,
            lines: snippet_lines,
            has_more_before,
            has_more_after,
            total_lines,
        })
    }

    fn split_reference(code: &str) -> (String, Option<usize>, Option<usize>) {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return (String::new(), None, None);
        }

        if let Some((path_part, anchor_part)) = trimmed.split_once('#') {
            let numbers = Self::extract_numbers(anchor_part);
            let start_line = numbers.first().copied();
            let end_line = numbers.get(1).copied();
            (path_part.trim().to_string(), start_line, end_line)
        } else {
            (trimmed.to_string(), None, None)
        }
    }

    fn extract_numbers(anchor: &str) -> Vec<usize> {
        let mut numbers = Vec::new();
        let mut buffer = String::new();
        for ch in anchor.chars() {
            if ch.is_ascii_digit() {
                buffer.push(ch);
            } else if !buffer.is_empty() {
                if let Ok(value) = buffer.parse::<usize>() {
                    numbers.push(value);
                }
                buffer.clear();
            }
        }
        if !buffer.is_empty()
            && let Ok(value) = buffer.parse::<usize>()
        {
            numbers.push(value);
        }
        numbers
    }

    pub(crate) fn resolve_path(repo_root: &Path, raw_path: &str) -> Result<PathBuf, String> {
        let path = PathBuf::from(raw_path);
        let candidate = if path.is_absolute() {
            path
        } else {
            repo_root.join(path)
        };
        let canonical = candidate
            .canonicalize()
            .map_err(|_| format!("Reference target not found: {}", candidate.display()))?;
        if !canonical.starts_with(repo_root) {
            return Err("Reference path escapes repository".into());
        }
        if !canonical.is_file() {
            return Err(format!(
                "Reference target is not a file: {}",
                canonical.display()
            ));
        }
        Ok(canonical)
    }

    fn normalize_path_for_display(path: &Path) -> String {
        let raw = path.to_string_lossy();
        raw.replace('\\', "/")
    }
}

fn trim_platform_prefix(value: &str, prefix: &str) -> String {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    let needle = format!("{}:", prefix.to_ascii_lowercase());
    if lower.starts_with(&needle) {
        trimmed[needle.len()..].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_jira_reference(value: &str) -> String {
    let trimmed = trim_platform_prefix(value, "jira");
    if let Some((prefix, rest)) = trimmed.split_once('-') {
        let prefix = prefix.trim();
        let rest = rest.trim();
        if !prefix.is_empty() && !rest.is_empty() {
            return format!("{}-{}", prefix.to_ascii_uppercase(), rest);
        }
    }
    trimmed
}

fn normalize_github_reference(value: &str) -> String {
    let trimmed = trim_platform_prefix(value, "github");
    if let Some((repo, rest)) = trimmed.split_once('#') {
        let repo = repo.trim().trim_matches('/').to_ascii_lowercase();
        let rest = rest.trim();
        if !repo.is_empty() && !rest.is_empty() {
            return format!("{}#{}", repo, rest);
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::TaskCreate;
    use crate::services::task_service::TaskService;
    use std::fs;
    use std::path::Path;

    fn unpack(outcome: ReferenceMutationOutcome) -> (TaskDTO, bool) {
        (outcome.task, outcome.changed)
    }

    #[test]
    fn snippet_for_code_returns_expected_context() {
        let temp = tempfile::tempdir().unwrap();
        let repo_root = temp.path();
        fs::create_dir_all(repo_root.join("src")).unwrap();
        fs::write(
            repo_root.join("src/example.rs"),
            "fn main() {}\n// line two\nlet value = 10;\nprintln!(\"{}\", value);\n",
        )
        .unwrap();

        let snippet = ReferenceService::snippet_for_code(repo_root, "src/example.rs#2-3", 1, 1)
            .expect("snippet should load");

        assert_eq!(snippet.path, "src/example.rs");
        assert_eq!(snippet.highlight_start, 2);
        assert_eq!(snippet.highlight_end, 3);
        assert_eq!(snippet.start_line, 1);
        assert_eq!(snippet.end_line, 4);
        assert_eq!(snippet.lines.len(), 4);
        assert_eq!(snippet.lines[1].text.trim(), "// line two");
        assert_eq!(snippet.lines[2].text.trim(), "let value = 10;");
        assert!(!snippet.has_more_before);
        assert!(!snippet.has_more_after);
        assert_eq!(snippet.total_lines, 4);
    }

    #[test]
    fn snippet_for_code_handles_top_of_file() {
        let temp = tempfile::tempdir().unwrap();
        let repo_root = temp.path();
        fs::create_dir_all(repo_root.join("src")).unwrap();
        fs::write(repo_root.join("src/lib.rs"), "first\nsecond\nthird\n").unwrap();

        let snippet = ReferenceService::snippet_for_code(repo_root, "src/lib.rs#1", 3, 3)
            .expect("snippet should load");

        assert_eq!(snippet.start_line, 1);
        assert_eq!(snippet.highlight_start, 1);
        assert_eq!(snippet.highlight_end, 1);
        assert_eq!(snippet.lines.len(), 3);
        assert_eq!(snippet.lines[0].text, "first");
        assert!(!snippet.has_more_before);
        assert!(!snippet.has_more_after);
        assert_eq!(snippet.total_lines, 3);
    }

    #[test]
    fn snippet_for_code_errors_when_file_missing() {
        let temp = tempfile::tempdir().unwrap();
        let repo_root = temp.path();

        let err = ReferenceService::snippet_for_code(repo_root, "src/missing.rs#4", 2, 2)
            .expect_err("expected failure for missing file");

        assert!(err.contains("not found"));
    }

    #[test]
    fn normalize_path_for_display_converts_backslashes() {
        let path = Path::new("src\\example.rs");
        let normalized = ReferenceService::normalize_path_for_display(path);
        assert_eq!(normalized, "src/example.rs");
    }

    #[test]
    fn attach_and_detach_link_reference_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let tasks_dir = temp.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        let mut storage = Storage::new(&tasks_dir);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Link reference test".to_string(),
                project: Some("DEMO".to_string()),
                status: None,
                priority: None,
                task_type: None,
                reporter: None,
                assignee: None,
                due_date: None,
                effort: None,
                description: None,
                tags: vec![],
                acceptance_criteria: vec![],
                relationships: None,
                custom_fields: None,
                sprints: vec![],
            },
        )
        .unwrap();

        let url = "https://example.com/docs";
        let (updated, added) =
            unpack(ReferenceService::attach_link_reference(&mut storage, &task.id, url).unwrap());
        assert!(added);
        assert!(
            updated
                .references
                .iter()
                .any(|r| r.link.as_deref() == Some(url))
        );

        let (_updated2, added2) =
            unpack(ReferenceService::attach_link_reference(&mut storage, &task.id, url).unwrap());
        assert!(!added2);

        let (updated3, removed) =
            unpack(ReferenceService::detach_link_reference(&mut storage, &task.id, url).unwrap());
        assert!(removed);
        assert!(
            !updated3
                .references
                .iter()
                .any(|r| r.link.as_deref() == Some(url))
        );
    }

    #[test]
    fn attach_and_detach_file_reference_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let repo_root = temp.path();
        fs::create_dir_all(repo_root.join("src")).unwrap();
        fs::write(repo_root.join("src/example.rs"), "fn main() {}\n").unwrap();

        let storage_root = repo_root.join(".tasks");
        fs::create_dir_all(&storage_root).unwrap();
        let mut storage = Storage::new(&storage_root);

        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "File reference test".to_string(),
                project: Some("T".to_string()),
                status: None,
                ..TaskCreate::default()
            },
        )
        .unwrap();

        let (task, added) = unpack(
            ReferenceService::attach_file_reference(
                &mut storage,
                repo_root,
                &task.id,
                "src/example.rs",
            )
            .unwrap(),
        );
        assert!(added);
        assert!(
            task.references
                .iter()
                .any(|r| r.file.as_deref() == Some("src/example.rs"))
        );

        let (task, removed) = unpack(
            ReferenceService::detach_file_reference(
                &mut storage,
                repo_root,
                &task.id,
                "src/example.rs",
            )
            .unwrap(),
        );
        assert!(removed);
        assert!(
            !task
                .references
                .iter()
                .any(|r| r.file.as_deref() == Some("src/example.rs"))
        );
    }

    #[test]
    fn attach_link_reference_accepts_non_http_schemes() {
        let temp = tempfile::tempdir().unwrap();
        let tasks_dir = temp.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        let mut storage = Storage::new(&tasks_dir);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Link reference scheme test".to_string(),
                project: Some("DEMO".to_string()),
                status: None,
                priority: None,
                task_type: None,
                reporter: None,
                assignee: None,
                due_date: None,
                effort: None,
                description: None,
                tags: vec![],
                acceptance_criteria: vec![],
                relationships: None,
                custom_fields: None,
                sprints: vec![],
            },
        )
        .unwrap();

        let url = "ftp://example.com/path/to/file";
        let (updated, added) =
            unpack(ReferenceService::attach_link_reference(&mut storage, &task.id, url).unwrap());
        assert!(added);
        assert!(
            updated
                .references
                .iter()
                .any(|r| r.link.as_deref() == Some(url))
        );
    }

    #[test]
    fn attach_link_reference_rejects_javascript_scheme() {
        let temp = tempfile::tempdir().unwrap();
        let tasks_dir = temp.path().join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        let mut storage = Storage::new(&tasks_dir);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Link reference safety test".to_string(),
                project: Some("DEMO".to_string()),
                status: None,
                priority: None,
                task_type: None,
                reporter: None,
                assignee: None,
                due_date: None,
                effort: None,
                description: None,
                tags: vec![],
                acceptance_criteria: vec![],
                relationships: None,
                custom_fields: None,
                sprints: vec![],
            },
        )
        .unwrap();

        let err =
            ReferenceService::attach_link_reference(&mut storage, &task.id, "javascript:alert(1)")
                .expect_err("expected validation error");

        assert!(matches!(err, LoTaRError::ValidationError(_)));
    }

    #[test]
    fn attach_and_detach_code_reference_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let repo_root = temp.path();
        fs::create_dir_all(repo_root.join("src")).unwrap();
        fs::write(
            repo_root.join("src/example.rs"),
            "line1\nline2\nline3\nline4\n",
        )
        .unwrap();

        let tasks_dir = repo_root.join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        let mut storage = Storage::new(&tasks_dir);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Code reference test".to_string(),
                project: Some("DEMO".to_string()),
                status: None,
                priority: None,
                task_type: None,
                reporter: None,
                assignee: None,
                due_date: None,
                effort: None,
                description: None,
                tags: vec![],
                acceptance_criteria: vec![],
                relationships: None,
                custom_fields: None,
                sprints: vec![],
            },
        )
        .unwrap();

        let ref_code = "src/example.rs#2-3";
        let (updated, added) = unpack(
            ReferenceService::attach_code_reference(&mut storage, repo_root, &task.id, ref_code)
                .unwrap(),
        );
        assert!(added);
        assert!(
            updated
                .references
                .iter()
                .any(|r| r.code.as_deref() == Some(ref_code))
        );

        let (_updated2, added2) = unpack(
            ReferenceService::attach_code_reference(&mut storage, repo_root, &task.id, ref_code)
                .unwrap(),
        );
        assert!(!added2);

        let (updated3, removed) = unpack(
            ReferenceService::detach_code_reference(&mut storage, &task.id, ref_code).unwrap(),
        );
        assert!(removed);
        assert!(
            !updated3
                .references
                .iter()
                .any(|r| r.code.as_deref() == Some(ref_code))
        );
    }

    #[test]
    fn attach_code_reference_normalizes_legacy_l_format_and_detach_accepts_legacy_string() {
        let temp = tempfile::tempdir().unwrap();
        let repo_root = temp.path();
        fs::create_dir_all(repo_root.join("src")).unwrap();
        fs::write(
            repo_root.join("src/example.rs"),
            "line1\nline2\nline3\nline4\n",
        )
        .unwrap();

        let tasks_dir = repo_root.join(".tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        let mut storage = Storage::new(&tasks_dir);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Legacy format test".to_string(),
                project: Some("DEMO".to_string()),
                status: None,
                ..TaskCreate::default()
            },
        )
        .unwrap();

        let legacy = "src/example.rs#L2-L3";
        let canonical = "src/example.rs#2-3";

        let (updated, added) = unpack(
            ReferenceService::attach_code_reference(&mut storage, repo_root, &task.id, legacy)
                .unwrap(),
        );
        assert!(added);
        assert!(
            updated
                .references
                .iter()
                .any(|r| r.code.as_deref() == Some(canonical))
        );

        let (updated2, removed) = unpack(
            ReferenceService::detach_code_reference(&mut storage, &task.id, legacy).unwrap(),
        );
        assert!(removed);
        assert!(
            !updated2
                .references
                .iter()
                .any(|r| r.code.as_deref() == Some(canonical))
        );
    }

    #[test]
    fn normalize_jira_reference_uppercases_prefix() {
        assert_eq!(normalize_jira_reference("jira:abc-123"), "ABC-123");
        assert_eq!(normalize_jira_reference("AbC- 42"), "ABC-42");
        assert_eq!(normalize_jira_reference("XYZ-9"), "XYZ-9");
    }

    #[test]
    fn normalize_github_reference_lowercases_repo() {
        assert_eq!(
            normalize_github_reference("github:Org/Repo#1"),
            "org/repo#1"
        );
        assert_eq!(normalize_github_reference("ORG/Repo#42"), "org/repo#42");
        assert_eq!(normalize_github_reference("repo#7"), "repo#7");
    }
}
