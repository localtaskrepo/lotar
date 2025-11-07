# lotar sprint

Manage sprint definitions, lifecycle, and task membership directly from the CLI. All commands work against the canonical `.tasks/@sprints/<id>.yml` files and share behaviour with the REST API, MCP tools, and web UI.

## Usage

```bash
lotar sprint <command> [OPTIONS]
```

## Override a locked sprint

When a sprint is locked because its lifecycle timestamps are already set, you can manually override them from the CLI. Run the following command to reopen or adjust the recorded window in one step:

```bash
lotar sprint update --sprint <id> --actual-closed-at <iso8601 timestamp> --actual-started-at <iso8601 timestamp>
```

Use an empty string (`""`) for either timestamp when you need to clear it entirely. Pairing both flags lets you resume a sprint by clearing the close time and correcting the start in a single invocation.

## Sprint lifecycle

- `lotar sprint create` — writes a new sprint file using configuration defaults for capacity, length, and overdue thresholds. Pass `--label`, `--goal`, `--length`, or `--end` to shape the plan up front.
- `lotar sprint update <id>` — edits plan or actual metadata. Mutually excludes `--length` and `--end`; the CLI drops `plan.length` when an explicit end date is present and surfaces a warning.
- `lotar sprint list` / `lotar sprint show <id>` — inspect pending, active, overdue, or completed sprints. Include `--cleanup-missing` to automatically strip orphaned task references before rendering results.
- `lotar sprint start [<id>]` — records `actual.started_at`. When no ID is supplied the CLI selects the next pending sprint and warns if other runs remain active or overdue.
- `lotar sprint close [<id>]` — records `actual.closed_at`. Pair with `--review` to render metrics immediately after closing.
- `lotar sprint delete <id> [--force] [--cleanup-missing]` — removes the sprint file and optionally drops dangling task references. The CLI prompts for confirmation unless `--force` is supplied. Tasks that only belonged to the deleted sprint return to the backlog; tasks with additional sprint memberships keep their remaining assignments. The REST API, MCP tools, and web UI follow the same behaviour.
- `lotar sprint update <id> --actual-closed-at ""` — clears the observed close timestamp so a completed sprint can be reopened (the CLI treats an empty string as `null`). Pair with `--actual-started-at` when you also need to adjust the start time while resuming.
- `lotar sprint review [<id>]` — prints lifecycle timings, completion counts, and outstanding work to help plan carry-over.
- `lotar sprint stats [<id>]` — surfaces completion percentages, effort totals, and capacity utilisation alongside timeline metrics for the selected sprint. When omitted, the CLI chooses the most recent complete sprint or the active sprint.
- `lotar sprint summary [<id>]` — emits a concise status digest (started, overdue, blocked, and completion totals) formatted for quick CLI checks or chatops posts.

### Lifecycle example

```bash
# Start the next pending sprint immediately and capture the timestamps
lotar sprint start

# Close the sprint once work wraps up and review the outcome in one step
lotar sprint close --review

# Reopen the sprint after discovering more work and resume where you left off
lotar sprint update 12 --actual-closed-at ""
```

The web sprint list mirrors these flows: pending sprints expose a **Start** button, active sprints offer **Complete**, and completed sprints surface a **Reopen** action that clears the close timestamp without touching recorded starts.

## Sprint analytics

- `lotar sprint burndown [<id>] [--metric tasks|points|hours]` — generates day-by-day burn data for charting. The CLI prints a table by default and includes a JSON payload with the raw samples for automation.
- `lotar sprint calendar [--limit N] [--include-complete]` — lists upcoming, in-flight, and optionally completed sprints with relative timing (“starts in 3d”, “ended 2w ago”) plus the observed/plan window. Use `--format json` to integrate the schedule with dashboards.
- `lotar sprint velocity [--limit N] [--metric tasks|points|hours] [--include-active]` — aggregates completed sprint throughput, showing per-sprint committed vs completed work, completion ratios, and rolling averages. JSON output includes capacity consumption ratios for dashboards.
- `lotar sprint stats` and `lotar sprint summary` support `--format json` when you need machine-readable analytics alongside CLI-friendly tables.

## Task assignments

- `lotar sprint add --sprint <ref> <task...>` — attaches tasks to a sprint. The command enforces single membership by default: if any task already belongs to another sprint the CLI fails with guidance to rerun using `--force` or to use `sprint move`.
- `lotar sprint move --sprint <ref> <task...>` — replaces the existing sprint membership with the selected sprint and reports which sprint IDs were displaced.
- `lotar sprint remove --sprint <ref> <task...>` — detaches tasks from the referenced sprint without modifying other memberships.
- `lotar sprint backlog` — lists tasks that are not currently assigned to any sprint, with optional filters for project, status, tags, assignee, and `--limit`.

All assignment commands accept `--cleanup-missing` to drop references to deleted sprint files before acting, and `--allow-closed` to backfill tasks into closed sprints on purpose.
REST API and MCP tools expose the same `cleanup_missing` flag and return `missing_sprints` integrity diagnostics so scripted workflows can surface orphaned memberships or confirm automatic cleanup.

## Normalization and cleanup

- `lotar sprint normalize [<id>] --check` — exits successfully after scanning sprint files, warning about any that require canonical formatting. Ideal for CI guardrails.
- `lotar sprint normalize [<id>] --write` — rewrites affected sprint files into canonical YAML, trimming whitespace, dropping redundant fields, and emitting informational notices for any automatic corrections (for example, `plan.length` removal when `plan.ends_at` is present).
- `lotar sprint cleanup-refs [<id>]` — removes sprint IDs from tasks when the backing sprint file has been deleted or intentionally de-scoped. Pair with `--cleanup-missing` on other commands to keep workspaces tidy.

## Tips

- Sprint identifiers accept numeric values or keywords such as `next`, `previous`, and `active` wherever a sprint is required.
- Use `--format=json` on any sprint command when integrating with scripts; JSON payloads now include human-readable reassignment summaries under `messages` alongside the structured `replaced` payload so automation and dashboards can surface forced membership changes immediately.
- When multiple sprints are active, pass `--sprint <id>` explicitly; the CLI refuses to guess to avoid accidental updates.
- Web UI sprint assignment flows (task panel, backlog, board) automatically run the same integrity checks, cleaning orphaned memberships when possible and toasting any remaining missing sprint IDs so the guardrails match CLI/API/MCP behaviour.
- The sprint list view defaults to hiding completed sprints so planning stays focused; enable **Show completed sprints** to review archived work, or use the inline **Reopen** action to resume a finished run without editing YAML by hand.
