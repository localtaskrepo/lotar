//! The core modules (DEV-498) never call up into the layers built on them,
//! so they can become a crate of their own (DEV-493).

use std::path::{Path, PathBuf};

/// Modules and files under `src/` that form the core.
const CORE: &[&str] =
    &["api_events.rs", "api_types.rs", "config", "errors.rs", "output", "project.rs", "scanner.rs", "storage", "types.rs", "utils", "workspace.rs"];

/// Top-level modules above the core.
const UPPER: &[&str] = &["api_server", "automation", "cli", "help", "mcp", "routes", "services", "web_server"];

fn rust_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            rust_files(&entry.unwrap().path(), out);
        }
    } else if path.extension().is_some_and(|ext| ext == "rs") {
        out.push(path.to_path_buf());
    }
}

fn is_word_end(code: &str, at: usize) -> bool {
    !code[at..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
}

/// Byte offsets and names of upper modules that `code` reaches through
/// `crate::`, also inside grouped imports (`use crate::{cli, storage::{…}}`).
fn upper_references(code: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for module in UPPER {
        let path = format!("crate::{module}");
        found.extend(code.match_indices(&path).filter(|(at, _)| is_word_end(code, at + path.len())).map(|(at, _)| (at, *module)));
    }
    for (start, _) in code.match_indices("crate::{") {
        let mut depth = 0;
        let mut first = true;
        for (offset, c) in code[start + "crate::".len()..].char_indices() {
            let at = start + "crate::".len() + offset;
            match c {
                '{' => {
                    depth += 1;
                    first = true;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                ',' => first = depth == 1,
                c if c.is_whitespace() => {}
                _ if first => {
                    first = false;
                    if depth == 1
                        && let Some(module) = UPPER.iter().find(|module| code[at..].starts_with(**module) && is_word_end(code, at + module.len()))
                    {
                        found.push((at, *module));
                    }
                }
                _ => {}
            }
        }
    }
    found
}

/// `code` with `//` comments blanked out, so offsets keep their lines.
fn without_comments(code: &str) -> String {
    code.lines()
        .map(|line| match line.find("//") {
            Some(at) => format!("{}{}", &line[..at], " ".repeat(line.len() - at)),
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn core_modules_do_not_reference_upper_layers() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    for module in CORE {
        let path = src.join(module);
        assert!(path.exists(), "core module {} is missing", path.display());
        rust_files(&path, &mut files);
    }
    let mut found = Vec::new();
    for file in &files {
        let code = without_comments(&std::fs::read_to_string(file).unwrap());
        for (at, module) in upper_references(&code) {
            let line = code[..at].matches('\n').count() + 1;
            found.push(format!("{}:{line}: crate::{module}", file.strip_prefix(&src).unwrap().display()));
        }
    }
    assert!(found.is_empty(), "core modules reference the layers above them:\n{}", found.join("\n"));
}

#[test]
fn finds_upper_module_paths() {
    let modules = |code: &str| upper_references(code).into_iter().map(|(_, module)| module).collect::<Vec<_>>();
    assert_eq!(modules("crate::services::trust_service::TrustService::rewrite(x)"), ["services"]);
    assert_eq!(modules("use crate::{cli::ScanArgs, storage::Storage};"), ["cli"]);
    assert_eq!(modules("use crate::{\n    storage::{Storage, Task},\n    mcp,\n};"), ["mcp"]);
    assert!(modules("use crate::cli_like::X;").is_empty());
    assert!(modules("use crate::{storage::{services, cli}};").is_empty());
    assert!(modules("crate::config::trust::TrustService").is_empty());
    assert!(modules(&without_comments("// see crate::services::x\nlet a = 1;")).is_empty());
}
