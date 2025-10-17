# Templates Guide

Quick reference for initializing project configuration from built-in templates.

## Available templates

- default — Basic task management
- agile — Agile/Scrum workflow (statuses/types tuned for sprints)
- kanban — Kanban style setup

List them anytime:

```bash
lotar config templates
```

## What templates contain

- Issue vocabularies are plain arrays (no legacy `values:` wrapper)
  - `issue.states`, `issue.types`, `issue.priorities`, `issue.tags`
  - The default template also includes `issue.categories` to ease migrations; remove or rename if unwanted
- Custom field hints live under `custom.fields`
  - Agile/Kanban templates still include a `category` custom field for legacy compatibility; rename or drop it if your workflow prefers `product` or other custom properties
- Optional defaults appear under the `default.*` group when a template needs them
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

An example canonical project config produced by the default template:

```yaml
project:
  name: Demo Service
issue:
  states: [Todo, InProgress, Done]
  types: [Feature, Bug, Chore]
  priorities: [Low, Medium, High]
  tags: ["*"]
  categories: ["*"]
# Agile/Kanban add custom field stubs such as:
# custom:
#   fields: ["category", "sprint"]
# default, custom, and scan sections are added when needed
# automation is enabled globally; add `auto.*` only if overriding

After initializing, you can set defaults and vocabularies via config set:

```bash
lotar config set default.tags '["triage","sev"]' --project=DEMO
lotar config set custom.fields '["product","sprint"]' --project=DEMO
lotar config set issue.tags '["frontend","backend"]' --project=DEMO
```
```

## Migration notes

- Older templates used `values:` wrappers; these are no longer used. Arrays are written directly.
- The deprecated `require_assignee` key has been removed from templates; auto-assign on status change is controlled by `auto.assign_on_status` (default true).
- Legacy `taxonomy.categories` and `taxonomy.tags` are still accepted on input, but normalization now writes them under `issue.categories` and `issue.tags` respectively.
