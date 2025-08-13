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
