# Templates Guide

Use templates to bootstrap `.tasks/<prefix>/config.yml` with sensible vocabularies before you start editing by hand. Templates ultimately feed `lotar config init`.


## Built-in templates

List the available names (they are case-sensitive) at any time:

```bash
lotar config templates
```

- **default** — Basic backlog/bug triage.
  - `issue.states`: Todo, InProgress, NeedsReview, Done
  - `issue.types`: Feature, Bug, Chore
  - `issue.priorities`: Low, Medium, High
  - `issue.tags`: ["*"] (wildcard means "accept anything")
  - `issue.categories`: ["*"] to smooth migrations from legacy taxonomy files
  - `custom_fields`: *(not set; configure later if needed)*
- **agile** — Sprint workflow with a verify column and rich issue types.
  - `issue.states`: Todo, InProgress, Verify, Done
  - `issue.types`: Epic, Feature, Bug, Spike, Chore
  - `issue.priorities`: Low, Medium, High, Critical
  - `issue.tags`: ["*"]
  - `issue.categories`: *(not set)*
  - `custom_fields`: ["category", "sprint"] to hint at reporting dimensions
- **kanban** — Continuous flow with review/verification.
  - `issue.states`: Todo, InProgress, Verify, Done
  - `issue.types`: Feature, Bug, Epic, Chore
  - `issue.priorities`: Low, Medium, High
  - `issue.tags`: ["*"]
  - `issue.categories`: *(not set)*
  - `custom_fields`: ["category"]
- **jira** — Jira-aligned workflow with Jira remote mapping.
  - `issue.states`: Todo, InProgress, NeedsReview, Done
  - `issue.types`: Feature, Bug, Chore
  - `issue.priorities`: Low, Medium, High, Critical
  - `remotes.jira`: summary/description/status/issuetype/priority/assignee/reporter/labels mappings
  - `remotes.jira.filter`: `labels = lotar` to avoid pulling unrelated issues
- **github** — GitHub issues workflow with GitHub remote mapping.
  - `issue.states`: Todo, InProgress, NeedsReview, Done
  - `issue.types`: Feature, Bug, Chore
  - `issue.priorities`: Low, Medium, High, Critical
  - `remotes.github`: title/body/state/assignees/labels mappings
  - `remotes.github.filter`: `label:lotar` to avoid pulling unrelated issues
- **jira-github** — Dual Jira + GitHub remote mapping.
  - `remotes.jira` + `remotes.github` configured together
- **agent-pipeline** — Fully automated agent workflow.
  - `issue.states`: Todo, InProgress, Testing, Merging, Done, HelpNeeded
  - Agent profiles `@implement`, `@test`, `@merge` (Claude by default)
  - Worktrees on, with the `agent/` branch prefix
  - Automation: Implementation → Testing → Merging → Done
- **agent-reviewed** — Agent workflow with a human review before merging.
  - `issue.states`: Todo, InProgress, Testing, Review, Merging, Done, HelpNeeded
  - Agent profiles `@implement`, `@test`, `@merge` (Claude by default)
  - After testing, the ticket goes to the reporter in `Review`; assigning it to `@merge` merges it
- **agent-team** — Agent workflow that runs without a person watching each step.
  - `issue.states`: Todo, InProgress, Reviewing, Merging, Blocked, HelpNeeded, Done, WontDo (Done and WontDo are done)
  - Agent profiles `@implement`, `@implement-retry`, `@review`, `@merge` (Claude by default)
  - An agent review in a fresh session, rework rounds, one retry, prerequisites and comments that reach the agent (see below)

Each template file includes metadata (`name`, `description`) plus a `config:` block, and the agent templates an `automation:` block.

## Initializing from a template

Run `lotar config init --template=<name>` to materialize one of the templates.

- `--project <NAME>` replaces the `{{project_name}}` placeholder stored in the template and is used when deriving a prefix. If omitted, the generator falls back to `DEFAULT`.
- `--prefix <PREFIX>` forces the output directory (`.tasks/<PREFIX>/config.yml`). Without it, the CLI derives a unique prefix from the project name and refuses to overwrite an existing project unless `--force` is provided.
- `--copy-from <PREFIX>` merges settings from an existing project before applying the template (project name/prefix metadata are excluded). Use this to reuse branch aliases, scanner options, etc.
- `--global` writes `.tasks/config.yml` instead of a project-specific file.
- `--dry-run` prints the plan (including prefix availability checks) without touching disk.
- `--force` overwrites an existing config file after validation.

