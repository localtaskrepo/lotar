//! Shared task query executor and field accessor (DEV-57).
//!
//! One strict, deterministic post-storage pipeline for task listing across
//! REST `/api/tasks/list`, REST `/api/tasks/export`, MCP `task_list`, and the
//! CLI `list` comparator: smart filters (`due`, `recent`, `needs`), sorting
//! (`sort_by` builtins plus `custom:<name>` / `field:<name>`), `order`, and
//! the canonical-ID ascending tiebreak.
//!
//! Contract:
//! - Default order: `modified` descending, ties broken by canonical task ID
//!   lexical ascending (the tiebreak never flips with `order`).
//! - Explicit invalid enum values (`order`, `sort_by`, `due`, `recent`,
//!   `needs`, `sprints`, config enums) are errors; nothing fails open.
//! - Due smart filters bucket by the task's local calendar date computed
//!   from a injectable clock so boundaries stay DST-safe; `soon` covers
//!   `(today, today+7]`, `later` is strictly after `today+7`, `overdue` is
//!   strictly before today.
//! - Stored due dates are parsed as RFC3339, naive local datetimes, or
//!   date-only (local midnight) — the persisted formats. Human keywords
//!   ("today", "+3d") are intentionally NOT re-evaluated at query time.
//! - `recent=7d` compares parsed `modified` instants against `now - 7d`
//!   (inclusive lower bound).

use crate::api_types::TaskDTO;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use std::cmp::Ordering;
use std::collections::BTreeSet;

/// Builtin sort keys (the CLI `--sort-by` vocabulary).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Priority,
    Status,
    Effort,
    DueDate,
    Created,
    Modified,
    Assignee,
    Reporter,
    Title,
    Type,
    Project,
    Id,
    Tags,
    Sprints,
}

/// Resolved sort specification: a builtin key or a custom field name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SortSpec {
    Builtin(SortKey),
    Custom(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl SortOrder {
    pub fn is_desc(self) -> bool {
        matches!(self, SortOrder::Desc)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DueFilter {
    Today,
    Soon,
    Later,
    Overdue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecentFilter {
    Last7Days,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NeedsFlag {
    Due,
    Effort,
}

/// Advertised `soon` window in days (docs/help + openapi vocabulary).
pub const SOON_WINDOW_DAYS: i64 = 7;

#[derive(Clone, Debug)]
pub struct TaskQueryOptions {
    pub sort: SortSpec,
    pub order: SortOrder,
    pub due: Option<DueFilter>,
    pub recent: Option<RecentFilter>,
    pub needs: BTreeSet<NeedsFlag>,
}

impl Default for TaskQueryOptions {
    fn default() -> Self {
        Self {
            sort: SortSpec::Builtin(SortKey::Modified),
            order: SortOrder::Desc,
            due: None,
            recent: None,
            needs: BTreeSet::new(),
        }
    }
}

fn parse_builtin_sort_key(lower: &str) -> Option<SortKey> {
    match lower {
        "priority" => Some(SortKey::Priority),
        "status" => Some(SortKey::Status),
        "effort" => Some(SortKey::Effort),
        "due-date" | "due" => Some(SortKey::DueDate),
        "created" => Some(SortKey::Created),
        "modified" => Some(SortKey::Modified),
        "assignee" => Some(SortKey::Assignee),
        "reporter" => Some(SortKey::Reporter),
        "title" => Some(SortKey::Title),
        "type" => Some(SortKey::Type),
        "project" => Some(SortKey::Project),
        "id" => Some(SortKey::Id),
        "tags" => Some(SortKey::Tags),
        "sprints" => Some(SortKey::Sprints),
        _ => None,
    }
}

/// Strict `sort_by` parser shared by REST and MCP: builtin keys plus the
/// `custom:<name>` form and the CLI `field:<name>` alias (case-insensitive
/// prefix; the field name keeps its original spelling for lookups).
pub fn parse_sort_by(raw: &str) -> Result<SortSpec, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("sort_by must not be empty".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["custom:", "field:"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let name = rest.trim();
            if name.is_empty() {
                return Err(format!(
                    "sort_by prefix '{prefix}' requires a field name (sort_by='{raw}')"
                ));
            }
            // Keep the original spelling for case-insensitive field lookup.
            let original = &trimmed[trimmed.len() - name.len()..];
            return Ok(SortSpec::Custom(original.to_string()));
        }
    }
    if let Some(key) = parse_builtin_sort_key(&lower) {
        return Ok(SortSpec::Builtin(key));
    }
    Err(format!(
        "Invalid sort_by: '{raw}' (expected a builtin key (priority, status, effort, due-date, created, modified, assignee, reporter, title, type, project, id, tags, sprints), custom:<name>, or field:<name>)"
    ))
}

pub fn parse_order(raw: &str) -> Result<SortOrder, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "asc" => Ok(SortOrder::Asc),
        "desc" => Ok(SortOrder::Desc),
        other => Err(format!("Invalid order: '{other}' (expected asc or desc)")),
    }
}

pub fn parse_due(raw: &str) -> Result<DueFilter, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "today" => Ok(DueFilter::Today),
        "soon" => Ok(DueFilter::Soon),
        "later" => Ok(DueFilter::Later),
        "overdue" => Ok(DueFilter::Overdue),
        other => Err(format!(
            "Invalid due: '{other}' (expected today, soon, later, or overdue)"
        )),
    }
}

