# Automation Rules

Automation rules live in a dedicated YAML file separate from the main config. Rules are evaluated when a task is created, updated, or commented on, and can also respond to agent job lifecycle events.

## File locations & precedence

Rules are loaded from the first file found in this order:

1. Project: `.tasks/<PROJECT>/automation.yml`
2. Home: `~/.lotar.automation.yml` (or `~/.lotar/automation.yml` if the home config is a directory)
3. Global: `.tasks/automation.yml`

If a project automation file exists it fully replaces the global/home rules for that project.

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
            status: HelpNeeded
```

## Triggers (`when`)

Rules only evaluate on task changes. The short form `status: InProgress` means “status changed to InProgress.” Use `changes` for explicit transitions.

Supported combinators:

- `all`: all conditions must match (AND)
- `any`: at least one condition matches (OR)
- `not`: invert a condition
- `changes`: explicit from/to checks

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
- `start`: legacy catch-all that fires on every matching task change, after the specific hook

Older files may use `job_start`, `complete`/`success`, `error`/`failure` and `cancel` for the job hooks; they still work.

Actions support `set`, `add`, `remove`, and `run`.

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

Relationship actions use the shared relationship mutation rules: adding or
removing `depends_on`, `blocks`, or `related` also updates its reciprocal
assertion. Dependency evaluation includes incoming `blocks` declarations from
legacy one-sided data, not only a task's stored `depends_on` array. Missing,
deleted, or unreadable prerequisites remain unresolved. Each prerequisite is
evaluated against its own project's done-state policy. Relationship repair is
maintenance and does not itself request a status change or agent launch.

### Assignee tokens for actions

- `@assignee`, `@reporter`, `@assignee_or_reporter`, `@me`

### Running commands

Use `run` to execute a command when a rule action fires. Commands run on the server host.

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

When using the short-form shell syntax, template variable values are automatically shell-escaped (single-quoted) to prevent injection.

#### Synchronous vs asynchronous execution

By default, `run` commands execute synchronously (`wait: true`). When `wait: false`, the command runs in the background. Async command failures are logged to stderr but do not block the automation.

#### Command environment

LoTaR injects the following environment variables for automation commands:

- `LOTAR_AUTOMATION_EVENT`
- `LOTAR_TASKS_DIR`
- `LOTAR_WORKSPACE_ROOT`
- `LOTAR_TICKET_ID`
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
| `comment.*` | `text` (available on `commented` events) |
| `agent.*` | `job_id`, `runner`, `profile`, `worktree_path`, `worktree_branch` (available on job events) |

Example:

```yaml
on:
  start:
    comment: "Status changed from ${{previous.status}} to ${{ticket.status}}"
  commented:
    run: "echo ${{comment.text}}"
```

## Execution and composition semantics

- Rules run in file order. A matching rule fires its event hook(s) (`created`, `updated`, `assigned`, `sprint_changed`, `commented`, or the job hooks) and then the legacy `start` catch-all. Each action is committed before the next one applies.
- Reference changes (link/code/file/attachment/platform attach and detach, including uploads, removals, and sync-linked platform references) dispatch the `updated` event with the previous/current task pair. Reference values are not part of the condition vocabulary, so the change set is empty: bare `on.updated` rules and the legacy `start` catch-all fire once per changed operation. Conditions keep their normal semantics — change-dependent predicates (the `changes` group, short-form field conditions, and custom-field conditions, which all require the named field to have changed) cannot match an empty change set and stay silent, while current-state predicates such as `sprint` conditions and date-window checks (`before`/`within`/`older_than` on `due_date`) still evaluate the task as it is and can fire. No-op reference operations (the value was already present or absent) dispatch nothing. Comments written by the automation `comment` action never re-dispatch `on.commented`.
- Every action composes against the task's latest persisted state: tag and relationship `add`/`remove` operations from earlier rules and hooks accumulate instead of overwriting each other, and `set.custom_fields` writes only the listed keys (existing keys keep their values; writing the same key twice keeps the last write).
- Explicit replacement still wins: `set.tags`/`set.labels` replaces the whole tag list, and scalar `set` fields overwrite. When several rules set the same field, the last rule in file order wins. Within a single action, `set` applies before `add`, which applies before `remove`.
- Conditions, template variables (`ticket.*`, `previous.*`, `comment.*`), and assignee alias tokens resolve against the task as it was when the event fired, not against values written by earlier actions in the same dispatch.
- Actions in one dispatch run sequentially but are not atomic: another writer editing the task between two actions can still interleave with the sequence.

## Cooldown

Actions that resolve to the same event can trigger further automation. To prevent infinite loops, rules support a cooldown window and a maximum iteration count (`max_iterations`, default 10). Set `cooldown` to a nonnegative integer followed by `s`, `m`, `h`, or `d`, such as `30s` or `5m`; `0s` permits immediate firing. Invalid or overflowing durations are rejected when saving through LoTaR. If malformed cooldowns are introduced by hand-editing YAML, those rules are skipped rather than disabling loop protection. If a rule has already fired for the same task within the cooldown period, subsequent matches are skipped.

## Simulating rules

Use `lotar automation simulate` to test what actions a rule change would produce without applying them:

```bash
lotar automation simulate --ticket PROJ-1
```

The preview applies the matched action the same way a real run would, without writing anything: every `set` field (including due date, effort, custom fields and the `labels` alias), templates built from the ticket's current values, `@me`/`@reporter`/`@assignee`/`@assignee_or_reporter`/`@least_busy` assignees, and case-insensitive tag adds and removes. A few things can only be known when the rule actually runs; the preview lists them as warnings instead of guessing:

- Template values from the triggering event (`${{previous.*}}`, `${{comment.text}}`, `${{agent.*}}`) stay unexpanded.
- `@round_robin` and `@random` assignees are shown as the token (previewing round robin would advance the real rotation).
- Unknown status, priority or type values are reported as skipped.
- Sprint additions and removals, comments and run commands are listed as actions but not applied to the previewed task.

## Validation

LoTaR validates status/type/priority values against the project config. Invalid values are skipped at runtime and reported as warnings when saving the automation file.
