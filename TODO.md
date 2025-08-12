Features:
- Due Date
- Assignee, Reporter
- Source Code Scanning
- Task relationship queries and graphs
- Task contexts (attachments?)
- Comments
- Audit Log
- MCP
- Web

Bugs:
- We're using default_project instead of default_prefix in the global config.
- project templates need to be reviewed and updated for the latest features
- Add console output still only shows a single line of info without any details about priority or status in some cases
- Tests show log warnings

Chores:
- TODO: Replace parcel with vite
- Add shortcuts for arguments with -- (e.g. --project shoud have -p as well)
- Binary size optimizations
