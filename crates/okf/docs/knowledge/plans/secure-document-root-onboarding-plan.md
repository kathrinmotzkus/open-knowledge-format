---
title: Secure OKF Document Root Onboarding Plan
type: Plan
kind: knowledge-plan
topic: okf-document-roots
status: completed
updated: 2026-07-02
tags: [okf, document-roots, security, import, identity, configuration, monitoring]
---

# Secure OKF Document Root Onboarding Plan

## Purpose

This plan replaces direct browser-side root configuration with a secure,
reviewable onboarding and change-detection workflow. It also closes the gap
between arbitrary user directories and canonical Open Knowledge Format
repositories.

The work is intentionally split into phases. Each phase must be implemented,
tested, summarized, and reviewed before work starts on the next phase.

All twelve phases are complete and form part of the coordinated OKF and
`okf-http` 0.3.0 release prepared on 2026-07-02.

## Security Prerequisite

The
[OKF Local Access, Authentication, and TLS Plan](local-access-authentication-and-tls-plan.md)
must be implemented first. This plan consumes its access modes, pairing
sessions, persistent users, TLS policy, and capability checks.

Pure reading remains anonymous. Root proposals, physical path details,
configuration writes, source initialization, and change decisions require a
local editor pairing session or a persistent HTTPS-authenticated account with
the corresponding capability.

## Baseline at Plan Creation

The implementation at plan creation already provided:

- ordered mounted and unmounted document roots
- process environment, `.env`, and CLI root configuration
- authenticated root mutation endpoints that write `OKF_DOCUMENT_ROOTS` to
  `.env`
- canonical path checks that prevent `..` and symlink escapes outside a root
- recursive Markdown discovery and diagnostics for missing `type` metadata
- browser navigation built from the OKF repository API

The following gaps defined the work; completed phase status sections record
which ones are now closed:

- document routes can serve arbitrary regular files below an approved root
- hidden paths and unsupported formats are not a complete admission boundary
- numeric root indexes are runtime locators, not stable identities
- browser-managed roots would modify the same `.env` file that can hold secrets
- there is no preview tied to an exact filesystem snapshot
- non-OKF directories have no initialization workflow
- CSV resources do not yet have a canonical OKF metadata contract
- later filesystem changes are not reported as pending review

## Accepted Decisions

- Browser-managed persistent roots are stored in
  `~/.config/okf/config.toml` on Linux.
- `.env` remains console/operator-managed. The browser must never modify it.
- CLI and environment configuration remain explicit operator overrides.
- Document-root admission uses an allowlist, initially Markdown and CSV.
- Hidden files and hidden directory components are never admitted.
- Symlinks and non-regular files are never admitted.
- Packaged browser JavaScript is trusted application code and remains
  physically separate from document-root content. JavaScript found in a
  document root is not admitted.
- Every included directory, including nested directories, requires its own
  `index.md` under the OKF standard.
- Adding a root and modifying its source files are separate confirmations.
- A preview must show accepted, rejected, conflicting, and proposed files
  before anything is persisted.
- A confirmation applies only to the exact previewed snapshot.
- Spaces remain part of canonical filenames and are percent-encoded only in
  URLs. They are never rewritten to underscores or hyphens automatically.
- Case-only and Unicode-normalization path collisions are blocking conflicts.
- Optional change detection is read-only. It never changes configuration,
  source files, canonical relations, or Voyage state automatically.
- Root onboarding and change detection never call Voyage AI.
- Anonymous users can read admitted OKF documents and declared resources but
  cannot propose, register, initialize, monitor, or modify roots.
- Root-management authorization uses the shared capability model; this plan
  does not create a second token or login system.

## Configuration Precedence

The target precedence is:

1. CLI `--root` replacements
2. process-level `OKF_DOCUMENT_ROOTS`
3. console-managed `.env`
4. browser-managed `~/.config/okf/config.toml`
5. host defaults

`--add-root` remains an explicit lower-priority addition. The browser must show
when its persistent configuration is overridden by a higher-precedence source.

## Non-Goals

