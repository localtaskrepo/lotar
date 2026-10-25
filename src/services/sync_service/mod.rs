//! Two-way sync between local tasks and Jira or GitHub issues. The entry
//! points live here; the work is split across the submodules.

use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue, json};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::api_types::{
    SyncReport, SyncReportEntry, SyncReportMeta, SyncResponse, SyncSummary, SyncValidateResponse,
    TaskCreate, TaskDTO, TaskListFilter, TaskUpdate,
};
use crate::config::manager::ConfigManager;
use crate::config::types::{
    SyncAuthProfile, SyncFieldMapping, SyncFieldMappingDetail, SyncProvider, SyncRemoteConfig,
    SyncWhenEmpty,
};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::reference_service::ReferenceService;
use crate::services::sync_report_service::SyncReportService;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::types::{CustomFieldValue, CustomFields, Priority, TaskStatus, TaskType};
use crate::workspace::TasksDirectoryResolver;

mod client;
mod github;
mod jira;
mod journal;
mod mapping;
mod pull;
mod push;
mod report;

use client::*;
use github::*;
use jira::*;
use journal::*;
use mapping::*;
use pull::*;
use push::*;
use report::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDirection {
    Push,
    Pull,
}

impl SyncDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncDirection::Push => "push",
            SyncDirection::Pull => "pull",
        }
    }
}

pub struct SyncService;

#[cfg(test)]
#[path = "../../../tests/common/sync_retry_cases.rs"]
mod retry_safety_tests;

#[cfg(test)]
#[path = "../../../tests/common/sync_push_characterization.rs"]
mod push_characterization_tests;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod provider_tests;

/// Options for a push or pull run, shared by the CLI, REST and MCP entry
/// points.
#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOptions<'a> {
    /// Name of the configured remote to sync with.
    pub remote: &'a str,
    pub project: Option<&'a str>,
    pub dry_run: bool,
    pub auth_profile: Option<&'a str>,
    /// Limit the run to a single task.
    pub task_id: Option<&'a str>,
    /// Override the configured `sync_write_reports` setting.
    pub write_report: Option<bool>,
    /// Return the per-task report entries in the response.
    pub include_report: bool,
    pub client_run_id: Option<&'a str>,
}

impl<'a> From<&'a crate::api_types::SyncRequest> for SyncOptions<'a> {
    fn from(request: &'a crate::api_types::SyncRequest) -> Self {
        Self {
            remote: &request.remote,
            project: request.project.as_deref(),
            dry_run: request.dry_run,
            auth_profile: request.auth_profile.as_deref(),
            task_id: request.task_id.as_deref(),
            write_report: request.write_report,
            include_report: request.include_report.unwrap_or(false),
            client_run_id: request.client_run_id.as_deref(),
        }
    }
}

/// Fixed inputs of a push or pull pass.
#[derive(Clone, Copy)]
struct SyncTarget<'a> {
    resolver: &'a TasksDirectoryResolver,
    remote: &'a SyncRemoteConfig,
    task_id: Option<&'a str>,
    dry_run: bool,
}

/// Where a push or pull pass records its results.
struct SyncSinks<'a> {
    recorder: &'a mut SyncReportRecorder,
    warnings: &'a mut Vec<String>,
    journal: &'a mut SyncJournal,
}

impl SyncService {
    pub fn push(
        resolver: &TasksDirectoryResolver,
        options: SyncOptions<'_>,
    ) -> LoTaRResult<SyncResponse> {
        Self::run(resolver, SyncDirection::Push, options)
    }

    pub fn pull(
        resolver: &TasksDirectoryResolver,
        options: SyncOptions<'_>,
    ) -> LoTaRResult<SyncResponse> {
        Self::run(resolver, SyncDirection::Pull, options)
    }

