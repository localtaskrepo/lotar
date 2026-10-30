#!/bin/sh
# Install the LoTaR CLI from GitHub Releases on Linux or macOS (DEV-250).
#
#   curl -fsSL https://localtaskrepo.github.io/lotar/install.sh | sh
#   curl -fsSL https://localtaskrepo.github.io/lotar/install.sh | sh -s -- --version v0.11.0 --to /usr/local/bin
#
# Options (each also has an environment variable):
#   --version TAG   LOTAR_VERSION      release to install, e.g. v0.11.0 (default: latest)
#   --to DIR        LOTAR_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
#   -h, --help
#
# Linux gets the static musl build, which runs on any distribution. The archive is
# checked against the release's SHA-256 file before anything is installed. Releases
# are also signed with Sigstore; see https://github.com/localtaskrepo/lotar/blob/main/docs/help/install.md
#
# Everything runs from main at the bottom, so a partial download does nothing.

set -eu

REPO="localtaskrepo/lotar"
# Override only for mirrors or tests; the layout must match GitHub Releases.
RELEASES="${LOTAR_RELEASES_URL:-https://github.com/$REPO/releases}"

say() { printf '%s\n' "$*"; }
err() {
    printf 'lotar install: %s\n' "$*" >&2
    exit 1
}
have() { command -v "$1" >/dev/null 2>&1; }

usage() {
    cat <<'EOF'
Install the LoTaR CLI from GitHub Releases.

Usage: install.sh [--version TAG] [--to DIR]
  --version TAG  release to install, e.g. v0.11.0 (default: latest; env LOTAR_VERSION)
  --to DIR       install directory (default: ~/.local/bin; env LOTAR_INSTALL_DIR)
EOF
}

download() { # url output
    if have curl; then
        case "$1" in
        https://*) curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$1" ;;
        *) curl -fsSL -o "$2" "$1" ;;
        esac
    else
        wget -q -O "$2" "$1"
    fi
}

latest_tag() {
    # The /latest page redirects to /tag/<version>, which avoids GitHub API rate limits.
    if have curl; then
        url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$RELEASES/latest") || return 1
    else
        url=$(wget -S --spider "$RELEASES/latest" 2>&1 |
            sed -n 's/^ *[Ll]ocation: *\([^ ]*\).*/\1/p' | tail -n 1 | tr -d '\r')
    fi
    tag=${url##*/}
    case "$tag" in
    v[0-9]*) printf '%s\n' "$tag" ;;
    *) return 1 ;;
    esac
}

sha256_of() {
    if have sha256sum; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif have shasum; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    elif have openssl; then
        openssl dgst -sha256 -r "$1" | cut -d ' ' -f 1
    else
        err "no SHA-256 tool found (sha256sum, shasum or openssl)"
    fi
}

detect_platform() {
    case "$(uname -s)" in
    Linux) os=linux-musl ;;
    Darwin) os=macos ;;
    *) err "unsupported system $(uname -s); on Windows use install.ps1, or see $RELEASES" ;;
    esac
    case "$(uname -m)" in
    x86_64 | amd64) arch=x64 ;;
    aarch64 | arm64) arch=arm64 ;;
    *) err "unsupported CPU $(uname -m); see $RELEASES for the available builds" ;;
    esac
    # A shell running under Rosetta reports x86_64 on Apple Silicon.
    if [ "$os" = macos ] && [ "$arch" = x64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
        arch=arm64
    fi
    printf '%s-%s\n' "$os" "$arch"
}

main() {
    version="${LOTAR_VERSION:-}"
    dir="${LOTAR_INSTALL_DIR:-}"
    while [ $# -gt 0 ]; do
        case "$1" in
        --version)
            [ $# -ge 2 ] || err "--version needs a value"
            version=$2
            shift 2
            ;;
        --version=*)
            version=${1#*=}
            shift
            ;;
        --to)
            [ $# -ge 2 ] || err "--to needs a value"
            dir=$2
            shift 2
            ;;
        --to=*)
            dir=${1#*=}
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *) err "unknown option $1 (try --help)" ;;
        esac
    done

    have curl || have wget || err "curl or wget is required"
    have tar || err "tar is required"
    if [ -z "$dir" ]; then
        [ -n "${HOME:-}" ] || err "HOME is not set; pass --to DIR"
        dir="$HOME/.local/bin"
    fi

    platform=$(detect_platform)
    if [ -z "$version" ]; then
        version=$(latest_tag) || err "could not find the latest release at $RELEASES/latest"
    fi
    case "$version" in
    v*) ;;
    *) version="v$version" ;;
    esac

    asset="lotar-$version-$platform"
    tmp=$(mktemp -d 2>/dev/null || mktemp -d -t lotar)
    trap 'rm -rf "$tmp"' EXIT
    trap 'rm -rf "$tmp"; exit 130' INT TERM

    say "Downloading LoTaR $version ($platform)"
    download "$RELEASES/download/$version/$asset.tar.gz" "$tmp/$asset.tar.gz" ||
        err "download failed: $RELEASES/download/$version/$asset.tar.gz"
    download "$RELEASES/download/$version/$asset.sha256" "$tmp/$asset.sha256" ||
        err "checksum download failed: $RELEASES/download/$version/$asset.sha256"

    expected=$(cut -d ' ' -f 1 "$tmp/$asset.sha256" | tr 'A-F' 'a-f')
    actual=$(sha256_of "$tmp/$asset.tar.gz")
    [ -n "$expected" ] && [ "$expected" = "$actual" ] ||
        err "checksum mismatch for $asset.tar.gz (expected $expected, got $actual); nothing was installed"

    tar -xzf "$tmp/$asset.tar.gz" -C "$tmp" lotar || err "could not unpack $asset.tar.gz"
    mkdir -p "$dir" 2>/dev/null || err "cannot create $dir; pick another --to or rerun with sudo"
    [ -w "$dir" ] || err "cannot write to $dir; pick another --to or rerun with sudo"
    # Copy then rename, so an existing lotar is replaced in one step even while it runs.
    cp "$tmp/lotar" "$dir/.lotar.$$"
    chmod 0755 "$dir/.lotar.$$"
    mv -f "$dir/.lotar.$$" "$dir/lotar"

    say "Installed $("$dir/lotar" --version 2>/dev/null || say lotar) to $dir/lotar"
    case ":${PATH:-}:" in
    *":$dir:"*)
        found=$(command -v lotar 2>/dev/null || true)
        if [ -n "$found" ] && [ "$found" != "$dir/lotar" ]; then
            say "Note: $found comes first on your PATH and will run instead."
        fi
        ;;
    *) say "Add $dir to your PATH to run lotar, e.g. export PATH=\"$dir:\$PATH\"" ;;
    esac
}

main "$@"
