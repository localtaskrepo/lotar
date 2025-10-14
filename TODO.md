Features:
- Task relationship queries and graphs
- Sprints
- Shell completion with install command
- Git Hooks (e.g. for scanner)
- Project members property (for auto fill in web interface)
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

Chores:
- Tickets without ticket type (using default type) do not store that information in the file on disk
- Check if we're Windows compatible
- Test release workflow
- Check if any of the auto features can be applied to MCP and web endpoints (or they already are)
- Config validation may need an update
- properties that don't have any special functions associated with them (e.g. categories) should be custom properties that just allow generic querying by matching terms like all custom properties should support. Only when we add special function should we promote them to standard fields.
- The project config file has an "id property" that it shouldn't have, because the directory is teh ID.
- Add option to add project in config page

Bugs:
- "Category" which should now be a custom property is still treated as a first-class property in some places and should be removed. The task list for example still have a category column shown by default, indicating that it's still a first-class property.
- Add smoke testing framework for cli commands in tmp directories (doesn't need to use rust)

# Implementation Roadmap

Legend: [ ] = TODO, [x] = Done, [~] = In Progress

## Feature: Web Interface


## Backlog
- [ ] Include README and LICENSE in archives
- [ ] Universal macOS binary via lipo (optional)
- [ ] Post-upload verification job (download & verify checksum/signature)
- [ ] Package managers: Homebrew, Scoop, cargo-binstall, Docker image