    pub fn validate(
        resolver: &TasksDirectoryResolver,
        project: Option<&str>,
        remote: Option<&str>,
        remote_override: Option<SyncRemoteConfig>,
        auth_profile: Option<&str>,
    ) -> LoTaRResult<SyncValidateResponse> {
        let mgr = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| LoTaRError::ValidationError(format!("Failed to load config: {}", e)))?;

        let trimmed_remote = remote.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        let (remote_label, remote_config) = if let Some(override_config) = remote_override {
            let label = trimmed_remote
                .clone()
                .unwrap_or_else(|| "custom".to_string());
            (label, override_config)
        } else {
            let name = trimmed_remote.clone().ok_or_else(|| {
                LoTaRError::ValidationError("Sync remote name is required".to_string())
            })?;
            let project_prefix = resolve_project_prefix(&mgr, resolver, project, None)?;
            let resolved = if let Some(prefix) = project_prefix.as_deref() {
                mgr.get_project_config(prefix).map_err(|e| {
                    LoTaRError::ValidationError(format!(
                        "Failed to load project config '{}': {}",
                        prefix, e
                    ))
                })?
            } else {
                mgr.get_resolved_config().clone()
            };
            let remote_config = resolved.remotes.get(&name).cloned().ok_or_else(|| {
                let available = resolved.remotes.keys().cloned().collect::<Vec<_>>();
                if available.is_empty() {
                    LoTaRError::ValidationError("No remotes are configured".to_string())
                } else {
                    LoTaRError::ValidationError(format!(
                        "Unknown sync remote '{}'. Available: {}",
                        name,
                        available.join(", ")
                    ))
                }
            })?;
            (name, remote_config)
        };

        let mut warnings = Vec::new();
        let info = Vec::new();
        let auth_profile_name = select_auth_profile(auth_profile, &remote_config);
        let profile_name = auth_profile_name.as_deref().ok_or_else(|| {
            LoTaRError::ValidationError("Auth profile is required for sync validation".to_string())
        })?;

        let home_config = ConfigManager::load_home_config().map_err(|e| {
            LoTaRError::ValidationError(format!("Failed to load home config: {}", e))
        })?;
        let profile = home_config.auth_profiles.get(profile_name).ok_or_else(|| {
            LoTaRError::ValidationError(format!(
                "Auth profile '{}' not found in home config",
                profile_name
            ))
        })?;
        let auth_context =
            build_auth_context(&remote_config, profile_name, profile, &mut warnings)?;
        let client = SyncClient::new(auth_context);

        match remote_config.provider {
            SyncProvider::Jira => {
                if let Some(project_key) = remote_config.project.as_deref() {
                    let trimmed = project_key.trim();
                    if !trimmed.is_empty() {
                        let _ = jira_fetch_project(&client, trimmed)?;
                    }
                }
                let jql = jira_query_for_remote(&remote_config)?;
                jira_validate_query(&client, &jql)?;
            }
            SyncProvider::Github => {
                let repo = remote_config.repo.as_deref().ok_or_else(|| {
                    LoTaRError::ValidationError("GitHub remote must define repo".to_string())
                })?;
                let _ = github_fetch_repo(&client, repo)?;
                if let Some(filter) = remote_config.filter.as_deref() {
                    let trimmed = filter.trim();
                    if !trimmed.is_empty() {
                        github_validate_search(&client, repo, trimmed, &mut warnings)?;
                    }
                }
            }
        }

