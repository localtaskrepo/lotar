# Contributing

Thanks for helping improve LoTaR.

## Quick dev setup

Prereqs:

- Rust (stable toolchain)
- Node.js (see `package.json` engines)

Install and validate:

```bash
npm ci
npm run lint
npm test
npm run smoke
```

Notes:

- Use `cargo nextest` via `npm test` / `npm run test:rust` (legacy `cargo test` is intentionally discouraged here).
- The web UI is under `view/` and is bundled into `target/web`.

## Local development

```bash
npm run dev
cargo run -- serve --port 8080
```

## API contract changes

If you change any REST request/response shapes, keep these in sync:

- `src/api_types.rs`
- `view/api/types.ts`
- `docs/openapi.json`

## Docs

- User-facing docs: `docs/help/`
- Developer docs: `docs/developers/`

## Submitting changes

- Prefer small, focused PRs.
- Include tests for behavior changes.
- Update docs alongside user-facing changes.
