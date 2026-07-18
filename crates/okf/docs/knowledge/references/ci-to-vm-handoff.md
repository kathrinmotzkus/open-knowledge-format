---
title: OKF CI-to-VM Handoff
type: Reference
status: draft
created: 2026-07-18
updated: 2026-07-18
---

# OKF CI-to-VM Handoff

This document describes how a CI-built `okf-http` artifact should be tested on
a clean virtual machine without rebuilding from source.

The handoff is intentionally explicit: the VM should test the same artifact
that would be shipped to users.

## Required Inputs

Record these values before starting the VM test:

- GitHub Actions workflow name
- GitHub Actions run ID
- commit SHA
- `okf-http` version
- `okf-open-knowledge-format` version
- Rust target triple
- artifact filename
- checksum filename
- VM profile
- installation method

## Artifact Types

Phase 6 supports two artifact types:

- release archive: `okf-http-<version>-<target>.tar.gz`
- Debian package: `okf-http_<version>_<arch>.deb`

Linux VM tests should prefer the `.deb` artifact once it exists. Archive tests
remain useful for users who do not want package-manager installation.

## Download

Download the artifact from the GitHub Actions run or GitHub Release candidate.

Verify the checksum before installing:

```bash
sha256sum -c okf-http_<version>_<arch>.deb.sha256
```

or:

```bash
sha256sum -c okf-http-<version>-<target>.tar.gz.sha256
```

## Debian Package Smoke Test

On the VM:

```bash
sudo apt install ./okf-http_<version>_<arch>.deb
command -v okf-http
okf-http --help
cat /usr/share/doc/okf-http/VERSION
ls /usr/share/okf/browser
ls /usr/lib/systemd/system/okf-http.service.example
ls /usr/lib/systemd/user/okf-http.service.example
```

The package should not require Rust, Cargo, or rustup on the VM.

## Archive Smoke Test

On the VM:

```bash
tar -xzf okf-http-<version>-<target>.tar.gz
./okf-http-<version>-<target>/bin/okf-http --help
cat ./okf-http-<version>-<target>/VERSION
```

The archive should run without installing Rust.

## Report

Use [VM test report template](vm-test-report-template.md) to capture the
result.

Do not paste secrets, private paths, SSH keys, API keys, or server credentials
into the report.
