````markdown
# lotar whoami

Show the resolved current user identity LoTaR will use for reporter/assignee.

## Usage

```bash
lotar whoami [--explain]
```

## Details

Resolution order:
1. `default_reporter` from merged configuration, resolved with precedence: CLI > env > home > project > global > defaults
2. Project manifest author (package.json author, Cargo.toml authors, .csproj Authors)
3. `git` configuration from .git/config (user.name, then user.email)
4. System username (USER/USERNAME)

## Examples

```bash
lotar whoami
lotar whoami --explain
lotar whoami --format=json
```

Output is a single identity string in text mode, or `{ "user": "name" }` in JSON mode. When using `--explain` in JSON, fields include `source`, `confidence`, `details`, and effective `auto.identity` flags.

With `--explain`:
- Text mode prints the user and an info line: `source: <origin>, confidence: <0-100>, details: <optional>`
- JSON mode adds fields: `source`, `confidence`, and `details`.
````
