use crate::output::OutputRenderer;
use crate::services::sprint_integrity::{MissingSprintReport, SprintCleanupOutcome};
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub(crate) struct SprintCleanupRefsRemoved {
    pub(crate) sprint_id: u32,
    pub(crate) count: usize,
}

#[derive(Debug, Serialize, Clone)]
pub(crate) struct SprintCleanupSummaryPayload {
    pub(crate) removed_references: usize,
    pub(crate) updated_tasks: usize,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) removed_by_sprint: Vec<SprintCleanupRefsRemoved>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) remaining_missing: Vec<u32>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SprintAssignmentIntegrityPayload {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) missing_sprints: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) tasks_with_missing: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) auto_cleanup: Option<SprintCleanupSummaryPayload>,
}

pub(crate) fn cleanup_summary_payload(
    outcome: &SprintCleanupOutcome,
) -> SprintCleanupSummaryPayload {
    SprintCleanupSummaryPayload {
        removed_references: outcome.removed_references,
        updated_tasks: outcome.updated_tasks,
        removed_by_sprint: outcome
            .removed_by_sprint
            .iter()
            .map(|entry| SprintCleanupRefsRemoved {
                sprint_id: entry.sprint_id,
                count: entry.count,
            })
            .collect(),
        remaining_missing: outcome.remaining_missing.clone(),
    }
}

pub(crate) fn emit_cleanup_summary(
    renderer: &OutputRenderer,
    outcome: &SprintCleanupOutcome,
    context: &str,
) {
    if outcome.removed_references == 0 {
        renderer.emit_info(&format!(
            "No missing sprint references were removed while {}.",
            context
        ));
        return;
    }

    renderer.emit_info(&format!(
        "Removed {} missing sprint reference(s) while {}.",
        outcome.removed_references, context
    ));

    if !outcome.removed_by_sprint.is_empty() {
        renderer.emit_info("Removed references by sprint:");
        for entry in &outcome.removed_by_sprint {
            renderer.emit_info(&format!("  - Sprint #{}: {}", entry.sprint_id, entry.count));
        }
    }

    if !outcome.remaining_missing.is_empty() {
        let formatted = format_missing_ids(&outcome.remaining_missing);
        renderer.emit_warning(&format!(
            "Remaining missing sprint references detected: {}.",
            formatted
        ));
    }
}

pub(crate) fn emit_missing_report(
    renderer: &OutputRenderer,
    report: &MissingSprintReport,
    context: &str,
) {
    if report.missing_sprints.is_empty() {
        return;
    }

    let formatted = format_missing_ids(&report.missing_sprints);
    renderer.emit_warning(&format!(
        "Missing sprint references detected while {}: {}.",
        context, formatted
    ));

    if report.tasks_with_missing > 0 {
        renderer.emit_info(&format!(
            "{} task(s) currently reference missing sprints.",
            report.tasks_with_missing
        ));
    }

    renderer.emit_info(
        "Re-run with --cleanup-missing to remove stale sprint memberships automatically.",
    );
}

pub(crate) fn build_assignment_integrity(
    baseline: &MissingSprintReport,
    current: &MissingSprintReport,
    cleanup: Option<&SprintCleanupOutcome>,
) -> Option<SprintAssignmentIntegrityPayload> {
    let missing_sprints = current.missing_sprints.clone();
    let tasks_with_missing = baseline.tasks_with_missing.max(current.tasks_with_missing);

    let auto_cleanup = cleanup.map(cleanup_summary_payload);

    if missing_sprints.is_empty()
        && tasks_with_missing == 0
        && auto_cleanup
            .as_ref()
            .map(|summary| summary.removed_references == 0 && summary.remaining_missing.is_empty())
            .unwrap_or(true)
    {
        return None;
    }

    Some(SprintAssignmentIntegrityPayload {
        missing_sprints,
        tasks_with_missing: (tasks_with_missing > 0).then_some(tasks_with_missing),
        auto_cleanup,
    })
}

pub(crate) fn format_missing_ids(ids: &[u32]) -> String {
    ids.iter()
        .map(|id| format!("#{}", id))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_string();
    }

    if max == 0 {
        return String::new();
    }

    if max <= 3 {
        return "...".chars().take(max).collect();
    }

    let mut truncated = value
        .chars()
        .take(max.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}
