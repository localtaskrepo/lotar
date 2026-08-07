---
name: lotar-dev-tracking
description: Use this to track development work in LoTaR's own committed `.tasks/` backlog — create tasks, update status, and leave progress notes (dogfooding).
---

## Why

LoTaR tracks its own development in `.tasks/` (committed, version-controlled — see `.tasks/README.md`). Use it for anything that spans more than a quick fix: it gives you a durable plan + progress log and lets work resume across sessions.

## Quick start

```bash
# List the backlog
lotar list -p DEV

# Create a task
lotar add "<title>" -p DEV --type=feature --priority=high

# Update status / leave a progress note
lotar status DEV-1 in_progress
lotar comment DEV-1 -m "what changed + where + next step"
```

## IDs + the `DEV` project

The backlog lives under project `DEV` (see `.tasks/DEV/`), and `.tasks/config.yml` sets `DEV` as the default project, so bare commands resolve to it. Still, **pass `-p DEV`** (or `--project=DEV`) explicitly for clarity and to be safe in fresh clones or before config resolution. Prefer fully-qualified IDs (`DEV-1`); numeric-only IDs (`1`) work only once the project is unambiguous.

## Complex tasks

For multi-step work, create a task early and treat it as the plan + log:
- Title = the outcome; add a first comment with the plan (files, approach, risks, test plan).
- Post short progress comments as you go (what changed, where, next step) — not full prose.
- Reference the task ID in your handoff summary (see `review-handoff`).

## Good update pattern

Keep comments short and actionable:
- What changed (1 sentence)
- Where (paths/symbols)
- Next step (1 line)

## Test data ≠ backlog

**Never** seed throwaway/test data into `.tasks/`. For manual or ad-hoc testing use the generator: `npm run seed:test-tasks` (see `.tasks/README.md`). The automated test suite uses isolated temp workspaces and never touches `.tasks/`.

## Safety

Don't paste secrets/PII into task titles or comments (tokens, auth headers, cookies).
