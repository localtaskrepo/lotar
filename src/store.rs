use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use std::fmt;
use crate::types::{TaskStatus, TaskType, Priority, TaskRelationships, TaskComment, CustomFields};
use crate::index::{TaskIndex, TaskFilter};


#[derive(Clone, Serialize, Deserialize)]
pub struct Task {
    // Built-in standard fields (special handling in UI)
    pub id: String, // Changed from u64 to String for IDs like "AUTH-001"
    pub title: String,
    pub status: TaskStatus,
    pub priority: Priority,
    pub task_type: TaskType,
    pub assignee: Option<String>,
    pub project: String,
    pub created: String,
    pub modified: String,
    pub due_date: Option<String>,
    pub effort: Option<String>, // e.g., "5d", "2w", "3h"

    // Built-in structured fields (special UI components)
    pub acceptance_criteria: Vec<String>,
    pub relationships: TaskRelationships,
    pub comments: Vec<TaskComment>,

    // Legacy fields (keeping for backward compatibility)
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,

    // Team-specific custom fields (generic UI treatment based on type)
    pub custom_fields: CustomFields,
}

impl Task {
    pub fn new(_root_path: PathBuf, title: String, project: String, priority: Priority) -> Self {
        let now = chrono::Utc::now().to_rfc3339();

        Self {
            id: "0".to_string(), // Will be set by Storage::add()
            title,
            status: TaskStatus::default(),
            priority,
            task_type: TaskType::default(),
            assignee: None,
            project,
            created: now.clone(),
            modified: now,
            due_date: None,
            effort: None,
            acceptance_criteria: vec![],
            relationships: TaskRelationships::default(),
            comments: vec![],
            subtitle: None,
            description: None,
            category: None,
            tags: vec![],
            custom_fields: HashMap::new(),
        }
    }

    pub fn update_status(&mut self, new_status: TaskStatus) -> Result<(), String> {
        // TODO: Add transition validation once we implement transitions.yaml
        self.status = new_status;
        self.modified = chrono::Utc::now().to_rfc3339();
        Ok(())
    }

    pub fn update_modified(&mut self) {
        self.modified = chrono::Utc::now().to_rfc3339();
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "id: {}\ntitle: {}\nstatus: {}\nsubtitle: {:?}\ndescription: {:?}\npriority: {}\nproject: {}\ncategory: {:?}\ncreated: {}\nmodified: {}\ndue_date: {:?}\ntags: {:?}",
               self.id, self.title, self.status, self.subtitle, self.description, self.priority, self.project, self.category, self.created, self.modified, self.due_date, self.tags)
    }
}

pub struct Storage {
    root_path: PathBuf,
    index: TaskIndex,
}

impl Storage {
    pub fn new(root_path: PathBuf) -> Self {
        fs::create_dir_all(&root_path).unwrap();

        // Load or create index - changed from index.json to index.yml
        let index_path = root_path.join("index.yml");
        let index = TaskIndex::load_from_file(&index_path)
            .unwrap_or_else(|_| TaskIndex::new());

        Self { root_path, index }
    }

    fn save_index(&self) -> Result<(), Box<dyn std::error::Error>> {
        let index_path = self.root_path.join("index.yml"); // Changed from .json to .yml
        self.index.save_to_file(&index_path)
    }

    fn get_file_path(&self, task: &Task) -> PathBuf {
        let mut file_path = self.root_path.clone();
        file_path.push(task.project.clone());

        if let Some(category) = &task.category {
            file_path.push(category.clone());
        }

        let mut file_name = task.title.clone();
        file_name = file_name.replace("/", "_");
        file_name = file_name.replace("\\", "_");
        file_name = file_name.replace(".", "_");
        file_path.push(format!("{}.yml", file_name)); // Changed from .yaml to .yml

        if !file_path.exists() {
            return file_path;
        }

        let mut counter = 1;
        while file_path.exists() {
            let new_file_name = format!("{}_{}.yml", file_name, counter); // Changed from .yaml to .yml
            file_path.pop();
            file_path.push(new_file_name);
            counter += 1;
        }

        file_path
    }

    pub fn add(&mut self, task: &Task) -> String {
        // Get or create project metadata to assign proper ID
        let project_metadata_path = self.root_path.join(format!("{}/metadata.yml", &task.project));
        let mut project_metadata = Metadata::from_file(&project_metadata_path);

        // Create a mutable copy of the task and assign the next formatted ID
        let mut task_with_id = task.clone();
        let next_id = project_metadata.current_id + 1;
        // Generate formatted ID: PROJECT-001 format
        let project_prefix = task.project.to_uppercase().chars().take(4).collect::<String>();
        task_with_id.id = format!("{}-{:03}", project_prefix, next_id);

        let file_path = &self.get_file_path(&task_with_id);
        let file_string = serde_yaml::to_string(&task_with_id).unwrap();
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(file_path, file_string).unwrap();

        // Update project metadata with the new task
        project_metadata.add_task(&task_with_id, &file_path);
        let project_metadata_string = serde_yaml::to_string(&project_metadata).unwrap();
        fs::write(project_metadata_path, project_metadata_string).unwrap();

        // Update index
        let relative_path = file_path.strip_prefix(&self.root_path)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();
        self.index.add_task(&task_with_id, &relative_path);
        let _ = self.save_index();

        task_with_id.id
    }

