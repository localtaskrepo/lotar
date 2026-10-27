# Agent Jobs

LoTaR can orchestrate supported agent CLIs (Copilot CLI, Claude Code, Codex CLI, Gemini CLI) and stream progress over SSE.

## CLI commands

- `lotar agent run <TICKET> <PROMPT> [--runner <copilot|claude|codex|gemini>] [--agent <profile>] [--model <MODEL>] [--effort <LEVEL>] [--wait] [--follow] [--timeout-seconds <N>]`
- `lotar agent status <JOB_ID>`
- `lotar agent logs <JOB_ID>`
- `lotar agent cancel <JOB_ID>`
- `lotar agent list-running`
- `lotar agent list-jobs [-n <LIMIT>]` — list job logs from disk
- `lotar agent check [--status <value>] [--assignee <handle>]`
- `lotar agent worktree list` — list agent worktrees
- `lotar agent worktree cleanup [--all] [--delete-branches] [--dry-run]` — remove stale worktrees

## Runners

Each agent profile names a runner. LoTaR starts the CLI headless, reads its JSON event stream, and shows tool calls as progress and the agent's replies as messages.

| Runner | Checked against | How LoTaR starts it |
| --- | --- | --- |
| `claude` | Claude Code 2.1.308 | `claude -p --output-format stream-json --input-format stream-json --verbose`; the prompt goes to stdin, so you can send messages while it runs |
| `codex` | Codex CLI 0.163 | `codex exec --json … <prompt>` |
| `copilot` | GitHub Copilot CLI 1.0.69 | `copilot -p <prompt> --output-format json --no-ask-user …` |
| `gemini` | Gemini CLI headless docs (not run live) | `gemini --output-format stream-json … -p <prompt>` |
| `cursor` | Cursor CLI docs (not run live) | `cursor-agent -p --output-format stream-json --trust … <prompt>`; set `command: agent` if your install only has the newer `agent` binary |
| `amp` | Amp execute-mode docs (not run live) | `amp --stream-json … -x <prompt>` |
| `droid` | Factory Droid `droid exec` docs (not run live) | `droid exec --output-format json … <prompt>`; output arrives when the run ends, so there is no live progress |
| `opencode` | OpenCode CLI docs (not run live) | `opencode run --format json … <prompt>` |
| `kilo` | Kilo Code CLI (an OpenCode fork; not run live) | `kilo run --format json … <prompt>` |
| `antigravity` | Antigravity CLI headless docs (not run live) | `agy --output-format stream-json --disable-slash-commands … -p <prompt>` |
| `command` | any program | runs `command` with `args`; the prompt is in `LOTAR_AGENT_PROMPT` |

The runners marked "not run live" follow each CLI's documented headless interface; their parsers are tested against the documented example events. Treat them as a starting point and report differences you see with a real install; the research behind them is in [agent CLI research notes](../developers/agent-cli-research.md).

Profile `args` come after LoTaR's flags. A job fails when the CLI exits non-zero or reports an error in its final result, even if it exits with 0.

### Permissions

An agent runs without anyone at the keyboard, so it can't answer permission prompts. The profile's `permissions` field picks what it may do; LoTaR turns it into each CLI's own flags.

```yaml
agents:
  reviewer:
    runner: claude
    permissions: read-only   # read-only | workspace (default) | full
```

| Level | claude | codex | copilot | gemini |
| --- | --- | --- | --- | --- |
| `read-only` | `--permission-mode plan` | `--sandbox read-only` | all tools except write and shell | `--approval-mode plan` |
| `workspace` | `--permission-mode acceptEdits` plus Claude's sandbox, with sandboxed shell commands allowed | `--sandbox workspace-write` | all tools except `git push` | `--approval-mode auto_edit` |
| `full` | `--permission-mode bypassPermissions` | `--dangerously-bypass-approvals-and-sandbox` | `--allow-all` | `--approval-mode yolo` |

