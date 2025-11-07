use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use include_dir::{Dir, DirEntry, include_dir};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use regex::Regex;
use std::collections::HashMap;
use std::io::IsTerminal;
use std::sync::LazyLock;

static HELP_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/docs/help");
const ROOT_DIR: &str = env!("CARGO_MANIFEST_DIR");
// Default to version-pinned docs on GitHub (e.g., v0.3.0); override with LOTAR_DOCS_BASE_URL
static DEFAULT_DOCS_BASE_URL: LazyLock<String> = LazyLock::new(|| {
    format!(
        "https://github.com/localtaskrepo/lotar/blob/v{}/",
        env!("CARGO_PKG_VERSION")
    )
});
static HELP_ALIASES: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| HashMap::from([("sprint", "sprints")]));

pub struct HelpSystem {
    renderer: OutputRenderer,
}

impl HelpSystem {
    pub fn new(format: OutputFormat, _verbose: bool) -> Self {
        // Help rendering uses minimal logs; default Warn log level (no banners)
        Self {
            renderer: OutputRenderer::new(format, LogLevel::Warn),
        }
    }

    pub fn show_command_help(&self, command: &str) -> Result<String, String> {
        if let Some(file) = self.fetch_help_file(command) {
            return self.render_help_file(command, command, file);
        }

        if let Some(alias) = HELP_ALIASES.get(command)
            && let Some(file) = self.fetch_help_file(alias)
        {
            return self.render_help_file(command, alias, file);
        }

        Err(format!("No help available for command '{}'", command))
    }

    pub fn show_global_help(&self) -> Result<String, String> {
        self.show_command_help("main")
    }

