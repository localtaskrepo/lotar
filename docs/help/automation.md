# Automation Rules

Automation rules live in a dedicated YAML file separate from the main config. Rules are evaluated when a task is created, updated, or commented on, can respond to agent job lifecycle events, and can be run on purpose, for example from cron (see [Running rules on a schedule](#running-rules-on-a-schedule)).

In the web UI, the **Automations** page lists every rule as plain sentences ("When an agent job finishes, if assignee is @agent: then set status to NeedsReview"), with an on/off switch, **Run now** for manual rules, and the rule editor: one screen with **When** (triggers), **If** (conditions) and **Then** (actions for each trigger). Values come from your project (statuses, priorities, types, tags, people and agent profiles); text fields have an **Insert value** menu and suggest placeholders as you type `${{`, with a preview on a real task; **Try it** shows what the rule would do to a task without changing it; and **Start from** offers ready-made examples.

## File locations & precedence

Rules are loaded from the first file found in this order:

1. Project: `.tasks/<PROJECT>/automation.yml`
2. Home: `~/.lotar.automation.yml` (or `~/.lotar/automation.yml` if the home config is a directory)
3. Global: `.tasks/automation.yml`

If a project automation file exists it fully replaces the global/home rules for that project.

`run` actions from the repository files (project and global) only run after you trust the repository with [`lotar trust`](trust.md), and again after those files change outside LoTaR. Until then they are skipped with a warning; the rule's other actions still apply. Rules from the home file always run.

## Agent job queueing

When you assign a ticket to an agent profile (e.g., `@claude-review`), LoTaR automatically starts an agent job. No explicit action is needed in your automation rules—assigning to an agent **is** the trigger.

Automation rules define what happens during the job lifecycle:

```yaml
automation:
  rules:
    - name: Agent lifecycle
      when:
        assignee: "@agent"
      on:
        job_started:
          set:
            status: InProgress
        job_completed:
          set:
            status: NeedsReview
            assignee: "@assignee_or_reporter"
        job_failed:
          set:
            status: Todo
            assignee: "@reporter"
```

Statuses in rules must exist in your project. The default states are Todo, InProgress, NeedsReview and Done. Some examples below also use `Testing` and `HelpNeeded`; the agent templates add those states (`lotar init --template agent-pipeline` or `--template agent-reviewed`). `lotar automation simulate` warns about a status that does not exist.

## Triggers (`when`)

Rules only evaluate on task changes. The short form `status: InProgress` means “status changed to InProgress.” Use `changes` for explicit transitions.

Every field condition needs its field to change in the update that fires the rule, also inside `all`. `all: [ {status: Done}, {tags: {any: [urgent]}} ]` fires only when one update sets the status to Done and changes the tags; setting the status of a ticket that is already tagged `urgent` does not fire it. The exceptions look at the task as it is now: `sprint` conditions, the time conditions below, `before`/`within`/`older_than` on `due_date`, and `assignee: { exists: ... }`. `lotar automation simulate` tells you which fields a rule needs to change ([Simulating rules](#simulating-rules)).

To check a field's value without requiring it to change, put the condition under `current`. It looks at the task as it is now, takes the same conditions as `when`, and works next to change conditions and inside `all`, `any` and `not`. This rule fires when a Change ticket moves to InProgress, although its type did not change:

```yaml
when:
  all:
    - current: { type: Change }
    - status: InProgress
```

A rule whose only condition is under `current` fires on every update of a matching task. `changes` inside `current` never matches (nothing changes in the task as it is now), and saving such a rule through LoTaR warns about it. The web UI's rule editor shows rules with `current` but edits them only in YAML, as with `not`.

Supported combinators:

- `all`: all conditions must match (AND)
- `any`: at least one condition matches (OR)
- `not`: invert a condition
- `changes`: explicit from/to checks
- `current`: conditions on the task as it is now
- `comment`: conditions on the comment of a `commented` event

A field condition can also test the value with a regular expression, e.g. `title: { matches: '^\[ops\]' }`. Patterns use the same syntax as [search text](list.md#search-text); an invalid pattern is rejected when the rules are saved through LoTaR.

### Version conditions

`version` tests a [version field](./config.md#version-fields) against a range, written as in [searches](./list.md#version-fields): `>=0.13 <0.14`, `0.13`, `^1.2`, `~0.13.2`. It matches when one of the field's versions is in the range. A declared custom field can stand at the top level or under `custom_fields`; both need the field to change in the update, unless the condition is under `current`:

```yaml
when:
  all:
    - current: { affects: { version: ">=0.13 <0.14" } }
    - status: InProgress
```

```yaml
when:
  custom_fields:
    fixed_in: { version: "^1.0" }
```

An unreadable range is rejected when the rules are saved through LoTaR.

### Time conditions

Three fields name a moment in the task's past and take `older_than`, `within` or `before` (and `exists`). They always look at the task as it is now, so they work in every rule:

- `created`: when the task was created.
- `modified`: when it last changed.
- `status_changed`: when it entered its current status (the latest status change in its history, else its creation).

`older_than: 30d` means more than 30 days ago; `within: 7d` means in the last 7 days. Spans take `h`, `d` and `w` (for example `12h`, `30d`, `2w`); an unreadable span, or one of these fields without a time operator, is rejected when saving through LoTaR. For `due_date`, `within` looks ahead instead (due in the next N days).

```yaml
when:
  all:
    - status: Done
    - status_changed: { older_than: 30d }
```

Examples:

```yaml
when:
  all:
    - status: InProgress
    - any:
        - priority: High
        - tags: { any: [urgent, customer] }
```

```yaml
when:
  changes:
    status: { from: Todo, to: InProgress }
```

### Comment conditions

`comment: { author: ... }` checks who wrote the comment that fired a `commented` rule. The author takes the same conditions as `assignee`: a name, a list of names, operators such as `starts_with` or `exists`, and the placeholders `@user`, `@agent` and `@any`. Names match case-insensitively. It works inside `all`, `any`, `not` and `current`, and never matches on other events, so put it only on rules with a `commented` hook. Comments from versions before 0.14 have no author: they match only `exists: false` (or `not`).

```yaml
- name: Owner answered
  when:
    all:
      - comment: { author: mallox }
      - current: { assignee: mallox }
  on:
    commented:
      set: { assignee: "@reporter" }
      comment: "${{comment.author}} answered; back to the reporter."
```

`lotar automation simulate --event commented` checks comment conditions against the ticket's latest comment and says whose it is (`Checked against the latest comment, by mallox.`).

### Special assignee placeholders

- `@agent`: any configured agent profile
- `@user`: any non-agent assignee
- `@any`: any non-empty assignee

## Actions (`on`)

Actions define what happens at different points in the automation lifecycle:

- `created`: when the task is created
- `updated`: when the task changes
- `assigned`: when the task's assignee changes
- `commented`: when a comment is added to the task
- `sprint_changed`: when the task's sprint memberships change
- `job_started`: when an agent job begins
- `job_completed`: when an agent job succeeds
- `job_failed`: when an agent job fails
- `job_cancelled`: when an agent job is cancelled
- `manual`: only when started on purpose: **Run now** in the web UI, `lotar automation run`, `POST /api/automation/run`, or cron calling one of them (see below)
- `unblocked`: on a ticket whose last open prerequisite (`depends_on`) reaches a done state (see [When prerequisites are done](#when-prerequisites-are-done))
- `start`: legacy catch-all that fires on every matching task change, after the specific hook

Older files may use `job_start`, `complete`/`success`, `error`/`failure` and `cancel` for the job hooks; they still work.

A status or assignee someone sets while the job runs (an agent handing the ticket back, for example) is the job's outcome: it replaces the `set.status`/`set.assignee` of the `job_completed`, `job_failed` and `job_cancelled` rules, whose other actions still run. See [Status and assignee during a job](agent.md#status-and-assignee-during-a-job).

Actions support `set`, `add`, `remove`, `comment`, `run`, `agent`, `tell_agent`, and `archive`.

```yaml
on:
  start:
    set:
      status: InProgress
      assignee: "@assignee_or_reporter"
    add:
      tags: [automation]
  job_completed:
    set:
      status: NeedsReview
  job_failed:
    set:
      status: HelpNeeded
    add:
      label: "Help Needed!"
```

### Action fields

- `set`: any task field (status, assignee, reporter, priority, type, title, description, due_date, effort, tags, custom_fields)
- `add/remove`: tags/labels, sprint membership (`sprint: <sprint reference or @active>`), and relationships (`depends_on`, `blocks`, `related`)
- `run`: execute a command when the action fires
- `comment`: add a comment to the task
- `agent`: `model`, `effort` and `prompt` for the agent job this action starts by assigning the ticket to an agent profile (see below)
- `tell_agent`: a message for the ticket's agent, queued to its running job or starting the agent (see [Telling the ticket's agent](#telling-the-tickets-agent))
- `archive`: `true` moves the task into its project's `@archive` folder, `false` brings it back (see `lotar task archive`). It runs after every other part of the action, because archived tasks are read-only. To archive tasks some time after they finish, use a manual rule run from cron (below), or `lotar task archive --select 'status:done -recent:30d'`.

```yaml
automation:
  rules:
    - name: Archive finished work
      when:
        status: Done
      on:
        updated:
          comment: "Done; archiving."
          archive: true
```

Relationship actions use the shared relationship mutation rules: adding or
removing `depends_on`, `blocks`, or `related` also updates its reciprocal
assertion. Dependency evaluation includes incoming `blocks` declarations from
legacy one-sided data, not only a task's stored `depends_on` array. Missing,
deleted, or unreadable prerequisites remain unresolved. Each prerequisite is
evaluated against its own project's done-state policy. Relationship repair is
maintenance and does not itself request a status change or agent launch.

### Agent run settings

When an action hands the ticket to an agent profile, `agent` sets how that job runs: `model`, `effort`, `provider` and `session` (for the [`t3` runner](agent.md#t3-code-threads)) and `prompt`. `session: continue` runs the job as the next turn of the ticket's last T3 Code thread, in its worktree (see [Continuing the ticket's last thread](agent.md#continuing-the-tickets-last-thread)). Each value overrides the profile's own; leave one out to keep the profile's. `prompt` replaces the default request ("work on the ticket") and accepts the same templates as comments. Settings on an action that doesn't assign an agent have no effect.

```yaml
- name: Hand over to testing
  when:
    assignee: "@implement"
  on:
    job_completed:
      set:
        status: Testing
        assignee: "@test"
      agent:
        model: haiku
        effort: low
        prompt: "Run the test suite for ${{ticket.id}} and report failures as a comment."
```

`model`, `effort` and `provider` accept templates too, so a ticket can pick its own model. A value that expands to nothing counts as not set, so a ticket without the field gets the profile's values:

```yaml
- name: Fix new findings
  on:
    created:
      set:
        assignee: "@fix"
      agent:
        provider: "${{ticket.field:provider}}"
        model: "${{ticket.field:model}}"    # the profile's model when the ticket has no `model` field
```

A merge that runs in the fix agent's thread, with its context, instead of a thread of its own:

```yaml
- name: Merge the fix
  when:
    assignee: "@fix"
  on:
    job_completed:
      set:
        assignee: "@merge"
      agent:
        session: continue
```

`lotar automation simulate` lists these settings next to the job it would queue.

### Telling the ticket's agent

`tell_agent` hands a message to the ticket's agent, so a comment (or any other event) reaches the agent that works on the ticket:

- A job runs on the ticket: the message is queued to it. A [`t3`](agent.md#t3-code-threads) job sends it to its thread as the next turn once the current turn ends, still in the same job, so the `job_completed` rules run after the agent has dealt with it. A job that runs [in another process](agent.md#jobs-in-other-processes) gets it too. Claude jobs of the same process get it on stdin; for other runners a job with the message is queued and starts when the running one ends.
- No job runs: the ticket goes to its agent, which starts with the message as its prompt, as the next turn of the ticket's last T3 Code thread where there is one (as `session: continue` does). The ticket's agent is its assignee when that is an agent profile, else the profile in `agent`, else the last agent profile it was assigned to. Without any of them the action fails and says so. Name `agent` when the last profile may be the wrong one: a ticket a merge agent handed back was last assigned to `@merge`.

`message` takes the same templates as comments; `agent` (optional) is a profile name, with or without `@`. The `model`, `effort` and `provider` of the action's `agent` settings apply to a job it starts.

Who may talk to the agent is up to the rule's conditions. Pair it with a [comment condition](#comment-conditions), or an agent's own comments come back to it as messages: only runners that get LoTaR's environment (not T3 Code threads) are kept from telling themselves. A job runs at most 10 queued turns; later messages stay on the ticket.

```yaml
- name: The owner's comments reach the agent
  when:
    comment: { author: mallox }
  on:
    commented:
      tell_agent:
        message: |
          ${{comment.author}} commented on ${{ticket.id}}: ${{comment.text}}
          Read it and act on it: answer with a ticket comment, or change the work as it asks.
        agent: fix   # for a ticket not assigned to an agent
```

The web UI describes these rules ("tell the ticket's agent …"); its rule editor edits them only in YAML.

### Assignee tokens for actions

- `@assignee`, `@reporter`, `@assignee_or_reporter`, `@me`

### Running commands

Use `run` to execute a command when a rule action fires. Commands run on the server host, and only once the repository is trusted ([`lotar trust`](trust.md)) when the rule comes from a repository file.

```yaml
on:
  job_completed:
    set:
      status: Done
    run:
      command: lotar
      args: [agent, worktree, cleanup, --delete-branches]
      # env: { FOO: bar }
      # cwd: "."
      # ignore_failure: true
      # wait: false  (default: true)
```

Short form (shell command):

```yaml
on:
  job_completed:
    run: "sh -c 'echo $LOTAR_TICKET_ID'"
```

In the short form, template values never become part of the script text. Each `${{...}}` placeholder is replaced with a reference to an environment variable (`LOTAR_TEMPLATE_1`, `LOTAR_TEMPLATE_2`, ...) that holds the value, so a title such as `$(rm -rf ~)` is printed, not run:

- unquoted, `echo ${{ticket.title}}` becomes `echo "${LOTAR_TEMPLATE_1}"` (one argument)
- inside double quotes, `echo "New: ${{ticket.title}}"` becomes `echo "New: ${LOTAR_TEMPLATE_1}"`
- inside single quotes, `sh -c 'echo ${{ticket.title}}'` becomes `sh -c 'echo ${LOTAR_TEMPLATE_1}'`, which the nested shell expands. Without a nested shell, single quotes keep the reference literal, so put placeholders outside single quotes.

The structured form passes `args` to the program directly, without a shell. If you start a shell there yourself (`command: sh`, `args: [-c, ...]`), read values from the `LOTAR_TICKET_*` variables (`"$LOTAR_TICKET_TITLE"`) rather than placing `${{...}}` in the script. On Windows the short form runs through `cmd`, which cannot keep values out of the command line, so a short-form command with placeholders is refused there; use the structured form or the environment variables.

#### Synchronous vs asynchronous execution

By default, `run` commands execute synchronously (`wait: true`): LoTaR waits for them, gives them an empty stdin, and prints their output to its own stderr, so a command can't break `--format json` output or an MCP connection. A synchronous command that runs longer than its `timeout` (structured form, e.g. `timeout: 30s`; default 5 minutes) is stopped together with the processes it started, and the action fails. When `wait: false`, the command runs in the background. Async command failures are logged to stderr but do not block the automation.

#### Command environment

LoTaR injects the following environment variables for automation commands:

- `LOTAR_AUTOMATION_EVENT`
- `LOTAR_TASKS_DIR`
- `LOTAR_WORKSPACE_ROOT`
- `LOTAR_TICKET_ID`
- `LOTAR_TICKET_PROJECT`
- `LOTAR_TICKET_STATUS`
- `LOTAR_TICKET_PRIORITY`
- `LOTAR_TICKET_TYPE`
- `LOTAR_TICKET_TITLE`
- `LOTAR_TICKET_ASSIGNEE` (when present)
- `LOTAR_TICKET_REPORTER` (when present)
- `LOTAR_AGENT_JOB_ID` (when fired from a job event)
- `LOTAR_AGENT_PROFILE` (when available)
- `LOTAR_AGENT_RUNNER` (when fired from a job event)
- `LOTAR_AGENT_WORKTREE_PATH` (when fired from a job event and worktrees are enabled)
- `LOTAR_AGENT_WORKTREE_BRANCH` (when fired from a job event and worktrees are enabled)

### Template variables

Template variables use `${{key}}` syntax in `run`, `comment`, and other string-valued action fields.

Available variables:

| Prefix | Variables |
|--------|-----------|
| `ticket.*` | `id`, `title`, `status`, `priority`, `type`, `assignee`, `reporter`, `description`, `due_date`, `effort`, `tags`, `field:<name>` |
| `previous.*` | Same as `ticket.*` but for the pre-change state (available on the update-derived hooks `updated`, `assigned`, `sprint_changed`, and the legacy `start`, which also covers task creation where no prior state exists; not available on `commented` or job hooks) |
| `comment.*` | `text`, `author` (available on `commented` events; `author` is empty for a comment without one) |
| `agent.*` | `job_id`, `runner`, `profile`, `worktree_path`, `worktree_branch` (available on job events); after a job ends also `usage`, `cost_usd`, `input_tokens`, `output_tokens`, `turns` when the CLI reported them |
| `unblocked.*` | `by` (the prerequisites with their titles: `DEV-1 (Docs), DEV-2 (Release v1.4.0)`) and `ids` (`DEV-1,DEV-2`), on `unblocked` events |

Lists take a selector in square brackets. `[*]` joins every item with ", ", a number picks one item (`[0]` is the first, `[-1]` the last), and `.field` reads a field of each item:

| List | Fields |
|------|--------|
| `ticket.tags`, `ticket.acceptance_criteria`, `ticket.sprints` | — |
| `ticket.relationships.depends_on`, `.blocks`, `.related`, `.children`, `.fixes` | — (`ticket.relationships.parent` and `.duplicate_of` are plain values) |
| `ticket.comments` | `text`, `date`, `author` (the item itself is the text) |
| `ticket.references` | `code`, `link`, `file`, `attachment`, `jira`, `github` (the item itself is whichever is set) |
| `unblocked.by` | `id`, `title`, `status` (the item itself is `ID (title)`) |

For example `${{ticket.references[*].link}}` lists every link, and `${{ticket.comments[-1].text}}` is the latest comment. The same selectors work for `previous.*`. An index out of range or an unknown field expands to nothing.

Example:

```yaml
on:
  start:
    comment: "Status changed from ${{previous.status}} to ${{ticket.status}}"
  commented:
    run: "echo ${{comment.author}} wrote ${{comment.text}}"
```

## Execution and composition semantics

- Rules run in file order. A matching rule fires its event hook(s) (`created`, `updated`, `assigned`, `sprint_changed`, `commented`, `unblocked`, or the job hooks) and then the legacy `start` catch-all. Each action is committed before the next one applies.
- Reference changes (link/code/file/attachment/platform attach and detach, including uploads, removals, and sync-linked platform references) dispatch the `updated` event with the previous/current task pair. Reference values are not part of the condition vocabulary, so the change set is empty: bare `on.updated` rules and the legacy `start` catch-all fire once per changed operation. Conditions keep their normal semantics — change-dependent predicates (the `changes` group, short-form field conditions, and custom-field conditions, which all require the named field to have changed) cannot match an empty change set and stay silent, while current-state predicates such as `sprint` conditions and date-window checks (`before`/`within`/`older_than` on `due_date`) still evaluate the task as it is and can fire. No-op reference operations (the value was already present or absent) dispatch nothing. Comments written by the automation `comment` action never re-dispatch `on.commented`.
- Every action composes against the task's latest persisted state: tag and relationship `add`/`remove` operations from earlier rules and hooks accumulate instead of overwriting each other, and `set.custom_fields` writes only the listed keys (existing keys keep their values; writing the same key twice keeps the last write).
- Explicit replacement still wins: `set.tags`/`set.labels` replaces the whole tag list, and scalar `set` fields overwrite. When several rules set the same field, the last rule in file order wins. Within a single action, `set` applies before `add`, which applies before `remove`.
- Conditions, template variables (`ticket.*`, `previous.*`, `comment.*`), and assignee alias tokens resolve against the task as it was when the event fired, not against values written by earlier actions in the same dispatch.
- Actions in one dispatch run sequentially but are not atomic: another writer editing the task between two actions can still interleave with the sequence.
- When a rule's action fails (a `run` command exits non-zero without `ignore_failure`, a timeout, a missing program), the rules after it still run. The failure is printed to stderr (`Automation rule '<name>' failed on <ID>: ...`) and sent as an `automation_failed` SSE event; the change that triggered the rules is kept.

## When prerequisites are done

A ticket that waits for others (`depends_on`, or another ticket's `blocks`) gets the `unblocked` event when the last of them reaches a done state. Each prerequisite counts as done by its own project's [done states](config.md#done-states), the same way agent assignment checks dependencies. Every way of closing one fires it: the CLI, the web UI, REST, MCP, a rule's `set` (from `lotar automation run` too) and the status an agent job ends with. It fires once per close: closing a prerequisite that is already done, or one while others are still open, does nothing; a ticket that is done itself is skipped.

The rule decides what happens, usually starting the ticket's agent with a prompt that says what it waited for. `session: continue` makes the job the next turn of the ticket's last T3 Code thread, so the agent picks up where it stopped:

```yaml
automation:
  rules:
    - name: Prerequisites done
      when:
        status: Blocked
      on:
        unblocked:
          set:
            status: InProgress
            assignee: "@fix"
          agent:
            session: continue
            prompt: |
              ${{ticket.id}} waited for ${{unblocked.by}}, which are done now.
              Read the ticket and continue the work.
```

The rule's conditions are checked against the waiting ticket as it is (`status: Blocked` above), and moving it out of that status keeps the rule from running on it again. Prerequisites in another tasks directory do not fire the event.

## Running rules on a schedule

LoTaR has no clock of its own: rules react to events. For anything time-based, give a rule a `manual` action and run it from cron, CI or any scheduler:

```yaml
automation:
  rules:
    - name: Archive finished work
      when:
        all:
          - status: Done
          - status_changed: { older_than: 30d }
      on:
        manual:
          comment: "Done for a month; archiving."
          archive: true
```

```bash
# crontab: every night at 02:00, from the repository
0 2 * * * cd /path/to/repo && lotar automation run "Archive finished work"
```

- `lotar automation run [RULE...] [--select '<query>'] [--dry-run|-n]` runs the `manual` action of the named rules (all rules that have one when no name is given) on every task their conditions match right now. `--select` narrows the tasks with the [search syntax of `lotar list`](list.md#filters-in-the-query); `--dry-run` lists what would change. Each task's own project rules apply, and `--format json` gives a report for scripts. It exits with an error when an action fails.
- `POST /api/automation/run` (`{ rules?, query?, dry_run? }`) does the same over REST; the web UI's **Run now** previews first, then runs on confirmation.
- Manual rules never run on events, and switched-off rules never run at all. Cooldowns apply as usual, so a rule with `cooldown: 1d` skips tasks it already handled within a day.
- Write conditions so a task stops matching once it is handled (for example, archived tasks drop out of the default task set, and a tag can mark tasks already processed). Then running a rule twice, or from two machines, changes nothing the second time.

## Turning rules off

`enabled: false` keeps a rule in the file but stops it from running, for events and manual runs alike; remove the key (or use the switch in the web UI) to turn it back on.

```yaml
- name: Paused for the release
  enabled: false
  when: { status: InProgress }
  on:
    updated:
      comment: "..."
```

## Cooldown

Actions that resolve to the same event can trigger further automation. To prevent infinite loops, rules support a cooldown window and a maximum iteration count (`max_iterations`, default 10: once a ticket has had that many completed or failed agent jobs in the last 24 hours in this process, further automatic agent starts on it are refused and the ticket is tagged `automation-limit-reached`). Set `cooldown` to a nonnegative integer followed by `s`, `m`, `h`, or `d`, such as `30s` or `5m`; `0s` permits immediate firing. Invalid or overflowing durations are rejected when saving through LoTaR. If malformed cooldowns are introduced by hand-editing YAML, those rules are skipped rather than disabling loop protection. If a rule has already fired for the same task within the cooldown period, subsequent matches are skipped. The last firing is remembered across processes (in `automation-cooldowns.json` in your user cache directory, or in `LOTAR_AUTOMATION_STATE_DIR`), so cron runs of `lotar automation run` and CLI edits honour cooldowns too. Rules are identified by name (or position when unnamed).

## Simulating rules

Use `lotar automation simulate` to test what actions a rule change would produce without applying them:

```bash
lotar automation simulate --ticket PROJ-1
lotar automation simulate --ticket PROJ-1 --event manual
```

The web UI's rule editor runs the same simulation on the rule you are editing (**Try it**), before it is saved.

For the change events (`created`, `updated`, `assigned`, `sprint_changed`) the simulation matches like a real update: `created` treats every field the ticket has as new, and the others look for an update of one field that fires the rule (`assigned` always changes the assignee). A match that needs a field to change says so (`Matches only when status changes in the same update.`). A rule that only fires when several fields change in the same update does not match; when nothing matches, the simulation names it (`Done and tagged: matches only when status and tags change in the same update; ...`). A rule with `current` conditions also names the fields it checked on the ticket as it is now (`Checked on the ticket as it is now: type.`).

The preview applies the matched action the same way a real run would, without writing anything: every `set` field (including due date, effort, custom fields and the `labels` alias), templates built from the ticket's current values, `@me`/`@reporter`/`@assignee`/`@assignee_or_reporter`/`@least_busy` assignees, and case-insensitive tag adds and removes. A few things can only be known when the rule actually runs; the preview lists them as warnings instead of guessing:

- Template values from the triggering event (`${{previous.*}}`, `${{comment.*}}`, `${{agent.*}}`) stay unexpanded. For `unblocked`, `${{unblocked.*}}` shows the ticket's current prerequisites.
- `@round_robin` and `@random` assignees are shown as the token (previewing round robin would advance the real rotation).
- Unknown status, priority or type values are reported as skipped.
- Sprint additions and removals, comments and run commands are listed as actions but not applied to the previewed task.

## Validation

LoTaR validates status/type/priority values against the project config. Invalid values are skipped at runtime and reported as warnings when saving the automation file.