        Ok(SyncValidateResponse {
            status: "ok".to_string(),
            provider: provider_label(&remote_config.provider).to_string(),
            remote: remote_label,
            project: remote_config.project.clone(),
            repo: remote_config.repo.clone(),
            filter: remote_config.filter.clone(),
            checked_at: Utc::now().to_rfc3339(),
            warnings,
            info,
        })
    }

    pub fn run(
        resolver: &TasksDirectoryResolver,
        direction: SyncDirection,
        options: SyncOptions<'_>,
    ) -> LoTaRResult<SyncResponse> {
        let SyncOptions {
            remote,
            project,
            dry_run,
            auth_profile,
            task_id,
            write_report,
            include_report,
            client_run_id,
        } = options;
        let trimmed_remote = remote.trim();
        if trimmed_remote.is_empty() {
            return Err(LoTaRError::ValidationError(
                "Sync remote name is required".to_string(),
            ));
        }

        let mgr = ConfigManager::new_manager_with_tasks_dir_readonly(&resolver.path)
            .map_err(|e| LoTaRError::ValidationError(format!("Failed to load config: {}", e)))?;

        let project_prefix = resolve_project_prefix(&mgr, resolver, project, task_id)?;
        if let (Some(task_id), Some(prefix)) = (task_id, project_prefix.as_deref()) {
            let trimmed = task_id.trim();
            if !trimmed.is_empty() {
                let expected = format!("{}-", prefix);
                if !trimmed.starts_with(&expected) {
                    return Err(LoTaRError::ValidationError(format!(
                        "Task '{}' does not belong to project '{}'",
                        trimmed, prefix
                    )));
                }
            }
        }
        let project_prefix = project_prefix
            .map(|prefix| canonical_sync_project(&resolver.path, &prefix))
            .transpose()?;
        let canonical_target = task_id
            .map(|id| {
                let id = id.trim();
                let prefix =
                    crate::storage::operations::StorageOperations::get_project_for_task(id)
                        .ok_or_else(|| LoTaRError::InvalidTaskId(id.to_string()))?;
                canonical_sync_task_id(&resolver.path, &prefix, id)
            })
            .transpose()?;
        let task_id = canonical_target.as_deref();
        let resolved = if let Some(prefix) = project_prefix.as_deref() {
            mgr.get_project_config(prefix).map_err(|e| {
                LoTaRError::ValidationError(format!(
                    "Failed to load project config '{}': {}",
                    prefix, e
                ))
            })?
        } else {
            mgr.get_resolved_config().clone()
        };

        let remote_config = resolved
            .remotes
            .get(trimmed_remote)
            .cloned()
            .ok_or_else(|| {
                let available = resolved.remotes.keys().cloned().collect::<Vec<_>>();
                if available.is_empty() {
                    LoTaRError::ValidationError("No remotes are configured".to_string())
                } else {
                    LoTaRError::ValidationError(format!(
                        "Unknown sync remote '{}'. Available: {}",
                        trimmed_remote,
                        available.join(", ")
                    ))
                }
            })?;

        let mut warnings = Vec::new();
        let mut info = Vec::new();
        let auth_profile_name = select_auth_profile(auth_profile, &remote_config);
        let needs_auth = direction == SyncDirection::Pull || !dry_run;

        let auth_context = if needs_auth {
            let profile_name = auth_profile_name.as_deref().ok_or_else(|| {
                LoTaRError::ValidationError(
                    "Auth profile is required for sync operations".to_string(),
                )
            })?;
            let home_config = ConfigManager::load_home_config().map_err(|e| {
                LoTaRError::ValidationError(format!("Failed to load home config: {}", e))
            })?;
            let profile = home_config.auth_profiles.get(profile_name).ok_or_else(|| {
                LoTaRError::ValidationError(format!(
                    "Auth profile '{}' not found in home config",
                    profile_name
                ))
            })?;
            Some(build_auth_context(
                &remote_config,
                profile_name,
                profile,
                &mut warnings,
            )?)
        } else {
            if let Some(profile_name) = auth_profile_name.as_deref() {
                if let Ok(home) = ConfigManager::load_home_config() {
                    if !home.auth_profiles.contains_key(profile_name) {
                        warnings.push(format!(
                            "Auth profile '{}' not found in home config",
                            profile_name
                        ));
                    }
                } else {
                    warnings
                        .push("Home config not available; auth profile cannot be resolved".into());
                }
            }
            None
        };

        let client = auth_context.map(SyncClient::new);

        let pull_project = if direction == SyncDirection::Pull {
            Some(resolve_pull_project_prefix(
                project_prefix.clone(),
                &remote_config,
                &mut warnings,
            )?)
        } else {
            None
        };

        let run_project = if direction == SyncDirection::Pull {
            pull_project.clone()
        } else {
            project_prefix.clone()
        };
        let context = SyncRunContext {
            run_id: make_run_id(client_run_id),
            started_at: Utc::now().to_rfc3339(),
            direction,
            provider: provider_label(&remote_config.provider).to_string(),
            remote: trimmed_remote.to_string(),
            project: run_project.clone(),
            dry_run,
        };
        emit_sync_event(
            "sync_started",
            json!({
                "run_id": context.run_id.clone(),
                "direction": context.direction.as_str(),
                "provider": context.provider.clone(),
                "remote": context.remote.clone(),
                "project": context.project.clone(),
                "dry_run": context.dry_run,
                "started_at": context.started_at.clone(),
            }),
        );

        let mut recorder = SyncReportRecorder::new(context);

        let outcome = (|| {
            let mut journal = SyncJournal::open(&resolver.path, dry_run)?;
            if let Some(client) = client.as_ref() {
                journal.reconcile(
                    resolver,
                    &pending_scope(&remote_config, client),
                    run_project.as_deref(),
                    task_id,
                    &remote_config,
                )?;
            } else if !journal.links.is_empty() {
                warnings.push(
                    "Pending sync recovery exists; dry-run creation estimates do not reconcile it"
                        .into(),
                );
            }

            let target = SyncTarget {
                resolver,
                remote: &remote_config,
                task_id,
                dry_run,
            };
            let sinks = SyncSinks {
                recorder: &mut recorder,
                warnings: &mut warnings,
                journal: &mut journal,
            };
            match direction {
                SyncDirection::Push => {
                    perform_push(target, project_prefix.as_deref(), client.as_ref(), sinks)
                }
                SyncDirection::Pull => perform_pull(
                    target,
                    pull_project
                        .as_deref()
                        .expect("pull project must be resolved"),
                    client.as_ref().expect("sync client required"),
                    sinks,
                ),
            }
        })();

        if let Err(err) = outcome {
            emit_sync_event(
                "sync_failed",
                json!({
                    "run_id": recorder.context.run_id.clone(),
                    "direction": recorder.context.direction.as_str(),
                    "provider": recorder.context.provider.clone(),
                    "remote": recorder.context.remote.clone(),
                    "project": recorder.context.project.clone(),
                    "dry_run": recorder.context.dry_run,
                    "error": err.to_string(),
                    "finished_at": Utc::now().to_rfc3339(),
                }),
            );
            return Err(err);
        }

        if dry_run {
            info.push("Dry run: no changes applied".into());
        }

        let report_status = "ok";
        let report = recorder.report(report_status, warnings.clone(), info.clone());
        let write_enabled = !dry_run && write_report.unwrap_or(resolved.sync_write_reports);
        let stored_path = match SyncReportService::write_report(
            &resolver.path,
            &resolved,
            &report,
            write_enabled,
        ) {
            Ok(path) => path,
            Err(err) => {
                warnings.push(format!("Failed to write sync report: {}", err));
                None
            }
        };

        let report_meta = recorder.meta(report_status, warnings.clone(), info.clone(), stored_path);
        emit_sync_event(
            "sync_completed",
            json!({
                "run_id": report_meta.id.clone(),
                "project": report_meta.project.clone(),
                "report": report_meta.clone(),
                "finished_at": Utc::now().to_rfc3339(),
            }),
        );

        let report_entries = if include_report {
            recorder.entries.clone()
        } else {
            Vec::new()
        };

        Ok(SyncResponse {
            status: "ok".to_string(),
            direction: direction.as_str().to_string(),
            provider: provider_label(&remote_config.provider).to_string(),
            remote: trimmed_remote.to_string(),
            project: run_project,
            dry_run,
            summary: recorder.summary.clone(),
            warnings,
            info,
            run_id: recorder.context.run_id.clone(),
            report: Some(report_meta),
            report_entries,
        })
    }
}

