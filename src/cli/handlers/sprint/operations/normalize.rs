use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::cli::args::sprint::SprintNormalizeArgs;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sprint_service::SprintService;
use crate::storage::manager::Storage;
use crate::storage::sprint::Sprint;
use serde::Serialize;

pub(crate) fn handle_normalize(
    normalize_args: SprintNormalizeArgs,
    tasks_root: PathBuf,
    renderer: &OutputRenderer,
) -> Result<(), String> {
    let mode = resolve_normalize_mode(&normalize_args)?;
    let is_json = matches!(renderer.format, OutputFormat::Json);

    let storage = match Storage::try_open(tasks_root.clone()) {
        Some(storage) => storage,
        None => {
            if is_json {
                let payload = SprintNormalizeResponse {
                    status: "ok",
                    mode: mode.as_label(),
                    processed: 0,
                    updated: 0,
                    changes_required: Vec::new(),
                    updated_ids: Vec::new(),
                    warnings: Vec::new(),
                    skipped: Vec::new(),
                };
                renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
            } else {
                renderer.emit_info("No sprints found. Nothing to normalize.");
            }
            return Ok(());
        }
    };

    let records = SprintService::list(&storage).map_err(|err| err.to_string())?;
    if records.is_empty() {
        if is_json {
            let payload = SprintNormalizeResponse {
                status: "ok",
                mode: mode.as_label(),
                processed: 0,
                updated: 0,
                changes_required: Vec::new(),
                updated_ids: Vec::new(),
                warnings: Vec::new(),
                skipped: Vec::new(),
            };
            renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
        } else {
            renderer.emit_info("No sprints found. Nothing to normalize.");
        }
        return Ok(());
    }

    let available: HashSet<u32> = records.iter().map(|record| record.id).collect();
    let target_ids: Vec<u32> = if let Some(requested) = normalize_args.sprint_id {
        if !available.contains(&requested) {
            return Err(format!("Sprint #{} not found.", requested));
        }
        vec![requested]
    } else {
        records.iter().map(|record| record.id).collect()
    };

    let mut processed = 0usize;
    let mut updated = 0usize;
    let mut pending_ids = Vec::new();
    let mut updated_ids = Vec::new();
    let mut warning_payloads = Vec::new();
    let mut skipped = Vec::new();

    for sprint_id in target_ids {
        let path = Sprint::path_for_id(&storage.root_path, sprint_id);
        if !path.exists() {
            let message = format!(
                "Skipping sprint #{}; {} is missing.",
                sprint_id,
                path.display()
            );
            if is_json {
                skipped.push(SprintNormalizeSkipPayload {
                    sprint_id,
                    reason: message,
                });
            } else {
                renderer.emit_warning(&message);
            }
            continue;
        }

        let original = fs::read_to_string(&path)
            .map_err(|err| format!("Failed to read {}: {}", path.display(), err))?;
        let mut sprint: Sprint = serde_yaml::from_str(&original)
            .map_err(|err| format!("Failed to parse sprint #{}: {}", sprint_id, err))?;

        let warnings = sprint.canonicalize();
        let canonical = sprint
            .to_yaml()
            .map_err(|err| format!("Failed to serialize sprint #{}: {}", sprint_id, err))?;
        let dirty = canonical != original;

        match mode {
            SprintNormalizeMode::Write => {
                if dirty {
                    fs::write(&path, canonical.as_bytes())
                        .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
                    updated += 1;
                    updated_ids.push(sprint_id);
                    if !is_json {
                        renderer.emit_success(&format!("Normalized sprint #{}.", sprint_id));
                    }
                } else if !is_json {
                    renderer.emit_info(&format!("Sprint #{} already canonical.", sprint_id));
                }
            }
            SprintNormalizeMode::Check => {
                if dirty {
                    pending_ids.push(sprint_id);
                    if !is_json {
                        renderer.emit_warning(&format!(
                            "Sprint #{} requires normalization (run with --write to update).",
                            sprint_id
                        ));
                    }
                } else if !is_json {
                    renderer.emit_info(&format!("Sprint #{} already canonical.", sprint_id));
                }
            }
        }

        if !warnings.is_empty() {
            if is_json {
                for warning in &warnings {
                    warning_payloads.push(SprintNormalizeWarningPayload {
                        sprint_id,
                        code: warning.code(),
                        message: warning.message(),
                    });
                }
            } else {
                let emit = match mode {
                    SprintNormalizeMode::Write => OutputRenderer::emit_info,
                    SprintNormalizeMode::Check => OutputRenderer::emit_warning,
                };
                for warning in warnings {
                    emit(
                        renderer,
                        &format!(
                            "Sprint #{} canonicalization notice: {}",
                            sprint_id,
                            warning.message()
                        ),
                    );
                }
            }
        }

        processed += 1;
    }

    if is_json {
        let status = match mode {
            SprintNormalizeMode::Write => "ok",
            SprintNormalizeMode::Check => {
                if pending_ids.is_empty() {
                    "ok"
                } else {
                    "needs_normalization"
                }
            }
        };

        let payload = SprintNormalizeResponse {
            status,
            mode: mode.as_label(),
            processed,
            updated,
            changes_required: pending_ids.clone(),
            updated_ids: updated_ids.clone(),
            warnings: warning_payloads,
            skipped,
        };
        renderer.emit_raw_stdout(&serde_json::to_string(&payload).unwrap_or_default());
    }

    match mode {
        SprintNormalizeMode::Write => {
            if !is_json {
                renderer.emit_success(&format!(
                    "Normalization complete ({} sprint(s) processed, {} updated).",
                    processed, updated
                ));
            }
            Ok(())
        }
        SprintNormalizeMode::Check => {
            if pending_ids.is_empty() {
                if !is_json {
                    renderer
                        .emit_success(&format!("All {} sprint(s) already canonical.", processed));
                }
                Ok(())
            } else {
                let summary = pending_ids
                    .iter()
                    .map(|id| format!("#{}", id))
                    .collect::<Vec<_>>()
                    .join(", ");
                if !is_json {
                    renderer.emit_warning("Sprint normalization required.");
                }
                Err(format!(
                    "Sprint normalization required for: {}. Run with --write to update.",
                    summary
                ))
            }
        }
    }
}

fn resolve_normalize_mode(args: &SprintNormalizeArgs) -> Result<SprintNormalizeMode, String> {
    match (args.check, args.write) {
        (true, true) => Err("--check and --write are mutually exclusive".to_string()),
        (false, false) | (true, false) => Ok(SprintNormalizeMode::Check),
        (false, true) => Ok(SprintNormalizeMode::Write),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SprintNormalizeMode {
    Write,
    Check,
}

impl SprintNormalizeMode {
    fn as_label(&self) -> &'static str {
        match self {
            SprintNormalizeMode::Write => "write",
            SprintNormalizeMode::Check => "check",
        }
    }
}

#[derive(Debug, Serialize)]
struct SprintNormalizeResponse {
    status: &'static str,
    mode: &'static str,
    processed: usize,
    updated: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    changes_required: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    updated_ids: Vec<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    warnings: Vec<SprintNormalizeWarningPayload>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    skipped: Vec<SprintNormalizeSkipPayload>,
}

#[derive(Debug, Serialize)]
struct SprintNormalizeWarningPayload {
    sprint_id: u32,
    code: &'static str,
    message: &'static str,
}

#[derive(Debug, Serialize)]
struct SprintNormalizeSkipPayload {
    sprint_id: u32,
    reason: String,
}
