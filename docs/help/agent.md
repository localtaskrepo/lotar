# Agent Jobs

LoTaR runs agent CLIs (Claude Code, Codex CLI, Copilot CLI, T3 Code and the others listed under [Runners](#runners)) on tickets through [harnessmux](https://github.com/localtaskrepo/harnessmux) sessions and streams their progress over SSE.

## CLI commands

- `lotar agent run <TICKET> <PROMPT> [--runner <RUNNER>] [--agent <profile>] [--model <MODEL>] [--effort <LEVEL>] [--provider <PROVIDER>] [--session new|continue|fresh] [--wait] [--follow] [--timeout-seconds <N>]`
  - `<RUNNER>` is one of the [runners](#runners): `claude`, `codex`, `copilot`, `gemini`, `cursor`, `amp`, `droid`, `opencode`, `kilo`, `antigravity`, `t3` or `command`.
  - Without `--wait` or `--follow`, the job is checked (ticket, profile or runner, settings) and handed to the background queue worker, and the command returns at once with `Queued <TICKET> for <agent>`. Follow it with `lotar agent queue` and `lotar agent list-jobs`. With `--wait`, the job runs in this terminal and the command exits non-zero unless it completes; it returns once the job's end automation (the `job_completed`/`job_failed` rules), context and worktree cleanup are done. With `--format json` it prints the finished job.
- `lotar agent status <JOB_ID>`
- `lotar agent logs <JOB_ID>`
- `lotar agent cancel <JOB_ID>` — stop a job that has started; a ticket still waiting in the queue has no job yet, so remove it with `lotar agent queue remove`
- `lotar agent queue` — list the tickets waiting in the queue and whether the queue worker runs
- `lotar agent queue remove <TICKET>` — take one ticket out of the queue before its job starts, for example `lotar agent queue remove DEV-501`
- `lotar agent queue flush` — take every waiting ticket out of the queue; running jobs go on
- `lotar agent list-running`
- `lotar agent list-jobs [-n <LIMIT>]` — list job logs from disk
- `lotar agent check [--status <value>] [--assignee <handle>]`
- `lotar agent worktree list` — list agent worktrees
- `lotar agent worktree cleanup [--all] [--delete-branches] [--dry-run]` — remove stale worktrees

Agent profiles defined in the repository's config files only start after you trust the repository with [`lotar trust`](trust.md), because they pick the program that runs. Profiles from your home config and `--runner` jobs without a profile always start.

## Runners

Each agent profile names a runner. Every runner except `command` is the [harnessmux](https://github.com/localtaskrepo/harnessmux) agent of the same name: LoTaR runs the job as a harnessmux session over the Agent Client Protocol (ACP), which maps the profile's model, effort and permissions onto each agent and reports tool calls, replies, permission requests and usage the same way for all of them.

| Runner | harnessmux agent |
| --- | --- |
| `claude` | Claude Code through its ACP adapter (`claude-agent-acp`) |
| `codex` | Codex CLI through its ACP adapter (`codex-acp`) |
| `copilot` | GitHub Copilot CLI (`copilot --acp`) |
| `gemini`, `cursor`, `droid`, `opencode`, `kilo`, `antigravity` | each CLI's own ACP mode |
| `amp` | Amp's headless stream (Amp has no ACP mode; no resume) |
| `t3` | a [T3 Code thread](#t3-code-threads), through T3's CLI |
| `command` | any program: runs `command` with `args`; the prompt is in `LOTAR_AGENT_PROMPT` |

harnessmux installs the pinned ACP adapters (Claude, Codex) with npm on first use and starts the other CLIs from `PATH`; each CLI keeps its own sign-in. Which agents ran live and which follow their docs, the adapter versions, and how to point an agent at another program are in harnessmux's README and its `agents.toml`; a user `agents.toml` (in `~/.config/harnessmux/`) overrides them without a new LoTaR build. A profile for these runners takes no `command` or `args`: the job is refused with a hint.

A job ends when its session's last turn ends: completed for a normal end, failed when the turn fails or the agent sends a known error (a missing model, a sign-in failure) as its reply.

### Sessions

Each job runs in the harnessmux session `<repo>-<hash>-<ticket>-<profile>` (`lotar-1a2b3c-DEV-7-fix`), or without `-<profile>` for a job without a profile, which lives where the harnessmux CLI keeps its own (`~/.local/state/harnessmux/`). The repository's folder name and a hash of its path keep two checkouts with the same ticket IDs apart. The job log names the session, and `harnessmux attach lotar-1a2b3c-DEV-7-fix` follows it from a terminal. The LoTaR process that runs the job owns the session; a minute after the job's last turn the agent stops, and the session stays resumable.

- `session: new` (the default) and `session: fresh` start a new session under that name, replacing an earlier one nobody runs.
- `session: continue` runs the job as the next turn of the ticket's session of the same profile, so a rework after a review goes back to the `@fix` session with its context. harnessmux resumes the agent's session (`session/resume`, else `session/load`). With no such session, a closed one, one of another runner or repository, or one whose working directory is gone, the job starts a new session and logs why. The continued session keeps its model and effort.

Set `session` on a profile, on a rule's [`agent` settings](automation.md#agent-run-settings), with `lotar agent run --session`, or in the `session` field of `POST /api/jobs`.

#### Context between phases and agents

A ticket's phases often run on different agents (fix on Claude, review on Codex), each in a session of its own. Each job's prompt hands the ticket's earlier work to its agent:

- **Recent context**: the ticket's last 20 prompts and agent replies, from its `.context` file next to the ticket (`agent.context_enabled`, on by default). A job adds its own when it ends, before the rules that start the next phase run, so the next agent sees them.
- **Earlier agent sessions on this ticket**: the ticket's other harnessmux sessions in this repository, each with its agent and the path of its event log, which holds that agent's replies, tool calls and prompts. The agent reads one when the recent context leaves out something it needs.

A continued session also keeps its own history in the agent.

### Messages and questions

A message from the Agents page, `POST /api/jobs/message`, the MCP `agent_send_message` tool, a rule's [`tell_agent`](automation.md#telling-the-tickets-agent) action or `harnessmux send <session>` is queued as the session's next turn; the job ends after the last queued turn, at most 10 turns. Another LoTaR process hands its message to the job's session over harnessmux's socket.

When the agent asks for permission, the profile's `on_permission` decides:

```yaml
agents:
  reviewer:
    runner: claude
    permissions: read-only
    on_permission: ask   # deny | allow-reads | allow | ask
```

- `allow` (the default for `workspace` and `full`) and `allow-reads` (the default for `read-only`) answer without asking; the job log shows the request and the answer.
- `ask` shows the request as a question with the agent's options on the job's card on the Agents page, as the job's `question` over REST and SSE, and as an `agent_job_question` log entry. The turn waits until someone answers (`POST /api/jobs/answer`, or the buttons on the Agents page).

### Permissions

An agent runs without anyone at the keyboard. The profile's `permissions` field picks what it may do; harnessmux turns it into the agent's own mode.

```yaml
agents:
  reviewer:
    runner: claude
    permissions: read-only   # read-only | workspace (default) | full
```

| Level | claude | codex | copilot |
| --- | --- | --- | --- |
| `read-only` | `plan` | `read-only` | plan mode |
| `workspace` | `acceptEdits` | `workspace-write` | agent mode |
| `full` | `bypassPermissions` | `agent-full-access` | autopilot mode |

- `workspace` lets the agent edit the job's working directory. It can also write the tasks directory (to update its ticket) and, in a worktree, the repository's `.git` directory (to commit). A merge job (profile `merge`, or a ticket in `Merging`) gets the main checkout instead of `.git`, so it can fast-forward main. These go to the agent as ACP `additionalDirectories`.
- Commits from inside a sandbox can fail: Codex keeps git metadata read-only, and a signing key agent (1Password, `gpg-agent`, `ssh-agent`) is usually out of reach. LoTaR then commits the agent's changes itself when the job ends (see [Default instructions](#default-instructions)).
- An agent without a mode for a level follows the `on_permission` policy alone; harnessmux logs a note. `command` and `t3` runners get no permissions; a T3 Code thread runs in its project's own runtime mode.

### Model and effort

A profile can pick the model and reasoning effort (and, for `t3`, the provider); a single run can override them with `lotar agent run --model/--effort/--provider`, the `model`/`effort`/`provider` fields of `POST /api/jobs` and the MCP `agent_run` tool, or an automation action's [`agent` settings](automation.md#agent-run-settings).

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

- `model` is the agent's own model id, as it lists it (`harnessmux capabilities claude`); harnessmux refuses one it doesn't list.
- `effort` is `low`, `medium`, `high`, `xhigh` or `max`, mapped to each agent's own values; an agent without effort logs a note. Any other value is refused.
- For `command`, both are passed as given in `LOTAR_AGENT_MODEL` and `LOTAR_AGENT_EFFORT`.

Values may use letters, digits and `. _ - : / @ [ ]`. Jobs show their model and effort in `lotar agent status`, the job list, and on the Agents page.

### Usage and cost

At the end of each turn LoTaR adds what the agent reports to the job's usage: input tokens (with cached), output tokens, turns, and the cost in USD where the agent reports one (Claude). The usage appears in `lotar agent status`, at the end of `lotar agent run --wait`, on the Agents page (per job, plus a cost total per ticket), in the job's `usage` field over REST, SSE and MCP, and in the job log. Automation actions on job events can use `${{agent.usage}}`, `${{agent.cost_usd}}`, `${{agent.input_tokens}}`, `${{agent.output_tokens}}` and `${{agent.turns}}`, for example to comment the cost on the ticket.

`max_budget_usd`, `tools` and `mcp.allow` are not passed: ACP has no budget limit or tool lists. A job whose profile sets them logs a note. Limit an agent with `permissions` and `on_permission`; its own MCP servers follow its own settings.

#### LoTaR's own MCP server

With `mcp.lotar: true`, a job's session also gets LoTaR's MCP server (`lotar mcp`, the same build that started the job, bound to the job's tasks directory) under the name `lotar`, so the agent reads and updates its ticket through tools instead of the CLI:

```yaml
agents:
  implement:
    runner: claude
    mcp:
      lotar: true
```

The server only offers the tools a job needs: `task_get`, `task_list`, `task_update`, `task_comment_add`, `task_reference_add`, `relationship_graph`, `config_show` and `whoami` (set through `LOTAR_MCP_TOOLS`, which `lotar mcp` honours in general). An agent can't start other agents (including by assigning a ticket to an agent profile), delete tasks or sync through it. This limits the MCP tools only: the agent still has a shell and `lotar` on its PATH, so it is not a security boundary. Because the server is named `lotar`, it takes the place of a LoTaR server with that name in the CLI's own settings for the job. `amp`, `t3` and `command` jobs take no MCP servers; they log a note and use the CLI.

### T3 Code threads

The `t3` runner is harnessmux's t3 agent: each session is a T3 Code thread on the T3 Code server running on the same machine, so you follow and answer it in T3 Code like any other thread. Its first turn is `t3 thread start` in the job's T3 project with the prompt on stdin, later turns `t3 thread send`.

```yaml
agents:
  fix:
    runner: t3
    provider: claudeAgent   # a T3 Code provider instance: claudeAgent, codex, …
    model: claude-opus-5-5
```

- `provider` and `model` go together (T3 Code's rule). Leave both out to use the project's default model in T3 Code. A rule, a run or the ticket can pick them per job: `agent: { provider: "${{ticket.field:provider}}", model: "${{ticket.field:model}}" }` falls back to the profile's values when the ticket has no such fields.
- The thread's title is `<ID> <title>`. T3 Code threads no longer create worktrees. Do not rely on starting a thread to create an isolated checkout.
- The thread runs on the T3 Code server, so it doesn't get the job's environment (`LOTAR_TASKS_DIR`, `LOTAR_TICKET_ID`, …). Its "This job" section tells the agent to pass `--tasks-dir "<tasks folder>"` to every lotar command, so a thread in a worktree updates the ticket in the main checkout, not the worktree's copy.
- The job ends when the thread's turn ends, also while T3 Code's `t3_background_wait` keeps the thread working; the job's prompt tells the agent to finish its checks in the foreground first. A thread that waits for an answer fails the job with its link: answer in T3 Code.
- T3's CLI has no effort, permissions, MCP servers or interrupt: cancelling a job stops LoTaR waiting, and the thread runs on until it is stopped in T3 Code.

A profile in your home config (`~/.lotar`) needs no `lotar trust`, so a repository only needs a rule that assigns tickets to it and one that closes them:

```yaml
# .tasks/automation.yml
automation:
  rules:
    - name: Fix reported findings
      when:
        custom_fields: { finding: { exists: true } }   # tickets reported with --finding
      on:
        created:
          set:
            assignee: "@fix"
    - name: Done when the fix finished
      when:
        assignee: "@fix"
      on:
        job_completed:
          set:
            status: Done
            assignee: "@reporter"
```

## Jobs in other processes

A job runs inside the process that started it: `lotar serve`, the background queue worker, an MCP session, or `lotar agent run --wait`. Each running job also leaves a small record next to the agent queue (in `LOTAR_AGENT_QUEUE_DIR`, `$XDG_RUNTIME_DIR/lotar` or your cache directory), so any other `lotar` process for the same tasks directory can:

- list it with `lotar agent list-running` and show it with `lotar agent status <JOB_ID>`
- cancel it with `lotar agent cancel <JOB_ID>`; the owning process stops the agent and finishes the job as cancelled, so `job_cancelled` automation runs
- hold back `lotar status` and `lotar assignee` changes on its ticket until it ends (see [Status and assignee during a job](#status-and-assignee-during-a-job)), and skip its worktree in `lotar agent worktree cleanup`

When an MCP session ends (or reloads after an upgrade), it cancels and finishes the jobs it started instead of leaving agents running unsupervised.

### Jobs whose process died

A job whose process exited before the job ended (a killed or crashed worker or server) no longer counts as running, and its record stays. The queue worker for the same tasks directory (the running one, or the next to start) or the next `lotar serve` there takes it over and ends it as if it had run there: the job log gets its end, the `job_completed`, `job_failed` or `job_cancelled` rules run once, and the [outcome](#status-and-assignee-during-a-job) recorded during the job applies.

- A session job resumes its [session](#sessions) in the new process (its agent stopped with the old one) and asks the agent to continue where it stopped; the job then ends like any other. When the session can't be resumed, the job fails and says why.
- A `command` job fails: its result went with the process.
- The job log notes which process took the job over. A worker that waits for its last jobs before an upgrade takes over none.

`list-running` also lists the `lotar-agent-wrapper` processes (which run `command` jobs) of this tasks directory if that binary is available (next to `lotar`, on `PATH`, or `LOTAR_AGENT_WRAPPER`). A wrapper started for another tasks directory, such as another worktree's, is not listed and doesn't block changes to a ticket with the same ID.

## Status and assignee during a job

While a job runs (or waits in the queue) for a ticket, its status and assignee belong to the job: the job's own lifecycle and its automation rules change them, and nothing else does mid-run. A status or assignee change from anyone else is not refused; it is recorded as the job's **outcome** and applied when the job ends:

```console
$ lotar status QIS-174 HelpNeeded
✅ QIS-174: HelpNeeded will be set when job job-20261111T182920.808Z-1 ends
$ lotar assignee QIS-174 ravi
✅ QIS-174: ravi will be assigned when job job-20261111T182920.808Z-1 ends
```

- The command exits 0 and leaves the ticket as it is; `--format json` adds `"changed": false` and `"deferred": {"job_id", "status", "assignee", "cleared"}`. REST (`/api/tasks/update`, `/api/tasks/status`) answers `deferred` next to `data`, MCP `task_update` adds `deferred` and `message` to the task, and the web UI shows a notice. Other fields (description, tags, comments, links) change at once.
- This holds for every process, agents included: a T3 Code thread gets no `LOTAR_*` variables, so lotar can't tell the agent's own commands from anyone else's. An agent that stops for a decision can set the waiting status and reassign the ticket as the [built-in instructions](agent-instructions.md) say (to whoever the request names for questions, else its reporter), and that is what the ticket ends with.
- When the job ends (completed, failed or cancelled), the `job_completed`, `job_failed` or `job_cancelled` rules run first, without the fields the outcome sets: a recorded status replaces the rule's `set.status`, a recorded assignee its `set.assignee`. The rest of the rule (other `set` fields, `add`, `remove`, `comment`, `run`) still runs. Then the outcome is applied, also when no rule matches, as the change it is: it fires the `updated` rules (and `assigned` when the assignee changes) with `previous.*` the ticket after the job's rules ran, as the same commands outside a job would. Assigning an agent profile there starts that agent.
- The job log then says where the ticket went when its status or assignee changed (`DEV-1 is now Review, assigned to mallox`), so the agent's summary needn't.
- Several changes during one job: the last status and the last assignee win. A change back to the value the ticket already has drops what was recorded for that field, so the job's rules decide it again; the command says so (`STD-37: deferred assignee cleared; the job's rules decide when job job-… ends`), and JSON, REST and MCP list the field in `deferred.cleared`. With nothing recorded it changes nothing. Clearing the assignee (`lotar assignee <ID> --clear`) records a cleared assignee, also on a ticket that has none.
- Automation `run` commands of the job's own events carry `LOTAR_AGENT_JOB_ID` and `LOTAR_TICKET_ID`; their changes are part of the lifecycle and apply at once.
- The outcome lives next to the job's record and goes with it. A job that only a `lotar-agent-wrapper` process shows (no job record) still refuses the change.

## Environment variables

- `LOTAR_AGENT_WRAPPER` — absolute path to the wrapper binary if it is not on `PATH`.
- `LOTAR_AGENT_CONTEXT_EXTENSION` — extension for context files (default `.context`).
- `LOTAR_AGENT_LOGS_DIR` — where job logs go instead of `<tasks dir>/@agent-logs` (relative or absolute; `off` turns logging off).

Every job also gets the ticket's `LOTAR_*` variables (`LOTAR_TICKET_ID`, `LOTAR_TICKET_PROJECT`, `LOTAR_TICKET_REPORTER`, `LOTAR_TASKS_DIR`, the worktree path and branch, …) and `LOTAR_BIN`, the absolute path of the `lotar` that started the job. The built-in instructions call that path (`${{lotar.bin}}`) and its directory is put first on PATH, so an older `lotar` elsewhere (or a CLI that runs commands in a login shell, like Codex) can't answer instead.

## Windows agent commands

A `command` runner's program runs through `lotar-agent-wrapper`. Windows cannot pass arguments with newlines or other special characters through `cmd.exe` safely, so when `command` resolves to a `.cmd`/`.bat` file, the wrapper tries to resolve the shim to the executable it forwards to and runs that directly: npm-generated `cmd` shims are recognized and executed as `node <cli.js>` with the original arguments followed by any baked shim suffix. Batch commands the wrapper cannot recognize keep going through `cmd.exe`. The prompt itself is in `LOTAR_AGENT_PROMPT`, not an argument, so it arrives byte-exact either way.

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

A merge job (profile `merge`, or a ticket in `Merging`) has to end with its branch in the main checkout's current branch. If the agent couldn't fast-forward it (a sandbox may keep git metadata read-only), LoTaR runs `git merge --ff-only <branch>` in the main checkout, which never overwrites local changes. A branch whose work is on main already counts as merged, as described under [Worktrees](#worktrees), and for a merge job also when main differs from it only in the tasks directory (the merge amends ticket changes into the landed commit). So does a branch the agent landed reworked, in a form that conflicts with it: main gained commits since the job started whose subject starts with the ticket ID (`DEV-7 Fix the login`) and that together change every file the branch changes. If the branch still isn't merged, the job fails, so `job_failed` rules run instead of `job_completed`. The other way round, a merge job whose runner fails (the agent crashed, T3 Code was unreachable) after its branch reached main completes when its worktree holds no uncommitted changes: its work is done, so `job_completed` rules run instead of a retry. The job log says why. A merge job that succeeds records the branch's commit as landed in main's current commit, as the git ref `refs/lotar/landed/<branch>/<branch commit>`; cleanup deletes the record with the branch.

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
- You can stop a running job, remove a queued one, or stop all of them. While a job runs you can send it a message, its session's next turn (not for `command` jobs). An agent's permission question shows on its job's card with a button per option (profile `on_permission: ask`).

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
    cleanup_on_done: true         # Remove worktrees when the ticket reaches done
    cleanup_on_failure: false     # Remove worktrees after failed jobs (default: false)
    cleanup_on_cancel: false      # Remove worktrees after cancelled jobs (default: false)
    cleanup_delete_branches: true # Delete worktree branches during cleanup
    cleanup_ignored_files: true   # Git-ignored files (build output) don't block cleanup (default: true)
    cleanup_paths: [target]       # Build output deleted when a job ends; the worktree stays (default: none)
```

Change these on the Config page (Agents) or with `config set`, globally or per project, for example `lotar config set agent_worktree_max_parallel_jobs 2 --global` or `lotar config set agent_worktree_cleanup_on_done true -p APP`. A job limit of `0` pauses new jobs; an empty value removes the limit.

When enabled:
- Each ticket gets a dedicated worktree at `<dir>/<repo>/<ticket-id>`
- A branch is created at `<branch_prefix><ticket-id>` (e.g., `agent/TEST-1`)
- Multiple job phases for the same ticket reuse the same worktree
- The worktree persists after the job completes unless cleanup is enabled
- With `cleanup_on_done`, a ticket that reaches a done status loses the worktree on its ticket branch, whoever set the status: a job, automation, the CLI or the web UI. While a job runs for the ticket, its end does the cleanup. A successful job also removes its own worktree when the ticket is done by then.
- A ticket's worktrees and branches include those of its T3 Code threads: a thread's branch (`t3/<topic>`) that T3 Code created from the ticket branch or from an earlier thread's branch, as git's branch reflog records (`branch: Created from agent/TEST-1`), and a worktree detached from the ticket branch (LoTaR detached them so a new thread could take over the branch, before DEV-503), as its HEAD reflog records (`checkout: moving from agent/TEST-1 to HEAD`). Done cleanup removes each of them that passes the checks below, a detached worktree when its commit's work is in the main checkout, and with `cleanup_delete_branches` also deletes the ticket's branches that no worktree has checked out and whose work landed. Deleting a branch deletes its reflog, so before it does, cleanup records the ticket of each branch and detached worktree traced through that branch (in the branch's `branch.<name>.lotarticket` config, and in a `lotar-ticket-branch` file in the worktree's git directory), and a later cleanup still finds them once their work lands.
- Cleanup only removes a worktree whose ticket is done, whose branch is merged into the main checkout, and that has no untracked or modified files. A branch counts as merged when every commit on it is in the main checkout, also when it landed rebased or cherry-picked (`git cherry` lists no `+` commits), or when it landed reworked: merging the branch into the main checkout's `HEAD`, or into a commit since the branch's fork point that changed the branch's files, adds nothing (`git merge-tree --write-tree` yields that commit's own tree), or when a merge job recorded the branch's current commit as landed and that record's main commit is in the main checkout (see [Default instructions](#default-instructions)). A branch with any change main doesn't hold is kept. So is a branch whose lines the merge changed (main holds other content than the branch) without a merge job's record, or one with commits added after the record; check it and remove it by hand. Git-ignored files such as build and test output are removed with it; set `cleanup_ignored_files: false` to keep any worktree that still holds ignored files. When cleanup keeps a worktree, the job log says why.
- With `cleanup_paths`, every job end (completed, failed, cancelled, or handed back with the ticket open) deletes those paths from the job's worktree, which stays. This also covers existing T3 Code thread worktrees recorded on earlier jobs. Build output such as Rust's `target/` takes gigabytes in each worktree; a resumed job rebuilds it (a shared compiler cache such as sccache makes that fast). A path is deleted only when git ignores it and nothing under it is tracked; symlinks, paths outside the worktree and `.git` are never deleted. Nothing is deleted while another job runs for the ticket, as it shares the worktree. The job log lists what was deleted.
- Merge-profile jobs require worktrees so merges happen on isolated branches
- When worktrees are enabled, setup failure fails the job; the runner never falls back to the repository checkout.
- Git setup, checks, cleanup, and runners discard inherited/profile repository-selection overrides (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_COMMON_DIR`, `GIT_INDEX_FILE`, object/discovery overrides, and `GIT_CONFIG*`). Worktree top-level and common-directory identity are verified. SSH/askpass and author/committer identity environment variables are retained.
- Merge-profile jobs are serialized one-at-a-time even when `max_parallel_jobs` allows more general agent concurrency

### Managing worktrees

List all agent worktrees:
```bash
lotar agent worktree list
```

This shows each worktree with its ticket ID, branch (or `detached from <branch>`), path, and status (active job, done, or in-progress), T3 Code thread worktrees included.

Clean up stale worktrees:
```bash
# Remove worktrees for completed/closed tickets
lotar agent worktree cleanup

# Also delete the associated git branches, and the tickets' landed branches
# without a worktree (T3 Code thread branches included)
lotar agent worktree cleanup --delete-branches

# Remove ALL agent worktrees (except those with active jobs)
lotar agent worktree cleanup --all

# Preview what would be removed
lotar agent worktree cleanup --dry-run
```

Worktrees with active jobs are never removed, even with `--all`. Cleanup refuses dirty worktrees (including untracked or ignored files), unknown Git state, and branches with commits that are not in the repository checkout's `HEAD`; a branch that landed rebased, cherry-picked or reworked counts as merged, as described above. Branches are deleted with `git branch -D`, because `-d` refuses a rebased branch, and only after these checks pass. `--all` broadens ticket selection only; it is not a force-delete option. Refusals are reported and return a nonzero exit status.

Default CLI cleanup and all automatic cleanup require a readable, valid ticket in a configured done state. Missing, malformed, unreadable, or unknown-status tickets are preserved, even with `--all`. Failure/cancellation cleanup settings only enable cleanup when that same done-state and Git safety check succeeds.

The queue worker drains pending entries and waits for dispatched jobs, including finalization, before exiting. It also waits for dispatched jobs if queue/config processing fails. There is no single-poll `--once` mode. Start failures retain the existing bounded retry policy (three retries), with exhausted retries reported as an error after draining; failed runner jobs are terminal rather than automatically rerun. A ticket whose previous job is still running (in any process) is not a failure: the entry waits in the queue until that job ends. When the `lotar` binary the worker was started from is replaced (an upgrade or reinstall), the worker takes no new entries; once its running jobs end it exits and a new worker, started from the upgraded binary, runs the rest of the queue.

Cancelling a dispatched job requests termination immediately, but cancellation automation and reassignment wait until process/output teardown and context persistence finish. The job retains its capacity slot through finalization, so a replacement is queued safely and cancellation automation runs once. Cancelling an undispatched queued job finalizes immediately.

Both output readers are joined before final summaries, persisted status, and slot release. After the runner exits, the output it left in the pipes is read in full, however long its lines take to record, without waiting for a surviving descendant to close inherited streams. Continuous descendant output after that is bounded to five seconds of draining and reported as an output failure rather than keeping the worker alive indefinitely.

## Automation rules

Agent lifecycle automation lives in the automation rules file. Without a rule, a finished job leaves the ticket's status and assignee as they are. See [Automation Rules](automation.md#agent-job-queueing) for a lifecycle example.

A rule's [`tell_agent`](automation.md#telling-the-tickets-agent) action hands the ticket's agent a message, for example a person's comment: a running job gets it as its next turn, an idle ticket starts its agent with it.

## Notes

- Job status is streamed over `/api/events?kinds=agent_job_*` while the server is running. Streamed events carry the same `sequence` (stable per-job identity, positive, strictly monotonic, never reused) and `at` as the matching `GET /api/jobs/logs` entries, so live and snapshot logs can be joined losslessly.
- Context history is persisted to `<project>/<ticket><extension>` files next to each ticket.
- Job events are persisted to `<tasks dir>/@agent-logs/<JOB_ID>.jsonl` by default (`agent.logs_dir` or `LOTAR_AGENT_LOGS_DIR` picks another folder; `off` turns it off). LoTaR writes a `.gitignore` into the folder so logs, which contain prompts and agent output, aren't committed, and keeps the newest 500 job logs. The `.gitignore` is only written when the folder holds nothing but job logs, only `job-*.jsonl` files are ever pruned, and a setting that names the workspace root itself (`agent.logs_dir: .`) uses a `lotar-agent-logs` subfolder.
- `lotar agent status`, `lotar agent logs` and `lotar agent list-jobs` read these files when the job isn't in the current process.
- Assigning a ticket to an agent profile (e.g., `@claude-review`) automatically queues a job for that ticket.
- Review-stage tickets can only be advanced by their reporter when the change would alter assignee or status.
