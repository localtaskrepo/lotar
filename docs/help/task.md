# lotar task

Legacy multi-action entry point that wraps the modern task commands for backward compatibility.


## Invocation

```bash
lotar task <ACTION> [OPTIONS]
lotar tasks <ACTION> [OPTIONS]   # alias
```

- All global flags (`--project/-p`, `--tasks-dir`, `--format`, `--log-level`, `--verbose`) behave exactly as they do for the top-level commands because this mode reuses the same workspace/project detection logic.
- Running `lotar task` (or `lotar tasks`) with no action prints the available subcommands via `emit_subcommand_overview`.
- Numeric task IDs are expanded to full IDs using the same project detection chain described in `docs/help/precedence.md`.

## Actions at a glance

| Action | Purpose | Shares handler with |
|--------|---------|---------------------|
| `add` | Create a task with the core fields. | `lotar add`. |
| `list` | Search/filter tasks with pagination helpers. | `lotar list`. |
| `show` (alias `view`) | Print one task with all its details. | `lotar show`. |
| `edit` (alias `update`) | Modify an existing task, optionally previewing changes. | `lotar edit` / `lotar update`; same pipeline as `lotar add`. |
| `status` | Change a task status (write-only in legacy mode). | `lotar status`. |
| `priority` | Show or update priority. | `lotar priority`. |
| `assignee` | Show or update assignee. | `lotar assignee`. |
| `due-date` | Show or update due date. | `lotar due-date`. |
| `relationships` | List, check, repair, graph, or edit relationship categories. | Shared relationship service. |
| `effort` | Show, set, or clear effort estimates. | `lotar effort`. |
| `delete` | Soft-delete a task, or permanently remove it with `--hard`. | Delete helper. |
| `restore` | Restore a soft-deleted task. | Restore helper. |
| `archive` / `unarchive` | Move tasks into the project's `@archive` folder (read-only) or back. | Archive helper. |
| `move` | Move tasks to another project; old IDs keep working. | Move helper. |
| `history` / `history-field` | Git history utilities. | See `docs/help/history.md`. |
| `diff` | Show the latest (or specific) git diff for a task. | History/diff helper. |
| `at` | Print the file at a specific commit. | History helper. |
| `comment` | Add or list comments for a task. | `lotar comment`. |
| `reference` | Add or remove links, repository files, code ranges and attachments. | Reference and attachment services. |

Each action below highlights the legacy-specific nuances; for behavior details see the dedicated help pages linked in the rightmost column.

## Action details

### add

```bash
lotar task add --title "Task title" [--type|-t] [--priority|-P] [--reporter|-R] [--assignee|-a] [--due|-d] [--effort|-E] [--description|-D] [--tag|-i ...] [--field|-F key=value ...]
```

- Uses the same pipeline as `lotar add`, so status/priority/type defaults, auto-tags, branch inference, and validation all match the dedicated command.
- Legacy mode only exposes the core fields. Flags such as `--dry-run` and `--explain` live exclusively on the top-level `lotar add` command.

### list

```bash
lotar task list [QUERY...] [--assignee|-a <VALUE>] [--status|-s <VALUE>...] [--priority|-P <VALUE>...] [--type|-t <VALUE>...] [--tag|-i <VALUE>...] [--sprint ID...] [--due BUCKET] [--due-within DAYS] [--recent 7d] [--needs FIELDS] [--has KINDS] [--effort-min VALUE] [--effort-max VALUE] [--sort-by|-S FIELD] [--reverse|-R] [--page-size|-L N] [--offset N | --page N] [--deleted | --include-deleted] [--details] [--sum FIELD...]
```

- Same as `lotar list`: `QUERY` uses the web search syntax (`status:todo mine:true has:tag field:area=billing`), see `docs/help/list.md`; the query is the one REST and MCP run.
- Sorting accepts `priority|due-date|created|modified|status|assignee|type|project|id` and defaults to 20 results per page.
- `--details` adds task descriptions to the text output.
- Use `--page-size` (alias: `--limit`) with `--offset` or `--page` to paginate.

### show

```bash
lotar show <TASK_ID>            # also: lotar view, lotar task show
lotar --format json show <TASK_ID>
```

- Prints the task's fields, relationships, description, acceptance criteria, references and comments. Empty sections are left out.
- Accepts the same IDs as other commands: `DEV-12`, `12` inside a project, an old ID of a moved task, or a soft-deleted task.
- `--format json` prints the full task as the REST API returns it.

