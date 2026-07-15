---
title: Post-Extraction Release Follow-up Plan
type: Plan
kind: knowledge-plan
topic: okf-release
status: active
updated: 2026-07-15
tags: [okf, okf-http, release, crates-io, scanlab, security]
---

# Post-Extraction Release Follow-up Plan

## Purpose

OKF and `okf-http` now live in the independent `open-knowledge-format`
repository. The immediate source tree is release-gated, but several follow-up
decisions remain before OKF should be treated as a public, stable community
module and before scanlab should consume it like an ordinary downstream
project.

This plan keeps those remaining tasks out of the extraction commit while making
their order explicit.

## Current Release-Readiness Baseline

- `okf` and `okf-http` are independent workspace members in
  `open-knowledge-format`.
- The release gate includes formatting, strict Clippy, RustSec audit,
  locked/offline tests, browser security checks, browser asset synchronization,
  root-security checks, package-content checks, and whitespace checks.
- `cargo audit` reports no known vulnerabilities.
- The remaining `rustls-pemfile` advisory is an unmaintained-warning, not a
  vulnerability, and is tracked below.
- `okf` can be packaged locally as a `.crate`.
- `okf-http` cannot be fully registry-packaged until `okf 0.3.0` is published,
  because its dependency declaration correctly resolves `okf = "^0.3.0"` from
  crates.io during package verification.

## Phase 1: Publishable `okf` Library Boundary

- Review the public Rust API exposed by `okf`.
- Decide which APIs are intended to remain stable through the 0.3.x line.
- Confirm package metadata, README, license, security policy, contribution
  guide, and changelog.
- Run `cargo package -p okf --locked --no-verify`.
- Optionally run `cargo publish -p okf --dry-run` before the real publish.

Completion criteria:

- `okf` is ready to publish as `0.3.0`.
- The release notes identify breaking pre-1.0 API and format changes.

## Phase 2: Publish `okf 0.3.0`

- Publish `okf` first, because `okf-http` depends on it by version.
- Verify crates.io shows `okf 0.3.0`.
- Tag the library release if the release process uses component tags.

Completion criteria:

- crates.io can resolve `okf = "^0.3.0"`.

## Phase 3: Package and Publish `okf-http`

- Re-run `cargo package -p okf-http --locked --no-verify` after `okf 0.3.0`
  exists on crates.io.
- Confirm the packaged browser assets and standalone tests are included.
- Decide whether `okf-http` should be distributed through crates.io, GitHub
  release binaries, or both.
- Publish only after the distribution channel decision is explicit.

Completion criteria:

- `okf-http` can be installed independently without a scanlab checkout.

## Phase 4: Replace scanlab's Local Path Dependency

Current scanlab dependency:

```toml
okf = { version = "0.3.0", path = "../open-knowledge-format/crates/okf" }
```

Planned transition:

1. Keep the local path while both repositories are developed together.
2. Replace it with a Git dependency if scanlab should follow OKF before a
   crates.io release.
3. Replace it with `okf = "0.3"` after `okf 0.3.0` is published.

Completion criteria:

- scanlab imports OKF as an ordinary external dependency.
- scanlab does not build or install `okf-http`.

## Phase 5: Resolve or Contain `rustls-pemfile`

`cargo audit` currently reports `rustls-pemfile` as unmaintained. It is used by
`okf-http` for certificate and private-key PEM loading.

Options:

1. Replace it with an actively maintained PEM parser.
2. Implement a small, well-tested, limited PEM loader for the exact certificate
   and private-key formats OKF accepts.
3. Keep it temporarily as an explicitly documented allowed warning while
   monitoring RustSec and Rustls ecosystem changes.

Completion criteria:

- Either the warning is gone, or the release notes and security policy clearly
  explain the temporary exception and its scope.

## Phase 6: Browser Document-Root Onboarding

- Finish the browser flow for adding OKF document roots.
- Store browser-managed roots in `~/.config/okf/config.toml`, never by editing
  `.env`.
- Preview admitted and rejected files before registration.
- Reject hidden files, symlinks, dangerous file types, and traversal attempts.
- Propose missing `index.md`, frontmatter, `type`, and resource declarations
  without silently modifying source files.
- Require explicit confirmation for both registration and source
  initialization.

Completion criteria:

- A user can onboard a non-OKF folder safely through the browser.
- No source or configuration mutation happens without review.

## Phase 7: Opt-In Change Monitoring UX

- Let users opt into change monitoring per document root.
- On restart or browser refresh, show that changes exist without applying them.
- Reuse the same review rules as initial onboarding.
- Keep dismissal separate from acceptance; dismissal must not advance the
  baseline.

Completion criteria:

- External file changes are visible, reviewable, and never auto-applied.

## Phase 8: Authentication and TLS Usability Polish

- Keep anonymous reading simple.
- Make protected actions clearly require pairing or authenticated HTTPS.
- Improve guidance for users without root/admin rights.
- Document the local CA workflow without requiring system trust-store changes.
- Ensure browser messages distinguish read-only, local-editor, authenticated,
  and trusted-proxy modes.

Completion criteria:

- A Markdown-focused user can start read-only OKF without security ceremony.
- A power user can enable protected actions with clear, local-only steps.

## Phase 9: scanlab Compatibility Adapter Decision

`okf-http --scanlab-compat` is an explicit transition adapter, not a core OKF
dependency.

Options:

1. Keep it for one or two OKF minor releases.
2. Mark it deprecated after scanlab no longer needs legacy browser paths.
3. Remove it before OKF 1.0.

Completion criteria:

- The adapter's lifecycle is documented.
- Default OKF behavior remains scanlab-independent.

## Phase 10: CI and Release Automation

- Ensure CI runs the same release gate used locally.
- Decide whether `cargo audit` warnings should fail CI or be allowlisted by an
  explicit audit policy file.
- Add a release job or checklist step for publishing order:
  1. `okf`
  2. `okf-http`
  3. downstream scanlab dependency update
- Keep live Voyage AI tests opt-in and ignored by default.

Completion criteria:

- A release can be reproduced from a clean checkout.
- CI catches dependency vulnerabilities, package drift, browser drift, and
  route-security regressions before publication.
