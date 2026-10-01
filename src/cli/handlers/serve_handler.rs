use crate::api_server;
use crate::cli::ServeArgs;
use crate::cli::handlers::CommandHandler;
use crate::config::persistence;
use crate::output::OutputRenderer;
use crate::routes;
use crate::web_server::{self, WebServerConfig};
use crate::workspace::TasksDirectoryResolver;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

/// Built-in serve port used only when no source configured a port at all.
pub(crate) const DEFAULT_SERVE_PORT: u16 = 8080;

/// Where the effective serve port came from. Sources are ranked exactly like
/// the documented config precedence; the winner supplies both the value and
/// the label, so an error never blames a layer the value did not come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortSource {
    /// `--port` / `-p` / positional on the serve command itself.
    CliFlag,
    /// `lotar --config server.port=...` (inline override layer).
    ConfigOverride,
    /// `LOTAR_PORT` / `LOTAR_SERVER_PORT` (via the env override machinery).
    Environment,
    /// `server.port` in the honored home config (`~/.lotar`).
    HomeConfig,
    /// `server.port` in the global config (`.tasks/config.yml`).
    GlobalConfig,
    /// Nothing configured a port anywhere; the built-in default applies.
    ImplicitDefault,
}

impl PortSource {
    /// Human-readable note appended to bind errors naming the requester.
    fn note(self) -> &'static str {
        match self {
            PortSource::CliFlag => " (port requested via --port)",
            PortSource::ConfigOverride => " (port set by --config server.port)",
            PortSource::Environment => " (port set by LOTAR_PORT/LOTAR_SERVER_PORT)",
            PortSource::HomeConfig => " (port set by home config server.port)",
            PortSource::GlobalConfig => " (port set by config server.port)",
            PortSource::ImplicitDefault => "",
        }
    }

    fn label(self) -> &'static str {
        match self {
            PortSource::CliFlag => "cli",
            PortSource::ConfigOverride => "config-override",
            PortSource::Environment => "env",
            PortSource::HomeConfig => "home-config",
            PortSource::GlobalConfig => "config",
            PortSource::ImplicitDefault => "built-in-default",
        }
    }

    /// Only the built-in default may move to an OS-assigned port when busy.
    fn allows_fallback(self) -> bool {
        matches!(self, PortSource::ImplicitDefault)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ServePortSelection {
    pub(crate) port: u16,
    pub(crate) source: PortSource,
}

/// Pure selection over per-layer explicit ports, each carrying presence
/// (not value) metadata. Ranking mirrors the documented precedence:
/// CLI flag > `--config` > env > home config > global config > built-in.
/// Every explicit layer is strict; only the all-absent case is
/// fallback-eligible. No value comparisons: an explicit request for the
/// built-in default number is still an explicit request.
fn select_port(
    cli_flag: Option<u16>,
    config_override: Option<u16>,
    env: Option<u16>,
    home_config: Option<u16>,
    global_config: Option<u16>,
) -> ServePortSelection {
    let (port, source) = if let Some(port) = cli_flag {
        (port, PortSource::CliFlag)
    } else if let Some(port) = config_override {
        (port, PortSource::ConfigOverride)
    } else if let Some(port) = env {
        (port, PortSource::Environment)
    } else if let Some(port) = home_config {
        (port, PortSource::HomeConfig)
    } else if let Some(port) = global_config {
        (port, PortSource::GlobalConfig)
    } else {
        (DEFAULT_SERVE_PORT, PortSource::ImplicitDefault)
    };
    ServePortSelection { port, source }
}

/// Resolve the effective serve port with true layer provenance.
///
/// `--port` wins once. Otherwise the first layer that explicitly configures
/// a port — `--config`, then env, then the home config, then the global
/// config — supplies both the value and the label. Genuine configuration
/// errors (an unparseable config file that might have configured the port,
/// or an invalid `LOTAR_PORT`) are propagated so serve fails before binding
/// instead of silently moving ports; a merely missing optional config is the
/// normal implicit-default case.
fn resolve_serve_port(
    cli_port: Option<u16>,
    tasks_root: &Path,
) -> Result<ServePortSelection, String> {
    if let Some(port) = cli_port {
        return Ok(select_port(Some(port), None, None, None, None));
    }

    let config_override = crate::config::resolution::active_cli_port_override();

    let env_snapshot = crate::config::env_overrides::capture_env_override_snapshot();
    if let Some(err) = env_snapshot
        .report
        .errors
        .iter()
        .find(|err| err.key == "server_port")
    {
        return Err(format!(
            "Invalid port configured in {}: {}",
            err.env_var, err.message
        ));
    }
    let env = env_snapshot
        .report
        .applied_keys
        .contains("server_port")
        .then_some(env_snapshot.resolved.server_port);

    let home_config = persistence::home_config_server_port()?;
    let global_config = persistence::config_file_server_port(
        &crate::utils::paths::global_config_path(tasks_root),
        "global config",
    )?;

    Ok(select_port(
        cli_port,
        config_override,
        env,
        home_config,
        global_config,
    ))
}

// Config-file port provenance (presence + value + error policy) lives in
// `config::persistence` so serve, the generic resolution chain, and source
// labels share one home/env/global chain instead of duplicating raw reads.

/// Host spelling for advertised URLs: a bare IPv6 literal must be
/// bracketed (`http://[::1]:8080/`) so browsers and parsers resolve it;
/// IPv4 and hostnames pass through unchanged. The `Host:` banner line and
/// the bind itself keep the raw user-provided value.
fn url_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{}]", host)
    } else {
        host.to_string()
    }
}

