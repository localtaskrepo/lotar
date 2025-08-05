use lotar::scanner::Scanner;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

mod common;
use common::TestFixtures;

#[test]
fn test_scan() {
    // Create a temporary directory and files to scan
    let temp_dir = tempdir().unwrap();
    let rust_file_path = temp_dir.path().join("test.rs");
    let js_file_path = temp_dir.path().join("test.js");
    let mut rust_file = File::create(&rust_file_path).unwrap();
    let mut js_file = File::create(&js_file_path).unwrap();
    writeln!(rust_file, "fn main() {{\n    // TODO: Test Rust\n}}").unwrap();
    writeln!(js_file, "function test() {{\n    // TODO: Test JavaScript\n}}").unwrap();

    // Create a new scanner and scan the temporary directory
    let mut scanner = Scanner::new(temp_dir.path().to_path_buf());
    let references = scanner.scan();

    // Check that the correct number of references were found
    assert_eq!(references.len(), 2);

    // Verify the references contain expected data
    let rust_todo = references.iter().find(|r| r.file_path.ends_with("test.rs"));
    assert!(rust_todo.is_some());

    let js_todo = references.iter().find(|r| r.file_path.ends_with("test.js"));
    assert!(js_todo.is_some());

    // Check that TODO text is captured correctly
    if let Some(todo) = rust_todo {
        assert!(todo.title.contains("Test Rust"));
    }
}

#[test]
fn test_scan_with_uuid_in_comment() {
    let fixtures = TestFixtures::new();
    let _source_files = fixtures.create_test_source_files();

    let mut scanner = Scanner::new(fixtures.get_temp_path().to_path_buf());
    let references = scanner.scan();

    // Should find multiple TODO comments
    assert!(references.len() >= 3);

    // Check for UUID extraction - look in the uuid field, not the title
    let uuid_todo = references.iter().find(|r| r.uuid.contains("uuid-1234"));
    assert!(uuid_todo.is_some());

    if let Some(todo) = uuid_todo {
        assert!(todo.uuid.contains("uuid-1234"));
        assert!(todo.title.contains("Test Rust with UUID"));
    }
}

#[test]
fn test_scan_multiple_languages() {
    let fixtures = TestFixtures::new();
    fixtures.create_test_source_files();

    let mut scanner = Scanner::new(fixtures.get_temp_path().to_path_buf());
    let references = scanner.scan();

    // Should find TODOs in Rust, JavaScript, and Python files
    let rust_todos = references.iter().filter(|r| {
        r.file_path.extension().map_or(false, |ext| ext == "rs")
    }).count();
    let js_todos = references.iter().filter(|r| {
        r.file_path.extension().map_or(false, |ext| ext == "js")
    }).count();
    let py_todos = references.iter().filter(|r| {
        r.file_path.extension().map_or(false, |ext| ext == "py")
    }).count();

    assert!(rust_todos > 0, "Should find Rust TODOs");
    assert!(js_todos > 0, "Should find JavaScript TODOs");
    assert!(py_todos > 0, "Should find Python TODOs");
}
