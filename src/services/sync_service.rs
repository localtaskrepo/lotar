use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::Utc;
use serde_json::{Map as JsonMap, Value as JsonValue, json};
use std::collections::{HashMap, HashSet};
use std::env;
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

static RUN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
struct SyncRunContext {
    run_id: String,
    started_at: String,
    direction: SyncDirection,
    provider: String,
    remote: String,
    project: Option<String>,
    dry_run: bool,
}

#[derive(Clone, Copy, Debug)]
enum SyncEntryStatus {
    Created,
    Updated,
    Skipped,
    Failed,
}

impl SyncEntryStatus {
    fn as_str(self) -> &'static str {
        match self {
            SyncEntryStatus::Created => "created",
            SyncEntryStatus::Updated => "updated",
            SyncEntryStatus::Skipped => "skipped",
            SyncEntryStatus::Failed => "failed",
        }
    }
}

struct SyncReportRecorder {
    context: SyncRunContext,
    summary: SyncSummary,
    entries: Vec<SyncReportEntry>,
}

impl SyncReportRecorder {
    fn new(context: SyncRunContext) -> Self {
        Self {
            context,
            summary: SyncSummary::default(),
            entries: Vec::new(),
        }
    }

    fn record(&mut self, status: SyncEntryStatus, entry: SyncReportEntry) {
        match status {
            SyncEntryStatus::Created => self.summary.created += 1,
            SyncEntryStatus::Updated => self.summary.updated += 1,
            SyncEntryStatus::Skipped => self.summary.skipped += 1,
            SyncEntryStatus::Failed => self.summary.failed += 1,
        }
        self.entries.push(entry.clone());

        emit_sync_event(
            "sync_progress",
            json!({
                "run_id": self.context.run_id.clone(),
                "direction": self.context.direction.as_str(),
                "provider": self.context.provider.clone(),
                "remote": self.context.remote.clone(),
                "project": self.context.project.clone(),
                "dry_run": self.context.dry_run,
                "summary": self.summary.clone(),
                "entry": entry,
            }),
        );
    }

    fn report(&self, status: &str, warnings: Vec<String>, info: Vec<String>) -> SyncReport {
        SyncReport {
            id: self.context.run_id.clone(),
            created_at: self.context.started_at.clone(),
            status: status.to_string(),
            direction: self.context.direction.as_str().to_string(),
            provider: self.context.provider.clone(),
            remote: self.context.remote.clone(),
            project: self.context.project.clone(),
            dry_run: self.context.dry_run,
            summary: self.summary.clone(),
            warnings,
            info,
            entries: self.entries.clone(),
        }
    }

    fn meta(
        &self,
        status: &str,
        warnings: Vec<String>,
        info: Vec<String>,
        stored_path: Option<String>,
    ) -> SyncReportMeta {
        SyncReportMeta {
            id: self.context.run_id.clone(),
            created_at: self.context.started_at.clone(),
            status: status.to_string(),
            direction: self.context.direction.as_str().to_string(),
            provider: self.context.provider.clone(),
            remote: self.context.remote.clone(),
            project: self.context.project.clone(),
            dry_run: self.context.dry_run,
            summary: self.summary.clone(),
            warnings,
            info,
            entries_total: self.entries.len(),
            stored_path,
        }
    }
}

fn emit_sync_event(kind: &str, payload: JsonValue) {
    crate::api_events::emit(crate::api_events::ApiEvent {
        kind: kind.to_string(),
        data: payload,
    });
}

