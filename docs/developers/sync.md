# Manual Sync (Push/Pull)

This document defines the manual-only sync contract for Jira and GitHub integrations. The sync pipeline is invoked explicitly (CLI, UI button, or MCP tool). No polling or webhooks are used.

Sync integrations are still in beta. Use least-privilege credentials and verify the repositories or projects you target before running pull/push, since LoTaR can create and update issues.

## Goals

- Manual only: `lotar pull <remote>` or `lotar push <remote>`.
- Source-of-truth is directional:
  - Pull: remote is authoritative for mapped fields only.
  - Push: local is authoritative for mapped fields only.
- Identity is based solely on platform references (no title matching).
- No repo-stored secrets; auth lives in home config or env vars.

## CLI entrypoints

- `lotar pull jira-home`
- `lotar push jira-home`
- `lotar pull github-company`
- `lotar push github-company`
- `lotar sync check <remote>` (validate credentials, filters, and project/repo settings)

Flags (shared):
- `--project <PREFIX>` (limit to a specific project)
- `--auth-profile <name>` (override home config)
- `--dry-run` (show planned changes only)

## UI + MCP entrypoints

- UI: expose separate Push and Pull buttons per remote.
- MCP: separate tools for `sync_pull` and `sync_push` (no combined sync tool).

## Identity (platform references)

Platform references are the canonical link between a LoTaR task and a remote issue. Matching uses only these references; titles are never used.

- Jira reference format: `PROJ-123` (prefix `jira:` accepted and normalized away)
- GitHub reference format: `owner/repo#123` (prefix `github:` accepted and normalized away)

References are stored as typed reference entries on the task (see `ReferenceEntry`), not as ad-hoc custom fields.

Behavior:
- Pull: if a remote issue has no matching local reference, create a new local task and attach the platform reference.
- Push: if a local task has no platform reference, create a remote issue and attach the platform reference.
- If the reference is removed locally after a completed sync, a subsequent push creates a new remote issue (by definition, the reference is the source-of-truth for link identity). An unfinished creation is reconciled first, not recreated.

### Creation recovery

Sync serializes mutating runs within a tasks workspace using `.tasks/.sync-pending.lock` (an OS file lock released when the process exits). Remote aliases and push/pull share this lock. Tasks outside that workspace are not eligible for creation/link recovery. This does not lock out ordinary task editors or coordinate separate workspace copies.

Before creating a Jira/GitHub issue or importing a local task, sync persists an intent in `.tasks/.sync-pending.json`. After creation returns, it persists the returned reference/task ID before attaching the platform reference. Journal writes and the linked task are synced to disk before the recovery entry is removed. This journal is required even when optional reports are disabled; do not delete it to clear a sync error.

A retry reconciles known IDs before selecting tasks or indexing references, including task-targeted retries. Recovery is scoped by local project, provider, API endpoint hash, and remote project/repository; remote aliases with identical identity can recover the same operation. A conflicting endpoint/repository or existing reference fails closed instead of attaching across scopes. The journal stores no auth headers/tokens or task bodies; the endpoint is hashed rather than stored as a URL.

Task identity is the file resolved by the current storage lookup, not the raw request string. Storage aliases such as `TEST-01`, `TEST-+1`, and `TEST-1-extra` all key the same `TEST-1` operation. Target selection, new intents, and loaded journal entries are canonicalized before comparison; unknown or invalid stored task IDs block recovery rather than being skipped. Canonicalization during a dry run is in-memory only.

There is no atomic transaction between a remote server and local storage. If creation fails or the process stops after creation but before its returned ID is journaled, sync reports **Indeterminate sync creation** and will not create again automatically. This also applies when local task creation commits but fails before returning an ID. Even an apparently rejected create is treated conservatively.

To recover an indeterminate operation:

1. Stop sync runs and back up the pending journal. Keep the original project, remote destination, and auth endpoint configuration.
2. Inspect the remote issues and local tasks to establish whether creation committed. Do not infer identity from title similarity alone.
3. If it committed, fill only the missing `task_id` or `reference` in the matching JSON entry with the verified ID. Retry the same project/remote to attach the reference. Resolve any conflicting local reference explicitly first.
4. Remove an intent only after verifying that creation did not occur. If the outcome remains uncertain, keep it blocked. Never delete a remote issue as blind compensation.

Recovery guarantees depend on keeping the journal with its workspace and on filesystem locking/fsync support. A read-only task or directory is checked before remote creation, but concurrent non-sync edits and later I/O failures can still cause linking to fail; the returned identity remains recoverable. Dry runs do not acquire/create the lock, persist recovery state, attach references, write reports, or mutate the remote. Pull dry runs still read remote data; push dry runs without resolved auth warn that pending recovery is not reconciled.

