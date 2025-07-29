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

const FILE_TYPES: &[(&str, &str, &str, &str)] = &[
    ("py", "#", "'''", "'''"),
    ("go", "//", "/*", "*/"),
    ("rs", "//", "/*", "*/"),
    ("java", "//", "/*", "*/"),
    ("js", "//", "/*", "*/"),
    ("cpp", "//", "/*", "*/"),
    ("cs", "//", "/*", "*/"),
    ("scala", "//", "/*", "*/"),
    ("hs", "--", "{-", "-}"),
    ("groovy", "//", "/*", "*/"),
    ("rb", "#", "=begin", "=end"),
    ("c", "//", "/*", "*/"),
    ("h", "//", "/*", "*/"),
    ("swift", "//", "/*", "*/"),
    ("php", "//", "/*", "*/"),
    ("elixir", "#", "'''", "'''"),
    ("erlang", "%", "%%", "%%"),
    ("clojure", ";", ";;", ";;"),
    ("elm", "--", "{-", "-}"),
    ("kotlin", "//", "/*", "*/"),
    ("dart", "//", "/*", "*/"),
    ("haxe", "//", "{", "}"),
    ("rust", "//", "/*", "*/"),
    ("fsharp", "//", "(*", "*)"),
    ("ocaml", "(*", "(*", "*)"),
    ("haskell", "--", "{-", "-}"),
    ("pascal", "//", "{", "}"),
    ("perl", "#", "=pod", "=cut"),
    ("r", "#", "'''", "'''"),
    ("powershell", "#", "<#", "#>"),
    ("nim", "#", "\"\"", "\"\""),
    ("scheme", ";;", "#|", "|#"),
    ("commonlisp", ";;", "", ""),
    ("racket", ";", "#|", "|#"),
    ("c", "//", "/", "/"),
    ("c++", "//", "/", "/"),
    ("c#", "//", "/", "/"),
    ("java", "//", "/", "/"),
    ("javascript", "//", "/", "/"),
    ("python", "#", "\"\"\"", "\"\"\""),
    ("go", "//", "/", "/"),
    ("rust", "//", "/", "/"),
    ("scala", "//", "/", "/"),
    ("haskell", "--", "{-", "-}"),
    ("groovy", "//", "/", "/"),
    ("ruby", "#", "=begin", "=end"),
];

use std::fs;
use uuid::Uuid;
use regex::Regex;
use std::collections::HashMap;
use std::clone::Clone;
use std::path::{PathBuf};

// Define a struct to hold the reference information
#[derive(Clone, Debug)]
pub struct Reference {
    pub id: String,
    pub file_path: PathBuf,
    pub line_number: usize,
    pub title: String,  // Changed from annotation to title for test compatibility
    pub uuid: String,   // Added uuid field for test compatibility
    pub annotation: String,
    pub code_block: String,
    pub comment_block: String,
}

struct Search {
    start_comment: Regex,
    end_comment: Regex,
    todo: Regex,
}

pub struct Scanner {
    path: PathBuf,
    last_scan: Vec<Reference>,
    search: HashMap<String, Search>,
}

impl Scanner {
    pub fn new(path: PathBuf) -> Scanner {
        let mut search = HashMap::new();
        for (file_type, start_comment, _, _) in FILE_TYPES {
            search.insert(file_type.to_string(), Search {
                start_comment: Regex::new(start_comment).unwrap(),
                end_comment: Regex::new("\n").unwrap(),
                todo: Regex::new(r"@todo").unwrap(),
            });
        }
        Scanner {
            path,
            last_scan: vec![],
            search,
        }
    }

    pub fn scan(&mut self) -> Vec<Reference> {
        let mut references = vec![];

        // Recursively search through the directory for source code files
        for entry in fs::read_dir(&self.path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let extension = match path.extension() {
                Some(ext) => ext.to_str().unwrap(),
                None => continue,
            };

            if path.is_dir() {
                references.append(&mut self.scan());
            } else {
                let search = match self.search.get(extension) {
                    Some(search) => search,
                    None => continue,
                };

                let path_buf = path.clone();
                let file_contents = fs::read_to_string(path).unwrap();
                let (mut in_block, mut comment_block, mut code_block) = (false, String::new(), String::new());

                for line in file_contents.lines() {
                    if search.start_comment.is_match(line) {
                        in_block = true;
                    } else if search.end_comment.is_match(line) {
                        in_block = false;
                    }
                    if in_block {
                        comment_block.push_str(line);
                        comment_block.push_str("\n");
                    } else {
                        code_block.push_str(line);
                        code_block.push_str("\n");
                    }
                }
                for (line_number, line) in code_block.lines().enumerate() {
                    if search.todo.is_match(line) {
                        let reference = Reference {
                            id: Uuid::new_v4().to_string(),
                            file_path: path_buf.clone(),
                            line_number,
                            title: line.to_string(),  // Changed from annotation to title for test compatibility
                            uuid: Uuid::new_v4().to_string(),   // Added uuid field for test compatibility
                            annotation: line.to_string(),
                            code_block: code_block.clone(),
                            comment_block: comment_block.clone(),
                        };
                        references.push(reference);
                    }
                }
            }
        }

        // Save the current scan results for later comparison
        self.last_scan = references.clone();

        references
    }

    pub fn detect_changes(&self) -> Vec<Reference> {
        let mut new_references = vec![];
        for reference in &self.last_scan {
            if !self.contains(&reference) {
                new_references.push(reference.clone());
            }
        }
        new_references
    }

    fn contains(&self, reference: &Reference) -> bool {
        for r in &self.last_scan {
            if r.id == reference.id {
                return true;
            }
        }
        false
    }
}
