# Task Model

Canonical fields, enums, and invariants for tasks returned by REST, MCP, and CLI renderers. Sources of truth live in `src/api_types.rs` (DTO definitions), `src/types.rs` (strongly-typed enums/records), and the [OpenAPI spec](../openapi.json).

## Identity and IDs

- Tasks are stored under `.tasks/<PROJECT>/TASK.yml`. The project prefix is derived from the project name unless overridden (see `generate_project_prefix` in `src/project.rs`).
- IDs always follow `PREFIX-<NUMBER>` (e.g. `AUTH-42`). CLI commands accept the numeric portion when the active project is unambiguous (`lotar status 42`), otherwise pass the fully-qualified ID or `--project`.
- Content updates retain `created` and bump `modified` to the wall-clock time recorded by `TaskService`. Soft deletion and restoration preserve `modified` and append timestamped history only when the lifecycle changes.
- **ID grammar (canonical parse):** the numeric suffix is the FINAL dash-separated segment, so prefixes may themselves contain dashes, digits, underscores, or Unicode letters (`ABC-OPS-12` -> project `ABC-OPS`, task `12`; `dev-ops-4` -> project `dev-ops`). Prefix grammar = `is_valid_project_prefix` in `src/storage/safety.rs`; the parser lives in `src/storage/identity.rs` (`TaskId::parse`).
- **Exact case, no folding:** IDs and project prefixes match exactly (`abc` and `ABC` are different projects).
- **Padded aliases are deliberate:** `TP-001` and `TP-1` denote the same task file (`TP/1.yml`); reads surface the canonical unpadded spelling in DTO `id` fields. Malformed IDs (empty, missing numeric suffix, non-digit suffix such as `TP-+12` or `TP-1-extra`, path traversal, `u64` overflow) fail closed.
- **Multiple storage roots:** `StorageLocator::candidate_task_roots` (primary + sibling workspace `.tasks`) drives unfiltered search, project-filtered search, full-ID get, and numeric resolution consistently. Config, sprint memberships, and attachments for a nested-root task come from the root that actually holds it.
- **Attachment store concurrency:** all LoTaR surfaces that touch managed attachments (REST upload/remove and the MCP/CLI `attachment` reference kind) serialize blob creation, dedup, reference attach/detach, reclamation checks, and rollback cleanup on a cross-process advisory lock (`.attachments-store.lock` inside the store directory, fs2-backed, bounded contention retry that fails closed). Lock order is always store-lock -> task-lock. Cooperating LoTaR processes are covered; external non-cooperating writers that create or delete blobs in the store directory directly are explicitly unsupported and can produce orphans or dangling references. Repository `file` references can never resolve inside the store (store paths are rejected on add/remove), so they take no store lock.
- **Ambiguity fails closed:** duplicate full IDs or numbers across roots never resolve to an arbitrary task; resolvers report the candidate locations. Mutations refuse cross-root targets before any side effect (edit the task from inside its own workspace); `canonical_sync_task_id` keeps sync locked to the primary workspace via single-root reads. Listing endpoints enumerate every stored copy (duplicates appear as separate entries) rather than deduplicating an identity they cannot prove.

## TaskDTO fields

