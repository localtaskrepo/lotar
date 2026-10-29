//! Shared anchor maintenance for `lotar scan` and `POST /api/scan` (DEV-119).
//!
//! A code anchor is a `path#line` reference that scan stores on a task when a
//! comment mentions the task's ID. This pass keeps anchors pointing at live
//! mentions:
//!
//! - Relocation (always on in CLI scans): an anchor whose line no longer
//!   mentions the ID moves to the nearest mention in the same file, following
//!   simple git renames.
//! - Pruning (opt-in `reanchor`): an anchor whose file is gone, or whose file
//!   no longer mentions the ID anywhere, is removed. Anchors at live mentions,
//!   in any file, are kept, and links, repository files, attachments and
//!   Jira/GitHub references are never touched.
//! - Code references a person attached on purpose (ranges, path-only, or any
//!   reference with a `reference_added` history record) are never moved or
//!   removed; only scan-managed `path#N` anchors are maintained.
//!
//! When scan targets specific paths, only anchors under those paths are
//! considered, so a narrow scan never edits tasks anchored elsewhere.

use crate::errors::LoTaRResult;
use crate::storage::TaskFilter;
use crate::storage::manager::Storage;
use crate::types::ReferenceEntry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Default distance (in lines) preferred when relocating an anchor.
pub const DEFAULT_RELOCATION_WINDOW: usize = 7;

pub struct AnchorPassOptions<'a> {
    /// Scanned roots; empty means the whole workspace.
    pub scope: &'a [PathBuf],
    /// Remove anchors whose file is gone or no longer mentions the ID.
    pub prune_dead: bool,
    /// Plan only; never write task files.
    pub dry_run: bool,
    pub window: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct AnchorPassReport {
    /// Tasks whose anchors were (or, in a dry run, would be) changed.
    pub updated_tasks: usize,
    /// (task id, old anchor -> new anchor)
    pub relocated: Vec<(String, String, String)>,
    /// (task id, removed anchor)
    pub pruned: Vec<(String, String)>,
}

impl AnchorPassReport {
    /// Human-readable summary lines for CLI output and REST `info`.
    pub fn messages(&self, dry_run: bool) -> Vec<String> {
        let mut out = Vec::new();
        let (relocate_verb, prune_verb) = if dry_run {
            ("Would re-anchor", "Would remove dead anchor")
        } else {
            ("Re-anchored", "Removed dead anchor")
        };
        if !self.relocated.is_empty() {
            let tasks: std::collections::BTreeSet<_> =
                self.relocated.iter().map(|(task, _, _)| task).collect();
            out.push(format!("{} {} task(s).", relocate_verb, tasks.len()));
        }
        for (task, anchor) in &self.pruned {
            out.push(format!("{} {} from {}.", prune_verb, anchor, task));
        }
        out
    }
}

enum AnchorChange {
    Move(String),
    Remove,
}

/// Run the anchor pass over every task. `before_apply` fires after planning
/// and right before each task write (a test seam for concurrent editors).
pub fn run_anchor_pass(
    storage: &mut Storage,
    line_scanner: &crate::scanner::Scanner,
    repo_root: Option<&Path>,
    options: &AnchorPassOptions<'_>,
    mut before_apply: impl FnMut(&str) -> Result<(), String>,
) -> LoTaRResult<AnchorPassReport> {
    let mut report = AnchorPassReport::default();
    let tasks = storage.search(&TaskFilter::default());
    if tasks.is_empty() {
        return Ok(report);
    }
    let rename_map = repo_root.map(git_rename_map).unwrap_or_default();
    let scope: Vec<PathBuf> = options
        .scope
        .iter()
        .map(|root| std::fs::canonicalize(root).unwrap_or_else(|_| root.clone()))
        .collect();

    for (task_id, task) in tasks {
        // Plan from the snapshot; all source I/O happens here. The plan is
        // applied to the freshest task state under the task lock so
        // concurrent comments, references, custom fields and extensions
        // survive.
        let mut plan: Vec<(String, AnchorChange)> = Vec::new();
        for reference in &task.references {
            let Some(code_ref) = reference.code.clone() else {
                continue;
            };
            if plan.iter().any(|(old, _)| *old == code_ref) {
                continue;
            }
            // Only scan-managed anchors are maintained; references a person
            // attached on purpose are never moved or removed (DEV-119).
            if is_user_attached(&task.history, &code_ref) {
                continue;
            }
            let (path_str, orig_line) = parse_code_ref(&code_ref);
            if path_str.is_empty() {
                continue;
            }
            let original = match repo_root {
                Some(root) => root.join(&path_str),
                None => PathBuf::from(&path_str),
            };
            let resolved = if original.exists() {
                Some(original.clone())
            } else {
                rename_map
                    .get(&path_str)
                    .and_then(|new_rel| repo_root.map(|root| root.join(new_rel)))
                    .filter(|candidate| candidate.exists())
            };
            let in_scope = |path: &Path| scope.is_empty() || is_under_any(path, &scope);

            let Some(file) = resolved else {
                if options.prune_dead && in_scope(&original) {
                    plan.push((code_ref, AnchorChange::Remove));
                }
                continue;
            };
            if !in_scope(&file) {
                continue;
            }
            // Unreadable files are left alone rather than treated as dead.
            let Ok(content) = std::fs::read_to_string(&file) else {
                continue;
            };
            // Comments written before the task moved project still name an
            // old ID (DEV-183); they anchor the task as well.
            let mut candidates = line_scanner.key_mention_lines(&file, &content, &task_id);
            for old in &task.moved_from {
                candidates.extend(line_scanner.key_mention_lines(&file, &content, old));
            }
            candidates.sort_unstable();
            candidates.dedup();
            if candidates.is_empty() {
                if options.prune_dead {
                    plan.push((code_ref, AnchorChange::Remove));
                }
                continue;
            }
            let rel = crate::utils::paths::repo_relative_display(&file);
            if let Some(line) = orig_line
                && candidates.contains(&line)
            {
                // Still live; only normalize the spelling.
                let canonical = format!("{}#{}", rel, line);
                if canonical != code_ref {
                    plan.push((code_ref, AnchorChange::Move(canonical)));
                }
                continue;
            }
            let best = match orig_line {
                Some(line) => candidates
                    .iter()
                    .copied()
                    .filter(|candidate| candidate.abs_diff(line) <= options.window)
                    .min_by_key(|candidate| candidate.abs_diff(line))
                    .or_else(|| {
                        candidates
                            .iter()
                            .copied()
                            .min_by_key(|candidate| candidate.abs_diff(line))
                    }),
                None => candidates.first().copied(),
            };
            if let Some(line) = best {
                let moved = format!("{}#{}", rel, line);
                if moved != code_ref {
                    plan.push((code_ref, AnchorChange::Move(moved)));
                }
            }
        }

        if plan.is_empty() {
            continue;
        }
        for (old, change) in &plan {
            match change {
                AnchorChange::Move(new) => {
                    report
                        .relocated
                        .push((task_id.clone(), old.clone(), new.clone()));
                }
                AnchorChange::Remove => report.pruned.push((task_id.clone(), old.clone())),
            }
        }
        if options.dry_run {
            report.updated_tasks += 1;
            continue;
        }
        before_apply(&task_id).map_err(crate::errors::LoTaRError::ValidationError)?;
        let outcome = storage.mutate_task(&task_id, |fresh| {
            let changed = apply_plan(&mut fresh.references, &plan);
            if changed {
                fresh.modified = chrono::Utc::now().to_rfc3339();
            }
            Ok(changed)
        })?;
        if outcome.changed {
            report.updated_tasks += 1;
        }
    }
    Ok(report)
}

