# Git Hooks

Opt-in, read-only checks that finish in seconds. CI runs the full test,
build and smoke suites; these hooks only catch the common slips early.

```bash
npm run hooks:install   # sets git config core.hooksPath .githooks
```

**pre-commit** checks only the staged files and never formats, stages or
stashes anything:
- Rust: `rustfmt --check` on staged `.rs` files
- Frontend: Prettier check and ESLint on staged TypeScript/Vue/JS files

Fix formatting with `npm run fmt`, then commit again. The checks read the
working-tree copy; the hook says so when a file is only partially staged.

**pre-push** checks what the push changes, Rust and frontend in parallel:
- Rust files changed: `cargo fmt --check`, clippy on lib and bins (`-D warnings`)
- Frontend files changed: vue-tsc typecheck, ESLint, Prettier check
- Nothing else changed (docs, tasks): no checks

The pre-push checks read the working tree, so commit or stash unrelated
edits first if they would fail on their own.

Bypass once with `git commit --no-verify` or `git push --no-verify`.
