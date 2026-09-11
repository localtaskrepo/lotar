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
    ("jsx", "//"),
    ("tsx", "//"),
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
    ("lua", "--"),
    // Hash-based comment languages
    ("py", "#"),
    ("rb", "#"),
    ("sh", "#"),
    ("bash", "#"),
    ("perl", "#"),
    ("r", "#"),
    ("elixir", "#"),
    ("powershell", "#"),
    ("ps1", "#"),
    ("nim", "#"),
    ("yaml", "#"),
    ("yml", "#"),
    ("toml", "#"),
    ("hcl", "#"),
    ("tf", "#"),
    // Double-dash comment languages
    ("hs", "--"),
    ("haskell", "--"),
    ("elm", "--"),
    ("pascal", "--"),
    ("sql", "--"),
    // Semicolon comment languages
    ("clojure", ";"),
    ("scheme", ";"),
    ("commonlisp", ";"),
    ("racket", ";"),
    ("ini", ";"),
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

#[derive(Default)]
struct CommentLexer {
    block: Option<(&'static str, &'static str, usize)>,
    quote: Option<(String, bool, bool)>, // terminator, escapes, multiline
    ambiguous: bool,
}

impl CommentLexer {
    // Conservative lexical subset, not a language parser. Unsupported literal
    // forms or unterminated ordinary quotes stop further discovery in the source.
    fn ranges(&mut self, line: &str, extension: &str) -> Vec<std::ops::Range<usize>> {
        let mut ranges = Vec::new();
        let mut index = 0;
        let single = Scanner::get_comment_token(extension);
        let (open, close) = Scanner::block_tokens_for(extension);
        while index < line.len() && !self.ambiguous {
            if let Some((open, close, mut depth)) = self.block {
                let start = index;
                while index < line.len() {
                    if line[index..].starts_with(close) {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        index += close.len();
                    } else if line[index..].starts_with(open) {
                        if !matches!(extension, "rs" | "rust" | "swift" | "scala" | "kotlin") {
                            self.ambiguous = true;
                            return ranges;
                        }
                        depth += 1;
                        index += open.len();
                    } else {
                        index += line[index..].chars().next().unwrap().len_utf8();
                    }
                }
                ranges.push(start..index);
                self.block = if depth == 0 {
                    None
                } else {
                    Some((open, close, depth))
                };
                if depth == 0 {
                    index += close.len();
                }
                continue;
            }
            let tail = &line[index..];
            if let Some((terminator, escapes, _)) = self.quote.as_ref() {
                if *escapes
                    && ["${", "#{", "$(", "\\("]
                        .iter()
                        .any(|prefix| tail.starts_with(prefix))
                {
                    self.ambiguous = true;
                    break;
                } else if *escapes
                    && tail.starts_with(if matches!(extension, "ps1" | "powershell") {
                        '`'
                    } else {
                        '\\'
                    })
                {
                    index += 1;
                    if index < line.len() {
                        index += line[index..].chars().next().unwrap().len_utf8();
                    }
                } else if tail.starts_with(terminator) {
                    index += terminator.len();
                    self.quote = None;
                } else {
                    index += tail.chars().next().unwrap().len_utf8();
                }
                continue;
            }
            let block = if extension.is_empty() {
                [("/*", "*/"), ("<!--", "-->")]
                    .into_iter()
                    .find(|(open, _)| tail.starts_with(open))
            } else {
                open.zip(close).filter(|(open, _)| tail.starts_with(open))
            };
            if let Some((open, close)) = block {
                index += open.len();
                self.block = Some((open, close, 1));
                continue;
            }
            let token = single.filter(|token| tail.starts_with(token)).or_else(|| {
                if extension.is_empty() {
                    ["//", "#", "--", ";", "%"].into_iter().find(|token| {
                        tail.starts_with(token)
                            && (*token != ";" || line[..index].trim().is_empty())
                    })
                } else {
                    None
                }
            });
            if let Some(token) = token {
                ranges.push(index + token.len()..line.len());
                break;
            }
            if matches!(extension, "rs" | "rust") && tail.starts_with('r') {
                let hashes = tail[1..].bytes().take_while(|b| *b == b'#').count();
                if tail[1 + hashes..].starts_with('"') {
                    self.quote = Some((format!("\"{}", "#".repeat(hashes)), false, true));
                    index += hashes + 2;
                    continue;
                }
            }
            if ["R\"", "@\"", "@'", "$\"", "f\"", "f'", "F\"", "F'", "<<"]
                .iter()
                .any(|prefix| tail.starts_with(prefix))
                || (matches!(extension, "js" | "ts" | "jsx" | "tsx") && tail.starts_with('/'))
                || (extension == "lua" && (tail.starts_with("[[") || tail.starts_with("[=")))
                || (extension == "sql"
                    && (tail.starts_with('$') || tail.starts_with("E'") || tail.starts_with("e'")))
                || (matches!(extension, "yaml" | "yml") && tail.starts_with(['|', '>']))
                || (matches!(extension, "rb" | "perl") && tail.starts_with(['/', '%']))
                || (extension == "r" && tail.starts_with("r\""))
            {
                self.ambiguous = true;
                break;
            }
            let ch = tail.chars().next().unwrap();
            if matches!(ch, '\'' | '"' | '`') {
                let triple = ch != '`' && tail.starts_with(&ch.to_string().repeat(3));
                let terminator = ch.to_string().repeat(if triple { 3 } else { 1 });
                index += terminator.len();
                let escapes = !matches!(
                    extension,
                    "html" | "htm" | "xml" | "vue" | "svelte" | "md" | "markdown" | "sql"
                ) && !(ch == '\''
                    && matches!(
                        extension,
                        "sh" | "bash" | "toml" | "yaml" | "yml" | "ps1" | "powershell"
                    ));
                self.quote = Some((terminator, escapes, triple || ch == '`'));
            } else {
                index += ch.len_utf8();
            }
        }
        if self
            .quote
            .as_ref()
            .is_some_and(|(_, _, multiline)| !multiline)
        {
            self.ambiguous = true;
        }
        ranges
    }
}

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
    ticket_attr_regex: Regex,                      // [ticket=KEY]
    ticket_key_regex: Regex,                       // DEMO-123 style
    include_ext: Option<Vec<String>>,              // lowercase extensions without dot
    exclude_ext: Option<Vec<String>>,              // lowercase extensions without dot
    signal_words: Vec<String>,                     // lowercase tokens like todo, fixme
    custom_ticket_key_regexes: Option<Vec<Regex>>, // configured ticket key patterns
    enable_ticket_words: bool,                     // treat ticket keys as triggers
    modified_only: bool,                           // limit scan to git-modified files
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
        let (ticket_attr_regex, ticket_key_regex) = Self::build_ticket_regexes();

        Self {
            path,
            last_scan: Vec::new(),
            signal_regex,
            uuid_extract_regex,
            simple_extract_regex,
            include_ext: None,
            exclude_ext: None,
            signal_words: default_words,
            ticket_attr_regex,
            ticket_key_regex,
            custom_ticket_key_regexes: None,
            enable_ticket_words: false,
            modified_only: false,
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
}

