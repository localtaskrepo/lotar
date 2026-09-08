---
name: review-handoff
description: Review changes, verify UI behavior, or leave a concise handoff for developer review or a model/harness transition.
---

## Review

For review-only requests, inspect without edits or task mutations. Prioritize
correctness, regressions, compatibility, and missing behavior coverage; give
findings with paths/lines and evidence, or explicitly state no findings. Distinguish
confirmed failures from risks; do not spend a review redoing formatter checks.

## Verify changed UI behavior

Use the available browser to exercise the affected flow, preferably with a durable
Playwright smoke test. For visual changes, inspect a rendered screenshot or compare
it to the reference using available image tools. Text output is preferable to
screenshot OCR for CLI/server diagnostics. Use isolated smoke workspaces or seeded
local scenario data; do not mutate the tracked DEV backlog to test the UI.

If browser or vision tools are unavailable, report the limitation and the evidence
you do have. Static code inspection alone does not establish working UI behavior.
See [testing-strategy](../testing-strategy/SKILL.md) and, for environment failures,
[smoke-suite-debugging](../smoke-suite-debugging/SKILL.md).

## Handoff

Summarize the outcome and verification at the level the task needs. For non-trivial
implementation, record resumable state in the associated LoTaR task and give its ID
to the developer or next agent. Include only relevant fields:

```text
Task / checkout: <ID, branch/path, dirty/staged state>
Outcome / decisions: <implemented scope, invariants, reason for key choice>
Evidence: <important paths/symbols; checks run, results and artifact paths>
Remaining: <next exact action, blocker, failed approach not worth repeating>
Constraints: <authorization, ownership, unavailable tools/providers>
Guidance: <correction and evidence, or no durable finding>
```

Reference existing specs, code, diffs, and task comments instead of copying them.
Do not create another handoff document when the task already holds the state.
Keep session-specific availability in the handoff, not permanent project policy.
The receiving agent verifies current checkout/task state before continuing.

Use configured review-ready status only after applicable AGENTS.md checks; an
interrupted handoff may remain in progress or blocked with missing checks named.
Completion of editing is not permission to commit or mark the task shipped.

Handoff techniques adapted from AI Hero's `handoff`; see
[sources and license](../THIRD_PARTY_NOTICES).
