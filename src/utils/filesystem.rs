use std::fs;
use std::path::{Path, PathBuf};

/// List visible (non-dot, non-`@`) subdirectories under a directory.
/// Returns tuples of (directory_name, full_path).
pub fn list_visible_subdirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut result = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                if name.starts_with('.')
                    || name.contains('@')
                    || name.eq_ignore_ascii_case("sprints")
                {
                    continue;
                }
                result.push((name.to_string(), path));
            }
        }
    }

    result
}

/// List files under a directory that have the given extension (case-insensitive).
/// Returns a Vec of full paths.
pub fn list_files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let ext_lc = ext.to_ascii_lowercase();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && let Some(e) = path.extension().and_then(|s| s.to_str())
                && e.to_ascii_lowercase() == ext_lc
            {
                result.push(path);
            }
        }
    }

    result
}

/// Recursively list files with the given extension (case-insensitive) under a
/// directory and all of its subdirectories.
pub fn list_files_with_ext_recursive(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let ext_lc = ext.to_ascii_lowercase();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if let Some(e) = path.extension().and_then(|s| s.to_str())
                    && e.to_ascii_lowercase() == ext_lc
                {
                    result.push(path);
                }
            }
        }
    }

    result
}

/// Parse a numeric file stem (e.g., "42" from "42.yml") into u64.
pub fn parse_numeric_stem(stem: &str) -> Option<u64> {
    stem.parse::<u64>().ok()
}

/// Extract numeric stem from a file path if its stem is a u64.
pub fn file_numeric_stem(path: &Path) -> Option<u64> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(parse_numeric_stem)
}
