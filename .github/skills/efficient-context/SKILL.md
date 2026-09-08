---
name: efficient-context
description: Reduce repeated exploration and context churn; use for cache-aware sessions, delegation, or context-pressure decisions.
---

## Optimize the completed task

Prefer a correct, verified result over fewer tokens or tool calls in isolation.
Use current capabilities and measured costs; do not encode today's model rankings,
quota windows, context limits, or latency assumptions in repository policy.

Start with the assigned LoTaR task, relevant guidance, and current code. Search by
known symbol/path first; use semantic search for an unknown location. Read a
coherent relevant section rather than repeatedly sampling tiny slices or dumping
entire generated files. Keep full logs as local artifacts and report the failing
command and diagnostic excerpt. Use the existing low-noise test scripts.

## Preserve useful context

- Keep stable instructions in versioned files and changing task state in LoTaR.
  Avoid rewriting startup instructions with dates, progress, quotas, or histories.
- Reuse relevant context already loaded; verify changed files rather than reread
  everything. Prior memory is a pointer, not a substitute for current state.
- Continue a coherent task in its existing session, appending new findings.
  Avoid repeated model/effort/tool-set changes solely as a token-saving ritual.
- Compact when context pressure, irrelevance, or the harness requires it. At a
  real task boundary or harness switch, use the
  [handoff](../review-handoff/SKILL.md). Do not preserve an unhelpful long history
  just for a high cache-hit ratio, or compact repeatedly at arbitrary turn counts.
- Make necessary instruction corrections once at a meaningful checkpoint, rather
  than rewriting the prefix every turn. Correctness takes priority over cache reuse.

## Delegate only when it earns its cost

Respect explicit no-subagent requests and provider availability. Do not launch a
worker, model comparison, or quota probe when that route is unavailable. Ordinary
internal delegation does not authorize new visible sessions or worktrees.

For permitted workers, give one bounded outcome, owned paths, read-only/edit scope,
acceptance criteria, and verification command. Keep the reusable role instructions
stable and put the assignment after them. Supply relevant context, not the whole
conversation; do not repeat the worker's investigation in the parent.

Resume the same worker for follow-ups to the same assignment when supported. A
new independent reviewer may need fresh context to avoid anchoring; do not trade
review independence or necessary permissions for potential cache reuse. Report
findings/results, paths, checks, and unresolved risks rather than a transcript.
One coordinator records shared LoTaR updates.

## Cache boundaries: what agents can and cannot control

Provider caching reuses eligible input prefixes, not previous answers. Identical
wording helps, but reuse also depends on model, provider, routing, retention,
breakpoints, tool schemas, and the harness's actual request construction. Parent
and worker sessions do not necessarily share a cache, even on the same model.

When configuring a harness (only when requested):

- Put stable role instructions and tools before dynamic assignments, and keep
  their ordering stable. Choose the minimal adequate tool set at session start;
  use supported deferred tool discovery rather than continually replacing tools.
- Let the harness preserve conversation and required reasoning/tool-call state.
  Do not strip protocol fields, change roles of untrusted content, or weaken
  permissions to make requests look cacheable.
- Check the current provider protocol before adding cache keys, retention, or
  effort settings. API options are not automatically harness settings, and OAuth
  subscription routes may expose different controls/accounting from the API.
- A common prefix alone may be insufficient: some protocols need a breakpoint
  at that prefix to reuse it across different worker assignments. Confirm harness
  support and usage telemetry before promising cross-worker savings.
- Do not pad prompts or send warm-up/keepalive requests just to chase cache hits.
  Compare total cost per accepted change, including cache writes, uncached input,
  output/reasoning, retries, and summarization. Cached tokens may still count
  toward limits, and a high hit rate does not establish subscription savings.

If telemetry is available, record aggregated input/cached/output counts, reported
cost or allowance change, latency, and correction rounds for comparable tasks.
Keep account identifiers and raw request payloads out of tickets. If unavailable,
report savings as unmeasured. Re-evaluate on model/harness changes or a regression,
not every coding turn; faster and cheaper models may justify a different balance.

## References

Consult only when changing harness/cache configuration; these are not startup reads:

- [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)
- [OpenAI model and prompting guidance](https://developers.openai.com/api/docs/guides/latest-model)
- [Z.ai context caching](https://docs.z.ai/guides/capabilities/cache)
- [Agent Skills progressive disclosure](https://agentskills.io/specification)
