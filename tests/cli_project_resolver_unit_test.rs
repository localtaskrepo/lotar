use lotar::cli::project::ProjectResolver;
use lotar::cli::project::test_support::resolver_from_config;
use lotar::config::StringConfigField;
use lotar::config::types::{ConfigurableField, ResolvedConfig};
use lotar::types::{Priority, TaskStatus, TaskType};
use lotar::utils::{
    generate_project_prefix, generate_unique_project_prefix, resolve_project_input,
    validate_explicit_prefix,
};
use tempfile::TempDir;

fn make_resolver() -> ProjectResolver {
    let cfg = ResolvedConfig {
        server_port: 8080,
        default_prefix: "TEST".to_string(),
        issue_states: ConfigurableField {
            values: vec![TaskStatus::Todo, TaskStatus::Done],
        },
        issue_types: ConfigurableField {
            values: vec![TaskType::Feature, TaskType::Bug],
        },
        issue_priorities: ConfigurableField {
            values: vec![Priority::Low, Priority::High],
        },
        categories: StringConfigField::new_wildcard(),
        tags: StringConfigField::new_wildcard(),
        default_assignee: None,
        default_reporter: None,
        default_category: None,
        default_tags: vec![],
        auto_set_reporter: true,
        auto_assign_on_status: true,
        auto_codeowners_assign: true,
        default_priority: Priority::Medium,
        default_status: None,
        custom_fields: StringConfigField::new_wildcard(),
        scan_signal_words: vec![
            "TODO".to_string(),
            "FIXME".to_string(),
            "HACK".to_string(),
            "BUG".to_string(),
            "NOTE".to_string(),
        ],
        auto_identity: true,
        auto_identity_git: true,
    };
    resolver_from_config(cfg, std::path::PathBuf::from("/tmp"))
}

#[test]
fn extract_project_from_task_id() {
    let resolver = make_resolver();
    assert_eq!(
        resolver.extract_project_from_task_id("AUTH-123"),
        Some("AUTH".to_string())
    );
    assert_eq!(
        resolver.extract_project_from_task_id("TI-456"),
        Some("TI".to_string())
    );
    assert_eq!(
        resolver.extract_project_from_task_id("MOBILE-789"),
        Some("MOBILE".to_string())
    );
    assert_eq!(resolver.extract_project_from_task_id("123"), None);
    assert_eq!(resolver.extract_project_from_task_id("auth-123"), None);
    assert_eq!(resolver.extract_project_from_task_id("AUTH123"), None);
    assert_eq!(resolver.extract_project_from_task_id("-123"), None);
}

#[test]
fn resolve_project_scenarios() {
    let mut resolver = make_resolver();
    assert_eq!(
        resolver.resolve_project("AUTH-123", Some("AUTH")).unwrap(),
        "AUTH"
    );
    assert_eq!(
        resolver.resolve_project("AUTH-123", Some("auth")).unwrap(),
        "AUTH"
    );
    let result = resolver.resolve_project("AUTH-123", Some("FRONTEND"));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Project mismatch"));
    assert_eq!(
        resolver.resolve_project("123", Some("MOBILE")).unwrap(),
        "MOBI"
    );
    assert_eq!(resolver.resolve_project("AUTH-123", None).unwrap(), "AUTH");
    assert_eq!(resolver.resolve_project("123", None).unwrap(), "TEST");
    assert_eq!(resolver.resolve_project("", None).unwrap(), "TEST");
    let result = resolver.resolve_project("123", Some("INVALID!"));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid project name"));
}

#[test]
fn resolve_project_original_behavior() {
    let mut resolver = make_resolver();
    assert_eq!(resolver.resolve_project("no-prefix", None).unwrap(), "TEST");
}

#[test]
fn get_full_task_id_works() {
    let mut resolver = make_resolver();
    assert_eq!(
        resolver.get_full_task_id("AUTH-123", None).unwrap(),
        "AUTH-123"
    );
    assert_eq!(resolver.get_full_task_id("123", None).unwrap(), "TEST-123");
    assert_eq!(
        resolver.get_full_task_id("123", Some("MOBILE")).unwrap(),
        "MOBI-123"
    );
}

