# Agent CLI research notes

Headless interfaces of agent CLIs, gathered on 2026-10-27 for DEV-203 by read-only LoTaR agent jobs (Claude, web search and fetch) from each CLI's public docs and repositories. LoTaR's `cursor`, `amp`, `droid`, `opencode`, `kilo` and `antigravity` runners are built from these notes and were not run live. Verify against a real install before relying on details, and update the runner and its `*-docs.jsonl` fixture when a real run differs.

## OpenCode

Official docs cover the CLI flags, install and permissions. The event schema comes only from community write-ups. I found no official page that documents it.

### 1. Install and auth
- **Package and binary:**
  - The binary is `opencode`.
  - Install with `npm install -g opencode-ai` (also bun, pnpm and yarn), `curl -fsSL https://opencode.ai/install | bash`, or `brew install anomalyco/tap/opencode`.
  - OpenCode 1 and OpenCode 2 both ship the `opencode` command. Plain `brew install opencode` and the Arch `pacman` package now install V2. The docs don't say whether `run` behaves differently in V2.
- **Sign-in:**
  - `opencode auth login` stores credentials in `~/.local/share/opencode/auth.json`. `opencode auth list` and `opencode auth logout` also exist.
  - In the TUI, `/connect` does the same.
  - The docs don't list provider API-key environment variables on these pages. They point to the Providers page, which I didn't fetch.
  - A headless job needs `auth.json` to exist already.
- Sources: https://opencode.ai/docs/ and https://opencode.ai/docs/cli/

### 2. Headless invocation
- The command is `opencode run [message..]`. The prompt is positional arguments, for example `opencode run Explain the use of context in Go`.
- The docs show no stdin or prompt-file input. `-f/--file` attaches files to the message but doesn't carry the prompt. Whether piped stdin works is not documented.
- `--dir <path>` sets the working directory.
- `--title` sets the session title.
- `--agent` picks an agent.
- `--command` runs a named command, with `message` as its arguments.
- `--attach http://localhost:4096` reuses a running `opencode serve`. `--username/-u` and `--password/-p` are basic-auth credentials for it. They default to `OPENCODE_SERVER_USERNAME` and `OPENCODE_SERVER_PASSWORD`.
- `--port` sets the port of the local server that `run` starts. It defaults to a random one.
- Source: https://opencode.ai/docs/cli/

