#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PROJECT_DIR/target}"
PREFIX="/usr/local"
BUILD_MODE="release"
DO_BUILD=1
INSTALL_BROWSER=1
BROWSER_FORCE=0
REMOVE_LEGACY_SCANLAB=0

usage() {
  cat <<'USAGE'
Usage: ./install.sh [OPTIONS]

Build and install the okf-http binary. Run this script as your regular user;
it invokes sudo only for the system-wide binary installation. Browser assets
are installed for the current user without sudo.

Options:
  --release              Build and install the release binary (default)
  --debug                Build and install the debug binary
  --no-build             Install an already built binary
  --prefix PATH          Installation prefix (default: /usr/local)
  --no-browser           Do not install browser assets in the user account
  --force-browser        Replace locally modified packaged browser assets
  --remove-legacy-scanlab
                         Remove the former scanlab-owned OKF installation
  -h, --help             Show this help
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) BUILD_MODE="release" ;;
    --debug) BUILD_MODE="debug" ;;
    --no-build) DO_BUILD=0 ;;
    --prefix)
      PREFIX="${2:?--prefix needs a path}"
      shift
      ;;
    --no-browser) INSTALL_BROWSER=0 ;;
    --force-browser) BROWSER_FORCE=1 ;;
    --remove-legacy-scanlab) REMOVE_LEGACY_SCANLAB=1 ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [[ "$EUID" -eq 0 ]]; then
  cat >&2 <<'ERROR'
Do not run this installer as root or with `sudo ./install.sh`.

Run it as your regular user instead. The installer invokes sudo only for the
system-wide binary installation and keeps browser files owned by your user.
ERROR
  exit 1
fi

TARGET_DIR="$CARGO_TARGET_DIR/$BUILD_MODE"
SOURCE_BINARY="$TARGET_DIR/okf-http"
INSTALL_BINARY="$PREFIX/bin/okf-http"

permission_probe="$PREFIX"
while [[ ! -e "$permission_probe" ]]; do
  permission_probe="$(dirname "$permission_probe")"
done
if [[ -w "$permission_probe" ]]; then
  privileged=()
else
  privileged=(sudo)
fi

if [[ "$DO_BUILD" -eq 1 ]]; then
  if [[ "$BUILD_MODE" == "release" ]]; then
    cargo build --release --locked --package okf-http
  else
    cargo build --locked --package okf-http
  fi
fi

if [[ ! -x "$SOURCE_BINARY" ]]; then
  echo "Missing built binary: $SOURCE_BINARY" >&2
  exit 1
fi

if [[ "$REMOVE_LEGACY_SCANLAB" -eq 1 ]]; then
  sudo rm -rf -- \
    /usr/local/bin/okf-http \
    /usr/local/share/scanlab/component-docs/okf
fi

"${privileged[@]}" install -d -m 0755 "$PREFIX/bin"
if [[ "${#privileged[@]}" -eq 0 ]]; then
  install -m 0755 "$SOURCE_BINARY" "$INSTALL_BINARY"
else
  "${privileged[@]}" install -o root -g root -m 0755 "$SOURCE_BINARY" "$INSTALL_BINARY"
fi

if [[ "$INSTALL_BROWSER" -eq 1 ]]; then
  browser_args=(--install-browser)
  if [[ "$BROWSER_FORCE" -eq 1 ]]; then
    browser_args+=(--force)
  fi
  "$INSTALL_BINARY" "${browser_args[@]}"
fi

echo
echo "Installed OKF HTTP server: $INSTALL_BINARY"
if [[ "$INSTALL_BROWSER" -eq 1 ]]; then
  echo "Installed browser assets for the current user."
fi
