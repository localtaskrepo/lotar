<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="view/assets/branding/lotar-logo-dark.svg">
    <img src="view/assets/branding/lotar-logo.svg" width="320" alt="LoTaR">
  </picture>
</h1>

<p align="center"><b>A git-native issue tracker that lives in your repo, and can run your coding agents for you.</b></p>

<p align="center"><a href="https://localtaskrepo.github.io/lotar/">Homepage</a> · <a href="docs/help/index.md">Docs</a> · <a href="https://github.com/localtaskrepo/lotar/discussions">Discussions</a></p>

<p align="center">
  <a href="https://github.com/localtaskrepo/lotar/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/localtaskrepo/lotar"></a>
  <a href="https://github.com/localtaskrepo/lotar/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/localtaskrepo/lotar/actions/workflows/ci.yml/badge.svg"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/github/license/localtaskrepo/lotar"></a>
  <a href="https://github.com/localtaskrepo/homebrew-lotar"><img alt="Homebrew tap" src="https://img.shields.io/badge/homebrew-localtaskrepo%2Flotar-orange"></a>
  <a href="https://hub.docker.com/r/mallox/lotar"><img alt="Docker pulls" src="https://img.shields.io/docker/pulls/mallox/lotar"></a>
</p>

<p align="center">
  <img src="docs/assets/screenshots/hero-agent-loop.webp" width="900"
       alt="Assigning a task to the claude agent profile in the web UI: the job starts in its own git worktree, its log streams live, and automation moves the task to NeedsReview with every change in the task history">
</p>

LoTaR keeps tasks as plain YAML files in `.tasks/`, versioned and branched with your code. The CLI, the built-in web UI, the REST API, and the MCP server all read and write those same files, so there is no database and no hosted service. Assign a task to an agent profile and LoTaR runs your coding agent CLI on it in an isolated git worktree, streams the log, and hands the task back for review.

## Quick start

