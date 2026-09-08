---
name: skill-discovery
description: Troubleshoot missing skill discovery or adapt instruction loading to a different agent harness.
---

Use the routing map in [AGENTS.md](../../../AGENTS.md) for ordinary work; this
skill is only for loading problems, not a mandatory startup step.

- Copilot supports `.github/skills` and applies `.github/instructions` according
  to `applyTo`. Its thin overlay adds no separate policy.
- For another harness, verify its current discovery rules. Point its supported
  skill directory/configuration at the existing sources using personal settings.
  Reading `SKILL.md` directly is always a fallback. Read applicable path-scoped
  instruction files explicitly when not auto-loaded.
- Keep one authoritative copy of each rule. Do not install both a plugin and
  editable copies of the same skill, or globally inject the full skill directory.

Validate with one matching task and one unrelated task: the required guidance
should be discoverable for the first and stay unloaded for the second. Record the
observed harness/version and result in LoTaR, not in the shared startup policy.
