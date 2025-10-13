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
}