- `workspace` lets the agent edit the job's working directory. It can also write the tasks directory (to update its ticket) and, in a worktree, the repository's `.git` directory (to commit). A merge job (profile `merge`, or a ticket in `Merging`) gets the main checkout instead of `.git`, so it can fast-forward main. These are passed as `--add-dir` (Gemini: `--include-directories`).
- Commits from inside a sandbox can fail: Codex keeps git metadata read-only, and a signing key agent (1Password, `gpg-agent`, `ssh-agent`) is usually out of reach. LoTaR then commits the agent's changes itself when the job ends (see [Default instructions](#default-instructions)).
- Copilot has no sandbox. At `workspace` its file tools stay in the working directory, but shell commands are not confined.
- The newer runners map the levels to their documented options:

  | Level | cursor | amp | droid | opencode, kilo | antigravity |
  | --- | --- | --- | --- | --- | --- |
  | `read-only` | `--mode plan` | refused (Amp has no read-only mode) | default (Droid is read-only without `--auto`) | permission config denies edit and bash | `--mode plan` |
  | `workspace` | `--force --sandbox enabled` | Amp's defaults | `--auto medium` | `--auto` with permission config: bash allowed except `git push`, the extra directories allowed | `--mode accept-edits` plus `--add-dir` |
  | `full` | `--force --sandbox disabled` | `--dangerously-allow-all` | `--skip-permissions-unsafe` | `--auto` with every permission allowed | `--dangerously-skip-permissions` |

  OpenCode and Kilo get their permission rules through `OPENCODE_CONFIG_CONTENT`, unless the profile's `env` sets it.
- If the profile's `args` already set permissions (for example `--permission-mode`, `--sandbox`, `--allow-tool`/`--deny-tool`, `--approval-mode` or `--yolo`), LoTaR adds no permission flags of its own.
- `command` runners get no permission flags.

### Model and effort

A profile can pick the model and reasoning effort; a single run can override them with `lotar agent run --model/--effort`, the `model`/`effort` fields of `POST /api/jobs` and the MCP `agent_run` tool, or an automation action's [`agent` settings](automation.md#agent-run-settings).

```yaml
agents:
  implement:
    runner: claude
    model: sonnet
    effort: high
  review:
    runner: codex
    model: gpt-5.5
    effort: medium
```

| Runner | `model` | `effort` |
| --- | --- | --- |
| `claude` | `--model` | `--effort` (low, medium, high, xhigh, max) |
| `codex` | `-m` | `-c model_reasoning_effort=<value>` (depends on the model, for example low, medium, high, xhigh) |
| `copilot` | `--model` | `--reasoning-effort` (none, minimal, low, medium, high, xhigh, max) |
| `gemini` | `-m` | not supported |
| `cursor` | `--model` | not supported |
| `amp` | not supported (Amp chooses models through its modes) | not supported |
| `droid` | `-m` | `--reasoning-effort` |
| `opencode`, `kilo` | `-m provider/model` | `--variant` |
| `antigravity` | `--model` | `--effort` |
| `command` | `LOTAR_AGENT_MODEL` | `LOTAR_AGENT_EFFORT` |

Values are passed through as given, so each CLI decides which names it accepts. They may use letters, digits and `. _ - : / @ [ ]`. If the profile's `args` already set the model or effort, those win over the profile's `model`/`effort` fields; a single run's override wins over both, replacing the matching flags from the profile's `args` for that run. Jobs show their model and effort in `lotar agent status`, the job list, and on the Agents page.

### Usage and cost

When a job ends, LoTaR records what its CLI reports about it:

| Runner | Recorded |
| --- | --- |
| `claude` | input tokens (with cached), output tokens, cost in USD, turns |
| `codex` | input tokens (with cached), output tokens (including reasoning) |
| `copilot` | premium requests, lines added and removed |
| `gemini` | input and output tokens |
| `amp`, `droid` | input tokens (with cached), output tokens, turns |
| `opencode`, `kilo` | input tokens (with cached), output tokens, cost in USD, steps as turns |
| `antigravity` | input tokens (with cached), output tokens including thinking, turns |
| `cursor` | nothing (its output reports no usage) |

