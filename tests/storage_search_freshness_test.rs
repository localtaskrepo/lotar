//! Contract tests for storage search freshness: the mtime-keyed task cache
//! must behave exactly like an uncached scan — external edits, deletes, and
//! new files are all reflected in subsequent searches.

use lotar::storage::manager::Storage;
use lotar::storage::task::Task;
use lotar::types::Priority;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn write_task_file(dir: &Path, id: u32, title: &str) {
    let task = Task::new(dir.to_path_buf(), title.to_string(), Priority::default());
    let yaml = serde_yaml_ng::to_string(&task).expect("serialize task");
    fs::write(dir.join(format!("{id}.yml")), yaml).expect("write task file");
}

fn search_titles(storage: &Storage) -> Vec<String> {
    storage
        .search(&Default::default())
        .into_iter()
        .map(|(_, task)| task.title.clone())
        .collect()
}

/// Sleep long enough that a subsequent file write is guaranteed a strictly
/// newer mtime than the cached one (APFS has ns granularity, but be safe).
fn tick() {
    thread::sleep(Duration::from_millis(20));
}

#[test]
fn search_reflects_external_file_edits_across_calls() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join("tasks");
    let project_dir = tasks_dir.join("CACHE");
    fs::create_dir_all(&project_dir).unwrap();
    write_task_file(&project_dir, 1, "original title");

    let storage = Storage::new(&tasks_dir);
    assert_eq!(search_titles(&storage), vec!["original title"]);

    // Prime any cache with a second search, then mutate the file behind the
    // storage layer's back (as an external editor or `lotar` CLI would).
    assert_eq!(search_titles(&storage), vec!["original title"]);
    tick();
    write_task_file(&project_dir, 1, "edited externally");

    assert_eq!(search_titles(&storage), vec!["edited externally"]);
}

#[test]
fn search_reflects_external_deletes_and_adds_across_calls() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join("tasks");
    let project_dir = tasks_dir.join("CACHE");
    fs::create_dir_all(&project_dir).unwrap();
    write_task_file(&project_dir, 1, "keep me");
    write_task_file(&project_dir, 2, "delete me");

    let storage = Storage::new(&tasks_dir);
    assert_eq!(search_titles(&storage).len(), 2);

    // Prime, then delete one file and add another externally.
    assert_eq!(search_titles(&storage).len(), 2);
    fs::remove_file(project_dir.join("2.yml")).unwrap();
    tick();
    write_task_file(&project_dir, 3, "added externally");

    let mut titles = search_titles(&storage);
    titles.sort();
    assert_eq!(titles, vec!["added externally", "keep me"]);
}

#[test]
fn search_survives_a_file_replaced_with_garbage() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join("tasks");
    let project_dir = tasks_dir.join("CACHE");
    fs::create_dir_all(&project_dir).unwrap();
    write_task_file(&project_dir, 1, "will break");

    let storage = Storage::new(&tasks_dir);
    assert_eq!(search_titles(&storage).len(), 1);

    tick();
    fs::write(project_dir.join("1.yml"), "{{{ not yaml").unwrap();

    // Corrupt files are skipped, not fatal — same as the uncached behavior.
    assert_eq!(search_titles(&storage).len(), 0);

    tick();
    write_task_file(&project_dir, 1, "repaired");
    assert_eq!(search_titles(&storage), vec!["repaired"]);
}