/// A successfully bound serve socket plus the ports that matter for the
/// banner and the fallback notice.
#[derive(Debug)]
pub(crate) struct BoundPort {
    pub(crate) listener: TcpListener,
    pub(crate) preferred_port: u16,
    pub(crate) actual_port: u16,
    pub(crate) fell_back: bool,
}

impl BoundPort {
    fn new(listener: TcpListener, preferred_port: u16, fell_back: bool) -> Result<Self, String> {
        let actual_port = listener
            .local_addr()
            .map(|addr| addr.port())
            .map_err(|err| format!("Failed to inspect the bound socket: {}", err))?;
        Ok(Self {
            listener,
            preferred_port,
            actual_port,
            fell_back,
        })
    }
}

/// Bind the serve socket.
///
/// The tuple form `(host, port)` keeps IPv6 literal hosts bindable. Only an
/// AddrInUse failure of a fallback-eligible (implicit default) preferred
/// port retries once with an OS-assigned port on the same host; every other
/// failure — including permission errors and unresolvable hosts — aborts
/// with the established `Failed to bind` error prefix. No probe/close dance
/// happens here: the successful listener itself is kept and served on.
fn bind_server_socket(
    host: &str,
    preferred_port: u16,
    allow_fallback: bool,
    source_note: &str,
) -> Result<BoundPort, String> {
    match TcpListener::bind((host, preferred_port)) {
        Ok(listener) => BoundPort::new(listener, preferred_port, false),
        Err(err) if allow_fallback && err.kind() == std::io::ErrorKind::AddrInUse => {
            let listener = TcpListener::bind((host, 0)).map_err(|fallback_err| {
                format!(
                    "Failed to bind to {}:0 (fallback after port {} was in use): {}",
                    host, preferred_port, fallback_err
                )
            })?;
            BoundPort::new(listener, preferred_port, true)
        }
        Err(err) => Err(format!(
            "Failed to bind to {}:{}{}: {}",
            host, preferred_port, source_note, err
        )),
    }
}

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
            ..
        } = args;

        // Resolve web_ui_path: CLI/env first, then fall back to global config
        let effective_web_ui_path = web_ui_path.or_else(|| {
            persistence::load_global_config(Some(&resolver.path))
                .ok()
                .and_then(|cfg| cfg.web_ui_path)
        });

        let selection = resolve_serve_port(port, &resolver.path)?;
        renderer.log_info(format_args!(
            "serve: host={} port={} port_source={} open={} web_ui_path={:?} embedded_only={}",
            host,
            selection.port,
            selection.source.label(),
            open,
            effective_web_ui_path,
            web_ui_embedded
        ));

        renderer.emit_success("Starting LoTaR web server...");

        // Bind before advertising readiness so the URL banner only ever
        // appears for a socket this process owns. `--port 0` binds an
        // OS-assigned ephemeral port; the banner then reports the actual
        // bound port. A bind failure (e.g. occupied port) aborts the command
        // with a non-zero exit instead of logging after a "ready" banner.
        // The implicit built-in default port is the single exception: when
        // nothing configured a port and the default is already taken, the
        // server moves once to an OS-assigned port on the same host and says
        // so; every explicitly configured port keeps DEV-76's strict
        // fail-fast semantics.
        let bound = bind_server_socket(
            &host,
            selection.port,
            selection.source.allows_fallback(),
            selection.source.note(),
        )?;
        let actual_port = bound.actual_port;

        if bound.fell_back {
            renderer.emit_warning(format_args!(
                "Default port {} is already in use on {}; serving on port {} instead",
                bound.preferred_port, host, actual_port
            ));
        }

        renderer.emit_raw_stdout(format_args!("   Host: {}", host));
        let port_line = format!("   Port: {}", actual_port);
        if crate::output::stdout_styling_enabled(&renderer.format) {
            let styled = if bound.fell_back {
                crate::output::ansi_bold_highlight(&port_line)
            } else {
                crate::output::ansi_bold(&port_line)
            };
            renderer.emit_raw_stdout(format_args!("{}", styled));
        } else {
            renderer.emit_raw_stdout(format_args!("{}", port_line));
        }
        // The URL line always stays plain: scripts and the smoke harness
        // parse it, so it must never carry ANSI sequences. IPv6 literals are
        // bracketed so the advertised URL is actually navigable.
        let advertised_host = url_host(&host);
        renderer.emit_raw_stdout(format_args!(
            "   URL: http://{}:{}",
            advertised_host, actual_port
        ));

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
            let url = format!("http://{}:{}", advertised_host, actual_port);
            if let Err(e) = open_browser(&url) {
                renderer.emit_warning(format_args!("Failed to open browser: {}", e));
                renderer.emit_raw_stdout(format_args!("   Please navigate to {} manually", url));
            }
        }

        renderer.emit_warning("Press Ctrl+C to stop the server");

        pin_process_tasks_dir(resolver)?;

        let mut api_server = api_server::ApiServer::new();
        routes::initialize(&mut api_server);
        // Serve on the pre-bound listener; API and UI served together
        web_server::serve_listener(&api_server, &bound.listener, &web_config);

        Ok(())
    }
}