Field | Type | Notes
----- | ---- | -----
`id` | `string` | Canonical task identifier (`PROJECT-N`).
`title` | `string` | Required summary/title.
`status` | `TaskStatus` | Validated against `config.issue_states` (see below). Stored as user-provided casing.
`priority` | `Priority` | Validated against `config.issue_priorities`.
`task_type` | `TaskType` | Validated against `config.issue_types`.
`reporter` | `string?` | Optional; resolved via identity helpers if omitted during creation.
`assignee` | `string?` | Optional; never auto-cleared when statuses change.
`created` | `RFC3339 string` | UTC timestamp recorded at creation.
`modified` | `RFC3339 string` | Content-mutation timestamp; unchanged by soft delete and restore.
`deleted_at` | `RFC3339 string?` | In-place soft-deletion timestamp; omitted for active tasks.
`due_date` | `string?` | ISO8601 date/time or natural-language token parsed by CLI validators.
`effort` | `string?` | Stored exactly as provided (e.g., `3h`, `5pts`).
`subtitle` | `string?` | Short secondary label; omitted unless explicitly set.
`description` | `string?` | Markdown-friendly long description.
`tags` | `string[]` | Normalized, unique tags. Empty array when unset.
`relationships` | `TaskRelationships` | Structured references to other tasks (see below).
`comments` | `TaskComment[]` | Each comment carries `{ date, text }`.
`references` | `ReferenceEntry[]` | Code locations (`code`), external URLs (`link`), repository files (`file`), managed attachments (`attachment`), or platform references (`jira`, `github`).
`sprints` | `u32[]` | Numeric sprint IDs the task belongs to.
`sprint_order` | `BTreeMap<u32, u32>` | Optional manual ordering per sprint (task id → order index).
`history` | `TaskChangeLogEntry[]` | Chronological change log entries (field deltas, actor, timestamp).
`custom_fields` | `CustomFields` | Map of configured custom-field keys → YAML/JSON values. Skipped when empty.

### Relationships & related structs

- `TaskRelationships` exposes dedicated arrays for `depends_on`, `blocks`, `related`, `children`, `fixes`, plus single-value `parent` and `duplicate_of`. All properties are optional; empty collections are dropped on serialization.
- `TaskComment` holds `{ date: RFC3339, text: string }`. Comments do not store authorship today.
- `ReferenceEntry` supports `code` (e.g., `src/lib.rs#120`, stored canonically as `path#line` or `path#start-end`), `link` (URL), `file` (repository-relative path; store paths rejected), `attachment` (managed attachments-store blob name), and `jira`/`github` platform references. The two file-ish kinds are distinct: managed blobs live only under `attachment`, repository files only under `file`.
- `TaskChangeLogEntry` captures `{ at, actor?, changes[] }`, where each `TaskChange` includes `field`, `old`, and `new` values for audit review.

### Custom fields

`custom_fields` is a free-form map backed by `HashMap<String, serde_yaml::Value>` (or JSON when schema generation is enabled). Keys correspond to names declared in `config.custom_fields` or dynamic fields permitted by `custom_fields.values`. CLI and REST mutate them via `field:<name>` syntax, while listing/filtering converts them back into strings using `custom_value_to_string`.

### Sprints & ordering

`sprints` mirrors the sprint memberships stored on the task file. When sprint assignment commands enable manual ordering, `sprint_order` stores a per-sprint sequence so boards and reports can render deterministic lanes. Both properties are managed by `SprintService` and `sprint_assignment` helpers.

## Enumerations & config-driven values

- `TaskStatus`, `Priority`, and `TaskType` are thin wrappers around strings (see `src/types.rs`). They validate against the arrays declared in `config.yml` under `issue_states`, `issue_priorities`, and `issue_types`. The examples below are defaults; workspaces routinely customize them.
	- `issue_states`: e.g., `TODO`, `IN_PROGRESS`, `VERIFY`, `BLOCKED`, `DONE`, `CANCELED`.
	- `issue_priorities`: e.g., `Low`, `Medium`, `High`, `Critical`, `Blocker`.
	- `issue_types`: e.g., `feature`, `bug`, `epic`, `spike`, `chore`.
- Because these values are data-driven, client code should not assume a fixed enum list; always render the exact casing stored on the task.

## Create/update payloads

- `TaskCreate` accepts `title`, optional `project`, and optional metadata for `priority`, `task_type`, `reporter`, `assignee`, `due_date`, `effort`, `description`, `tags`, `relationships`, `custom_fields`, and `sprints`. Missing properties are defaulted downstream.
- `TaskUpdate` treats every field as optional and only patches values that are present in the JSON body. Passing `tags` replaces the entire list; omitting it leaves tags untouched.
- Both structs live in `src/api_types.rs` and share the same schema in `docs/openapi.json`.

## Invariants & best practices

