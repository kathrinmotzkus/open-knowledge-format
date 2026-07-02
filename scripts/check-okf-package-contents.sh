#!/bin/sh
set -eu

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/okf-package-check.XXXXXX")
trap 'rm -rf "$work_dir"' EXIT HUP INT TERM

cargo package -p okf --allow-dirty --list >"$work_dir/okf.list"
cargo package -p okf-http --allow-dirty --list >"$work_dir/okf-http.list"
CARGO_NET_OFFLINE=true cargo package -p okf --allow-dirty --locked --no-verify

require_entry() {
    package_list=$1
    entry=$2
    if ! grep -Fxq "$entry" "$package_list"; then
        echo "missing package entry: $entry" >&2
        exit 1
    fi
}

for entry in LICENSE README.md CHANGELOG.md CONTRIBUTING.md SECURITY.md src/lib.rs \
    docs/knowledge/getting-started.md \
    docs/knowledge/plans/versioning-roadmap.md \
    docs/knowledge/references/release-checklist.md; do
    require_entry "$work_dir/okf.list" "$entry"
done

for entry in LICENSE README.md CHANGELOG.md src/main.rs browser/index.html browser/app.js \
    browser/security.js browser/styles.css browser/vendor/cytoscape.min.js \
    src/monitoring.rs tests/browser_security.cjs tests/standalone_e2e.rs; do
    require_entry "$work_dir/okf-http.list" "$entry"
done

for package_list in "$work_dir/okf.list" "$work_dir/okf-http.list"; do
    if grep -E '(^|/)(\.env($|\.)|\.okf-voyage($|/)|.*\.sqlite(-wal|-shm)?$|\.~lock\.|.*\.(swp|lck)$|okf-ai-accepted-edges)' "$package_list"; then
        echo "forbidden local or derived artifact in package list" >&2
        exit 1
    fi
done

if git ls-files | grep -E '(^|/)(\.env($|\.)|\.okf-voyage($|/)|.*\.sqlite(-wal|-shm)?$|\.~lock\.|.*\.(swp|lck)$|okf-ai-accepted-edges)'; then
    echo "forbidden local or derived artifact tracked by Git" >&2
    exit 1
fi

if grep -R -E 'pa-[A-Za-z0-9_-]{20,}' crates/okf crates/okf-http \
    --exclude='*.lock' --exclude='check-okf-package-contents.sh'; then
    echo "possible Voyage API key in packaged source" >&2
    exit 1
fi

if grep -R -n -E '/home/[^[:space:]]+' crates/okf crates/okf-http \
    --include='*.rs' --include='*.js' --include='*.cjs' --include='*.md'; then
    echo "physical home-directory path in packaged source" >&2
    exit 1
fi

if grep -n 'localStorage' crates/okf-http/browser/app.js; then
    echo "browser package still contains localStorage review state" >&2
    exit 1
fi

echo "OKF package contents are clean"
