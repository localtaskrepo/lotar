Features:
- lotar config validate/normalize needs to be revisited
- Allow env override for all config values that are in all other chains
- Publish to docker hub, homebrew, npm?, ...
- VSCode Plugin (Contexts?, Issue updates, in-editor quick hints for TODOs with references/quick create dialogs)
- IntelliJ Plugin
- custom properties can be used to filter and query. Custom properties are accessed like any other property (no custom: prefix anywhere)
- External references implementation: Add "link" type reference that can point to external URLs (e.g. design docs, Figma files, etc.)
- Allow CLI list/search to filter by age older than N days
- Allow ticket files to be stored in neseted folders and handle them transparently (allows for devs to group tickets together without breaking the scanning logic. This also paves the way for very large repos that e.g. want to put old tickets into archives). Maybe we even support an "archive" function.
- Option to disable history and to rely completely on git for any historic lookups
- New issues property and search: blocked (Do we need that or can custom fields cover this?)
- Add option to choose which screen open by default (e.g. tasks, sprints, calendar, etc.)
- Add ability to add references to a ticket via CLI/WEB UI/MCP. Introduce a smart short format e.g. <filename>#<line(s)

## Backlog
- [ ] Include README and LICENSE in archives
- [ ] Universal macOS binary via lipo (optional)
- [ ] Post-upload verification job (download & verify checksum/signature)
- [ ] Package managers: Scoop, cargo-binstall, Docker image
