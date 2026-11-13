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
    assert!(sprint.expect("sprint help should resolve").contains("lotar sprint"));
}

#[test]
fn sync_help_answers_for_pull_and_push() {
    // DEV-294: `lotar help sync|pull|push` had no page.
    let help = HelpSystem::new(OutputFormat::Text, false);
    for topic in ["sync", "pull", "push"] {
        let text = help.show_command_help(topic).unwrap_or_else(|e| panic!("{topic}: {e}"));
        assert!(text.contains("sync check"), "{topic}: {text}");
    }
}

/// DEV-284: placeholders survive rendering, plain output carries no
/// escapes, numbered lists keep numbers, and topics are listable.
#[test]
fn rendering_keeps_placeholders_numbers_and_plain_text() {
    // SAFETY: test-local style override.
    unsafe { std::env::set_var("LOTAR_HELP_STYLE", "plain") };
    let help = HelpSystem::new(OutputFormat::Text, false);
    let changelog = help.show_command_help("changelog").unwrap();
    assert!(changelog.contains("<ref>"), "{changelog}");
    let main = help.show_command_help("main").unwrap();
    assert!(!main.contains('\u{1b}'), "plain help must not carry escapes");
    let precedence = help.show_command_help("precedence").unwrap();
    assert!(precedence.contains("1. "), "{precedence}");
    let topics = help.show_command_help("topics").unwrap();
    assert!(topics.contains("environment") && topics.contains("sse"), "{topics}");
    unsafe { std::env::remove_var("LOTAR_HELP_STYLE") };
}