- Do not create a general-purpose file manager in the browser.
- Do not expose a server-side directory listing or filesystem browser.
- Do not execute, transform, or serve JavaScript from document roots.
- Do not infer canonical metadata with AI during onboarding.
- Do not automatically accept later filesystem changes.
- Do not copy an entire external root into an OKF-owned directory.
- Do not delete source documents when a root is removed from configuration.
- Do not make SQLite canonical OKF knowledge.
- Do not duplicate authentication, pairing, users, sessions, TLS, or CA setup
  inside the root-management subsystem.

## Phase 1: Format, Compatibility, and Threat Baseline

- Audit the upstream OKF material and the current local format documentation
  before introducing identity or resource metadata.
- Record which fields are standard, which are local extensions, and how unknown
  fields remain forward-compatible.
- Extend the HTTP threat model with browser-managed roots, source mutation,
  physical path disclosure, stale previews, oversized trees, and malicious
  document content.
- Add regression tests that demonstrate the current arbitrary-file serving
  exposure without checking secret file contents into the repository.
- Define limits for file size, root file count, nesting depth, and total scanned
  bytes. Limits must be configurable only through trusted host configuration.
- Define the terms `configured`, `admitted`, `OKF-compatible`, `pending`,
  `rejected`, and `canonical` for all later phases.

Completion criteria:

- Format extensions have documented compatibility rules.
- The threat model covers root onboarding and change review.
- Current unsafe serving behavior is captured by a failing or quarantined
  regression test that Phase 5 must close.
- Resource and scan limits have explicit defaults and error behavior.

Status: implemented. The upstream OKF v0.1 specification, example-bundle
inventory, local format reference, and current Rust model were audited without
adding an external runtime dependency. The document-root onboarding baseline
now distinguishes upstream fields from local extensions and stricter producer
policy, defines forward-compatible handling and the vocabulary used by later
phases, and records explicit trusted scan limits and closed failure behavior.
The HTTP threat model covers browser-managed roots, path disclosure, distinct
source mutation consent, stale snapshots and races, oversized trees, and
malicious content. A generated regression test captured the regular-file
serving exposure without placing secret content in the repository. It was
quarantined through Phase 4; Phase 5 closed the exposure and made the test a
permanent passing release gate.

## Phase 2: Stable Identity and Path Model

- Separate stable knowledge identity from physical path, mount name, browser
  URL, and numeric runtime root index.
- Define a portable stable root identity stored in the root `index.md`, so it
  travels with the repository rather than with one machine's configuration.
- Decide whether every document also receives a stable frontmatter identity.
  If so, define generation, preservation, rename, and collision rules before
  migrating existing relations.
- Preserve the exact UTF-8 display path while computing a separate normalized
  comparison key.
- Normalize logical separators to `/` and reject root, prefix, `.` and `..`
  components, control characters, and NUL bytes.
- Use Unicode normalization and case folding only for collision detection, not
  for rewriting user-visible names.
- Treat case-only and normalization-equivalent names as blocking conflicts even
  on case-sensitive filesystems.
- Keep spaces unchanged in stored paths and use percent encoding only at the
  HTTP boundary.
- Specify how canonical relations survive root reordering, mount changes, file
  moves, and document renames.

Completion criteria:

- Numeric root indexes are documented and tested as runtime routing details
  only.
- Stable root and document identity rules are represented by public OKF types.
- Path normalization and collision tests pass on portable fixtures.
- Existing repositories can be migrated without silently changing filenames.

Status: implemented. Public `RootId` and `DocumentId` types validate separate
opaque URN namespaces, while `PortablePath` preserves exact UTF-8 display text
and computes a separate NFKC/case-folded collision key. Portable paths reject
absolute and prefixed forms, empty, dot and parent components, backslashes,
control characters, NUL, and non-UTF-8 platform components. Collision tests
cover case-only, composed/decomposed Unicode, compatibility spelling, German
sharp S, Greek sigma, spaces, and portable separators.

The bundle-root `index.md` carries `okf_root_id`; each non-reserved concept will
carry `okf_document_id`. Existing repositories without IDs remain readable.
IDs are generated randomly by the future migration proposal, preserved across
moves and renames, and changed for copies. Repository documents now expose
valid IDs without making physical paths, mounts, URLs, or numeric root indexes
canonical. Tests prove identities survive mount changes and renames without
rewriting filenames. The format reference defines identity-first relation
resolution with logical paths retained only as display and legacy fallback.

