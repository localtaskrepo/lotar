---
name: development-workflow
description: Plan and carry non-trivial implementation through tracking, verification, review, and authorized integration.
---

## Implement

1. Inspect the current checkout and working-tree state. Follow the checkout and
   safety policy in [AGENTS.md](../../../AGENTS.md); no automatic worktree setup.
2. Read the assigned LoTaR task and relevant code. If no task exists, search before
   creating one. Use [lotar-dev-tracking](../lotar-dev-tracking/SKILL.md) for the
   plan, status, acceptance criteria, and progress log.
3. State the outcome, scope, risks, and verification plan. Ask only about decisions
   that materially affect correctness or authorization; an already approved plan
   does not need another approval ceremony. Do not implement a plan-only request.
4. Implement the smallest complete change. Test the relevant behavior as you go,
   recording decisions and milestones rather than each tool call.
5. Before final handoff, satisfy the applicable AGENTS.md gates, reconcile tests
   and docs with behavior, and inspect the final diff for scope and safety.
   Report blockers instead of presenting incomplete verification as success.
6. Reconcile the task's final state under [AGENTS.md](../../../AGENTS.md) and leave
   a concise [handoff](../review-handoff/SKILL.md). Record the outcome, verification,
   remaining work, and commit/integration state separately so completed work does
   not retain a stale in-progress status.

## Integrate only when authorized

For branch work, rebase onto the agreed current main line, resolve conflicts in
the assigned checkout, and rerun affected gates. Surface conflict-resolution
changes for review before integration. Use a fast-forward to preserve linear
history. For work already on main, no branch integration step is needed.

Commit only when requested. After authorized integration/commit, append the commit
reference and any additional verification to the task; ticket completion follows
the handoff policy, not the timing of a commit. Worktree cleanup remains the
developer's decision.
