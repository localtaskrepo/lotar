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
fn custom_ticket_pattern_captures_group_1() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_file(root, "src/a.rs", "// TODO: BLZ_2025-999 do something");

    // Configure a non-standard key like BLZ_2025-999
    let patterns = vec![r"\b(BLZ_\d{4}-\d{3})\b".to_string()];

    // Without toggle: should still detect because of TODO signal, and fill uuid via fallback
    let mut scanner =
        Scanner::new(PathBuf::from(root)).with_ticket_detection(Some(&patterns), false);
    let refs = scanner.scan();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].uuid, "BLZ_2025-999");
}

#[test]
fn custom_pattern_does_not_trigger_without_signal_even_when_toggle_on() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_file(root, "src/b.rs", "// BLZ_2025-777 implement");

    let patterns = vec![r"\b(BLZ_\d{4}-\d{3})\b".to_string()];

    let mut scanner =
        Scanner::new(PathBuf::from(root)).with_ticket_detection(Some(&patterns), true);
    let refs = scanner.scan();
    // With new semantics, a signal word is required regardless of custom patterns or toggles
    assert_eq!(refs.len(), 0);
}

#[test]
fn invalid_patterns_are_ignored_gracefully() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_file(root, "src/c.rs", "// TODO: AUTH-321 fix");

    // Invalid regex should be ignored; built-in key still extracted
    let patterns = vec!["[".to_string()];
    let mut scanner =
        Scanner::new(PathBuf::from(root)).with_ticket_detection(Some(&patterns), false);
    let refs = scanner.scan();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].uuid, "AUTH-321");
}