- `created <= modified` (enforced by `TaskService`).
- Default reads and queries hide soft-deleted tasks. Explicit deleted/all listings and `include_deleted` lookups inspect them; ordinary mutations require restoration. Hard deletion and low-level rollback still physically remove task files; soft deletion is a separate in-place marker.
- Soft deletion retains all content, sprint memberships, and attachment reachability. Hard deletion removes only the task file and returns warnings listing retained managed attachments and detectable incoming relationships. Restore clears `deleted_at`, validates against current project configuration, and preserves the ID and content.
- ID allocation remains highest stored numeric filename plus one. Tombstones reserve IDs; physically removing the highest file permits reuse. Missing IDs are tolerated, but reuse can bind older relationships, automation history, or worktree associations to a new task.
- Comments and reference mutations run through a locked read-modify-write primitive (`Storage::mutate_task`): the task file is re-parsed under the project task lock, the mutation applies to that fresh state (concurrent writes to other fields cannot be lost), and unchanged operations skip the write entirely. Pending DEV-55 transaction journals still fail these writes closed.
- Change-log vocabulary: `comment_added` (comment append), `comment#N` (comment edit, old/new text), and `reference_added`/`reference_removed` (one entry per changed reference operation, old/new as `kind:value` displays). Changed reference operations bump `modified` once and dispatch post-commit automation (`updated` event, empty change set) plus one `task_updated` SSE event per server surface — strictly after any attachments-store lock drops. Automation comments are recorded under the `automation` actor and never re-dispatch `on.commented`.
- `tags`, `comments`, `references`, `history`, and `relationships.*` are always arrays even when empty, simplifying client iteration.
- Explicit `assignee` values persist across status transitions; automation must clear them deliberately if needed.
- When exporting/importing YAML directly, keep field names lower_snake_case to match the DTOs. Unknown keys are preserved by serde but ignored by CLI readers.

## Atomic Temp Maintenance Design (DEV-84)

**Status: design complete, not implemented.** No cleanup command, ownership markers,
or automatic sweep is shipped by this decision. Implementation is tracked in
[DEV-110](../../.tasks/DEV/110.yml); enabling deletion requires a separate reviewed
change. Ordinary writes and existing task YAML remain unchanged.

### Current Behavior and Scope

[`atomic_write_file`](../../src/storage/safety.rs) reserves a sibling
`.<name>.tmp-<pid>-<counter>` with exclusive creation, syncs it, and renames it over
the target. Preparation/publication errors attempt to remove only that write's
own temp. A crash can leave the temp behind. A filename, PID, age, hash, or free
task lock cannot prove that such a file is abandoned or was created by LoTaR.
**All existing-format leftovers stay report-only, even when their PID is dead.**

| Writer | Existing coordination | Maintenance boundary |
| --- | --- | --- |
| Tasks and sprints | Project task and sprint advisory locks | Selected tasks root and validated project/sprint parents only |
| Global/project config and init scaffolds | Atomic replacement; not consistently task-lock coordinated | Include only registered parents in the selected tasks root |
| Transaction and sync journals | Coordinated transaction locks or a separate sync lock | Recovery data and journal-purpose staging are always protected |
| Scanner source replacement | Source locks; outer `.lotar-scan-*` stage plus inner atomic temp | Separate explicit source-parent scope; never inferred from a recursive workspace scan |

The current production call sites are in [storage operations](../../src/storage/operations.rs),
[sprint service](../../src/services/sprint_service.rs), [config operations](../../src/config/operations.rs),
[init scaffolds](../../src/cli/handlers/config/init/scaffolds.rs),
[sync service](../../src/services/sync_service.rs), and [source preparation](../../src/utils/scan.rs).
These atomic helpers do not establish ownership of home artifacts, agent logs,
queue data, reports, attachments, worktrees, or arbitrary operating-system temp
directories. Those remain outside this operation and keep their existing owners.
Sibling task roots discovered by `StorageLocator` are not cleanup authorization.

### Proposed Writer Protocol

