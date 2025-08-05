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

# Scan with custom patterns
lotar scan --pattern="TODO|FIXME|HACK"

# Scan and create tasks automatically
lotar scan --create-tasks

# Scan specific file types
lotar scan --extensions=rs,js,py
```

## Options

- `<PATH>` - Directory or file to scan (default: current directory)
- `--pattern <PATTERN>` - Custom regex pattern for comments to find
- `--extensions <EXT1,EXT2>` - File extensions to scan
- `--create-tasks` - Automatically create tasks from found comments
- `--exclude <PATTERN>` - Exclude files/directories matching pattern
- `--recursive` - Scan directories recursively (default: true)

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

- Scan results are not automatically saved as tasks
- Use `--create-tasks` to convert comments to actual tasks
- Patterns are case-insensitive
- Binary files are automatically skipped
- Hidden files and directories are ignored by default