/// Apply planned changes only where the old anchor is still present in the
/// fresh state: references a concurrent editor added, changed or removed are
/// left exactly as written. A removed anchor drops the whole entry when it
/// carries nothing else, otherwise only its `code` value.
fn apply_plan(references: &mut Vec<ReferenceEntry>, plan: &[(String, AnchorChange)]) -> bool {
    let mut changed = false;
    let mut kept = Vec::with_capacity(references.len());
    for mut reference in references.drain(..) {
        let change = reference
            .code
            .as_deref()
            .and_then(|code| plan.iter().find(|(old, _)| old == code))
            .map(|(_, change)| change);
        match change {
            Some(AnchorChange::Move(new)) => {
                reference.code = Some(new.clone());
                changed = true;
            }
            Some(AnchorChange::Remove) => {
                changed = true;
                reference.code = None;
                if reference == ReferenceEntry::default() {
                    continue;
                }
            }
            None => {}
        }
        kept.push(reference);
    }
    *references = kept;
    changed
}

/// Whether a code reference was attached by a person rather than by scan.
/// Scan only ever writes single-line `path#N` anchors and records no history
/// for them, while explicit attachments (CLI, REST, MCP, UI) record a
/// `reference_added` entry. Path-only and ranged references (`path#A-B`)
/// therefore always count as user-attached.
pub(crate) fn is_user_attached(
    history: &[crate::types::TaskChangeLogEntry],
    code_ref: &str,
) -> bool {
    let Some((_, anchor)) = code_ref.rsplit_once('#') else {
        return true;
    };
    if anchor.parse::<usize>().is_err() {
        return true;
    }
    let recorded = format!("code:{code_ref}");
    history.iter().any(|entry| {
        entry.changes.iter().any(|change| {
            change.field == "reference_added" && change.new.as_deref() == Some(recorded.as_str())
        })
    })
}

fn is_under_any(path: &Path, roots: &[PathBuf]) -> bool {
    let normalized = std::fs::canonicalize(path)
        .ok()
        .or_else(|| {
            let parent = std::fs::canonicalize(path.parent()?).ok()?;
            Some(parent.join(path.file_name()?))
        })
        .unwrap_or_else(|| path.to_path_buf());
    roots.iter().any(|root| normalized.starts_with(root))
}

pub(crate) fn parse_code_ref(code: &str) -> (String, Option<usize>) {
    let Some((path, anchor)) = code.split_once('#') else {
        return (code.to_string(), None);
    };
    let digits: String = anchor
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    (path.to_string(), digits.parse::<usize>().ok())
}

/// Best-effort map of renamed paths from `git status`.
fn git_rename_map(repo_root: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(output) = crate::utils::git::git_command(repo_root)
        .args(["status", "--porcelain"])
        .output()
    else {
        return map;
    };
    if !output.status.success() {
        return map;
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.len() < 4 {
            continue;
        }
        if line[..2].contains('R')
            && let Some(pos) = line.find(" -> ")
        {
            map.insert(
                line[3..pos].trim().to_string(),
                line[pos + 4..].trim().to_string(),
            );
        }
    }
    map
}
