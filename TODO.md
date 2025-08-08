- Need to validate scaning for TODOs actually works and creates issues
- MCP
- Web
- We need to add an author to issues ideally read from the current git user, otherwise fallback to environment value or .lotar config
    - Actually there are two fields: reporter and assignee. Reporter is the one that is auto filled with the current user on creation
- TODO: Replace parcel with vite
- we're using default_project instead of default_prefix in the global config.
- Tasks have relationships, so we should have commands to show them.
- Feature: context with references to relevant files for the task. how does this work together with scanning for TODOs?
- Comments feature (can we allow references to code?)
- project templates need to be reviewed and updated for the latest features
- Add console output still only shows a single line of info without any details about priority or status in some cases
- Audit log in issue that tracks all changes made by which user
- Due date commands don't work yet
- Add shortcuts for arguments with -- (e.g. --project shoud have -p as well)

Other refactor jobs:
- Log system