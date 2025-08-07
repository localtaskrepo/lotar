# lotar scan

Scan source code files for TODO, FIXME, and other comment-based tasks.

## Usage

```bash
lotar scan [PATH] [OPTIONS]
```

## Examples

```bash
# Scan current directory
lotar scan

# Scan specific directory
lotar scan src/

# Detailed output with more information
lotar scan --detailed

# Include specific file extensions
lotar scan --include=rs,js,py

# Exclude certain file extensions
lotar scan --exclude=log,tmp

# Custom tasks directory
lotar scan --tasks-dir=/custom/path src/

# Environment variable usage
export LOTAR_TASKS_DIR=/project/tasks
lotar scan src/  # Uses environment-configured directory
```

## Options

- `<PATH>` - Directory or file to scan (default: current directory)
- `--include <EXT1,EXT2>` - Include specific file extensions
- `--exclude <EXT1,EXT2>` - Exclude specific file extensions  
- `--detailed` - Show detailed output with file paths and line numbers

## Global Options

- `--format <FORMAT>` - Output format: text, table, json, markdown
- `--verbose` - Enable verbose output
- `--tasks-dir <PATH>` - Custom tasks directory (overrides environment/config)

## Environment Variables

- `LOTAR_TASKS_DIR` - Default tasks directory location

## Default Patterns

The scanner looks for these comment patterns:
- `TODO:` or `TODO -` 
- `FIXME:` or `FIXME -`
- `HACK:` or `HACK -`
- `BUG:` or `BUG -`
- `NOTE:` or `NOTE -`

## Supported File Types

- Rust (`.rs`)
- JavaScript/TypeScript (`.js`, `.ts`, `.jsx`, `.tsx`)
- Python (`.py`)
- Java (`.java`)
- C/C++ (`.c`, `.cpp`, `.h`, `.hpp`)
- Shell scripts (`.sh`, `.bash`)
- And many more...

## Output Formats

Use global `--format` option for different output formats:
- `text` - Human-readable text (default)
- `json` - Machine-readable JSON
- `table` - Formatted table
- `markdown` - Markdown format

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

## Notes

- Scan results are displayed but not automatically saved as tasks
- Patterns are built-in for common comment styles (TODO, FIXME, etc.)
- Binary files are automatically skipped
- Hidden files and directories are ignored by default
- Scanning is recursive by default
