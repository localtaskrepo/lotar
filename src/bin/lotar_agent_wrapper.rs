use std::env;
use std::process::{Command, Stdio};

#[derive(Debug, Default)]
struct WrapperConfig {
    job_id: Option<String>,
    ticket_id: Option<String>,
    runner: Option<String>,
    child_program: String,
    child_args: Vec<String>,
}

fn parse_args(args: &[String]) -> Result<WrapperConfig, String> {
    let mut cfg = WrapperConfig::default();
    let mut idx = 0;
    let mut seen_separator = false;
    while idx < args.len() {
        let arg = &args[idx];
        if arg == "--" {
            seen_separator = true;
            idx += 1;
            break;
        }
        match arg.as_str() {
            "--job-id" => {
                let value = args.get(idx + 1).ok_or("Missing value for --job-id")?;
                cfg.job_id = Some(value.to_string());
                idx += 2;
            }
            "--ticket-id" => {
                let value = args.get(idx + 1).ok_or("Missing value for --ticket-id")?;
                cfg.ticket_id = Some(value.to_string());
                idx += 2;
            }
            "--runner" => {
                let value = args.get(idx + 1).ok_or("Missing value for --runner")?;
                cfg.runner = Some(value.to_string());
                idx += 2;
            }
            _ => {
                return Err(format!("Unexpected argument '{arg}'"));
            }
        }
    }

    if !seen_separator {
        return Err("Missing '--' separator before child command".to_string());
    }

    if idx >= args.len() {
        return Err("Missing child command after '--'".to_string());
    }

    cfg.child_program = args[idx].clone();
    cfg.child_args = args[idx + 1..].to_vec();
    Ok(cfg)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let cfg = match parse_args(&args) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("lotar-agent-wrapper: {err}");
            std::process::exit(2);
        }
    };

    let mut cmd = Command::new(&cfg.child_program);
    cmd.args(&cfg.child_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(job_id) = cfg.job_id.as_ref() {
        cmd.env("LOTAR_JOB_ID", job_id);
    }
    if let Some(ticket_id) = cfg.ticket_id.as_ref() {
        cmd.env("LOTAR_TICKET_ID", ticket_id);
    }
    if let Some(runner) = cfg.runner.as_ref() {
        cmd.env("LOTAR_RUNNER", runner);
    }
    cmd.env("LOTAR_WRAPPER", "1");

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            eprintln!("lotar-agent-wrapper: failed to spawn child: {err}");
            std::process::exit(1);
        }
    };

    let status = match child.wait() {
        Ok(status) => status,
        Err(err) => {
            eprintln!("lotar-agent-wrapper: failed to wait for child: {err}");
            std::process::exit(1);
        }
    };

    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(test)]
mod tests {
    use super::parse_args;

    #[test]
    fn parse_wrapper_args() {
        let args = vec![
            "--job-id".to_string(),
            "job-1".to_string(),
            "--ticket-id".to_string(),
            "TEST-1".to_string(),
            "--runner".to_string(),
            "codex".to_string(),
            "--".to_string(),
            "codex".to_string(),
            "exec".to_string(),
            "--json".to_string(),
        ];
        let cfg = parse_args(&args).expect("parse args");
        assert_eq!(cfg.job_id.as_deref(), Some("job-1"));
        assert_eq!(cfg.ticket_id.as_deref(), Some("TEST-1"));
        assert_eq!(cfg.runner.as_deref(), Some("codex"));
        assert_eq!(cfg.child_program, "codex");
        assert_eq!(cfg.child_args, vec!["exec", "--json"]);
    }
}
