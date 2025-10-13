use std::borrow::Cow;

/// Rewrite CLI arguments to support additional convenience forms.
///
/// Currently this normalizes the `serve` subcommand so that legacy positional
/// port values and the `-p` short flag are mapped to the canonical `--port`
/// option that Clap natively understands. Global flags and other serve options
/// are preserved untouched.
pub fn normalize_args(raw_args: &[String]) -> Result<Vec<String>, String> {
    let Some(serve_idx) = find_serve_index(raw_args) else {
        return Ok(raw_args.to_vec());
    };

    let mut normalized = Vec::with_capacity(raw_args.len() + 2);
    normalized.extend(raw_args[..=serve_idx].iter().cloned());

    let mut i = serve_idx + 1;
    let mut port_seen = false;
    let mut pending_value: Option<Cow<'static, str>> = None;

    while i < raw_args.len() {
        let token = &raw_args[i];

        if pending_value.take().is_some() {
            normalized.push(token.clone());
            i += 1;
            continue;
        }

        if token == "--" {
            normalized.push(token.clone());
            i += 1;
            continue;
        }

        // Explicit long forms for port (always handled, even if previously seen)
        if token == "--port" {
            normalized.push(token.clone());
            pending_value = Some(Cow::Borrowed("--port"));
            port_seen = true;
            i += 1;
            continue;
        }
        if token.starts_with("--port=") {
            normalized.push(token.clone());
            port_seen = true;
            i += 1;
            continue;
        }

        // Short alias (-p, -pXXXX)
        if token == "-p" {
            normalized.push("--port".to_string());
            pending_value = Some(Cow::Borrowed("--port"));
            port_seen = true;
            i += 1;
            continue;
        }
        if token.starts_with("-p") && token.len() > 2 {
            normalized.push(format!("--port={}", &token[2..]));
            port_seen = true;
            i += 1;
            continue;
        }

        // First bare positional after `serve` is treated as port
        if !port_seen && !token.starts_with('-') {
            normalized.push("--port".to_string());
            normalized.push(token.clone());
            port_seen = true;
            i += 1;
            continue;
        }

        // Options that require a value (without =)
        match token.as_str() {
            "--host" => {
                normalized.push(token.clone());
                pending_value = Some(Cow::Borrowed("--host"));
                i += 1;
                continue;
            }
            "--tasks-dir" => {
                normalized.push(token.clone());
                pending_value = Some(Cow::Borrowed("--tasks-dir"));
                i += 1;
                continue;
            }
            "--format" => {
                normalized.push(token.clone());
                pending_value = Some(Cow::Borrowed("--format"));
                i += 1;
                continue;
            }
            "--log-level" => {
                normalized.push(token.clone());
                pending_value = Some(Cow::Borrowed("--log-level"));
                i += 1;
                continue;
            }
            "--project" => {
                normalized.push(token.clone());
                pending_value = Some(Cow::Borrowed("--project"));
                i += 1;
                continue;
            }
            "-f" => {
                normalized.push(token.clone());
                pending_value = Some(Cow::Borrowed("-f"));
                i += 1;
                continue;
            }
            "-l" => {
                normalized.push(token.clone());
                pending_value = Some(Cow::Borrowed("-l"));
                i += 1;
                continue;
            }
            _ => {
                normalized.push(token.clone());
                i += 1;
            }
        }
    }

    if let Some(name) = pending_value {
        return Err(format!("Option '{}' requires a value", name));
    }

    Ok(normalized)
}

fn find_serve_index(raw_args: &[String]) -> Option<usize> {
    let mut idx = 1; // skip binary name
    let mut pending_global: Option<&'static str> = None;

    while idx < raw_args.len() {
        let token = &raw_args[idx];

        if pending_global.take().is_some() {
            idx += 1;
            continue;
        }

        // Handle global options that consume the next token
        match token.as_str() {
            "--format" => {
                pending_global = Some("--format");
                idx += 1;
                continue;
            }
            "--tasks-dir" => {
                pending_global = Some("--tasks-dir");
                idx += 1;
                continue;
            }
            "--log-level" => {
                pending_global = Some("--log-level");
                idx += 1;
                continue;
            }
            "--project" => {
                pending_global = Some("--project");
                idx += 1;
                continue;
            }
            "-f" => {
                pending_global = Some("-f");
                idx += 1;
                continue;
            }
            "-l" => {
                pending_global = Some("-l");
                idx += 1;
                continue;
            }
            "-p" => {
                pending_global = Some("-p");
                idx += 1;
                continue;
            }
            _ => {}
        }

        // Global flags with inline assignments (no extra token)
        if token.starts_with("--format=")
            || token.starts_with("--tasks-dir=")
            || token.starts_with("--log-level=")
            || token.starts_with("--project=")
            || (token.starts_with("-f") && token.len() > 2)
            || (token.starts_with("-l") && token.len() > 2)
            || (token.starts_with("-p") && token.len() > 2 && token != "-p")
        {
            idx += 1;
            continue;
        }

        if token == "--" {
            idx += 1;
            break;
        }

        if token.starts_with('-') {
            idx += 1;
            continue;
        }

        return if token == "serve" { Some(idx) } else { None };
    }

    // If we hit "--" terminate and search for serve afterwards as positional argument
    while idx < raw_args.len() {
        let token = &raw_args[idx];
        if token == "serve" {
            return Some(idx);
        }
        idx += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::normalize_args;

    fn to_vec(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn leaves_non_serve_commands_untouched() {
        let args = to_vec(&["lotar", "add", "task"]);
        let normalized = normalize_args(&args).unwrap();
        assert_eq!(normalized, args);
    }

    #[test]
    fn leaves_add_arguments_named_serve_untouched() {
        let args = to_vec(&["lotar", "add", "serve", "feature"]);
        let normalized = normalize_args(&args).unwrap();
        assert_eq!(normalized, args);
    }

    #[test]
    fn converts_positional_port_to_long_option() {
        let args = to_vec(&["lotar", "serve", "9090"]);
        let normalized = normalize_args(&args).unwrap();
        assert_eq!(normalized, to_vec(&["lotar", "serve", "--port", "9090"]));
    }

    #[test]
    fn converts_short_port_flag() {
        let args = to_vec(&["lotar", "serve", "-p", "4500"]);
        let normalized = normalize_args(&args).unwrap();
        assert_eq!(normalized, to_vec(&["lotar", "serve", "--port", "4500"]));
    }

    #[test]
    fn keeps_long_port_flag() {
        let args = to_vec(&["lotar", "serve", "--port", "3200"]);
        let normalized = normalize_args(&args).unwrap();
        assert_eq!(normalized, args);
    }

    #[test]
    fn respects_host_value_when_port_missing() {
        let args = to_vec(&["lotar", "serve", "--host", "0.0.0.0"]);
        let normalized = normalize_args(&args).unwrap();
        assert_eq!(normalized, to_vec(&["lotar", "serve", "--host", "0.0.0.0"]));
    }

    #[test]
    fn transforms_with_global_flags() {
        let args = to_vec(&["lotar", "--format", "json", "serve", "-p", "7000", "--open"]);
        let normalized = normalize_args(&args).unwrap();
        assert_eq!(
            normalized,
            to_vec(&[
                "lotar", "--format", "json", "serve", "--port", "7000", "--open"
            ])
        );
    }

    #[test]
    fn errors_when_short_port_missing_value() {
        let args = to_vec(&["lotar", "serve", "-p"]);
        let err = normalize_args(&args).unwrap_err();
        assert!(err.contains("--port"));
    }
}
