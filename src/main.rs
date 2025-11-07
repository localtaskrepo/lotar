// Allow uninlined format args since it's mostly a style preference
#![allow(clippy::uninlined_format_args)]

use clap::Parser;
use std::env;
use std::str::FromStr;

use lotar::cli::handlers::assignee::{AssigneeArgs, AssigneeHandler};
use lotar::cli::handlers::comment::{CommentArgs, CommentHandler};
use lotar::cli::handlers::duedate::{DueDateArgs, DueDateHandler};
use lotar::cli::handlers::priority::{PriorityArgs, PriorityHandler};
use lotar::cli::handlers::status::{StatusArgs, StatusHandler};
use lotar::cli::handlers::{
    AddHandler, CommandHandler, ConfigHandler, GitHandler, ScanHandler, ServeHandler,
    SprintHandler, StatsHandler, TaskHandler,
};
use lotar::cli::preprocess::normalize_args;
use lotar::cli::{Cli, Commands, TaskAction};
use lotar::utils::resolve_project_input;
use lotar::workspace::TasksDirectoryResolver;
use lotar::{help, output};

/// Resolve the tasks directory based on config and command line arguments
fn resolve_tasks_directory_with_override(
    override_path: Option<String>,
) -> Result<TasksDirectoryResolver, String> {
    TasksDirectoryResolver::resolve(
        override_path.as_deref(),
        None, // Use default .tasks folder name
    )
}