// Merged from utils_project_unit_test.rs
#[test]
fn generate_prefix_variants() {
    assert_eq!(generate_project_prefix("test"), "TEST");
    assert_eq!(generate_project_prefix("a"), "A");
    assert_eq!(generate_project_prefix("ab"), "AB");
    assert_eq!(generate_project_prefix("abc"), "ABC");
    assert_eq!(generate_project_prefix("abcd"), "ABCD");
    assert_eq!(generate_project_prefix("my-cool-project"), "MCP");
    assert_eq!(generate_project_prefix("super-awesome-tool"), "SAT");
    assert_eq!(generate_project_prefix("a-b-c-d"), "ABCD");
    assert_eq!(generate_project_prefix("my_project"), "MP");
    assert_eq!(generate_project_prefix("longprojectname"), "LONG");
    assert_eq!(generate_project_prefix("verylongname"), "VERY");
    assert_eq!(generate_project_prefix("project"), "PROJ");
    assert_eq!(generate_project_prefix("MyProject"), "MYPR");
    assert_eq!(generate_project_prefix("my-Cool-Project"), "MCP");
    assert_eq!(generate_project_prefix("Test_Project"), "TP");
    assert_eq!(generate_project_prefix("My Test Project"), "MTP");
    assert_eq!(generate_project_prefix("Super Awesome Tool"), "SAT");
    assert_eq!(generate_project_prefix("A B C D"), "ABCD");
    assert_eq!(generate_project_prefix("my project"), "MP");
    assert_eq!(generate_project_prefix("Local Task Repository"), "LTR");
    assert_eq!(generate_project_prefix(".tmp"), "TMP");
    assert_eq!(generate_project_prefix(".tmpABC123"), "TMPA");
    assert_eq!(generate_project_prefix(".test_dot_prefix"), "TDP");
    assert_eq!(generate_project_prefix("..hidden"), "HIDD");
}

#[test]
fn resolve_project_input_variants() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let prefix_dir = tasks_dir.join("FRON");
    std::fs::create_dir_all(&prefix_dir).unwrap();
    assert_eq!(resolve_project_input("FRON", &tasks_dir), "FRON");
    assert_eq!(resolve_project_input("FRONTEND", &tasks_dir), "FRON");
    assert_eq!(resolve_project_input("NEW-PROJECT", &tasks_dir), "NP");
    assert_eq!(resolve_project_input("BACKEND", &tasks_dir), "BACK");
    let ab = tasks_dir.join("AB");
    std::fs::create_dir_all(&ab).unwrap();
    assert_eq!(resolve_project_input("AB", &tasks_dir), "AB");
    assert_eq!(resolve_project_input("API-BACKEND", &tasks_dir), "AB");
}

#[test]
fn unique_prefix_generation_and_validation() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join(".tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    assert_eq!(
        generate_unique_project_prefix("frontend", &tasks_dir).unwrap(),
        "FRON"
    );
    assert_eq!(
        generate_unique_project_prefix("api-backend", &tasks_dir).unwrap(),
        "AB"
    );
    let existing_project_dir = tasks_dir.join("BACK");
    std::fs::create_dir_all(&existing_project_dir).unwrap();
    std::fs::write(
        lotar::utils::paths::project_config_path(
            temp_dir.path().join(".tasks").as_path(),
            existing_project_dir.file_name().unwrap().to_str().unwrap(),
        ),
        "project_name: backend\n",
    )
    .unwrap();
    let err = generate_unique_project_prefix("BACK", &tasks_dir).unwrap_err();
    assert!(err.contains("Project names cannot match existing prefixes"));
    let err = generate_unique_project_prefix("backend", &tasks_dir).unwrap_err();
    assert!(err.contains("prefix is already in use"));
    assert!(generate_unique_project_prefix("backend-api", &tasks_dir).is_ok());
    assert!(generate_unique_project_prefix("back-end", &tasks_dir).is_ok());
    let err = generate_unique_project_prefix("back", &tasks_dir).unwrap_err();
    assert!(err.contains("Project names cannot match existing prefixes"));
    let existing_project_dir2 = tasks_dir.join("FRON");
    std::fs::create_dir_all(&existing_project_dir2).unwrap();
    std::fs::write(
        lotar::utils::paths::project_config_path(
            temp_dir.path().join(".tasks").as_path(),
            existing_project_dir2.file_name().unwrap().to_str().unwrap(),
        ),
        "project_name: frontend\n",
    )
    .unwrap();
    let err = validate_explicit_prefix("frontend", "new-project", &tasks_dir).unwrap_err();
    assert!(err.contains("prefix conflicts with existing project name"));
    let err = validate_explicit_prefix("FRON", "backend", &tasks_dir).unwrap_err();
    assert!(err.contains("prefix is already used by project"));
    assert!(validate_explicit_prefix("BACK", "backend", &tasks_dir).is_ok());
    assert!(validate_explicit_prefix("FRON", "frontend", &tasks_dir).is_ok());
}
