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
        job_start:
          set:
            status: InProgress
        complete:
          set:
            status: NeedsReview
            assignee: "@assignee_or_reporter"
        error:
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

- `start`: when the rule first matches (task change matches `when`)
- `commented`: when a comment is added to the task
- `sprint_changed`: when the task's sprint memberships change
- `job_start`: when an agent job begins
- `complete`: when an agent job succeeds
- `error`: when an agent job fails
- `cancel`: when an agent job is cancelled

Actions support `set`, `add`, `remove`, and `run`.

```yaml
on:
  start:
    set:
      status: InProgress
      assignee: "@assignee_or_reporter"
    add:
      tags: [automation]
  complete:
    set:
      status: NeedsReview
  error:
    set:
      status: HelpNeeded
    add:
      label: "Help Needed!"
```

### Action fields

- `set`: any task field (status, assignee, reporter, priority, type, title, description, due_date, effort, tags, custom_fields)
- `add/remove`: currently only tags/labels
- `run`: execute a command when the action fires
- `comment`: add a comment to the task

### Assignee tokens for actions

- `@assignee`, `@reporter`, `@assignee_or_reporter`, `@me`

### Running commands

Use `run` to execute a command when a rule action fires. Commands run on the server host.

```yaml
on:
  complete:
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
  complete:
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
| `previous.*` | Same as `ticket.*` but for the pre-change state (available on `start`, `sprint_changed`) |
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

## Cooldown

Actions that resolve to the same event can trigger further automation. To prevent infinite loops, rules enforce a configurable cooldown window and a maximum iteration count (`max_iterations`, default 5). If a rule has already fired for the same task within the cooldown period, subsequent matches are skipped.

## Simulating rules

Use `lotar automation simulate` to test what actions a rule change would produce without applying them:

```bash
lotar automation simulate --task PROJ-1
```

## Validation

LoTaR validates status/type/priority values against the project config. Invalid values are skipped at runtime and reported as warnings when saving the automation file.
