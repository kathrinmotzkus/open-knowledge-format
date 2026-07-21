---
title: OKF Debian Packaging
type: Reference
status: draft
created: 2026-07-18
updated: 2026-07-21
---

# OKF Debian Packaging

This directory contains the Debian `.deb` packaging structure for `okf-http`.

It is intentionally small and reviewable. It prepares installable Linux
packages and later `cargo-dist` or APT repository evaluation without requiring
a Rust toolchain on the target system.

## Current Scope

The package installs immutable application files:

```text
/usr/bin/okf-http
/usr/share/doc/okf-http/
/usr/share/okf/browser/
/usr/share/okf/themes/
/usr/lib/systemd/user/okf-http.service.example
/usr/lib/systemd/system/okf-http.service.example
```

Mutable data remains outside the package:

```text
/var/lib/okf/sites/<sitename>/
/etc/okf/
~/.config/okf/
```

The package must not overwrite user content below `/var/lib/okf` or browser
configuration below `~/.config/okf`.

## Build

Build a local package for the native Linux target:

```bash
scripts/package-okf-http-deb.sh x86_64-unknown-linux-gnu
```

The script writes the package to `dist/deb/`.

Linux aarch64 packages may be built after the corresponding cross-compiled
binary exists:

```bash
scripts/package-okf-http-deb.sh aarch64-unknown-linux-gnu
```

## Install from GitHub Releases

For Debian-compatible Linux systems, download the package and checksum from
the matching GitHub Release. Use `amd64` for Linux x86_64 and `arm64` for Linux
aarch64:

```bash
version=0.4.3
arch=amd64
wget "https://github.com/kathrinmotzkus/open-knowledge-format/releases/download/okf-http-v${version}/okf-http_${version}_${arch}.deb"
wget "https://github.com/kathrinmotzkus/open-knowledge-format/releases/download/okf-http-v${version}/okf-http_${version}_${arch}.deb.sha256"
sha256sum -c "okf-http_${version}_${arch}.deb.sha256"
sudo apt install "./okf-http_${version}_${arch}.deb"
```

These commands install a stable release asset. For frequent development testing
from mutable nightly assets, use
[`../../nightly/install-okf-http-nightly-deb.sh`](../../nightly/install-okf-http-nightly-deb.sh)
instead.

Then verify:

```bash
command -v okf-http
okf-http --version
cat /usr/share/doc/okf-http/VERSION
ls /usr/share/okf/browser
```

Service examples are installed as examples only. They are not enabled
automatically.

## Install a Locally Built Package

Install a package built from the current checkout with:

```bash
sudo apt install ./dist/deb/okf-http_<version>_<arch>.deb
okf-http --version
cat /usr/share/doc/okf-http/VERSION
ls /usr/share/okf/browser
```
