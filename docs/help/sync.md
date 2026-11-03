# lotar pull / push / sync check

Copy tasks between LoTaR and Jira or GitHub issues. Sync is in beta and only runs when you start it.

## Usage

```bash
lotar sync check jira-home              # test the credentials and settings; changes nothing
lotar pull jira-home --dry-run          # preview what a pull would change
lotar pull jira-home                    # bring remote issues into local tasks
lotar push jira-home --task DEMO-1      # send one task to the remote
lotar push jira-home                    # send every task in the project
```

`jira-home` is the name of a remote you set up in the config (see below). The same runs are on the **Sync** page of the web UI (`lotar serve`), and as the `sync_pull` and `sync_push` [MCP tools](./mcp-tools.md).

## How it works

- **Pull** reads the issues that match the remote's filter. An issue that is not linked to a task yet becomes a new task. A linked task gets the remote values.
- **Push** sends tasks to the remote. A task that is not linked yet becomes a new issue. A linked issue gets the local values.
- Only the fields listed under `mapping` change. Everything else stays as it is, on both sides.
- A task and an issue are linked by a reference on the task, such as `jira: ENG-12` or `github: your-org/your-repo#12`. Sync adds it when it creates a task or an issue. Titles are never used to match.
- Nothing runs in the background. There is no polling and no webhook.

Push can create many issues at once, and pull can create many tasks. Start with `lotar sync check`, then `--dry-run`, then a single task with `--task`. Use credentials that can only reach the projects or repositories you sync.

To link a task to an issue that already exists, add the reference to the task file before you push or pull:

```yaml
references:
- jira: ENG-12          # or: github: your-org/your-repo#12
```

If you remove the reference after a sync, the next push creates a new issue. A pull skips issues linked to a task that was deleted or moved to another project, so they are not imported again.

## Options

| Option | Commands | What it does |
| --- | --- | --- |
| `<remote>` | all | Name of the remote under `remotes:` in the config. |
| `--project, -p <PREFIX>` | all | The local project to sync. Without it, LoTaR uses the default project. |
| `--task <ID>` | pull, push | Sync only this task. For pull, the task must already be linked to an issue. |
| `--auth-profile <name>` | all | Use this auth profile instead of the one set on the remote. |
| `--dry-run, -n` | pull, push | Show what would change. Nothing is written, locally or on the remote. |
| `--format json` | all | Print the result as JSON. |

A pull with no project and no default project uses the Jira project key as the local prefix and warns about it. A GitHub pull needs a project.

`lotar sync check` connects to the remote. It checks the credentials, the Jira project or GitHub repository, and the filter. It does not read or change any task.

## Set up a remote

Put remotes in `.tasks/<PROJECT>/config.yml` for one project, or in `.tasks/config.yml` for every project. A project remote with the same name replaces the shared one. Remotes hold no secrets, so they are safe to commit.

```yaml
# .tasks/DEMO/config.yml
remotes:
  jira-home:
    provider: jira
    project: ENG                  # Jira project key; new issues are created here
    auth_profile: jira.default    # name of a profile in ~/.lotar
    mapping:
      title: summary
      description: description
      status:
        field: status
        values:
          Todo: To Do
          InProgress: In Progress
          Done: Done

  github:
    provider: github
    repo: your-org/your-repo
    auth_profile: github.default
    mapping:
      title: title
      description: body
      status:
        field: state
        values:
          Todo: open
          InProgress: open
          NeedsReview: open
          Done: closed
```

Run `lotar config validate --project DEMO` to check the file. That only checks the YAML. `lotar sync check <remote>` checks that the remote works.

The `filter` setting chooses which issues a pull reads:

- **Jira**: `filter` is the whole JQL query and replaces the default `project = <key>`. Include the project yourself, for example `filter: "project = ENG AND status != Done"`.
- **GitHub**: `filter` adds search terms to the repository, for example `filter: "label:bug"`. Without a `state:` term, LoTaR adds `state:all`. GitHub search returns at most 1000 issues and can leave out some fields.

## Set up credentials

Credentials live in your home config, `~/.lotar`, never in the repository. LoTaR does not create or edit them, so add them yourself. Keep the file private (`chmod 600 ~/.lotar`); LoTaR warns when others can read it.

```yaml
# ~/.lotar
auth_profiles:
  jira.default:
    method: basic
    base_url: https://your-domain.atlassian.net
    email_env: LOTAR_JIRA_EMAIL
    token_env: LOTAR_JIRA_TOKEN
  github.default:
    method: token
    token_env: LOTAR_GITHUB_TOKEN
```

```bash
export LOTAR_JIRA_EMAIL=you@example.com
export LOTAR_JIRA_TOKEN=...       # your Jira API token
export LOTAR_GITHUB_TOKEN=...     # your GitHub token
```

