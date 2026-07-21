---
title: OKF Platform Support
type: Reference
status: active
created: 2026-07-18
updated: 2026-07-21
---

# OKF Platform Support

This document defines the first public OKF platform support baseline.

It applies to:

- `okf-open-knowledge-format`, the Rust library crate
- `okf-http`, the local HTTP server and browser-facing binary
- future release artifacts for `okf-http`

## Supported Platforms

The first supported platform baseline is:

| Platform | Architecture | Rust target | Status |
| --- | --- | --- | --- |
| Linux | x86_64 | `x86_64-unknown-linux-gnu` | supported |
| Linux | aarch64 | `aarch64-unknown-linux-gnu` | supported |
| macOS | x86_64 | `x86_64-apple-darwin` | supported |
| macOS | aarch64 | `aarch64-apple-darwin` | supported |

Supported means that OKF intends to build, test, and eventually ship release
artifacts for the platform. A supported platform failure in required CI is a
release blocker.

## Experimental Platforms

| Platform | Architecture | Status |
| --- | --- | --- |
| Windows | x86_64 | experimental |
| Windows | ARM64 | experimental |

Experimental means that OKF has not yet made the platform a release promise.
Experimental failures may be visible in optional CI jobs, but they are not
release blockers until support is declared explicitly.

Windows is not rejected. It needs separate review for path handling, browser
installation, service integration, local TLS, filesystem watching, and shell
examples.

## Installation Expectations

Source builds for `okf-open-knowledge-format` require Rust 1.85 or newer.
Source builds and `cargo install` for `okf-http` require Rust 1.88 or newer
because the HTTP server includes TLS and certificate handling dependencies.

Release artifacts for `okf-http` do not require Rust on the target system.
They allow users to install or run `okf-http` from prebuilt archives or
packages.

## Current CI Coverage

The current CI baseline covers:

- release gates on Linux x86_64
- workspace tests on Linux x86_64, macOS x86_64, and macOS aarch64
  GitHub-hosted runners
- target builds for:
  - `x86_64-unknown-linux-gnu`
  - `aarch64-unknown-linux-gnu`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
- MSRV checks for `okf-open-knowledge-format` on Rust 1.85 and `okf-http` on
  Rust 1.88
- offline tests without Voyage AI credentials
- optional experimental Windows tests on manual workflow dispatch
- release-candidate `okf-http` archives with SHA-256 checksums on manual
  workflow dispatch and `okf-http-v*` tags
- local and CI-generated `.deb` packages for Linux x86_64 and Linux aarch64

Linux aarch64 target builds use cross-compilation first. Native Linux-aarch64
tests are still expected later through a VM, runner, or hardware test path.

Dedicated release publication and Debian packaging are tracked in
[Platform, CI, and release artifacts](../plans/platform-ci-and-release-artifacts-plan.md).

## Release Policy Notes

`okf-open-knowledge-format` and `okf-http` may use separate release policies.

If only the library changes, new operating-system binaries are not necessarily
required. If `okf-http` changes in a release, new release binaries must be
built.
