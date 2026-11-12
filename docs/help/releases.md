# lotar release

Releases are the versions you plan and ship, and the tickets that go out in them. They work like [sprints](./sprints.md): each release is a file of its own, a ticket has a list of releases, and the CLI, REST API, MCP tools and web UI's Releases page all read and change the same data.

A release is not the same as a ticket's version fields. `fixed_in` and `affects` ([version fields](./list.md#version-fields)) say what a ticket is about; a ticket's releases say which releases ship it. The two can differ.

## Usage

```bash
lotar release <command> [OPTIONS]
```

## Where releases live

Each release is `.tasks/@releases/<id>.yml`, numbered like sprints:

```yaml
version: 0.15.0
label: Spring
notes: Search and releases
planned_date: 2026-12-01
released_at: 2026-12-02T10:00:00+00:00
created: 2026-11-12T20:00:00+00:00
modified: 2026-12-02T10:00:00+00:00
history:
- at: 2026-12-02T10:00:00+00:00
  actor: alice
  changes:
  - field: released_at
    new: 2026-12-02T10:00:00+00:00
```

- `version` is required and unique. It is stored without a leading `v`, so `v0.15.0` and `0.15.0` are the same release. It must be a version such as `1.4.2`, `0.13` or `1.0.0-rc.1`.
- `planned_date` is a day (`YYYY-MM-DD`). A release planned for a day before today that is not released is overdue.
- `released_at` is set when the release goes out; while it is empty the release is planned.
- Changes are recorded in `history`. Keys lotar does not know are kept.

A ticket lists its releases by ID:

```yaml
title: Fix crash on start
releases: [3, 4]
```

A ticket can be in several releases: a fix can ship in 0.15.1 and in 0.16.0. Because the list is on the ticket, a ticket keeps its releases when it is moved to another project, renamed or archived. Task files without `releases` are in no release.

## Naming a release

Commands take a release by ID (`3` or `#3`), version (`0.15.0`, `v0.15.0`) or label (`Spring`, any case). A bare number is an ID; write a version with a dot (`2.0`).

## Commands

- `lotar release create <version> [--label <text>] [--notes <text>] [--planned YYYY-MM-DD] [--released-at <when>]` — creates a release. `--released-at` records one that is already out (RFC 3339 or `YYYY-MM-DD`).
- `lotar release list` — every release in version order with its state, dates and how many of its tickets are done.
- `lotar release show <release>` — one release and its tickets.
- `lotar release update <release> [--version <v>] [--label <text>] [--notes <text>] [--planned <date>] [--released-at <when>]` — changes the fields given. An empty value (`--label ""`) clears a field; `--released-at ""` marks the release planned again. A change that changes nothing writes nothing.
- `lotar release release <release> [--at <when>]` — marks it released, now or at `--at`.
- `lotar release reopen <release>` — marks a released release planned again.
- `lotar release delete <release> [--force]` — deletes the release and takes it off its tickets. It asks first; JSON output and non-interactive input need `--force`.

### Tickets in a release

- `lotar release add <release> [<task-id>...] [--select '<query>']` — adds tickets. They keep their other releases.
- `lotar release move <release> [<task-id>...] [--select '<query>']` — moves tickets to the release: their other releases are replaced by this one.
- `lotar release remove <release> [<task-id>...] [--select '<query>']` — takes the release off the tickets.
- `lotar release backlog [<query>...] [--project <prefix>] [--limit N]` — open tickets that are in no release, narrowed with the [search syntax](./list.md#filters-in-the-query).

`--select` picks tickets with the same search syntax as `lotar list`, and the `--select-status`, `--select-tag`, `--select-priority`, `--select-type` and `--select-project` flags work as for [`lotar sprint add`](./sprints.md). Each membership change is an ordinary ticket update: it is recorded in the ticket's history and runs automation.

### Example

```bash
lotar release create 0.15.0 --label Spring --planned 2026-12-01
lotar release add 0.15.0 --select 'status:done release:none'
lotar release add 0.15.0 APP-12 APP-14
lotar list 'release:0.15 -status:done'      # what is still open
lotar release release 0.15.0
```

All commands take `--format json`; the JSON matches the REST responses.

## Finding tickets by release

The `release:` term works in `lotar list`, the web UI's search box, REST `query` and MCP `task_list`:

| Term | Tickets |
| --- | --- |
| `release:0.15` | in a release whose version is 0.15.x |
| `release:0.15.0`, `release:v0.15.0` | in release 0.15.0 |
| `release:3`, `release:#3` | in release #3 |
| `release:spring` | in the release labelled Spring |
| `release>=0.15 release<0.16` | in a release in that version range |
| `release:none` | in no release |
| `-release:none` | in at least one release |

Several values (`release:0.15,0.16`) match tickets in any of them. REST `/api/tasks/list` also takes `releases=<value>,...`, and MCP `task_list` a `releases` parameter. `--sort-by releases` (REST and MCP `sort_by=releases`) orders tickets by their earliest release in version order; tickets in no release come first when ascending.

## REST API

- `GET /api/releases/list` — releases in version order with counts (`task_count`, `done_count`, `open_count`), `state` (`planned` or `released`) and `overdue`.
- `GET /api/releases/get?release=<release>` — one release with its tickets and history.
- `GET /api/releases/backlog` — open tickets in no release; takes the `/api/tasks/list` filters plus `limit` and `offset`.
- `POST /api/releases/create`, `/update`, `/release`, `/reopen`, `/delete`.
- `POST /api/releases/add`, `/move`, `/remove` with `{ "release": ..., "tasks": [...], "query": "..." }`.
- Task create and update take `releases: [<id>]` (update replaces the list; `[]` or `null` clears it). Every task carries `releases`.

The schemas are in [openapi.json](../openapi.json). A change to a release file sends a `release_changed` [event](./sse.md).

## MCP tools

`release_list`, `release_get`, `release_create`, `release_update`, `release_delete`, `release_add`, `release_move`, `release_remove`, `release_backlog`, `release_release` and `release_reopen`; see [MCP Tools](./mcp-tools.md#releases).

## Web UI

The Releases page lists the releases in version order, each with its tickets, a progress bar of done tickets, and its planned and release dates. The backlog holds open tickets in no release. Drag a ticket from the backlog onto a release to add it, from one release to another to move it, or back to the backlog to take it out of that release; select several to add, move or remove them at once. A release's menu edits, deletes, marks it released or reopens it. The task panel shows a ticket's releases and sets them.

## Automation

Rules can check, change and react to releases:

- condition `release:` on the ticket as it is now: a value as in the search term above, `@next` (the first planned release in version order), `@planned` or `@released` (in a release in that state), or `{ exists: false }` (in no release), `{ in: [...] }`, `{ version: '>=0.15' }`.
- action `add: { release: <release or @next> }` and `remove: { release: <release> }`.
- event `release_changed`, when a ticket's releases change.
- template value `${{ticket.releases[*]}}`.

```yaml
automation:
  rules:
    - name: Done work goes into the next release
      when: { status: Done, release: { exists: false } }
      on:
        updated:
          add: { release: '@next' }
```

See [Automation](./automation.md).
