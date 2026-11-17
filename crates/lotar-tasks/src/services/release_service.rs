//! Releases (DEV-393): release records under `@releases/` and the tickets
//! that belong to them.
//!
//! Releases follow sprints: numeric IDs from the file names, a create /
//! update / delete lifecycle, add / move / remove of tickets, and a backlog
//! of tickets in no release. Unlike sprint membership, a ticket's releases
//! live on the ticket (`releases: [<id>]`) and are not exclusive: a fix can
//! ship in 0.15.1 and in 0.16.0. `move` replaces a ticket's releases with
//! one; `add` keeps the others.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use chrono::Utc;

use crate::api_types::{ReleaseListItem, TaskDTO, TaskUpdate};
use crate::errors::{LoTaRError, LoTaRResult};
use crate::services::task_service::{TaskService, TaskUpdateContext};
use crate::storage::manager::Storage;
use crate::storage::release::Release;
use crate::types::{TaskChange, TaskChangeLogEntry};
use crate::utils::version::{Version, VersionReq};

const RELEASE_LOCK_NAME: &str = ".releases.lock";

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseRecord {
    pub id: u32,
    pub release: Release,
}

/// Set (`Some(Some(_))`), clear (`Some(None)`) or keep (`None`) each field.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReleasePatch {
    pub version: Option<String>,
    pub label: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    pub planned_date: Option<Option<String>>,
    pub released_at: Option<Option<String>>,
}

