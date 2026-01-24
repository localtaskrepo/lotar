use crate::api_types::{ScanEntry, ScanRequest, ScanResponse, ScanSummary, TaskCreate};
use crate::config::manager::ConfigManager;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::scanner;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::types::{CustomFields, Priority, ReferenceEntry, TaskType, custom_value_string};
use crate::utils::paths::repo_relative_display;
use crate::utils::scan::{parse_inline_attributes, strip_bracket_attributes};
use crate::workspace::TasksDirectoryResolver;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const STATUS_CREATED: &str = "created";
const STATUS_UPDATED: &str = "updated";
const STATUS_SKIPPED: &str = "skipped";
const STATUS_FAILED: &str = "failed";

const ACTION_CREATE: &str = "create";
const ACTION_REFRESH: &str = "refresh";
const ACTION_SKIP: &str = "skip";

pub struct ScanService;

impl ScanService {
    pub fn run(
        resolver: &TasksDirectoryResolver,
        request: ScanRequest,
    ) -> LoTaRResult<ScanResponse> {
        let dry_run = request.dry_run;
        let mut warnings = Vec::new();
        let info = Vec::new();
        let mut summary = ScanSummary::default();

        let default_root = resolver
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .or_else(crate::project::get_project_path)
            .unwrap_or_else(|| PathBuf::from("."));
        let roots: Vec<PathBuf> = if request.paths.is_empty() {
            vec![default_root]
        } else {
            request.paths.iter().map(PathBuf::from).collect()
        };

        for root in &roots {
            if !root.exists() {
                return Err(LoTaRError::ValidationError(format!(
                    "Path '{}' does not exist",
                    root.display()
                )));
            }
        }

        let project_hint = request.project.as_ref().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let project_name = project_hint
            .clone()
            .unwrap_or_else(|| crate::project::get_effective_project_name(resolver));

        let scan_config = resolve_scan_config(resolver, project_hint.as_deref());

        let strip_attributes = request
            .strip_attributes
            .unwrap_or(scan_config.strip_attributes);

        let mut final_words = scan_config.signal_words.clone();
        for word in &scan_config.issue_type_words {
            if !final_words
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(word))
            {
                final_words.push(word.clone());
            }
        }

        let mut all_results = Vec::new();
        for root in roots {
            let mut scan = scanner::Scanner::new(root)
                .with_include_ext(&request.include)
                .with_exclude_ext(&request.exclude)
                .with_modified_only(request.modified_only);

            if !final_words.is_empty() {
                scan = scan.with_signal_words(&final_words);
            }

            scan = scan.with_ticket_detection(
                scan_config.ticket_patterns.as_deref(),
                scan_config.enable_ticket_words,
            );
            all_results.extend(scan.scan());
        }

