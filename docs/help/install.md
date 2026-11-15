# Installation

Every package below is produced by the same release workflow from the same tagged source.
After installing, check the binary with `lotar --version`.

## macOS and Linux: Homebrew

LoTaR is not in Homebrew core. It ships as a formula in its own third-party tap,
[localtaskrepo/homebrew-lotar](https://github.com/localtaskrepo/homebrew-lotar), so add the tap first:

```bash
brew tap localtaskrepo/lotar
brew trust localtaskrepo/lotar
brew install lotar
lotar --version
```

Homebrew 6 and newer refuse formulae from third-party taps until you trust them, so `brew trust` is
needed once per machine (older Homebrew has no `brew trust` command; skip that line). On macOS the formula installs the
Apple Silicon or Intel build; on Linux (x64 or arm64) it installs the static musl build. Upgrade with
`brew upgrade lotar`.

## Linux and macOS: install script

```bash
curl -fsSL https://localtaskrepo.github.io/lotar/install.sh | sh
```

The script downloads the latest release for your system from GitHub, checks it against the release's
SHA-256 file, and installs `lotar` to `~/.local/bin` without `sudo`. Linux gets the static musl build,
which runs on any distribution, on x64, arm64, and (in releases after v0.12.0) 32-bit ARM (armv7,
such as 32-bit Raspberry Pi OS; armv6 boards like the Pi Zero are not supported). It needs `curl` or
`wget`, `tar`, and a SHA-256 tool, and works with BusyBox. Options:

```bash
# A specific release and another directory (the directory must be writable)
curl -fsSL https://localtaskrepo.github.io/lotar/install.sh | sh -s -- --version v0.11.0 --to /usr/local/bin
curl -fsSL https://localtaskrepo.github.io/lotar/install.sh | sudo sh -s -- --to /usr/local/bin
```

The same options can be set with `LOTAR_VERSION` and `LOTAR_INSTALL_DIR`. Run the script again to
upgrade. To read the script before running it, download it first:
`curl -fsSLO https://localtaskrepo.github.io/lotar/install.sh`.

## Linux: .deb and .rpm packages

Releases after v0.11.0 also attach Debian/Ubuntu and Fedora/RHEL packages for x64 and arm64, built from
the static musl binaries, each with a SHA-256 file, a Sigstore signature, and a certificate:

```bash
VERSION=v0.12.0   # pick the release you want
curl -LO https://github.com/localtaskrepo/lotar/releases/download/$VERSION/lotar-$VERSION-linux-x64.deb
sudo apt install ./lotar-$VERSION-linux-x64.deb      # Fedora/RHEL: sudo dnf install ./lotar-$VERSION-linux-x64.rpm
lotar --version
```

Use `linux-arm64` for 64-bit ARM. Releases after v0.12.0 also have a `linux-armv7` `.deb` (armhf) for
32-bit ARM such as Raspberry Pi OS; there is no 32-bit ARM `.rpm`, since Fedora and RHEL no longer support
it. The packages install `/usr/bin/lotar`; there is no package repository yet, so upgrade by installing
the newer package. The `.rpm` files have no GPG signature (their Sigstore signature is a separate file),
so on RPM 6 (Fedora 45 and newer) install them with `dnf` as shown; plain `rpm -i` needs `--nosignature`.

## Any OS with Node.js: npm

```bash
npm install --global @localtaskrepo/lotar
lotar --version
```

The `@localtaskrepo/lotar` package installs the `lotar` command, using the release binary for your platform (macOS, Linux, or Windows on x64 or
arm64, and Linux on 32-bit ARM; up to v0.12.0, Windows on ARM ran the x64 build under emulation). npm picks the matching
`@localtaskrepo/lotar-<platform>` package from its optional dependencies; there is no install script,
so `--ignore-scripts` works. Node.js only starts the binary. Upgrade with `npm update --global @localtaskrepo/lotar`.

To run it without installing, use `npx -y @localtaskrepo/lotar <command>`. For an MCP client, that makes the server
configuration independent of how LoTaR is installed:

```json
{ "command": "npx", "args": ["-y", "@localtaskrepo/lotar", "mcp"] }
```

Packages are published with npm provenance from the same release workflow as the binaries, starting
with the first release after v0.11.0.

## Windows: Scoop

```powershell
# Only if Scoop is not installed yet
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser -Force
iwr -useb get.scoop.sh | iex

scoop bucket add lotar https://github.com/localtaskrepo/scoop-lotar
scoop install lotar
lotar --version
```

The bucket is updated during every release. Upgrade with `scoop update lotar`.

## Windows: install script

```powershell
irm https://localtaskrepo.github.io/lotar/install.ps1 | iex
```

The script downloads the latest release for your CPU (x64 or ARM64), checks it against the release's SHA-256 file, installs
`lotar.exe` to `%LOCALAPPDATA%\Programs\lotar`, and adds that folder to your user `PATH` (open a new
terminal afterwards). Set `$env:LOTAR_VERSION` (for example `v0.11.0`) or `$env:LOTAR_INSTALL_DIR` before
running it to pick a release or folder. On ARM64 Windows it installs the native ARM64 build; for releases
up to v0.12.0, which have none, it installs the x64 build, which Windows 11 runs under emulation. Run it
again to upgrade.

## Linux, macOS, Windows: release archives

Each [GitHub release](https://github.com/localtaskrepo/lotar/releases/latest) has archives for
`linux-x64`, `linux-musl-x64`, `linux-musl-arm64`, `linux-musl-armv7`, `macos-arm64`, `macos-x64`,
`windows-x64`, and `windows-arm64` (`linux-musl-armv7` and `windows-arm64` in releases after v0.12.0),
each with a SHA-256 checksum file and a keyless Sigstore signature (`.sig`) and certificate (`.pem`).
From v0.15.0 on the macOS binary is also signed with a Developer ID and notarized by
Apple, so macOS keeps the firewall and file-access permissions you gave `lotar` when you upgrade.

```bash
VERSION=v0.8.0   # pick the release you want
ASSET=lotar-$VERSION-linux-musl-x64
curl -LO https://github.com/localtaskrepo/lotar/releases/download/$VERSION/$ASSET.tar.gz
curl -LO https://github.com/localtaskrepo/lotar/releases/download/$VERSION/$ASSET.sha256
sha256sum --check $ASSET.sha256      # macOS: shasum -a 256 --check $ASSET.sha256
tar -xzf $ASSET.tar.gz
sudo mv lotar /usr/local/bin/
lotar --version
```

Release archives contain the `lotar` binary only. Agent jobs, `lotar agent list-running` and
cancelling a job from another process work without the optional `lotar-agent-wrapper` binary; see
[Jobs in other processes](agent.md#jobs-in-other-processes).

## Any OS: Docker

The [`mallox/lotar`](https://hub.docker.com/r/mallox/lotar) image contains the static musl build
and `git` on Alpine Linux, for `linux/amd64`, `linux/arm64`, and (after v0.12.0) `linux/arm/v7`. Its entrypoint is `lotar`, so
pass subcommands directly. Mount your repository at `/workspace`, the working directory, and on
Linux run as your own user so LoTaR can write to it:

```bash
docker run --rm mallox/lotar --version
docker run --rm \
    --user "$(id -u):$(id -g)" \
    -v "$PWD":/workspace \
    mallox/lotar list
```

`latest` is the newest stable release and `next` the newest prerelease. See
[docs/docker.md](../docker.md) for file ownership, Docker Compose, a long-running server, and
shared task directories.

## Build from source

LoTaR is not published on crates.io: the binary embeds the web UI, which is built with Node first.
Use the Rust toolchain pinned by `rust-toolchain.toml` and stable Node.js 24.15+ on
the 24.x line, or Node.js 26+. Node 25 is not supported by the current build/test
dependencies. Node is needed to build the web UI, not to run the release binary.

```bash
git clone https://github.com/localtaskrepo/lotar
cd lotar
npm ci
npm run install:local             # builds the web UI and both binaries, then installs them
lotar --version
```

`npm run install:local` runs `npm run build` (the web UI, then `cargo build --release`) and installs `lotar`
and `lotar-agent-wrapper` into `LOTAR_INSTALL_DIR`, by default `~/.local/bin` (Windows:
`%LOCALAPPDATA%\Programs\lotar`). It prints a warning when that directory is not on `PATH`. To
replace the `lotar` already on your `PATH`, such as a Homebrew install, install into its directory:

```bash
LOTAR_INSTALL_DIR="$(dirname "$(realpath "$(command -v lotar)")")" npm run install:local
```

On macOS the step signs both binaries before installing them, with the hardened runtime and the fixed
identifiers `net.pyxzl.lotar` (the release binary's) and `net.pyxzl.lotar-agent-wrapper`. macOS ties
firewall and file-access grants to the signature, so with a stable one every new build keeps the grants
of the one before:

- It signs with `LOTAR_SIGNING_IDENTITY` when set: an identity's name or SHA-1 hash from
  `security find-identity -v -p codesigning`, or `-` for an ad-hoc signature. If that identity is not
  in the keychain, the step stops.
- Otherwise it signs with the maintainer's `Developer ID Application: Ravi Gairola (3HKD37M482)` when
  the keychain has it, with a secure timestamp from Apple (so it needs the network). The first time,
  macOS may ask at the Mac to let `codesign` use the key: choose **Always Allow**. The step waits
  two minutes for that answer, then stops.
- Without that identity it signs ad hoc and prints a warning: macOS then treats every build as a new
  app and asks for its firewall and file-access grants again.

Each binary is signed and checked in a staging file next to its destination, and both are then renamed
over the installed ones. When signing or the check fails, the step stops with codesign's error and both
installed binaries stay as they were; a running `lotar serve` keeps running the file it started from.
On Linux and Windows the binaries are installed unsigned. To only build, run `npm run build`; the
binaries are in `target/release`.

For development builds, tests, and the smoke suite, see [AGENTS.md](../../AGENTS.md) and
[docs/developers/](../developers/README.md). Release steps are in the
[release guide](../release-guide.md).

## Next steps

- [CLI tour](cli-tour.md): a first workflow, multiple projects, global options, configuration
- [Serve](serve.md): the web UI and REST API
- [Help index](index.md): every command
