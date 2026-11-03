# LoTaR Documentation

LoTaR keeps your tasks as YAML files in your git repository. You can use it from the command line, a local web app, or an AI agent through MCP.

The same help pages are built into the `lotar` binary: run `lotar help <topic>`, for example `lotar help add`.

## Start here

- [Installation](help/install.md): Homebrew, Scoop, npm, install script, release downloads and Docker.
- [CLI Tour](help/cli-tour.md): a short walk through the everyday commands.
- [Help Index](help/index.md): every user help page, by topic.
- [Docker image](docker.md): run the CLI or the web server in a container.

## Quick start

```bash
cd path/to/your/repo
lotar add "Set up the project" --priority=high
lotar list
lotar status 1 in_progress
lotar serve --open   # opens the web app
```

The first command that writes creates the `.tasks` folder. See [Resolution & Precedence](help/precedence.md) for how LoTaR finds it.

## Support

- Issues and feature requests: https://github.com/localtaskrepo/lotar/issues
- Security issues: https://github.com/localtaskrepo/lotar/security/advisories/new
- Something went wrong? See [Errors](help/errors.md).
- Using a coding agent? Install the [LoTaR workflow skill](help/agent-skills.md).

## For contributors

- [Contributing](../CONTRIBUTING.md): how to build, test and send changes.
- [Developer documentation](developers/README.md): how LoTaR works inside.
- [Release guide](release-guide.md): the release checklist.
- [Architecture decisions](architecture-decisions.md).
