#!/bin/sh
set -eu

target=${1:-}

if [ -z "$target" ]; then
    echo "usage: $0 <rust-target-triple>" >&2
    exit 2
fi

version=$(awk -F '"' '
    /^version = "/ {
        print $2
        exit
    }
' crates/okf-http/Cargo.toml)

if [ -z "$version" ]; then
    echo "failed to read okf-http version from crates/okf-http/Cargo.toml" >&2
    exit 1
fi

rustup target add "$target"

env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY -u OKF_HTTP_TRUSTED_PROXY_TOKEN \
    cargo build --release --locked -p okf-http --target "$target"

artifact="okf-http-${version}-${target}"
dist_dir="dist"
stage_dir="${dist_dir}/${artifact}"
target_dir=$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
binary="${target_dir}/${target}/release/okf-http"

if [ ! -f "$binary" ]; then
    fallback_binary="${target_dir}/release/okf-http"
    if [ -f "$fallback_binary" ]; then
        binary="$fallback_binary"
    else
        echo "failed to locate okf-http binary for target ${target}" >&2
        echo "checked: ${binary}" >&2
        echo "checked: ${fallback_binary}" >&2
        exit 1
    fi
fi

rm -rf "$stage_dir"
mkdir -p "$stage_dir/bin" "$stage_dir/doc"

install -m 755 "$binary" "$stage_dir/bin/okf-http"
install -m 644 README.md "$stage_dir/README.md"
install -m 644 LICENSE "$stage_dir/LICENSE"
install -m 644 crates/okf-http/README.md "$stage_dir/doc/okf-http-README.md"
install -m 644 crates/okf-http/CHANGELOG.md "$stage_dir/doc/okf-http-CHANGELOG.md"
install -m 644 crates/okf/README.md "$stage_dir/doc/okf-library-README.md"
install -m 644 crates/okf/CHANGELOG.md "$stage_dir/doc/okf-library-CHANGELOG.md"

cat >"$stage_dir/VERSION" <<EOF
okf-http=${version}
target=${target}
okf-open-knowledge-format=$(awk -F '"' '/^version = "/ { print $2; exit }' crates/okf/Cargo.toml)
EOF

tar_path="${dist_dir}/${artifact}.tar.gz"
tar -C "$dist_dir" -czf "$tar_path" "$artifact"

if command -v sha256sum >/dev/null 2>&1; then
    (cd "$dist_dir" && sha256sum "${artifact}.tar.gz" >"${artifact}.tar.gz.sha256")
else
    (cd "$dist_dir" && shasum -a 256 "${artifact}.tar.gz" >"${artifact}.tar.gz.sha256")
fi

printf '%s\n' "$tar_path"
