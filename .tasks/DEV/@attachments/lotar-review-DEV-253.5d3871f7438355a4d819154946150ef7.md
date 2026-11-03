# LoTaR product review (DEV-253)

Reviewed: 2026-10-30 to 2026-10-31, at commit 1fa968b (v0.11.2). Method: nine read-only reviewers, one per area, each followed by an independent checker that tried to refute every finding by re-reading the code and reproducing it with the release binary in throwaway workspaces.

Result: 149 findings reported, 146 confirmed, 3 refuted. Confirmed by severity: 2 critical, 12 high, 66 medium, 66 low (some are the same issue found by two areas). They are grouped into 41 tickets, one per fixable unit; each ticket holds the evidence, reproduction, impact and suggested fix for its findings.

## Tickets, most urgent first

| Ticket | Priority | Findings | Title |
| --- | --- | --- | --- |
| DEV-254 | high | 6 | Critical: Block DNS rebinding and other browser attacks on the local server |
| DEV-255 | high | 3 | Task listing is quadratic: every result is located again by walking the project tree |
| DEV-256 | high | 2 | Non-ASCII searches and filters return nothing: query strings are decoded as Latin-1 |
| DEV-257 | high | 1 | `lotar agent run` without --wait reports "Job started" but the job never runs |
| DEV-258 | high | 1 | Automation template values can inject shell commands despite the documented escaping |
| DEV-259 | high | 2 | Require trust before running automation commands that come from repository files |
| DEV-260 | high | 1 | Global -p makes `sprint add\|move\|remove` change every task in the project |
| DEV-261 | high | 3 | Scanner aborts on doc comments, stops at Rust lifetimes and rewrites CRLF line endings |
| DEV-262 | high | 4 | Jira sync: pull stops after 50 issues, wipes custom fields, flattens descriptions, mis-assigns users |
| DEV-263 | high | 2 | Task panel overwrites newer edits and discards unsaved edits on Escape |
| DEV-264 | high | 3 | Scan page: "Global" save writes to the default project; pattern and apply bugs |
| DEV-265 | medium | 5 | Config resolution drops default-valued overrides, fails open on bad config, and docs misstate precedence |
| DEV-266 | medium | 1 | Custom fields and config maps are written in random key order (git noise, merge conflicts) |
| DEV-267 | medium | 1 | Read-only commands create workspaces, and workspace search crosses repository boundaries |
| DEV-268 | medium | 1 | Unknown nested YAML keys in task and sprint files are dropped on write |
| DEV-269 | medium | 3 | Project prefixes alias on case-insensitive filesystems and when generated prefixes match |
| DEV-270 | low | 3 | Task ID allocation can reuse or collide IDs, with no way to renumber |
| DEV-271 | low | 4 | Storage robustness: lock files in git status, non-atomic attachments, hidden tasks, removed statuses |
| DEV-272 | medium | 2 | `lotar serve` on localhost binds only ::1 and misses ports that are already taken |
| DEV-273 | medium | 7 | SSE and file watcher: delayed delivery, duplicate or bogus events, missing sprint/config events |
| DEV-274 | medium | 2 | Web UI refetches the whole task list on every event and ships one 1.4 MB bundle |
| DEV-275 | medium | 3 | CSV export: `format` treated as a filter, broken quoting, formula injection |
| DEV-276 | medium | 6 | REST contract consistency: bad JSON accepted, OpenAPI drift, status codes, upload cap, missing endpoint docs |
| DEV-277 | medium | 2 | MCP tool restrictions (LOTAR_MCP_TOOLS, job tool set) can be bypassed |
| DEV-278 | medium | 3 | Automation runner: commands corrupt MCP stdio, cooldowns reset per process, one failure stops later rules |
| DEV-279 | medium | 8 | Agent jobs across processes: detection, cancel/status, orphaned MCP jobs, queue drops, Windows cancel |
| DEV-280 | low | 4 | MCP protocol details and docs: error codes, logging capability, stale tool counts |
| DEV-281 | medium | 5 | CLI argument preprocessing: help/version words and -f/-l values hijack commands |
| DEV-282 | medium | 6 | CLI safety and validation: empty --select matches everything, edit skips validation, silent failures |
| DEV-283 | medium | 7 | CLI output consistency and missing edits: IDs, JSON envelopes, changelog, clearing fields, completions |
| DEV-284 | medium | 3 | Terminal help rendering and discoverability |
| DEV-285 | medium | 5 | Scanner polish: renamed files, --modified-only, empty titles, NOTE default, --include docs |
| DEV-286 | medium | 3 | Sync robustness: Jira keys auto-linked as local tasks, no rate-limit handling, dropped GitHub fields |
| DEV-287 | medium | 6 | Git integration: identity parsing, branch inference from the server cwd, CODEOWNERS, hooks install |
| DEV-288 | medium | 6 | Web UI: unsaved automation edits, timezone dates, keyboard and screen-reader gaps, preferences reset |
| DEV-289 | medium | 2 | Docker image lacks git and runs as a fixed UID, so git-backed features fail |
| DEV-290 | medium | 1 | Prerelease tags are published as the stable latest on GitHub, Docker Hub, Homebrew and Scoop |
| DEV-291 | low | 3 | Release and install hardening: token scope, workflow expressions, install.ps1 PATH handling |
| DEV-292 | medium | 3 | Performance: activity feed spawns git per commit and file, exclusion filters and @least_busy rerun full queries |
| DEV-293 | medium | 12 | Docs accuracy pass: stale pages, internal ticket notes, failing examples, missing flags and commands |
| DEV-294 | medium | 1 | Write user help for Jira/GitHub sync (pull, push, sync check) |

