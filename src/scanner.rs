//! Simple source scanner for TODO-like references in comments across many languages.

// Supported extensions mapped to their single-line comment tokens
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

use ignore::WalkBuilder;
#[cfg(feature = "parallel")]
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
    signal_regex: Regex,
    uuid_extract_regex: Regex,
    simple_extract_regex: Regex,
    include_ext: Option<Vec<String>>, // lowercase extensions without dot
    exclude_ext: Option<Vec<String>>, // lowercase extensions without dot
    signal_words: Vec<String>,        // lowercase tokens like todo, fixme
}

impl Scanner {
    pub fn new(path: PathBuf) -> Self {
        // Default signal words (case-insensitive)
        let default_words = vec![
            "todo".to_string(),
            "fixme".to_string(),
            "hack".to_string(),
            "bug".to_string(),
            "note".to_string(),
        ];
        let (signal_regex, uuid_extract_regex, simple_extract_regex) =
            Self::build_signal_regexes(&default_words);

        Self {
            path,
            last_scan: Vec::new(),
            signal_regex,
            uuid_extract_regex,
            simple_extract_regex,
            include_ext: None,
            exclude_ext: None,
            signal_words: default_words,
        }
    }

    fn build_signal_regexes(words: &[String]) -> (Regex, Regex, Regex) {
        // Join words as alternation, escaping handled by assuming simple alphanumerics
        let joined = words
            .iter()
            .map(|w| regex::escape(w))
            .collect::<Vec<_>>()
            .join("|");
        // Signal present anywhere in line (word boundary), case-insensitive
        let signal_re = match Regex::new(&format!(r"(?i)\b(?:{})\b", joined)) {
            Ok(r) => r,
            Err(_) => match Regex::new(r"(?i)\b(?:todo|fixme|hack|bug|note)\b") {
                Ok(r) => r,
                Err(_) => Regex::new(r"todo").unwrap(), // extremely unlikely; minimal fallback
            },
        };
        // With id in parens: WORD (<id>): <text>
        let uuid_re = Regex::new(&format!(
            r"(?i)\b(?:{})[ \t]*\(([^)]+)\)[ \t]*:?\s*(.*)",
            joined
        ))
        .unwrap_or_else(|_| {
            Regex::new(r"(?i)\b(?:todo|fixme|hack|bug|note)[ \t]*\(([^)]+)\)[ \t]*:?\s*(.*)")
                .unwrap_or_else(|_| {
                    Regex::new(r"\(([^)]+)\)\s*(.*)").expect("regex compile fallback")
                })
        });
        // Simple form: WORD: <text> (or just WORD <text>)
        let simple_re = match Regex::new(&format!(r"(?i)\b(?:{})[ \t]*:?\s*(.*)", joined)) {
            Ok(r) => r,
            Err(_) => match Regex::new(r"(?i)\b(?:todo|fixme|hack|bug|note)[ \t]*:?\s*(.*)") {
                Ok(r) => r,
                Err(_) => Regex::new(r"\s*(.*)").expect("regex compile fallback"),
            },
        };
        (signal_re, uuid_re, simple_re)
    }

    pub fn with_include_ext(mut self, exts: &[String]) -> Self {
        if !exts.is_empty() {
            self.include_ext = Some(exts.iter().map(|e| e.to_ascii_lowercase()).collect());
        }
        self
    }

    pub fn with_exclude_ext(mut self, exts: &[String]) -> Self {
        if !exts.is_empty() {
            self.exclude_ext = Some(exts.iter().map(|e| e.to_ascii_lowercase()).collect());
        }
        self
    }

    pub fn with_signal_words(mut self, words: &[String]) -> Self {
        if !words.is_empty() {
            self.signal_words = words.iter().map(|w| w.to_ascii_lowercase()).collect();
            let (signal_regex, uuid_extract_regex, simple_extract_regex) =
                Self::build_signal_regexes(&self.signal_words);
            self.signal_regex = signal_regex;
            self.uuid_extract_regex = uuid_extract_regex;
            self.simple_extract_regex = simple_extract_regex;
        }
        self
    }

    pub fn scan(&mut self) -> Vec<Reference> {
        // Collect candidate files first, then process in parallel
        let files = self.collect_candidate_files(self.path.as_path());

        #[cfg(feature = "parallel")]
        let mut references: Vec<Reference> = files
            .par_iter()
            .flat_map(|path| self.scan_file_collect(path.as_path()))
            .collect();

        #[cfg(not(feature = "parallel"))]
        let mut references: Vec<Reference> = files
            .iter()
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
        let mut files = Vec::new();

        // Build walker that respects .lotarignore or falls back to .gitignore
        let mut builder = WalkBuilder::new(dir_path);
        builder.hidden(true); // skip hidden by default

        let lotarignore = dir_path.join(".lotarignore");
        if lotarignore.exists() {
            // Use .lotarignore rules only (no gitignore fallback)
            builder.ignore(false);
            builder.git_ignore(false);
            builder.git_global(false);
            builder.git_exclude(false);
            builder.add_ignore(lotarignore);
        } else {
            // Fallback to git ignore files
            builder.ignore(true);
            builder.git_ignore(true);
            builder.git_global(true);
            builder.git_exclude(true);
            // Also explicitly add root .gitignore so it works even without a .git repo
            let gitignore = dir_path.join(".gitignore");
            if gitignore.exists() {
                builder.add_ignore(gitignore);
            }
        }

        let walker = builder.build();
        for entry in walker.flatten() {
            if entry.file_type().is_some_and(|t| t.is_file()) {
                let path = entry.path().to_path_buf();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lc = ext.to_ascii_lowercase();
                    if !Self::is_supported_ext(&ext_lc) {
                        continue;
                    }
                    if let Some(ref excludes) = self.exclude_ext {
                        if excludes.iter().any(|e| e == &ext_lc) {
                            continue;
                        }
                    }
                    if let Some(ref includes) = self.include_ext {
                        if !includes.iter().any(|e| e == &ext_lc) {
                            continue;
                        }
                    }
                    files.push(path);
                }
            }
        }

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
                            // Check if line contains a comment and any configured signal word
                            if line.contains(start_comment) && self.signal_regex.is_match(line) {
                                let (uuid, title) = if let Some(c) =
                                    self.uuid_extract_regex.captures(line)
                                {
                                    let uuid =
                                        c.get(1).map_or(String::new(), |m| m.as_str().to_string());
                                    let title = c
                                        .get(2)
                                        .map_or(String::new(), |m| m.as_str().trim().to_string());
                                    (uuid, title)
                                } else if let Some(c) = self.simple_extract_regex.captures(line) {
                                    let title = c
                                        .get(1)
                                        .map_or(String::new(), |m| m.as_str().trim().to_string());
                                    (String::new(), title)
                                } else {
                                    (String::new(), String::new())
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
