# Identity & Users

Central reference for identity resolution and people fields.


## Sources and resolution order

Used wherever a person is needed (assignee, reporter, default_reporter):
1) Merged config `default_reporter` (same precedence chain described in [config.md](./config.md): CLI overrides → project → env vars like `LOTAR_DEFAULT_REPORTER` → home → global → defaults)
2) Git user (user.name or user.email at repo root) — gated by `auto.identity_git`
3) System user ($USER or $USERNAME)
4) Project manifest author (package.json author, Cargo.toml authors, or .csproj `<Authors>`) — last resort only, since manifest authors are a static guess that may not be the current user

Automation toggles (under the `auto.*` namespace in config/env/CLI `config set`):
- `auto.identity` (default: true) — when false, the git and manifest detectors are disabled; only `default_reporter` and the system user ($USER/$USERNAME) are considered.
- `auto.identity_git` (default: true) — when false, git detectors are skipped but env and manifest fallbacks remain.

Use `lotar whoami --explain` to see the chosen source, confidence, and toggle states.
Use `--project <NAME_OR_PREFIX>` to inspect a specific project. Otherwise identity
uses configured `default.project`, or the base workspace configuration when no
default exists. Task operations and scoped queries use their own project.
Configuration identity details identify the winning tier; toggles are resolved
in that same project scope.

The special value `@me` resolves to the current user via the order above across CLI, REST, and MCP.
Notes:
- Project manifest detection is best-effort and reads only local files in your repo root (no external tools). For package.json it supports both string and object forms of `author`, and falls back to the first `contributors` entry.
- For Cargo.toml we parse the first `authors` entry. For .csproj we read the `<Authors>` element.
- Identity is cached per tasks directory and effective project. Configuration changes refresh identity on the next lookup, including in a running server: no restart or watcher delay is needed. This covers workspace/project/home configuration and active environment/CLI overrides, including reporter and detector-toggle changes. Content checks detect equal-length edits even when timestamps are preserved.
- Git identity reads the local configuration from the repository's shared Git directory, including linked worktrees with relative or absolute `gitdir`/`commondir` metadata. Branch details use the checkout's own `HEAD`; remote details use the same shared local config. Missing or unreadable routing metadata skips this detector rather than borrowing another repository. This remains local-file detection, not a complete expansion of Git's global config, includes, or worktree-specific overrides.
- The Git identity cache tracks resolved metadata paths and modification timestamps, so shared-config edits, worktree branch switches, and changed metadata targets refresh cached identity details.

## Inspecting the resolved identity

### CLI

```bash
lotar whoami
lotar whoami --project AUTH --explain
lotar whoami --explain --format=json
```

- Plain output prints the effective user string.
- `--explain` adds source, confidence, optional details (e.g., which manifest/gitrepo supplied the value), and confirms `auto.identity` / `auto.identity_git` states.
- JSON mode emits `{ "user": "..." }` by default, or `{ "user", "source", "confidence", "details", "auto_identity", "auto_identity_git" }` when `--explain` is present.

### REST / MCP

- `GET /api/whoami` returns `{ "data": "..." }` using the workspace's configured default project. MCP `whoami` uses that same default; neither endpoint accepts an explicit project parameter.
- MCP task operations and REST task mutations rely on the same helper, so `@me` aliases and default reporter fallbacks behave identically across interfaces.

## Reporter vs Assignee
- reporter: who created or owns reporting responsibility; can be auto-set if missing when `auto.set_reporter: true`.
- assignee: who should execute the task; can be set explicitly or inferred via first-change semantics (below).

## First-change auto-assign
- With `auto.assign_on_status: true`, the shared mutation service can assign the resolved current user on an effective status transition while the task is unassigned. A no-op transition does not gain an owner. The standalone CLI status handler additionally uses the configured default/first state and can prefer a CODEOWNERS default owner before identity; this is not part of the `@me` resolution chain.
- Explicit assignees are never overwritten.

Example (CLI):
```bash
# No assignee; first change from TODO to IN_PROGRESS
lotar status 1 in_progress  # assignee becomes @me
```

## Tips
- Use `lotar whoami --explain` to see your identity and source chain.
- Use `lotar whoami --explain --format=json` to inspect identity without mutating a task. Supported previews include `lotar status ID STATE --dry-run --explain` and `lotar task edit ID --assignee @me --dry-run`; consult each command's help rather than assuming `--dry-run` exists on every creation alias or person-field command.
- See also: [Resolution & Precedence](./precedence.md).
