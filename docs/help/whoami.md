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
2. Project manifest author picked from the current workspace or repo root. We read `package.json` (`author` string/object or first `contributors` entry), `Cargo.toml` (`authors` array), and any `.csproj` file's `<Authors>` tag.
3. Git metadata (`user.name`, then `user.email`) from `.git/config` when `auto.identity_git` is true (default).
4. Environment fallbacks: `$USER`, then `$USERNAME`.

Toggles:

- `auto.identity` (default: true) - When false, only step 1 runs. If no configured reporter is present, the command emits `Could not resolve current user` and exits with `no-identity`.
- `auto.identity_git` (default: true) - When false, git detectors are skipped, but manifest and env fallbacks still run.

The CLI also shows when these toggles are disabled inside `--explain` output.

## Output shapes

- Text (default): prints the resolved user. With `--explain`, extra informational lines include `source`, `confidence`, optional `details`, and a reminder about which detectors are active (for example, "Auto identity disabled" or the literal resolution order string).
- JSON (`--format=json`): prints `{ "user": "..." }`. With `--explain`, the payload expands to `{ "user", "source", "confidence", "details", "auto_identity", "auto_identity_git" }`.

When no identity can be resolved, the command prints `Could not resolve current user` and exits non-zero.

## Relationship to other surfaces

- `@me` aliases inside CLI/REST/MCP flows call the non-explain resolver. That helper prioritizes `default_reporter`, then git `user.name`/`user.email`, then `$USER`/`$USERNAME`. Project manifest authors are not considered there yet, so `lotar whoami` may succeed (via manifest) even if `@me` still fails.
- `GET /api/whoami` currently uses the same fast-path resolver as `@me`. Use the CLI if you need manifest-aware resolution or the `--explain` diagnostics.

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
