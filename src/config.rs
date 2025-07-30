/// Configuration constants for LoTaR
pub struct Config {
    pub default_port: u16,
    pub default_priority: crate::types::Priority,
    pub task_file_extension: &'static str,
    pub metadata_file_name: &'static str,
    pub index_file_name: &'static str,
    pub max_title_length: usize,
    pub max_description_length: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_port: 8080,
            default_priority: crate::types::Priority::Medium,
            task_file_extension: "yml",
            metadata_file_name: "metadata.yml",
            index_file_name: "index.yml",
            max_title_length: 200,
            max_description_length: 5000,
        }
    }
}

pub const CONFIG: Config = Config {
    default_port: 8080,
    default_priority: crate::types::Priority::Medium,
    task_file_extension: "yml",
    metadata_file_name: "metadata.yml",
    index_file_name: "index.yml",
    max_title_length: 200,
    max_description_length: 5000,
};

/// Project-related constants
pub const TASKS_DIR_NAME: &str = ".tasks";
pub const DEFAULT_PROJECT_NAME: &str = "default";

/// Scanner constants
pub const TODO_PATTERN: &str = r"(?i)todo";
pub const UUID_PATTERN: &str = r"(?i)todo\s*\(([^)]+)\)\s*:?\s*(.*)";
pub const SIMPLE_TODO_PATTERN: &str = r"(?i)todo\s*:?\s*(.*)";

/// Task ID formatting
pub const ID_FORMAT: &str = "{}-{:03}";  // PROJECT-001 format
pub const PROJECT_PREFIX_LENGTH: usize = 4;