## Phase 3: Secure File Admission Scanner

- Add an OKF-core inventory scanner that does not depend on Axum, SQLite, or the
  browser.
- Admit only regular UTF-8 text files with approved extensions:
  - `.md`
  - `.markdown`
  - `.csv`
- Reject every path containing a component whose name starts with `.`.
- Reject symlinks, sockets, devices, pipes, executables, unsupported formats,
  invalid UTF-8, NUL bytes, and files exceeding configured limits.
- Do not trust extensions alone: verify that admitted Markdown and CSV are text
  content and reject conflicting or suspicious content signatures.
- Produce structured reasons for every rejected file.
- Stop safely at count, size, or depth limits and report partial-scan status.
- Keep packaged browser assets outside this scanner and policy.

Completion criteria:

- Hidden directories such as `.ssh` and files such as `.env` cannot enter an
  inventory at any depth.
- Symlinks are rejected even when their target remains inside the root.
- Unsupported and non-regular files receive stable machine-readable reasons.
- Boundary, Unicode, size, depth, and malformed-content tests pass without
  accessing user files outside temporary fixtures.

Status: implemented. `scan_document_root` is a synchronous OKF-core API with no
Axum, SQLite, browser, configuration-write, or Voyage dependency. It accepts
only regular UTF-8 `.md`, `.markdown`, and `.csv` text, derives fixed media
types, and rejects hidden components, hidden or symlink roots, symlinks at any
depth, Unix sockets/FIFOs/devices, other non-regular files, executable modes,
unsupported extensions, non-UTF-8 paths or content, NUL and suspicious control
characters, known binary/executable signatures, read failures, and portable
path collisions. Every rejection has a stable machine-readable code.

The scanner walks deterministically without following discovered symlinks and
returns accepted and rejected inventories without retaining document content.
Entry, depth, per-file, and aggregate-byte limits have validated hard ceilings;
the first breach stops traversal, records the configured and observed values,
and makes the partial inventory unconfirmable. Temporary-fixture tests cover
formats, hidden trees, internal symlinks, sockets where the test environment
permits them, executable files, Unicode and non-UTF-8 paths, malformed content,
collisions, every limit, and every rejection code. CI requires the real Unix
socket fixture while restricted local sandboxes may skip that one assertion.

## Phase 4: OKF Compliance and Resource Manifest Model

- Classify admitted Markdown as:
  - canonical OKF
  - missing `index.md`
  - missing frontmatter
  - missing required fields
  - invalid or conflicting metadata
- Require an `index.md` for every admitted directory containing documents or
  resources. Empty and fully rejected directories do not receive an index.
- Define the canonical `resources` schema for non-Markdown resources such as
  CSV before implementing writes. At minimum it must include path, type, and
  media type.
- Serve CSV only when declared by the owning directory's `index.md`.
- Define CSV validation, delimiter/encoding reporting, size limits, escaped
  browser rendering, and spreadsheet-formula safety warnings.
- Generate deterministic proposals for missing indexes and minimal Markdown
  frontmatter.
- Derive titles from existing H1 headings or filenames, but use a generic or
  explicitly user-selected `type`; do not silently invent semantic types.
- Merge existing metadata instead of replacing it, and preserve unknown fields.

Completion criteria:

- The format documentation specifies nested indexes and declared resources.
- Repository parsing exposes declared resources through typed APIs.
- Missing or invalid OKF structure produces proposals and diagnostics without
  changing source files.
- Existing OKF repositories continue to load unchanged.

Status: implemented. `analyze_compliance` consumes a completed admission
inventory without writing source files. It classifies concept Markdown as
canonical, missing frontmatter, missing required `type`, malformed, or
conflicting; reserved files remain separate. It requires exact `index.md`
files for every admitted content directory and creates stable `CreateIndex` or
field-only `MergeFrontmatter` proposals. Titles come from an existing H1 or
filename and missing types receive only the explicit generic `Concept` value;
unknown existing metadata is never included in the patch fields and therefore
cannot be overwritten. Incomplete admission reports remain unconfirmable.

