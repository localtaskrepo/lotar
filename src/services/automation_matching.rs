use crate::api_types::TaskDTO;
use crate::automation::types::{
    AutomationConditionGroup, AutomationFieldCondition, AutomationRule,
};
use crate::config::types::ResolvedConfig;
use crate::utils::tags::normalize_tags;
use regex::Regex;
use std::collections::{HashMap, HashSet};

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub(crate) enum MatchMode {
    OnChange,
    Current,
}

#[derive(Debug, Clone)]
pub(crate) struct ChangeSet {
    fields: HashMap<String, FieldChange>,
    custom_fields: HashMap<String, FieldChange>,
}

#[derive(Debug, Clone)]
struct FieldChange {
    before: Option<AutomationValue>,
    after: Option<AutomationValue>,
}

#[derive(Debug, Clone, PartialEq)]
enum AutomationValue {
    String(String),
    List(Vec<String>),
}

// ── ChangeSet ───────────────────────────────────────────────────────────────

impl ChangeSet {
    pub(crate) fn new(previous: Option<&TaskDTO>, current: &TaskDTO) -> Self {
        let mut fields = HashMap::new();
        let mut custom_fields = HashMap::new();

        let prev = previous;
        capture_change(
            "status",
            prev.map(|t| t.status.to_string()),
            Some(current.status.to_string()),
            &mut fields,
        );
        capture_change(
            "assignee",
            prev.and_then(|t| t.assignee.clone()),
            current.assignee.clone(),
            &mut fields,
        );
        capture_change(
            "reporter",
            prev.and_then(|t| t.reporter.clone()),
            current.reporter.clone(),
            &mut fields,
        );
        capture_change(
            "priority",
            prev.map(|t| t.priority.to_string()),
            Some(current.priority.to_string()),
            &mut fields,
        );
        capture_change(
            "type",
            prev.map(|t| t.task_type.to_string()),
            Some(current.task_type.to_string()),
            &mut fields,
        );
        capture_change(
            "title",
            prev.map(|t| t.title.clone()),
            Some(current.title.clone()),
            &mut fields,
        );
        capture_change(
            "description",
            prev.and_then(|t| t.description.clone()),
            current.description.clone(),
            &mut fields,
        );
        capture_change(
            "due_date",
            prev.and_then(|t| t.due_date.clone()),
            current.due_date.clone(),
            &mut fields,
        );
        capture_change(
            "effort",
            prev.and_then(|t| t.effort.clone()),
            current.effort.clone(),
            &mut fields,
        );

        let prev_tags = prev
            .map(|t| normalize_tags(t.tags.clone()))
            .unwrap_or_default();
        let next_tags = normalize_tags(current.tags.clone());
        if prev.is_none() || prev_tags != next_tags {
            fields.insert(
                "tags".to_string(),
                FieldChange {
                    before: if prev.is_none() {
                        None
                    } else {
                        Some(AutomationValue::List(prev_tags))
                    },
                    after: Some(AutomationValue::List(next_tags)),
                },
            );
        }

        let prev_custom = prev.map(|t| t.custom_fields.clone()).unwrap_or_default();
        let next_custom = current.custom_fields.clone();
        let mut keys: HashSet<String> = prev_custom.keys().cloned().collect();
        keys.extend(next_custom.keys().cloned());
        for key in keys {
            let before = prev_custom.get(&key).map(custom_value_to_value);
            let after = next_custom.get(&key).map(custom_value_to_value);
            if prev.is_none() || before != after {
                custom_fields.insert(key, FieldChange { before, after });
            }
        }

        Self {
            fields,
            custom_fields,
        }
    }

    pub(crate) fn empty(current: &TaskDTO) -> Self {
        Self::new(Some(current), current)
    }

    fn changed_field(&self, field: &str) -> Option<&FieldChange> {
        self.fields.get(&field.to_lowercase())
    }

    fn changed_custom_field(&self, field: &str) -> Option<&FieldChange> {
        self.custom_fields.get(field)
    }
}

