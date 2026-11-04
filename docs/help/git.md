# git

Activates git hooks that **your repository keeps in a `.githooks/` folder**. LoTaR does not ship hooks for your repository: create the folder and put your hook scripts in it (for example a `pre-commit` that runs your checks), then run `lotar git hooks install` so every clone can turn them on the same way.

```sh
mkdir -p .githooks
printf '#!/bin/sh\nexec cargo fmt --check\n' > .githooks/pre-commit
lotar git hooks install
```

Setting `core.hooksPath` makes git ignore `.git/hooks`. If that folder holds active hooks (anything but `*.sample`), the command lists them and stops; move them into `.githooks/` or re-run with `--force`.


## Hooks

### Install

```shell
lotar git hooks install [--force] [--dry-run] [--format json]
```

Configures your current repository so git executes the scripts in `.githooks`.

#### What the command does

1. Finds the root of the current git repository.
2. Verifies `.githooks/` exists under that root and that it contains at least one non-hidden script (files starting with `.` or ending in `.md` are ignored).
3. Reads the existing `core.hooksPath` value:
	- If it already equals `.githooks`, the command simply ensures every script is executable (Unix only) and prints a summary.
	- If another value is set, you must pass `--force` to overwrite it.
4. Unless `--dry-run` is active, runs `git config --local core.hooksPath .githooks` and makes each script executable (`chmod 755`) on Unix platforms. Windows keeps the files untouched because executable bits are not required.
5. Prints a human/JSON summary showing the new path, how many scripts were touched, and the previous value (when known).

#### Options

| Flag | Effect |
| --- | --- |
| `--dry-run` | Skips `git config` and permission changes. You get a preview message (or JSON payload) describing what would happen. |
| `--force` | Allows overwriting an existing `core.hooksPath` that points somewhere other than `.githooks`. |
| `--format json` | Emits `{ "status": "success", "action": "git_hooks_install", ... }` to stdout instead of the text summary. Errors continue to stream to stderr via the standard CLI renderer. |

#### Failure cases

- Not inside a repository → `Git repository not found…`
- `.githooks` missing or empty → error explaining what needs to be created.
- Unable to run `git` or modify permissions → error string from the failing system call.

Once installed, git will execute the repository’s `pre-commit`, `pre-push`, and any other scripts you added under `.githooks/` automatically.
