---
title: OKF HTTP Server Plan
type: Plan
kind: knowledge-plan
topic: okf
status: completed
updated: 2026-07-02
tags: [okf, okf-http, axum, docs-browser, server]
---

# OKF HTTP Server Plan

This plan replaced the temporary `python3 -m http.server` workflow with a
dedicated Rust binary serving the OKF browser and APIs.

> A post-implementation audit found release-blocking integration, security,
> packaging, and portability gaps. They are tracked in the
> [OKF Community Readiness Remediation Plan](okf-community-readiness-plan.md).
> The phase status notes below describe the implementation milestone, not final
> community-release readiness.
> Route examples now use the stable `/api/v1` namespace. The deprecated
> `/api/okf` aliases exist only for temporary compatibility.
> Community-readiness phases 1–12 are complete; the original phases 1–10 are
> now backed by strict CI, package checks, MSRV tests, offline/no-token gates,
> and a packaged standalone end-to-end test.
> The resulting server, security, and onboarding work is prepared for the
> coordinated `okf`/`okf-http` 0.3.0 release.

## Decision

OKF gets a separate HTTP binary crate:

```text
crates/okf
  Library: format, repository, documents, Voyage analysis API

crates/okf-http
  Binary: static web server and OKF-specific HTTP endpoints

docs-browser/
  Frontend files served by okf-http from the user's home directory
```

The server framework will be `axum`.

## Goals

- Remove Python's `http.server` from the regular OKF browser workflow.
- Provide a Rust-native `okf-http` binary.
- Serve `docs-browser/` from the user's home directory.
- Redirect `/` to `/docs-browser/index.html`.
- Accept an unprivileged port argument, for example `okf-http 8003`.
- Default to a safe local bind address: `127.0.0.1`.
- Make binding explicit enough to avoid the previous `127.0.0.1` vs
  `0.0.0.0` confusion.
- Prepare API routes for OKF graph data, Voyage planning, indexing, semantic
  search, and applying accepted suggestions.
- Use a local SQLite database as the working/index store for embeddings,
  vector similarity results, suggested edges, and review state.

## Non-Goals

- Do not treat repository-local `docs-browser/` as the long-term browser
  location.
- Do not make `okf` depend on `axum`.
- Do not automatically run Voyage AI on server startup.
- Do not write canonical OKF files without an explicit API call.
- Do not require privileged ports.
- Do not store raw embedding vectors in canonical Markdown files.

## Phase 1: Crate Skeleton

- Add `crates/okf-http` to the workspace.
- Create a binary named `okf-http`.
- Depend on `okf`.
- Add `axum` and the minimal async runtime dependencies needed by the server.

Completion criteria:

- `cargo check -p okf-http` succeeds.
- `okf` remains independent from `okf-http`.

Status: implemented. `crates/okf-http` exists as a workspace member, builds a
binary named `okf-http`, depends on `okf`, and keeps `axum` out of the core OKF
crate.

## Phase 2: CLI Contract

Initial command shape:

```bash
okf-http <port>
okf-http --host 127.0.0.1 <port>
okf-http --root scanlab=docs --root okf=crates/okf/docs/knowledge <port>
```

Rules:

- The positional port is required at first.
- The port must be unprivileged (`>= 1024`).
- The default host is `127.0.0.1`.
- Do not document all-interface binding as a troubleshooting path. Intranet and
  Internet deployments must keep `okf-http` on loopback and expose a reverse
  proxy instead.
- `okf-http` accepts document roots from both `.env` and CLI arguments.
- `.env` remains the persistent storage location for configured document roots.
- CLI `--root` arguments replace persistent roots with the same mount for the
  current process and have highest priority. `--add-root` appends explicit
  lower-priority fallbacks.
- The future OKF website/browser should provide a UI for adding, removing, and
  editing configured document roots; saving those changes writes back to `.env`
  through an explicit server-side action.

Completion criteria:

- Invalid or privileged ports fail with a clear error.
- Help text explains host and port behavior.
- Help text explains how `.env` roots and CLI `--root` values are combined.

