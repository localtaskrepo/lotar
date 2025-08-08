//! Source code scanner for TODO references in comment lines.
//! - Recursively scans a directory for supported source files
//! - Detects TODOs in comment lines across many languages
//! - Extracts optional UUID and a title after the TODO token

// Optimized file types configuration - removed duplicates and organized by comment style
const FILE_TYPES: &[(&str, &str)] = &[
    // Single-line comment languages with //
    ("rs", "//"),
    ("rust", "//"),
    ("go", "//"),
    ("java", "//"),
    ("js", "//"),
    ("ts", "//"),
    ("cpp", "//"),
    ("cc", "//"),
    ("cxx", "//"),
    ("c", "//"),
    ("h", "//"),
    ("hpp", "//"),
    ("cs", "//"),
    ("scala", "//"),
    ("groovy", "//"),
    ("swift", "//"),
    ("php", "//"),
    ("kotlin", "//"),
    ("dart", "//"),
    ("fsharp", "//"),
    // Hash-based comment languages
    ("py", "#"),
    ("rb", "#"),
    ("sh", "#"),
    ("bash", "#"),
    ("perl", "#"),
    ("r", "#"),
    ("elixir", "#"),
    ("powershell", "#"),
    ("nim", "#"),
    ("yaml", "#"),
    ("yml", "#"),
    // Double-dash comment languages
    ("hs", "--"),
    ("haskell", "--"),
    ("elm", "--"),
    ("pascal", "--"),
    // Semicolon comment languages
    ("clojure", ";"),
    ("scheme", ";"),
    ("commonlisp", ";"),
    ("racket", ";"),
    // Percent comment languages
    ("erlang", "%"),
    ("matlab", "%"),
    ("tex", "%"),
];

use rayon::prelude::*;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Reference {
    #[allow(dead_code)]
    pub file_path: PathBuf,
    #[allow(dead_code)]
    pub line_number: usize,
    #[allow(dead_code)]
    pub title: String,
    #[allow(dead_code)]
    pub uuid: String,
    #[allow(dead_code)]
    pub annotation: String,
    #[allow(dead_code)]
    pub code_block: String,
    #[allow(dead_code)]
    pub comment_block: String,
}

pub struct Scanner {
    path: PathBuf,
    last_scan: Vec<Reference>,
    todo_regex: Regex,
    uuid_extract_regex: Regex,
    simple_todo_regex: Regex,
}

impl Scanner {
    pub fn new(path: PathBuf) -> Self {
        let todo_regex = Regex::new(r"[Tt][Oo][Dd][Oo]").unwrap();
        let uuid_extract_regex =
            Regex::new(r"[Tt][Oo][Dd][Oo][ \t]*\(([^)]+)\)[ \t]*:?[ \t]*(.*)").unwrap();
        let simple_todo_regex = Regex::new(r"[Tt][Oo][Dd][Oo][ \t]*:?[ \t]*(.*)").unwrap();

        Self {
            path,
            last_scan: Vec::new(),
            todo_regex,
            uuid_extract_regex,
            simple_todo_regex,
        }
    }

    pub fn scan(&mut self) -> Vec<Reference> {
        // Collect candidate files first, then process in parallel
        let files = self.collect_candidate_files(self.path.as_path());

        let mut references: Vec<Reference> = files
            .par_iter()
            .flat_map(|path| self.scan_file_collect(path.as_path()))
            .collect();

        // Deterministic ordering by file then line
        references.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.line_number.cmp(&b.line_number))
        });

        self.last_scan = references.clone();
        references
    }

    fn is_supported_ext(ext: &str) -> bool {
        Self::get_comment_token(ext).is_some()
    }

    fn collect_candidate_files(&self, dir_path: &Path) -> Vec<PathBuf> {
        fn walk(dir_path: &Path, out: &mut Vec<PathBuf>) {
            // Recurse into visible subdirectories using filesystem helper
            for (_, subdir_path) in crate::utils::filesystem::list_visible_subdirs(dir_path) {
                walk(subdir_path.as_path(), out);
            }

            // Collect supported files in current directory
            if let Ok(entries) = fs::read_dir(dir_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if Scanner::is_supported_ext(&ext.to_ascii_lowercase()) {
                                out.push(path);
                            }
                        }
                    }
                }
            }
        }

        let mut files = Vec::new();
        walk(dir_path, &mut files);
        files
    }

    fn get_comment_token(extension: &str) -> Option<&'static str> {
        FILE_TYPES
            .iter()
            .find(|(file_type, _)| file_type == &extension)
            .map(|(_, token)| *token)
    }

    fn scan_file(&self, file_path: &Path, references: &mut Vec<Reference>) {
        if let Ok(file_contents) = fs::read_to_string(file_path) {
            if let Some(extension) = file_path.extension() {
                if let Some(ext_str) = extension.to_str() {
                    let ext_str = ext_str.to_ascii_lowercase();
                    // Find the correct comment token for this file type
                    if let Some(start_comment) = Self::get_comment_token(&ext_str) {
                        // Process each line to find TODOs in comments
                        for (line_number, line) in file_contents.lines().enumerate() {
                            // Check if line contains a comment and TODO (case insensitive)
                            if line.contains(start_comment) && self.todo_regex.is_match(line) {
                                let (uuid, title) = if let Some(uuid_captures) =
                                    self.uuid_extract_regex.captures(line)
                                {
                                    // Extract UUID and title from UUID format
                                    let uuid = uuid_captures
                                        .get(1)
                                        .map_or(String::new(), |m| m.as_str().to_string());
                                    let title = uuid_captures
                                        .get(2)
                                        .map_or(String::new(), |m| m.as_str().trim().to_string());
                                    (uuid, title)
                                } else if let Some(simple_captures) =
                                    self.simple_todo_regex.captures(line)
                                {
                                    // Extract title from simple TODO format
                                    let title = simple_captures
                                        .get(1)
                                        .map_or(String::new(), |m| m.as_str().trim().to_string());
                                    (String::new(), title)
                                } else {
                                    // Fallback: extract whatever comes after TODO without extra allocation
                                    let lower = "todo";
                                    if let Some(todo_pos) = line.find(lower) {
                                        let after_todo = &line[todo_pos + 4..].trim();
                                        (String::new(), after_todo.to_string())
                                    } else if let Some(todo_pos) = line.find("TODO") {
                                        let after_todo = &line[todo_pos + 4..].trim();
                                        (String::new(), after_todo.to_string())
                                    } else {
                                        (String::new(), String::new())
                                    }
                                };

                                let reference = Reference {
                                    file_path: file_path.to_path_buf(),
                                    line_number: line_number + 1, // 1-based line numbers
                                    title,
                                    uuid,
                                    annotation: line.trim().to_string(),
                                    code_block: String::new(),
                                    comment_block: String::new(),
                                };
                                references.push(reference);
                            }
                        }
                    }
                }
            }
        }
    }

    fn scan_file_collect(&self, file_path: &Path) -> Vec<Reference> {
        let mut refs = Vec::new();
        self.scan_file(file_path, &mut refs);
        refs
    }
}
