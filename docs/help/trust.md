# lotar trust

Allow this repository's automation commands and agent profiles to run on your machine, like `direnv allow`.

## Usage

```bash
lotar trust            # trust the current contents
lotar trust --status   # show whether the repository is trusted and what it would run
lotar trust --revoke   # stop trusting it
```

## Why

Four kinds of repository settings can make LoTaR run programs:

- automation rules with a `run` action: `.tasks/automation.yml` and `.tasks/<PROJECT>/automation.yml`
- agent profiles under `agents:` in `.tasks/config.yml` and `.tasks/<PROJECT>/config.yml`, which pick the program an agent job starts
- the merge's checks under `agent.merge` in those config files, which lotar's own merge runs (see [agent](agent.md#lotars-own-merge))
- the command under `agent.worktree.waiting_run` in those config files, which LoTaR runs in a waiting ticket's worktrees (see [agent](agent.md#build-output-while-a-ticket-waits))

These files are committed with the repository, so they can come from a clone, a pull request or a fork. Until you trust them, LoTaR:

- skips automation `run` actions from those files and reports each one on stderr, as an `automation_skipped` SSE event and as a comment on the ticket (the rule's other actions still apply; see [automation](automation.md)). It keeps each skipped command for `lotar trust` (see [Skipped runs](#skipped-runs))
- refuses to start agent jobs that use a profile defined in those files
- leaves a merge job to its merge agent instead of running `agent.merge`
- doesn't run `agent.worktree.waiting_run`, and says so in the job's log (on stderr when no job ends)

Rules from your home automation file (`~/.lotar.automation.yml`), agent jobs started with `--runner` and no profile, and an `agent.worktree.waiting_run` from your home config (`~/.lotar`) are your own, so they always run. A project config's `waiting_run` wins over your home one, so it needs trust again.

## What is recorded

`lotar trust` records a fingerprint of the automation files and the `agents:`, `agent.merge` and `agent.worktree.waiting_run` sections of the config files, keyed by the tasks directory. The record lives in your home directory: `~/.lotar.trust.yml`, or `~/.lotar/trust.yml` when `~/.lotar` is a directory. Set `LOTAR_TRUST_FILE` to use another file.

Any change to those files made outside LoTaR (an edit, a pull, a merge) makes the repository untrusted again until you review it and run `lotar trust`. `lotar trust --status` lists every `run` command, agent profile, merge and waiting command so you can review them. A structured `run` (`command` with `args`) is listed as its command line, followed by the settings that differ from their defaults: `runs echo x (env A=1, cwd sub)`. Changes to other settings in `config.yml` do not affect trust.

A new LoTaR version that adds a kind of setting to this list doesn't make a trusted repository untrusted: the record names the kinds it covers, and settings of a kind added later count from your next `lotar trust`. A version that stops checking a kind shows the repository as changed until you run `lotar trust` again.

Edits made through LoTaR itself (the web UI, REST, MCP, `lotar config set`) keep a trusted repository trusted. They cannot make an untrusted repository trusted.

## Skipped runs

`lotar trust` runs the rule `run` commands it skipped while the repository was untrusted, oldest first, as they would have run then: same command, same ticket values in the `LOTAR_TICKET_*` variables and templates. A skipped run whose rule's `run` changed since is listed and not run. Each one is listed in the output and in a comment on its ticket (the rule ran, failed, or didn't run because its run command changed), and then dropped, so a second `lotar trust` doesn't run it again. LoTaR's own merge, when it trusts the merged rules again, does the same.

The skipped commands are kept in the trust file under `skipped_runs`, with the ticket, the rule and the time they were skipped. Each process that writes the trust file (`lotar serve`, the CLI, agent jobs) locks it while it writes, through a lock file beside it, so no skipped run is lost when two write at once. A run skipped while `lotar trust` was already replaying runs is replayed right after its skip is recorded.

## CI and containers

Set `LOTAR_TRUST_ALL=1` to trust every repository, for example in CI or in a container that only ever sees your own checkout. A repository with no automation files and no agent profiles needs no trust.

## Output

With `--format json`, `lotar trust` prints `{"trusted": true, "commands": [...], "skipped_runs": [{"ticket", "rule", "skipped_at", "result": "ran" | "failed" | "rule_changed", "error"?}]}`, `--status` prints `{"state": "trusted" | "changed" | "untrusted" | "nothing-to-trust", "allows_commands": ..., "commands": [...]}`, and `--revoke` prints `{"revoked": true}`.
