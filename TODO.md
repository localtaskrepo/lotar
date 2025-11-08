Features:
- Shell completion with install command
- lotar config validate/normalize needs to be revisited
- Allow env override for all config values that are in all other chains
- Publish to docker hub, homebrew, npm?, ...
- VSCode Plugin (Contexts?, Issue updates, in-editor quick hints for TODOs with references/quick create dialogs)
- IntelliJ Plugin
- Show source code snippets (e.g. around TODOs) in cli
- custom properties can be used to filter and query. Custom properties are accessed like any other property (no custom: prefix anywhere)
- Github integration:
  - Link tasks to github issues (by URL or ID)
  - Auto-create github issues from tasks (with optional mapping of fields)
  - Sync status updates between lotar tasks and linked github issues
  - GitHook to update lotar tasks when linked github issues change
  - Configurable per-project settings for github integration (e.g. repo, auth token, field mappings)
  - UI elements in web interface to manage github links and sync actions
- Allow CLI list/search to filter by age older than N days
- Allow ticket files to be stored in neseted folders and handle them transparently (allows for devs to group tickets together without breaking the scanning logic. This also paves the way for very large repos that e.g. want to put old tickets into archives). Maybe we even support an "archive" function.
- Option to disable history and to rely completely on git for any historic lookups
- New issues property and search: blocked (Do we need that or can custom fields cover this?)
- Add option to choose which screen open by default (e.g. tasks, sprints, calendar, etc.)

Chores:
- Test release workflow
- parameter order is not flexible (e.g. lotar --format json sprint list works, but lotar sprint list --format json does not).

## Backlog
- [ ] Include README and LICENSE in archives
- [ ] Universal macOS binary via lipo (optional)
- [ ] Post-upload verification job (download & verify checksum/signature)
- [ ] Package managers: Homebrew, Scoop, cargo-binstall, Docker image
