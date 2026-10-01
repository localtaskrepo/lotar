use clap::Args;

#[derive(Args)]
pub struct ServeArgs {
    /// Port to serve on (`--port`, `-p`, or a bare positional). When omitted,
    /// the port comes from the resolved config (`LOTAR_PORT` /
    /// `LOTAR_SERVER_PORT`, then the config file's `server.port`); with
    /// nothing configured anywhere, the built-in default 8080 is preferred
    /// and an OS-assigned port is used instead when it is already taken.
    /// Any explicitly requested port — including `8080` or `0` — is honored
    /// exactly: `0` binds an OS-assigned ephemeral port and a busy port
    /// fails instead of moving.
    #[arg(short = 'p', long = "port", value_name = "PORT")]
    pub port: Option<u16>,

    /// Accept-and-ignore shadow of the global `--project` option.
    ///
    /// `serve` is the deliberate exception where `-p` selects the port, so
    /// the global project argument (whose own `-p` short would otherwise be
    /// propagated into this subcommand and clash with the port short) is
    /// scoped out here by declaring an argument with the same id. The long
    /// form stays accepted for compatibility and, like the global form
    /// before it, has no effect on the server.
    #[arg(long = "project", hide = true)]
    pub project: Option<String>,

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