    /// List all available help topics
    #[allow(dead_code)]
    pub fn list_available_help(&self) -> Result<String, String> {
        let mut help_files = Vec::new();

        self.collect_help_files(&HELP_DIR, &mut help_files);

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
                    let description = self
                        .extract_description(&file)
                        .unwrap_or_else(|| String::from("No description available"));
                    output.push_str(&format!("  {} - {}\n", command, description));
                }
                Ok(output)
            }
        }
    }

    fn find_help_file(&self, filename: &str) -> Option<include_dir::File<'_>> {
        HELP_DIR.get_file(filename).cloned()
    }

    fn fetch_help_file(&self, topic: &str) -> Option<include_dir::File<'_>> {
        let help_file = format!("{}.md", topic);
        self.find_help_file(&help_file)
    }

    fn render_help_file(
        &self,
        command: &str,
        topic: &str,
        file: include_dir::File<'_>,
    ) -> Result<String, String> {
        let content = file
            .contents_utf8()
            .ok_or_else(|| format!("Help file '{}.md' is not valid UTF-8", topic))?;

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
                let with_links = self.render_with_hyperlinks(content, &base_dir);
                Ok(self.render_markdown(&with_links))
            }
        }
    }

    #[allow(dead_code)]
    #[allow(clippy::only_used_in_recursion)]
    fn collect_help_files(&self, dir: &Dir<'_>, files: &mut Vec<String>) {
        for entry in dir.entries() {
            match entry {
                DirEntry::File(file) => {
                    if let Some(name) = file.path().file_name()
                        && let Some(name_str) = name.to_str()
                        && name_str.ends_with(".md")
                    {
                        files.push(name_str.to_string());
                    }
                }
                DirEntry::Dir(subdir) => {
                    self.collect_help_files(subdir, files);
                }
            }
        }
    }

    #[allow(dead_code)]
    fn extract_description(&self, filename: &str) -> Option<String> {
        if let Some(file) = self.find_help_file(filename)
            && let Some(content) = file.contents_utf8()
        {
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

    fn render_with_hyperlinks(&self, markdown: &str, base_dir: &str) -> String {
        // Convert Markdown links [text](target) into OSC 8 clickable hyperlinks.
        // Preserve original link text and do not change JSON output.
        let re = match Regex::new(r"\[([^\]]+)\]\(([^)]+)\)") {
            Ok(r) => r,
            Err(_) => return markdown.to_string(),
        };
        let mut out = String::with_capacity(markdown.len() + 64);
        let mut last = 0usize;
        for caps in re.captures_iter(markdown) {
            if let (Some(m), Some(text), Some(target)) = (
                caps.get(0),
                caps.get(1).map(|m| m.as_str()),
                caps.get(2).map(|m| m.as_str()),
            ) {
                // Push preceding text
                out.push_str(&markdown[last..m.start()]);
                // Skip OSC-8 for in-page anchors (e.g., #section)
                if target.starts_with('#') {
                    out.push_str(text);
                    last = m.end();
                    continue;
                }

                let url = self.resolve_link_target(target, base_dir);
                // OSC 8: ESC ] 8 ;; URL ST, text, ESC ] 8 ;; ST
                out.push_str("\x1b]8;;");
                out.push_str(&url);
                out.push_str("\x1b\\");
                out.push_str(text);
                out.push_str("\x1b]8;;\x1b\\");
                last = m.end();
            }
        }
        out.push_str(&markdown[last..]);
        out
    }

    fn render_markdown(&self, markdown: &str) -> String {
        // Choose style based on TTY/NO_COLOR: ANSI when TTY and NO_COLOR not set, else plain
        let style = match std::env::var("LOTAR_HELP_STYLE").ok().as_deref() {
            Some("ansi") => HelpStyle::Ansi,
            Some("plain") => HelpStyle::Plain,
            _ => {
                let no_color = std::env::var("NO_COLOR").is_ok();
                let is_tty = std::io::stdout().is_terminal();
                if !no_color && is_tty {
                    HelpStyle::Ansi
                } else {
                    HelpStyle::Plain
                }
            }
        };

        let mut out = String::with_capacity(markdown.len() + 64);
        let mut list_level: usize = 0;
        let parser = Parser::new_ext(markdown, pulldown_cmark::Options::empty());
        let mut in_code_block = false;

        for event in parser {
            match event {
                Event::Start(Tag::Heading(level, _, _)) => {
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
                Event::End(Tag::Heading(_, _, _)) => {
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
                Event::Start(Tag::List(_)) => {
                    list_level += 1;
                }
                Event::End(Tag::List(_)) => {
                    list_level = list_level.saturating_sub(1);
                }
                Event::Start(Tag::Item) => {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(&"  ".repeat(list_level.saturating_sub(1)));
                    out.push_str("• ");
                }
                Event::End(Tag::Item) => {}
                Event::Start(Tag::CodeBlock(_)) => {
                    in_code_block = true;
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                Event::End(Tag::CodeBlock(_)) => {
                    in_code_block = false;
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    // Add a blank line after code blocks for readability
                    if !out.ends_with("\n\n") {
                        out.push('\n');
                    }
                }
                Event::Start(Tag::BlockQuote) => {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                Event::End(Tag::BlockQuote) => {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                Event::Text(text) => {
                    if in_code_block {
                        for line in text.lines() {
                            match style {
                                HelpStyle::Ansi => {
                                    out.push_str(&format!("    \x1b[2m{}\x1b[0m\n", line))
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
                    HelpStyle::Ansi => out.push_str(&format!("\x1b[2m`{}`\x1b[0m", inline)),
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
                Event::Start(Tag::Paragraph) | Event::End(Tag::Paragraph) => {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn resolve_link_target(&self, target: &str, base_dir: &str) -> String {
        use std::path::{Path, PathBuf};

        // If target is an absolute URL or file, return as-is.
        if target.starts_with("http://")
            || target.starts_with("https://")
            || target.starts_with("file://")
            || target.starts_with("mailto:")
        {
            return target.to_string();
        }

        // Determine the base URL for docs (env override or default to GitHub).
        let base_url = std::env::var("LOTAR_DOCS_BASE_URL")
            .ok()
            .unwrap_or_else(|| DEFAULT_DOCS_BASE_URL.clone());
        let base_url = if base_url.ends_with('/') {
            base_url
        } else {
            format!("{}/", base_url)
        };

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

        let candidate_paths: [PathBuf; 2] = [
            Path::new(base_dir).join(target),
            Path::new(ROOT_DIR).join(target),
        ];

        // Pick the first existing candidate; else use the first as best-effort
        let chosen = candidate_paths
            .iter()
            .find(|p| p.exists())
            .cloned()
            .unwrap_or_else(|| candidate_paths[0].clone());

        // Try to compute repo-relative by stripping ROOT_DIR
        let repo_rel = chosen
            .strip_prefix(ROOT_DIR)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| PathBuf::from(target));

        // Normalize path separators to '/'
        let repo_rel = repo_rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

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