fn capture_change(
    field: &str,
    before: Option<String>,
    after: Option<String>,
    out: &mut HashMap<String, FieldChange>,
) {
    if before.is_none() && after.is_none() {
        return;
    }
    if before.as_ref().map(|v| v.trim()) == after.as_ref().map(|v| v.trim()) {
        return;
    }
    out.insert(
        field.to_lowercase(),
        FieldChange {
            before: before.map(AutomationValue::String),
            after: after.map(AutomationValue::String),
        },
    );
}

// ── Rule matching ───────────────────────────────────────────────────────────

pub(crate) fn matches_rule(
    rule: &AutomationRule,
    current: &TaskDTO,
    changes: &ChangeSet,
    mode: MatchMode,
    config: &ResolvedConfig,
    active_sprint_ids: &[u32],
) -> bool {
    let Some(group) = rule.when.as_ref() else {
        return true;
    };
    matches_group(group, current, changes, mode, config, active_sprint_ids)
}

fn matches_group(
    group: &AutomationConditionGroup,
    current: &TaskDTO,
    changes: &ChangeSet,
    mode: MatchMode,
    config: &ResolvedConfig,
    active_sprint_ids: &[u32],
) -> bool {
    if let Some(all) = group.all.as_ref()
        && !all
            .iter()
            .all(|entry| matches_group(entry, current, changes, mode, config, active_sprint_ids))
    {
        return false;
    }
    if let Some(any) = group.any.as_ref()
        && !any
            .iter()
            .any(|entry| matches_group(entry, current, changes, mode, config, active_sprint_ids))
    {
        return false;
    }
    if let Some(not) = group.not.as_ref()
        && matches_group(not, current, changes, mode, config, active_sprint_ids)
    {
        return false;
    }

    if let Some(change_group) = group.changes.as_ref()
        && !matches_changes(change_group, changes, mode)
    {
        return false;
    }

    if !matches_field_map(
        &group.fields,
        current,
        changes,
        mode,
        config,
        active_sprint_ids,
    ) {
        return false;
    }

    if !matches_custom_fields(&group.custom_fields, current, changes, mode) {
        return false;
    }

    true
}

fn matches_changes(
    group: &HashMap<String, crate::automation::types::AutomationChangeCondition>,
    changes: &ChangeSet,
    mode: MatchMode,
) -> bool {
    if matches!(mode, MatchMode::Current) {
        return false;
    }
    for (field, condition) in group {
        let Some(change) = changes.changed_field(&field.to_lowercase()) else {
            return false;
        };
        if let Some(from) = condition.from.as_ref()
            && !matches_value_condition(from, change.before.as_ref())
        {
            return false;
        }
        if let Some(to) = condition.to.as_ref()
            && !matches_value_condition(to, change.after.as_ref())
        {
            return false;
        }
    }
    true
}