Status: implemented and later tightened by the reverse-proxy deployment
policy. `okf-http` parses `--host`, repeated `--root` arguments, repeated
`--add-root` fallbacks, `--help`, and one required unprivileged port. The
default bind host is `127.0.0.1`. Intranet and Internet deployment keeps
`okf-http` on loopback and exposes a reverse proxy instead of using
all-interface binding as troubleshooting. Invalid, missing, duplicate, or
privileged ports fail with clear CLI errors. Root syntax is parsed by the
shared OKF configuration API; malformed mounts fail early, CLI mounts override
persistent mounts, and loopback IPv4/IPv6 bind targets are supported.
Persistent root editing is available through an atomic `.env` configuration
API.

## Phase 2a: OKF Binary CLI Foundation

`okf-http` introduces the first OKF-owned binary surface. This should become
the foundation for later OKF CLI commands, not a scanlab subcommand.

Initial root argument shape:

```bash
--root <path>
--root <mount>=<path>
```

Rules:

- The syntax matches the current OKF document-root convention.
- Mounted roots expose logical OKF paths.
- Unmounted roots are allowed but mounted roots are preferred for multi-root
  projects.
- The parser should be shared or kept compatible with existing scanlab root
  parsing.

Completion criteria:

- `okf-http --root okf=crates/okf/docs/knowledge 8003` can start with an
  explicit OKF root.
- `.env` can still provide roots when CLI roots are omitted.
- The design leaves room for a future broader `okf` binary if `okf-http`
  should later become a subcommand or sibling command.

Status: implemented. Root parsing accepts `<path>` and `<mount>=<path>`,
combines process environment or `.env` roots with CLI roots, deduplicates roots,
and reuses `okf::DocumentRoot` so the HTTP binary stays aligned with the OKF
library model.

## Phase 3: Static Browser Serving

- Serve files below the user's docs-browser directory.
- On Linux the default browser directory is `~/docs-browser`.
- Redirect `/` to `/docs-browser/index.html`.
- Serve `docs-browser/index.html`, `app.js`, `styles.css`, and vendored
  Cytoscape.
- Prevent path traversal.
- Return useful `404` responses for missing files.
- Keep the browser asset root separate from OKF document roots.

Completion criteria:

- The OKF browser loads from `okf-http`.
- No Python server is needed.
- Static file paths used by the browser resolve correctly.
- A request for browser assets cannot escape `~/docs-browser`.

Status: implemented. `okf-http` now starts an Axum server, redirects `/` to
`/docs-browser/index.html`, and serves browser assets from the configured
browser root. The default browser root is `~/docs-browser`; `--browser-root`
and `OKF_BROWSER_ROOT` allow explicit development or installation paths.
Requests are canonicalized and rejected if they contain parent-directory,
absolute-path, or symlink-escape attempts. Missing browser assets return `404`.

Distribution update: packaged browser sources live under
`crates/okf-http/browser/`. `okf-http --install-browser` provisions them
atomically into the configured browser root, records a version/hash manifest,
protects modified packaged assets, preserves unrelated user files, and supports
explicit replacement through `--force`. Normal server startup fails with an
actionable provisioning command when required assets are missing.

Note: browser-side links/fetches that intentionally point from `docs-browser`
to OKF document roots are not solved in this phase. They remain part of Phase 4,
where document roots get explicit static mounts or API routes.

## Phase 4: Repository-Aware Static Roots

OKF must support arbitrary configured directories and subdirectories as document
roots. Serving those roots must never allow a request, Markdown link, browser
asset path, or browser-provided target to escape its configured root.

The current browser fetches files outside `docs-browser/`, such as:

- `docs/...`
- `crates/okf/docs/...`
- `crates/scql/docs/...`
- repository README files

`okf-http` must serve those files deliberately through explicit static mounts
or API routes, not by accidentally exposing the repository root.

Options:

1. Add explicit static mount routes for configured document roots and browser
   assets.
2. Add API endpoints and gradually stop browser-side raw-file fetching.
3. Serve the repository root only as a temporary development fallback, if at
   all, and never as the default security model.

Initial recommendation: use explicit mounts from the beginning. Each mount maps
a public URL prefix to one canonical filesystem root.

Security requirements:

- Canonicalize the configured root.
- Canonicalize every requested path after joining it to the root.
- Reject the request unless the canonical requested path still starts with the
  canonical root.
- Reject path traversal attempts such as `../`, encoded `..`, symlink escapes,
  and absolute paths.
- Treat `docs-browser/` as a static asset root, not as permission to serve its
  parent directory. On Linux this is `~/docs-browser` by default.