## What each area found sound, and what was not covered

### Storage, domain model, config, YAML compatibility

Reviewed and sound:
- Atomic task/config writes (safety.rs atomic_write_file*): temp file created with create_new and the destination's permissions, fsync, rename, failure cleanup of only its own temp
- Advisory locks (acquire_storage_lock / with_storage_lock): bounded retry, the Windows contention code handled, pending-journal refusal checked after the lock is taken
- Concurrent task creation: 30 parallel `lotar add` produced 30 unique IDs with no lost writes
- Locked read-modify-write (`StorageOperations::mutate`) for comments and references, and the no-op guard in `edit` keeps the bytes unchanged
- TaskId canonical parsing (hyphenated prefixes, padded aliases, overflow, traversal rejection) and fail-closed ambiguity handling for duplicate files
- Top-level unknown task YAML keys preserved and protected from shadowing built-ins (`serialize_extra_fields`)
- YAML scalar round-trip: titles like `null`, `~`, `no`, `0x10`, `.inf`, `key: value`, `#hash` are quoted and read back as strings; numeric `title`/`effort`/`tags` hand-written values parse
- Custom field names that collide with built-ins are rejected
- Malformed global or project config fails closed in CLI commands (status/list)
- Sprint metadata patch applied fresh under the sprints lock; top-level sprint extra fields preserved
- Archive layout (`@archive`, nested folders, max_number includes the archive, move stubs reserve numbers)
- Attachment name validation and path resolution (`validate_managed_name`, canonicalized containment check)
- Due dates stored as calendar dates (not UTC instants), so they display consistently across time zones

Not covered or only skimmed:
- transaction.rs journal recovery and rollback internals: only skimmed begin and locking; crash-recovery paths not exercised
- config/normalization.rs, config/validation, config/candidate.rs, config/operations.rs beyond `config set` round-trips
- utils: text_search, search_syntax, scan, identity_detectors, codeowners, effort parsing (only skimmed time.rs)
- Relationship reciprocity and graph validation (services/relationships.rs) and history rendering
- Real Windows behavior: path, lock and rename semantics checked by code reading only
- Lock-timeout behavior under heavy multi-process contention (2s limit with begin_all locking every project)
- REST-path fallback repro: my first REST attempt reached another reviewer's server through the IPv6/IPv4 dual bind (see finding), so the fail-open config evidence comes from MCP only
- Side effect to flag: that mistaken REST call created one task, `via rest todo`, at /private/tmp/claude-501/-Users-mallox-Development--localtaskrepo-lotar/1ccea527-f479-43c5-90da-05c1572a4061/scratchpad/review-api-sse-web/ws/.tasks/ENV/1.yml in the API/SSE reviewer's scratch workspace; I left it in place so I would not disturb their state. My two other calls there wrote nothing (one rejected status, one not-found). The repository itself was not touched. My own server (pid 26910) was stopped.

