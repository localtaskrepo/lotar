# Harness tool design

Design for DEV-492, 2026-11-13. Status: approved by the owner on 2026-11-13; built as
[localtaskrepo/harnessmux](https://github.com/localtaskrepo/harnessmux) (DEV-504). lotar's switch to it is DEV-503.

The owner, 2026-11-13: "Why not pull it out and put it in its own repo next to lotar as a tool that allows
multiplexing between harnesses. Then lotar can use, we can use it, any other app that needs it could use it. And we
have a separate codebase that we can update at a higher frequency (for when the apis change) and faster than
rebuilding lotar each time." Decision record: standards `docs/decisions/2026-11-13-harness-tool.md`.

## Owner decisions

2026-11-13, after the first draft. He asked: "with this available why do we still provide support for other
harnesses? If we're doing this just because it's easier, it might also be a good opportunity to review the part of
the code since it's already a little older, and may have been written by less capable models." Then he chose:

- Interface: "Link the crate" (the draft proposed calling the CLI).
- lotar's own runners: "Remove right away" (the draft kept them next to the new runner until it worked).
- Review of the code that stays in lotar: "After the switch" (DEV-501).
- Asked to approve the design with these decisions in it: "Approve".

2026-11-13, after the approval:

- Asked whether automations should still be able to start T3 Code threads after the switch: "Yes, as a harnessmux
  agent". The tool gets a t3 agent driven through T3's CLI; lotar keeps no T3 code.
- On grouping chats under their projects: "harnessmux only". Every session records its project (the repository's
  main checkout, also for a worktree); Claude and Codex get no project setting of their own and no upstream requests.

## Recommendation

Build a small Rust tool of our own, working name `harnessmux`, on the Agent Client Protocol (ACP) with the official
Rust SDK. Model it on acpx, which already does most of what we need and which we tried: named sessions, a queue,
cancel from another process, resume after the agent process dies, and a session log another process can follow.
Do not ship acpx itself inside lotar: it is Node, pre-1.0, pins outdated adapters and leaves the mapping of model,
effort and permissions to each caller. HarnessRouter doesn't fit: it is a hosted service in Docker that runs its
own copies of the CLIs on provider API keys, not the owner's subscriptions and checkouts.

lotar links the crate and drops its own harness support in the same change: the per-CLI runners and the t3 runner
go, and the existing runner names become names of the tool's agents. The job code that stays in lotar gets a review
of its own after the switch.

## What lotar has today

- `crates/lotar-lib/src/services/agent_runner.rs` (1771 lines): 12 runner kinds (`codex`, `claude`, `copilot`, `gemini`, `cursor`,
  `amp`, `droid`, `opencode`, `kilo`, `antigravity`, `t3`, `command`), flag mapping for model, effort, permissions
  and MCP, and one stream parser per CLI. Almost none of it is lotar-specific.
- `crates/lotar-lib/src/services/agent_job_service.rs` (4629 lines): mostly lotar's own job logic (queue, prompts, worktrees,
  landing, rules), plus generic process handling: spawn through the wrapper, kill the process tree, read pipes,
  write stdin messages.
- `AgentProfileDetail` (`crates/lotar-lib/src/config/types.rs`): runner, permissions, model, effort, provider, session, budget,
  command, args, env, tools.
- Only the t3 runner continues sessions. Claude, Codex, OpenCode and Cursor session ids are parsed but never resumed.
  Only Claude takes messages while it runs (stream-json on stdin).
- Jobs run as T3 Code threads by default. Every missing piece waits for T3's CLI: child threads, effort, an existing
  worktree, rename (pingdotgg/t3code discussion 36281). T3 is itself an ACP client of the same adapters.

## Evaluation: what was tried

All runs on 2026-11-13 on the owner's Mac, in an empty scratch directory, with the owner's existing Claude Max and
ChatGPT sign-ins. Outputs stayed in the session scratchpad; they hold account details and are not checked in.

### acpx 0.22.0 (openclaw/acpx, MIT, Node)

Installed with `npm install acpx@0.22.0` into a scratch directory.

