# CLI Internals

LoTaR’s CLI is defined with Clap (see `crates/lotar-lib/src/cli/mod.rs`) and shares the same services as the REST and MCP layers. This file maps the high-level commands to the modules, services, and tests that keep them running. For user instructions, read `docs/help/*.md`; developers should live here.

## Quick links

- **Core flow:** [Execution pipeline](#execution-pipeline), [Command reference](#command-reference)
- **Creation & search:** [lotar add](#lotar-add), [lotar list](#lotar-list)
- **State + metadata:** [lotar status](#lotar-status), [lotar priority](#lotar-priority), [lotar assignee](#lotar-assignee), [lotar due-date](#lotar-due-date), [lotar effort](#lotar-effort), [lotar comment](#lotar-comment), [lotar task history](#lotar-task-history)
- **Planning & analytics:** [lotar changelog](#lotar-changelog), [lotar config](#lotar-config), [lotar stats](#lotar-stats), [lotar sprint](#lotar-sprint)
- **Tooling & integrations:** [lotar scan](#lotar-scan), [lotar serve](#lotar-serve), [lotar git](#lotar-git), [lotar completions](#lotar-completions), [lotar task](#lotar-task), [lotar whoami](#lotar-whoami), [lotar mcp](#lotar-mcp)

## Execution pipeline

1. **Argument parsing** – `cli::base_command()` builds the Clap graph. Shared global flags are normalized via `cli::preprocess` before individual handlers run.
2. **Project + config resolution** – `cli::project::ProjectResolver` pulls `.tasks` paths from `workspace::TasksDirectoryResolver` and merges config precedence (`precedence.md`).
3. **Validation & identity** – `cli::validation::CliValidator` enforces schema rules; `utils::identity` resolves `@me`, while `utils::task_intel` infers type/priority/status from branches when enabled.
4. **Handlers** – Each command implements `CommandHandler` (`cli/handlers/mod.rs`). Handlers call into `services::*` and emit results through `output::OutputRenderer`.
5. **Storage & events** – `services::task_service` (and friends) read/write YAML using `storage::manager::Storage`. Every mutation may emit SSE/MCP events via `api_events`.

## Command reference

Each section links to relevant source files and tests. Additions should follow the same pattern.

### lotar add

- **User help:** [../help/add.md](../help/add.md)
- **Args & handler:** `TaskAddArgs` in `crates/lotar-lib/src/cli/args/task.rs`; `AddHandler` in `crates/lotar-lib/src/cli/handlers/mod.rs`.
- **Core services:** `services::task_service::TaskService`, `CliValidator`, `utils::task_intel` (branch inference), `ProjectResolver` (context), `utils::project::generate_project_prefix` (prefix fallbacks).
- **Key behaviors:** branch/auto tags, CODEOWNERS-aware auto-assign, config-driven defaults, dry-run + explain traces rendered via `output::renderers`.
- **Tests:** `crates/lotar-lib/tests/cli_add_defaults_unit_test.rs`, `crates/lotar-lib/tests/cli_commands_integration_test.rs`, `crates/lotar-lib/tests/auto_branch_infer_type_toggle_test.rs`, `crates/lotar-lib/tests/auto_tags_from_path_toggle_test.rs`.

### lotar list

- **User help:** [../help/list.md](../help/list.md)
- **Args & handler:** `TaskSearchArgs` in `crates/lotar-lib/src/cli/args/task.rs`; `SearchHandler` and `TaskPostFilters` in `crates/lotar-lib/src/cli/handlers/task/search.rs`.
- **Core services:** `storage::manager::Storage` for reads and text/custom-field filtering, the post-filter pipeline in `search.rs`, and the shared `services::task_query` sorting executor.
- **Features:** alias `ls`, multi-project resolution, the web search syntax as the positional query (`crates/lotar-lib/src/utils/search_syntax.rs`, checked against `crates/lotar-lib/tests/fixtures/search_syntax_cases.json` together with the web parser), value flags that add to it, the shared task query (`crates/lotar-lib/src/services/task_list.rs`), limit/sort combination, text and JSON output. `table`/`markdown`/`md` are text aliases; malformed terms fail before searching.
- **Tests:** `crates/lotar-lib/tests/cli_list_alias_test.rs`, `crates/lotar-lib/tests/cli_list_effort_filters_and_sort_test.rs`, `crates/lotar-lib/tests/cli_list_pagination_test.rs`, `crates/lotar-lib/tests/dev75_cli_list_contract_test.rs`.

### lotar status

- **User help:** [../help/status.md](../help/status.md)
- **Implementation:** `StatusArgs` in `crates/lotar-lib/src/cli/args/task.rs`; handler in `crates/lotar-lib/src/cli/handlers/status.rs` which reuses `TaskService::update_status` and CODEOWNERS auto-assignment hooks in `utils::codeowners.rs`.
- **Highlights:** first-change auto-assign, dry-run/explain output, relationship-aware transitions.
- **Tests:** `crates/lotar-lib/tests/cli_status_pattern_test.rs`, `crates/lotar-lib/tests/cli_task_dryrun_test.rs`, `crates/lotar-lib/tests/cli_task_relationships_test.rs`.

### lotar priority

- **User help:** [../help/priority.md](../help/priority.md)
- **Implementation:** `PriorityArgs` + `PriorityHandler` (`crates/lotar-lib/src/cli/handlers/priority.rs`). Validation piggybacks on `CliValidator::validate_priority`.
- **Notes:** Accepts explicit configured values only. JSON renderer mirrors `render_status_change` for consistency.
- **Tests:** `crates/lotar-lib/tests/cli_task_dryrun_test.rs`, `crates/lotar-lib/tests/cli_task_relationships_test.rs` (shared harness ensures preview accuracy).

### lotar assignee

- **User help:** [../help/assignee.md](../help/assignee.md)
- **Implementation:** `AssigneeArgs` + `AssigneeHandler` (`crates/lotar-lib/src/cli/handlers/assignee.rs`). Accepts `@me`, integrates with members enforcement from config (`strict_members`).
- **Services:** `TaskService::assign` updates YAML and manipulates members via `services::project_service` when auto-population is enabled.
- **Tests:** `crates/lotar-lib/tests/cli_task_relationships_test.rs`, `crates/lotar-lib/tests/cli_commands_integration_test.rs` (assignment flows), `crates/lotar-lib/tests/me_alias_behavior_test.rs`.

### lotar due-date

- **User help:** [../help/due-date.md](../help/due-date.md)
- **Implementation:** `DueDateArgs` + `DueDateHandler` (`crates/lotar-lib/src/cli/handlers/duedate.rs`) rely on `CliValidator::parse_due_date` for relative tokens.
- **Notes:** dry-run/explain share the same payloads as the write path; CLI strips timezone noise before persisting.
- **Tests:** `crates/lotar-lib/tests/due_date_new_formats_test.rs`, `crates/lotar-lib/tests/due_date_and_sorting_unit_test.rs`.

### lotar effort

- **User help:** [../help/effort.md](../help/effort.md)
- **Implementation:** `EffortArgs` + `EffortHandler` (`crates/lotar-lib/src/cli/handlers/effort.rs`) using `utils::effort::parse_effort` to normalize time/point inputs.
- **Features:** `--clear`, dry-run previews, JSON diff output for scripts.
- **Tests:** `crates/lotar-lib/tests/effort_util_test.rs`, `crates/lotar-lib/tests/effort_normalization_on_write_test.rs`.

### lotar comment

- **User help:** [../help/comment.md](../help/comment.md)
- **Implementation:** `CommentArgs` + `CommentHandler` (`crates/lotar-lib/src/cli/handlers/comment.rs`). Supports stdin, `-m`, or `--file` content sources and powers the `lotar task comment` alias.
- **Services:** `TaskService::add_comment_with_context` appends a `TaskComment` with its author and an optional hand-off `report` (`CommentReportArgs` parses `--commit`/`--check`/`--next`; `normalize_report` checks it for every surface).
- **Tests:** `crates/lotar-lib/tests/comment_command_test.rs`, `crates/lotar-lib/tests/comment_shortcut_test.rs`, `crates/lotar-lib/tests/comment_task_parity_test.rs`, `crates/lotar-lib/tests/dev490_report_comment_test.rs`.

### lotar task history

- **User help:** [../help/history.md](../help/history.md)
- **Implementation:** Subcommands wired through `TaskHandler` (`crates/lotar-lib/src/cli/handlers/task`) and implemented in `task/history.rs`. Git plumbing lives in `utils::git`.
- **Highlights:** structured JSON diffs, `history-field` alias for targeted property timelines, and the `diff`/`at` helpers exposed under the same namespace.
- **Tests:** `crates/lotar-lib/tests/task_history_git_test.rs`, `crates/lotar-lib/tests/task_diff_structured_unit_test.rs`, `crates/lotar-lib/tests/task_diff_fields_test.rs`.

### lotar changelog

- **User help:** [../help/changelog.md](../help/changelog.md)
- **Implementation:** Directly in `src/main.rs` under the `Commands::Changelog` arm. Reads git history for `.tasks/**` and emits delta feeds via `output::OutputRenderer`.
- **Tests:** `crates/lotar-lib/tests/changelog_integration_test.rs`, `crates/lotar-lib/tests/audit_last_change_test.rs`.

### lotar config

- **User help:** [../help/config.md](../help/config.md) & [../help/templates.md](../help/templates.md)
- **Implementation:** Args in `crates/lotar-lib/src/cli/args/config.rs`; handlers under `crates/lotar-lib/src/cli/handlers/config/{show,set,validate,normalize}.rs` and in the `config/init/` directory, plus the renderer utilities in `render.rs`.
- **Services:** `services::config_service` for persistence + schema validation; template sources under `crates/lotar-lib/src/config/templates/`.
- **Tests:** `crates/lotar-lib/tests/config_validation_test.rs`, `crates/lotar-lib/tests/config_integration_test.rs`, `crates/lotar-lib/tests/config_service_branch_alias_test.rs`, `crates/lotar-lib/tests/config_set_categories_tags_test.rs`, `crates/lotar-lib/tests/config_explain_test.rs`.

### lotar templates

- Alias for `lotar config templates`. Shares the same handler; we keep this heading for backwards compatibility with older links.

### lotar stats

- **User help:** [../help/stats.md](../help/stats.md)
- **Implementation:** `StatsArgs` in `crates/lotar-lib/src/cli/args/stats.rs`; handlers under `crates/lotar-lib/src/cli/handlers/stats/` (per-metric modules such as `age`, `authors`, `effort`, `status`); burndown, calendar, and velocity views live with sprint reporting in `crates/lotar-lib/src/cli/handlers/sprint/reporting/`.
- **Data flow:** Stats aggregate via `services::sprint_analytics`, `services::sprint_metrics`, and direct reads from `storage::task`.
- **Tests:** `crates/lotar-lib/tests/stats_snapshot_test.rs`, `crates/lotar-lib/tests/stats_git_test.rs`, `crates/lotar-lib/tests/cli_sprint_velocity_test.rs` (shared helpers).

### lotar sprint

- **User help:** [../help/sprints.md](../help/sprints.md)
- **Implementation:** Args in `crates/lotar-lib/src/cli/args/sprint.rs`; handler tree in `crates/lotar-lib/src/cli/handlers/sprint/{operations,assignment,reporting}`.
- **Services:** `services::sprint_service`, `sprint_assignment`, `sprint_integrity`, `sprint_reports`.
- **Tests:** `crates/lotar-lib/tests/cli_sprint_basic_test.rs`, `crates/lotar-lib/tests/cli_sprint_burndown_test.rs`, `crates/lotar-lib/tests/cli_sprint_calendar_test.rs`, `crates/lotar-lib/tests/sprint_status_unit_test.rs`, `crates/lotar-lib/tests/sprint_storage_test.rs`.

### lotar scan

- **User help:** [../help/scan.md](../help/scan.md)
- **Implementation:** Args in `crates/lotar-lib/src/cli/args/scan.rs`; handler in `crates/lotar-lib/src/cli/handlers/scan_handler.rs`; heavy lifting in `crates/lotar-lib/src/scanner.rs` (walkdir + attribute parsing).
- **Features:** include/exclude filters, inline metadata parsing, re-anchoring, dry-run/detailed context.
- **Tests:** `crates/lotar-lib/tests/scanner_integration_test.rs`, `crates/lotar-lib/tests/scan_bidir_references_test.rs`.

### lotar serve

- **User help:** [../help/serve.md](../help/serve.md)
- **Implementation:** Args in `crates/lotar-lib/src/cli/args/serve.rs`; handler in `crates/lotar-lib/src/cli/handlers/serve_handler.rs` launches `web_server::start_app`.
- **Server stack:** Axum router in `crates/lotar-lib/src/routes/`, SSE wiring in `crates/lotar-lib/src/web_server.rs`, static assets from `target/web` (Vite build). Watches `.tasks/**` to broadcast `project_changed` events.
- **Tests:** `crates/lotar-lib/tests/cli_serve_features_test.rs`, `crates/lotar-lib/tests/cli_serve_port_parsing_test.rs`, smoke suites under `smoke/tests/ui.*` and `smoke/tests/api.*`.

### lotar git

- **User help:** [../help/git.md](../help/git.md)
- **Implementation:** Args in `crates/lotar-lib/src/cli/args/git.rs`; handler `crates/lotar-lib/src/cli/handlers/git.rs` installs hooks under `.git/hooks` and manages idempotency.
- **Notes:** Validates repo roots, writes hook scripts with shebang detection, supports `--force`/`--dry-run`.
- **Tests:** `crates/lotar-lib/tests/git_hooks_install_test.rs`, `smoke/tests/cli.git-hooks.smoke.spec.ts`.

### lotar completions

- **User help:** [../help/completions.md](../help/completions.md)
- **Implementation:** `CompletionsHandler` in `crates/lotar-lib/src/cli/handlers/completions.rs` generates Clap completions or writes them to known shell paths.
- **Tests:** `crates/lotar-lib/tests/cli_commands_integration_test.rs` (covers listing/generation) plus targeted expectations in `crates/lotar-lib/tests/output_format_consistency_test.rs`.

### lotar task

- **User help:** [../help/task.md](../help/task.md)
- **Implementation:** `TaskHandler` in `crates/lotar-lib/src/cli/handlers/task/mod.rs` routes to the modern handlers for mutation/list/history operations. Maintains backwards-compatible argument shapes.
- **Notes:** `task list` takes the same arguments as `lotar list` and runs the shared task query.

### lotar whoami

- **User help:** [../help/whoami.md](../help/whoami.md)
- **Implementation:** Wired directly in `src/main.rs`; reuses `utils::identity::resolve_current_identity` and the explain helpers in `utils::identity_detectors.rs`.
- **Outputs:** Text and JSON share the same structure; `--explain` surfaces sources and toggle states.
- **Tests:** `crates/lotar-lib/tests/cli_whoami_and_dryrun_test.rs`, `crates/lotar-lib/tests/me_alias_behavior_test.rs`.

### lotar mcp

- **User help:** [../help/mcp.md](../help/mcp.md)
- **Implementation:** CLI arm spawns the JSON-RPC stdio server defined in `crates/lotar-lib/src/mcp/server.rs`. Tool schemas come from `mcp/server/tools.rs` (see also `mcp-tools.md`).
- **Notes:** Shares the same project/config resolution and services as the CLI. Automatic restarts are handled via `LOTAR_MCP_AUTORELOAD` watchers.
- **Tests:** `crates/lotar-lib/tests/mcp_server_unit_test.rs`, `smoke/tests/mcp.*.smoke.spec.ts`.

---

For integrations beyond the CLI (web, SSE, MCP tools, REST), continue with the docs listed in `index.md`. When adding a new command, create a matching section here and update any legacy stub (e.g., `docs/developers/foo.md`) to point at the new anchor.
