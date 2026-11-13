# Task Model

Canonical fields, enums, and invariants for tasks returned by REST, MCP, and CLI renderers. Sources of truth live in `crates/lotar-lib/src/api_types.rs` (DTO definitions), `crates/lotar-lib/src/types.rs` (strongly-typed enums/records), and the [OpenAPI spec](../openapi.json).

## Identity and IDs

- Tasks are stored under `.tasks/<PROJECT>/TASK.yml`. The project prefix is derived from the project name unless overridden (see `generate_project_prefix` in `crates/lotar-lib/src/project.rs`).
- IDs always follow `PREFIX-<NUMBER>` (e.g. `AUTH-42`). CLI commands accept the numeric portion when the active project is unambiguous (`lotar status 42`), otherwise pass the fully-qualified ID or `--project`.
- Content updates retain `created` and bump `modified` to the wall-clock time recorded by `TaskService`. Soft deletion and restoration preserve `modified` and append timestamped history only when the lifecycle changes.
- **ID grammar (canonical parse):** the numeric suffix is the FINAL dash-separated segment, so prefixes may themselves contain dashes, digits, underscores, or Unicode letters (`ABC-OPS-12` -> project `ABC-OPS`, task `12`; `dev-ops-4` -> project `dev-ops`). Prefix grammar = `is_valid_project_prefix` in `crates/lotar-lib/src/storage/safety.rs`; the parser lives in `crates/lotar-lib/src/storage/identity.rs` (`TaskId::parse`).
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
`comments` | `TaskComment[]` | Each comment carries `{ date, author?, text }`.
`references` | `ReferenceEntry[]` | Code locations (`code`), external URLs (`link`), repository files (`file`), managed attachments (`attachment`), or platform references (`jira`, `github`).
`sprints` | `u32[]` | Numeric sprint IDs the task belongs to.
`sprint_order` | `BTreeMap<u32, u32>` | Optional sprint-ID to task-position map projected from ordered membership records.
`history` | `TaskChangeLogEntry[]` | Chronological change log entries (field deltas, actor, timestamp).
`custom_fields` | `CustomFields` | Map of configured custom-field keys → YAML/JSON values. Skipped when empty.

### Relationships & related structs

- `TaskRelationships` exposes dedicated arrays for `depends_on`, `blocks`, `related`, `children`, `fixes`, plus single-value `parent` and `duplicate_of`. All properties are optional; empty collections are dropped on serialization.
- `TaskComment` holds `{ date: RFC3339, author?: string, text: string }`. `author` is the identity the history records as actor for the `comment_added` entry; comments written before DEV-327 have none.
- `ReferenceEntry` supports `code` (e.g., `crates/lotar-lib/src/lib.rs#120`, stored canonically as `path#line` or `path#start-end`), `link` (URL), `file` (repository-relative path; store paths rejected), `attachment` (managed attachments-store blob name), and `jira`/`github` platform references. The two file-ish kinds are distinct: managed blobs live only under `attachment`, repository files only under `file`.
- `TaskChangeLogEntry` captures `{ at, actor?, changes[] }`, where each `TaskChange` includes `field`, `old`, and `new` values for audit review.

Relationship storage remains in each task's existing YAML. The shared relationship
service interprets `A.depends_on: [B]` and `B.blocks: [A]` as one logical dependency
edge from prerequisite B to dependent A, retaining both stored origins. Likewise,
`parent`/`children` form one hierarchy edge and `related` is symmetric. `fixes` and
`duplicate_of` remain directional; their incoming links are projections, not new
stored fields. Hierarchy does not imply blocking and transitive dependencies are
not materialized as additional links.

Supported relationship mutations validate fresh state and stage all affected
assertions in a coordinated transaction. Removing a logical relationship removes
both assertions; changing its category is not two independent saves. Graph reads
coordinate with those writers, because multi-file rollback alone does not provide
simultaneous visibility to readers. Noncooperating external file edits remain
outside that guarantee. Existing inconsistent data is reported, not repaired by
reads or unrelated metadata edits. Repair previews distinguish safe duplicate or
reciprocal normalization from decisions such as target selection, cycle breaking,
or conflicting parents. An additive mirror repair can restore an intentionally
manually deleted side, so it always requires explicit review/application.

Read coordination can initialize ignored advisory lock metadata and its
coordinator directory inside existing tasks roots. It never initializes an
absent tasks root or changes task/configuration/history/journal data. Read
snapshots share advisory locks and exclude exclusive writers. Transient
contention is retried for a 500 ms wait budget, releasing every partial lock set
before rediscovering roots/projects. External context acquired while holding a
mutation's local locks stays fail-fast to avoid circular waits. Unsupported or
persistent lock failures and pending transaction recovery remain explicit
unavailable results, not successful empty inventories.

### Custom fields

`custom_fields` is a free-form map backed by `HashMap<String, serde_yaml::Value>` (or JSON when schema generation is enabled). Keys correspond to names declared in `config.custom_fields` or dynamic fields permitted by `custom_fields.values`. CLI and REST mutate them via `field:<name>` syntax, while listing/filtering converts them back into strings using `custom_value_to_string`.

