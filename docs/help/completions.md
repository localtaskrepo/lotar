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

- `generate` – Produce a completion script for a specific shell. Supports optional `--output` to write the script to a file and `--print` to echo it after writing.
- `install` – Install completion scripts into the default location for a specific shell (or all supported shells when `--shell` is omitted).

## Supported Shells

The following shells are supported via `--shell <name>`:

- `bash`
- `zsh`
- `fish`
- `powershell`
- `elvish`

## Default Install Locations

`lotar completions install` writes scripts to the conventional directories for each shell, using `$XDG_DATA_HOME`, `$ZDOTDIR`, or `$HOME` as appropriate. After installation, restart your shell or source the generated file to enable the completions.
