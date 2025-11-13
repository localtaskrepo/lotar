use std::path::PathBuf;

use chrono::Utc;
use serde::Serialize;

use crate::config::types::ResolvedConfig;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_analytics::{SprintDetail, SprintSummary};
use crate::services::sprint_service::{SprintRecord, SprintService};
use crate::services::sprint_status;
use crate::storage::manager::Storage;
use crate::storage::sprint::SprintCanonicalizationWarning;

pub(crate) struct SprintRecordsContext {
    pub(crate) storage: Storage,
    pub(crate) resolved_config: ResolvedConfig,
    pub(crate) records: Vec<SprintRecord>,
}

pub(crate) fn resolve_sprint_records_context(
    tasks_root: PathBuf,
    action_label: &str,
) -> Result<SprintRecordsContext, String> {
    let missing_message = format!("No sprints found. Create one before {}.", action_label);

    let storage = Storage::try_open(tasks_root.clone()).ok_or_else(|| missing_message.clone())?;

    let resolved_config =
        crate::config::resolution::load_and_merge_configs(Some(tasks_root.as_path()))
            .map_err(|err| err.to_string())?;

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    if records.is_empty() {
        return Err(missing_message);
    }

    Ok(SprintRecordsContext {
        storage,
        resolved_config,
        records,
    })
}

#[derive(Copy, Clone, Debug)]
pub(super) enum SprintOperationKind {
    Create,
    Update,
    Start,
    Close,
}

pub(super) fn render_operation_response(
    kind: SprintOperationKind,
    record: SprintRecord,
    warnings: Vec<SprintCanonicalizationWarning>,
    applied_defaults: Vec<String>,
    renderer: &OutputRenderer,
    warnings_enabled: bool,
    _resolved_config: &ResolvedConfig,
) {
    let lifecycle = sprint_status::derive_status(&record.sprint, Utc::now());
    let summary = SprintSummary::from_record(&record, &lifecycle);
    let detail = SprintDetail::from_record(&record, &summary, &lifecycle);
    let actual_started_at = record
        .sprint
        .actual
        .as_ref()
        .and_then(|actual| actual.started_at.clone());
    let mut canonical_warnings = if warnings_enabled {
        warnings
    } else {
        Vec::new()
    };

    match renderer.format {
        OutputFormat::Json => {
            let payload = SprintOperationResponse {
                status: "ok",
                action: match kind {
                    SprintOperationKind::Create => "created",
                    SprintOperationKind::Update => "updated",
                    SprintOperationKind::Start => "started",
                    SprintOperationKind::Close => "closed",
                },
                sprint: detail,
                applied_defaults: applied_defaults.clone(),
                warnings: canonical_warnings
                    .iter()
                    .map(|warning| SprintWarningPayload {
                        code: warning.code(),
                        message: warning.message(),
                    })
                    .collect(),
            };
            renderer.emit_json(&payload);
        }
        _ => {
            let verb = match kind {
                SprintOperationKind::Create => "Created",
                SprintOperationKind::Update => "Updated",
                SprintOperationKind::Start => "Started",
                SprintOperationKind::Close => "Closed",
            };

            renderer.emit_success(&format!(
                "{} sprint #{}{}.",
                verb,
                record.id,
                summary
                    .label
                    .as_ref()
                    .map(|label| format!(" ({})", label))
                    .unwrap_or_default()
            ));

            if !applied_defaults.is_empty() {
                let defaults_list = applied_defaults.join(", ");
                renderer.emit_info(&format!("Applied sprint defaults: {}.", defaults_list));
            }

            if let Some(computed_end) = detail.computed_end.as_ref() {
                renderer.emit_info(&format!("Computed end: {}", computed_end));
            }

            if !canonical_warnings.is_empty() {
                for warning in canonical_warnings.drain(..) {
                    renderer.emit_warning(warning.message());
                }
            }

            if warnings_enabled && !lifecycle.warnings.is_empty() {
                for warning in &lifecycle.warnings {
                    renderer.emit_warning(&warning.message());
                }
            }

            if matches!(kind, SprintOperationKind::Start) {
                if let Some(started_at) = actual_started_at {
                    renderer.emit_info(&format!("Started at: {}", started_at));
                }
            }
            if matches!(kind, SprintOperationKind::Close) {
                if let Some(closed_at) = record
                    .sprint
                    .actual
                    .as_ref()
                    .and_then(|actual| actual.closed_at.clone())
                {
                    renderer.emit_info(&format!("Closed at: {}", closed_at));
                }
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct SprintOperationResponse {
    status: &'static str,
    action: &'static str,
    sprint: SprintDetail,
    applied_defaults: Vec<String>,
    warnings: Vec<SprintWarningPayload>,
}

#[derive(Debug, Serialize)]
struct SprintWarningPayload {
    code: &'static str,
    message: &'static str,
}