| Platform | Install |
| --- | --- |
| macOS, Linux | `brew tap localtaskrepo/lotar && brew install lotar` |
| Linux, macOS (script) | `curl -fsSL https://localtaskrepo.github.io/lotar/install.sh \| sh` |
| Windows | `scoop bucket add lotar https://github.com/localtaskrepo/scoop-lotar; scoop install lotar` |
| Windows (script) | `irm https://localtaskrepo.github.io/lotar/install.ps1 \| iex` |
| Any OS with Node.js | `npm install --global @localtaskrepo/lotar`, or `npx -y @localtaskrepo/lotar` |
| Linux, macOS, Windows | Signed binaries and `.deb`/`.rpm` packages on [GitHub Releases](https://github.com/localtaskrepo/lotar/releases/latest) |
| Any OS with Docker | `docker run --rm -v "$PWD":/workspace -v "$PWD/.tasks":/tasks -w /workspace mallox/lotar list` |

LoTaR is not published on crates.io; to build it yourself, see [Installation](docs/help/install.md#build-from-source).

Then, inside any repository:

```bash
lotar add "Fix the login redirect loop" --type bug --priority high
lotar list
lotar serve --open
```

The first command creates `.tasks/` with sensible defaults. Commit it along with your code.

<p align="center"><img src="docs/assets/screenshots/terminal.gif" width="900" alt="Terminal session: lotar add creates a task, lotar list filters by status, lotar scan previews TODO comments it would turn into tasks, and lotar stats lists authors from git history"></p>

## Feature tour

### Agents and automation

Configure an agent profile once, then assign a task to it like you would to a teammate. LoTaR queues a job, runs the agent CLI in a dedicated worktree on its own branch, and streams the output to the CLI and the web UI. Claude Code, Codex, Copilot CLI and Gemini CLI are supported, with Cursor, Amp, Droid, OpenCode, Kilo and Antigravity following their documented headless modes. Choose the model and reasoning effort per profile or per run, see each job's token usage and cost, and let the agent update its own task through LoTaR's MCP server. Automation rules in YAML decide what happens on start, success, and failure, and also react to ordinary task changes. See [Agent jobs](docs/help/agent.md) and [Automation rules](docs/help/automation.md).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshots/agents-dark.webp">
  <img src="docs/assets/screenshots/agents-light.webp" alt="Agent jobs page with a running claude job, its worktree branch, and the live log">
</picture>

```yaml
# .tasks/automation.yml
automation:
  rules:
    - name: Agent lifecycle
      when: { assignee: "@agent" }
      on:
        job_started:   { set: { status: InProgress } }
        job_completed: { set: { status: NeedsReview, assignee: "@reporter" } }
        job_failed:    { set: { status: Blocked } }
```

### Web UI: board, sprints, calendar, insights

`lotar serve` starts a local web app on the same files: a filterable task list, a board with WIP limits, sprints with burndown and velocity, a calendar of due dates and sprint windows, a dependency graph, and insights for developers, leads and managers. Changes made in the CLI or by agents show up live. See [Serve](docs/help/serve.md) and [Sprints](docs/help/sprints.md).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshots/board-dark.webp">
  <img src="docs/assets/screenshots/board-light.webp" alt="Board view with Todo, InProgress, NeedsReview, Blocked, and Done columns">
</picture>

<table>
  <tr>
    <td><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshots/sprints-dark.webp"><img src="docs/assets/screenshots/sprints-light.webp" alt="Sprint burndown chart for the active sprint"></picture></td>
    <td><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshots/calendar-dark.webp"><img src="docs/assets/screenshots/calendar-light.webp" alt="Calendar with task due dates"></picture></td>
    <td><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshots/insights-dark.webp"><img src="docs/assets/screenshots/insights-light.webp" alt="Insights overview with headline numbers and a created vs completed trend"></picture></td>
  </tr>
</table>

Paste or drop screenshots and files onto a task: images get previews, a cover on the board card and a full-size viewer, and photo location data is removed before anything is stored. `lotar task reference add upload` attaches files from the command line. The command palette (<kbd>⌘K</kbd> / <kbd>Ctrl K</kbd>) jumps to any page or task and runs task actions, and keyboard shortcuts cover navigation and lists.

<table>
  <tr>
    <td><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshots/task-attachments-dark.webp"><img src="docs/assets/screenshots/task-attachments-light.webp" alt="Task panel with an attached design mockup shown as an image preview"></picture></td>
    <td><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshots/command-palette-dark.webp"><img src="docs/assets/screenshots/command-palette-light.webp" alt="Command palette searching for sync, listing the Sync page and matching tasks"></picture></td>
  </tr>
</table>

### Themes and branding

Pick from Classic and High contrast or playful skins such as Terminal, Matrix, Synthwave, Blueprint, Paper, Retro and Candy, each in light and dark and most with several colour palettes. Everyone chooses their own theme in Preferences; a repository can set the default, its name, logo and browser icon, and even ship its own skin file. See [Serve](docs/help/serve.md#web-interface).

<table>
  <tr>
    <td><img src="docs/assets/screenshots/theme-terminal.webp" alt="Board in the Terminal theme: green monospace text on black"></td>
    <td><img src="docs/assets/screenshots/theme-synthwave.webp" alt="Board in the Synthwave theme: neon pink and purple on dark"></td>
  </tr>
  <tr>
    <td><img src="docs/assets/screenshots/theme-paper.webp" alt="Board in the Paper theme: serif text in blue ink on warm paper"></td>
    <td><img src="docs/assets/screenshots/theme-retro.webp" alt="Board in the Retro theme: grey bevelled 90s desktop chrome"></td>
  </tr>
</table>

### MCP server for coding agents

`lotar mcp` speaks the Model Context Protocol over stdio, so agents in your editor or terminal can list, create, and update tasks, manage sprints, and read config without scraping CLI output. See [MCP server](docs/help/mcp.md) and the [LoTaR agent skill](docs/help/agent-skills.md).

```json
{ "mcpServers": { "lotar": { "command": "lotar", "args": ["mcp"] } } }
```

### Search

`lotar list`, every search box in the web UI, the REST API and MCP share one search. Every word must match, in any order; quote a phrase, exclude with `-word`, and small typos are forgiven. Limit a term to one field with `title:`, `desc:` or `comment:`, use `/regular expressions/`, and order by best match on request. See [search text](docs/help/list.md#search-text).

```bash
lotar list 'login redirect -oauth'            # all words, one excluded
lotar list 'title:/^fix/ comment:safari'      # field scopes and a regex
lotar list 'redirct loop' --sort-by relevance # typo-tolerant, best match first
```

### TODO scanner

`lotar scan` turns `TODO`, `FIXME`, and similar comments into tasks, writes the new task ID back into the comment, and keeps a code reference on the task that follows the line when code moves. Preview first with `--dry-run`, or pick findings individually on the Scan page. See [Scan](docs/help/scan.md).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshots/scan-dark.webp">
  <img src="docs/assets/screenshots/scan-light.webp" alt="Scan page listing TODO and FIXME comments with the edits a scan would make">
</picture>

### Jira and GitHub sync (beta)

Pull issues from Jira or GitHub into local tasks, or push local changes back, with field and value mappings per remote. Sync runs only when you ask: from the CLI, the Sync page, or MCP. Credentials stay in your home config or environment, never in the repo. See the [sync guide](docs/developers/sync.md).

```yaml
# .tasks/<PROJECT>/config.yml
remotes:
  github:
    provider: github
    repo: your-org/your-repo
    auth_profile: github.default
```

```bash
lotar pull github --dry-run
lotar push github
```

### Git history and stats

Because tasks are files in git, every change has an author and a commit. LoTaR reads that history to show per-task commits and diffs, and to report churn, activity, and authors; these commands only read the repository. See [History](docs/help/history.md) and [Stats](docs/help/stats.md).

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/screenshots/task-commits-dark.webp">
  <img src="docs/assets/screenshots/task-commits-light.webp" alt="Task panel showing the git commits that touched the task file">
</picture>

```bash
lotar task history ATLS-26           # commits that touched the task
lotar stats churn --since 30d        # most-changed tasks
lotar stats authors --since 90d      # who changed tasks
```

### Custom fields and templates

Define your own states, types, priorities, tags, and custom fields per project, or start from the `default`, `agile`, or `kanban` template. Every surface validates against the same config. See [Configuration](docs/help/config.md) and [Templates](docs/help/templates.md).

```bash
lotar init --template=agile --project=mobile
lotar config set custom_fields component,team --project=mobile
lotar add "Offline mode for the editor" --project=mobile --field component=editor
lotar list component=editor
```

## How LoTaR compares

An honest snapshot as of October 2026. Rows marked **◀** are where LoTaR is behind at least one alternative.

| | LoTaR | [Backlog.md](https://github.com/MrLesk/Backlog.md) | GitHub Issues | Jira |
| --- | --- | --- | --- | --- |
| Tasks live in your repo | ✅ YAML | ✅ Markdown | ❌ hosted | ❌ hosted |
| Works offline | ✅ | ✅ | ❌ | ❌ |
| Web UI | ✅ local | ✅ local | ✅ | ✅ |
| MCP server | ✅ | ✅ | ✅ official server | ✅ official server (Cloud) |
| Runs coding agents on tasks | ✅ local agent CLIs in worktrees | ❌ | ✅ Copilot coding agent (paid) | ✅ Rovo and third-party agents (Cloud, paid) |
| Automation rules | ✅ | ❌ | ⚠️ via Actions and Projects workflows | ✅ |
| Sprints and burndown | ✅ | ⚠️ milestones, no burndown | ⚠️ iterations, no burndown chart | ✅ |
| Sync with other trackers | ⚠️ Jira and GitHub (beta) | ❌ | n/a | n/a |
| TODO comment scanning | ✅ | ❌ | ❌ | ❌ |
| Typo-tolerant search | ✅ all words, phrases, exclusions, typos, field scopes, regex, best-match sort | ✅ fuzzy search | ❌ | ❌ |
| **◀** Docs and decision records | ❌ | ✅ | ⚠️ wiki | ⚠️ via Confluence |

Pick GitHub Issues or Jira when you need a hosted tracker for non-developers, permissions, and integrations. Pick LoTaR when you want tasks reviewed in the same pull request as the code, and agents working those tasks on your own machine.

## How it works

```text
.tasks/
├── config.yml          # workspace defaults: states, types, members, agent profiles
├── automation.yml      # optional automation rules
├── @sprints/1.yml      # sprint plans and actual start/close times
├── @attachments/       # attached files, stored once per content
└── ATLS/               # one folder per project
    ├── config.yml      # optional project overrides and sync remotes
    ├── 26.yml          # one file per task
    └── 27.yml
```

```yaml
# .tasks/ATLS/26.yml
title: Offline edits are lost when reconnecting mid-sync
status: Done
priority: Critical
type: Bug
reporter: priya
assignee: marco
due_date: 2026-09-29
effort: 3pt
tags: [sync, customer]
references:
  - link: https://github.com/atlas-notes/atlas/pull/412
  - code: web/src/sync/queue.ts#20-33
history:
  - at: 2026-09-28T15:12:00Z
    actor: priya
    changes: [{ field: status, old: NeedsReview, new: Done }]
```

## Documentation

- [Installation](docs/help/install.md): every install method, Docker usage, building from source
- [CLI tour](docs/help/cli-tour.md): example workflow, multi-project setups, global options, configuration basics
- [Help index](docs/help/index.md): every command and reference page
- [Configuration reference](docs/help/config-reference.md), [precedence](docs/help/precedence.md), and [environment variables](docs/help/environment.md)
- [Agent jobs](docs/help/agent.md), [automation rules](docs/help/automation.md), and the [LoTaR agent skill](docs/help/agent-skills.md)
- [REST API quick reference](docs/help/api-quick-reference.md) and [OpenAPI spec](docs/openapi.json)
- [Architecture decisions](docs/architecture-decisions.md) and [developer docs](docs/developers/README.md)

## Contributing

Questions and ideas are welcome in [Discussions](https://github.com/localtaskrepo/lotar/discussions), and bug reports and pull requests in [issues](https://github.com/localtaskrepo/lotar/issues/new/choose); report security problems privately as described in [SECURITY.md](SECURITY.md). Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md). Development setup, test commands, and conventions are in [AGENTS.md](AGENTS.md) and [docs/developers/](docs/developers/README.md). After UI changes, `npm run screenshots` regenerates every image in this README using an isolated demo workspace.

Full regeneration needs a Git-capable environment. `npm run screenshots -- --no-git` refreshes the non-Git UI screenshots while preserving the existing agent, hero, commit-history and terminal media.

## License

[MIT](LICENSE)