### 3. Machine-readable output
- **Flag:** `--format json` gives raw JSON events. The other value is `default` (formatted text). The output is JSONL, one object per line, each with `type`, `timestamp` (milliseconds) and `sessionID` (`ses_…`).
- **Event types:** these come from the community Untether cheatsheet (https://littlebearapps.com/help/untether/opencode-stream-json-cheatsheet/, last updated February 2026). It is not official, and only the opening fragment of each example line was retrievable.
  - **Session start:** there is no dedicated event. The `sessionID` appears on every event, starting with `step_start`.
    - `{"type":"step_start","timestamp":1767036059338,"sessionID":"ses_494719016ffe85dkDMj0FPRbHK","part":{…`
    - The `part` holds `id`, `messageID`, `type:"step-start"` and a git snapshot hash.
  - **Assistant text:** `{"type":"text",…,"part":{…` with the text in `part.text`, plus start and end times.
  - **Tool calls:**
    - `{"type":"tool_use",…` is emitted only when the tool finishes. `state.status` is always `"completed"` in CLI output.
    - The part carries `callID`, tool name, `input`, `output`, `title`, `metadata` and times.
    - The cheatsheet says a non-zero `metadata.exit` means that tool failed.
  - **Final result:**
    - `{"type":"step_finish",…` carries `part.reason` (`"stop"` means final, `"tool-calls"` means the loop continues), a snapshot hash, `cost` in USD, and `tokens` (input, output, reasoning, cache read and write).
    - There is one `step_finish` per step. The run is done when the last one has `reason:"stop"`. If `reason` is missing, the cheatsheet treats a clean process exit as success.
  - **Errors:** `{"type":"error",…,"error":{"name":…,"data":{"message":…}}}`. A top-level `message` may also appear.
- **Not documented:** the full JSON schema and the exact field names for tokens and cost. Capture a real run and test the parser against it before relying on them.
- Another community reference is the Takopi runner docs on izackpgit.duckdns.org. It agrees on the event types.

### 4. Model and reasoning-effort flags
- `-m/--model provider/model`, for example `anthropic/claude-sonnet-4-5`. Whether a bare model name works is not documented.
- `--variant` sets "provider-specific reasoning effort". The accepted values aren't listed on this page and depend on the provider.
- `--thinking` shows thinking blocks.
- A default model can also be set in `opencode.json`. I didn't verify that page.
- Source: https://opencode.ai/docs/cli/

### 5. Permissions and sandbox
- **No sandbox:** there is no read-only or sandbox mode flag. Permissions are config only.
- **Config:** `opencode.json`, or `OPENCODE_CONFIG` for a file path, or `OPENCODE_CONFIG_CONTENT` for inline JSON.
  - Each rule is `"allow"`, `"ask"` or `"deny"`.
  - `"permission": "allow"` sets everything at once.
  - Per-tool rules go under `permission`, for example `"bash"` and `"edit"`.
  - The `*` catch-all goes first because the last matching rule wins.
  - Most tools default to allow, and `.env` reads need approval.
  - Example, which gives a read-only run: `{"permission":{"*":"ask","edit":"deny","bash":{"*":"ask","git *":"allow","rm *":"deny"}}}`
- **`--auto`:** auto-approves permissions that are not explicitly denied, and deny rules still apply. This is the nearest thing to full auto.
- **`OPENCODE_PERMISSION`:** the CLI page lists it as an inline JSON permissions config. The permissions page doesn't mention it, so test its exact shape.
- **`ask` without `--auto`:** the docs don't say what happens when `run` is headless. It may hang or reject. To be safe, set explicit rules or pass `--auto`.
- Sources: https://opencode.ai/docs/permissions/ and https://opencode.ai/docs/cli/

### 6. Extra writable directories
- There is no CLI flag for this.
- `permission.external_directory` governs paths outside the start directory. It defaults to `"ask"`. For example, `{"external_directory":{"~/projects/personal/**":"allow"}}` allows that path.
- Allowed directories inherit the workspace defaults, so edits are allowed there unless an `edit` rule denies them.
- A job could pass this through `OPENCODE_CONFIG_CONTENT` or `OPENCODE_PERMISSION`.
- Source: https://opencode.ai/docs/permissions/

### 7. Resuming and follow-up input
- `-c/--continue` continues the last session.
- `-s/--session <id>` continues a specific session. Take the id from `sessionID` in the events.
- `--fork` forks the session when used with either flag.
- Follow-up input while the process is running is not documented. A follow-up message means starting another `opencode run -s <id> "<message>"`.
- Source: https://opencode.ai/docs/cli/

### 8. Exit codes and failures
- Exit codes are not documented.
- Failures appear as `error` events in the JSON stream.
- Failed tools show up as a non-zero `metadata.exit` on `tool_use`.
- Safest approach for LoTaR: treat a final `step_finish` with `reason:"stop"` and no `error` event as success. Treat an `error` event or a non-zero process exit as failure. Verify both against a real failing run, such as bad credentials, a missing model, or a denied permission.

## Amp

### 1. Install and auth
- **Install:** `curl -fsSL https://ampcode.com/install.sh | bash` (Mac/Linux/WSL). The docs also list Windows and Homebrew options but give no commands. https://ampcode.com/docs/cli
- **npm:** the package is now `@ampcode/cli`, renamed from `@sourcegraph/amp`. The old names were scheduled for removal on 2026-06-15. I couldn't confirm whether they still resolve. Amp recommends the install script over npm. https://ampcode.com/news/npm-package-changes
- **Interactive sign-in:** `amp login`, plus `amp account list`, `amp account switch <email-or-user-id>` and `amp logout [--all]`. https://ampcode.com/docs/cli
- **Scripts and CI:** set `AMP_API_KEY` to an access token from Settings. The token starts with `sgamp_`.
  - The CLI rejects the short-lived session token that `amp login` stores.
  - If `AMP_API_KEY` is set, it takes precedence over saved accounts. https://ampcode.com/docs/cli/execute-mode

### 2. Headless invocation
- **Command:** `amp -x "prompt"` (`--execute`). It runs until the turn ends, prints the final message and exits.
- **Prompt passing:**
  - As an argument: `amp -x "..."`.
  - Via stdin: `echo "..." | amp -x`.
  - Both together: `cat file | amp -x "question"` (the agent sees both).
- **Redirected stdout:** redirecting stdout turns on execute mode automatically.
- **Prompt from a file:** there is no documented flag, so pipe it with `cat`.
- **Other flags:**
  - `--attach <path>` (repeatable) is documented for remote runner execution (`--executor runner:<id>`).
  - `--mcp-config '<json>'`.
  - `--settings-file <path>`.
  - `--timings` (timing report on stderr).
  - `--plugin-ready-timeout [s]`.
  - `--executor local|orb|runner:<id>`.
  - `-ox` (run on Amp's servers and exit).
- Source: https://ampcode.com/docs/cli/execute-mode

### 3. Machine-readable output
**Flags:** `amp -x "..." --stream-json` prints newline-delimited JSON, one object per line. `--stream-json-thinking` adds thinking blocks. `--stream-json-input` is covered in section 7. Every message carries `session_id`, which is the thread ID and starts with `T-`. Sources: https://ampcode.com/docs/cli/streaming-json and https://ampcode.com/news/streaming-json

The docs page returned fields and schematic examples, not verbatim sample lines. The lines below are illustrative samples from the docs, not captured real output.

| Event | Shape |
|---|---|
| Session start | `{"type":"system","subtype":"init","cwd":"/home/dev/app","session_id":"T-example-1","tools":["Bash","Read"],"mcp_servers":[{"name":"postgres","status":"connected"}],"subagents":[]}` |
| Assistant text | `{"type":"assistant","message":{"type":"message","role":"assistant","content":[{"type":"text","text":"8"}],"stop_reason":"end_turn"},"parent_tool_use_id":null,"session_id":"T-example-1"}` |
| Tool call | A `tool_use` block inside an `assistant` message, with `stop_reason: "tool_use"`: `{"type":"tool_use","id":"toolu_example_1","name":"Read","input":{"path":"/home/dev/app"}}` |
| Tool result | A `tool_result` block inside a `user` message: `{"type":"tool_result","tool_use_id":"toolu_example_1","content":"[\"index.js\"]","is_error":false}` |
| Final result | `{"type":"result","subtype":"success","duration_ms":5400,"is_error":false,"num_turns":1,"result":"8","session_id":"T-example-1"}` |
| Error | See below. |

- **Init fields:** `agent_mode?` is optional. `mcp_servers[].status` is one of `awaiting-approval`, `authenticating`, `connecting`, `reconnecting`, `connected`, `denied`, `failed` or `blocked-by-registry`. `subagents` lists background subagents still running.
- **Assistant content blocks:** `text`, `tool_use`, `thinking` and `redacted_thinking` (the thinking blocks only with `--stream-json-thinking`).
- **Assistant `stop_reason`:** `end_turn`, `max_tokens`, `stop_sequence`, `tool_use`, `pause_turn`, `refusal` or null.
- **Usage:** it sits on the assistant `message.usage`, with `input_tokens`, `output_tokens`, optional cache token counts, `cache_creation` (5m and 1h ephemeral), `max_tokens` and `service_tier`.
- **Result usage and cost:** `result` has an optional `usage` and `permission_denials?` (a string array). No cost field is documented anywhere; derive cost from token usage.
- **Errors:**
  - A failed `result` has `subtype` `error_during_execution` or `error_max_turns`, `is_error: true` and an `error` string.
  - The docs also show a `system` message with `subtype: "error_max_turns"` and `error`, for example `{"type":"system","subtype":"error_max_turns","error":"Turn limit reached","session_id":"T-example-1"}`.
  - Parse both.
- **Other events:** `{"type":"system","subtype":"subagent_finished","tool_use_id":"...","status":"completed|failed|cancelled"}`. A `user` message can also carry `subagent_tool_use_id` and `parent_tool_use_id`.
- **Last line:** the `result` message marks the end of the run.

### 4. Model and reasoning-effort flags
- **Modes:** the built-in modes are `low`, `medium`, `high` and `ultra`. Each bundles a model, reasoning effort, system prompt and tools. Plugins can add custom modes. https://ampcode.com/docs/models-and-subagents
- **SDK options:** the TypeScript SDK exposes `mode` (default `medium`) and `effort` (`none` through `max`, for supported modes). https://ampcode.com/docs/sdk/typescript
- **Not documented:** the docs fetched don't show the exact CLI flag names, such as `--mode`, and `--effort` is not documented either. I couldn't confirm them. Run `amp --help` on the build you target.
- **Third-party names:** a third-party Elixir SDK README mentions an older mode name, `deep`. Treat it as possibly outdated.
- **Fast mode:** `--fast` (alias for `--features fast`) enables Fast mode for one invocation. Execute mode defaults to Standard each time. https://ampcode.com/docs/cli/execute-mode
- **Choosing a model:** there is no documented way to pick a specific model name. Modes are the unit of choice.

### 5. Permissions and sandbox
- **Pre-approve everything:** `--dangerously-allow-all`. The TypeScript SDK exposes it as `dangerouslyAllowAll` (default false). E2B's Amp docs show `-x` paired with `--dangerously-allow-all` for non-interactive use. https://ampcode.com/docs/sdk/typescript and https://docs.e2b.dev/agents/amp.md
- **Fine-grained rules:** the third-party Elixir SDK writes a `permissions` array into a temporary settings file and passes it with `--settings-file`. That is a third-party detail. The exact rule format was not confirmed, because https://ampcode.com/docs/permissions returned 404.
- **Tool and MCP controls:** `amp.tools.disable` (settings, supports `builtin:` prefixes and `*` globs), `amp.mcpPermissions` and `--mcp-config`. The SDK's `enabledTools` maps to `amp.tools.enable`. https://ampcode.com/docs/cli/settings
- **Read-only and sandbox:** there is no documented read-only or sandbox flag. Use a permissions rule set or an external sandbox.
- **Settings files:** user settings are `~/.config/amp/settings.json`. Workspace settings are the nearest `.amp/settings.json`, and they override user settings.
- **Denied tools:** `permission_denials` in the `result` message reports them.

### 6. Extra writable directories
Not documented. I found no `--add-dir` equivalent. The SDK has `cwd` (working directory of the CLI subprocess). `--runner-dir` applies only to remote runners.

### 7. Resuming and follow-up input
- **Continue a thread:** `amp threads continue [T-id]`. It combines with `-x --stream-json`, so a conversation can span several invocations. The docs example continues with `-x "and now add 7 to that"` and gets `num_turns: 2`. https://ampcode.com/news/streaming-json and https://ampcode.com/docs/threads
- **Wake an orb thread:** `amp threads continue T-… -ox '<message>'`.
- **List and search:** `amp threads list` and `amp threads search`.
- **SDK equivalent:** `continue: true | "<threadId>"`.
- **Streaming input while running:** `--stream-json-input` requires both `-x` and `--stream-json`.
  - Each stdin line is a JSON object: `{"type":"user","message":{"role":"user","content":[{"type":"text","text":"now add 8 to that"}]}}`.
  - Optional `"steer": true` handles the message at the next interruption point while the agent is busy.
  - Image blocks (base64) are accepted.
  - Amp processes each message as it arrives. It exits after stdin closes and the agent finishes, and it waits for background subagents.
  - Source: https://ampcode.com/docs/cli/streaming-json

### 8. Exit codes and failures
- **Exit codes:** not documented. Don't rely on specific codes. Treat the final `result` line as authoritative and check `is_error`.
- **Failures:** they appear as `result` or `system` messages with `error_during_execution` or `error_max_turns`.
- **MCP problems:** they show up in `init.mcp_servers[].status` (`failed`, `denied`, `blocked-by-registry`).
- **Retries:** reusing the same `requestId` on the same thread avoids duplicate user messages (SDK).
- **Unverified:** whether the process exits non-zero on `is_error: true`, how auth failures are reported (stderr text versus JSON), and what happens if the stream ends with no `result` line. I'd treat a missing `result` as a failure. Check these with a live run before relying on them.

**Sources:**
- https://ampcode.com/docs/cli
- https://ampcode.com/docs/cli/execute-mode
- https://ampcode.com/docs/cli/streaming-json
- https://ampcode.com/news/streaming-json
- https://ampcode.com/docs/cli/settings
- https://ampcode.com/docs/threads
- https://ampcode.com/docs/models-and-subagents
- https://ampcode.com/docs/sdk/typescript
- https://ampcode.com/news/npm-package-changes
- https://docs.e2b.dev/agents/amp.md

## Factory Droid

Only the main `droid exec` reference page was read; it shows no full example lines for the event stream.

### 1. Install and auth
- **Install:** `curl -fsSL https://app.factory.ai/cli | sh`. Homebrew, Windows and npm installers also exist, but the page shows no package names for them. The binary is `droid`.
- **Platform:** Linux needs a glibc image such as Debian or Ubuntu. Alpine is not supported.
- **Auth:** Create a key in Factory's Settings page, then `export FACTORY_API_KEY=fk-...`.
- **Source:** https://docs.factory.com/cli/droid-exec/overview (docs.factory.ai redirects there).

### 2. Headless invocation
The syntax is `droid exec [options] [prompt]`. The prompt can be passed three ways:
- **Argument:** `droid exec "analyze code quality"`
- **File:** `droid exec -f prompt.md`
- **Stdin:** `echo "summarize repo structure" | droid exec`

Other flags that matter for headless runs:
- `--cwd <path>` sets the working directory (an absolute path is recommended).
- `-w, --worktree [name]` runs in an isolated git worktree. A clean worktree is removed on exit and a dirty one is kept.
- `--append-system-prompt <text>` and `--append-system-prompt-file <path>` add to the system prompt.
- `--settings <path>`, `--tag <spec>` (repeatable) and `--log-group-id <id>` are also listed.

### 3. Machine-readable output
The flag is `-o, --output-format`, with these values:
- `text` is the default and prints the final response only.
- `json` prints one object when the run ends.
- `stream-json` prints one event per line during the run.
- `stream-jsonrpc` is the full JSON-RPC protocol.
- `acp` also appears in the options list.

**`json` result keys.** The docs show the keys but no values:
- Top level: `type`, `subtype`, `is_error`, `duration_ms`, `num_turns`, `result`, `session_id`, `usage`.
- Inside `usage`: `input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`, `factory_credits`, `ttft_ms`.

**`stream-json` event types:**
- The documented types are `system` (session start), `message` (assistant text), `tool_call`, `tool_result` and a final `completion`.
- The only line-level hint is the doc's example `jq -r 'select(.type == "tool_call") | .toolName'`, so tool calls carry a `toolName` field.
- **Not documented:** full example lines, the exact fields of each event, the shape of error events, and whether `completion` carries usage and cost like the `json` result. I'd guess it does, but that is only a guess. Someone needs to run `droid exec -o stream-json` on a trivial prompt and capture the lines before the parser is written.

### 4. Model and reasoning-effort flags
- `-m, --model <id>`
- `-r, --reasoning-effort <level>`, which defaults per model.
- Custom models use `custom:<Display-Name-with-dashes>-<0-based index>`, for example `custom:My-Custom-Model-0`. Reasoning effort does not apply to them.
- Separate spec-mode and mission-mode flags exist: `--spec-model`, `--spec-reasoning-effort`, `--use-spec`, `--worker-model`, `--worker-reasoning-effort`, `--validator-model`, `--validator-reasoning-effort`.
- The page does not list valid model IDs or effort levels.

### 5. Permissions and sandbox
- **Default (no flags):** read-only. Droid can inspect files and run read-only git commands but can't edit.
- **`--auto low`:** creates and edits low-risk project files.
- **`--auto medium`:** adds trusted package installs, builds, tests, `git commit`, `git checkout` and `git pull`.
- **`--auto high`:** adds remote writes, deployments and migrations, subject to hard safety checks.
- **`--skip-permissions-unsafe`:** skips permission prompts, but command blocks still apply. It can't be combined with `--auto`, and the docs say to use it only in disposable environments.
- **Tool selection:** `--only-tools`, `--add-tools`, `--remove-tools` and `--list-tools`. Tool IDs include `Read` and `Execute`, plus MCP selectors `MCP:<server>` or `MCP:<server>/<tool>`.
- **Unknown tool name:** fails the run before it starts.
- **Mission mode (`--mission`):** requires `--auto high` or `--skip-permissions-unsafe`.
- **Not documented:** an OS-level sandbox, and per-command allowlists in the style of `Bash(git:*)`.

### 6. Extra writable directories
Not documented. The page shows no `--add-dir` or `--additional-dir` equivalent. `--cwd` and `-w` are the only directory controls it lists.

### 7. Resuming and follow-up input
- **Continue a session:** `-s, --session-id <id>`. Take the `session_id` from `-o json` output (or presumably from the `system` event in `stream-json`).
- **Fork a session:** `--fork <id>` creates a new session ID.
- **Multi-turn input while running:** `--input-format stream-jsonrpc` together with `--output-format stream-jsonrpc`. The methods are `droid.initialize_session`, `droid.load_session`, `droid.add_user_message`, `droid.session_notification`, `droid.request_permission` and `droid.ask_user`.
- **Deprecated:** the older `stream-json` input mode prints a warning.
- **Not documented:** the JSON-RPC message payloads. They are on separate pages I didn't fetch.

### 8. Exit codes and failures
- **Exit 0:** the run completed, which does not mean the task succeeded. LoTaR should also check `is_error` and `subtype` in the result.
- **Non-zero:** the run stopped early, for example on a permission stop, an invalid flag or an API error.
- **Action above the autonomy level:** the run fails fast, the error goes to stderr, and earlier edits stay in the working tree.
- **Not documented:** specific exit-code numbers, and the JSON shape of an error event.

**Source for all sections:** https://docs.factory.com/cli/droid-exec/overview

## Cursor CLI

### 1. Install and auth
- **Install:** Cursor's headless page says to use a curl script on macOS, Linux and WSL, or a PowerShell command on Windows. The exact install command was not in the pages I fetched. I recall it as `curl https://cursor.com/install -fsS | bash`, but treat that as unverified. [headless](https://cursor.com/docs/cli/headless)
- **Binary name:** The docs now use `agent`. The older name was `cursor-agent`. The pages I fetched don't say whether the old name still works, so check both on PATH.
- **API key (for automation):** Create a user API key in the Cursor Dashboard under API Keys, then `export CURSOR_API_KEY=...`. You can also pass `--api-key <key>` for one command. [authentication](https://cursor.com/docs/cli/reference/authentication)
- **Interactive login:** `agent login` opens a browser. Set `NO_OPEN_BROWSER=1` to print the URL instead. `agent status` shows the auth state and `agent logout` clears credentials.
- **Which source was used:** The `system/init` event reports it in `apiKeySource` (`env`, `flag` or `login`).

### 2. Headless invocation
- **Command:** `agent -p "prompt" --output-format stream-json [--force] [--model X]`. `-p` or `--print` is non-interactive. [headless](https://cursor.com/docs/cli/headless)
- **Prompt passing:** The prompt is a positional argument, as in `agent --api-key KEY "implement user authentication"` from the authentication page. Stdin and file input are not documented in the pages I fetched.
- **Writes:** By default, print mode only proposes changes. `--force` (alias `--yolo`) lets it modify files.
- **Files and images:** Put file paths in the prompt text. The agent reads them with tool calls.

### 3. Machine-readable output
- **Flags:** `--output-format text|json|stream-json`, used with `--print`. `text` is the default and gives only the final answer. `json` gives structured output with a `.result` field.
- **Partial output:** `--stream-partial-output` adds incremental text deltas. It requires `--print` and `stream-json`. [parameters](https://cursor.com/docs/cli/reference/parameters)
- **Format:** NDJSON, one event per line ending in `\n`. Ignore unknown fields. [output-format](https://cursor.com/docs/cli/reference/output-format)
- **Session start:** `{"type":"system","subtype":"init","apiKeySource":"login","cwd":"/Users/user/project",...}`. It also carries `session_id`, `model` and `permissionMode`. The docs say `tools` and `mcp_servers` may be added later.
- **User echo:** `{"type":"user","message":{"role":"user","content":[{"type":"text","text":...}]},"session_id":...}`.
- **Assistant text:** `{"type":"assistant","message":{"content":[{"type":"text","text":...}]},"session_id":...}`. Each event is one complete segment, meaning the text between tool calls.
  - With `--stream-partial-output`, events can carry `timestamp_ms` and `model_call_id`.
  - Append only the deltas, which have `timestamp_ms` and no `model_call_id`.
  - Skip the pre-tool-call flush (both fields) and the end-of-turn flush (neither field), because they duplicate the deltas.
- **Tool call start:** `{"type":"tool_call","subtype":"started","call_id":...,"tool_call":{...}}`. The `tool_call` object is keyed by tool type:
  - `readToolCall.args.path`
  - `writeToolCall.args` with `path`, `fileText` and `toolCallId`
  - Other tools may use `function` with `name` and `arguments`.
- **Tool call end:** `{"type":"tool_call","subtype":"completed","call_id":...}` with `result.success` under the same key.
  - `readToolCall.result.success` has `content`, `isEmpty`, `exceededLimit`, `totalLines` and `totalChars`.
  - `writeToolCall.result.success` has `path`, `linesCreated` and `fileSize`.
  - Match start and completion events by `call_id`.
- **Final result:** `{"type":"result","subtype":"success","is_error":false,"result":"<full text>","duration_ms":...,"duration_api_ms":...,"session_id":...,"request_id":...}`. `request_id` is optional. `duration_api_ms` currently equals `duration_ms`.
- **Usage and cost:** Not documented. The result event has no token or cost fields in the docs.
- **Thinking:** `thinking` events are suppressed in print mode.
- **Errors:** There is no documented error event. The process exits non-zero, the error goes to stderr, and the stream may end with no `result` event. The `result` event has an `is_error` field, but the docs only show it as `false` on success.
- **Caveat:** I could not quote the full example lines verbatim, because the docs' lines are long and my fetch tool shortened them. Verify against a real run.

### 4. Model and reasoning-effort flags
- **Model:** `--model <model>`. `--list-models` lists the available ones. [parameters](https://cursor.com/docs/cli/reference/parameters)
- **Reasoning effort:** There is no documented flag. Effort appears to be part of the model choice, so check `--list-models`, but I did not confirm this.

### 5. Permissions and sandbox
- **Read-only or plan:** `--mode plan` (or `--plan`) and `--mode ask`. Without `--force`, print mode only proposes changes.
- **Full auto:** `-f`, `--force` or `--yolo`. The docs say this "allows commands unless explicitly denied".
- **Sandbox:** `--sandbox enabled|disabled`. A separate `sandbox run` subcommand has `--allow-paths`, `--readonly-paths`, `--blocked-patterns` and `--network`. The permissions page does not describe sandbox behaviour.
- **MCP servers:** `--approve-mcps` approves all of them.
- **Workspace trust:** `--trust` trusts the workspace without prompting. It is headless-only.
- **Pre-approving tools:** Use `permissions.allow` and `permissions.deny` arrays in `~/.cursor/cli-config.json` (global) or `<project>/.cursor/cli.json` (project). [permissions](https://cursor.com/docs/cli/reference/permissions) Deny rules override allow rules.
  - Example: `{"permissions":{"allow":["Shell(git)","Read(src/**/*.ts)"],"deny":["Shell(rm)","Read(.env*)"]}}`
  - Token types are `Shell(cmd)` (first token only, or `Shell(curl:*)`), `Read(glob)`, `Write(glob)`, `WebFetch(domain)` and `Mcp(server:tool)`.
  - `WebFetch` asks for approval unless there is an allow entry.
  - Relative paths are scoped to the workspace. Absolute paths can reach outside it.

### 6. Extra writable directories
- There is no `--add-dir` style flag.
- `--workspace <path>` sets the workspace directory.
- Absolute `Write(...)` allow tokens can reach outside the project.
- `sandbox run --allow-paths` exists, but I could not confirm how it applies to the agent.

### 7. Resuming and follow-up input
- **Resume:** `--resume [chatId]` resumes a chat. `--continue` is an alias for `--resume=-1`, meaning the previous session.
- **Session ID:** Take `session_id` from the `system/init` event and pass it back with `--resume`.
- **While running:** Sending follow-up input to a running print-mode process is not documented. There is no documented stream-json input format, so a follow-up looks like a new `-p --resume <id>` run.
- **Other flags:** `-w` / `--worktree [name]` runs in a new Git worktree. `-H` / `--header` is also available.

### 8. Exit codes and failures
- **Exit codes:** Only "non-zero on failure" is documented. There are no specific code values.
- **Reporting:** The error goes to stderr. The stream may end with no `result` event. [output-format](https://cursor.com/docs/cli/reference/output-format)
- **Parser advice:** Treat a missing `result` event, or a non-zero exit, as a failure. Use stderr as the message.

Sources:
- https://cursor.com/docs/cli/headless
- https://cursor.com/docs/cli/reference/output-format
- https://cursor.com/docs/cli/reference/parameters
- https://cursor.com/docs/cli/reference/authentication
- https://cursor.com/docs/cli/reference/permissions

## Google Antigravity CLI

Antigravity does have a headless CLI, called `agy`. Everything below comes from the official docs, the official GitHub issue tracker and a third-party cheat sheet. I did not run `agy` myself. Version numbers cited are 1.2.4 (cheat sheet) and fixes up to 1.1.8.

### 1. Install and auth

- **Install:** `curl -fsSL https://antigravity.google/cli/install.sh | bash` on Linux and macOS. The binary is `agy` and goes to `~/.local/bin`. Check it with `agy --version`. The installer verifies a SHA512 checksum. The binary needs AES-NI support (cheat sheet).
- **Interactive sign-in:** on a desktop, `agy` uses the OS keyring or opens a browser on first run. Over SSH it prints a URL, and you paste the code back.
- **API key mode:** this is the practical choice for unattended jobs. Create `~/.gemini/antigravity-cli/settings.json` containing `{ "modelProvider": "gemini" }`, then export `GEMINI_API_KEY`.
- **No credentials:** with no cached credentials in a non-interactive environment, the run exits with an `authentication required` error instead of prompting. This is documented.
- **Other:** `GOOGLE_GEMINI_BASE_URL` redirects model calls to a compatible endpoint (cheat sheet).
- **Policy risk:** the official FAQ says `agy -p` is meant for scripting and CI. It also says wrapping `agy` inside third-party coding agents or applications is not supported under consumer plans. That could affect LoTaR's use.

Sources: https://computingforgeeks.com/antigravity-cli-cheat-sheet/, https://antigravity.google/docs/cli/reference, https://antigravity.google/docs/cli/headless, https://antigravity.google/docs/faq/

### 2. Headless invocation

- **Command:** `agy -p "<prompt>" --output-format stream-json`. The flag has three equivalent spellings: `-p`, `--print` and `--prompt`.
- **Prompt delivery:** the prompt is an argument to `-p`. Reading the prompt from stdin works only with `--input-format stream-json` (see section 7). Passing the prompt from a file is not documented.
- **Timeout:** `--print-timeout` defaults to 5m per the headless docs and cheat sheet. The reference page says the default is `0s`, which means wait until the turn completes. The docs disagree, so set it explicitly.
- **Slash commands:** `--disable-slash-commands` treats input starting with `/` as plain text. Use it for ticket text that may start with a slash.
- **Known bug with non-TTY stdout:** GitHub issue #76 reported `agy -p` exiting 0 with empty stdout when stdout was a pipe, redirect or subprocess. The maintainer says 1.1.1 fixed several parts of this:
  - Server errors now go to stderr with a non-zero exit code.
  - Stdin is no longer read when the prompt is given by flag.
  - The issue was closed on 2026-07-12, but I found no confirmation from reporters on 1.1.1 or later.
  - Workarounds people used: close stdin (`stdin=DEVNULL`), or read `transcript.jsonl` under `brain/<cid>/.system_generated/logs/` from disk.
  - I'd close stdin and test on the version LoTaR targets.

Sources: https://antigravity.google/docs/cli/reference, https://github.com/google-antigravity/antigravity-cli/issues/76

### 3. Machine-readable output

The flag is `--output-format text|json|stream-json`. The default is `text`. Diagnostics go to stderr and responses go to stdout. The headless page is the source for all example lines below (https://antigravity.google/docs/cli/headless).

### `json`

This prints a single envelope when the run finishes:

```json
{"conversation_id":"055a398f-...","status":"SUCCESS","response":"A git rebase rewrites...","duration_seconds":7.16,"num_turns":1,"usage":{"input_tokens":10415,"output_tokens":657,"thinking_tokens":616,"cache_read_tokens":8113,"total_tokens":11072}}
```

- `error` appears only on failure.
- `structured_output` and `json_schema` appear only when `--json-schema` is used. `--json-schema` takes a schema string or a file path.

### `stream-json`

This is NDJSON, one event per line. The order is one `init`, then any number of `step_update`, then one `result`. Every line has an `event` field.

- **Session start (`init`):**
  ```json
  {"event":"init","conversation_id":"c3b66b04-...","init":{"cwd":"/home/user/project","tools":["ask_permission","run_command","write_to_file","..."],"permission_mode":"request-review"}}
  ```
- **Assistant text:** `step_type` is `agent_response`. Long replies first emit `ACTIVE` events with partial `text_delta` fragments, then a final `DONE`.
  ```json
  {"event":"step_update","step_update":{"conversation_id":"c3b66b04-...","step_index":3,"state":"DONE","step_type":"agent_response","text_delta":"Git rebase destructively rewrites...","duration_seconds":6.28,"usage":{"input_tokens":10302,"output_tokens":582,"thinking_tokens":551,"cache_read_tokens":8113,"total_tokens":10884}}}
  ```
- **Tool call:** `step_type` is `tool`. A tool failure adds an `error` object with `type` and `message` inside `tool_info`.
  ```json
  {"event":"step_update","step_update":{"conversation_id":"edb1c8c1-...","step_index":4,"state":"DONE","step_type":"tool","tool_name":"run_command","duration_seconds":0.07,"tool_info":{"name":"run_command","parameters":{"CommandLine":"echo hello_headless_demo"},"output":"hello_headless_demo\r\n"}}}
  ```
- **Final result:**
  ```json
  {"event":"result","result":{"conversation_id":"c3b66b04-...","status":"SUCCESS","response":"Git rebase destructively rewrites...","duration_seconds":6.88,"num_turns":1,"usage":{"input_tokens":10418,"output_tokens":589,"thinking_tokens":551,"cache_read_tokens":8113,"total_tokens":11007}}}
  ```
- **Status values:** `SUCCESS`, `ERROR`, `CANCELED`, `INTERRUPTED`, `INVALID`, `WAITING`, `RUNNING`.
- **Errors:** failures appear in `status` and `error` in json and stream-json modes. The exact shape of the `error` field is not documented.
- **Cost:** there is no cost field in the usage object. It carries tokens only, including `thinking_tokens` and `cache_read_tokens`. Release 1.1.8 notes mention `subagent_info` with `conversation_id` and `log_uri` for delegated subagents (https://newreleases.io/project/github/google-antigravity/antigravity-cli/release/1.1.8).

### 4. Model and reasoning-effort flags

- `--model <slug>` picks the model. `agy models` lists the choices and needs a signed-in session. An unknown model makes the run fail with exit code 1.
- `--effort` sets reasoning effort. The reference page says it ranges from `low` to `max` depending on the model. The cheat sheet lists only `low|medium|high`. I could not confirm which values are valid.
- `--agent <name>` selects an agent, and `agy agents` lists them.
- `settings.json` keys `model` and `modelProvider` store defaults.

Sources: https://antigravity.google/docs/cli/reference, https://computingforgeeks.com/antigravity-cli-cheat-sheet/

### 5. Permissions and sandbox

- **No prompts in headless mode.** Reads and writes inside the workspace are auto-allowed. Shell commands default to Ask, so they are soft-denied. A soft-denied tool lets the run continue, exits 0 and writes a notice to stderr. LoTaR should therefore treat a successful run that did nothing as a possible permissions issue.
- **`--mode accept-edits|auto|plan`:**
  - `plan` is the read-only-style mode.
  - `accept-edits` is the workspace-edit mode.
  - `auto` is Preview. It requires the terminal sandbox and is not available on business or enterprise accounts.
- **`--dangerously-skip-permissions`:** approves every tool call, which is the full-auto option. The docs recommend scoped rules instead.
- **`--sandbox`:** runs commands in an OS-contained terminal sandbox. It does nothing where the sandbox is unavailable (v1.4.4+).
- **Pre-approving tools:** add `action(target)` rules to `permissions.allow` in `~/.gemini/antigravity-cli/settings.json`. Examples are `command(git)` and `write_file(src/)`. There are also `ask` and `deny` rules, and deny rules apply in every mode.
- **Other settings keys:** `toolPermission` (`request-review` default, `proceed-in-sandbox`, `always-proceed`, `strict`), `enableTerminalSandbox` (default false), `agentMode`, and `allowNonWorkspaceAccess` (default false).
- **Not documented:** a per-invocation flag for allow rules. They live in the settings file, so a LoTaR job would need to write or override that file.

Sources: https://antigravity.google/docs/cli/headless, https://antigravity.google/docs/cli/reference

### 6. Extra writable directories

`--add-dir <path>` adds a workspace directory and can be repeated. Workspace access is auto-allowed. Reaching files outside the workspace needs `allowNonWorkspaceAccess: true` in settings.

Source: https://antigravity.google/docs/cli/reference

### 7. Resuming and follow-up input

- **Resume:** `-c` or `--continue` resumes the most recent conversation. `--conversation <id>` resumes a specific one, using the `conversation_id` from `init` or `result`. These flags are documented, but combining them with `-p` is not shown explicitly. `/resume` and `/fork` exist in the TUI only.
- **Follow-up while running:** `--input-format stream-json` requires `--output-format stream-json`. Write one JSON object per line to stdin, for example `{"event":"user","message":{"content":"Reply with exactly: banana"}}`.
  - One process handles multiple turns, and each turn emits its own `result`.
  - `response` is per turn. `num_turns`, `usage` and `duration_seconds` are cumulative.
  - Closing stdin ends the session after the current turn.
  - `content` can be a string or a list of blocks, but only `text` blocks are supported. Any other block type ends the session with an error.
  - Control messages and CLI slash commands (such as `/model`) are not allowed. Unknown event names are skipped with a warning on stderr. Malformed input ends the session.
- **Not documented:** interrupting a turn that is already running.

Source: https://antigravity.google/docs/cli/headless

### 8. Exit codes and failures

| Exit | Meaning |
|---|---|
| `0` | Success, or stdin closed cleanly. Also returned when a tool was soft-denied. |
| `1` | Run failed (for example an unknown `--model`), missing `event` field, invalid JSON line, or non-text content block. |
| `2` | A control message or CLI slash command was sent on stdin. |

- Since 1.1.1, server-side errors such as quota errors go to stderr with a non-zero exit code. Before that they could exit 0 silently.
- The `authentication required` error is documented, but its exit code is not listed.
- **For LoTaR:** don't rely on the exit code alone. Parse the final `result.status` and `error`. Also watch for a run that ends with no `result` event, which is the failure mode from issue #76.

Sources: https://antigravity.google/docs/cli/headless, https://github.com/google-antigravity/antigravity-cli/issues/76

## Kilo Code CLI

### 1. Install and auth
- **Install:** `npm install -g @kilocode/cli`. This gives a `kilo` binary. Older docs call the binary `kilocode`.
- **Version:** The current CLI is a fork of OpenCode. The old `kilocode --auto --json` form belongs to an earlier generation. Check `kilo --version` and `kilo run --help` before relying on any flag.
- **Sign-in:** Run `kilo`, then `/connect`, or use `kilo auth` to manage credentials.
- **API key in config:** Config can reference an environment variable, e.g. `"apiKey": "{env:ANTHROPIC_API_KEY}"`. This only resolves in trusted config: `~/.config/kilo`, `KILO_CONFIG`, `KILO_CONFIG_CONTENT`, or org-managed config. A project-level config can't use it.
- **Environment overrides:** `KILO_PROVIDER`, `KILO_API_KEY` (for non-kilocode providers) and `KILOCODE_<FIELD>` (for example `KILOCODE_MODEL`).
- **Organizations:** `KILO_ORG_ID` selects the organization for non-interactive runs.
- **Gotcha:** On CPUs without AVX the CLI can crash with "Illegal instruction". Use the `-baseline` build from the GitHub releases.

### 2. Headless invocation
- **Command:** `kilo run --auto "Implement feature X"`. The syntax is `kilo run [message..]`.
- **Prompt:** The message words are joined into the prompt, and piped stdin is appended to it (from `run.ts`). `--file/-f` attaches files, and `--command` runs a slash command.
- **Exit:** The CLI exits on its own when the task finishes or times out.
- **Follow-up questions:** With `--auto`, follow-up questions get an automatic reply telling the agent to decide on its own.

### 3. Machine-readable output
- **Flag:** `--format json` (default `default`). The docs only mention it in passing. I found no `--json` on the current `kilo run`.
- **Shape:** I read the shape from the source, not from docs. There are no example lines in the docs.
- **Line format:** One JSON object per line on stdout: `{ "type", "timestamp": <ms>, "sessionID", ...data }`.
- **Event types:**

| `type` | Payload | When |
|---|---|---|
| `step_start` | `part` | A step begins. This is the closest thing to session start. `sessionID` is on every line. |
| `text` | `part` | Assistant text, emitted only once the part has finished (`time.end` set). |
| `reasoning` | `part` | Only emitted when `--thinking` is on. |
| `tool_use` | `part` | Emitted once, when the tool status is `completed` or `error`. |
| `step_finish` | `part` | A step ends. Usage and cost presumably sit in this `part` (OpenCode-style tokens and cost). The source I read doesn't show the field names, so verify them with a real run. |
| `error` | `error` (object or string) | A session error, a failed prompt, an exception, an auto-rejected permission, or no assistant output. |

- **No final result event:** I found none. The last `step_finish` and the exit code appear to be the end signal.
- **Part and usage field names:** Not documented.
- **Errors:** They are also written to stderr.

### 4. Model and reasoning effort
- **Model:** `--model/-m provider/model`, split at the first `/`. The docs don't mention this flag, but the source has it. A config default `"model": "anthropic/claude-sonnet-4-20250514"` also works.
- **Reasoning effort:** `--variant <name>` is passed to the model. The variant names depend on the provider and aren't documented.
- **Agent:** `--agent <name>` selects an agent. An unknown name falls back to the default with a warning.
- **`--thinking`:** Shows reasoning output. It is off by default when non-interactive.

### 5. Permissions and sandbox
- **`--auto`:** Approves each permission request once. The source calls it "dangerous!". The docs say approvals are resolved by your auto-approval settings, and anything not auto-approved is denied. The source and docs read slightly differently here.
- **Hidden aliases:** `--yolo` and `--dangerously-skip-permissions` also approve once and skip the headless denial.
- **Without `--auto`:** Permission requests are auto-rejected and the run exits 1.
- **Config:** The `permission` key takes `allow`, `ask` or `deny` globally or per tool. Patterns are allowed, e.g. `"bash": {"*":"ask","git *":"allow","rm *":"deny"}`. The last matching rule wins, so put the `*` catch-all first. `"permission":"allow"` approves everything, and `"edit":"deny"` gives read-only behavior.
- **OS sandbox:** No flag is documented.

### 6. Extra writable directories
- **No `--add-dir`:** I found no such flag. The `external_directory` permission rule controls access to paths outside the launch directory.
- **`--dir <path>`:** Changes the working directory for the run.

### 7. Resume and follow-up
- **`kilo run --continue/-c`:** Resumes the first root session. The `kilo --continue` form in the docs is described as not combinable with `--auto` or a prompt, which conflicts with the source. Test it.
- **`kilo run --session/-s <id>`:** Resumes a specific session. It exits 1 with "Session not found" if missing.
- **`--fork`:** Forks the session first. It needs `-c` or `-s`.
- **`--attach <url>`:** Attaches to a running server. It takes `--password/-p` (or `KILO_SERVER_PASSWORD`) and `--username/-u`.
- **Mid-run input:** Not documented.

### 8. Exit codes and failures
- **`0`:** The task completed.
- **`124`:** The task timed out (documented).
- **`1`:** Any error. That includes a failed prompt, an exception, bad flags, a missing session, a run with no assistant message, or an auto-rejected permission when not using `--auto`.
- **JSON mode:** Failures show up as an `error` line on stdout and as a message on stderr.

### Sources
- https://kilo.ai/docs/cli
- https://kilo.ai/docs/code-with-ai/platforms/cli
- https://raw.githubusercontent.com/Kilo-Org/kilocode/main/packages/opencode/src/cli/cmd/run.ts
- https://github.com/Kilo-Org/kilocode

## Gemini CLI

Sources are listed per section. The `stream-json` field names come partly from third-party material and the TypeScript types, because the official docs have no example lines.

### 1. Install and auth
- **Package:** the npm package is `@google/gemini-cli` and the binary is `gemini`. The authentication page I fetched does not give the install command. I am relying on general knowledge for the package name, and it is not confirmed from the docs.
- **Google login:** run `gemini` interactively once and pick "Sign in with Google". Credentials are cached locally. Org, school and Workspace accounts must also set `GOOGLE_CLOUD_PROJECT`.
- **API key:** set `GEMINI_API_KEY` (from AI Studio).
- **Vertex AI:** set `GOOGLE_CLOUD_PROJECT` and `GOOGLE_CLOUD_LOCATION`. Then use one of these credentials:
  - Application Default Credentials (`gcloud auth application-default login`).
  - `GOOGLE_APPLICATION_CREDENTIALS` pointing at a service-account JSON key.
  - `GOOGLE_API_KEY`.
- **Headless:** it reuses cached credentials from an earlier interactive sign-in. Otherwise it needs the API key or Vertex environment variables, and the docs recommend service-account keys for CI.
- **`.env` files:** the CLI loads the first `.gemini/.env` it finds and does not merge files.
- Source: https://geminicli.com/docs/get-started/authentication.md

### 2. Headless invocation
- **Command:** `gemini -p "<prompt>" --output-format stream-json`.
- **Prompt:** `-p/--prompt` takes a string. Per the CLI reference, "Prompt text. Appended to stdin input if provided. Forces non-interactive mode." So you can pipe the prompt on stdin (`echo "..." | gemini -p "..."`). Headless mode also starts when stdin is not a TTY.
- **Prompt from a file:** there is no documented flag for this. Pipe the file on stdin instead.
- Sources: https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/headless.md and https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/cli-reference.md

### 3. Machine-readable output
- **Flag:** `-o/--output-format` takes `text` (the default), `json` or `stream-json`.
- **`json`:** one object with `response`, `stats` and an optional `error`.
- **`stream-json`:** newline-delimited JSON events. Every event has `type` and `timestamp`.
- **Event types:** the official docs list `init`, `message`, `tool_use`, `tool_result`, `error` and `result`, but give no example lines. The shapes below come from the TypeScript types in `packages/core/src/output/types.ts` and a third-party cheatsheet.
  - **Session start:** `{"type":"init","session_id":"abc123def","model":"gemini-2.0-flash-exp","timestamp":"2026-01-15T10:00:00Z"}`. The session ID is what you resume with.
  - **Assistant text:** `{"type":"message","role":"assistant","content":"The command output `hello`.","timestamp":"..."}`. `role` is `user` or `assistant`, and an optional `delta` flag marks streamed chunks. The CLI also echoes the user prompt as a `role:"user"` message.
  - **Tool call:** `{"type":"tool_use","tool_name":"Bash","tool_id":"tool_1","parameters":{"command":"echo hello"},"timestamp":"..."}`. The tool name in the example may not match real tool names such as `run_shell_command` and `write_file`.
  - **Tool output:** `{"type":"tool_result","tool_id":"tool_1","status":"success","output":"hello","timestamp":"..."}`. It pairs with `tool_use` by `tool_id`. `status` is `success` or `error`, and the types add optional `output` and error details.
  - **Final result:** `{"type":"result","status":"success","stats":{"input_tokens":100,"output_tokens":50},"timestamp":"..."}`.
    - According to the types, `StreamStats` has `total_tokens`, `input_tokens`, `output_tokens`, `cached`, `input`, `duration_ms`, `tool_calls` and a per-model `models` record.
    - The result event also has optional error details.
    - The cheatsheet mentions an optional `total_cost_usd`. The types excerpt does not list a cost field, so treat cost as unconfirmed.
  - **Error:** `{"type":"error","message":"API key invalid or expired","timestamp":"..."}`. The types add `severity` (`warning` or `error`). The official docs describe error events as non-fatal warnings and system errors, while the cheatsheet says an error ends the session. A fatal failure should also show up as `result` with a non-success status.
- **Open item:** the exact field names are not confirmed against real CLI output. The Mintlify mirror reportedly uses `sessionId` and `totalTokens`, which conflicts with the snake_case above. Capture real output from the target version before writing the parser.
- Sources: https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/output/types.ts, https://littlebearapps.com/help/untether/gemini-stream-json-cheatsheet/ (this page says its own `gemini` engine was deprecated in v0.36.0), https://geminicli.com/docs/cli/headless.md

### 4. Model and reasoning-effort flags
- **Model:** `-m/--model <name>`, default `auto`. The reference also lists aliases `auto`, `pro`, `flash` and `flash-lite`. Concrete IDs seen in the docs are `gemini-3-pro-preview`, `gemini-3-flash-preview`, `gemini-2.5-pro` and `gemini-2.5-flash`.
- **Sub-agents:** `--model` does not override the models sub-agents use, so usage may show other models.
- **Reasoning effort:** there is no CLI flag and the docs do not describe one. Any thinking-budget setting would be in `settings.json` model configuration, which I could not confirm.
- Sources: https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/cli-reference.md and https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/model.md

### 5. Permissions and sandbox
- **Approval mode:** `--approval-mode` takes `default`, `auto_edit`, `yolo` or `plan`. `plan` is the read-only mode. `-y/--yolo` is deprecated.
- **Headless without pre-approval:** a rule that would ask the user is treated as `deny` in non-interactive mode. Anything not pre-approved therefore gets blocked.
- **Sandbox:** `-s/--sandbox`, or `GEMINI_SANDBOX=true|docker|podman|sandbox-exec|runsc|lxc`, or `tools.sandbox` in `settings.json`. On macOS the default seatbelt profile (`permissive-open`) limits writes to the project directory. Docker and Podman mount the working directory.
- **Pre-approving tools:** `--allowed-tools` is deprecated in favour of the policy engine. Policy rules go in `~/.gemini/policies/*.toml`:
  ```toml
  [[rule]]
  toolName = "run_shell_command"
  commandPrefix = "git"
  decision = "allow"
  priority = 100
  ```
  - Optional fields are `argsPattern`, `commandRegex`, `mcpName`, `modes` and `interactive = false` (headless only).
  - Workspace-level `.gemini/policies` is currently disabled.
  - An `--admin-policy` flag exists, but there is no documented `--policy` flag.
- Sources: https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/cli-reference.md, https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/sandbox.md, https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/reference/policy-engine.md

### 6. Extra writable directories
- `--include-directories <dir>` adds directories to the workspace. It takes comma-separated values or repeated flags. The docs call these "workspace" directories and do not say how they interact with the sandbox write restrictions.
- Source: https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/cli-reference.md

### 7. Resume and follow-up input
- **Resume flag:** `-r/--resume` takes `latest`, an index number from `--list-sessions`, or a full session UUID.
- **Storage:** sessions are saved per project under `~/.gemini/tmp/<project_hash>/chats/`, so resume only works from the same project directory. Retention defaults to 30 days.
- **Headless resume:** the docs do not give a documented headless resume workflow. In principle you can combine `--resume <session_id>` with `-p` using the `session_id` from the `init` event, but I did not confirm this.
- **Mid-run input:** I found nothing documented for sending input while a headless run is in progress. Plan on one process per turn.
- Source: https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/session-management.md

### 8. Exit codes and failures
| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | General error or API failure |
| 42 | Input error (invalid prompt or arguments) |
| 53 | Turn limit exceeded |

- **Failure reporting:** in `json` mode, failures go in the `error` object. In `stream-json` mode, they go in `error` events and in a `result` event whose status is not success. The exit code reflects fatal failures.
- **Turn limit:** headless mode exits with an error when `maxSessionTurns` is reached.
- Source: https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/headless.md
