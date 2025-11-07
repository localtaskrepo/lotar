# git

Manage repository-level integrations, including git hooks shipped with LoTaR.

## Hooks

### Install

```shell
lotar git hooks install [--force] [--dry-run]
```

Configure your repository to use the hooks bundled under `.githooks`:

- detects the git repository root from the current directory
- ensures the hook scripts are executable (Unix platforms)
- runs `git config --local core.hooksPath .githooks`
- refuses to overwrite an existing value unless `--force` is supplied
- supports `--dry-run` to preview changes without modifying anything

After installation, git will run the provided `pre-commit` and `pre-push` checks automatically.
