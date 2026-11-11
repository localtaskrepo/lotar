---
applyTo: "view/**"
excludeAgent: ["code-review"]
---

# Frontend instructions (LoTaR)

## Tech + UI conventions

- Vue 3 + TypeScript + Vite.
- Use Composition API with `<script setup lang="ts">`.
- Prefer existing UI components in `view/components/` (notably the `Ui*` components) and the local class-naming conventions.

## API connectivity

- Use the existing API client: `view/api/client.ts`.
- Use the shared DTOs: `view/api/types.ts` (kept aligned with `src/api_types.rs`).
- Prefer same-origin `/api/*` URLs (the Rust server serves both the SPA and the API).

## Tests + lint

- Typecheck frontend and smoke: `npm run lint:frontend`; lint: `npm run lint:js` (oxlint for TypeScript/JavaScript including `<script>` blocks, ESLint for Vue templates); format: `npm run fmt:js` (Prettier).
- Lint rules are the owner's shared standard (`oxlintrc.standard.json`, a copy: replace it whole when the standard changes; lotar's additions go in `.oxlintrc.json`). Fix findings rather than suppressing them; type data from the server or storage honestly (`unknown` plus a check) instead of `any`.
- Multi-statement inline handlers must be arrow functions (`@click="() => { a(); b() }"`): Prettier drops semicolons, and Vue cannot split statements without them.
- UI unit tests: `npm run test:ui` (see the `testing-strategy` skill for targeting).

## Change discipline

- Stay within the task's scope; don't refactor unrelated UI.
- If changing UX/behavior, add/update Vitest coverage where it fits, and verify visually (Playwright smoke / vision tools — see `review-handoff`) rather than eyeballing.
- Attach cropped result snippets to tickets that change how the UI looks (`scripts/ticket-screenshot.mjs`, see `review-handoff`).