impl Scanner {
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

    pub fn with_modified_only(mut self, on: bool) -> Self {
        self.modified_only = on;
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

    /// Configure ticket detection patterns and whether ticket keys should act as signal words
    pub fn with_ticket_detection(
        mut self,
        patterns: Option<&[String]>,
        enable_words: bool,
    ) -> Self {
        self.enable_ticket_words = enable_words;
        if let Some(list) = patterns
            && !list.is_empty()
        {
            let mut compiled: Vec<Regex> = Vec::new();
            for p in list {
                if let Ok(re) = Regex::new(p) {
                    compiled.push(re);
                }
            }
            if !compiled.is_empty() {
                self.custom_ticket_key_regexes = Some(compiled);
            }
        }
        self
    }

    fn build_ticket_regexes() -> (Regex, Regex) {
        // [ticket=DEMO-123] or [ticket = DEMO-123]; keys may contain hyphenated
        // prefixes (ABC-OPS-12) consistent with the canonical ID grammar.
        let ticket_attr_regex =
            Regex::new(r"(?i)\[\s*ticket\s*=\s*([A-Z][A-Z0-9]+(?:-[A-Z0-9]+)*-\d+)\s*\]")
                .unwrap_or_else(|_| Regex::new(r"\[ticket=([A-Z]+-\d+)\]").unwrap());
        // Generic key like DEMO-123; capture the FULL hyphenated key
        // (ABC-OPS-12, ABC-OPS-12-3) instead of the trailing slice (OPS-12).
        let ticket_key_regex = Regex::new(r"\b([A-Z][A-Z0-9]+(?:-[A-Z0-9]+)*-\d+)\b")
            .unwrap_or_else(|_| Regex::new(r"([A-Z]+-\d+)").unwrap());
        (ticket_attr_regex, ticket_key_regex)
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
        if Self::get_comment_token(ext).is_some() {
            return true;
        }
        let (open, close) = Self::block_tokens_for(ext);
        open.is_some() && close.is_some()
    }

    fn collect_candidate_files(&self, dir_path: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();

        // If modified_only is enabled and we are inside a git repo, restrict to modified/renamed files
        if self.modified_only
            && let Some(repo_root) = crate::utils::git::find_repo_root(dir_path)
        {
            let modified = Self::git_modified_files(&repo_root);
            for p in modified {
                // Keep only files under dir_path and supported extensions
                let abs = repo_root.join(&p);
                if abs.starts_with(dir_path)
                    && let Some(ext) = abs.extension().and_then(|e| e.to_str())
                {
                    let ext_lc = ext.to_ascii_lowercase();
                    if Self::is_supported_ext(&ext_lc) {
                        files.push(abs);
                    }
                }
            }
            return files;
        }

        // Build walker that honors .lotarignore (root and nested) with fallback to .gitignore
        // when no root .lotarignore exists. If a root .lotarignore is present, gitignore fallback
        // is disabled to match project semantics.
        let mut builder = WalkBuilder::new(dir_path);
        builder.hidden(true); // skip hidden by default

        // Always discover nested .lotarignore files
        builder.add_custom_ignore_filename(".lotarignore");

        let root_lotar = dir_path.join(".lotarignore");
        if root_lotar.exists() {
            // Use .lotarignore rules only (no gitignore fallback)
            builder.ignore(false);
            builder.git_ignore(false);
            builder.git_global(false);
            builder.git_exclude(false);
            // Ensure root .lotarignore is loaded even if discovery misses it for any reason
            builder.add_ignore(root_lotar);
        } else {
            // Fallback to git ignore files
            builder.ignore(true); // .ignore
            builder.git_ignore(true); // .gitignore
            builder.git_global(true);
            builder.git_exclude(true);
            // Explicitly add root .gitignore so it works even without a .git repo
            let root_gitignore = dir_path.join(".gitignore");
            if root_gitignore.exists() {
                builder.add_ignore(root_gitignore);
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
                    if let Some(ref excludes) = self.exclude_ext
                        && excludes.iter().any(|e| e == &ext_lc)
                    {
                        continue;
                    }
                    if let Some(ref includes) = self.include_ext
                        && !includes.iter().any(|e| e == &ext_lc)
                    {
                        continue;
                    }
                    files.push(path);
                }
            }
        }

        files
    }