The canonical local `resources` schema is a JSON array in the owning
directory's `index.md`, with a same-directory portable `path`, open non-empty
`type`, and fixed `text/csv; charset=utf-8` media type. `DeclaredResource`
exposes only valid declarations through the repository API. Compliance reports
distinguish declared, undeclared, missing, duplicate, and invalid resources.
CSV analysis reports UTF-8, delimiter, dimensions, structural validity, and
spreadsheet-formula warnings; documentation requires escaped text rendering
and forbids evaluation. Tests cover deterministic proposals, nested missing
indexes, all Markdown classifications, metadata preservation, typed resources,
missing and undeclared CSV, malformed declarations and CSV, formula warnings,
and unchanged source bytes. Existing repositories remain best-effort readable.

## Phase 5: Restricted Document and Resource Serving

- Replace generic root-relative static serving with repository-authorized
  document and declared-resource resolution.
- Serve admitted Markdown only through its selected OKF repository identity.
- Serve CSV only when it is admitted and declared in the correct `index.md`.
- Return `404` or `403` for hidden, unsupported, undeclared, conflicting,
  rejected, and non-regular files.
- Revalidate containment, file type, admission status, and declaration on every
  request.
- Send fixed safe content types with `X-Content-Type-Options: nosniff`.
- Preserve Markdown URL sanitization, CSP, and escaped CSV rendering.
- Keep browser assets on their separate packaged/browser-root routes.

Completion criteria:

- A configured home directory cannot expose `.ssh`, `.env`, JavaScript, or an
  undeclared file through any document route.
- The Phase 1 serving regression is fixed.
- Valid Markdown and declared CSV remain reachable through stable URLs.
- Encoded, double-encoded, symlink, case-collision, and traversal tests pass.

Status: implemented. All mounted, unmounted, and scanlab-compatibility document
routes now use the admission-aware resolver; the generic regular-file resolver
is retained only for packaged browser assets and the tiny explicit repository
allowlist. Each document request validates a portable exact path, runs bounded
admission, refuses incomplete inventories and path collisions, revalidates
canonical containment and regular-file status, and emits fixed Markdown or CSV
content types with `nosniff`. Markdown must be admitted. CSV must additionally
have valid structure and exactly one valid same-directory declaration in its
owning `index.md`.

The former arbitrary `.txt` serving regression now passes permanently and is
no longer ignored. HTTP fixtures prove valid Markdown and declared CSV remain
available while `.env`, `.ssh`, JavaScript, undeclared or malformed CSV,
symlinks, unsupported files, case-fold collisions, encoded and double-encoded
hidden paths, parent traversal, and unsafe mounts cannot return content. The
browser CSV parser handles comma, semicolon, tab, quotes, and escaped quotes;
all cells pass through HTML escaping and formula-like values produce a visible
warning without evaluation. Browser assets remain physically and logically
outside document admission.

## Phase 6: Versioned XDG Configuration

- Introduce `~/.config/okf/config.toml` with an explicit schema version.
- Respect `XDG_CONFIG_HOME`; use `~/.config/okf` as the Linux fallback.
- Store stable root reference, mount, physical path, enabled state, priority,
  and `check_for_changes` in structured TOML.
- Create the configuration directory privately and write `config.toml`
  atomically with mode `0600`.
- Add incremental migration tests for every released configuration schema.
- Read existing `.env` roots as higher-precedence operator configuration, but
  never write `.env` through browser APIs.
- Add console-only migration/import commands for users who want to move `.env`
  roots into `config.toml` deliberately.
- Report active CLI, process, or `.env` overrides without exposing secrets.

Completion criteria:

- Browser-owned configuration survives restart at a stable location independent
  of the process working directory.
- Browser API code has no path that writes `.env`.
- Existing CLI and `.env` workflows remain supported and documented.
- Permissions, atomic replacement, malformed TOML, migration, and precedence
  tests pass.

Status: implemented. OKF core now owns schema-versioned browser configuration,
XDG path selection, schema-zero migration, validation, stable root identity,
priority ordering, enablement, change-monitoring preference, and private atomic
persistence. `okf-http` loads enabled browser roots as the persistent base;
process and `.env` values remain higher-precedence operator overrides, while
CLI overrides and additions retain their established behavior.