fn make_run_id(client_run_id: Option<&str>) -> String {
    if let Some(value) = client_run_id {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
    let counter = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("sync-{stamp}-{counter}")
}

fn make_entry(
    status: SyncEntryStatus,
    task_id: Option<String>,
    reference: Option<String>,
    title: Option<String>,
    message: Option<String>,
) -> SyncReportEntry {
    SyncReportEntry {
        status: status.as_str().to_string(),
        at: Utc::now().to_rfc3339(),
        task_id,
        reference,
        title,
        message,
        fields: Vec::new(),
    }
}

fn make_entry_with_fields(
    status: SyncEntryStatus,
    task_id: Option<String>,
    reference: Option<String>,
    title: Option<String>,
    fields: Vec<String>,
    message: Option<String>,
) -> SyncReportEntry {
    SyncReportEntry {
        status: status.as_str().to_string(),
        at: Utc::now().to_rfc3339(),
        task_id,
        reference,
        title,
        message,
        fields,
    }
}

pub struct SyncService;

impl SyncService {
    #[allow(clippy::too_many_arguments)]
    pub fn push(
        resolver: &TasksDirectoryResolver,
        remote: &str,
        project: Option<&str>,
        dry_run: bool,
        auth_profile: Option<&str>,
        task_id: Option<&str>,
        write_report: Option<bool>,
        include_report: bool,
        client_run_id: Option<&str>,
    ) -> LoTaRResult<SyncResponse> {
        Self::run(
            resolver,
            SyncDirection::Push,
            remote,
            project,
            dry_run,
            auth_profile,
            task_id,
            write_report,
            include_report,
            client_run_id,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn pull(
        resolver: &TasksDirectoryResolver,
        remote: &str,
        project: Option<&str>,
        dry_run: bool,
        auth_profile: Option<&str>,
        task_id: Option<&str>,
        write_report: Option<bool>,
        include_report: bool,
        client_run_id: Option<&str>,
    ) -> LoTaRResult<SyncResponse> {
        Self::run(
            resolver,
            SyncDirection::Pull,
            remote,
            project,
            dry_run,
            auth_profile,
            task_id,
            write_report,
            include_report,
            client_run_id,
        )
    }

    #[allow(clippy::too_many_arguments)]
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

    #[allow(clippy::too_many_arguments)]
    fn run(
        resolver: &TasksDirectoryResolver,
        direction: SyncDirection,
        remote: &str,
        project: Option<&str>,
        dry_run: bool,
        auth_profile: Option<&str>,
        task_id: Option<&str>,
        write_report: Option<bool>,
        include_report: bool,
        client_run_id: Option<&str>,
    ) -> LoTaRResult<SyncResponse> {
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

        let outcome = match direction {
            SyncDirection::Push => perform_push(
                resolver,
                &remote_config,
                project_prefix.as_deref(),
                task_id,
                dry_run,
                client.as_ref(),
                &mut recorder,
                &mut warnings,
            ),
            SyncDirection::Pull => perform_pull(
                resolver,
                &remote_config,
                pull_project
                    .as_deref()
                    .expect("pull project must be resolved"),
                task_id,
                dry_run,
                client.as_ref().expect("sync client required"),
                &mut recorder,
                &mut warnings,
            ),
        };

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
        let write_enabled = write_report.unwrap_or(resolved.sync_write_reports);
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

    if let Some(task_id) = task_id {
        let derived = task_id.split('-').next().unwrap_or("").trim();
        if !derived.is_empty() {
            return Ok(Some(derived.to_string()));
        }
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

#[derive(Debug, Clone, Copy)]
enum SyncOperation {
    Create,
    Update,
}

#[derive(Clone)]
struct AuthContext {
    provider: SyncProvider,
    api_base: String,
    auth_header: Option<String>,
    user_agent: String,
}

struct SyncClient {
    agent: ureq::Agent,
    auth: AuthContext,
}

impl SyncClient {
    fn new(auth: AuthContext) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout_read(Duration::from_secs(30))
                .timeout_write(Duration::from_secs(30))
                .build(),
            auth,
        }
    }
}

#[derive(Clone, Debug)]
enum FieldValue {
    String(String),
    List(Vec<String>),
}

impl FieldValue {
    fn is_empty(&self) -> bool {
        match self {
            FieldValue::String(value) => value.trim().is_empty(),
            FieldValue::List(values) => values.iter().all(|v| v.trim().is_empty()),
        }
    }

    fn to_string_value(&self) -> Option<String> {
        match self {
            FieldValue::String(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            FieldValue::List(values) => values
                .iter()
                .find(|v| !v.trim().is_empty())
                .map(|v| v.trim().to_string()),
        }
    }

    fn to_list_value(&self) -> Vec<String> {
        match self {
            FieldValue::String(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    Vec::new()
                } else {
                    vec![trimmed.to_string()]
                }
            }
            FieldValue::List(values) => values
                .iter()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect(),
        }
    }
}

fn normalize_field_values(value: &FieldValue) -> Vec<String> {
    let mut values = value
        .to_list_value()
        .into_iter()
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn field_values_match(a: &FieldValue, b: &FieldValue) -> bool {
    normalize_field_values(a) == normalize_field_values(b)
}

#[derive(Default)]
struct LocalFieldValues {
    title: Option<String>,
    description: Option<String>,
    status: Option<TaskStatus>,
    task_type: Option<TaskType>,
    priority: Option<Priority>,
    assignee: Option<String>,
    reporter: Option<String>,
    tags: Option<Vec<String>>,
    custom_fields: Option<CustomFields>,
}

#[derive(Default, Debug)]
struct GithubIssuePayload {
    title: Option<String>,
    body: Option<String>,
    state: Option<String>,
    labels: Vec<String>,
    assignees: Vec<String>,
    labels_explicit: bool,
    assignees_explicit: bool,
}

impl GithubIssuePayload {
    fn is_empty(&self, include_state: bool) -> bool {
        self.title.is_none()
            && self.body.is_none()
            && self.labels.is_empty()
            && self.assignees.is_empty()
            && !self.labels_explicit
            && !self.assignees_explicit
            && (!include_state || self.state.is_none())
    }

    fn to_json(&self, include_state: bool) -> JsonValue {
        let mut map = JsonMap::new();
        if let Some(title) = self.title.as_ref() {
            map.insert("title".to_string(), JsonValue::String(title.clone()));
        }
        if let Some(body) = self.body.as_ref() {
            map.insert("body".to_string(), JsonValue::String(body.clone()));
        }
        if include_state && let Some(state) = self.state.as_ref() {
            map.insert("state".to_string(), JsonValue::String(state.clone()));
        }
        if self.labels_explicit || !self.labels.is_empty() {
            map.insert(
                "labels".to_string(),
                JsonValue::Array(
                    self.labels
                        .iter()
                        .map(|label| JsonValue::String(label.clone()))
                        .collect(),
                ),
            );
        }
        if self.assignees_explicit || !self.assignees.is_empty() {
            map.insert(
                "assignees".to_string(),
                JsonValue::Array(
                    self.assignees
                        .iter()
                        .map(|assignee| JsonValue::String(assignee.clone()))
                        .collect(),
                ),
            );
        }
        JsonValue::Object(map)
    }
}

#[derive(Default, Debug)]
struct JiraIssuePayload {
    fields: JsonMap<String, JsonValue>,
    desired_status: Option<String>,
}

impl JiraIssuePayload {
    fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.desired_status.is_none()
    }
}

#[derive(Default)]
struct JiraLookupCache {
    issue_types: Option<Vec<String>>,
    user_cache: HashMap<String, String>,
}

#[derive(Debug)]
enum ReferenceState {
    Matching(String),
    ProviderOnly(String),
    None,
}

#[allow(clippy::too_many_arguments)]
fn perform_push(
    resolver: &TasksDirectoryResolver,
    remote: &SyncRemoteConfig,
    project: Option<&str>,
    task_id: Option<&str>,
    dry_run: bool,
    client: Option<&SyncClient>,
    recorder: &mut SyncReportRecorder,
    warnings: &mut Vec<String>,
) -> LoTaRResult<()> {
    if remote.provider == SyncProvider::Github
        && remote.repo.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(LoTaRError::ValidationError(
            "GitHub remote must define repo".to_string(),
        ));
    }

    let storage = Storage::new(resolver.path.clone());
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

    for (_id, task) in tasks {
        match determine_reference_state(remote, &task) {
            ReferenceState::Matching(reference) => {
                let task_id = Some(task.id.clone());
                let title = Some(task.title.clone());
                let reference_value = Some(reference.clone());
                let (updated, update_fields) = match remote.provider {
                    SyncProvider::Jira => {
                        if !dry_run {
                            ensure_jira_issue_types(&mut jira_lookup, client, remote, warnings);
                        }
                        let mut payload = build_jira_payload(
                            remote,
                            &task,
                            SyncOperation::Update,
                            client,
                            &mut jira_lookup,
                            warnings,
                        );
                        let mut changed_fields = jira_payload_field_names(&payload);
                        if payload.is_empty() {
                            recorder.record(
                                SyncEntryStatus::Skipped,
                                make_entry(
                                    SyncEntryStatus::Skipped,
                                    task_id.clone(),
                                    reference_value.clone(),
                                    title.clone(),
                                    Some("No changes to push".to_string()),
                                ),
                            );
                            continue;
                        }
                        if dry_run {
                            recorder.record(
                                SyncEntryStatus::Updated,
                                make_entry_with_fields(
                                    SyncEntryStatus::Updated,
                                    task_id.clone(),
                                    reference_value.clone(),
                                    title.clone(),
                                    changed_fields.clone(),
                                    Some("Dry run: would update".to_string()),
                                ),
                            );
                            continue;
                        }
                        let client = client.ok_or_else(|| {
                            LoTaRError::ValidationError(
                                "Auth profile is required for sync operations".to_string(),
                            )
                        })?;
                        if !payload.is_empty() {
                            match jira_fetch_issue(client, &reference, &changed_fields) {
                                Ok(issue) => {
                                    changed_fields =
                                        filter_jira_payload_against_issue(&issue, &mut payload);
                                }
                                Err(err) => {
                                    warnings.push(format!(
                                        "Jira issue {} fetch failed; skipping diff: {}",
                                        reference, err
                                    ));
                                }
                            }
                        }
                        if payload.is_empty() {
                            recorder.record(
                                SyncEntryStatus::Skipped,
                                make_entry(
                                    SyncEntryStatus::Skipped,
                                    task_id.clone(),
                                    reference_value.clone(),
                                    title.clone(),
                                    Some("No changes to push".to_string()),
                                ),
                            );
                            continue;
                        }
                        if !payload.fields.is_empty()
                            && let Err(err) = jira_update_issue(client, &reference, &payload)
                        {
                            failures.push(format!(
                                "Failed to update Jira issue {}: {}",
                                reference, err
                            ));
                            recorder.record(
                                SyncEntryStatus::Failed,
                                make_entry(
                                    SyncEntryStatus::Failed,
                                    task_id.clone(),
                                    reference_value.clone(),
                                    title.clone(),
                                    Some(format!("Failed to update Jira issue: {}", err)),
                                ),
                            );
                            continue;
                        }
                        if let Some(status) = payload.desired_status.as_deref()
                            && let Err(err) = jira_transition_issue(client, &reference, status)
                        {
                            warnings.push(format!(
                                "Jira status transition for {} failed: {}",
                                reference, err
                            ));
                        }
                        (true, changed_fields)
                    }
                    SyncProvider::Github => {
                        let mut payload =
                            build_github_payload(remote, &task, SyncOperation::Update);
                        let mut changed_fields = github_payload_field_names(&payload);
                        if payload.is_empty(true) {
                            recorder.record(
                                SyncEntryStatus::Skipped,
                                make_entry(
                                    SyncEntryStatus::Skipped,
                                    task_id.clone(),
                                    reference_value.clone(),
                                    title.clone(),
                                    Some("No changes to push".to_string()),
                                ),
                            );
                            continue;
                        }
                        if dry_run {
                            recorder.record(
                                SyncEntryStatus::Updated,
                                make_entry_with_fields(
                                    SyncEntryStatus::Updated,
                                    task_id.clone(),
                                    reference_value.clone(),
                                    title.clone(),
                                    changed_fields.clone(),
                                    Some("Dry run: would update".to_string()),
                                ),
                            );
                            continue;
                        }
                        let client = client.ok_or_else(|| {
                            LoTaRError::ValidationError(
                                "Auth profile is required for sync operations".to_string(),
                            )
                        })?;
                        let (repo, number) =
                            parse_github_reference(&reference, remote.repo.as_deref()).ok_or_else(
                                || {
                                    LoTaRError::ValidationError(format!(
                                        "Invalid GitHub reference '{}'",
                                        reference
                                    ))
                                },
                            )?;
                        match github_fetch_issue(client, &repo, number) {
                            Ok(issue) => {
                                changed_fields =
                                    filter_github_payload_against_issue(&issue, &mut payload);
                            }
                            Err(err) => {
                                warnings.push(format!(
                                    "GitHub issue {} fetch failed; skipping diff: {}",
                                    reference, err
                                ));
                            }
                        }
                        if payload.is_empty(true) {
                            recorder.record(
                                SyncEntryStatus::Skipped,
                                make_entry(
                                    SyncEntryStatus::Skipped,
                                    task_id.clone(),
                                    reference_value.clone(),
                                    title.clone(),
                                    Some("No changes to push".to_string()),
                                ),
                            );
                            continue;
                        }
                        if let Err(err) = github_update_issue(client, &repo, number, &payload) {
                            failures.push(format!(
                                "Failed to update GitHub issue {}: {}",
                                reference, err
                            ));
                            recorder.record(
                                SyncEntryStatus::Failed,
                                make_entry(
                                    SyncEntryStatus::Failed,
                                    task_id.clone(),
                                    reference_value.clone(),
                                    title.clone(),
                                    Some(format!("Failed to update GitHub issue: {}", err)),
                                ),
                            );
                            continue;
                        }
                        (true, changed_fields)
                    }
                };

                if updated {
                    recorder.record(
                        SyncEntryStatus::Updated,
                        make_entry_with_fields(
                            SyncEntryStatus::Updated,
                            task_id,
                            reference_value,
                            title,
                            update_fields,
                            None,
                        ),
                    );
                }
            }
            ReferenceState::ProviderOnly(reference) => {
                recorder.record(
                    SyncEntryStatus::Skipped,
                    make_entry(
                        SyncEntryStatus::Skipped,
                        Some(task.id.clone()),
                        Some(reference),
                        Some(task.title.clone()),
                        Some("Reference does not match remote".to_string()),
                    ),
                );
            }
            ReferenceState::None => {
                if dry_run {
                    let message = match remote.provider {
                        SyncProvider::Jira => {
                            let payload = build_jira_payload(
                                remote,
                                &task,
                                SyncOperation::Create,
                                client,
                                &mut jira_lookup,
                                warnings,
                            );
                            format_field_list(&jira_payload_field_names(&payload))
                                .map(|fields| format!("Dry run: would create {}", fields))
                        }
                        SyncProvider::Github => {
                            let payload =
                                build_github_payload(remote, &task, SyncOperation::Create);
                            format_field_list(&github_payload_field_names(&payload))
                                .map(|fields| format!("Dry run: would create {}", fields))
                        }
                    };
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
                let client = client.ok_or_else(|| {
                    LoTaRError::ValidationError(
                        "Auth profile is required for sync operations".to_string(),
                    )
                })?;
                let mut storage = Storage::new(resolver.path.clone());
                match remote.provider {
                    SyncProvider::Jira => {
                        ensure_jira_issue_types(&mut jira_lookup, Some(client), remote, warnings);
                        let payload = build_jira_payload(
                            remote,
                            &task,
                            SyncOperation::Create,
                            Some(client),
                            &mut jira_lookup,
                            warnings,
                        );
                        let project = remote.project.as_deref().ok_or_else(|| {
                            LoTaRError::ValidationError(
                                "Jira remote must define project".to_string(),
                            )
                        })?;
                        let key = match jira_create_issue(client, project, &payload) {
                            Ok(key) => key,
                            Err(err) => {
                                failures.push(format!(
                                    "Failed to create Jira issue for {}: {}",
                                    task.id, err
                                ));
                                recorder.record(
                                    SyncEntryStatus::Failed,
                                    make_entry(
                                        SyncEntryStatus::Failed,
                                        Some(task.id.clone()),
                                        None,
                                        Some(task.title.clone()),
                                        Some(format!("Failed to create Jira issue: {}", err)),
                                    ),
                                );
                                continue;
                            }
                        };
                        let reference = normalize_jira_reference(&key).unwrap_or(key);
                        if let Err(err) = ReferenceService::attach_platform_reference(
                            &mut storage,
                            &task.id,
                            "jira",
                            &reference,
                        ) {
                            failures.push(format!(
                                "Failed to attach Jira reference for {}: {}",
                                task.id, err
                            ));
                            recorder.record(
                                SyncEntryStatus::Failed,
                                make_entry(
                                    SyncEntryStatus::Failed,
                                    Some(task.id.clone()),
                                    Some(reference.clone()),
                                    Some(task.title.clone()),
                                    Some(format!("Failed to attach Jira reference: {}", err)),
                                ),
                            );
                            continue;
                        }
                        if let Some(status) = payload.desired_status.as_deref()
                            && let Err(err) = jira_transition_issue(client, &reference, status)
                        {
                            warnings.push(format!(
                                "Jira status transition for {} failed: {}",
                                reference, err
                            ));
                        }
                        recorder.record(
                            SyncEntryStatus::Created,
                            make_entry(
                                SyncEntryStatus::Created,
                                Some(task.id.clone()),
                                Some(reference),
                                Some(task.title.clone()),
                                format_field_list(&jira_payload_field_names(&payload))
                                    .map(|fields| format!("Created fields: {}", fields)),
                            ),
                        );
                    }
                    SyncProvider::Github => {
                        let payload = build_github_payload(remote, &task, SyncOperation::Create);
                        let repo = remote.repo.as_deref().ok_or_else(|| {
                            LoTaRError::ValidationError(
                                "GitHub remote must define repo".to_string(),
                            )
                        })?;
                        let number = match github_create_issue(client, repo, &payload) {
                            Ok(number) => number,
                            Err(err) => {
                                failures.push(format!(
                                    "Failed to create GitHub issue for {}: {}",
                                    task.id, err
                                ));
                                recorder.record(
                                    SyncEntryStatus::Failed,
                                    make_entry(
                                        SyncEntryStatus::Failed,
                                        Some(task.id.clone()),
                                        None,
                                        Some(task.title.clone()),
                                        Some(format!("Failed to create GitHub issue: {}", err)),
                                    ),
                                );
                                continue;
                            }
                        };
                        let reference = format!("{}#{}", normalize_github_repo(repo), number);
                        if let Err(err) = ReferenceService::attach_platform_reference(
                            &mut storage,
                            &task.id,
                            "github",
                            &reference,
                        ) {
                            failures.push(format!(
                                "Failed to attach GitHub reference for {}: {}",
                                task.id, err
                            ));
                            recorder.record(
                                SyncEntryStatus::Failed,
                                make_entry(
                                    SyncEntryStatus::Failed,
                                    Some(task.id.clone()),
                                    Some(reference.clone()),
                                    Some(task.title.clone()),
                                    Some(format!("Failed to attach GitHub reference: {}", err)),
                                ),
                            );
                            continue;
                        }
                        if payload.state.as_deref() == Some("closed") {
                            let close_payload = GithubIssuePayload {
                                state: Some("closed".to_string()),
                                ..Default::default()
                            };
                            if let Err(err) =
                                github_update_issue(client, repo, number, &close_payload)
                            {
                                warnings.push(format!(
                                    "Failed to close GitHub issue {}: {}",
                                    reference, err
                                ));
                            }
                        }
                        recorder.record(
                            SyncEntryStatus::Created,
                            make_entry(
                                SyncEntryStatus::Created,
                                Some(task.id.clone()),
                                Some(reference),
                                Some(task.title.clone()),
                                format_field_list(&github_payload_field_names(&payload))
                                    .map(|fields| format!("Created fields: {}", fields)),
                            ),
                        );
                    }
                }
            }
        }
    }

    append_failure_warnings(&failures, warnings);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn perform_pull(
    resolver: &TasksDirectoryResolver,
    remote: &SyncRemoteConfig,
    project: &str,
    task_id: Option<&str>,
    dry_run: bool,
    client: &SyncClient,
    recorder: &mut SyncReportRecorder,
    warnings: &mut Vec<String>,
) -> LoTaRResult<()> {
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

        let storage = Storage::new(resolver.path.clone());
        let existing = TaskService::get(&storage, trimmed, Some(project))?;
        let reference = match determine_reference_state(remote, &existing) {
            ReferenceState::Matching(value) => value,
            ReferenceState::ProviderOnly(_) => {
                return Err(LoTaRError::ValidationError(
                    "Task reference does not match this remote".to_string(),
                ));
            }
            ReferenceState::None => {
                return Err(LoTaRError::ValidationError(
                    "Task is missing a remote reference".to_string(),
                ));
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

        let mut storage = Storage::new(resolver.path.clone());
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

    let storage = Storage::new(resolver.path.clone());

    let filter = TaskListFilter {
        project: Some(project.to_string()),
        ..Default::default()
    };
    let tasks = TaskService::list(&storage, &filter);
    let mut tasks_by_id: HashMap<String, TaskDTO> = HashMap::new();
    for (_id, task) in &tasks {
        tasks_by_id.insert(task.id.clone(), task.clone());
    }
    let reference_index = build_reference_index(remote, &tasks);

    let mut failures = Vec::new();

    let mut storage = Storage::new(resolver.path.clone());

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
            let (create, status) =
                build_task_create_from_issue(remote.provider, remote, &issue, project)?;
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
            let created_task = match TaskService::create(&mut storage, create) {
                Ok(task) => task,
                Err(err) => {
                    failures.push(format!("Failed to create local task: {}", err));
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
            if let Err(err) = ReferenceService::attach_platform_reference(
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
            if let Some(status) = status {
                let update = TaskUpdate {
                    status: Some(status),
                    ..Default::default()
                };
                if let Err(err) = TaskService::update(&mut storage, &created_task.id, update) {
                    warnings.push(format!(
                        "Failed to update status for {}: {}",
                        created_task.id, err
                    ));
                }
            }
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

fn resolve_pull_project_prefix(
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

fn build_auth_context(
    remote: &SyncRemoteConfig,
    profile_name: &str,
    profile: &SyncAuthProfile,
    warnings: &mut Vec<String>,
) -> LoTaRResult<AuthContext> {
    let provider = profile.provider.unwrap_or(remote.provider);

    if profile.provider.is_some() && provider != remote.provider {
        warnings.push(format!(
            "Auth profile '{}' provider does not match remote provider",
            profile_name
        ));
    }

    let method = profile
        .method
        .as_deref()
        .unwrap_or(match provider {
            SyncProvider::Jira => "basic",
            SyncProvider::Github => "token",
        })
        .trim()
        .to_ascii_lowercase();

    let api_base = match provider {
        SyncProvider::Jira => profile
            .api_url
            .as_deref()
            .or(profile.base_url.as_deref())
            .map(strip_trailing_slash)
            .ok_or_else(|| {
                LoTaRError::ValidationError(format!(
                    "Auth profile '{}' must set api_url or base_url",
                    profile_name
                ))
            })?,
        SyncProvider::Github => profile
            .api_url
            .as_deref()
            .map(strip_trailing_slash)
            .unwrap_or_else(|| "https://api.github.com".to_string()),
    };

    let auth_header = match method.as_str() {
        "basic" => {
            let email_env = profile.email_env.as_deref().ok_or_else(|| {
                LoTaRError::ValidationError(format!(
                    "Auth profile '{}' must set email_env (env var name or literal) for basic auth",
                    profile_name
                ))
            })?;
            let token_env = profile.token_env.as_deref().ok_or_else(|| {
                LoTaRError::ValidationError(format!(
                    "Auth profile '{}' must set token_env (env var name or literal) for basic auth",
                    profile_name
                ))
            })?;
            let email = resolve_auth_value(email_env, profile_name, "email", warnings)?;
            let token = resolve_auth_value(token_env, profile_name, "token", warnings)?;
            let raw = format!("{}:{}", email, token);
            Some(format!("Basic {}", BASE64_STANDARD.encode(raw.as_bytes())))
        }
        "bearer" | "token" | "pat" => {
            let token_env = profile.token_env.as_deref().ok_or_else(|| {
                LoTaRError::ValidationError(format!(
                    "Auth profile '{}' must set token_env (env var name or literal)",
                    profile_name
                ))
            })?;
            let token = resolve_auth_value(token_env, profile_name, "token", warnings)?;
            Some(format!("Bearer {}", token))
        }
        other => {
            return Err(LoTaRError::ValidationError(format!(
                "Unsupported auth method '{}'",
                other
            )));
        }
    };

    Ok(AuthContext {
        provider,
        api_base,
        auth_header,
        user_agent: "lotar".to_string(),
    })
}

fn resolve_auth_value(
    name_or_value: &str,
    profile_name: &str,
    label: &str,
    warnings: &mut Vec<String>,
) -> LoTaRResult<String> {
    let trimmed = name_or_value.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError(format!(
            "Auth profile '{}' {} value is empty",
            profile_name, label
        )));
    }

    match env::var(trimmed) {
        Ok(value) => {
            let resolved = value.trim();
            if resolved.is_empty() {
                Err(LoTaRError::ValidationError(format!(
                    "Environment variable '{}' is empty",
                    trimmed
                )))
            } else {
                Ok(resolved.to_string())
            }
        }
        Err(_) => {
            if looks_like_env_var(trimmed) {
                warnings.push(format!(
                    "Auth profile '{}' {} env var is not set; using the literal value from home config",
                    profile_name, label
                ));
            }
            Ok(trimmed.to_string())
        }
    }
}

fn looks_like_env_var(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn strip_trailing_slash(raw: &str) -> String {
    raw.trim_end_matches('/').to_string()
}

fn request_with_headers(client: &SyncClient, req: ureq::Request) -> ureq::Request {
    let mut req = req.set("Accept", "application/json");
    req = req.set("Content-Type", "application/json");
    req = req.set("User-Agent", &client.auth.user_agent);
    if client.auth.provider == SyncProvider::Github {
        req = req.set("Accept", "application/vnd.github+json");
        req = req.set("X-GitHub-Api-Version", "2022-11-28");
    }
    if let Some(auth) = client.auth.auth_header.as_ref() {
        req = req.set("Authorization", auth);
    }
    req
}

fn send_json_request(
    client: &SyncClient,
    req: ureq::Request,
    body: Option<JsonValue>,
) -> LoTaRResult<JsonValue> {
    let response = match body {
        Some(payload) => request_with_headers(client, req).send_json(payload),
        None => request_with_headers(client, req).call(),
    };

    match response {
        Ok(resp) => parse_json_response(resp),
        Err(ureq::Error::Status(code, resp)) => {
            let payload = resp.into_string().unwrap_or_default();
            let message = payload.trim();
            let detail = if message.is_empty() {
                format!("Remote API error (HTTP {})", code)
            } else {
                format!("Remote API error (HTTP {}): {}", code, message)
            };
            Err(LoTaRError::ValidationError(detail))
        }
        Err(err) => Err(LoTaRError::ValidationError(format!(
            "Remote API request failed: {}",
            err
        ))),
    }
}

fn parse_json_response(resp: ureq::Response) -> LoTaRResult<JsonValue> {
    let payload = resp.into_string().unwrap_or_default();
    if payload.trim().is_empty() {
        return Ok(JsonValue::Null);
    }
    serde_json::from_str(&payload)
        .map_err(|e| LoTaRError::SerializationError(format!("Invalid JSON response: {}", e)))
}

fn jira_query_for_remote(remote: &SyncRemoteConfig) -> LoTaRResult<String> {
    let jql = if let Some(filter) = remote.filter.as_deref() {
        let trimmed = filter.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    } else {
        None
    }
    .or_else(|| {
        remote.project.as_ref().and_then(|project| {
            let trimmed = project.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(format!("project = {}", trimmed))
            }
        })
    })
    .ok_or_else(|| {
        LoTaRError::ValidationError("Jira remote requires filter or project".to_string())
    })?;
    Ok(jql)
}

fn jira_validate_query(client: &SyncClient, jql: &str) -> LoTaRResult<()> {
    let url = format!("{}/rest/api/3/search/jql", client.auth.api_base);
    let req = client
        .agent
        .get(&url)
        .query("jql", jql)
        .query("startAt", "0")
        .query("maxResults", "1")
        .query("fields", "summary");
    let _ = send_json_request(client, req, None)?;
    Ok(())
}

fn jira_fetch_project(client: &SyncClient, key: &str) -> LoTaRResult<JsonValue> {
    let url = format!("{}/rest/api/3/project/{}", client.auth.api_base, key);
    let req = client.agent.get(&url);
    send_json_request(client, req, None)
}

fn jira_search_issues(
    client: &SyncClient,
    remote: &SyncRemoteConfig,
) -> LoTaRResult<Vec<JsonValue>> {
    let jql = jira_query_for_remote(remote)?;

    let mut issues = Vec::new();
    let mut start_at = 0usize;
    let max_results = 50usize;
    loop {
        let url = format!("{}/rest/api/3/search/jql", client.auth.api_base);
        let req = client
            .agent
            .get(&url)
            .query("jql", &jql)
            .query("startAt", &start_at.to_string())
            .query("maxResults", &max_results.to_string())
            .query(
                "fields",
                "summary,description,status,issuetype,priority,assignee,reporter,labels",
            );
        let payload = send_json_request(client, req, None)?;
        let total = payload.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let batch = payload
            .get("issues")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if batch.is_empty() {
            break;
        }
        issues.extend(batch);
        start_at += max_results;
        if issues.len() >= total {
            break;
        }
    }
    Ok(issues)
}

fn jira_fetch_issue(client: &SyncClient, key: &str, fields: &[String]) -> LoTaRResult<JsonValue> {
    let url = format!("{}/rest/api/3/issue/{}", client.auth.api_base, key);
    let mut req = client.agent.get(&url);
    if !fields.is_empty() {
        req = req.query("fields", &fields.join(","));
    }
    send_json_request(client, req, None)
}

fn jira_create_issue(
    client: &SyncClient,
    project: &str,
    payload: &JiraIssuePayload,
) -> LoTaRResult<String> {
    let mut fields = payload.fields.clone();
    if !fields.contains_key("project") {
        fields.insert("project".to_string(), json!({"key": project}));
    }
    let url = format!("{}/rest/api/3/issue", client.auth.api_base);
    let req = client.agent.post(&url);
    let mut body = JsonMap::new();
    body.insert("fields".to_string(), JsonValue::Object(fields));
    let response = send_json_request(client, req, Some(JsonValue::Object(body)))?;
    response
        .get("key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| LoTaRError::SerializationError("Missing Jira issue key".to_string()))
}

fn jira_update_issue(
    client: &SyncClient,
    key: &str,
    payload: &JiraIssuePayload,
) -> LoTaRResult<()> {
    let url = format!("{}/rest/api/3/issue/{}", client.auth.api_base, key);
    let req = client.agent.put(&url);
    let mut body = JsonMap::new();
    body.insert(
        "fields".to_string(),
        JsonValue::Object(payload.fields.clone()),
    );
    send_json_request(client, req, Some(JsonValue::Object(body)))?;
    Ok(())
}

fn jira_transition_issue(client: &SyncClient, key: &str, status: &str) -> LoTaRResult<()> {
    let url = format!(
        "{}/rest/api/3/issue/{}/transitions",
        client.auth.api_base, key
    );
    let req = client.agent.get(&url);
    let payload = send_json_request(client, req, None)?;
    let transitions = payload
        .get("transitions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let desired = transitions.into_iter().find(|transition| {
        transition
            .get("name")
            .and_then(|v| v.as_str())
            .map(|name| name.eq_ignore_ascii_case(status))
            .unwrap_or(false)
    });

    let Some(transition) = desired else {
        return Err(LoTaRError::ValidationError(format!(
            "No Jira transition named '{}'",
            status
        )));
    };
    let transition_id = transition
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| LoTaRError::SerializationError("Missing transition id".to_string()))?;

    let req = client.agent.post(&url);
    let body = json!({"transition": {"id": transition_id}});
    send_json_request(client, req, Some(body))?;
    Ok(())
}

fn github_list_issues(
    client: &SyncClient,
    remote: &SyncRemoteConfig,
    warnings: &mut Vec<String>,
) -> LoTaRResult<Vec<JsonValue>> {
    let repo = remote
        .repo
        .as_deref()
        .ok_or_else(|| LoTaRError::ValidationError("GitHub remote must define repo".to_string()))?;
    let normalized_repo = normalize_github_repo(repo);
    if let Some(filter) = remote.filter.as_deref() {
        let trimmed = filter.trim();
        if !trimmed.is_empty() {
            warnings.push(
                "GitHub filter uses search API (results may omit fields and are capped at 1000)"
                    .to_string(),
            );
            return github_search_issues(client, &normalized_repo, trimmed, warnings);
        }
    }

    let mut issues = Vec::new();
    let mut page = 1u32;
    loop {
        let url = format!("{}/repos/{}/issues", client.auth.api_base, normalized_repo);
        let req = client
            .agent
            .get(&url)
            .query("state", "all")
            .query("per_page", "100")
            .query("page", &page.to_string());
        let payload = send_json_request(client, req, None)?;
        let batch = payload.as_array().cloned().unwrap_or_default();
        if batch.is_empty() {
            break;
        }
        for issue in &batch {
            if issue.get("pull_request").is_some() {
                continue;
            }
            issues.push(issue.clone());
        }
        if batch.len() < 100 {
            break;
        }
        page += 1;
    }
    Ok(issues)
}

fn github_fetch_repo(client: &SyncClient, repo: &str) -> LoTaRResult<JsonValue> {
    let url = format!(
        "{}/repos/{}",
        client.auth.api_base,
        normalize_github_repo(repo)
    );
    let req = client.agent.get(&url);
    send_json_request(client, req, None)
}

fn github_build_search_query(repo: &str, filter: &str) -> String {
    let normalized = normalize_github_repo(repo);
    let mut query = format!("repo:{} type:issue", normalized);
    if !filter_has_state(filter) {
        query.push_str(" state:all");
    }
    if !filter.trim().is_empty() {
        query.push(' ');
        query.push_str(filter.trim());
    }
    query
}

fn github_validate_search(
    client: &SyncClient,
    repo: &str,
    filter: &str,
    warnings: &mut Vec<String>,
) -> LoTaRResult<()> {
    warnings.push(
        "GitHub filter uses search API (results may omit fields and are capped at 1000)"
            .to_string(),
    );
    let query = github_build_search_query(repo, filter);
    let url = format!("{}/search/issues", client.auth.api_base);
    let req = client
        .agent
        .get(&url)
        .query("q", &query)
        .query("per_page", "1")
        .query("page", "1");
    let _ = send_json_request(client, req, None)?;
    Ok(())
}

fn github_search_issues(
    client: &SyncClient,
    repo: &str,
    filter: &str,
    warnings: &mut Vec<String>,
) -> LoTaRResult<Vec<JsonValue>> {
    let mut issues = Vec::new();
    let mut page = 1u32;
    let mut total_processed = 0usize;
    let query = github_build_search_query(repo, filter);

    loop {
        let url = format!("{}/search/issues", client.auth.api_base);
        let req = client
            .agent
            .get(&url)
            .query("q", &query)
            .query("per_page", "100")
            .query("page", &page.to_string());
        let payload = send_json_request(client, req, None)?;
        let batch = payload
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if batch.is_empty() {
            break;
        }
        for issue in &batch {
            if issue.get("pull_request").is_some() {
                continue;
            }
            issues.push(issue.clone());
        }
        total_processed += batch.len();
        if batch.len() < 100 {
            break;
        }
        if total_processed >= 1000 {
            warnings.push("GitHub search results truncated at 1000 issues".to_string());
            break;
        }
        page += 1;
    }
    let mut detailed = Vec::new();
    for issue in issues {
        let number = issue.get("number").and_then(|v| v.as_u64());
        if let Some(number) = number {
            match github_fetch_issue(client, repo, number) {
                Ok(full) => detailed.push(full),
                Err(err) => {
                    warnings.push(format!(
                        "GitHub issue {} fetch failed; using search payload: {}",
                        number, err
                    ));
                    detailed.push(issue);
                }
            }
        } else {
            detailed.push(issue);
        }
    }

    Ok(detailed)
}

fn github_create_issue(
    client: &SyncClient,
    repo: &str,
    payload: &GithubIssuePayload,
) -> LoTaRResult<u64> {
    let url = format!(
        "{}/repos/{}/issues",
        client.auth.api_base,
        normalize_github_repo(repo)
    );
    let req = client.agent.post(&url);
    let response = send_json_request(client, req, Some(payload.to_json(false)))?;
    response
        .get("number")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| LoTaRError::SerializationError("Missing GitHub issue number".to_string()))
}

fn github_update_issue(
    client: &SyncClient,
    repo: &str,
    number: u64,
    payload: &GithubIssuePayload,
) -> LoTaRResult<()> {
    let url = format!(
        "{}/repos/{}/issues/{}",
        client.auth.api_base,
        normalize_github_repo(repo),
        number
    );
    let req = client.agent.patch(&url);
    send_json_request(client, req, Some(payload.to_json(true)))?;
    Ok(())
}

fn github_fetch_issue(client: &SyncClient, repo: &str, number: u64) -> LoTaRResult<JsonValue> {
    let url = format!(
        "{}/repos/{}/issues/{}",
        client.auth.api_base,
        normalize_github_repo(repo),
        number
    );
    let req = client.agent.get(&url);
    send_json_request(client, req, None)
}

fn normalize_github_repo(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn filter_has_state(filter: &str) -> bool {
    filter
        .split_whitespace()
        .any(|token| token.to_ascii_lowercase().starts_with("state:"))
}

fn determine_reference_state(remote: &SyncRemoteConfig, task: &TaskDTO) -> ReferenceState {
    let mut provider_reference = None;
    let mut other_reference = None;

    for reference in &task.references {
        let (candidate, other_candidate) = match remote.provider {
            SyncProvider::Jira => (reference.jira.as_deref(), reference.github.as_deref()),
            SyncProvider::Github => (reference.github.as_deref(), reference.jira.as_deref()),
        };
        if let Some(raw) = candidate {
            if provider_reference.is_none() {
                provider_reference = Some(raw);
            }
            if let Some(normalized) = normalize_reference_for_remote(remote, raw) {
                return ReferenceState::Matching(normalized);
            }
        }
        if other_reference.is_none()
            && let Some(raw) = other_candidate
        {
            other_reference = Some(raw);
        }
    }

    if let Some(reference) = provider_reference {
        ReferenceState::ProviderOnly(reference.to_string())
    } else if let Some(reference) = other_reference {
        ReferenceState::ProviderOnly(reference.to_string())
    } else {
        ReferenceState::None
    }
}

fn normalize_reference_for_remote(remote: &SyncRemoteConfig, value: &str) -> Option<String> {
    match remote.provider {
        SyncProvider::Jira => {
            let normalized = normalize_jira_reference(value)?;
            if let Some(project) = remote.project.as_ref() {
                let prefix = format!("{}-", project.trim().to_ascii_uppercase());
                if !normalized.to_ascii_uppercase().starts_with(&prefix) {
                    return None;
                }
            }
            Some(normalized)
        }
        SyncProvider::Github => {
            let (repo, number) = parse_github_reference(value, remote.repo.as_deref())?;
            if let Some(expected) = remote.repo.as_deref() {
                let expected_norm = normalize_github_repo(expected);
                if normalize_github_repo(&repo) != expected_norm {
                    return None;
                }
            }
            Some(format!("{}#{}", normalize_github_repo(&repo), number))
        }
    }
}

fn normalize_jira_reference(value: &str) -> Option<String> {
    let trimmed = trim_platform_prefix(value, "jira");
    let (prefix, rest) = trimmed.split_once('-')?;
    if rest.trim().is_empty() {
        return None;
    }
    Some(format!(
        "{}-{}",
        prefix.trim().to_ascii_uppercase(),
        rest.trim()
    ))
}

fn parse_github_reference(value: &str, default_repo: Option<&str>) -> Option<(String, u64)> {
    let trimmed = trim_platform_prefix(value, "github");
    if let Some((repo, number)) = trimmed.split_once('#') {
        let repo_trimmed = repo.trim();
        let num = number.trim().parse::<u64>().ok()?;
        if repo_trimmed.is_empty() {
            let default_repo = default_repo?;
            return Some((default_repo.to_string(), num));
        }
        return Some((repo_trimmed.to_string(), num));
    }

    if let Ok(num) = trimmed.trim().parse::<u64>() {
        let default_repo = default_repo?;
        return Some((default_repo.to_string(), num));
    }

    None
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

fn build_reference_index(
    remote: &SyncRemoteConfig,
    tasks: &[(String, TaskDTO)],
) -> HashMap<String, String> {
    let mut index = HashMap::new();
    for (_id, task) in tasks {
        for reference in &task.references {
            let raw = match remote.provider {
                SyncProvider::Jira => reference.jira.as_deref(),
                SyncProvider::Github => reference.github.as_deref(),
            };
            if let Some(raw) = raw
                && let Some(normalized) = normalize_reference_for_remote(remote, raw)
            {
                index.insert(normalized, task.id.clone());
            }
        }
    }
    index
}

fn issue_reference_for_remote(remote: &SyncRemoteConfig, issue: &JsonValue) -> Option<String> {
    match remote.provider {
        SyncProvider::Jira => issue
            .get("key")
            .and_then(|v| v.as_str())
            .and_then(|key| normalize_reference_for_remote(remote, key)),
        SyncProvider::Github => {
            let repo = remote.repo.as_deref()?;
            let number = issue.get("number").and_then(|v| v.as_u64())?;
            Some(format!("{}#{}", normalize_github_repo(repo), number))
        }
    }
}

fn build_task_update_from_issue(
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
        status: local.status,
        priority: local.priority,
        task_type: local.task_type,
        reporter: local.reporter,
        assignee: local.assignee,
        due_date: None,
        effort: None,
        description: local.description,
        tags: local.tags,
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
        if update.status.as_ref() == Some(&existing.status) {
            update.status = None;
        }
        if update.priority.as_ref() == Some(&existing.priority) {
            update.priority = None;
        }
        if update.task_type.as_ref() == Some(&existing.task_type) {
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

fn build_task_create_from_issue(
    provider: SyncProvider,
    remote: &SyncRemoteConfig,
    issue: &JsonValue,
    project: &str,
) -> LoTaRResult<(TaskCreate, Option<TaskStatus>)> {
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
        priority: local.priority,
        task_type: local.task_type,
        reporter: local.reporter,
        assignee: local.assignee,
        due_date: None,
        effort: None,
        description: local.description,
        tags: local.tags.unwrap_or_default(),
        relationships: None,
        custom_fields: local.custom_fields,
        sprints: Vec::new(),
    };
    Ok((create, local.status))
}

fn map_issue_to_local_fields(
    provider: SyncProvider,
    remote: &SyncRemoteConfig,
    issue: &JsonValue,
    existing: Option<&TaskDTO>,
) -> LocalFieldValues {
    let mut out = LocalFieldValues::default();

    // Collect label values consumed by scalar field mappings (type, priority, status)
    // so we can exclude them when mapping labels to tags
    let mut consumed_labels: HashSet<String> = HashSet::new();
    for (local_key, mapping) in &remote.mapping {
        if !is_scalar_local_field(local_key) {
            continue;
        }
        let detail = normalize_mapping_detail(local_key, mapping);
        let remote_field = detail
            .field
            .clone()
            .unwrap_or_else(|| local_key.to_string());
        // Only track consumption from labels field
        if !matches!(remote_field.to_ascii_lowercase().as_str(), "labels") {
            continue;
        }
        // All mapped remote values are consumed
        for remote_val in detail.values.values() {
            consumed_labels.insert(remote_val.to_ascii_lowercase());
        }
    }

    for (local_key, mapping) in &remote.mapping {
        let detail = normalize_mapping_detail(local_key, mapping);
        let remote_field = detail
            .field
            .clone()
            .unwrap_or_else(|| local_key.to_string());
        let remote_value = read_remote_value(provider, issue, &remote_field);
        let existing_value = existing.and_then(|task| local_value_for_field(task, local_key));

        // When mapping tags from labels, filter out consumed label values
        let remote_value = if local_key == "tags"
            && matches!(remote_field.to_ascii_lowercase().as_str(), "labels")
            && !consumed_labels.is_empty()
        {
            remote_value.map(|v| filter_consumed_labels(v, &consumed_labels))
        } else {
            remote_value
        };

        let mapped =
            apply_mapping_for_pull(local_key, &detail, remote_value, existing_value.as_ref());
        if let Some(value) = mapped {
            apply_local_value(&mut out, local_key, value);
        }
    }
    out
}

fn filter_consumed_labels(value: FieldValue, consumed: &HashSet<String>) -> FieldValue {
    match value {
        FieldValue::List(values) => FieldValue::List(
            values
                .into_iter()
                .filter(|v| !consumed.contains(&v.to_ascii_lowercase()))
                .collect(),
        ),
        other => other,
    }
}

fn default_title_from_issue(provider: SyncProvider, issue: &JsonValue) -> Option<String> {
    match provider {
        SyncProvider::Jira => issue
            .get("fields")
            .and_then(|v| v.get("summary"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        SyncProvider::Github => issue
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

fn apply_local_value(out: &mut LocalFieldValues, key: &str, value: FieldValue) {
    match key {
        "title" => out.title = value.to_string_value(),
        "description" => out.description = field_value_to_string_allow_empty(&value),
        "status" => {
            if let Some(value) = value.to_string_value() {
                out.status = Some(TaskStatus::from(value));
            }
        }
        "task_type" | "type" => {
            if let Some(value) = value.to_string_value() {
                out.task_type = Some(TaskType::from(value));
            }
        }
        "priority" => {
            if let Some(value) = value.to_string_value() {
                out.priority = Some(Priority::from(value));
            }
        }
        "assignee" => out.assignee = field_value_to_string_allow_empty(&value),
        "reporter" => out.reporter = field_value_to_string_allow_empty(&value),
        "tags" => {
            out.tags = Some(dedupe_list(value.to_list_value()));
        }
        other => {
            let mut map = out.custom_fields.take().unwrap_or_default();
            if value.is_empty() {
                map.insert(other.to_string(), custom_value_null());
            } else {
                map.insert(other.to_string(), custom_value_from_field(value));
            }
            out.custom_fields = Some(map);
        }
    }
}

fn field_value_to_string_allow_empty(value: &FieldValue) -> Option<String> {
    match value {
        FieldValue::String(raw) => Some(raw.clone()),
        FieldValue::List(values) => values
            .iter()
            .find(|v| !v.trim().is_empty())
            .map(|v| v.trim().to_string()),
    }
}

fn normalize_mapping_detail(local_key: &str, mapping: &SyncFieldMapping) -> SyncFieldMappingDetail {
    match mapping {
        SyncFieldMapping::Simple(field) => SyncFieldMappingDetail {
            field: Some(field.clone()),
            ..Default::default()
        },
        SyncFieldMapping::Detailed(detail) => {
            let mut out = detail.clone();
            if out.field.is_none() {
                out.field = Some(local_key.to_string());
            }
            out
        }
    }
}

fn apply_mapping_for_pull(
    local_key: &str,
    detail: &SyncFieldMappingDetail,
    remote_value: Option<FieldValue>,
    existing_value: Option<&FieldValue>,
) -> Option<FieldValue> {
    let empty = remote_value.as_ref().map(|v| v.is_empty()).unwrap_or(true);
    if empty {
        return match detail.when_empty {
            Some(SyncWhenEmpty::Clear) => Some(clear_value_for_pull(local_key, &remote_value)),
            Some(SyncWhenEmpty::Skip) | None => None,
        };
    }

    let mut value = remote_value?;
    if !detail.values.is_empty() {
        value = map_remote_value_for_pull(local_key, detail, value, existing_value);
        if value.is_empty() {
            return None;
        }
    }
    Some(value)
}

fn clear_value_for_pull(local_key: &str, remote_value: &Option<FieldValue>) -> FieldValue {
    if local_key == "tags" || matches!(remote_value, Some(FieldValue::List(_))) {
        FieldValue::List(Vec::new())
    } else {
        FieldValue::String(String::new())
    }
}

fn map_remote_value_for_pull(
    local_key: &str,
    detail: &SyncFieldMappingDetail,
    value: FieldValue,
    existing_value: Option<&FieldValue>,
) -> FieldValue {
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
    for (local, remote) in &detail.values {
        reverse
            .entry(remote.to_ascii_lowercase())
            .or_default()
            .push(local.clone());
    }

    let existing_scalar = existing_value.and_then(|value| value.to_string_value());
    let pick_candidate = |candidates: &[String]| {
        if let Some(existing) = existing_scalar.as_ref() {
            let existing_norm = existing.to_ascii_lowercase();
            if let Some(found) = candidates
                .iter()
                .find(|candidate| candidate.to_ascii_lowercase() == existing_norm)
            {
                return found.clone();
            }
        }
        candidates.first().cloned().unwrap_or_default()
    };

    match value {
        FieldValue::String(raw) => {
            let lookup = raw.to_ascii_lowercase();
            if let Some(candidates) = reverse.get(&lookup) {
                FieldValue::String(pick_candidate(candidates))
            } else {
                FieldValue::String(raw)
            }
        }
        FieldValue::List(values) => {
            if is_scalar_local_field(local_key) {
                for raw in &values {
                    let lookup = raw.to_ascii_lowercase();
                    if let Some(candidates) = reverse.get(&lookup) {
                        return FieldValue::String(pick_candidate(candidates));
                    }
                }
                if let Some(existing) = existing_scalar {
                    FieldValue::String(existing)
                } else {
                    FieldValue::String(String::new())
                }
            } else {
                FieldValue::List(
                    values
                        .into_iter()
                        .map(|raw| {
                            let lookup = raw.to_ascii_lowercase();
                            reverse
                                .get(&lookup)
                                .and_then(|candidates| candidates.first().cloned())
                                .unwrap_or(raw)
                        })
                        .collect(),
                )
            }
        }
    }
}

fn is_scalar_local_field(local_key: &str) -> bool {
    matches!(
        local_key,
        "title"
            | "description"
            | "status"
            | "task_type"
            | "type"
            | "priority"
            | "assignee"
            | "reporter"
    )
}

fn build_jira_payload(
    remote: &SyncRemoteConfig,
    task: &TaskDTO,
    op: SyncOperation,
    client: Option<&SyncClient>,
    lookup: &mut JiraLookupCache,
    warnings: &mut Vec<String>,
) -> JiraIssuePayload {
    let mut payload = JiraIssuePayload::default();
    for (local_key, mapping) in &remote.mapping {
        let detail = normalize_mapping_detail(local_key, mapping);
        let remote_field = detail
            .field
            .clone()
            .unwrap_or_else(|| local_key.to_string());
        let value = local_value_for_field(task, local_key);
        let mapped = apply_mapping_for_push(remote.provider, &remote_field, value, &detail);
        let Some(value) = mapped else {
            continue;
        };
        if remote_field.eq_ignore_ascii_case("status") {
            payload.desired_status = value.to_string_value();
            continue;
        }
        apply_jira_field(
            &mut payload.fields,
            &remote_field,
            value,
            client,
            lookup,
            warnings,
        );
    }

    if matches!(op, SyncOperation::Create) {
        if !payload.fields.contains_key("summary") {
            payload
                .fields
                .insert("summary".to_string(), JsonValue::String(task.title.clone()));
        }
        if !payload.fields.contains_key("project")
            && let Some(project) = remote.project.as_deref()
        {
            payload
                .fields
                .insert("project".to_string(), json!({"key": project}));
        }
        if !payload.fields.contains_key("issuetype") {
            payload
                .fields
                .insert("issuetype".to_string(), json!({"name": "Task"}));
        }
    }

    payload
}

fn build_github_payload(
    remote: &SyncRemoteConfig,
    task: &TaskDTO,
    op: SyncOperation,
) -> GithubIssuePayload {
    let mut payload = GithubIssuePayload::default();
    for (local_key, mapping) in &remote.mapping {
        let detail = normalize_mapping_detail(local_key, mapping);
        let remote_field = detail
            .field
            .clone()
            .unwrap_or_else(|| local_key.to_string());
        let value = local_value_for_field(task, local_key);
        let mapped = apply_mapping_for_push(remote.provider, &remote_field, value, &detail);
        let Some(value) = mapped else {
            continue;
        };
        apply_github_field(&mut payload, &remote_field, value);
    }

    if matches!(op, SyncOperation::Create) && payload.title.is_none() {
        payload.title = Some(task.title.clone());
    }

    payload.labels = dedupe_list(payload.labels);
    payload.assignees = dedupe_list(payload.assignees);
    payload
}

fn apply_mapping_for_push(
    provider: SyncProvider,
    remote_field: &str,
    local_value: Option<FieldValue>,
    detail: &SyncFieldMappingDetail,
) -> Option<FieldValue> {
    if let Some(set) = detail.set.as_ref() {
        return Some(FieldValue::String(set.clone()));
    }

    let mut value = local_value;
    let is_list = field_expects_list(provider, remote_field);

    if value.as_ref().map(|v| v.is_empty()).unwrap_or(true)
        && let Some(default) = detail.default.as_ref()
    {
        value = Some(FieldValue::String(default.clone()));
    }

    let empty = value.as_ref().map(|v| v.is_empty()).unwrap_or(true);
    if empty {
        return match detail.when_empty {
            Some(SyncWhenEmpty::Clear) => Some(if is_list {
                FieldValue::List(Vec::new())
            } else {
                FieldValue::String(String::new())
            }),
            Some(SyncWhenEmpty::Skip) | None => None,
        };
    }

    let mut value = value?;
    if !detail.values.is_empty() {
        value = map_local_value(detail, value);
    }
    if is_list && !detail.add.is_empty() {
        let mut list = value.to_list_value();
        list.extend(detail.add.iter().cloned());
        value = FieldValue::List(list);
    }
    Some(value)
}

fn map_local_value(detail: &SyncFieldMappingDetail, value: FieldValue) -> FieldValue {
    match value {
        FieldValue::String(raw) => {
            FieldValue::String(lookup_mapping_value(&detail.values, &raw).unwrap_or(raw))
        }
        FieldValue::List(values) => FieldValue::List(
            values
                .into_iter()
                .map(|raw| lookup_mapping_value(&detail.values, &raw).unwrap_or(raw))
                .collect(),
        ),
    }
}

fn lookup_mapping_value(map: &HashMap<String, String>, value: &str) -> Option<String> {
    if let Some(found) = map.get(value) {
        return Some(found.clone());
    }
    let target = value.to_ascii_lowercase();
    map.iter()
        .find(|(key, _)| key.to_ascii_lowercase() == target)
        .map(|(_, val)| val.clone())
}

fn field_expects_list(provider: SyncProvider, remote_field: &str) -> bool {
    let name = remote_field.to_ascii_lowercase();
    match provider {
        SyncProvider::Jira => matches!(name.as_str(), "labels"),
        SyncProvider::Github => matches!(name.as_str(), "labels" | "assignees"),
    }
}

fn apply_jira_field(
    fields: &mut JsonMap<String, JsonValue>,
    field: &str,
    value: FieldValue,
    client: Option<&SyncClient>,
    lookup: &mut JiraLookupCache,
    warnings: &mut Vec<String>,
) {
    let field_lower = field.to_ascii_lowercase();
    match field_lower.as_str() {
        "summary" => {
            if let Some(text) = value.to_string_value() {
                fields.insert(field.to_string(), JsonValue::String(text));
            }
        }
        "description" => {
            let text = value.to_string_value().unwrap_or_default();
            fields.insert(field.to_string(), jira_text_to_adf(&text));
        }
        "issuetype" => {
            if let Some(text) = value.to_string_value() {
                let resolved = resolve_jira_issue_type(&text, lookup, warnings);
                fields.insert(field.to_string(), json!({"name": resolved}));
            }
        }
        "priority" => {
            if let Some(text) = value.to_string_value() {
                fields.insert(field.to_string(), json!({"name": text}));
            }
        }
        "assignee" | "reporter" => {
            if let Some(text) = value.to_string_value() {
                if let Some(payload) = jira_user_value(&text, field, client, lookup, warnings) {
                    fields.insert(field.to_string(), payload);
                }
            } else {
                fields.insert(field.to_string(), JsonValue::Null);
            }
        }
        "labels" => {
            let mut list = value.to_list_value();
            list = dedupe_list(list);
            let next = JsonValue::Array(list.into_iter().map(JsonValue::String).collect());
            if let Some(existing) = fields.get(field).cloned() {
                let mut merged = match existing {
                    JsonValue::Array(items) => items
                        .iter()
                        .filter_map(|item| item.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                if let JsonValue::Array(next_items) = next {
                    merged.extend(
                        next_items
                            .into_iter()
                            .filter_map(|item| item.as_str().map(|s| s.to_string())),
                    );
                }
                fields.insert(
                    field.to_string(),
                    JsonValue::Array(
                        dedupe_list(merged)
                            .into_iter()
                            .map(JsonValue::String)
                            .collect(),
                    ),
                );
            } else {
                fields.insert(field.to_string(), next);
            }
        }
        _ => {
            let json_value = match value {
                FieldValue::String(text) => JsonValue::String(text),
                FieldValue::List(values) => {
                    JsonValue::Array(values.into_iter().map(JsonValue::String).collect())
                }
            };
            fields.insert(field.to_string(), json_value);
        }
    }
}

fn apply_github_field(payload: &mut GithubIssuePayload, field: &str, value: FieldValue) {
    let field_lower = field.to_ascii_lowercase();
    match field_lower.as_str() {
        "title" => payload.title = value.to_string_value(),
        "body" | "description" => payload.body = value.to_string_value(),
        "state" | "status" => payload.state = value.to_string_value(),
        "labels" => {
            payload.labels_explicit = true;
            payload.labels.extend(value.to_list_value());
        }
        "assignees" => {
            payload.assignees_explicit = true;
            payload.assignees.extend(value.to_list_value());
        }
        _ => {}
    }
}

fn local_value_for_field(task: &TaskDTO, field: &str) -> Option<FieldValue> {
    match field {
        "title" => Some(FieldValue::String(task.title.clone())),
        "description" => task
            .description
            .as_ref()
            .map(|value| FieldValue::String(value.clone())),
        "status" => Some(FieldValue::String(task.status.to_string())),
        "task_type" | "type" => Some(FieldValue::String(task.task_type.to_string())),
        "priority" => Some(FieldValue::String(task.priority.to_string())),
        "assignee" => task
            .assignee
            .as_ref()
            .map(|value| FieldValue::String(value.clone())),
        "reporter" => task
            .reporter
            .as_ref()
            .map(|value| FieldValue::String(value.clone())),
        "tags" => Some(FieldValue::List(task.tags.clone())),
        other => task
            .custom_fields
            .get(other)
            .and_then(custom_field_value_to_field_value),
    }
}

fn read_remote_value(provider: SyncProvider, issue: &JsonValue, field: &str) -> Option<FieldValue> {
    match provider {
        SyncProvider::Jira => read_jira_field(issue, field),
        SyncProvider::Github => read_github_field(issue, field),
    }
}

fn format_field_list(fields: &[String]) -> Option<String> {
    let mut values = fields
        .iter()
        .map(|field| field.trim().to_string())
        .filter(|field| !field.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
    }
}

fn jira_pull_field_names(remote: &SyncRemoteConfig) -> Vec<String> {
    let mut fields = vec![
        "summary".to_string(),
        "description".to_string(),
        "status".to_string(),
        "issuetype".to_string(),
        "priority".to_string(),
        "assignee".to_string(),
        "reporter".to_string(),
        "labels".to_string(),
    ];
    for (local_key, mapping) in &remote.mapping {
        let detail = normalize_mapping_detail(local_key, mapping);
        let remote_field = detail
            .field
            .clone()
            .unwrap_or_else(|| local_key.to_string());
        fields.push(remote_field);
    }
    fields.sort();
    fields.dedup();
    fields
}

fn jira_payload_field_value(field: &str, value: &JsonValue) -> Option<FieldValue> {
    let field_lower = field.to_ascii_lowercase();
    match field_lower.as_str() {
        "summary" => value
            .as_str()
            .map(|text| FieldValue::String(text.to_string())),
        "description" => match value {
            JsonValue::String(text) => Some(FieldValue::String(text.clone())),
            _ => Some(FieldValue::String(jira_adf_to_text(value))),
        },
        "issuetype" | "priority" => value
            .get("name")
            .and_then(|v| v.as_str())
            .map(|text| FieldValue::String(text.to_string())),
        "assignee" | "reporter" => value
            .get("accountId")
            .or_else(|| value.get("name"))
            .or_else(|| value.get("emailAddress"))
            .or_else(|| value.get("displayName"))
            .and_then(|v| v.as_str())
            .map(|text| FieldValue::String(text.to_string())),
        "labels" => json_array_to_list(value),
        _ => read_generic_field(Some(value)),
    }
}

fn jira_payload_field_names(payload: &JiraIssuePayload) -> Vec<String> {
    let mut names = payload.fields.keys().cloned().collect::<Vec<_>>();
    if payload.desired_status.is_some() {
        names.push("status".to_string());
    }
    names
}

fn github_payload_field_names(payload: &GithubIssuePayload) -> Vec<String> {
    let mut names = Vec::new();
    if payload.title.is_some() {
        names.push("title".to_string());
    }
    if payload.body.is_some() {
        names.push("body".to_string());
    }
    if payload.state.is_some() {
        names.push("state".to_string());
    }
    if payload.labels_explicit || !payload.labels.is_empty() {
        names.push("labels".to_string());
    }
    if payload.assignees_explicit || !payload.assignees.is_empty() {
        names.push("assignees".to_string());
    }
    names
}

fn filter_jira_payload_against_issue(
    issue: &JsonValue,
    payload: &mut JiraIssuePayload,
) -> Vec<String> {
    let mut changed = Vec::new();
    let mut next = JsonMap::new();
    for (field, value) in &payload.fields {
        let desired = jira_payload_field_value(field, value);
        let current = read_jira_field(issue, field);
        let matches = match (desired.as_ref(), current.as_ref()) {
            (Some(desired), Some(current)) => field_values_match(desired, current),
            _ => false,
        };
        if !matches {
            changed.push(field.clone());
            next.insert(field.clone(), value.clone());
        }
    }
    payload.fields = next;

    if let Some(status) = payload.desired_status.clone() {
        let desired = FieldValue::String(status.clone());
        let matches = read_jira_field(issue, "status")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.desired_status = None;
        } else {
            changed.push("status".to_string());
        }
    }

    changed
}

fn filter_github_payload_against_issue(
    issue: &JsonValue,
    payload: &mut GithubIssuePayload,
) -> Vec<String> {
    let mut changed = Vec::new();

    if let Some(title) = payload.title.clone() {
        let desired = FieldValue::String(title);
        let matches = read_github_field(issue, "title")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.title = None;
        } else {
            changed.push("title".to_string());
        }
    }

    if let Some(body) = payload.body.clone() {
        let desired = FieldValue::String(body);
        let matches = read_github_field(issue, "body")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.body = None;
        } else {
            changed.push("body".to_string());
        }
    }

    if let Some(state) = payload.state.clone() {
        let desired = FieldValue::String(state);
        let matches = read_github_field(issue, "state")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.state = None;
        } else {
            changed.push("state".to_string());
        }
    }

    if payload.labels_explicit || !payload.labels.is_empty() {
        let desired = FieldValue::List(dedupe_list(payload.labels.clone()));
        let matches = read_github_field(issue, "labels")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.labels.clear();
            payload.labels_explicit = false;
        } else {
            changed.push("labels".to_string());
        }
    }

    if payload.assignees_explicit || !payload.assignees.is_empty() {
        let desired = FieldValue::List(dedupe_list(payload.assignees.clone()));
        let matches = read_github_field(issue, "assignees")
            .as_ref()
            .map(|current| field_values_match(&desired, current))
            .unwrap_or(false);
        if matches {
            payload.assignees.clear();
            payload.assignees_explicit = false;
        } else {
            changed.push("assignees".to_string());
        }
    }

    changed
}

fn read_jira_field(issue: &JsonValue, field: &str) -> Option<FieldValue> {
    let fields = issue.get("fields")?;
    let field_lower = field.to_ascii_lowercase();
    match field_lower.as_str() {
        "summary" => fields
            .get("summary")
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "description" => {
            let value = fields.get("description")?;
            let text = match value {
                JsonValue::String(s) => s.clone(),
                JsonValue::Object(_) => jira_adf_to_text(value),
                _ => String::new(),
            };
            Some(FieldValue::String(text))
        }
        "status" => fields
            .get("status")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "issuetype" => fields
            .get("issuetype")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "priority" => fields
            .get("priority")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "assignee" => jira_user_to_string(fields.get("assignee")),
        "reporter" => jira_user_to_string(fields.get("reporter")),
        "labels" => fields.get("labels").and_then(json_array_to_list),
        _ => read_generic_field(fields.get(field)),
    }
}

fn jira_user_to_string(value: Option<&JsonValue>) -> Option<FieldValue> {
    let user = value?;
    let value = user
        .get("emailAddress")
        .and_then(|v| v.as_str())
        .or_else(|| user.get("displayName").and_then(|v| v.as_str()))
        .or_else(|| user.get("accountId").and_then(|v| v.as_str()))
        .map(|v| v.to_string())?;
    Some(FieldValue::String(value))
}

fn read_github_field(issue: &JsonValue, field: &str) -> Option<FieldValue> {
    let field_lower = field.to_ascii_lowercase();
    match field_lower.as_str() {
        "title" => issue
            .get("title")
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "body" | "description" => issue
            .get("body")
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "state" | "status" => issue
            .get("state")
            .and_then(|v| v.as_str())
            .map(|v| FieldValue::String(v.to_string())),
        "labels" => issue.get("labels").and_then(github_labels_to_list),
        "assignees" => issue.get("assignees").and_then(github_assignees_to_list),
        _ => read_generic_field(issue.get(field)),
    }
}

fn read_generic_field(value: Option<&JsonValue>) -> Option<FieldValue> {
    match value? {
        JsonValue::String(s) => Some(FieldValue::String(s.clone())),
        JsonValue::Number(n) => Some(FieldValue::String(n.to_string())),
        JsonValue::Bool(b) => Some(FieldValue::String(b.to_string())),
        JsonValue::Array(values) => Some(FieldValue::List(
            values
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
        )),
        _ => None,
    }
}

fn github_labels_to_list(value: &JsonValue) -> Option<FieldValue> {
    let list = value.as_array()?;
    let mut out = Vec::new();
    for item in list {
        if let Some(name) = item.as_str() {
            out.push(name.to_string());
        } else if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
            out.push(name.to_string());
        }
    }
    Some(FieldValue::List(out))
}

fn github_assignees_to_list(value: &JsonValue) -> Option<FieldValue> {
    let list = value.as_array()?;
    let mut out = Vec::new();
    for item in list {
        if let Some(login) = item.get("login").and_then(|v| v.as_str()) {
            out.push(login.to_string());
        }
    }
    Some(FieldValue::List(out))
}

fn json_array_to_list(value: &JsonValue) -> Option<FieldValue> {
    let list = value.as_array()?;
    let mut out = Vec::new();
    for item in list {
        if let Some(text) = item.as_str() {
            out.push(text.to_string());
        }
    }
    Some(FieldValue::List(out))
}

fn jira_adf_to_text(value: &JsonValue) -> String {
    let mut parts = Vec::new();
    if let Some(content) = value.get("content").and_then(|v| v.as_array()) {
        for node in content {
            let mut buffer = String::new();
            collect_adf_text(node, &mut buffer);
            let trimmed = buffer.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
    }
    parts.join("\n")
}

fn collect_adf_text(value: &JsonValue, out: &mut String) {
    match value {
        JsonValue::Object(map) => {
            if let Some(JsonValue::String(text)) = map.get("text") {
                out.push_str(text);
            }
            if let Some(JsonValue::Array(content)) = map.get("content") {
                for node in content {
                    collect_adf_text(node, out);
                }
            }
        }
        JsonValue::Array(items) => {
            for item in items {
                collect_adf_text(item, out);
            }
        }
        _ => {}
    }
}

fn jira_text_to_adf(text: &str) -> JsonValue {
    if text.trim().is_empty() {
        JsonValue::Null
    } else {
        json!({
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {"type": "text", "text": text}
                    ]
                }
            ]
        })
    }
}

fn jira_user_value(
    value: &str,
    field: &str,
    client: Option<&SyncClient>,
    lookup: &mut JiraLookupCache,
    warnings: &mut Vec<String>,
) -> Option<JsonValue> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(account_id) = normalize_jira_account_id(trimmed) {
        return Some(json!({"accountId": account_id}));
    }

    if let Some(account_id) = lookup_jira_account_id(trimmed, client, lookup, warnings) {
        return Some(json!({"accountId": account_id}));
    }

    warnings.push(format!(
        "Skipping Jira {} assignment; value '{}' is not an accountId",
        field, trimmed
    ));
    None
}

fn lookup_jira_account_id(
    value: &str,
    client: Option<&SyncClient>,
    lookup: &mut JiraLookupCache,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let key = trimmed.to_ascii_lowercase();
    if let Some(account_id) = lookup.user_cache.get(&key) {
        return Some(account_id.clone());
    }

    let client = client?;
    match jira_search_account_id(client, trimmed) {
        Ok(Some(account_id)) => {
            lookup.user_cache.insert(key, account_id.clone());
            Some(account_id)
        }
        Ok(None) => None,
        Err(err) => {
            warnings.push(format!(
                "Failed to resolve Jira user '{}': {}",
                trimmed, err
            ));
            None
        }
    }
}

fn jira_search_account_id(client: &SyncClient, query: &str) -> LoTaRResult<Option<String>> {
    let url = format!("{}/rest/api/3/user/search", client.auth.api_base);
    let req = client
        .agent
        .get(&url)
        .query("query", query)
        .query("maxResults", "1");
    let payload = send_json_request(client, req, None)?;
    let entries = payload.as_array().cloned().unwrap_or_default();
    Ok(entries
        .iter()
        .find_map(|entry| entry.get("accountId").and_then(|v| v.as_str()))
        .map(|value| value.to_string()))
}

fn normalize_jira_account_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = trimmed.strip_prefix("accountId:").unwrap_or(trimmed).trim();
    if candidate.len() < 8 {
        return None;
    }
    if candidate
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':')
    {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn resolve_jira_issue_type(
    value: &str,
    lookup: &JiraLookupCache,
    warnings: &mut Vec<String>,
) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }
    let Some(types) = lookup.issue_types.as_ref() else {
        return trimmed.to_string();
    };
    if let Some(found) = types
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(trimmed))
    {
        return found.clone();
    }

    let fallback = types
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case("Task"))
        .cloned()
        .or_else(|| types.first().cloned());
    if let Some(fallback) = fallback {
        warnings.push(format!(
            "Jira issue type '{}' not found; falling back to '{}'",
            trimmed, fallback
        ));
        return fallback;
    }

    trimmed.to_string()
}

fn ensure_jira_issue_types(
    lookup: &mut JiraLookupCache,
    client: Option<&SyncClient>,
    remote: &SyncRemoteConfig,
    warnings: &mut Vec<String>,
) {
    if lookup.issue_types.is_some() {
        return;
    }
    let Some(client) = client else {
        return;
    };
    let Some(project) = remote
        .project
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };

    match jira_fetch_issue_types(client, project) {
        Ok(types) => lookup.issue_types = Some(types),
        Err(err) => warnings.push(format!(
            "Failed to load Jira issue types for '{}': {}",
            project, err
        )),
    }
}

fn jira_fetch_issue_types(client: &SyncClient, project: &str) -> LoTaRResult<Vec<String>> {
    let project_id = jira_resolve_project_id(client, project)?;
    let url = format!("{}/rest/api/3/issuetype/project", client.auth.api_base);
    let req = client.agent.get(&url).query("projectId", &project_id);
    let payload = send_json_request(client, req, None)?;
    let entries = payload.as_array().cloned().unwrap_or_default();
    let mut types: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry.get("name").and_then(|v| v.as_str()))
        .map(|value| value.to_string())
        .collect();
    types.sort();
    types.dedup();
    if types.is_empty() {
        return Err(LoTaRError::ValidationError(format!(
            "No issue types returned for Jira project '{}'",
            project
        )));
    }
    Ok(types)
}

fn jira_resolve_project_id(client: &SyncClient, project: &str) -> LoTaRResult<String> {
    let trimmed = project.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError(
            "Jira project key is required".to_string(),
        ));
    }
    let url = format!("{}/rest/api/3/project/{}", client.auth.api_base, trimmed);
    let req = client.agent.get(&url);
    let payload = send_json_request(client, req, None)?;
    payload
        .get("id")
        .and_then(|v| v.as_str())
        .map(|value| value.to_string())
        .ok_or_else(|| {
            LoTaRError::SerializationError(format!("Missing Jira project id for '{}'", trimmed))
        })
}

