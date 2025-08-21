# lotar effort

Change or view a task's effort estimate.

Usage

```bash
lotar effort <TASK_ID> [NEW_EFFORT] [--clear] [--dry-run] [--explain]
```

- NEW_EFFORT is optional. If omitted (and not using --clear), the current effort is shown.
- Effort strings support time units and points:
  - Time: h (hours), d (days=8h), w (weeks=40h). Examples: 5h, 2d, 1w.
  - Points: plain numbers are treated as story points. Example: 3 (-> 3pt).
- Values are normalized on write (e.g., 1d -> 8.00h; 3 -> 3pt).

Options

- `--clear` Clear the effort value
- `-n, --dry-run` Preview without saving
- `-e, --explain` Explain normalization and parsing

Examples

```bash
# Show current effort
lotar effort PROJ-12

# Set effort using time
lotar effort PROJ-12 2d

# Set effort using points
lotar effort PROJ-12 5

# Clear effort
lotar effort PROJ-12 --clear

# Preview (no write)
lotar effort PROJ-12 1w --dry-run --explain
```
