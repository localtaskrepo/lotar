//! Contract tests for storage search freshness: the content-keyed parse cache
//! must behave exactly like an uncached scan — external edits, deletes, and
//! new files are all reflected in subsequent searches.
//!
//! Tests control timestamps explicitly, including unchanged mtime/length cases,
//! without wall-clock sleeps or filesystem clock-granularity assumptions.

mod common;

use lotar::storage::manager::Storage;
use lotar::storage::task::Task;
use lotar::types::Priority;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn write_task_file(dir: &Path, id: u32, title: &str) {
    let mut task = Task::new(dir.to_path_buf(), title.to_string(), Priority::default());
    // `Task::new` stamps `created` from the wall clock, and RFC3339 output
    // trims subsecond trailing zeros (0/3/6/9 fractional digits vary by call
    // and host clock granularity). Pin it so the serialized byte length
    // depends only on the title bytes.
    task.created = "2026-01-01T00:00:00.000000000+00:00".to_string();
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

struct MtimeClock {
    next: SystemTime,
}

impl MtimeClock {
    fn new() -> Self {
        Self {
            next: SystemTime::now()
                .checked_add(Duration::from_secs(600))
                .expect("clock arithmetic"),
        }
    }

    fn advance(&mut self) -> SystemTime {
        self.next = self
            .next
            .checked_add(Duration::from_secs(1))
            .expect("clock arithmetic");
        self.next
    }
}

fn set_mtime(path: &Path, at: SystemTime) {
    let file = fs::File::options()
        .write(true)
        .open(path)
        .expect("open file for set_modified");
    file.set_modified(at).expect("set modified time");
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
    write_task_file(&project_dir, 1, "edited externally");
    let mut clock = MtimeClock::new();
    set_mtime(&project_dir.join("1.yml"), clock.advance());

    assert_eq!(search_titles(&storage), vec!["edited externally"]);
}

#[test]
fn search_detects_same_length_title_edit_via_mtime_only() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join("tasks");
    let project_dir = tasks_dir.join("CACHE");
    fs::create_dir_all(&project_dir).unwrap();
    let task_file = project_dir.join("1.yml");
    write_task_file(&project_dir, 1, "original title");

    let storage = Storage::new(&tasks_dir);
    assert_eq!(search_titles(&storage), vec!["original title"]);
    assert_eq!(search_titles(&storage), vec!["original title"]);
    let len_before = fs::metadata(&task_file).unwrap().len();

    // Same byte length: only the mtime dimension of the (mtime, len)
    // fingerprint changes, so a length-keyed or mtime-blind cache would
    // serve the stale task.
    write_task_file(&project_dir, 1, "modified title");
    let len_after = fs::metadata(&task_file).unwrap().len();
    assert_eq!(
        len_before, len_after,
        "test premise: replacement must keep the file length identical"
    );
    let mut clock = MtimeClock::new();
    set_mtime(&task_file, clock.advance());

    assert_eq!(search_titles(&storage), vec!["modified title"]);
}

#[test]
fn search_and_get_detect_same_length_edits_with_unchanged_mtime() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join("tasks");
    let project_dir = tasks_dir.join("CACHE");
    fs::create_dir_all(&project_dir).unwrap();
    let task_file = project_dir.join("1.yml");
    write_task_file(&project_dir, 1, "original title");
    let storage = Storage::new(&tasks_dir);
    assert_eq!(search_titles(&storage), vec!["original title"]);
    assert_eq!(
        storage.get("CACHE-1", "CACHE").unwrap().title,
        "original title"
    );
    let before = fs::metadata(&task_file).unwrap();
    let mtime = before.modified().unwrap();
    write_task_file(&project_dir, 1, "modified title");
    set_mtime(&task_file, mtime);
    assert_eq!(fs::metadata(&task_file).unwrap().len(), before.len());
    assert_eq!(
        storage.get("CACHE-1", "CACHE").unwrap().title,
        "modified title"
    );
    assert_eq!(search_titles(&storage), vec!["modified title"]);
    write_task_file(&project_dir, 1, "original title");
    set_mtime(&task_file, mtime);
    assert_eq!(search_titles(&storage), vec!["original title"]);
    assert_eq!(
        storage.get("CACHE-1", "CACHE").unwrap().title,
        "original title"
    );
}

#[test]
fn cached_task_detects_atomic_replacement_with_preserved_metadata() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join("tasks");
    let project_dir = tasks_dir.join("CACHE");
    fs::create_dir_all(&project_dir).unwrap();
    write_task_file(&project_dir, 1, "original title");
    let task_file = project_dir.join("1.yml");
    let storage = Storage::new(&tasks_dir);
    assert_eq!(search_titles(&storage), vec!["original title"]);
    let before = fs::metadata(&task_file).unwrap();
    write_task_file(&project_dir, 2, "modified title");
    let replacement = project_dir.join("2.yml");
    set_mtime(&replacement, before.modified().unwrap());
    assert_eq!(fs::metadata(&replacement).unwrap().len(), before.len());
    fs::rename(replacement, &task_file).unwrap();
    assert_eq!(search_titles(&storage), vec!["modified title"]);
    assert_eq!(
        storage.get("CACHE-1", "CACHE").unwrap().title,
        "modified title"
    );
}

#[test]
fn cached_task_does_not_mask_corruption_with_preserved_metadata() {
    let tmp = TempDir::new().unwrap();
    let tasks_dir = tmp.path().join("tasks");
    let project_dir = tasks_dir.join("CACHE");
    fs::create_dir_all(&project_dir).unwrap();
    write_task_file(&project_dir, 1, "original title");
    let task_file = project_dir.join("1.yml");
    let storage = Storage::new(&tasks_dir);
    assert_eq!(search_titles(&storage), vec!["original title"]);
    let before = fs::metadata(&task_file).unwrap();
    let mtime = before.modified().unwrap();
    fs::write(&task_file, "x".repeat(before.len() as usize)).unwrap();
    set_mtime(&task_file, mtime);
    assert!(search_titles(&storage).is_empty());
    assert!(storage.get("CACHE-1", "CACHE").is_none());
    write_task_file(&project_dir, 1, "modified title");
    set_mtime(&task_file, mtime);
    assert_eq!(search_titles(&storage), vec!["modified title"]);
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

    let mut clock = MtimeClock::new();
    fs::write(project_dir.join("1.yml"), "{{{ not yaml").unwrap();
    set_mtime(&project_dir.join("1.yml"), clock.advance());

    // Corrupt files are skipped, not fatal — same as the uncached behavior.
    assert_eq!(search_titles(&storage).len(), 0);

    write_task_file(&project_dir, 1, "repaired");
    set_mtime(&project_dir.join("1.yml"), clock.advance());
    assert_eq!(search_titles(&storage), vec!["repaired"]);
}
