//! `lotar usage`: each provider's newest usage-limit reading from agent
//! jobs (DEV-862), for people and for scripts (`--format json`).

use crate::output::{OutputFormat, OutputRenderer};
use crate::services::provider_usage::{self, UsageReading};

pub struct UsageHandler;

impl UsageHandler {
    pub fn execute(renderer: &OutputRenderer) -> Result<(), String> {
        let file = provider_usage::file_path();
        let readings = provider_usage::readings()?;
        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({ "file": file, "readings": readings }));
            return Ok(());
        }
        if readings.is_empty() {
            renderer.emit_info(format!("No usage readings yet: an agent job saves one when its agent reports a usage limit ({}).", file.display()));
            return Ok(());
        }
        let rows: Vec<[String; 7]> = readings.iter().map(row).collect();
        let header = ["PROVIDER", "ACCOUNT", "WINDOW", "USED", "STATUS", "RESETS AT", "READ AT"].map(str::to_string);
        let widths: Vec<usize> =
            (0..header.len()).map(|column| rows.iter().chain([&header]).map(|row| row[column].chars().count()).max().unwrap_or(0)).collect();
        for line in [&header].into_iter().chain(&rows) {
            let cells: Vec<String> = line.iter().zip(&widths).map(|(cell, width)| format!("{cell:<width$}")).collect();
            renderer.emit_raw_stdout(cells.join("  ").trim_end());
        }
        Ok(())
    }
}

fn row(reading: &UsageReading) -> [String; 7] {
    let or_dash = |value: Option<&str>| value.unwrap_or("-").to_string();
    [
        reading.provider.clone(),
        or_dash(reading.account.as_deref()),
        or_dash(reading.window.as_deref()),
        reading.used_percent.map_or_else(|| "-".to_string(), |used| format!("{used}%")),
        reading.status.clone(),
        or_dash(reading.resets_at.as_deref()),
        reading.read_at.clone(),
    ]
}
