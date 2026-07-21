---
title: OKF Community Readiness Remediation Plan
type: Plan
kind: knowledge-plan
topic: okf
status: completed
updated: 2026-07-02
tags: [okf, community, hardening, security, packaging, release]
---

# OKF Community Readiness Remediation Plan

This plan closes the gaps found by the post-implementation audit of phases
1–10 in the OKF HTTP server plan. Its target is a community-usable source and
binary release whose behavior is secure, portable, documented, and internally
consistent.

Publishing `okf` or `okf-http` to crates.io is explicitly outside this plan.
The release gates below must be satisfied before a separate publication plan is
created.

All remediation phases and release gates are complete. Their combined result,
together with the access/TLS and secure-root-onboarding plans, is prepared as
the coordinated 0.3.0 source and binary release. crates.io publication remains
a separate future decision.

## Audit Baseline

The following list records conditions at the start of remediation. Completed
phase status notes below supersede it; it is not a description of the current
implementation.

The existing implementation has a sound Rust crate boundary, working static
serving, read-only metadata APIs, token-free Voyage planning, explicit Voyage
execution routes, SQLite storage, and an accepted-relation writer. The
following gaps prevent community release:

- the browser and compatibility routes remain scanlab-specific
- unmounted roots appear in the API but cannot be served by their generated
  browser paths
- mutating and token-spending loopback endpoints have no origin/CSRF defense
- rendered Markdown accepts unsafe URL schemes such as `javascript:`
- API responses expose physical filesystem paths by default
- the configured Voyage timeout is not applied by the transport
- direct indexing of a fresh SQLite database does not synchronize document and
  chunk inventory before writing embeddings
- SQLite has no productive suggestion-generation or review-write path
- browser review state remains isolated in `localStorage`
- accepted frontmatter relations are not read back into the graph/browser
- documentation describes an end-to-end workflow that does not yet exist
- strict Clippy currently fails
- no CI or documented minimum supported Rust version protects the public
  contract

## Release Invariants

Every remediation phase must preserve these rules:

- canonical OKF knowledge remains Markdown plus frontmatter
- SQLite and embedding files remain rebuildable derived/work state
- normal repository loading and normal browsing spend no Voyage tokens
- token-spending and canonical writes require explicit user actions
- API keys are never returned to the browser or written to canonical files
- configured roots and browser assets never grant access to parent directories
- `okf` remains independent from Axum, SQLite, and `okf-http`
- scanlab compatibility may exist as an adapter, but not as the generic OKF
  default

## Phase 1: Legal, Package, and Repository Baseline

- Use Apache License 2.0 consistently across the workspace.
- Keep the unmodified official Apache License 2.0 text at repository level.
- Ensure both crate packages include the license file or reference it through
  `license-file` where appropriate.
- Add package metadata needed for community builds:
  - `rust-version`
  - useful keywords and categories
  - documentation/homepage links when stable
- Remove stale implementation wording such as `server skeleton`.
- Verify editor lock files, secrets, SQLite databases, and local browser state
  are excluded from packages and Git.
- Define whether `okf-http` remains `publish = false`; do not change that flag
  as part of this plan without a separate publication decision.

License status: implemented. The workspace and all four component crates use
the SPDX identifier `Apache-2.0`. The repository and every separately
packageable component contain byte-identical copies of the official Apache
License 2.0 text as `LICENSE`.

Completion criteria:

- `cargo package -p okf-open-knowledge-format --allow-dirty --list` includes
  the license.
- `cargo package -p okf-http --allow-dirty --list` includes the license and
  server sources. Browser asset packaging and provisioning are completed in
  Phase 3.
- No generated, secret, editor, or local database artifact enters either
  package.

Status: implemented. Repository and package licensing use Apache-2.0, all
workspace packages reference the byte-identical official license file, package
metadata declares explicit minimum Rust versions, stale skeleton wording was
removed, and local secret/index/editor/export artifacts are ignored. Both OKF
package lists contain `LICENSE`. Browser asset distribution remains Phase 3.

## Phase 2: Shared CLI and Configuration Contract

- Move document-root specification parsing into an OKF-owned shared API so
  scanlab and `okf-http` cannot drift.
- Parse `<path>` and `<mount>=<path>` into a typed result with explicit errors.
- Reject malformed mount syntax instead of treating it as a literal path.
- Define and implement exact precedence:
  1. CLI
  2. process environment
  3. `.env`
  4. application defaults
