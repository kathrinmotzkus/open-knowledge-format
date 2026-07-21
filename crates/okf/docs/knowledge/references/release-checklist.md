---
title: OKF Community Release Checklist
type: Checklist
kind: knowledge-reference
topic: okf-release
status: active
updated: 2026-07-21
tags: [okf, release, ci, security, packaging]
---

# OKF Community Release Checklist

This checklist covers source and binary community releases of
`okf-open-knowledge-format` and `okf-http`. `okf-open-knowledge-format 0.3.2`
and `okf-http 0.4.3` are the current prepared release versions. The library
package exports the Rust crate name `okf`.

## Clean Source

- [ ] Start from a clean checkout of the intended commit.
- [ ] Confirm the Apache-2.0 `LICENSE` files are byte-identical and package
      metadata declares `Apache-2.0`.
- [ ] Confirm every normal `docs/knowledge` document has non-empty `type`
      frontmatter; only reserved `index.md` and `log.md` are exempt.
- [ ] Review `git status --short` for generated or private files.

## Required Local Gate

Fetch dependencies once, remove provider credentials from the environment, and
run:

```bash
cargo fetch --locked
env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY \
  -u OKF_HTTP_TRUSTED_PROXY_TOKEN scripts/check-okf-release.sh
```

The script verifies formatting, strict Clippy, offline locked workspace tests,
RustSec audit status, JavaScript syntax, browser security tests,
browser-source synchronization, root-security invariants, package contents, and
whitespace errors.

`cargo audit` must report no known vulnerabilities and no unreviewed
unmaintained dependency warnings. `okf-http` intentionally uses a limited
internal PEM loader for TLS certificate chains and private keys so the release
does not depend on the unmaintained `rustls-pemfile` crate.

## Versions and Changelogs

- [ ] The `okf-open-knowledge-format` and `okf-http` versions reflect their
      independent release scopes; matching versions are never assumed
      automatically.
- [ ] Every internal version requirement accepts exactly the intended release.
- [ ] Each component's `Unreleased` entries have moved into a dated release
      section without deleting the new empty `Unreleased` section.
- [ ] Breaking format, Rust API, HTTP API, and database changes are identified
      separately in the release notes.
- [ ] The release commit receives the matching Git tag or tags.

## Minimum Rust and Platforms

- [ ] `RUSTUP_TOOLCHAIN=1.85 scripts/check-okf-msrv.sh okf` passes.
- [ ] `RUSTUP_TOOLCHAIN=1.88 scripts/check-okf-msrv.sh okf-http` passes.
- [ ] GitHub Actions passes on Linux and macOS stable Rust.
- [ ] The CI release-gate job passes with an empty
      `OKF_VOYAGE_API_KEY` and Cargo offline after dependency fetch.
- [ ] Before publishing any package to crates.io, all required GitHub test and
      release-gate jobs for the exact commit being released have completed
      successfully.

## Package Contents

- [ ] `scripts/check-okf-package-contents.sh` passes.
- [ ] Cargo can build the `okf-open-knowledge-format` `.crate` archive with
      `--locked --no-verify`, and `cargo publish -p
      okf-open-knowledge-format --dry-run --locked` passes before publication.
- [ ] `okf-http` has a versioned `okf-open-knowledge-format` dependency
      aliased as `okf`.
- [ ] Platform-specific `okf-http` modules are included in the `okf-http`
      package list; no separate platform packages are published for this
      internal code.
- [ ] Both package lists contain their license, README, and changelog.
- [ ] `okf-open-knowledge-format` contains its documentation, contribution
      guide, security policy, versioning roadmap, library sources, and offline
      tests.
- [ ] `okf-http` contains all browser assets, the browser security test, and
      the standalone packaged-installation test.
- [ ] No `.env`, API key, `.okf-voyage/`, SQLite/WAL/SHM database, accepted-edge
      export, editor lock/swap file, or private physical path is included.
- [ ] `src/monitoring.rs` and all packaged browser onboarding assets are in the
      `okf-http` package list.
- [ ] Packaged browser JavaScript contains no `localStorage` review state.

## Standalone Behavior

- [ ] The `standalone_e2e` test installs browser assets from the compiled
      binary into a temporary directory.
