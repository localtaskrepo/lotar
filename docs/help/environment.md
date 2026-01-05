# Environment Variables

LoTaR reads environment variables for workspace discovery, configuration overrides, runtime toggles, and diagnostics. Unless noted otherwise each variable is evaluated once when the CLI or server process starts.


## Resolution & Precedence

- CLI flags → project config (when applicable) → environment variables → home config (`~/.lotar`) → global config (`.tasks/config.yml`) → built-in defaults. Commands without a project context skip the project step. See [Configuration Reference](./config-reference.md) and [Resolution & Precedence](./precedence.md) for resolver details.
- Override variables feed directly into the configuration system; provide values in the same shape used in YAML (simple scalars) or JSON/YAML strings for lists and maps.
- Changes require restarting the CLI/server so the fresh environment is read.

## Tasks Directory & Config Discovery

| Variable | Values | Behavior/Source |
| --- | --- | --- |
| `LOTAR_TASKS_DIR` | Absolute path or simple folder name | Highest-priority location for the `.tasks` workspace. When a simple relative name is provided the resolver searches parent directories; missing paths are created automatically. |
| `LOTAR_IGNORE_ENV_TASKS_DIR` | `1` | Test helper that forces the resolver to ignore `LOTAR_TASKS_DIR` even when set, preventing cross-test bleed. |
| `LOTAR_IGNORE_HOME_CONFIG` | `1` | Skips loading `~/.lotar`. Useful for hermetic CI runs. |
| `LOTAR_TEST_MODE` | `1` | Enables the test heuristics: disables `LOTAR_TASKS_DIR`, ignores home config, and constrains parent-directory discovery boundaries. |
| `LOTAR_PROJECT` | Project name/prefix | Overrides project auto-detection and acts as an alias when overriding `default_project`. |

## Config Overrides (global config replacements)

All rows below are defined inside the same override table and map directly to the indicated key within `config.yml`.

### Server defaults & identity

| Config key | Env var(s) | Accepted values | Purpose |
| --- | --- | --- | --- |
| `server_port` | `LOTAR_PORT`, `LOTAR_SERVER_PORT` | Integer | Forces the HTTP/SSE server to a specific port. |
| `default_project` | `LOTAR_PROJECT`, `LOTAR_DEFAULT_PROJECT` | Project prefix/slug | Sets the fallback project for CLI commands that omit `-p/--project`. |
| `default_assignee` | `LOTAR_DEFAULT_ASSIGNEE` | username / `@me` | Auto-populates the assignee on new tasks. |
| `default_reporter` | `LOTAR_DEFAULT_REPORTER` | username / email | Sets the reporter used when resolving `@me`. |
| `default_tags` | `LOTAR_DEFAULT_TAGS` | JSON/YAML list or comma string | Extra tags added to every new task. |
| `default_priority` | `LOTAR_DEFAULT_PRIORITY` | Priority slug | Sets the initial priority when not provided. |
| `default_status` | `LOTAR_DEFAULT_STATUS` | Status slug | Sets the initial workflow status. |
| `members` | `LOTAR_MEMBERS` | JSON/YAML list or comma string | Replaces the members directory used by mention auto-complete. |
| `strict_members` | `LOTAR_STRICT_MEMBERS` | `true`/`false` | When true, limits assignments to the declared members list. |
| `auto_populate_members` | `LOTAR_AUTO_POPULATE_MEMBERS` | `true`/`false` | Automatically add new assignees/reporters into the members list. |
| `auto_identity` | `LOTAR_AUTO_IDENTITY` | `true`/`false` | Enables identity guessing from local config when reporter/assignee is omitted. |
| `auto_identity_git` | `LOTAR_AUTO_IDENTITY_GIT` | `true`/`false` | Extends identity detection with git config (name/email). |

### Issue metadata & custom fields

| Config key | Env var(s) | Accepted values | Purpose |
| --- | --- | --- | --- |
| `issue_states` | `LOTAR_ISSUE_STATES` | JSON/YAML list | Overrides the allowed workflow states. |
| `issue_types` | `LOTAR_ISSUE_TYPES` | JSON/YAML list | Replaces the list of type options. |
| `issue_priorities` | `LOTAR_ISSUE_PRIORITIES` | JSON/YAML list | Replaces the priority scale shown in CLI/UI. |
| `tags` | `LOTAR_ISSUE_TAGS` | JSON/YAML list | Provides the canonical tag whitelist. |
| `custom_fields` | `LOTAR_CUSTOM_FIELDS` | JSON/YAML list or comma string | Declares additional per-task fields. |

### Attachments

| Config key | Env var(s) | Accepted values | Purpose |
| --- | --- | --- | --- |
| `attachments_dir` | `LOTAR_ATTACHMENTS_DIR` | Path string | Root directory for storing attachments. Relative paths resolve under the tasks directory; `..` is rejected. |
| `attachments_max_upload_mb` | `LOTAR_ATTACHMENTS_MAX_UPLOAD_MB` | Integer MiB (`-1`, `0`, or positive) | Maximum upload size for attachments. `0` disables uploads; `-1` allows unlimited uploads; positive values are MiB. |

### Automation & branching behavior