### Sprints & ordering

Canonical sprint records under `.tasks/@sprints/<id>.yml` hold ordered task
memberships. `sprints` and `sprint_order` are task DTO projections of those
records; the latter maps sprint IDs to each task's position. Legacy task YAML
memberships remain a compatibility input, not a backlog ordering mechanism.

Backlog order is a separate ordered task-ID list in
`.tasks/@sprints/backlog.yml`, with user extensions preserved. It is not a fake
sprint or a typed field on task YAML. Stored active backlog IDs are projected
in sequence, followed by unlisted active IDs in canonical lexical order;
inactive IDs may remain stored to preserve their place on return. Filters and
pagination project this global sequence rather than replacing it.

Relative `before`/`after` moves reload metadata under the coordinated sprint
lock and update only that collection's file. Existing sprint moves reorder
its membership vector without adding/removing members. Neither form rewrites
task files, task history, or task timestamps, and no-op requests preserve
metadata bytes and timestamps. General task-list sorting is unchanged.

## Enumerations & config-driven values

- `TaskStatus`, `Priority`, and `TaskType` are thin wrappers around strings (see `crates/lotar-lib/src/types.rs`). They validate against the arrays declared in `config.yml` under `issue_states`, `issue_priorities`, and `issue_types`. The examples below are defaults; workspaces routinely customize them.
	- `issue_states`: e.g., `TODO`, `IN_PROGRESS`, `VERIFY`, `BLOCKED`, `DONE`, `CANCELED`.
	- `issue_priorities`: e.g., `Low`, `Medium`, `High`, `Critical`, `Blocker`.
	- `issue_types`: e.g., `feature`, `bug`, `epic`, `spike`, `chore`.
- Because these values are data-driven, client code should not assume a fixed enum list; always render the exact casing stored on the task.

## Create/update payloads

- `TaskCreate` accepts `title`, optional `project`, and optional metadata for `priority`, `task_type`, `reporter`, `assignee`, `due_date`, `effort`, `description`, `tags`, `relationships`, `custom_fields`, and `sprints`. Missing properties are defaulted downstream.
- `TaskUpdate` treats every field as optional and only patches values that are present in the JSON body. Passing `tags` replaces the entire list; omitting it leaves tags untouched.
- Task creation/update does not set collection order. `TaskOrderRequest` supplies `task`, optional `sprint` (omitted/null means backlog), exactly one of `before`/`after`, and optional `allow_closed`. The response includes the full current `tasks` order and `changed`; caller-visible page subsets are never an authoritative replacement list.
- Both structs live in `crates/lotar-lib/src/api_types.rs` and share the same schema in `docs/openapi.json`.

## Invariants & best practices

- `created <= modified` (enforced by `TaskService`).
- Default reads and queries hide soft-deleted tasks. Explicit deleted/all listings and `include_deleted` lookups inspect them; ordinary mutations require restoration. Hard deletion and low-level rollback still physically remove task files; soft deletion is a separate in-place marker.
- Soft deletion retains all content, sprint memberships, and attachment reachability. Hard deletion removes the task file and its sprint memberships (DEV-270) and returns warnings listing retained managed attachments, detectable incoming relationships, the sprints it left, and ID reuse when it was the highest number. Restore clears `deleted_at`, validates against current project configuration, and preserves the ID and content.
- ID allocation remains highest stored numeric filename plus one, taken over every related tasks root: this root, its sibling roots, and the parent workspace that lists it as a sibling (`StorageOperations::get_current_id`), so nested workspaces sharing a prefix do not create one ID twice. Tombstones reserve IDs; physically removing the highest file permits reuse (a deliberate choice: there is no stored high-water mark). Missing IDs are tolerated, but reuse can bind older relationships, automation history, or worktree associations to a new task. Sprint numbers work the same way. Duplicated numbers are resolved with `lotar task renumber` (`TaskService::renumber_duplicate`).
- Comments and reference mutations run through a locked read-modify-write primitive (`Storage::mutate_task`): the task file is re-parsed under the project task lock, the mutation applies to that fresh state (concurrent writes to other fields cannot be lost), and unchanged operations skip the write entirely. Pending DEV-55 transaction journals still fail these writes closed.
- Change-log vocabulary: `comment_added` (comment append), `comment#N` (comment edit, old/new text), and `reference_added`/`reference_removed` (one entry per changed reference operation, old/new as `kind:value` displays). Changed reference operations bump `modified` once and dispatch post-commit automation (`updated` event, empty change set) plus one `task_updated` SSE event per server surface — strictly after any attachments-store lock drops. Automation comments are recorded under the `automation` actor and never re-dispatch `on.commented`.
- `tags`, `comments`, `references`, `history`, and `relationships.*` are always arrays even when empty, simplifying client iteration.
- Explicit `assignee` values persist across status transitions; automation must clear them deliberately if needed.
- When exporting/importing YAML directly, keep field names lower_snake_case to match the DTOs. Unknown keys are preserved (top level, and inside relationships, comments, references, history, sprint `plan` and sprint task entries through their `extra` maps) but ignored by CLI readers. Over REST and MCP these nested extras appear as additional JSON keys on the same objects.