- [ ] It starts `okf-http` with only a temporary mounted document root.
- [ ] Browser, inventory API, logical route, and Markdown fetch all succeed
      without scanlab files or compatibility routes.
- [ ] Ordinary tests make no Voyage call; the paid live test remains ignored.
- [ ] CI sets `OKF_REQUIRE_SOCKET_E2E=1`, so a runner that cannot bind loopback
      fails instead of silently accepting the local restricted-sandbox fallback.
- [ ] Release gates run without Voyage credentials or a trusted-proxy secret;
      no paid provider request can be made accidentally.

## Security and Cost Review

- [ ] `cargo audit` reports no vulnerabilities. Any non-vulnerability warnings
      are explicitly documented in the release notes or follow-up plan.
- [ ] Default binding remains loopback-only.
- [ ] Default startup is read-only, creates no token file, and does not mount
      mutating, sensitive-read, canonical-write, derived-state, or
      token-spending routes.
- [ ] Explicit local-editor mode is loopback-only; pairing is one-time,
      expiring, attempt-limited, and creates only an in-memory session.
- [ ] Paired mutations require the `HttpOnly` session cookie, CSRF header,
      exact Host/Origin, and acceptable Fetch Metadata; logout, expiry, and
      restart revoke authority.
- [ ] The transitional session-token path cannot bypass read-only route
      omission and is documented as temporary.
- [ ] Browser pairing is keyboard-accessible, displays scope and expiry without
      rendering credentials, and supports explicit logout and expired sessions.
- [ ] Browser assets contain no legacy token field, credential storage, or
      Markdown path capable of invoking a protected endpoint.
- [ ] Root writes additionally require `X-OKF-Config-Write: confirm`.
- [ ] Root change acceptance requires an exact digest and
      `X-OKF-Change-Review: accept`; dismissal never advances the baseline.
- [ ] Added and modified monitored files remain unavailable before acceptance.
- [ ] API keys, pairing codes, and session identifiers are absent from
      responses, logs, Markdown, browser assets, test output, and packages.
- [ ] CSRF values appear only in successful pair/refresh responses and remain
      absent from logs, rendered text, URLs, exports, and browser storage.
- [ ] Authenticated mode refuses missing, malformed, mismatched, expired,
      not-yet-valid, wrong-SAN, or insecurely permissioned TLS material before
      binding and never falls back to HTTP.
- [ ] Plain HTTP remains loopback-only.
- [ ] Intranet and Internet deployments keep `okf-http` bound to loopback and
      expose only a reverse proxy to the network.
- [ ] Trusted-proxy reads require a persistent account; remote document-root
      inspection and mutation remain unavailable.
- [ ] Trusted-proxy mode requires a loopback HTTPS backend, exact public HTTPS
      origin, matching singular forwarded host/protocol fields, and a separate
      deployment secret; direct-backend bypass and HTTP downgrade fail.
- [ ] The local CA workflow changes only owner-private user files. Tests and
      installation never modify a system or browser trust store and never need
      root privileges or external TLS executables.
- [ ] Indexing still presents a token/request estimate before the browser asks
      for confirmation.
- [ ] Release notes distinguish token-free browsing/rebuild from paid Voyage
      connectivity, indexing, and semantic search.

## Documentation and Migration

- [ ] The standalone quickstart commands match the released binary.
- [ ] CLI/environment/`.env` precedence and mount behavior are current.
- [ ] API v1, SQLite rebuild, canonical relation, migration, security, and
      contribution documentation match tested behavior.
- [ ] The route-contract gate still rejects unclassified routes, authenticated
      HTTP login, anonymous mutation, and token-spending routes without
      `voyage.spend`; browser gates reject persisted credentials.
- [ ] Relative documentation links resolve.
- [ ] Browser assets are reprovisioned after upgrade with
      `okf-http --install-browser`; `--force` is used only intentionally.
- [ ] Two tabs or server instances using one configuration revision cannot
      both commit; the stale writer receives a revision conflict.

## Final Decision

- [ ] Record the tested commit, Rust versions, operating systems, and gate
      results in the release notes.
- [ ] Resolve every CI failure; do not waive security, package-content,
      no-token, or standalone-installation gates.