fn matches_field_map(
    fields: &HashMap<String, AutomationFieldCondition>,
    current: &TaskDTO,
    changes: &ChangeSet,
    mode: MatchMode,
    config: &ResolvedConfig,
    active_sprint_ids: &[u32],
) -> bool {
    for (key, condition) in fields {
        let field_key = key.to_lowercase();
        if field_key == "assignee" {
            let map = condition.as_map();
            let assignee = if matches!(mode, MatchMode::OnChange) && map.exists.is_none() {
                let change = changes.changed_field("assignee");
                let Some(change) = change else {
                    return false;
                };
                change.after.as_ref().and_then(|value| match value {
                    AutomationValue::String(v) => Some(v.as_str()),
                    _ => None,
                })
            } else {
                current.assignee.as_deref()
            };
            if !matches_assignee_placeholder_str(condition, assignee, config) {
                let assignee_value = assignee.map(|v| AutomationValue::String(v.to_string()));
                if !matches_value_condition(condition, assignee_value.as_ref()) {
                    return false;
                }
            }
            continue;
        }

        let value = match field_key.as_str() {
            "status" => Some(AutomationValue::String(current.status.to_string())),
            "reporter" => current.reporter.clone().map(AutomationValue::String),
            "priority" => Some(AutomationValue::String(current.priority.to_string())),
            "type" | "task_type" => Some(AutomationValue::String(current.task_type.to_string())),
            "title" => Some(AutomationValue::String(current.title.clone())),
            "description" => current.description.clone().map(AutomationValue::String),
            "due_date" => current.due_date.clone().map(AutomationValue::String),
            "effort" => current.effort.clone().map(AutomationValue::String),
            "tags" | "labels" => Some(AutomationValue::List(normalize_tags(current.tags.clone()))),
            "sprint" | "sprints" => {
                // Sprint conditions evaluate current state (not changeset).
                if !matches_sprint_condition(condition, current, active_sprint_ids) {
                    return false;
                }
                continue;
            }
            _ => {
                return false;
            }
        };

        if matches!(mode, MatchMode::OnChange) {
            let change = changes.changed_field(&field_key);
            // Date conditions (before/within/older_than) evaluate the current value
            // against "now", so they don't require the field itself to have changed.
            let map = condition.as_map();
            let is_date_only =
                map.before.is_some() || map.within.is_some() || map.older_than.is_some();
            if change.is_none() && !is_date_only {
                return false;
            }
            let target = if is_date_only {
                value.as_ref()
            } else {
                change.and_then(|c| c.after.as_ref())
            };
            if !matches_value_condition(condition, target) {
                return false;
            }
        } else if !matches_value_condition(condition, value.as_ref()) {
            return false;
        }
    }
    true
}

fn matches_custom_fields(
    fields: &HashMap<String, AutomationFieldCondition>,
    current: &TaskDTO,
    changes: &ChangeSet,
    mode: MatchMode,
) -> bool {
    for (key, condition) in fields {
        let current_value = current.custom_fields.get(key).map(custom_value_to_value);
        let change_value = changes
            .changed_custom_field(key)
            .and_then(|c| c.after.clone());

        let target = if matches!(mode, MatchMode::OnChange) {
            if change_value.is_none() {
                return false;
            }
            change_value.as_ref()
        } else {
            current_value.as_ref()
        };
        if !matches_value_condition(condition, target) {
            return false;
        }
    }
    true
}

fn matches_sprint_condition(
    condition: &AutomationFieldCondition,
    current: &TaskDTO,
    active_sprint_ids: &[u32],
) -> bool {
    let map = condition.as_map();

    // exists check
    if let Some(expected) = map.exists {
        let has_sprints = !current.sprints.is_empty();
        if has_sprints != expected {
            return false;
        }
    }

    // Scalar: sprint: "@active" or sprint: "3"
    if let AutomationFieldCondition::Scalar(raw) = condition {
        let trimmed = raw.trim().to_ascii_lowercase();
        if trimmed == "@active" {
            return current
                .sprints
                .iter()
                .any(|id| active_sprint_ids.contains(id));
        }
        if let Ok(id) = trimmed.parse::<u32>() {
            return current.sprints.contains(&id);
        }
        return false;
    }

    // Map: equals: "@active" or in: ["@active", "3"]
    if let Some(eq) = map.equals.as_ref() {
        let trimmed = eq.trim().to_ascii_lowercase();
        if trimmed == "@active" {
            if !current
                .sprints
                .iter()
                .any(|id| active_sprint_ids.contains(id))
            {
                return false;
            }
        } else if let Ok(id) = trimmed.parse::<u32>() {
            if !current.sprints.contains(&id) {
                return false;
            }
        } else {
            return false;
        }
    }

    if let Some(options) = map.r#in.as_ref() {
        let matched = options.iter().any(|opt| {
            let trimmed = opt.trim().to_ascii_lowercase();
            if trimmed == "@active" {
                return current
                    .sprints
                    .iter()
                    .any(|id| active_sprint_ids.contains(id));
            }
            if let Ok(id) = trimmed.parse::<u32>() {
                return current.sprints.contains(&id);
            }
            false
        });
        if !matched {
            return false;
        }
    }

    true
}