Authenticated root APIs write only `config.toml` and expose stable metadata and
boolean override state without revealing operator values. The console-only
`okf-http roots import-env` command deliberately copies compatible roots while
leaving `.env` and source documents untouched. Contract and HTTP tests cover
XDG independence, permissions, atomic replacement, malformed and future
schemas, migration, precedence, symlink refusal, confirmation, restart
persistence, and the absence of browser-side `.env` mutation.

## Phase 7: Snapshot-Bound Preview and Proposal Engine

- Build a read-only root proposal from the secure inventory and OKF compliance
  analysis.
- Show a tree containing:
  - accepted files
  - rejected files with reasons
  - pending Markdown metadata
  - missing or invalid nested indexes
  - declared and undeclared CSV resources
  - identity, case, Unicode, mount, and shadowing conflicts
- Include canonical root path, mount, counts, total bytes, limits, writeability,
  and override effects.
- Compute a proposal digest from the complete relevant snapshot, not only
  timestamps.
- Store proposals as expiring derived state outside canonical documents.
- Re-scan before confirmation and reject stale proposals if any relevant file
  changed.
- Keep registration proposals separate from source-initialization proposals.
- Never call Voyage AI or create embeddings during preview.

Completion criteria:

- A proposal is deterministic for an unchanged fixture.
- A changed, added, removed, or renamed file invalidates confirmation.
- Users can see exactly what configuration and source changes would occur.
- Preview creates no source, configuration, relation, or Voyage mutation.

Status: implemented in OKF core. `build_root_proposal` combines bounded secure
admission and compliance analysis into separate registration and
source-initialization proposals. Its deterministic tree reports accepted and
rejected entries, reason codes, pending metadata, missing indexes, declared and
undeclared CSV resources, identity and portable-path conflicts, mount reuse,
logical shadowing, configured-root failures, counts, bytes, trusted limits,
declarative writeability, and operator override effects.

Every proposal binds its canonical path, mount, operation, limits, configured
root context, conflicts, operation-specific changes, and a SHA-256 inventory
snapshot. The snapshot contains deterministic path and file-kind records plus
content hashes for every bounded candidate file read by admission; it does not
rely on timestamps. Proposal construction verifies that the tree remained
stable during analysis. `RootProposalStore` keeps expiring proposals in process
memory outside canonical data and performs a complete rebuild before returning
a fresh validation result. Changed, added, removed, renamed, unreadable,
retyped, newly colliding, or newly shadowed state invalidates the whole
proposal.

Contract tests prove deterministic output, operation separation, expiry,
same-size content detection, addition, removal, rename detection, conflict and
override reporting, and absence of configuration, source, relation, SQLite, or
Voyage mutation. Versioned HTTP exposure and browser rendering remain assigned
to the later API and workflow phase.

## Phase 8: Transactional Registration and OKF Initialization

- Implement two independent confirmation actions:
  1. register the approved root in `config.toml`
  2. initialize or repair approved OKF source files
- Permit read-only registration with diagnostics when users do not want source
  modification.
- Present exact frontmatter, `index.md`, and resource-manifest diffs before the
  second confirmation.
- Preserve permissions, line endings where practical, unknown frontmatter, and
  existing content.
- Use locks, revisions, an operation journal, and recoverable staged writes for
  multi-file initialization.
- Keep rollback material outside the document root and remove it after a
  verified successful transaction.
- Warn when files belong to a dirty Git worktree and show the final diff without
  invoking destructive Git commands.
- Never overwrite an existing index or metadata field blindly.

Completion criteria:

- Registration cannot mutate source files.
- Initialization cannot mutate `config.toml` without its separate approval.
- Mid-operation failure is recoverable without mixed old/new canonical state.
- Retrying an interrupted operation is idempotent.
- Existing permissions and unrelated metadata survive successful writes.

Status: implemented in OKF core. Registration and source initialization consume
different proposal kinds and require independent confirmations. Registration
uses an advisory cross-process lock, exact configuration revision, repeated
snapshot validation, and the existing atomic private `config.toml` writer. It
is idempotent for an already identical stable identity and rejects conflicting
revisions or identities. Configuration and lock paths inside the document root
are refused before anything is created.

