use crate::config::types::*;
use crate::types::{TaskStatus, TaskType, Priority};

/// Create the default project template
pub fn create_default_template() -> ProjectTemplate {
    ProjectTemplate {
        name: "default".to_string(),
        description: "Basic project template with standard task categories".to_string(),
        config: ProjectConfig {
            project_name: "{{project_name}}".to_string(),
            categories: Some(StringConfigField::new_wildcard()),
            issue_states: Some(ConfigurableField { values: vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done] }),
            issue_types: Some(ConfigurableField { values: vec![TaskType::Feature, TaskType::Bug, TaskType::Chore] }),
            issue_priorities: Some(ConfigurableField { values: vec![Priority::Low, Priority::Medium, Priority::High] }),
            tags: Some(StringConfigField::new_wildcard()),
            default_assignee: None,
            default_priority: Some(Priority::Medium),
        },
    }
}

/// Create the simple project template
pub fn create_simple_template() -> ProjectTemplate {
    ProjectTemplate {
        name: "simple".to_string(),
        description: "Minimal project template with basic task management".to_string(),
        config: ProjectConfig {
            project_name: "{{project_name}}".to_string(),
            categories: Some(StringConfigField::new_wildcard()),
            issue_states: Some(ConfigurableField { values: vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done] }),
            issue_types: Some(ConfigurableField { values: vec![TaskType::Feature] }),
            issue_priorities: Some(ConfigurableField { values: vec![Priority::Medium] }),
            tags: Some(StringConfigField::new_wildcard()),
            default_assignee: None,
            default_priority: Some(Priority::Medium),
        },
    }
}

/// Create the agile project template
pub fn create_agile_template() -> ProjectTemplate {
    ProjectTemplate {
        name: "agile".to_string(),
        description: "Agile development template with sprints and story points".to_string(),
        config: ProjectConfig {
            project_name: "{{project_name}}".to_string(),
            categories: Some(StringConfigField::new_strict(vec![
                "frontend".to_string(), "backend".to_string(), "testing".to_string(), 
                "documentation".to_string(), "devops".to_string()
            ])),
            issue_states: Some(ConfigurableField { values: vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Verify, TaskStatus::Done] }),
            issue_types: Some(ConfigurableField { values: vec![TaskType::Feature, TaskType::Bug, TaskType::Epic] }),
            issue_priorities: Some(ConfigurableField { values: vec![Priority::Low, Priority::Medium, Priority::High, Priority::Critical] }),
            tags: Some(StringConfigField::new_wildcard()),
            default_assignee: None,
            default_priority: Some(Priority::Medium),
        },
    }
}

/// Create the kanban project template
pub fn create_kanban_template() -> ProjectTemplate {
    ProjectTemplate {
        name: "kanban".to_string(),
        description: "Kanban board template with workflow stages".to_string(),
        config: ProjectConfig {
            project_name: "{{project_name}}".to_string(),
            categories: Some(StringConfigField::new_wildcard()),
            issue_states: Some(ConfigurableField { values: vec![TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Verify, TaskStatus::Blocked, TaskStatus::Done] }),
            issue_types: Some(ConfigurableField { values: vec![TaskType::Feature, TaskType::Bug, TaskType::Chore] }),
            issue_priorities: Some(ConfigurableField { values: vec![Priority::Low, Priority::Medium, Priority::High] }),
            tags: Some(StringConfigField::new_wildcard()),
            default_assignee: None,
            default_priority: Some(Priority::Medium),
        },
    }
}
