/**
GPT Instructions:
Write a module in Rust called Scanner that implements the following features:
* A function called scan() - returns a vector of Todo objects (you will need to define it)
* This function should scan the current directory for files with source code
* It should scan each file and look for TODOs (The search token should be configurable, but default to "TODO")
* It should look for TODO token only in comments (not in strings)
* To identify comments the module should have a mapping of file extensions to comment tokens
* For example, for Rust the comment token is "//" and for C++ it is "//" or "/* */"
* We should support all known comment styles for all programming languages we support, which could be more than 2
* Implement as many comment styles as you can, for as many programming languages as you can
* The constructor will be given a path to the directory to scan
* The scan should be recursive, so it should scan all subdirectories as well
* The scan should be case insensitive for the TODO token
* Todo objects should be clonable, serializable/deserializable (in yaml), and printable
* Favor a configuration object that has the different comment mappings for the different file types for the languages we're trying to search for
* The TODO object should have the following fields:
* - The file path
* - The line number for the TODO
* - The line number for code related to the TODO (if any)
* - The line text after the TODO token as title (only the first sentence if it's a multi-sentence TODO)
* - If it's a multi-sentence TODO, the line text after the first sentence as description
* - A generated UUID for the TODO
* We want to support a special TODO token: when the TODO token has "(<id>)" behind it that should be used as the UUID for the TODO. For example: "TODO (1234-5678-90ab-cdef) - This is a TODO" should have the UUID 1234-5678-90ab-cdef
* Otherwise we just want to generate a new UUID for the TODO
* We will need to keep track of existing TODOs as we don't want to create new ID's for existing TODOs
* There should be two ways of doing this, for which we can use 2 different functions:
* - Either by fuzzy matching the line number and the line text of a TODO with known TODOs
* - Or by using the UUID in the TODO token. If the UUID is not found, we should create a new TODO and modify the source file to add the UUID to the TODO token using the format above
* To be able to persist the information you will need to ask an external module called Store whether a TODO with a given ID already exists or not. For that you use the Store module which has 1 public function: bool, exists(todo: TODO) -> bool. Add a comment to this function that it implements both looking up by id first and if that doesn't work does fuzzy matching
* Use a static configuration object to configure the search tokens for the different file types. Use Regex to simplify code where possible
* Don't assume the comment mapping is passed to the scanner. The scanner should be able to figure out the right mapping on its own based on the file extensions associated with different programming languages. Be sure to implement a one to many relationship between a file and the comment that filetype supports. If you want to optimized you can also you use a many to many relationship if want to group multiple file extension types together. You may optimize even more with a regex.
* Write concise, readable, and maintainable code, but skip any tests for now
*/

// Optimized file types configuration - removed duplicates and organized by comment style
const FILE_TYPES: &[(&str, &str)] = &[
    // Single-line comment languages with //
    ("rs", "//"), ("rust", "//"), ("go", "//"), ("java", "//"), ("js", "//"), ("ts", "//"),
    ("cpp", "//"), ("cc", "//"), ("cxx", "//"), ("c", "//"), ("h", "//"), ("hpp", "//"),
    ("cs", "//"), ("scala", "//"), ("groovy", "//"), ("swift", "//"), ("php", "//"),
    ("kotlin", "//"), ("dart", "//"), ("fsharp", "//"),

    // Hash-based comment languages
    ("py", "#"), ("rb", "#"), ("sh", "#"), ("bash", "#"), ("perl", "#"), ("r", "#"),
    ("elixir", "#"), ("powershell", "#"), ("nim", "#"), ("yaml", "#"), ("yml", "#"),

    // Double-dash comment languages
    ("hs", "--"), ("haskell", "--"), ("elm", "--"), ("pascal", "--"),

    // Semicolon comment languages
    ("clojure", ";"), ("scheme", ";"), ("commonlisp", ";"), ("racket", ";"),

    // Percent comment languages
    ("erlang", "%"), ("matlab", "%"), ("tex", "%"),
];

use std::fs;
use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Reference {
    pub file_path: PathBuf,
    pub line_number: usize,
    pub title: String,
    pub uuid: String,
    pub annotation: String,
    pub code_block: String,
    pub comment_block: String,
}

pub struct Scanner {
    path: PathBuf,
    last_scan: Vec<Reference>,
}

impl Scanner {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            last_scan: Vec::new(),
        }
    }

    pub fn scan(&mut self) -> Vec<Reference> {
        let mut references = vec![];

        // Recursively search through the directory for source code files
        self.scan_directory(&self.path.clone(), &mut references);

        // Save the current scan results for later comparison
        self.last_scan = references.clone();
        references
    }

    fn scan_directory(&self, dir_path: &PathBuf, references: &mut Vec<Reference>) {
        if let Ok(entries) = fs::read_dir(dir_path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();

                    if path.is_dir() {
                        // Recursive directory scanning
                        self.scan_directory(&path, references);
                    } else if let Some(extension) = path.extension() {
                        if let Some(ext_str) = extension.to_str() {
                            // Check for file extension match in our supported types
                            if self.is_supported_file_type(ext_str) {
                                self.scan_file(&path, references);
                            }
                        }
                    }
                }
            }
        }
    }

    fn is_supported_file_type(&self, extension: &str) -> bool {
        // Check if this extension is supported
        for (file_type, _) in FILE_TYPES {
            if file_type == &extension {
                return true;
            }
        }
        false
    }

    fn scan_file(&self, file_path: &PathBuf, references: &mut Vec<Reference>) {
        if let Ok(file_contents) = fs::read_to_string(file_path) {
            if let Some(extension) = file_path.extension() {
                if let Some(ext_str) = extension.to_str() {
                    // Find the correct comment pattern for this file type
                    if let Some((_, start_comment)) = FILE_TYPES.iter()
                        .find(|(file_type, _)| file_type == &ext_str) {

                        let comment_regex = Regex::new(start_comment).unwrap();
                        let todo_regex = Regex::new(r"(?i)todo").unwrap();
                        let uuid_extract_regex = Regex::new(r"(?i)todo\s*\(([^)]+)\)\s*:?\s*(.*)").unwrap();
                        let simple_todo_regex = Regex::new(r"(?i)todo\s*:?\s*(.*)").unwrap();

                        // Process each line to find TODOs in comments
                        for (line_number, line) in file_contents.lines().enumerate() {
                            // Check if line contains a comment and TODO (case insensitive)
                            if comment_regex.is_match(line) && todo_regex.is_match(line) {
                                let (uuid, title) = if let Some(uuid_captures) = uuid_extract_regex.captures(line) {
                                    // Extract UUID and title from UUID format
                                    let uuid = uuid_captures.get(1).map_or(String::new(), |m| m.as_str().to_string());
                                    let title = uuid_captures.get(2).map_or(String::new(), |m| m.as_str().trim().to_string());
                                    (uuid, title)
                                } else if let Some(simple_captures) = simple_todo_regex.captures(line) {
                                    // Extract title from simple TODO format
                                    let title = simple_captures.get(1).map_or(String::new(), |m| m.as_str().trim().to_string());
                                    (String::new(), title)
                                } else {
                                    // Fallback: extract whatever comes after TODO
                                    if let Some(todo_pos) = line.to_lowercase().find("todo") {
                                        let after_todo = &line[todo_pos + 4..].trim();
                                        (String::new(), after_todo.to_string())
                                    } else {
                                        (String::new(), String::new())
                                    }
                                };

                                let reference = Reference {
                                    file_path: file_path.clone(),
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

    pub fn extract_todos_from_content(&self, content: &str, file_path: &Path) -> Vec<Reference> {
        let mut references = vec![];

        // Create the regex patterns needed for this method
        let uuid_extract_regex = Regex::new(r"(?i)todo\s*\(([^)]+)\)\s*:?\s*(.*)").unwrap();
        let simple_todo_regex = Regex::new(r"(?i)todo\s*:?\s*(.*)").unwrap();

        for (line_number, line) in content.lines().enumerate() {
            // Check if line contains TODO (case insensitive)
            if line.to_lowercase().contains("todo") {
                let (uuid, title) = if let Some(uuid_captures) = uuid_extract_regex.captures(line) {
                    // Extract UUID and title from UUID format
                    let uuid = uuid_captures.get(1).map_or(String::new(), |m| m.as_str().to_string());
                    let title = uuid_captures.get(2).map_or(String::new(), |m| m.as_str().trim().to_string());
                    (uuid, title)
                } else if let Some(simple_captures) = simple_todo_regex.captures(line) {
                    // Extract title from simple TODO format
                    let title = simple_captures.get(1).map_or(String::new(), |m| m.as_str().trim().to_string());
                    (String::new(), title)
                } else {
                    (String::new(), String::new())
                };

                let reference = Reference {
                    file_path: file_path.to_path_buf(),
                    line_number: line_number + 1, // Convert to 1-based line number
                    title,
                    uuid,
                    annotation: line.trim().to_string(),
                    code_block: String::new(),
                    comment_block: String::new(),
                };
                references.push(reference);
            }
        }

        references
    }
}