Initialization renders exact before/after bytes and diffs before confirmation,
including missing indexes, minimal frontmatter, preapproved portable root
identity, and explicitly typed CSV resource declarations. Existing fields are
never blindly replaced; unknown metadata, body content, LF/CRLF style, and Unix
permissions are preserved. Read-only `git rev-parse` and `git status` results
warn about dirty worktrees and the completion report contains final diffs
without invoking a destructive or mutating Git command.

Multi-file writes use a cross-process lock, revision-bound plan digest,
external private staging, verified backups, a synced versioned journal,
per-file pre-write revalidation, atomic file replacement, final hash checks,
and a committed marker. Failures roll back already changed files and remove new
ones; startup recovery restores an interrupted journal or only cleans a
verified committed journal. Unexpected third-party file state blocks rollback
rather than being overwritten. Transaction state inside the document root is
forbidden and successful cleanup removes all rollback material.

Contract tests cover registration/source separation, revision conflicts,
idempotent registration, exact previews, explicit resources, new root identity,
line endings, unknown metadata, permissions, external-state enforcement,
mid-operation rollback, and restart recovery. No test or implementation path
calls Voyage AI.

## Phase 9: Authorized Root Management API

- Add versioned endpoints for proposal creation, proposal details, root
  registration, source initialization, root removal, and configuration status.
- Keep admitted document and declared-resource reading anonymous.
- Require `roots.propose` for physical path details and proposal creation.
- Require `roots.configure` for registration, edit, priority, monitoring, and
  removal actions.
- Require `content.initialize` or `content.write` for canonical source changes.
- Accept authority only from the shared local pairing or persistent HTTPS
  session established by the access-security plan.
- Preserve same-origin, CSRF, Host, and Fetch Metadata checks for every
  sensitive action.
- Require explicit confirmation headers for configuration and source writes.
- Bind every mutation to an unexpired proposal ID and digest.
- Disable root-management writes entirely for non-loopback server operation by
  default, even when authenticated TLS remote API access is enabled.
- Return structured conflict, stale-proposal, limit, permission, and recovery
  errors.
- Add revision checks so browser tabs and multiple processes cannot silently
  overwrite newer configuration.
- Audit root-management actions without logging secrets or private document
  contents.
- Deprecate the existing `.env`-writing root mutation semantics explicitly.

Completion criteria:

- No browser endpoint writes `.env`.
- Anonymous document reading remains functional without a login wall.
- No anonymous endpoint reveals physical paths or creates a proposal.
- Pairing and persistent HTTPS sessions enforce the same capability contract.
- Stale and concurrent mutations fail safely.
- Remote root mutation is unavailable unless a future separate security design
  deliberately enables it.

Status: implemented. `okf-http` now exposes versioned local endpoints for
configuration status, proposal creation and details, registration,
initialization planning and execution, stable-ID updates, and removal. Proposal
responses contain the complete authorized review tree, limits, conflicts,
override state, canonical path, exact digests, and remaining lifetime.
Initialization plans return exact before/after source and diffs; mutations bind
the reviewed proposal, plan, configuration revision, and operation-specific
confirmation header.

The route capability matrix requires `roots.propose`, `roots.configure`, or
`content.initialize` as appropriate. Only paired local sessions and persistent
HTTPS accounts can enter the workflow: the transitional automation token is
explicitly rejected for root management. Existing same-origin, Host, Fetch
Metadata, cookie, CSRF, expiry, logout, and live-role-revision controls apply
before handlers run. Read-only mode does not mount the routes, while remote
binds and trusted proxies reject proposal and mutation capabilities even for an
otherwise authenticated account.

Configuration updates and removal use stable root IDs and exact revisions;
registration and initialization revalidate expiring snapshots. Structured API
codes distinguish missing, expired, stale, unstable, digest-mismatched,
revision-conflicted, unconfirmable, unsafe, and recovery failures. Audit events
record action, opaque proposal or stable-root reference, and success only—never
physical paths, source contents, `.env` values, credentials, or secrets. The
legacy mutation routes return `410 Gone`; their deprecated GET compatibility
view remains temporarily read-only. HTTP has no `.env` write path.