fn resolve_project_prefix(
    mgr: &ConfigManager,
    resolver: &TasksDirectoryResolver,
    project: Option<&str>,
    task_id: Option<&str>,
) -> LoTaRResult<Option<String>> {
    if let Some(project) = project {
        let resolved = crate::utils::project::resolve_project_input(project, &resolver.path);
        if resolved.trim().is_empty() {
            return Err(LoTaRError::ValidationError(
                "Project cannot be empty".to_string(),
            ));
        }
        return Ok(Some(resolved));
    }

    if let Some(task_id) = task_id
        && let Ok(parsed) = crate::storage::TaskId::parse(task_id.trim())
    {
        return Ok(Some(parsed.project));
    }

    let default_project = mgr.get_resolved_config().default_project.trim().to_string();
    if default_project.is_empty() {
        Ok(None)
    } else {
        Ok(Some(default_project))
    }
}

fn select_auth_profile(provided: Option<&str>, remote: &SyncRemoteConfig) -> Option<String> {
    if let Some(profile) = provided {
        let trimmed = profile.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    } else if let Some(profile) = remote.auth_profile.as_deref() {
        let trimmed = profile.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    } else {
        None
    }
}

fn append_failure_warnings(failures: &[String], warnings: &mut Vec<String>) {
    if failures.is_empty() {
        return;
    }
    let limit = 5usize;
    for failure in failures.iter().take(limit) {
        warnings.push(failure.clone());
    }
    if failures.len() > limit {
        warnings.push(format!(
            "{} additional sync errors omitted",
            failures.len() - limit
        ));
    }
}