impl ReleasePatch {
    /// From request fields where an empty string clears the field.
    pub fn from_text(version: Option<String>, label: Option<String>, notes: Option<String>, planned_date: Option<String>, released_at: Option<String>) -> Self {
        let clearable = |value: Option<String>| value.map(|text| Some(text).filter(|text| !text.trim().is_empty()));
        Self { version, label: clearable(label), notes: clearable(notes), planned_date: clearable(planned_date), released_at: clearable(released_at) }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseAssignmentAction {
    Add,
    Move,
    Remove,
}

impl ReleaseAssignmentAction {
    pub fn as_str(self) -> &'static str {
        match self {
            ReleaseAssignmentAction::Add => "add",
            ReleaseAssignmentAction::Move => "move",
            ReleaseAssignmentAction::Remove => "remove",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReleaseAssignmentOutcome {
    pub action: ReleaseAssignmentAction,
    pub record: ReleaseRecord,
    pub modified: Vec<String>,
    pub unchanged: Vec<String>,
}

/// Which releases a `release:` search term selects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseFilter {
    pub ids: BTreeSet<u32>,
    /// Tickets in no release (`release:none`).
    pub none: bool,
}

impl ReleaseFilter {
    /// Resolve `release:` values: `none`, an ID (`3`, `#3`), a label, or a
    /// version or version range (`0.15`, `>=0.15 <0.16`), which selects every
    /// release whose version meets it.
    pub fn resolve(values: &[String], records: &[ReleaseRecord]) -> Result<Self, String> {
        let mut filter = Self::default();
        for raw in values {
            let value = raw.trim();
            if value.is_empty() {
                continue;
            }
            if value.eq_ignore_ascii_case("none") {
                filter.none = true;
                continue;
            }
            if let Some(id) = parse_id(value) {
                filter.ids.insert(id);
                continue;
            }
            if let Some(record) = records.iter().find(|record| record.release.label.as_deref().is_some_and(|label| label.eq_ignore_ascii_case(value))) {
                filter.ids.insert(record.id);
                continue;
            }
            let range = VersionReq::parse(value).map_err(|message| format!("release:{value}: {message}"))?;
            filter
                .ids
                .extend(records.iter().filter(|record| record.release.parsed_version().is_some_and(|version| range.matches(&version))).map(|record| record.id));
        }
        Ok(filter)
    }

    pub fn matches(&self, releases: &[u32]) -> bool {
        (self.none && releases.is_empty()) || releases.iter().any(|id| self.ids.contains(id))
    }
}

fn parse_id(value: &str) -> Option<u32> {
    // A bare number is an ID only when it has no dot: `1` is release #1,
    // `1.0` is a version.
    let digits = value.strip_prefix('#').unwrap_or(value);
    if value.starts_with('#') || !digits.contains('.') {
        return digits.parse::<u32>().ok().filter(|id| *id > 0);
    }
    None
}

pub struct ReleaseService;

impl ReleaseService {
    /// Every release, in version order (unreadable versions last, by ID).
    pub fn list(tasks_root: &Path) -> LoTaRResult<Vec<ReleaseRecord>> {
        let dir = Release::dir(tasks_root);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut records = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()).and_then(|stem| stem.parse::<u32>().ok()) else {
                continue;
            };
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("yml") {
                continue;
            }
            match Self::load(&path) {
                Ok(release) => records.push(ReleaseRecord { id, release }),
                Err(error) => crate::storage::safety::warn_corrupt_once(&path, &error.to_string()),
            }
        }
        sort_records(&mut records);
        Ok(records)
    }

    pub fn get(tasks_root: &Path, id: u32) -> LoTaRResult<ReleaseRecord> {
        let path = Release::path_for_id(tasks_root, id);
        if !path.exists() {
            return Err(LoTaRError::ReleaseNotFound(id));
        }
        Ok(ReleaseRecord { id, release: Self::load(&path)? })
    }

    /// Find a release by ID (`3`, `#3`), version (`v0.15.0` is `0.15.0`) or
    /// label.
    pub fn resolve(tasks_root: &Path, reference: &str) -> LoTaRResult<ReleaseRecord> {
        let records = Self::list(tasks_root)?;
        resolve_in(&records, reference).cloned()
    }

    pub fn create(tasks_root: &Path, mut release: Release) -> LoTaRResult<ReleaseRecord> {
        release.version = normalize_version(&release.version)?;
        release.canonicalize();
        validate_dates(&release)?;
        let now = Utc::now().to_rfc3339();
        release.created = Some(now.clone());
        release.modified = Some(now);
        let dir = Release::dir(tasks_root);
        fs::create_dir_all(&dir)?;
        crate::storage::safety::with_storage_lock(&dir, RELEASE_LOCK_NAME, || {
            let records = Self::list(tasks_root)?;
            ensure_unique_version(&records, &release.version, None)?;
            let id = records.iter().map(|record| record.id).max().unwrap_or(0).saturating_add(1);
            crate::storage::safety::atomic_write_file(&Release::path_for_id(tasks_root, id), &release.to_yaml()?)?;
            Ok(ReleaseRecord { id, release })
        })
    }

    /// Apply a patch under the releases lock. Returns the record and whether
    /// anything changed; a no-op leaves the file untouched.
    pub fn update(tasks_root: &Path, id: u32, patch: &ReleasePatch) -> LoTaRResult<(ReleaseRecord, bool)> {
        let dir = Release::dir(tasks_root);
        if !Release::path_for_id(tasks_root, id).exists() {
            return Err(LoTaRError::ReleaseNotFound(id));
        }
        crate::storage::safety::with_storage_lock(&dir, RELEASE_LOCK_NAME, || {
            let current = Self::get(tasks_root, id)?.release;
            let mut next = current.clone();
            if let Some(version) = patch.version.as_deref() {
                next.version = normalize_version(version)?;
                ensure_unique_version(&Self::list(tasks_root)?, &next.version, Some(id))?;
            }
            for (target, value) in [
                (&mut next.label, &patch.label),
                (&mut next.notes, &patch.notes),
                (&mut next.planned_date, &patch.planned_date),
                (&mut next.released_at, &patch.released_at),
            ] {
                if let Some(value) = value {
                    target.clone_from(value);
                }
            }
            next.canonicalize();
            validate_dates(&next)?;
            if next == current {
                return Ok((ReleaseRecord { id, release: current }, false));
            }
            let now = Utc::now().to_rfc3339();
            let changes = diff(&current, &next);
            next.history.push(TaskChangeLogEntry {
                extra: Default::default(),
                at: now.clone(),
                actor: crate::utils::identity::resolve_current_user(Some(tasks_root)),
                changes,
            });
            next.modified = Some(now);
            crate::storage::safety::atomic_write_file(&Release::path_for_id(tasks_root, id), &next.to_yaml()?)?;
            Ok((ReleaseRecord { id, release: next }, true))
        })
    }

    /// Mark a release as released at `at` (default now).
    pub fn mark_released(tasks_root: &Path, id: u32, at: Option<String>) -> LoTaRResult<(ReleaseRecord, bool)> {
        let at = at.filter(|text| !text.trim().is_empty()).unwrap_or_else(|| Utc::now().to_rfc3339());
        Self::update(tasks_root, id, &ReleasePatch { released_at: Some(Some(at)), ..ReleasePatch::default() })
    }

    /// Mark a released release as planned again.
    pub fn reopen(tasks_root: &Path, id: u32) -> LoTaRResult<(ReleaseRecord, bool)> {
        Self::update(tasks_root, id, &ReleasePatch { released_at: Some(None), ..ReleasePatch::default() })
    }

    /// Delete a release and take it off its tickets. Returns the tickets
    /// that were in it.
    pub fn delete(storage: &mut Storage, id: u32) -> LoTaRResult<Vec<String>> {
        let path = Release::path_for_id(&storage.root_path, id);
        if !path.exists() {
            return Err(LoTaRError::ReleaseNotFound(id));
        }
        let members: Vec<String> = Self::tasks_in(storage, id).into_iter().map(|(task_id, _)| task_id).collect();
        for task_id in &members {
            let task = TaskService::get(storage, task_id, None)?;
            let releases: Vec<u32> = task.releases.iter().copied().filter(|release| *release != id).collect();
            TaskService::update_with_receipt(storage, task_id, releases_patch(releases), TaskUpdateContext::default())?;
        }
        let dir = Release::dir(&storage.root_path);
        crate::storage::safety::with_storage_lock(&dir, RELEASE_LOCK_NAME, || fs::remove_file(&path).map_err(LoTaRError::from))?;
        Ok(members)
    }

    /// Add tickets to a release, move them into it (leaving their other
    /// releases), or remove them from it.
    pub fn assign(storage: &mut Storage, reference: &str, tasks: &[String], action: ReleaseAssignmentAction) -> Result<ReleaseAssignmentOutcome, String> {
        if tasks.is_empty() {
            return Err("Provide at least one task identifier via arguments or --select filters.".to_string());
        }
        let record = Self::resolve(&storage.root_path, reference).map_err(|error| error.to_string())?;
        let mut modified = Vec::new();
        let mut unchanged = Vec::new();
        let mut seen = BTreeSet::new();
        for raw in tasks {
            let task_id = crate::services::sprint_assignment::resolve_task_identifier(storage, raw)?;
            if !seen.insert(task_id.clone()) {
                continue;
            }
            let task = TaskService::get(storage, &task_id, None).map_err(|error| error.to_string())?;
            let mut releases: Vec<u32> = match action {
                ReleaseAssignmentAction::Add => task.releases.iter().copied().chain([record.id]).collect(),
                ReleaseAssignmentAction::Move => vec![record.id],
                ReleaseAssignmentAction::Remove => task.releases.iter().copied().filter(|id| *id != record.id).collect(),
            };
            releases.sort_unstable();
            releases.dedup();
            if releases == task.releases {
                unchanged.push(task_id);
                continue;
            }
            TaskService::update_with_receipt(storage, &task_id, releases_patch(releases), TaskUpdateContext::default()).map_err(|error| error.to_string())?;
            modified.push(task_id);
        }
        Ok(ReleaseAssignmentOutcome { action, record, modified, unchanged })
    }

    /// The tickets in a release.
    pub fn tasks_in(storage: &Storage, id: u32) -> Vec<(String, TaskDTO)> {
        let mut tasks = TaskService::list(storage, &crate::api_types::TaskListFilter::default());
        tasks.retain(|(_, task)| task.releases.contains(&id));
        tasks
    }

    /// Open tickets in no release that a task query selects.
    pub fn backlog(
        storage: &Storage,
        mut input: crate::services::task_list::TaskQueryInput,
        config: &crate::config::types::ResolvedConfig,
    ) -> Result<Vec<(String, TaskDTO)>, crate::services::task_list::TaskQueryError> {
        input.releases.push("none".to_string());
        let query = input.resolve(config, &storage.root_path)?;
        let mut tasks = crate::services::task_list::run(storage, &query, Utc::now());
        tasks.retain(|(_, task)| !task.task_state.as_ref().is_some_and(|state| state.is_done));
        Ok(tasks)
    }

    /// The tickets named plus those a search selects, for add / move /
    /// remove over REST and MCP.
    pub fn selected_tasks(
        storage: &Storage,
        config: &crate::config::types::ResolvedConfig,
        tasks: &[String],
        query: Option<&str>,
    ) -> Result<Vec<String>, crate::services::task_list::TaskQueryError> {
        let mut selected = tasks.to_vec();
        if let Some(search) = query.filter(|search| !search.trim().is_empty()) {
            selected.extend(crate::services::task_list::select_ids(storage, config, &storage.root_path, search)?);
        }
        selected.sort();
        selected.dedup();
        Ok(selected)
    }

    /// A release's list item and its tickets.
    pub fn detail(storage: &Storage, record: &ReleaseRecord) -> (ReleaseListItem, Vec<TaskDTO>) {
        let tasks = Self::tasks_in(storage, record.id);
        let item = Self::list_item(record, &tasks);
        (item, tasks.into_iter().map(|(_, task)| task).collect())
    }

    /// Check that every ID names a release, for task create and update.
    pub fn ensure_exist(tasks_root: &Path, ids: &[u32]) -> LoTaRResult<()> {
        for id in ids {
            if !Release::path_for_id(tasks_root, *id).exists() {
                return Err(LoTaRError::ReleaseNotFound(*id));
            }
        }
        Ok(())
    }

    /// The list item for a release, with counts over its tickets.
    pub fn list_item(record: &ReleaseRecord, tasks: &[(String, TaskDTO)]) -> ReleaseListItem {
        let members: Vec<&TaskDTO> = tasks.iter().map(|(_, task)| task).filter(|task| task.releases.contains(&record.id)).collect();
        let done = members.iter().filter(|task| task.task_state.as_ref().is_some_and(|state| state.is_done)).count();
        let release = &record.release;
        let today = chrono::Local::now().date_naive();
        let overdue = !release.is_released()
            && release.planned_date.as_deref().and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()).is_some_and(|date| date < today);
        ReleaseListItem {
            id: record.id,
            version: release.version.clone(),
            label: release.label.clone(),
            display_name: release.display_name(),
            notes: release.notes.clone(),
            planned_date: release.planned_date.clone(),
            released_at: release.released_at.clone(),
            state: if release.is_released() { "released" } else { "planned" }.to_string(),
            overdue,
            task_count: members.len(),
            done_count: done,
            open_count: members.len() - done,
            created: release.created.clone(),
            modified: release.modified.clone(),
        }
    }