    pub fn edit(&mut self, id: &str, new_task: &Task) {
        // Get old task for index update
        let old_task = self.get(id, new_task.project.clone());

        let project_metadata_path = self.root_path.join(format!("{}/metadata.yml", &new_task.project));
        let metadata = Metadata::from_file(&project_metadata_path);
        let file_path = match metadata.id_to_file.get(id) {
            Some(path) => PathBuf::from(path),
            None => return,
        };

        let file_string = serde_yaml::to_string(new_task).unwrap();
        fs::write(&file_path, file_string).unwrap();

        // Update index
        if let Some(old) = old_task {
            let relative_path = file_path.strip_prefix(&self.root_path)
                .unwrap_or(&file_path)
                .to_string_lossy()
                .to_string();
            self.index.update_task(&old, new_task, &relative_path);
            let _ = self.save_index();
        }
    }

    pub fn delete(&mut self, id: &str, project: String) -> bool {
        // Get task for index update
        let task = self.get(id, project.clone());

        let project_metadata_path = self.root_path.join(format!("{}/metadata.yml", &project));
        let mut project_metadata = Metadata::from_file(&project_metadata_path);
        let file_path = match project_metadata.id_to_file.remove(id) {
            Some(path) => PathBuf::from(path),
            None => return false,
        };

        match fs::remove_file(file_path) {
            Ok(_) => {
                // Update index
                if let Some(t) = task {
                    self.index.remove_task(&t);
                    let _ = self.save_index();
                }

                project_metadata.task_count -= 1;
                let metadata_string = serde_yaml::to_string(&project_metadata).unwrap();
                fs::write(project_metadata_path, metadata_string).unwrap();
                true
            }
            Err(_) => false
        }
    }

    pub fn search(&self, filter: &TaskFilter) -> Vec<Task> {
        let task_ids = self.index.find_by_filter(filter);
        let mut results = Vec::new();

        for task_id in task_ids {
            // Try to find the task in any project if no project filter specified
            if let Some(project) = &filter.project {
                if let Some(task) = self.get(&task_id, project.clone()) {
                    if self.matches_text_filter(&task, &filter.text_query) {
                        results.push(task);
                    }
                }
            } else {
                // Search across all projects
                for (_, project_ids) in &self.index.project2id {
                    if project_ids.contains(&task_id) {
                        // Find which project this task belongs to
                        for project_name in self.index.project2id.keys() {
                            if let Some(task) = self.get(&task_id, project_name.clone()) {
                                if self.matches_text_filter(&task, &filter.text_query) {
                                    results.push(task);
                                    break;
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }

        results
    }

    fn matches_text_filter(&self, task: &Task, text_query: &Option<String>) -> bool {
        if let Some(query) = text_query {
            let query_lower = query.to_lowercase();
            task.title.to_lowercase().contains(&query_lower) ||
            task.subtitle.as_ref().map_or(false, |s| s.to_lowercase().contains(&query_lower)) ||
            task.description.as_ref().map_or(false, |s| s.to_lowercase().contains(&query_lower)) ||
            task.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
        } else {
            true
        }
    }

    pub fn list_by_project(&self, project: &str) -> Vec<Task> {
        let filter = TaskFilter {
            project: Some(project.to_string()),
            ..Default::default()
        };
        self.search(&filter)
    }

    pub fn rebuild_index(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.index = TaskIndex::rebuild_from_storage(&self.root_path)?;
        self.save_index()?;
        Ok(())
    }

    pub fn get(&self, id: &str, project: String) -> Option<Task> {
        let project_metadata_path = self.root_path.join(format!("{}/metadata.yml", &project));
        let metadata = Metadata::from_file(&project_metadata_path);
        let file_path = match metadata.id_to_file.get(id) {
            Some(path) => PathBuf::from(path),
            None => return None,
        };
        let file_string = fs::read_to_string(file_path).unwrap();
        Some(serde_yaml::from_str(&file_string).unwrap())
    }
}

#[derive(Serialize, Deserialize)]
struct Metadata {
    task_count: u64,
    id_to_file: HashMap<String, String>,
    current_id: u64,
}

impl Metadata {
    fn add_task(&mut self, task: &Task, file_path: &Path) {
        let id = task.id.clone();
        self.id_to_file.insert(id, file_path.to_str().unwrap().to_owned());
        self.task_count += 1;
        // Extract numeric part from formatted ID (e.g., "TEST-001" -> 1)
        if let Some(numeric_part) = task.id.split('-').last() {
            if let Ok(num) = numeric_part.parse::<u64>() {
                // Update current_id to be the maximum of current and new ID
                self.current_id = self.current_id.max(num);
            }
        }
    }

    pub fn from_file(path: &Path) -> Self {
        if !path.exists() {
            return Self {
                task_count: 0,
                id_to_file: HashMap::new(),
                current_id: 0,
            }
        }
        let file_string = fs::read_to_string(path).unwrap();
        serde_yaml::from_str(&file_string).unwrap()
    }
}