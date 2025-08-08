use crate::api_server;
use crate::cli::ServeArgs;
use crate::cli::handlers::CommandHandler;
use crate::output::OutputRenderer;
use crate::routes;
use crate::web_server;
use crate::workspace::TasksDirectoryResolver;

/// Handler for serve command
pub struct ServeHandler;

impl CommandHandler for ServeHandler {
    type Args = ServeArgs;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        _project: Option<&str>,
        _resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let port = args.port.unwrap_or(8080);
        let host = args.host;

        println!(
            "{}",
            renderer.render_success("Starting LoTaR web server...")
        );
        println!("   Host: {}", host);
        println!("   Port: {}", port);
        println!("   URL: http://{}:{}", host, port);

        if args.open {
            // Open browser automatically
            let url = format!("http://{}:{}", host, port);
            if let Err(e) = open_browser(&url) {
                eprintln!(
                    "{}",
                    renderer.render_warning(&format!("Failed to open browser: {}", e))
                );
                println!("   Please navigate to {} manually", url);
            }
        }

        eprintln!(
            "{}",
            renderer.render_warning("Press Ctrl+C to stop the server")
        );

        let mut api_server = api_server::ApiServer::new();
        routes::initialize(&mut api_server);
        web_server::serve(&api_server, port);

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
