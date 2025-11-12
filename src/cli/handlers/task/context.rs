use crate::cli::project::ProjectResolver;
use crate::config::types::ResolvedConfig;
use crate::storage::manager::Storage;
use crate::workspace::TasksDirectoryResolver;

/// Shared bootstrap context for task subcommands that need storage,
/// resolved configuration and a project resolver.
pub struct TaskCommandContext {
    pub storage: Storage,
    pub project_resolver: ProjectResolver,
    pub effective_project: Option<String>,
    pub config: ResolvedConfig,
    pub tasks_dir: TasksDirectoryResolver,
}

impl TaskCommandContext {
    pub fn new(
        resolver: &TasksDirectoryResolver,
        project: Option<&str>,
        task_id_hint: Option<&str>,
    ) -> Result<Self, String> {
        Self::with_storage_mode(resolver, project, task_id_hint, true)
    }

    pub fn new_read_only(
        resolver: &TasksDirectoryResolver,
        project: Option<&str>,
        task_id_hint: Option<&str>,
    ) -> Result<Self, String> {
        Self::with_storage_mode(resolver, project, task_id_hint, false)
    }

    pub fn storage_root(&self) -> &std::path::Path {
        self.storage.root_path.as_path()
    }

    pub fn project_prefix_for(&self, project: Option<&str>) -> String {
        if let Some(explicit) = project {
            crate::utils::project::resolve_project_input(explicit, self.tasks_dir.path.as_path())
        } else if let Some(prefix) = self.effective_project.as_ref() {
            prefix.clone()
        } else {
            crate::project::get_effective_project_name(&self.tasks_dir)
        }
    }

    pub fn resolve_full_task_id(
        &mut self,
        raw_id: &str,
        project: Option<&str>,
    ) -> Result<String, String> {
        self.project_resolver.get_full_task_id(raw_id, project)
    }

    pub fn update_effective_project(&mut self, project_prefix: Option<&str>) -> Result<(), String> {
        match project_prefix {
            Some(prefix) if !prefix.trim().is_empty() => {
                let cfg = self
                    .project_resolver
                    .get_project_config(prefix)
                    .map_err(|e| format!("Failed to get project configuration: {}", e))?;
                self.config = cfg;
                self.effective_project = Some(prefix.to_string());
            }
            _ => {
                self.config = self.project_resolver.get_config().clone();
                self.effective_project = None;
            }
        }
        Ok(())
    }

    pub fn resolved_project_name(&self) -> Option<&str> {
        self.effective_project.as_deref()
    }

    fn with_storage_mode(
        resolver: &TasksDirectoryResolver,
        project: Option<&str>,
        task_id_hint: Option<&str>,
        create_storage: bool,
    ) -> Result<Self, String> {
        let storage = if create_storage {
            Storage::new(resolver.path.clone())
        } else {
            Storage::try_open(resolver.path.clone()).ok_or_else(|| {
                "No tasks found. Use 'lotar add' to create tasks first.".to_string()
            })?
        };

        let mut project_resolver = ProjectResolver::new(resolver)
            .map_err(|e| format!("Failed to initialize project resolver: {}", e))?;
        let hint = task_id_hint.unwrap_or("");
        let resolved_prefix = project_resolver
            .resolve_project(hint, project)
            .map_err(|e| format!("Failed to resolve project context: {}", e))?;
        let effective_project = if resolved_prefix.trim().is_empty() {
            None
        } else {
            Some(resolved_prefix.clone())
        };
        let config = match effective_project.as_deref() {
            Some(project_name) => project_resolver
                .get_project_config(project_name)
                .map_err(|e| format!("Failed to get project configuration: {}", e))?,
            None => project_resolver.get_config().clone(),
        };

        Ok(Self {
            storage,
            project_resolver,
            effective_project,
            config,
            tasks_dir: resolver.clone(),
        })
    }
}
