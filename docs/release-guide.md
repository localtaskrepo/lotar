# Release & Verification Guide

LoTaR ships signed binaries, a Docker image, a Homebrew tap, and a Scoop bucket. This
guide summarizes the release automation and the manual checks to run after every tag.

## Automation Overview

- **GitHub workflow**: `.github/workflows/release.yml` runs whenever a tag that starts with
  `v` lands on `main` (for example `v0.6.3`).
- **Artifacts produced**:
  - Signed tarballs/zips for every supported target, attached to the GitHub Release page.
  - Docker image `mallox/lotar:<version>` built via Buildx.
  - Homebrew tap update pushed to `localtaskrepo/homebrew-lotar`.
  - Scoop manifest update pushed to `localtaskrepo/scoop-lotar`.
- **Quick-links**: The workflow edits the release notes to include clickable download
  shortcuts for macOS, Linux, and Windows assets.

## Pre-Release Checklist

1. Update versions in `Cargo.toml`, `package.json`, and any docs that mention the release
   number.
2. Update `CHANGELOG.md` (or the release notes draft) describing the highlights.
3. Run the full quality gates locally:
   ```bash
   cargo fmt --all
   npm run lint
   npm test
   npm run smoke
   ```
4. Commit the changes on `main` and push.

## Cutting a Release

1. Tag the commit on `main`:
   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
2. Monitor the **Release** workflow in GitHub Actions. The important jobs are:
   - `build-and-release`: compiles, signs, and uploads release assets.
   - `update-release-page`: injects quick download links into the release notes.
   - `publish-docker-image`: builds/pushes the Docker image to Docker Hub.
   - `update-homebrew-tap`: refreshes the tap formula checksums and pushes to
     `homebrew-lotar` (uses `HOMEBREW_TAP_TOKEN`).
   - `update-scoop-bucket`: updates `scoop-lotar/bucket/lotar.json` with the new URLs and
     checksums (uses the same token).
3. Make sure the release is marked as **latest** on GitHub once the workflow finishes.

## Post-Release Verification

While the release workflow performs automated updates, we keep two manual verification
workflows handy so we can validate installs on real runners before announcing the release.

### Windows: Verify Scoop Install

Workflow: **Verify Scoop Install** (`.github/workflows/scoop-verify.yml`).

Inputs:
- `bucket_repo` (defaults to `localtaskrepo/scoop-lotar`).
- `bucket_branch` (defaults to `main`).
- `lotar_version` (optional). When supplied, the workflow ensures `lotar --version`
  contains that string.

What it does:
1. Checks out the Scoop bucket for reference.
2. Installs Scoop, adds the bucket, and installs LoTaR (pinning the requested version if
   provided).
3. Runs `lotar --version`, verifies the output, and uploads the Scoop logs plus the
   installed app directory.

Use this workflow after every release (and whenever you tweak the Scoop manifest) to
confirm the bucket stays healthy.

### macOS: Verify Homebrew Install

Workflow: **Verify Homebrew Install** (`.github/workflows/homebrew-verify.yml`).

Inputs:
- `tap_repo` (defaults to `localtaskrepo/homebrew-lotar`).
- `tap_branch` (defaults to `main`).
- `lotar_version` (optional) to assert a specific version string.

What it does:
1. Runs on both `macos-13` (Intel) and `macos-14` (Apple Silicon) runners.
2. Untaps and re-taps the requested repository/branch, installs LoTaR via Homebrew, then
   runs `lotar --version` to confirm the installation.
3. Uses `file` to ensure the binary architecture matches the runner.
4. Uploads Homebrew logs, the tapped repo, and the installed Cellar contents for post-run
   inspection.

Kick off this workflow once the release workflow finishes updating the tap so you have a
smoke test for both architectures.

## Troubleshooting Tips

- **Homebrew tap issues**: If the tap workflow fails while cloning or tapping, run the
  manual workflow above and inspect the uploaded logs. You can also `brew tap --repair
  localtaskrepo/lotar` locally to ensure the tap is healthy.
- **Scoop manifest issues**: The released manifest lives in
  `https://github.com/localtaskrepo/scoop-lotar`. When adjusting the JSON structure,
  re-run the manual Scoop workflow to confirm the package can still be installed.
- **Release workflow failures**: Re-run the failed job from the Actions tab. Make sure the
  required secrets (`HOMEBREW_TAP_TOKEN`, Docker Hub creds) exist in the repository
  settings before retrying.

With the release workflow and the two manual verification jobs, you can be confident the
published artifacts install correctly on macOS, Linux, and Windows before announcing the
release.
