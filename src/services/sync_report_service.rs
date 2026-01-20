use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::api_types::{SyncReport, SyncReportListResponse, SyncReportMeta};
use crate::config::types::ResolvedConfig;
use crate::errors::{LoTaRError, LoTaRResult};

#[derive(Default, Debug, Clone)]
pub struct SyncReportListFilter {
    pub project: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

pub struct SyncReportService;

impl SyncReportService {
    pub fn compute_reports_root(tasks_dir: &Path, config: &ResolvedConfig) -> LoTaRResult<PathBuf> {
        let raw = config.sync_reports_dir.trim();
        if raw.is_empty() {
            return Err(LoTaRError::ValidationError(
                "sync_reports_dir cannot be empty".to_string(),
            ));
        }

        let configured = Path::new(raw);
        let root = if configured.is_absolute() {
            configured.to_path_buf()
        } else {
            if configured
                .components()
                .any(|c| matches!(c, Component::ParentDir))
            {
                return Err(LoTaRError::ValidationError(
                    "sync_reports_dir cannot contain '..'".to_string(),
                ));
            }
            tasks_dir.join(configured)
        };
        Ok(root)
    }

    pub fn resolve_reports_root(tasks_dir: &Path, config: &ResolvedConfig) -> LoTaRResult<PathBuf> {
        let root = Self::compute_reports_root(tasks_dir, config)?;
        fs::create_dir_all(&root)?;
        Ok(root)
    }

    pub fn write_report(
        tasks_dir: &Path,
        config: &ResolvedConfig,
        report: &SyncReport,
        write_enabled: bool,
    ) -> LoTaRResult<Option<String>> {
        if !write_enabled {
            return Ok(None);
        }

        let root = Self::resolve_reports_root(tasks_dir, config)?;
        let filename = build_report_filename(report);
        let path = root.join(&filename);

        let payload = serde_yaml::to_string(report).map_err(|err| {
            LoTaRError::SerializationError(format!("Failed to serialize sync report: {}", err))
        })?;
        fs::write(&path, payload)?;
        Ok(Some(filename))
    }

    pub fn list_reports(
        tasks_dir: &Path,
        config: &ResolvedConfig,
        filter: SyncReportListFilter,
    ) -> LoTaRResult<SyncReportListResponse> {
        let root = Self::compute_reports_root(tasks_dir, config)?;
        if !root.exists() {
            return Ok(SyncReportListResponse {
                total: 0,
                limit: filter.limit,
                offset: filter.offset,
                reports: Vec::new(),
            });
        }

        let mut reports = Vec::new();
        for entry in fs::read_dir(&root)? {
            let entry = match entry {
                Ok(value) => value,
                Err(_) => continue,
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext != "json" && ext != "yml" && ext != "yaml" {
                continue;
            }
            let payload = match fs::read_to_string(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let report = match parse_report_payload(&payload) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if let Some(project) = filter.project.as_deref()
                && report.project.as_deref() != Some(project)
            {
                continue;
            }
            let stored_path = path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());
            reports.push(report_to_meta(&report, stored_path));
        }

        reports.sort_by(sort_report_meta);

        let total = reports.len();
        let start = filter.offset.min(total);
        let end = start.saturating_add(filter.limit).min(total);
        let page = if start >= end {
            Vec::new()
        } else {
            reports[start..end].to_vec()
        };

        Ok(SyncReportListResponse {
            total,
            limit: filter.limit,
            offset: filter.offset,
            reports: page,
        })
    }

    pub fn read_report(
        tasks_dir: &Path,
        config: &ResolvedConfig,
        rel_path: &str,
    ) -> LoTaRResult<SyncReport> {
        let root = Self::compute_reports_root(tasks_dir, config)?;
        let path = resolve_report_path(&root, rel_path)?;
        let payload = fs::read_to_string(&path)?;
        parse_report_payload(&payload).map_err(|err| {
            LoTaRError::SerializationError(format!("Invalid report content: {}", err))
        })
    }
}

fn resolve_report_path(root: &Path, rel: &str) -> LoTaRResult<PathBuf> {
    let trimmed = rel.trim();
    if trimmed.is_empty() {
        return Err(LoTaRError::ValidationError(
            "Missing report path".to_string(),
        ));
    }
    let rel_path = Path::new(trimmed);
    if rel_path.is_absolute() {
        return Err(LoTaRError::ValidationError(
            "Invalid report path".to_string(),
        ));
    }
    if rel_path
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(LoTaRError::ValidationError(
            "Invalid report path".to_string(),
        ));
    }

    let root_canon = fs::canonicalize(root)
        .map_err(|e| LoTaRError::ValidationError(format!("Invalid reports root: {}", e)))?;
    let joined = root.join(rel_path);
    let joined_canon = fs::canonicalize(&joined)
        .map_err(|_| LoTaRError::ValidationError("Report not found".to_string()))?;
    if !joined_canon.starts_with(&root_canon) {
        return Err(LoTaRError::ValidationError(
            "Invalid report path".to_string(),
        ));
    }
    Ok(joined_canon)
}

fn build_report_filename(report: &SyncReport) -> String {
    let timestamp = format_report_timestamp(&report.created_at);
    let remote = sanitize_component(&report.remote, 24);
    let provider = sanitize_component(&report.provider, 12);
    let prefix = if !remote.is_empty() { remote } else { provider };

    if prefix.is_empty() {
        return format!("{}.yml", timestamp);
    }

    format!("{}-{}.yml", prefix, timestamp)
}

fn format_report_timestamp(value: &str) -> String {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return parsed.format("%Y-%m-%dT%H-%M-%S").to_string();
    }
    sanitize_component(&value.replace([':', '.'], "-"), 32)
}

fn parse_report_payload(payload: &str) -> Result<SyncReport, serde_yaml::Error> {
    serde_yaml::from_str(payload)
}

fn sanitize_component(input: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    let cleaned = out.trim_matches('-').trim_matches('.').to_string();
    let trimmed = if cleaned.is_empty() {
        "sync".to_string()
    } else {
        cleaned
    };
    if trimmed.chars().count() <= max_len {
        trimmed
    } else {
        trimmed.chars().take(max_len).collect()
    }
}

fn report_to_meta(report: &SyncReport, stored_path: Option<String>) -> SyncReportMeta {
    SyncReportMeta {
        id: report.id.clone(),
        created_at: report.created_at.clone(),
        status: report.status.clone(),
        direction: report.direction.clone(),
        provider: report.provider.clone(),
        remote: report.remote.clone(),
        project: report.project.clone(),
        dry_run: report.dry_run,
        summary: report.summary.clone(),
        warnings: report.warnings.clone(),
        info: report.info.clone(),
        entries_total: report.entries.len(),
        stored_path,
    }
}

fn sort_report_meta(a: &SyncReportMeta, b: &SyncReportMeta) -> std::cmp::Ordering {
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    };
    let a_time = parse(&a.created_at);
    let b_time = parse(&b.created_at);
    match (a_time, b_time) {
        (Some(a_dt), Some(b_dt)) => b_dt.cmp(&a_dt),
        _ => b.created_at.cmp(&a.created_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_component_strips_unwanted_chars() {
        let value = sanitize_component("Report: Sync/Run #1", 32);
        assert!(!value.contains(' '));
        assert!(!value.contains('/'));
        assert!(!value.is_empty());
    }
}
