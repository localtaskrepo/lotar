# Inline transcript review on the ticket timeline (DEV-850)

Research for [DEV-850](https://lotar.pyxzl.net/task/DEV-850), checked 2026-11-24. No code was changed.

## What was asked

Ravi wants to read the conversation an agent job had right in the ticket's Timeline, next to the comments and
changes, for T3 jobs and for headless Claude Code and Codex jobs. The answer should let him decide:

- how a job points to its conversation: extend the existing job and session references, or add a typed transcript
  reference;
- when it loads: when a phase is opened, or prefetched in a limited way;
- how it is read: reuse the job log endpoint, or add a bounded, paged reader;
- the loading, expired, offline and unsupported-format states, retention and access, how the phone reaches the
  machine that has the transcript, and reading only from known places;
- Open in T3 as a link to the existing thread; selective import is a separate evaluation.

Related: [DEV-849](https://lotar.pyxzl.net/task/DEV-849) (reciprocal links and live T3 state, in progress). I did not
find the T3 thread where these two tickets came up (ticket reporter `codex`, created 2026-11-24 20:22Z), so I asked no
running thread. The DEV-849 fix thread works on its own fix and was not disturbed.

## What exists today

Checked in this checkout (lotar 0.15.6, `main` at `855ff6f46`):

- The Timeline tab already shows each agent run with **Show thread**. It loads the job's events when opened and follows
  a running job ([TaskPanelTimelineTab.vue](../../view/components/task-panel/TaskPanelTimelineTab.vue), lines 89–108
  and 287). It also shows the run's prompt and settings (`AgentRunInputs`, DEV-767).
- That thread is the **job log**, `<tasks>/@agent-logs/<job>.jsonl`, read by `GET /api/jobs/logs?id=`, which returns
  the whole log unpaged ([routes/jobs.rs](../../crates/lotar-lib/src/routes/jobs.rs), lines 61–86;
  [agent.md, Notes](../help/agent.md)). It holds lotar's own progress lines, tool titles, and each agent message, but
  not tool input or output. It is about 5 KB per job.
- Job logs are pruned by `agent.job_history.keep_days` (14) and `max_mb` (100). Logs of open tickets and running jobs
  are kept ([agent.md](../help/agent.md)).
- The job log knows its session only as free text: "Session `<name>` (`harnessmux attach <name>` follows it)",
  "Continuing the session `<name>`…", and for T3 "t3 session new|resumed, `<threadId>`: `<link>`". Claude and Codex
  session ids are not in the job log.
- `lotar serve` has no login. Off loopback it warns, and the docs require a reverse proxy with auth or a VPN
  ([serve.md](../help/serve.md), `--host` and the security notes). The phone reaches the Mac's lotar this way already
  (`https://lotar.pyxzl.net`).

## Where the conversations live

| Source | Where | Format | Kept | Checked |
| --- | --- | --- | --- | --- |
| harnessmux (all runners) | `~/.local/state/harnessmux/sessions/<name>.json` + `<name>.events.jsonl` | documented JSONL, `v` 1: `session.started` (`agent_session`, `how`, `link`), `turn.started` (`prompt`), `message`, `thought.delta`, `tool.started/updated/finished` (input, output up to 16 KB), `plan`, `usage`, `turn.finished`; every event has `seq` and `turn` | no pruning documented; here 2591 sessions, 188 MB, oldest 2026-11-15 | [harnessmux README, Sessions and Events](https://github.com/localtaskrepo/harnessmux#events) (v0.3.0, `651e4a1`) |
| Claude Code | `~/.claude/projects/<encoded cwd>/<session id>.jsonl` | internal, no format version; a per-line CLI `version` | deleted after `cleanupPeriodDays`, default 30 | [Claude Code settings](https://code.claude.com/docs/en/settings-reference), [.claude directory](https://code.claude.com/docs/en/claude-directory) |
| Codex | `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<id>.jsonl` | internal; a `session_meta` line with `cli_version` | no official retention found; users report multi-GB logs | not in official docs: only [discussion #1076](https://github.com/openai/codex/discussions/1076), [issue #24948](https://github.com/openai/codex/issues/24948) |
| T3 Code | T3's own store, read with `t3 thread show --json` (latest N messages only) or the T3 MCP `t3_thread_read` (pages by run id `run:thread:<id>:ordinal:N`) | T3's API | as long as the thread exists | observed locally with t3 v0.1.11; no public doc found for the read API |

Versions, from the projects' release pages on 2026-11-24:
[harnessmux v0.3.0](https://github.com/localtaskrepo/harnessmux/releases/tag/v0.3.0) (2026-11-22; lotar depends on
`0.3.0`, the installed CLI is 0.1.10, and the installed lotar 0.15.6 that runs the jobs links harnessmux `0.2.0`:
[its Cargo.lock](https://github.com/localtaskrepo/lotar/blob/v0.15.6/Cargo.lock)),
[T3 Code v0.1.11](https://github.com/pingdotgg/t3code/releases/tag/v0.1.11) (2026-11-20),
[Claude Code 2.1.331](https://raw.githubusercontent.com/anthropics/claude-code/main/CHANGELOG.md) (top entry),
[Codex rust-v0.172.0](https://github.com/openai/codex/releases/tag/rust-v0.172.0) (2026-11-24).

harnessmux resumes an agent session with ACP `session/resume`, or `session/load` when the agent only has that
([harnessmux README](https://github.com/localtaskrepo/harnessmux#sessions);
[ACP session setup](https://agentclientprotocol.com/protocol/session-setup);
[session/resume stabilized](https://agentclientprotocol.com/announcements/session-resume-stabilized)). So a reused
session continues the same Claude or Codex conversation, and its turns land in the **same** harnessmux event log.

## The read paths, exercised

All read-only, on 2026-11-24.

- **T3 job**, autospec ASP-18 fix `job-20261124T163658.622Z-1`: the job log's init line names thread
  `mcp:launch:8c15bcaa…`. `t3 thread show --json --messages 20 <thread>` returned it (1 run, settled). The harnessmux
  log of a t3 session holds `session.started` with the thread `link`, `turn.started` and the replies (`message`).
  **Tool calls depend on the harnessmux version that wrote the log.** harnessmux 0.3.0 reads the thread's activity
  every 2 seconds and sends each command, file change and tool as `tool.started`/`tool.finished` (with `output`), plus
  `thought.delta` ([harnessmux README, Sessions, t3](https://github.com/localtaskrepo/harnessmux#sessions); tested
  live against T3 Code 0.1.11). harnessmux 0.2.0 reads the activity only for a failed run's errors
  ([`backend/t3.rs` in 0.2.0](https://docs.rs/crate/harnessmux/0.2.0/source/src/backend/t3.rs)). Every t3 log here
  was written by lotar 0.15.6, so by 0.2.0: `lotar-95e687-DEV-849-fix`, `…DEV-850-research.20261124T212513Z`,
  `…DEV-859-fix.20261125T124231Z` and standards `…STD-353-fix` (2026-11-25 19:55) hold `message` and
  `message.delta` but no `tool.*` event. **Not verified:** a t3 log written by lotar's 0.3.0 library; none exists
  until a lotar release with DEV-774 runs a t3 job. So the reader shows t3 tool calls from new logs and only the
  replies from older ones.
- **Claude Code, reused session**, ASP-18 review `job-…163813.241Z-3` and merge `job-…164005.195Z-4`. The merge job
  logged "Continuing the session `autospec-c2ad4b-ASP-18-review`". That session (Claude session `fe831934-…`) has
  turn 1 from 16:38:25 to 16:40:04 (the review job, 16:38:21–16:40:04) and turn 2, `how: resumed`, from 16:40:10 to
  16:41:49 (the merge job, 16:40:06–16:41:50). The session record's env names only the latest job
  (`LOTAR_AGENT_JOB_ID` is overwritten), and `turn.started` carries no job id. **Today the right turn can only be
  found from the job's time window.** The Claude transcript (`fe831934-….jsonl`, 340 KB) exists too.
- **Codex**, standards INBOX-688 triage `job-20261123T193113.247Z-104`: session
  `standards-04f015-INBOX-688-triage-codex`, Codex session `01a209e4-…`, 1 turn, event log 329 KB, rollout file under
  `~/.codex/sessions/2026/11/23/`. I found no Codex session with more than one turn, so Codex reuse was not exercised.
- **Name reuse**, lotar DEV-791: after a handoff a new session takes the name `lotar-95e687-DEV-791-fix` and the old one
  is kept as `…fix.20261124T194322Z`; the last job resumed the kept one. **The session name alone is not a stable
  identity.**
- Counts: lotar has 1306 job logs (518 claude, 788 t3); standards 989 (578 claude, 54 codex, 357 t3). 296 t3 and 24
  claude sessions have more than one turn.

## Options compared

**1. How a job points to its conversation.**
- *Extend the existing references* (parse the free-text progress lines, or the ticket's thread link reference): no
  format change, but the text is not an interface, the name is reused (DEV-791), and it can't say which turn.
- *Typed transcript reference* (recommended): when lotar starts or continues a session it already knows the session,
  and `Session::send` returns the turn number ([harnessmux library](https://github.com/localtaskrepo/harnessmux#library)).
  Record one event in the job log: `{source: "harnessmux", session, agent, agent_session, turn, format: 1, link?}`
  (a T3 job adds the thread `link`). Old job logs fall back to the time window. The job log is lotar's own file, so this
  stays backwards compatible.

**2. When it loads.**
- *On expand* (Ravi's choice, 2026-11-26): it matches how **Show thread** works today. One turn is tens to hundreds of KB (the logs
  above); a ticket can have many runs (DEV-791 has 19 job logs). Prefetching would load megabytes no one reads.
- *Limited prefetch* (e.g. the last run): saves one tap. Not chosen.

**3. How it is read.**
- *Reuse `/api/jobs/logs`*: it returns the whole job log unpaged and reads only `@agent-logs`. Folding a transcript
  into it makes every Show thread heavy.
- *A bounded, paged reader* (recommended): a new `GET /api/jobs/transcript?id=<job>&after=<seq>&limit=<n>`. It takes a
  job id only, never a path: the server finds the reference in that job's log, opens only
  `<harnessmux home>/sessions/<name>.events.jsonl` with a validated name, keeps events of the referenced `turn`, and
  returns them in pages, with tool output already capped at 16 KB by harnessmux. That is the "known sources only" rule
  by construction. It sits behind the same access as the rest of the API (the proxy).

**Which format to read.** The harnessmux `v:1` event log is the only documented one, and it is the same for Claude,
Codex and T3. The native Claude and Codex files are internal, change with each CLI release, and Claude deletes them
after 30 days by default. Don't parse them; at most name their path in the unsupported state.

## States

- **Loading**: a loader in the phase, as today.
- **Expired**: the job log or the session's event log is gone. The phase still shows what lotar keeps in the ticket:
  status, duration, cost, summary, commits and checks from the hand-off comment. Text: "The conversation is no longer on
  this machine."
- **Offline / other machine**: the job ran on a machine whose lotar the phone can't reach, or the reader fails. Show the
  error and a retry; the ticket's own entries stay.
- **Unsupported**: an event log with `v` other than 1, or a job without a reference and no time-window match: "This run's
  conversation can't be shown here" plus `harnessmux attach <name> --from-start` to copy.
- **T3 jobs** read like the others: replies, thoughts and tool calls from the harnessmux log, plus **Open in T3**:
  the thread `link` the job already records (`https://app.t3.codes/threads/<environment>/<thread>`), opening the
  existing thread. No copy into lotar. A t3 log from before harnessmux 0.3.0 has no `tool.*` events: the view shows
  its replies and the note "Tool calls are in T3" next to Open in T3.

## Retention and access

- The transcript lives as long as the harnessmux session files. harnessmux documents no pruning (188 MB here after 9
  days), and job logs go after 14 days for done tickets. The view therefore expires with the job log, not with
  harnessmux. A harnessmux retention setting belongs to harnessmux and is a separate ticket there if Ravi wants it.
- Transcripts contain tool output (commands, file contents). They get the same exposure as the job log today: same
  server, same proxy auth. Nothing is copied into `.tasks` or git.
- The phone reaches the conversation through the `lotar serve` of the machine that ran the job, the same way it reaches
  the ticket. A job from another machine shows the offline state; reaching other machines is not part of this.

## Mobile sketch

```
┌─────────────────────────────────┐
│ DEV-791  Timeline        [Agent]│
├─────────────────────────────────┤
│ ● Fix  t3 · opus high   done    │  collapsed phase
│   42m · $1.80                   │
│   Fixed -m dropped with stdin…  │
│   [Show thread] [Open in T3 ↗]  │
│   (expanded: same as Review;    │
│    an older t3 log: replies     │
│    and "Tool calls are in T3")  │
├─────────────────────────────────┤
│ ● Review  claude · high  done   │  expanded conversation
│   [Hide thread]                 │
│   Conversation · turn 1 of 2    │
│   ▸ Prompt (DEV-767 inputs)     │
│   ● Read crates/…/comment.rs    │
│   ● Ran nextest -E 'test(cli)'  │
│     ✓ 312 passed  [output ▾]    │
│   “The fix reads stdin only     │
│    when -m is absent…”          │
│   [Load 40 more]                │
├─────────────────────────────────┤
│ ● Merge  claude  failed         │  missing transcript
│   ⓘ The conversation is no      │
│     longer on this machine.     │
│     Summary, commits and checks │
│     stay on the ticket.         │
└─────────────────────────────────┘
```

## Smallest slice

1. Job log: write the typed transcript reference (session, agent, agent_session, turn, format, link) when a turn is
   sent. Rust, in the agent job service; docs in [agent.md](../help/agent.md).
2. `GET /api/jobs/transcript` reading the referenced turn from the harnessmux event log, paged by `seq`; the time
   window as fallback for old logs. `api_types.rs`, `view/api/types.ts`, `docs/openapi.json` per the REST contract.
3. Timeline: a **Conversation** section under Show thread, loaded on expand, with the four states and Open in T3.
   The same rendering for every runner; a t3 turn without `tool.*` events shows its replies and "Tool calls are in
   T3". A Playwright smoke test with a seeded harnessmux home, holding a t3 log with tool events (0.3.0) and one
   without (0.2.0).

Not in the slice: native Claude/Codex files, other machines, selective import into the ticket (evaluate separately),
live T3 state (DEV-849).

## Recommendation and cost

Typed reference in the job log, harnessmux `v:1` event log as the only source, a paged reader by job id, loaded on
expand; T3 jobs show the same conversation (tool calls from harnessmux 0.3.0 logs, replies only from older ones) and
Open in T3.

- Agent time: one ticket of about three fix-review-merge runs (Rust reader and API, UI section, smoke test).
- Money: no new service; agent runs at the usual rate.
- Upkeep: low. It follows one documented format (harnessmux, owned by the same owner); a new `v` shows the unsupported
  state until the reader learns it.

## Decision

Ravi, 2026-11-26 ([DEV-850](https://lotar.pyxzl.net/task/DEV-850), answered in INBOX-765): "expand (Load on open only)".
A run's conversation loads only when its phase is opened; nothing is prefetched.
