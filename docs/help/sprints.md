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

- `lotar sprint create [--label <text>] [--goal <text>] [--length <duration> | --ends-at <iso8601>] [--starts-at <iso8601>] [--capacity-points <n>] [--capacity-hours <n>] [--overdue-after <duration>] [--notes <text>] [--no-defaults]` — writes a new sprint file and seeds plan metadata. Unless `--no-defaults` is passed the handler merges your inputs with `sprint_defaults`, trims empty strings, and persists the result under `.tasks/@sprints/<id>.yml`.
- `lotar sprint update <id|--sprint <id>> [OPTIONS]` — edits plan or actual metadata in-place. The command aborts if you omit mutation flags. All plan fields from `create` plus `--capacity-points`, `--capacity-hours`, and `--overdue-after` are available, as are `--actual-started-at` / `--actual-closed-at` for lifecycle correction. Supplying both `--length` and `--ends-at` is allowed; canonicalization keeps `plan.ends_at`, drops `plan.length`, and emits a warning so you know what was persisted. Pass an empty string (`""`) to clear any timestamp.
- `lotar sprint list [--limit N] [--cleanup-missing]` / `lotar sprint show <id>` — inspect pending, active, overdue, or completed sprints. `--limit` must be greater than zero, and `--cleanup-missing` optionally runs the integrity cleanup routine before rendering; otherwise the handler surfaces which sprint IDs are still dangling so you can decide when to scrub them.
- `lotar sprint start [<id>] [--at <iso8601|relative>] [--force] [--no-warn]` — records `actual.started_at`. When no ID is supplied the CLI selects the next pending sprint (preferring ones whose plan says they should already be running) and prints which sprint it chose. `--at` lets you backdate/future-date the start, `--force` overrides existing timestamps or restarts a sprint whose close time has been cleared, and `--no-warn` suppresses overdue/future/overlap notices when you intentionally diverge from the plan. By default the handler warns when other sprints remain active or overdue at the requested instant.
- `lotar sprint close [<id>] [--at <iso8601|relative>] [--force] [--no-warn] [--review]` — records `actual.closed_at`. Without an explicit ID the handler closes the most recent active sprint. `--force` allows closing an unstarted sprint or resetting a prior close time, while `--review` immediately runs the review renderer after persisting. The handler warns when other sprints are still active and when the close time is overdue; both sets of notices respect the `--no-warn` flag.
- `lotar sprint delete <id|--sprint <id>> [--force] [--cleanup-missing]` — removes the sprint file after an interactive confirmation (unless `--force`). Deleting the file alone does **not** rewrite tasks; references linger until you run `--cleanup-missing` (invokes the same integrity cleanup used elsewhere) or call `lotar sprint cleanup-refs`. Use `--cleanup-missing` here when you want the sprint removal and membership cleanup to happen in one step.
- `lotar sprint review [<id>]` — prints lifecycle timings, completion counts, and outstanding work. When no ID is supplied the handler picks the most relevant sprint: latest complete sprint, otherwise the active one, then the newest pending entry. Reviews are also available via `--format json`.
- `lotar sprint stats [<id>]` — surfaces completion percentages, effort totals, and capacity utilisation alongside timeline metrics for the selected sprint. ID selection follows the same heuristic as `review`, and both the table and JSON payloads mirror the REST responses.
- `lotar sprint summary [<id>]` — emits a concise status digest (started, overdue, blocked, and completion totals) formatted for quick CLI checks or chatops posts. JSON output uses the same payload as the REST/MCP summary endpoints so automated posts do not need to shell out separately.

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

- `lotar sprint burndown [<id>] [--metric tasks|points|hours]` — generates day-by-day burn data for charting. Omit the sprint ID to reuse the same “most relevant sprint” heuristic as `review`. When the requested metric lacks data (for example, no point estimates) the handler falls back to tasks, emits a warning, and still includes the full JSON series for dashboards.
- `lotar sprint calendar [--limit N] [--include-complete]` — lists upcoming, in-flight, and optionally completed sprints with relative timing (“starts in 3d”, “ended 2w ago”) plus the observed/plan window. `--limit` must be greater than zero, truncation is called out in the output, and the JSON payload matches the REST calendar response (including whether completed sprints were skipped).
- `lotar sprint velocity [--limit N] [--metric tasks|points|hours] [--include-active]` — aggregates completed sprint throughput, showing per-sprint committed vs completed work, completion ratios, and rolling averages. The default window is 6 sprints when `--limit` is omitted, `--include-active` opts into including active/overdue sprints, and the JSON payload surfaces capacity commitment/consumption ratios for dashboards.
- All sprint analytics commands honor the global `--format json` flag, so the tabular output you see in the CLI always has a structured equivalent for dashboards and MCP tools.

