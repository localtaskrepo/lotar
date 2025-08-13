# Identity & Users

Central reference for identity resolution and people fields.

## Sources and resolution order

Used wherever a person is needed (assignee, reporter, default_reporter):
1) Merged config default_reporter (precedence: CLI > env > home > project > global > defaults)
2) git user (user.name or user.email at repo root)
3) System user ($USER or $USERNAME)

The special value `@me` resolves to the current user via the order above across CLI, REST, and MCP.

## Reporter vs Assignee
- reporter: who created or owns reporting responsibility; can be auto-set if missing when `auto_set_reporter: true`.
- assignee: who should execute the task; can be set explicitly or inferred via first-change semantics (below).

## First-change auto-assign
- If `auto_assign_on_status: true`, when a task moves away from the default/first status for the first time and has no assignee, LoTaR sets `assignee = resolved current user`.
- Explicit assignees are never overwritten.

Example (CLI):
```bash
# No assignee; first change from TODO to IN_PROGRESS
lotar status AUTH-1 in_progress  # assignee becomes @me
```

## Tips
- Use `lotar whoami --explain` to see your identity and source chain.
- Use `--dry-run --format=json` to preview identity effects on create/status/edit.
- See also: [Resolution & Precedence](./precedence.md).
