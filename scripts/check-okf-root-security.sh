#!/bin/sh
set -eu

if [ -n "${OKF_VOYAGE_API_KEY:-}" ] || [ -n "${VOYAGE_API_KEY:-}" ]; then
    echo "refusing root security gates while Voyage credentials are set" >&2
    exit 1
fi

env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY CARGO_NET_OFFLINE=true \
    cargo test -p okf-http document_routes_reject_regular_files_outside_the_admitted_inventory --locked
env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY CARGO_NET_OFFLINE=true \
    cargo test -p okf-http monitored_routes_withhold_added_and_modified_documents_until_acceptance --locked
env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY CARGO_NET_OFFLINE=true \
    cargo test -p okf-http authorized_root_api_is_proposal_bound_revision_safe_and_never_writes_dotenv --locked

if grep -E 'type="file"|localStorage|sessionStorage' crates/okf-http/browser/index.html crates/okf-http/browser/app.js; then
    echo "browser root workflow contains filesystem browsing or persistent authority" >&2
    exit 1
fi

if grep -E '(authorizedFetch|fetchWithTimeout).*\.env|/api/[^" ]*(dotenv|env-file)' \
    crates/okf-http/browser/app.js crates/okf-http/src/lib.rs crates/okf-http/src/routing.rs; then
    echo "browser-accessible .env path detected" >&2
    exit 1
fi

echo "OKF root security gates passed without provider credentials"
