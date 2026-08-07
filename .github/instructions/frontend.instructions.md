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

- Typecheck/lint frontend: `npm run lint:frontend`.
- UI unit tests: `npm run test:ui` (see the `testing-strategy` skill for targeting).

## Change discipline

- Stay within the task's scope; don't refactor unrelated UI.
- If changing UX/behavior, add/update Vitest coverage where it fits, and verify visually (Playwright smoke / vision tools — see `review-handoff`) rather than eyeballing.
