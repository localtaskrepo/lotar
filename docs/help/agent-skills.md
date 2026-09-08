# LoTaR Agent Skills

Use LoTaR as the shared task record for your coding agent: scope, decisions,
progress, verification, and the next action survive a model or harness switch.
The same workflow works through the CLI or MCP without a LoTaR-managed agent job.

## Install the reusable skill

The standalone [lotar-workflow/SKILL.md](./skills/lotar-workflow/SKILL.md) is an
editable template, not a new server or dependency. It follows the
[Agent Skills format](https://agentskills.io/specification) and has no required
sibling files. Install it as `lotar-workflow/SKILL.md` in your harness's supported
skill directory. Install one copy, rather than both a plugin and editable copies.

GitHub Copilot supports `.github/skills/lotar-workflow/SKILL.md`. For other agents,
use their supported skills directory or personal configuration to reference the
same file; no harness-specific repository configuration is required.

Verify the installed version's discovery behavior. A Markdown file can be read by
any agent, but auto-loading and custom frontmatter fields vary by harness. A small
root `AGENTS.md` pointer is sufficient for a host without native skill support:

```markdown
For non-trivial implementation or task management, use the lotar-workflow skill
(or read .github/skills/lotar-workflow/SKILL.md). Record scope, meaningful progress,
verification, and handoff in LoTaR. Read-only requests do not mutate tasks.
```

Adjust that path to where the skill is installed. Skills in this
repository's `.github/skills` describe **developing LoTaR itself**, including DEV
tickets and its Rust/UI test gates. Install the generic template above in your own
project instead of adopting those repository-specific conventions.

## Configure the workspace

Install `lotar` and verify `lotar --version`. Identify the existing `.tasks`
directory and project prefix. If this is a new workspace, initialize it through
your normal LoTaR setup before asking the agent to manage it. Use an absolute path
when a host might launch from another directory.

```bash
# Replace these with your existing workspace and project.
TASKS_DIR="/absolute/path/to/project/.tasks"
PROJECT="APP"
lotar --tasks-dir "$TASKS_DIR" config show --project "$PROJECT"
lotar --tasks-dir "$TASKS_DIR" list -p "$PROJECT" --limit 10
```

`--tasks-dir` is explicit per command; `LOTAR_TASKS_DIR` is the equivalent
environment setting. Confirm it before writes because a missing path can create
a new workspace. The template deliberately does not hardcode status/type/priority
values: use your configured workflow. Decide who closes tasks and whether review
or integration is part of completion. See [configuration](./config.md),
[precedence](./precedence.md), and [task operations](./task.md).

## Optional MCP connection

CLI access is sufficient. For an MCP-capable agent, configure a local server in
the host's personal settings using these transport-neutral values:

| Setting | Value |
| --- | --- |
| Transport | Standard input/output (stdio) |
| Executable | `lotar` |
| Arguments | `mcp` |
| Environment | `LOTAR_TASKS_DIR=/absolute/path/to/project/.tasks` |

Hosts use different configuration keys; the process and environment remain the
same. Keep credentials in the host's secret storage, not shared config. Verify
the connection with `config_show` for your project or a filtered `task_list` before
mutating tasks. Use the advertised schemas, not assumed host-specific tool prefixes.
See [MCP setup](./mcp.md) and [tool reference](./mcp-tools.md).

If MCP rejects a project-specific status that the CLI accepts, inspect the
project config and current task, use the validated CLI path for the intended
transition, and report the mismatch. This was reproduced with LoTaR 0.8.0
(`task_update` checked default states instead of project states). Do not change
your workflow values to work around it; recheck this limitation after upgrading.

## Efficient sessions and caching

- Start from the assigned task and recent relevant comments, not the whole backlog.
  Load the skill once when relevant; its description is the discovery trigger.
- Keep repository rules and reusable agent roles stable. Put changing task IDs,
  progress, timestamps, and restrictions in task/session context, not the role header.
- Continue a coherent task in the same session when useful. For a real handoff,
  record decisions, paths, verification results, and the next action in LoTaR.
  A new agent verifies current state before acting; private chat memory is optional.
- If delegation is allowed, give a worker one bounded assignment and relevant
  context. Resume it for related follow-ups; use an independent context when the
  review needs it. One coordinator updates shared task state.
- Caching depends on the provider and host's actual request prefixes, schemas,
  breakpoints, and retention. Shared wording alone does not guarantee cache reuse
  between workers. Compaction can lose cache reuse yet reduce total cost. Do not
  preserve irrelevant context, send warm-up requests, or weaken permissions for it.
- LoTaR MCP updates tool metadata when enum hints change. Hosts must process
  `tools/listChanged` for correctness; do not suppress necessary schema updates
  to preserve a cache. Prefer focused task queries and filtered `schema_discover`
  over repeatedly requesting the whole catalog.

Model/effort routing and provider cache parameters belong in personal harness
configuration, not this skill. Verify current
[OpenAI caching](https://developers.openai.com/api/docs/guides/prompt-caching) and
[Z.ai caching](https://docs.z.ai/guides/capabilities/cache) documentation when tuning
those settings. API cache discounts need not translate directly to subscription
allowance. Measure accepted outcomes, total usage, retries, and elapsed time; keep
economic assumptions adjustable as models and harnesses improve.

## Verify and improve

Test the installed skill on an existing read-only task lookup first. Confirm that
the expected workspace is used and no tickets or code change. In an isolated test
workspace, exercise creation, a configured status transition, a progress comment,
and a handoff. Resume from the ticket in a fresh session when budget allows.
No live model calls are needed to validate the CLI/MCP operations themselves.

For instruction improvements, record an observed failure, the smallest correction,
and the check that demonstrates it. Test an unrelated request too, so a broad skill
trigger does not turn every interaction into task management. Keep evidence in the
task; promote only reusable, verified guidance. Apply your project's authorization
rules before changing instructions or policy. No new finding is a valid result.

## Related guides

- [Agent jobs](./agent.md) run external agent commands under LoTaR automation.
- [Built-in job instructions](./agent-instructions.md) are the default prompt for
  those jobs, not an installable skill; they are separate from this template.
- [AI Hero skills](https://www.aihero.dev/skills) provide optional writing, testing,
  and handoff techniques. Review selected skills and their dependencies, license,
  and default approvals/delegation before importing; avoid a second task tracker
  or a competing mandatory lifecycle.
