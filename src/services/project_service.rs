use crate::api_types::{ProjectDTO, ProjectStatsDTO};
use crate::storage::manager::Storage;
use crate::types::TaskChangeLogEntry;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};

pub struct ProjectService;

impl ProjectService {
    pub fn list(storage: &Storage) -> Vec<ProjectDTO> {
        let tasks_root = &storage.root_path;

        crate::utils::filesystem::list_visible_subdirs(tasks_root)
            .into_iter()
            .map(|(prefix, _)| {
                let name = match crate::config::persistence::load_project_config_from_dir(
                    &prefix, tasks_root,
                ) {
                    Ok(cfg) => {
                        let trimmed = cfg.project_name.trim();
                        if trimmed.is_empty() {
                            prefix.clone()
                        } else {
                            trimmed.to_string()
                        }
                    }
                    Err(_) => prefix.clone(),
                };

                ProjectDTO {
                    prefix: prefix.clone(),
                    name,
                }
            })
            .collect()
    }

    pub fn stats(storage: &Storage, name: &str) -> ProjectStatsDTO {
        // Minimal placeholder; refine later as needed
        let filter = crate::storage::TaskFilter {
            project: Some(name.to_string()),
            ..Default::default()
        };
        let tasks = storage.search(&filter);
        let done_statuses = determine_done_statuses(storage);
        let (open, done) = tasks
            .iter()
            .fold((0_u64, 0_u64), |(open_acc, done_acc), (_, task)| {
                if is_done_status(&task.status, &done_statuses) {
                    (open_acc, done_acc + 1)
                } else {
                    (open_acc + 1, done_acc)
                }
            });

        let mut latest: Option<DateTime<Utc>> = None;
        let mut tag_counts: HashMap<String, usize> = HashMap::new();

        for (_, task) in tasks.iter() {
            update_latest(&mut latest, &task.created);
            update_latest(&mut latest, &task.modified);
            task.history
                .iter()
                .for_each(|entry| update_history_entry(&mut latest, entry));
            task.comments
                .iter()
                .for_each(|comment| update_latest(&mut latest, &comment.date));

            for tag in task.tags.iter() {
                let trimmed = tag.trim();
                if trimmed.is_empty() {
                    continue;
                }
                *tag_counts.entry(trimmed.to_string()).or_insert(0) += 1;
            }
        }

        let mut tags_top: Vec<(String, usize)> = tag_counts.into_iter().collect();
        tags_top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let tags_top: Vec<String> = tags_top.into_iter().take(10).map(|(tag, _)| tag).collect();

        ProjectStatsDTO {
            name: name.to_string(),
            open_count: open,
            done_count: done,
            recent_modified: latest.map(|dt| dt.to_rfc3339()),
            tags_top,
        }
    }
}

fn update_latest(target: &mut Option<DateTime<Utc>>, value: &str) {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        let utc = dt.with_timezone(&Utc);
        if target.is_none_or(|current| utc > current) {
            *target = Some(utc);
        }
    }
}

fn update_history_entry(target: &mut Option<DateTime<Utc>>, entry: &TaskChangeLogEntry) {
    update_latest(target, &entry.at);
}

fn determine_done_statuses(storage: &Storage) -> HashSet<String> {
    let mut done = HashSet::new();
    if let Ok(config) = crate::config::resolution::load_and_merge_configs(Some(&storage.root_path))
    {
        if let Some(last) = config.issue_states.values.last() {
            done.insert(last.as_str().to_lowercase());
        }
        for (alias, status) in &config.branch_status_aliases {
            if alias.eq_ignore_ascii_case("done") {
                done.insert(status.as_str().to_lowercase());
            }
        }
    }
    if done.is_empty() {
        done.insert("done".to_string());
        done.insert("completed".to_string());
        done.insert("closed".to_string());
    }
    done
}

fn is_done_status(status: &crate::types::TaskStatus, done: &HashSet<String>) -> bool {
    done.contains(&status.as_str().to_lowercase())
}
