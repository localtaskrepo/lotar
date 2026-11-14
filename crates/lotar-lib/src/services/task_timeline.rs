//! A task's timeline (DEV-518): its field changes, comments, the commits of its file and its agent jobs in one
//! list, oldest first, for the task panel, `lotar task timeline` and the MCP tool `task_timeline`.

use crate::api_types::{TaskTimeline, TaskTimelineEntry};
use crate::errors::LoTaRResult;
use crate::services::agent_job_service::AgentJobService;
use crate::services::audit_service::AuditService;
use crate::services::task_service::TaskService;
use crate::storage::manager::Storage;
use std::path::Path;

pub struct TaskTimelineService;

impl TaskTimelineService {
    /// The task's timeline. `threads` adds each job's whole thread.
    pub fn build(tasks_dir: &Path, id: &str, project: Option<&str>, threads: bool) -> LoTaRResult<TaskTimeline> {
        let storage = Storage::new(tasks_dir);
        let task = TaskService::get(&storage, id, project)?;
        let mut entries: Vec<TaskTimelineEntry> = Vec::new();
        // A comment's own entry stands for its `comment_added` change.
        entries.extend(task.history.iter().filter_map(|entry| {
            let changes: Vec<_> = entry.changes.iter().filter(|change| change.field != "comment_added").cloned().collect();
            (!changes.is_empty()).then(|| TaskTimelineEntry::Change { at: entry.at.clone(), actor: entry.actor.clone(), changes })
        }));
        entries.extend(task.comments.iter().enumerate().map(|(index, comment)| TaskTimelineEntry::Comment {
            at: comment.date.clone(),
            index,
            comment: comment.clone(),
        }));
        entries.extend(task_commits(tasks_dir, &task.id).into_iter().map(|commit| TaskTimelineEntry::Commit {
            at: commit.date.to_rfc3339(),
            commit: commit.commit,
            author: commit.author,
            email: commit.email,
            message: commit.message,
        }));
        // Jobs from before a move name the task's old ID (DEV-183).
        let moves = crate::storage::moves::MoveResolver::new(tasks_dir);
        entries.extend(
            AgentJobService::list_jobs_in(tasks_dir).into_iter().filter(|job| job.ticket_id == task.id || moves.same(&job.ticket_id, &task.id)).map(|job| {
                let thread = threads.then(|| AgentJobService::full_thread_in(tasks_dir, &job.id));
                TaskTimelineEntry::Job { at: job.created_at.clone(), job, thread }
            }),
        );
        entries.sort_by_key(|entry| sort_key(entry.at()));
        Ok(TaskTimeline { task_id: task.id, entries })
    }
}

/// Times come in different offsets and precisions; ones that don't parse sort first.
fn sort_key(at: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(at).ok().map(|time| time.with_timezone(&chrono::Utc))
}

/// The commits of the task's file; none outside a git repository.
fn task_commits(tasks_dir: &Path, id: &str) -> Vec<crate::services::audit_service::FileCommitEvent> {
    let Ok(parsed) = crate::storage::TaskId::parse(id) else {
        return Vec::new();
    };
    let Some(repo_root) = crate::utils::git::find_repo_root(tasks_dir) else {
        return Vec::new();
    };
    let Some(file) = crate::storage::operations::StorageOperations::get_file_path_for_id(&tasks_dir.join(&parsed.project), &parsed.canonical()) else {
        return Vec::new();
    };
    let Some(file_rel) = crate::utils::git::repo_relative_path(&repo_root, &file) else {
        return Vec::new();
    };
    AuditService::list_commits_for_task(&repo_root, tasks_dir, &file_rel).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_offsets_sort_by_time() {
        let mut times = ["2026-01-01T10:00:00+02:00", "2026-01-01T09:30:00Z", "bad"];
        times.sort_by_key(|at| sort_key(at));
        assert_eq!(times, ["bad", "2026-01-01T10:00:00+02:00", "2026-01-01T09:30:00Z"]);
    }
}
