use crate::api_types::{ReferenceSnippetDTO, ReferenceSnippetLineDTO};
use std::fs;
use std::path::{Path, PathBuf};

pub struct ReferenceService;

impl ReferenceService {
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
            .unwrap_or(&resolved)
            .display()
            .to_string();

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
        if !buffer.is_empty() {
            if let Ok(value) = buffer.parse::<usize>() {
                numbers.push(value);
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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

        let snippet = ReferenceService::snippet_for_code(repo_root, "src/example.rs#L2-L3", 1, 1)
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

        let snippet = ReferenceService::snippet_for_code(repo_root, "src/lib.rs#L1", 3, 3)
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

        let err = ReferenceService::snippet_for_code(repo_root, "src/missing.rs#L4", 2, 2)
            .expect_err("expected failure for missing file");

        assert!(err.contains("not found"));
    }
}
