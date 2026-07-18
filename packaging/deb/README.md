---
title: OKF Debian Packaging
type: Reference
status: draft
created: 2026-07-18
updated: 2026-07-18
---

# OKF Debian Packaging

This directory contains the first local `.deb` packaging structure for
`okf-http`.

It is intentionally small and reviewable. It prepares the project for VM tests
and later `cargo-dist` evaluation without requiring a Rust toolchain on the
target system.

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

## Install on a Test VM

Copy the `.deb` to the VM and install it there:

```bash
sudo apt install ./okf-http_<version>_<arch>.deb
```

Then verify:

```bash
command -v okf-http
okf-http --help
cat /usr/share/doc/okf-http/VERSION
ls /usr/share/okf/browser
```

Service examples are installed as examples only. They are not enabled
automatically.
