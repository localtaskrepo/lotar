//! Run context, sync events and the per-task report recorder.

use super::*;

pub(super) static RUN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub(super) struct SyncRunContext {
    pub(super) run_id: String,
    pub(super) started_at: String,
    pub(super) direction: SyncDirection,
    pub(super) provider: String,
    pub(super) remote: String,
    pub(super) project: Option<String>,
    pub(super) dry_run: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum SyncEntryStatus {
    Created,
    Updated,
    Skipped,
    Failed,
}

impl SyncEntryStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            SyncEntryStatus::Created => "created",
            SyncEntryStatus::Updated => "updated",
            SyncEntryStatus::Skipped => "skipped",
            SyncEntryStatus::Failed => "failed",
        }
    }
}

pub(super) struct SyncReportRecorder {
    pub(super) context: SyncRunContext,
    pub(super) summary: SyncSummary,
    pub(super) entries: Vec<SyncReportEntry>,
}

impl SyncReportRecorder {
    pub(super) fn new(context: SyncRunContext) -> Self {
        Self {
            context,
            summary: SyncSummary::default(),
            entries: Vec::new(),
        }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "the caller hands this value over and does not use it afterwards"
    )]
    pub(super) fn record(&mut self, status: SyncEntryStatus, entry: SyncReportEntry) {
        match status {
            SyncEntryStatus::Created => self.summary.created += 1,
            SyncEntryStatus::Updated => self.summary.updated += 1,
            SyncEntryStatus::Skipped => self.summary.skipped += 1,
            SyncEntryStatus::Failed => self.summary.failed += 1,
        }
        self.entries.push(entry.clone());

        emit_sync_event(
            "sync_progress",
            json!({
                "run_id": self.context.run_id.clone(),
                "direction": self.context.direction.as_str(),
                "provider": self.context.provider.clone(),
                "remote": self.context.remote.clone(),
                "project": self.context.project.clone(),
                "dry_run": self.context.dry_run,
                "summary": self.summary.clone(),
                "entry": entry,
            }),
        );
    }

    pub(super) fn report(
        &self,
        status: &str,
        warnings: Vec<String>,
        info: Vec<String>,
    ) -> SyncReport {
        SyncReport {
            id: self.context.run_id.clone(),
            created_at: self.context.started_at.clone(),
            status: status.to_string(),
            direction: self.context.direction.as_str().to_string(),
            provider: self.context.provider.clone(),
            remote: self.context.remote.clone(),
            project: self.context.project.clone(),
            dry_run: self.context.dry_run,
            summary: self.summary.clone(),
            warnings,
            info,
            entries: self.entries.clone(),
        }
    }

    pub(super) fn meta(
        &self,
        status: &str,
        warnings: Vec<String>,
        info: Vec<String>,
        stored_path: Option<String>,
    ) -> SyncReportMeta {
        SyncReportMeta {
            id: self.context.run_id.clone(),
            created_at: self.context.started_at.clone(),
            status: status.to_string(),
            direction: self.context.direction.as_str().to_string(),
            provider: self.context.provider.clone(),
            remote: self.context.remote.clone(),
            project: self.context.project.clone(),
            dry_run: self.context.dry_run,
            summary: self.summary.clone(),
            warnings,
            info,
            entries_total: self.entries.len(),
            stored_path,
        }
    }
}

pub(super) fn emit_sync_event(kind: &str, payload: JsonValue) {
    crate::api_events::emit(&crate::api_events::ApiEvent {
        kind: kind.to_string(),
        data: payload,
    });
}

pub(super) fn make_run_id(client_run_id: Option<&str>) -> String {
    if let Some(value) = client_run_id {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
    let counter = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("sync-{stamp}-{counter}")
}

pub(super) fn make_entry(
    status: SyncEntryStatus,
    task_id: Option<String>,
    reference: Option<String>,
    title: Option<String>,
    message: Option<String>,
) -> SyncReportEntry {
    SyncReportEntry {
        status: status.as_str().to_string(),
        at: Utc::now().to_rfc3339(),
        task_id,
        reference,
        title,
        message,
        fields: Vec::new(),
    }
}

pub(super) fn make_entry_with_fields(
    status: SyncEntryStatus,
    task_id: Option<String>,
    reference: Option<String>,
    title: Option<String>,
    fields: Vec<String>,
    message: Option<String>,
) -> SyncReportEntry {
    SyncReportEntry {
        status: status.as_str().to_string(),
        at: Utc::now().to_rfc3339(),
        task_id,
        reference,
        title,
        message,
        fields,
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum SyncOperation {
    Create,
    Update,
}