Router and end-to-end tests cover anonymous denial, paired local authority,
persistent HTTPS capability enforcement, transitional-token denial, read-only
route absence, remote denial, separate confirmations, exact revision handling,
stale source rejection, registration, initialization, update, removal, legacy
retirement, anonymous document continuity, and unchanged `.env` content.

## Phase 10: Browser Onboarding and Review UI

Status: implemented. The packaged browser now has a protected **Document
roots** view with text-only path entry, mount, priority, and opt-in change
checking. It reads the canonical configuration and exposes active process,
`.env`, CLI, and effective-runtime overrides prominently. Preview performs one
explicit bounded scan and renders the complete filterable proposal tree,
summary, conflicts, rejected entries and reasons. Filtering never mutates the
proposal and there is no force-admission control.

Registration remains disabled until the current proposal is confirmable and
the user acknowledges reviewing it. Read-only registration writes only the
versioned browser configuration and reports the restart requirement. The
combined workflow registers first, then creates a distinct
`source_initialization` proposal and plan, displays every exact diff and Git
dirty-state warning in a modal, and requires a second source-write
acknowledgement. Explicit resource types can be supplied for undeclared
resources. Pairing/login sessions and CSRF stay in memory and existing CSP/XSS
protections remain in force; page refresh reads configuration but never starts
an expensive root scan.

- Add a root-management view with path, mount, priority, and an optional
  `Check this root for changes automatically` checkbox.
- Use path text entry; do not implement a general server filesystem browser.
- Present the complete proposal tree and summary before enabling confirmation.
- Separate buttons and warnings for:
  - register read-only
  - register and initialize OKF
  - cancel
- Require a second confirmation for source writes.
- Show rejected files and reasons without offering unsafe overrides.
- Show higher-precedence CLI, process, and `.env` overrides prominently.
- Show restart requirements until safe live reload is implemented.
- Reuse the shared pairing/login session UX and preserve CSP/XSS protections.
- Never persist pairing, login, CSRF, or session secrets in browser storage.
- Provide accessible keyboard navigation and usable large-tree filtering.

Completion criteria:

- A user cannot register a root without reviewing its current proposal.
- Source initialization is visually and technically separate from registration.
- Unsupported, hidden, conflicting, and oversized files are understandable in
  the UI but cannot be force-admitted.
- Browser refresh does not repeat expensive scans unnecessarily.

## Phase 11: Opt-In Change Detection and Pending Review

Status: implemented. Roots whose canonical browser configuration enables
`check_for_changes` are scanned once asynchronously at startup. Baseline
inventories, content hashes, cached pending snapshots and scan timestamps are
kept in a dedicated rebuildable SQLite database under private OKF state. A
browser refresh reads only cached status; every later scan requires an explicit
**Check now** action and never calls Voyage.

New monitored registrations seed their baseline from the already revalidated
registration proposal, closing the restart race between review and first scan.
Reviewed source initialization replaces that baseline transactionally with its
post-write inventory. Pre-existing monitored configuration without a baseline
uses its first startup scan for migration initialization.

The comparison classifies added, modified, deleted, rename candidates, newly
hidden, newly rejected, metadata-invalid and conflicting entries. The browser
shows root-level indicators and a Details review containing both the exact
change list and complete current admission inventory with local filtering.
Accept and dismiss remain disabled until the user acknowledges that snapshot.
Acceptance rescans and verifies the digest before atomically moving the
baseline; stale roots are rejected. Dismissal removes only the cached notice,
leaves the baseline unchanged, and therefore grants no continuing authority.

Monitored document routes and document/graph inventories consult the accepted
baseline, so added or modified paths remain unavailable while pending. Change
detection and review never alter configuration, source files, canonical
relations, embeddings, Voyage state, or `.env`. Root removal deletes only its
rebuildable monitoring rows after configuration removal and never touches
source documents.

- Store only the `check_for_changes` preference in canonical configuration.
- Store baseline inventory hashes and pending change sets as rebuildable SQLite
  state.
- Scan opted-in roots once asynchronously at server startup and only on an
  explicit `Check now` action afterward.
- Let browser refresh read cached status rather than automatically start a new
  filesystem scan.