pub fn parse_recent(raw: &str) -> Result<RecentFilter, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "7d" => Ok(RecentFilter::Last7Days),
        other => Err(format!("Invalid recent: '{other}' (expected 7d)")),
    }
}

/// Strict `needs` CSV parser: every non-blank token must be `effort` or
/// `due`; blank tokens inside a non-blank value are rejected.
pub fn parse_needs(raw: &str) -> Result<BTreeSet<NeedsFlag>, String> {
    let mut out = BTreeSet::new();
    if raw.trim().is_empty() {
        return Err(
            "Invalid needs: must not be blank (omit the parameter or supply effort, due)"
                .to_string(),
        );
    }
    for token in raw.split(',') {
        let token = token.trim();
        match token.to_ascii_lowercase().as_str() {
            "effort" => {
                out.insert(NeedsFlag::Effort);
            }
            "due" => {
                out.insert(NeedsFlag::Due);
            }
            "" => {
                return Err("Invalid needs: empty entry (expected CSV of effort, due)".to_string());
            }
            other => {
                return Err(format!(
                    "Invalid needs entry: '{other}' (expected effort or due)"
                ));
            }
        }
    }
    Ok(out)
}

/// Parse the REST query parts handled by the shared executor. Absent keys
/// keep the defaults; present-but-invalid values error.
pub fn parse_query_options(
    query: &std::collections::HashMap<String, String>,
) -> Result<TaskQueryOptions, String> {
    let mut options = TaskQueryOptions::default();
    if let Some(raw) = query.get("sort_by") {
        options.sort = parse_sort_by(raw)?;
    }
    if let Some(raw) = query.get("order") {
        options.order = parse_order(raw)?;
    }
    // Explicitly supplied blank values are errors (strict blanks): callers
    // must omit empty parameters rather than sending `due=` &c.
    if let Some(raw) = query.get("due") {
        options.due = Some(parse_due(raw)?);
    }
    if let Some(raw) = query.get("recent") {
        options.recent = Some(parse_recent(raw)?);
    }
    if let Some(raw) = query.get("needs") {
        options.needs = parse_needs(raw)?;
    }
    Ok(options)
}

/// Field accessor for the shared comparator, implemented for `TaskDTO`
/// (REST/MCP) and `Task` (CLI).
pub trait TaskSortFields {
    fn title_str(&self) -> &str;
    fn status_str(&self) -> String;
    fn priority_str(&self) -> String;
    fn type_str(&self) -> String;
    fn assignee(&self) -> Option<&str>;
    fn reporter(&self) -> Option<&str>;
    /// Tags in stored order; the comparator array-lexes them as stored
    /// (no normalization, no reordering).
    fn tags(&self) -> &[String];
    /// Sprint memberships, ascending (deterministic numeric vec-lex key).
    fn sprints(&self) -> Vec<u32>;
    fn due_date(&self) -> Option<&str>;
    fn effort(&self) -> Option<&str>;
    fn created(&self) -> &str;
    fn modified(&self) -> &str;
    fn custom_field_value(&self, name: &str) -> Option<String>;
}

