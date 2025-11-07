use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::config::types::ResolvedConfig;
use crate::storage::task::Task as StoredTask;
use crate::utils::effort::{self, EffortKind};

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum, Default, Serialize, Deserialize)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "lowercase")]
pub enum SprintBurndownMetric {
    #[default]
    Tasks,
    Points,
    Hours,
}

pub fn metric_label(metric: SprintBurndownMetric) -> &'static str {
    match metric {
        SprintBurndownMetric::Tasks => "tasks",
        SprintBurndownMetric::Points => "points",
        SprintBurndownMetric::Hours => "hours",
    }
}

pub fn ratio(done: f64, total: f64) -> Option<f64> {
    if total.abs() < f64::EPSILON {
        None
    } else {
        Some(done / total)
    }
}

pub fn ratio_usize(done: usize, total: usize) -> Option<f64> {
    if total == 0 {
        None
    } else {
        Some(done as f64 / total as f64)
    }
}

pub fn determine_done_statuses_from_config(config: &ResolvedConfig) -> HashSet<String> {
    let mut done = HashSet::new();

    if let Some(last) = config.issue_states.values.last() {
        done.insert(last.as_str().to_ascii_lowercase());
    }

    for status in &config.issue_states.values {
        if status.eq_ignore_case("done")
            || status.eq_ignore_case("completed")
            || status.eq_ignore_case("closed")
        {
            done.insert(status.as_str().to_ascii_lowercase());
        }
    }

    for (alias, status) in &config.branch_status_aliases {
        if alias.eq_ignore_ascii_case("done") {
            done.insert(status.as_str().to_ascii_lowercase());
        }
    }

    if done.is_empty() {
        done.insert("done".to_string());
        done.insert("completed".to_string());
        done.insert("closed".to_string());
    }

    done
}

pub fn determine_blocked_statuses_from_config(config: &ResolvedConfig) -> HashSet<String> {
    let mut blocked = HashSet::new();

    for status in &config.issue_states.values {
        let lowered = status.as_str().to_ascii_lowercase();
        if lowered.contains("block") {
            blocked.insert(lowered);
        }
    }

    for (alias, status) in &config.branch_status_aliases {
        if alias.to_ascii_lowercase().contains("block") {
            blocked.insert(status.as_str().to_ascii_lowercase());
        }
        let lowered = status.as_str().to_ascii_lowercase();
        if lowered.contains("block") {
            blocked.insert(lowered);
        }
    }

    blocked
}

#[derive(Debug, Default, Clone)]
pub struct VelocityTotals {
    pub total_tasks: usize,
    pub done_tasks: usize,
    pub total_points: f64,
    pub done_points: f64,
    pub total_hours: f64,
    pub done_hours: f64,
}

pub fn compute_velocity_totals(
    tasks: &[(String, StoredTask)],
    done_statuses: &HashSet<String>,
) -> VelocityTotals {
    let mut totals = VelocityTotals::default();

    for (_, task) in tasks.iter() {
        totals.total_tasks += 1;
        let is_done = done_statuses.contains(&task.status.as_str().to_ascii_lowercase());
        if is_done {
            totals.done_tasks += 1;
        }

        if let Some(effort_str) = task.effort.as_ref() {
            if let Ok(parsed) = effort::parse_effort(effort_str) {
                match parsed.kind {
                    EffortKind::Points(points) => {
                        totals.total_points += points;
                        if is_done {
                            totals.done_points += points;
                        }
                    }
                    EffortKind::TimeHours(hours) => {
                        totals.total_hours += hours;
                        if is_done {
                            totals.done_hours += hours;
                        }
                    }
                }
            }
        }
    }

    totals
}
