# Agent Jobs

LoTaR runs agent CLIs (Claude Code, Codex CLI, Copilot CLI, T3 Code and the others listed under [Runners](#runners)) on tickets through [harnessmux](https://github.com/localtaskrepo/harnessmux) sessions and streams their progress over SSE.

## CLI commands

- `lotar agent run <TICKET> <PROMPT> [--runner <RUNNER>] [--agent <profile>] [--model <MODEL>] [--effort <LEVEL>] [--provider <PROVIDER>] [--session new|continue|fresh] [--wait] [--follow] [--timeout-seconds <N>]`
  - `<RUNNER>` is one of the [runners](#runners): `claude`, `codex`, `copilot`, `gemini`, `cursor`, `amp`, `droid`, `opencode`, `kilo`, `antigravity`, `t3` or `command`.
  - Without `--wait` or `--follow`, the job is checked (ticket, profile or runner, settings) and handed to the background queue worker, and the command returns at once with `Queued <TICKET> for <agent>`. Follow it with `lotar agent queue` and `lotar agent list-jobs`. With `--wait`, the job runs in this terminal and the command exits non-zero unless it completes; it returns once the job's end automation (the `job_completed`/`job_failed` rules), context and worktree cleanup are done. With `--format json` it prints the finished job.
- `lotar agent status <JOB_ID>`
- `lotar agent logs <JOB_ID>`
- `lotar agent cancel <JOB_ID>` — stop a job that has started; a ticket still waiting in the queue has no job yet, so remove it with `lotar agent queue remove`
- `lotar agent queue` — list the tickets waiting in the queue, the jobs waiting for their tickets' prerequisites ([Agent job queueing](automation.md#agent-job-queueing)), and the queue worker's state: `active` (a worker process runs; with tickets left it says what they wait for: a free slot under the global or their project's `agent.worktree.max_parallel_jobs`, their ticket's running job, or a [paused queue](#paused-queue)), `stopped` (tickets wait but no worker process runs) or `idle` (nothing queued). While the machine is short of memory the reason says so and since when ([memory gate](#memory-gate)). With `--format json` the state is in `worker_state`, the explanation in `reason`, a memory hold in `memory_hold` (`reason`, `since`) a failed memory reading in `memory_error`, an open pause in `pause` and the jobs waiting for prerequisites in `waiting_for_prerequisites`. Jobs waiting for prerequisites don't count as queued: they need no worker until their prerequisites are done
- `lotar agent queue resume` — start the queue worker for the waiting tickets without queueing a job, for example after the worker was killed. With a worker running or nothing queued it changes nothing. A worker that can't start is reported (`error` in JSON, the state stays `stopped`); the command fails only when the queue can't be read. `lotar serve` does the same when it starts and prints a warning if the worker can't start
- `lotar agent queue remove <TICKET>` — take one ticket out of the queue before its job starts, also a job that waits for prerequisites, for example `lotar agent queue remove DEV-501`
- `lotar agent queue flush` — take every waiting ticket out of the queue, also jobs that wait for prerequisites; running jobs go on
- `lotar agent list-running`
- `lotar agent list-jobs [-n <LIMIT>]` — list job logs from disk. A [paused queue](#paused-queue) is named first (`pause` in JSON)
- `lotar agent check [--status <value>] [--assignee <handle>]`
- `lotar agent worktree list` — list agent worktrees
- `lotar agent worktree cleanup [--all] [--delete-branches] [--dry-run]` — remove stale worktrees
- `lotar agent worktree run <TICKET> -- <command> [args...]` — run a command in the ticket's worktrees while no job or T3 Code thread works there, for example `lotar agent worktree run DEV-7 -- cargo clean --workspace` (see [Build output while a ticket waits](#build-output-while-a-ticket-waits))

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
| `command` | any program: runs `command` with `args`; the prompt is in the file `LOTAR_AGENT_PROMPT_FILE` names, and in `LOTAR_AGENT_PROMPT` up to 128 KiB |

harnessmux installs the pinned ACP adapters (Claude, Codex) with npm on first use and starts the other CLIs from `PATH`; each CLI keeps its own sign-in. Which agents ran live and which follow their docs, the adapter versions, and how to point an agent at another program are in harnessmux's README and its `agents.toml`; a user `agents.toml` (in `~/.config/harnessmux/`) overrides them without a new LoTaR build. A profile can also set the program for its own jobs: `command` (with `args`) replaces the agent's command, and the pinned adapter with it, as a `command` in `agents.toml` does; `args` alone go after the agent's command. The agent keeps its option ids and permission modes from the table.

The agent's reply shows in the job log while it writes it: each finished paragraph (outside a code block) as it arrives, the rest when a tool starts, the agent asks for permission or the turn ends.

A job ends when its session's last turn ends: completed for a normal end, failed when the turn fails, the agent sends a known error (a missing model, a sign-in failure) as its reply, or the job's cost passes the profile's [`max_budget_usd`](#usage-and-cost).

### Stalls and tickets that move on

A job ends early in two more cases, for every runner:

- **Stalled.** With `stall_timeout_seconds` in the profile, a job whose agent sends nothing for that long (no session event; for `command`, no output line) has its turn cancelled (its process stopped) and fails with `Stalled: the agent sent nothing for <N>s …`, so `job_failed` rules decide, for example a retry with `@fix` or a question to a person. A question waiting for an answer is not a stall. Unset or `0`: no limit.
- **The ticket moved on.** A job whose ticket goes to a done status (`Done`, `WontDo`) or is assigned to a person while it runs, also as a change recorded for the job's end (see [Status and assignee during a job](#status-and-assignee-during-a-job)), is cancelled when it still runs two minutes later, so `job_cancelled` rules run and the recorded change applies. The job log says so when it sees the change (`DEV-1 went Done while the job ran; LoTaR cancels the job unless the agent ends its turn within 120s.`) and when it cancels. The two minutes let the job's own agent, which sets the ticket Done or hands it back as its last step, end its turn as usual. An assignment to an agent profile (`@fix`) or an agent's name doesn't count, nor does the status or assignee the job started with (a job on a ticket that was Done already runs on). Merge jobs don't stop this way: their ticket goes Done as they land.

### Sessions

Each job runs in the harnessmux session `<repo>-<hash>-<ticket>-<profile>` (`lotar-1a2b3c-DEV-7-fix`), or without `-<profile>` for a job without a profile, which lives where the harnessmux CLI keeps its own (`~/.local/state/harnessmux/`). The repository's folder name and a hash of its path keep two checkouts with the same ticket IDs apart. The job log names the session, and `harnessmux attach lotar-1a2b3c-DEV-7-fix` follows it from a terminal. The LoTaR process that runs the job owns the session; a minute after the job's last turn the agent stops, and the session stays resumable.

- `session: new` (the default) and `session: fresh` start a new session under that name, replacing an earlier one nobody runs.
- `session: continue` runs the job as the next turn of the ticket's session of the same profile, so a rework after a review goes back to the `@fix` session with its context. A profile without a session on the ticket takes the ticket's newest session instead, whatever profile ran it: a merge after a review continues the reviewer's session. harnessmux resumes the agent's session (`session/resume`, else `session/load`). With no such session, a closed one, one of another runner or repository, or one whose working directory is gone, the job starts a new session and logs why. A session whose last turn ended `continue_within_minutes` (profile, default 55) or more ago isn't resumed either: the agent's prompt cache (one hour) has expired by then, and resuming would write the whole session to the cache again. The job starts a new session of its own profile instead, the old T3 Code thread stays as it is, and the prompt ends with a handoff: the session it didn't continue with its event log (and its T3 Code thread for the `t3` runner), the ticket branch and its newest commit, and the ticket's newest 3 comments (each cut at 1,500 characters). When the new session takes the skipped one's name, LoTaR first keeps the skipped session, closed, as `<name>.<last used>` (`lotar-1a2b3c-DEV-7-fix.20261115T190000Z`), so the handoff and later jobs' earlier sessions still point at its event log; if it can't, the job fails and starts no session. A session the agent refuses to resume or load (Claude never wrote its transcript: `Resource not found: sessionId`) gets the same: the job keeps it, starts a new session under its name with the handoff, and logs `This job would have continued the session <name>, but it could not be resumed (<error>), so it starts a new one with a handoff.` Other start errors (the agent's program is missing, a file or socket fails) still fail the job, and so does a `t3` session that can't be resumed, as its thread may still run in T3 Code. The job log says either `Continuing the session <name>, last used <N> minutes ago (within continue_within_minutes, <M>).` or `The session <name> was last used <N> minutes ago, not within continue_within_minutes (<M>); this job starts a new one with a handoff.` The job's own profile sets the time, also for a merge that continues the reviewer's session; `0` never resumes. A session another lotar process still owns (the `agent run` whose end rule started this job) is resumed once that process lets it go. The continued session keeps its model and effort, whatever the job's profile or `--model`/`--effort` say, and the job shows those (job list, `lotar agent status`, timeline, Agents page; until the job picks its session, its profile's); its agent runs with this job's environment: an agent still open for the earlier job in the same process stops, and the session resumes.

Set `session` on a profile, on a rule's [`agent` settings](automation.md#agent-run-settings), with `lotar agent run --session`, or in the `session` field of `POST /api/jobs`.

#### Context between phases and agents

A ticket's phases often run on different agents (fix on Claude, review on Codex), each in a session of its own. Each job's prompt hands the ticket's earlier work to its agent:

- **Comments**: the ticket's open question and its answer, and the newest fix and review comments, each cut at 1,500 characters, with the count of all comments and the commands that show them (`lotar show`) and the history (`lotar task timeline`). The open question is the newer of the newest comment written before the ticket was last assigned to a person and not by that person, and the newest comment with a question. Its answer is the first later comment that answers a question or is written by the person the question's phase handed the ticket to; with one the prompt labels it `Question`, without one `Open question`. When no comment is picked (none has a question, none was written before the ticket was last assigned to a person except by that person, and none but automation's was written in a fix or review phase, as with the owner's notes on a new ticket before its first fix job), the section is one line with the count and the two commands. A comment's phase is the profile the ticket was last assigned to when it was written (an agent working on after a hand-back continues that phase): `@review…` is a review, `@merge…` neither, any other profile a fix; automation's comments are never the newest fix or review comment, but can be the open question.
- **Recent context**: the newest 2 of the ticket's stored prompts and agent replies (the previous job's), each cut at 1,500 characters, from its `.context` file next to the ticket (`agent.context_enabled`, on by default; it keeps the last 20). A job adds its own when it ends, before the rules that start the next phase run, so the next agent sees them. A profile's `context_entries` sets how many the prompt carries (`0`: none); `full_history: true` brings back the prompt before DEV-620: all 20, uncut, and no comments, whatever `context_entries` says.
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
| `workspace` | `acceptEdits` and Claude's OS sandbox | `workspace-write` | agent mode |
| `full` | `bypassPermissions` | `agent-full-access` | autopilot mode |

- `workspace` lets the agent edit the job's working directory. It can also write the tasks directory (to update its ticket) and, in a worktree, the repository's `.git` directory (to commit). A merge job (profile `merge`, or a ticket in `Merging`) gets the main checkout instead of `.git`, so it can fast-forward main. These go to the agent as ACP `additionalDirectories`.
- At `workspace` the job also refuses `git push` and edits outside those directories, whatever the agent: harnessmux answers the agent's permission requests by these rules before `on_permission`. The command line is read as a shell reads it, so a push is refused also with git's options before it (`git -C "/tmp/my repo" push`, `git -c key=value push`), through `env` or `sudo`, or inside `sh -c "…"` or `$(…)`; a command the line builds while it runs (a program from a variable) isn't seen, so these rules are a best-effort layer. What stops a push for sure is Claude's OS sandbox: its shell commands can't write outside those directories, can't use SSH and can't reach github.com, gitlab.com or bitbucket.org, so a workspace Claude job can't fetch from those hosts either. The sandbox has no unsandboxed retry; each command still asks, so the rules see it, and Claude also gets `git push` as a denied tool (`Bash(git push:*)`). Codex runs in its own `workspace-write` sandbox. Other agents have only the rules.
- The profile's `tools.allowed` and `tools.disallowed` list tools by the agent's own name for them (a trailing `*` matches a prefix), and `mcp.allow` names MCP servers whose tools are allowed (`mcp__<server>__*`). harnessmux answers permission requests for them before `on_permission`; Claude also gets them as its own `allowedTools` and `disallowedTools`, so they apply to tools it runs without asking.
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
- `effort` is `low`, `medium`, `high`, `xhigh` or `max`, mapped to each agent's own values; an agent without effort logs a note. Any other value is refused: set an agent's own value (Codex's `ultra`, Copilot's `minimal`) in `options`.
- `options` sets the agent's own ACP config options, id to value, after `model` and `effort`. `harnessmux capabilities <agent>` lists the ids and values; an id or value the agent doesn't offer fails the job. The `command` runner refuses `options`.

```yaml
agents:
  deep:
    runner: codex
    options:
      reasoning_effort: ultra
```
- For `command`, both are passed as given in `LOTAR_AGENT_MODEL` and `LOTAR_AGENT_EFFORT`.

Values may use letters, digits and `. _ - : / @ [ ]`. Jobs show their model and effort in `lotar agent status`, the job list, and on the Agents page.

### Usage and cost

At the end of each turn LoTaR adds what the agent reports to the job's usage: input tokens (with cached), output tokens, turns, and the cost in USD where the agent reports one (Claude). Agents report the session's cost so far; a job's cost is what its turns added to it. Turns are the job's turns in its session. Copilot's premium requests and changed lines and Claude's model round trips are not reported over ACP, so jobs no longer show them; jobs from before the switch to harnessmux keep theirs. The usage appears in `lotar agent status`, at the end of `lotar agent run --wait`, on the Agents page (per job, plus a cost total per ticket), in the job's `usage` field over REST, SSE and MCP, and in the job log. Automation actions on job events can use `${{agent.usage}}`, `${{agent.cost_usd}}`, `${{agent.input_tokens}}`, `${{agent.output_tokens}}` and `${{agent.turns}}`, for example to comment the cost on the ticket.

With `max_budget_usd` in the profile, LoTaR cancels the agent's turn and fails the job as soon as the job's cost passes it, for every agent that reports cost. A job whose agent reports none logs that the budget was not checked.

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

The `t3` runner is harnessmux's t3 agent: each session is a T3 Code thread on the T3 Code server running on the same machine, so you follow and answer it in T3 Code like any other thread. Its first turn starts the thread in the job's T3 project: in the ticket's worktree through T3 Code's MCP tool `t3_thread_launch` (harnessmux signs in to T3 Code's MCP server once, with a pairing code it makes itself), without worktrees `t3 thread start` with the prompt on stdin. Later turns go to the same thread.

```yaml
agents:
  fix:
    runner: t3
    provider: claudeAgent   # a T3 Code provider instance: claudeAgent, codex, …
    model: claude-opus-5-5
```

- `provider` and `model` go together (T3 Code's rule). Leave both out to use the project's default model in T3 Code. A rule, a run or the ticket can pick them per job: `agent: { provider: "${{ticket.field:provider}}", model: "${{ticket.field:model}}" }` falls back to the profile's values when the ticket has no such fields.
- A new thread's title is the profile's `thread_name`, or a rule's `agent.thread_name`, which wins, with the ticket's templates filled in when the job starts: `thread_name: "${{ticket.id}} fix: ${{ticket.title}}"` names it `DEV-7 fix: Crash on save`. Without one, or when it comes out empty, the title is `<ID> <title>`. With a `thread_name`, a continued thread (`session: continue`) is renamed to it before its turn is sent, and the job log says `Renamed the T3 Code thread <thread> to "<name>".` T3's CLI can't rename a thread, so lotar takes the T3 Code server's internal API, which T3 Code doesn't document: it issues a two-minute `orchestration:operate` session with `t3 auth session issue`, sends a `thread.metadata.update` command over `/ws` and revokes the session. A rename that fails (server not running, no answer within 10 seconds, the API changed) leaves the name, the job log says `Could not rename the continued T3 Code thread to "<name>", so it keeps its name: <why>`, and the job runs on. After a T3 Code update, check that a continued job's log still says `Renamed`, and whether `t3 thread --help` now lists a rename command to use instead. The job links the thread on the ticket (a link reference).
- With `agent.worktree.enabled`, the thread runs in the ticket's worktree on the ticket branch, the one every other runner's job on the ticket uses (`<dir>/<repo>/<ticket-id>` on `agent/<TICKET>`, see [Worktrees](#worktrees)). LoTaR makes the worktree and starts the thread in it (T3 Code's `t3_thread_launch` with an existing worktree), so T3 Code makes no worktree or branch of its own and only shows the thread. What the agent leaves uncommitted is committed to the ticket branch when the job succeeds, as for any worktree job. Without worktrees the thread runs in the main checkout.
- A job that continues a thread (`session: continue`) sends its turn to that thread, which works in the ticket's worktree, so a review or merge that continues an earlier thread reads the latest rework. A thread of a job before DEV-652 ran in a worktree T3 Code made; a job that would continue it starts a new thread in the ticket's worktree instead, with a handoff from the old one, and its log says `The session <name>'s T3 Code thread runs in a worktree T3 Code made; this job starts a new thread in the ticket's worktree, with a handoff.`
- A completed job settles its thread in T3 Code (`t3_thread_organize`) after its end rules ran, so T3 Code's sidebar keeps only the threads that still need someone; the job log says `Settled the T3 Code thread <link>`. The thread stays active when the job failed or was cancelled, or when the ticket waits for a person: it is `HelpNeeded`, or it is open and assigned to someone other than an agent profile (`@<profile>`); the log says `The T3 Code thread stays active: <ID> is HelpNeeded.` (or `is <status>, assigned to <name>`). When T3 Code refuses (the thread still runs or waits, or the server doesn't answer), the job log says `The T3 Code thread was not settled: <reason>` and the job's outcome stays as it is. A thread kept active this way is settled later, once its ticket is in a done state (whichever way it got there) or once the thread of a later job on the ticket is settled: LoTaR checks as each job ends, and in the look `lotar serve` and the queue worker run at their start and every 30 seconds (see below), so threads kept before this check existed are settled too. It settles only a thread `t3 thread list` shows unsettled with status `completed`, so a thread that still runs or waits, or one a job follows, is left alone. The log of the newest job that ran the thread says `Settled the T3 Code thread <link>`, or once per process `The T3 Code thread <link> was not settled: <reason>` (its id when T3 Code lists no link).
- Of the profile's settings, a thread gets `provider`, `model` and, through its worktree's Claude settings, `tools.disallowed` and `env`: T3's CLI takes no Claude settings, and T3's Claude sessions read the worktree's `.claude/settings.local.json`. Before a new thread starts in the ticket's worktree, a job whose profile sets `tools.disallowed` or `env` and runs a Claude provider (`claudeAgent`, any instance whose name contains `claude`) or none adds `tools.disallowed` to `permissions.deny` and `env` to `env` in that file. Keys already in the file stay; the profile's env values win. When no ignore rule covers the file, it goes into the repository's `.git/info/exclude`, so no commit takes it; the worktree's `.gitignore` stays as it is. Deny the claude.ai connectors by name (`mcp__claude_ai_Gmail`, …): `ENABLE_CLAUDEAI_MCP_SERVERS` works only as a process variable, which a thread doesn't get. The job log says `Wrote .claude/settings.local.json in <worktree> …`, or why not (a tracked or invalid settings file). Without worktrees nothing is written, so the main checkout's file stays as it is; a continued thread keeps its worktree and file.
- The thread runs on the T3 Code server, so it doesn't get the job's environment (`LOTAR_TASKS_DIR`, `LOTAR_TICKET_ID`, …). Its "This job" section tells the agent to pass `--tasks-dir "<tasks folder>"` to every lotar command, so a thread in a worktree updates the ticket in the main checkout, not the worktree's copy.
- The job ends when the thread's work ends: when the agent waits on background work with T3 Code's `t3_background_wait` and ends its turn, the job ends after the run that work wakes, with that run's reply (T3 Code 0.1.10 and later). T3's CLI looks for that run for up to 10 seconds after the background work ends, so a wake run that starts later ends the job with the earlier turn's reply. A thread that waits for an answer hands the ticket back to its reporter (status `HelpNeeded` where the project has it) with a comment that links the thread, and the job fails: answer in T3 Code.
- A `t3` that exits with an error while the thread still runs, or because the T3 Code server doesn't answer ("Could not reach the T3 Code server"), doesn't fail the job: harnessmux (0.1.3) follows the thread's run, waits up to 60 minutes for the server, and the job ends from the thread's state.
- T3's CLI has no effort, permissions, MCP servers or interrupt: cancelling a job stops LoTaR waiting, and the thread runs on until it is stopped in T3 Code.
- A thread can run on after its job ended: T3 Code resumes a turn it paused for a usage limit (the job [paused](#usage-limits) on it), and an answer in the thread starts a new turn. While T3 Code still runs a thread of an earlier job on the ticket (its status is `running` or it has an `activeRunId`), or the thread waits for an answer, no other job on the ticket starts, as a second thread would work beside it in the same worktree. The queue keeps the entry until the thread ends and comments on the ticket once which thread holds it (for a waiting thread: answer it in T3 Code). `lotar agent run --wait` fails with `Ticket '<ID>' already has an active job: the T3 Code thread <thread> of the job <job> still runs` (or `waits for an answer`). A thread T3 Code can't show holds nothing. LoTaR asks T3 Code only when the ticket has no active job, and keeps the answer, running or not, for 30 seconds, until a later job on the ticket ends.
- Such a thread's outcome reaches the ticket: LoTaR takes it over as a job. The queue worker (while it runs) and `lotar serve` look every 30 seconds for threads of the tasks directory's ended `t3` jobs that T3 Code runs again (`running` in `t3 thread list`); they call `t3` only while an ended `t3` job has a thread. After a restart, T3 Code resumes every turn the restart cut off, outside any job. On an open ticket with no active job (in any process) and no queued entry other than a usage-limit retry, the thread of the ticket's newest job is taken over as a new job of that job's profile: the ticket goes back to `@<profile>` (with no assignment rules), its `job_started` rules run, and a comment names the thread, the old job and the new one (`T3 Code runs the thread <link> of the job <old> again, with no lotar job … lotar takes it over as the job <new>: DEV-1 goes back to @fix, and the job's end rules run when the thread's run ends.`). The job follows the thread's run without sending a turn and ends like any job, so its end rules run once. It counts toward `max_parallel_jobs`, so queued jobs wait for it, and it runs even when the limit is full: T3's CLI can't stop a thread, and taking it over keeps its work instead of redoing it. Its record lets another worker or server take it over in turn. One process at a time looks at a ticket, and each run of a thread is handled once.
- A ticket has one running thread a job follows. T3 Code may resume several of a ticket's threads (a fix, a merge); the others, and a thread that runs beside the ticket's active job, get one comment each per run: `T3 Code runs the thread <link> of the job <old> again, beside <the thread … that the job … took over | the running job … | the thread … that waits for an answer>. lotar can't stop a T3 Code thread (T3's CLI has no interrupt) and takes over only one per ticket: stop it in T3 Code.` A thread that waits for an answer (`pendingRequestCount` above 0 in `t3 thread show`, or `waiting` in `t3 thread list`) holds the ticket as above and is left alone, and no other thread of the ticket is taken over while it waits: the others get the comment; so is a ticket with a queued entry (the entry waits for the thread; a job's retry after a [usage limit](#usage-limits) doesn't), a done ticket's thread, and the thread of a cancelled job (it runs on until it is stopped in T3 Code). When T3 Code can't list its threads, nothing happens and the next look tries again; when the profile is gone or no longer runs on `t3`, or the thread has no session, the ticket gets a comment that says why. A taken-over job whose session can't be resumed fails with the reason, and its `job_failed` rules run.

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

- A session job resumes its [session](#sessions) in the new process (its agent stopped with the old one) and asks the agent to continue where it stopped; the job then ends like any other. A `t3` job's thread kept running in T3 Code, so the job follows the thread's run to its end (or takes the result of a run that ended meanwhile) without sending the agent another turn. A process that died right after T3 Code made the thread, before its id was saved, still leads to that thread: harnessmux finds it by its title. When T3 Code can't list its threads then, the job fails and starts no thread, as that could be a second one; the session looks again at its next turn. When the session can't be resumed (its agent kept no record of it: `Resource not found: sessionId`), the job ends paused, says why, and runs again first in the queue, in a new session (`session: fresh`), with the status and assignee change it recorded; its `job_paused` rules run, not `job_failed`. A job lotar took over from a T3 Code thread, or one an older build started, has nothing to run again and fails as before.
- A `command` job fails: its result went with the process.
- A job whose process exited after it started but before its runner started (while its `job_started` rules ran, for example) fails with "The lotar process running the job exited before the job ended, before it started the <runner> session." A session job doesn't start or resume a session then, also when its profile continues an earlier job's session.
- A job whose process exited while it set up the job's worktree, before the job started (any runner), fails with "The lotar process running the job exited while it set up the job's worktree, before the job started." It had no job log yet; the takeover writes one, so `agent status` and `list-jobs` show the failure. A worktree setup that fails on its own is in the job log too.
- The job log notes which process took the job over. A worker that waits for its last jobs before an upgrade takes over none.
- A taken-over job runs as the kind it started as: a review job stays a review job when the ticket is Merging by then, and only a job that started as a merge job gets the merge check, on the branch its job log names.
- A job whose log already has its end (its process exited after that, before it dropped its record) is not taken over: only its record goes, and its log, rules and ticket stay as they are.

### Paused queue

Some failures would fail every job like it: commit signing that can't reach its key, a full disk. A job that fails on one of them doesn't fail: it ends as `paused` (job status, log status line and the event `agent_job_paused`), runs its `job_paused` rules instead of `job_failed` ones and leaves its ticket's status and assignee as they are. The queue pauses instead:

- **What pauses it.** LoTaR reads its own note on the failure (its commit of the agent's leftover work, the worktree setup, the runner's error), the agent's last message and the job's last 10 events, not the whole log. Signing: `Couldn't find key in agent`, `agent refused operation`, `cannot run gpg`, `signing failed`, `gpg failed to sign`, `failed to sign the data`. Full disk: `No space left on device`, `ENOSPC`, `os error 28`. When both show, the full disk wins.
- **What waits.** A signing pause holds the jobs that commit: with `agent.worktree.enabled`, every `t3` job and every other job whose profile isn't `permissions: read-only`. A job that doesn't commit by that rule and fails on signing fails as before. A disk pause holds every job. Held queue entries stay queued; a job started in `lotar serve` waits as `queued` and says why until the pause ends.
- **Retry.** The paused job's retry goes first in the queue. Every 10 minutes (`LOTAR_AGENT_PAUSE_RETRY_SECONDS` sets another interval) the queue worker starts one held entry, the first, once it has a free slot and the job that paused the queue or took the last try has ended: a try that outlasts the interval holds the next one back. A job held in `lotar serve` takes that try only when no held queue entry waits for it, so a pause whose retry left the queue (it failed another way, was cancelled or removed) still ends. When the paused job failed at LoTaR's commit of its work, the worktree stays as it is (no cleanup, no reset) and its retry runs no agent: it commits the work in that worktree and then ends as the job would have, with `job_completed` rules. A worktree that is gone by then runs the agent's task again; so does a job that failed before its work was done. A disk pause still deletes the job's build output (`agent.worktree.cleanup_paths`), which frees space.
- **The ticket.** A status or assignee change made during the paused job (an agent's hand-back, see [Status and assignee during a job](#status-and-assignee-during-a-job)) isn't applied at the pause: its retry takes it over and applies it when it ends, like its own. A field the ticket changed in during the pause (you set it meanwhile) keeps your value. The retry's log says what it took over or dropped.
- `lotar agent run --wait` returns when its job pauses, with a non-zero exit (`ended with status paused`).
- **Resume.** The first job the pause holds that completes ends it, and the held jobs start as slots allow. A retry that fails the same way pauses again for another interval; one that fails another way runs the `job_failed` rules, and the pause stays.
- The pause is kept in the queue's state file (next to the job records), so the server, the queue worker and later processes see it, also after a restart. `lotar agent queue` and `lotar agent list-jobs` show the reason, since when, the next try and the job that paused it.

### Usage limits

A job that stops on its provider's usage limit doesn't fail either: it pauses until the limit resets, and only that job waits.

- **What counts.** Only the runner's own report of a failed turn (harnessmux's turn failure, such as `t3 exited with exit status: 1: …` with T3's error output), not the agent's messages or tool calls: a job whose agent talked about a rate limit and then failed for another reason fails. harnessmux gives the failure a kind, from the agent's error data (claude-agent-acp's `errorKind`, codex-acp's error info) or else its text: `rate_limited` and `quota` are limits. A failure of any other kind (`other`, or a sign-in, overload, context or connection failure) is read from its text, since harnessmux sorts a text that names both a sign-in error and a limit as a sign-in failure: a line counts when it names `usage limit`, `rate limit`, `hit your limit`, an `-hour limit` or `weekly limit`, `quota exceeded`, `exceeded your current quota`, `insufficient_quota`, `too many requests` or `try again at`. A limit wins over any other failure in the same output, a full disk or a signing failure included.
- **When it runs again.** At the reset time harnessmux reports with the failure (`resets_at`, from the agent's error data), else the one its text names: `try again at Nov 20th, 2026 7:43 PM` (Codex, read in the machine's time zone), `resets 3pm (Europe/Berlin)` (Claude; a time already past today means tomorrow), an RFC 3339 time, a Unix time after `|`, or `try again in 20 minutes`; at least a minute from the pause. A line with no time (`Claude API rate limit reached. Try again later.`) waits an hour, or `LOTAR_AGENT_USAGE_LIMIT_WAIT_SECONDS`.
- **The job.** It ends as `paused` (job status, log status line with `resumes_at`, the event `agent_job_paused`) with a note that names the provider, the line and the time, and runs its `job_paused` rules, not `job_failed` ones: nobody is asked. Its ticket keeps its status and assignee, and its worktree stays. Its retry goes first in the queue on the same profile, model and settings, with a `not_before` time; other jobs run meanwhile. A status or assignee change the agent made is carried to the retry as for a [paused queue](#paused-queue).
- **Again.** A retry that hits the limit again pauses again, as often as it happens. One that completes or fails runs its `job_completed` or `job_failed` rules.
- **T3 Code resumes the turn.** When T3 Code itself resumes the paused job's thread after the reset, and the ticket's only queued entries are such retries, the thread is [taken over](#t3-code-threads) as a job (with the status and assignee change the retry carried) and the retries leave the queue, so the work doesn't run twice. Any other queued job still waits for the thread.
- `lotar agent list-jobs` shows the job as `paused (until <time>)` (JSON: `resumes_at`); `lotar agent queue` shows the entry's `not_before` and, while every entry waits for a reset, `waiting for a provider's usage limit to reset`. `lotar agent queue remove <ID>` drops the retry.

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
- This holds for every process, agents included: a T3 Code thread gets no `LOTAR_*` variables, and the job's other agents get the job's but an empty `LOTAR_AUTOMATION_EVENT`, so their commands are treated like anyone else's. An agent that stops for a decision can set the waiting status and reassign the ticket as the [built-in instructions](agent-instructions.md) say (to whoever the request names for questions, else its reporter), and that is what the ticket ends with.
- When the job starts, before its runner, its `job_started` rules run. Each `run` they start leaves a line in `lotar agent logs <job>`: ``Automation rule '<name>' ran `<command>` on <ID>: exit 0 in 1m43s.`` with the last 10 lines of its output. A non-zero exit that `ignore_failure` lets pass says ``failed (exit 1) in 12s, ignored (ignore_failure)``; a run that fails or outlives its `timeout` gives the error and how long it ran, and the job still starts. In an untrusted repository the line says ``Automation rule '<name>' skipped its run on <ID>: this workspace isn't trusted (`lotar trust`).`` A failure of the rules themselves is logged as `The start rules for <ID> failed: <error>`. The end rules' runs below get the same lines.
- When the job ends (completed, failed or cancelled), the `job_completed`, `job_failed` or `job_cancelled` rules run first, without the fields the outcome sets: a recorded status replaces the rule's `set.status`, a recorded assignee its `set.assignee`. The rest of the rule (other `set` fields, `add`, `remove`, `comment`, `run`) still runs. Then the outcome is applied, also when no rule matches, as the change it is: it fires the `updated` rules (and `assigned` when the assignee changes) with `previous.*` the ticket after the job's rules ran, as the same commands outside a job would. Assigning an agent profile there starts that agent. When many jobs end at once and the ticket's storage lock stays busy, these writes run again for a few seconds; a rule or outcome that still fails is logged on the job (`lotar agent logs <job>`). End-rule actions that still find the lock busy are kept too: the worker or server resumes the uncompleted parts (ticket changes, comment, command, agent message, archive), keeping completed parts from running again. A step that fails on a full disk (`ENOSPC`) is kept the same way. When a full disk refuses even the checkpoint (or its lock file), none of the end rules ran: the process that ended the job keeps them in memory and tries every 30 seconds to save the checkpoint and run them, and a queue worker stays up while it holds any. They are lost if that process exits first. Matching rules share one ordered checkpoint, so a later rule waits for every unfinished step of the earlier rules. The outcome waits behind that checkpoint too, and behind end rules kept in memory: it is applied after the last kept step, so `previous.*` is still the ticket after all of the job's rules ran. An outcome that still finds the lock busy is kept, and the queue worker for the same tasks directory or the next `lotar serve` there applies it once the lock is free; so does one whose process exited before applying it.
- A ticket set Done or assigned to a person while its job runs cancels the job two minutes later, as [Stalls and tickets that move on](#stalls-and-tickets-that-move-on) says.
- A ticket reassigned while its job ran, by a change that doesn't wait for the job's end (a rule the agent started with `lotar automation run`, for example one that sends the ticket to review), is in its next stage already: the job's end rules don't run, so they don't move it a second time, and `lotar agent logs <job>` says so. A recorded outcome still applies. Jobs taken over from an older lotar, whose record has no starting assignee, run their end rules as before.
- A hand-back to the ticket's reporter when that reporter is an agent name (a runner such as `claude` or `codex`, not a person) reaches no one, since nothing picks up a ticket assigned to it. When one of the job's end rules sets an assignee, that recorded status and assignee are dropped and the rule decides: a `job_failed` rule that retries with `@fix` still retries. A hand-back to a person still wins.
- The job log then says where the ticket went when its status or assignee changed (`DEV-1 is now Review, assigned to mallox`), so the agent's summary needn't.
- Several changes during one job: the last status and the last assignee win. A change back to the value the ticket already has drops what was recorded for that field, so the job's rules decide it again; the command says so (`STD-37: deferred assignee cleared; the job's rules decide when job job-… ends`), and JSON, REST and MCP list the field in `deferred.cleared`. With nothing recorded it changes nothing. Clearing the assignee (`lotar assignee <ID> --clear`) records a cleared assignee, also on a ticket that has none.
- Automation `run` commands of the job's own events carry `LOTAR_AUTOMATION_EVENT`, `LOTAR_AGENT_JOB_ID` and `LOTAR_TICKET_ID`; their changes are part of the lifecycle and apply at once.
- The outcome lives next to the job's record and goes once it is applied. A job that only a `lotar-agent-wrapper` process shows (no job record) still refuses the change.

## Environment variables

- `LOTAR_AGENT_WRAPPER` — absolute path to the wrapper binary if it is not on `PATH`.
- `LOTAR_AGENT_CONTEXT_EXTENSION` — extension for context files (default `.context`).
- `LOTAR_AGENT_LOGS_DIR` — where job logs go instead of `<tasks dir>/@agent-logs` (relative or absolute; `off` turns logging off).
- `LOTAR_AGENT_PAUSE_RETRY_SECONDS` — how long a [paused queue](#paused-queue) waits before it tries one held job again (default 600).
- `LOTAR_AGENT_USAGE_LIMIT_WAIT_SECONDS` — how long a job paused on a [usage limit](#usage-limits) that names no reset time waits (default 3600).

Every job also gets the ticket's `LOTAR_*` variables (`LOTAR_TICKET_ID`, `LOTAR_TICKET_PROJECT`, `LOTAR_TICKET_REPORTER`, `LOTAR_TASKS_DIR`, the worktree path and branch, …) and `LOTAR_BIN`, the absolute path of the `lotar` that started the job. The built-in instructions call that path (`${{lotar.bin}}`) and its directory is put first on PATH, so an older `lotar` elsewhere (or a CLI that runs commands in a login shell, like Codex) can't answer instead.

## Windows agent commands

A `command` runner's program runs through `lotar-agent-wrapper`. Windows cannot pass arguments with newlines or other special characters through `cmd.exe` safely, so when `command` resolves to a `.cmd`/`.bat` file, the wrapper tries to resolve the shim to the executable it forwards to and runs that directly: npm-generated `cmd` shims are recognized and executed as `node <cli.js>` with the original arguments followed by any baked shim suffix. Batch commands the wrapper cannot recognize keep going through `cmd.exe`. The prompt itself is not an argument, so it arrives byte-exact either way: it is in a file only the job's user can read, which `LOTAR_AGENT_PROMPT_FILE` names and LoTaR removes when the job ends, and also in `LOTAR_AGENT_PROMPT` when it is at most 128 KiB. Linux refuses to start a program with a longer environment variable, so a program that takes long prompts reads the file.

## Default instructions

Every job prompt starts with instructions, then a "This job" section, then the ticket and the request.

The instructions are the base text followed by the profile's own `instructions`:

- The base text is `agent.instructions` when set, otherwise the built-in text in [agent-instructions.md](agent-instructions.md). It covers the `lotar` commands for reading and updating the ticket, retrying a command the sandbox blocks (with the harness's approval, then the repository's documented fallback) before giving up, and how to hand the ticket back when the agent needs a decision or stays blocked. A check that fails on the work still stops a merge. Set `agent.instructions: ""` to leave it out.
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

- With worktrees on, it names the branch (`agent/<TICKET>`) and tells the agent to commit its work there. A T3 Code thread is also told where earlier jobs on the ticket left uncommitted changes in other worktrees: those of thread branches T3 Code made from the ticket branch before DEV-652, or detached from it. Those changes stay there after a job carried them over or dropped them, so the note says only a job that takes the work over (a fix or rework) carries over what it still needs, after checking the ticket's commits and comments, and a review leaves them alone. A merge job doesn't get the note. Without worktrees the agent works in the main checkout and leaves its changes uncommitted unless asked to commit.
- If an automation rule moves the ticket when the job ends (a rule that matches the ticket and has a `job_completed` action), the agent is told not to change the status or assignee when it finished, only to comment, and to hand the ticket back when it could not finish. Otherwise it is told to set a fitting status when it finishes.

When a worktree job succeeds with changes still uncommitted, LoTaR commits them to the ticket branch (`<TICKET>: changes from the <profile> agent`), so the next phase and the merge see them. Agents can't always commit themselves: Codex's `workspace-write` sandbox keeps git metadata read-only, and commit signing may be out of reach. LoTaR commits as you, outside the sandbox, so your hooks and signing apply; if that commit fails, the job fails with git's error, unless signing or a full disk failed it: then it [pauses the queue](#paused-queue) and its retry makes only the commit.

A merge job (profile `merge`, or a ticket in `Merging`) has to end with its branch in the main checkout's current branch. If the agent couldn't fast-forward it (a sandbox may keep git metadata read-only), LoTaR runs `git merge --ff-only <branch>` in the main checkout, which never overwrites local changes. A merge agent that hands the ticket back instead (for example because a check failed on the work) keeps its branch unmerged: a handback reassigns the ticket, so when the agent changed the assignee during the job, LoTaR doesn't fast-forward, and the job log says so. This holds for a job another lotar process took over, too: the job's record keeps the assignee it started with. Setting only a status, as an agent does when it finishes without rules for the job's end, doesn't stop the fast-forward. A branch whose work is on main already counts as merged, as described under [Worktrees](#worktrees), and for a merge job also when main differs from it only in the tasks directory (the merge amends ticket changes into the landed commit). So does a branch the agent landed reworked, in a form that conflicts with it: main gained commits since the job started whose subject starts with the ticket ID (`DEV-7 Fix the login`) and that together change every file the branch changes. If the branch still isn't merged, the job fails, so `job_failed` rules run instead of `job_completed`. A branch that had nothing beyond main when the job started fails it too while another of the ticket's branches holds work main lacks (`Merge job finished, but <branch> has nothing beyond the main checkout's branch, while <other> holds <ticket>'s work that isn't in it`): the ticket doesn't go Done without its fix on main. Likewise, the job fails while the ticket's description or a comment names a path in another repository whose branch of the same name holds commits beyond that repository's main checkout branch (`Merge job finished, but <ticket>'s work is in another repository: ...`), unless the agent handed the ticket back; a failed runner doesn't complete it either. This holds also when the ticket's own branch has commits: LoTaR then leaves it unmerged until that repository's main holds the work, and fast-forwards it after. A ticket with no unlanded work on any branch merges as before, unless nothing was built: when the branch brought nothing since the job started and no commit whose subject starts with the ticket ID (`DEV-7 Fix the login`, `DEV-7: ...`) is on the main checkout's branch, here or in a repository the ticket names, the job fails (`Merge job finished, but no commit named after <ticket> is on the main checkout's branch, ...: nothing was built`), so a fix job that committed nothing doesn't end Done. The other way round, a merge job whose runner fails (the agent crashed, T3 Code was unreachable) after its branch reached main completes when its worktree holds no uncommitted changes: its work is done, so `job_completed` rules run instead of a retry. The job log says why. A merge job that succeeds records the branch's commit as landed in main's current commit, as the git ref `refs/lotar/landed/<branch>/<branch commit>`; cleanup deletes the record with the branch.

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
    waiting_run: [cargo, clean, --workspace] # Runs in a ticket's worktrees when it is left waiting (default: none)
```

Change these on the Config page (Agents) or with `config set`, globally or per project, for example `lotar config set agent_worktree_max_parallel_jobs 2 --global` or `lotar config set agent_worktree_cleanup_on_done true -p APP`. A job limit of `0` pauses new jobs; an empty value removes the limit. The queue worker, `lotar serve` (web, MCP, automation) and `agent run --wait` apply the same two limits: the global one caps all running jobs together, and a project's own limit caps that project's jobs on top of it. They count the running jobs of every lotar process (the queue worker, `lotar serve`, `agent run --wait`, jobs taken over after a restart). A job without a free slot waits in its process and starts once a slot frees, also when the job that ends ran in another process; its ticket says which limit holds it.

When enabled:
- Each ticket gets a dedicated worktree at `<dir>/<repo>/<ticket-id>`
- A branch is created at `<branch_prefix><ticket-id>` (e.g., `agent/TEST-1`)
- Multiple job phases for the same ticket reuse the same worktree
- Every job starts from the ticket's newest work that isn't in the main checkout's current branch, whatever branch holds it. A thread of a job before DEV-652 could commit on an earlier thread's branch (`t3/<topic>`) and leave the ticket branch behind; the review and merge after it would then find nothing. Before a job sets up its worktree, LoTaR looks at the ticket branch, the branches traced to it (as under cleanup below) and the branches the ticket's earlier jobs logged, and points the ticket branch at the one whose newest unlanded commit is newest; the log says `Pointed <prefix><ticket> at <branch> (<commit>), which holds <ticket>'s newest work …`. The ticket branch stays where it is when it has work of its own that the newer branch doesn't build on. A branch whose work landed, also rebased, doesn't count, nor does one main supersedes (as under cleanup below: a later commit on main whose subject starts with the ticket ID), such as an earlier copy of a fix the merge rebased and amended. A branch that counts only because an earlier job logged its name must still hold the ticket's work: its tip builds on a commit a job logged that main lacks, or one of its commits main lacks has a subject that starts with the ticket ID (`DEV-12 fix`, `DEV-12: fix`). Cleanup at Done frees a `t3/<topic>` name, and T3 Code may give it to another ticket's thread. When git refuses the move, the job fails before it starts, naming both branches.
- When the ticket branch is checked out in another worktree of the repository (one made by hand, or an earlier T3 Code thread's from before DEV-652), a job runs in that worktree instead, as git checks a branch out in one worktree only; the job log says so. A T3 Code fix, a headless review, a T3 Code rework that continues the fix thread (`session: continue`) and a headless merge all work in the ticket's one worktree, and done cleanup removes it. A ticket branch checked out in the main checkout still fails the job.
- A new worktree starts from a copy-on-write clone of an existing build instead of a cold one: right after `git worktree add`, LoTaR clones `target/` from the ticket's earlier worktree (also one of its T3 Code threads' from before DEV-652, traced as under cleanup below; with several, the one built last), else from the main checkout. The job log says `Cloned target/ from <path> (copy-on-write).`, or why nothing was cloned. Only macOS `clonefile(2)` clones (APFS): a clone takes no disk until a file in it changes, and a full copy would take what it saves, so on another filesystem or system, or when the clone fails, the job logs it and builds cold; it never fails the job. A reused worktree gets nothing cloned, and `cleanup_paths` and done cleanup remove a cloned `target/` like any other.
- The worktree persists after the job completes unless cleanup is enabled
- With `cleanup_on_done`, a ticket that reaches a done status loses the worktree on its ticket branch, whoever set the status: a job, automation, the CLI or the web UI. While a job runs for the ticket, its end does the cleanup. A successful job also removes its own worktree when the ticket is done by then. Every job end, for any ticket, also retries the cleanup of each done ticket that still has a worktree, so one a process kept (a T3 Code thread whose session still ran when the ticket went done) goes with its build output once that process exits.
- A ticket's worktrees and branches include those of its T3 Code threads from before DEV-652, when T3 Code made a worktree per thread: a thread's branch (`t3/<topic>`) that T3 Code created from the ticket branch or from an earlier thread's branch, as git's branch reflog records (`branch: Created from agent/TEST-1`), and a worktree detached from the ticket branch (LoTaR detached them so a new thread could take over the branch, before DEV-503), as its HEAD reflog records (`checkout: moving from agent/TEST-1 to HEAD`). Done cleanup removes each of them that passes the checks below, a detached worktree when its commit's work is in the main checkout, and with `cleanup_delete_branches` also deletes the ticket's branches that no worktree has checked out and whose work landed. Deleting a branch deletes its reflog, so before it does, cleanup records the ticket of each branch and detached worktree traced through that branch (in the branch's `branch.<name>.lotarticket` config, and in a `lotar-ticket-branch` file in the worktree's git directory), and a later cleanup still finds them once their work lands.
- Cleanup only removes a worktree whose ticket is done, whose branch is merged into the main checkout, and that has no untracked or modified files. A branch counts as merged when every commit on it is in the main checkout, also when it landed rebased or cherry-picked (`git cherry` lists no `+` commits), or when it landed reworked: merging the branch into the main checkout's `HEAD`, or into a commit since the branch's fork point that changed the branch's files, adds nothing (`git merge-tree --write-tree` yields that commit's own tree), or when a merge job recorded the branch's current commit as landed and that record's main commit is in the main checkout (see [Default instructions](#default-instructions)). A branch with any change main doesn't hold is kept, unless main supersedes it: the ticket's earlier fix, review or rework versions that the merge rebased or reworked, so they differ from what landed. Main supersedes a branch or detached worktree when it gained a commit since their fork point whose subject starts with the ticket ID (`DEV-7 Fix the login`), committed after the branch's or worktree's commit. Cleanup keeps that commit as the git ref `refs/archive/<ticket>/<branch>/<commit>` (`refs/archive/<ticket>/detached/<commit>` for a detached worktree) and then removes it; a worktree a process still runs in (its working directory is inside) stays. Work committed after the landing commit is kept, as is a branch whose lines the merge changed without a merge job's record and without such a commit; check it and remove it by hand. Restore an archived version with `git branch <name> refs/archive/...`. Git-ignored files such as build and test output are removed with it; set `cleanup_ignored_files: false` to keep any worktree that still holds ignored files. When cleanup keeps a worktree, the job log says why.
- With `cleanup_paths`, every job end (completed, failed, cancelled, or handed back with the ticket open) deletes those paths from the job's worktree, which stays. This also covers existing T3 Code thread worktrees recorded on earlier jobs. Build output such as Rust's `target/` takes gigabytes in each worktree; a resumed job rebuilds it (a shared compiler cache such as sccache makes that fast). A path is deleted only when git ignores it and nothing under it is tracked; symlinks, paths outside the worktree and `.git` are never deleted. Nothing is deleted while another job runs for the ticket, as it shares the worktree. The job log lists what was deleted.
- Merge-profile jobs require worktrees so merges happen on isolated branches
- When worktrees are enabled, setup failure fails the job; the runner never falls back to the repository checkout.
- Git setup, checks, cleanup, and runners discard inherited/profile repository-selection overrides (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_COMMON_DIR`, `GIT_INDEX_FILE`, object/discovery overrides, and `GIT_CONFIG*`). Worktree top-level and common-directory identity are verified. SSH/askpass and author/committer identity environment variables are retained.
- With `agent.merge`, a merge job merges without an agent when it can; see [lotar's own merge](#lotars-own-merge)
- Merge-profile jobs are serialized one-at-a-time even when `max_parallel_jobs` allows more general agent concurrency

### lotar's own merge

With `agent.merge` in the config, a merge job (profile `merge`, or a ticket in `Merging`) merges without an agent first, the steps a merge agent's prompt names:

```yaml
agent:
  merge:
    checks: [npm run lint, npm test]   # run in the ticket's worktree, in order
    install: npm ci                    # after a rebase that changed a lockfile, or in a new worktree
    push: true                         # push the main checkout's branch after landing
    finding_check: node scripts/health.js   # a finding ticket's check; must no longer print [<key>]
```

1. The ticket branch (`agent/<TICKET>`) holds no commits beyond the main checkout's current branch, or its work is there already: lotar comments "Nothing to merge" and the job completes, or fails when no commit named after the ticket is on main anywhere (nothing was built, see above). Unless the ticket's description or a comment names a path in another repository whose branch of the same name holds commits beyond that repository's main checkout branch: then the work is there, and lotar hands off to the merge agent, naming that repository. lotar checks this first, so it hands off before landing anything also when the ticket branch has commits of its own: the merge agent merges both.
2. In the worktree that has the branch checked out (it must be clean), else in a new worktree of its own, lotar rebases the branch onto the main checkout's branch, runs `install` when needed and each check. A check is skipped when the rebase changed nothing and the ticket's latest report on that commit (`lotar comment --commit ... --check "<command>=passed"`) lists it as passed.
3. It fast-forwards the main checkout. When main moved on meanwhile it rebases and checks again, at most 3 rounds.
4. Pending changes of the tasks directory in the main checkout are amended into the merged commit while it is still main's tip.
5. When the merge changed rules, agent profiles or `agent.merge` of a trusted repository, lotar trusts them again (`lotar trust`); an untrusted one stays untrusted.
6. With `push: true`, it pushes when the main checkout is clean and its unpushed commits are all this merge's. It never passes `--no-verify`.
7. For a ticket with a finding key, `finding_check` runs in the main checkout with the key in `LOTAR_FINDING`; its output must no longer contain `[<key>]`.

lotar comments the outcome with a report (the commits landed, the checks) and the job completes, so `job_completed` rules close the ticket; no agent starts. A step that needs judgment stops there: a rebase conflict (lotar aborts the rebase), a failed install or check, a dirty worktree or checks that changed files, a fast-forward git refuses, unpushed commits of others, no upstream or a refused push, a failed trust, a finding check that still lists the finding. lotar comments why, and the merge agent starts in a new session (`fresh`, whatever the rule or profile says) on the profile's model, with lotar's comment before its prompt; work lotar landed already stays landed. The job then ends as any merge job does.

The commands run without LoTaR's `LOTAR_*` environment, so a check that runs lotar doesn't change the workspace. What a command leaves running in the background (a server a test started) is stopped 5 seconds after the command exits, so it can't hold up later merges. `agent.merge` runs commands from the repository: `lotar trust` covers it, and in an untrusted repository the merge agent merges as before.

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

Worktrees with active jobs are never removed, even with `--all`. Cleanup refuses dirty worktrees (including untracked or ignored files), unknown Git state, and branches with commits that are not in the repository checkout's `HEAD`; a branch that landed rebased, cherry-picked or reworked counts as merged, and a done ticket's version that main supersedes is archived and removed, as described above. Branches are deleted with `git branch -D`, because `-d` refuses a rebased branch, and only after these checks pass. `--all` broadens ticket selection only; it is not a force-delete option. Refusals are reported and return a nonzero exit status.

### Build output while a ticket waits

A ticket that waits for a person or another ticket keeps its worktree, and with it its build output, until it is done. `lotar agent worktree run <TICKET> -- <command> [args...]` runs a command in each of the ticket's worktrees (its T3 Code thread worktrees included), so an automation rule can delete part of that build output when the ticket starts to wait. The command's output goes to stderr, its exit status decides the command's own. It runs nothing, and says why, when the ticket has no worktree, a job runs for it in any process, or the T3 Code thread of an earlier job on it still runs or waits for an answer (`Ran nothing: the T3 Code thread … of the job … waits for an answer`): the job or thread shares the worktree, and a thread can run on after its job ended (T3 Code resumes a turn it paused for a usage limit, or someone answers in the thread). A running job is named first. When T3 Code can't be asked (no `t3` agent, `t3 thread show` fails), the thread doesn't count and the command runs, as LoTaR's own worktree cleanup does. The check comes right before the command runs; a job that starts while the command runs isn't held back. Git's repository-selection variables (`GIT_DIR`, `GIT_WORK_TREE`, …) are removed from its environment.

Waits that LoTaR or a rule sets fire no rule: a rule's own `set` (a failed review or merge that hands the ticket to its owner), a T3 Code thread that waits for an answer, and a ticket blocked on open prerequisites when its job would start. For those, set `agent.worktree.waiting_run` to a command, program first (`[cargo, clean, --workspace]`; no shell). LoTaR runs it in each of the ticket's worktrees when:

- a job ends (completed, failed or cancelled; not paused for a usage limit or the queue) with its ticket HelpNeeded or Blocked, whoever set it: the job's end rules, LoTaR's hand-back of a waiting T3 Code thread, or the agent's own hand-back. It runs after the end rules, so a rule's own `set` counts.
- a job is held back for open prerequisites and its ticket goes to Blocked (where the project has that status). No job ends there.

It runs nothing while another job runs for the ticket (in any process) or a T3 Code thread on it runs a turn. A thread that waits for an answer doesn't hold it back: the job just ended on that question, and the thread rebuilds when it is answered. A job for the ticket waits to start until the command ends. From a repository's config the command runs only after [`lotar trust`](trust.md); untrusted, the job's log says it was skipped. One from your home config (`~/.lotar`), which wins over the repository's `.tasks/config.yml`, always runs; a project's config wins over both. Each run, skip and failure (with its exit status and last line of error output; a command that can't start says whether the worktree is gone or the program isn't on the PATH of the lotar that ran it, and shows that PATH) goes to the job's log (on stderr when no job ended); a failure changes nothing else. So does each `run` of the rules the job's end fires, such as the waiting rule below ([running commands](automation.md#running-commands)). A ticket whose wait also fires a waiting rule, such as an agent's hand-back, gets both; the second run finds little left to delete.

LoTaR's own development config sets `waiting_run: [cargo, clean, --workspace]`, and its rules drop the same build output (`target/debug/incremental` and the workspace's own artifacts; dependencies stay) when a person sets HelpNeeded or Blocked, which ends no job. The rule:

```yaml
- name: "Waiting: drop the workspace build output"
  when:
    any:
      - status: HelpNeeded
      - status: Blocked
  on:
    updated:
      run:
        command: lotar
        args: [agent, worktree, run, "${{ticket.id}}", --, cargo, clean, --workspace]
        ignore_failure: true
```

In a lotar worktree after lint, tests and smoke this took `target/` from 3.4 GB to 0.7 GB in about a second; the job that resumes the ticket rebuilds the workspace crates in about a minute more (`cargo build --bins --tests`: 55 s with the Mac under heavy load). The steps between fix, review and merge keep their build output: the next step reuses it, and the job's end starts the next step before `cleanup_paths` would run. Done cleanup removes the rest.

Default CLI cleanup and all automatic cleanup require a readable, valid ticket in a configured done state. Missing, malformed, unreadable, or unknown-status tickets are preserved, even with `--all`. Failure/cancellation cleanup settings only enable cleanup when that same done-state and Git safety check succeeds.

The queue worker drains pending entries and waits for dispatched jobs, including finalization, before exiting. It runs in its own process group, so it outlives the command that started it, also when that command's group is killed (a launch agent's run ends, a terminal closes). It drops the session variables of a coding agent whose `lotar` call started it (`CLAUDECODE`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_MESSAGING_SOCKET`, `CLAUDE_PID`, `CODEX_THREAD_ID`, `T3CODE_THREAD_ID` and the like), so later jobs' agents don't act as that session; settings such as `T3CODE_HOME` stay. Its PATH is the one of the process that started it, followed by the entries of your login shell's PATH (`$SHELL -l`) that it lacks, so its jobs and their `agent.worktree.waiting_run` find the same programs whichever agent shell or launch agent started it; when `SHELL` is unset or the login shell fails or takes over 5 seconds (also when something its profile started in the background keeps its output open), the worker keeps the PATH it got. It also waits for dispatched jobs if queue/config processing fails. There is no single-poll `--once` mode. An entry the worker took stays in the queue file until its job is registered: a worker killed in between leaves it for the next worker, which starts it first. Start failures retain the existing bounded retry policy (three retries); an entry that exhausts them leaves the queue with a comment on its ticket that gives the error (`The queued @implement job did not start after 4 tries and left the queue: …`). Failed runner jobs are terminal rather than automatically rerun. A ticket whose previous job is still running (in any process), or whose earlier job's T3 Code thread still runs or waits for an answer, is not a failure: the entry waits in the queue until that job or thread ends. An entry held by a T3 Code thread says so in one comment on the ticket. When the entry comes up and its ticket is now assigned to another agent profile (a merge queued while the review ran, whose blocking review sent the ticket back to `@fix`), the job doesn't start: the ticket has moved past that stage, and its comment says so (`The queued @merge job didn't start: DEV-1 is assigned to @fix now.`). A merge job (profile `merge`, or any job on a ticket in `Merging`) never starts, queued or not, while its ticket carries the tag `review-blocked`: a blocking review set it, and only its rework removes it. The ticket's comment says so (`The @merge job didn't start: DEV-1 carries tag review-blocked, and no rework has removed it.`). When the `lotar` binary the worker was started from is replaced (an upgrade or reinstall), the worker takes no new entries; once its running jobs end it exits and a new worker, started from the upgraded binary, runs the rest of the queue.

### Memory gate

LoTaR starts no new job while the machine is short of memory: macOS reports memory pressure (`sysctl kern.memorystatus_vm_pressure_level` is warn or critical), or swap use is over `agent.memory_gate.max_swap_percent` of the swap total (default 85%). Either one holds new jobs; running jobs go on, and so do jobs taken over from a process that died.

- The queue worker takes no entry while the gate holds: entries stay queued with their attempts unchanged, and no ticket gets a comment. In `lotar serve` a new job waits as `queued`, and its waiting reason names the memory hold.
- While held, the queue reads memory again every 30 s. Held jobs start once memory has been normal for `agent.memory_gate.settle_seconds` (default 120); short of memory again during that time, the wait starts over. A job is never failed or retried for this.
- `lotar agent queue` and `lotar agent list-jobs` say why jobs wait and since when, for example `waiting for memory: swap 9.4 of 10.0 GB used (94%, limit 85%) (held since 2026-11-16T21:30:00Z)`. With `--format json` both print `memory_hold`.
- A memory reading that fails holds nothing; `lotar agent queue` warns about it.
- On by default on macOS, off elsewhere. `agent.memory_gate.enabled: true` turns it on elsewhere too; on Linux it then reads swap from `/proc/meminfo` (there is no pressure level there).

```yaml
# .tasks/config.yml
agent:
  memory_gate:
    enabled: true          # unset: on on macOS only
    max_swap_percent: 85
    settle_seconds: 120
```

The last reading is kept next to the agent queue (`memory-gate-<hash>.json`), so the worker, `lotar serve` and `lotar agent queue` share it. A hold or failed reading that no process read again for 60 s (the worker and `lotar serve` stopped) is not shown as current: `lotar agent queue` and `lotar agent list-jobs` then read memory themselves, without saving it. Tests set `LOTAR_MEMORY_READING` to replace the reading: `normal`, or for example `pressure=warn,swap=9.5/10` (swap in GB).

Cancelling a dispatched job requests termination immediately, but cancellation automation and reassignment wait until process/output teardown and context persistence finish. The job retains its capacity slot through finalization, so a replacement is queued safely and cancellation automation runs once. Cancelling an undispatched queued job finalizes immediately.

Both output readers are joined before final summaries, persisted status, and slot release. After the runner exits, the output it left in the pipes is read in full, however long its lines take to record, without waiting for a surviving descendant to close inherited streams. Continuous descendant output after that is bounded to five seconds of draining and reported as an output failure rather than keeping the worker alive indefinitely.

## Automation rules

Agent lifecycle automation lives in the automation rules file. Without a rule, a finished job leaves the ticket's status and assignee as they are. See [Automation Rules](automation.md#agent-job-queueing) for a lifecycle example.

A rule's [`tell_agent`](automation.md#telling-the-tickets-agent) action hands the ticket's agent a message, for example a person's comment: a running job gets it as its next turn, an idle ticket starts its agent with it.

## Notes

- Job status is streamed over `/api/events?kinds=agent_job_*` while the server is running. Streamed events carry the same `sequence` (stable per-job identity, positive, strictly monotonic, never reused) and `at` as the matching `GET /api/jobs/logs` entries, so live and snapshot logs can be joined losslessly.
- Context history is persisted to `<project>/<ticket><extension>` files next to each ticket.
- Job events are persisted to `<tasks dir>/@agent-logs/<JOB_ID>.jsonl` by default (`agent.logs_dir` or `LOTAR_AGENT_LOGS_DIR` picks another folder; `off` turns it off). LoTaR writes a `.gitignore` into the folder so logs, which contain prompts and agent output, aren't committed. The `.gitignore` is only written when the folder holds nothing but job logs, and a setting that names the workspace root itself (`agent.logs_dir: .`) uses a `lotar-agent-logs` subfolder.
- Job history: `lotar agent list-jobs`, `lotar agent logs`, the timeline and the Agents page show a job as long as its log is kept (a server holds finished jobs in memory for a day only). When a job starts, LoTaR prunes that job's log folder:
  - A log last written more than `agent.job_history.keep_days` days ago (default 14) is removed.
  - While the folder's job logs take more than `agent.job_history.max_mb` MiB (default 100; `0`: no limit), the oldest removable logs go first, also ones younger than `keep_days`.
  - Never removed, by age or by size: a job of an open ticket (its status is not a done state), of a ticket whose file can't be read or whose ID several tasks roots hold, and a running job (in this process or another one). These win over both limits, so the folder can stay over `max_mb`. Jobs of done, archived and deleted tickets go, and so do logs without a header.
  - Only `job-*.jsonl` files are pruned. A file that can't be read or removed is skipped; pruning never stops a job from starting.
  - Both settings are global config only, not settable with `config set`. One job log takes about 5 KB, so at 250 jobs a day 14 days take about 18 MB.

  ```yaml
  agent:
    job_history:
      keep_days: 14
      max_mb: 100
  ```
- `lotar agent status`, `lotar agent logs` and `lotar agent list-jobs` read these files when the job isn't in the current process. So do `GET /api/jobs`, `/api/jobs/get` and `/api/jobs/logs`, and with them the Agents page and the task panel's Timeline tab: jobs of earlier server runs and of other processes stay listed with their threads. A logged job without an end is `running` while a live process holds it, else `failed`. `/api/jobs/logs` returns a job's whole thread: its log holds every event with the `sequence` the live stream gave it, while the server keeps only a job's last 200 events in memory. Logs written before this version have no sequences; their events count by line.
- `lotar task timeline <TASK_ID> [--threads]` and the MCP tool `task_timeline` list a task's agent jobs next to its changes, comments and commits, so an agent can read what earlier agents on the task said and did ([timeline](task.md#timeline)).
- Assigning a ticket to an agent profile (e.g., `@claude-review`) automatically queues a job for that ticket, with the prompt and run settings of the first automation rule that assigns that profile ([agent run settings](automation.md#agent-run-settings)).
- Review-stage tickets can only be advanced by their reporter when the change would alter assignee or status.
