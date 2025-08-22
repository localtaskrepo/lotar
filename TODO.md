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
- Show source code snippets (e.g. around TODOs) in web ui and cli
- lock issue file in git if in progress? (or provide a command e.g. lotar task un-/lock <Task-ID>)
- custom properties can be used to filter and query. Custom properties are accessed like any other property (no custom: prefix anywhere)

Chores:
- Replace parcel with vite
- Check if we're Windows compatible
- Test release workflow
- Check if any of the auto features can be applied to MCP and web endpoints (or they already are)
- Config validation may need an update
- properties that don't have any special functions associated with them (e.g. categories) should be custom properties that just allow generic querying by matching terms like all custom properties should support. Only when we add special function should we promote them to standard fields.

Bugs:
- We have an operation that creates an empty config.yml and nothing else
- Help output shows raw markdown (Maybe we should split docs from direct help and more detailed help linked to)
- `lotar scan src` in this project throws an error

# Implementation Roadmap

Legend: [ ] = TODO, [x] = Done, [~] = In Progress

## Chore: 


## Backlog
- [ ] Include README and LICENSE in archives
- [ ] Universal macOS binary via lipo (optional)
- [ ] Post-upload verification job (download & verify checksum/signature)
- [ ] Package managers: Homebrew, Scoop, cargo-binstall, Docker image