| Setting | Meaning |
| --- | --- |
| `method` | `basic` (email and token, the Jira default) or `token` (a bearer token, the GitHub default; `bearer` and `pat` mean the same). |
| `token_env` | Name of the environment variable that holds the token. |
| `email_env` | Name of the environment variable that holds your email. Needed for `basic`. |
| `base_url` | Your Jira site, such as `https://your-domain.atlassian.net`. Needed for Jira. |
| `api_url` | Use another API address, such as GitHub Enterprise. GitHub defaults to `https://api.github.com`. |
| `provider` | Optional. `jira` or `github`; LoTaR warns if it does not match the remote. |

If no environment variable with the given name is set, LoTaR uses the text itself as the value and prints a warning. Environment variables are the safer choice.

Where to get a token:

- **Jira**: Atlassian account settings → Security → API tokens. Use it with your Atlassian email and `method: basic`. Sign-in with OAuth is not supported.
- **GitHub**: Settings → Developer settings → Personal access tokens. A fine-grained token needs access to the repository and read/write access to issues. A classic token needs `public_repo` for public repositories or `repo` for private ones. A GitHub App installation token also works.

## Field mapping

Each key under `mapping` is a local field: `title`, `description`, `status`, `priority`, `type`, `assignee`, `reporter`, `tags`, or the name of a custom field. The value is the remote field name, or a block with these settings:

| Setting | Meaning |
| --- | --- |
| `field` | The remote field, such as `summary`, `labels` or `customfield_10010` in Jira, or `title`, `body`, `state`, `labels` or `assignees` in GitHub. |
| `values` | Translate values: local value on the left, remote value on the right. Pull uses the same list the other way round. |
| `set` | Always push this value. |
| `default` | Push this value when the local field is empty. |
| `add` | Extra items to push into a list field, such as labels. |
| `when_empty` | `clear` empties the other side when this side is empty. `skip` (the default) leaves it alone. |

```yaml
mapping:
  priority:
    field: priority
    values:
      Low: Low
      Medium: Medium
      High: High
  tags:
    field: labels
    add: [lotar]
  assignee:
    field: assignee
    when_empty: clear
```

A pulled status, priority or type must exist in the project. If it does not, that issue fails and the report lists the allowed values. Extend `values` or the project config to fix it. When several local values map to the same remote value, a pull keeps the task's current value.

## Jira and GitHub notes

**Jira**

- Descriptions become Markdown-style text on pull: blank lines between paragraphs, `- ` and `1. ` lists, `#` headings, fenced code, `> ` quotes and `[text](url)` links. Mentions, emoji, dates and status labels become plain text. A push turns the text back into Jira's format. Bold, colors and mention links are lost when you edit a description locally and push it.
- A push changes the Jira status through a workflow transition. The mapped status value must match the name of a transition that is available for the issue.
- New issues get the `Task` issue type unless you map `type`. If the mapped type does not exist in the Jira project, LoTaR uses `Task` and warns.
- Jira needs account IDs for the assignee and reporter. A pull stores the user's email or display name. A push looks that up and uses an account only when the name or email matches exactly; otherwise it skips the field with a warning. Use account IDs in `values` to avoid the lookup.

**GitHub**

- An issue is either `open` or `closed`, so map every local status to one of the two.
- Assignees and labels are lists. Map `assignee` to `assignees` and `tags` to `labels`.

## Results and reports

Each run prints how many tasks or issues it created, updated, skipped and failed, followed by any warnings. A failed task or issue does not stop the run and does not change the exit code, so read the counts and warnings. `--format json` prints the same summary.

Every run except a dry run writes a report to `.tasks/@reports/`. The Sync page in the web UI lists the reports and every task in them. Change the folder with `sync.reports_dir`, or turn reports off with `sync.write_reports: false`.

## When a create was interrupted

Before sync creates an issue or a task, it notes the job in `.tasks/.sync-pending.json`. If it cannot tell whether the create worked (for example, the network dropped), it stops with **Indeterminate sync creation** and will not create that item again on its own. This protects you from duplicates.

To fix it:

1. Look on the remote and in your tasks to see whether the item was created.
2. If it was, put the issue key in the entry's `reference` (or the new task ID in `task_id`) and run the same command again. Sync then links them.
3. If it was not, remove that entry from the file and run the command again.

Do not delete the file to get rid of the error. Leave `.tasks/.sync-pending.lock` alone too.

## See also

- [Configuration Reference](./config-reference.md) for every `remotes` and `auth_profiles` setting
- [API Quick Reference](./api-quick-reference.md) for the `/api/sync/*` endpoints
- [SSE Events](./sse.md) for live sync progress
- [Sync contract](../developers/sync.md) for the full rules, for developers