- Connect `~/docs-browser` to configured OKF roots only through explicit
  server routes or API endpoints.
- Audit every browser path that contains `../`; if it would leave
  `~/docs-browser`, replace it with an explicit server route or API path.
- Validate or rewrite Markdown links before exposing them as navigable browser
  links when they point outside the mounted document root.
- Links inside served Markdown files must not become a path-escape mechanism.

Completion criteria:

- Existing docs-browser fetches work unchanged.
- Arbitrary configured OKF roots and subdirectories can be served.
- Requests cannot escape the configured root for their mount.
- Browser-side `../` paths are either eliminated, rewritten, or proven safe.
- Markdown links that point outside an allowed mount are rendered as external or
  blocked links, not served as local files.
- `~/docs-browser` can display documents from configured OKF roots without
  gaining general filesystem access.

Status: implemented. `okf-http` now serves configured mounted document roots
through `/okf-docs/<mount>/<path>` and unmounted roots through
`/okf-root/<root-index>/<path>`. It rejects unsafe mount names, `../`,
encoded parent-directory traversal, absolute paths, and symlink escapes.
Existing scanlab browser paths are bridged only when `--scanlab-compat` enables
the explicit compatibility adapter:

- `/docs/...` maps to the configured `scanlab` mount.
- `/crates/scql/docs/knowledge/...` maps to the configured `scql` mount.
- `/crates/okf/docs/knowledge/...` maps to the configured `okf` mount.

There is no repository-root static fallback. The compatibility adapter alone
exposes its small `/repo-files/` allowlist for `README.md`, `README.de.md`, and
`HOSTS.md`.

The browser has no fixed document list. It discovers documents through
`GET /api/v1/documents` and accepts only `/okf-docs/` and `/okf-root/` as
same-origin document routes. Other links render as external links.

## Phase 5: OKF Metadata API

Add read-only JSON endpoints:

```text
GET /api/v1/documents
GET /api/v1/graph
GET /api/v1/document?path=...
```

These endpoints should use the `okf` crate rather than duplicating browser-side
parsing.

Completion criteria:

- The browser replaces its hard-coded `DOCS` list with API data.
- API responses include `type`, `kind`, title, topic, status, logical path, and
  source path where appropriate.

Status: implemented. `okf-http` now exposes versioned read-only JSON endpoints:

- `GET /api/v1/documents`
- `GET /api/v1/graph`
- `GET /api/v1/document?path=...`

The endpoints open the configured document roots through `okf::Repository`.
Document summaries include `type`, `kind`, title, topic, status, updated,
logical path, source-relative path, browser path, plan status, root index, and
optional mount. Physical paths are available only with
`--expose-physical-paths`. The browser consumes this inventory directly.
Diagnostics are returned in machine-readable form. The graph endpoint
returns document nodes, topic nodes, `has_topic` edges, and same-repository
Markdown `links_to` edges. The document endpoint accepts both logical paths
such as `scanlab/index.md` and browser paths such as
`/okf-docs/scanlab/index.md`.

## Phase 6: Voyage Planning API

Add explicit, non-token-spending endpoints first:

```text
POST /api/v1/voyage/plan
GET  /api/v1/voyage/status
```

These endpoints should report:

- configured model
- document count
- chunk count
- estimated tokens
- estimated requests
- cached chunks
- changed chunks
- configured TPM/RPM limits

Completion criteria:

- Users can inspect cost/rate-limit expectations before spending tokens.
- Missing `OKF_VOYAGE_API_KEY` is reported without breaking the server.

Status: implemented. `okf-http` now exposes:

- `GET /api/v1/voyage/status`
- `POST /api/v1/voyage/plan`

Both endpoints are dry-run only and spend zero Voyage AI tokens. They open the
configured OKF roots through `okf::Repository`, build the deterministic Voyage
chunk set, load the local `.okf-voyage` index if present, and return model,
index root, API-key status, document count, chunk count, estimated tokens,
estimated requests, cached chunks, changed chunks, TPM/RPM limits, and
`within_limits`. Missing `OKF_VOYAGE_API_KEY` is reported as configuration
status, not as a server failure.

## Phase 7: Explicit Voyage Execution API

Add token-spending endpoints only after planning is visible:

```text
POST /api/v1/voyage/check
POST /api/v1/voyage/index
POST /api/v1/voyage/search
```

Rules:

- No endpoint runs automatically on page load.
- API keys are never returned to the browser.
- Errors include HTTP status and provider error codes/messages where safe.
- OpenSnitch/network delays should produce clear timeout messages.

Completion criteria:

- A user action can trigger a tiny connectivity check.
- A user action can trigger indexing changed chunks.
- Normal browsing still spends zero Voyage tokens.

Status: implemented. `okf-http` now exposes token-spending endpoints only as
explicit `POST` actions:

- `POST /api/v1/voyage/check`
- `POST /api/v1/voyage/index`
- `POST /api/v1/voyage/search`

`check` performs the tiny Voyage connectivity embedding request. `index`
chunks the configured OKF repository, embeds changed chunks only, and saves the
local `.okf-voyage` index on success. `search` requires a non-empty JSON query,
embeds that query, and searches the local index. These routes are not called by
normal browser loading. Missing API keys and provider failures are returned as
structured JSON reports with mapped HTTP status codes.

The `index` endpoint uses the shared OKF Voyage rate-limit executor: batches
are checked against configured TPM/RPM limits before each provider request,
oversized batches fail before spending tokens, and a provider `429` triggers one
wait-and-retry cycle for that batch. Live token-spending verification remains
explicit and opt-in.

## Phase 8: SQLite Working Index

`okf-http` needs a durable local working state that is richer than Markdown.
SQLite is the default storage layer for this state.

Default location:

```text
.okf-voyage/okf.sqlite
```

SQLite stores derived and reviewable data:

- document inventory
- chunks
- content hashes
- embeddings
- provider/model metadata
- vector dimensions
- similarity scores
- suggested edges
- review status
- accepted relation provenance

Canonical OKF Markdown files remain the source of truth. SQLite is a local
index/work database that can be deleted and rebuilt.

Initial vector strategy:

- Store vectors in SQLite as BLOB or JSON.
- Compute cosine similarity in Rust first.
- Keep SQLite vector extensions optional for later.

Possible later vector extensions:

- `sqlite-vec`
- `sqlite-vss`
- another cosine-capable SQLite vector extension

Completion criteria:

- `okf-http` can create and migrate `.okf-voyage/okf.sqlite`.
- Index data survives server restarts.
- Deleting the database does not destroy canonical OKF knowledge.
- Cosine similarity works without requiring a SQLite extension.

Status: implemented. `okf-http` creates and incrementally migrates the SQLite
working database at `.okf-voyage/okf.sqlite`. The transactional indexing action
itself synchronizes document inventory, chunks, embeddings, provider/model
metadata, dimensions, and valid suggestions. SQLite is authoritative derived
state; generation-marked TSV files are a recoverable compatibility mirror.
WAL, busy timeout, integrity triggers/checks, stale-row removal, read-only
status diagnostics, and a token-free rebuild action cover concurrent reads and
interrupted mirror writes. Semantic search uses SQLite embeddings and computes
cosine similarity in Rust, so no SQLite vector extension is required yet.

## Phase 9: Persistent Edge Application API

Add a server-side route that can apply accepted suggestions:

```text
POST /api/v1/edges/apply
```

Rules:

- Only accepted suggestions may be applied.
- Suggestions must carry AI provenance.
- Accepted suggestions come from SQLite review state.
- The server writes accepted relationships as structured OKF `relations`
  frontmatter metadata.
- The human-readable presentation is generated by the OKF website/browser first.
- Markdown body links are optional later projections, not the first canonical
  storage mechanism.
- The write operation must be auditable.
- Raw vectors remain in SQLite and are not copied into OKF Markdown files.

Completion criteria:

- Browser-accepted suggestions can become durable OKF file changes through an
  explicit user action.
- The source of AI-assisted changes remains visible.
- The accepted relation can be traced back to its SQLite suggestion/provenance
  row.

Status: implemented. `okf-http` now exposes
`POST /api/v1/edges/apply`. The endpoint reads accepted, AI-derived
suggestions from SQLite review state, resolves their source and target chunks
back to logical OKF documents, and writes structured `relations: [...]`
frontmatter metadata to the source document. Each relation includes suggestion
ID, target document, source/target chunks, provider, model, generation method,
AI marker, score, creation timestamp, and accepted status. The write path is
idempotent by suggestion ID. Applied relations are recorded in SQLite
`applied_relations` for auditability and are not reapplied. Markdown body links
remain a later optional projection; raw vectors stay in SQLite.