### REST API, SSE, embedded web server, contract parity

Reviewed and sound:
- Same-task write concurrency: 30 parallel POST /api/tasks/comment on one task stored all 30 comments; 40 parallel creates gave 40 unique IDs
- Path traversal guards: /api/sync/reports/get, /api/attachments/get and /api/references/snippet reject ../ and absolute paths; static serving rejects dot segments; the path is not percent-decoded, so %2e%2e is inert
- Attachment serving: nosniff, sandboxing CSP for SVG, active types served as octet-stream, RFC 5987 Content-Disposition with control characters replaced (no header injection), Range/206/416 handling, hash ETag/304
- Branding image serving: content-sniffed type, SVG sandbox CSP, ETag/304
- Cross-origin POSTs with a mismatched or null Origin are rejected with 403 (only the Host/rebinding gap remains)
- commit_diff validates revisions with ensure_safe_git_rev and passes `--` before paths (no git option injection)
- Error envelope mapping (map_lotar_error) and its exhaustive unit test
- OpenAPI route-set parity: all server routes are in docs/openapi.json and the reverse (enforced by tests/openapi_contract_test.rs); list/export query parameters in the spec match KNOWN_QUERY_KEYS
- Static asset caching: immutable for hashed assets/, no-cache for HTML, gzip negotiation for embedded assets and openapi.json
- SSE frames serialize payloads as JSON, so injected query values cannot break framing; a dead client is detected by heartbeat write failure

Not covered or only skimmed:
- src/routes/sync.rs pull/push behaviour and long-running request handling (sync logic belongs to another area)
- src/routes/relationships.rs mutate/repair semantics beyond route registration
- Agent job routes beyond request parsing (job lifecycle and runner behaviour)
- Automation simulate/run semantics beyond the RCE path
- Field-by-field parity of request types (TaskCreate/TaskUpdate/Sprint* requests) between api_types.rs, types.ts and the spec; only response shapes of about 25 GET endpoints were checked live
- Browser-level verification of UI effects (e.g. how the stale-SSE scenarios render); conclusions about UI impact come from reading code
- The SSE debounce_ms overflow edge case (u64::MAX): no panic was visible in the server log; not investigated further
- HTTP request parsing edge cases: headers over 32 KiB, a Content-Length larger than the body (30 s hang), HEAD on static assets sending a body
- Optimistic concurrency / lost updates between two clients editing the same task (last write wins; no ETag/If-Match), noted but not assessed as a finding

### MCP server, agent jobs, automation rules

Reviewed and sound:
- MCP NDJSON/Content-Length transport: bounded line reads, oversize drain/resync, UTF-8 handling, sticky framed output (src/mcp/server/transport.rs, server.rs)
- MCP initialize lifecycle: -32002 before notifications/initialized, a second initialize does not reset the session, deferred list_changed is coalesced (session.rs); envelope validation for -32700/-32600 (parse_incoming)
- Schema validator fails closed on unsupported keywords and is applied before every handler on both surfaces (schema.rs, registry.rs invoke_tool)
- task_list pagination clamps cursor and validates limit range
- Agent job registration: canonical ticket ids, one active job per ticket, refusal of cross-root tickets before side effects, cancellation serialized with spawn under the registry lock (agent_job_service.rs start_job / run_job)
- Worktree reuse and cleanup safety checks (same common dir, own branch, clean, merged) in prepare_worktree/check_worktree_cleanup
- Runner command construction: model/effort value validation, permission flag mapping, Codex MCP flags spliced before the prompt
- Automation actions recompose against the latest persisted task state and use automation_disabled to prevent cascades; async run monitor drains stderr with a bounded tail
- Agent context path sanitization (no traversal) and log path resolution

Not covered or only skimmed:
- MCP sprint/relationship/sync handler internals (src/mcp/server/handlers/sprints.rs, sync.rs, most of tasks.rs) beyond a few calls
- REST job routes (src/routes/jobs.rs) and SSE delivery of job events
- Runner output parsers for each CLI (parse_*_event) and usage accounting
- automation_validation.rs and the template edge cases beyond shell quoting; time-condition date math
- Automation simulate/preview parity with apply
- Windows-specific behavior (only code-read); the Linux argv size limit for long prompts passed as arguments (unverified)
- Concurrency of agent context file read-modify-write (non-atomic fs::write, no lock): noted but not tested

