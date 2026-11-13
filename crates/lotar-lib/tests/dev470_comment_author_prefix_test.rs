//! DEV-470: a comment that starts with its own author's name ("claude: ...")
//! is stored without it; the author is recorded already. A comment that is
//! only the name is refused.
use serde_json::Value;
use std::path::Path;

use crate::common;
use common::TestFixtures;

fn run(dir: &Path, args: &[&str]) -> String {
    let out = common::lotar_cmd().unwrap().current_dir(dir).env("LOTAR_DEFAULT_REPORTER", "claude").args(args).output().unwrap();
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn comment_starting_with_author_name_is_stored_without_it() {
    let tf = TestFixtures::new();
    let dir = tf.get_temp_path();
    let created: Value = serde_json::from_str(&run(dir, &["--format", "json", "add", "Prefixed comments"])).unwrap();
    let id = created["task"]["id"].as_str().expect("created id").to_string();

    for (text, stored) in [
        ("claude: test", "test"),
        ("Claude, done here", "done here"),
        ("claude (DEV-459 agent): fixed it", "fixed it"),
        ("claude, 2026-11-13: merged", "merged"),
        ("claudette: not the author", "claudette: not the author"),
        ("codex: someone else", "codex: someone else"),
    ] {
        let added: Value = serde_json::from_str(&run(dir, &["--format", "json", "comment", &id, text])).unwrap();
        assert_eq!(added["added_comment"]["text"], stored, "{text}: {added}");
        assert_eq!(added["added_comment"]["author"], "claude", "{added}");
    }

    let preview: Value = serde_json::from_str(&run(dir, &["--format", "json", "comment", &id, "claude: preview", "--dry-run"])).unwrap();
    assert_eq!(preview["added_comment"]["text"], "preview", "the dry run shows what would be stored: {preview}");

    for args in [vec!["comment", &id, "claude:"], vec!["comment", &id, "Claude (agent):"], vec!["comment", &id, "claude:", "--dry-run"]] {
        let out = common::lotar_cmd().unwrap().current_dir(dir).env("LOTAR_DEFAULT_REPORTER", "claude").args(&args).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && stderr.contains("records the author (claude) already"), "{args:?}: {stderr}");
    }
    let listed: Value = serde_json::from_str(&run(dir, &["--format", "json", "comment", &id])).unwrap();
    assert_eq!(listed["comments"], 6, "refused comments are not stored: {listed}");

    let shown = run(dir, &["show", &id]);
    assert!(shown.contains("claude: test") && !shown.contains("claude: claude: test"), "{shown}");
}
