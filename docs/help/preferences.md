# Preferences (Web UI)

Personalize the LoTaR web experience and reset per-browser state without touching your repository files.

## Access

Open the Preferences view from the main navigation (top-right menu). All settings are stored in the browser (localStorage or sessionStorage) and apply only to the current machine and browser profile.

## Theme Controls

- **System** — Follow the operating system appearance (light/dark).
- **Light / Dark** — Force a specific theme regardless of the OS.
- Theme choices update instantly and persist between sessions.
- Browser chrome (`<meta name="theme-color">`) stays in sync so the mobile/desktop address bar tint matches the active theme.

## Accent Color

1. Enable **Use custom accent color** to override the default LoTaR blue.
2. Pick a color with the visual picker or type a hex value (normalized automatically).
3. Preview chip shows foreground contrast, and focus rings/buttons update immediately.
4. Use **Reset** to return to the default accent and disable the override.

Accent choices are stored in `localStorage` (`lotar.accent`). Disable the toggle to fall back to the theme defaults at any time.

## Table Layout Reset

Use **Reset saved columns and sorting** to clear TaskTable preferences stored per project (removes keys that start with `lotar.taskTable.columns*` and `lotar.taskTable.sort*`). This is helpful if columns were hidden accidentally or you want to revert to defaults.

## Filter Reset

Select **Clear last used filter** to remove the saved filter state for the Tasks list (`sessionStorage` key `lotar.tasks.filter`). The next visit will start with the default filter set.

## Storage Notes

- Settings never modify project configuration files; they are browser-scoped only.
- Clearing site data or using a different browser/device resets preferences.
- If storage access fails (e.g., private browsing restrictions), the UI falls back to defaults and logs a warning in the console.

## Related CLI Options

The serve command now supports both `--port` and the short `-p` flag for quickly changing the web server port:

```bash
lotar serve -p 3090 --host=0.0.0.0
```

Use the long `--project` flag alongside serve when you need to point LoTaR at a specific project, since `-p` is reserved for the port in this context.