fn pin_process_tasks_dir(resolver: &TasksDirectoryResolver) -> Result<(), String> {
    let tasks_dir = resolver.absolute_path()?;
    let tasks_dir_value = tasks_dir.to_string_lossy().into_owned();
    unsafe {
        std::env::set_var("LOTAR_TASKS_DIR", tasks_dir_value);
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn held_ephemeral_port() -> (TcpListener, u16) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("reserve ephemeral port");
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[test]
    fn select_port_ranks_layers_by_documented_precedence() {
        // Arbitrary port numbers: the decision must never depend on the
        // built-in default value appearing (or not) in a layer.
        let cli = select_port(
            Some(11001),
            Some(11002),
            Some(11003),
            Some(11004),
            Some(11005),
        );
        assert_eq!(cli.port, 11001);
        assert_eq!(cli.source, PortSource::CliFlag);

        let over = select_port(None, Some(11002), Some(11003), Some(11004), Some(11005));
        assert_eq!(over.port, 11002);
        assert_eq!(over.source, PortSource::ConfigOverride);

        let env = select_port(None, None, Some(11003), Some(11004), Some(11005));
        assert_eq!(env.port, 11003);
        assert_eq!(env.source, PortSource::Environment);

        let home = select_port(None, None, None, Some(11004), Some(11005));
        assert_eq!(home.port, 11004);
        assert_eq!(home.source, PortSource::HomeConfig);

        let global = select_port(None, None, None, None, Some(11005));
        assert_eq!(global.port, 11005);
        assert_eq!(global.source, PortSource::GlobalConfig);
    }

    #[test]
    fn select_port_explicit_default_number_is_still_strict() {
        // The exact regression from round 1: a --config request for 8080 (or
        // any layer asking for the default number) must not be classified as
        // the implicit default just because the value matches it.
        let over = select_port(None, Some(8080), Some(9999), Some(9998), Some(9997));
        assert_eq!(over.source, PortSource::ConfigOverride);
        assert_eq!(over.port, 8080);
        assert!(!over.source.allows_fallback());

        let env = select_port(None, None, Some(8080), Some(9998), Some(9997));
        assert_eq!(env.source, PortSource::Environment);
        assert_eq!(env.port, 8080);
        assert!(!env.source.allows_fallback());

        let home = select_port(None, None, None, Some(8080), Some(9997));
        assert_eq!(home.source, PortSource::HomeConfig);
        assert!(!home.source.allows_fallback());

        let global = select_port(None, None, None, None, Some(8080));
        assert_eq!(global.source, PortSource::GlobalConfig);
        assert!(!global.source.allows_fallback());
    }

    #[test]
    fn select_port_zero_is_an_explicit_request() {
        let over = select_port(None, Some(0), None, None, None);
        assert_eq!(over.port, 0);
        assert_eq!(over.source, PortSource::ConfigOverride);
        assert!(
            !over.source.allows_fallback(),
            "explicit 0 never falls back"
        );
    }

    #[test]
    fn select_port_all_absent_is_the_only_fallback_case() {
        let implicit = select_port(None, None, None, None, None);
        assert_eq!(implicit.source, PortSource::ImplicitDefault);
        assert_eq!(implicit.port, DEFAULT_SERVE_PORT);
        assert!(implicit.source.allows_fallback());
    }

    #[test]
    fn url_host_brackets_only_bare_ipv6_literals() {
        assert_eq!(url_host("::1"), "[::1]");
        assert_eq!(url_host("fe80::1"), "[fe80::1]");
        assert_eq!(url_host("[::1]"), "[::1]", "already bracketed stays as-is");
        assert_eq!(url_host("localhost"), "localhost");
        assert_eq!(url_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(url_host("0.0.0.0"), "0.0.0.0");
        assert_eq!(url_host("my.host.example"), "my.host.example");
    }

    #[test]
    fn bind_falls_back_to_os_port_only_when_allowed() {
        let (_holder, held) = held_ephemeral_port();
        let bound =
            bind_server_socket("127.0.0.1", held, true, "").expect("fallback bind must succeed");
        assert!(bound.fell_back);
        assert_eq!(bound.preferred_port, held);
        assert_ne!(bound.actual_port, held);
        assert_ne!(bound.actual_port, 0, "OS-assigned port must be concrete");
        // The fallback listener is live and holds its actual port.
        let probe = TcpListener::bind(("127.0.0.1", bound.actual_port));
        assert!(probe.is_err(), "bound listener must own its port");
    }

    #[test]
    fn bind_strict_failure_reports_failed_to_bind_with_note() {
        let (_holder, held) = held_ephemeral_port();
        let err = bind_server_socket(
            "127.0.0.1",
            held,
            false,
            " (port set by --config server.port)",
        )
        .expect_err("strict bind on a held port must fail");
        assert!(err.starts_with("Failed to bind"), "prefix preserved: {err}");
        assert!(err.contains(&held.to_string()), "names the port: {err}");
        assert!(
            err.contains("(port set by --config server.port)"),
            "carries source note: {err}"
        );
    }

    #[test]
    fn bind_free_port_is_exact_when_strict() {
        let (holder, free) = held_ephemeral_port();
        drop(holder);
        let bound = bind_server_socket("127.0.0.1", free, false, "").expect("strict bind");
        assert!(!bound.fell_back);
        assert_eq!(bound.actual_port, free);
    }

    #[test]
    fn bind_explicit_zero_is_strict_not_fallback() {
        let bound = bind_server_socket("127.0.0.1", 0, false, "").expect("port 0 binds");
        assert!(!bound.fell_back);
        assert_ne!(bound.actual_port, 0);
    }

    #[test]
    fn bind_non_addr_in_use_errors_never_fall_back() {
        // An unresolvable host fails for a non-AddrInUse reason; even with
        // fallback allowed the error must surface, not move ports.
        let err = bind_server_socket("lotar-invalid-host.invalid", DEFAULT_SERVE_PORT, true, "")
            .expect_err("invalid host must fail");
        assert!(err.starts_with("Failed to bind"), "prefix preserved: {err}");
    }
}
