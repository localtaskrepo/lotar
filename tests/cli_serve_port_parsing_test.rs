use clap::Parser;
use lotar::cli::preprocess::normalize_args;
use lotar::cli::{Cli, Commands};

fn parse_cli(args: &[&str]) -> Cli {
    let raw: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let normalized = normalize_args(&raw).expect("args normalize");
    Cli::try_parse_from(normalized).expect("cli parse")
}

#[test]
fn serve_accepts_long_port_flag() {
    let cli = parse_cli(&["lotar", "serve", "--port", "4242"]);
    match cli.command {
        Commands::Serve(serve_args) => {
            assert_eq!(serve_args.port, 4242);
            assert_eq!(serve_args.host, "localhost");
            assert!(!serve_args.open);
        }
        _ => panic!("expected serve command"),
    }
}

#[test]
fn serve_short_p_alias_maps_to_port() {
    let cli = parse_cli(&["lotar", "serve", "-p", "5050"]);
    match cli.command {
        Commands::Serve(serve_args) => {
            assert_eq!(serve_args.port, 5050);
        }
        _ => panic!("expected serve command"),
    }
}

#[test]
fn serve_legacy_positional_port_is_supported() {
    let cli = parse_cli(&["lotar", "serve", "7000"]);
    match cli.command {
        Commands::Serve(serve_args) => assert_eq!(serve_args.port, 7000),
        _ => panic!("expected serve command"),
    }
}

#[test]
fn global_project_short_remains_available_before_serve() {
    let cli = parse_cli(&["lotar", "-p", "web", "serve", "-p", "8081"]);
    assert_eq!(cli.project.as_deref(), Some("web"));
    match cli.command {
        Commands::Serve(serve_args) => assert_eq!(serve_args.port, 8081),
        _ => panic!("expected serve command"),
    }
}
