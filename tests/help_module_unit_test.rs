use lotar::help::HelpSystem;
use lotar::output::OutputFormat;

#[test]
fn help_system_creation() {
    let help = HelpSystem::new(OutputFormat::Text, false);
    // Just ensure instance is created; internal fields are private
    let _ = help;
}

#[test]
fn help_list_available() {
    let help = HelpSystem::new(OutputFormat::Text, false);
    let result = help.list_available_help();
    assert!(result.is_ok());
}

#[test]
fn help_alias_resolution() {
    let help = HelpSystem::new(OutputFormat::Text, false);
    let sprint = help.show_command_help("sprint");
    assert!(
        sprint
            .expect("sprint help should resolve")
            .contains("lotar sprint")
    );
}

#[test]
fn sync_help_answers_for_pull_and_push() {
    // DEV-294: `lotar help sync|pull|push` had no page.
    let help = HelpSystem::new(OutputFormat::Text, false);
    for topic in ["sync", "pull", "push"] {
        let text = help
            .show_command_help(topic)
            .unwrap_or_else(|e| panic!("{topic}: {e}"));
        assert!(text.contains("sync check"), "{topic}: {text}");
    }
}
