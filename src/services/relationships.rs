//! Canonical relationship graph and bounded, transactional relationship edits.
//!
//! Advisory locks coordinate participating LoTaR writers, not external editors.
//! Read snapshots initialize only advisory-lock infrastructure, never task or
//! configuration data and never journal recovery. Pending recovery or an
//! unavailable coherent inventory is an error, never a clean check.
//! Standalone snapshots retry writer contention for at most 500 ms, releasing
//! every partial lock set before sleeping. External context under a local writer
//! is fail-fast to avoid circular waits between workspace mutations.
use crate::api_types::*;
use crate::errors::{LoTaRError, LoTaRResult};
use crate::storage::TaskId;
use crate::storage::manager::Storage;
use crate::storage::task::Task;
use crate::storage::transaction::MultiFileTransaction;
use crate::types::{TaskChange, TaskChangeLogEntry, TaskRelationships};
use fs2::FileExt;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

type Edge = (String, String, RelationshipKind);

#[derive(Clone)]
struct Record {
    root: PathBuf,
    path: PathBuf,
    task: Option<Task>,
}

struct Inventory {
    records: BTreeMap<String, Vec<Record>>,
    /// Move stubs (DEV-183): old ID -> the ID its task moved to.
    moved: BTreeMap<String, String>,
    revision: String,
    local_root: PathBuf,
    sources: BTreeMap<PathBuf, Vec<String>>,
}

struct ReadSnapshot {
    _locks: Vec<File>,
    projects: BTreeMap<PathBuf, Vec<String>>,
}

impl ReadSnapshot {
    fn load(&self, storage: &Storage) -> LoTaRResult<Inventory> {
        let inventory = Inventory::load(storage)?;
        if inventory.sources != self.projects {
            return Err(invalid(
                "Relationship snapshot unavailable: project inventory changed during inspection (loaded source manifest differs from locked manifest)",
            ));
        }
        let current_roots = roots(storage)?;
        if current_roots != self.projects.keys().cloned().collect::<Vec<_>>() {
            return Err(invalid(
                "Relationship snapshot unavailable: workspace roots changed during inspection",
            ));
        }
        for (root, projects) in &self.projects {
            if *projects != MultiFileTransaction::project_prefixes(root)? {
                return Err(invalid(
                    "Relationship snapshot unavailable: project inventory changed during inspection",
                ));
            }
        }
        Ok(inventory)
    }
}

pub(crate) struct ExternalSnapshot(ReadSnapshot);

impl ExternalSnapshot {
    pub(crate) fn verify(&self, storage: &Storage) -> LoTaRResult<()> {
        let local = canonical_root(&storage.root_path);
        let current: Vec<_> = roots(storage)?
            .into_iter()
            .filter(|root| *root != local)
            .collect();
        if current != self.0.projects.keys().cloned().collect::<Vec<_>>() {
            return Err(invalid(
                "Relationship snapshot unavailable: external roots changed after lock acquisition",
            ));
        }
        for (root, projects) in &self.0.projects {
            if *projects != MultiFileTransaction::project_prefixes(root)? {
                return Err(invalid(
                    "Relationship snapshot unavailable: external project manifest changed after lock acquisition",
                ));
            }
        }
        Ok(())
    }

    fn load(&self, storage: &Storage) -> LoTaRResult<Inventory> {
        self.verify(storage)?;
        let inventory = Inventory::load(storage)?;
        let external: BTreeMap<_, _> = inventory
            .sources
            .iter()
            .filter(|(root, _)| **root != inventory.local_root)
            .map(|(root, projects)| (root.clone(), projects.clone()))
            .collect();
        if external != self.0.projects {
            return Err(invalid(
                "Relationship snapshot unavailable: loaded external sources differ from locked manifest",
            ));
        }
        self.verify(storage)?;
        Ok(inventory)
    }
}

fn invalid(message: impl Into<String>) -> LoTaRError {
    LoTaRError::ValidationError(message.into())
}

fn canonical_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

fn roots(storage: &Storage) -> LoTaRResult<Vec<PathBuf>> {
    let selected_root = storage.root_path.canonicalize().map_err(|error| {
        invalid(format!(
            "Relationship snapshot unavailable: cannot resolve tasks root {}: {error}",
            storage.root_path.display()
        ))
    })?;
    let mut roots = vec![selected_root.clone()];
    if !roots[0].is_dir() {
        return Err(invalid("Relationship tasks root is not a directory"));
    }
    if let Some(parent) = selected_root.parent() {
        for entry in fs::read_dir(parent).map_err(|error| invalid(format!("Relationship snapshot unavailable: cannot enumerate workspace directory {}: {error}", parent.display())))? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name.contains('@') || name.eq_ignore_ascii_case("sprints") {
                continue;
            }
            let ty = entry.file_type()?;
            let is_dir = if ty.is_symlink() {
                fs::metadata(entry.path()).map_err(|error| invalid(format!("Relationship snapshot unavailable: cannot inspect workspace source {}: {error}", entry.path().display())))?.is_dir()
            } else {
                ty.is_dir()
            };
            if is_dir {
                let candidate = entry.path().join(".tasks");
                match fs::metadata(&candidate) {
                    Ok(metadata) if metadata.is_dir() => roots.push(candidate.canonicalize()?),
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(invalid(format!("Relationship snapshot unavailable: cannot inspect tasks root {}: {e}", candidate.display()))),
                }
            }
        }
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

/// Lock-file creation is synchronization infrastructure only. In particular,
/// this never delegates to transaction begin, which could recover task data.
fn read_lock(path: &Path, busy: &mut bool) -> LoTaRResult<File> {
    read_lock_with(path, busy, FileExt::try_lock_shared)
}

fn read_lock_with(
    path: &Path,
    busy: &mut bool,
    acquire: impl FnOnce(&File) -> std::io::Result<()>,
) -> LoTaRResult<File> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_file()) {
        return Err(invalid(format!(
            "Relationship snapshot refuses symlink or nonregular lock {}",
            path.display()
        )));
    }
    let mut options = File::options();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Refuse a symlink/FIFO swapped in between inspection and open too.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| {
        invalid(format!(
            "Relationship snapshot unavailable: cannot open synchronization lock {}: {e}",
            path.display()
        ))
    })?;
    if !file.metadata()?.is_file() {
        return Err(invalid("Relationship snapshot refuses nonregular lock"));
    }
    acquire(&file).map_err(|e| {
        *busy = e.kind() == std::io::ErrorKind::WouldBlock
            || e.raw_os_error()
                .is_some_and(|code| Some(code) == fs2::lock_contended_error().raw_os_error());
        invalid(format!(
            "Relationship snapshot unavailable: lock {} is busy or unsupported: {e}",
            path.display()
        ))
    })?;
    Ok(file)
}

fn snapshot_locks(storage: &Storage) -> LoTaRResult<ReadSnapshot> {
    snapshot_locks_with_wait(storage, std::thread::sleep)
}

fn snapshot_locks_with_wait(
    storage: &Storage,
    mut wait: impl FnMut(Duration),
) -> LoTaRResult<ReadSnapshot> {
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        let mut busy = false;
        // Re-discover roots and manifests on each attempt. An error drops the
        // entire partial set before retrying, including coordinator locks.
        match lock_roots(&roots(storage)?, &mut busy) {
            Err(_) if busy && Instant::now() < deadline => {
                wait(
                    Duration::from_millis(10)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Err(error) if busy => {
                return Err(invalid(format!(
                    "{error}; still busy after the 500 ms snapshot wait budget; retry after the writer finishes"
                )));
            }
            result => return result,
        }
    }
}

/// External roots are read-only context, but their assertions and identity
/// collisions must not change during local cycle validation and publication.
pub(crate) fn external_locks(storage: &Storage) -> LoTaRResult<ExternalSnapshot> {
    let local = canonical_root(&storage.root_path);
    let external: Vec<_> = roots(storage)?
        .into_iter()
        .filter(|r| *r != local)
        .collect();
    lock_roots(&external, &mut false).map(ExternalSnapshot)
}

fn lock_roots(roots: &[PathBuf], busy: &mut bool) -> LoTaRResult<ReadSnapshot> {
    let mut locks = Vec::new();
    let mut projects = BTreeMap::new();
    for root in roots {
        let coordinator_dir = root.join("@sprints");
        if fs::symlink_metadata(&coordinator_dir).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err(invalid(
                "Relationship snapshot refuses a symlink coordinator directory",
            ));
        }
        fs::create_dir_all(&coordinator_dir).map_err(|error| invalid(format!("Relationship snapshot unavailable: cannot initialize synchronization directory {}: {error}", coordinator_dir.display())))?;
        locks.push(read_lock(&root.join("@sprints/.sprints.lock"), busy)?);
        if root
            .join(crate::storage::transaction::JOURNAL_FILE_NAME)
            .try_exists()?
        {
            return Err(invalid(format!(
                "Relationship snapshot unavailable: pending transaction journal in {}; recovery must run through a mutation",
                root.display()
            )));
        }
    }
    for root in roots {
        let prefixes = MultiFileTransaction::project_prefixes(root)?;
        for project in &prefixes {
            locks.push(read_lock(&root.join(project).join(".task.lock"), busy)?);
        }
        projects.insert(root.clone(), prefixes);
    }
    Ok(ReadSnapshot {
        _locks: locks,
        projects,
    })
}

