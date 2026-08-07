# `.tasks/` — LoTaR's own backlog

This directory is **LoTaR dogfooding itself**: it is the real, version-controlled task
backlog for developing LoTaR. It is committed to git (see the repo-root `.gitignore` —
only the runtime subdirs `@attachments/`, `@reports/`, and `.context/` are ignored).

## Working with it

```bash
# List the backlog
lotar list -p DEV

# Add a task
lotar add "Your task title" -p DEV --type=feature --priority=medium

# Update status / leave a progress note
lotar status DEV-1 in_progress
lotar comment DEV-1 -m "what changed + where + next step"
```

The backlog is project `DEV` (`.tasks/config.yml` sets it as the default), so bare commands
resolve to DEV — but pass `-p DEV` (or `--project=DEV`) explicitly to be unambiguous.

## For testing / dev experiments

**Do not pollute this backlog with test data.** The automated suite uses isolated temp
workspaces (`smoke/helpers/workspace.ts`) and never touches this directory. For manual or
ad-hoc testing, use the task generator (`npm run seed:test-tasks`; see
`.github/skills/lotar-dev-tracking`), which materializes throwaway tasks into a directory
of your choice.
