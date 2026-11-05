use lotar::workspace::TasksDirectoryResolver;
use tempfile::TempDir;

#[test]
fn explicit_path_resolution_creates_dir_and_resolves() {
    let temp_dir = TempDir::new().unwrap();
    let tasks_dir = temp_dir.path().join("custom_tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    let resolver = TasksDirectoryResolver::resolve(tasks_dir.to_str(), None).unwrap();
    assert_eq!(resolver.path, tasks_dir);
}
