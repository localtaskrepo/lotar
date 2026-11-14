# lotar trust

Allow this repository's automation commands and agent profiles to run on your machine, like `direnv allow`.

## Usage

```bash
lotar trust            # trust the current contents
lotar trust --status   # show whether the repository is trusted and what it would run
lotar trust --revoke   # stop trusting it
```

## Why

Two kinds of repository files can make LoTaR run programs:

- automation rules with a `run` action: `.tasks/automation.yml` and `.tasks/<PROJECT>/automation.yml`
- agent profiles under `agents:` in `.tasks/config.yml` and `.tasks/<PROJECT>/config.yml`, which pick the program an agent job starts

These files are committed with the repository, so they can come from a clone, a pull request or a fork. Until you trust them, LoTaR:

- skips automation `run` actions from those files and reports each one on stderr, as an `automation_skipped` SSE event and as a comment on the ticket (the rule's other actions still apply; see [automation](automation.md))
- refuses to start agent jobs that use a profile defined in those files

Rules from your home automation file (`~/.lotar.automation.yml`) and agent jobs started with `--runner` and no profile are your own, so they always run.

## What is recorded

`lotar trust` records a fingerprint of the automation files and the `agents:` sections of the config files, keyed by the tasks directory. The record lives in your home directory: `~/.lotar.trust.yml`, or `~/.lotar/trust.yml` when `~/.lotar` is a directory. Set `LOTAR_TRUST_FILE` to use another file.

Any change to those files made outside LoTaR (an edit, a pull, a merge) makes the repository untrusted again until you review it and run `lotar trust`. `lotar trust --status` lists every `run` command and agent profile so you can review them. Changes to other settings in `config.yml` do not affect trust.

Edits made through LoTaR itself (the web UI, REST, MCP, `lotar config set`) keep a trusted repository trusted. They cannot make an untrusted repository trusted.

## CI and containers

Set `LOTAR_TRUST_ALL=1` to trust every repository, for example in CI or in a container that only ever sees your own checkout. A repository with no automation files and no agent profiles needs no trust.

## Output

With `--format json`, `lotar trust` prints `{"trusted": true, "commands": [...]}`, `--status` prints `{"state": "trusted" | "changed" | "untrusted" | "nothing-to-trust", "allows_commands": ..., "commands": [...]}`, and `--revoke` prints `{"revoked": true}`.