impl TaskSortFields for TaskDTO {
    fn title_str(&self) -> &str {
        self.title.as_str()
    }
    fn status_str(&self) -> String {
        self.status.to_string()
    }
    fn priority_str(&self) -> String {
        self.priority.to_string()
    }
    fn type_str(&self) -> String {
        self.task_type.to_string()
    }
    fn assignee(&self) -> Option<&str> {
        self.assignee.as_deref()
    }
    fn reporter(&self) -> Option<&str> {
        self.reporter.as_deref()
    }
    fn tags(&self) -> &[String] {
        self.tags.as_slice()
    }
    fn sprints(&self) -> Vec<u32> {
        let mut ids = self.sprints.clone();
        ids.sort_unstable();
        ids
    }
    fn due_date(&self) -> Option<&str> {
        self.due_date.as_deref()
    }
    fn effort(&self) -> Option<&str> {
        self.effort.as_deref()
    }
    fn created(&self) -> &str {
        self.created.as_str()
    }
    fn modified(&self) -> &str {
        self.modified.as_str()
    }
    fn custom_field_value(&self, name: &str) -> Option<String> {
        crate::utils::custom_fields::extract_value_strings(&self.custom_fields, name)
            .and_then(|values| values.into_iter().next())
    }
}

impl TaskSortFields for crate::storage::task::Task {
    fn title_str(&self) -> &str {
        self.title.as_str()
    }
    fn status_str(&self) -> String {
        self.status.to_string()
    }
    fn priority_str(&self) -> String {
        self.priority.to_string()
    }
    fn type_str(&self) -> String {
        self.task_type.to_string()
    }
    fn assignee(&self) -> Option<&str> {
        self.assignee.as_deref()
    }
    fn reporter(&self) -> Option<&str> {
        self.reporter.as_deref()
    }
    fn tags(&self) -> &[String] {
        self.tags.as_slice()
    }
    fn sprints(&self) -> Vec<u32> {
        let mut ids = self.sprints.clone();
        ids.sort_unstable();
        ids
    }
    fn due_date(&self) -> Option<&str> {
        self.due_date.as_deref()
    }
    fn effort(&self) -> Option<&str> {
        self.effort.as_deref()
    }
    fn created(&self) -> &str {
        self.created.as_str()
    }
    fn modified(&self) -> &str {
        self.modified.as_str()
    }
    fn custom_field_value(&self, name: &str) -> Option<String> {
        crate::utils::custom_fields::extract_value_strings(&self.custom_fields, name)
            .and_then(|values| values.into_iter().next())
    }
}

/// A stored due value in its natural granularity: a bare calendar date or a
/// full instant. Date-only values keep their date identity so bucketing and
/// sorting never depend on local-midnight resolution (which does not exist
/// for every date in every timezone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredDue {
    Date(NaiveDate),
    Instant(DateTime<Utc>),
}

impl StoredDue {
    /// The local calendar date this due falls on.
    pub fn local_date(self) -> NaiveDate {
        match self {
            StoredDue::Date(date) => date,
            StoredDue::Instant(instant) => instant.with_timezone(&Local).date_naive(),
        }
    }
}

/// Parse a stored due value: RFC3339 (any offset), naive local datetime, or
/// date-only. Returns `None` for anything the storage layer would not have
/// written.
pub fn parse_stored_due(raw: &str) -> Option<StoredDue> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(StoredDue::Instant(dt.with_timezone(&Utc)));
    }
    if let Some(dt) = crate::utils::time::parse_naive_local_datetime_to_utc(trimmed) {
        return Some(StoredDue::Instant(dt));
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Some(StoredDue::Date(date));
    }
    None
}

/// Parse a stored value into a sortable instant: RFC3339 (any offset), naive
/// local datetime, or date-only mapped to the earliest valid local instant
/// of that calendar date (DST-gap safe: if local midnight does not exist,
/// the first resolvable minute of the date is used).
pub fn parse_stored_datetime_to_utc(raw: &str) -> Option<DateTime<Utc>> {
    match parse_stored_due(raw) {
        Some(StoredDue::Instant(instant)) => Some(instant),
        Some(StoredDue::Date(date)) => earliest_local_instant(date),
        None => None,
    }
}

