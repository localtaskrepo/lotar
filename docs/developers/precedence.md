# Resolution & Precedence

This page explains how LoTaR resolves values for configuration, identity (for @me), and common paths, with a consistent, predictable order.

## Configuration precedence

`src/config/resolution.rs` and `src/config/manager.rs` merge configuration layers in a fixed order (highest wins):
1. Command-line flags for the current invocation (e.g., `lotar config set --project`, `--tasks-dir`, `--format`). These are evaluated inside each command handler and never persisted.
2. Environment overrides defined in `src/config/env_overrides.rs` (see `docs/help/environment.md` for the full table). The first set variable wins per key and is applied globally before any project overlays.
3. Home config (`~/.lotar` or `%APPDATA%/lotar/config.yml`). Read via `ConfigManager::load_home_config*` and allowed to override both global and project scopes.
4. Project config (`.tasks/<PROJECT>/config.yml`). Loaded via `ConfigManager::get_project_config` and applied after global defaults but before home/env so smart settings such as `issue_states`, `issue_priorities`, `default_status`, branch aliases, and scan toggles can remain project-specific.
5. Global config (`.tasks/config.yml` in the resolved workspace). Provides the shared baseline for every project.
6. Built-in defaults from `src/config/types.rs`.

Notes:
- The same chain powers CLI, REST, and MCP. Project-aware commands always call `ProjectResolver::resolve_project` and then `ConfigManager::get_project_config`, so they inherit project-level overrides while still respecting user/home/env tweaks.
- Automation toggles (`auto.set_reporter`, `auto.assign_on_status`, `auto.identity`, `auto.identity_git`, `auto.branch_infer_*`, etc.) default to true when unspecified (see `src/config/types.rs`) and honor the same precedence chain.
- When a field cannot be expressed per-project (for example `tasks_folder`), only the global/home/env layers are considered.

## Identity resolution and @me

Anywhere a person field is accepted (assignee, reporter, default_reporter), the special value @me is allowed. `src/utils/identity_detectors.rs` runs detectors in this order:
1) Merged config `default_reporter` (using the precedence above). `LOTAR_DEFAULT_REPORTER` feeds this via the env overrides table.
2) Project manifest author (package.json `author`/`contributors`, Cargo.toml `authors`, or the first `.csproj` `<Authors>` tag) searched from repo root downward.
3) Git config (`user.name`, then `user.email`) at the repository root, gated by `auto.identity_git`.
4) System user from `$USER` / `$USERNAME`.

`src/utils/identity.rs` caches these results per workspace so CLI/REST/MCP share the same answer. Set `auto.identity=false` to restrict @me lookups to the configured reporter only; set `auto.identity_git=false` to skip git-based fallbacks while still honoring manifests and env values. `lotar whoami --explain` surfaces the same order along with detector metadata.

## Tasks directory resolution

`src/workspace.rs` (`TasksDirectoryResolver`) discovers the workspace root using these steps:
1. `--tasks-dir <PATH>` (highest priority). The directory is created automatically when missing so initialization commands can run in fresh folders.
2. `LOTAR_TASKS_DIR` environment variable (skipped when `LOTAR_TEST_MODE=1`, `RUST_TEST_THREADS` is set, or `LOTAR_IGNORE_ENV_TASKS_DIR=1`). Relative single-segment values first try to match parent directories before falling back to creating the path.
3. Home config `tasks_folder` setting (in `~/.lotar` unless overridden by `LOTAR_IGNORE_HOME_CONFIG=1`).
4. Global config `tasks_folder` setting (allows repos to standardize on a folder such as `.work` instead of `.tasks`).
5. Parent directory search for an initialized tasks folder (matching the configured name and containing `config.yml`). The resolver returns the parent path so status output can explain where it was found.
6. Current directory + configured folder name (created on demand).

These rules apply everywhere (CLI, REST server, MCP). Run commands with `LOTAR_DEBUG=1` to emit the resolved workspace path whenever you need to confirm which folder was selected.

## Project resolution (short guide)

Project context is determined by `src/cli/project.rs`/`ProjectResolver`:
- Explicit `--project` flag (highest). Accepts either a prefix or the full project name; both are normalized via `resolve_project_input`.
- Task ID prefix (e.g., AUTH-123 → AUTH) when present.
- Auto-detection from the current `.tasks/<PREFIX>` directory name when the repo has a single initialized project.
- Default project (`default_prefix`) from merged configuration. `ConfigManager::ensure_default_prefix` will generate one from the repo name when no project yet exists.

## Automation semantics

- `auto.set_reporter`: When true, reporter is auto-populated during `TaskService::create` whenever an explicit value is absent. The service first honors `default_reporter`, then falls back to the identity detectors above.
- `auto.assign_on_status`: When true, the first time a task moves away from the default/first status and has no assignee, it is set to the resolved current user. The logic lives inside `TaskService::update` and is also exposed through the `lotar status` CLI handler.

Both settings honor the configuration precedence chain, and they can be toggled globally, per-project, or at runtime through `LOTAR_AUTO_SET_REPORTER` / `LOTAR_AUTO_ASSIGN_ON_STATUS`.

## Tips

- Use `--explain` (where available) to see how values were chosen. Commands such as `lotar whoami --explain` and `lotar config show --explain` surface each layer in the order above.
- Use `lotar whoami` to see your resolved identity and source chain.
- Enable `LOTAR_DEBUG=1` temporarily to log the resolved tasks directory, config sources, and identity detector outcomes when diagnosing precedence issues.