- A CLI root with an existing mount must replace that mount's persistent roots
  for the current process. Additive roots must have a separate documented
  behavior if both operations are needed.
- Support IPv4, hostnames, and IPv6 bind addresses without string-concatenation
  ambiguity.
- Validate mount names once at configuration load time.
- Report missing/unreadable roots at startup while retaining OKF's ordered
  fallback-root behavior.
- Add an explicit configuration API for listing, adding, removing, and editing
  persistent roots.
- Persist root changes to `.env` atomically while preserving unrelated entries
  and never returning or rewriting the Voyage API key accidentally.

Completion criteria:

- scanlab and `okf-http` pass the same root parser contract suite.
- CLI override behavior is proven for duplicate mount names.
- malformed roots fail before server startup with actionable errors.
- `127.0.0.1`, `localhost`, `::1`, and explicit all-interface bindings have
  tests.
- persistent root edits survive restart and preserve unrelated `.env` values.

Status: implemented and subsequently superseded for browser persistence by the
secure root-onboarding plan. `okf` owns the typed root parser, formatter, deduplication,
and precedence merge used by both scanlab and `okf-http`. CLI roots replace
persistent roots with the same mount; `--add-root` provides explicit fallback
semantics. Malformed mounts fail early. IPv4, hostnames, and IPv6 are accepted
without URL-format ambiguity. The HTTP configuration API lists and atomically
edits persistent `.env` roots while preserving unrelated values and reporting
active process-environment overrides. Writes currently require the interim
`X-OKF-Config-Write: confirm` safeguard; Phase 5 replaces it with the general
session security model. Browser APIs now write the private versioned XDG
`config.toml` instead; `.env` is read-only operator configuration and can be
imported only through an explicit console command.

## Phase 3: Browser Asset Distribution

- Keep the user's browser directory at `~/docs-browser` on Linux as already
  decided.
- Package a versioned source copy of the browser assets with `okf-http`.
- Add an explicit provisioning command, for example:

  ```text
  okf-http --install-browser
  ```

- Provision atomically and never overwrite user-modified assets without an
  explicit confirmation/force option.
- Report a clear startup error with the provisioning command when browser
  assets are missing.
- Document development, packaged, and upgraded browser workflows separately.

Completion criteria:

- A user can build/install `okf-http`, provision `~/docs-browser`, start the
  server, and load the browser without a scanlab checkout.
- Package tests prove that `index.html`, JavaScript, CSS, and vendored Cytoscape
  are available to the provisioning workflow.
- Browser asset upgrades are deterministic and preserve user-owned changes.

Status: implemented. The `okf-http` package contains a versioned browser source
copy and embeds all required assets. `okf-http --install-browser` provisions
them atomically into `~/docs-browser` or an explicit browser root. A manifest
tracks package version and content hashes; unchanged installs are idempotent,
modified packaged files require `--force`, and unrelated user files survive an
upgrade. Server startup reports the exact provisioning command when assets are
missing.

## Phase 4: Generic Root Routing and Dynamic Browser Discovery

- Replace the hard-coded scanlab `DOCS` catalog with the versioned document
  inventory API as the primary browser inventory.
- Generate navigation groups from OKF metadata and logical paths.
- Replace the `scanlab.okf.*` local-storage namespace with an OKF-owned,
  repository-scoped namespace.
- Define a routable logical namespace for unmounted roots, or reject unmounted
  roots for HTTP serving with a clear CLI error. API-generated `browser_path`
  values must always resolve.
- Move scanlab legacy paths and `/repo-files/` behavior behind an explicit
  compatibility adapter or flag; they must not be generic OKF defaults.
- Test arbitrary roots outside the scanlab repository, nested roots, duplicate
  mount fallbacks, spaces, and non-ASCII path components.

Completion criteria:

- A standalone OKF repository appears in the browser without editing
  `app.js`.
- Every document returned by the metadata API has a working browser path.
- The default OKF server contains no hard-coded scanlab, SCQL, or repository
  README assumptions.
- Compatibility mode remains available to scanlab without entering the core
  OKF route contract.

Status: implemented. The browser now builds its navigation from
`GET /api/v1/documents`, groups documents using configured-root and OKF
metadata, and stores accepted AI state in an OKF-owned namespace scoped to the
current root/document inventory. Mounted roots remain under `/okf-docs/`, while
unmounted roots use the routable `/okf-root/<root-index>/` namespace. API paths
are URL-encoded and tested with nested directories, spaces, and non-ASCII
names. Duplicate mounted roots retain fallback behavior. scanlab's old
document routes and `/repo-files/` are disabled by default and are available
only with `--scanlab-compat`. The packaged and development browser sources are
kept identical and contain no scanlab, SCQL, or repository-README catalog
entries.