    /// Return paths from `git status --porcelain` that are modified/renamed/added/deleted
    fn git_modified_files(repo_root: &Path) -> Vec<PathBuf> {
        // Prefer parsing porcelain output via invoking git; avoid adding a dependency.
        // If git not available or fails, return empty to fall back to full scan.
        use std::process::Command;
        let out = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("status")
            .arg("--porcelain")
            .output();
        let output = match out {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            _ => return Vec::new(),
        };
        let mut files = Vec::new();
        for line in output.lines() {
            if line.len() < 4 {
                continue;
            }
            // Format: XY <path> or R <old> -> <new>
            let status = &line[..2];
            // Handle renames with "R" in either position
            if status.contains('R') {
                if let Some(pos) = line.find(" -> ") {
                    let new_path = &line[pos + 4..];
                    files.push(PathBuf::from(new_path.trim()));
                }
                continue;
            }
            // Else take path after 3rd char
            let path = line[3..].trim();
            if !path.is_empty() {
                files.push(PathBuf::from(path));
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
        if let Ok(file_contents) = fs::read_to_string(file_path)
            && let Some(extension) = file_path.extension()
            && let Some(ext_str) = extension.to_str()
        {
            let ext_str = ext_str.to_ascii_lowercase();

            let mut lexer = CommentLexer::default();
            for (line_number, raw_line) in file_contents.lines().enumerate() {
                for range in lexer.ranges(raw_line, &ext_str) {
                    self.process_comment_line(
                        file_path,
                        references,
                        line_number + 1,
                        &raw_line[range],
                    );
                }
            }
        }
    }

    fn process_comment_line(
        &self,
        file_path: &Path,
        references: &mut Vec<Reference>,
        line_number_1based: usize,
        line: &str,
    ) {
        // Only proceed when a signal word is present. Bare ticket keys do not trigger scanning
        // (they are handled as mentions by the scan handler if enabled).
        if !self.signal_regex.is_match(line) {
            return;
        }

        // Strip common leading block-comment adornments: *, /**, */ etc.
        let trimmed = line.trim().trim_start_matches('*').trim();

        // Prefer explicit UUID in parens after signal word, then [ticket=...], then generic DEMO-123 pattern
        let (mut uuid, title) = if let Some(c) = self.uuid_extract_regex.captures(trimmed) {
            let uuid = c.get(1).map_or(String::new(), |m| m.as_str().to_string());
            let title = c
                .get(2)
                .map_or(String::new(), |m| m.as_str().trim().to_string());
            (uuid, title)
        } else if let Some(c) = self.ticket_attr_regex.captures(trimmed) {
            let uuid = c.get(1).map_or(String::new(), |m| m.as_str().to_string());
            let title = self
                .simple_extract_regex
                .captures(trimmed)
                .and_then(|cap| cap.get(1).map(|m| m.as_str().trim().to_string()))
                .unwrap_or_default();
            (uuid, title)
        } else if let Some(c) = self.simple_extract_regex.captures(trimmed) {
            let title = c
                .get(1)
                .map_or(String::new(), |m| m.as_str().trim().to_string());
            // Try to find a generic key within the line
            if let Some(k) = self.ticket_key_regex.captures(trimmed) {
                let uuid = k.get(1).map_or(String::new(), |m| m.as_str().to_string());
                (uuid, title)
            } else {
                (String::new(), title)
            }
        } else {
            (String::new(), String::new())
        };

        // Fallback: if uuid is still empty (e.g., bare key without signal words), try any configured ticket patterns
        if uuid.is_empty()
            && let Some(k) = self.extract_ticket_key_from_line(trimmed)
        {
            uuid = k;
        }

        let reference = Reference {
            file_path: file_path.to_path_buf(),
            line_number: line_number_1based,
            title,
            uuid,
            annotation: trimmed.to_string(),
            code_block: String::new(),
            comment_block: String::new(),
        };
        references.push(reference);
    }

    fn block_tokens_for(ext: &str) -> (Option<&'static str>, Option<&'static str>) {
        // Provide block comment tokens per language family
        match ext {
            // C-style block comments
            "rs" | "rust" | "c" | "h" | "hpp" | "cpp" | "cc" | "cxx" | "js" | "ts" | "jsx"
            | "tsx" | "java" | "cs" | "scala" | "kotlin" | "go" | "dart" | "swift" | "groovy"
            | "css" | "scss" | "less" => (Some("/*"), Some("*/")),
            // HTML/XML/Markdown style block comments
            "html" | "htm" | "xml" | "vue" | "svelte" | "md" | "markdown" => {
                (Some("<!--"), Some("-->"))
            }
            // Others: no block comments
            _ => (None, None),
        }
    }

    /// Locate the matched comment, excluding source before and after block comments.
    pub fn matched_comment_range(&self, line: &str) -> Option<std::ops::Range<usize>> {
        self.matched_source_comment_range(&self.path, line, 1)
    }

    pub fn matched_source_comment_range(
        &self,
        path: &Path,
        contents: &str,
        line_number: usize,
    ) -> Option<std::ops::Range<usize>> {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let mut lexer = CommentLexer::default();
        for (index, line) in contents.lines().enumerate() {
            let ranges = lexer.ranges(line, &extension);
            if index + 1 == line_number {
                return ranges.into_iter().find(|range| {
                    let comment = line[range.clone()]
                        .trim_start_matches(|c: char| c.is_whitespace() || c == '*');
                    self.signal_regex
                        .find(comment)
                        .is_some_and(|matched| matched.start() == 0)
                });
            }
        }
        None
    }

    pub fn suggest_source_insertion(
        &self,
        path: &Path,
        contents: &str,
        line_number: usize,
        key: &str,
        strip: bool,
    ) -> Option<String> {
        let line = contents.lines().nth(line_number.checked_sub(1)?)?;
        let range = self.matched_source_comment_range(path, contents, line_number)?;
        if strip {
            let stripped = crate::utils::scan::strip_bracket_attributes(&line[range.clone()]);
            let next_range = range.start..range.start + stripped.len();
            let line = format!("{}{}{}", &line[..range.start], stripped, &line[range.end..]);
            self.suggest_insertion_in_range(&line, key, next_range)
        } else {
            self.suggest_insertion_in_range(line, key, range)
        }
    }

    pub fn strip_scan_attributes(&self, line: &str) -> String {
        let Some(range) = self.matched_comment_range(line) else {
            return line.to_string();
        };
        format!(
            "{}{}{}",
            &line[..range.start],
            crate::utils::scan::strip_bracket_attributes(&line[range.clone()]),
            &line[range.end..]
        )
    }

    /// Suggest an idempotent insertion of ` (KEY)` right after the first signal word on the line.
    /// Returns Some(edited_line) when an insertion is proposed, or None if not applicable
    /// (no signal word found or the key already exists on the line).
    pub fn suggest_insertion_for_line(&self, line: &str, key: &str) -> Option<String> {
        self.suggest_insertion_in_range(line, key, self.matched_comment_range(line)?)
    }

    fn suggest_insertion_in_range(
        &self,
        line: &str,
        key: &str,
        range: std::ops::Range<usize>,
    ) -> Option<String> {
        if key.is_empty() {
            return None;
        }
        if line[range.clone()].contains(&format!("({})", key)) {
            return None; // idempotence: already present
        }
        // Find the first signal word, but only insert if it's the first token in the comment text.
        if let Some(m) = self.signal_regex.find_at(line, range.start) {
            let comment = &line[range.clone()];
            // Compute the absolute index of the first non-decorative char in the comment
            let trimmed = comment.trim_start_matches(|c: char| c.is_whitespace() || c == '*');
            let offset = comment.len() - trimmed.len();
            let first_token_abs = range.start + offset;
            // Only insert if the match begins exactly at the first token in the comment
            if m.start() != first_token_abs {
                return None;
            }
            let end = m.end();
            let before = &line[..end];
            let after = &line[end..];

            // Determine insertion point: before a following ':' or '-' (with optional spaces),
            // otherwise directly after the signal word.
            let after_trimmed = after;
            let mut insert_at_end = true;
            let mut split_idx = 0usize;
            // pattern: ^\s*([:-])
            for (i, ch) in after_trimmed.char_indices() {
                if ch.is_whitespace() {
                    continue;
                }
                if ch == ':' || ch == '-' {
                    insert_at_end = false;
                    split_idx = i; // insert before this punctuation
                }
                break;
            }

            let insertion = format!(" ({})", key);
            let edited = if insert_at_end {
                format!("{}{}{}", before, insertion, after)
            } else {
                let (left, right) = after_trimmed.split_at(split_idx);
                format!("{}{}{}{}", before, insertion, left, right)
            };

            Some(edited)
        } else {
            None
        }
    }

    /// Extract an existing ticket key from the given line, if present.
    /// Order: [ticket=KEY] takes precedence, then generic KEY like DEMO-123.
    pub fn extract_ticket_key_from_line(&self, line: &str) -> Option<String> {
        if let Some(c) = self.ticket_attr_regex.captures(line) {
            return c.get(1).map(|m| m.as_str().to_string());
        }
        if let Some(c) = self.ticket_key_regex.captures(line) {
            return c.get(1).map(|m| m.as_str().to_string());
        }
        if let Some(list) = &self.custom_ticket_key_regexes {
            for re in list {
                if let Some(c) = re.captures(line)
                    && let Some(m) = c.get(1)
                {
                    return Some(m.as_str().to_string());
                }
            }
        }
        None
    }

    fn scan_file_collect(&self, file_path: &Path) -> Vec<Reference> {
        let mut refs = Vec::new();
        self.scan_file(file_path, &mut refs);
        refs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_regexes_compile_with_unicode_features() {
        let words = vec!["todo".to_string(), "fixme".to_string()];
        let (signal, uuid, simple) = Scanner::build_signal_regexes(&words);
        assert!(signal.is_match("TODO something"));
        assert!(uuid.is_match("TODO (ABC-123): rest"));
        assert!(simple.is_match("TODO: rest"));
    }

    #[test]
    fn ticket_key_extraction_captures_full_hyphenated_prefix() {
        let s = Scanner::new(PathBuf::from("."));
        // Hyphenated prefixes capture the FULL key, not the OPS-12 slice.
        assert_eq!(
            s.extract_ticket_key_from_line("// TODO fix [ticket=ABC-OPS-12]"),
            Some("ABC-OPS-12".to_string())
        );
        assert_eq!(
            s.extract_ticket_key_from_line("// TODO: tracked as ABC-OPS-12 later"),
            Some("ABC-OPS-12".to_string())
        );
        // Multi-segment prefixes keep earlier numeric segments.
        assert_eq!(
            s.extract_ticket_key_from_line("ABC-OPS-12-3"),
            Some("ABC-OPS-12-3".to_string())
        );
        // Classic single-segment keys unchanged.
        assert_eq!(
            s.extract_ticket_key_from_line("DEMO-123"),
            Some("DEMO-123".to_string())
        );
    }

    #[test]
    fn insertion_skips_example_comments_not_starting_with_signal() {
        let s = Scanner::new(PathBuf::from("."));
        let line =
            "signal_words: Vec<String>,                     // lowercase tokens like todo, fixme";
        let edited = s.suggest_insertion_for_line(line, "DEMO-1");
        // Should not insert into 'like todo' example
        assert!(edited.is_none());
    }
}
