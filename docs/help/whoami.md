# lotar whoami

Show the identity LoTaR will use whenever it needs a person (reporter defaults, `@me` expansion, auto-assignment).


## Usage

```bash
lotar whoami [--explain] [--format=json]
```

`--format` is the global output flag shared by every CLI command. Text is the default.

## Resolution pipeline

The CLI runs a detector stack and stops at the first hit:

1. `default_reporter` from the merged configuration (CLI overrides -> env such as `LOTAR_DEFAULT_REPORTER` -> home directory config -> project config -> global config -> compiled defaults).
2. Git metadata (`user.name`, then `user.email`) from the repo root when `auto.identity_git` is true (default).
3. Environment fallbacks: `$USER`, then `$USERNAME`.
4. Project manifest author as a last resort. We read `package.json` (`author` string/object or first `contributors` entry), `Cargo.toml` (`authors` array), and any `.csproj` file's `<Authors>` tag. Manifest authors are a static guess, so they only apply when no git or system identity exists.

Toggles:

- `auto.identity` (default: true) - When false, the git and manifest detectors are skipped and only `default_reporter` plus the `$USER`/`$USERNAME` fallback run. If neither is present, the command emits `Could not resolve current user` and exits with `no-identity`.
- `auto.identity_git` (default: true) - When false, git detectors are skipped, but env and manifest fallbacks still run.

The CLI also shows when these toggles are disabled inside `--explain` output.

## Output shapes

- Text (default): prints the resolved user. With `--explain`, extra informational lines include `source`, `confidence`, optional `details`, and a reminder about which detectors are active (for example, "Auto identity disabled" or the literal resolution order string).
- JSON (`--format=json`): prints `{ "user": "..." }`. With `--explain`, the payload expands to `{ "user", "source", "confidence", "details", "auto_identity", "auto_identity_git" }`.

When no identity can be resolved, the command prints `Could not resolve current user` and exits non-zero.

## Relationship to other surfaces

- `@me` aliases, `GET /api/whoami`, and task stamping all use this exact resolver (single shared implementation), so CLI, REST, and MCP can never disagree about the current identity.
- Filtering by `@me` (for example `GET /api/tasks/list?assignee=@me` or MCP `task_list`) fails closed with an explicit error when no identity can be resolved, rather than returning an unfiltered list.

## Examples

```bash
# Basic lookup in text mode
lotar whoami

# Inspect detectors, confidence, and toggle states
lotar whoami --explain

# Machine-friendly output
lotar whoami --format=json
lotar whoami --format=json --explain
```

Sample JSON (with `--explain`):

```json
{
  "user": "Jane Example",
  "source": "project manifest author",
  "confidence": 90,
  "details": "package.json at /repo/package.json",
  "auto_identity": true,
  "auto_identity_git": true
}
```

Use [identity.md](./identity.md) for a deeper reference on how identities flow through automation and caching.