## Phase 5: HTTP and Markdown Security Hardening

- Allow only explicitly supported Markdown URL schemes:
  - same-origin OKF document routes
  - `http`
  - `https`
  - optionally `mailto`
- Block `javascript:`, `data:`, `file:`, credential-bearing URLs, malformed
  URLs, and attribute-injection attempts.
- Escape every generated HTML attribute, not only visible labels.
- Add a restrictive Content Security Policy and appropriate security headers.
- Add origin/CSRF protection for every token-spending or mutating endpoint.
- Require an explicit local session token for:
  - Voyage connectivity checks
  - Voyage indexing
  - semantic search when it spends tokens
  - persistent root changes
  - review-state changes
  - canonical relation application
- Refuse non-loopback binding for mutating APIs unless the user explicitly
  enables remote access and configures authentication.
- Keep traversal and symlink protections and add adversarial URL-encoding tests.

Completion criteria:

- Browser tests prove unsafe Markdown URLs are inert.
- Cross-origin form and fetch attempts cannot spend tokens or modify files.
- Security headers are present on browser and API responses.
- Plain, encoded, double-encoded, and symlink traversal suites pass.
- A documented threat model covers browser assets, roots, `.env`, SQLite,
  Voyage credentials, and canonical writes.

Status: implemented. Markdown URL handling is isolated in a testable browser
security module. Only same-origin OKF document routes and credential-free
external `http`, `https`, and `mailto` targets become links; active, local,
malformed, credential-bearing, and control-character URLs remain inert. HTML
text and attribute delimiters are escaped. Every response receives a
restrictive Content Security Policy plus MIME-sniffing, clickjacking, referrer,
and browser-permission headers.

Token-spending and mutating APIs require a cryptographically generated
`X-OKF-Session-Token`; root writes retain their additional explicit
confirmation header. Mismatching `Origin` and cross-site Fetch Metadata are
rejected before side effects. Plain HTTP non-loopback binding is refused. The
later reverse-proxy deployment policy keeps `okf-http` on loopback for intranet
and Internet deployments.
Tests cover all sensitive routes, cross-site requests, security headers,
symlink escapes, and plain, encoded, and double-encoded traversal attempts. The
[OKF HTTP threat model](../security/http-threat-model.md) documents protected
assets, controls, and residual risk.

## Phase 6: Stable and Privacy-Aware HTTP API

- Introduce a versioned API namespace or an explicit version field and
  compatibility policy before community consumers depend on the current JSON.
- Publish request/response schemas for documents, graph data, Voyage actions,
  review state, and errors.
- Return stable machine-readable error codes in addition to human messages.
- Do not expose physical root or document paths by default. Make local debug
  paths an explicit opt-in capability.
- Separate read-only status from state-changing synchronization. A `GET` status
  request must not create or rewrite SQLite state.
- Move repository scans, Markdown graph extraction, and other blocking
  filesystem work off asynchronous request workers.
- Add request IDs, structured logging, graceful shutdown, and useful startup
  diagnostics without logging secrets.
- Split the 2,500-line server module into CLI/config, routing, static serving,
  API, storage, Voyage, and relation modules.

Completion criteria:

- API contract tests validate complete JSON bodies and error codes.
- Default responses reveal logical paths but no physical filesystem layout.
- Read-only endpoints remain responsive during indexing and repository scans.
- Public API compatibility rules are documented.
- Logs identify failed operations without exposing API keys or document
  contents unnecessarily.

Status: implemented. `/api/v1` is the stable namespace and all JSON responses
use a versioned success or error envelope. Errors carry stable codes; 5xx
details remain in request-ID-correlated structured logs instead of responses.
The former `/api/okf` aliases emit deprecation, sunset, and successor-version
headers. The [API v1 contract](../integration/http-api-v1.md) documents schemas
and compatibility rules.

Physical filesystem, SQLite, and index paths are omitted by default and can be
enabled only with `--expose-physical-paths`. Repository discovery, graph
extraction, planning, indexing preparation, search preparation, relation
application, configuration I/O, and static canonicalization run on blocking
workers instead of async request workers. `GET voyage/status` uses read-only
SQLite inspection and neither creates nor synchronizes derived state.

