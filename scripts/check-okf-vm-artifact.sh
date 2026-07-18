#!/bin/sh
set -eu

mode=${1:-}
path=${2:-}

usage() {
    cat >&2 <<'EOF'
usage:
  check-okf-vm-artifact.sh deb-installed
  check-okf-vm-artifact.sh archive <extracted-archive-dir>

This script performs smoke checks for a CI-built okf-http artifact on a VM.
It does not install packages and does not require Rust.
EOF
}

case "$mode" in
    deb-installed)
        binary=$(command -v okf-http || true)
        if [ -z "$binary" ]; then
            echo "okf-http not found in PATH" >&2
            exit 1
        fi
        "$binary" --help >/dev/null
        test -f /usr/share/doc/okf-http/VERSION
        test -d /usr/share/okf/browser
        test -f /usr/share/okf/browser/index.html
        test -f /usr/lib/systemd/system/okf-http.service.example
        test -f /usr/lib/systemd/user/okf-http.service.example
        ;;
    archive)
        if [ -z "$path" ]; then
            usage
            exit 2
        fi
        test -x "$path/bin/okf-http"
        "$path/bin/okf-http" --help >/dev/null
        test -f "$path/VERSION"
        test -f "$path/LICENSE"
        test -f "$path/README.md"
        ;;
    *)
        usage
        exit 2
        ;;
esac

if command -v cargo >/dev/null 2>&1 || command -v rustc >/dev/null 2>&1 || command -v rustup >/dev/null 2>&1; then
    echo "warning: Rust tooling is installed on this VM; this smoke check cannot prove a Rust-free system"
fi

echo "OKF VM artifact smoke checks passed"