The usage appears in `lotar agent status`, at the end of `lotar agent run --wait`, on the Agents page (per job, plus a cost total per ticket), in the job's `usage` field over REST, SSE and MCP, and in the job log. Automation actions on job events can use `${{agent.usage}}`, `${{agent.cost_usd}}`, `${{agent.input_tokens}}`, `${{agent.output_tokens}}` and `${{agent.turns}}`, for example to comment the cost on the ticket.

A Claude profile can cap what a run may spend with `max_budget_usd` (passed as `--max-budget-usd`); other CLIs have no such limit.

```yaml
agents:
  implement:
    runner: claude
    max_budget_usd: 2.5
```

### Tools and MCP servers

`tools` and `mcp` fine-tune what a Claude or Copilot agent may use, in that CLI's own tool syntax. They add to the permission level rather than replace it.

```yaml
agents:
  implement:
    runner: claude
    tools:
      allowed: ["WebFetch", "Bash(npm test:*)"]
      disallowed: ["Bash(git push:*)"]
    mcp:
      allow: [docs]   # tools of the MCP server named "docs" need no approval
```

| Setting | claude | copilot |
| --- | --- | --- |
| `tools.allowed` | `--allowedTools=<tool>` | `--allow-tool=<tool>` |
| `tools.disallowed` | `--disallowedTools=<tool>` | `--deny-tool=<tool>` |
| `mcp.allow` | `--allowedTools=mcp__<server>` | `--allow-tool=<server>` |

Codex and Gemini have no per-tool flags, so these settings don't apply to them. The MCP servers themselves come from each CLI's own configuration.

#### LoTaR's own MCP server

With `mcp.lotar: true`, a job also gets LoTaR's MCP server (`lotar mcp`, the same build that started the job, bound to the job's tasks directory) under the name `lotar`, so the agent reads and updates its ticket through tools instead of the CLI:

```yaml
agents:
  implement:
    runner: claude
    mcp:
      lotar: true
```

| Runner | How the server is added | Approval |
| --- | --- | --- |
| `claude` | `--mcp-config` | `--allowedTools=mcp__lotar` |
| `copilot` | `--additional-mcp-config` | `--allow-tool=lotar` |
| `codex` | `-c mcp_servers.lotar.*` | `default_tools_approval_mode = "approve"` |

The server only offers the tools a job needs: `task_get`, `task_list`, `task_update`, `task_comment_add`, `task_reference_add`, `relationship_graph`, `config_show` and `whoami` (set through `LOTAR_MCP_TOOLS`, which `lotar mcp` honours in general). An agent can't start other agents, delete tasks or sync through it. Because the server is named `lotar`, it takes the place of a LoTaR server with that name in the CLI's own settings for the job, which would point at a different tasks directory or build. Other runners don't support the switch; their jobs log a note and use the CLI.

## list-running requirements

`lotar agent list-running` discovers running jobs by scanning for the wrapper process name. This requires the wrapper binary to be available in the same directory as `lotar` or on `PATH` (or set `LOTAR_AGENT_WRAPPER` to an absolute path).

If the wrapper is not available, jobs can still run, but `list-running` will show no results.

## Environment variables

- `LOTAR_AGENT_WRAPPER` — absolute path to the wrapper binary if it is not on `PATH`.
- `LOTAR_AGENT_CONTEXT_EXTENSION` — extension for context files (default `.context`).
- `LOTAR_AGENT_LOGS_DIR` — where job logs go instead of `<tasks dir>/@agent-logs` (relative or absolute; `off` turns logging off).

Every job also gets the ticket's `LOTAR_*` variables (`LOTAR_TICKET_ID`, `LOTAR_TICKET_PROJECT`, `LOTAR_TICKET_REPORTER`, `LOTAR_TASKS_DIR`, the worktree path and branch, …) and `LOTAR_BIN`, the absolute path of the `lotar` that started the job. The built-in instructions call that path (`${{lotar.bin}}`) and its directory is put first on PATH, so an older `lotar` elsewhere (or a CLI that runs commands in a login shell, like Codex) can't answer instead.

