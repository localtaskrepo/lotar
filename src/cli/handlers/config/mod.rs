use crate::cli::handlers::{CommandHandler, emit_subcommand_overview};
use crate::cli::{ConfigAction, ConfigNormalizeArgs, ConfigShowArgs, ConfigValidateArgs};
use crate::output::OutputRenderer;
use crate::workspace::TasksDirectoryResolver;

pub struct ConfigHandler;

mod init;
mod normalize;
mod render;
mod set;
mod show;
mod validate;

impl CommandHandler for ConfigHandler {
    type Args = Option<ConfigAction>;
    type Result = Result<(), String>;

    fn execute(
        args: Self::Args,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Self::Result {
        let Some(action) = args else {
            emit_subcommand_overview(renderer, &["config"]);
            return Ok(());
        };

        match action {
            ConfigAction::Show(ConfigShowArgs {
                project,
                explain,
                full,
            }) => Self::handle_config_show(resolver, renderer, project, explain, full),
            ConfigAction::Set(crate::cli::ConfigSetArgs {
                field,
                value,
                dry_run,
                force,
                global,
            }) => Self::handle_config_set(
                resolver, renderer, &field, &value, dry_run, force, global, project,
            ),
            ConfigAction::Init(args) => Self::handle_config_init(resolver, renderer, args.as_ref()),
            ConfigAction::Validate(ConfigValidateArgs {
                project,
                global,
                fix,
                errors_only,
            }) => {
                Self::handle_config_validate(resolver, renderer, project, global, fix, errors_only)
            }
            ConfigAction::Templates => {
                renderer.emit_success("Available Workflow Presets:");
                renderer.emit_raw_stdout("  • default   - Minimal setup; inherits global defaults");
                renderer.emit_raw_stdout(
                    "  • agile     - Epic/Feature/Bug/Spike/Chore with Verify state",
                );
                renderer.emit_raw_stdout(
                    "  • kanban    - Flow-based states (Todo/InProgress/Verify/Done)",
                );
                renderer.emit_raw_stdout("");
                renderer.emit_success("Legacy template aliases (produce preset + scaffolds):");
                renderer.emit_raw_stdout("  • agent-pipeline - default workflow + --with=agents:pipeline,automation:pipeline");
                renderer.emit_raw_stdout("  • agent-reviewed - default workflow + --with=agents:reviewed,automation:reviewed");
                renderer
                    .emit_raw_stdout("  • jira           - default workflow + --with=sync:jira");
                renderer
                    .emit_raw_stdout("  • github         - default workflow + --with=sync:github");
                renderer.emit_raw_stdout(
                    "  • jira-github    - default workflow + --with=sync:jira,sync:github",
                );
                renderer.emit_info(
                    "Use 'lotar init --workflow=<name>' (or --template=<legacy-name>) to initialize.",
                );
                Ok(())
            }
            ConfigAction::Normalize(ConfigNormalizeArgs {
                global,
                project,
                write,
            }) => Self::handle_config_normalize(resolver, renderer, global, project, write),
        }
    }
}
