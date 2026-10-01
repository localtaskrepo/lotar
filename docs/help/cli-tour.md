# CLI tour

A guided walk through everyday LoTaR use from the terminal: a first workflow, multiple projects,
global options, and the configuration basics. Each section links to the full reference.
Install LoTaR first: see [Installation](install.md).

## A first workflow

```bash
# Create a task. The first command in a repository creates .tasks/ with default settings.
lotar add "Add user authentication" --type feature --priority high

# List and search (the query matches IDs, titles, descriptions, and tags)
lotar list
lotar list "auth"
lotar list --status todo --priority high

# Move work forward
lotar status 1 in_progress
lotar assignee 1 alex@example.com
lotar comment 1 -m "OAuth app registered; callback URL still pending"

# Turn TODO comments in your code into tasks (preview first)
lotar scan ./src --dry-run
lotar scan ./src

# Finish and commit the task files with your code
lotar status 1 done
git add .tasks/ && git commit -m "Complete user authentication"
```

Numeric IDs such as `1` work when LoTaR can tell which project you mean: there is a single project,
or `default_project` is set. Otherwise use the full ID (`AUTH-1`) or `--project`.

More: [add](add.md), [list](list.md), [status](status.md), [assignee](assignee.md),
[comment](comment.md), [scan](scan.md).

## Multiple projects

Each project gets its own folder and ID prefix under `.tasks/`. A new project is created the first
time you use it; its prefix is derived from the name (`backend` becomes `BACK`).

```bash
lotar add "Set up API auth" --project=backend --priority=high
lotar add "Design login UI" --project=frontend --priority=medium

# Full names and prefixes both work
lotar list --project=backend
lotar list --project=BACK

# Make one project the default for short IDs
lotar config set default_project BACK
```

Use a tasks directory outside the current repository with `--tasks-dir` or `LOTAR_TASKS_DIR`:

```bash
export LOTAR_TASKS_DIR=/shared/project-tasks
lotar add "Integration test" --project=testing

lotar add "Deploy script" --tasks-dir=/ops/tasks --project=deployment
```

More: [Resolution & precedence](precedence.md) explains how the tasks directory and project are found.

## Global options and output formats

These flags work with every command:

| Flag | Purpose |
| --- | --- |
| `--format text\|table\|json\|markdown` | Output style. Use `json` for scripts. |
| `--tasks-dir <PATH>` | Use a different `.tasks` workspace (created if missing). |
| `--project, -p <PREFIX>` | Force the project context. `lotar serve` ignores it; use `--port` there. |
| `--log-level <level>`, `--verbose` | Diagnostic output. |

```bash
lotar --format json list --project BACK --status todo --limit 50
lotar --format json stats changed --since 7d
```

Common environment variables:

| Variable | Purpose |
| --- | --- |
| `LOTAR_TASKS_DIR` | Location of the `.tasks` workspace. |
| `LOTAR_PROJECT` | Default project prefix. |
| `LOTAR_DEFAULT_ASSIGNEE`, `LOTAR_DEFAULT_REPORTER` | People used for new tasks and for `@me`. |
| `LOTAR_PORT` | Fixed port for `lotar serve` when `--port` is not given. |

More: [Environment variables](environment.md) lists every variable.

## Configuration

No configuration is needed to start: LoTaR writes defaults on first use. Change them with
`lotar config` or by editing the YAML files directly.

```bash
lotar config show                       # merged configuration
lotar config show --project=backend     # including project overrides
lotar config templates                  # default, agile, kanban
lotar init --template=agile --project=mobile   # same as `lotar config init`

lotar config set server_port 9000
lotar config set issue_states Todo,InProgress,Review,Done --project=backend
lotar config set custom_fields component,team --project=backend
```

### Where settings come from

From highest to lowest priority:

1. Command-line flags for the current command
2. Project config: `.tasks/<PROJECT>/config.yml`
3. Environment variables such as `LOTAR_TASKS_DIR` and `LOTAR_DEFAULT_ASSIGNEE`
4. Home config: `~/.lotar` (or `%APPDATA%/lotar/config.yml`)
5. Global config: `.tasks/config.yml`
6. Built-in defaults

Commands without a project context skip step 2. See [Resolution & precedence](precedence.md).

### Commonly changed settings

| Setting (`lotar config set` name) | Scope | Meaning |
| --- | --- | --- |
| `server_port` | global | Web UI port (default 8080; if unset and 8080 is busy, `lotar serve` picks a free port and says so) |
| `default_project` | global | Project used when a command does not name one |
| `issue_states` | global or project | Workflow states, for example `Todo,InProgress,Done` |
| `issue_types` | global or project | Task types, for example `Feature,Bug,Chore` |
| `issue_priorities` | global or project | Priority scale, for example `Low,Medium,High,Critical` |
| `tags` | global or project | Allowed tags (any tag by default) |
| `custom_fields` | global or project | Extra task fields such as `component` or `team` |
| `default_assignee`, `default_priority` | global or project | Defaults for new tasks |

Templates: `default` uses the global defaults; `agile` adds epics, spikes, and a Verify state;
`kanban` is a flow-based Todo/InProgress/Verify/Done workflow.

More: [Config command](config.md), [Configuration reference](config-reference.md),
[Templates](templates.md).

## History and stats from git

These commands only read the repository.

```bash
lotar task history PROJ-123                 # commits that touched the task
lotar task diff PROJ-123                    # diff of the latest such commit
lotar task diff PROJ-123 --commit abcdef1
lotar task at PROJ-123 abcdef1              # the task file at a commit

lotar stats changed --since 14d             # tasks changed in a window
lotar stats churn --since 30d --global      # commits per task, across projects
lotar stats authors --since 90d --global    # who changes tasks
lotar stats activity --since 60d --group-by day   # or author, week, project
```

More: [History](history.md), [Stats](stats.md).

## Sync with Jira or GitHub (beta)

Remotes live in the project config; credentials live in your home config or environment, never in
the repository. Sync runs only when you ask, and it can create and update issues on the remote, so
start with least-privilege credentials and `--dry-run`.

```yaml
# .tasks/<PROJECT>/config.yml
remotes:
  jira-home:
    provider: jira
    project: ENG
    auth_profile: jira.default
```

```yaml
# ~/.lotar
auth_profiles:
  jira.default:
    provider: jira
    method: basic
    email_env: LOTAR_JIRA_EMAIL
    token_env: LOTAR_JIRA_TOKEN
```

```bash
lotar pull jira-home --dry-run
lotar pull jira-home
lotar push jira-home
```

More: [Sync guide](../developers/sync.md), [Configuration reference](config-reference.md).

## Where LoTaR fits

- **Development teams** tracking features, bugs, and technical debt next to the code they affect.
- **Solo developers** who want structured tasks without an external service.
- **Code review**: task changes appear in the same pull request and git history as the code.
- **Audit trail**: every status change, comment, and edit is a commit you can inspect later.
- **Living requirements** that evolve on branches together with the implementation.
