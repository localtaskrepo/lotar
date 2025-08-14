use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tempfile::tempdir;

use lotar::scanner::Scanner;

fn write(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut f = fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[test]
fn detects_common_signal_words_by_default() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    write(
        &root.join("file.rs"),
        "
// TODO: one
// FIXME: two
// HACK: three
// BUG: four
// NOTE: five
",
    );

    let mut scanner = Scanner::new(PathBuf::from(root));
    let results = scanner.scan();

    // Should find 5 references, one per line
    assert_eq!(results.len(), 5);
}

#[test]
fn custom_signal_words_can_be_provided() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    write(
        &root.join("file.rs"),
        "
// FOO: hidden
// BAR: visible
",
    );

    let mut scanner = Scanner::new(PathBuf::from(root)).with_signal_words(&["bar".into()]);

    let results = scanner.scan();
    assert_eq!(results.len(), 1);
}