## Task assignments

- `lotar sprint add [--sprint <ref>] [--allow-closed] [--force] [--cleanup-missing] <task...>` — attaches tasks to a sprint. When `--sprint` is omitted the handler tries to use the single active sprint; if none or multiple active sprints exist you must pass a sprint reference explicitly. You can also prefix the task list with a numeric ID or keyword (`next`, `previous`, `active`) and the CLI will treat it as the target if it recognises it. Membership is exclusive by default, so attempting to add a task that already belongs to another sprint raises an error unless you pass `--force`, in which case the CLI moves the task and prints which sprints were displaced. `--allow-closed` lets you backfill a closed sprint on purpose.
- `lotar sprint move [--sprint <ref>] [--allow-closed] [--cleanup-missing] <task...>` — switches tasks to a different sprint in one step. The handler accepts the same sprint-reference shortcuts as `add`, always replaces the existing membership, and prints per-task reassignment summaries so you can see what changed.
- `lotar sprint remove [--sprint <ref>] [--cleanup-missing] <task...>` — detaches tasks from the referenced sprint without touching their other memberships. Tasks that were not part of the sprint are listed under “Tasks without that sprint membership”.
- `lotar sprint backlog [--project <prefix>] [--tag TAG...] [--status STATUS...] [--assignee <value|@me>] [--limit N] [--cleanup-missing]` — lists tasks that are not assigned to any sprint. Filters mirror the standard list command, `--assignee` resolves `@me`, and `--limit` defaults to 20 (values must be greater than zero). The table output calls out when results were truncated, and JSON responses include integrity diagnostics plus the same backlog entries used by the web UI.

Add, move, remove, and backlog share the `--cleanup-missing` flag so you can scrub dangling sprint IDs before the command runs; missing references are also reported when you choose not to clean them. Only `add` and `move` support `--allow-closed`, and only `add` exposes `--force` because `move` already replaces existing memberships. The REST API and MCP tools expose the same cleanup toggles and reassignment metadata (messages plus structured `replaced` entries) for automation.

## Normalization and cleanup

- `lotar sprint normalize [<id>] --check` — verifies that sprint files are already canonical. This is the default mode when neither `--check` nor `--write` is provided, making it ideal for CI guardrails: the CLI warns about offending sprint IDs and exits with a non-zero status so you know which ones need attention.
- `lotar sprint normalize [<id>] --write` — rewrites affected sprint files into canonical YAML, trimming whitespace, dropping redundant fields, and emitting informational notices for any automatic corrections (for example, `plan.length` removal when `plan.ends_at` is present). Specify a sprint ID to limit the scope or omit it to rewrite every sprint.
- `lotar sprint cleanup-refs [<id>]` — removes sprint IDs from tasks when the backing sprint file has been deleted or intentionally de-scoped. The command accepts an optional sprint ID, warns if you ask it to purge references for a sprint that still exists, and reports how many task files were touched (including per-sprint breakdowns and any remaining missing IDs). This is the same cleanup routine invoked by the `--cleanup-missing` flag that appears on other sprint commands.

## Tips

- Sprint identifiers accept numeric values or keywords such as `next`, `previous`, and `active` wherever a sprint is required, and the add/move/remove commands will treat the first positional token as the sprint reference whenever it matches one of those forms.
- Use `--format=json` on any sprint command when integrating with scripts; JSON payloads include human-readable reassignment summaries under `messages` alongside the structured `replaced` payload so automation and dashboards can surface forced membership changes immediately.
- When multiple sprints are active, pass `--sprint <id>` explicitly; the CLI refuses to guess to avoid accidental updates.
- `lotar sprint start` and `lotar sprint close` honour `--no-warn` when you intentionally overlap sprints or set timestamps far from now. Leaving warnings enabled is useful when you want the CLI to echo the same overdue/future signals that the `sprint_notifications` config feeds to reviews.
- Web UI sprint assignment flows (task panel, backlog, board) automatically run the same integrity checks, cleaning orphaned memberships when possible and toasting any remaining missing sprint IDs so the guardrails match CLI/API/MCP behaviour.
- If a command warns about missing sprint IDs, rerun it with `--cleanup-missing` or call `lotar sprint cleanup-refs` to scrub task files after deleting sprints. That keeps list/backlog output tidy and prevents stale sprint IDs from masking real integrity issues.