impl Inventory {
    fn load(storage: &Storage) -> LoTaRResult<Self> {
        let mut records: BTreeMap<String, Vec<Record>> = BTreeMap::new();
        let mut hash = blake3::Hasher::new();
        let mut count = 0;
        let mut sources = BTreeMap::new();
        for root in roots(storage)? {
            hash.update(root.as_os_str().as_encoded_bytes());
            let mut paths = Vec::new();
            let projects = MultiFileTransaction::project_prefixes(&root)?;
            sources.insert(root.clone(), projects.clone());
            for project in projects {
                let directory = root.join(&project);
                let unavailable = |error: std::io::Error| {
                    invalid(format!(
                        "Relationship snapshot unavailable: cannot enumerate task directory {}: {error}",
                        directory.display()
                    ))
                };
                fs::read_dir(&directory).map_err(unavailable)?;
                // Archived tasks included: links to them must still resolve (DEV-4).
                for file in crate::storage::layout::task_files(
                    &directory,
                    crate::storage::layout::ArchiveFilter::All,
                ) {
                    paths.push((project.clone(), file.path));
                }
                let config = crate::utils::paths::project_config_path(&root, &project);
                hash_file(&mut hash, &config)?;
            }
            hash_file(&mut hash, &crate::utils::paths::global_config_path(&root))?;
            paths.sort();
            for (project, path) in paths {
                count += 1;
                if count > 10_000 {
                    return Err(invalid(
                        "Relationship inventory exceeds 10000 files; snapshot unavailable",
                    ));
                }
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                let raw = format!("{project}-{stem}");
                let Ok(parsed) = TaskId::parse(&raw) else {
                    // Configuration and other non-task YAML files are not task identities.
                    continue;
                };
                hash.update(path.as_os_str().as_encoded_bytes());
                let task = if fs::symlink_metadata(&path).map_err(|error| invalid(format!("Relationship snapshot unavailable: cannot inspect task source {}: {error}", path.display())))?.is_symlink() {
                    hash.update(b"symlink");
                    None
                } else {
                    match fs::read(&path) {
                        Ok(bytes) => {
                            hash.update(&bytes);
                            serde_yaml_ng::from_slice::<Task>(&bytes).ok()
                        }
                        Err(error) => {
                            hash.update(error.to_string().as_bytes());
                            None
                        }
                    }
                };
                records.entry(parsed.canonical()).or_default().push(Record {
                    root: root.clone(),
                    path,
                    task,
                });
            }
        }
        let assertion_count: usize = records
            .values()
            .flatten()
            .filter_map(|record| record.task.as_ref())
            .map(|task| {
                let rel = &task.relationships;
                rel.depends_on.len()
                    + rel.blocks.len()
                    + rel.children.len()
                    + rel.related.len()
                    + rel.fixes.len()
                    + usize::from(rel.parent.is_some())
                    + usize::from(rel.duplicate_of.is_some())
            })
            .sum();
        if assertion_count > 50_000 {
            return Err(invalid(
                "Relationship inventory exceeds 50000 assertions; snapshot unavailable",
            ));
        }
        let moved = records
            .iter()
            .filter_map(|(id, list)| {
                let [record] = list.as_slice() else {
                    return None;
                };
                let target = record.task.as_ref()?.moved_to.as_deref()?;
                Some((id.clone(), TaskId::parse(target.trim()).ok()?.canonical()))
            })
            .collect();
        Ok(Self {
            records,
            moved,
            revision: hash.finalize().to_hex().to_string(),
            local_root: canonical_root(&storage.root_path),
            sources,
        })
    }

    /// The current ID of a canonical ID: old IDs of moved tasks follow their
    /// stubs (DEV-183), so every relationship naming an old ID means the task
    /// it became. Loops and overlong chains stop where they are.
    fn current(&self, canonical: String) -> String {
        let mut id = canonical;
        for _ in 0..crate::storage::moves::MAX_MOVE_HOPS {
            match self.moved.get(&id) {
                Some(next) if next != &id => id = next.clone(),
                _ => break,
            }
        }
        id
    }

    fn unique(&self, id: &str) -> Option<&Record> {
        let parsed = TaskId::parse(id).ok()?;
        let records = self.records.get(&self.current(parsed.canonical()))?;
        (records.len() == 1).then(|| &records[0])
    }

    fn key(&self, raw: &str) -> String {
        if self.unique(raw).is_some() {
            self.current(TaskId::parse(raw).unwrap().canonical())
        } else {
            raw.to_string()
        }
    }

    fn state(&self, id: &str) -> &'static str {
        let Ok(parsed) = TaskId::parse(id) else {
            return "missing";
        };
        match self.records.get(&self.current(parsed.canonical())) {
            None => "missing",
            Some(records) if records.len() != 1 => "ambiguous",
            Some(records) => match &records[0].task {
                None => "unreadable",
                Some(task) if task.deleted_at.is_some() => "deleted",
                Some(_) => "active",
            },
        }
    }

    fn active_local(&self, root: &Path, raw: &str) -> LoTaRResult<String> {
        let id = self.current(
            TaskId::parse(raw)
                .map_err(|e| invalid(format!("Invalid relationship task '{raw}': {e}")))?
                .canonical(),
        );
        if self.state(&id) != "active" {
            return Err(invalid(format!(
                "Relationship task '{raw}' is {}",
                self.state(&id)
            )));
        }
        if self.unique(&id).unwrap().root != canonical_root(root) {
            return Err(invalid(format!(
                "Cross-root relationship writes are not supported for '{raw}'; external context is read-only"
            )));
        }
        Ok(id)
    }

    fn assertions(&self) -> BTreeMap<Edge, Vec<RelationshipProvenance>> {
        let mut edges: BTreeMap<Edge, Vec<RelationshipProvenance>> = BTreeMap::new();
        for (id, records) in &self.records {
            if records.len() != 1 {
                continue;
            }
            if let Some(task) = &records[0].task {
                for (edge, field, raw) in assertions(id, &task.relationships, self) {
                    edges.entry(edge).or_default().push(RelationshipProvenance {
                        task_id: id.clone(),
                        field: field.into(),
                        value: raw,
                    });
                }
            }
        }
        edges
    }
}

