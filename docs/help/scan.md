# lotar scan

Scan source code files for comment lines containing TODO. This is an early stub intended to evolve into the full scanner.

## Usage

```bash
lotar scan [PATH ...] [OPTIONS]
```

## Examples

```bash
# Scan current directory
lotar scan

# Scan a specific directory (recursively)
lotar scan src

# Detailed output with more information
lotar scan --detailed

# Include specific file extensions
lotar scan src app --include rs --include js --include py

# Exclude certain file extensions
lotar scan src app --exclude log --exclude tmp

# Custom tasks directory
lotar scan --tasks-dir=/custom/path src/

# Environment variable usage
export LOTAR_TASKS_DIR=/project/tasks
lotar scan src/  # Uses environment-configured directory
```

## Options

- `<PATH>` - One or more paths (files or directories) to scan (default: current project or '.')
- `--include <EXT>` - Include specific file extensions (repeated flag). When provided, only these extensions are scanned
- `--exclude <EXT>` - Exclude specific file extensions (repeated flag). Exclusions take precedence over includes  
- `--detailed` - Show detailed output with file paths and line numbers

## Global Options

- `--format <FORMAT>` - Output format: text, table, json, markdown
- `--verbose` - Enable verbose output
- `--tasks-dir <PATH>` - Custom tasks directory (overrides environment/config)

## Environment Variables

- `LOTAR_TASKS_DIR` - Default tasks directory location

## Current Patterns

By default the scanner detects common signal words (case-insensitive) in single-line comments:
- TODO, FIXME, HACK, BUG, NOTE

Supported shapes:
- `WORD: <text>`
- `WORD (<id>): <text>`  (captures the optional id into the internal result)

## Supported File Types

- Rust (`.rs`)
- JavaScript/TypeScript (`.js`, `.ts`, `.jsx`, `.tsx`)
- Python (`.py`)
- Java (`.java`)
- C/C++ (`.c`, `.cpp`, `.h`, `.hpp`)
- Shell scripts (`.sh`, `.bash`)
- And many more...

## Output

Use the global `--format` option:
- `json` - Machine-readable JSON: an array of objects with fields `file`, `line`, `title`, `annotation`
- other formats - Human-readable lines; table/markdown are not yet specialized for scan and fall back to plain text

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
// removed: now multiple common signal words are supported by default
- Hidden files/dirs are skipped by default; ignore files are supported:
  - `.lotarignore`: project-specific ignore rules at the scan root (if present)
  - fallback `.gitignore`: used when `.lotarignore` is absent; global and git exclude files are honored
- Single-line comment tokens only; block comments not yet parsed
  
- Recursive by default

## Configuration

You can configure which signal words are recognized. Keys:

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

You can also add regex-based ticket key detection under `scan.ticket_patterns`.
```

See also: [Configuration Reference](./config-reference.md) and [Resolution & Precedence](./precedence.md).
