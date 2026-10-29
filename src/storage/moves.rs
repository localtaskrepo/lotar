//! Moved tasks (DEV-183).
//!
//! Moving a task to another project gives it a new ID there and turns the
//! old file into a small *stub*: a soft-deleted task whose `moved_to` names
//! the new ID. The stub keeps the old number reserved (task numbers come
//! from the files on disk, there is no counter) and lets every old reference
//! (relationships, sprint lists, links in text, URLs, commit messages)
//! resolve at run time instead of being rewritten when the task moves.
//!
//! The moved task lists its earlier IDs in `moved_from`, which covers the
//! other direction: finding history, commits and code comments that still
//! use an old ID.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use crate::storage::identity::{self, TaskId, TaskLookupError};

/// Longest chain of moves followed before giving up (guards against loops
/// written by hand).
pub const MAX_MOVE_HOPS: usize = 32;

/// Why an old ID could not be followed to a current task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    /// The chain ends at an ID that no longer exists (the moved task was
    /// permanently deleted, or the target was moved away by hand).
    Broken { from: String, missing: String },
    /// The chain revisits an ID or exceeds [`MAX_MOVE_HOPS`].
    Loop { from: String },
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveError::Broken { from, missing } => write!(
                f,
                "Task '{from}' was moved to '{missing}', which no longer exists"
            ),
            MoveError::Loop { from } => write!(
                f,
                "Task '{from}' has a loop or an overly long chain of moves; fix the moved_to fields by hand"
            ),
        }
    }
}

impl std::error::Error for MoveError {}

/// The `moved_to` of a stored task file, when it is a move stub. Cheap for
/// ordinary task files: the YAML is only parsed when it mentions the key.
pub fn stub_target(file: &Path) -> Option<String> {
    let content = std::fs::read_to_string(file).ok()?;
    if !content.contains("moved_to") {
        return None;
    }
    crate::storage::task::parse_task_yaml_tolerant(&content)?
        .moved_to
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

/// Follow `raw` through any move stubs. `Ok(None)` when `raw` is not a
/// stub (an ordinary task, an unknown or malformed ID, or an ambiguous one:
/// the caller's own lookup reports those). `Ok(Some(id))` is the canonical
/// ID of the task the stub chain ends at.
pub fn follow(root: &Path, raw: &str) -> Result<Option<String>, MoveError> {
    let Ok(start) = TaskId::parse(raw.trim()) else {
        return Ok(None);
    };
    let start = start.canonical();
    let mut current = start.clone();
    let mut seen = vec![current.clone()];
    let mut moved = false;
    loop {
        // Fast path: almost every task sits at `<project>/<number>.yml`, so
        // check that file before walking the project's folders.
        let parsed = TaskId::parse(&current).expect("canonical IDs parse");
        let direct = root
            .join(&parsed.project)
            .join(format!("{}.yml", parsed.number));
        let file = if direct.is_file() {
            direct
        } else {
            match identity::resolve(root, &current) {
                Ok(location) => location.file,
                Err(TaskLookupError::NotFound(_)) if moved => {
                    return Err(MoveError::Broken {
                        from: start,
                        missing: current,
                    });
                }
                Err(_) => return Ok(moved.then_some(current)),
            }
        };
        let Some(next) = stub_target(&file) else {
            return Ok(moved.then_some(current));
        };
        let next = match TaskId::parse(&next) {
            Ok(parsed) => parsed.canonical(),
            Err(_) => {
                return Err(MoveError::Broken {
                    from: start,
                    missing: next,
                });
            }
        };
        if seen.contains(&next) || seen.len() > MAX_MOVE_HOPS {
            return Err(MoveError::Loop { from: start });
        }
        seen.push(next.clone());
        current = next;
        moved = true;
    }
}

/// The ID a reference means today: the end of its move chain, or the
/// reference itself (canonicalized when it parses) when it was never moved
/// or the chain cannot be followed.
pub fn current_id(root: &Path, raw: &str) -> String {
    match follow(root, raw) {
        Ok(Some(id)) => id,
        _ => TaskId::parse(raw.trim())
            .map(|id| id.canonical())
            .unwrap_or_else(|_| raw.trim().to_string()),
    }
}

/// Resolve many references against one tasks root, reading each stored ID
/// at most once. Meant for one request or command: create it, use it, drop it.
pub struct MoveResolver<'a> {
    root: &'a Path,
    cache: RefCell<HashMap<String, String>>,
}

