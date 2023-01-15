use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use std::fmt;

#[derive(Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub priority: u8,
    pub project: String,
    pub category: Option<String>,
    pub created: String,
    pub due_date: Option<String>,
    pub tags: Vec<String>,
}

impl Task {
    pub fn new(root_path: PathBuf, title: String, project: String, priority: u8) -> Self {
        let metadata_path = root_path.join(project.clone()+"/metadata.yaml");
        let metadata = Metadata::from_file(&metadata_path);
        Self {
            id: metadata.current_id + 1,
            title,
            subtitle: None,
            description: None,
            priority,
            project,
            created: chrono::Utc::now().to_rfc3339(),
            due_date: None,
            category: None,
            tags: vec![],
        }
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "id: {}\ntitle: {}\nsubtitle: {:?}\ndescription: {:?}\npriority: {}\nproject: {}\ncategory: {:?}\ncreated: {}\ndue_date: {:?}\ntags: {:?}", self.id, self.title, self.subtitle, self.description, self.priority, self.project, self.category, self.created, self.due_date, self.tags)
    }
}

pub struct Storage {
    root_path: PathBuf
}

impl Storage {
    pub fn new(root_path: PathBuf) -> Self {
        fs::create_dir_all(&root_path).unwrap();
        Self { root_path }
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
        file_path.push(format!("{}.yaml", file_name));

        if !file_path.exists() {
            return file_path;
        }

        let mut counter = 1;
        while file_path.exists() {
            let new_file_name = format!("{}_{}.yaml", file_name, counter);
            file_path.pop();
            file_path.push(new_file_name);
            counter += 1;
        }

        file_path
    }

    pub fn add(&mut self, task: &Task) -> u64 {
        let file_path = &self.get_file_path(task);
        let file_string = serde_yaml::to_string(task).unwrap();
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(file_path, file_string).unwrap();

        let metadata_path = &file_path.with_file_name("metadata.yaml");
        let mut metadata = Metadata::from_file(&metadata_path);
        metadata.task_count += 1;
        let metadata_string = serde_yaml::to_string(&metadata).unwrap();
        fs::write(metadata_path, metadata_string).unwrap();

        // Add code to update metadata in project folder
        let project_metadata_path = self.root_path.join(format!("{}/metadata.yaml", &task.project));
        let mut project_metadata = Metadata::from_file(&project_metadata_path);
        project_metadata.add_task(task, &file_path);
        let project_metadata_string = serde_yaml::to_string(&project_metadata).unwrap();
        fs::write(project_metadata_path, project_metadata_string).unwrap();

        task.id
    }

    pub fn get(&self, id: u64, project: String) -> Option<Task> {
        let project_metadata_path = self.root_path.join(format!("{}/metadata.yaml", &project));
        let metadata = Metadata::from_file(&project_metadata_path);
        let file_path = match metadata.id_to_file.get(&id) {
            Some(path) => PathBuf::from(path),
            None => return None,
        };
        let file_string = fs::read_to_string(file_path).unwrap();
        Some(serde_yaml::from_str(&file_string).unwrap())
    }

    pub fn edit(&self, id: u64, new_task: &Task) {
        let project_metadata_path = self.root_path.join(format!("{}/metadata.yaml", &new_task.project));
        let metadata = Metadata::from_file(&project_metadata_path);
        let file_path = match metadata.id_to_file.get(&id) {
            Some(path) => PathBuf::from(path),
            None => return,
        };
        let file_string = serde_yaml::to_string(new_task).unwrap();
        fs::write(file_path, file_string).unwrap();
    }

    pub fn delete(&self, id: u64, project: String) -> bool {
        let project_metadata_path = self.root_path.join(format!("{}/metadata.yaml", &project));
        let mut project_metadata = Metadata::from_file(&project_metadata_path);
        let file_path = match project_metadata.id_to_file.remove(&id) {
            Some(path) => PathBuf::from(path),
            None => return false,
        };
        match fs::remove_file(file_path) {
            Ok(_) => {
                project_metadata.task_count -= 1;
                let metadata_string = serde_yaml::to_string(&project_metadata).unwrap();
                fs::write(project_metadata_path, metadata_string).unwrap();
                true
            }
            Err(_) => false
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Metadata {
    task_count: u64,
    id_to_file: HashMap<u64, String>,
    current_id: u64,
}

impl Metadata {
    fn add_task(&mut self, task: &Task, file_path: &Path) {
        let id = task.id.clone();
        self.id_to_file.insert(id, file_path.to_str().unwrap().to_owned());
        self.task_count += 1;
        self.current_id = id;
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