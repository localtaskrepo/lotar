# Preferences (Web UI)

Personalize the LoTaR web experience and reset per-browser state without touching your repository files.


## Access

Open the Preferences view from the main navigation (top-right menu). All settings live in the browser (`localStorage`) and apply only to the current machine and browser profile—`view/pages/Preferences.vue` never touches your repo files.

## Theme Controls

- **System** — Follow the operating system appearance (light/dark).
- **Light / Dark** — Force a specific theme regardless of the OS.
- Theme choices update instantly and persist between sessions (`localStorage` key `lotar.theme`, handled in `view/utils/theme.ts`).
- Browser chrome (`<meta name="theme-color">`) stays in sync so the mobile/desktop address bar tint matches the active theme.

## Accent Color

1. Enable **Use custom accent color** to override the default LoTaR blue.
2. Pick a color with the visual picker or type a hex value (normalized automatically).
3. Preview chip shows foreground contrast, and focus rings/buttons update immediately.
4. Use **Reset** to return to the default accent and disable the override.

Accent choices are stored in `localStorage` (`lotar.accent`). Disable the toggle to fall back to the theme defaults at any time.

## Table Layout Reset

Use **Reset saved columns and sorting** to clear TaskTable preferences stored per project. The handler walks through `localStorage` and removes keys that start with `lotar.taskTable.columns` or `lotar.taskTable.sort`, matching the persistence logic in `view/composables/useTaskTableState.ts`.

## Filter Reset

Select **Clear last used filter** to remove the saved filter state stored in this browser (for example `localStorage` key `lotar.tasks.filter`). The next visit will start with the default filter set.

## Board Filters

Boards now include a **Filters** popover next to the WIP controls. Use it to keep the final columns readable without touching server data:

- **Statuses** — pick which board columns count as “done.” Only the selected statuses are affected; everything else stays untouched.
- **Hide cards older than (days)** — optionally drop cards whose most recent status-change timestamp is older than the provided window. The helper (`findLastStatusChangeAt`) scans each task’s history, so cards without history stay visible.
- **Limit visible cards** — cap the number of done cards per column. Cards are sorted by `modified` before trimming so the newest ones stay on the board.
- **Reset** — clears the per-project settings stored in `localStorage` (`lotar.doneFilters::<PROJECT>`), restoring the full column view.

These preferences apply per project and per browser—other users or machines keep their own thresholds. The filtering happens client-side, so hidden cards still exist in the underlying tasks list and remain searchable via `lotar list` or the REST API.

## Storage Notes

- Settings never modify project configuration files; they are browser-scoped only.
- Clearing site data or using a different browser/device resets preferences.
- If storage access fails (e.g., private browsing restrictions), the UI falls back to defaults and logs a warning in the console.

## Related CLI Options

Use `lotar serve` to launch the web UI. The subcommand exposes a long `--port` flag (default 8080) plus the global `--project` flag (`-p`) shared with every CLI command:

```bash
lotar serve --port=3090 --host=0.0.0.0 --project=AUTH --tasks-dir=/work/.tasks
```

The `lotar serve` command intentionally omits a short `-p` for port because `-p` is reserved for the global project flag. Pass `--tasks-dir` or other global flags the same way—whatever context you pick for the CLI is what the browser UI will read.
