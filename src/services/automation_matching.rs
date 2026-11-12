use crate::api_types::TaskDTO;
use crate::automation::types::{AutomationCommentCondition, AutomationConditionGroup, AutomationFieldCondition, AutomationRule};
use crate::config::types::ResolvedConfig;
use crate::utils::tags::normalize_tags;
use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};

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
    /// The comment of a `commented` event (DEV-383).
    comment: Option<CommentFacts>,
}

#[derive(Debug, Clone)]
struct CommentFacts {
    author: Option<String>,
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
        capture_change("status", prev.map(|t| t.status.to_string()), Some(current.status.to_string()), &mut fields);
        capture_change("assignee", prev.and_then(|t| t.assignee.clone()), current.assignee.clone(), &mut fields);
        capture_change("reporter", prev.and_then(|t| t.reporter.clone()), current.reporter.clone(), &mut fields);
        capture_change("priority", prev.map(|t| t.priority.to_string()), Some(current.priority.to_string()), &mut fields);
        capture_change("type", prev.map(|t| t.task_type.to_string()), Some(current.task_type.to_string()), &mut fields);
        capture_change("title", prev.map(|t| t.title.clone()), Some(current.title.clone()), &mut fields);
        capture_change("description", prev.and_then(|t| t.description.clone()), current.description.clone(), &mut fields);
        capture_change("due_date", prev.and_then(|t| t.due_date.clone()), current.due_date.clone(), &mut fields);
        capture_change("effort", prev.and_then(|t| t.effort.clone()), current.effort.clone(), &mut fields);