Responses carry request IDs, security/version headers, and `no-store` for API
data. Startup and request diagnostics are structured and exclude session
tokens, query strings, API keys, and document bodies. The session token is
written to a private runtime file and removed during graceful shutdown.
Server responsibilities are separated into CLI, routing, security, static
file, API contract, storage inspection, Voyage policy, and relation modules;
`lib.rs` retains endpoint orchestration and the mutable SQLite implementation
that Phase 8 will make transactional.

## Phase 7: Voyage Transport and Execution Correctness

- Apply `OKF_VOYAGE_TIMEOUT_SECONDS` to connection and total request time.
- Map timeout, DNS, TLS, process-launch, HTTP, and provider failures to distinct
  safe error codes.
- Keep provider HTTP status and safe provider error details in responses.
- Verify returned embedding count and dimensions before mutating any index.
- Preserve the existing batch planning and TPM/RPM pacing behavior.
- Define cancellation and shutdown behavior for running indexing operations.
- Prevent simultaneous index jobs for the same index root.

Completion criteria:

- Deterministic transport tests cover timeout and malformed provider responses.
- OpenSnitch-style delayed connections terminate with a clear timeout.
- Failed or partial batches leave both index backends unchanged.
- Normal tests remain token-free; live tests remain explicit and tiny.

Status: implemented. `OKF_VOYAGE_TIMEOUT_SECONDS` now controls curl's connect
and per-request total timeout. Deterministic tests exercise a deliberately
delayed loopback response, malformed success data, provider diagnostics, and
the distinct timeout/DNS/TLS/launch/HTTP/transport error categories without
spending Voyage tokens.

Every batch is validated for response count, finite non-empty vectors, and a
single dimension shared with earlier batches and the existing index before
the in-memory index is rebuilt. Partial failures leave it untouched, and the
HTTP index path does not open, migrate, save, or synchronize either persistent
backend after a provider failure. Existing batching, TPM/RPM pacing, and the
single 429 retry remain unchanged.

`okf-http` serializes index jobs by normalized index root and returns HTTP 409
for a duplicate in-process request. Accepted v1 jobs intentionally have no
mid-job cancellation: disconnects do not produce partial commits, while
graceful shutdown waits for the operation to finish or fail and each provider
exchange remains timeout-bounded.
Cross-process storage locking and transactional two-backend commit/recovery
remain part of Phase 8's consistency work.

## Phase 8: Transactional SQLite and Index Consistency

- Make `index` synchronize document and chunk inventory itself before writing
  embeddings; it must not depend on a prior `status` or `plan` call.
- Define one authoritative transaction/order for file-backed and SQLite index
  updates, including failure recovery.
- Add foreign keys or explicit integrity checks between chunks, embeddings,
  suggestions, and applied relations.
- Remove stale embeddings and suggestions when chunks disappear or hashes
  change.
- Configure SQLite busy timeout/WAL behavior for concurrent local requests.
- Make schema migrations incremental, versioned, and tested from every released
  schema version.
- Add a rebuild command and integrity/status report.

Completion criteria:

- Delete-database → direct-index → semantic-search works without another API
  call in between.
- Restart, interrupted write, stale chunk, concurrent read, and migration tests
  pass.
- SQLite and the file-backed index cannot silently disagree after a failed
  operation.
- Deleting all derived state never damages canonical Markdown.

Status: implemented. The index action now inventories documents and chunks
itself and commits inventory, chunks, embeddings, valid suggestions, stale-row
cleanup, integrity checks, and a new generation in one SQLite transaction.
SQLite schema 3 adds generation state, chunk tags, integrity triggers, WAL,
full synchronous commits, and a five-second busy timeout. Incremental
migrations from every released schema version (1 and 2) preserve valid data
and are covered by fixtures.

SQLite is the authoritative derived backend. Only after its transaction commits
does OKF atomically replace the TSV files and generation marker. Status detects
content or generation disagreement; the authenticated, token-free
`POST /api/v1/voyage/rebuild` action reconstructs current inventory, retains
hash-matching embeddings, and repairs both backends. A failed SQLite
transaction leaves the previous database generation and file snapshot intact.

Tests cover direct index-to-search without preparatory status/plan calls,
restart loading, rollback after an invalid write, stale chunk/hash cleanup,
concurrent WAL readers, migrations, mirror damage and recovery, and deletion of
all derived state without changing canonical Markdown.

## Phase 9: Productive Suggestion and Review API

