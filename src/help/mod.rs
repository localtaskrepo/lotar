use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::io::IsTerminal;
use std::sync::LazyLock;

// docs/help, embedded gzip-compressed by build.rs (DEV-221).
include!(concat!(env!("OUT_DIR"), "/help_docs.rs"));

/// The text of a help file (path relative to docs/help), decompressed.
fn help_doc(path: &str) -> Option<String> {
    use std::io::Read;
    let (_, compressed) = HELP_DOCS.iter().find(|(name, _)| *name == path)?;
    let mut text = String::new();
    flate2::read::GzDecoder::new(*compressed).read_to_string(&mut text).ok()?;
    Some(text)
}
const ROOT_DIR: &str = env!("CARGO_MANIFEST_DIR");
// Default to version-pinned docs on GitHub (e.g., v0.3.0); override with LOTAR_DOCS_BASE_URL
static DEFAULT_DOCS_BASE_URL: LazyLock<String> = LazyLock::new(|| format!("https://github.com/localtaskrepo/lotar/blob/v{}/", env!("CARGO_PKG_VERSION")));
static HELP_ALIASES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("sprint", "sprints"),
        ("release", "releases"),
        ("init", "config"),
        ("show", "task"),
        ("view", "task"),
        ("edit", "task"),
        ("update", "task"),
        ("pull", "sync"),
        ("push", "sync"),
        ("ls", "list"),
        ("tasks", "task"),
    ])
});

pub struct HelpSystem {
    renderer: OutputRenderer,
}

impl HelpSystem {
    pub fn new(format: OutputFormat, _verbose: bool) -> Self {
        // Help rendering uses minimal logs; default Warn log level (no banners)
        Self { renderer: OutputRenderer::new(format, LogLevel::Warn) }
    }

    pub fn show_command_help(&self, command: &str) -> Result<String, String> {
        // `lotar help topics` lists every page (DEV-284).
        if command == "topics" {
            return self.list_available_help();
        }
        if let Some(file) = self.fetch_help_file(command) {
            return self.render_help_file(command, &file);
        }

        if let Some(alias) = HELP_ALIASES.get(command)
            && let Some(file) = self.fetch_help_file(alias)
        {
            return self.render_help_file(command, &file);
        }

        Err(format!("No help available for command '{}'", command))
    }

    pub fn show_global_help(&self) -> Result<String, String> {
        self.show_command_help("main")
    }

    /// List all available help topics
    pub fn list_available_help(&self) -> Result<String, String> {
        let mut help_files = Vec::new();

        self.collect_help_files(&mut help_files);
        help_files.sort();
        help_files.retain(|file| file != "main.md" && file != "index.md");

        if help_files.is_empty() {
            return Ok(self.renderer.render_warning("No help files found"));
        }

        match self.renderer.format {
            OutputFormat::Json => Ok(serde_json::json!({
                "available_help": help_files
            })
            .to_string()),
            _ => {
                let mut output = String::from("Available Help Topics:\n\n");
                for file in help_files {
                    let command = file.replace(".md", "");
                    let description = self.extract_description(&file).unwrap_or_else(|| String::from("No description available"));
                    let _ = writeln!(output, "  {} - {}", command, description);
                }
                Ok(output)
            }
        }
    }

    fn find_help_file(&self, filename: &str) -> Option<String> {
        help_doc(filename)
    }

    fn fetch_help_file(&self, topic: &str) -> Option<String> {
        let help_file = format!("{}.md", topic);
        self.find_help_file(&help_file)
    }

    fn render_help_file(&self, command: &str, content: &str) -> Result<String, String> {
        match self.renderer.format {
            OutputFormat::Json => Ok(serde_json::json!({
                "command": command,
                "help": content,
                "format": "markdown"
            })
            .to_string()),
            _ => {
                // Render Markdown to ANSI/Plain text depending on TTY & NO_COLOR.
                let base_dir = format!("{}/docs/help", ROOT_DIR);
                Ok(self.render_markdown(content, &base_dir))
            }
        }
    }

