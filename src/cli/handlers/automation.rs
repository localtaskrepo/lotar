use crate::cli::args::{AutomationAction, AutomationRunArgs, AutomationSimulateArgs};
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
            AutomationAction::Simulate(args) => Self::simulate(&args, resolver, renderer),
            AutomationAction::Run(args) => Self::run(&args, resolver, renderer),
        }
    }

    /// `lotar automation run` (DEV-241): the way to run automations on a
    /// schedule is to call this from cron or CI.
    fn run(
        args: &AutomationRunArgs,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let mut storage = crate::storage::manager::Storage::new(&resolver.path);
        let report = AutomationService::run_manual(
            &mut storage,
            &args.rules,
            args.select.as_deref(),
            args.dry_run,
        )
        .map_err(|e| e.to_string())?;
        let failed = report.entries.iter().filter(|e| e.error.is_some()).count();
        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&serde_json::json!({
                "status": if failed == 0 { "ok" } else { "error" },
                "action": "automation.run",
                "report": report,
            }));
        } else {
            if report.rules.is_empty() {
                renderer.emit_info(format_args!(
                    "No enabled rule has a manual action (on.manual); nothing to run."
                ));
            } else if report.entries.is_empty() {
                renderer.emit_info(format_args!(
                    "Checked {} task(s) against {}: none match.",
                    report.checked,
                    report.rules.join(", ")
                ));
            }
            for entry in &report.entries {
                let verb = if report.dry_run { "Would run" } else { "Ran" };
                match &entry.error {
                    Some(error) => renderer.emit_error(format_args!(
                        "{} on {} failed: {error}",
                        entry.rule, entry.task
                    )),
                    None => renderer
                        .emit_success(format_args!("{verb} '{}' on {}", entry.rule, entry.task)),
                }
                for action in &entry.actions {
                    renderer.emit_info(format_args!("  {}", action.description));
                }
                for warning in &entry.warnings {
                    renderer.emit_info(format_args!("  note: {warning}"));
                }
            }
        }
        if failed > 0 {
            return Err(format!("{failed} automation action(s) failed"));
        }
        Ok(())
    }

    fn simulate(
        args: &AutomationSimulateArgs,
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
                "warnings": result.warnings,
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
        for warning in &result.warnings {
            renderer.emit_warning(format_args!("{}", warning));
        }

        Ok(())
    }
}
