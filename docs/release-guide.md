# Release & Verification Guide

LoTaR ships signed binaries, `.deb` and `.rpm` packages, npm packages, crates on crates.io, a Docker image, a
Homebrew tap (macOS and Linux), a Scoop bucket, and install scripts. This guide summarizes the release
automation and the manual checks to run after every tag.

## Automation Overview

- **GitHub workflow**: `.github/workflows/release.yml` runs whenever a tag that starts with
  `v` lands on `main` (for example `v0.6.3`).
- **Artifacts produced**:
  - Signed tarballs/zips for every supported target, attached to the GitHub Release page.
    The macOS binaries inside are also signed with the owner's Developer ID and notarized by
    Apple; see [macOS Signing](#macos-signing).
  - Signed `.deb` and `.rpm` packages for x64 and arm64, plus a `.deb` for armv7 (armhf), built with nfpm from the static
    musl binaries (`packaging/nfpm.yaml`), attached to the same release.
  - npm packages: `@localtaskrepo/lotar` (a small Node launcher that installs the `lotar` command) and one `@localtaskrepo/lotar-<platform>`
    package per binary, assembled by `scripts/npm-packages.mjs` from the release archives.
  - Crates on crates.io: `lotar` (the binary, `cargo install lotar`), `lotar-lib` and `lotar-web-assets`.
  - Docker image `mallox/lotar:<version>` built via Buildx, also tagged `latest` (or `next`
    for a prerelease).
  - Homebrew tap update pushed to `localtaskrepo/homebrew-lotar`. The formula installs the
    macOS builds on macOS and the static musl builds on Linux.
  - Scoop manifest update pushed to `localtaskrepo/scoop-lotar`.
  - Prereleases get less; see [Prereleases](#prereleases).
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

## Tickets waiting for a release

Some tickets can only be checked once their fix is in an installed release (for example a
change to how the agent worker restarts after an upgrade). The next release has a ticket,
`Release vX.Y.Z` (for example DEV-369), and those tickets wait for it (owner 2026-11-11):

1. After a release ships, create the next one's ticket: `lotar add "Release vX.Y.Z" --type
   Chore --assignee mallox` with these steps as its description. Its title must start with
   `Release v`.
2. A ticket that waits sets its status to `Blocked` and depends on that ticket:
   `lotar task relationships add DEV-N DEV-R --kind depends-on`. Its comments say what to check.
3. The last step of a release, after `brew upgrade lotar` on this Mac
   ([Cutting a Release](#cutting-a-release), step 4), closes the release ticket with
   `lotar status DEV-R Done`.

Closing it fires the `unblocked` event ([When prerequisites are done](help/automation.md#when-prerequisites-are-done))
on every ticket whose last open prerequisite it was. The rule "Prerequisites done: continue the waiting
ticket" in `.tasks/DEV/automation.yml` assigns each one that is still `Blocked` to `@fix`, as the next turn
of its last T3 Code thread, with a prompt to run the check against the installed lotar. A passing check
ends with the ticket `Done`; a failing one comes back to mallox as `HelpNeeded`. The same rule continues
tickets that waited for other tickets. To start one ticket's check by hand:
`lotar automation run "Prerequisites done: continue the waiting ticket" --select 'id:/^DEV-N$/'`.

## Release records

lotar's own [releases](help/releases.md) (`.tasks/@releases/`, tracked in git since 2026-11-14, owner
"Track and use now") list the DEV tickets each version ships; 0.14.0 is the first one recorded. The
release ticket is in its own release.

- With the next release's ticket, create its record: `lotar release create X.Y.Z`, then
  `lotar release add X.Y.Z DEV-R`.
- A ticket planned for it, or merged on `main` for it: `lotar release add X.Y.Z DEV-N`. Before the
  bump, add the tickets whose commits are on `main` since the last tag and are in no release:
  `git log --format=%s vA.B.C..main | grep -oE '^DEV-[0-9]+' | sort -u`, then
  `lotar list 'release:none'` to cross-check.
- `lotar list 'release:X.Y.Z -status:Done'` is what is still open. A ticket that won't make it
  moves on: `lotar release move X.Y.(Z+1) DEV-N`.
- After the tag is pushed: `lotar release release X.Y.Z`.

## Pre-Release Checklist

1. Update the version under `[workspace.package]` in the root `Cargo.toml`, which every
   workspace package inherits, and the exact versions of `lotar-lib` and `lotar-web-assets`
   under `[workspace.dependencies]` (a mismatch fails the build), plus `Cargo.lock`, which the
   next `cargo build` updates, and any docs that mention the release number. The root `package.json` keeps its own workspace
   version; the npm packages take theirs from the tag. The owner picks the bump:
   a patch by default ("stay on version 0.13.x until we add a major feature or something breaks
   compatibility", 2026-11-11), a minor when he asks for one (v0.14.0: "create a release now with
   a minor bump", 2026-11-12). The release notes are written by the workflow from the commit list
   ([Automation Overview](#automation-overview)).
2. Run the full quality gates locally:
   ```bash
   npm run lint
   npm test
   npm run smoke
   ```
3. Commit the changes on `main` and push.
4. Push the tag only after the Quality Assurance run for that commit has succeeded: the release job checks it
   ("Ensure QA succeeded") and fails, publishing nothing, while QA is still running (v0.13.0, 2026-11-11). Then
   `git tag -s vX.Y.Z -m vX.Y.Z` and `git push origin vX.Y.Z`.

## macOS Signing

`sign-macos` signs the binary in each macOS archive with the Developer ID Application
certificate `Developer ID Application: Ravi Gairola (3HKD37M482)`, under the fixed
identifier `net.pyxzl.lotar`, with the hardened runtime and a secure timestamp. It then has
Apple notarize it. macOS ties firewall and file-access (TCC, Dropbox) grants to the
signature's team ID and identifier, so with this signature an upgraded `lotar` keeps the
grants of the version before. With the linker's ad-hoc signature every release was a new
program to macOS, which asked again (DEV-482). Never change the identifier: users would
lose their grants once more.

A bare binary can't be stapled, so the notarization ticket is not in the archive; Gatekeeper
looks it up online the first time the binary runs.

The job fails when a secret is missing, so a release never ships unsigned macOS archives.

### Secrets

Set in the repository's **Settings → Secrets and variables → Actions**:

| Secret | Contents | Where it comes from |
| --- | --- | --- |
| `MACOS_DEVELOPER_ID_P12_BASE64` | The Developer ID Application certificate with its private key, as a base64-encoded `.p12` | Keychain Access → My Certificates → the Developer ID Application certificate → Export as `.p12`, then `base64 -i developer-id.p12 \| pbcopy` |
| `MACOS_DEVELOPER_ID_P12_PASSWORD` | The password set during that export | Chosen at export |
| `APPLE_NOTARY_KEY_P8_BASE64` | An App Store Connect API key (`AuthKey_<id>.p8`), base64-encoded | App Store Connect → Users and Access → Integrations → Team Keys → generate a key with the Developer role (the `.p8` downloads only once), then `base64 -i AuthKey_<id>.p8 \| pbcopy` |
| `APPLE_NOTARY_KEY_ID` | That key's Key ID | The Team Keys list |
| `APPLE_NOTARY_ISSUER_ID` | The team's Issuer ID | Shown above the Team Keys list |

The job imports the certificate into a temporary keychain and deletes it when it ends, even
after a failure.

### Renewing the certificate

A Developer ID Application certificate is valid for five years. Releases signed before it
expires keep working afterwards, because the signature carries a secure timestamp. Before it
expires:

1. In the Apple Developer account, under Certificates, create a new Developer ID Application
   certificate (it needs the Account Holder role), and install it in the login keychain.
2. Export it with its private key as a `.p12` and replace `MACOS_DEVELOPER_ID_P12_BASE64` and
   `MACOS_DEVELOPER_ID_P12_PASSWORD`.

The team ID and identifier stay the same, so the grants users gave survive the renewal. The
notarization API key does not expire; replace its three secrets if it is revoked.

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
   for p in macos-arm64 macos-x64 linux-musl-arm64 linux-musl-armv7 linux-musl-x64 windows-x64 windows-arm64; do
     ext=tar.gz; case "$p" in windows-*) ext=zip ;; esac
     gh release download "$TAG" --repo localtaskrepo/lotar --dir downloads \
       --pattern "lotar-$TAG-$p.$ext" --pattern "lotar-$TAG-$p.sha256"
   done
   node ../scripts/npm-packages.mjs --tag "$TAG" --archives downloads --out dist
   npm login
   for d in lotar-darwin-arm64 lotar-darwin-x64 lotar-linux-arm lotar-linux-arm64 lotar-linux-x64 \
            lotar-win32-arm64 lotar-win32-x64 lotar; do
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
            @localtaskrepo/lotar-linux-arm @localtaskrepo/lotar-linux-arm64 @localtaskrepo/lotar-linux-x64 \
            @localtaskrepo/lotar-win32-arm64 @localtaskrepo/lotar-win32-x64 @localtaskrepo/lotar; do
     npm trust github "$p" --repo localtaskrepo/lotar --file release.yml --allow-publish --yes
     sleep 2
   done
   ```

   This needs npm 11.15.0 or newer and an interactive login (not a token). The same settings
   are under **Settings → Trusted publishing** for each package on npmjs.com.
4. Optionally select **Require two-factor authentication and disallow tokens** for each
   package once its first workflow publish succeeds.

### Adding a platform package

A new platform (for example `@localtaskrepo/lotar-win32-arm64` and `@localtaskrepo/lotar-linux-arm`,
added in DEV-309) needs the same one-time setup before the first release that contains it, but
there is no release to build it from yet. Publish an empty placeholder version to create the
package, then connect it:

```bash
npm login
for p in lotar-win32-arm64 lotar-linux-arm; do
  dir=$(mktemp -d)
  cat > "$dir/package.json" <<JSON
{
  "name": "@localtaskrepo/$p",
  "version": "0.0.0",
  "description": "Placeholder; install @localtaskrepo/lotar instead.",
  "license": "MIT",
  "repository": { "type": "git", "url": "git+https://github.com/localtaskrepo/lotar.git" }
}
JSON
  (cd "$dir" && npm publish --access public) || break
done
for p in @localtaskrepo/lotar-win32-arm64 @localtaskrepo/lotar-linux-arm; do
  npm trust github "$p" --repo localtaskrepo/lotar --file release.yml --allow-publish --yes
  sleep 2
done
```

Until the next release publishes the real version, `0.0.0` (which contains no binary) is the only
version, and nothing depends on it: the main package only lists the new packages from the release
that contains them. Do the `npm trust` step shortly before tagging (the 48-hour expiry above
applies). Afterwards, `npm deprecate @localtaskrepo/<package>@0.0.0 "placeholder"` makes installs of
that version warn.

The job needs npm 11.5.1 or newer for trusted publishing and installs it if the runner's
Node.js bundles an older npm. If a publish fails with `E404`, the package has no trusted
publisher matching this repository and `release.yml`, or its configuration expired; check
`npm trust list <package>`.

## Cutting a Release

1. Tag the commit on `main`, signed, once its Quality Assurance run has succeeded:
   ```bash
   git tag -s vX.Y.Z -m vX.Y.Z
   git push origin vX.Y.Z
   ```
2. Monitor the **Release** workflow in GitHub Actions. The important jobs are:
   - `build`: compiles and packages each platform with a read-only token and no
     signing identity, and hands the archives to the next job as workflow artifacts.
   - `sign-macos`: signs and notarizes the macOS binaries with the Developer ID and re-packs
     their archives ([macOS Signing](#macos-signing)). It holds the Apple secrets and runs no
     build code.
   - `build-and-release`: signs the archives with cosign and uploads them to the release.
     It holds the write token and the signing identity and runs no build code. The
     aarch64 cross build is pinned by commit (cross) and digest (`Cross.toml` image).
   - `package-linux`: builds `.deb`/`.rpm` packages from the uploaded musl archives,
     signs them, and uploads them. Homebrew, Scoop, and Docker do not wait for it.
   - `publish-npm`: publishes the platform packages, then `@localtaskrepo/lotar`, with npm provenance.
     Versions that are already on npm are skipped, so the job can be re-run. Prerelease
     tags (`v1.0.0-rc.1`) publish under the `next` dist-tag instead of `latest`.
   - `publish-crates`: publishes `lotar-web-assets`, `lotar-lib` and `lotar` to crates.io
     through trusted publishing in the GitHub `release` environment (restricted to `v*` tags),
     with `id-token: write` and the pinned `rust-lang/crates-io-auth-action`. Both library crates read files outside their folder, which
     `cargo package` leaves out, so `node scripts/stage-crates.mjs` first copies `docs/help`,
     `docs/openapi.json` and `target/web-embed` (after `npm run build:web`) into them. Versions
     already on crates.io are skipped. To check the packages locally:
     `npm run build:web && node scripts/stage-crates.mjs && cargo package --allow-dirty -p lotar-web-assets -p lotar-lib -p lotar`.
   - `update-release-page`: injects quick download links and snapshot-linked
     ticket references in the custom commit list.
   - `publish-docker-image`: builds/pushes the Docker image to Docker Hub as
     `mallox/lotar:<version>` and `mallox/lotar:latest` (`next` for a prerelease).
   - `update-homebrew-tap`: refreshes the tap formula checksums and pushes to
     `homebrew-lotar` (uses `HOMEBREW_TAP_TOKEN`).
   - `update-scoop-bucket`: updates `scoop-lotar/bucket/lotar.json` with the new URLs and
     checksums (uses the same token).
3. Once the workflow finishes, check that GitHub shows a stable release as **Latest** and a
   prerelease as **Pre-release**.
4. On this Mac, `brew upgrade lotar` and check that `lotar --version` prints the new version,
   and that `codesign -dvv /opt/homebrew/opt/lotar/bin/lotar` shows
   `Authority=Developer ID Application: Ravi Gairola (3HKD37M482)`,
   `Identifier=net.pyxzl.lotar` and `TeamIdentifier=3HKD37M482`.
   Then close the release ticket (`lotar status DEV-R Done`): that starts the checks of the
   [tickets waiting for it](#tickets-waiting-for-a-release). Create the next release's ticket.

### Prereleases

A tag with a suffix, such as `v1.0.0-rc.1` or `v1.0.0-beta.2`, is a prerelease. The tag
without its suffix must still match the version in `Cargo.toml`. The workflow uses the same
rule everywhere (a `-` in the tag), so prerelease users opt in and nobody else gets one:

| Channel | Stable tag (`v1.0.0`) | Prerelease tag (`v1.0.0-rc.1`) |
| --- | --- | --- |
| GitHub release | Marked **Latest** | Marked **Pre-release**, never latest |
| Install scripts | Install it by default | Only with `--version v1.0.0-rc.1` (`$env:LOTAR_VERSION` on Windows); the release page shows that command |
| npm | `latest` dist-tag | `next` dist-tag |
| Docker | `1.0.0` and `latest` | `1.0.0-rc.1` and `next` |
| Homebrew tap, Scoop bucket | Updated | Not updated (`update-homebrew-tap` and `update-scoop-bucket` are skipped) |
| crates.io | Published | Not published (`publish-crates` is skipped: `Cargo.toml` has the version without the suffix) |
| `.deb`/`.rpm` packages | Attached to the release | Attached to the release |

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
3. Uses `file` to ensure the binary architecture matches the runner, and on macOS checks that
   the binary carries the Developer ID signature (`Identifier=net.pyxzl.lotar`,
   `TeamIdentifier=3HKD37M482`).
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
   7 and Windows PowerShell 5.1 on Windows x64 and ARM64, and checks the version and the
   user `PATH` entry; manual runs also check that the binary matches the CPU.
3. Downloads the `.deb` and `.rpm` packages for x64 and arm64 and the armv7 `.deb`, checks
   their SHA-256 files, and installs, runs, and removes them in Debian and Fedora containers
   (armv7 under QEMU).
4. Installs the `lotar` npm package globally on Linux (x64, arm64), macOS (Apple Silicon,
   Intel), and Windows (x64, ARM64), checks the version and that only the matching platform
   package was installed, and runs it once with `npx`.
5. Runs `install.sh` (Alpine) and the npm package (Debian) on 32-bit ARM under QEMU.

Pull requests that change the install scripts run steps 1 and 2 against the latest release.

## Troubleshooting Tips

- **Homebrew tap issues**: If the tap workflow fails while cloning or tapping, run the
  manual workflow above and inspect the uploaded logs. You can also `brew tap --repair
  localtaskrepo/lotar` locally to ensure the tap is healthy.
- **Scoop manifest issues**: The released manifest lives in
  `https://github.com/localtaskrepo/scoop-lotar`. When adjusting the JSON structure,
  re-run the manual Scoop workflow to confirm the package can still be installed.
- **macOS signing failures**: `sign-macos` prints the notarization log when Apple rejects a
  binary. To check a downloaded archive's binary by hand: `spctl -a -t install -vv lotar`
  should say `accepted` and `source=Notarized Developer ID`. Use `-t install`: `-t execute`
  rejects any command-line binary that isn't in an app bundle, notarized or not.
- **Release workflow failures**: Re-run the failed job from the Actions tab. Make sure the
  required secrets (`HOMEBREW_TAP_TOKEN`, Docker Hub creds, the [macOS signing
  secrets](#secrets)) exist in the repository
  settings before retrying. `publish-npm` needs no secret once trusted publishing is set up.

With the release workflow and the three manual verification jobs, you can be confident the
published artifacts install correctly on macOS, Linux, and Windows before announcing the
release.

## crates.io trusted publishing setup

Only the first release, `v0.15.0`, may fall back to `CARGO_REGISTRY_TOKEN` when
trusted authentication fails: crates must exist before their publishers can be registered.
Later tags require trusted authentication.

After v0.15.0, follow DEV-508 (waiting on the DEV-417 release): the owner registers
a GitHub trusted publisher for each of `lotar-web-assets`, `lotar-lib` and `lotar`,
using owner `localtaskrepo`, repository `lotar`, workflow `release.yml`, and
environment `release`. Enable "trusted publishing only" for all three crates,
revoke the bootstrap API token, and delete `CARGO_REGISTRY_TOKEN` from both
`localtaskrepo/lotar` and `localtaskrepo/harnessmux`. Then hand DEV-508 back to an
agent to remove the first-release fallback from the workflow and this guide.