    fn collect_help_files(&self, files: &mut Vec<String>) {
        for (path, _) in HELP_DOCS {
            let name = path.rsplit('/').next().unwrap_or(path);
            if name.ends_with(".md") {
                files.push(name.to_string());
            }
        }
    }

    fn extract_description(&self, filename: &str) -> Option<String> {
        if let Some(content) = self.find_help_file(filename) {
            // Extract first line after # header as description
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('#') {
                    continue;
                }
                if !line.is_empty() {
                    return Some(line.to_string());
                }
            }
        }
        None
    }

    /// Render help Markdown for the terminal. Links become OSC 8 hyperlinks
    /// only in the ANSI style, while walking the parsed events (DEV-284:
    /// inserting them into the raw Markdown broke on punctuation and code
    /// spans, and leaked escapes into plain or piped output).
    fn render_markdown(&self, markdown: &str, base_dir: &str) -> String {
        // Choose style based on TTY/NO_COLOR: ANSI when TTY and NO_COLOR not set, else plain
        let style = match std::env::var("LOTAR_HELP_STYLE").ok().as_deref() {
            Some("ansi") => HelpStyle::Ansi,
            Some("plain") => HelpStyle::Plain,
            _ => {
                let no_color = std::env::var("NO_COLOR").is_ok();
                let is_tty = std::io::stdout().is_terminal();
                if !no_color && is_tty { HelpStyle::Ansi } else { HelpStyle::Plain }
            }
        };

        let mut out = String::with_capacity(markdown.len() + 64);
        let mut list_level: usize = 0;
        // Next number per open list; None for bullet lists.
        let mut list_numbers: Vec<Option<u64>> = Vec::new();
        let mut open_links: Vec<bool> = Vec::new();
        let parser = Parser::new_ext(markdown, pulldown_cmark::Options::empty());
        let mut in_code_block = false;

        for event in parser {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    // Ensure a blank line before headings (except at very start)
                    if !out.is_empty() && !out.ends_with("\n\n") {
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push('\n');
                    }
                    let (pre, post) = match (style, level) {
                        (HelpStyle::Ansi, HeadingLevel::H1) => ("\x1b[1m\x1b[4m", "\x1b[0m\n"),
                        (HelpStyle::Ansi, HeadingLevel::H2) => ("\x1b[1m", "\x1b[0m\n"),
                        (HelpStyle::Ansi, _) => ("\x1b[1m", "\x1b[0m\n"),
                        (HelpStyle::Plain, _) => ("", "\n"),
                    };
                    out.push_str(pre);
                    // We'll close with post on End(Heading)
                    // Store marker by pushing post at End
                    STYLE_STACK.with(|s| s.borrow_mut().push(post.to_string()));
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some(post) = STYLE_STACK.with(|s| s.borrow_mut().pop()) {
                        out.push_str(&post);
                    } else {
                        out.push('\n');
                    }
                    // Ensure a blank line after headings
                    if !out.ends_with("\n\n") {
                        out.push('\n');
                    }
                }
                Event::Start(Tag::List(start)) => {
                    list_level += 1;
                    list_numbers.push(start);
                }
                Event::End(TagEnd::List(_)) => {
                    list_level = list_level.saturating_sub(1);
                    list_numbers.pop();
                }
                Event::Start(Tag::Item) => {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(&"  ".repeat(list_level.saturating_sub(1)));
                    // Numbered lists keep their numbers; the text refers
                    // to steps by number.
                    match list_numbers.last_mut() {
                        Some(Some(number)) => {
                            let _ = write!(out, "{}. ", number);
                            *number += 1;
                        }
                        _ => out.push_str("• "),
                    }
                }
                Event::Start(Tag::Link { dest_url, .. }) => {
                    let link = style == HelpStyle::Ansi && !dest_url.starts_with('#');
                    if link {
                        let url = self.resolve_link_target(&dest_url, base_dir);
                        let _ = write!(out, "\x1b]8;;{}\x1b\\", url);
                    }
                    open_links.push(link);
                }
                Event::End(TagEnd::Link) => {
                    if open_links.pop().unwrap_or(false) {
                        out.push_str("\x1b]8;;\x1b\\");
                    }
                }
                // `<PREFIX>`, `<ref>` and similar placeholders are text.
                Event::InlineHtml(html) => out.push_str(&html),
                Event::End(TagEnd::Item) => {}
                Event::Start(Tag::CodeBlock(_)) => {
                    in_code_block = true;
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    // Add a blank line after code blocks for readability
                    if !out.ends_with("\n\n") {
                        out.push('\n');
                    }
                }
                Event::Start(Tag::BlockQuote(_)) if !out.ends_with('\n') => {
                    out.push('\n');
                }
                Event::End(TagEnd::BlockQuote(_)) if !out.ends_with('\n') => {
                    out.push('\n');
                }
                Event::Text(text) => {
                    if in_code_block {
                        for line in text.lines() {
                            match style {
                                HelpStyle::Ansi => {
                                    let _ = writeln!(out, "    \x1b[2m{}\x1b[0m", line);
                                }
                                HelpStyle::Plain => {
                                    out.push_str("    ");
                                    out.push_str(line);
                                    out.push('\n');
                                }
                            }
                        }
                    } else {
                        out.push_str(&text);
                    }
                }
                Event::Code(inline) => match style {
                    HelpStyle::Ansi => {
                        let _ = write!(out, "\x1b[2m`{}`\x1b[0m", inline);
                    }
                    HelpStyle::Plain => {
                        out.push('`');
                        out.push_str(&inline);
                        out.push('`');
                    }
                },
                Event::SoftBreak | Event::HardBreak => out.push('\n'),
                Event::Rule => {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    let line = "────";
                    out.push_str(line);
                    out.push('\n');
                }
                Event::Html(html) => {
                    out.push_str(&html);
                }
                Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph) if !out.ends_with('\n') => {
                    out.push('\n');
                }
                _ => {}
            }
        }
        out
    }

    fn resolve_link_target(&self, target: &str, base_dir: &str) -> String {
        use std::path::{Path, PathBuf};

        // If target is an absolute URL or file, return as-is.
        if target.starts_with("http://") || target.starts_with("https://") || target.starts_with("file://") || target.starts_with("mailto:") {
            return target.to_string();
        }

        // Determine the base URL for docs (env override or default to GitHub).
        let base_url = std::env::var("LOTAR_DOCS_BASE_URL").ok().unwrap_or_else(|| DEFAULT_DOCS_BASE_URL.clone());
        let base_url = if base_url.ends_with('/') { base_url } else { format!("{}/", base_url) };

        // Compute a repo-relative path for the target.
        // Strategy:
        //  - If target starts with "./" or "../", resolve relative to base_dir
        //  - Else if it's an absolute filesystem path, keep it as file://
        //  - Else try base_dir/target first; if it doesn't exist, fall back to ROOT_DIR/target
        //  - If resolution fails, use the raw target as repo-relative

        // Absolute filesystem path: keep as local file URL (useful in dev)
        if target.starts_with('/') {
            let abs = PathBuf::from(target);
            let abs = abs.canonicalize().unwrap_or(abs);
            return format!("file://{}", abs.to_string_lossy());
        }

        let candidate_paths: [PathBuf; 2] = [Path::new(base_dir).join(target), Path::new(ROOT_DIR).join(target)];

        // Pick the first existing candidate; else use the first as best-effort
        let chosen = candidate_paths.iter().find(|p| p.exists()).cloned().unwrap_or_else(|| candidate_paths[0].clone());

        // Try to compute repo-relative by stripping ROOT_DIR
        let repo_rel = chosen.strip_prefix(ROOT_DIR).map(|p| p.to_path_buf()).unwrap_or_else(|_| PathBuf::from(target));

        // Normalize path separators to '/'
        let repo_rel = repo_rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");

        format!("{}{}", base_url, repo_rel)
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum HelpStyle {
    Ansi,
    Plain,
}

thread_local! {
    static STYLE_STACK: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

// inline tests moved to tests/help_module_unit_test.rs
