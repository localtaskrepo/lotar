# Identity & Users

Central reference for identity resolution and people fields.

## Sources and resolution order

Used wherever a person is needed (assignee, reporter, default_reporter):
1) Merged config `default_reporter` (same precedence chain described in [config.md](./config.md): CLI overrides → env vars like `LOTAR_DEFAULT_REPORTER` → home → project → global → defaults)
2) Project manifest author (package.json author, Cargo.toml authors, or .csproj `<Authors>`) detected under the current repo root
3) Git user (user.name or user.email at repo root) — gated by `auto.identity_git`
4) System user ($USER or $USERNAME)

Automation toggles (under the `auto.*` namespace in config/env/CLI `config set`):
- `auto.identity` (default: true) — disables all smart detectors when false. Only `default_reporter` is considered, so git/manifests/system are ignored.
- `auto.identity_git` (default: true) — when false, git detectors are skipped but manifest + env fallbacks remain.

Use `lotar whoami --explain` to see the chosen source, confidence, and toggle states.

The special value `@me` resolves to the current user via the order above across CLI, REST, and MCP.
Notes:
- Project manifest detection is best-effort and reads only local files in your repo root (no external tools). For package.json it supports both string and object forms of `author`, and falls back to the first `contributors` entry.
- For Cargo.toml we parse the first `authors` entry. For .csproj we read the `<Authors>` element.
- Identity is cached per tasks directory. Updating configuration (`lotar config set default.reporter ...`) or editing manifests automatically invalidates the cache via the config persistence layer.

## Inspecting the resolved identity

### CLI

```bash
lotar whoami
lotar whoami --explain --format=json
```

- Plain output prints the effective user string.
- `--explain` adds source, confidence, optional details (e.g., which manifest/gitrepo supplied the value), and confirms `auto.identity` / `auto.identity_git` states (see `src/main.rs`).
- JSON mode emits `{ "user": "..." }` by default, or `{ "user", "source", "confidence", "details", "auto_identity", "auto_identity_git" }` when `--explain` is present.

### REST / MCP

- `GET /api/whoami` returns `{ "status": "ok", "user": "..." }` using the same resolver (`src/routes.rs`).
- MCP task operations (`src/mcp/server/handlers/tasks.rs`) and REST task mutations rely on the same helper, so `@me` aliases and default reporter fallbacks behave identically across interfaces.

## Reporter vs Assignee
- reporter: who created or owns reporting responsibility; can be auto-set if missing when `auto.set_reporter: true`.
- assignee: who should execute the task; can be set explicitly or inferred via first-change semantics (below).

## First-change auto-assign
- If `auto.assign_on_status: true`, when a task moves away from the default/first status for the first time and has no assignee, LoTaR sets `assignee = resolved current user`.
- Explicit assignees are never overwritten.

Example (CLI):
```bash
# No assignee; first change from TODO to IN_PROGRESS
lotar status 1 in_progress  # assignee becomes @me
```

## Tips
- Use `lotar whoami --explain` to see your identity and source chain.
- Use `--dry-run --format=json` to preview identity effects on create/status/edit.
- See also: [Resolution & Precedence](./precedence.md).