1. Pass an explicit ownership context into participating atomic writers: canonical
   selected tasks-root identity, approved destination-parent identity, and purpose
   (`task`, `sprint`, `config`, `source`, or protected `journal`). Never derive this
   authority from the current directory or a basename. Source parents require an
   additional explicit scope grant; outside-workspace paths remain excluded.
2. Use a versioned private staging namespace in each approved destination parent,
   partitioned by workspace identity so separate task roots cannot adopt each
   other's stages. Keeping the payload on the destination filesystem preserves
   rename semantics. Initialize control roots with exclusive creation and
   restrictive owner permissions/ACLs; never adopt a pre-existing lookalike root.
3. A stable workspace maintenance gate serializes registration, publication,
   collection, and capsule retirement. The gate's lock file is never removed.
   Every participant must open a capsule lease only while holding this gate;
   a collector's lease acquisition is non-blocking, never a wait under the gate.
4. Under the gate, reserve a fresh private per-write capsule and acquire its
   exclusive lease **before publishing a valid registration or creating payload
   contents**. Registration is two-phase: a provisional allocation record is not
   collectable; reserve the empty payload exclusively, capture its filesystem
   identity, then publish the complete registration under the gate before writing
   content. Register schema version, workspace/parent/capsule/lease identities,
   write nonce, purpose, root-relative destination, and payload file identity.
   PID/start-time may be diagnostic metadata, never the deletion authority.
   Record only metadata, never task/source contents or credentials.
5. Release the gate while preparing/syncing the payload, but retain the lease
   through callbacks, target/source locking, revalidation, rename, and failure
   cleanup. Apply destination permissions before writing sensitive contents.
   Do not recursively call the ownership-aware writer to write its own control
   records. Scanner preparation must carry one ownership lease over its whole
   outer-stage lifecycle, not stop when its inner atomic preparation finishes.
6. Reacquire the gate for publication/retirement while retaining the lease.
   Preserve all existing domain locks, expected-source checks, rollback, journal,
   and permission contracts. Release the lease only after the write no longer
   needs its staged data. Never reopen/adopt an abandoned capsule for a new write.

An interruption before the registration and payload identity are complete leaves
an uncertain capsule, not permission to infer ownership later. Initialization or
registration failures must not publish a target or make an unregistered file
collectable. Older writers keep generating non-collectable legacy stages; markers
must never be retrofitted onto those files from their names or contents.

### Proposed Maintenance Operation

The default operation is inspection. It opens only existing control data and
approved capsule directories: no initialization, lock-file creation/truncation,
reports persisted into the workspace, journal recovery, recursive generic scan,
or deletion. A transient non-blocking probe of existing gate/lease handles may
establish eligibility without writing files; inability to obtain the necessary
access is an uncertainty, not a reason to create or replace a lock.

For each candidate, report a relative path, purpose, size, ownership/lease proof,
and one of `protected`, `uncertain`, `eligible`, or `gone`, with a concrete reason.
There is no age threshold and no force option that bypasses proof. An inspection
result is not a reusable deletion permit.

An explicitly requested future apply operation must re-evaluate the complete
predicate, under the existing workspace gate, then hold an acquired capsule lease
through deletion:

- The caller selected this workspace and parent scope; root, parent, capsule,
  registration, lease, and payload identities still match the recorded objects.
- Control format/purpose is supported, permissions establish private ownership,
  and the complete capsule contains only its registered control files/payload.
  Missing/malformed markers, unexpected entries, hardlinks, symlinks, reparse
  points, traversal, permission errors, or changed identities are refusals.
- The lease is exclusively acquired. An active lease protects a writer even if
  it holds no task lock, has no visible heartbeat, or uses a reused PID. Any
  lock error other than positively supported acquisition stays report-only.
- No pending transaction/sync recovery exists for the workspace. Corrupt,
  inaccessible, or unfamiliar recovery state is protected, not discarded.
  Journal publication and this check must share the protocol's gate; no check
  followed by an unlocked deletion is a recovery fence. Unsupported writer or
  journal protocols cannot authorize collection based on that check.
