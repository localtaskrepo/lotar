# Contributing

Thanks for helping improve LoTaR. Everyone taking part is expected to follow the [code of conduct](CODE_OF_CONDUCT.md).

Not sure where to start? Questions and ideas are welcome in [Discussions](https://github.com/localtaskrepo/lotar/discussions), and bugs or concrete proposals in [issues](https://github.com/localtaskrepo/lotar/issues/new/choose).

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
- Optional fast git hooks (formatting and lint checks, no test runs): `npm run hooks:install`. See [.githooks/SETUP.md](.githooks/SETUP.md).

## Local development

Run the server and the UI dev server in two terminals:

```bash
# Terminal 1: the LoTaR server and REST API on port 8080
cargo run -- serve --port 8080

# Terminal 2: the web UI with hot reload; it proxies /api to port 8080
npm run dev
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
- History stays linear: rebase onto `main` rather than merging it into your branch.
- The project's own backlog lives in [`.tasks/DEV/`](.tasks/DEV). Coding agents working in this repo follow [AGENTS.md](AGENTS.md).
