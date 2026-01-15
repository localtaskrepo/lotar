# Git Hooks Setup

To enable automatic code quality checks and fixes:

```bash
git config core.hooksPath .githooks
```

## What the hooks do:

**Pre-commit hook:**
- ✅ Fast: runs `cargo fmt --all`
- ✅ Safe staging: re-stages only the Rust files that were already staged (preserves partial staging)
- ❌ Blocks commits only if it cannot re-apply your unstaged changes cleanly

**Pre-push hook:**
- ✅ Validates formatting, lints, build, and full test suite
- ❌ Blocks pushes if any quality checks fail

## Bypass (not recommended):
```bash
git commit --no-verify  # Skip pre-commit
git push --no-verify    # Skip pre-push
```
