use crate::api_server;
use crate::cli::ServeArgs;
use crate::cli::handlers::CommandHandler;
use crate::config::persistence;
use crate::output::OutputRenderer;
use crate::routes;
use crate::web_server::{self, WebServerConfig};
use crate::workspace::TasksDirectoryResolver;
use std::path::PathBuf;

/// Handler for serve command
pub struct ServeHandler;

impl CommandHandler for ServeHandler {
    type Args = ServeArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        _project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let ServeArgs {
            port,
            host,
            open,
            web_ui_path,
            web_ui_embedded,
        } = args;

        // Resolve web_ui_path: CLI/env first, then fall back to global config
        let effective_web_ui_path = web_ui_path.or_else(|| {
            persistence::load_global_config(Some(&resolver.path))
                .ok()
                .and_then(|cfg| cfg.web_ui_path)
        });

        renderer.log_info(format_args!(
            "serve: host={} port={} open={} web_ui_path={:?} embedded_only={}",
            host, port, open, effective_web_ui_path, web_ui_embedded
        ));

        renderer.emit_success("Starting LoTaR web server...");
        renderer.emit_raw_stdout(format_args!("   Host: {}", host));
        renderer.emit_raw_stdout(format_args!("   Port: {}", port));
        renderer.emit_raw_stdout(format_args!("   URL: http://{}:{}", host, port));

        // Build web server config from CLI args
        let web_config = WebServerConfig {
            web_ui_path: effective_web_ui_path.map(PathBuf::from),
            embedded_only: web_ui_embedded,
        };

        if let Some(ref path) = web_config.web_ui_path {
            if web_config.embedded_only {
                renderer.emit_raw_stdout(format_args!(
                    "   UI: embedded (--web-ui-embedded overrides --web-ui-path)"
                ));
            } else if path.is_dir() {
                renderer.emit_raw_stdout(format_args!("   UI: {} (custom)", path.display()));
            } else {
                renderer.emit_warning(format_args!(
                    "Custom web UI path '{}' not found, using embedded UI",
                    path.display()
                ));
            }
        }

        if open {
            // Open browser automatically
            let url = format!("http://{}:{}", host, port);
            if let Err(e) = open_browser(&url) {
                renderer.emit_warning(format_args!("Failed to open browser: {}", e));
                renderer.emit_raw_stdout(format_args!("   Please navigate to {} manually", url));
            }
        }

        renderer.emit_warning("Press Ctrl+C to stop the server");

        let mut api_server = api_server::ApiServer::new();
        routes::initialize(&mut api_server);
        // Bind to provided host; API and UI served together
        web_server::serve_with_config(&api_server, &host, port, &web_config);

        Ok(())
    }
}

/// Helper function to open browser (cross-platform)
fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(&["/c", "start", url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
