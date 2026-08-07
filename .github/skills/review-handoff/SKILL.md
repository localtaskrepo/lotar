---
name: review-handoff
description: Use this when finishing a task to produce a clean, consistent handoff for developer review and to verify UI changes with browser/vision tools instead of eyeballing.
---

## Handoff summary (give this to the developer)

End every task with a concise, scannable summary so review is fast and parallelizable:

- **What changed** — 1–3 sentences on the intent.
- **Files** — grouped by kind (Rust, UI, config, docs, tests), with the role of each non-obvious file.
- **How verified** — which gates ran and passed (`cargo fmt --all`, `npm run lint`, `npm test`, `npm run smoke`), plus any manual/browser/vision verification and its result.
- **Tests** — new/updated tests and what they cover.
- **Contract/migration impact** — REST shape changes (were `src/api_types.rs` / `view/api/types.ts` / `docs/openapi.json` synced?), storage/YAML migrations, config changes.
- **Open risks / follow-ups** — anything deferred, uncertain, or worth a second look.

Keep it to what a reviewer needs; link to the plan doc/LoTaR task if there is one.

## Verify UI changes in a browser, not by inspection

When a change affects the web UI, **don't** claim it works based only on reading the code. Prefer, in roughly this order:

1. **Playwright smoke** — add or extend a smoke test under `smoke/tests/` and run it (`npm run test:smoke:quick -- -t "<name>"`, or full `npm run smoke`). This is the durable, automated proof. See `smoke-suite-debugging`.
2. **Vision MCP for quick visual checks** — use the available vision tools when you need to confirm rendering, layout, or visual regressions:
   - `ui_diff_check` — compare an expected/reference screenshot against the actual rendered UI (best for "does this match the design / did I regress it").
   - `analyze_image` / screenshots — inspect a rendered page you capture.
   - `extract_text_from_screenshot` — pull text out of a screenshot (handy for terminal/CLI output or verifying UI labels).
   - `diagnose_error_screenshot` — when an error appears on screen.
3. **Manual serve** (last resort) — `lotar serve --port 8080` + `npm run dev` for poking around; document what you did rather than leaving it implicit.

> For CLI/server output, screenshot OCR (`extract_text_from_screenshot`) or `npm run` output is fine; don't paste secrets/PII into any captured image.

## Review-ready checklist (definition of done)

(See `development-workflow` "Definition of done" — gates green, tests added, contracts synced, UI browser-verified, docs updated.) Don't ask for review until every box is checked; a half-done handoff wastes a review round.