### CLI commands, output formats, help text

Reviewed and sound:
- Error paths in JSON mode: not-found/validation errors go to stderr as {"status":"error"} with exit 1 (status, priority, assignee, effort, comment, show, edit, relationships, sprint show/add, agent status, list status:bogus)
- Project mismatch detection (`-p ALPH status BETA-1` etc.) across quick commands
- Reference validation: absolute/traversal paths blocked ('escapes repository'), javascript: links rejected, duplicate links idempotent
- sprint delete confirmation (errors in JSON / non-interactive without --force)
- config set: unknown keys rejected; issue.priorities change blocked by task conflicts unless --force; dry-run doesn't write
- scan --dry-run preview, missing path error with exit 1
- @me resolution in list (mine:true, -a @me, assignee:@ME) and whoami/--explain
- list JSON pagination metadata; excluded-filter routing (`-status:done`) and removed-flag hints
- Status/priority/effort/due-date JSON outputs use canonical task_id; dry-run previews don't write
- Soft delete is idempotent at the storage level (deleted_at unchanged on repeat)

Not covered or only skimmed:
- MCP stdio server (`lotar mcp`) and agent run/worker/queue/worktree behavior (only args read)
- sync pull/push/check against real remotes (exit code on partial failures not verified)
- serve handler internals beyond arg normalization
- config init interactive wizard and --with scaffolds
- stats numerical correctness (only smoke-ran subcommands)
- scan write mode / --reanchor / --modified-only
- relationships repair/graph/move CLI paths
- Windows-specific behavior (case-insensitivity finding verified on macOS only)
- zsh/powershell/elvish completion install paths being auto-loaded (not verified)

### Sync, TODO scanner, git integration, identity