The shared atomic writer syncs its temporary file before publishing and never deletes the old journal to retry a failed replacement. Sync additionally fsyncs parent directories on Unix after publication and before forgetting a durable task link. Portable `std::fs` does not provide directory fsync on Windows; Windows power-loss durability is not claimed or runtime-verified by the macOS regression tests.

## Config structure

### Project config (`.tasks/<PROJECT>/config.yml`)

Project config holds all non-auth sync settings.

```yaml
remotes:
  jira-home:
    provider: jira
    project: ENG
    auth_profile: jira.default
    filter: "status != Done"
    mapping:
      title: summary
      description:
        field: description
        default: "Synced by LoTaR"
      status:
        field: status
        values:
          ToDo: to-do
          InProgress: in-progress
          Done: done
      tags:
        field: labels
        add: [lotar, synced]
      assignee:
        field: assignee
        when_empty: clear

  github-company:
    provider: github
    repo: your-org/your-repo
    auth_profile: github.default
    mapping:
      title: title
      description: body
      status:
        field: state
        values:
          Done: closed
          InProgress: open
```

### Home config (`~/.lotar`)

Home config is auth-only. Project config never stores secrets.
Manage auth profiles manually; the UI and CLI do not create or edit them.

```yaml
auth_profiles:
  jira.default:
    method: basic
    email_env: LOTAR_JIRA_EMAIL
    token_env: LOTAR_JIRA_TOKEN
    base_url: https://your-domain.atlassian.net
  github.default:
    method: token
    token_env: LOTAR_GITHUB_TOKEN
```

## Mapping rules

- `field` indicates the remote property to map to.
- `values` maps local value strings to remote value strings.
- `set` overwrites the remote value with a constant.
- `default` applies only when the local value is empty.
- `add` appends to list fields (e.g., tags/labels).
- `when_empty: clear` removes the remote value when the local field is empty.

Shorthand is allowed for basic mappings:

```yaml
mapping:
  title: summary
  status:
    field: status
    values:
      Done: done
```

## Merge semantics

- Pull:
  - Only mapped fields are updated locally.
  - Unmapped local fields remain untouched.
- Push:
  - Only mapped fields are updated remotely.
  - Unmapped remote fields remain untouched.

This guarantees pull→push and push→pull round-trips are no-ops unless a mapped field changed on the source side.

## Pitfalls

- Pull without `--project` uses `default_project` when set; if it is empty, Jira remotes fall back to the remote Jira project key as the local prefix (which may not match your local naming).
- GitHub `filter` uses the Search API, which caps results at 1000 issues and may omit fields compared to the issues list API.

## Platform notes

### Jira

- CLI auth can use email + API token (basic auth) via env variables.
- OAuth flows are not supported; use manual tokens or basic auth.
- Description updates should use Atlassian Document Format (ADF) when pushing.
- Jira Cloud requires accountId values for reporter/assignee. Use account IDs in mappings if you want those fields set.

How to get Jira values:
- `email_env`: use your Atlassian account email (direct) or set an env var name.
- `token_env`: create an API token in Atlassian → Account settings → Security → API tokens. Store the token directly or set an env var name.
- `base_url`: your Jira site URL (e.g., `https://your-domain.atlassian.net`).

### GitHub

- Use a PAT or GitHub App token.
- Issue `state` is `open|closed`, and `state_reason` can be set when closing.

How to get GitHub values:
- `token_env`: create a Personal Access Token in GitHub → Settings → Developer settings → Personal access tokens. Store the token directly or set an env var name.
  - Fine-grained: grant access to the repo and issues.
  - Classic: use `public_repo` for public repos or `repo` for private repos.
- For GitHub App tokens, install the app on the repo and generate an installation token; store it in your chosen env var.

## Error handling and reporting

- Each sync run returns a summary plus per-task results (created/updated/skipped/failed).
- `--dry-run` returns the same report without applying changes.
- Per-task failures are normally non-fatal to the overall run. Invalid/indeterminate recovery state and concurrent sync runs abort safely with an actionable error.
- Sync runs emit SSE progress events (`sync_started`, `sync_progress`, `sync_completed`, `sync_failed`) so the UI can show live status.
- Non-dry-run reports are persisted to disk (default `.tasks/@reports`) when `sync.write_reports` is enabled; disable by setting `sync.write_reports: false`. Filenames include a bounded, sanitized run ID and use exclusive creation with a numeric collision suffix. Identical timestamps, repeated client IDs, sanitization collisions, and existing files never overwrite earlier reports. Original IDs remain unchanged inside reports; slashes and other unsafe filename characters are not used in paths.
- REST helpers: `GET /api/sync/reports/list` and `GET /api/sync/reports/get?path=<relative>`.