fn matches_assignee_placeholder_str(
    condition: &AutomationFieldCondition,
    assignee: Option<&str>,
    config: &ResolvedConfig,
) -> bool {
    let Some(assignee) = assignee else {
        return false;
    };
    let raw = match condition {
        AutomationFieldCondition::Scalar(value) => value.trim(),
        _ => return false,
    };
    match raw.to_ascii_lowercase().as_str() {
        "@agent" => config
            .agent_profiles
            .keys()
            .any(|name| assignee.eq_ignore_ascii_case(&format!("@{}", name))),
        "@any" => !assignee.trim().is_empty(),
        "@user" => !config
            .agent_profiles
            .keys()
            .any(|name| assignee.eq_ignore_ascii_case(&format!("@{}", name))),
        _ => assignee.eq_ignore_ascii_case(raw),
    }
}

// ── Value matching ──────────────────────────────────────────────────────────

fn matches_value_condition(
    condition: &AutomationFieldCondition,
    value: Option<&AutomationValue>,
) -> bool {
    let map = condition.as_map();
    let Some(value) = value else {
        return map.exists == Some(false);
    };

    if let Some(expected) = map.equals.as_ref()
        && !value_equals(value, expected)
    {
        return false;
    }
    if let Some(options) = map.r#in.as_ref()
        && !options.iter().any(|option| value_equals(value, option))
    {
        return false;
    }
    if let Some(sub) = map.contains.as_ref()
        && !value_contains(value, sub)
    {
        return false;
    }
    if let Some(list) = map.any.as_ref()
        && !list.iter().any(|entry| value_contains(value, entry))
    {
        return false;
    }
    if let Some(list) = map.all.as_ref()
        && !list.iter().all(|entry| value_contains(value, entry))
    {
        return false;
    }
    if let Some(list) = map.none.as_ref()
        && list.iter().any(|entry| value_contains(value, entry))
    {
        return false;
    }
    if let Some(prefix) = map.starts_with.as_ref()
        && !value_starts_with(value, prefix)
    {
        return false;
    }
    if let Some(exists) = map.exists {
        let present = match value {
            AutomationValue::String(v) => !v.trim().is_empty(),
            AutomationValue::List(v) => !v.is_empty(),
        };
        if present != exists {
            return false;
        }
    }
    if let Some(pattern) = map.matches.as_ref() {
        if let Ok(regex) = Regex::new(pattern) {
            if !value_matches_regex(value, &regex) {
                return false;
            }
        } else {
            return false;
        }
    }
    if (map.before.is_some() || map.within.is_some() || map.older_than.is_some())
        && !matches_date_conditions(value, &map)
    {
        return false;
    }
    true
}

fn value_equals(value: &AutomationValue, expected: &str) -> bool {
    match value {
        AutomationValue::String(v) => v.eq_ignore_ascii_case(expected.trim()),
        AutomationValue::List(values) => values
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(expected.trim())),
    }
}

fn value_contains(value: &AutomationValue, expected: &str) -> bool {
    match value {
        AutomationValue::String(v) => v
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase()),
        AutomationValue::List(values) => values
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(expected.trim())),
    }
}

fn value_starts_with(value: &AutomationValue, prefix: &str) -> bool {
    match value {
        AutomationValue::String(v) => v
            .to_ascii_lowercase()
            .starts_with(&prefix.to_ascii_lowercase()),
        AutomationValue::List(values) => values.iter().any(|entry| {
            entry
                .to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
        }),
    }
}

fn value_matches_regex(value: &AutomationValue, regex: &Regex) -> bool {
    match value {
        AutomationValue::String(v) => regex.is_match(v),
        AutomationValue::List(values) => values.iter().any(|entry| regex.is_match(entry)),
    }
}