- Classify changes as added, modified, deleted, renamed candidate, newly hidden,
  newly rejected, metadata-invalid, or conflict.
- Display a root-level change indicator and a `Details` action.
- Reuse the Phase 7 preview and Phase 8 confirmation workflow for every change
  set.
- Never update the baseline until the user explicitly accepts or dismisses the
  reviewed state according to documented rules.
- Never change configuration, source files, canonical relations, SQLite
  embeddings, or Voyage state merely because a change was detected.
- Removing a root removes only configuration and derived monitoring state; it
  never deletes source files.

Completion criteria:

- Startup detection is read-only and token-free.
- New files remain pending rather than becoming automatically admitted.
- Details show the exact changed snapshot and all current admission results.
- Accepted changes use the same stale-snapshot and transaction protections as
  initial onboarding.

## Phase 12: Tests, Documentation, Migration, and Release Gates

Status: implemented. The final matrix now combines OKF-core contracts for
portable identity, Unicode/case collision keys, admission, nested indexes,
resources, proposals, revisions and transactions with HTTP contracts for every
access mode, remote denial, stale snapshots, restricted serving, monitoring,
configuration persistence and recovery. A dedicated concurrency contract proves
that two browser tabs or server instances cannot commit from one configuration
revision.

Browser security checks cover the onboarding and pending-review controls,
absence of filesystem upload/browsing and persistent authority, separate
configuration/source/change confirmations, and the rule that refresh never
starts a monitoring scan. Standalone binary tests install packaged assets and
exercise anonymous HTTP and authenticated HTTPS without scanlab. Explicit root
security gates rerun arbitrary-file denial, pending-file withholding, and
unchanged-`.env` tests without Voyage credentials.

Getting-started, format, root, API, threat-model, standalone migration,
changelog and release-checklist documents now describe the same precedence,
identity, preview, initialization and monitoring contracts. Release automation
runs formatting, strict Clippy, locked offline workspace tests, browser checks,
source synchronization, root-security invariants, package-content/secret
hygiene and whitespace checks. CI additionally runs Linux and macOS, Rust 1.85
MSRV copies of both standalone crates, package construction, and socket-required
standalone tests. The paid Voyage test remains opt-in and ordinary gates refuse
provider credentials.

- Add OKF-core contract tests for identity, paths, admission, nested indexes,
  resources, and proposals.
- Add HTTP tests for authorization, remote denial, stale proposals, revisions,
  restricted serving, configuration writes, and recovery.
- Run root-management authorization tests in anonymous read-only, local paired
  editor, and persistent HTTPS-authenticated modes.
- Add browser security and end-to-end tests for onboarding and change review.
- Test case-sensitive and simulated case-insensitive collision behavior.
- Test multiple server instances and multiple browser tabs against one
  configuration revision.
- Test migrations from `.env`-only installations and every released
  `config.toml` schema.
- Update getting-started, format, root, HTTP API, threat-model, migration,
  changelog, and release-checklist documentation.
- Add release gates that reject arbitrary-file serving and browser `.env`
  writes.
- Verify ordinary tests and scans make no Voyage API calls.
- Run MSRV, Linux/macOS, package-content, standalone installation, and complete
  release gates.

Completion criteria:

- The full workflow passes from a clean standalone installation.
- Security regressions fail CI before release.
- Documentation matches the final precedence, identity, format, preview,
  initialization, and monitoring contracts.
- No required behavior depends on scanlab.

## Dependency Order

0. Local access, pairing, TLS, users, and capabilities are implemented first.
1. Format compatibility and threats define the constraints.
2. Stable identities and paths define every later key and URL.
3. Admission policy defines what may be inspected.
4. Compliance and resources define what may become canonical.
5. Restricted serving closes the current exposure before convenient onboarding.
6. Versioned configuration provides the safe persistence target.
7. Snapshot proposals provide the review boundary.
8. Transactions provide safe explicit mutation.
9. The API exposes only those established operations through shared authority.
10. The browser presents the API without weakening it or blocking public reads.
11. Change detection reuses onboarding rather than creating a second policy.
12. Release gates validate the complete chain.

No later phase may bypass an earlier phase's policy through a browser-only or
`okf-http`-only shortcut.