        let prev_tags = prev.map(|t| normalize_tags(t.tags.clone())).unwrap_or_default();
        let next_tags = normalize_tags(current.tags.clone());
        if prev.is_none() || prev_tags != next_tags {
            fields.insert(
                "tags".to_string(),
                FieldChange {
                    before: if prev.is_none() { None } else { Some(AutomationValue::List(prev_tags)) },
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

        Self { fields, custom_fields, comment: None }
    }

    pub(crate) fn empty(current: &TaskDTO) -> Self {
        Self::new(Some(current), current)
    }

    /// No field changes: a comment by `author` was added to `current`.
    pub(crate) fn commented(current: &TaskDTO, author: Option<&str>) -> Self {
        Self { comment: Some(CommentFacts { author: author.map(str::to_string) }), ..Self::empty(current) }
    }

    fn changed_field(&self, field: &str) -> Option<&FieldChange> {
        self.fields.get(&change_key(field))
    }

    fn changed_custom_field(&self, field: &str) -> Option<&FieldChange> {
        self.custom_fields.get(field)
    }
}

/// The name a [`ChangeSet`] records `field` under: conditions may use the
/// aliases `labels` and `task_type` (DEV-365).
fn change_key(field: &str) -> String {
    match field.to_lowercase().as_str() {
        "labels" => "tags".to_string(),
        "task_type" => "type".to_string(),
        other => other.to_string(),
    }
}

fn capture_change(field: &str, before: Option<String>, after: Option<String>, out: &mut HashMap<String, FieldChange>) {
    if before.is_none() && after.is_none() {
        return;
    }
    if before.as_ref().map(|v| v.trim()) == after.as_ref().map(|v| v.trim()) {
        return;
    }
    out.insert(field.to_lowercase(), FieldChange { before: before.map(AutomationValue::String), after: after.map(AutomationValue::String) });
}

// ── Change requirements (DEV-362) ───────────────────────────────────────────

/// The fields a [`ChangeSet`] records.
const CHANGE_FIELDS: [&str; 10] = ["status", "assignee", "reporter", "priority", "type", "title", "description", "due_date", "effort", "tags"];

impl ChangeSet {
    /// The changes of an update that changed only `fields` and left the task
    /// as `current`. The values before the update are unknown.
    fn only(current: &TaskDTO, fields: &[String]) -> Self {
        let mut all = Self::new(None, current);
        let mut out = Self { fields: HashMap::new(), custom_fields: HashMap::new(), comment: None };
        for field in fields {
            let key = change_key(field);
            if let Some(change) = all.fields.remove(&key) {
                out.fields.insert(key, change);
            } else if CHANGE_FIELDS.contains(&key.as_str()) {
                out.fields.insert(key, FieldChange { before: None, after: None });
            }
            let change = all.custom_fields.remove(field).unwrap_or(FieldChange { before: None, after: None });
            out.custom_fields.insert(field.clone(), change);
        }
        out
    }
}

/// What an update has to change for a rule to fire on a task as it is now.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum UpdateMatch {
    /// Any update with the event's own changes fires it.
    Always,
    /// An update that also changes one of these fields fires it.
    OneOf(Vec<String>),
    /// Only an update that changes all of these fields together fires it.
    Together(Vec<String>),
    Never,
}

/// How `rule` matches real updates of `current` that change at least `base`
/// (`assignee` for the assigned event), as [`MatchMode::OnChange`] judges them.
pub(crate) fn update_match(rule: &AutomationRule, current: &TaskDTO, base: &[&str], config: &ResolvedConfig, active_sprint_ids: &[u32]) -> UpdateMatch {
    let fires = |extra: &[String]| {
        let mut fields: Vec<String> = base.iter().map(|field| field.to_string()).collect();
        fields.extend_from_slice(extra);
        matches_rule(rule, current, &ChangeSet::only(current, &fields), MatchMode::OnChange, config, active_sprint_ids)
    };
    if fires(&[]) {
        return UpdateMatch::Always;
    }
    let mut referenced = BTreeSet::new();
    if let Some(group) = rule.when.as_ref() {
        collect_change_fields(group, &mut referenced);
    }
    let fields: Vec<String> = referenced.into_iter().filter(|field| !base.contains(&field.as_str())).collect();
    let single: Vec<String> = fields.iter().filter(|field| fires(std::slice::from_ref(*field))).cloned().collect();
    if !single.is_empty() {
        return UpdateMatch::OneOf(single);
    }
    if fields.len() < 2 || !fires(&fields) {
        return UpdateMatch::Never;
    }
    // Drop the fields the rule fires without, so only the needed ones are named.
    let mut needed = fields.clone();
    for field in &fields {
        let without: Vec<String> = needed.iter().filter(|other| *other != field).cloned().collect();
        if fires(&without) {
            needed = without;
        }
    }
    UpdateMatch::Together(needed)
}

/// The fields `rule` checks on the task as it is now with `current` (DEV-298).
pub(crate) fn current_fields(rule: &AutomationRule) -> Vec<String> {
    fn collect(group: &AutomationConditionGroup, inside: bool, out: &mut BTreeSet<String>) {
        for entry in group.all.iter().flatten().chain(group.any.iter().flatten()).chain(group.not.as_deref()) {
            collect(entry, inside, out);
        }
        if let Some(now) = group.current.as_deref() {
            collect(now, true, out);
        }
        if inside {
            out.extend(group.fields.keys().map(|field| change_key(field)));
            out.extend(group.custom_fields.keys().cloned());
        }
    }
    let mut out = BTreeSet::new();
    if let Some(group) = rule.when.as_ref() {
        collect(group, false, &mut out);
    }
    out.into_iter().collect()
}

/// Whether `rule` has a condition on the comment of a `commented` event.
pub(crate) fn has_comment_condition(rule: &AutomationRule) -> bool {
    fn walk(group: &AutomationConditionGroup) -> bool {
        group.comment.is_some()
            || group.all.iter().flatten().chain(group.any.iter().flatten()).chain(group.not.as_deref()).chain(group.current.as_deref()).any(walk)
    }
    rule.when.as_ref().is_some_and(walk)
}

/// Fields under `current` are left out: they need not change.
fn collect_change_fields(group: &AutomationConditionGroup, out: &mut BTreeSet<String>) {
    for entry in group.all.iter().flatten().chain(group.any.iter().flatten()) {
        collect_change_fields(entry, out);
    }
    if let Some(not) = group.not.as_ref() {
        collect_change_fields(not, out);
    }
    if let Some(changes) = group.changes.as_ref() {
        out.extend(changes.keys().map(|field| change_key(field)));
    }
    out.extend(group.fields.keys().map(|field| change_key(field)).filter(|field| CHANGE_FIELDS.contains(&field.as_str())));
    // Custom fields named at the top level (DEV-373).
    out.extend(group.fields.keys().filter(|field| is_top_level_custom(field)).cloned());
    out.extend(group.custom_fields.keys().cloned());
}

/// Whether a top-level condition key can only be a custom field.
fn is_top_level_custom(field: &str) -> bool {
    crate::utils::fields::is_reserved_field(field).is_none()
        && !matches!(field.to_lowercase().as_str(), "labels" | "task_type" | "sprint" | "sprints" | "status_changed")
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
    if !rule.is_enabled() {
        return false;
    }
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
        && !all.iter().all(|entry| matches_group(entry, current, changes, mode, config, active_sprint_ids))
    {
        return false;
    }
    if let Some(any) = group.any.as_ref()
        && !any.iter().any(|entry| matches_group(entry, current, changes, mode, config, active_sprint_ids))
    {
        return false;
    }
    if let Some(not) = group.not.as_ref()
        && matches_group(not, current, changes, mode, config, active_sprint_ids)
    {
        return false;
    }

    if let Some(now) = group.current.as_ref()
        && !matches_group(now, current, changes, MatchMode::Current, config, active_sprint_ids)
    {
        return false;
    }

    if let Some(change_group) = group.changes.as_ref()
        && !matches_changes(change_group, changes, mode)
    {
        return false;
    }

    if let Some(comment) = group.comment.as_ref()
        && !matches_comment(comment, changes, config)
    {
        return false;
    }

    if !matches_field_map(&group.fields, current, changes, mode, config, active_sprint_ids) {
        return false;
    }

    if !matches_custom_fields(&group.custom_fields, current, changes, mode) {
        return false;
    }

    true
}

fn matches_changes(group: &HashMap<String, crate::automation::types::AutomationChangeCondition>, changes: &ChangeSet, mode: MatchMode) -> bool {
    if matches!(mode, MatchMode::Current) {
        return false;
    }
    for (field, condition) in group {
        let Some(change) = changes.changed_field(field) else {
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

fn matches_comment(condition: &AutomationCommentCondition, changes: &ChangeSet, config: &ResolvedConfig) -> bool {
    let Some(comment) = changes.comment.as_ref() else {
        return false;
    };
    let Some(author_condition) = condition.author.as_ref() else {
        return true;
    };
    let author = comment.author.as_deref();
    matches_assignee_placeholder_str(author_condition, author, config)
        || matches_value_condition(author_condition, author.map(|value| AutomationValue::String(value.to_string())).as_ref())
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

        // Moments in the task's past (DEV-241): only the time operators apply,
        // always to the task as it is now, so they also work for rules run on
        // a schedule (`on.manual`) where nothing changed.
        if let Some(moment) = past_moment(field_key.as_str(), current) {
            if !matches_past_moment(moment.as_deref(), &condition.as_map()) {
                return false;
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
                // A declared custom field may stand at the top level too
                // (DEV-373): `affects: { version: ">=0.13" }`.
                let custom =
                    if crate::utils::fields::is_reserved_field(key).is_none() { crate::utils::custom_fields::resolve_filter_name(key, config) } else { None };
                match custom {
                    Some(name) if matches_custom_field(&name, condition, current, changes, mode) => continue,
                    _ => return false,
                }
            }
        };

        if matches!(mode, MatchMode::OnChange) {
            let change = changes.changed_field(&field_key);
            // Date conditions (before/within/older_than) evaluate the current value
            // against "now", so they don't require the field itself to have changed.
            let map = condition.as_map();
            let is_date_only = map.before.is_some() || map.within.is_some() || map.older_than.is_some();
            if change.is_none() && !is_date_only {
                return false;
            }
            let target = if is_date_only { value.as_ref() } else { change.and_then(|c| c.after.as_ref()) };
            if !matches_value_condition(condition, target) {
                return false;
            }
        } else if !matches_value_condition(condition, value.as_ref()) {
            return false;
        }
    }
    true
}

fn matches_custom_fields(fields: &HashMap<String, AutomationFieldCondition>, current: &TaskDTO, changes: &ChangeSet, mode: MatchMode) -> bool {
    fields.iter().all(|(key, condition)| matches_custom_field(key, condition, current, changes, mode))
}

fn matches_custom_field(key: &str, condition: &AutomationFieldCondition, current: &TaskDTO, changes: &ChangeSet, mode: MatchMode) -> bool {
    let current_value = current.custom_fields.get(key).map(custom_value_to_value);
    let change_value = changes.changed_custom_field(key).and_then(|c| c.after.clone());

    let target = if matches!(mode, MatchMode::OnChange) {
        if change_value.is_none() {
            return false;
        }
        change_value.as_ref()
    } else {
        current_value.as_ref()
    };
    matches_value_condition(condition, target)
}

fn matches_sprint_condition(condition: &AutomationFieldCondition, current: &TaskDTO, active_sprint_ids: &[u32]) -> bool {
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
            return current.sprints.iter().any(|id| active_sprint_ids.contains(id));
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
            if !current.sprints.iter().any(|id| active_sprint_ids.contains(id)) {
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
                return current.sprints.iter().any(|id| active_sprint_ids.contains(id));
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

fn matches_assignee_placeholder_str(condition: &AutomationFieldCondition, assignee: Option<&str>, config: &ResolvedConfig) -> bool {
    let Some(assignee) = assignee else {
        return false;
    };
    let raw = match condition {
        AutomationFieldCondition::Scalar(value) => value.trim(),
        _ => return false,
    };
    match raw.to_ascii_lowercase().as_str() {
        "@agent" => config.agent_profiles.keys().any(|name| assignee.eq_ignore_ascii_case(&format!("@{}", name))),
        "@any" => !assignee.trim().is_empty(),
        "@user" => !config.agent_profiles.keys().any(|name| assignee.eq_ignore_ascii_case(&format!("@{}", name))),
        _ => assignee.eq_ignore_ascii_case(raw),
    }
}

// ── Value matching ──────────────────────────────────────────────────────────

fn matches_value_condition(condition: &AutomationFieldCondition, value: Option<&AutomationValue>) -> bool {
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
    if (map.before.is_some() || map.within.is_some() || map.older_than.is_some()) && !matches_date_conditions(value, &map) {
        return false;
    }
    if let Some(range) = map.version.as_deref() {
        let Ok(range) = crate::utils::version::VersionReq::parse(range) else {
            return false;
        };
        let items: Vec<&String> = match value {
            AutomationValue::String(v) => vec![v],
            AutomationValue::List(values) => values.iter().collect(),
        };
        let matched = items.iter().filter_map(|raw| crate::utils::version::Version::parse(raw).ok()).any(|version| range.matches(&version));
        if !matched {
            return false;
        }
    }
    true
}

fn value_equals(value: &AutomationValue, expected: &str) -> bool {
    match value {
        AutomationValue::String(v) => v.eq_ignore_ascii_case(expected.trim()),
        AutomationValue::List(values) => values.iter().any(|entry| entry.eq_ignore_ascii_case(expected.trim())),
    }
}

fn value_contains(value: &AutomationValue, expected: &str) -> bool {
    match value {
        AutomationValue::String(v) => v.to_ascii_lowercase().contains(&expected.to_ascii_lowercase()),
        AutomationValue::List(values) => values.iter().any(|entry| entry.eq_ignore_ascii_case(expected.trim())),
    }
}

fn value_starts_with(value: &AutomationValue, prefix: &str) -> bool {
    match value {
        AutomationValue::String(v) => v.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase()),
        AutomationValue::List(values) => values.iter().any(|entry| entry.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase())),
    }
}

fn value_matches_regex(value: &AutomationValue, regex: &Regex) -> bool {
    match value {
        AutomationValue::String(v) => regex.is_match(v),
        AutomationValue::List(values) => values.iter().any(|entry| regex.is_match(entry)),
    }
}

// ── Date conditions ─────────────────────────────────────────────────────────

/// Resolve a date reference string to a local NaiveDate.
/// Supports keywords ("today", "tomorrow", …), "YYYY-MM-DD", RFC 3339 datetimes,
/// and offset expressions via the shared human date grammar.
fn resolve_date_reference(s: &str) -> Option<chrono::NaiveDate> {
    crate::utils::time::parse_human_datetime_to_utc(s).ok().map(|dt| dt.with_timezone(&chrono::Local).date_naive())
}

/// Fields that name a moment in the task's past: when it was created, last
/// modified, or entered its current status (the latest status change in its
/// history, else its creation). `None` for other fields; `Some(None)` when
/// the moment is unknown.
fn past_moment(field: &str, task: &TaskDTO) -> Option<Option<String>> {
    match field {
        "created" => Some(Some(task.created.clone())),
        "modified" => Some(Some(task.modified.clone()).filter(|v| !v.is_empty())),
        "status_changed" => Some(
            task.history
                .iter()
                .filter(|entry| entry.changes.iter().any(|change| change.field == "status"))
                .map(|entry| entry.at.clone())
                .max()
                .or_else(|| Some(task.created.clone())),
        ),
        _ => None,
    }
}

/// `older_than` and `within` count back from now (`within: 7d` means in the
/// last seven days); `before` compares with a date; `exists` checks presence.
/// Other operators never match a moment.
fn matches_past_moment(value: Option<&str>, map: &crate::automation::types::AutomationFieldConditionMap) -> bool {
    let has_time_operator = map.before.is_some() || map.within.is_some() || map.older_than.is_some();
    if let Some(exists) = map.exists
        && value.is_some() != exists
    {
        return false;
    }
    if !has_time_operator {
        return map.exists.is_some() && map.equals.is_none() && map.r#in.is_none() && map.contains.is_none() && map.matches.is_none();
    }
    let Some(moment) = value.and_then(|raw| crate::utils::time::parse_human_datetime_to_utc(raw).ok()) else {
        return false;
    };
    let now = chrono::Utc::now();
    if let Some(reference) = map.before.as_deref() {
        let Some(date) = resolve_date_reference(reference) else {
            return false;
        };
        if moment.with_timezone(&chrono::Local).date_naive() >= date {
            return false;
        }
    }
    if let Some(span) = map.within.as_deref() {
        let Some(duration) = parse_date_offset(span) else {
            return false;
        };
        if moment < now - duration || moment > now {
            return false;
        }
    }
    if let Some(span) = map.older_than.as_deref() {
        let Some(duration) = parse_date_offset(span) else {
            return false;
        };
        if moment > now - duration {
            return false;
        }
    }
    true
}

/// Parse a duration string like "3d", "2w", "30d" into chrono::Duration.
fn parse_date_offset(s: &str) -> Option<chrono::Duration> {
    crate::utils::time::parse_duration_like(s)
}

/// Evaluate date-specific conditions (before, within, older_than) against a value.
fn matches_date_conditions(value: &AutomationValue, map: &crate::automation::types::AutomationFieldConditionMap) -> bool {
    let date_str = match value {
        AutomationValue::String(v) => v.as_str(),
        AutomationValue::List(_) => return false,
    };
    let Some(field_date) = resolve_date_reference(date_str) else {
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
        serde_yaml_ng::Value::String(v) => AutomationValue::String(v.clone()),
        serde_yaml_ng::Value::Sequence(list) => AutomationValue::List(list.iter().map(yaml_value_to_string).collect()),
        _ => AutomationValue::String(yaml_value_to_string(value)),
    }
}

#[cfg(not(feature = "schema"))]
fn yaml_value_to_string(value: &serde_yaml_ng::Value) -> String {
    match value {
        serde_yaml_ng::Value::String(v) => v.clone(),
        serde_yaml_ng::Value::Bool(v) => v.to_string(),
        serde_yaml_ng::Value::Number(v) => v.to_string(),
        serde_yaml_ng::Value::Null => "null".to_string(),
        other => serde_yaml_ng::to_string(other).unwrap_or_else(|_| format!("{other:?}")).trim().to_string(),
    }
}

#[cfg(feature = "schema")]
fn custom_value_to_value(value: &crate::types::CustomFieldValue) -> AutomationValue {
    match value {
        serde_json::Value::String(v) => AutomationValue::String(v.clone()),
        serde_json::Value::Array(list) => AutomationValue::List(list.iter().map(|v| v.as_str().unwrap_or(&v.to_string()).to_string()).collect()),
        _ => AutomationValue::String(value.to_string()),
    }
}