## Write locking and transactions

The user help ([../help/serve.md](../help/serve.md#data-safety)) keeps only a short summary. The details:

- Cross-process write safety: task and sprint writes take an exclusive advisory lock (`.task.lock` / `.sprints.lock` dot-files inside the affected directory) and are written atomically via temp-file + rename. Locks are descriptor-based, so they release automatically if a process crashes; leftover zero-byte lock files are inert and must not be deleted as a cleanup technique. An interrupted atomic write can leave a `.<name>.tmp-*` dot-file behind; it is never swept automatically, and its name, age, PID, or a free task lock does not prove abandonment because config/source writers may use other coordination. There is no temp-cleanup command today. Preserve uncertain files and recovery journals; the [ownership-safe maintenance design](#atomic-temp-maintenance-design-dev-84) below is a future protocol, not an available deletion operation.
- Transactional task/sprint mutations: task creation, task updates with sprint changes, sprint assignment/removal, and missing-sprint cleanup stage all of their file writes (task files, sprint files, auto-populated project config) and publish them under the coordinated locks (sprints lock first, then per-project task locks; the lock set also covers every project referenced by a pending journal, so recovery itself never touches unlocked files). A validation or write failure — including a failed directory sync or a failed journal unlink after a fully published batch — rolls every affected file back to its exact previous bytes before the error is returned, so an error means unchanged files and retries cannot orphan tasks or duplicate memberships. The single exception is fail-closed by design: if the automatic rollback itself cannot complete, the journal is retained, participating mutations refuse to proceed, and affected files may remain mixed until the next participating mutation finishes the rollback or the files are reconciled manually; retries stay duplicate-safe either way. A `.txn-pending.json` journal at the tasks root lets the next such mutation recover a crashed publish the same way; if the journal is corrupt or a file was changed by someone else in the meantime, participating mutations fail closed and ask for manual reconciliation instead of overwriting. While a journal is pending, other lock-taking writers (task comments, direct sprint edits, scan-driven creation) refuse to run with a pointer to the journal rather than risk changes a rollback would clobber; lock-less config edits remain outside this guard.
- Acknowledgement semantics: a committed transaction is acknowledged once its journal unlink succeeds — if that unlink cannot complete, the published writes are rolled back immediately under the still-held locks and the caller sees an error (a retained journal then matches the already-restored state, and recovery merely re-verifies and retries its removal), so a retry re-applies instead of duplicating; an error is never returned together with a removed journal.
- Limits: recovery runs on the next participating mutation rather than at startup, readers may observe intermediate states (atomic per file, never torn), plain config edits made outside these commands are not lock-coordinated, rollback restores files but not directories or lock files, and NO power-loss durability is claimed on any platform: the Unix fsyncs are best-effort hardening, not a proven durable-commit protocol, and after an acknowledged commit a power failure can in principle resurrect the unlinked journal entry, which recovery would then roll back.

## Atomic Temp Maintenance Design (DEV-84)

**Status: design complete, not implemented.** No cleanup command, ownership markers,
or automatic sweep is shipped by this decision. Implementation is tracked in
[DEV-110](../../.tasks/DEV/110.yml); enabling deletion requires a separate reviewed
change. Ordinary writes and existing task YAML remain unchanged.

### Current Behavior and Scope

[`atomic_write_file`](../../crates/lotar-lib/src/storage/safety.rs) reserves a sibling
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

The current production call sites are in [storage operations](../../crates/lotar-lib/src/storage/operations.rs),
[sprint service](../../crates/lotar-lib/src/services/sprint_service.rs), [config operations](../../crates/lotar-lib/src/config/operations.rs),
[init scaffolds](../../crates/lotar-lib/src/cli/handlers/config/init/scaffolds.rs),
[sync service](../../crates/lotar-lib/src/services/sync_service/mod.rs), and [source preparation](../../crates/lotar-lib/src/utils/scan.rs).
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
component checks from [transactions](../../crates/lotar-lib/src/storage/transaction.rs) and current
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
`crates/lotar-lib/src/storage/safety.rs` prove why lock-time sweeping must not return.
`corrupt_journal_fails_closed_and_is_kept` and the recovery lock/path tests in
`crates/lotar-lib/src/storage/transaction.rs` remain baseline guards. They do **not** test the
proposed capsule protocol. The implementation must keep those guards and pass
new native Linux/Windows cases plus the ordinary lint/test/smoke gates before
enabling apply. No destructive implementation is authorized by this design alone.

See also: [OpenAPI spec](../openapi.json) for the full REST contract and [Identity & Users](./identity.md) for `reporter`/`assignee` resolution rules.