## Phase 10: Documentation and Workflow Cleanup

- Update OKF README.
- Update docs-browser instructions.
- Update Voyage AI plan references.
- Remove Python `http.server` from the recommended workflow.
- Keep a short troubleshooting note about host binding and local firewalls.
- Document the SQLite working database and its rebuildable nature.

Completion criteria:

- The documented startup path is `okf-http`.
- Python appears only as historical/troubleshooting context, if at all.
- Users understand that SQLite is an index/work database, not the canonical OKF
  source.

Status: implemented. `okf-http` is now documented as the regular startup path
in the OKF HTTP README, OKF README, docs-browser instructions, and scanlab
documentation pointers. The old Python `http.server` workflow has been removed
from the recommended documentation because it cannot provide OKF API routes.
SQLite is documented as a rebuildable local work/index database for Voyage
analysis, vectors, candidate edges, review state, and audit rows; canonical OKF
source remains Markdown plus frontmatter.

## Phase 11: Tests

- Unit-test CLI parsing.
- Unit-test static path resolution and traversal rejection.
- Unit-test redirect route.
- Unit-test OKF metadata API serialization.
- Unit-test SQLite schema creation and migration.
- Unit-test vector storage and Rust-side cosine similarity.
- Unit-test accepted-edge review state persistence.
- Keep Voyage live tests opt-in and ignored by default.
- Add a smoke test for serving `docs-browser/index.html`.

Completion criteria:

- `cargo test -p okf-http` passes without network or Voyage tokens.
- `cargo test --workspace` remains green.

Status: implemented. The OKF HTTP test surface now covers CLI parsing, browser
asset resolution, traversal and symlink rejection, root redirects, metadata API
path normalization, SQLite schema/migration, SQLite vector storage with
Rust-side cosine similarity, accepted-edge review/audit persistence, and a
router smoke test for serving `docs-browser/index.html`. Voyage live tests
remain opt-in and ignored by default. `cargo test -p okf-http`,
`cargo test -p okf --test voyage_contract`, and `cargo test --workspace` all
pass without mandatory network access or Voyage token spending.

## Entschiedene Fragen

- `docs-browser/` gehört langfristig in das Homeverzeichnis des Benutzers.
  Unter Linux ist der Default `~/docs-browser`.
- OKF-Dokumente können aus beliebigen konfigurierten Ordnern kommen, solange
  Ordnerstruktur und Dokumente OKF-kompatibel sind.
- Die zentrale Herausforderung ist die sichere Verbindung zwischen
  `~/docs-browser` und den konfigurierten OKF-Dokumentwurzeln. Weder
  Browser-Assets noch Markdown-Links dürfen Dateisystemausbrüche ermöglichen.

## Offene Fragen

## Entschiedene Speicherfragen

- Vektoren, Scores, Kandidatenkanten und Review-Zustände gehören in SQLite.
- Die Standarddatenbank ist `.okf-voyage/okf.sqlite`.
- Cosine Similarity wird zuerst in Rust berechnet.
- SQLite-Vektor-Erweiterungen bleiben eine spätere optionale Beschleunigung.
- Canonical OKF Markdown enthält keine rohen Vektoren.
- Akzeptierte Relationen können aus SQLite heraus explizit in OKF-Dateien
  übernommen werden.
- Akzeptierte Relationen werden zunächst als strukturierte
  Frontmatter-/Relations-Metadaten in canonical OKF Markdown gespeichert.
- Die menschenlesbare Darstellung akzeptierter Relationen erzeugt zunächst die
  OKF-Webseite.
- Markdown-Links im Body sind eine optionale spätere Projektion, nicht der erste
  Speichermechanismus.

- Welche Pfade soll die HTTP-API zurückgeben?
  - Die logischen, gemounteten OKF-Pfade, wie `okf::Repository` sie sieht
    (`scanlab/...`, `scql/...`, `okf/...`)?
  - Oder physische Dateipfade?
  - Vorläufige Tendenz: API-Antworten sollten primär logische OKF-Pfade
    verwenden; physische Pfade nur dort, wo sie für lokale Werkzeuge wirklich
    nötig sind.
