#!/bin/sh
set -eu

component=${1:-}
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/okf-msrv.XXXXXX")
trap 'rm -rf "$work_dir"' EXIT HUP INT TERM

case "$component" in
    okf)
        cp -R crates/okf "$work_dir/okf"
        cp -R crates/okf-http "$work_dir/okf-http"
        printf '%s\n' \
            '[workspace]' \
            'members = ["okf", "okf-http"]' \
            'resolver = "2"' >"$work_dir/Cargo.toml"
        cp Cargo.lock "$work_dir/Cargo.lock"
        cargo fetch --locked --manifest-path "$work_dir/Cargo.toml"
        CARGO_NET_OFFLINE=true cargo test --offline --manifest-path "$work_dir/Cargo.toml" -p okf-open-knowledge-format
        ;;
    okf-http)
        cp -R crates/okf "$work_dir/okf"
        cp -R crates/okf-http "$work_dir/okf-http"
        rm -f "$work_dir/okf/Cargo.lock" "$work_dir/okf-http/Cargo.lock"
        printf '%s\n' \
            '[workspace]' \
            'members = ["okf", "okf-http"]' \
            'resolver = "2"' >"$work_dir/Cargo.toml"
        cp Cargo.lock "$work_dir/Cargo.lock"
        cargo fetch --locked --manifest-path "$work_dir/Cargo.toml"
        CARGO_NET_OFFLINE=true cargo test --offline --manifest-path "$work_dir/Cargo.toml" -p okf-http
        ;;
    *)
        echo "usage: $0 <okf|okf-http>" >&2
        exit 2
        ;;
esac