// ── Date conditions ─────────────────────────────────────────────────────────

/// Parse a string into a NaiveDate, supporting YYYY-MM-DD and RFC 3339 datetime.
fn parse_date_value(s: &str) -> Option<chrono::NaiveDate> {
    let trimmed = s.trim();
    if let Ok(d) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Some(d);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.date_naive());
    }
    // Try ISO without timezone
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt.date());
    }
    None
}

/// Resolve a date reference string to a NaiveDate.
/// Supports "today", "tomorrow", "YYYY-MM-DD", offset "+Nd", "+Nw".
fn resolve_date_reference(s: &str) -> Option<chrono::NaiveDate> {
    let trimmed = s.trim().to_lowercase();
    let today = chrono::Local::now().date_naive();
    match trimmed.as_str() {
        "today" | "now" => Some(today),
        "tomorrow" => Some(today + chrono::Duration::days(1)),
        "yesterday" => Some(today - chrono::Duration::days(1)),
        _ => {
            // Try as YYYY-MM-DD
            if let Some(d) = parse_date_value(s) {
                return Some(d);
            }
            None
        }
    }
}

/// Parse a duration string like "3d", "2w", "30d" into chrono::Duration.
fn parse_date_offset(s: &str) -> Option<chrono::Duration> {
    crate::utils::time::parse_duration_like(s)
}

/// Evaluate date-specific conditions (before, within, older_than) against a value.
fn matches_date_conditions(
    value: &AutomationValue,
    map: &crate::automation::types::AutomationFieldConditionMap,
) -> bool {
    let date_str = match value {
        AutomationValue::String(v) => v.as_str(),
        AutomationValue::List(_) => return false,
    };
    let Some(field_date) = parse_date_value(date_str) else {
        return false; // unparseable date → condition fails
    };
    let today = chrono::Local::now().date_naive();

    if let Some(ref_str) = map.before.as_ref() {
        let Some(ref_date) = resolve_date_reference(ref_str) else {
            return false;
        };
        if field_date >= ref_date {
            return false;
        }
    }

    if let Some(dur_str) = map.within.as_ref() {
        let Some(duration) = parse_date_offset(dur_str) else {
            return false;
        };
        let deadline = today + duration;
        // "within 3d" means: field_date is between today and today+3d (inclusive)
        if field_date < today || field_date > deadline {
            return false;
        }
    }

    if let Some(dur_str) = map.older_than.as_ref() {
        let Some(duration) = parse_date_offset(dur_str) else {
            return false;
        };
        let cutoff = today - duration;
        if field_date > cutoff {
            return false;
        }
    }

    true
}

// ── Custom field value conversion ───────────────────────────────────────────

#[cfg(not(feature = "schema"))]
fn custom_value_to_value(value: &crate::types::CustomFieldValue) -> AutomationValue {
    match value {
        serde_yaml::Value::String(v) => AutomationValue::String(v.clone()),
        serde_yaml::Value::Sequence(list) => {
            AutomationValue::List(list.iter().map(yaml_value_to_string).collect())
        }
        _ => AutomationValue::String(yaml_value_to_string(value)),
    }
}

#[cfg(not(feature = "schema"))]
fn yaml_value_to_string(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(v) => v.clone(),
        serde_yaml::Value::Bool(v) => v.to_string(),
        serde_yaml::Value::Number(v) => v.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        other => serde_yaml::to_string(other)
            .unwrap_or_else(|_| format!("{other:?}"))
            .trim()
            .to_string(),
    }
}

#[cfg(feature = "schema")]
fn custom_value_to_value(value: &crate::types::CustomFieldValue) -> AutomationValue {
    match value {
        serde_json::Value::String(v) => AutomationValue::String(v.clone()),
        serde_json::Value::Array(list) => AutomationValue::List(
            list.iter()
                .map(|v| v.as_str().unwrap_or(&v.to_string()).to_string())
                .collect(),
        ),
        _ => AutomationValue::String(value.to_string()),
    }
}
