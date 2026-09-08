# Copilot adapter

Shared policy and the task-to-guidance map live in [AGENTS.md](../AGENTS.md).
Path-scoped conventions use `instructions/*.instructions.md` with `applyTo`.

Copilot supports on-demand skills in `.github/skills/<name>/SKILL.md`.
Use native skill discovery when available; otherwise open the relevant file from
the AGENTS.md map. Do not load the whole collection or duplicate shared policy here.
