# completions

Generate or install interactive shell completions for `lotar`.


## Usage

```bash
# Print bash completions to stdout
lotar completions generate --shell bash

# Write zsh completion script to a file and also print it
lotar completions generate --shell zsh --output ~/.cache/lotar/_lotar --print

# Install the default script locations for all supported shells
lotar completions install

# Install just the fish completion into the standard directory
lotar completions install --shell fish
```

Running `lotar completions` without a subcommand shows the available actions.

## Subcommands

- `generate` – Produce a completion script for a specific shell. Defaults to writing the script to `stdout`; add `--output <path>` to save it to a file/directory. When the path refers to an existing directory we let Clap name the file (`lotar`, `_lotar`, etc.). Pass `--print` alongside `--output` if you still want the script echoed after writing.
- `install` – Install completion scripts into the default location for a specific shell (or every supported shell when `--shell` is omitted). Directories are created automatically when needed, and failures are reported per shell.

## Supported Shells

The following shells are supported via `--shell <name>`:

- `bash`
- `zsh`
- `fish`
- `powershell`
- `elvish`

## Default Install Locations

`lotar completions install` writes scripts to the conventional directories below. Environment variables override the defaults when present.

| Shell | Target path |
| --- | --- |
| bash | `${XDG_DATA_HOME:-$HOME/.local/share}/bash-completion/completions/lotar` |
| zsh | `${ZDOTDIR}/completions/_lotar`, `${ZSH}/completions/_lotar`, `${XDG_DATA_HOME}/zsh/site-functions/_lotar`, or `$HOME/.zsh/completions/_lotar` (first match wins). |
| fish | `$HOME/.config/fish/completions/lotar.fish` |
| powershell | `${XDG_DATA_HOME:-$HOME/.local/share}/powershell/Scripts/lotar.ps1` |
| elvish | `$HOME/.config/elvish/lib/lotar.elv` |

If a shell’s destination cannot be created or written, the command emits a warning and keeps processing the remaining shells. At least one install must succeed or the overall command exits with an error. After installation, restart your shell or `source` the generated file to enable completions.
