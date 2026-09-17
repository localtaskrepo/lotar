//! One validated candidate per config-set operation (DEV-71).
//!
//! Every mutation path (CLI `config set`, REST/MCP `ConfigService::set`, and
//! the `update_config_field`/`clear_project_field` compatibility wrappers)
//! builds a single candidate here: field names are canonicalized through the
//! shared dotted-to-flat table, all requested entries are applied, schema,
//! config, resolved, and real-task validations run against a
//! precedence-faithful in-memory resolution snapshot, and exactly one atomic
//! write persists the result. Dry runs execute the identical pipeline and omit
//! only the write. Rejected changes leave the target file bytes untouched,
//! create no configuration artifacts, and never bootstrap storage.

use crate::config::operations::{
    apply_field_to_global_config, apply_field_to_project_config, canonicalize_field_name,
    clear_project_override_field, validate_field_value,
};
use crate::config::resolution;
use crate::config::types::{ConfigError, GlobalConfig, ProjectConfig, ResolvedConfig};
use crate::config::validation::ConfigValidator;
use crate::config::validation::errors::{ValidationError, ValidationResult};
use crate::storage::task::Task;
use crate::types::enum_token_eq;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Scope a config-set operation applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigScope {
    Global,
    Project(String),
}

/// A config-set request: raw field names and values in any order.
pub struct ConfigSetRequest {
    pub entries: Vec<(String, String)>,
    /// CLI-only escape hatch: allows applying despite task-compatibility
    /// conflicts. Schema errors, malformed configuration files, and failed
    /// task enumeration are never force-escapable.
    pub force: bool,
    /// Run the full validation pipeline and skip only the write.
    pub dry_run: bool,
}

/// Outcome of an applied (or dry-run) candidate.
pub struct ConfigSetResult {
    /// Whether the configuration file was written (always false for dry runs).
    pub updated: bool,
    /// Validation feedback for the after-state (warnings may include forced
    /// task conflicts; pre-existing issues are surfaced, not hidden).
    pub validation: ValidationResult,
    /// Canonical (flat) names of the fields this request covers.
    pub normalized_fields: Vec<String>,
}

/// Fields whose effective values can invalidate existing tasks when changed.
const TASK_CONFLICT_FIELDS: &[&str] = &["issue_states", "issue_types", "issue_priorities"];

/// Fields that participate in resolved-config validation. Global changes to
/// these fields require per-project revalidation; other fields (server port,
/// sprints, agent settings, ...) cannot introduce resolved errors or task
/// conflicts, so project enumeration is skipped for them.
const RESOLVED_RELEVANT_FIELDS: &[&str] = &[
    "issue_states",
    "issue_types",
    "issue_priorities",
    "tags",
    "default_tags",
    "default_priority",
    "default_status",
    "members",
    "strict_members",
    "branch_type_aliases",
    "branch_status_aliases",
    "branch_priority_aliases",
];

/// Canonicalize request entries and reject ambiguous duplicates.
///
/// Multiple spellings that map to the same canonical field (for example
/// `tags` and `issue.tags`) are accepted when they carry the identical value
/// and rejected deterministically when their values differ, regardless of
/// input order.
pub fn canonicalize_entries(
    entries: &[(String, String)],
    is_global: bool,
) -> Result<Vec<(String, String)>, ConfigError> {
    let mut canonical: Vec<(String, String)> = Vec::new();
    for (raw_name, raw_value) in entries {
        let canonical_field = canonicalize_field_name(raw_name, is_global).map_err(|e| {
            ConfigError::ParseError(format!("Invalid config entry '{}': {}", raw_name.trim(), e))
        })?;
        match canonical
            .iter_mut()
            .find(|(field, _)| *field == canonical_field)
        {
            Some((_, existing)) if existing != raw_value => {
                // Report the pair sorted so the message is identical
                // regardless of input order.
                let (first, second) = if existing.trim() <= raw_value.trim() {
                    (existing.trim(), raw_value.trim())
                } else {
                    (raw_value.trim(), existing.trim())
                };
                return Err(ConfigError::ParseError(format!(
                    "Conflicting duplicate entries for configuration field '{}' ('{}' vs '{}'); supply a single value",
                    canonical_field, first, second
                )));
            }
            Some(_) => {}
            None => canonical.push((canonical_field, raw_value.clone())),
        }
    }
    canonical.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(canonical)
}

