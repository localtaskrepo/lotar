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

Use **Reset saved columns and sorting** to clear saved columns, column order, and sort settings for both Tasks and Sprints across projects. It removes the `lotar.taskTable.columns*`, `lotar.taskTable.columnOrder*`, `lotar.taskTable.sort*`, and corresponding `lotar.sprints.*` keys, plus `lotar.boardFields*`, `lotar.boardSort*` and `lotar.calendarHoverFields*`. It does not reset theme, accent, page size, hidden tabs, or repository data.

## Filters

Each page with a search box (Tasks, Boards, Calendar, Sprints, Insights and Dependencies) remembers its last search and filters separately. Turn on **Use the same search and filters on every page** to keep one search and filter for all of them instead: search for something on Tasks, switch to Boards or Calendar, and the same search and filters are already applied. Clearing the filters on any page then clears them everywhere. Turning the option off returns each page to the search it last had on its own. The choice is stored in `localStorage` (`lotar.preferences.filters.shareAcrossPages`, off by default), and the shared search in `lotar.shared.filter`. A filter given in the URL still takes precedence when you open a page.

Select **Clear last used filter** to remove the saved search and filters of every page (`lotar.tasks.filter`, `lotar.sprints.filter`, `lotar.calendar.filter`, `lotar.boards.filter`, `lotar.insights.filter`, `lotar.dependencies.filter` and `lotar.shared.filter`) from localStorage and legacy sessionStorage. It does not delete saved-filter presets or repository data. An explicit URL query can still restore filters on the next visit.

## Keyboard

**Single-key shortcuts** (on by default) lets `c`, `/`, `?` and `g` followed by a letter work anywhere outside text fields (see [keyboard shortcuts](serve.md)). Turn it off if single keys get in the way of a screen reader, voice control or your own key bindings; **Ctrl+K** (**⌘K** on a Mac) still opens the command palette. The choice is stored in `localStorage` (`lotar.preferences.shortcuts.singleKey`).

## Tasks List

Use **Default page size** (25, 50, 100, or 200; default 50) for Tasks and the independent Sprint/backlog pagers. It is browser-scoped; an explicit Tasks URL page size takes precedence. See [list pagination](./list.md#pagination) for the separate CLI contract.

## Navigation

Hide tabs you do not use to reduce clutter in the main navigation. Uncheck a tab on the Preferences page and it disappears from the nav immediately; the page itself stays reachable by URL. Preferences is always visible so you can restore tabs. The setting persists in `localStorage` (key `lotar.preferences.hiddenTabs`).

**Dependencies** is visible by default and can be hidden or restored with its Navigation checkbox. Hiding it does not block direct access to `/dependencies`. Existing hidden-tab choices are preserved when new tabs are added. Table and filter resets do not change these choices; clearing browser site data restores default visibility.

## Board Columns

The **Columns** button on a board opens one dialog that arranges the board for this project and browser without changing the workflow. It shows the columns side by side, like the board:

- **Reorder columns** — drag a column by its header (or focus its ⠿ grip and use the left/right arrow keys).
- **Stack statuses** — drag a status onto another column (or onto a status to place it before that one), or use the status's **⋯** menu (**Move to …**). For example, put `WontDo` with `Done` or `QA` with `NeedsReview`. Each stacked status keeps its own section with a label, count, add button, and drop target, so dragging a card into a section sets exactly that status. Drop a status on **New column**, or choose **Own column** from its menu, to split it out again.
- **Name columns** — click the pencil next to a column title to name it (for example "Closed"); Enter saves, Escape cancels. Unnamed columns show their statuses, such as `Done · WontDo`.
- **Hide statuses** — the eye toggle hides a status; a note above the board lists hidden statuses and their task counts, with **Show all** to bring them back. Hidden tasks keep their status and still appear in lists, insights, and the CLI.
- **WIP limit** — one limit per column, counting every status in it. The header shows `count / limit` and the column is outlined when it is over the limit.
- **Hide after** (days) — hide cards that entered their current status more than this many days ago. The entry time comes from the task history (status names compared without regard to case or separators); a task that never changed status counts from its creation time. Cards that cannot be dated at all stay visible.
- **Max cards** — show only the most recently updated cards, per status.
- **Reset to defaults** — one column per configured status, nothing hidden, no limits.

Changes apply when you press **Apply**. Swimlane grouping (assignee, priority, type) keeps per-status sections inside stacked columns, with a single header row. Settings are stored in `localStorage` (`lotar.boardColumns::<PROJECT>`); WIP limits, Done-column settings, and stacking saved by earlier versions are picked up automatically. Statuses that are renamed or removed are ignored, newly configured statuses appear next to their configured neighbour, and the presentation-only **Other** column for unknown statuses is never hidden or stacked. Card rules are client-side only: hidden cards still exist and remain searchable via `lotar list` or the REST API.

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