| Config key | Env var(s) | Accepted values | Purpose |
| --- | --- | --- | --- |
| `auto_set_reporter` | `LOTAR_AUTO_SET_REPORTER` | `true`/`false` | Automatically set the reporter to the caller when missing. |
| `auto_assign_on_status` | `LOTAR_AUTO_ASSIGN_ON_STATUS` | `true`/`false` | Toggles first-change auto-assignment. |
| `auto_codeowners_assign` | `LOTAR_AUTO_CODEOWNERS_ASSIGN` | `true`/`false` | Assign tasks based on CODEOWNERS matches. |
| `auto_tags_from_path` | `LOTAR_AUTO_TAGS_FROM_PATH` | `true`/`false` | Adds tags inferred from filesystem paths. |
| `auto_branch_infer_type` | `LOTAR_AUTO_BRANCH_INFER_TYPE` | `true`/`false` | Infers issue type from git branches. |
| `auto_branch_infer_status` | `LOTAR_AUTO_BRANCH_INFER_STATUS` | `true`/`false` | Infers status from branch prefixes. |
| `auto_branch_infer_priority` | `LOTAR_AUTO_BRANCH_INFER_PRIORITY` | `true`/`false` | Infers priority from branch prefixes. |
| `branch_type_aliases` | `LOTAR_BRANCH_TYPE_ALIASES` | JSON/YAML map | Custom branch-to-type mapping. |
| `branch_status_aliases` | `LOTAR_BRANCH_STATUS_ALIASES` | JSON/YAML map | Custom branch-to-status mapping. |
| `branch_priority_aliases` | `LOTAR_BRANCH_PRIORITY_ALIASES` | JSON/YAML map | Custom branch-to-priority mapping. |

### Source scanning

| Config key | Env var(s) | Accepted values | Purpose |
| --- | --- | --- | --- |
| `scan_signal_words` | `LOTAR_SCAN_SIGNAL_WORDS` | JSON/YAML list | Additional keywords that trigger TODO/scan detection. |
| `scan_ticket_patterns` | `LOTAR_SCAN_TICKET_PATTERNS` | JSON/YAML list | Regex patterns for ticket detection. |
| `scan_enable_ticket_words` | `LOTAR_SCAN_ENABLE_TICKET_WORDS` | `true`/`false` | Toggles recognition of words such as `ticket:` during scans. |
| `scan_enable_mentions` | `LOTAR_SCAN_ENABLE_MENTIONS` | `true`/`false` | Enables mention parsing inside scanned files. |
| `scan_strip_attributes` | `LOTAR_SCAN_STRIP_ATTRIBUTES` | `true`/`false` | Removes HTML attributes before scanning content. |

### Sprint defaults

| Config key | Env var(s) | Accepted values | Purpose |
| --- | --- | --- | --- |
| `sprints.defaults.capacity_points` | `LOTAR_SPRINTS_DEFAULT_CAPACITY_POINTS` | Integer | Default story point capacity per sprint. |
| `sprints.defaults.capacity_hours` | `LOTAR_SPRINTS_DEFAULT_CAPACITY_HOURS` | Integer | Default hour capacity per sprint. |
| `sprints.defaults.length` | `LOTAR_SPRINTS_DEFAULT_LENGTH` | Integer days | Sprint length in days. |
| `sprints.defaults.overdue_after` | `LOTAR_SPRINTS_DEFAULT_OVERDUE_AFTER` | Integer days | When overdue counts begin. |
| `sprints.notifications.enabled` | `LOTAR_SPRINTS_NOTIFICATIONS_ENABLED` | `true`/`false` | Toggles sprint reminder notifications. |

## Runtime Toggles & Services

| Variable | Values | Effect |
| --- | --- | --- |
| `LOTAR_SSE_DEBOUNCE_MS` | Integer milliseconds | Default debounce for `/api/events` when clients omit `?debounce_ms=`. |
| `LOTAR_SSE_READY` | `1` | Allows clients to request an immediate `ready` SSE event; primarily used by integration tests. |
| `LOTAR_STATS_EFFORT_CAP` | Integer | Caps the rolling window size used by `lotar stats effort`. |
| `LOTAR_MCP_AUTORELOAD` | `0`/`1` (default `1`) | Controls whether the MCP stdio server restarts when the binary changes. |
| `LOTAR_DOCS_BASE_URL` | URL | Overrides the base used when rendering hyperlinks inside `lotar help`. |
| `LOTAR_HELP_STYLE` | `ansi`/`plain` | Forces colored (`ansi`) or plain-text help output regardless of TTY detection. |

## Diagnostics & Test Helpers

| Variable | Values | Effect |
| --- | --- | --- |
| `LOTAR_TEST_SILENT` | `1` | Suppresses most warnings during automated tests. |
| `LOTAR_VERBOSE` | `1` | Re-enables informational logs (such as config creation) even when tests are silenced. |
| `LOTAR_DEBUG` | Any non-empty | Dumps search/transition internals to `/tmp/lotar_*_debug.log` for troubleshooting. |
| `LOTAR_DEBUG_ADD` | `1` | Emits detailed payload logs for `lotar add` into `/tmp/lotar_add_debug.log`. |
| `LOTAR_DEBUG_STATUS` | Any non-empty | Emits verbose `status` command and storage logs. |
| `LOTAR_TEST_FAST_IO` | `1` | Shortens SSE debounce timers and heartbeats for local tests. |
| `LOTAR_TEST_FAST_NET` | `1` | Integration-test helper that shrinks HTTP client timeouts. |

Related docs: [Configuration Reference](./config-reference.md), [Precedence](./precedence.md), [serve](./serve.md), and [sse](./sse.md).
