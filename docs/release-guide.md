# Release & Verification Guide

LoTaR ships signed binaries, `.deb` and `.rpm` packages, npm packages, a Docker image, a
Homebrew tap (macOS and Linux), a Scoop bucket, and install scripts. This guide summarizes the release
automation and the manual checks to run after every tag.

## Automation Overview

- **GitHub workflow**: `.github/workflows/release.yml` runs whenever a tag that starts with
  `v` lands on `main` (for example `v0.6.3`).
- **Artifacts produced**:
  - Signed tarballs/zips for every supported target, attached to the GitHub Release page.
  - Signed `.deb` and `.rpm` packages for x64 and arm64, built with nfpm from the static
    musl binaries (`packaging/nfpm.yaml`), attached to the same release.
  - npm packages: `@localtaskrepo/lotar` (a small Node launcher that installs the `lotar` command) and one `@localtaskrepo/lotar-<platform>`
    package per binary, assembled by `scripts/npm-packages.mjs` from the release archives.
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

## One-Time npm Setup

`publish-npm` uses npm trusted publishing: GitHub Actions proves its identity to npm, so no
token is stored. npm only allows that for packages that already exist, and tokens that bypass
two-factor authentication can neither configure it nor (from January 2027) publish directly.
So the first version of each package is published by hand from a maintainer's machine:

1. On npmjs.com, create the free organization `localtaskrepo` (it owns the
   `@localtaskrepo/lotar-*` platform packages). Two-factor authentication must be on for the
   account.
2. Build the packages from a published release and publish them, platform packages first:

   ```bash
   TAG=v0.11.1
   mkdir -p npm-bootstrap/downloads && cd npm-bootstrap
   for p in macos-arm64 macos-x64 linux-musl-arm64 linux-musl-x64 windows-x64; do
     ext=tar.gz; [ "$p" = windows-x64 ] && ext=zip
     gh release download "$TAG" --repo localtaskrepo/lotar --dir downloads \
       --pattern "lotar-$TAG-$p.$ext" --pattern "lotar-$TAG-$p.sha256"
   done
   node ../scripts/npm-packages.mjs --tag "$TAG" --archives downloads --out dist
   npm login
   for d in lotar-darwin-arm64 lotar-darwin-x64 lotar-linux-arm64 lotar-linux-x64 lotar-win32-x64 lotar; do
     (cd "dist/$d" && npm publish --access public) || break
   done
   ```

   Each publish asks for two-factor authentication; the npm website offers to skip it for
   the next five minutes. Versions published this way have no provenance statement; later
   releases from the workflow do.
3. Shortly before tagging the next release, connect every package to the release workflow.
   npm expires a trusted publishing configuration that has not published within 48 hours,
   so do not do this days ahead:

   ```bash
   for p in @localtaskrepo/lotar-darwin-arm64 @localtaskrepo/lotar-darwin-x64 \
            @localtaskrepo/lotar-linux-arm64 @localtaskrepo/lotar-linux-x64 \
            @localtaskrepo/lotar-win32-x64 @localtaskrepo/lotar; do
     npm trust github "$p" --repo localtaskrepo/lotar --file release.yml --allow-publish --yes
     sleep 2
   done
   ```

   This needs npm 11.15.0 or newer and an interactive login (not a token). The same settings
   are under **Settings → Trusted publishing** for each package on npmjs.com.
4. Optionally select **Require two-factor authentication and disallow tokens** for each
   package once its first workflow publish succeeds.

The job needs npm 11.5.1 or newer for trusted publishing and installs it if the runner's
Node.js bundles an older npm. If a publish fails with `E404`, the package has no trusted
publisher matching this repository and `release.yml`, or its configuration expired; check
`npm trust list <package>`.

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
   - `publish-npm`: publishes the platform packages, then `@localtaskrepo/lotar`, with npm provenance.
     Versions that are already on npm are skipped, so the job can be re-run. Prerelease
     tags (`v1.0.0-rc.1`) publish under the `next` dist-tag instead of `latest`.
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

### Install scripts and packages

Workflow: **Verify Install Scripts and Packages** (`.github/workflows/install-verify.yml`).

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
4. Installs the `lotar` npm package globally on Linux (x64, arm64), macOS (Apple Silicon,
   Intel), and Windows, checks the version and that only the matching platform package was
   installed, and runs it once with `npx`.

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
  settings before retrying. `publish-npm` needs no secret once trusted publishing is set up.

With the release workflow and the three manual verification jobs, you can be confident the
published artifacts install correctly on macOS, Linux, and Windows before announcing the
release.