/// Earliest valid local instant of a calendar date. Probes midnight first,
/// then successive minutes (up to 4h) so dates whose midnight falls in a DST
/// gap still resolve; falls back to naive-UTC midnight only if the whole
/// window is unresolvable.
fn earliest_local_instant(date: NaiveDate) -> Option<DateTime<Utc>> {
    for minute in 0..=240u32 {
        let (h, m) = (minute / 60, minute % 60);
        if let Some(dt) = Local
            .with_ymd_and_hms(date.year(), date.month(), date.day(), h, m, 0)
            .single()
        {
            return Some(dt.with_timezone(&Utc));
        }
    }
    date.and_hms_opt(0, 0, 0)
        .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc))
}

/// The local calendar date an instant falls on.
fn local_date_of(instant: DateTime<Utc>) -> NaiveDate {
    instant.with_timezone(&Local).date_naive()
}

/// Local "today" for a clock value.
pub fn today_local(now: DateTime<Utc>) -> NaiveDate {
    local_date_of(now)
}

/// Inclusive lower bound for `recent=7d`.
pub fn recent_cutoff(now: DateTime<Utc>) -> DateTime<Utc> {
    now - Duration::days(7)
}

/// Pure, DST-safe date-bucket test (calendar arithmetic only).
fn due_bucket_matches(due_local: NaiveDate, today: NaiveDate, filter: DueFilter) -> bool {
    let soon_end = today + Duration::days(SOON_WINDOW_DAYS);
    match filter {
        DueFilter::Today => due_local == today,
        DueFilter::Soon => due_local > today && due_local <= soon_end,
        DueFilter::Later => due_local > soon_end,
        DueFilter::Overdue => due_local < today,
    }
}

fn task_matches_due(task: &TaskDTO, filter: DueFilter, today: NaiveDate) -> bool {
    due_raw_matches_bucket(task.due_date.as_deref(), filter, today)
}

/// Shared predicate: does a stored due value fall in the requested bucket?
/// Date-only values bucket directly on their calendar date; instants on the
/// local date they fall on. Missing/unparseable values match nothing.
pub fn due_raw_matches_bucket(raw: Option<&str>, filter: DueFilter, today: NaiveDate) -> bool {
    let Some(due) = raw.and_then(parse_stored_due) else {
        return false;
    };
    due_bucket_matches(due.local_date(), today, filter)
}

/// Shared predicate for window filters (`--due-soon[=days]`): the due falls
/// on today or any of the next `days` calendar days (inclusive on both
/// ends, matching the documented "due within N days" wording).
pub fn due_raw_within_window(raw: Option<&str>, today: NaiveDate, days: i64) -> bool {
    let Some(due) = raw.and_then(parse_stored_due) else {
        return false;
    };
    let date = due.local_date();
    date >= today && date <= today + Duration::days(days)
}

fn task_matches_recent(task: &TaskDTO, cutoff: DateTime<Utc>) -> bool {
    let Some(modified) = DateTime::parse_from_rfc3339(task.modified.as_str()).ok() else {
        return false;
    };
    modified.with_timezone(&Utc) >= cutoff
}

fn task_matches_needs(task: &TaskDTO, needs: &BTreeSet<NeedsFlag>) -> bool {
    for flag in needs {
        let missing = match flag {
            NeedsFlag::Effort => task.effort.as_deref().unwrap_or("").trim().is_empty(),
            NeedsFlag::Due => task.due_date.as_deref().unwrap_or("").trim().is_empty(),
        };
        if !missing {
            return false;
        }
    }
    true
}

/// Apply the smart filters (`due`, `recent`, `needs`) in place.
pub fn apply_smart_filters(
    tasks: &mut Vec<(String, TaskDTO)>,
    options: &TaskQueryOptions,
    now: DateTime<Utc>,
) {
    if options.due.is_none() && options.recent.is_none() && options.needs.is_empty() {
        return;
    }
    let today = today_local(now);
    let cutoff = recent_cutoff(now);
    tasks.retain(|(_, task)| {
        options
            .due
            .is_none_or(|filter| task_matches_due(task, filter, today))
            && options
                .recent
                .is_none_or(|_| task_matches_recent(task, cutoff))
            && task_matches_needs(task, &options.needs)
    });
}