    /// List items for every release, counted over every ticket once.
    pub fn list_items(storage: &Storage) -> LoTaRResult<Vec<ReleaseListItem>> {
        let records = Self::list(&storage.root_path)?;
        if records.is_empty() {
            return Ok(Vec::new());
        }
        let mut tasks = TaskService::list(storage, &crate::api_types::TaskListFilter::default());
        tasks.retain(|(_, task)| !task.releases.is_empty());
        Ok(records.iter().map(|record| Self::list_item(record, &tasks)).collect())
    }

    fn load(path: &Path) -> LoTaRResult<Release> {
        let mut release: Release = serde_yaml_ng::from_str(&fs::read_to_string(path)?)?;
        release.canonicalize();
        Ok(release)
    }
}

pub fn resolve_in<'a>(records: &'a [ReleaseRecord], reference: &str) -> LoTaRResult<&'a ReleaseRecord> {
    let value = reference.trim();
    if value.is_empty() {
        return Err(LoTaRError::ValidationError("Name a release: its ID, version or label".to_string()));
    }
    if let Some(id) = parse_id(value) {
        return records.iter().find(|record| record.id == id).ok_or(LoTaRError::ReleaseNotFound(id));
    }
    let version = Version::parse(value).ok().map(|version| version.as_str().to_string());
    records
        .iter()
        .find(|record| version.as_deref() == Some(record.release.version.as_str()))
        .or_else(|| records.iter().find(|record| record.release.label.as_deref().is_some_and(|label| label.eq_ignore_ascii_case(value))))
        .ok_or_else(|| LoTaRError::ValidationError(format!("No release '{value}': name one by ID, version or label (`lotar release list`)")))
}