- Generate Voyage similarity suggestions from indexed embeddings through a
  server-side action.
- Persist suggestions and full AI provenance in SQLite.
- Add authenticated endpoints to:
  - list suggestions
  - accept one
  - accept all in a defined review set
  - deny one
  - deny all in a defined review set
  - import a validated browser review artifact if export/import remains useful
- Connect the browser's analysis, semantic search, save, accept, and deny
  controls to these endpoints.
- Remove `pending-voyage-ai` placeholders from persisted suggestions.
- Keep local prefiltering visually distinct from Voyage-derived suggestions.

Completion criteria:

- A user can plan, index, generate, review, restart, and recover the same review
  state from SQLite.
- Browser acceptance creates the exact accepted SQLite row consumed by the
  apply endpoint.
- Every suggestion is visibly and machine-readably marked with provider,
  model, method, score, chunks, timestamp, and AI origin.
- No browser-only hidden state is required for canonical application.

Status: implemented. Schema 4 introduces durable review sets and assigns every
suggestion to one. Authenticated APIs generate Voyage-embedding similarity
sets, list them, accept or deny one row, accept or deny the complete defined
set, and validate imports against existing provenance before changing status.
Suggestion and review mutations are transactional, advance the SQLite/file
generation, and survive restart. The exact accepted row returned by review is
the row consumed by the relation-application query.

The browser now plans, confirms token estimates, indexes, generates suggestions,
performs explicit semantic search, refreshes durable review state, and sends
single/all review decisions through `/api/v1`. It holds the session token only
in the current password field. `localStorage` and `pending-voyage-ai` are gone
from the review path. Local graph-metadata prefilters remain visible in a
separate, non-reviewable “not Voyage AI” section.

Tests cover schema migration through version 3, full provenance, restart,
single/all review actions, validated and tampered imports, the exact apply-row
handoff, session authentication, packaged-browser endpoint wiring, and absence
of placeholder or browser-only durable review state.

## Phase 10: Canonical Relation Application and Projection

- Keep explicit application as the only path from accepted derived state to
  canonical OKF files.
- Parse and write frontmatter structurally rather than replacing one line of
  text.
- Preserve unknown metadata, formatting where practical, and existing
  relations.
- Validate source and target documents immediately before writing.
- Make file write and SQLite audit recovery explicit when one side succeeds and
  the other fails.
- Read canonical `relations` metadata in the OKF document model.
- Include accepted canonical relations in the graph API and browser with AI
  provenance labels.
- Keep Markdown body links as an optional later projection.

Completion criteria:

- Browser-accepted suggestion → SQLite acceptance → explicit apply → frontmatter
  relation → graph display works end to end.
- Reapplying the same suggestion is idempotent.
- Multiline frontmatter, existing relations, unknown fields, write failures,
  and audit failures have tests.
- Applied relations remain understandable and traceable without raw vectors.

Status: implemented. The explicit authenticated apply action reopens the
current repository, rebuilds the current chunk inventory, and validates both
documents and both chunk identities immediately before changing a source file.
It parses existing `relations` structurally, preserves unknown frontmatter and
existing relation objects, deduplicates by suggestion ID, and atomically
replaces the document while retaining its permissions.

Canonical relations are exposed as typed `okf::CanonicalRelation` values and
projected by `/api/v1/graph` with provider, model, method, AI marker, chunks,
score, timestamp, status, and suggestion ID. The browser distinguishes these
solid canonical edges from dotted accepted-but-not-applied review edges and
offers a separately confirmed “Apply accepted relations” action.

SQLite auditing is idempotent. If the canonical write succeeded but its audit
did not, the next explicit apply recognizes the existing suggestion ID and
repairs the audit row without duplicating frontmatter. Tests cover the complete
accept-to-graph path, multiline and CRLF frontmatter, existing and unknown
metadata, malformed relations, atomic write failures, permission retention,
idempotency, and missing-audit recovery. Markdown body links remain outside
this phase.

## Phase 11: Documentation Reconciliation

- Correct all statements that currently describe the workflow as end to end.
- Move already answered Voyage questions out of `Open Questions` and into
  decisions.
- Document exact CLI precedence, mounted/unmounted policy, browser provisioning,
  API security, SQLite rebuild behavior, and canonical relation semantics.
- Add a migration guide from the scanlab-embedded browser to standalone OKF.
- Add a security/reporting policy and a community contribution guide.
- Ensure every normal OKF concept document has the required `type` metadata;
  document the reserved-index exception explicitly.