fn dedupe_list(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(trimmed.to_string());
        }
    }
    out
}

fn merge_custom_fields(existing: Option<&CustomFields>, updates: CustomFields) -> CustomFields {
    let mut merged = existing.cloned().unwrap_or_default();
    for (key, value) in updates {
        if custom_field_value_is_null(&value) {
            merged.remove(&key);
        } else {
            merged.insert(key, value);
        }
    }
    merged
}

fn custom_field_value_is_null(value: &CustomFieldValue) -> bool {
    #[cfg(feature = "schema")]
    {
        matches!(value, serde_json::Value::Null)
    }
    #[cfg(not(feature = "schema"))]
    {
        matches!(value, serde_yaml::Value::Null)
    }
}

fn task_update_is_empty(update: &TaskUpdate) -> bool {
    update.title.is_none()
        && update.status.is_none()
        && update.priority.is_none()
        && update.task_type.is_none()
        && update.reporter.is_none()
        && update.assignee.is_none()
        && update.due_date.is_none()
        && update.effort.is_none()
        && update.description.is_none()
        && update.tags.is_none()
        && update.relationships.is_none()
        && update.custom_fields.is_none()
        && update.sprints.is_none()
}

fn task_update_field_names(update: &TaskUpdate) -> Vec<String> {
    let mut fields = Vec::new();
    if update.title.is_some() {
        fields.push("title".to_string());
    }
    if update.description.is_some() {
        fields.push("description".to_string());
    }
    if update.status.is_some() {
        fields.push("status".to_string());
    }
    if update.task_type.is_some() {
        fields.push("type".to_string());
    }
    if update.priority.is_some() {
        fields.push("priority".to_string());
    }
    if update.assignee.is_some() {
        fields.push("assignee".to_string());
    }
    if update.reporter.is_some() {
        fields.push("reporter".to_string());
    }
    if update.due_date.is_some() {
        fields.push("due_date".to_string());
    }
    if update.effort.is_some() {
        fields.push("effort".to_string());
    }
    if update.tags.is_some() {
        fields.push("tags".to_string());
    }
    if update.relationships.is_some() {
        fields.push("relationships".to_string());
    }
    if update.custom_fields.is_some() {
        fields.push("custom_fields".to_string());
    }
    if update.sprints.is_some() {
        fields.push("sprints".to_string());
    }
    fields
}

