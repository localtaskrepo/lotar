use lotar::scanner::Scanner;
use std::fs;
use std::path::PathBuf;

fn write_file(root: &std::path::Path, rel: &str, contents: &str) -> PathBuf {
    let p = root.join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&p, contents).unwrap();
    p
}

#[test]
fn bare_ticket_key_is_not_detected_without_toggle() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // No signal word, just a key
    write_file(root, "src/main.rs", "// AUTH-123 implement login");

    let mut scanner = Scanner::new(PathBuf::from(root));
    // default is enable=false; make it explicit
    scanner = scanner.with_ticket_detection(None, false);
    let refs = scanner.scan();
    assert_eq!(refs.len(), 0, "expected no detections without toggle");

    // But extraction helper should still find the key
    assert_eq!(
        scanner.extract_ticket_key_from_line("// AUTH-123 implement login"),
        Some("AUTH-123".to_string())
    );
}

#[test]
fn issue_type_word_is_detected_as_signal() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_file(root, "src/lib.rs", "// Feature: implement signup");

    // Simulate config by adding issue-type word to signal list
    let mut scanner = Scanner::new(PathBuf::from(root))
        .with_signal_words(&["TODO".into(), "FIXME".into(), "Feature".into()])
        .with_ticket_detection(None, false);
    let refs = scanner.scan();
    assert_eq!(refs.len(), 1, "expected one detection with issue-type word");
    assert_eq!(refs[0].title.to_lowercase(), "implement signup");
}