        all_results.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.line_number.cmp(&b.line_number))
        });

        let mut line_scanner = scanner::Scanner::new(PathBuf::from(".")).with_ticket_detection(
            scan_config.ticket_patterns.as_deref(),
            scan_config.enable_ticket_words,
        );
        if !final_words.is_empty() {
            line_scanner = line_scanner.with_signal_words(&final_words);
        }

        let target_keys = build_target_keys(&request.targets);

        let mut entries = Vec::new();
        let mut storage = if dry_run {
            None
        } else {
            Some(Storage::new(resolver.path.clone()))
        };

        for entry in all_results {
            let rel_path = repo_relative_display(&entry.file_path);
            let key = target_key(&rel_path, entry.line_number);
            if !target_keys.is_empty() && !target_keys.contains(&key) {
                continue;
            }

            let (original_line, line_error) = match read_line(&entry.file_path, entry.line_number) {
                Ok(line) => (Some(line), None),
                Err(err) => (None, Some(err)),
            };

            if let Some(err) = line_error {
                let scan_entry = ScanEntry {
                    status: STATUS_FAILED.to_string(),
                    action: ACTION_SKIP.to_string(),
                    file: rel_path.clone(),
                    line: entry.line_number,
                    title: entry.title.clone(),
                    annotation: entry.annotation.clone(),
                    code_reference: format!("{}#{}", rel_path, entry.line_number),
                    existing_key: None,
                    task_id: None,
                    original_line: None,
                    updated_line: None,
                    message: Some(err),
                };
                summary.failed += 1;
                entries.push(scan_entry);
                continue;
            }

            let original_line = original_line.unwrap_or_default();
            let existing_key = line_scanner.extract_ticket_key_from_line(&original_line);
            let action = if existing_key.is_none() {
                ACTION_CREATE
            } else if scan_config.enable_mentions {
                ACTION_REFRESH
            } else {
                ACTION_SKIP
            };

            if dry_run {
                let (status, message, updated_line) = match action {
                    ACTION_CREATE => {
                        let placeholder_id = format!("{}-NEW", project_name);
                        let proposed = line_scanner
                            .suggest_insertion_for_line(&original_line, &placeholder_id)
                            .map(|line| {
                                if strip_attributes {
                                    strip_bracket_attributes(&line)
                                } else {
                                    line
                                }
                            });
                        (
                            STATUS_CREATED,
                            Some("Dry run: would create task".to_string()),
                            proposed,
                        )
                    }
                    ACTION_REFRESH => (
                        STATUS_UPDATED,
                        Some("Dry run: would refresh reference".to_string()),
                        None,
                    ),
                    _ => (STATUS_SKIPPED, Some("No changes".to_string()), None),
                };

                let scan_entry = ScanEntry {
                    status: status.to_string(),
                    action: action.to_string(),
                    file: rel_path.clone(),
                    line: entry.line_number,
                    title: entry.title.clone(),
                    annotation: entry.annotation.clone(),
                    code_reference: format!("{}#{}", rel_path, entry.line_number),
                    existing_key: existing_key.clone(),
                    task_id: None,
                    original_line: Some(original_line.clone()),
                    updated_line,
                    message,
                };
                update_summary(&mut summary, status);
                entries.push(scan_entry);
                continue;
            }

            let status_result = match action {
                ACTION_CREATE => {
                    let mut task_id = None;
                    let mut message = None;
                    let mut updated_line = None;

                    let title = coerce_entry_title(
                        &entry.title,
                        &entry.annotation,
                        &rel_path,
                        entry.line_number,
                    );
                    let inline_attrs = parse_inline_attributes(&original_line);

                    let task_create = TaskCreate {
                        title,
                        project: Some(project_name.clone()),
                        priority: inline_attrs.priority.map(Priority::from),
                        task_type: inline_attrs.task_type.map(TaskType::from),
                        reporter: None,
                        assignee: inline_attrs.assignee,
                        due_date: inline_attrs.due,
                        effort: inline_attrs.effort,
                        description: None,
                        tags: inline_attrs.tags,
                        relationships: None,
                        custom_fields: build_custom_fields(inline_attrs.fields),
                        sprints: Vec::new(),
                    };

                    let created = match storage
                        .as_mut()
                        .ok_or_else(|| {
                            LoTaRError::ValidationError("Storage unavailable".to_string())
                        })
                        .and_then(|storage| TaskService::create(storage, task_create))
                    {
                        Ok(task) => {
                            task_id = Some(task.id.clone());
                            Ok(task.id)
                        }
                        Err(err) => Err(err),
                    };

                    match created {
                        Ok(created_id) => {
                            let code_ref = format!("{}#{}", rel_path, entry.line_number);
                            if let Some(storage) = storage.as_mut()
                                && let Err(err) = update_task_code_reference(
                                    storage,
                                    &created_id,
                                    &code_ref,
                                    request.reanchor,
                                )
                            {
                                warnings.push(format!(
                                    "Failed to update references for {}: {}",
                                    created_id, err
                                ));
                            }

                            let proposed = line_scanner
                                .suggest_insertion_for_line(&original_line, &created_id)
                                .map(|line| {
                                    if strip_attributes {
                                        strip_bracket_attributes(&line)
                                    } else {
                                        line
                                    }
                                });

                            if let Some(next_line) = proposed
                                && next_line != original_line
                            {
                                match replace_line(&entry.file_path, entry.line_number, &next_line)
                                {
                                    Ok(()) => {
                                        updated_line = Some(next_line);
                                    }
                                    Err(err) => {
                                        warnings.push(format!(
                                            "Failed to update {}:{}: {}",
                                            rel_path, entry.line_number, err
                                        ));
                                        message =
                                            Some("Task created but file update failed".to_string());
                                    }
                                }
                            }

                            (STATUS_CREATED, task_id, updated_line, message)
                        }
                        Err(err) => (STATUS_FAILED, task_id, None, Some(err.to_string())),
                    }
                }
                ACTION_REFRESH => {
                    if let Some(task_id) = existing_key.as_deref() {
                        let code_ref = format!("{}#{}", rel_path, entry.line_number);
                        let updated = storage
                            .as_mut()
                            .ok_or_else(|| {
                                LoTaRError::ValidationError("Storage unavailable".to_string())
                            })
                            .and_then(|storage| {
                                update_task_code_reference(
                                    storage,
                                    task_id,
                                    &code_ref,
                                    request.reanchor,
                                )
                            });
                        match updated {
                            Ok(_) => (STATUS_UPDATED, Some(task_id.to_string()), None, None),
                            Err(err) => (
                                STATUS_FAILED,
                                Some(task_id.to_string()),
                                None,
                                Some(err.to_string()),
                            ),
                        }
                    } else {
                        (
                            STATUS_SKIPPED,
                            None,
                            None,
                            Some("No existing key".to_string()),
                        )
                    }
                }
                _ => (STATUS_SKIPPED, None, None, Some("No changes".to_string())),
            };

            let (status, task_id, updated_line, message) = status_result;

            let scan_entry = ScanEntry {
                status: status.to_string(),
                action: action.to_string(),
                file: rel_path.clone(),
                line: entry.line_number,
                title: entry.title.clone(),
                annotation: entry.annotation.clone(),
                code_reference: format!("{}#{}", rel_path, entry.line_number),
                existing_key: existing_key.clone(),
                task_id,
                original_line: Some(original_line.clone()),
                updated_line,
                message,
            };
            update_summary(&mut summary, status);
            entries.push(scan_entry);
        }

        let status = if summary.failed > 0 { "partial" } else { "ok" };

        Ok(ScanResponse {
            status: status.to_string(),
            dry_run,
            project: Some(project_name),
            summary,
            warnings,
            info,
            entries,
        })
    }
}