fn hash_file(hash: &mut blake3::Hasher, path: &Path) -> LoTaRResult<()> {
    hash.update(path.as_os_str().as_encoded_bytes());
    match fs::read(path) {
        Ok(bytes) => {
            hash.update(&bytes);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            hash.update(b"absent");
        }
        Err(e) => {
            return Err(invalid(format!(
                "Relationship snapshot unavailable: cannot read configuration source {}: {e}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn edge(source: String, target: String, kind: RelationshipKind) -> Edge {
    if kind == RelationshipKind::Related && source > target {
        (target, source, kind)
    } else {
        (source, target, kind)
    }
}

fn field_edge(source: String, target: String, field: RelationshipField) -> Edge {
    match field {
        RelationshipField::DependsOn => edge(target, source, RelationshipKind::Dependency),
        RelationshipField::Blocks => edge(source, target, RelationshipKind::Dependency),
        RelationshipField::Parent => edge(target, source, RelationshipKind::Hierarchy),
        RelationshipField::Children => edge(source, target, RelationshipKind::Hierarchy),
        RelationshipField::Related => edge(source, target, RelationshipKind::Related),
        RelationshipField::Fixes => edge(source, target, RelationshipKind::Fixes),
        RelationshipField::DuplicateOf => edge(source, target, RelationshipKind::Duplicate),
    }
}

fn assertions(
    id: &str,
    rel: &TaskRelationships,
    inventory: &Inventory,
) -> Vec<(Edge, &'static str, String)> {
    let mut out = Vec::new();
    for (field, values, kind, reverse) in [
        (
            "depends_on",
            &rel.depends_on,
            RelationshipKind::Dependency,
            true,
        ),
        ("blocks", &rel.blocks, RelationshipKind::Dependency, false),
        ("related", &rel.related, RelationshipKind::Related, false),
        (
            "children",
            &rel.children,
            RelationshipKind::Hierarchy,
            false,
        ),
        ("fixes", &rel.fixes, RelationshipKind::Fixes, false),
    ] {
        for raw in values {
            let target = inventory.key(raw);
            let (source, target) = if reverse {
                (target, id.to_string())
            } else {
                (id.to_string(), target)
            };
            out.push((edge(source, target, kind), field, raw.clone()));
        }
    }
    for (field, raw, kind, reverse) in [
        ("parent", &rel.parent, RelationshipKind::Hierarchy, true),
        (
            "duplicate_of",
            &rel.duplicate_of,
            RelationshipKind::Duplicate,
            false,
        ),
    ] {
        if let Some(raw) = raw {
            let target = inventory.key(raw);
            let (source, target) = if reverse {
                (target, id.to_string())
            } else {
                (id.to_string(), target)
            };
            out.push((edge(source, target, kind), field, raw.clone()));
        }
    }
    out
}

fn issue(code: &str, tasks: Vec<String>, repairable: bool, message: String) -> RelationshipIssue {
    let mut identity_tasks = tasks.clone();
    identity_tasks.sort();
    identity_tasks.dedup();
    let identity = serde_json::to_vec(&(code, identity_tasks, &message)).unwrap();
    RelationshipIssue {
        id: blake3::hash(&identity).to_hex().to_string(),
        code: code.into(),
        severity: if repairable { "warning" } else { "error" }.into(),
        message,
        tasks,
        repairable,
    }
}

/// Iterative strongly-connected components keep checks bounded by graph size,
/// rather than doing a graph traversal for every assertion or recursing on
/// user-controlled chain depth. Each directional category is independent.
fn conflicting_edges(edges: &BTreeSet<Edge>) -> BTreeSet<Edge> {
    let mut bad: BTreeSet<_> = edges.iter().filter(|e| e.0 == e.1).cloned().collect();
    for kind in [
        RelationshipKind::Dependency,
        RelationshipKind::Hierarchy,
        RelationshipKind::Duplicate,
    ] {
        let mut forward: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut reverse: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (source, target, _) in edges.iter().filter(|e| e.2 == kind) {
            forward.entry(source).or_default().push(target);
            forward.entry(target).or_default();
            reverse.entry(target).or_default().push(source);
        }
        let mut visited = BTreeSet::new();
        let mut order = Vec::new();
        for start in forward.keys() {
            let mut stack = vec![(*start, false)];
            while let Some((id, exiting)) = stack.pop() {
                if exiting {
                    order.push(id);
                    continue;
                }
                if !visited.insert(id) {
                    continue;
                }
                stack.push((id, true));
                for next in &forward[id] {
                    if !visited.contains(next) {
                        stack.push((next, false));
                    }
                }
            }
        }
        let mut component = BTreeMap::new();
        for (index, start) in order.into_iter().rev().enumerate() {
            if component.contains_key(start) {
                continue;
            }
            let mut stack = vec![start];
            while let Some(id) = stack.pop() {
                if component.contains_key(id) {
                    continue;
                }
                component.insert(id, index);
                if let Some(previous) = reverse.get(id) {
                    stack.extend(previous);
                }
            }
        }
        for e in edges.iter().filter(|e| e.2 == kind) {
            if component.get(e.0.as_str()) == component.get(e.1.as_str()) {
                bad.insert(e.clone());
            }
        }
        if matches!(
            kind,
            RelationshipKind::Hierarchy | RelationshipKind::Duplicate
        ) {
            let mut counts = BTreeMap::new();
            for e in edges.iter().filter(|e| e.2 == kind) {
                *counts
                    .entry(if kind == RelationshipKind::Hierarchy {
                        &e.1
                    } else {
                        &e.0
                    })
                    .or_insert(0usize) += 1;
            }
            for e in edges.iter().filter(|e| e.2 == kind) {
                if counts[if kind == RelationshipKind::Hierarchy {
                    &e.1
                } else {
                    &e.0
                }] > 1
                {
                    bad.insert(e.clone());
                }
            }
        }
    }
    bad
}

/// DEV-16: whether links added to this task are also written to the other
/// task (`auto.reciprocal_relationships`, on by default).
fn reciprocal_links(root: &Path, task_id: &str) -> bool {
    let Ok(id) = TaskId::parse(task_id) else {
        return true;
    };
    crate::config::resolution::config_for_project(root, Some(&id.project))
        .map(|config| config.auto_reciprocal_relationships)
        .unwrap_or(true)
}

/// Projects whose tasks do not get the other side of links written; their
/// one-sided links are expected, not missing mirrors.
fn one_sided_projects(inventory: &Inventory) -> BTreeSet<String> {
    let mut seen = BTreeMap::<String, bool>::new();
    for (id, records) in &inventory.records {
        let Ok(parsed) = TaskId::parse(id) else {
            continue;
        };
        if seen.contains_key(&parsed.project) {
            continue;
        }
        let root = records
            .first()
            .map(|record| record.root.clone())
            .unwrap_or_else(|| inventory.local_root.clone());
        let on = crate::config::resolution::config_for_project(&root, Some(&parsed.project))
            .map(|config| config.auto_reciprocal_relationships)
            .unwrap_or(true);
        seen.insert(parsed.project, on);
    }
    seen.into_iter()
        .filter(|(_, on)| !on)
        .map(|(project, _)| project)
        .collect()
}

fn one_sided(projects: &BTreeSet<String>, e: &Edge) -> bool {
    !projects.is_empty()
        && [&e.0, &e.1]
            .iter()
            .any(|id| TaskId::parse(id).is_ok_and(|id| projects.contains(&id.project)))
}

fn mirrors(e: &Edge) -> Vec<(String, &'static str, String)> {
    let (source, target, kind) = e;
    match kind {
        RelationshipKind::Dependency => vec![
            (source.clone(), "blocks", target.clone()),
            (target.clone(), "depends_on", source.clone()),
        ],
        RelationshipKind::Hierarchy => vec![
            (source.clone(), "children", target.clone()),
            (target.clone(), "parent", source.clone()),
        ],
        RelationshipKind::Related => vec![
            (source.clone(), "related", target.clone()),
            (target.clone(), "related", source.clone()),
        ],
        RelationshipKind::Fixes => vec![(source.clone(), "fixes", target.clone())],
        RelationshipKind::Duplicate => vec![(source.clone(), "duplicate_of", target.clone())],
    }
}

// Apply selected alias substitutions before deduplication, independently of
// request order. Batch repair deliberately keeps its combined normalization.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum RepairAction {
    PaddedAlias {
        owner: String,
        field: &'static str,
        target: String,
        raw: String,
    },
    DuplicateAssertion {
        owner: String,
        field: &'static str,
        target: String,
    },
    MissingMirror {
        owner: String,
        field: &'static str,
        target: String,
    },
}

impl RepairAction {
    fn assertion(&self) -> (&str, &str, &str) {
        match self {
            Self::PaddedAlias {
                owner,
                field,
                target,
                ..
            }
            | Self::DuplicateAssertion {
                owner,
                field,
                target,
            }
            | Self::MissingMirror {
                owner,
                field,
                target,
            } => (owner, field, target),
        }
    }

    fn apply(&self, rel: &mut TaskRelationships, inventory: &Inventory) {
        let (_, field, target) = self.assertion();
        let scalar = match field {
            "parent" => Some(&mut rel.parent),
            "duplicate_of" => Some(&mut rel.duplicate_of),
            _ => None,
        };
        if let Some(scalar) = scalar {
            match self {
                Self::PaddedAlias { raw, .. } if scalar.as_ref() == Some(raw) => {
                    *scalar = Some(target.into())
                }
                Self::MissingMirror { .. } if scalar.is_none() => *scalar = Some(target.into()),
                _ => {}
            }
            return;
        }
        let values = match field {
            "blocks" => &mut rel.blocks,
            "depends_on" => &mut rel.depends_on,
            "children" => &mut rel.children,
            "related" => &mut rel.related,
            "fixes" => &mut rel.fixes,
            _ => unreachable!(),
        };
        match self {
            Self::PaddedAlias { raw, .. } => {
                for value in values.iter_mut().filter(|value| *value == raw) {
                    *value = target.into();
                }
            }
            Self::DuplicateAssertion { .. } => {
                let mut found = false;
                values.retain(|value| {
                    if inventory.key(value) != target {
                        return true;
                    }
                    let keep = !found;
                    found = true;
                    keep
                });
            }
            Self::MissingMirror { .. } => {
                if !values.iter().any(|value| inventory.key(value) == target) {
                    values.push(target.into());
                }
            }
        }
    }
}

fn inspect(inventory: &Inventory) -> Vec<RelationshipIssue> {
    inspect_actions(inventory).0
}

fn inspect_actions(
    inventory: &Inventory,
) -> (Vec<RelationshipIssue>, BTreeMap<String, RepairAction>) {
    let assertions = inventory.assertions();
    let one_sided_projects = one_sided_projects(inventory);
    let edges: BTreeSet<Edge> = assertions.keys().cloned().collect();
    let conflicting = conflicting_edges(&edges);
    let conflicted_tasks: BTreeSet<_> = conflicting.iter().flat_map(|e| [&e.0, &e.1]).collect();
    let mut parent_counts = BTreeMap::new();
    for e in edges.iter().filter(|e| e.2 == RelationshipKind::Hierarchy) {
        *parent_counts.entry(&e.1).or_insert(0usize) += 1;
    }
    let mut issues = Vec::new();
    let mut actions = BTreeMap::new();
    let fully_known = inventory
        .records
        .keys()
        .all(|id| !matches!(inventory.state(id), "ambiguous" | "unreadable"));
    for id in inventory.records.keys() {
        let state = inventory.state(id);
        if state == "ambiguous" || state == "unreadable" {
            issues.push(issue(
                state,
                vec![id.clone()],
                false,
                format!("Task '{id}' is {state}"),
            ));
        }
    }
    for (e, provenance) in &assertions {
        let tasks = vec![e.0.clone(), e.1.clone()];
        let valid = inventory.state(&e.0) == "active" && inventory.state(&e.1) == "active";
        for id in [&e.0, &e.1] {
            let state = inventory.state(id);
            if state != "active" {
                issues.push(issue(
                    state,
                    tasks.clone(),
                    false,
                    format!("Relationship endpoint '{id}' is {state}"),
                ));
            }
        }
        let conflict = conflicting.contains(e);
        if conflict {
            let code = if e.0 == e.1 {
                "self_link"
            } else if e.2 == RelationshipKind::Hierarchy && parent_counts[&e.1] > 1 {
                "parent_conflict"
            } else {
                "cycle"
            };
            issues.push(issue(
                code,
                tasks.clone(),
                false,
                format!("Conflicting {:?} relationship {} -> {}", e.2, e.0, e.1),
            ));
        }
        let local_pair = inventory
            .unique(&e.0)
            .zip(inventory.unique(&e.1))
            .is_some_and(|(a, b)| a.root == inventory.local_root && b.root == inventory.local_root);
        let safe = valid
            && !conflict
            && local_pair
            && fully_known
            && !conflicted_tasks.contains(&e.0)
            && !conflicted_tasks.contains(&e.1);
        for (owner, field, target) in mirrors(e) {
            let found: Vec<_> = provenance
                .iter()
                .filter(|p| p.task_id == owner && p.field == field)
                .collect();
            if found.is_empty() && !one_sided(&one_sided_projects, e) {
                let diagnostic = issue(
                    "missing_mirror",
                    tasks.clone(),
                    safe,
                    format!("Missing {owner}.{field} = {target}"),
                );
                if safe {
                    actions.insert(
                        diagnostic.id.clone(),
                        RepairAction::MissingMirror {
                            owner: owner.clone(),
                            field,
                            target: target.clone(),
                        },
                    );
                }
                issues.push(diagnostic);
            }
            if found.len() > 1 {
                let diagnostic = issue(
                    "duplicate_assertion",
                    tasks.clone(),
                    safe,
                    format!("Duplicate {owner}.{field} = {target}"),
                );
                if safe {
                    actions.insert(
                        diagnostic.id.clone(),
                        RepairAction::DuplicateAssertion {
                            owner: owner.clone(),
                            field,
                            target: target.clone(),
                        },
                    );
                }
                issues.push(diagnostic);
            }
            for p in found {
                // Only padded spellings: an old ID of a moved task is a valid
                // reference that resolves at run time (DEV-183).
                let padded = TaskId::parse(&p.value).is_ok_and(|id| id.canonical() == target);
                if p.value != target && padded && inventory.unique(&p.value).is_some() {
                    let diagnostic = issue(
                        "padded_alias",
                        tasks.clone(),
                        safe,
                        format!("Canonicalize {owner}.{field}: {} -> {target}", p.value),
                    );
                    if safe {
                        actions.insert(
                            diagnostic.id.clone(),
                            RepairAction::PaddedAlias {
                                owner: owner.clone(),
                                field,
                                target: target.clone(),
                                raw: p.value.clone(),
                            },
                        );
                    }
                    issues.push(diagnostic);
                }
            }
        }
    }
    (issues, actions)
}

pub fn check(storage: &Storage, project: Option<&str>) -> LoTaRResult<RelationshipCheckResponse> {
    let snapshot = snapshot_locks(storage)?;
    validate_scope(storage, project)?;
    let inventory = snapshot.load(storage)?;
    let issues = scoped_issues(inspect(&inventory), project);
    let complete = !inventory
        .records
        .keys()
        .any(|id| matches!(inventory.state(id), "ambiguous" | "unreadable"))
        && !issues.iter().any(|i| i.code == "missing");
    Ok(RelationshipCheckResponse {
        issues,
        revision: inventory.revision,
        complete,
    })
}

fn validate_scope(storage: &Storage, project: Option<&str>) -> LoTaRResult<()> {
    let Some(project) = project else {
        return Ok(());
    };
    crate::storage::safety::validate_project_prefix(project).map_err(invalid)?;
    for root in roots(storage)? {
        if MultiFileTransaction::project_prefixes(&root)?
            .iter()
            .any(|prefix| prefix == project)
        {
            return Ok(());
        }
    }
    Err(invalid(format!(
        "Relationship project scope '{project}' does not exist"
    )))
}

fn scoped_issues(issues: Vec<RelationshipIssue>, project: Option<&str>) -> Vec<RelationshipIssue> {
    issues
        .into_iter()
        .filter(|i| {
            project.is_none_or(|p| {
                i.tasks
                    .iter()
                    .any(|id| TaskId::parse(id).is_ok_and(|id| id.project == p))
            })
        })
        .collect()
}

pub fn graph(
    storage: &Storage,
    request: &RelationshipGraphRequest,
) -> LoTaRResult<RelationshipGraphResponse> {
    if request.depth.is_some_and(|depth| depth > 8) {
        return Err(invalid("Relationship graph depth must be between 0 and 8"));
    }
    if request
        .max_nodes
        .is_some_and(|limit| !(1..=500).contains(&limit))
    {
        return Err(invalid(
            "Relationship graph max_nodes must be between 1 and 500",
        ));
    }
    if let Some(project) = &request.project {
        crate::storage::safety::validate_project_prefix(project).map_err(invalid)?;
    }
    if let Some(id) = &request.task_id {
        TaskId::parse(id).map_err(|e| invalid(format!("Invalid graph task '{id}': {e}")))?;
    }
    let snapshot = snapshot_locks(storage)?;
    validate_scope(storage, request.project.as_deref())?;
    let inventory = snapshot.load(storage)?;
    let assertions = inventory.assertions();
    let projection_edges: Vec<_> = assertions
        .keys()
        .filter(|edge| {
            request
                .relationship_kinds
                .as_ref()
                .is_none_or(|kinds| kinds.contains(&edge.2))
        })
        .cloned()
        .collect();
    let now = chrono::Utc::now();
    let mut query: std::collections::HashMap<_, _> = request.filters.clone().into_iter().collect();
    if let Some(project) = &request.project {
        query.insert("project".into(), project.clone());
    }
    // Like task-list, cross-project validation uses the selected base config;
    // explicit project validation/identity uses that project's actual root.
    let query_root = query
        .get("project")
        .and_then(|project| {
            if inventory
                .sources
                .get(&inventory.local_root)
                .is_some_and(|projects| projects.contains(project))
            {
                Some(&inventory.local_root)
            } else {
                inventory
                    .sources
                    .iter()
                    .find(|(_, projects)| projects.contains(project))
                    .map(|(root, _)| root)
            }
        })
        .unwrap_or(&inventory.local_root);
    let query_config = crate::config::resolution::load_and_merge_configs(Some(query_root))
        .map_err(|error| {
            invalid(format!(
                "Relationship filter configuration unavailable: {error}"
            ))
        })?;
    let crate::services::task_list::TaskQuery {
        filter,
        field_filters: unknown,
        options,
        config: query_config,
        exclude,
    } = crate::services::task_list::TaskQueryInput::from_params(&query)
        .and_then(|input| input.resolve(&query_config, query_root))
        .map_err(|e| invalid(e.to_string()))?;
    // Resolve typed identity against the strict project-scoped inventory, not
    // filtered/title matches: other predicates must not hide numeric ambiguity.
    let exact = if request.relationship_kinds.is_some() || request.include_isolated.is_some() {
        filter.text_query.as_deref().and_then(|text| {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            let full = TaskId::parse(text).ok();
            let numeric = text.bytes().all(|byte| byte.is_ascii_digit());
            let number = text.parse::<u64>().ok();
            let candidates: Vec<_> = inventory
                .records
                .keys()
                .filter(|key| {
                    let Ok(id) = TaskId::parse(key) else {
                        return false;
                    };
                    if filter
                        .project
                        .as_ref()
                        .is_some_and(|project| *project != id.project)
                    {
                        return false;
                    }
                    if let Some(full) = &full {
                        return *full == id;
                    }
                    if numeric {
                        return number == Some(id.number);
                    }
                    text.strip_prefix(&id.project).is_some_and(|suffix| {
                        !suffix.is_empty()
                            && suffix.bytes().all(|byte| byte.is_ascii_digit())
                            && suffix.parse::<u64>().ok() == Some(id.number)
                    })
                })
                .collect();
            (candidates.len() == 1 && inventory.unique(candidates[0]).is_some())
                .then(|| candidates[0].clone())
        })
    } else {
        None
    };
    let storage_filter = crate::storage::TaskFilter {
        status: filter.status.clone(),
        priority: filter.priority.clone(),
        task_type: filter.task_type.clone(),
        project: filter.project.clone(),
        tags: filter.tags.clone(),
        text_query: filter.text_query.clone(),
        custom_fields: filter.custom_fields.clone(),
        deletion: filter.deletion,
        archived: filter.archived,
        sprints: Vec::new(),
    };
    let exact_filter = crate::storage::TaskFilter {
        text_query: None,
        ..storage_filter.clone()
    };
    let mut configs = BTreeMap::new();
    let mut sprint_lookups = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    let mut matching = Vec::new();
    // The locked strict inventory is the sole source of graph membership. In
    // particular, do not call the task-list search (which can omit corrupt IDs).
    for (root, projects) in &inventory.sources {
        let root_storage = Storage {
            root_path: root.clone(),
        };
        let sprints = sprint_lookups.entry(root.clone()).or_insert_with(|| {
            crate::services::task_service::TaskService::sprint_lookup_for_root(&root_storage)
        });
        for project in projects {
            configs.insert(
                (root.clone(), project.clone()),
                crate::config::resolution::config_for_project(root, Some(project)).map_err(
                    |error| {
                        invalid(format!(
                            "Relationship metadata configuration unavailable: {error}"
                        ))
                    },
                )?,
            );
        }
        for (id, records) in &inventory.records {
            if records.len() != 1 || records[0].root != *root {
                continue;
            }
            let Some(task) = &records[0].task else {
                continue;
            };
            let project = TaskId::parse(id).unwrap().project;
            let cfg = &configs[&(root.clone(), project.clone())];
            let mut normalized = task.clone();
            crate::services::task_service::TaskService::ensure_task_defaults(
                &mut normalized,
                cfg,
                false,
            );
            let dto = crate::services::task_service::TaskService::to_dto(
                id,
                normalized,
                Some(sprints),
                cfg,
                now,
            );
            metadata.insert(id.clone(), RelationshipTaskSummary::from(&dto));
            if filter.project.as_ref().is_some_and(|p| *p != project)
                || !crate::storage::search::StorageSearch::task_matches_filter(
                    id,
                    task,
                    if exact.as_ref() == Some(id) {
                        &exact_filter
                    } else {
                        &storage_filter
                    },
                )
                || (!filter.sprints.is_empty()
                    && !dto.sprints.iter().any(|s| filter.sprints.contains(s)))
            {
                continue;
            }
            let mut candidate = vec![(id.clone(), dto)];
            crate::services::task_list::apply_field_filters(
                &mut candidate,
                &unknown,
                &query_config,
            );
            matching.extend(candidate);
        }
    }
    crate::services::task_service::TaskService::filter_assignees(&mut matching, &filter);
    crate::services::task_query::apply(&mut matching, &options, now);
    // Excluded filters (`-key:value`, DEV-5) leave tasks out of the matches.
    if !exclude.is_empty() {
        let excluded: std::collections::HashSet<String> = exclude
            .iter()
            .flat_map(|exclusion| {
                crate::services::task_list::run(storage, exclusion, now)
                    .into_iter()
                    .map(|(id, _)| id)
            })
            .collect();
        matching.retain(|(id, _)| !excluded.contains(id));
    }
    matching.sort_by_key(|(id, _)| exact.as_ref() != Some(id));
    let matches: BTreeSet<_> = matching.iter().map(|(id, _)| id.clone()).collect();
    let exact = exact.filter(|id| matches.contains(id));
    let mut selected: BTreeSet<String> = if let Some(id) = &request.task_id {
        BTreeSet::from([inventory.key(id)])
    } else {
        inventory
            .records
            .keys()
            .filter(|id| {
                request
                    .project
                    .as_deref()
                    .is_none_or(|p| TaskId::parse(id).is_ok_and(|id| id.project == p))
            })
            .cloned()
            .collect()
    };
    let limit = request.max_nodes.unwrap_or(100);
    if request.task_id.is_none() && !request.filters.is_empty() {
        selected = matches.clone();
    }
    let mut truncated = selected.len() > limit;
    selected = if request.task_id.is_none() && !request.filters.is_empty() {
        matching
            .iter()
            .take(limit)
            .map(|(id, _)| id.clone())
            .collect()
    } else {
        selected.into_iter().take(limit).collect()
    };
    let depth = request
        .depth
        .unwrap_or(if request.task_id.is_some() { 1 } else { 0 });
    // Even project-wide graphs show boundary endpoints rather than dangling IDs.
    let expansion_depth = if request.task_id.is_some() {
        depth
    } else {
        depth.max(1)
    };
    for _ in 0..expansion_depth {
        let mut additions = BTreeSet::new();
        for e in &projection_edges {
            if selected.contains(&e.0) || selected.contains(&e.1) {
                additions.insert(e.0.clone());
                additions.insert(e.1.clone());
            }
        }
        let before = selected.len();
        for id in additions {
            if selected.contains(&id) {
                continue;
            }
            if selected.len() == limit {
                truncated = true;
                break;
            }
            selected.insert(id);
        }
        if selected.len() == before {
            break;
        }
    }
    if request.relationship_kinds.is_some() || request.include_isolated.is_some() {
        // Match and sort before budgeting. Allocate actual endpoint pairs rather
        // than filling the budget with seeds whose counterparts cannot fit.
        let adjacency = projection_edges.iter().fold(
            BTreeMap::<String, BTreeSet<String>>::new(),
            |mut map, e| {
                map.entry(e.0.clone()).or_default().insert(e.1.clone());
                map.entry(e.1.clone()).or_default().insert(e.0.clone());
                map
            },
        );
        let explicit = request.task_id.as_ref().map(|id| inventory.key(id));
        let mut seeds: Vec<String> = if let Some(id) = &explicit {
            vec![id.clone()]
        } else {
            matching.iter().map(|(id, _)| id.clone()).collect()
        };
        // Unknown identities remain visible diagnostics even without metadata.
        if explicit.is_none() && request.filters.is_empty() {
            seeds.extend(
                inventory
                    .records
                    .keys()
                    .filter(|id| {
                        matches!(inventory.state(id), "ambiguous" | "unreadable")
                            && request
                                .project
                                .as_ref()
                                .is_none_or(|p| TaskId::parse(id).is_ok_and(|id| id.project == *p))
                    })
                    .cloned(),
            );
        }
        seeds.retain(|id| {
            request.include_isolated != Some(false)
                || adjacency.contains_key(id)
                || explicit.as_ref() == Some(id)
                || exact.as_ref() == Some(id)
                || matches!(inventory.state(id), "ambiguous" | "unreadable" | "missing")
        });
        let mut eligible: BTreeSet<_> = seeds.iter().cloned().collect();
        let mut frontier = eligible.clone();
        for _ in 0..expansion_depth {
            let next: BTreeSet<_> = frontier
                .iter()
                .flat_map(|id| adjacency.get(id).into_iter().flatten())
                .filter(|id| !eligible.contains(*id))
                .cloned()
                .collect();
            eligible.extend(next.iter().cloned());
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        selected.clear();
        if let Some(id) = &explicit {
            selected.insert(id.clone());
        }
        for seed in seeds {
            if selected.len() == limit {
                break;
            }
            let mut queue = std::collections::VecDeque::from([seed]);
            let mut visited = BTreeSet::new();
            while let Some(id) = queue.pop_front() {
                if !visited.insert(id.clone()) || !eligible.contains(&id) {
                    continue;
                }
                let neighbors = adjacency.get(&id);
                if !selected.contains(&id) {
                    let attached = neighbors
                        .is_some_and(|neighbors| neighbors.iter().any(|n| selected.contains(n)));
                    let partner = neighbors
                        .and_then(|neighbors| neighbors.iter().find(|n| eligible.contains(*n)));
                    if attached
                        || partner.is_none()
                        || partner == Some(&id)
                        || exact.as_ref() == Some(&id)
                    {
                        if selected.len() < limit {
                            selected.insert(id.clone());
                        }
                    } else if let Some(partner) = partner.filter(|_| selected.len() + 2 <= limit) {
                        selected.insert(id.clone());
                        selected.insert(partner.clone());
                    }
                }
                if selected.contains(&id) {
                    queue.extend(
                        neighbors
                            .into_iter()
                            .flatten()
                            .filter(|n| eligible.contains(*n))
                            .cloned(),
                    );
                }
                if selected.len() == limit {
                    break;
                }
            }
        }
        truncated = eligible.iter().any(|id| !selected.contains(id));
    }
    let mut nodes = Vec::new();
    let node_order: Vec<_> = if request.filters.is_empty() {
        selected.iter().collect()
    } else {
        matching
            .iter()
            .map(|(id, _)| id)
            .filter(|id| selected.contains(*id))
            .chain(selected.iter().filter(|id| !matches.contains(*id)))
            .collect()
    };
    for id in node_order {
        let record = inventory.unique(id);
        let task = record.and_then(|r| r.task.as_ref());
        let project = TaskId::parse(id).map(|id| id.project).unwrap_or_default();
        let is_done = match (record, task) {
            (Some(record), Some(task)) if task.deleted_at.is_none() => {
                let config = &configs[&(record.root.clone(), project.clone())];
                let status = if task.status.is_empty() {
                    config.effective_default_status().unwrap_or_default()
                } else {
                    task.status.clone()
                };
                crate::services::completion::effective_done_statuses(config)
                    .contains(&status.as_str().to_ascii_lowercase())
            }
            _ => false,
        };
        nodes.push(RelationshipNode {
            id: id.clone(),
            title: task.map(|t| t.title.clone()),
            status: task.map(|t| t.status.to_string()),
            project,
            state: if inventory.state(id) == "active"
                && record.is_some_and(|record| record.root != inventory.local_root)
            {
                "external".into()
            } else {
                inventory.state(id).into()
            },
            is_done,
            matches_filter: matches.contains(id),
            task: metadata.remove(id),
        });
    }
    let edges = assertions
        .into_iter()
        .filter(|(e, _)| {
            selected.contains(&e.0)
                && selected.contains(&e.1)
                && request
                    .relationship_kinds
                    .as_ref()
                    .is_none_or(|kinds| kinds.contains(&e.2))
        })
        .map(|((source, target, kind), provenance)| RelationshipEdge {
            source,
            target,
            kind,
            provenance,
        })
        .collect();
    let mut issues: Vec<_> = inspect(&inventory)
        .into_iter()
        .filter(|i| i.tasks.iter().any(|id| selected.contains(id)))
        .collect();
    for node in &nodes {
        if node.state == "missing" && !issues.iter().any(|issue| issue.tasks.contains(&node.id)) {
            issues.push(issue(
                "missing",
                vec![node.id.clone()],
                false,
                format!("Task '{}' is missing", node.id),
            ));
        }
    }
    let complete = !truncated
        && !inventory
            .records
            .keys()
            .any(|id| matches!(inventory.state(id), "ambiguous" | "unreadable"))
        && !issues.iter().any(|i| i.code == "missing");
    Ok(RelationshipGraphResponse {
        nodes,
        edges,
        issues,
        revision: inventory.revision,
        complete,
        truncated,
    })
}

fn change_assertion(
    rel: &mut TaskRelationships,
    field: &str,
    target: &str,
    add: bool,
    inventory: &Inventory,
) {
    let scalar = match field {
        "parent" => Some(&mut rel.parent),
        "duplicate_of" => Some(&mut rel.duplicate_of),
        _ => None,
    };
    if let Some(scalar) = scalar {
        if add {
            *scalar = Some(target.into());
        } else if scalar
            .as_deref()
            .is_some_and(|raw| inventory.key(raw) == target)
        {
            *scalar = None;
        }
        return;
    }
    let values = match field {
        "blocks" => &mut rel.blocks,
        "depends_on" => &mut rel.depends_on,
        "children" => &mut rel.children,
        "related" => &mut rel.related,
        "fixes" => &mut rel.fixes,
        _ => unreachable!(),
    };
    if add {
        if !values.iter().any(|raw| inventory.key(raw) == target) {
            values.push(target.into());
        }
    } else {
        values.retain(|raw| inventory.key(raw) != target);
    }
}

fn repair_assertion(rel: &mut TaskRelationships, field: &str, target: &str, inventory: &Inventory) {
    match field {
        "parent" => {
            rel.parent = Some(target.into());
            return;
        }
        "duplicate_of" => {
            rel.duplicate_of = Some(target.into());
            return;
        }
        _ => {}
    }
    let values = match field {
        "blocks" => &mut rel.blocks,
        "depends_on" => &mut rel.depends_on,
        "children" => &mut rel.children,
        "related" => &mut rel.related,
        "fixes" => &mut rel.fixes,
        _ => unreachable!(),
    };
    let mut found = false;
    values.retain_mut(|raw| {
        if inventory.key(raw) != target {
            return true;
        }
        if found {
            return false;
        }
        found = true;
        *raw = target.into();
        true
    });
    if !found {
        values.push(target.into());
    }
}

fn apply_edge(
    inventory: &Inventory,
    root: &Path,
    tasks: &mut BTreeMap<String, Task>,
    e: &Edge,
    add: bool,
    only_owner: Option<&str>,
) -> LoTaRResult<()> {
    let mut sides = mirrors(e);
    // DEV-16: with reciprocal links off, an added link is written only to the
    // task being edited. Removing still clears both sides, because a link
    // stays in place while either task lists it.
    if add
        && let Some(owner) = only_owner
        && sides.iter().any(|(side, _, _)| side == owner)
    {
        sides.retain(|(side, _, _)| side == owner);
    }
    for (owner, field, target) in sides {
        // Missing/corrupt targets can be detached from the surviving task,
        // but are never guessed, rewritten, or resurrected.
        if !add {
            let Some(record) = inventory.unique(&owner) else {
                if inventory.state(&owner) == "ambiguous" {
                    return Err(invalid(format!(
                        "Cannot completely remove relationship from ambiguous task '{owner}'"
                    )));
                }
                continue;
            };
            let Some(task) = &record.task else {
                continue;
            };
            if !assertions(&owner, &task.relationships, inventory)
                .iter()
                .any(|(old, old_field, _)| old == e && *old_field == field)
            {
                continue;
            }
            if task.deleted_at.is_some() {
                return Err(invalid(format!(
                    "Cannot completely remove relationship asserted by deleted task '{owner}'; restore it first"
                )));
            }
        }
        inventory.active_local(root, &owner)?;
        let task = tasks
            .entry(owner.clone())
            .or_insert_with(|| inventory.unique(&owner).unwrap().task.clone().unwrap());
        change_assertion(&mut task.relationships, field, &target, add, inventory);
    }
    Ok(())
}

fn validate_additions(
    inventory: &Inventory,
    root: &Path,
    before: &BTreeSet<Edge>,
    after: &BTreeSet<Edge>,
) -> LoTaRResult<()> {
    let conflicting = conflicting_edges(after);
    for e in after.difference(before) {
        inventory.active_local(root, &e.0)?;
        inventory.active_local(root, &e.1)?;
        if conflicting.contains(e) {
            return Err(invalid(format!(
                "Relationship {:?} {} -> {} creates a self-link, cycle, or parent conflict",
                e.2, e.0, e.1
            )));
        }
    }
    // Unknown local tasks could conceal a cycle or a second parent. Removal
    // and unrelated metadata edits remain possible despite legacy corruption.
    if after.difference(before).next().is_some()
        && inventory
            .records
            .keys()
            .any(|id| matches!(inventory.state(id), "ambiguous" | "unreadable"))
    {
        return Err(invalid(
            "Relationship additions require a complete, unambiguous task inventory",
        ));
    }
    Ok(())
}

/// Stage reciprocal differences in an existing task transaction. The primary
/// task is staged by its caller, preserving its ordinary history/automation.
/// Returns counterpart IDs whose events must be emitted after commit.
pub(crate) fn stage_task_change(
    txn: &mut MultiFileTransaction,
    storage: &Storage,
    id: &str,
    previous: &TaskRelationships,
    candidate: &TaskRelationships,
    external: Option<&ExternalSnapshot>,
) -> LoTaRResult<Vec<String>> {
    if previous == candidate {
        return Ok(Vec::new());
    }
    let mut inventory = if let Some(external) = external {
        external.load(storage)?
    } else {
        Inventory::load(storage)?
    };
    // A newly allocated task does not exist physically until the transaction
    // commits; include only its identity in the candidate graph.
    if !inventory.records.contains_key(id) {
        let parsed = TaskId::parse(id).map_err(|e| invalid(e.to_string()))?;
        inventory.records.insert(
            id.into(),
            vec![Record {
                root: canonical_root(&storage.root_path),
                path: storage
                    .root_path
                    .join(&parsed.project)
                    .join(format!("{}.yml", parsed.number)),
                task: Some(Task::default()),
            }],
        );
    }
    let before: BTreeSet<_> = inventory.assertions().keys().cloned().collect();
    let old: BTreeSet<_> = assertions(id, previous, &inventory)
        .into_iter()
        .map(|a| a.0)
        .collect();
    let new: BTreeSet<_> = assertions(id, candidate, &inventory)
        .into_iter()
        .map(|a| a.0)
        .collect();
    let mut after = before.clone();
    for e in old.difference(&new) {
        after.remove(e);
    }
    after.extend(new.iter().cloned());
    validate_additions(&inventory, &storage.root_path, &before, &after)?;
    let mut tasks = BTreeMap::new();
    for e in old.difference(&new) {
        apply_edge(&inventory, &storage.root_path, &mut tasks, e, false, None)?;
    }
    let only_owner = (!reciprocal_links(&storage.root_path, id)).then_some(id);
    for e in new.difference(&old) {
        apply_edge(
            &inventory,
            &storage.root_path,
            &mut tasks,
            e,
            true,
            only_owner,
        )?;
    }
    tasks.remove(id);
    stage_tasks(txn, storage, &inventory, tasks)
}

fn stage_tasks(
    txn: &mut MultiFileTransaction,
    storage: &Storage,
    inventory: &Inventory,
    tasks: BTreeMap<String, Task>,
) -> LoTaRResult<Vec<String>> {
    txn.ensure_covers(&MultiFileTransaction::project_prefixes(&storage.root_path)?)?;
    let mut changed = Vec::new();
    for (id, mut task) in tasks {
        let record = inventory
            .unique(&id)
            .ok_or_else(|| invalid(format!("Ambiguous task '{id}'")))?;
        let previous = record.task.as_ref().unwrap();
        if previous.relationships == task.relationships {
            continue;
        }
        let parsed = TaskId::parse(&id).unwrap();
        txn.ensure_covers(std::slice::from_ref(&parsed.project))?;
        // Reciprocal links write both tasks; an archived one stays read-only.
        crate::storage::operations::ensure_writable(
            &record.root.join(&parsed.project),
            &record.path,
            &id,
        )?;
        let now = chrono::Utc::now().to_rfc3339();
        task.modified = now.clone();
        task.history.push(TaskChangeLogEntry {
            at: now,
            actor: crate::utils::identity::resolve_current_user_for_project(
                Some(&storage.root_path),
                Some(&parsed.project),
            ),
            changes: vec![TaskChange {
                field: "relationships".into(),
                old: serde_json::to_string(&previous.relationships).ok(),
                new: serde_json::to_string(&task.relationships).ok(),
            }],
        });
        let relative = record
            .path
            .strip_prefix(canonical_root(&storage.root_path))
            .map_err(|_| invalid("Cross-root relationship staging refused"))?;
        txn.stage(
            &storage.root_path.join(relative),
            serde_yaml_ng::to_string(&task)?,
        )?;
        changed.push(id);
    }
    Ok(changed)
}

/// Counterparts are API events only: reciprocal maintenance must not recurse
/// through automation (and events must never be sent before commit).
pub(crate) fn emit_counterparts(storage: &Storage, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    let Ok(inventory) = Inventory::load(storage) else {
        return;
    };
    let sprint_lookup = crate::services::task_service::TaskService::load_sprint_lookup(storage);
    for id in ids {
        if let Some(record) = inventory.unique(id)
            && let Some(mut task) = record.task.clone()
        {
            let project = TaskId::parse(id).unwrap().project;
            let config = crate::services::task_service::TaskService::resolve_config_for_project(
                &record.root,
                &project,
            );
            crate::services::task_service::TaskService::ensure_task_defaults(
                &mut task, &config, false,
            );
            let dto = crate::services::task_service::TaskService::to_dto(
                id,
                task,
                Some(&sprint_lookup),
                &config,
                chrono::Utc::now(),
            );
            let actor = crate::utils::identity::resolve_current_user_for_project(
                Some(&storage.root_path),
                Some(&project),
            );
            crate::api_events::emit_task_updated(&dto, actor.as_deref());
        }
    }
}

pub fn mutate(
    storage: &Storage,
    request: &RelationshipMutationRequest,
) -> LoTaRResult<RelationshipMutationResponse> {
    let external = external_locks(storage)?;
    let mut txn = MultiFileTransaction::begin_all(&storage.root_path, &[])?;
    let inventory = external.load(storage)?;
    let source = if request.operation == RelationshipOperation::Remove {
        inventory.key(&request.source)
    } else {
        inventory.active_local(&storage.root_path, &request.source)?
    };
    let target = if request.operation == RelationshipOperation::Remove {
        inventory.key(&request.target)
    } else {
        inventory.active_local(&storage.root_path, &request.target)?
    };
    let request_source = source.clone();
    let e = field_edge(source.clone(), target.clone(), request.kind);
    let before: BTreeSet<_> = inventory.assertions().keys().cloned().collect();
    let mut after = before.clone();
    let mut removals = Vec::new();
    if request.operation == RelationshipOperation::ChangeKind {
        let from = request
            .from_kind
            .ok_or_else(|| invalid("change_kind requires from_kind"))?;
        if from == request.kind {
            return Err(invalid("change_kind requires a different kind"));
        }
        let old = field_edge(source, target, from);
        if !after.remove(&old) {
            return Err(invalid("The relationship to change no longer exists"));
        }
        removals.push(old);
    } else if request.from_kind.is_some() {
        return Err(invalid("from_kind is only valid for change_kind"));
    }
    if request.operation == RelationshipOperation::Remove {
        after.remove(&e);
        removals.push(e.clone());
    } else {
        if matches!(
            e.2,
            RelationshipKind::Hierarchy | RelationshipKind::Duplicate
        ) {
            let current: Vec<_> = after
                .iter()
                .filter(|other| {
                    other.2 == e.2
                        && if e.2 == RelationshipKind::Hierarchy {
                            other.1 == e.1
                        } else {
                            other.0 == e.0
                        }
                })
                .cloned()
                .collect();
            let expected = request
                .expected_current
                .as_deref()
                .map(|p| inventory.key(p));
            let current_target = current.first().map(|old| {
                if e.2 == RelationshipKind::Hierarchy {
                    old.0.clone()
                } else {
                    old.1.clone()
                }
            });
            if current.len() > 1 || (expected.is_some() && expected != current_target) {
                return Err(invalid(
                    "Expected current relationship is stale or conflicting",
                ));
            }
            if current.first().is_some_and(|old| *old != e) {
                if expected.is_none() {
                    return Err(invalid(
                        "Scalar relationship replacement requires expected_current",
                    ));
                }
                for old in current {
                    after.remove(&old);
                    removals.push(old);
                }
            }
        } else if request.expected_current.is_some() {
            return Err(invalid(
                "expected_current is only valid for parent, children, or duplicate_of",
            ));
        }
        after.insert(e.clone());
    }
    validate_additions(&inventory, &storage.root_path, &before, &after)?;
    let mut tasks = BTreeMap::new();
    for old in removals {
        apply_edge(
            &inventory,
            &storage.root_path,
            &mut tasks,
            &old,
            false,
            None,
        )?;
    }
    if request.operation != RelationshipOperation::Remove && !before.contains(&e) {
        let focused = request_source.as_str();
        let only_owner = (!reciprocal_links(&storage.root_path, focused)).then_some(focused);
        apply_edge(
            &inventory,
            &storage.root_path,
            &mut tasks,
            &e,
            true,
            only_owner,
        )?;
    }
    let changed_tasks = stage_tasks(&mut txn, storage, &inventory, tasks)?;
    let changed = !changed_tasks.is_empty();
    let locks = txn.commit_retaining_locks_checked(|| external.verify(storage))?;
    let revision = external.load(storage)?.revision;
    drop(locks);
    drop(external);
    emit_counterparts(storage, &changed_tasks);
    Ok(RelationshipMutationResponse {
        changed,
        changed_tasks,
        revision,
    })
}

fn repair_plan(
    storage: &Storage,
    inventory: &Inventory,
    project: Option<&str>,
) -> LoTaRResult<BTreeMap<String, Task>> {
    let assertions = inventory.assertions();
    let one_sided_projects = one_sided_projects(inventory);
    let edges: BTreeSet<_> = assertions.keys().cloned().collect();
    let conflicting = conflicting_edges(&edges);
    let conflicted_tasks: BTreeSet<_> = conflicting.iter().flat_map(|e| [&e.0, &e.1]).collect();
    let mut tasks = BTreeMap::new();
    // Unknown assertions elsewhere could conceal a parent conflict or cycle.
    if inventory
        .records
        .keys()
        .any(|id| matches!(inventory.state(id), "ambiguous" | "unreadable"))
    {
        return Ok(tasks);
    }
    for e in &edges {
        if project.is_some_and(|p| {
            ![&e.0, &e.1]
                .iter()
                .any(|id| TaskId::parse(id).is_ok_and(|id| id.project == p))
        }) {
            continue;
        }
        if inventory.active_local(&storage.root_path, &e.0).is_err()
            || inventory.active_local(&storage.root_path, &e.1).is_err()
            || conflicting.contains(e)
            || conflicted_tasks.contains(&e.0)
            || conflicted_tasks.contains(&e.1)
        {
            continue;
        }
        let skip_missing = one_sided(&one_sided_projects, e);
        for (owner, field, target) in mirrors(e) {
            if skip_missing
                && !assertions[e]
                    .iter()
                    .any(|p| p.task_id == owner && p.field == field)
            {
                continue;
            }
            let task = tasks
                .entry(owner.clone())
                .or_insert_with(|| inventory.unique(&owner).unwrap().task.clone().unwrap());
            repair_assertion(&mut task.relationships, field, &target, inventory);
        }
    }
    tasks.retain(|id, task| {
        inventory
            .unique(id)
            .unwrap()
            .task
            .as_ref()
            .unwrap()
            .relationships
            != task.relationships
    });
    Ok(tasks)
}

fn selected_repair_plan(
    storage: &Storage,
    inventory: &Inventory,
    request: &RelationshipRepairRequest,
) -> LoTaRResult<BTreeMap<String, Task>> {
    if request.issue_ids.is_empty() {
        return repair_plan(storage, inventory, request.project.as_deref());
    }
    let (issues, actions) = inspect_actions(inventory);
    let scoped: BTreeSet<_> = scoped_issues(issues, request.project.as_deref())
        .into_iter()
        .map(|issue| issue.id)
        .collect();
    let mut tasks = BTreeMap::new();
    let mut selected = BTreeSet::new();
    for id in &request.issue_ids {
        if !scoped.contains(id) {
            return Err(invalid(format!(
                "Unknown, stale, or out-of-scope relationship issue '{id}'; refresh diagnostics and preview again"
            )));
        }
        let action = actions.get(id).ok_or_else(|| {
            invalid(format!(
                "Relationship issue '{id}' is unsafe for automatic repair"
            ))
        })?;
        selected.insert(action);
    }
    for action in selected {
        let (owner, _, _) = action.assertion();
        inventory.active_local(&storage.root_path, owner)?;
        let task = tasks
            .entry(owner.to_string())
            .or_insert_with(|| inventory.unique(owner).unwrap().task.clone().unwrap());
        action.apply(&mut task.relationships, inventory);
    }
    tasks.retain(|id, task| {
        inventory
            .unique(id)
            .unwrap()
            .task
            .as_ref()
            .unwrap()
            .relationships
            != task.relationships
    });
    Ok(tasks)
}

fn repair_issues(
    inventory: &Inventory,
    request: &RelationshipRepairRequest,
) -> Vec<RelationshipIssue> {
    scoped_issues(inspect(inventory), request.project.as_deref())
        .into_iter()
        .filter(|issue| request.issue_ids.is_empty() || request.issue_ids.contains(&issue.id))
        .collect()
}

pub fn repair(
    storage: &Storage,
    request: &RelationshipRepairRequest,
) -> LoTaRResult<RelationshipRepairResponse> {
    validate_scope(storage, request.project.as_deref())?;
    if !request.apply {
        let snapshot = snapshot_locks(storage)?;
        validate_scope(storage, request.project.as_deref())?;
        let inventory = snapshot.load(storage)?;
        let tasks = selected_repair_plan(storage, &inventory, request)?;
        return Ok(RelationshipRepairResponse {
            applied: false,
            changed_tasks: tasks.into_keys().collect(),
            revision: repair_revision(&inventory, request),
            issues: repair_issues(&inventory, request),
        });
    }
    let expected = request
        .revision
        .as_deref()
        .ok_or_else(|| invalid("Repair application requires the preview revision"))?;
    let external = external_locks(storage)?;
    let mut txn = MultiFileTransaction::begin_all(&storage.root_path, &[])?;
    validate_scope(storage, request.project.as_deref())?;
    let inventory = external.load(storage)?;
    if expected != repair_revision(&inventory, request) {
        return Err(invalid(
            "Relationship repair preview is stale or has a different scope; preview again",
        ));
    }
    let tasks = selected_repair_plan(storage, &inventory, request)?;
    let changed_tasks = stage_tasks(&mut txn, storage, &inventory, tasks)?;
    let locks = txn.commit_retaining_locks_checked(|| external.verify(storage))?;
    let inventory = external.load(storage)?;
    drop(locks);
    drop(external);
    emit_counterparts(storage, &changed_tasks);
    Ok(RelationshipRepairResponse {
        applied: true,
        changed_tasks,
        revision: repair_revision(&inventory, request),
        issues: repair_issues(&inventory, request),
    })
}

fn repair_revision(inventory: &Inventory, request: &RelationshipRepairRequest) -> String {
    let ids: BTreeSet<_> = request.issue_ids.iter().collect();
    let scope = serde_json::to_string(&(&request.project, ids)).unwrap();
    blake3::hash(format!("{}:{scope}", inventory.revision).as_bytes())
        .to_hex()
        .to_string()
}

/// Incoming `blocks` is authoritative even before repair. Caller-supplied
/// depends_on is included for freshly projected, not-yet-persisted DTOs.
pub fn blocked_dependencies(
    storage: &Storage,
    id: &str,
    declared: &[String],
) -> LoTaRResult<Vec<String>> {
    let snapshot = snapshot_locks(storage)?;
    let inventory = snapshot.load(storage)?;
    if inventory
        .records
        .keys()
        .any(|id| matches!(inventory.state(id), "ambiguous" | "unreadable"))
    {
        return Err(invalid("Dependency graph is incomplete or ambiguous"));
    }
    let key = inventory.key(id);
    let mut dependencies: BTreeSet<_> = declared.iter().map(|d| inventory.key(d)).collect();
    for (source, target, kind) in inventory.assertions().keys() {
        if *kind == RelationshipKind::Dependency && *target == key {
            dependencies.insert(source.clone());
        }
    }
    let mut blocked = Vec::new();
    for dep in dependencies {
        let done = if inventory.state(&dep) == "active" {
            let record = inventory.unique(&dep).unwrap();
            task_is_done(record, &dep)?
        } else {
            false
        };
        if !done {
            blocked.push(dep);
        }
    }
    Ok(blocked)
}

fn task_is_done(record: &Record, id: &str) -> LoTaRResult<bool> {
    let project = TaskId::parse(id)
        .map_err(|e| invalid(e.to_string()))?
        .project;
    let config = crate::config::resolution::config_for_project(&record.root, Some(&project))
        .map_err(|e| {
            invalid(format!(
                "Relationship completion policy unavailable for '{id}': {e}"
            ))
        })?;
    let task = record.task.as_ref().unwrap();
    let status = if task.status.is_empty() {
        config.effective_default_status().unwrap_or_default()
    } else {
        task.status.clone()
    };
    Ok(
        crate::services::completion::effective_done_statuses(&config)
            .contains(&status.as_str().to_ascii_lowercase()),
    )
}

#[cfg(test)]
#[path = "../../tests/common/dev15_relationship_cases.rs"]
mod dev15_relationship_cases;

/// Task IDs written in `text` (DEV-7): `DEV-12`, `ABC-OPS-3`. Words glued to
/// more ID characters (`DEV-12a`, `x-DEV-12`) and text inside inline code or
/// fenced code blocks do not count.
pub fn mentioned_ids(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        // Drop inline code spans (`...`).
        let mut prose = String::with_capacity(line.len());
        for (index, part) in line.split('`').enumerate() {
            if index % 2 == 0 {
                prose.push_str(part);
            }
            prose.push(' ');
        }
        let is_id_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
        for word in prose.split(|c: char| !is_id_char(c)) {
            let word = word.trim_end_matches(['-', '_']);
            let Some((prefix, number)) = word.rsplit_once('-') else {
                continue;
            };
            if prefix.is_empty()
                || number.is_empty()
                || !number.chars().all(|c| c.is_ascii_digit())
                || !prefix.chars().next().is_some_and(|c| c.is_alphanumeric())
            {
                continue;
            }
            if TaskId::parse(word).is_ok() {
                found.insert(word.to_string());
            }
        }
    }
    found
}

/// DEV-7: link task `id` as `related` to tasks first mentioned in `text`
/// (mentions already in `previous` were handled when they were written).
/// Mentions of unknown, deleted or moved-away IDs, the task itself, and tasks
/// already linked to it in any way are skipped. Links are only ever added.
/// Respects `auto.link_mentions` and, for the other side,
/// `auto.reciprocal_relationships`. Returns whether anything changed.
pub(crate) fn link_mentions(
    storage: &Storage,
    id: &str,
    text: &str,
    previous: &str,
) -> LoTaRResult<bool> {
    let mut mentioned = mentioned_ids(text);
    if mentioned.is_empty() {
        return Ok(false);
    }
    for old in mentioned_ids(previous) {
        mentioned.remove(&old);
    }
    if mentioned.is_empty() {
        return Ok(false);
    }
    let Ok(parsed) = TaskId::parse(id) else {
        return Ok(false);
    };
    let enabled =
        crate::config::resolution::config_for_project(&storage.root_path, Some(&parsed.project))
            .map(|config| config.auto_link_mentions)
            .unwrap_or(true);
    if !enabled {
        return Ok(false);
    }

    let external = external_locks(storage)?;
    let mut txn = MultiFileTransaction::begin_all(&storage.root_path, &[])?;
    let inventory = external.load(storage)?;
    let source = inventory.active_local(&storage.root_path, id)?;
    let before: BTreeSet<_> = inventory.assertions().keys().cloned().collect();
    let linked = |other: &str| {
        before
            .iter()
            .any(|e| (e.0 == source && e.1 == other) || (e.1 == source && e.0 == other))
    };
    let only_owner = (!reciprocal_links(&storage.root_path, &source)).then_some(source.as_str());
    let mut additions = Vec::new();
    for raw in mentioned {
        let Ok(target) = inventory.active_local(&storage.root_path, &raw) else {
            continue;
        };
        if target == source || linked(&target) {
            continue;
        }
        // An archived task is read-only; link it only when its side is not
        // written.
        if only_owner.is_none()
            && let Some(record) = inventory.unique(&target)
            && let Ok(target_id) = TaskId::parse(&target)
            && crate::storage::operations::ensure_writable(
                &record.root.join(&target_id.project),
                &record.path,
                &target,
            )
            .is_err()
        {
            continue;
        }
        let e = field_edge(source.clone(), target, RelationshipField::Related);
        if !additions.contains(&e) {
            additions.push(e);
        }
    }
    if additions.is_empty() {
        return Ok(false);
    }
    let mut after = before.clone();
    after.extend(additions.iter().cloned());
    validate_additions(&inventory, &storage.root_path, &before, &after)?;
    let mut tasks = BTreeMap::new();
    for e in &additions {
        apply_edge(
            &inventory,
            &storage.root_path,
            &mut tasks,
            e,
            true,
            only_owner,
        )?;
    }
    let changed = stage_tasks(&mut txn, storage, &inventory, tasks)?;
    let locks = txn.commit_retaining_locks_checked(|| external.verify(storage))?;
    drop(locks);
    drop(external);
    emit_counterparts(storage, &changed);
    Ok(!changed.is_empty())
}
