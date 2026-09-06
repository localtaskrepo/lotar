# Agent Jobs

LoTaR can orchestrate supported agent CLIs (Copilot CLI, Claude Code, Codex CLI, Gemini CLI) and stream progress over SSE.

## CLI commands

- `lotar agent run <TICKET> <PROMPT> [--runner <copilot|claude|codex|gemini>] [--agent <profile>] [--wait] [--follow] [--timeout-seconds <N>]`
- `lotar agent status <JOB_ID>`
- `lotar agent logs <JOB_ID>`
- `lotar agent cancel <JOB_ID>`
- `lotar agent list-running`
- `lotar agent list-jobs [-n <LIMIT>]` — list job logs from disk
- `lotar agent check [--status <value>] [--assignee <handle>]`
- `lotar agent worktree list` — list agent worktrees
- `lotar agent worktree cleanup [--all] [--delete-branches] [--dry-run]` — remove stale worktrees

## list-running requirements

`lotar agent list-running` discovers running jobs by scanning for the wrapper process name. This requires the wrapper binary to be available in the same directory as `lotar` or on `PATH` (or set `LOTAR_AGENT_WRAPPER` to an absolute path).

If the wrapper is not available, jobs can still run, but `list-running` will show no results.

## Environment variables

- `LOTAR_AGENT_WRAPPER` — absolute path to the wrapper binary if it is not on `PATH`.
- `LOTAR_AGENT_CONTEXT_EXTENSION` — extension for context files (default `.context`).
- `LOTAR_AGENT_LOGS_DIR` — when set, enables debug logging to this directory (supports relative or absolute paths).

## Default instructions

LoTaR prepends agent instructions to each run. The built-in default describes the environment and workflow expectations; you can override it with an inline string or a file path under `agent.instructions`.

```yaml
agent:
  # Inline string
  instructions: "Follow AGENTS.md and keep changes focused."
  # Or point to a file (relative to the tasks directory by default)
  # instructions:
  #   file: "@agent-instructions.md"
```

The built-in instruction text is kept in [agent-instructions.md](agent-instructions.md). Repository guidance lives in:

- [AGENTS.md](../../AGENTS.md)
- [Backend instructions](../../.github/instructions/backend.instructions.md)
- [Frontend instructions](../../.github/instructions/frontend.instructions.md)
- [Smoke instructions](../../.github/instructions/smoke.instructions.md)

## Web UI

The Agents page shows live job output as it streams over SSE and lets you interrupt running jobs, queued jobs, or stop all queued/running jobs at once. You can also send messages to a running agent from the UI; this only works for runners that accept stdin in the current configuration (otherwise the API returns an error).

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
```

When enabled:
- Each ticket gets a dedicated worktree at `<dir>/<repo>/<ticket-id>`
- A branch is created at `<branch_prefix><ticket-id>` (e.g., `agent/TEST-1`)
- Multiple job phases for the same ticket reuse the same worktree
- The worktree persists after the job completes unless cleanup is enabled
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

Agent lifecycle automation now lives in the dedicated automation rules file. See [Automation Rules](automation.md) for the schema and examples.

## Notes

- Job status is streamed over `/api/events?kinds=agent_job_*` while the server is running.
- Context history is persisted to `<project>/<ticket><extension>` files next to each ticket.
- When `agent_logs_dir` is configured, job events are also persisted to `<logs_dir>/<JOB_ID>.jsonl` files for debugging.
- `lotar agent status` and `lotar agent logs` read from log files if the job is not in memory (requires `agent_logs_dir` to be set).
- Assigning a ticket to an agent profile (e.g., `@claude-review`) automatically queues a job for that ticket.
- Review-stage tickets can only be advanced by their reporter when the change would alter assignee or status.
