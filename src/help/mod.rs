use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use include_dir::{Dir, DirEntry, include_dir};
use regex::Regex;
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
        let help_file = format!("{}.md", command);

        if let Some(file) = self.find_help_file(&help_file) {
            let content = file
                .contents_utf8()
                .ok_or_else(|| format!("Help file '{}' is not valid UTF-8", help_file))?;

            match self.renderer.format {
                OutputFormat::Json => Ok(serde_json::json!({
                    "command": command,
                    "help": content,
                    "format": "markdown"
                })
                .to_string()),
                _ => {
                    // Render Markdown with terminal hyperlinks (OSC 8) by default.
                    // Links remain readable text if the terminal doesn't support OSC 8.
                    let base_dir = format!("{}/docs/help", ROOT_DIR);
                    Ok(self.render_with_hyperlinks(content, &base_dir))
                }
            }
        } else {
            Err(format!("No help available for command '{}'", command))
        }
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

    #[allow(dead_code)]
    #[allow(clippy::only_used_in_recursion)]
    fn collect_help_files(&self, dir: &Dir<'_>, files: &mut Vec<String>) {
        for entry in dir.entries() {
            match entry {
                DirEntry::File(file) => {
                    if let Some(name) = file.path().file_name() {
                        if let Some(name_str) = name.to_str() {
                            if name_str.ends_with(".md") {
                                files.push(name_str.to_string());
                            }
                        }
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
        if let Some(file) = self.find_help_file(filename) {
            if let Some(content) = file.contents_utf8() {
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

// inline tests moved to tests/help_module_unit_test.rs
