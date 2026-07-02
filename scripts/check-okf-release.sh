#!/bin/sh
set -eu

if [ -n "${OKF_VOYAGE_API_KEY:-}" ]; then
    echo "refusing ordinary release gates while OKF_VOYAGE_API_KEY is set" >&2
    exit 1
fi

if [ -n "${VOYAGE_API_KEY:-}" ]; then
    echo "refusing ordinary release gates while legacy VOYAGE_API_KEY is set" >&2
    exit 1
fi

if [ -n "${OKF_HTTP_TRUSTED_PROXY_TOKEN:-}" ]; then
    echo "refusing ordinary release gates while OKF_HTTP_TRUSTED_PROXY_TOKEN is set" >&2
    exit 1
fi

env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY -u OKF_HTTP_TRUSTED_PROXY_TOKEN \
    cargo fmt --all -- --check
env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY -u OKF_HTTP_TRUSTED_PROXY_TOKEN \
    cargo clippy --workspace --all-targets -- -D warnings

env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY -u OKF_HTTP_TRUSTED_PROXY_TOKEN \
    CARGO_NET_OFFLINE=true cargo test --workspace --locked

node --check crates/okf-http/browser/app.js
node --test crates/okf-http/tests/browser_security.cjs
scripts/check-okf-root-security.sh

cmp crates/okf-http/browser/README.md docs-browser/README.md
cmp crates/okf-http/browser/app.js docs-browser/app.js
cmp crates/okf-http/browser/index.html docs-browser/index.html
cmp crates/okf-http/browser/security.js docs-browser/security.js
cmp crates/okf-http/browser/styles.css docs-browser/styles.css

scripts/check-okf-package-contents.sh
git diff --check

echo "OKF release gates passed without provider or proxy credentials"