fn compare_effort(a: Option<&str>, b: Option<&str>) -> Ordering {
    let parse = |raw: Option<&str>| raw.and_then(|s| crate::utils::effort::parse_effort(s).ok());
    match (parse(a), parse(b)) {
        (Some(x), Some(y)) => match (&x.kind, &y.kind) {
            (
                crate::utils::effort::EffortKind::TimeHours(av),
                crate::utils::effort::EffortKind::TimeHours(bv),
            ) => av.partial_cmp(bv).unwrap_or(Ordering::Equal),
            (
                crate::utils::effort::EffortKind::Points(av),
                crate::utils::effort::EffortKind::Points(bv),
            ) => av.partial_cmp(bv).unwrap_or(Ordering::Equal),
            _ => x.canonical.cmp(&y.canonical),
        },
        // Missing/unparseable effort sorts after present values (CLI parity).
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_optional_due(a: Option<&str>, b: Option<&str>) -> Ordering {
    let parse = |raw: Option<&str>| raw.and_then(parse_stored_datetime_to_utc);
    match (parse(a), parse(b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_parsed_instant(a: &str, b: &str) -> Ordering {
    let parse = |raw: &str| DateTime::parse_from_rfc3339(raw).ok();
    match (parse(a), parse(b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.cmp(b),
    }
}

fn project_prefix_of(id: &str) -> String {
    crate::storage::TaskId::parse(id)
        .map(|parsed| parsed.project)
        .unwrap_or_default()
}

/// Compare two tasks by the resolved sort spec (ascending semantics).
fn compare_by_spec<T: TaskSortFields + ?Sized>(
    id_a: &str,
    a: &T,
    id_b: &str,
    b: &T,
    spec: &SortSpec,
) -> Ordering {
    match spec {
        SortSpec::Builtin(key) => match key {
            SortKey::Priority => a.priority_str().cmp(&b.priority_str()),
            SortKey::Status => a.status_str().cmp(&b.status_str()),
            SortKey::Effort => compare_effort(a.effort(), b.effort()),
            SortKey::DueDate => compare_optional_due(a.due_date(), b.due_date()),
            SortKey::Created => compare_parsed_instant(a.created(), b.created()),
            SortKey::Modified => compare_parsed_instant(a.modified(), b.modified()),
            SortKey::Assignee => a.assignee().cmp(&b.assignee()),
            SortKey::Reporter => a.reporter().cmp(&b.reporter()),
            SortKey::Title => a.title_str().cmp(b.title_str()),
            SortKey::Tags => a.tags().to_vec().cmp(&b.tags().to_vec()),
            SortKey::Sprints => a.sprints().cmp(&b.sprints()),
            SortKey::Type => a.type_str().cmp(&b.type_str()),
            SortKey::Project => project_prefix_of(id_a).cmp(&project_prefix_of(id_b)),
            SortKey::Id => id_a.cmp(id_b),
        },
        SortSpec::Custom(name) => a.custom_field_value(name).cmp(&b.custom_field_value(name)),
    }
}

/// Sort tasks by the spec and order with the canonical-ID ascending
/// tiebreak. The tiebreak never flips with the requested order.
pub fn sort_tasks<T: TaskSortFields>(tasks: &mut [(String, T)], spec: &SortSpec, order: SortOrder) {
    tasks.sort_by(|(id_a, a), (id_b, b)| {
        let primary = compare_by_spec(id_a, a, id_b, b, spec);
        let primary = if order.is_desc() {
            primary.reverse()
        } else {
            primary
        };
        if primary == Ordering::Equal {
            id_a.cmp(id_b)
        } else {
            primary
        }
    });
}

/// Full executor: smart filters, then global ordering. Returns the filtered
/// and ordered task list; pagination stays a caller concern.
pub fn apply(tasks: &mut Vec<(String, TaskDTO)>, options: &TaskQueryOptions, now: DateTime<Utc>) {
    apply_smart_filters(tasks, options, now);
    sort_tasks(tasks, &options.sort, options.order);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sort_by_accepts_builtins_and_prefixes() {
        assert_eq!(
            parse_sort_by("modified"),
            Ok(SortSpec::Builtin(SortKey::Modified))
        );
        assert_eq!(
            parse_sort_by(" Due-Date "),
            Ok(SortSpec::Builtin(SortKey::DueDate))
        );
        assert_eq!(
            parse_sort_by("due"),
            Ok(SortSpec::Builtin(SortKey::DueDate))
        );
        assert_eq!(
            parse_sort_by("effort"),
            Ok(SortSpec::Builtin(SortKey::Effort))
        );
        assert_eq!(
            parse_sort_by("custom:MyField"),
            Ok(SortSpec::Custom("MyField".to_string()))
        );
        assert_eq!(
            parse_sort_by("FIELD:other"),
            Ok(SortSpec::Custom("other".to_string()))
        );
    }

    #[test]
    fn parse_sort_by_rejects_bare_unknown_and_empty_prefix() {
        assert!(parse_sort_by("nope").is_err());
        assert!(parse_sort_by("custom:").is_err());
        assert!(parse_sort_by("").is_err());
    }

    #[test]
    fn parse_order_due_recent_needs_strict() {
        assert_eq!(parse_order("ASC"), Ok(SortOrder::Asc));
        assert_eq!(parse_order("desc"), Ok(SortOrder::Desc));
        assert!(parse_order("descending").is_err());
        assert_eq!(parse_due("TODAY"), Ok(DueFilter::Today));
        assert!(parse_due("tomorrow").is_err());
        assert_eq!(parse_recent("7d"), Ok(RecentFilter::Last7Days));
        assert!(parse_recent("30d").is_err());
        let needs = parse_needs("effort,due").unwrap();
        assert!(needs.contains(&NeedsFlag::Effort) && needs.contains(&NeedsFlag::Due));
        assert!(parse_needs("effort,size").is_err());
        assert!(parse_needs("effort,,due").is_err());
        assert!(
            parse_needs("").is_err(),
            "blank needs must be an explicit error"
        );
    }

    #[test]
    fn due_buckets_cover_boundaries_without_overlap() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap(); // US DST spring-forward day
        let mk = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert!(due_bucket_matches(
            mk(2026, 3, 7),
            today,
            DueFilter::Overdue
        ));
        assert!(due_bucket_matches(today, today, DueFilter::Today));
        assert!(due_bucket_matches(mk(2026, 3, 9), today, DueFilter::Soon));
        assert!(due_bucket_matches(mk(2026, 3, 15), today, DueFilter::Soon));
        assert!(due_bucket_matches(mk(2026, 3, 16), today, DueFilter::Later));
        // No overlaps: each boundary date matches exactly one bucket.
        for offset in -2..=10 {
            let d = today + Duration::days(offset);
            let matched: Vec<DueFilter> = [
                DueFilter::Today,
                DueFilter::Soon,
                DueFilter::Later,
                DueFilter::Overdue,
            ]
            .into_iter()
            .filter(|f| due_bucket_matches(d, today, *f))
            .collect();
            assert_eq!(matched.len(), 1, "date {d} matched {matched:?}");
        }
    }

    #[test]
    fn stored_datetime_parses_rfc3339_naive_and_date_only() {
        assert!(parse_stored_datetime_to_utc("2026-06-15T10:00:00Z").is_some());
        assert!(parse_stored_datetime_to_utc("2026-06-15T10:00:00+02:00").is_some());
        assert!(parse_stored_datetime_to_utc("2026-06-15").is_some());
        assert!(parse_stored_datetime_to_utc("2026-06-15 10:00").is_some());
        assert!(parse_stored_datetime_to_utc("today").is_none());
        assert!(parse_stored_datetime_to_utc("").is_none());
        assert!(parse_stored_datetime_to_utc("not a date").is_none());
    }

    #[test]
    fn date_only_due_keeps_identity_and_sort_instant_resolves() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(); // US DST fall-back day
        assert_eq!(parse_stored_due("2026-11-01"), Some(StoredDue::Date(date)));
        // The sortable instant resolves even where midnight is ambiguous and
        // maps back onto the same local date.
        let instant = parse_stored_datetime_to_utc("2026-11-01")
            .expect("date-only must resolve to a sortable instant");
        assert_eq!(local_date_of(instant), date);
    }

    #[test]
    fn due_bucket_predicates_cover_window_and_overdue() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 10).unwrap();
        let due = |d: i64| Some((today + Duration::days(d)).format("%Y-%m-%d").to_string());
        // Overdue bucket: strictly before today.
        assert!(due_raw_matches_bucket(
            due(-1).as_deref(),
            DueFilter::Overdue,
            today
        ));
        assert!(!due_raw_matches_bucket(
            due(0).as_deref(),
            DueFilter::Overdue,
            today
        ));
        // Window predicate (CLI --due-soon): inclusive today..=today+N.
        assert!(due_raw_within_window(due(0).as_deref(), today, 3));
        assert!(due_raw_within_window(due(3).as_deref(), today, 3));
        assert!(!due_raw_within_window(due(4).as_deref(), today, 3));
        assert!(!due_raw_within_window(None, today, 3));
        assert!(!due_raw_matches_bucket(None, DueFilter::Today, today));
    }

    fn dto(id: &str, modified: &str) -> crate::api_types::TaskDTO {
        crate::api_types::TaskDTO {
            id: id.to_string(),
            title: format!("task {id}"),
            status: crate::types::TaskStatus::from("Todo"),
            priority: crate::types::Priority::from("Medium"),
            task_type: crate::types::TaskType::from("task"),
            reporter: None,
            assignee: None,
            created: "2026-01-01T00:00:00Z".to_string(),
            modified: modified.to_string(),
            due_date: None,
            effort: None,
            subtitle: None,
            description: None,
            tags: Vec::new(),
            relationships: crate::types::TaskRelationships::default(),
            comments: Vec::new(),
            references: Vec::new(),
            acceptance_criteria: Vec::new(),
            sprints: Vec::new(),
            sprint_order: Default::default(),
            history: Vec::new(),
            custom_fields: crate::types::CustomFields::new(),
        }
    }

    #[test]
    fn sort_tasks_orders_custom_values_with_id_tiebreak() {
        let with_value = |id: &str, value: &str, modified: &str| {
            let mut t = dto(id, modified);
            t.custom_fields
                .insert("size".to_string(), crate::types::custom_value_string(value));
            t
        };
        let mut tasks = vec![
            (
                "B-2".to_string(),
                with_value("B-2", "large", "2026-01-01T00:00:00Z"),
            ),
            ("A-10".to_string(), dto("A-10", "2026-01-01T00:00:00Z")),
            (
                "A-2".to_string(),
                with_value("A-2", "small", "2026-01-01T00:00:00Z"),
            ),
            ("C-1".to_string(), dto("C-1", "2026-01-01T00:00:00Z")),
        ];
        sort_tasks(
            &mut tasks,
            &SortSpec::Custom("size".to_string()),
            SortOrder::Asc,
        );
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
        // Missing custom values sort first (empty-string semantics, CLI
        // parity); equal values tie on canonical ID lexical asc.
        assert_eq!(ids, vec!["A-10", "C-1", "B-2", "A-2"]);

        sort_tasks(
            &mut tasks,
            &SortSpec::Custom("size".to_string()),
            SortOrder::Desc,
        );
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
        // Desc flips the primary order only; the tiebreak stays ID asc.
        assert_eq!(ids, vec!["A-2", "B-2", "A-10", "C-1"]);
    }

    #[test]
    fn sort_tasks_modified_desc_is_default_shape() {
        let mut tasks = vec![
            ("A-1".to_string(), dto("A-1", "2026-01-01T00:00:00Z")),
            ("B-1".to_string(), dto("B-1", "2026-03-01T00:00:00Z")),
            ("C-1".to_string(), dto("C-1", "2026-02-01T00:00:00Z")),
        ];
        sort_tasks(
            &mut tasks,
            &SortSpec::Builtin(SortKey::Modified),
            SortOrder::Desc,
        );
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["B-1", "C-1", "A-1"]);
    }

    #[test]
    fn modified_offsets_sort_chronologically_not_lexically() {
        let mut tasks = vec![
            (
                "A-1".to_string(),
                dto("A-1", "2026-06-15T23:00:00+02:00"), // 21:00Z
            ),
            ("B-1".to_string(), dto("B-1", "2026-06-15T22:00:00Z")),
        ];
        sort_tasks(
            &mut tasks,
            &SortSpec::Builtin(SortKey::Modified),
            SortOrder::Asc,
        );
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["A-1", "B-1"]);
    }

    #[test]
    fn sort_tasks_orders_tags_and_sprints_vec_lex_with_empty_first() {
        let with_tags = |id: &str, tags: &[&str], modified: &str| {
            let mut t = dto(id, modified);
            t.tags = tags.iter().map(|s| s.to_string()).collect();
            t
        };
        let with_sprints = |id: &str, sprints: &[u32], modified: &str| {
            let mut t = dto(id, modified);
            t.sprints = sprints.to_vec();
            t
        };
        let mut tasks = vec![
            (
                "A-1".to_string(),
                with_tags("A-1", &["alpha"], "2026-01-01T00:00:00Z"),
            ),
            (
                "B-1".to_string(),
                with_tags("B-1", &[], "2026-01-01T00:00:00Z"),
            ),
            (
                "C-1".to_string(),
                with_tags("C-1", &["alpha", "beta"], "2026-01-01T00:00:00Z"),
            ),
        ];
        sort_tasks(
            &mut tasks,
            &SortSpec::Builtin(SortKey::Tags),
            SortOrder::Asc,
        );
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
        // Empty vec first; then array-lex ([alpha] < [alpha, beta]).
        assert_eq!(ids, vec!["B-1", "A-1", "C-1"]);
        sort_tasks(
            &mut tasks,
            &SortSpec::Builtin(SortKey::Tags),
            SortOrder::Desc,
        );
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["C-1", "A-1", "B-1"]);

        let mut tasks = vec![
            (
                "A-1".to_string(),
                with_sprints("A-1", &[9], "2026-01-01T00:00:00Z"),
            ),
            (
                "B-1".to_string(),
                with_sprints("B-1", &[], "2026-01-01T00:00:00Z"),
            ),
            (
                "C-1".to_string(),
                with_sprints("C-1", &[2, 3], "2026-01-01T00:00:00Z"),
            ),
        ];
        sort_tasks(
            &mut tasks,
            &SortSpec::Builtin(SortKey::Sprints),
            SortOrder::Asc,
        );
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
        // Numeric vec-lex: [] < [2,3] < [9].
        assert_eq!(ids, vec!["B-1", "C-1", "A-1"]);
    }

    #[test]
    fn sort_tasks_reporter_and_title_follow_scalar_and_option_semantics() {
        let with = |id: &str, title: &str, reporter: Option<&str>, modified: &str| {
            let mut t = dto(id, modified);
            t.title = title.to_string();
            t.reporter = reporter.map(str::to_string);
            t
        };
        let mut tasks = vec![
            (
                "A-1".to_string(),
                with("A-1", "Beta", Some("zoe"), "2026-01-01T00:00:00Z"),
            ),
            (
                "B-1".to_string(),
                with("B-1", "alpha", None, "2026-01-01T00:00:00Z"),
            ),
        ];
        sort_tasks(
            &mut tasks,
            &SortSpec::Builtin(SortKey::Title),
            SortOrder::Asc,
        );
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
        // Case-sensitive lexical: "Beta" < "alpha" (B < a in ASCII).
        assert_eq!(ids, vec!["A-1", "B-1"]);

        sort_tasks(
            &mut tasks,
            &SortSpec::Builtin(SortKey::Reporter),
            SortOrder::Asc,
        );
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["B-1", "A-1"],
            "None reporter sorts first asc, like assignee"
        );
    }

    #[test]
    fn smart_filters_match_needs_and_recent_boundaries() {
        let now = DateTime::parse_from_rfc3339("2026-06-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let cutoff = recent_cutoff(now);
        let t_exact = dto("A-1", cutoff.to_rfc3339().as_str());
        let t_older = dto(
            "B-1",
            (cutoff - chrono::Duration::seconds(1))
                .to_rfc3339()
                .as_str(),
        );
        let mut needs_both = dto("C-1", now.to_rfc3339().as_str());
        needs_both.effort = None;
        needs_both.due_date = None;
        let mut tasks = vec![
            ("A-1".to_string(), t_exact),
            ("B-1".to_string(), t_older),
            ("C-1".to_string(), needs_both),
        ];
        let options = TaskQueryOptions {
            recent: Some(RecentFilter::Last7Days),
            needs: [NeedsFlag::Effort, NeedsFlag::Due].into_iter().collect(),
            ..TaskQueryOptions::default()
        };
        apply_smart_filters(&mut tasks, &options, now);
        let ids: Vec<&str> = tasks.iter().map(|(id, _)| id.as_str()).collect();
        // Exact cutoff is included (>=), one second older is excluded; needs
        // requires blank effort AND blank due, which the fixture has.
        assert_eq!(ids, vec!["A-1", "C-1"]);
    }
}
