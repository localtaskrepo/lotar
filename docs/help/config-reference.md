# Configuration Reference

All configuration keys by scope with types and notes. Canonical YAML uses nested groups and only emits values that differ from the built-in defaults. If `.tasks/config.yml` contains just a comment, you're still using the defaults listed below.

## Precedence
CLI > project > env > home > global > defaults (commands without a project skip the project step). See [Resolution & Precedence](./precedence.md).

## Canonical keys (nested)
- server.port: number (default 8080)
- default.project: string (default project prefix)
- default.assignee: string
- default.reporter: string
- default.priority: enum Priority
- default.status: enum TaskStatus
- default.tags: string[]
- default.strict_members: boolean (default false)
- members: string[]
- issue.states: string[] (e.g., [Todo, InProgress, Done])
- issue.types: string[] (feature, bug, epic, spike, chore)
- issue.priorities: string[] (Low, Medium, High, Critical)
- issue.tags: string[]
- custom_fields: string[]
- attachments.dir: string (default "@attachments")
- attachments.max_upload_mb: number (default 10) — `0` disables uploads; `-1` is unlimited; positive values are MiB
- sync.reports_dir: string (default "@reports")
- sync.write_reports: boolean (default true)
- scan.signal_words: string[] (default: [TODO, FIXME, HACK, BUG, NOTE])
- scan.ticket_patterns: string[] (regex patterns to detect ticket keys)
- scan.enable_ticket_words: boolean (default: false) — when true, issue-type words (like Feature/Bug/Chore) act as signal words in addition to TODO/FIXME/etc. Note: bare ticket keys alone do not trigger creation.
- scan.enable_mentions: boolean (default: true) — when true, add code anchors under `references` for existing ticket keys found in source
- scan.strip_attributes: boolean (default: true) — when true, remove inline [key=value] attribute blocks from source after inserting the ticket key
- auto.identity: boolean (default true)
- auto.identity_git: boolean (default true)
- auto.set_reporter: boolean (default true)
- auto.assign_on_status: boolean (default true)
- auto.codeowners_assign: boolean (default true)
- auto.tags_from_path: boolean (default true)
- auto.branch_infer_type: boolean (default true)
- auto.branch_infer_status: boolean (default true)
- auto.branch_infer_priority: boolean (default true)
- auto.populate_members: boolean (default true)
- remotes: map (named remotes)
- remotes.<name>.provider: enum (jira|github)
- remotes.<name>.project: string (Jira project key)
- remotes.<name>.repo: string (GitHub owner/repo)
- remotes.<name>.filter: string (JQL or GitHub qualifiers)
- remotes.<name>.auth_profile: string (home profile name)
- remotes.<name>.mapping: map (local field name -> mapping)
- remotes.<name>.mapping.<field>.field: string (remote field name)
- remotes.<name>.mapping.<field>.values: map (local value -> remote value)
- remotes.<name>.mapping.<field>.set: string (force remote value)
- remotes.<name>.mapping.<field>.default: string (apply when local value is empty)
- remotes.<name>.mapping.<field>.add: string[] (append for list fields like tags/labels)
- remotes.<name>.mapping.<field>.when_empty: enum (skip|clear)
- auth_profiles: map (home config only; ignored in project configs)
- auth_profiles.<name>.provider: enum (jira|github)
- auth_profiles.<name>.method: string
- auth_profiles.<name>.token_env: string (env var name or literal token)
- auth_profiles.<name>.email_env: string (env var name or literal email)
- auth_profiles.<name>.base_url: string
- auth_profiles.<name>.api_url: string

## Home and Project keys
Same shape as global; project values override global for that project. Use `project.name` for an optional human-readable label (the folder name remains the canonical identifier).

Auth profiles are home-only: `auth_profiles` is ignored in project configs (secrets never live in the repo). Remotes can be configured globally or per-project; project remotes override/extend global remotes.

Project configs can also override any automation toggle inherited from global defaults. Supported keys: `auto.populate_members`, `auto.set_reporter`, `auto.assign_on_status`, `auto.codeowners_assign`, `auto.tags_from_path`, `auto.branch_infer_type`, `auto.branch_infer_status`, `auto.branch_infer_priority`, `auto.identity`, and `auto.identity_git`.

> Legacy: older configs may include `issue.categories`. The key is normalized for compatibility but is not consumed by the runtime. Prefer modeling these labels with `custom_fields` (the dotted alias remains accepted for compatibility).

## Examples
```yaml
# ~/.lotar (home config)
default:
	project: DEMO

# .tasks/config.yml (global)
issue:
	states: [Todo, InProgress, Done]
	priorities: [Low, Medium, High]
scan:
	signal_words: [TODO, FIXME]
	ticket_patterns: ["[A-Z]{2,}-\\d+"]
	enable_ticket_words: true
	enable_mentions: true

attachments:
	# Relative paths resolve under the tasks directory (the `.tasks` folder)
	dir: "@attachments"
	# 0 disables uploads, -1 is unlimited, positive values are MiB
	max_upload_mb: 10

sync:
	# Sync reports (stored under .tasks/@reports by default)
	reports_dir: "@reports"
	write_reports: true

# .tasks/DEMO/config.yml (project)
project:
	name: Demo Service
issue:
	types: [feature, bug, chore]
attachments:
	# Project-specific override (optional)
	max_upload_mb: 25
scan:
	enable_mentions: false  # example: disable adding anchors for existing keys in this project
auto:
	identity: true
	identity_git: true
	assign_on_status: true
default:
	tags: [oncall, sev]

remotes:
	jira-home:
		provider: jira
		project: DEMO
		auth_profile: jira.default
		mapping:
			title: summary
			status:
				field: status
				values:
					Todo: to-do
					Done: done
```

## Validation
- Values are checked against allowed enums.
- Unknown keys are ignored or flagged during validation (see `lotar config validate`).
- If `scan.ticket_patterns` include invalid regex or overlapping patterns, validation will report errors/warnings.
