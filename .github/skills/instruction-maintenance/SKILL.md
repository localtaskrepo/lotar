---
name: instruction-maintenance
description: Repair proven guidance errors or missing conventions during authorized implementation; validate instruction changes without growing a diary.
---

## Trigger and scope

Use this when a developer correction, failed documented command, conflicting rule,
or reproducible missing convention would mislead the next agent. Mere difficulty,
one model's stylistic preference, or a speculative optimization is not evidence.
Read-only tasks report findings without edits or task mutations.

## Evidence, correction, check

1. **Capture evidence in the current LoTaR task.** Name the triggering request,
   instruction/path, actual failure, and expected behavior. Redact sensitive data.
   Separate a stale command or rule from a tool/environment limitation.
2. **Locate the owner.** Shared safety/workflow policy belongs in AGENTS.md,
   per-area conventions in `.github/instructions`, procedures in the matching
   skill, and product usage in `docs/help`. Keep non-GitHub harness configuration
   and model/effort/quota preferences in personal settings. Search for existing
   guidance and contradictory copies before adding anything.
3. **Make the smallest supported correction.** During authorized implementation,
   fix factual errors and clarify already established conventions. Ask before
   changing safety, review, or quality policy. Do not edit staged/approved files
   without permission. Preserve uncertain findings as task follow-ups.
4. **Check the trigger and a counterexample.** Replay the failed command safely or
   compare the documented procedure with the current implementation. For behavior
   rules, walk through the original scenario and a nearby case that must remain
   unchanged. Check skill frontmatter, relative links, and `git diff --check`.
   Test executable examples in isolated workspaces, never the DEV backlog.
5. **Record and stop.** Add one task comment with evidence, changed path, check
   result, and any limitation. Report the correction at handoff. Do not recursively
   optimize unrelated instructions or require a new lesson after every task.

Use this compact evidence record:

```text
Trigger: <observed request/failure and source>
Correction: <path and smallest change; why reusable>
Check: <command/scenario and result; counterexample preserved>
Limit: <not runtime-tested, unavailable harness, or none>
```

## Write for the next agent

- Give each rule one authoritative home. Keep critical guardrails visible; place
  branch-specific reference behind a precise trigger. Avoid discovery chains.
- Describe the outcome, constraints, and observable completion criterion. Use
  familiar vocabulary and concise positive actions; retain explicit prohibitions
  for safety boundaries. Avoid generic exhortations to be smart or thorough.
- Prefer a tested command for a hard-to-discover gotcha; otherwise link to the
  source of truth instead of caching an entire command catalog or directory map.
- Add a new skill only for a distinct reusable procedure. Use standard `name` and
  `description` frontmatter with a narrow trigger and a Markdown-read fallback.
  No model names, mandatory delegation, or harness-specific tool names in shared
  steps when a capability description will do.
- For external skills, inspect their dependencies, side effects, autonomy gates,
  license, and scope before adoption. Adapt techniques into existing skills where
  possible; record the upstream revision and preserve required notices. Updates
  are reviewed changes, not unattended replacements of project policy.

## Verify the loop over time

Before adding another rule for a previously recorded problem, read its LoTaR
evidence and check the original correction. On the next matching task, record
whether it prevented recurrence. If not, refine or remove the ineffective rule;
do not stack synonyms. A scenario walkthrough validates intent, not future model
compliance: distinguish it from an actual agent/harness replay.

Useful counterexamples: a read-only audit stays read-only; a main-checkout task
does not create a worktree; an explicit no-subagent request stays single-agent;
a typo fix does not initiate a full product workflow; instruction tuning cannot
authorize commits or remove required verification.

No durable finding is a valid outcome. Revisit model-specific workarounds when
upgrading the harness/model or when they fail, rather than preserving restrictions
from older agents indefinitely. Keep the evidence in LoTaR, not AGENTS.md.

Writing techniques adapted from AI Hero's `writing-for-agents`; see
[sources and license](../THIRD_PARTY_NOTICES).
