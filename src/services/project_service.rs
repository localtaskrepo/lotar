use crate::api_types::{ProjectDTO, ProjectStatsDTO};
use crate::storage::manager::Storage;

pub struct ProjectService;

impl ProjectService {
    pub fn list(storage: &Storage) -> Vec<ProjectDTO> {
        crate::utils::filesystem::list_visible_subdirs(&storage.root_path)
            .into_iter()
            .map(|(name, _)| ProjectDTO {
                prefix: name.clone(),
                name,
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
        let open = tasks
            .iter()
            .filter(|(_, t)| t.status != crate::types::TaskStatus::Done)
            .count() as u64;
        let done = tasks
            .iter()
            .filter(|(_, t)| t.status == crate::types::TaskStatus::Done)
            .count() as u64;
        ProjectStatsDTO {
            name: name.to_string(),
            open_count: open,
            done_count: done,
            recent_modified: None,
            tags_top: vec![],
        }
    }
}
