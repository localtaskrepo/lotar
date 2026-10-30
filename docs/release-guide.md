# Release & Verification Guide

LoTaR ships signed binaries, `.deb` and `.rpm` packages, a Docker image, a Homebrew tap
(macOS and Linux), a Scoop bucket, and install scripts. This guide summarizes the release
automation and the manual checks to run after every tag.

## Automation Overview

- **GitHub workflow**: `.github/workflows/release.yml` runs whenever a tag that starts with
  `v` lands on `main` (for example `v0.6.3`).
- **Artifacts produced**:
  - Signed tarballs/zips for every supported target, attached to the GitHub Release page.
  - Signed `.deb` and `.rpm` packages for x64 and arm64, built with nfpm from the static
    musl binaries (`packaging/nfpm.yaml`), attached to the same release.
  - Docker image `mallox/lotar:<version>` built via Buildx.
  - Homebrew tap update pushed to `localtaskrepo/homebrew-lotar`. The formula installs the
    macOS builds on macOS and the static musl builds on Linux.
  - Scoop manifest update pushed to `localtaskrepo/scoop-lotar`.
- **Install scripts**: `site/install.sh` (Linux, macOS) and `site/install.ps1` (Windows)
  are published with the homepage at `https://localtaskrepo.github.io/lotar/` by the
  Homepage workflow when they change on `main`, not by the release. They look up the
  latest release when run, so a release needs no script change, but they depend on the
  release asset names (`lotar-<tag>-<platform>.tar.gz|zip` plus `.sha256`).
- **Quick-links**: The workflow edits the release notes to include clickable download
  shortcuts for macOS, Linux, and Windows assets, the Linux packages, and the install
  script commands.
- **Commit list**: The custom `Changes:` list shows each commit's subject line,
  grouped by the type of the first ticket it names: Features, then Bug fixes,
  Chores, and Other changes (commits without a ticket, or with another type).
  Within a group, subjects sort alphabetically with ticket numbers in numeric
  order. Identical subjects (for example follow-up commits that reuse their
  ticket's `DEV-N Title` subject) appear once. If the ticket snapshot cannot be
  read, the list stays one alphabetical list.
- **Feature screenshots**: Under each feature, the page shows up to two image
  attachments of its ticket (twelve per release), read from the release's commit,
  so the tracked attachments folder (`.tasks/DEV/@attachments`) is the source.
  Images named `*-2x.*` (scale-2 captures from `scripts/ticket-screenshot.mjs`)
  show at half their pixel width; all are capped at 720 px.
- **Ticket links**: Known ticket IDs in the custom `Changes:` commit list link to
  their tracked YAML file on GitHub at the release's immutable commit, not `main`
  or a local LoTaR server. For example, `DEV-129` links to `.tasks/DEV/129.yml`
  as it existed for that release. Unknown, missing, or ambiguous IDs remain plain
  text. Existing links, URLs, and inline code are not nested inside ticket links.
  If the optional ticket snapshot cannot be read safely, the job warns and keeps
  the commit list without ticket links; download links and other release text
  are still maintained. Existing releases are not edited automatically.
  Re-running a release job that already contains this renderer is idempotent;
  re-running an older workflow definition does not add the new linking code.

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
   - `package-linux`: builds `.deb`/`.rpm` packages from the uploaded musl archives,
     signs them, and uploads them. Homebrew, Scoop, and Docker do not wait for it.
   - `update-release-page`: injects quick download links and snapshot-linked
     ticket references in the custom commit list.
   - `publish-docker-image`: builds/pushes the Docker image to Docker Hub.
   - `update-homebrew-tap`: refreshes the tap formula checksums and pushes to
     `homebrew-lotar` (uses `HOMEBREW_TAP_TOKEN`).
   - `update-scoop-bucket`: updates `scoop-lotar/bucket/lotar.json` with the new URLs and
     checksums (uses the same token).
3. Make sure the release is marked as **latest** on GitHub once the workflow finishes.

## Post-Release Verification

While the release workflow performs automated updates, we keep three manual verification
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
1. Runs on `macos-15-intel` (Intel), `macos-15` (Apple Silicon), and `ubuntu-latest`
   (Homebrew on Linux) runners.
2. Untaps and re-taps the requested repository/branch, installs LoTaR via Homebrew, runs
   the formula's `brew test`, then runs `lotar --version` to confirm the installation.
3. Uses `file` to ensure the binary architecture matches the runner.
4. Uploads Homebrew logs, the tapped repo, and the installed Cellar contents for post-run
   inspection.

Kick off this workflow once the release workflow finishes updating the tap so you have a
smoke test for every platform the formula supports.

### Install scripts and Linux packages

Workflow: **Verify Install Scripts and Linux Packages** (`.github/workflows/install-verify.yml`).

Inputs:
- `lotar_version` (optional) release tag to install and assert, e.g. `v0.12.0`. Defaults to
  the latest release.

What it does:
1. Runs `site/install.sh` from the checkout on Ubuntu (x64, arm64), Alpine (BusyBox `sh`
   and `wget`), and macOS (Apple Silicon, Intel), checks the version and binary
   architecture, and reinstalls over the existing binary.
2. Runs `site/install.ps1` piped into `Invoke-Expression`, as documented, under PowerShell
   7 and Windows PowerShell 5.1, and checks the version and the user `PATH` entry.
3. Downloads the `.deb` and `.rpm` packages for x64 and arm64, checks their SHA-256 files,
   and installs, runs, and removes them in Debian and Fedora containers.

Pull requests that change the install scripts run steps 1 and 2 against the latest release.

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

With the release workflow and the three manual verification jobs, you can be confident the
published artifacts install correctly on macOS, Linux, and Windows before announcing the
release.
