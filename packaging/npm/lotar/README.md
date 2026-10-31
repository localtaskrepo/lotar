# @localtaskrepo/lotar

LoTaR is a git-native task manager: tasks live as YAML files in your repository, next to the code.
It ships as one binary with a CLI, a web UI (`lotar serve`), and an MCP server (`lotar mcp`) for
coding agents.

This package installs the `lotar` command: the prebuilt binary for your platform (macOS, Linux, and Windows on x64 or
arm64). Node.js only starts it; nothing is compiled and no install script runs.

```bash
npm install --global @localtaskrepo/lotar
lotar add "Fix the login redirect loop" --type bug --priority high
lotar list
lotar serve          # web UI on http://localhost:8080
```

Or run it without installing:

```bash
npx -y @localtaskrepo/lotar list
```

## MCP clients

Point an MCP client at `npx` to run the server over stdio:

```json
{
  "command": "npx",
  "args": ["-y", "@localtaskrepo/lotar", "mcp"]
}
```

Pin a version (`@localtaskrepo/lotar@<version>`) if you want every session to use the same release.

## More

- [Homepage](https://localtaskrepo.github.io/lotar/)
- [Documentation](https://github.com/localtaskrepo/lotar/tree/main/docs) and
  [other ways to install](https://github.com/localtaskrepo/lotar/blob/main/docs/help/install.md)
  (Homebrew, Scoop, install scripts, `.deb`/`.rpm`, Docker)
- [Source and issues](https://github.com/localtaskrepo/lotar)

The binaries come from the same signed GitHub release as every other install method. MIT licensed.
