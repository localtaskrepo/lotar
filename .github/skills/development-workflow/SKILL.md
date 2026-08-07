---
name: development-workflow
description: Use this at the start of any non-trivial task to follow the standard worktree lifecycle — discuss, plan, develop, gate, review, rebase, commit.
---

## Lifecycle (every task)

1. **Worktree** — you start in an isolated git worktree (usually under `.kilo/worktrees/<name>/`). Treat it as your whole world: develop here, test here, fix here. Don't edit files in other worktrees or the main checkout unless the developer asks.

2. **Discuss** — understand the task with the developer before coding.
   - *Simple task* → keep the discussion in-context and start.
   - *Complex/multi-file task* → write a short plan (what changes, which files, risks, test plan) and get alignment first. Track it as a LoTaR task if the work spans sessions (see the `lotar-dev-tracking` skill).

3. **Develop** — implement, running quality gates as you go (see AGENTS.md "Quality gates"). Keep changes scoped; resist refactoring unrelated code.

4. **Developer review (incremental)** — the developer reviews as you go. A **staged** file (`git add`) means "reviewed and approved." That staging is intentional bookkeeping so the developer can verify in parallel with your work. Do **not** be confused by staged files, do not unstage them, and do not re-edit an already-staged file unless asked. You can keep working on the *unstaged* parts.

5. **Greenlight → integrate** — once the developer approves the whole task, bring your branch back to `main`. Prefer **rebasing** (linear history, no merge commits):
   ```bash
   git fetch origin main
   git rebase origin/main              # replay your commits on top of main
   ```
   In an Agent Manager worktree you can equivalently use the extension's **Apply** (selected changes) or **PR** flow — whichever the developer prefers. The rule is: linear history via rebase, not merge commits.

6. **Conflicts** — if the rebase conflicts:
   - Resolve the conflict in **your worktree** (keep the smallest set of conflict-resolution changes).
   - Re-run the quality gates (`npm run lint`, `npm test`, `npm run smoke`).
   - Surface **only** the conflict-resolution diff for one more developer review before finishing.

7. **Commit** — commit per the developer's instruction (they may do it themselves). The worktree and task are then done; clean up the worktree if the developer wants.

## Git rules (in detail)

- **No git for recovery.** Recover lost code from `.history/` (VS Code Local History), never via `git revert`/`reflog`/`reset --hard` unless the developer directs it.
- **Rebase, don't merge.** Linear history is the project convention.
- **Never `git stash` across worktrees.** Stashes are global and leak between worktrees — a stash made in one worktree can clobber another. If you absolutely must stash, it must be a single `git stash` immediately followed by `git stash pop` **in the same worktree**.
- **Staged = reviewed.** See step 4. Don't fight the staging area.
- **Scope.** Only touch files relevant to the current task. Ignore unrelated changes in the working tree unless the developer asks you to coordinate.

## Definition of done

A task is ready to hand off for final review when ALL hold:
- Feature complete for the agreed scope.
- `cargo fmt --all`, `npm run lint`, `npm test`, `npm run smoke` all pass (gates in AGENTS.md).
- Builds warning-free (`npm run lint` enforces `-D warnings`).
- Tests added/updated for behavior changes.
- Contract files in sync if any REST shape changed.
- For UI changes: verified in a browser (Playwright smoke, screenshot, or UI diff), not just eyeballed (see `review-handoff`).
- Docs updated for user-visible changes.

## When to ask vs. proceed

- Ask: scope is ambiguous, a decision reverses a prior one, or a change is risky/irreversible (data migrations, public API removal, `.tasks/` backlog edits).
- Proceed: you have a clear, scoped task and the gates are green. Keep going until the feature is review-ready or you genuinely need input.
