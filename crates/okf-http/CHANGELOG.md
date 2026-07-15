# Changelog

All notable changes to `okf-http` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/). During the
pre-1.0 period, a minor release may contain breaking changes.

## Unreleased

No unreleased changes yet.

## [0.3.0] - 2026-07-15

This release publishes `okf-http` as the standalone local HTTP server for OKF.
It completes the local-access/authentication/TLS and secure document-root
onboarding plans. It is a pre-1.0 minor release because it adds substantial
HTTP, browser, configuration, security, and persistent-state contracts.

### Added

- Added versioned, owner-private XDG document-root configuration with atomic
  writes, stable root identities, priorities, enablement and change-monitoring
  preferences, plus a console-only `.env` import command.
- Added authorized snapshot-proposal, configuration-status, registration,
  initialization-plan, initialization, update, and removal APIs with capability
  enforcement, CSRF and origin protection, confirmation headers, revisions,
  structured stale/conflict errors, remote denial, and privacy-safe auditing.
- Added an accessible browser document-root onboarding view with a complete
  filterable proposal tree, visible rejection reasons and override warnings,
  review-gated registration, and separately reviewed source initialization.
- Added opt-in, token-free document-root change detection with asynchronous
  startup scans, explicit subsequent checks, SQLite baselines and pending
  reviews, classified changes, stale-snapshot rejection, browser review, and
  baseline-gated document serving.
- Added explicit root-security release gates, concurrent configuration-revision
  coverage, packaged monitoring checks, and Linux/macOS/MSRV CI coverage for
  the complete standalone onboarding workflow.

- Added a central route capability matrix covering anonymous reads, sensitive
  reads, host mutations, canonical writes, derived-state mutations,
  token-spending actions, planned security administration, access modes, and
  required capabilities.
- Added a standalone Axum server for the packaged OKF documentation browser.
- Added configured and mounted document roots with traversal and symlink escape
  protection.
- Added versioned document, graph, configuration, Voyage AI, review, and
  canonical-relation APIs.
- Added SQLite-backed derived state, incremental migrations, integrity checks,
  rebuild support, and recoverable review records.
- Added explicit browser installation, loopback defaults, session-token
  authorization, origin checks, and remote-bind safeguards.
- Added one-time local-editor pairing, short-lived server-side sessions,
  `HttpOnly`/`SameSite=Strict` cookies, CSRF-bound requests, logout, expiry,
  replay protection, and pairing-attempt throttling.
- Added an accessible browser pairing dialog, session scope and expiry display,
  explicit logout, reload-safe same-origin CSRF recovery, and protected-control
  state without persistent browser credential storage.
- Added Rustls-based HTTPS with explicit certificate/key paths, pre-bind SAN,
  validity, key-match, parse, and Unix key-permission validation, plus
  non-secret certificate status and HTTPS end-to-end coverage.
- Added Rust-native `tls init`, `status`, `verify`, and `renew` commands for an
  optional local CA and localhost certificate in owner-only XDG state, without
  trust-store mutation, root privileges, or external executables.
- Added CLI-only persistent user, password, role, disablement, removal, and
  listing commands backed by a private versioned SQLite database, Argon2id
  hashes, random salts, parameter upgrades, uniform failure handling, and
  security audit events.
- Added HTTPS-only browser login, secure server-side sessions, CSRF recovery,
  self-service password change, session revocation, administrative user/session
  inspection, and live invalidation after password, role, disablement, or
  removal changes.
- Connected every productive route contract to shared capability-enforcement
  middleware, including separate `voyage.spend`, `users.manage`, and
  `security.manage` boundaries.
- Added explicit direct-remote and loopback trusted-proxy boundaries with
  backend TLS, exact public-origin validation, a separate proxy secret,
  forwarded-header ambiguity/downgrade rejection, authenticated remote reads,
  and disabled remote root administration.
- Added standalone end-to-end, browser security, API, storage, and CLI tests.
- Added offline release gates for access-mode classification, anonymous-read
  continuity, protected mutations and Voyage actions, pairing and persistent
  session lifecycle, TLS validation, trusted-proxy downgrade resistance,
  browser credential handling, and package-secret hygiene.
- Restricted every OKF document route to freshly admitted Markdown and valid,
  uniquely declared CSV resources, closing arbitrary regular-file serving and
  adding fixed safe content types plus escaped CSV rendering warnings.

### Changed

- Moved `okf-http` and its browser into the independent
  `open-knowledge-format` workspace.
- Updated package metadata for independent publication on crates.io.
- Added the standalone project installer, including explicit one-time cleanup
  of the former scanlab-owned system installation.
- Changed browser navigation to follow actual mounted directory structure;
  frontmatter topics no longer create synthetic navigation folders.
- Collapsed unrelated navigation directories by default while keeping the
  current document ancestry and directories containing search matches open.
- Reduced anonymous document-root management to an authentication notice and
  linked guide; configuration forms and details render only after login or
  local-editor pairing.
- Retired direct browser mutation through `/config/roots`; mutation aliases now
  return `410 Gone`, while `.env` remains console-owned and read-only to HTTP.
- Made read-only operation the default, added explicit loopback-only
  `--local-editor` mode, omitted sensitive routes and token files from
  read-only startup, and exposed the active mode through the health API and
  browser.
- Protected sensitive root-configuration reads with the same authorization
  boundary as other local-editor operations.
- Standardized the workspace MSRV on Rust 1.85 while retaining the maintained
  `axum-server` 0.8 TLS stack.
- Made plain HTTP loopback-only and require authenticated TLS, explicit opt-in,
  a concrete bind address, and explicit automation authority for remote binds.
- Replaced the Python development-server workflow with an installable Rust
  binary.
- Kept scanlab routes behind an explicit compatibility adapter instead of
  making them part of generic OKF behavior.
- Established `okf-http` as independently versioned from both scanlab and the
  `okf` library.

### Security

- Updated the pinned `time` dependency from 0.3.41 to 0.3.47 to resolve the
  RustSec stack-exhaustion advisory inherited through TLS/X.509 dependencies.
- Added `cargo audit` to the release gate so dependency vulnerabilities are
  checked together with formatting, Clippy, tests, browser security checks, and
  package-content checks.
- Switched the internal OKF dependency to the publishable package name
  `okf-open-knowledge-format` while keeping the local crate alias `okf`.
