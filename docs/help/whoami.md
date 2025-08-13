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
2. `git` configuration from .git/config (user.name, then user.email)
3. System username (USER/USERNAME)

## Examples

```bash
lotar whoami
lotar whoami --explain
lotar whoami --format=json
```

Output is a single identity string in text mode, or `{ "user": "name" }` in JSON mode.
````