fn update_summary(summary: &mut ScanSummary, status: &str) {
    match status {
        STATUS_CREATED => summary.created += 1,
        STATUS_UPDATED => summary.updated += 1,
        STATUS_SKIPPED => summary.skipped += 1,
        STATUS_FAILED => summary.failed += 1,
        _ => {}
    }
}

struct ScanConfigSnapshot {
    signal_words: Vec<String>,
    ticket_patterns: Option<Vec<String>>,
    enable_ticket_words: bool,
    enable_mentions: bool,
    strip_attributes: bool,
    issue_type_words: Vec<String>,
}

fn resolve_scan_config(
    resolver: &TasksDirectoryResolver,
    project: Option<&str>,
) -> ScanConfigSnapshot {
    let manager = ConfigManager::new_manager_with_tasks_dir_readonly(resolver.path.as_path()).ok();
    if let Some(manager) = manager {
        let resolved = match project {
            Some(project_name) => manager
                .get_project_config(project_name)
                .unwrap_or_else(|_| manager.get_resolved_config().clone()),
            None => manager.get_resolved_config().clone(),
        };
        let issue_type_words = if resolved.scan_enable_ticket_words {
            resolved
                .issue_types
                .values
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        ScanConfigSnapshot {
            signal_words: resolved.scan_signal_words,
            ticket_patterns: resolved.scan_ticket_patterns,
            enable_ticket_words: resolved.scan_enable_ticket_words,
            enable_mentions: resolved.scan_enable_mentions,
            strip_attributes: resolved.scan_strip_attributes,
            issue_type_words,
        }
    } else {
        ScanConfigSnapshot {
            signal_words: Vec::new(),
            ticket_patterns: None,
            enable_ticket_words: false,
            enable_mentions: true,
            strip_attributes: true,
            issue_type_words: Vec::new(),
        }
    }
}

fn build_custom_fields(fields: Vec<(String, String)>) -> Option<CustomFields> {
    if fields.is_empty() {
        return None;
    }
    let mut custom: CustomFields = HashMap::new();
    for (key, value) in fields {
        custom.insert(key, custom_value_string(value));
    }
    Some(custom)
}

fn read_line(path: &Path, line_number: usize) -> Result<String, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let lines: Vec<&str> = contents.lines().collect();
    if line_number == 0 || line_number > lines.len() {
        return Err(format!(
            "Line {} is out of range (max {})",
            line_number,
            lines.len()
        ));
    }
    Ok(lines[line_number - 1].to_string())
}

