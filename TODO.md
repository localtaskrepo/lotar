- Rework help system and show help per command in addition to global help
    Maybe there's a way to reuse the help in the docs to keep them in sync
- Make sure project name is used when talking about the config.yml setting, otherwise we're working with prefixes and they should be implicit if modifying e.g.a task with id PROJ-12
- Wire up config to be used as validation values for status and other fields
- inconsistent variable naming with snake and camel case mixed
- Need to validate scaning for TODOs actually works and creates issues
- Check how well the search system works
- Update architecture so that commands can be executed via MCP, web-interface, or CLI
- we have different help args for CLI:
    lotar help
    lotar config --help
- Add ways to control output verbosity to allow more control as a tool
    Add debug logging for better error tracing
- MCP
- Web
- Index files is generated when there's no content (no mappings)
- We need to add an author to issues ideally read from the current git user, otherwise fallback to environment value or .lotar config
- Add some command short cuts for most commonly used commands, e.g. lotar task edit <id> --status=IN_PROGRESS  do lotar status <id> <status>
- Add github actions to run tests and link the results in the readme
- Also check the help output of the binary