# lotar scan

Scan source code files for comment lines containing TODO and other signal words. By default, it applies safe in-place key insertions and creates tasks for TODOs missing a ticket key; use --dry-run to preview without writing.

## Usage

```bash
lotar scan [PATH ...] [OPTIONS]
```
## Examples


# Scan a specific directory (recursively)
lotar scan src

# Custom tasks directory
lotar scan --tasks-dir=/custom/path src/

```

## Options

- `--exclude <EXT>` - Exclude specific file extensions (repeated flag). Exclusions take precedence over includes  
- `--detailed` - Show detailed output with file paths and line numbers (includes 📄 file/line and unified-style +/- hunks)
- `--context <N>` - With `--detailed`, include N lines of surrounding context from the source file
- `--dry-run` - Preview proposed edits; do not write files
- `--strip-attributes[=<bool>]` - Override attribute stripping policy (default from config). When true, removes `[key=value]` blocks from the line after inserting the ticket key
- `--reanchor` - When adding an anchor for a TODO or existing key, prune all other `references.code` anchors for that task and keep only the newest
- `--modified-only` - Only scan files changed according to `git status` (staged/unstaged/renamed); falls back to full scan when not in a git repo

## Global Options

- `--format <FORMAT>` - Output format: text, table, json, markdown
- `--verbose` - Enable verbose output

## Environment Variables

- `LOTAR_TASKS_DIR` - Default tasks directory location

## Current Patterns

By default the scanner detects common signal words (case-insensitive) in single-line comments:
- TODO, FIXME, HACK, BUG, NOTE

Supported shapes:
- `WORD: <text>`
- `WORD (<id>): <text>`  (captures the optional id into the internal result)
- `WORD: <text> [assignee=@me] [priority=High] [tags=a,b] [due=2025-08-31]` (inline attributes parsed into the created task when an id is missing)

## Supported File Types

- Rust (`.rs`)
- JavaScript/TypeScript (`.js`, `.ts`, `.jsx`, `.tsx`)
- HTML/XML and frameworks (`.html`, `.htm`, `.xml`, `.vue`, `.svelte`)
- CSS preprocessors (`.css`, `.scss`, `.less`)
- Python (`.py`)
- Java (`.java`)
- C/C++ (`.c`, `.cpp`, `.h`, `.hpp`)
- Go, Kotlin, Scala, C#, Swift, Groovy, Dart
- Shell scripts (`.sh`, `.bash`, `.ps1`)
- Config/data: (`.yaml`, `.yml`, `.toml`, `.hcl`, `.tf`, `.ini`)
- SQL (`.sql`)
- And more with common `//`, `#`, `--`, `;`, `%`, `/* ... */`, or `<!-- ... -->` comment styles

## Output

Use the global `--format` option:
- `json` - Machine-readable JSON: an array of objects with fields `file`, `line`, `title`, `uuid`, `annotation` (aliases: `jsonl`, `json-lines`, `ndjson` currently behave the same as `json`)
- other formats - Human-readable lines; table/markdown are not yet specialized for scan and fall back to plain text
- Inline attributes recognized

Scan will parse inline metadata in bracket form on the TODO line and apply them to the created task when no key exists yet:

Recognized keys (case-insensitive):
- `assignee`, `priority`, `type`, `category`, `effort`, `due` (or `due_date`), `tag`/`tags`
- Unknown keys are preserved as custom fields on the task

Examples:

```text
// TODO: Improve logging [assignee=@me] [priority=High] [tags=core,logging] [due=2025-08-30]
```

After creating the task and inserting the key, attribute blocks can be removed from the source depending on `scan.strip_attributes` (config) or the `--strip-attributes` override.

Bi-directional references

When scan creates a task from a TODO that lacks a key, it also records a back-link in the created task under the top-level `references` array. Each entry is a minimal reference with either:
- `code`: a repo-relative code anchor like `path/to/file.rs#L118` (no code snippets are stored), or
- `link`: a generic URL

Movement resilience:
- If a scan encounters an existing ticket key in source, it ensures the corresponding task has a `code` reference for the current location.
- When adding a new `code` reference for a file, older anchors for the same file are pruned, keeping only the latest line for that file. Exact-duplicate anchors are not duplicated.

Relocation resilience (automatic re-anchoring):
- On each scan (non-dry-run), LoTaR attempts to re-anchor existing task references when code moves.
- It searches for the task key near the previously anchored line using a small proximity window; if not found, it scans the entire file.
- If the file was renamed, LoTaR uses `git status --porcelain` to map old paths to new ones and re-anchors in the new file when possible.
- This re-anchoring runs even if no new TODOs are found. Use `--reanchor` if you also want to prune cross-file anchors down to the newest one during updates.


## Example Output

```
Found 3 code comments:

src/main.rs:42
  TODO: Implement error handling for network requests

src/utils.rs:128  
  FIXME: This function is too complex, needs refactoring

tests/integration.rs:56
  HACK: Temporary workaround for API rate limiting
```

## Notes & limitations (stub)

- Results are displayed only; nothing is written or modified
- Hidden files/dirs are skipped by default; ignore files are supported:
  - `.lotarignore`: project-specific ignore rules at the scan root (if present)
  - fallback `.gitignore`: used when `.lotarignore` is absent; global and git exclude files are honored
- Block comments for common `/* ... */` languages and HTML-style `<!-- ... -->` are supported
- Recursive by default

## Configuration

You can configure which signal words are recognized and whether to strip inline attributes after insertion. Keys:

- Global (tasks scope): `scan.signal_words` in `.tasks/config.yml`
- Project: `scan.signal_words` in `.tasks/<PROJECT>/config.yml`
- Home (user scope): `scan.signal_words` in `~/.lotar`

Precedence (highest wins): CLI > env > home > project > global > defaults. In practice for this key, project overrides global; home/env can override both.

Examples:

```
# ~/.lotar (home config)
scan:
  signal_words:
  - TODO
  - FIXME
  - DEBT
  - IDEA
```

```
# .tasks/MYPROJ/config.yml (project)
scan:
  signal_words:
  - TODO
  - BUG
  - PERF

You can also add regex-based ticket key detection under `scan.ticket_patterns`, and control whether issue-type words act as signal words with `scan.enable_ticket_words`. Bare ticket keys alone do not create tasks; when `scan.enable_mentions` is true (default), they only add code anchors under `references`.

```
# .tasks/config.yml (global)
scan:
  strip_attributes: true # default: remove [key=value] from source after creating the task
  # Treat issue-type words (e.g., Feature/Bug/Chore) as signals in addition to TODO/FIXME
  enable_ticket_words: true
  # Add code anchors for existing keys found in source
  enable_mentions: true
  # Optional regex patterns to detect project-specific keys (first capture group is the key)
  ticket_patterns: ["[A-Z]{2,}-\\d+"]
```

Precedence: CLI `--strip-attributes` > project config > global config > defaults.
```

See also: [Configuration Reference](./config-reference.md) and [Resolution & Precedence](./precedence.md).