### edit

```bash
lotar task edit <TASK_ID> [--title|-T] [--type|-t] [--priority|-P] [--reporter|-R] [--assignee|-a] [--effort|-E] [--due|-d] [--description|-D] [--tag|-i ...] [--field|-F key=value ...] [--dry-run|-n]
```

- Also available as `lotar edit <TASK_ID> …` and `lotar update <TASK_ID> …`.
- Validates and normalizes edits in the task's project context, including `@me` handling, before updating through the shared task service.
- When `auto.populate_members` is enabled, new reporter/assignee members and the task edit are saved in one coordinated transaction. Validation or write failures do not leave member configuration changes behind; a pending transaction that cannot be safely recovered blocks the edit.
- `--dry-run` works with both text and JSON output (`--format=json` emits the preview envelope described in `docs/help/effort.md`).
- Dry runs validate membership against an in-memory member list without writing task or configuration files.
- Backlog and sprint task order is stored in collection metadata, not task fields. Use row drag-and-drop or `lotar sprint order`; see [manual backlog ordering](./sprints.md#manual-backlog-ordering).
- Review-state status/assignee changes require the reporter or an explicit project reviewer from `review.reviewers`; empty lists keep reporter-only behavior. Reporter metadata is preserved and history records the acting identity. See [review resolution](./status.md#review-resolution).

### status

```bash
lotar task status <TASK_ID> <NEW_STATUS>
```

- This alias always supplies a new status and therefore writes immediately; use `lotar status` if you need to inspect the current status or run `--dry-run/--explain`.
- See `docs/help/status.md` for validation, auto-assign semantics, and output formats.

### priority

```bash
lotar task priority <TASK_ID> [NEW_PRIORITY]
```

- Without `NEW_PRIORITY` it prints the current value; otherwise it reuses `PriorityHandler` to validate and update the task.
- Refer to `docs/help/priority.md` for supported values and renderer output.

### assignee

```bash
lotar task assignee <TASK_ID> [NEW_ASSIGNEE]
```

- Accepts raw emails, usernames, or the `@me` token (resolved via the identity chain in `docs/help/precedence.md`).
- Legacy mode does not expose `--dry-run` or `--explain`; use `lotar assignee` if you need a preview.

### due-date

```bash
lotar task due-date <TASK_ID> [NEW_DATE]
```

- `NEW_DATE` accepts absolute (`YYYY-MM-DD`) or relative values (`tomorrow`, `+3d`, etc.) as described in `docs/help/due-date.md`.
- Preview/explain flags are only available via the top-level `lotar due-date` command.

### relationships

```bash
lotar task relationships <TASK_ID> [--kind <KIND> ...]
```

- `--kind` accepts any combination of `depends-on`, `blocks`, `related`, `parent`, `children`, `fixes`, `duplicate-of` (case-insensitive) and filters the output accordingly.
- Loads relationships from storage and renders them in text or JSON form.

#### Check and repair

```bash
lotar --project APP task relationships check
lotar --project APP --format json task relationships repair --dry-run
lotar --project APP --format json task relationships repair --dry-run --issue-id "<diagnostic ID>"
lotar --project APP task relationships repair --apply --expected-revision "<repair-preview revision>"
```

`check` is read-only and exits nonzero for blocking findings or incomplete data.
Warnings remain visible but do not alone fail the check. Inspection and repair
preview do not initialize missing task directories or recover pending journals.
Application is explicit and requires the **repair preview's** revision, bound to
its project, selected issue IDs and inventory; graph/check revisions are not
interchangeable tokens. Repeat the same `--issue-id` options when applying a
selected preview. Alias-only repairs preserve duplicate multiplicity, duplicate
repairs preserve the retained spelling, and missing-mirror repairs do not normalize
other entries in that field. Batch repair may perform combined normalization.
If inputs changed, obtain and review a fresh preview.

Coordinated inspection may initialize ignored advisory lock files and the
coordinator directory inside existing tasks roots, including external context.
It never creates an absent tasks root or changes task, configuration, history, or
transaction-journal data. Concurrent inspection requests share read locks.
Transient writer contention is retried with a bounded 500 ms wait budget; the
entire partial lock set is released before retrying. Persistent contention,
unsupported/unwritable synchronization or pending recovery remains an explicit
unavailable result rather than a clean check.

Safe repairs deduplicate proven identities and add unambiguous reciprocal
assertions on active tasks. Review the preview: restoring a mirror may undo an
intentional manual deletion of just one side. Repair does not guess missing IDs,
create placeholder tasks, delete edges, break cycles, choose parents, restore
deleted tasks, change statuses, or request agent jobs. Unreadable or ambiguous
data is not treated as an absent dependency. See [Task Model](./task-model.md).

#### Edit categories

```bash
lotar task relationships add APP-42 APP-9 --kind depends-on
lotar task relationships move APP-42 APP-9 --from depends-on --to blocks
lotar task relationships remove APP-42 APP-9 --kind blocks
lotar task relationships add APP-42 APP-8 --kind parent --expected-current APP-7
```

Kinds are relative to the first, focused task. Adding Depends on maintains the
target's Blocks assertion; changing Depends on to Blocks reverses dependency
direction in one operation. Removing a relationship removes its reciprocal too.
Set `auto.reciprocal_relationships: false` (globally or per project) to write
only the focused task's side; removal still clears both sides.

Mentioning another ticket's ID in a description or comment (`see APP-9`) adds a
Related link to it, unless the two are already linked in any way. Only newly
written mentions count, so removing such a link by hand sticks, and links are
never removed when the text changes. Turn this off with `auto.link_mentions: false`.
Other categories between the same pair remain intact. Parent/Duplicate of
replacement requires the expected current target, preventing silent replacement
after another edit. Cross-project edits within the same tasks root are supported;
cross-workspace writes are refused.

The task panel uses compact category rows. Use each row's plus button to add a
link, drag a chip between categories to change its kind, and use its x button to
remove it. Direction reversals and occupied singular destinations require explicit
confirmation. The dependency preview stays collapsed until requested. Relationship
changes arrive through live backend updates without a manual reload or loss of
the current panel scroll/draft. Following a task link preserves browser history:
Back returns to the previous task, including when opened over another view.

#### Explore dependencies

```bash
lotar --project APP --format json task relationships graph --root APP-42 --depth 2 --max-nodes 100
```

`--filter '<query>'` keeps only tasks matching a query in the [web search syntax](./list.md#filters-in-the-query), the same as the Dependencies page filter, e.g. `--filter 'status:todo tag:ui'`. Referenced tasks outside the filter stay as context.

The graph coalesces reciprocal declarations into one typed edge with stored
provenance. Dependency arrows run from prerequisite to dependent. Graph depth is
bounded to 0-8 and node count to 1-500; partial/truncated output is explicit and
the CLI exits nonzero for incomplete graph data. Parenthood does not itself imply
a dependency, and transitive dependencies are not written as extra direct links.

In the UI, expand the Relationships tab's dependency preview or open the
Dependencies navigation tab. Its visibility is controlled in Preferences, like
other navigation tabs; hiding it does not block direct links. The compact task
view and full page share an accessible task/edge list and optional diagram. The
text list is expanded by default without the diagram and collapsible alongside it.
Independent relationship choices expose hierarchy, related, fixes, and duplicate links; unresolved
nodes and cycles stay visible. Activating a task card or connection endpoint on
the full page drills into that task as the graph root and opens its details in
one browser-history entry, preserving project, relationship choices and other
filters. Back/Forward restores the previous graph root and corresponding details;
closing details keeps the current root. Inline task-panel previews retain their
ordinary task-navigation behavior. The full page's **Fix relationships** button opens a safe-repair preview;
review its project/workspace scope and confirm before any task data changes.
Repairable problem cards also have **Fix** actions. These preview only the chosen
problem, not every safe fix in the project; the selected issue IDs are bound to
the preview revision and must match when applying. Unsafe problems require manual
review instead of choosing a cycle edge or parent automatically.

The full page uses one shared search for text, structured filters, and task
lookup by ID or title. Choose a task suggestion or press Enter on an exact ID to
explore one task and its connections; ordinary text searches more broadly.
Typing an exact full ID or a unique numeric ID highlights its visible match
without requiring a suggestion click, navigation, or dismissal of completions.
Ambiguous numeric matches are not chosen arbitrarily. Shared task suggestions
rank exact canonical IDs and numeric suffixes ahead of prefixes and textual
matches before applying the result limit, with deterministic natural-ID ties.
Type-filter suggestions also work when editing a token in the middle of search.
Clearing search removes the selected task scope while retaining other conditions.
**Fields** controls the details rendered directly on diagram and list cards,
with choices saved per project. Cards use the compact Board header, badges, and
wrapping metadata rather than a hover-card definition list. There is no separate
graph hover-only layer. Saved search and conditions restore both input and
matching results when returning to the view; explicit route values take priority.

The standard Filters panel includes **Relationship types** checkboxes and Depth
through its view-options extension. Any combination, including no types, is
valid; dependencies are not forced on. **All** selects or clears the whole group
and indicates partial selections. Only the selected connections are followed to
show context. Diagram connections use differentiated strokes, arrow styles, and
the matching legend without per-arrow text clutter. Accessible SVG titles and
the styled Connections list identify their types. Color is supplementary, not
the only way to identify a link. Parallel relationship types remain separate.
The diagram grows with the viewport, and the task list uses bounded compact
cards rather than full-width rows. Diagram frames and connection rows fit their
content instead of reserving an empty right gutter; wider graphs remain scrollable.
Many-stage graphs scroll inside the plot without widening the page. Tasks without
connections in the selected relationship types are hidden by default; enable
the **Show unlinked tasks** toggle inside Filters to include them. It is aligned
with Connection range and uses the same pressed-state button styling as other view controls. Explicitly
focused or exact-ID matched tasks and unresolved diagnostic context remain visible.
During a root reload, the previous view retains its geometry but is clearly
inactive until the requested snapshot arrives. The plot keeps its viewport height
and page position; inner scroll clamps naturally when the new graph is smaller,
so the newly focused card stays reachable without blank retained canvas extents.
View changes refresh only the affected data/components, not the search or page.
Status, priority, type, assignee, tag, sprint, date, and custom filters are applied
by the backend before the node limit; connected tasks outside the filters remain
labeled as context. Sorting determines bounded selection and order within each
dependency stage, without reversing prerequisite direction. The diagram uses
dependency stages rather than a fixed grid and groups cycles explicitly. With no
visible connections, the task list is the default view.

**Connection range** counts shortest hops from the selected task, or from each
matching task when no single task is selected. Stages are ordering columns, not
hop distances. For `A -> B -> C -> D`, focusing C with two hops includes A, B, C,
and D, producing four stages although every task is at most two links from C.
When unlinked tasks are hidden, relationship-kind and connectivity selection
occurs before the node budget so recent standalone tasks cannot crowd out the
connected graph. Partial projections still report their explicit node limit.
An accepted empty result shows the empty state rather than a retained blank plot.
The previous plot may remain inactive while loading, but not after an empty result
has been confirmed.

**Tasks in this graph** lists the returned tasks; **Connections** explains their
visible links in a responsive multi-column grid. **Problems shown in this graph**
uses severity-marked cards with readable titles, affected-task links and
safe-fix/manual-review guidance, and reports only what is detected in
that bounded view. **Validate project/workspace** checks the complete selected
scope, regardless of graph focus, depth, relationship choices, or filters. The URL
uses `relationships` for the comma-separated connection kinds (`none` for an
empty selection) and `type` for task-type filters. Older `layers` and layer-only
`type` links remain readable. Search synchronization between views is a separate
feature tracked in DEV-142; it is not enabled by this dependency-view change.

Unified configuration/dependency/scanner verification and read-only pre-commit
integration are tracked in DEV-138. `scan --dry-run` is a preview, not yet a
fail-on-findings validation command; ordinary `scan` writes. Do not substitute
the existing mutating formatting hook for read-only verification.

### effort

```bash
lotar task effort <TASK_ID> [NEW_EFFORT] [--clear] [--dry-run] [--explain]
```

- Wraps the primary `lotar effort` command, so it supports previews, explanations, and clearing effort values. See `docs/help/effort.md` for accepted formats (`2d`, `5h`, point counts, etc.).

### delete

```bash
lotar task delete <TASK_ID> [--hard] [--dry-run] [--force | --yes | -y]
```

- Default deletion keeps the YAML file, sets `deleted_at`, and hides the task from normal views. It preserves the task's content, ID, attachment references, and sprint memberships.
- Soft delete and restore leave `modified` unchanged. Lifecycle operations append timestamped history entries; repeating an operation that is already applied does not change history.
- `--hard` permanently removes the task file. Attachment blobs are **not** deleted: warnings list retained attachments and detectable incoming task relationships. Related task files are not rewritten.
- `--force`, `--yes`, and `-y` only skip confirmation; they never select hard deletion. `--dry-run` previews the selected deletion mode without writing.
- Use `lotar list --deleted` to list deleted tasks, or `--include-deleted` to list active and deleted tasks together. These flags are mutually exclusive.
- Physical file removal remains supported. Missing IDs do not prevent other tasks from loading. ID allocation uses the highest numeric filename anywhere in the project folder (subfolders and `@archive` included) plus one: tombstones reserve IDs, while removing the highest file permits its number to be reused. Reuse can associate old relationships, Git history, and agent records with a different task; increment the filename manually when retaining that distinction matters.

### restore

```bash
lotar task restore <TASK_ID> [--dry-run]
```

- Clears `deleted_at` while keeping the same ID, content, and surviving sprint memberships. The task becomes visible normally again.
- `--dry-run` previews restoration without changing task data.
- Restore requires values compatible with the current project configuration; it does not invent a replacement workflow status.
- Hard-deleted tasks cannot be restored by this command because their file is gone. User-managed history or backups remain independent recovery options.

### archive / unarchive

```bash
lotar task archive [TASK_ID...] [--select '<query>'] [--dry-run|-n]
lotar task unarchive [TASK_ID...] [--select '<query>'] [--dry-run|-n]
```

- Archiving moves the task file into its project's `@archive` folder, keeping the path below the project folder (`DEV/web/12.yml` becomes `DEV/@archive/web/12.yml`); unarchiving moves it back. Nothing else in the file changes except an `archived`/`unarchived` history entry; status, links and `modified` stay as they are.
- Archived tasks are left out of lists, search, boards, task stats and tag suggestions unless a query asks for them with `archived=yes` (only archived) or `archived=all`. Looking a task up by ID still finds it, so links, the dependency graph and sprint history keep working, and a `Done` dependency stays done.
- Archived tasks are read-only everywhere (web UI, CLI, REST, MCP, automations). To change one anyway, add the global `--force-archived` flag (`lotar --force-archived comment DEV-12 -m "..."`); REST takes `?force_archived=true` and MCP `force_archived: true`.
- `--select` takes the same query as `lotar list` and adds every match: `lotar task archive --select 'status:done -recent:30d'` archives finished tasks nobody changed for 30 days. For `unarchive` the query looks inside the archive. Run it from cron or CI to archive on a schedule, or use the automation action `archive: true` to archive right when something happens.
- Soft-deleted tasks can be archived too. Unarchiving requires the stored status, priority and type to be valid under the current configuration, like `restore`.
- Tasks can also be grouped in any subfolder of their project folder by hand (`DEV/backend/12.yml` is still `DEV-12`). Folders starting with `.` or `@` are not scanned, except `@archive`. Moving a file with `git mv` archives it just like the command does. A number stored twice in one project (for example after a merge) is listed once with a warning, and changes to that ID are refused until one copy is removed or renumbered.
- LoTaR versions before 0.11 only read the top level of a project folder: they do not see nested or archived tasks and can hand out their numbers again. Upgrade every client before archiving or nesting tasks.

### move

```bash
lotar task move [TASK_ID...] --to <PROJECT> [--select '<query>'] [--status S] [--type T] [--priority P]
                [--reporter R] [--assignee A] [--field key=value ...] [--dry-run|-n]
lotar task move <OLD_ID> --record <NEW_ID>
```

- The task gets the next free number in the target project (`DEV-12` becomes `WEB-40`) and keeps its title, description, comments, references, attachments, tags, relationships, sprints and history. Its history gains a `moved` entry and `moved_from: [DEV-12]` lists its earlier IDs. The target project must exist.
- The old file stays behind as a small *stub*: a soft-deleted task with `moved_to: WEB-40`. It keeps the number taken and makes the old ID a redirect. Every command, the REST API, MCP and the web UI accept `DEV-12` and act on `WEB-40`: links in descriptions, URLs and bookmarks, relationships, sprint and backlog lists, commit messages, TODO comments in code and `lotar list DEV-12` all find the moved task. Nothing else is rewritten; when LoTaR saves a list anyway (a sprint reorder, a relationship change) it stores the current ID. Stubs are hidden from every list, the trash included, and cannot be edited or restored.
- Values the target project does not have are fitted to it: a status, type or priority it does not know becomes its default (the change is printed), unless you choose one with `--status`, `--type` or `--priority`. A custom field the target does not allow stops the move; set it with `--field name=value` or drop it with `--field name=`. `--reporter` and `--assignee` change people for projects with strict members (an empty value clears). `--dry-run` shows the new ID and every change without moving anything.
- A task moved back to a project it came from takes its old number again. Moves chain: after `DEV-12` -> `WEB-40` -> `OPS-3`, all three IDs reach the task.
- Refused: moving within the same project, deleted tasks (restore them first), tasks with an agent job running or queued, and archived tasks unless `--force-archived` is given (the task then lands in the target's `@archive`). Both files are written in one transaction, so a failed move leaves nothing half done. Attachment files are copied when the projects keep attachments in different folders, and the agent context file is renamed with the task.
- `lotar task history`, `history-field`, `diff` and the stats commands include the commits from before the move, made to the old file.
- Moved a file by hand (`git mv DEV/12.yml WEB/40.yml`)? Run `lotar task move DEV-12 --record WEB-40` to add the redirect and the `moved_from` entry afterwards.
- A sync pull of the old project keeps matching the remote issue to the stub (it carries the remote reference) and skips it, rather than importing it again; sync the moved task from its new project.
- LoTaR versions before 0.11 show the stub as a deleted task and do not follow the redirect.

### Git history helpers (`history`, `history-field`, `diff`, `at`)

- `lotar task history <TASK_ID> [--limit|-L N]` - shows the git commit log touching the task file.
- `lotar task history-field <FIELD> <TASK_ID> [--limit|-L N]` - alias for `history-by-field` that traces only `status`, `priority`, `assignee`, or `tags` changes.
- `lotar task diff <TASK_ID> [--commit <SHA>] [--fields]` - prints the latest raw patch (or a specific commit). `--fields` switches to the structured renderer that lists per-field changes.
- `lotar task at <TASK_ID> <COMMIT>` - dumps the task file snapshot at `git show COMMIT`. All behaviors match `docs/help/history.md`.

### comment

```bash
lotar task comment <TASK_ID> [TEXT] [--message|-m TEXT] [--file|-F PATH] [--dry-run|-n] [--explain|-e]
```

- Shares `CommentHandler` with `lotar comment`: TEXT can come from the positional argument, `--message`, `--file`, or stdin. Without comment content the command lists existing comments.
- See `docs/help/comment.md` for JSON payloads and formatting rules.

### reference

```bash
lotar task reference add link <TASK_ID> <URL>
lotar task reference add file <TASK_ID> <REPO_PATH>
lotar task reference add code <TASK_ID> <PATH#LINE[-LINE]>
lotar task reference add attachment <TASK_ID> <STORED_NAME>
lotar task reference add upload <TASK_ID> <LOCAL_FILE>
lotar task reference remove <link|file|code|attachment> <TASK_ID> <VALUE>
```

- `upload` copies a local file into the attachments store and attaches it, with the same checks as the web UI: `attachments.max_upload_mb`, photo metadata stripping, and one stored copy per identical file. It prints the stored name (`Stored as <name>`); link it from a description or comment as `![caption](attachment:<name>)`. The task is checked before anything is stored.
- `attachment` attaches a file already in the store by its stored name.
- The store is `attachments.dir` of the task's project (default `@attachments` in the tasks directory). A project can point it at a folder inside its own directory, such as `DEV/@attachments`, to keep its attachments in git when other projects are ignored.

## Tips and examples

```bash
# Quick legacy flows
lotar tasks list --project=backend mine:true due_within:5
lotar task edit AUTH-42 --priority=High --tag backend --tag api --dry-run --format=json
lotar task history AUTH-42 --limit=5
lotar tasks comment AUTH-42 -m "Ready for review" --dry-run
```

- Prefer the direct commands (`lotar add`, `lotar status`, etc.) when you need the newest flags or shorthands. `lotar task` remains supported for scripts that rely on the legacy shape.
- All actions continue to respect identity resolution, workspace detection, and configuration precedence exactly as described in `docs/help/precedence.md`.
