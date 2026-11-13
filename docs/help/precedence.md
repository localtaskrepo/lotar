# Resolution & Precedence

This page explains how LoTaR resolves values for configuration, identity (for @me), and common paths, with a consistent, predictable order.


## Configuration precedence

Configuration layers merge in a fixed order (highest wins):
1. Command-line flags for the current invocation (e.g., `lotar config set --project`, `--tasks-dir`, `--format`) plus the global `--config KEY=VALUE` overrides. These are evaluated inside each command handler, apply only to the current process, and are never persisted.
2. Environment overrides (see `docs/help/environment.md` for the full table). The first set variable wins per key. They describe the running process (an agent job, CI, a one-off shell), so they override the committed project config; a variable exported in a shell profile applies to every project.
3. Project config (`.tasks/<PROJECT>/config.yml`) when a project context is resolved. Overrides home and global config.
4. Home config (`~/.lotar`, on Windows `%USERPROFILE%\.lotar`). User-wide defaults that sit above global config but below env/project.
5. Global config (`.tasks/config.yml` in the resolved workspace). Provides the shared baseline for every project.
6. Built-in defaults.

Commands that do not operate on a specific project simply skip step 3, so they evaluate CLI → env → home → global → defaults.

Notes:
- The same chain powers CLI, REST, and MCP. Project-aware commands always resolve the project context first, so they inherit project-level overrides while env and CLI values still win. Use `--config KEY=VALUE` to temporarily override any field without editing YAML (existing per-command flags stay available as shorthands).
- Automation toggles (`auto.set_reporter`, `auto.assign_on_status`, `auto.identity`, `auto.identity_git`, `auto.branch_infer_*`, etc.) default to true when unspecified and honor the same precedence chain.
- Fields that cannot be set per project skip the project layer. `tasks_folder` is a special case: only the home config can set it (see [Tasks directory resolution](#tasks-directory-resolution)).

Automation rules (`automation.yml`) are resolved separately from config values:
1. Project automation file (`.tasks/<PROJECT>/automation.yml`)
2. Home automation file (`~/.lotar.automation.yml` or `~/.lotar/automation.yml` when the home config is a directory)
3. Global automation file (`.tasks/automation.yml`)
4. Built-in defaults

Rules do not merge across scopes; the first file found wins.

## Identity resolution and @me

Anywhere a person field is accepted (assignee, reporter, default_reporter), the special value @me is allowed. Detectors run in this order:
1) `default_reporter`, in the order every setting uses: CLI `--config default.reporter=NAME` → `LOTAR_DEFAULT_REPORTER` → selected project → home → global. This identity is a comment's author, a history entry's actor and the reporter of a new ticket without `--reporter`.
2) Git user (user.name or user.email at repo root) — gated by `auto.identity_git`
3) System user ($USER or $USERNAME)
4) Project manifest author (package.json author, Cargo.toml authors, or .csproj `<Authors>`) — last resort only, since manifest authors are a static guess that may not be the current user

Identity lookups are cached per workspace and effective project, so CLI/REST/MCP share the same answer for the same scope. Task operations use the task's project; unscoped lookups use configured `default.project` when present. Use `lotar whoami --project <NAME_OR_PREFIX> --explain` to inspect a specific project without creating one. Set `auto.identity=false` to disable the git/manifest detectors (config and system user remain); set `auto.identity_git=false` to skip git-based fallbacks while still honoring env and manifest values. Explain output includes project-resolved toggles and configuration-tier details.

## Tasks directory resolution

Tasks directory discovery follows these steps:
1. `--tasks-dir <PATH>` (highest priority). The directory is created automatically when missing so initialization commands can run in fresh folders.
2. `LOTAR_TASKS_DIR` environment variable (skipped when `LOTAR_TEST_MODE=1`, `RUST_TEST_THREADS` is set, or `LOTAR_IGNORE_ENV_TASKS_DIR=1`). Relative single-segment values first try to match parent directories before falling back to creating the path.
3. Home config `tasks_folder` setting (in `~/.lotar` unless overridden by `LOTAR_IGNORE_HOME_CONFIG=1`). This sets the folder name to look for, such as `.work` instead of `.tasks`. It is a personal setting; a repository cannot set it in `.tasks/config.yml`, because LoTaR reads that file only after it has found the folder.
4. Parent directory search for an initialized tasks folder (matching the configured name and containing `config.yml`). The resolver returns the parent path so status output can explain where it was found. The search stops at the root of the git repository the command runs in (a directory with `.git`), so a new or nested repository gets its own workspace, and a tasks folder directly in your home directory is only used when the command runs in the home directory itself.
5. Current directory + configured folder name (created on demand). Commands that only read (`list`, `show`, `stats`, `config show`, ...) never create it.

These rules apply everywhere (CLI, REST server, MCP). To see which folder was picked, run `lotar config show`: its first line is the tasks directory.

## Project resolution (short guide)

Project context is determined in this order:
- Explicit `--project` flag (highest). Accepts either a prefix or the full project name; both are normalized via `resolve_project_input`.
- Task ID prefix (e.g., AUTH-123 → AUTH) when present.
- Default project (`default.project`, or `LOTAR_PROJECT`) from merged configuration. Manifest names never override it.
- Without a default: the existing project (first alphabetically; a folder with a `config.yml` or task files), else a prefix from the nearest manifest, repo or folder name. `lotar add` warns before it creates a project this way; see [add](./add.md#project-resolution).

## Automation semantics

- `auto.set_reporter`: When true, reporter is auto-populated during task creation whenever an explicit value is absent. The service first honors the merged `default_reporter` in the configuration precedence above (project over environment), then falls back to the identity detectors above.
- `auto.assign_on_status`: When true, the first time a task moves away from the default/first status and has no assignee, it is set to the resolved current user. The logic is shared across CLI, REST, and MCP updates.

Both settings honor the configuration precedence chain, and they can be toggled globally, per-project, or at runtime through `LOTAR_AUTO_SET_REPORTER` / `LOTAR_AUTO_ASSIGN_ON_STATUS`.

## Tips

- Use `--explain` (where available) to see how values were chosen. Commands such as `lotar whoami --explain` and `lotar config show --explain` surface each layer in the order above.
- Use `lotar whoami` to see your resolved identity and source chain.
- Use `lotar config show` to see which tasks directory was picked.
