use clap::Args;

#[derive(Args)]
pub struct ServeArgs {
    /// Port to serve on
    #[arg(default_value = "8080")]
    pub port: Option<u16>,

    /// Host to bind to
    #[arg(long, default_value = "localhost")]
    pub host: String,

    /// Open browser automatically
    #[arg(long)]
    pub open: bool,
}
