#!/bin/sh
set -eu

repo="kathrinmotzkus/open-knowledge-format"
tag="okf-http-nightly"
assume_yes=false
download_only=false

usage() {
    cat <<'EOF'
usage: nightly/install-okf-http-nightly-deb.sh [--yes] [--download-only] [--tag <tag>]

Install or download the latest okf-http nightly Debian package for Debian
architecture amd64 / Linux x86_64 from GitHub Release assets.

Options:
  --yes            Do not ask for interactive confirmation.
  --download-only  Download and verify the package without installing it.
  --tag <tag>      Use another GitHub release tag. Default: okf-http-nightly.
  --help           Show this help.

Nightly packages are development builds. They are not the stable installation
path and may contain unreleased changes. okf-http must be exposed to networks
through a reverse proxy; direct non-loopback binding is intentionally rejected.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --yes|-y)
            assume_yes=true
            shift
            ;;
        --download-only)
            download_only=true
            shift
            ;;
        --tag)
            if [ "$#" -lt 2 ]; then
                echo "missing value for --tag" >&2
                exit 2
            fi
            tag=$2
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if ! command -v wget >/dev/null 2>&1; then
    echo "wget is required" >&2
    exit 1
fi

if ! command -v sha256sum >/dev/null 2>&1; then
    echo "sha256sum is required" >&2
    exit 1
fi

if ! command -v dpkg >/dev/null 2>&1; then
    echo "dpkg is required to detect the Debian architecture" >&2
    exit 1
fi

deb_arch=$(dpkg --print-architecture)
case "$deb_arch" in
    amd64)
        ;;
    *)
        echo "unsupported Debian architecture for this nightly comfort path: $deb_arch" >&2
        echo "supported architecture: amd64 (Linux x86_64)" >&2
        echo "use stable release artifacts or source builds for other architectures" >&2
        exit 1
        ;;
esac

api_url="https://api.github.com/repos/${repo}/releases/tags/${tag}"

release_json=$(wget -qO- "$api_url") || {
    echo "failed to read GitHub release metadata for ${tag}" >&2
    echo "checked: ${api_url}" >&2
    exit 1
}

asset_urls=$(printf '%s\n' "$release_json" |
    sed -n 's/.*"browser_download_url": "\([^"]*\)".*/\1/p')

deb_url=$(printf '%s\n' "$asset_urls" |
    grep "/okf-http_.*_${deb_arch}\.deb$" |
    head -n 1 || true)

sha_url=$(printf '%s\n' "$asset_urls" |
    grep "/okf-http_.*_${deb_arch}\.deb\.sha256$" |
    head -n 1 || true)

if [ -z "$deb_url" ] || [ -z "$sha_url" ]; then
    echo "failed to find nightly Debian assets for architecture ${deb_arch}" >&2
    echo "release tag: ${tag}" >&2
    exit 1
fi

deb_file=${deb_url##*/}
sha_file=${sha_url##*/}
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/okf-http-nightly.XXXXXX")

cleanup() {
    if [ "$download_only" = false ]; then
        rm -rf "$work_dir"
    fi
}
trap cleanup EXIT HUP INT TERM

cat <<EOF
WARNING: You are about to install an okf-http nightly build.

Repository: ${repo}
Release tag: ${tag}
Architecture: ${deb_arch} / Linux x86_64
Package: ${deb_file}

Nightly builds may contain unreleased development changes.
Use stable releases for production systems.
EOF

if [ "$assume_yes" = false ]; then
    if [ ! -t 0 ]; then
        echo "refusing non-interactive install without --yes" >&2
        exit 1
    fi
    printf 'Type "nightly" to continue: '
    read answer
    if [ "$answer" != "nightly" ]; then
        echo "aborted" >&2
        exit 1
    fi
fi

echo "downloading ${deb_file}"
wget -q "$deb_url" -O "${work_dir}/${deb_file}"

echo "downloading ${sha_file}"
wget -q "$sha_url" -O "${work_dir}/${sha_file}"

echo "verifying checksum"
(
    cd "$work_dir"
    sha256sum -c "$sha_file"
)

if [ "$download_only" = true ]; then
    echo "downloaded and verified files:"
    echo "${work_dir}/${deb_file}"
    echo "${work_dir}/${sha_file}"
    exit 0
fi

if ! command -v apt >/dev/null 2>&1; then
    echo "apt is required to install the package" >&2
    exit 1
fi

sudo_cmd=
if [ "$(id -u)" -ne 0 ]; then
    if ! command -v sudo >/dev/null 2>&1; then
        echo "sudo is required when not running as root" >&2
        exit 1
    fi
    sudo_cmd=sudo
fi

echo "installing ${deb_file}"
$sudo_cmd apt install "${work_dir}/${deb_file}"

echo "installed okf-http nightly package"
if command -v okf-http >/dev/null 2>&1; then
    okf-http --version || true
fi
