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
which runs on any distribution. It needs `curl` or `wget`, `tar`, and a SHA-256 tool, and works with
BusyBox. Options:

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

Use `linux-arm64` for 64-bit ARM. The packages install `/usr/bin/lotar`; there is no package repository
yet, so upgrade by installing the newer package.

## Any OS with Node.js: npm

```bash
npm install --global @localtaskrepo/lotar
lotar --version
```

The `@localtaskrepo/lotar` package installs the `lotar` command, using the release binary for your platform (macOS, Linux, or Windows on x64 or
arm64; Windows on ARM runs the x64 build under emulation). npm picks the matching
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

The script downloads the latest x64 release, checks it against the release's SHA-256 file, installs
`lotar.exe` to `%LOCALAPPDATA%\Programs\lotar`, and adds that folder to your user `PATH` (open a new
terminal afterwards). Set `$env:LOTAR_VERSION` (for example `v0.11.0`) or `$env:LOTAR_INSTALL_DIR` before
running it to pick a release or folder. On ARM64 Windows it installs the x64 build, which Windows 11 runs
under emulation. Run it again to upgrade.

## Linux, macOS, Windows: release archives

Each [GitHub release](https://github.com/localtaskrepo/lotar/releases/latest) has archives for
`linux-x64`, `linux-musl-x64`, `linux-musl-arm64`, `macos-arm64`, `macos-x64`, and `windows-x64`,
each with a SHA-256 checksum file and a keyless Sigstore signature (`.sig`) and certificate (`.pem`).

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

Release archives contain the `lotar` binary only. Agent jobs run without the optional
`lotar-agent-wrapper` binary, but `lotar agent list-running` needs it; see
[Agent jobs](agent.md#list-running-requirements).

## Any OS: Docker

The [`mallox/lotar`](https://hub.docker.com/r/mallox/lotar) image contains the static musl build
for `linux/amd64` and `linux/arm64`. Its entrypoint is `lotar`, so pass subcommands directly.
Mount your repository at `/workspace` and your tasks directory at `/tasks`:

```bash
docker run --rm mallox/lotar --version
docker run --rm \
    -v "$PWD":/workspace \
    -v "$PWD/.tasks":/tasks \
    -w /workspace \
    mallox/lotar list
```

See [docs/docker.md](../docker.md) for Docker Compose, a long-running server, and shared task
directories.

## Build from source

LoTaR is not published on crates.io: the binary embeds the web UI, which is built with Node first.
Use the Rust toolchain pinned by `rust-toolchain.toml` and stable Node.js 24.15+ on
the 24.x line, or Node.js 26+. Node 25 is not supported by the current build/test
dependencies. Node is needed to build the web UI, not to run the release binary.

```bash
git clone https://github.com/localtaskrepo/lotar
cd lotar
npm ci
npm run build                     # builds the web UI, then `cargo build --release`
export PATH="$PATH:$(pwd)/target/release"
lotar --version
```

For development builds, tests, and the smoke suite, see [AGENTS.md](../../AGENTS.md) and
[docs/developers/](../developers/README.md). Release steps are in the
[release guide](../release-guide.md).

## Next steps

- [CLI tour](cli-tour.md): a first workflow, multiple projects, global options, configuration
- [Serve](serve.md): the web UI and REST API
- [Help index](index.md): every command
