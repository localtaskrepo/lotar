use crate::api_types::{ReferenceSnippetDTO, ReferenceSnippetLineDTO, TaskDTO};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use crate::types::ReferenceEntry;
use ignore::WalkBuilder;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ReferenceService;

impl ReferenceService {
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
    ) -> LoTaRResult<(TaskDTO, bool)> {
        let derived = task_id.split('-').next().unwrap_or("");
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

        let project = derived.to_string();
        let mut task = storage
            .get(task_id, project)
            .ok_or_else(|| LoTaRError::TaskNotFound(task_id.to_string()))?;

        let already = task
            .references
            .iter()
            .any(|r| r.link.as_deref() == Some(trimmed));

        let mut added = false;
        if !already {
            task.references.push(ReferenceEntry {
                code: None,
                link: Some(trimmed.to_string()),
                file: None,
            });
            task.modified = chrono::Utc::now().to_rfc3339();
            storage.edit(task_id, &task)?;
            added = true;
        }

        Ok((TaskService::get(storage, task_id, Some(derived))?, added))
    }

    pub fn detach_link_reference(
        storage: &mut Storage,
        task_id: &str,
        url: &str,
    ) -> LoTaRResult<(TaskDTO, bool)> {
        let derived = task_id.split('-').next().unwrap_or("");
        if derived.trim().is_empty() {
            return Err(LoTaRError::InvalidTaskId(task_id.to_string()));
        }

        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(LoTaRError::ValidationError("Missing url".to_string()));
        }

        let project = derived.to_string();
        let mut task = storage
            .get(task_id, project)
            .ok_or_else(|| LoTaRError::TaskNotFound(task_id.to_string()))?;

        let before_len = task.references.len();
        task.references
            .retain(|r| r.link.as_deref() != Some(trimmed));

        let removed = task.references.len() != before_len;
        if removed {
            task.modified = chrono::Utc::now().to_rfc3339();
            storage.edit(task_id, &task)?;
        }

        Ok((TaskService::get(storage, task_id, Some(derived))?, removed))
    }

    pub fn attach_code_reference(
        storage: &mut Storage,
        repo_root: &Path,
        task_id: &str,
        code: &str,
    ) -> LoTaRResult<(TaskDTO, bool)> {
        let derived = task_id.split('-').next().unwrap_or("");
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

        let project = derived.to_string();
        let mut task = storage
            .get(task_id, project)
            .ok_or_else(|| LoTaRError::TaskNotFound(task_id.to_string()))?;

        let already = task
            .references
            .iter()
            .any(|r| r.code.as_deref() == Some(normalized.as_str()));

        let mut added = false;
        if !already {
            task.references.push(ReferenceEntry {
                code: Some(normalized),
                link: None,
                file: None,
            });
            task.modified = chrono::Utc::now().to_rfc3339();
            storage.edit(task_id, &task)?;
            added = true;
        }

        Ok((TaskService::get(storage, task_id, Some(derived))?, added))
    }

    pub fn detach_code_reference(
        storage: &mut Storage,
        task_id: &str,
        code: &str,
    ) -> LoTaRResult<(TaskDTO, bool)> {
        let derived = task_id.split('-').next().unwrap_or("");
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

        let project = derived.to_string();
        let mut task = storage
            .get(task_id, project)
            .ok_or_else(|| LoTaRError::TaskNotFound(task_id.to_string()))?;

        let before_len = task.references.len();
        candidates.sort();
        candidates.dedup();

        task.references.retain(|r| {
            let Some(stored) = r.code.as_deref() else {
                return true;
            };
            !candidates.iter().any(|candidate| candidate == stored)
        });

        let removed = task.references.len() != before_len;
        if removed {
            task.modified = chrono::Utc::now().to_rfc3339();
            storage.edit(task_id, &task)?;
        }

        Ok((TaskService::get(storage, task_id, Some(derived))?, removed))
    }

    pub fn attach_file_reference(
        storage: &mut Storage,
        repo_root: &Path,
        task_id: &str,
        file: &str,
    ) -> LoTaRResult<(TaskDTO, bool)> {
        let derived = task_id.split('-').next().unwrap_or("");
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
        let rel = resolved
            .strip_prefix(&repo_root_canonical)
            .unwrap_or(&resolved);
        let normalized = Self::normalize_path_for_display(rel);

        let project = derived.to_string();
        let mut task = storage
            .get(task_id, project)
            .ok_or_else(|| LoTaRError::TaskNotFound(task_id.to_string()))?;

        let already = task
            .references
            .iter()
            .any(|r| r.file.as_deref() == Some(normalized.as_str()));

        let mut added = false;
        if !already {
            task.references.push(ReferenceEntry {
                code: None,
                link: None,
                file: Some(normalized),
            });
            task.modified = chrono::Utc::now().to_rfc3339();
            storage.edit(task_id, &task)?;
            added = true;
        }

        Ok((TaskService::get(storage, task_id, Some(derived))?, added))
    }

    pub fn detach_file_reference(
        storage: &mut Storage,
        repo_root: &Path,
        task_id: &str,
        file: &str,
    ) -> LoTaRResult<(TaskDTO, bool)> {
        let derived = task_id.split('-').next().unwrap_or("");
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
                let rel = resolved
                    .strip_prefix(&repo_root_canonical)
                    .unwrap_or(&resolved);
                Self::normalize_path_for_display(rel)
            }
            Err(_) => trimmed.to_string(),
        };

        let project = derived.to_string();
        let mut task = storage
            .get(task_id, project)
            .ok_or_else(|| LoTaRError::TaskNotFound(task_id.to_string()))?;

        let before_len = task.references.len();
        task.references
            .retain(|r| r.file.as_deref() != Some(normalized.as_str()));

        let removed = task.references.len() != before_len;
        if removed {
            task.modified = chrono::Utc::now().to_rfc3339();
            storage.edit(task_id, &task)?;
        }

        Ok((TaskService::get(storage, task_id, Some(derived))?, removed))
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

    fn resolve_path(repo_root: &Path, raw_path: &str) -> Result<PathBuf, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_types::TaskCreate;
    use crate::services::task_service::TaskService;
    use std::fs;
    use std::path::Path;

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

        let mut storage = Storage::new(tasks_dir);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Link reference test".to_string(),
                project: Some("DEMO".to_string()),
                priority: None,
                task_type: None,
                reporter: None,
                assignee: None,
                due_date: None,
                effort: None,
                description: None,
                tags: vec![],
                relationships: None,
                custom_fields: None,
                sprints: vec![],
            },
        )
        .unwrap();

        let url = "https://example.com/docs";
        let (updated, added) =
            ReferenceService::attach_link_reference(&mut storage, &task.id, url).unwrap();
        assert!(added);
        assert!(
            updated
                .references
                .iter()
                .any(|r| r.link.as_deref() == Some(url))
        );

        let (_updated2, added2) =
            ReferenceService::attach_link_reference(&mut storage, &task.id, url).unwrap();
        assert!(!added2);

        let (updated3, removed) =
            ReferenceService::detach_link_reference(&mut storage, &task.id, url).unwrap();
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
        let mut storage = Storage::new(storage_root);

        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "File reference test".to_string(),
                project: Some("T".to_string()),
                ..TaskCreate::default()
            },
        )
        .unwrap();

        let (task, added) = ReferenceService::attach_file_reference(
            &mut storage,
            repo_root,
            &task.id,
            "src/example.rs",
        )
        .unwrap();
        assert!(added);
        assert!(
            task.references
                .iter()
                .any(|r| r.file.as_deref() == Some("src/example.rs"))
        );

        let (task, removed) = ReferenceService::detach_file_reference(
            &mut storage,
            repo_root,
            &task.id,
            "src/example.rs",
        )
        .unwrap();
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

        let mut storage = Storage::new(tasks_dir);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Link reference scheme test".to_string(),
                project: Some("DEMO".to_string()),
                priority: None,
                task_type: None,
                reporter: None,
                assignee: None,
                due_date: None,
                effort: None,
                description: None,
                tags: vec![],
                relationships: None,
                custom_fields: None,
                sprints: vec![],
            },
        )
        .unwrap();

        let url = "ftp://example.com/path/to/file";
        let (updated, added) =
            ReferenceService::attach_link_reference(&mut storage, &task.id, url).unwrap();
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

        let mut storage = Storage::new(tasks_dir);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Link reference safety test".to_string(),
                project: Some("DEMO".to_string()),
                priority: None,
                task_type: None,
                reporter: None,
                assignee: None,
                due_date: None,
                effort: None,
                description: None,
                tags: vec![],
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

        let mut storage = Storage::new(tasks_dir);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Code reference test".to_string(),
                project: Some("DEMO".to_string()),
                priority: None,
                task_type: None,
                reporter: None,
                assignee: None,
                due_date: None,
                effort: None,
                description: None,
                tags: vec![],
                relationships: None,
                custom_fields: None,
                sprints: vec![],
            },
        )
        .unwrap();

        let ref_code = "src/example.rs#2-3";
        let (updated, added) =
            ReferenceService::attach_code_reference(&mut storage, repo_root, &task.id, ref_code)
                .unwrap();
        assert!(added);
        assert!(
            updated
                .references
                .iter()
                .any(|r| r.code.as_deref() == Some(ref_code))
        );

        let (_updated2, added2) =
            ReferenceService::attach_code_reference(&mut storage, repo_root, &task.id, ref_code)
                .unwrap();
        assert!(!added2);

        let (updated3, removed) =
            ReferenceService::detach_code_reference(&mut storage, &task.id, ref_code).unwrap();
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

        let mut storage = Storage::new(tasks_dir);
        let task = TaskService::create(
            &mut storage,
            TaskCreate {
                title: "Legacy format test".to_string(),
                project: Some("DEMO".to_string()),
                ..TaskCreate::default()
            },
        )
        .unwrap();

        let legacy = "src/example.rs#L2-L3";
        let canonical = "src/example.rs#2-3";

        let (updated, added) =
            ReferenceService::attach_code_reference(&mut storage, repo_root, &task.id, legacy)
                .unwrap();
        assert!(added);
        assert!(
            updated
                .references
                .iter()
                .any(|r| r.code.as_deref() == Some(canonical))
        );

        let (updated2, removed) =
            ReferenceService::detach_code_reference(&mut storage, &task.id, legacy).unwrap();
        assert!(removed);
        assert!(
            !updated2
                .references
                .iter()
                .any(|r| r.code.as_deref() == Some(canonical))
        );
    }
}
