#!/bin/sh
set -eu

target=${1:-}

if [ -z "$target" ]; then
    echo "usage: $0 <rust-target-triple>" >&2
    exit 2
fi

rustup target add "$target"

env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY -u OKF_HTTP_TRUSTED_PROXY_TOKEN \
    cargo build --workspace --locked --target "$target"
