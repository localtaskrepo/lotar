---
name: lotar-dev-tracking
description: Track non-trivial LoTaR development in DEV tickets; read, create, update progress, resume, and hand off work through CLI or MCP.
---

## Choose the workspace and task

Use LoTaR as the durable plan and progress log, not a second TODO file or private
harness memory. Read-only requests and tiny fixes do not require a new ticket.

- Work from the assigned checkout; explicitly use its `.tasks` directory and DEV
  project. A shared external backlog is used only when the developer designates it.
- Read the assigned fully qualified ID first. Otherwise search a small page for
  the outcome before creating a task; inspect likely matches and pagination.
- Inspect configured states/types when unknown. Use installed command help or
  live MCP schemas rather than assuming every binary matches this checkout.

```bash
lotar --tasks-dir .tasks list -p DEV "guidance" --limit 10
lotar --tasks-dir .tasks --format json list DEV-14
lotar --tasks-dir .tasks config show --project DEV
lotar --tasks-dir .tasks add "<outcome>" -p DEV --type chore --description "<scope and acceptance criteria>"
```

The list query is positional, not `--search`. Verify the returned ID when using
list to fetch a ticket. Read comments with `lotar --tasks-dir .tasks comment DEV-N`
when resuming; use relationships when dependencies matter.

## CLI or MCP, not both for every operation

Prefer the already connected LoTaR MCP tools when they target this checkout;
otherwise use the CLI. MCP host prefixes vary: use the advertised tools.

| Intent | MCP tool | CLI |
| --- | --- | --- |
| Find/read | `task_list`, `task_get` | `list -p DEV "query" --limit 10`, `--format json list DEV-N` |
| Create | `task_create` | `add "title" -p DEV` |
| Status | `task_update` with `patch.status` | `status DEV-N <configured-state>` |
| Progress | `task_comment_add` | `comment DEV-N -m "text"` |
| Allowed values | `config_show`, filtered `schema_discover` | `config show --project DEV` |

For arguments, consult [MCP tools](../../../docs/help/mcp-tools.md) or the live
schema only as needed. CLI mutations preserve validation/history; do not patch
backlog YAML by hand. Serialize mutations within a shared project: even different
ticket IDs can contend on its storage lock. A coordinator owns shared ticket state;
workers report results rather than overwriting its plan.

If MCP rejects an enum allowed by project config, inspect current state and use a
validated CLI operation rather than changing configuration to appease the tool.

## Lifecycle

1. Mark the task in progress using the configured state. Record a short plan:
   scope/acceptance criteria, important paths, risks, and checks.
2. Append comments at meaningful milestones or blockers: outcome, evidence,
   decision, and next step. Reference files/tests rather than pasting transcripts.
3. Before final handoff or a session transition, record checks and remaining work,
   then apply the completion policy in [AGENTS.md](../../../AGENTS.md). Use the CLI
   or MCP to update each task completed in the current scope and verify its stored
   status; do not leave status reconciliation as an instruction for the developer.
   Do not close unrelated tickets. Use a configured blocked state only for a real
   blocker, without inventing enums or reassigning unrelated work.
4. Append a closing comment with the completed outcome, verification, limitations,
   and linked follow-ups. State whether changes remain uncommitted or unintegrated;
   append the commit reference later if that operation is authorized.

Small guidance corrections belong in the current task. Independently actionable
work gets its own ticket; do not reopen historical Done tickets for a new scope.
Preserve the original acceptance criteria and human comments.

## Handoff comment

```text
Outcome: <what is implemented or decided>
Evidence: <paths, tests/commands and results, relevant failed approach>
Remaining: <blocker or next exact action; review/integration state>
Guidance: <instruction corrected and trigger checked, or no durable finding>
```

The user-facing, project-neutral version is
[LoTaR Agent Skills](../../../docs/help/agent-skills.md); DEV conventions above
are specific to developing this repository.
