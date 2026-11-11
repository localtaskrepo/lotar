//! DEV-258: template values in short-form `run` commands must never be
//! parsed as shell code, whether the placeholder is unquoted, inside double
//! quotes, or inside a nested `sh -c '...'`.
#![cfg(unix)]

use crate::common;
use common::TestFixtures;

#[test]
fn task_titles_cannot_inject_commands_into_run() {
    let fixtures = TestFixtures::new();
    let root = fixtures.get_temp_path();
    let log = root.join("notify.log");
    let log = log.to_string_lossy();
    let automation = format!(
        "automation:\n  rules:\n\
         \x20   - name: nested\n\
         \x20     on:\n\
         \x20       created:\n\
         \x20         run: \"sh -c 'echo nested: ${{{{ticket.title}}}} >> {log}'\"\n\
         \x20   - name: double\n\
         \x20     on:\n\
         \x20       created:\n\
         \x20         run: \"echo \\\"double: ${{{{ticket.title}}}}\\\" >> {log}\"\n\
         \x20   - name: plain\n\
         \x20     on:\n\
         \x20       created:\n\
         \x20         run: \"echo plain: ${{{{ticket.title}}}} >> {log}\"\n"
    );
    std::fs::write(fixtures.tasks_root.join("automation.yml"), automation).unwrap();

    let title = "Hi $(touch INJECTED1) `touch INJECTED2` it's; touch INJECTED3";
    let output = common::lotar_cmd()
        .unwrap()
        .current_dir(root)
        .env("LOTAR_TEST_SILENT", "1")
        .args(["--tasks-dir"])
        .arg(&fixtures.tasks_root)
        .args(["add", title, "--project=INJ"])
        .output()
        .unwrap();
    assert!(output.status.success(), "add failed: {}", String::from_utf8_lossy(&output.stderr));

    for marker in ["INJECTED1", "INJECTED2", "INJECTED3"] {
        assert!(!root.join(marker).exists(), "{marker} was created: the title ran as a command");
    }
    let logged = std::fs::read_to_string(root.join("notify.log")).unwrap();
    for prefix in ["nested", "double", "plain"] {
        assert!(logged.contains(&format!("{prefix}: {title}")), "{prefix} rule should print the title verbatim:\n{logged}");
    }
}
