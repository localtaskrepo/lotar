- Only use `npm test` or `cargo nextest` for running tests. Legacy `cargo test` is forbidden.
- Always run linting and formatting checks before completing a task when there was a code change.
- Always ensure the code builds without warnings before completing a task when there was a code change.
- Always complete a task with running `npm test` and `npm run smoke` when there was a code change.
- Run the required tests even if the user does not explicitly request them.
- Running smoke tests automatically builds the project, so no need to run `npm build` separately.
- Never use git to recover lost code. Use the .history folder instead to access previous versions of files.
- Prefer creating smoke tests instead of manually testing using tmp directories.

## Self-improvement loop (maintaining instructions + skills)

- Treat instruction files as part of the product: keep them accurate, minimal, and actionable.
- If you notice confusion, drift, or missing guidance while working (wrong command, unclear policy, repeated tribal knowledge), proactively improve the relevant file:
	- Prefer the smallest edit that fixes the issue.
	- Prefer links/pointers over duplicating long runbooks.
	- Keep repo-wide docs short/evergreen; put deep workflows in skills.
- If you notice a workflow you expect to repeat, add a narrowly-scoped skill under `.github/skills/<skill-name>/SKILL.md`.
	- Before adding a new skill, search existing skills to avoid duplicates.
	- Keep skills task-focused: fastest loop commands, verification steps, and safety notes.
- Never use git as part of this “self-improvement” loop (no commits/reverts); edits are visible to the user and recoverable via `.history/`.