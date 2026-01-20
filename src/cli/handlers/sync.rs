use crate::cli::args::{SyncArgs, SyncCheckArgs};
use crate::cli::project::ProjectResolver;
use crate::output::{OutputFormat, OutputRenderer};
use crate::services::sync_service::{SyncDirection, SyncService};
use crate::workspace::TasksDirectoryResolver;

pub struct SyncHandler;

impl SyncHandler {
    pub fn execute(
        direction: SyncDirection,
        args: SyncArgs,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let mut project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;

        let project_prefix = if let Some(explicit) = project {
            Some(
                project_resolver
                    .resolve_project("", Some(explicit))
                    .map_err(|e| format!("Project resolution failed: {}", e))?,
            )
        } else {
            let default_project = project_resolver
                .get_config()
                .default_project
                .trim()
                .to_string();
            if default_project.is_empty() {
                None
            } else {
                Some(default_project)
            }
        };

        let result = match direction {
            SyncDirection::Push => SyncService::push(
                resolver,
                &args.remote,
                project_prefix.as_deref(),
                args.dry_run,
                args.auth_profile.as_deref(),
                args.task_id.as_deref(),
                None,
                false,
                None,
            ),
            SyncDirection::Pull => SyncService::pull(
                resolver,
                &args.remote,
                project_prefix.as_deref(),
                args.dry_run,
                args.auth_profile.as_deref(),
                args.task_id.as_deref(),
                None,
                false,
                None,
            ),
        }
        .map_err(|e| e.to_string())?;

        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&result);
            return Ok(());
        }

        renderer.emit_success(format!(
            "Sync {} complete ({})",
            result.direction, result.remote
        ));
        if let Some(project) = result.project.as_deref() {
            renderer.emit_info(format!("Project: {}", project));
        }
        renderer.emit_info(format!(
            "Created: {}, Updated: {}, Skipped: {}, Failed: {}",
            result.summary.created,
            result.summary.updated,
            result.summary.skipped,
            result.summary.failed
        ));

        for warning in &result.warnings {
            renderer.emit_warning(warning);
        }
        for note in &result.info {
            renderer.emit_notice(note);
        }

        Ok(())
    }

    pub fn check(
        args: SyncCheckArgs,
        project: Option<&str>,
        resolver: &TasksDirectoryResolver,
        renderer: &OutputRenderer,
    ) -> Result<(), String> {
        let mut project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;

        let project_prefix = if let Some(explicit) = project {
            Some(
                project_resolver
                    .resolve_project("", Some(explicit))
                    .map_err(|e| format!("Project resolution failed: {}", e))?,
            )
        } else {
            let default_project = project_resolver
                .get_config()
                .default_project
                .trim()
                .to_string();
            if default_project.is_empty() {
                None
            } else {
                Some(default_project)
            }
        };

        let result = SyncService::validate(
            resolver,
            project_prefix.as_deref(),
            Some(&args.remote),
            None,
            args.auth_profile.as_deref(),
        )
        .map_err(|e| e.to_string())?;

        if matches!(renderer.format, OutputFormat::Json) {
            renderer.emit_json(&result);
            return Ok(());
        }

        renderer.emit_success(format!(
            "Sync check OK ({} {})",
            result.provider, result.remote
        ));
        if let Some(project) = result.project.as_deref() {
            renderer.emit_info(format!("Project: {}", project));
        }
        if let Some(repo) = result.repo.as_deref() {
            renderer.emit_info(format!("Repo: {}", repo));
        }
        if let Some(filter) = result.filter.as_deref() {
            renderer.emit_info(format!("Filter: {}", filter));
        }

        for warning in &result.warnings {
            renderer.emit_warning(warning);
        }
        for note in &result.info {
            renderer.emit_notice(note);
        }

        Ok(())
    }
}