/// Build, validate, and (unless dry-running) atomically save one candidate.
pub fn apply_config_set(
    tasks_dir: &Path,
    scope: &ConfigScope,
    request: &ConfigSetRequest,
) -> Result<ConfigSetResult, ConfigError> {
    let is_global = matches!(scope, ConfigScope::Global);
    let canonical = canonicalize_entries(&request.entries, is_global)?;
    if canonical.is_empty() {
        return Ok(ConfigSetResult {
            updated: false,
            validation: ValidationResult::new(),
            normalized_fields: Vec::new(),
        });
    }

    validate_raw_entries(&canonical, is_global)?;

    let validator = ConfigValidator::new(tasks_dir);
    let global_base = load_global_base(tasks_dir)?;

    let mut validation = ValidationResult::new();
    let mut conflicts: Vec<String> = Vec::new();

    match scope {
        ConfigScope::Global => {
            let mut candidate = global_base.clone();
            for (field, value) in &canonical {
                apply_field_to_global_config(&mut candidate, field, value)?;
            }
            block_newly_introduced_errors(
                &validator.validate_global_config(&global_base),
                &validator.validate_global_config(&candidate),
                "global",
            )?;

            let before_resolved = resolution::preview_base_resolved(&global_base);
            let after_resolved = resolution::preview_base_resolved(&candidate);
            block_newly_introduced_errors(
                &validator.validate_resolved_config(&before_resolved),
                &validator.validate_resolved_config(&after_resolved),
                "resolved global",
            )?;
            validation.merge(validator.validate_global_config(&candidate));
            validation.merge(validator.validate_resolved_config(&after_resolved));

            let changed: Vec<&str> = canonical
                .iter()
                .map(|(field, _)| field.as_str())
                .filter(|field| RESOLVED_RELEVANT_FIELDS.contains(field))
                .collect();
            if !changed.is_empty() {
                // Enumerate real projects fail-closed: a malformed project
                // config or task file must block the change rather than
                // silently shrinking the affected set.
                let mut projects = crate::utils::filesystem::list_visible_subdirs(tasks_dir);
                projects.sort_by(|a, b| a.0.cmp(&b.0));
                for (prefix, dir) in projects {
                    let project_cfg = load_project_base(tasks_dir, &prefix)?;
                    let affected: Vec<&str> = changed
                        .iter()
                        .copied()
                        .filter(|field| !project_overrides_field(&project_cfg, field))
                        .collect();
                    if affected.is_empty() {
                        continue;
                    }
                    let before = resolution::preview_project_resolved(&global_base, &project_cfg);
                    let after = resolution::preview_project_resolved(&candidate, &project_cfg);
                    block_newly_introduced_errors(
                        &validator.validate_resolved_config(&before),
                        &validator.validate_resolved_config(&after),
                        &format!("resolved project '{}'", prefix),
                    )?;
                    collect_task_conflicts(
                        &before,
                        &after,
                        &affected,
                        &dir,
                        &prefix,
                        &mut conflicts,
                    )?;
                }
            }

            finish(
                tasks_dir,
                scope,
                request,
                canonical,
                validation,
                &conflicts,
                || save_global(tasks_dir, &candidate),
            )
        }
        ConfigScope::Project(prefix) => {
            let project_base = load_project_base(tasks_dir, prefix)?;
            let mut candidate = project_base.clone();
            for (field, value) in &canonical {
                if value.trim().is_empty() {
                    clear_project_override_field(&mut candidate, field)?;
                } else {
                    apply_field_to_project_config(&mut candidate, field, value)?;
                }
            }
            block_newly_introduced_errors(
                &validator.validate_project_config(&project_base),
                &validator.validate_project_config(&candidate),
                &format!("project '{}'", prefix),
            )?;

            let before = resolution::preview_project_resolved(&global_base, &project_base);
            let after = resolution::preview_project_resolved(&global_base, &candidate);
            block_newly_introduced_errors(
                &validator.validate_resolved_config(&before),
                &validator.validate_resolved_config(&after),
                &format!("resolved project '{}'", prefix),
            )?;
            validation.merge(validator.validate_project_config(&candidate));
            validation.merge(validator.validate_resolved_config(&after));

            let changed: Vec<&str> = canonical
                .iter()
                .map(|(field, _)| field.as_str())
                .filter(|field| RESOLVED_RELEVANT_FIELDS.contains(field))
                .collect();
            if !changed.is_empty() {
                collect_task_conflicts(
                    &before,
                    &after,
                    &changed,
                    &tasks_dir.join(prefix),
                    prefix,
                    &mut conflicts,
                )?;
            }

            finish(
                tasks_dir,
                scope,
                request,
                canonical,
                validation,
                &conflicts,
                || save_project(tasks_dir, prefix, &candidate),
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finish(
    _tasks_dir: &Path,
    _scope: &ConfigScope,
    request: &ConfigSetRequest,
    canonical: Vec<(String, String)>,
    mut validation: ValidationResult,
    conflicts: &[String],
    save: impl FnOnce() -> Result<(), ConfigError>,
) -> Result<ConfigSetResult, ConfigError> {
    if !conflicts.is_empty() {
        if request.force {
            for conflict in conflicts {
                validation
                    .warnings
                    .push(ValidationError::warning(None, conflict.clone()));
            }
        } else {
            return Err(ConfigError::ParseError(format!(
                "Configuration change blocked due to task conflicts:\n  - {}\nUse --force to apply anyway (schema and malformed-file errors still block), or fix the conflicting tasks first.",
                conflicts.join("\n  - ")
            )));
        }
    }

    if request.dry_run {
        return Ok(ConfigSetResult {
            updated: false,
            validation,
            normalized_fields: canonical.into_iter().map(|(field, _)| field).collect(),
        });
    }

    save()?;

    Ok(ConfigSetResult {
        updated: true,
        validation,
        normalized_fields: canonical.into_iter().map(|(field, _)| field).collect(),
    })
}

fn save_global(tasks_dir: &Path, candidate: &GlobalConfig) -> Result<(), ConfigError> {
    crate::config::operations::save_global_config(tasks_dir, candidate)
}

fn save_project(
    tasks_dir: &Path,
    prefix: &str,
    candidate: &ProjectConfig,
) -> Result<(), ConfigError> {
    crate::config::operations::save_project_config(tasks_dir, prefix, candidate)
}

/// Fail-closed loads of the configuration being modified or consulted.
/// A missing file is a legitimate default base; a malformed file must block.
fn load_global_base(tasks_dir: &Path) -> Result<GlobalConfig, ConfigError> {
    let path = crate::utils::paths::global_config_path(tasks_dir);
    if path.exists() {
        crate::config::persistence::load_global_config(Some(tasks_dir)).map_err(|e| {
            ConfigError::ParseError(format!(
                "Malformed global config {} (required for validation): {}",
                path.display(),
                e
            ))
        })
    } else {
        Ok(GlobalConfig::default())
    }
}

fn load_project_base(tasks_dir: &Path, prefix: &str) -> Result<ProjectConfig, ConfigError> {
    let path = crate::utils::paths::project_config_path(tasks_dir, prefix);
    if path.exists() {
        crate::config::persistence::load_project_config_from_dir(prefix, tasks_dir).map_err(|e| {
            ConfigError::ParseError(format!(
                "Malformed config for project '{}' at {} (required for validation): {}",
                prefix,
                path.display(),
                e
            ))
        })
    } else {
        Ok(ProjectConfig::new(prefix.to_string()))
    }
}

/// Schema-validate the RAW value strings of canonicalized entries.
///
/// Shared by the candidate pipeline and `ConfigService::set` so the CLI and
/// REST/MCP enforce the exact same invalid-value contract BEFORE any rewrite
/// (including the dedup-to-clear conversion): a malformed value can never
/// normalize its way into an accepted clear. Schema errors are never
/// force-escapable. Project-scope empty values express "clear this override"
/// and skip value schema checks — empties reach validation only where the
/// field's validator accepts them as clears.
pub fn validate_raw_entries(
    canonical: &[(String, String)],
    is_global: bool,
) -> Result<(), ConfigError> {
    for (field, value) in canonical {
        if !is_global && value.trim().is_empty() {
            continue;
        }
        validate_field_value(field, value).map_err(|e| {
            ConfigError::ParseError(format!("Invalid value for '{}': {}", field, e))
        })?;
    }
    Ok(())
}

/// Block only on errors the candidate newly introduces: pre-existing errors in
/// an already-broken configuration must not make every unrelated update fail.
fn block_newly_introduced_errors(
    before: &ValidationResult,
    after: &ValidationResult,
    label: &str,
) -> Result<(), ConfigError> {
    let before_keys: HashSet<String> = before.errors.iter().map(|e| e.to_string()).collect();
    let introduced: Vec<String> = after
        .errors
        .iter()
        .filter(|e| !before_keys.contains(&e.to_string()))
        .map(|e| e.to_string())
        .collect();
    if introduced.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::ParseError(format!(
            "Validation failed for {} configuration:\n{}",
            label,
            introduced.join("\n")
        )))
    }
}

/// Whether a project-level config overrides the given canonical field (and
/// therefore shields the project from a global change of that field).
fn project_overrides_field(cfg: &ProjectConfig, field: &str) -> bool {
    match field {
        "issue_states" => cfg.issue_states.is_some(),
        "issue_types" => cfg.issue_types.is_some(),
        "issue_priorities" => cfg.issue_priorities.is_some(),
        "tags" => cfg.tags.is_some(),
        "custom_fields" => cfg.custom_fields.is_some(),
        "default_tags" => cfg.default_tags.is_some(),
        "default_assignee" => cfg.default_assignee.is_some(),
        "default_reporter" => cfg.default_reporter.is_some(),
        "default_priority" => cfg.default_priority.is_some(),
        "default_status" => cfg.default_status.is_some(),
        "members" => cfg.members.is_some(),
        "strict_members" => cfg.strict_members.is_some(),
        "branch_type_aliases" => cfg.branch_type_aliases.is_some(),
        "branch_status_aliases" => cfg.branch_status_aliases.is_some(),
        "branch_priority_aliases" => cfg.branch_priority_aliases.is_some(),
        _ => false,
    }
}

/// Compare per-task enum membership before/after the candidate and record
/// newly invalid tasks. Tasks already invalid under the current resolution
/// are preserved, not blocking; malformed task files fail closed.
fn collect_task_conflicts(
    before: &ResolvedConfig,
    after: &ResolvedConfig,
    fields: &[&str],
    project_dir: &Path,
    prefix: &str,
    conflicts: &mut Vec<String>,
) -> Result<(), ConfigError> {
    let needs_tasks = fields.iter().any(|field| {
        TASK_CONFLICT_FIELDS.contains(field)
            && match *field {
                "issue_states" => {
                    !status_lists_equal(&before.issue_states.values, &after.issue_states.values)
                }
                "issue_types" => {
                    !type_lists_equal(&before.issue_types.values, &after.issue_types.values)
                }
                "issue_priorities" => !priority_lists_equal(
                    &before.issue_priorities.values,
                    &after.issue_priorities.values,
                ),
                _ => false,
            }
    });
    if !needs_tasks {
        return Ok(());
    }

    let tasks = enumerate_project_tasks(project_dir, prefix)?;
    for (id, task) in tasks {
        for field in fields {
            if !TASK_CONFLICT_FIELDS.contains(field) {
                continue;
            }
            let (was_valid, now_valid, kind, value) = match *field {
                "issue_states" => (
                    before
                        .issue_states
                        .values
                        .iter()
                        .any(|s| enum_token_eq(s.as_str(), task.status.as_str())),
                    after
                        .issue_states
                        .values
                        .iter()
                        .any(|s| enum_token_eq(s.as_str(), task.status.as_str())),
                    "status",
                    task.status.as_str().to_string(),
                ),
                "issue_types" => (
                    before
                        .issue_types
                        .values
                        .iter()
                        .any(|t| enum_token_eq(t.as_str(), task.task_type.as_str())),
                    after
                        .issue_types
                        .values
                        .iter()
                        .any(|t| enum_token_eq(t.as_str(), task.task_type.as_str())),
                    "type",
                    task.task_type.as_str().to_string(),
                ),
                "issue_priorities" => (
                    before
                        .issue_priorities
                        .values
                        .iter()
                        .any(|p| enum_token_eq(p.as_str(), task.priority.as_str())),
                    after
                        .issue_priorities
                        .values
                        .iter()
                        .any(|p| enum_token_eq(p.as_str(), task.priority.as_str())),
                    "priority",
                    task.priority.as_str().to_string(),
                ),
                _ => continue,
            };
            if was_valid && !now_valid {
                conflicts.push(format!(
                    "Task {} has {} '{}' which is not in the new {} list",
                    id, kind, value, field
                ));
            }
        }
    }
    Ok(())
}

fn status_lists_equal(a: &[crate::types::TaskStatus], b: &[crate::types::TaskStatus]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| enum_token_eq(x.as_str(), y.as_str()))
}

fn type_lists_equal(a: &[crate::types::TaskType], b: &[crate::types::TaskType]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| enum_token_eq(x.as_str(), y.as_str()))
}

