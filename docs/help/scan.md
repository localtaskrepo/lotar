# lotar scan

Turns TODO/FIXME/BUG style comments into real tasks (or keeps their anchors fresh) without leaving your editor. Point it at a repo, filter the languages you care about, and decide whether the findings should update source files or stay in dry-run mode.

## Quick Start

```bash
# Crawl the current repo
lotar scan

# Limit the run to a directory and specific extensions
lotar scan app --include ts --include tsx

# Only touch files changed in git
lotar scan --modified-only

# Preview edits before writing any anchors
lotar scan --dry-run --detailed --context 2
```

`lotar scan [PATH ...]` accepts zero or more paths. With none, it walks from the repository root using the same workspace detection as other commands.

## When to reach for scan

| Need to… | Command | Notes |
| --- | --- | --- |
| Turn TODO comments into LoTaR tasks | `lotar scan src` | Writes tasks, inserts IDs beside the comments, and adds code references to the task file. |
| Refresh anchors after refactors | `lotar scan --reanchor` | Keeps only the latest location per task and fixes drifted line numbers even if no new TODOs exist. |
| Limit noise to active work | `lotar scan --modified-only` | Uses `git status` to choose files; automatically falls back to full scan outside a repo. |
| Feed results into tooling | `lotar --format json scan --include rs` | JSON entries provide `file`, `line`, `title`, `uuid`, and captured attributes. |
| Work in another workspace | `lotar scan --tasks-dir /path/to/.tasks ...` | Shares precedence with other commands plus `LOTAR_TASKS_DIR`. |

## Scope & filters

- Multiple paths are processed sequentially and merged in `file:line` order.
- `--include <ext>` and `--exclude <ext>` accept bare extensions (repeatable). Excludes win when both are provided.
- Hidden files and directories obey `.lotarignore`. If none exist, LoTaR falls back to `.gitignore`, `.ignore`, and git exclude files.
- Use shell globs or per-language `--include` pairs to keep scans tight inside large monorepos.

## Output & detail levels

- `--format text|table|markdown|json` behaves exactly like other commands (table/markdown fall back to text today).
- `--detailed` switches from compact summaries to a file/line header plus the matched comment; pair with `--context <N>` for surrounding lines.
- `--dry-run` stops before writing task files or editing source, letting you inspect the findings.
- Verbose logging (`--log-level debug` or `--verbose`) shows which files were skipped, ignores applied, and whether anchors were rewritten.

Sample text output:

```
Found 3 code comments:

app/main.rs:42
  TODO: Implement error handling for network requests

app/utils.rs:128
  FIXME: This function is too complex, needs refactoring

tests/integration.rs:56
  HACK: Temporary workaround for API rate limiting
```

## Inline metadata and anchor hygiene

- Inline `[key=value]` attributes on the comment are parsed when no ticket ID exists. Recognized keys: `assignee`, `priority`, `type`, `effort`, `due`/`due_date`, `tag`/`tags`. Unknown keys become custom fields.
- Mixed time/point effort values are rejected. Supported units include minutes/hours/days/weeks (`1h 30m`, `90m`) or points (`3pt`).
- `--strip-attributes[=<bool>]` overrides `scan.strip_attributes` to decide whether `[key=value]` blocks stay in the source after LoTaR injects the new ID.
- Each task receives a bidirectional reference entry: `code` anchors look like `path/to/file.rs#L118`. Existing anchors are refreshed whenever scan spots a known ID, even if no new TODOs are added.
- `--reanchor` prunes older anchors for the same file so only the newest location remains. Without the flag, LoTaR still repairs drifted anchors during non-dry-run scans.

## Signal words & supported files

Default (case-insensitive) triggers: **TODO**, **FIXME**, **HACK**, **BUG**, **NOTE**. Shapes that match:

- `WORD: message`
- `WORD (ABC-123): message` – captures an existing ID so only the anchor is updated.
- `WORD: message [assignee=@me] [priority=high] [tags=infra,login]`

LoTaR understands any language that uses common comment tokens (`//`, `#`, `--`, `;`, `%`, `/* */`, `<!-- -->`). Out of the box it covers Rust, JS/TS (including JSX/TSX), HTML/XML/Vue/Svelte, CSS/SCSS/LESS, Python, Java, C/C++, Go, Kotlin, Scala, C#, Swift, Groovy, Dart, Shell, SQL, Terraform/HCL, YAML/TOML/INI, and more.

Tweak the vocabulary via:

- `LOTAR_SCAN_SIGNAL_WORDS="TODO,FIXME,DEBT"`
- `scan.signal_words` in `.tasks/config.yml`, `.tasks/<PROJECT>/config.yml`, or `~/.lotar`
- `scan.ticket_patterns` to add regex capture groups for custom ticket IDs
- `scan.enable_ticket_words` / `scan.enable_mentions` to control whether bare ticket keys add anchors without creating new tasks

## Configuration cheat sheet

| Setting | Where | Notes |
| --- | --- | --- |
| `scan.strip_attributes` | `.tasks/config.yml`, project config, or CLI flag | CLI > project > env > home > global > defaults (drop the project layer when the command is global). |
| `LOTAR_SCAN_SIGNAL_WORDS` | Environment | Comma-separated list; beats any config file. |
| `scan.signal_words` | Home/global/project | Extend/replace the trigger words per scope. |
| `scan.enable_mentions` | Config | When true (default) bare IDs add anchors without creating tasks. |
| `scan.enable_ticket_words` | Config | Treat words like `Feature:` as valid triggers. |

Example snippets:

```yaml
# ~/.lotar
scan:
  signal_words: [TODO, FIXME, DEBT, IDEA]
  strip_attributes: true
  enable_ticket_words: true
  enable_mentions: true
  ticket_patterns:
    - "[A-Z]{2,}-\\d+"
```

```yaml
# .tasks/MYPROJ/config.yml
scan:
  signal_words: [TODO, BUG, PERF]
```

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| Comments are ignored | Confirm the extension is supported or add it via `--include`. When in doubt, add the comment token to the line (`// TODO`). |
| Nothing shows up in JSON output | Ensure you passed `lotar --format json scan …` (format flag comes before the subcommand). |
| Anchors stay stale after refactors | Run `lotar scan --reanchor` from the repo root so LoTaR can locate renamed files. |
| Inline attributes remain in code | Set `--strip-attributes=true` or enable `scan.strip_attributes` in config. |
| Scan feels slow on huge monorepos | Provide explicit paths (e.g., `lotar scan packages/auth`) and keep the default limit of languages by pairing `--include` filters with `.lotarignore`. |

See also: [Configuration Reference](./config-reference.md), [Resolution & Precedence](./precedence.md), and [list](./list.md) for ways to review the tasks you just created.
