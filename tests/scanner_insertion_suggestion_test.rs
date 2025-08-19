use lotar::scanner::Scanner;
use std::path::PathBuf;

#[test]
fn suggest_insertion_after_signal_word() {
    let s = Scanner::new(PathBuf::from("."));
    let line = "// TODO: implement later";
    let edited = s.suggest_insertion_for_line(line, "DEMO-123").unwrap();
    assert!(edited.contains("TODO (DEMO-123):"), "{}", edited);
}

#[test]
fn idempotent_no_duplicate_insertion() {
    let s = Scanner::new(PathBuf::from("."));
    let line = "// TODO (DEMO-123): implement later";
    assert!(s.suggest_insertion_for_line(line, "DEMO-123").is_none());
}

#[test]
fn insert_before_colon_or_dash() {
    let s = Scanner::new(PathBuf::from("."));
    let line_colon = "// TODO Feature: thing";
    let edited_colon = s
        .suggest_insertion_for_line(line_colon, "DEMO-123")
        .unwrap();
    assert!(
        edited_colon.contains("TODO (DEMO-123) Feature:"),
        "{}",
        edited_colon
    );

    let line_dash = "# TODO Chore - tidy";
    let edited_dash = s.suggest_insertion_for_line(line_dash, "DEMO-123").unwrap();
    assert!(
        edited_dash.contains("TODO (DEMO-123) Chore -"),
        "{}",
        edited_dash
    );
}