fn priority_lists_equal(a: &[crate::types::Priority], b: &[crate::types::Priority]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| enum_token_eq(x.as_str(), y.as_str()))
}

/// Enumerate a project's task files fail-closed. Non-numeric YAML files are
/// ignored (matching search semantics); numeric-stem files must be readable
/// and parseable or validation cannot claim the task set was inspected.
fn enumerate_project_tasks(
    project_dir: &Path,
    prefix: &str,
) -> Result<Vec<(String, Task)>, ConfigError> {
    if !project_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files: Vec<PathBuf> = crate::utils::filesystem::list_files_with_ext(project_dir, "yml")
        .into_iter()
        .filter(|path| crate::utils::filesystem::file_numeric_stem(path).is_some())
        .collect();
    files.sort();
    let mut tasks = Vec::new();
    for path in files {
        let numeric = crate::utils::filesystem::file_numeric_stem(&path).unwrap_or_default();
        let content = std::fs::read_to_string(&path).map_err(|e| {
            ConfigError::IoError(format!(
                "Failed to read task file {} during conflict validation: {}",
                path.display(),
                e
            ))
        })?;
        let task: Task = serde_yaml_ng::from_str(&content).map_err(|e| {
            ConfigError::ParseError(format!(
                "Malformed task file {} during conflict validation: {}",
                path.display(),
                e
            ))
        })?;
        tasks.push((format!("{}-{}", prefix, numeric), task));
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_accepts_flat_and_dotted_including_exceptions() {
        assert_eq!(
            canonicalize_field_name("issue_states", false).unwrap(),
            "issue_states"
        );
        assert_eq!(
            canonicalize_field_name("issue.states", false).unwrap(),
            "issue_states"
        );
        assert_eq!(
            canonicalize_field_name("default.strict_members", false).unwrap(),
            "strict_members"
        );
        assert_eq!(canonicalize_field_name("issue.tags", true).unwrap(), "tags");
        assert_eq!(
            canonicalize_field_name("server.port", true).unwrap(),
            "server_port"
        );
    }

    #[test]
    fn canonicalize_rejects_scope_mismatches_and_unknowns() {
        // Maps to a real global field, but not settable for projects.
        assert!(canonicalize_field_name("server.port", false).is_err());
        // Unknown name in both scopes.
        assert!(canonicalize_field_name("not_a_field", true).is_err());
        assert!(canonicalize_field_name("", true).is_err());
    }

    #[test]
    fn collision_detection_is_order_independent() {
        let a = vec![
            ("tags".to_string(), "a".to_string()),
            ("issue.tags".to_string(), "b".to_string()),
        ];
        let b = vec![
            ("issue.tags".to_string(), "b".to_string()),
            ("tags".to_string(), "a".to_string()),
        ];
        let err_a = canonicalize_entries(&a, true).unwrap_err().to_string();
        let err_b = canonicalize_entries(&b, true).unwrap_err().to_string();
        assert!(err_a.contains("Conflicting duplicate entries"), "{err_a}");
        assert_eq!(err_a, err_b, "deterministic regardless of input order");
    }

    #[test]
    fn identical_canonical_duplicates_dedup() {
        let entries = vec![
            ("tags".to_string(), "a,b".to_string()),
            ("issue.tags".to_string(), "a,b".to_string()),
        ];
        let canon = canonicalize_entries(&entries, true).unwrap();
        assert_eq!(canon, vec![("tags".to_string(), "a,b".to_string())]);
    }

    #[test]
    fn entries_are_sorted_deterministically() {
        let entries = vec![
            ("members".to_string(), "x".to_string()),
            ("default.assignee".to_string(), "y".to_string()),
            ("agent_logs_dir".to_string(), "z".to_string()),
        ];
        let canon = canonicalize_entries(&entries, true).unwrap();
        let fields: Vec<&str> = canon.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(
            fields,
            vec!["agent_logs_dir", "default_assignee", "members"]
        );
    }
}