fn custom_field_value_to_field_value(value: &CustomFieldValue) -> Option<FieldValue> {
    #[cfg(feature = "schema")]
    {
        match value {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) => Some(FieldValue::String(s.clone())),
            serde_json::Value::Number(n) => Some(FieldValue::String(n.to_string())),
            serde_json::Value::Bool(b) => Some(FieldValue::String(b.to_string())),
            serde_json::Value::Array(items) => {
                let list = items
                    .iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>();
                Some(FieldValue::List(list))
            }
            _ => None,
        }
    }
    #[cfg(not(feature = "schema"))]
    {
        match value {
            serde_yaml::Value::Null => None,
            serde_yaml::Value::String(s) => Some(FieldValue::String(s.clone())),
            serde_yaml::Value::Number(n) => Some(FieldValue::String(n.to_string())),
            serde_yaml::Value::Bool(b) => Some(FieldValue::String(b.to_string())),
            serde_yaml::Value::Sequence(items) => {
                let list = items
                    .iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>();
                Some(FieldValue::List(list))
            }
            _ => None,
        }
    }
}

fn custom_value_from_field(value: FieldValue) -> CustomFieldValue {
    match value {
        FieldValue::String(text) => crate::types::custom_value_string(text),
        FieldValue::List(values) => custom_value_list(values),
    }
}

