use crate::cli::args::{AutomationAction, AutomationSimulateArgs};
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::automation_service::{AutomationEvent, AutomationService};
use crate::workspace::TasksDirectoryResolver;

pub struct AutomationHandler;

impl AutomationHandler {
    pub fn execute(
        action: AutomationAction,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        match action {
            AutomationAction::Simulate(args) => Self::simulate(args, resolver, renderer),
        }
    }

    fn simulate(
        args: AutomationSimulateArgs,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let event: AutomationEvent = args.event.parse()?;

        let result = AutomationService::simulate(resolver.path.as_path(), &args.ticket, event)
            .map_err(|e| e.to_string())?;

        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({
                "matched": result.matched,
                "rule_name": result.rule_name,
                "actions": result.actions.iter().map(|a| serde_json::json!({
                    "action": a.action,
                    "description": a.description,
                })).collect::<Vec<_>>(),
                "task_after": result.task_after,
            }));
            return Ok(());
        }

        if !result.matched {
            renderer.emit_info(format_args!(
                "No automation rules matched ticket {} for event '{}'.",
                args.ticket, args.event
            ));
            return Ok(());
        }

        if let Some(name) = &result.rule_name {
            renderer.emit_info(format_args!("Rule matched: {}", name));
        }

        if result.actions.is_empty() {
            renderer.emit_info(format_args!(
                "Rule matched but no actions defined for this event."
            ));
        } else {
            renderer.emit_info(format_args!("Actions that would be performed:"));
            for action in &result.actions {
                renderer.emit_info(format_args!("  {} — {}", action.action, action.description));
            }
        }

        Ok(())
    }
}
