# Changelog

All notable changes to OKF are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/). During the
pre-1.0 period, a minor release may contain breaking changes.

## Unreleased

No unreleased changes yet.

## [0.3.1] - 2026-07-15

### Fixed

- Clarified the crates.io installation path for users who install
  `okf-http` without a GitHub source checkout.
- Clarified that the OKF browser is served by the separately published
  `okf-http` binary and can be installed with `cargo install okf-http`.

## [0.3.0] - 2026-07-15

This release publishes the extracted OKF library as
`okf-open-knowledge-format` on crates.io. It establishes the format, identity,
admission, compliance, proposal, configuration, and transactional foundations
used by secure standalone OKF root onboarding. It is a pre-1.0 minor release
and may contain intentional API changes from the earlier extraction snapshot.

### Changed

- Moved `okf`, `okf-http`, their browser, release tooling, and documentation
  from the scanlab workspace into the independent
  `open-knowledge-format` repository.
- Updated package metadata for independent publication on crates.io.
- Renamed the crates.io package to `okf-open-knowledge-format` while keeping
  the Rust library crate name `okf` for source compatibility.

### Added

- Added stable `okf://<mount>/<relative-path>` logical document URIs without
  exposing physical filesystem paths.

### Added

- Added the versioned XDG browser-configuration model, schema migration,
  private atomic persistence, stable-root import, and explicit precedence
  support for browser-managed roots below operator overrides.
- Added deterministic, snapshot-bound registration and source-initialization
  proposals with content digests, complete review trees, conflict analysis,
  expiring in-memory storage, and mandatory stale revalidation.
- Added revision-locked root registration and separately confirmed source
  initialization with exact diffs, explicit CSV resource types, portable root
  identity generation, external staging and rollback journals, permission and
  line-ending preservation, Git dirty-state reporting, and crash recovery.
- Added the initial public API boundary for the OKF extraction.
- Added ordered document roots, repository options, document queries,
  diagnostics, and structured errors.
- Added recursive Markdown discovery, frontmatter parsing, planning-section
  extraction, plan classification, root priority, collision diagnostics, and
  exact or partial document lookup.
- Added optional logical mount names so independent document trees can share a
  repository without collisions while same-mount roots retain priority
  shadowing.
- Added the shared typed document-root parser, formatter, deduplication, and
  precedence merge used by scanlab and `okf-http`.
- Added OKF `type` metadata as the primary concept classification while keeping
  the older `kind` metadata as a compatibility field.
- Added a non-fatal diagnostic for non-reserved Markdown documents that are
  missing the required OKF `type` field.
- Added an optional `okf::voyage` analysis module with OKF-prefixed
  configuration, explicit connectivity checks, repository inventory, token
  planning, deterministic Markdown chunking, local file-backed embedding index
  storage, semantic search, AI-marked suggested graph edges, and review actions.
- Added an OKF AI review surface to the documentation browser with a prominent
  analysis button, durable SQLite review state, JSON export, explicit canonical
  application, and distinct review/canonical edge visualization in the graph.
- Added typed canonical frontmatter relations, atomic and idempotent application
  of accepted AI suggestions, recoverable SQLite auditing, and graph/browser
  projection with complete AI provenance but without raw vectors.
- Added transactional SQLite generation tracking, stale-derived-row cleanup,
  integrity reporting, WAL concurrency, incremental migrations, and token-free
  rebuild/recovery through `okf-http`.
- Added durable Voyage similarity review sets, authenticated single/all review
  actions, validated review-artifact import, and browser API integration without
  placeholder models or browser-only durable state.
- Added public portable `RootId`, `DocumentId`, and `PortablePath` types,
  identity extraction from OKF frontmatter, Unicode-aware collision keys, and
  blocking collision detection without rewriting display paths.
- Added a dependency-light document-root admission scanner with deterministic
  inventories, Markdown/CSV text verification, stable rejection reasons,
  hidden/symlink/non-regular-file protection, and closed scan-limit behavior.
- Added write-free OKF compliance reports, deterministic missing-index and
  frontmatter proposals, typed same-directory CSV resource declarations,
  structural CSV analysis, and spreadsheet-formula warnings.

### Changed

- Made file-backed Voyage snapshots atomic per file and model-aware when
  deciding whether an embedding is still reusable.
- Established `okf` and `okf-http` as independently versioned components; both
  use `0.3.0` for this coordinated format and secure-onboarding milestone.
