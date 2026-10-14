# Preferences (Web UI)

Personalize the LoTaR web experience and reset per-browser state without touching your repository files.


## Access

Open the Preferences view from the main navigation (top-right menu). All settings live in the browser (`localStorage` or `sessionStorage`) and apply only to the current machine and browser profile—`view/pages/Preferences.vue` never touches your repo files.

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

Use **Reset saved columns and sorting** to clear saved columns, column order, and sort settings for both Tasks and Sprints across projects. It removes the `lotar.taskTable.columns*`, `lotar.taskTable.columnOrder*`, `lotar.taskTable.sort*`, and corresponding `lotar.sprints.*` keys, plus `lotar.boardFields*` and `lotar.calendarHoverFields*`. It does not reset theme, accent, page size, hidden tabs, or repository data.

## Filter Reset

Select **Clear last used filter** to remove `lotar.tasks.filter`, `lotar.sprints.filter`, `lotar.calendar.filter`, and `lotar.boards.filter` from localStorage and legacy sessionStorage. It does not delete saved-filter presets or repository data. An explicit URL query can still restore filters on the next visit.

## Tasks List

Use **Default page size** (25, 50, 100, or 200; default 50) for Tasks and the independent Sprint/backlog pagers. It is browser-scoped; an explicit Tasks URL page size takes precedence. See [list pagination](./list.md#pagination) for the separate CLI contract.

## Navigation

Hide tabs you do not use to reduce clutter in the main navigation. Uncheck a tab on the Preferences page and it disappears from the nav immediately; the page itself stays reachable by URL. Preferences is always visible so you can restore tabs. The setting persists in `localStorage` (key `lotar.preferences.hiddenTabs`).

## Board Filters

Boards include a **Done column** popover next to the WIP controls (both live in the page header). Use it to keep the final columns readable without touching server data:

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

Use `lotar serve` to launch the workspace-wide UI. Port selection uses `--port` or `-p` (configured `server.port`, default 8080). Serve accepts a hidden `--project` compatibility flag but ignores it; select/filter projects inside the UI, not by using `-p` for a project.

```bash
lotar serve -p 3090 --tasks-dir=/work/.tasks
```

The default bind is loopback; do not add `--host=0.0.0.0` unless network exposure is intentional. See [serve](./serve.md) for bind, port-0, and fallback behavior. Other project-scoped commands keep their own `--project` shorthand; the serve exception is specific to this subcommand.
