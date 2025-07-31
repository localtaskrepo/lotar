- Rework help system and show help per command in addition to global help
    Maybe there's a way to reuse the help in the docs to keep them in sync
- Make sure project name is used when talking about the config.yml setting, otherwise we're working with prefixes and they should be implicit if modifying e.g.a task with id PROJ-12
- Wire up config to be used as validation values when calling functions
- inconsistent variable naming with snake and camel case mixed
- Need to validate scaning for TODOs actually works and creates issues
- Check how well the search system works
- Update architecture so that commands can be executed via MCP, web-interface, or CLI
- we have different help args for CLI:
    lotar help
    lotar config --help
- option to change .tasks path for a command.
- Add ways to control output verbosity to allow more control as a tool
- default project is "auto" when it should just be what was detected at creation so the user can modify it and behavior is consistent
- config has an extension type that should be hard coded
- MCP
- Web
- Index files is generated when there's no content (no mappings)