## Windows agent commands

Every job prompt is multiline (it embeds ticket context). Windows cannot pass such arguments through `cmd.exe` safely: Rust's spawn validation rejects CR/LF newlines in batch arguments outright, and `cmd.exe` argument forwarding can mangle other special characters. When an agent profile `command` resolves to a `.cmd`/`.bat` file, the wrapper therefore tries to resolve the shim to the executable it forwards to and runs that directly: npm-generated `cmd` shims (the standard `claude`, `codex`, `copilot`, and `gemini` global installs) are recognized and executed as `node <cli.js>` with the original arguments followed by any baked shim suffix, so the complete prompt arrives byte-exact. Batch commands the wrapper cannot recognize keep going through `cmd.exe` with the limitations above.

For an unrecognized batch command, either point `command` at the CLI's native executable or a recognized npm shim path, or use a `command` runner profile: the `command` kind delivers the prompt through the `LOTAR_AGENT_PROMPT` environment variable instead of an argument, so any executable command line (including `command: node` with a script in `args`) works there. Setting `command: node` with the CLI entry in `args` does **not** work for the codex/claude/copilot/gemini kinds, because those prepend their own flags before profile arguments and node would parse those flags as its own.

## Default instructions

Every job prompt starts with instructions, then a "This job" section, then the ticket and the request.

The instructions are the base text followed by the profile's own `instructions`:

- The base text is `agent.instructions` when set, otherwise the built-in text in [agent-instructions.md](agent-instructions.md). It covers the `lotar` commands for reading and updating the ticket and how to hand the ticket back when the agent needs a decision. Set `agent.instructions: ""` to leave it out.
- `agents.<name>.instructions` is added after it, so a phase agent keeps the base text.

```yaml
agent:
  # Inline string
  instructions: "Follow AGENTS.md and keep changes focused."
  # Or point to a file (relative to the tasks directory by default)
  # instructions:
  #   file: "@agent-instructions.md"
agents:
  test:
    runner: claude
    instructions: "You are in the TESTING phase. Run the tests and add missing ones."
```

Instructions (inline, from a file, built-in or per profile) can use `${{...}}` templates, which LoTaR fills in when the job starts:

| Variable | Value |
| --- | --- |
| `ticket.id`, `ticket.title`, `ticket.status`, `ticket.priority`, `ticket.type`, `ticket.assignee`, `ticket.reporter`, `ticket.description`, `ticket.due_date`, `ticket.effort`, `ticket.tags`, `ticket.field:<name>` | the ticket's fields, as in automation templates |
| `ticket.project` | the ticket's project prefix |
| `agent.job_id`, `agent.runner`, `agent.profile` | the job |
| `agent.worktree_branch`, `agent.worktree_path` | the ticket's branch and worktree, when worktrees are on |
| `lotar.bin` | absolute path of the `lotar` that started the job |
| `workspace.root` | the main checkout |