fn custom_value_list(values: Vec<String>) -> CustomFieldValue {
    #[cfg(feature = "schema")]
    {
        serde_json::Value::Array(values.into_iter().map(serde_json::Value::String).collect())
    }
    #[cfg(not(feature = "schema"))]
    {
        serde_yaml::Value::Sequence(values.into_iter().map(serde_yaml::Value::String).collect())
    }
}

fn custom_value_null() -> CustomFieldValue {
    #[cfg(feature = "schema")]
    {
        serde_json::Value::Null
    }
    #[cfg(not(feature = "schema"))]
    {
        serde_yaml::Value::Null
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
mod sync_mapping_tests {
    use super::*;
    use crate::types::ReferenceEntry;
    use serde_json::json;
    use std::collections::HashMap;

    fn sample_task() -> TaskDTO {
        TaskDTO {
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
            sprints: vec![],
            sprint_order: Default::default(),
            history: vec![],
            custom_fields: Default::default(),
        }
    }

    #[test]
    fn jira_query_for_remote_prefers_filter() {
        let remote = SyncRemoteConfig {
            provider: SyncProvider::Jira,
            project: Some("DEMO".to_string()),
            repo: None,
            filter: Some("project = HELLO".to_string()),
            auth_profile: None,
            mapping: HashMap::new(),
        };
        let jql = jira_query_for_remote(&remote).expect("jql should build");
        assert_eq!(jql, "project = HELLO");
    }

    #[test]
    fn jira_query_for_remote_uses_project_when_no_filter() {
        let remote = SyncRemoteConfig {
            provider: SyncProvider::Jira,
            project: Some("DEMO".to_string()),
            repo: None,
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        let jql = jira_query_for_remote(&remote).expect("jql should build");
        assert_eq!(jql, "project = DEMO");
    }

    #[test]
    fn github_query_adds_state_when_missing() {
        let query = github_build_search_query("owner/repo", "label:bug");
        assert!(query.contains("repo:owner/repo"));
        assert!(query.contains("state:all"));
        assert!(query.contains("label:bug"));
    }

    #[test]
    fn github_label_mapping_prefers_matching_scalar_values() {
        let mut remote = SyncRemoteConfig {
            provider: SyncProvider::Github,
            project: None,
            repo: Some("org/repo".to_string()),
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        remote.mapping.insert(
            "task_type".to_string(),
            SyncFieldMapping::Detailed(SyncFieldMappingDetail {
                field: Some("labels".to_string()),
                values: HashMap::from([
                    (String::from("Feature"), String::from("type:feature")),
                    (String::from("Bug"), String::from("type:bug")),
                ]),
                ..Default::default()
            }),
        );
        remote.mapping.insert(
            "priority".to_string(),
            SyncFieldMapping::Detailed(SyncFieldMappingDetail {
                field: Some("labels".to_string()),
                values: HashMap::from([(String::from("Medium"), String::from("priority:medium"))]),
                ..Default::default()
            }),
        );

        let issue = json!({
            "labels": [
                {"name": "priority:medium"},
                {"name": "type:feature"}
            ]
        });

        let update = build_task_update_from_issue(SyncProvider::Github, &remote, &issue, None);
        assert_eq!(update.task_type.unwrap().to_string(), "Feature");
        assert_eq!(update.priority.unwrap().to_string(), "Medium");
    }

    #[test]
    fn github_status_mapping_prefers_existing_value_on_collisions() {
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
                    (String::from("InProgress"), String::from("open")),
                ]),
                ..Default::default()
            }),
        );

        let issue = json!({"state": "open"});
        let mut existing = sample_task();
        existing.status = TaskStatus::from("Todo");

        let update =
            build_task_update_from_issue(SyncProvider::Github, &remote, &issue, Some(&existing));
        assert!(update.status.is_none());
    }

    #[test]
    fn determine_reference_state_skips_other_provider_refs() {
        let remote = SyncRemoteConfig {
            provider: SyncProvider::Github,
            project: None,
            repo: Some("org/repo".to_string()),
            filter: None,
            auth_profile: None,
            mapping: HashMap::new(),
        };
        let mut task = sample_task();
        task.references.push(ReferenceEntry {
            jira: Some("LTS-1".to_string()),
            ..Default::default()
        });

        match determine_reference_state(&remote, &task) {
            ReferenceState::ProviderOnly(value) => assert_eq!(value, "LTS-1"),
            other => panic!("Expected ProviderOnly, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::custom_value_string;

    fn sample_task() -> TaskDTO {
        TaskDTO {
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
            sprints: vec![],
            sprint_order: Default::default(),
            history: vec![],
            custom_fields: Default::default(),
        }
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