Every `init` call serializes the merged YAML, validates it, and writes the canonical, normalized structure. If you later hand-edit the file, run `lotar config normalize --project=PREFIX --write` (or `--global`) to reformat it again.

## Template contents & normalization rules

- Issue vocabularies are simple arrays. Legacy wrappers like `{values: [...]}` or `{primitive: ...}` are stripped by the loader, so feel free to remove them manually as well.
- Default template includes `issue.categories: ["*"]`; agile and kanban omit it because they expect explicit categories if you need them.
- Agile and kanban templates seed `custom_fields` to encourage grouping ("category", "sprint"). Delete or rename these if your workflow prefers other dimensions.
- Wildcards (`"*"`) mean "allow any value" and map to `StringConfigField::new_wildcard()` internally.
- Template metadata such as `project_name`, `prefix`, or `issue_states` is also accepted for backward compatibility. The loader rewrites those keys into the modern nested `project.*` and `issue.*` structure so you never have to.

## Agent templates

`lotar init --template agent-pipeline` (or `agent-reviewed`, `agent-team`) sets up a project the agent workflow can run in:

- The project config gets the template's statuses, types, priorities, agent profiles and `agent.worktree` settings, and `issue.done_states: [Done]` (agent-team: `[Done, WontDo]`) so a handed-back `HelpNeeded` ticket doesn't count as finished. `--states`, `--types`, `--priorities` and `--tags` still override the vocabularies; with `--states` the done states are inferred from your list (see [Done States](config.md#done-states)).
- `.tasks/<PREFIX>/automation.yml` gets the phase rules.

Assign a ticket to `@implement` to start. Each phase works in the ticket's own worktree and branch and commits there (LoTaR commits anything an agent leaves uncommitted); when a job finishes, the automation rules move the ticket to the next phase:

- **agent-pipeline**: Implementation → Testing → Merging → Done. The merge agent rebases the branch onto main, reruns the checks and fast-forwards the main checkout (LoTaR fast-forwards it if the agent can't; the job fails if the branch can't be fast-forwarded). It does not push unless your repository guidance says to.
- **agent-reviewed**: Implementation → Testing → Review. The reporter reviews the branch, then assigns the ticket to `@merge` (or comments and assigns it to `@implement` for changes). Merging → Done.
- **agent-team**: Implementation → Review → Merging → Done, with agents in every step (described below).

When something goes wrong the ticket goes back to a person instead of moving on:

- An agent that needs a decision, or finds the work can't be finished, comments, sets `HelpNeeded` and assigns the reporter. Because the assignee is no longer the phase agent, no rule moves the ticket when the job ends.
- agent-pipeline and agent-reviewed: a job whose runner crashes ends up in `HelpNeeded` with the reporter and the `agent-failed` tag, and a cancelled job is assigned to the reporter. agent-team retries first (below).

### agent-team

agent-team comes from the rules LoTaR itself is developed with, without what only that project needs. What it adds over agent-reviewed:

- **An agent review instead of a person.** When `@implement` finishes, `@review` reads the branch in a fresh session, changes nothing and comments its findings. A blocking review tags the ticket `review-blocked`. Give `@review` another runner than `@implement` (`codex` while `@implement` runs `claude`, for example), so the review isn't the same model checking its own work. There is no Testing phase: the implementer tests, the reviewer reruns the quick checks, the merge runs the checks the repository names.
- **Rework rounds.** A blocking review goes back to `@implement` as the next turn of its session, twice; the third asks the reporter.
- **One retry.** A failed `@implement` job runs once more on `@implement-retry`, then asks the reporter. A failed merge goes back to `@implement` once. A failed review asks the reporter: the work is never merged unreviewed.
- **Questions with options.** When the agents can't go on, the ticket goes to its reporter in `HelpNeeded` with a question (try again, review again, merge, close as WontDo, …). Answering it in the web UI or with `lotar comment <ID> --answer <key>` sends the ticket on; an answer in your own words reaches `@implement` like a comment.
- **Comments reach the agent.** A comment by a person on a HelpNeeded ticket, or on an InProgress one that `@implement` (or `@implement-retry`) holds, goes to the running job as its next turn, else `@implement` starts with it ([`tell_agent`](automation.md#telling-the-tickets-agent)). A comment during the review or the merge stays on the ticket: those jobs keep their own role, the review reads the comments, and a blocking review hands them to the rework. A ticket a person works on stays with them. Agent jobs comment under their runner's name (`claude`, `codex`, …), and the rule skips those names and `automation`; a person whose name is a runner's is skipped too.
- **Prerequisites.** An agent that finds the ticket waits for another one adds the `depends-on` relationship and the tag `waits`; the ticket goes `Blocked` instead of to review, and when its last prerequisite is done it goes back to `InProgress` and `@implement` continues it. Leaving `Blocked` keeps a scheduled `lotar automation run` from telling the agent again while its job waits for a slot.
- **Closing without a change.** A job with nothing to commit sets the ticket Done (or WontDo) itself, so no review or merge follows.
- **Paused jobs.** A job stopped by a usage limit says so once and runs again by itself.

Assigning `@implement` by hand starts it with the work prompt, `@review` with the review prompt and `@merge` with the merge prompt: a profile assigned by hand gets the prompt of the first rule that assigns it ([Agent run settings](automation.md#agent-run-settings)), so keep the failure rules above the review rules when you reorder them.

### Runners and models

Each profile runs `claude`; change `runner` to `codex`, `copilot` or `gemini`, and set `model` and `effort` per phase (see [Agents](agent.md#model-and-effort)). Profile `instructions` (agent-pipeline, agent-reviewed) or the rules' `agent.prompt` (agent-team) describe the phase; LoTaR adds its own instructions to every job (see [Agents](agent.md#default-instructions)).

See [automation.md](automation.md) for the full automation rule syntax.

## Auto-assignment defaults

The following auto-assignment behaviors are enabled by default (not via automation rules):

- `auto.set_reporter`
- `auto.assign_on_status`
- `auto.codeowners_assign`
- `auto.tags_from_path`
- `auto.branch_infer_type`, `auto.branch_infer_status`, `auto.branch_infer_priority`
- `auto.identity`
- `auto.identity_git`

Add an `auto.*` key only if you need to override one of those booleans for a specific config.

## Examples

Create a sprint-ready project:

```bash
lotar config init --project=backend --template=agile
```

Reuse scanner settings but apply the kanban flow:

```bash
lotar config init --project=frontend --template=kanban --copy-from=BACK
```

Preview a default bootstrap without touching disk:

```bash
lotar config init --project=data-platform --template=default --dry-run
```

Canonical project config produced by the default template:

```yaml
project:
  name: Demo Service
issue:
  states: [Todo, InProgress, NeedsReview, Done]
  types: [Feature, Bug, Chore]
  priorities: [Low, Medium, High]
  tags: ["*"]
  categories: ["*"]
# Agile/Kanban add custom fields, e.g.:
# custom:
#   fields: ["category", "sprint"]
# Automation blocks are omitted unless you override the global defaults
```

Continue customizing after initialization:

```bash
lotar config set issue.tags '["frontend","backend","triage","sev"]' --project=DEMO
lotar config set default.tags '["triage","sev"]' --project=DEMO
lotar config set custom_fields '["product","squad"]' --project=DEMO
lotar config set custom_fields product,squad --project=DEMO  # comma form
```

Set `issue.tags` before `default.tags`: once the tag list is limited, every default tag must be in it.

## Migration notes

- Older templates used `values:` wrappers and flat `issue_states` keys; the loader still accepts them and rewrites everything into `issue.*` arrays.
- The deprecated `require_assignee` flag was dropped. Use `auto.assign_on_status` (true by default) to keep assignees synchronized with status updates.
- Legacy `taxonomy.categories` / `taxonomy.tags` inputs still parse, but normalization writes them to `issue.categories` / `issue.tags`.
- `project_name` and `prefix` fields are stripped before writing the final YAML; always rely on CLI flags for those values going forward.