Reviewed and sound:
- Sync journal and lock design (src/services/sync_service/journal.rs): exclusive lock, intent journaled before non-idempotent creates, canonicalized task IDs, fails closed on invalid entries; no secrets in the journal
- Sync reference normalization and scoping (references.rs): the Jira project prefix and the GitHub repo are both checked before a reference counts as matching
- GitHub /repos/{repo}/issues pagination (stops on a short page, PRs filtered out); Search API is capped at 1000 with a warning
- Sync report storage: exclusive-create filenames, `..`/absolute paths rejected, canonicalized root check on read (sync_report_service.rs)
- Auth profiles only come from home config; the token is sent only to the profile's own api_url/base_url; insecure-permission warning on ~/.lotar
- REST Origin/Host validation in web_server.rs (relevant to POST /api/scan/run)
- Anchor relocation after lines shift within a file (scratch ra: anchors moved from #2/#4 to #4/#6 correctly)
- Scan handles a UTF-8 BOM and files without a trailing newline; Latin-1 files are skipped rather than corrupted
- utils/git.rs: worktree gitdir/commondir resolution, detached HEAD returns no branch (branch inference is skipped), and git_command env scrubbing (where it is used)
- Pulls skip tombstoned and moved tasks (DEV-92/183) through the reference index

Not covered or only skimmed:
- Sync UI page and the per-task Sync dialog (view/), plus the REST/MCP sync route wiring beyond the shared service
- Deep journal recovery and indeterminate-creation paths, and concurrent sync plus editor races (design read only)
- GitHub push end-to-end against a mock (only pull was exercised); the Jira transition flow
- Scan UI page and the REST scan `targets` selection; the scan source-lock and concurrency implementation (utils/scan.rs write_source)
- History and stats parsing in audit_service.rs beyond the env-routing issue; agent worktree git usage in agent_job_service.rs
- Windows-specific behaviour (paths, the CRLF default with core.autocrlf, per-path locks)
- Very large repo performance of the scanner and the anchor pass (only reasoned about, not benchmarked)
- Whether REST scan accepting arbitrary absolute `paths` outside the workspace is acceptable when the server is bound beyond localhost

### Web UI (Vue 3)

Reviewed and sound:
- Markdown rendering (utils/markdown.ts, MarkdownContent.vue): micromark escapes raw HTML and blanks unsafe protocols, DOMPurify sanitizes, and task-ref linking builds DOM nodes. Checked in a browser with a task whose title and description held <img onerror>, <script> and javascript: links: everything was escaped or blanked, no dialog fired
- useSse shared-connection refcounting, per-consumer off/close cleanup and onReconnect semantics
- AgentJobs SSE merge and refresh sequencing (refreshSeq/eventSeq, log dedup by sequence, interval cleanup). The backend agent_job_* payload carries all fields the merge needs
- useTaskStore: query generations, tombstones and deletion revisions guard against late responses (read the design and the SSE wiring)
- utils/storage.ts: every localStorage/sessionStorage access is wrapped in try/catch. The direct calls in Preferences are also guarded
- TasksList CSV export: error handling and blob revoke
- Bulk delete reports partial failures
- SyncHub remote add/edit/delete: re-reads the remote list before writing and checks for conflicts. Its unmount handling (alive flag, disconnect) is also sound
- ConfigView save sends global/project correctly, shows errors and warnings, and guards unsaved changes on route leave and unload
- Most window/document listeners (ColumnsMenu, TaskHoverCard, UiModal, FilterBar, AttachmentViewer, Calendar, SprintsList, global shortcuts, list key nav) are removed on unmount
- UiModal Escape and focus-trap stack: stopImmediatePropagation keeps modals nested in the panel from closing the panel
- Date helpers parseTaskDate/formatTaskDate/toDateKey treat date-only values as local dates. Every due-date display except SprintTaskCell uses them
- Full Vitest UI suite (`vitest run --config view/vitest.config.ts`): all files passed (exit 0)

Not covered or only skimmed:
- SprintsList.vue (3456 lines) in depth: drag-reorder and manual-order persistence, sprint create/edit modal validation, analytics dialog
- FilterBar.vue and useFilterGrammar.ts: grammar parsing and round-trips between URL and filter (not explored)
- TaskPanel subareas: attachments upload and drop, references tab, relationships mutations, the autosave queue internals beyond reading the design
- Insights lenses and insightsMetrics accuracy; Dependencies graph layout and repair flow (SSE handling only skimmed)
- ConfigView form sections and useConfigForm validation; theme, branding and ThemeSelect
- CommandPalette, KeyboardShortcutsDialog, TaskHoverCard positioning
- Performance at scale. By design, every task SSE event triggers a debounced full re-list of every retained query, all pages. It was not measured with large task counts
- Calendar URL state: filters other than q are not written to the URL, and a route-driven project change refreshes twice. Noticed in code but not verified or filed
- Throwaway workspace: /private/tmp/claude-501/-Users-mallox-Development--localtaskrepo-lotar/1ccea527-f479-43c5-90da-05c1572a4061/scratchpad/review-frontend/w2. Repro scripts: /private/tmp/claude-501/-Users-mallox-Development--localtaskrepo-lotar/1ccea527-f479-43c5-90da-05c1572a4061/scratchpad/review-frontend/scripts/. The server on port 19473 has been stopped and the repo working tree is unchanged

### Documentation accuracy and readability

Reviewed and sound:
- README.md: relative links/images resolve; quick-start, search, custom-fields (init --template agile, config set custom_fields, --field, list component=editor), history/stats examples all ran without CLI parse errors in a scratch workspace
- Link/anchor check across README.md, docs/*.md, docs/help/*.md, skills SKILL.md and docs/developers/*.md: no broken relative links or anchors other than a placeholder in task.md:398 and developers/task-model.md:157 (src/services/sync_service.rs is now a directory)
- 253 runnable `lotar ...` lines extracted from doc code blocks were executed in an isolated workspace; only `config inspect` failed with a clap parse error (the other failures were data or default-config related, reported above)
- OpenAPI paths match the routes in src/routes (only template-path spelling differences)
- MCP tool names in mcp.md/mcp-tools.md match the registry, including task_archive/task_unarchive
- install.md asset names match the release.yml matrix; package.json build scripts match install.md; Dockerfile ENV/USER/ENTRYPOINT match docker.md apart from the missing git
- list.md filter table, pagination and removed-flag hints match CLI behavior for the cases tried (mine:true, -status, --sum, --limit alias)
- config-reference recipes (`config set sprints_defaults_length --dry-run`, `config show --full --format=json`) work
- serve.md port/`-p` semantics match src/cli/args/serve.rs and preprocess
- site/index.html: install commands match README/install.md; the 'six more agent CLIs' count matches the runner list; asset paths are produced by scripts/build-site.mjs

Not covered or only skimmed:
- docs/help/agent.md runner/permission tables and agent-instructions.md (only spot-checked)
- docs/help/stats.md and sprints.md in detail (only the commands extracted from code blocks were run)
- docs/help/mcp-tools.md parameter schemas compared field by field with the registry
- docs/architecture-decisions.md and docs/release-guide.md (read only for references)
- site/compare.html competitor claims and site/install.sh / install.ps1 behavior
- docs/developers/* beyond the sync doc and README
- Windows-specific behavior (the %APPDATA% finding is from reading code, not run on Windows)
- Commands needing network, credentials or long-running processes (serve, mcp beyond tools/list, pull/push, agent run) were not executed

### Performance and scalability

Reviewed and sound:
- Single-task operations at 2000 tasks: show 0.01s, status change 0.04s, comment 0.03s, edit 0.04s, archive/unarchive 0.02–0.03s, add (id allocation via max_number walk) 0.04s
- Bulk sprint add/remove of 300 tasks: ~0.9s each
- Text search: plain, multi-word, typo-tolerant and /regex/ queries 0.04–0.35s over 2000 tasks; per-task regex compile is negligible
- TODO scanner: dry-run over 1093 tracked repo files in 0.02s; uses the ignore crate (.lotarignore/.gitignore) with a rayon parallel scan
- CLI startup: --version and help ~0.00s
- Read-only REST requests (list, get, config, projects) do not write files or trigger watcher/SSE events
- GET /api/tasks/get 11ms, /api/relationships/graph 94ms, /api/sprints/list 100ms, /api/activity/series|changed_tasks|authors 11–18ms
- Static asset serving: gzip Content-Encoding, immutable caching, index.html small
- MCP tools-list watcher polls config-only enum hints every 2s (cheap, no task scan)
- UI Board (progressive disclosure via maxVisible) and Calendar (per-cell visible counts) bound DOM size; TasksList paginates rendering client-side
- Server stayed alive and memory dropped back after 26k-event bulk floods (two repeated runs)

Not covered or only skimmed:
- In-browser measurement of UI render cost, main-thread jank, and the conjectured refresh livelock; no Playwright or performance trace was run
- Sync (pull/push against remotes) performance: it uses list_with_moved, so it inherits the O(N²) listing, but this was not measured
- Agent job runner/queue throughput and log streaming
- Concurrency races in id allocation and transaction journal (other reviewers' areas)
- Windows/Linux watcher behaviour (inotify watch limits on large .tasks trees); only macOS FSEvents was tested
- Very long per-task histories (thousands of entries) and large descriptions/attachments: the effect on YAML parse and payload size was not quantified
- An early `lotar serve` instance on port 19473 exited silently during a bulk-rewrite test. Two repeats on 19474 did not reproduce it, it left no crash report, and other reviewers were killing servers concurrently, so it is unexplained and not reported
- `lotar serve` bound only IPv6 [::1] on one start and was reachable on 127.0.0.1 on another. Address-family selection for host 'localhost' was not investigated (not a performance issue)
- Vitest suites were not run; no Rust tests or builds were run, per the instructions

### Security, release and install tooling

Reviewed and sound:
- Origin vs Host check rejects real cross-origin browser POSTs (mismatched Origin -> 403; `Origin: null` rejected); no Access-Control-Allow-Origin is emitted, so cross-origin reads are blocked without rebinding
- Attachment path endpoints: /api/attachments/get rejects ../, encoded ../, absolute paths and foo/../../; hash route uses find_by_hash; Content-Disposition fallback/RFC5987 encoding prevents header injection; SVG served with sandbox CSP; active types served as octet-stream with nosniff
- Branding SVGs served with a sandbox CSP and nosniff
- /api/sync/reports/get rejects traversal ('Invalid report path'); task IDs validated ('expected PROJECT-NUMBER')
- references/snippet constrains reads to the canonicalized repo root (only .git/ and dotfiles inside the repo are a concern; covered under the rebinding finding)
- commit_diff validates the rev (ensure_safe_git_rev) and passes paths after `--`
- YAML billion-laughs: serde_yaml_ng enforces a repetition limit; the bad file is skipped with a clear warning (verified)
- Request body capped at 16 MiB; header read capped at 32 KiB; per-connection socket timeouts; connection cap
- Sync auth_profiles are honoured only from the home config (a repo .tasks/config.yml auth_profile pointing at an attacker api_url plus an env var was rejected: 'Auth profile not found in home config'); home config permission warning; /api/config/show and /api/config/inspect do not leak literal tokens from home auth_profiles (verified)
- Agent prompts always begin with fixed text, so they cannot be parsed as CLI flags; Windows .cmd shim handling for multiline prompts is in place
- Agent logs default to @agent-logs with a generated .gitignore; job id path traversal rejected
- Markdown rendering: micromark (raw HTML off) followed by DOMPurify; task-reference linking uses DOM APIs
- install.sh: everything runs from main (truncation-safe), https-only curl with TLS 1.2, SHA-256 verification before install, atomic copy plus rename, Rosetta detection, BusyBox support, clear PATH hints; covered by install-verify.yml on Ubuntu x64/arm64, Alpine, and macOS arm64/x64
- install.ps1: TLS 1.2 enablement, case-insensitive hash compare against the uppercase CRLF checksum, temp dir cleanup in finally, locked-exe message
- npm launcher: no postinstall, so --ignore-scripts works; correct platform map including win32-arm64 to x64; signal forwarding and exit-code propagation; clear error with --omit=optional
- npm-packages.mjs: version validation, platform packages published before the main package, exact optionalDependency versions, executable bit set, prerelease mapped to the `next` dist-tag; publish loop fails closed under pipefail
- nfpm config and the package-linux job (nfpm download pinned by SHA-256, checksums verified before packaging, persist-credentials false)
- publish-npm job uses OIDC trusted publishing with provenance, contents: read, persist-credentials false
- All workflow `uses:` are pinned to full commit SHAs; Dependabot covers cargo, npm and actions
- cargo-deny config (advisories, licenses, unknown registries and git denied, wildcards denied) runs in CI; the single ignored advisory is documented
- Homebrew formula generation validates 64-char checksums and guards the f-string interpolation

Not covered or only skimmed:
- MCP server tools (config_set, agent_run) beyond reading the tool list: did not check whether config_set can write agent `command`/`args`, or the prompt-injection exposure from synced issue content
- lotar_agent_wrapper binary and agent_queue_service process spawning: only skimmed
- Sync push/pull data flows: e.g. a repo-tracked remote pointing push at an attacker repo while using the user's home token (noted as plausible, not tested)
- SSE connection exhaustion: long-lived SSE sockets count against the 64-connection cap (not tested)
- Windows-only behaviours (install.ps1 PATH handling, the static join escape) could not be run here
- Docker image build and runtime were not executed (no Docker run); the findings are from code and Dockerfile reading
- Not checked: the release-notes renderer .github/scripts/release-ticket-links.cjs, pages.yml site build, scripts/build-site.mjs
- Non-security server correctness beyond what surfaced (query decoding, CSV); the REST contract parity is another reviewer's area
- Storage write atomicity and race conditions on task YAML (other reviewers' areas)

## Refuted findings

Reported by a reviewer, then rejected by the checker:

- A mistyped `-p` on `add` creates a new project without notice, and read commands with an unknown project just report no tasks (cli). Checker: `-p NOPE add x` does create NOPE-1, and `list` in an empty directory writes .tasks/config.yml. Both are documented design: cli-tour.md:40-41 says 'A new project is created the first time you use it; its prefix is derived from the name', and main.md:51 and precedence.md:44 say the tasks folder is created automatically. `-p NOPE list` returning 'No tasks found' is accurate output.
- MCP tool responses are pretty-printed and task_list returns full history/comments per task, inflating agent token usage about 4× (performance). Checker: Pretty-printing is intended and documented: docs/help/mcp.md:158 ('Tool handlers pretty-print JSON into the text field') and docs/help/mcp-tools.md:9. I measured MCP task_list {limit:20} on a 500-task copy. The JSON-RPC line was 77,621 B, but the decoded text content agents actually receive was 34,187 B, against 18,502 B compact: about 1.85×, not 4×. The '4×' compared the escaped wire line, not wh
- Task parse cache keeps full file text plus the parsed Task for every path ever read and never evicts stale paths (performance). Checker: In search.rs:13-68 the cache is keyed by path: re-reading a path overwrites its entry, so growth is bounded by the number of distinct task file paths seen in the process lifetime, with a hard cap of 100k entries. Stale entries only build up for paths that were deleted, moved or archived. At typical task sizes (a few KB, stored twice), even thousands of such moves cost a few tens of MB at most, and

## Finding to ticket map

- DEV-254: api-sse-web#0, security-release#0, api-sse-web#12, security-release#7, security-release#2, security-release#10
- DEV-255: storage-core#0, api-sse-web#1, performance#0
- DEV-256: api-sse-web#2, security-release#5
- DEV-257: mcp-agents-automation#0
- DEV-258: mcp-agents-automation#3
- DEV-259: security-release#1, mcp-agents-automation#4
- DEV-260: cli#0
- DEV-261: sync-scan-git#0, sync-scan-git#3, sync-scan-git#2
- DEV-262: sync-scan-git#1, sync-scan-git#4, sync-scan-git#11, sync-scan-git#12
- DEV-263: frontend#0, frontend#1
- DEV-264: frontend#2, frontend#3, frontend#10
- DEV-265: storage-core#1, storage-core#8, storage-core#13, docs#2, docs#4
- DEV-266: storage-core#2
- DEV-267: storage-core#3
- DEV-268: storage-core#5
- DEV-269: storage-core#7, cli#9, storage-core#9
- DEV-270: storage-core#4, storage-core#6, storage-core#16
- DEV-271: storage-core#10, storage-core#14, storage-core#15, storage-core#17
- DEV-272: api-sse-web#7, storage-core#11
- DEV-273: api-sse-web#3, performance#1, api-sse-web#4, performance#3, api-sse-web#10, api-sse-web#11, docs#13
- DEV-274: performance#2, performance#8
- DEV-275: api-sse-web#5, api-sse-web#6, security-release#6
- DEV-276: api-sse-web#8, api-sse-web#9, api-sse-web#13, api-sse-web#14, api-sse-web#15, api-sse-web#16
- DEV-277: mcp-agents-automation#1, mcp-agents-automation#11
- DEV-278: mcp-agents-automation#2, mcp-agents-automation#9, mcp-agents-automation#10
- DEV-279: mcp-agents-automation#5, mcp-agents-automation#6, mcp-agents-automation#7, mcp-agents-automation#8, mcp-agents-automation#12, mcp-agents-automation#13, mcp-agents-automation#17, docs#9
- DEV-280: mcp-agents-automation#14, mcp-agents-automation#15, mcp-agents-automation#16, docs#12
- DEV-281: cli#1, docs#0, cli#2, cli#10, cli#18
- DEV-282: cli#3, cli#4, cli#5, cli#6, cli#12, cli#13
- DEV-283: cli#7, cli#8, cli#15, cli#16, cli#17, cli#19, cli#21
- DEV-284: cli#11, docs#3, docs#20
- DEV-285: sync-scan-git#7, sync-scan-git#10, sync-scan-git#13, sync-scan-git#14, sync-scan-git#17
- DEV-286: sync-scan-git#6, sync-scan-git#16, sync-scan-git#18
- DEV-287: sync-scan-git#5, sync-scan-git#8, sync-scan-git#9, sync-scan-git#15, cli#22, docs#16
- DEV-288: frontend#4, frontend#5, frontend#6, frontend#7, frontend#8, frontend#9
- DEV-289: docs#1, security-release#8
- DEV-290: security-release#3
- DEV-291: security-release#4, security-release#11, security-release#9
- DEV-292: performance#4, performance#5, performance#6
- DEV-293: docs#5, docs#7, docs#8, docs#10, docs#11, docs#14, docs#15, docs#17, docs#18, docs#19, storage-core#12, cli#20
- DEV-294: docs#6