fn replace_line(path: &Path, line_number: usize, new_line: &str) -> Result<(), String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let mut lines: Vec<&str> = contents.lines().collect();
    if line_number == 0 || line_number > lines.len() {
        return Err(format!(
            "Line {} is out of range (max {})",
            line_number,
            lines.len()
        ));
    }
    lines[line_number - 1] = new_line;
    let mut out = String::new();
    for (idx, line) in lines.iter().enumerate() {
        out.push_str(line);
        if idx + 1 < lines.len() || contents.ends_with('\n') {
            out.push('\n');
        }
    }
    std::fs::write(path, out).map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(())
}

fn update_task_code_reference(
    storage: &mut Storage,
    task_id: &str,
    code_ref: &str,
    reanchor: bool,
) -> LoTaRResult<bool> {
    let derived = task_id.split('-').next().unwrap_or("");
    if derived.trim().is_empty() {
        return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
    }

    let mut task = storage
        .get(task_id, derived.to_string())
        .ok_or_else(|| LoTaRError::TaskNotFound(task_id.to_string()))?;

    let has_exact = task
        .references
        .iter()
        .any(|r| r.code.as_deref() == Some(code_ref));

    let mut changed = false;
    if reanchor {
        let before_len = task.references.len();
        task.references
            .retain(|r| r.code.as_deref() == Some(code_ref));
        if before_len != task.references.len() {
            changed = true;
        }
        if !has_exact {
            task.references.push(ReferenceEntry {
                code: Some(code_ref.to_string()),
                ..Default::default()
            });
            changed = true;
        }
    } else if !has_exact {
        let file_key = code_ref
            .split_once('#')
            .map(|(file, _)| file)
            .unwrap_or(code_ref);
        task.references.retain(|r| {
            if let Some(code) = &r.code
                && let Some((file_part, _)) = code.split_once('#')
                && file_part == file_key
                && code != code_ref
            {
                return false;
            }
            true
        });
        task.references.push(ReferenceEntry {
            code: Some(code_ref.to_string()),
            ..Default::default()
        });
        changed = true;
    }

    if changed {
        task.modified = Utc::now().to_rfc3339();
        storage.edit(task_id, &task)?;
    }
    Ok(changed)
}

fn coerce_entry_title(title: &str, annotation: &str, rel_path: &str, line: usize) -> String {
    if !title.trim().is_empty() {
        return title.trim().to_string();
    }
    if !annotation.trim().is_empty() {
        return annotation.trim().to_string();
    }
    format!("TODO in {}:{}", rel_path, line)
}

fn normalize_target_path(value: &str) -> String {
    value.trim().replace('\\', "/")
}

fn target_key(file: &str, line: usize) -> String {
    format!("{}:{}", normalize_target_path(file), line)
}

fn build_target_keys(targets: &[crate::api_types::ScanTarget]) -> HashSet<String> {
    targets
        .iter()
        .map(|target| target_key(&target.file, target.line))
        .collect()
}

#[cfg(test)]
mod scan_service_tests {
    use super::ScanService;
    use crate::api_types::{ScanRequest, ScanTarget};
    use crate::workspace::TasksDirectoryResolver;

    #[test]
    fn scan_service_dry_run_reports_create_action() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_path = temp.path();
        let tasks_dir = repo_path.join(".tasks");
        std::fs::create_dir_all(&tasks_dir).expect("create tasks dir");
        let src_dir = repo_path.join("src");
        std::fs::create_dir_all(&src_dir).expect("create src dir");
        let file_path = src_dir.join("lib.rs");
        std::fs::write(&file_path, "// TODO: Add retry logic\n").expect("write file");

        std::env::set_current_dir(repo_path).expect("set cwd");

        let resolver = TasksDirectoryResolver::resolve(Some(tasks_dir.to_str().unwrap()), None)
            .expect("resolve tasks dir");

        let request = ScanRequest {
            paths: vec![repo_path.to_string_lossy().to_string()],
            include: Vec::new(),
            exclude: Vec::new(),
            project: None,
            dry_run: true,
            strip_attributes: None,
            reanchor: false,
            modified_only: false,
            targets: Vec::<ScanTarget>::new(),
        };

        let response = ScanService::run(&resolver, request).expect("scan run");
        assert_eq!(response.summary.created, 1);
        assert_eq!(response.entries.len(), 1);
        let entry = &response.entries[0];
        assert_eq!(entry.action, "create");
        assert!(entry.code_reference.contains("src/lib.rs#1"));
    }
}
