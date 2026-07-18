#!/bin/sh
set -eu

target=${1:-}

if [ -z "$target" ]; then
    echo "usage: $0 <linux-rust-target-triple>" >&2
    exit 2
fi

case "$target" in
    x86_64-unknown-linux-gnu)
        deb_arch=amd64
        ;;
    aarch64-unknown-linux-gnu)
        deb_arch=arm64
        ;;
    *)
        echo "unsupported Debian package target: $target" >&2
        echo "supported targets: x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu" >&2
        exit 2
        ;;
esac

if ! command -v dpkg-deb >/dev/null 2>&1; then
    echo "dpkg-deb is required to build .deb packages" >&2
    exit 1
fi

version=$(awk -F '"' '
    /^version = "/ {
        print $2
        exit
    }
' crates/okf-http/Cargo.toml)

okf_version=$(awk -F '"' '
    /^version = "/ {
        print $2
        exit
    }
' crates/okf/Cargo.toml)

if [ -z "$version" ] || [ -z "$okf_version" ]; then
    echo "failed to read package versions" >&2
    exit 1
fi

rustup target add "$target"

env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY -u OKF_HTTP_TRUSTED_PROXY_TOKEN \
    cargo build --release --locked -p okf-http --target "$target"

target_dir=$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
binary="${target_dir}/${target}/release/okf-http"

if [ ! -f "$binary" ]; then
    echo "failed to locate okf-http binary for target ${target}" >&2
    echo "checked: ${binary}" >&2
    exit 1
fi

package="okf-http"
dist_dir="dist/deb"
stage_dir="${dist_dir}/${package}_${version}_${deb_arch}"
deb_path="${dist_dir}/${package}_${version}_${deb_arch}.deb"

rm -rf "$stage_dir"
mkdir -p \
    "$stage_dir/DEBIAN" \
    "$stage_dir/usr/bin" \
    "$stage_dir/usr/share/doc/okf-http" \
    "$stage_dir/usr/share/okf/browser" \
    "$stage_dir/usr/share/okf/themes" \
    "$stage_dir/usr/lib/systemd/system" \
    "$stage_dir/usr/lib/systemd/user"

install -m 755 "$binary" "$stage_dir/usr/bin/okf-http"
install -m 644 README.md "$stage_dir/usr/share/doc/okf-http/README.md"
install -m 644 LICENSE "$stage_dir/usr/share/doc/okf-http/copyright"
install -m 644 crates/okf-http/README.md "$stage_dir/usr/share/doc/okf-http/okf-http-README.md"
install -m 644 crates/okf-http/CHANGELOG.md "$stage_dir/usr/share/doc/okf-http/okf-http-CHANGELOG.md"
install -m 644 crates/okf/README.md "$stage_dir/usr/share/doc/okf-http/okf-library-README.md"
install -m 644 crates/okf/CHANGELOG.md "$stage_dir/usr/share/doc/okf-http/okf-library-CHANGELOG.md"
install -m 644 packaging/systemd/okf-http.service.example \
    "$stage_dir/usr/lib/systemd/system/okf-http.service.example"
install -m 644 packaging/systemd/okf-http.user.service.example \
    "$stage_dir/usr/lib/systemd/user/okf-http.service.example"

cp -R crates/okf-http/browser/. "$stage_dir/usr/share/okf/browser/"

cat >"$stage_dir/usr/share/okf/themes/README.md" <<'EOF'
# OKF Themes

This directory is reserved for packaged OKF themes.
EOF

cat >"$stage_dir/usr/share/doc/okf-http/VERSION" <<EOF
okf-http=${version}
okf-open-knowledge-format=${okf_version}
target=${target}
debian-architecture=${deb_arch}
EOF

cat >"$stage_dir/DEBIAN/control" <<EOF
Package: ${package}
Version: ${version}
Section: web
Priority: optional
Architecture: ${deb_arch}
Maintainer: OKF Maintainers <kathrin.cux@proton.me>
Depends: libc6
Homepage: https://github.com/kathrinmotzkus/open-knowledge-format
Description: HTTP server for Open Knowledge Format repositories
 okf-http serves and manages Open Knowledge Format repositories.
 It includes the local OKF browser assets and service example files.
 Mutable site data and user configuration are intentionally kept outside the
 package under /var/lib/okf, /etc/okf, or user configuration directories.
EOF

find "$stage_dir" -type d -exec chmod 755 {} +
find "$stage_dir" -type f -exec chmod 644 {} +
chmod 755 "$stage_dir/usr/bin/okf-http"

dpkg-deb --build --root-owner-group "$stage_dir" "$deb_path" >/dev/null

if command -v sha256sum >/dev/null 2>&1; then
    (cd "$dist_dir" && sha256sum "${package}_${version}_${deb_arch}.deb" >"${package}_${version}_${deb_arch}.deb.sha256")
else
    (cd "$dist_dir" && shasum -a 256 "${package}_${version}_${deb_arch}.deb" >"${package}_${version}_${deb_arch}.deb.sha256")
fi

printf '%s\n' "$deb_path"
