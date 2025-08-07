# Git Hooks Setup

To enable automatic code quality checks and fixes:

```bash
git config core.hooksPath .githooks
```

## What the hooks do:

**Pre-commit hook:**
- ✅ Auto-fixes code formatting (`cargo fmt`)
- ✅ Auto-fixes clippy issues where possible
- ❌ Blocks commits for: compilation errors, unfixable lints, test failures

**Pre-push hook:**
- ✅ Validates formatting, lints, build, and full test suite
- ❌ Blocks pushes if any quality checks fail

## Bypass (not recommended):
```bash
git commit --no-verify  # Skip pre-commit
git push --no-verify    # Skip pre-push
```
