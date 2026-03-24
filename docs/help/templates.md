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
- **agent-pipeline** — Fully automated multi-phase agent workflow.
  - `issue.states`: Todo, Implementation, Testing, Merging, Done, HelpNeeded
  - Agent profiles: `@implement`, `@test`, `@merge`
  - Worktree enabled with `agent/` branch prefix
  - Includes automation rules for phase transitions (copy to `automation.yml`)
- **agent-reviewed** — Agent workflow with human review before merge.
  - `issue.states`: Todo, Implementation, Testing, NeedsReview, Merging, Done, HelpNeeded
  - Agent profiles: `@implement`, `@test`, `@merge`
  - After testing, assigns to `@reporter` for human review
  - Human approves by assigning to `@merge` to continue

Each template file includes metadata (`name`, `description`) plus a `config:` block. Only the `config:` block is written to disk.

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

## Automation

Templates contain only the `config:` section by default. The agent templates (`agent-pipeline`, `agent-reviewed`) include an `automation:` section at the end of the template file that you should copy to `.tasks/automation.yml` after initialization.

These automation rules define phase transitions:
- **agent-pipeline**: Automatic flow from Implementation → Testing → Merging → Done
- **agent-reviewed**: Implementation → Testing → NeedsReview (human) → Merging → Done

When a task is assigned to an agent profile (e.g., `@implement`), the agent automatically picks up the work. On completion, automation rules transition the task to the next phase.

If an agent needs clarification, the default agent instructions tell it to comment on the ticket, set the task to `HelpNeeded`, and hand it back to `@reporter` so the author can clarify the request before the agent resumes.

Generic runner failures are still handled by the automation rules in the template.

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
lotar config set default.tags '["triage","sev"]' --project=DEMO
lotar config set custom_fields '["product","squad"]' --project=DEMO
lotar config set custom_fields product,squad --project=DEMO  # comma form
lotar config set issue.tags '["frontend","backend"]' --project=DEMO
```

## Migration notes

- Older templates used `values:` wrappers and flat `issue_states` keys; the loader still accepts them and rewrites everything into `issue.*` arrays.
- The deprecated `require_assignee` flag was dropped. Use `auto.assign_on_status` (true by default) to keep assignees synchronized with status updates.
- Legacy `taxonomy.categories` / `taxonomy.tags` inputs still parse, but normalization writes them to `issue.categories` / `issue.tags`.
- `project_name` and `prefix` fields are stripped before writing the final YAML; always rely on CLI flags for those values going forward.
