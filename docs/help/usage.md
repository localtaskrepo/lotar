# lotar usage

Show each provider's newest usage-limit reading from agent jobs: how much of a window is used and when it resets.

## Usage

```bash
lotar usage                 # a table: provider, account, window, used, status, resets at, read at
lotar --format json usage   # {"file": "...", "readings": [...]} for scripts
```

It reads one file for the whole machine, so it works in any directory, with no tasks directory.

```text
PROVIDER  ACCOUNT  WINDOW     USED  STATUS           RESETS AT                 READ AT
claude    -        five_hour  -     allowed          2026-11-26T03:00:00.000Z  2026-11-25T23:40:12Z
claude    -        seven_day  26%   allowed_warning  2026-12-01T16:00:00.000Z  2026-11-25T23:40:12Z
```

## What is recorded

An agent job saves a reading when:

- its agent reports a usage window. The Claude runner (claude-agent-acp) reports Claude's windows (`five_hour`, `seven_day` and others) during each turn: their status, and the used share once Claude warns about it.
- it pauses on a usage limit (see [agent](agent.md)). This saves a `rejected` reading with the reset time the job waits for. It is the only reading the Codex and T3 Code runners give: they report no windows while they work. The window is `five_hour` or `seven_day` when the limit's text names a 5-hour or weekly limit, else empty. A reset more than 7 days away is left out, as the retry ignores it.

Each reading has:

| Field | Meaning |
| --- | --- |
| `provider` | `claude` or `codex`; for a `t3` profile its `provider` (`claudeAgent` is `claude`), `t3` when it names none; else the runner's name |
| `account` | The profile's `CLAUDE_CONFIG_DIR` (Claude) or `CODEX_HOME` (Codex) from its `env`; `null` for the default login |
| `window` | The window as the agent names it, or `null` |
| `status` | `allowed`, `allowed_warning` or `rejected` |
| `used_percent` | 0 to 100, or `null` when the agent gave only the status |
| `resets_at` | When the window resets (RFC 3339, UTC), or `null` |
| `read_at` | When lotar got the reading (RFC 3339, UTC) |
| `job_id` | The job whose agent reported it |

The file keeps one reading per provider, account and window: the one read last wins. An unchanged reading is written again at most once a minute. A reading that can't be saved shows a warning and doesn't affect the job.

## Where

`usage.json` in lotar's state directory, next to the agent queue: `$LOTAR_AGENT_QUEUE_DIR`, else `$XDG_RUNTIME_DIR/lotar`, else the user cache directory (`~/Library/Caches/lotar` on macOS, `~/.cache/lotar` on Linux). Every repository's `lotar serve` and every CLI job write the same file. `lotar usage --format json` names it under `file`.

## Limits

- A reading is only as new as the last job that ran on that provider: with no job running, nothing updates it. Compare `resets_at` with the time now: a reset that has passed means a new window.
- No file yet, or no readings: `lotar usage` says so and exits 0 (JSON: an empty `readings`). A file that can't be read or parsed is an error naming the file, exit 1.
