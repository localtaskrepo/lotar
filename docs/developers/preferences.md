# Preferences (Web UI internals)

This page documents how the SPA stores per-user settings so you can reason about migrations or automation. End-user instructions live in `docs/help/preferences.md`.

## Components & files

| Area | Source files | Notes |
| --- | --- | --- |
| Preferences view | `view/pages/Preferences.vue` | Render logic + forms. Emits events that call utility composables. |
| Theme + accent hooks | `view/utils/theme.ts`, `view/composables/useAccent.ts` | Normalizes OS theme detection, updates `<meta name="theme-color">`, clamps accent hex strings, and writes to storage. |
| Table layout state | `view/composables/useTaskTableState.ts` | Persists per-project column order + sorting under prefixed keys. |
| Filter helpers | `view/components/FilterBar.vue` | Syncs last-used filters per page to `localStorage`. |

All actions are browser-scoped; nothing in this flow touches `.tasks` or config files.

## Storage keys

| Key | Scope | Purpose | Managed in |
| --- | --- | --- | --- |
| `lotar.theme` | `localStorage` | `light`, `dark`, or `system`. Updates happen through `setThemePreference` in `theme.ts`. |
| `lotar.accent` | `localStorage` | Custom hex color + enable flag. Used by `useAccentColor`. |
| `lotar.taskTable.columns.<PROJECT>` | `localStorage` | Column order per project. Reset button iterates through keys with that prefix. |
| `lotar.taskTable.sort.<PROJECT>` | `localStorage` | Sort descriptor per project. |
| `lotar.<page>.filter` | `localStorage` | Last filter payload for pages that include `FilterBar` (e.g. `lotar.tasks.filter`, `lotar.calendar.filter`). |

When `localStorage` is unavailable (private mode, CSP), the UI falls back to defaults; all reads/writes are wrapped in `try`/`catch`.

## Reset operations

- **Theme reset** – toggling "System" rewires the reactive store and removes `lotar.theme`, allowing OS changes to flow through.
- **Accent reset** – clears `lotar.accent` and flips the toggle so computed CSS variables fall back to theme defaults.
- **Table reset** – `Preferences.vue` calls `useTaskTableState().resetAll()`, which scans `localStorage` for `lotar.taskTable.columns`/`sort` prefixes and deletes them. It’s safe across thousands of projects because the helper batches reads before mutating.
- **Filter reset** – clears the per-page `FilterBar` keys (for example `localStorage.removeItem("lotar.tasks.filter")`).

## CLI/server considerations

`lotar serve` in `src/cli/handlers/serve_handler.rs` merely hosts the built assets and REST API; it does not persist preference changes. The browser always talks to `localStorage` directly, so there is no API surface to migrate.

When introducing a new preference:

1. Define a descriptive storage key (re-use the `lotar.<feature>` namespace).
2. Update `Preferences.vue` and any related composables.
3. Document the key in the table above so future maintainers know where it lives.
4. Consider whether automated resets or migrations are needed when the data shape changes; if so, add guards during initialization.