fn releases_patch(releases: Vec<u32>) -> TaskUpdate {
    TaskUpdate { releases: Some(releases), ..TaskUpdate::default() }
}

fn sort_records(records: &mut [ReleaseRecord]) {
    records.sort_by(|a, b| match (a.release.parsed_version(), b.release.parsed_version()) {
        (Some(va), Some(vb)) => va.sort_cmp(&vb).then(a.id.cmp(&b.id)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.id.cmp(&b.id),
    });
}

fn normalize_version(raw: &str) -> LoTaRResult<String> {
    Version::parse(raw).map(|version| version.as_str().to_string()).map_err(LoTaRError::ValidationError)
}

fn ensure_unique_version(records: &[ReleaseRecord], version: &str, except: Option<u32>) -> LoTaRResult<()> {
    match records.iter().find(|record| Some(record.id) != except && record.release.version == version) {
        Some(existing) => Err(LoTaRError::ValidationError(format!("Release #{} already has version {version}", existing.id))),
        None => Ok(()),
    }
}

fn validate_dates(release: &Release) -> LoTaRResult<()> {
    if let Some(date) = release.planned_date.as_deref()
        && chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err()
    {
        return Err(LoTaRError::ValidationError(format!("planned_date '{date}' is not a date: use YYYY-MM-DD")));
    }
    if let Some(at) = release.released_at.as_deref()
        && chrono::DateTime::parse_from_rfc3339(at).is_err()
        && chrono::NaiveDate::parse_from_str(at, "%Y-%m-%d").is_err()
    {
        return Err(LoTaRError::ValidationError(format!("released_at '{at}' is not a date or RFC 3339 timestamp")));
    }
    Ok(())
}

fn diff(before: &Release, after: &Release) -> Vec<TaskChange> {
    let mut changes = Vec::new();
    let mut push = |field: &str, old: Option<&String>, new: Option<&String>| {
        if old != new {
            changes.push(TaskChange { field: field.to_string(), old: old.cloned(), new: new.cloned() });
        }
    };
    push("version", Some(&before.version), Some(&after.version));
    push("label", before.label.as_ref(), after.label.as_ref());
    push("notes", before.notes.as_ref(), after.notes.as_ref());
    push("planned_date", before.planned_date.as_ref(), after.planned_date.as_ref());
    push("released_at", before.released_at.as_ref(), after.released_at.as_ref());
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u32, version: &str, label: Option<&str>) -> ReleaseRecord {
        ReleaseRecord { id, release: Release { version: version.into(), label: label.map(str::to_string), ..Release::default() } }
    }

    #[test]
    fn filter_resolves_ids_versions_ranges_labels_and_none() {
        let records = vec![record(1, "0.14.2", None), record(2, "0.15.0", Some("Spring")), record(3, "0.15.1", None)];
        let resolve = |values: &[&str]| ReleaseFilter::resolve(&values.iter().map(|v| v.to_string()).collect::<Vec<_>>(), &records).unwrap();
        assert_eq!(resolve(&["#1"]).ids, BTreeSet::from([1]));
        assert_eq!(resolve(&["3"]).ids, BTreeSet::from([3]));
        assert_eq!(resolve(&["0.15"]).ids, BTreeSet::from([2, 3]));
        assert_eq!(resolve(&["v0.15.1"]).ids, BTreeSet::from([3]));
        assert_eq!(resolve(&[">=0.14.2 <0.15.1"]).ids, BTreeSet::from([1, 2]));
        assert_eq!(resolve(&["spring"]).ids, BTreeSet::from([2]));
        let none = resolve(&["none"]);
        assert!(none.none && none.matches(&[]) && !none.matches(&[1]));
        assert!(ReleaseFilter::resolve(&["not a version".to_string()], &records).is_err());
    }

    #[test]
    fn resolve_in_finds_by_id_version_and_label() {
        let records = vec![record(1, "0.14.2", None), record(2, "0.15.0", Some("Spring"))];
        assert_eq!(resolve_in(&records, "#2").unwrap().id, 2);
        assert_eq!(resolve_in(&records, "v0.14.2").unwrap().id, 1);
        assert_eq!(resolve_in(&records, "SPRING").unwrap().id, 2);
        assert!(matches!(resolve_in(&records, "9"), Err(LoTaRError::ReleaseNotFound(9))));
        assert!(resolve_in(&records, "0.16").is_err());
    }

    #[test]
    fn records_sort_in_version_order() {
        let mut records = vec![record(1, "0.10.0", None), record(2, "0.9.1", None), record(3, "garbage", None), record(4, "0.9.0", None)];
        sort_records(&mut records);
        assert_eq!(records.iter().map(|r| r.id).collect::<Vec<_>>(), vec![4, 2, 1, 3]);
    }
}
