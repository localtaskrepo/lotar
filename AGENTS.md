# AGENTS.md — working in this repo (LoTaR)

This is the canonical guide for AI coding agents. It is read by both **Kilo** and
**GitHub Copilot**. Deeper, task-specific workflows live in skills (`.github/skills/`)
that load on demand (see "Where guidance lives" below), so this file stays short and evergreen.

## Development workflow

Every task runs the same lifecycle. For anything non-trivial, load the
`development-workflow` skill first.

1. **Start** from a forked/overview worktree — each task gets its own isolated git worktree.
2. **Discuss** the task with the developer. Simple tasks stay in-context; complex tasks get a written plan + tracking (prefer a LoTaR task — see the `lotar-dev-tracking` skill).
3. **Develop**, running quality gates as you go (see "Quality gates").
4. **Developer reviews** your changes incrementally. Files you see **staged** (`git add`) have already been reviewed and approved — that is intentional bookkeeping for parallel work, not an error. Leave staged files staged; don't unstage or re-edit them unless asked.
5. **Greenlight → integrate** to the main line by **rebasing** your branch onto `main` (keeps history linear). Do not merge-commit.
6. **Conflicts?** Resolve them in your worktree, re-run gates, and surface only the conflict-resolution changes for one more developer review.
7. **Commit** (by the developer or the agent, per their instruction). The worktree and task are then done.

## Git rules

- Never use git to **recover lost code** — use the `.history/` folder (VS Code Local History).
- Git operations (commit, rebase) are a **defined workflow step after greenlight**, not something to do proactively mid-task.
- **Rebase, don't merge**, when integrating to `main`.
- **Never `git stash` to move work across worktrees** — stashes are shared globally and leak between worktrees. If you must stash, keep it to a single, immediately-popped local operation.
- Stay inside your own worktree; don't edit files in other worktrees or the main checkout unless explicitly asked.

## Quality gates (run after any code change)

- **Lint:** `npm run lint` — `vue-tsc` + `cargo clippy --all-targets --all-features -- -D warnings` + `cargo fmt --all --check`. This mirrors CI's static checks; if formatting is flagged, fix with `npm run fmt` (= `cargo fmt --all`).
- **Tests:** `npm test` (Rust via `cargo nextest` + UI via `vitest`). **`cargo test` is forbidden.**
- **Smoke:** `npm run smoke` (it rebuilds first, so no separate `npm build` is needed).

Run these even if the developer doesn't explicitly ask. Start targeted, then widen (see the `testing-strategy` skill).

## Contract sync (REST changes)

Changing a REST request/response shape means updating **all three** together:

- `src/api_types.rs` · `view/api/types.ts` · `docs/openapi.json`

…plus the relevant `docs/help/*` pages. See the `api-contract-change-end-to-end` skill.

## Storage discipline

- `.tasks/` is the **real, version-controlled backlog** for this project — it is committed (not gitignored). Keep YAML backwards-compatible (these are user-owned files).
- The automated test suite uses **isolated temp workspaces** (`smoke/helpers/workspace.ts`) and never touches the repo `.tasks/`. For manual/dev testing use the task generator (`npm run seed:test-tasks`) instead of polluting `.tasks/`.

## Where guidance lives

Each piece of guidance has exactly one home — consume it via your tool's native mechanism rather than duplicating it:

- **Per-area structure & conventions** (Rust / UI / smoke): `.github/instructions/*.instructions.md`. Copilot applies these automatically per file via `applyTo`; in **Kilo, read the matching file on demand** (see the `skill-discovery` skill).
- **Workflow runbooks** (testing, handoff, dev-tracking, …): skills at `.github/skills/<name>/SKILL.md`. Kilo lists them via the `skill` tool and loads on demand; Copilot references them from `.github/copilot-instructions.md`.

At the start of non-trivial work, load the 1–3 most relevant. Key skills:

- `development-workflow` — full task lifecycle and worktree/rebase rules.
- `skill-discovery` — maps "what you're editing" → which instruction/skill to load.
- `testing-strategy` · `smoke-suite-debugging` · `local-dev-serve-troubleshooting` · `api-contract-change-end-to-end` · `lotar-dev-tracking` · `review-handoff`.

## Safety

- Never log or paste **secrets/PII** (tokens, auth headers, cookies). Treat `.env*` and credential files as sensitive.
- Prefer **creating smoke tests** over ad-hoc manual testing in temp dirs.
- Prefer **browser/vision-based verification** (Playwright smoke, screenshots, UI diff) over eyeballing when validating UI changes (see `review-handoff`).

## Self-improvement loop

Treat instruction/skill files as part of the product: keep them accurate, minimal, and actionable.

- Notice confusion, drift, or missing guidance while working? Make the **smallest edit** that fixes it.
- Prefer links/pointers over duplicating long runbooks; keep repo-wide docs short and evergreen; put deep workflows in skills.
- Expect to repeat a workflow? Add a narrowly-scoped skill under `.github/skills/<name>/SKILL.md` (search existing skills first to avoid duplicates).
- Do **not** use git (commits/reverts) for these edits — they're visible to the user and recoverable via `.history/`.
