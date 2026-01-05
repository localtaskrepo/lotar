---
applyTo: "view/**"
excludeAgent: ["code-review"]
---

# Frontend instructions (LoTaR)

## Tech + UI conventions

- Vue 3 + TypeScript + Vite.
- Use Composition API with `<script setup lang="ts">`.
- Prefer existing UI components in `view/components/` (notably `Ui*` components) and local class naming conventions.

## API connectivity

- Use the existing API client: `view/api/client.ts`.
- Use the shared DTOs: `view/api/types.ts` (kept aligned with `src/api_types.rs`).
- Prefer same-origin `/api/*` URLs (the Rust server serves both the SPA and API).

## Tests + lint (use repo scripts)

- Typecheck/lint frontend: `npm run lint:frontend` (`vue-tsc --noEmit`)
- UI unit tests: `npm run test:ui` (Vitest)

## Change discipline

- Stay within the task’s scope; don’t refactor unrelated UI.
- If changing UX/behavior, add/update Vitest coverage where it fits.
