# Resolution & Precedence

This page explains how LoTaR resolves values for configuration, identity (for @me), and common paths, with a consistent, predictable order.

## Configuration precedence

When resolving configuration values, this order applies (highest wins):
1. Command-line flags (per command invocation)
2. Environment variables
3. Home config (~/.lotar)
4. Project config (.tasks/<PROJECT>/config.yml)
5. Global config (.tasks/config.yml)
6. Built-in defaults

Notes:
- The same chain applies across CLI, REST, and MCP.
- Project config overrides global, but home/env can override both; CLI flags always win.
- Automation toggles (auto.set_reporter, auto.assign_on_status) default to true when unspecified and follow the same precedence.

## Identity resolution and @me

Anywhere a person field is accepted (assignee, reporter, default_reporter), the special value @me is allowed. It resolves to the current user using this order:
1) Merged config default_reporter (using the precedence above)
2) Project manifest author (package.json author, Cargo.toml authors, .csproj Authors) if present
3) git config (user.name or user.email) at the repository root
4) System user from $USER/$USERNAME

Applied consistently by CLI, REST, and MCP. Automation toggles `auto.identity` and `auto.identity_git` can gate steps in this order.

## Tasks directory resolution

LoTaR locates the tasks directory in this order:
1. --tasks-dir <PATH> command-line flag
2. LOTAR_TASKS_DIR environment variable
3. Parent directory search for existing .tasks folder
4. Current directory .tasks folder (created if needed)

See also: docs/help/main.md for quick start and global options.

## Project resolution (short guide)

Project context is determined based on:
- Explicit --project flag (highest)
- Task ID prefix (e.g., AUTH-123 → AUTH)
- Auto-detection from current directory/repo naming
- Default project from configuration

## Automation semantics

- auto.set_reporter: When true, reporter is auto-populated on create/update when missing (uses identity resolution above).
- auto.assign_on_status: When true, the first time a task moves away from the default/first status and has no assignee, assignee is set to the resolved current user. Explicit assignee values are never overwritten.

Both settings honor the configuration precedence chain.

## Tips

- Use --explain (where available) to see how values were chosen.
- Use whoami to see your resolved identity and source chain.
