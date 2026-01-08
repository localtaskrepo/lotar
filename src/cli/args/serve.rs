use clap::Args;

#[derive(Args)]
pub struct ServeArgs {
    /// Port to serve on (use `--port` or `-p`)
    #[arg(long = "port", value_name = "PORT", default_value_t = 8080)]
    pub port: u16,

    /// Host to bind to
    #[arg(long, default_value = "localhost")]
    pub host: String,

    /// Open browser automatically
    #[arg(long)]
    pub open: bool,

    /// Path to a directory containing custom web UI assets.
    /// When set, the server serves files from this directory instead of the bundled UI.
    /// Falls back to embedded assets if a requested file is not found.
    #[arg(long, value_name = "PATH", env = "LOTAR_WEB_UI_PATH")]
    pub web_ui_path: Option<String>,

    /// Force serving only embedded UI assets, ignoring any external web_ui_path.
    /// Useful for testing that the bundled UI works correctly.
    #[arg(
        long,
        env = "LOTAR_WEB_UI_EMBEDDED",
        value_parser = clap::builder::BoolishValueParser::new(),
        default_value_t = false
    )]
    pub web_ui_embedded: bool,
}
