use crate::config::{ConfigManager, types::ProjectConfig};
use crate::output::{LogLevel, OutputFormat, OutputRenderer};
use crate::storage::identity::{TaskId, TaskLocation};
use crate::storage::safety::{atomic_write_file, validate_project_prefix, with_storage_lock};
use crate::storage::task::Task;
#[cfg(test)]
use crate::utils::project::generate_project_prefix;
use std::fs;
use std::path::{Path, PathBuf};

/// Core CRUD operations for task storage
pub struct StorageOperations;

impl StorageOperations {
    /// Add a new task to storage
    pub fn add(
        root_path: &Path,
        task: &Task,
        project_prefix: &str,
        original_project_name: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if let Err(e) = validate_project_prefix(project_prefix) {
            return Err(e.into());
        }

        let project_path = root_path.join(project_prefix);
        fs::create_dir_all(&project_path)?;

        with_storage_lock(&project_path, "task", || {
            // Create project config.yml if it doesn't exist and we have a project name
            let config_file_path =
                crate::utils::paths::project_config_path(root_path, project_prefix);
            if let Some(original_name) = original_project_name {
                let normalized_name = original_name.trim();
                if config_file_path.exists() {
                    match crate::config::persistence::load_project_config_from_dir(
                        project_prefix,
                        root_path,
                    ) {
                        Ok(mut existing) => {
                            let current_name = existing.project_name.trim();
                            if current_name.is_empty()
                                || current_name.eq_ignore_ascii_case(project_prefix)
                            {
                                existing.project_name = normalized_name.to_string();
                                if let Err(e) = ConfigManager::save_project_config(
                                    root_path,
                                    project_prefix,
                                    &existing,
                                ) {
                                    OutputRenderer::new(OutputFormat::Text, LogLevel::Warn)
                                        .log_warn(format_args!(
                                            "Failed to update project config with detected name: {}",
                                            e
                                        ));
                                }
                            }
                        }
                        Err(e) => {
                            OutputRenderer::new(OutputFormat::Text, LogLevel::Warn).log_warn(
                                format_args!("Failed to load existing project config: {}", e),
                            );
                        }
                    }
                } else {
                    let project_config = ProjectConfig::new(normalized_name.to_string());
                    if let Err(e) = ConfigManager::save_project_config(
                        root_path,
                        project_prefix,
                        &project_config,
                    ) {
                        OutputRenderer::new(OutputFormat::Text, LogLevel::Warn)
                            .log_warn(format_args!("Failed to create project config: {}", e));
                    }
                }
            }

            // Get the next numeric ID by finding the highest existing ID
            let next_numeric_id = Self::get_current_id(&project_path) + 1;

            // Create the formatted ID for external use
            let formatted_id = format!("{}-{}", project_prefix, next_numeric_id);

            // Get file path using the numeric ID
            let file_path = Self::get_file_path(project_prefix, next_numeric_id, root_path);
            if std::env::var("LOTAR_DEBUG_STATUS").is_ok() {
                eprintln!("[lotar][debug] writing task file {}", file_path.display());
            }
            let file_string = serde_yaml_ng::to_string(task)?;
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            } else {
                return Err("Invalid target file path".into());
            }
            atomic_write_file(&file_path, &file_string)?;

            // No longer need to update index - simplified architecture

            Ok(formatted_id)
        })
    }

    /// Get a task by ID, searching the primary root and sibling workspace
    /// roots. Returns `None` when the ID is malformed, the explicit project
    /// does not match the ID's prefix (project isolation), the task exists in
    /// more than one root (ambiguous identity fails closed), or no root holds
    /// it. Use [`Self::resolve`] for diagnostics on the failure cases.
    pub fn get(root_path: &Path, id: &str, project: &str) -> Option<Task> {
        let parsed = match TaskId::parse(id) {
            Ok(parsed) => parsed,
            Err(_) => return None,
        };

        // SECURITY: Enforce project isolation - verify the project prefix from
        // the canonical ID parse matches the provided project
        let project_name: &str = if project.trim().is_empty() {
            "default"
        } else {
            project
        };
        if !Self::is_safe_folder_name(project_name) {
            return None;
        }
        if parsed.project != project_name {
            return None;
        }

        let locations = parsed.locate(root_path);
        match locations.len() {
            0 => None,
            // Route through the mtime-validated parse cache shared with search
            1 => crate::storage::search::StorageSearch::load_task_file(&locations[0].file),
            // Duplicate ID across roots: identity is ambiguous, never guess.
            _ => None,
        }
    }

    /// Get a task restricted to a single tasks root, without sibling-root
    /// discovery. Used where a caller must stay locked to one workspace
    /// (sync canonicalization, project-scoped reads).
    pub fn get_in_root(root_path: &Path, id: &str, project: &str) -> Option<Task> {
        let parsed = match TaskId::parse(id) {
            Ok(parsed) => parsed,
            Err(_) => return None,
        };
        let project_name: &str = if project.trim().is_empty() {
            "default"
        } else {
            project
        };
        if !Self::is_safe_folder_name(project_name) {
            return None;
        }
        if parsed.project != project_name {
            return None;
        }
        let file_path = root_path
            .join(&parsed.project)
            .join(format!("{}.yml", parsed.number));
        if file_path.is_file() {
            crate::storage::search::StorageSearch::load_task_file(&file_path)
        } else {
            None
        }
    }

    /// Resolve a task ID to its single storage location across candidate
    /// roots, with fail-closed ambiguity diagnostics.
    pub fn resolve(
        root_path: &Path,
        id: &str,
    ) -> Result<TaskLocation, crate::storage::identity::TaskLookupError> {
        crate::storage::identity::resolve(root_path, id)
    }

    /// Edit an existing task
    ///
    /// Mutations are locked to the tasks root they are issued from: a task
    /// stored in a sibling workspace root is refused before any side effect
    /// (edit it from inside that workspace), and an ID duplicated across
    /// roots is refused rather than guessed.
    pub fn edit(
        root_path: &Path,
        id: &str,
        new_task: &Task,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Extract the project folder from the canonical task ID parse
        let project_folder = match Self::get_project_for_task(id) {
            Some(folder) => folder,
            None => return Err("Invalid task ID format".into()),
        };

        Self::refuse_cross_root(root_path, id)?;

        let project_path = root_path.join(&project_folder);

        with_storage_lock(&project_path, "task", || {
            let (file_path, file_string) = Self::prepare_task_edit(&project_path, id, new_task)?;
            atomic_write_file(&file_path, &file_string)?;

            // No longer need to update index - simplified architecture

            Ok(())
        })
    }

    /// Compute the exact replacement bytes for a task edit without writing.
    /// Re-reads the current file (merging user-owned YAML extensions) so the
    /// result is safe to stage inside a coordinated transaction that already
    /// holds the project task lock (DEV-55).
    pub(crate) fn prepare_task_edit(
        project_path: &Path,
        id: &str,
        new_task: &Task,
    ) -> Result<(PathBuf, String), Box<dyn std::error::Error>> {
        // Use filesystem-based file path resolution
        let file_path = match Self::get_file_path_for_id(project_path, id) {
            Some(path) => path,
            None => return Err("Task file not found".into()),
        };

        // Read fresh rather than using a potentially stale caller snapshot.
        // DTO-based replacements do not carry YAML extensions.
        let content = fs::read_to_string(&file_path)?;
        let old_task = crate::storage::task::parse_task_yaml_tolerant(&content)
            .ok_or("Cannot safely edit malformed task YAML; repair the task file first")?;
        let mut task = new_task.clone();
        let mut extras = old_task.extra_fields;
        extras.extend(task.extra_fields);
        task.extra_fields = extras;
        let file_string = serde_yaml_ng::to_string(&task)?;
        Ok((file_path, file_string))
    }

    /// Delete a task
    ///
    /// Like [`Self::edit`], deletion is locked to the issuing tasks root and
    /// refuses cross-root or ambiguous targets before any side effect.
    pub fn delete(
        root_path: &Path,
        id: &str,
        project: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if let Err(e) = validate_project_prefix(project) {
            return Err(e.into());
        }

        // SECURITY: central project-isolation guard. The explicit project
        // context must equal the ID's canonical prefix; the numeric suffix
        // alone must never be re-targeted at a different project's folder
        // (delete of DEV-5 with project=TP must not touch TP/5.yml).
        let parsed = match TaskId::parse(id) {
            Ok(parsed) => parsed,
            Err(err) => return Err(format!("Invalid task ID '{id}': {err}").into()),
        };
        if parsed.project != project {
            return Err(format!(
                "Task ID '{id}' belongs to project '{}', not '{project}'; refusing to cross projects",
                parsed.project
            )
            .into());
        }

        Self::refuse_cross_root(root_path, id)?;

        let project_path = root_path.join(project);

        with_storage_lock(&project_path, "task", || {
            // Use filesystem-based file path resolution
            let file_path = match Self::get_file_path_for_id(&project_path, id) {
                Some(path) => path,
                None => return Ok(false), // Task file not found
            };

            match fs::remove_file(file_path) {
                Ok(_) => {
                    // No longer need to update index - simplified architecture
                    Ok(true)
                }
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::NotFound {
                        Ok(false)
                    } else {
                        Err(err.into())
                    }
                }
            }
        })
    }

    /// Get file path for a task
    pub fn get_file_path(project_folder: &str, numeric_id: u64, root_path: &Path) -> PathBuf {
        let mut file_path = root_path.to_path_buf();
        file_path.push(project_folder);
        file_path.push(format!("{}.yml", numeric_id));
        file_path
    }

    /// Get the file path for a task ID (relative to tasks root).
    /// Uses the canonical parse: the FINAL dash-separated segment is the
    /// numeric suffix, so hyphenated prefixes (`ABC-OPS-12`) resolve to
    /// `ABC-OPS/12.yml`, and padded aliases (`TP-001`) resolve to `TP/1.yml`.
    pub fn get_file_path_for_id(project_path: &Path, task_id: &str) -> Option<PathBuf> {
        let id = TaskId::parse(task_id).ok()?;
        let file_path = project_path.join(format!("{}.yml", id.number));
        if file_path.exists() {
            Some(file_path)
        } else {
            None
        }
    }

    /// Get the current highest task ID by scanning the project directory
    pub fn get_current_id(project_path: &Path) -> u64 {
        crate::utils::filesystem::list_files_with_ext(project_path, "yml")
            .into_iter()
            .filter_map(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .max()
            .unwrap_or(0)
    }

    /// Get the actual project folder name for a given task ID, from the
    /// canonical parse (e.g., "ABC-OPS-12" -> "ABC-OPS", "STAT-001" -> "STAT").
    pub fn get_project_for_task(task_id: &str) -> Option<String> {
        TaskId::parse(task_id).ok().map(|id| id.project)
    }

    /// SECURITY: every id-bearing read/write helper that accepts an explicit
    /// project context enforces that it equals the canonical ID prefix
    /// (see [`Self::get`], [`Self::get_in_root`], [`Self::delete`]). Helpers
    /// never silently substitute the provided project for the ID's own.
    ///
    /// Fail closed when `id` resolves to a task stored outside `root_path`,
    /// or to more than one storage location. Mutations must not write through
    /// a root whose locks and transaction journal they do not hold.
    fn refuse_cross_root(root_path: &Path, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let parsed = match TaskId::parse(id) {
            Ok(parsed) => parsed,
            Err(_) => return Ok(()), // callers report invalid IDs themselves
        };
        let locations = parsed.locate(root_path);
        match locations.as_slice() {
            [] => Ok(()), // missing files surface as not-found downstream
            [single] if single.is_in_root(root_path) => Ok(()),
            [single] => Err(format!(
                "Task '{}' is stored in workspace tasks root {} and cannot be modified from {}; run the command inside that workspace",
                single.full_id(),
                single.root.display(),
                root_path.display()
            )
            .into()),
            many => Err(format!(
                "Task ID '{}' matches multiple storage locations ({}); refusing to modify an arbitrary task",
                id,
                many.iter()
                    .map(|location| location.file.display().to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
            .into()),
        }
    }

    fn is_safe_folder_name(name: &str) -> bool {
        crate::storage::safety::is_valid_project_prefix(name)
            || name.eq_ignore_ascii_case("default")
    }

    /// Get or create a project prefix, ensuring it's unique and consistent
    #[cfg(test)]
    pub fn get_or_create_project_prefix(
        root_path: &Path,
        project_name: &str,
    ) -> Result<String, String> {
        // Check if we already have a folder for this exact project name
        let direct_path = root_path.join(project_name);
        if direct_path.exists() && direct_path.is_dir() {
            return Ok(project_name.to_string());
        }

        // Generate the expected prefix for this project name
        let expected_prefix = generate_project_prefix(project_name);

        // Check if a folder with this prefix already exists
        let prefix_path = root_path.join(&expected_prefix);
        if prefix_path.exists() && prefix_path.is_dir() {
            // Verify this is for the same project by checking config
            let config_path = crate::utils::paths::project_config_path(root_path, &expected_prefix);
            if config_path.exists()
                && let Ok(content) = fs::read_to_string(&config_path)
                && let Ok(config) =
                    serde_yaml_ng::from_str::<crate::config::types::ProjectConfig>(&content)
            {
                // Check if the project name in config matches (either exact or prefix)
                if config.project_name == project_name || config.project_name == expected_prefix {
                    return Ok(expected_prefix);
                }
            }
        }

        // If no existing folder found, generate a new unique prefix
        Self::generate_unique_folder_prefix(root_path, project_name)
    }

    /// Generate a unique folder name (prefix) for a project
    #[cfg(test)]
    pub fn generate_unique_folder_prefix(
        root_path: &Path,
        project_name: &str,
    ) -> Result<String, String> {
        // Generate candidate prefix using the shared utility
        let candidate = generate_project_prefix(project_name);

        // Check if this folder name is available
        if !root_path.join(&candidate).exists() {
            return Ok(candidate);
        }

        // If collision, try variations with numbers
        for i in 1..=99 {
            let candidate_with_number = if candidate.len() >= 4 {
                format!("{}{:02}", &candidate[..2], i)
            } else {
                format!("{}{}", candidate, i)
            };

            if !root_path.join(&candidate_with_number).exists() {
                return Ok(candidate_with_number);
            }
        }

        Err(format!(
            "Could not generate unique prefix for project '{}'",
            project_name
        ))
    }
}
