use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use include_dir::{Dir, DirEntry, include_dir};
use termimad::MadSkin;

static HELP_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/docs/help");

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
                OutputFormat::Markdown => Ok(content.to_owned()),
                _ => {
                    // Render markdown to terminal using termimad
                    let skin = MadSkin::default();
                    Ok(skin.term_text(content).to_string())
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
            OutputFormat::Table => {
                use comfy_table::{ContentArrangement, Table};
                let mut table = Table::new();
                table.set_content_arrangement(ContentArrangement::Dynamic);
                table.set_header(vec!["Command", "Description"]);

                for file in help_files {
                    let command = file.replace(".md", "");
                    let description = self
                        .extract_description(&file)
                        .unwrap_or_else(|| String::from("No description available"));
                    table.add_row(vec![command, description]);
                }

                Ok(format!("Available Help Topics:\n\n{}", table))
            }
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
}

// inline tests moved to tests/help_module_unit_test.rs
