# LoTaR Overview

LoTaR keeps lightweight task trackers inside your repository so you can plan work without leaving Git. Use it from the CLI, the web dashboard, or IDE integrations.

## Quick Start

```bash
# Initialize a workspace (creates .tasks if needed)
lotar add "Implement OAuth" --type feature --priority high

# Review work
lotar list --status todo --mine

# Move the work forward
lotar status 1 in_progress

# Capture context
lotar comment 1 -m "Ready for review"

# Launch the web UI
lotar serve --open
```

> Tip: LoTaR auto-detects your project prefix. Type the numeric ID (`1`) for day‑to‑day commands, and switch to the fully qualified ID (`AUTH-1`) when you hop between projects.

## Everyday Workflow

| Need to… | Command |
| --- | --- |
| Capture new work | `lotar add` with title, type, priority, tags, and custom fields |
| Find tasks | `lotar list` with filters such as `--status`, `--priority`, `--mine`, `--tag`, or `--where key=value` |
| Move tasks through states | `lotar status <id> <new_status>` (supports dry-run + explain) |
| Update ownership | `lotar assignee <id> <name|@me>` |
| Track deadlines and estimates | `lotar due-date`, `lotar effort` |
| Capture discussion | `lotar comment` or use the web UI task panel |
| Watch metrics | `lotar stats`, `lotar sprint ...`, or the Insights tab in the browser |

## Global CLI Flags

| Flag | What it does |
| --- | --- |
| `--format text|table|json|markdown` | Pick an output style once and reuse it everywhere. |
| `--log-level <level>` / `--verbose` | Control how chatty the command should be. |
| `--project, -p <PREFIX>` | Force a project context when auto-detection isn’t enough. On `lotar serve`, use `--project` (the short `-p` is reserved for `--port`). |
| `--tasks-dir <PATH>` | Point at a completely different workspace; the folder is created automatically when missing. |

These flags stack with command-specific options. Example:

```bash
lotar --format json list --project AUTH --status todo --limit 50
```

## Environment Essentials

| Variable | Why set it? |
| --- | --- |
| `LOTAR_TASKS_DIR` | Keep your workspace alongside the repo, or point CI at a shared cache. |
| `LOTAR_PROJECT` | Default prefix when you rarely switch projects. |
| `LOTAR_DEFAULT_ASSIGNEE` / `LOTAR_DEFAULT_REPORTER` | Seed new tasks with the right people. |
| `LOTAR_PORT` | Lock the web UI to a known port for tunnels or shared dev boxes. |
| `LOTAR_AUTO_IDENTITY` / `LOTAR_AUTO_IDENTITY_GIT` | Enable or disable `@me` lookups based on local policy. |

See [Environment Variables](./environment.md) for automation toggles, sprint defaults, diagnostics, and more.

## Working with Projects

LoTaR stores data in `.tasks/<PROJECT>` folders. Discovery order:

1. `--tasks-dir` flag
2. `LOTAR_TASKS_DIR`
3. Home/global `tasks_folder` setting
4. Parent directory search
5. Create `.tasks` beside the current directory

Configuration precedence follows the same idea: CLI flag → environment → home → project → global → defaults. Learn the full chain in [Resolution & Precedence](./precedence.md).

## Output Styles

- **Text**: colorful summaries for terminals.
- **Table**: clean columns when you need to scan IDs quickly.
- **JSON**: structured payloads for scripts and dashboards.
- **Markdown**: copy/paste directly into docs or pull requests.

Switch formats with `--format` and keep piping them into other tools (e.g., `lotar list --format markdown | tee TASKS.md`).

## Launch Points

- `lotar help <command>` – detailed usage for any subcommand
- `lotar serve --open` – browser UI + REST API + SSE stream
- `lotar mcp` – IDE or AI agent integrations (JSON-RPC)
- `lotar scan <paths>` – turn TODO/FIXME comments into tasks

