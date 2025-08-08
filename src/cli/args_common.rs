// Shared helpers for CLI argument parsing

/// Parse a key=value pair into (key, value) strings, trimming whitespace.
/// Used by Clap `value_parser` attributes across subcommands.
pub fn parse_key_value(s: &str) -> Result<(String, String), String> {
    if let Some((key, value)) = s.split_once('=') {
        Ok((key.trim().to_string(), value.trim().to_string()))
    } else {
        Err(format!(
            "Invalid key=value format: '{}'. Expected format: key=value",
            s
        ))
    }
}