/// Check if a string is a valid command name
fn is_valid_command(command: &str) -> bool {
    matches!(
        command,
        "add"
            | "list"
            | "ls"
            | "status"
            | "priority"
            | "assignee"
            | "due-date"
            | "effort"
            | "comment"
            | "task"
            | "tasks"
            | "config"
            | "scan"
            | "serve"
            | "whoami"
            | "stats"
            | "sprint"
            | "changelog"
            | "mcp"
            | "git"
    )
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // No arguments: print usage/help to stderr and exit with failure (tests expect "Usage")
    if args.len() == 1 {
        let renderer =
            output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
        renderer.emit_raw_stderr(
            "Usage: lotar <COMMAND> [ARGS]\nTry 'lotar help' for available commands.",
        );
        std::process::exit(1);
    }

    // Handle version manually
    for arg in &args[1..] {
        if arg == "version" || arg == "--version" || arg == "-V" {
            // version is user-facing
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_raw_stdout(&format!("lotar {}", env!("CARGO_PKG_VERSION")));
            return;
        }
    }
    // Handle subcommand help (e.g., "lotar help add")
    if args.len() >= 3 && args[1] == "help" {
        let command = &args[2];
        show_command_help(command);
        return;
    }

    // Check for help flags and determine context
    let has_help_flag = args[1..]
        .iter()
        .any(|arg| arg == "help" || arg == "--help" || arg == "-h");

    if has_help_flag {
        // If we have a valid command as first argument and a help flag anywhere, show command help
        if args.len() >= 2 && is_valid_command(&args[1]) && args[1] != "help" {
            show_command_help(&args[1]);
            return;
        } else {
            // Otherwise show global help
            show_enhanced_help();
            return;
        }
    }

    // Normalize arguments (serve command uses additional syntactic sugar)
    let normalized_args = match normalize_args(&args) {
        Ok(v) => v,
        Err(message) => {
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_error(&message);
            std::process::exit(1);
        }
    };

    // Parse with Clap
    let cli = match Cli::try_parse_from(normalized_args) {
        Ok(cli) => cli,
        Err(e) => {
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_raw_stderr(&e.to_string());
            std::process::exit(1);
        }
    };

    // Resolve tasks directory
    let resolver = match resolve_tasks_directory_with_override(cli.tasks_dir.clone()) {
        Ok(resolver) => resolver,
        Err(error) => {
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_error(&format!("Error resolving tasks directory: {}", error));
            std::process::exit(1);
        }
    };

    // Create output renderer
    let effective_level = if cli.verbose {
        output::LogLevel::Info
    } else {
        cli.log_level
    };
    let renderer = output::OutputRenderer::new(cli.format, effective_level);

    // Execute the command
    let result = match cli.command {
        Commands::Add(args) => {
            renderer.log_info("BEGIN ADD");
            match AddHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(task_id) => {
                    // Use the shared output rendering function
                    if task_id.ends_with("-PREVIEW") {
                        // Dry run preview already printed
                    } else {
                        AddHandler::render_add_success(
                            &task_id,
                            cli.project.as_deref(),
                            &resolver,
                            &renderer,
                        );
                    }
                    renderer.log_info("END ADD status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END ADD status=err");
                    Err(e)
                }
            }
        }
        Commands::List(args) => {
            renderer.log_info("BEGIN LIST");
            let task_action = TaskAction::List(args);
            match TaskHandler::execute(task_action, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END LIST status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END LIST status=err");
                    Err(e)
                }
            }
        }
        Commands::Status {
            id,
            status,
            dry_run,
            explain,
        } => {
            renderer.log_info("BEGIN STATUS");
            let mut status_args = StatusArgs::new(id, status, cli.project.clone());
            status_args.dry_run = dry_run;
            status_args.explain = explain;
            match StatusHandler::execute(status_args, cli.project.as_deref(), &resolver, &renderer)
            {
                Ok(()) => {
                    renderer.log_info("END STATUS status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END STATUS status=err");
                    Err(e)
                }
            }
        }
        Commands::Priority { id, priority } => {
            renderer.log_info("BEGIN PRIORITY");
            let priority_args = PriorityArgs::new(id, priority, cli.project.clone());
            match PriorityHandler::execute(
                priority_args,
                cli.project.as_deref(),
                &resolver,
                &renderer,
            ) {
                Ok(()) => {
                    renderer.log_info("END PRIORITY status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END PRIORITY status=err");
                    Err(e)
                }
            }
        }
        Commands::DueDate { id, due_date } => {
            renderer.log_info("BEGIN DUEDATE");
            let args = DueDateArgs {
                task_id: id,
                new_due_date: due_date,
            };
            match DueDateHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END DUEDATE status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END DUEDATE status=err");
                    Err(e)
                }
            }
        }
        Commands::Effort {
            id,
            effort,
            clear,
            dry_run,
            explain,
        } => {
            renderer.log_info("BEGIN EFFORT");
            let args = lotar::cli::handlers::effort::EffortArgs {
                task_id: id,
                new_effort: effort,
                clear,
                dry_run,
                explain,
            };
            match lotar::cli::handlers::effort::EffortHandler::execute(
                args,
                cli.project.as_deref(),
                &resolver,
                &renderer,
            ) {
                Ok(()) => {
                    renderer.log_info("END EFFORT status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END EFFORT status=err");
                    Err(e)
                }
            }
        }
        Commands::Assignee { id, assignee } => {
            renderer.log_info("BEGIN ASSIGNEE");
            let args = AssigneeArgs {
                task_id: id,
                new_assignee: assignee,
            };
            match AssigneeHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END ASSIGNEE status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END ASSIGNEE status=err");
                    Err(e)
                }
            }
        }
        Commands::Comment {
            id,
            text,
            message,
            file,
        } => {
            renderer.log_info("BEGIN COMMENT");
            // Resolve comment content from args: file > message > text > stdin
            let resolved_text = if let Some(path) = file {
                std::fs::read_to_string(&path)
                    .map(|s| s.trim_end_matches(['\n', '\r']).to_string())
                    .map_err(|e| e.to_string())
                    .unwrap_or_else(|_| {
                        renderer.emit_error("Failed to read --file");
                        String::new()
                    })
            } else if let Some(m) = message {
                m
            } else if let Some(t) = text {
                t
            } else {
                // Fallback: read from stdin if piped
                use std::io::{IsTerminal, Read};
                let mut buffer = String::new();
                if !std::io::stdin().is_terminal() {
                    if std::io::stdin().read_to_string(&mut buffer).is_ok() {
                        buffer.trim_end_matches(['\n', '\r']).to_string()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            };
            let args = CommentArgs {
                task_id: id,
                text: if resolved_text.trim().is_empty() {
                    None
                } else {
                    Some(resolved_text)
                },
            };
            match CommentHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END COMMENT status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END COMMENT status=err");
                    Err(e)
                }
            }
        }
        Commands::Task { action } => {
            renderer.log_info("BEGIN TASK");
            match TaskHandler::execute(action, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END TASK status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END TASK status=err");
                    Err(e)
                }
            }
        }
        Commands::Config { action } => {
            renderer.log_info("BEGIN CONFIG");
            match ConfigHandler::execute(action, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END CONFIG status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END CONFIG status=err");
                    Err(e)
                }
            }
        }
        Commands::Scan(args) => {
            renderer.log_info("BEGIN SCAN");
            match ScanHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END SCAN status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END SCAN status=err");
                    Err(e)
                }
            }
        }
        Commands::Serve(args) => {
            renderer.log_info("BEGIN SERVE");
            match ServeHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END SERVE status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END SERVE status=err");
                    Err(e)
                }
            }
        }
        Commands::Stats(args) => {
            renderer.log_info("BEGIN STATS");
            match StatsHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END STATS status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END STATS status=err");
                    Err(e)
                }
            }
        }
        Commands::Sprint(args) => {
            renderer.log_info("BEGIN SPRINT");
            match SprintHandler::execute(args, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END SPRINT status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END SPRINT status=err");
                    Err(e)
                }
            }
        }
        Commands::Changelog { since, global } => {
            renderer.log_info("BEGIN CHANGELOG");
            // Inline small implementation to avoid a new handler file
            // Determine repo root
            let cwd = std::env::current_dir().map_err(|e| e.to_string()).unwrap();
            let maybe_repo = lotar::utils::git::find_repo_root(&cwd);
            if let Some(repo_root) = maybe_repo {
                // Compute tasks relative path
                let tasks_abs = resolver.path.clone();
                let tasks_rel = if tasks_abs.starts_with(&repo_root) {
                    tasks_abs.strip_prefix(&repo_root).unwrap().to_path_buf()
                } else {
                    tasks_abs.clone()
                };
                // Resolve project scoping
                let project_filter: Option<String> = if global {
                    None
                } else {
                    Some(if let Some(p) = cli.project.as_deref() {
                        resolve_project_input(p, resolver.path.as_path())
                    } else {
                        lotar::project::get_effective_project_name(&resolver)
                    })
                };

                // Gather changed task files
                let changed_files: Vec<std::path::PathBuf> = if let Some(ref_base) = &since {
                    // Range: ref_base..HEAD
                    let mut cmd = std::process::Command::new("git");
                    cmd.arg("-C")
                        .arg(&repo_root)
                        .arg("diff")
                        .arg("--name-only")
                        .arg(format!("{}..HEAD", ref_base))
                        .arg("--")
                        .arg(&tasks_rel);
                    match cmd.output() {
                        Ok(o) if o.status.success() => {
                            let out_str = String::from_utf8_lossy(&o.stdout).to_string();
                            let base_iter = out_str
                                .lines()
                                .map(|l| std::path::PathBuf::from(l.trim()))
                                .filter(|p| !p.as_os_str().is_empty())
                                .filter(|p| p.starts_with(&tasks_rel));
                            let iter = if let Some(ref proj) = project_filter {
                                let expect = tasks_rel.join(proj);
                                Box::new(base_iter.filter(move |p| p.starts_with(&expect)))
                                    as Box<dyn Iterator<Item = std::path::PathBuf>>
                            } else {
                                Box::new(base_iter) as Box<dyn Iterator<Item = std::path::PathBuf>>
                            };
                            iter.collect()
                        }
                        _ => Vec::new(),
                    }
                } else {
                    // Working + staged vs HEAD, use porcelain to detect .tasks changes
                    use std::process::Command;
                    let out = Command::new("git")
                        .arg("-C")
                        .arg(&repo_root)
                        .arg("status")
                        .arg("--porcelain")
                        .output();
                    match out {
                        Ok(o) if o.status.success() => {
                            let mut files = Vec::new();
                            for line in String::from_utf8_lossy(&o.stdout).lines() {
                                if line.len() < 4 {
                                    continue;
                                }
                                let status = &line[..2];
                                if status.contains('R') {
                                    if let Some(pos) = line.find(" -> ") {
                                        let new_path = &line[pos + 4..];
                                        files.push(std::path::PathBuf::from(new_path.trim()));
                                    }
                                    continue;
                                }
                                let path = line[3..].trim();
                                if !path.is_empty() {
                                    files.push(std::path::PathBuf::from(path));
                                }
                            }
                            let iter = files.into_iter().filter(|p| p.starts_with(&tasks_rel));
                            let iter = if let Some(ref proj) = project_filter {
                                let expect = tasks_rel.join(proj);
                                Box::new(iter.filter(move |p| p.starts_with(&expect)))
                                    as Box<dyn Iterator<Item = std::path::PathBuf>>
                            } else {
                                Box::new(iter) as Box<dyn Iterator<Item = std::path::PathBuf>>
                            };
                            iter.collect()
                        }
                        _ => Vec::new(),
                    }
                };

                // For each changed file, compute field-level deltas
                #[derive(Clone, Debug)]
                struct BasicDelta {
                    field: String,
                    old: serde_json::Value,
                    new: serde_json::Value,
                }
                #[derive(Clone, Debug)]
                struct Item {
                    id: String,
                    project: String,
                    file: String,
                    changes: Vec<BasicDelta>,
                }

                let mut items: Vec<Item> = Vec::new();
                for rel_path in changed_files {
                    if rel_path.extension().and_then(|e| e.to_str()) != Some("yml") {
                        continue;
                    }
                    // Resolve ID from path .tasks/<PROJECT>/<NUM>.yml
                    let file_name = match rel_path.file_stem().and_then(|s| s.to_str()) {
                        Some(s) => s,
                        None => continue,
                    };
                    let numeric: u64 = match file_name.parse() {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    let project = match rel_path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                    {
                        Some(p) => p.to_string(),
                        None => continue,
                    };
                    let id = format!("{}-{}", project, numeric);

                    // Load snapshots with tolerant fallback for mixed-case enums in YAML
                    let load_yaml_as_task = |content: &str| -> Option<lotar::storage::task::Task> {
                        // First try strict parse
                        if let Ok(t) = serde_yaml::from_str::<lotar::storage::task::Task>(content) {
                            return Some(t);
                        }
                        // Fallback: tolerant parse via serde_yaml::Value and FromStr for enums
                        let v: serde_yaml::Value = serde_yaml::from_str(content).ok()?;
                        let get_str = |k: &str| -> Option<String> {
                            v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string())
                        };
                        let get_vec_str = |k: &str| -> Vec<String> {
                            v.get(k)
                                .and_then(|x| x.as_sequence())
                                .map(|seq| {
                                    seq.iter()
                                        .filter_map(|e| e.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default()
                        };

                        let title = get_str("title").unwrap_or_else(|| "".to_string());

                        let status = get_str("status")
                            .and_then(|s| lotar::types::TaskStatus::from_str(&s).ok())
                            .unwrap_or_default();
                        let priority = get_str("priority")
                            .and_then(|s| lotar::types::Priority::from_str(&s).ok())
                            .unwrap_or_default();
                        let task_type = get_str("task_type")
                            .and_then(|s| lotar::types::TaskType::from_str(&s).ok())
                            .unwrap_or_default();

                        let reporter = get_str("reporter");
                        let assignee = get_str("assignee");
                        let created = get_str("created")
                            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
                        let modified = get_str("modified")
                            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
                        let due_date = get_str("due_date");
                        let effort = get_str("effort");
                        let tags = get_vec_str("tags");

                        Some(lotar::storage::task::Task {
                            title,
                            status,
                            priority,
                            task_type,
                            reporter,
                            assignee,
                            created,
                            modified,
                            due_date,
                            effort,
                            acceptance_criteria: vec![],
                            relationships: lotar::types::TaskRelationships::default(),
                            comments: vec![],
                            references: vec![],
                            sprints: vec![],
                            history: vec![],
                            subtitle: None,
                            description: None,
                            tags,
                            custom_fields: std::collections::HashMap::new(),
                        })
                    };

                    // Current content (right side)
                    let current_content: Option<String> = if since.is_some() {
                        // Right = HEAD version
                        lotar::services::audit_service::AuditService::show_file_at(
                            &repo_root, "HEAD", &rel_path,
                        )
                        .ok()
                    } else {
                        // Working tree absolute path
                        let abs = repo_root.join(&rel_path);
                        std::fs::read_to_string(abs).ok()
                    };

                    // Base content (left side)
                    let base_content: Option<String> = if let Some(ref_base) = &since {
                        lotar::services::audit_service::AuditService::show_file_at(
                            &repo_root, ref_base, &rel_path,
                        )
                        .ok()
                    } else {
                        // HEAD version
                        lotar::services::audit_service::AuditService::show_file_at(
                            &repo_root, "HEAD", &rel_path,
                        )
                        .ok()
                    };

                    let cur_task = current_content.as_deref().and_then(load_yaml_as_task);
                    let base_task = base_content.as_deref().and_then(load_yaml_as_task);

                    // If both failed to parse, skip
                    if cur_task.is_none() && base_task.is_none() {
                        continue;
                    }

                    // Compute minimal field deltas using the same approach as task diff --fields
                    let mut deltas: Vec<BasicDelta> = Vec::new();
                    let mut push = |k: &str, old: serde_json::Value, new: serde_json::Value| {
                        if old != new {
                            deltas.push(BasicDelta {
                                field: k.to_string(),
                                old,
                                new,
                            });
                        }
                    };
                    match (cur_task.as_ref(), base_task.as_ref()) {
                        (Some(cur), Some(prev)) => {
                            push(
                                "title",
                                serde_json::json!(prev.title),
                                serde_json::json!(cur.title),
                            );
                            push(
                                "status",
                                serde_json::json!(prev.status.to_string()),
                                serde_json::json!(cur.status.to_string()),
                            );
                            push(
                                "priority",
                                serde_json::json!(prev.priority.to_string()),
                                serde_json::json!(cur.priority.to_string()),
                            );
                            push(
                                "task_type",
                                serde_json::json!(prev.task_type.to_string()),
                                serde_json::json!(cur.task_type.to_string()),
                            );
                            push(
                                "assignee",
                                serde_json::json!(prev.assignee),
                                serde_json::json!(cur.assignee),
                            );
                            push(
                                "reporter",
                                serde_json::json!(prev.reporter),
                                serde_json::json!(cur.reporter),
                            );
                            push(
                                "due_date",
                                serde_json::json!(prev.due_date),
                                serde_json::json!(cur.due_date),
                            );
                            push(
                                "effort",
                                serde_json::json!(prev.effort),
                                serde_json::json!(cur.effort),
                            );
                            push(
                                "tags",
                                serde_json::json!(prev.tags),
                                serde_json::json!(cur.tags),
                            );
                        }
                        (Some(cur), None) => {
                            // Created
                            push(
                                "created",
                                serde_json::Value::Null,
                                serde_json::json!(cur.title),
                            );
                        }
                        (None, Some(_prev)) => {
                            // Deleted
                            push("deleted", serde_json::json!(true), serde_json::Value::Null);
                        }
                        (None, None) => {}
                    }

                    if !deltas.is_empty() {
                        items.push(Item {
                            id,
                            project,
                            file: rel_path.to_string_lossy().to_string(),
                            changes: deltas,
                        });
                    }
                }

                // Sort stable by id for determinism
                items.sort_by(|a, b| a.id.cmp(&b.id));

                match renderer.format {
                    output::OutputFormat::Json => {
                        let items_json: Vec<_> = items
                            .iter()
                            .map(|it| {
                                serde_json::json!({
                                    "id": it.id,
                                    "project": it.project,
                                    "file": it.file,
                                    "changes": it.changes.iter().map(|d| serde_json::json!({
                                        "field": d.field,
                                        "old": d.old,
                                        "new": d.new,
                                    })).collect::<Vec<_>>()
                                })
                            })
                            .collect();
                        renderer.emit_raw_stdout(
                            &serde_json::json!({
                                "status": "ok",
                                "action": "changelog",
                                "mode": if since.is_some() { "range" } else { "working" },
                                "count": items_json.len(),
                                "items": items_json
                            })
                            .to_string(),
                        );
                    }
                    _ => {
                        if items.is_empty() {
                            renderer.emit_success("No task changes.");
                        } else {
                            for it in &items {
                                let mut parts: Vec<String> = Vec::new();
                                for d in &it.changes {
                                    // Compact representation for commit messages
                                    let s = match (&d.old, &d.new) {
                                        (serde_json::Value::Null, v) => {
                                            format!("{}: ∅ → {}", d.field, v)
                                        }
                                        (o, serde_json::Value::Null) => {
                                            format!("{}: {} → ∅", d.field, o)
                                        }
                                        (o, n) => format!("{}: {} → {}", d.field, o, n),
                                    };
                                    parts.push(s);
                                }
                                renderer.emit_raw_stdout(&format!(
                                    "{}  {}",
                                    it.id,
                                    parts.join("; ")
                                ));
                            }
                        }
                    }
                }
                renderer.log_info("END CHANGELOG status=ok");
                Ok(())
            } else {
                renderer.emit_warning("Not a git repository. No changelog.");
                renderer.log_info("END CHANGELOG status=ok");
                Ok(())
            }
        }
        Commands::Mcp => {
            lotar::mcp::server::run_stdio_server();
            Ok(())
        }
        Commands::Whoami { explain } => {
            renderer.log_info("BEGIN WHOAMI");
            // Try to resolve using detector framework (with explain)
            let det =
                lotar::utils::identity::resolve_current_user_explain(Some(resolver.path.as_path()));
            if let Some(info) = det {
                if matches!(renderer.format, output::OutputFormat::Json) {
                    if explain {
                        // Augment with toggle states
                        let cfg = lotar::config::resolution::load_and_merge_configs(Some(
                            resolver.path.as_path(),
                        ))
                        .ok();
                        renderer.emit_raw_stdout(
                            &serde_json::json!({
                                "user": info.user,
                                "source": info.source.to_string(),
                                "confidence": info.confidence,
                                "details": info.details,
                                "auto_identity": cfg.as_ref().map(|c| c.auto_identity),
                                "auto_identity_git": cfg.as_ref().map(|c| c.auto_identity_git)
                            })
                            .to_string(),
                        );
                    } else {
                        renderer.emit_raw_stdout(
                            &serde_json::json!({
                                "user": info.user
                            })
                            .to_string(),
                        );
                    }
                } else {
                    renderer.emit_success(&info.user);
                    if explain {
                        let mut msg =
                            format!("source: {}, confidence: {}", info.source, info.confidence);
                        if let Some(d) = info.details {
                            msg.push_str(&format!(", details: {}", d));
                        }
                        renderer.emit_info(&msg);
                        let cfg = lotar::config::resolution::load_and_merge_configs(Some(
                            resolver.path.as_path(),
                        ))
                        .ok();
                        if let Some(cfg) = cfg {
                            if !cfg.auto_identity {
                                renderer.emit_info(
                                    "Auto identity disabled; using configured default only",
                                );
                            } else if !cfg.auto_identity_git {
                                renderer.emit_info("Git identity auto-detection disabled");
                            } else {
                                renderer.emit_info("Resolution order: config.default_reporter → git user.name/email → system USER/USERNAME");
                            }
                        }
                    }
                }
                renderer.log_info("END WHOAMI status=ok");
                Ok(())
            } else {
                renderer.emit_error("Could not resolve current user");
                renderer.log_info("END WHOAMI status=err");
                Err("no-identity".to_string())
            }
        }
        Commands::Git { action } => {
            renderer.log_info("BEGIN GIT");
            match GitHandler::execute(action, cli.project.as_deref(), &resolver, &renderer) {
                Ok(()) => {
                    renderer.log_info("END GIT status=ok");
                    Ok(())
                }
                Err(e) => {
                    renderer.emit_error(&e);
                    renderer.log_info("END GIT status=err");
                    Err(e)
                }
            }
        }
    };

    if let Err(_error) = result {
        std::process::exit(1);
    }
}

/// Show enhanced help using our help system
fn show_enhanced_help() {
    let help_system = help::HelpSystem::new(output::OutputFormat::Text, false);
    match help_system.show_global_help() {
        Ok(help_text) => {
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_raw_stdout(&help_text);
        }
        Err(_) => {
            // Fall back to clap's help
            let _ = Cli::try_parse_from(["lotar", "--help"]);
        }
    }
}

/// Show command-specific help
fn show_command_help(command: &str) {
    let help_system = help::HelpSystem::new(output::OutputFormat::Text, false);
    match help_system.show_command_help(command) {
        Ok(help_text) => {
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_raw_stdout(&help_text);
        }
        Err(e) => {
            let renderer =
                output::OutputRenderer::new(output::OutputFormat::Text, output::LogLevel::Warn);
            renderer.emit_error(&format!("Error showing help for '{}': {}", command, e));
            renderer.emit_info("Try 'lotar help' for available commands.");
            std::process::exit(1);
        }
    }
}