- The platform backend binds inspection and deletion to validated directory/file
  handles. Rechecking a path and then calling generic `remove_file(path)` is not
  a TOCTOU proof. Cooperating participants may not replace capsule entries while
  the gate/lease are held. If the backend cannot enforce these invariants, apply
  is unavailable on that filesystem/platform; inspection remains available.

The collector never takes a task, sprint, sync, or source lock while holding the
maintenance gate. Existing writers can hold domain locks before taking the gate;
the collector must not introduce the reverse ordering or run recovery callbacks.
Gate contention is bounded and reported without mutation; busy leases are skipped.

Delete only registered staging payload/control entries, never the destination.
Retire a capsule while retaining the gate and lease. Collectors must close their
capsule handles before releasing the gate, and nobody may save an unclaimed lease
handle for a later attempt; writers alone retain their claimed lease between
registration and retirement. This prevents a collector from locking a removed
lease inode and acting on a replacement capsule. The stable workspace gate, ordinary
storage/source locks, and recovery journals are never cleanup candidates. Failure
partway through retirement reports the exact removed/remaining entries; uncertain
residue stays protected. Retrying a gone payload is idempotent, not a rollback or
power-loss durability guarantee.

This is a coordination/accidental-data-loss design, not a security boundary against
malicious code with the same account's access to private control data. POSIX
component checks from [transactions](../../src/storage/transaction.rs) and current
source inode revalidation are precedents, not proof of race-free collection.
Unix descriptor-relative operations and Windows handle identities/reparse refusal
must be implemented and tested natively. Unsupported/network filesystem locking
or ACL semantics cannot be assumed safe. PID probes alone do not solve this gap.

### Deterministic Acceptance Gates for Implementation

Use isolated fixtures, channels/release barriers, and real child exit/drain, not
sleep-based age assumptions. These are required future tests, not claims of an
implemented collector:

| Case | Required observation |
| --- | --- |
| Live task/config/sprint/source writer, including lockless config preparation | Pause after payload reservation; inspect/apply protects it; release writer and verify its normal publication succeeds |
| Writer after preparation but before rename, callback, or outer source-stage completion | Lease still blocks collection throughout each boundary |
| Child crashes after valid registration | After confirmed exit, owned payload becomes eligible; apply removes only its registered capsule; destination bytes/permissions remain unchanged |
| Crash before complete registration, malformed marker, or payload identity mismatch | Report uncertainty; retain every unproved entry |
| PID reused, missing heartbeat, denied process inspection | Decisions follow the lease/ownership proof, never PID guesses |
| Legacy/lookalike temp, unknown directory entry, foreign workspace/source parent | Report/protect; no adoption or deletion |
| Symlink/reparse/hardlink or candidate/parent replaced after inspection | Refuse under handle/identity validation; external/user file remains byte-for-byte unchanged |
| Pending/corrupt transaction or sync journal; journal publication racing apply | Protect recovery state; validate gate ordering with a deterministic barrier |
| Two collectors; writer/collector retirement race | One claimant, no stale-lease inode or namespace reuse, no deadlock |
| Permission/lock/deletion failure, unsupported filesystem | No fallback sweep; precise partial result and retained uncertain residue |
| Repeated inspection/apply and interrupted retirement | Inspection preserves bytes/mtimes/directory entries; gone payload is a no-op; no unrelated control files are removed |

Existing `lock_acquisition_preserves_live_atomic_writer_temp_and_rename` and
`held_lock_times_out_without_running_callback_or_removing_temp_files` in
`src/storage/safety.rs` prove why lock-time sweeping must not return.
`corrupt_journal_fails_closed_and_is_kept` and the recovery lock/path tests in
`src/storage/transaction.rs` remain baseline guards. They do **not** test the
proposed capsule protocol. The implementation must keep those guards and pass
new native Linux/Windows cases plus the ordinary lint/test/smoke gates before
enabling apply. No destructive implementation is authorized by this design alone.

See also: [OpenAPI spec](../openapi.json) for the full REST contract and [Identity & Users](./identity.md) for `reporter`/`assignee` resolution rules.