impl<'a> MoveResolver<'a> {
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// See [`current_id`].
    pub fn current(&self, raw: &str) -> String {
        if let Some(hit) = self.cache.borrow().get(raw) {
            return hit.clone();
        }
        let resolved = current_id(self.root, raw);
        self.cache
            .borrow_mut()
            .insert(raw.to_string(), resolved.clone());
        resolved
    }

    /// Whether two references mean the same task today (padded spellings
    /// and moves included). Unparseable references match nothing.
    pub fn same(&self, a: &str, b: &str) -> bool {
        if TaskId::parse(a.trim()).is_err() || TaskId::parse(b.trim()).is_err() {
            return false;
        }
        self.current(a) == self.current(b)
    }
}

/// Show relationship references under the current IDs of moved tasks.
/// Spellings that resolve to the same task collapse into one entry.
pub fn current_relationships(
    moves: &MoveResolver,
    relationships: &mut crate::types::TaskRelationships,
) {
    let list = |values: &mut Vec<String>| {
        let mut seen = Vec::new();
        for value in values.drain(..) {
            let current = moves.current(&value);
            let shown = if TaskId::parse(value.trim()).is_ok() {
                current
            } else {
                value
            };
            if !seen.contains(&shown) {
                seen.push(shown);
            }
        }
        *values = seen;
    };
    list(&mut relationships.depends_on);
    list(&mut relationships.blocks);
    list(&mut relationships.related);
    list(&mut relationships.children);
    list(&mut relationships.fixes);
    for single in [&mut relationships.parent, &mut relationships.duplicate_of] {
        if let Some(value) = single.as_mut()
            && TaskId::parse(value.trim()).is_ok()
        {
            *value = moves.current(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn stub(to: &str) -> String {
        format!(
            "title: Moved\ncreated: '2026-10-29T00:00:00Z'\ndeleted_at: '2026-10-29T00:00:00Z'\nmoved_to: {to}\n"
        )
    }

    const TASK: &str = "title: Live\ncreated: '2026-10-29T00:00:00Z'\n";

    #[test]
    fn follows_chains_and_leaves_ordinary_ids_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "DEV/12.yml", &stub("WEB-40"));
        write(root, "WEB/40.yml", &stub("OPS-3"));
        write(root, "OPS/3.yml", TASK);
        write(root, "DEV/1.yml", TASK);

        assert_eq!(follow(root, "DEV-12").unwrap(), Some("OPS-3".into()));
        assert_eq!(follow(root, "DEV-012").unwrap(), Some("OPS-3".into()));
        assert_eq!(follow(root, "WEB-40").unwrap(), Some("OPS-3".into()));
        assert_eq!(follow(root, "OPS-3").unwrap(), None);
        assert_eq!(follow(root, "DEV-1").unwrap(), None);
        assert_eq!(follow(root, "DEV-99").unwrap(), None);
        assert_eq!(follow(root, "not an id").unwrap(), None);
        assert_eq!(current_id(root, "DEV-12"), "OPS-3");
        assert_eq!(current_id(root, "DEV-01"), "DEV-1");

        let resolver = MoveResolver::new(root);
        assert!(resolver.same("DEV-12", "OPS-3"));
        assert!(resolver.same("WEB-40", "DEV-12"));
        assert!(!resolver.same("DEV-1", "OPS-3"));
        assert!(!resolver.same("", ""));
    }

    #[test]
    fn reports_broken_chains_and_loops() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "DEV/12.yml", &stub("WEB-40"));
        assert_eq!(
            follow(root, "DEV-12"),
            Err(MoveError::Broken {
                from: "DEV-12".into(),
                missing: "WEB-40".into()
            })
        );
        // A broken chain still names the old ID rather than guessing.
        assert_eq!(current_id(root, "DEV-12"), "DEV-12");

        write(root, "A/1.yml", &stub("B-1"));
        write(root, "B/1.yml", &stub("A-1"));
        assert_eq!(
            follow(root, "A-1"),
            Err(MoveError::Loop { from: "A-1".into() })
        );
    }
}