| Tried | Result |
| --- | --- |
| `acpx --format json claude exec "…"` | Works, 7 s. Starts `claude-agent-acp` through `npx`. The output is the raw ACP JSON-RPC stream, one message per line: `initialize`, `session/new`, `session/update` (`agent_message_chunk`, `usage_update` with cost, `session_info_update` with a title), and the `session/prompt` result with `stopReason` and token usage. |
| What the Claude adapter offers | `loadSession`, `resume`, `list`, `fork`, `close`, `additionalDirectories`, system prompt override, HTTP and SSE MCP servers, steering (messages while a turn runs). Config options `mode` (default, acceptEdits, plan, auto, bypassPermissions), `model`, `effort` (low … max), `fast`, `agent`. |
| `sessions new --name t1`, then two prompts with `-s t1` | The second prompt remembers the first. `status -s t1` shows the session id, the adapter command, pid, model and state. |
| Kill the agent process, prompt again | acpx starts a new adapter and calls `session/resume`; the session remembers. Works for Claude and Codex. |
| `--no-wait` prompt from a second process while a turn runs | Queued, then run after the turn. |
| `cancel -s t1` from a second process during a long turn | The turn ends with `stopReason: cancelled`. |
| Follow a running turn from another process | No attach command, but `~/.acpx/sessions/<id>.stream.ndjson` grows while the turn runs (364 → 404 → 553 → 831 lines), so a reader can tail it. |
| `acpx codex exec` | Fails. acpx pins `codex-acp@^1.1.5` (1.13.1), which picks `gpt-6.1-sol` from the owner's Codex config and gets `model_not_found` for a ChatGPT account. The error arrives as assistant text and the turn still ends `end_turn`, so a caller sees success. |
| Same through `--agent "npx -y @agentclientprotocol/codex-acp@2.6.0"` | Works. 2.6.0 offers `mode` (read-only, workspace-write, agent, agent-full-access), `model`, `reasoning_effort` (low … ultra), `fast-mode`, plus `resume`, `list`, `fork`, `subagents`. Named session, kill and resume work too. `set -s c2 reasoning_effort high` works. |
| `acpx copilot exec` (Copilot CLI 1.0.103, native `--acp --stdio`) | Works; `loadSession` true, no `resume`. |
| `acpx claude sessions` | Lists every Claude session on the machine, from other projects too (the adapter's `session/list`), not only acpx's. |

Not tried: Kilo, Gemini, OpenCode, Cursor, Droid and Antigravity are not installed here. acpx's registry starts them
as `kilo acp`, `gemini --acp`, `opencode acp`, `cursor-agent acp`, `droid exec --output-format acp`, and
Antigravity's `agy_acp_server`. acpx has no entry for Amp.

What acpx gets right and we copy: a session is a name plus a working directory; one owner process per session holds
the ACP connection and idles out after a TTL; other processes reach it over local IPC for prompt, queue, cancel,
status and config; every message goes to an append-only NDJSON file; resume falls back from `session/resume` to
`session/load`.

Why not use it as is:

- Node and npm at run time, `npx` downloads on first use, and a pre-1.0 interface ("Treat its CLI and runtime
  interfaces as evolving"). lotar is one Rust binary, and the owner's rule is one language per repo.
- The adapter versions lag (Claude 0.95 pinned, 0.107 current; Codex 1.x pinned, 2.6 current), and the lag already
  broke Codex for this account. We want to bump adapters on our own cadence.
- No common mapping: effort is `effort` for Claude and `reasoning_effort` for Codex, permission modes have different
  ids per agent, and acpx's own permission flags (approve-all, approve-reads, deny-all) don't express lotar's
  read-only/workspace/full levels.
- Errors that adapters send as text pass as success; we need a failed result.

### HarnessRouter v0.41.15 (Apache-2.0)

Repository created 2026-08-09, several releases a day. The PyPI package `harnessrouter` 0.1.0 is only a Python
client for its own protocol, the Unified Harness Protocol (UHP); the server is a Docker image.

| Tried | Result |
| --- | --- |
| `uv pip install harnessrouter` | A CLI with `discover`, `harnesses`, `models`, `run`, `stream`, `get`, `cancel`, `sessions`, talking HTTP to a server. |
| `docker run harnessrouter/harnessrouter` on loopback | A 1.9 GB image. The first start installs its own copies of 31 harness CLIs (Claude Code, Codex 0.154, Kilo 7.8.1, OpenCode, Gemini, …) into a volume, generates a password and a secret key, and serves a console, a gateway and an OpenAI Responses-compatible API. |
| `harnessrouter discover` and `harnesses` | Fail with a JSON decode error: every route, `/v1/...` too, redirects to the console's login page until you sign in. |
| Sign in with the generated password, `GET /api/harness/v1/models` | 31 backends, 0 models available. |
| `POST /api/harness/v1/responses` for Codex and Claude | Refused: "no provider configured for backend 'codex'" and "no connected provider serves it there". A task needs a provider API key or a cloud connection in its console; its docs mention no ChatGPT or Claude subscription sign-in. |

No task ran: we did not give it one of the owner's API keys. The container, volume and image were removed afterwards.

Why it doesn't fit: it runs harnesses inside its container as per-session users on provider API keys entered in its
console, against workspaces in its volume. lotar's jobs need the owner's own CLIs, sign-ins (subscriptions, not
API keys), git worktrees and MCP servers on the host. Its own protocol adds a hop over ACP rather than replacing it,
and a Docker service with a web console is more to run than lotar itself.

## Design answers

### ACP, acpx or per-CLI parsers

ACP. Every CLI we use speaks it, natively or through a maintained adapter: Claude (`claude-agent-acp`), Codex
(`codex-acp`), Copilot, Gemini, Kilo, OpenCode, Cursor, Droid, Antigravity. One client replaces nine of the per-CLI
parsers (Amp stays, below), and ACP gives what the parsers never had: resume, cancel, permission requests, messages
to a running session (Claude steering), config options with their allowed values.

Harnesses that need something of their own:

- Amp: no ACP server known. Keep lotar's current headless runner (`amp -x --stream-json`) as a legacy adapter in
  the tool, behind the same session interface, without resume.
- T3: not a harness but another ACP client in front of the same adapters. The tool drives it through T3's CLI as a
  legacy adapter like Amp (`t3 thread start` with provider, model and worktree; `thread send` for later turns;
  `thread show` to follow), so automations can still start T3 Code threads. T3's CLI has no effort, child threads,
  interrupt or permission answers; `capabilities` says so.
- `command`: lotar's escape hatch for any program stays in lotar, outside the tool.

### What the tool adds over ACP

- Session registry: a name, the agent, the working directory and its project (the repository's main checkout, from
  git's common dir, so a worktree belongs to its repository), the ACP session id, the adapter version, created and
  last-used times, the state (idle, running, waiting for permission, ended). Stored per user under
  `~/.local/state/harnessmux/` (`%LOCALAPPDATA%` on Windows), one JSON file per session.
- One owner process per running session holding the ACP connection, reachable over a Unix socket (named pipe on
  Windows): `send`, `cancel`, `answer` a permission request, `set` a config option, `status`. It exits after an idle
  TTL; the next `send` starts a new owner that resumes.
- Resume and cancel for every agent that offers them, with `session/load` as the fallback.
- One mapping, filled from each agent's advertised config options:
  - `model`: passed through; the tool rejects a value the agent doesn't list.
  - `effort`: low, medium, high, xhigh, max, mapped to `effort`, `reasoning_effort` or the agent's variant.
  - `permissions`: read-only, workspace, full, mapped to the agent's mode (Claude `plan`/`acceptEdits`/
    `bypassPermissions`, Codex `read-only`/`workspace-write`/`agent-full-access`) plus a policy for ACP permission
    requests (deny, allow reads, allow all, or ask the caller).
  - Extra writable directories through ACP `additionalDirectories`.
- An event stream of its own, JSONL, stable across agents and adapter versions: `session.started`,
  `message.delta`, `message`, `tool.started`, `tool.finished`, `permission.requested`, `usage`, `turn.finished`
  (stop reason, tokens, cost), `turn.failed`, `session.ended`. Each event carries the session's project, so lotar's
  UI can group by it, and keeps the raw ACP message under `raw`.
  Known error texts from adapters (`model_not_found`, auth failures) become `turn.failed`.
- MCP servers per session: the caller passes them (stdio, HTTP), the tool hands them to `session/new`. lotar passes
  its own `lotar mcp` server this way instead of writing per-CLI config flags.
- `capabilities`: installed agents, adapter versions, and per agent the models, effort values, modes, and whether it
  resumes, forks or takes messages while running.
- Pinned, tested adapter versions, bumped in the tool's own releases; `npx` only as an opt-in fallback.

### How the owner follows and answers sessions

lotar's web UI, not a viewer of the tool's own. lotar already shows a job's events live (SSE) and has a message box
(`send_message`), and the owner reaches it from his phone at lotar.pyxzl.net. With the tool, a job's events come from
the session's JSONL stream and the message box calls `send`, so this works for every agent instead of only Claude
and T3. Permission requests show up there as a question with the agent's options.

For sessions outside lotar (his own or another app's) the tool gets `harnessmux attach <name>`: it prints the stream
from the start and follows it, and reads lines from the terminal as messages. A web viewer of its own can come later
if a second app needs one; it is not part of the first release.

What T3 gives today and lotar's UI doesn't yet: in-thread tools (child threads, schedules, devices). Agents in lotar
jobs get child sessions through the tool's MCP server (below); schedules and devices stay with T3 for his chats.

### Interface

A Rust library crate and a CLI in one repo. lotar links the crate; the CLI is for the owner, other agents and other
apps. No central daemon: whoever starts a session owns it (lotar's process for lotar's jobs, a `harnessmux` process
for the CLI), holds its ACP connection and serves its socket, so `harnessmux send` and `attach` reach lotar's
sessions too. When the owner process stops (lotar restarts, the CLI idles out), the next `send` or `resume` starts a
new one that resumes the session.

```text
harnessmux run <agent> [--name N] [--cwd DIR] [--model M] [--effort E] [--permissions P]
               [--mcp FILE] [--add-dir DIR] [--json] [PROMPT | -f FILE]
harnessmux send <name> [--wait] [PROMPT | -f FILE]     # queued when a turn runs
harnessmux attach <name> [--from-start] [--json]       # follow; stdin lines become messages; --project: all of a repo
harnessmux resume <name>                               # reconnect after a crash or reboot
harnessmux cancel <name>
harnessmux answer <name> <request-id> <option>         # permission request
harnessmux set <name> <option> <value>
harnessmux list [--json] [--project [DIR]]  |  status <name>  |  close <name>
harnessmux capabilities [<agent>] [--json]
harnessmux mcp                                          # MCP server: start, send, wait, cancel child sessions
```

`--json` prints the event stream on stdout; the exit code is 0 for a finished turn, 1 for a failed one, 2 for
cancelled. The crate exposes the same as `Session::start`, `send`, `events()` (a stream of the event type),
`cancel`, `answer`, `resume`, `capabilities`, and the profile type.

What changes most often lives in data, not code: the agent table (command, pinned adapter version, the ids of each
agent's model, effort and mode options) is a TOML file shipped with the crate and overridable per user. An adapter
bump or a renamed option is a data change that reaches lotar without a lotar build; a change to the crate's code
reaches lotar with the next lotar release.

`AgentProfileDetail` moves into the crate as its profile: agent, model, effort, permissions, MCP servers, extra
directories, env, budget, and for the t3 agent provider, worktree base and branch. lotar keeps its config keys and
converts. `session: continue` stays in lotar.

lotar keeps everything that is about tickets: jobs, queue, worktrees, landing, prompts, instructions, rules, the
reviewer and merge flow, the web UI.

### Repo, license, release cadence

- Repo `localtaskrepo/harnessmux`, MIT like lotar, public from the start (it holds nothing personal).
- The name is a proposal: `harnessmux` is free on crates.io and GitHub (taken on npm, which we don't publish to).
- Releases when an adapter or ACP changes: the crate on crates.io, and the CLI as a binary on GitHub Releases and
  Homebrew. Agent-table updates reach lotar at run time; code changes reach it when lotar bumps the crate.
- CI runs recorded ACP sessions per agent; a weekly job runs a real prompt against the current adapters.

### How lotar adopts it, and what goes away

One change, no transition period:

1. lotar depends on the `harnessmux` crate and runs every agent job through it. Job events come from the session's
   event stream; the message box and cancel call `send` and `cancel`; permission requests become questions in the
   job view.
2. Profiles keep their `runner` key: `claude`, `codex`, `copilot`, `gemini`, `cursor`, `amp`, `droid`, `opencode`,
   `kilo`, `antigravity` and `t3` become the tool's agent of that name. `command` stays in lotar as the escape hatch
   for any program.
3. `session: continue` works through the tool's named sessions, one per ticket (named `DEV-492` and so on), for all
   agents.
4. Removed in the same change: the per-CLI command builders and parsers in `agent_runner.rs` (about 1700 lines),
   the stream fixtures under `crates/lotar-lib/tests/fixtures/agent-streams/`, the t3 runner and its thread handling in
   `agent_job_service.rs` (about 770 of its 4000 non-test lines), and the runner docs that describe single CLIs
   (`docs/developers/agent-cli-research.md` moves to the tool's repo).
5. After the switch, a review ticket goes through what stays in `agent_job_service.rs`: queue, worktrees, landing,
   prompts, rules, reviewer and merge flow.

## Risks

- No fallback: when an adapter or the tool breaks, lotar's agent jobs stop until a fix ships. The tool's recorded
  tests and the weekly live run have to catch it first, and a lotar release can pin the previous crate version.
- Adapters are young and change fast (claude-agent-acp 0.x, codex-acp went 1.x to 2.x). Pinning plus recorded tests
  catch it; that is the reason for a separate repo.
- Sign-in stays with each CLI. A job fails when a CLI is signed out; the tool reports it as `turn.failed` with the
  agent's auth message.
- Windows: named pipes instead of Unix sockets, so the tool's CI needs a Windows job like lotar's `windows_checks`.

## Question to the owner

Approve this design with your decisions in it: a Rust tool `harnessmux` on ACP in its own MIT repo next to lotar,
modelled on acpx, linked into lotar as a crate in one change that removes lotar's own harness runners and the t3
runner, followed from lotar's web UI?