fn provider_label(provider: &SyncProvider) -> &'static str {
    match provider {
        SyncProvider::Jira => "jira",
        SyncProvider::Github => "github",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::custom_value_string;

    fn sample_task() -> TaskDTO {
        TaskDTO {
            task_state: None,
            deleted_at: None,
            id: "TEST-1".to_string(),
            title: "Example".to_string(),
            status: TaskStatus::from("Todo"),
            priority: Priority::from("High"),
            task_type: TaskType::from("Feature"),
            reporter: Some("me@example.com".to_string()),
            assignee: Some("dev".to_string()),
            created: "2024-01-01T00:00:00Z".to_string(),
            modified: "2024-01-01T00:00:00Z".to_string(),
            due_date: None,
            effort: None,
            subtitle: None,
            description: Some("Body".to_string()),
            tags: vec!["alpha".to_string()],
            relationships: Default::default(),
            comments: vec![],
            references: vec![],
            acceptance_criteria: vec![],
            sprints: vec![],
            sprint_order: Default::default(),
            history: vec![],
            custom_fields: Default::default(),
        }
    }

    fn sync_workspace() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(".tasks");
        std::fs::create_dir_all(root.join("ABC-OPS")).unwrap();
        std::fs::create_dir_all(root.join("TP")).unwrap();
        std::fs::write(
            root.join("ABC-OPS").join("12.yml"),
            "title: nested\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n",
        )
        .unwrap();
        std::fs::write(
            root.join("TP").join("1.yml"),
            "title: padded\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n",
        )
        .unwrap();
        (tmp, root)
    }

    #[test]
    fn canonical_sync_task_id_accepts_hyphenated_prefix() {
        let (_tmp, root) = sync_workspace();
        let canonical = canonical_sync_task_id(&root, "ABC-OPS", "ABC-OPS-12").unwrap();
        assert_eq!(canonical, "ABC-OPS-12");
    }

    #[test]
    fn canonical_sync_task_id_collapses_padded_alias() {
        let (_tmp, root) = sync_workspace();
        // Padded spelling resolves to the same file and canonicalizes unpadded.
        let canonical = canonical_sync_task_id(&root, "TP", "TP-001").unwrap();
        assert_eq!(canonical, "TP-1");
    }

    #[test]
    fn canonical_sync_task_id_rejects_wrong_project_for_hyphenated_id() {
        let (_tmp, root) = sync_workspace();
        // First-dash project ('ABC') must no longer satisfy the guard.
        let err = canonical_sync_task_id(&root, "ABC", "ABC-OPS-12").unwrap_err();
        assert!(matches!(err, LoTaRError::InvalidTaskId(_)), "{err}");
    }

    #[test]
    fn canonical_sync_task_id_rejects_sibling_root_task() {
        let tmp = tempfile::tempdir().unwrap();
        // Primary workspace: .tasks with a different project.
        let primary = tmp.path().join("main").join(".tasks");
        std::fs::create_dir_all(primary.join("DEV")).unwrap();
        // Sibling workspace holding the target task.
        let sibling = tmp.path().join("sibling").join(".tasks");
        std::fs::create_dir_all(sibling.join("ABC-OPS")).unwrap();
        std::fs::write(
            sibling.join("ABC-OPS").join("12.yml"),
            "title: elsewhere\nstatus: Todo\npriority: Medium\ntype: Feature\ncreated: 2026-01-01T00:00:00Z\n",
        )
        .unwrap();

        let err = canonical_sync_task_id(&primary, "ABC-OPS", "ABC-OPS-12").unwrap_err();
        assert!(matches!(err, LoTaRError::InvalidTaskId(_)), "{err}");
    }

    #[test]
    fn canonical_sync_task_id_accepts_plain_primary_root_task() {
        let (_tmp, root) = sync_workspace();
        let canonical = canonical_sync_task_id(&root, "TP", "TP-1").unwrap();
        assert_eq!(canonical, "TP-1");
    }

    #[test]
    fn github_payload_maps_labels_and_state() {
        let mut remote = SyncRemoteConfig {
            provider: SyncProvider::Github,
            project: None,
            repo: Some("org/repo".to_string()),
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        remote.mapping.insert(
            "status".to_string(),
            SyncFieldMapping::Detailed(SyncFieldMappingDetail {
                field: Some("state".to_string()),
                values: HashMap::from([
                    (String::from("Todo"), String::from("open")),
                    (String::from("Done"), String::from("closed")),
                ]),
                ..Default::default()
            }),
        );
        remote.mapping.insert(
            "tags".to_string(),
            SyncFieldMapping::Simple("labels".to_string()),
        );

        let payload = build_github_payload(&remote, &sample_task(), SyncOperation::Update);
        assert_eq!(payload.state.as_deref(), Some("open"));
        assert_eq!(payload.labels, vec!["alpha".to_string()]);
    }

    #[test]
    fn jira_pull_mapping_extracts_status() {
        let mut remote = SyncRemoteConfig {
            provider: SyncProvider::Jira,
            project: Some("DEMO".to_string()),
            repo: None,
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        remote.mapping.insert(
            "status".to_string(),
            SyncFieldMapping::Simple("status".to_string()),
        );

        let issue = json!({
            "key": "DEMO-1",
            "fields": {
                "status": {"name": "In Progress"}
            }
        });
        let update = build_task_update_from_issue(SyncProvider::Jira, &remote, &issue, None);
        assert_eq!(update.status.unwrap().to_string(), "In Progress");
    }

    #[test]
    fn merge_custom_fields_removes_null_updates() {
        let mut existing: CustomFields = Default::default();
        existing.insert("keep".to_string(), custom_value_string("value"));
        existing.insert("drop".to_string(), custom_value_string("old"));

        let mut updates: CustomFields = Default::default();
        updates.insert("drop".to_string(), custom_value_null());
        updates.insert("new".to_string(), custom_value_string("fresh"));

        let merged = merge_custom_fields(Some(&existing), updates);
        assert!(merged.contains_key("keep"));
        assert!(!merged.contains_key("drop"));
        assert!(merged.contains_key("new"));
    }
}
