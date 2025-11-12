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
                resolver, renderer, field, value, dry_run, force, global, project,
            ),
            ConfigAction::Init(crate::cli::ConfigInitArgs {
                template,
                prefix,
                project,
                copy_from,
                global,
                dry_run,
                force,
            }) => Self::handle_config_init(
                resolver, renderer, template, prefix, project, copy_from, global, dry_run, force,
            ),
            ConfigAction::Validate(ConfigValidateArgs {
                project,
                global,
                fix,
                errors_only,
            }) => {
                Self::handle_config_validate(resolver, renderer, project, global, fix, errors_only)
            }
            ConfigAction::Templates => {
                renderer.emit_success("Available Configuration Templates:");
                renderer.emit_raw_stdout("  • default - Basic task management setup");
                renderer.emit_raw_stdout("  • agile - Agile/Scrum workflow configuration");
                renderer.emit_raw_stdout("  • kanban - Kanban board style setup");
                renderer.emit_info(
                    "Use 'lotar config init --template=<n>' to initialize with a template.",
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
