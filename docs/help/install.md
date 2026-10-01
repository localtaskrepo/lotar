# Installation

Every package below is produced by the same release workflow from the same tagged source.
After installing, check the binary with `lotar --version`.

## macOS: Homebrew

LoTaR is not in Homebrew core. It ships as a formula in its own third-party tap,
[localtaskrepo/homebrew-lotar](https://github.com/localtaskrepo/homebrew-lotar), so add the tap first:

```bash
brew tap localtaskrepo/lotar
brew install lotar
lotar --version
```

`brew install localtaskrepo/lotar/lotar` does both steps in one command. The formula is macOS-only and
ships separate Apple Silicon and Intel builds; on Linux, use a [release archive](#linux-macos-windows-release-archives)
or [Docker](#any-os-docker). Upgrade with `brew upgrade lotar`.

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
You need the stable Rust toolchain and Node.js 24 or newer.

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