Completion criteria:

- Documentation describes only behavior proven by tests.
- A new user can install, configure, browse, index, review, and rebuild using
  documented commands alone.
- Security-sensitive actions and token costs are conspicuous before execution.

Status: implemented. The standalone getting-started guide provides tested
commands for binary/browser installation, mounted and unmounted roots, server
startup, token-file use, optional Voyage configuration, planned indexing,
durable review, explicit canonical application, status, and token-free rebuild.
The root reference records exact CLI/environment/`.env` precedence and restart
semantics; the API reference now distinguishes authentication, provider-token
cost, and persistent effects for every action class.

The scanlab migration guide keeps canonical Markdown in place, moves browser
provisioning to the packaged standalone frontend, explains generic versus
compatibility routes, and defines derived-state rebuild and rollback behavior.
`CONTRIBUTING.md` and `SECURITY.md` provide community checks, secret/token
rules, private vulnerability reporting, and the Apache-2.0 contribution
boundary. Voyage decisions now replace resolved open questions about reserved
documents, SQLite, semantic-search ownership, and canonical relation storage.

Every normal Markdown document under `crates/okf/docs/knowledge` has non-empty
`type` metadata. The sole exception is the reserved navigation file `index.md`;
the `index.md`/`log.md` exception is documented in the format reference,
quickstart, contribution guide, and documentation index.

## Phase 12: Community Release Gates and CI

- Add CI for supported platforms and the declared minimum Rust version.
- Run at least:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - package-content checks for `okf` and `okf-http`
  - JavaScript syntax and browser security tests
  - offline/no-token tests
- Fix the current strict-Clippy failure in mounted-root resolution.
- Add end-to-end tests using temporary browser and document roots.
- Test a packaged installation rather than only the repository checkout.
- Add a release checklist that verifies no API key, `.env`, SQLite database,
  localStorage export, editor lock, or physical test path is included.

Completion criteria:

- All release gates pass from a clean checkout.
- The packaged binary/browser workflow passes without scanlab-specific files.
- CI proves ordinary builds and tests use neither network access nor Voyage
  tokens.
- The original OKF HTTP phases 1–10 can be marked complete without qualification.

Status: implemented. `.github/workflows/okf-ci.yml` runs consolidated release
gates on Linux, workspace tests on Linux and macOS, and copied standalone crate
tests at the declared Rust 1.73 (`okf`) and Rust 1.82 (`okf-http`) minimums.
Dependencies are fetched before ordinary tests switch Cargo to offline mode;
provider credentials are empty/unset and the paid Voyage test remains ignored.

`scripts/check-okf-release.sh` enforces formatting, strict workspace Clippy,
locked offline tests, JavaScript syntax/security tests, synchronized browser
sources, package-content policy, and clean diffs. Strict Clippy now passes
without blanket lint suppression, including MSRV-compatible scanlab parsing and
the mounted-root/OKF HTTP paths identified by the audit.

The packaged standalone end-to-end test executes the compiled `okf-http`
binary, installs its embedded browser into a temporary directory, starts it
with a temporary mounted OKF root, and verifies browser, inventory, logical
document route, and Markdown delivery without scanlab files. Package checks
require licenses, docs, browser assets, security and E2E tests while rejecting
secrets, `.env`, SQLite state, export/lock files, `localStorage` review state,
and private physical paths. The release checklist records all gates and forbids
waiving security, no-token, package, or standalone-installation failures.

All original OKF HTTP phases 1–10 are therefore implemented and covered by the
community release gates. crates.io publication remains explicitly out of scope.

## Dependency Order

The phases are intentionally ordered:

1. Legal/package baseline must exist before distributing artifacts.
2. Configuration semantics must stabilize before browser provisioning and root
   management depend on them.
3. Browser assets must be distributable before standalone browser tests are
   meaningful.
4. Generic discovery must replace scanlab assumptions before public API/UI
   contracts freeze.
5. Security hardening must precede wiring token-spending and mutating actions
   into the browser.
6. API contracts must stabilize before the Voyage/review frontend depends on
   them.
7. Transport correctness must precede transactional indexing.
8. Index consistency must precede suggestion generation.
9. Persistent review state must precede canonical application.
10. Canonical relations must be readable before the workflow is called end to
    end.
11. Documentation follows verified behavior.
12. CI and release gates validate the complete chain.

No phase may be declared complete solely because its isolated unit tests pass;
its incoming and outgoing dependencies must also pass their integration tests.
