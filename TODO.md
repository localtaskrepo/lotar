Features:
- VSCode Plugin (Contexts?, Issue updates, in-editor quick hints for TODOs with references/quick create dialogs)
- IntelliJ Plugin
- External references implementation: Add "link" type reference that can point to external URLs (e.g. design docs, Figma files, etc.)
- Allow CLI list/search to filter by age older than N days
- Allow ticket files to be stored in neseted folders and handle them transparently (allows for devs to group tickets together without breaking the scanning logic. This also paves the way for very large repos that e.g. want to put old tickets into archives). Maybe we even support an "archive" function.
- Add ability to add references to a ticket via CLI/WEB UI/MCP. Introduce a smart short format e.g. <filename>#<line(s)
- Extend custom fields to support more complex queries (AND/OR logic)
- Should the api support pagination?