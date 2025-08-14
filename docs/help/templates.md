# Templates Guide

Quick reference for initializing project configuration from built-in templates.

## Available templates

- default — Basic task management
- agile — Agile/Scrum workflow (statuses/types tuned for sprints)
- kanban — Kanban style setup
- simple — Minimal configuration

List them anytime:

```bash
lotar config templates
```

## What templates contain

- Issue fields as plain arrays (no legacy `values:` wrapper)
  - issue.states, issue.types, issue.priorities, issue.categories, issue.tags
- Optional defaults under the `default.*` group
- No automation flags are written; automation defaults are already enabled globally

Resulting files are written in the canonical nested YAML shape. You can also run:

```bash
lotar config normalize --project=PREFIX --write
```

to reformat any config into the canonical shape.

## Automation defaults

- auto.set_reporter: true (default)
- auto.assign_on_status: true (default)
- Identity helpers are also on by default:
- auto.identity: true
- auto.identity_git: true

Notes:
- To disable either behavior, edit the YAML and set the corresponding `auto.*` key to `false`.
- The current `lotar config set` command does not toggle `auto.*` keys; change them in YAML.

## Examples

Initialize a project with the agile template:

```bash
lotar config init --project=backend --template=agile
```

A minimal project config produced by a template (canonical shape):

```yaml
project:
  id: DEMO
issue:
  states: [Todo, InProgress, Done]
  types: [feature, bug, chore]
  priorities: [Low, Medium, High]
  categories: ["*"]
  tags: ["*"]
# default, custom, and scan sections are added when needed
# automation is enabled globally; add `auto.*` only if overriding

After initializing, you can set defaults and vocabularies via config set:

```bash
lotar config set default.category Engineering --project=DEMO
lotar config set default.tags '["triage","sev"]' --project=DEMO
lotar config set issue.categories '["Engineering","QA","Ops"]' --project=DEMO
lotar config set issue.tags '["frontend","backend"]' --project=DEMO
```
```

## Migration notes

- Older templates used `values:` wrappers; these are no longer used. Arrays are written directly.
- The deprecated `require_assignee` key has been removed from templates; auto-assign on status change is controlled by `auto.assign_on_status` (default true).
- Legacy `taxonomy.categories` and `taxonomy.tags` are still accepted on input, but normalization now writes them under `issue.categories` and `issue.tags`.