Lists work as in automation templates, for example `${{ticket.references[*].link}}` or `${{ticket.comments[-1].text}}` (see [Template variables](automation.md#template-variables)). Unknown variables expand to nothing. The built-in instructions use them so that commands contain literal values: Claude Code refuses commands with shell variables (`"$LOTAR_TICKET_ID"`, `$?`) in headless runs.

LoTaR writes the "This job" section itself (a `read-only` job is only told that it can't change anything and that its final answer is saved as the job summary):

- With worktrees on, it names the branch (`agent/<TICKET>`) and tells the agent to commit its work there. Without worktrees the agent works in the main checkout and leaves its changes uncommitted unless asked to commit.
- If an automation rule moves the ticket when the job ends (a rule that matches the ticket and has a `job_completed` action), the agent is told not to change the status or assignee when it finished, only to comment, and to hand the ticket back when it could not finish. Otherwise it is told to set a fitting status when it finishes.

When a worktree job succeeds with changes still uncommitted, LoTaR commits them to the ticket branch (`<TICKET>: changes from the <profile> agent`), so the next phase and the merge see them. Agents can't always commit themselves: Codex's `workspace-write` sandbox keeps git metadata read-only, and commit signing may be out of reach. LoTaR commits as you, outside the sandbox, so your hooks and signing apply; if that commit fails, the job fails with git's error.

A merge job (profile `merge`, or a ticket in `Merging`) has to end with its branch in the main checkout's current branch. If the agent couldn't fast-forward it (a sandbox may keep git metadata read-only), LoTaR runs `git merge --ff-only <branch>` in the main checkout, which never overwrites local changes. If the branch still isn't merged, the job fails, so `job_failed` rules run instead of `job_completed`.

To hand a ticket back, an agent comments, sets a waiting status such as `HelpNeeded` and assigns the reporter (`lotar assignee <TICKET> "<reporter>"`; the ticket's own reporter is always accepted, even when it isn't username-shaped). Once the assignee is no longer the agent, rules that match on that agent no longer fire when its job ends.

Repository guidance for agents working on LoTaR itself lives in:

- [AGENTS.md](../../AGENTS.md)
- [Backend instructions](../../.github/instructions/backend.instructions.md)
- [Frontend instructions](../../.github/instructions/frontend.instructions.md)
- [Smoke instructions](../../.github/instructions/smoke.instructions.md)

## Web UI

Without any agent profile, the Agents page shows a setup panel: pick a name and a CLI (the ones installed on the machine running LoTaR are preselected) and add the profile to the current project or to all projects. The same works from the command line with `lotar config set agents '{"implement": "claude"}' --project PROJ` (the value replaces that scope's whole `agents` map) or over REST with `POST /api/agents/profiles`.

The Agents page lists jobs grouped by ticket and updates live over SSE:

- Each ticket shows its title, current status and assignee, and the order its jobs ran in (for example `@implement → @test`).
- Each job shows its agent, runner, model and effort, when it started and how long it ran, its branch (hover for the worktree path), the exit code when it failed, and the agent's last message.
- **Show logs** opens a timeline of the job: tool calls, the agent's messages, denied tools and notes from LoTaR (worktree, commits, fast-forwards).
- **Run agent** (or **Run again** on a ticket) starts a job on a ticket without changing its assignee; pick the agent profile and optionally a model, effort and request. Clicking a profile in the profile list opens the same dialog.
- You can stop a running job, remove a queued one, or stop all of them. While a Claude job runs you can send it a message; other runners don't accept messages.

## Worktrees

Agent jobs can optionally run inside dedicated git worktrees. This isolates agent work from your main working directory, preventing conflicts when agents are working in parallel or while you are making changes.

### Configuration

Configure worktrees under `agent.worktree`:

```yaml
agent:
  worktree:
    enabled: true                # Enable worktree isolation (default: false)
    dir: ".lotar-worktrees"      # Directory for worktrees (relative to repo parent or absolute)
    branch_prefix: "agent/"      # Branch prefix for worktree branches (default: "agent/")
    max_parallel_jobs: 3         # Limit concurrent agent jobs (optional, default: unlimited)
    cleanup_on_done: true         # Remove worktrees after successful jobs when ticket is done
    cleanup_on_failure: false     # Remove worktrees after failed jobs (default: false)
    cleanup_on_cancel: false      # Remove worktrees after cancelled jobs (default: false)
    cleanup_delete_branches: true # Delete worktree branches during cleanup
    cleanup_ignored_files: true   # Git-ignored files (build output) don't block cleanup (default: true)
```

When enabled:
- Each ticket gets a dedicated worktree at `<dir>/<repo>/<ticket-id>`
- A branch is created at `<branch_prefix><ticket-id>` (e.g., `agent/TEST-1`)
- Multiple job phases for the same ticket reuse the same worktree
- The worktree persists after the job completes unless cleanup is enabled
- Cleanup only removes a worktree whose ticket is done, whose branch is merged into the main checkout, and that has no untracked or modified files. Git-ignored files such as build and test output are removed with it; set `cleanup_ignored_files: false` to keep any worktree that still holds ignored files. When cleanup keeps a worktree, the job log says why.
- Merge-profile jobs require worktrees so merges happen on isolated branches
- When worktrees are enabled, setup failure fails the job; the runner never falls back to the repository checkout.
- Git setup, checks, cleanup, and runners discard inherited/profile repository-selection overrides (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_COMMON_DIR`, `GIT_INDEX_FILE`, object/discovery overrides, and `GIT_CONFIG*`). Worktree top-level and common-directory identity are verified. SSH/askpass and author/committer identity environment variables are retained.
- Merge-profile jobs are serialized one-at-a-time even when `max_parallel_jobs` allows more general agent concurrency

### Managing worktrees

List all agent worktrees:
```bash
lotar agent worktree list
```

This shows each worktree with its ticket ID, branch, path, and status (active job, done, or in-progress).

Clean up stale worktrees:
```bash
# Remove worktrees for completed/closed tickets
lotar agent worktree cleanup

# Also delete the associated git branches
lotar agent worktree cleanup --delete-branches

# Remove ALL agent worktrees (except those with active jobs)
lotar agent worktree cleanup --all

# Preview what would be removed
lotar agent worktree cleanup --dry-run
```

Worktrees with active jobs are never removed, even with `--all`. Cleanup refuses dirty worktrees (including untracked or ignored files), unknown Git state, and branches not merged into the repository checkout's `HEAD`. Branch deletion uses Git's non-forced deletion checks. `--all` broadens ticket selection only; it is not a force-delete option. Refusals are reported and return a nonzero exit status.

Default CLI cleanup and all automatic cleanup require a readable, valid ticket in a configured done state. Missing, malformed, unreadable, or unknown-status tickets are preserved, even with `--all`. Failure/cancellation cleanup settings only enable cleanup when that same done-state and Git safety check succeeds.

The queue worker drains pending entries and waits for dispatched jobs, including finalization, before exiting. It also waits for dispatched jobs if queue/config processing fails. There is no single-poll `--once` mode. Start failures retain the existing bounded retry policy (three retries), with exhausted retries reported as an error after draining; failed runner jobs are terminal rather than automatically rerun.

Cancelling a dispatched job requests termination immediately, but cancellation automation and reassignment wait until process/output teardown and context persistence finish. The job retains its capacity slot through finalization, so a replacement is queued safely and cancellation automation runs once. Cancelling an undispatched queued job finalizes immediately.

Both output readers are joined before final summaries, persisted status, and slot release. After the runner exits, already-available output is drained without waiting for a surviving descendant to close inherited streams. Continuous descendant output is bounded to five seconds of draining and reported as an output failure rather than keeping the worker alive indefinitely.

## Automation rules

Agent lifecycle automation lives in the automation rules file. Without a rule, a finished job leaves the ticket's status and assignee as they are. See [Automation Rules](automation.md#agent-job-queueing) for a lifecycle example.

## Notes

- Job status is streamed over `/api/events?kinds=agent_job_*` while the server is running. Streamed events carry the same `sequence` (stable per-job identity, positive, strictly monotonic, never reused) and `at` as the matching `GET /api/jobs/logs` entries, so live and snapshot logs can be joined losslessly.
- Context history is persisted to `<project>/<ticket><extension>` files next to each ticket.
- Job events are persisted to `<tasks dir>/@agent-logs/<JOB_ID>.jsonl` by default (`agent.logs_dir` or `LOTAR_AGENT_LOGS_DIR` picks another folder; `off` turns it off). LoTaR writes a `.gitignore` into the folder so logs, which contain prompts and agent output, aren't committed, and keeps the newest 500 job logs.
- `lotar agent status`, `lotar agent logs` and `lotar agent list-jobs` read these files when the job isn't in the current process.
- Assigning a ticket to an agent profile (e.g., `@claude-review`) automatically queues a job for that ticket.
- Review-stage tickets can only be advanced by their reporter when the change would alter assignee or status.
