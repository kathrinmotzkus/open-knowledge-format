---
title: OKF Voyage AI Analysis Layer
type: Architecture Decision
kind: knowledge-document
topic: okf
status: draft
updated: 2026-07-15
tags: [okf, voyage-ai, embeddings, vectors, architecture]
---

# OKF Voyage AI Analysis Layer

OKF may use Voyage AI as an optional analysis and indexing layer. This layer is
not part of the canonical OKF file format.

## Decision

OKF source documents remain plain Markdown files with YAML frontmatter. They are
the source of truth.

Voyage AI integration must be physically and conceptually separate from those
source documents. It may create derived artifacts such as embeddings, similarity
indexes, suggested graph edges, clusters, or analysis reports, but those
artifacts must be treated as rebuildable cache/index data.

## Motivation

Voyage AI embeddings can make OKF repositories more useful without weakening
the format:

- semantic search can find related documents beyond exact metadata matches
- similar documents can be grouped or ranked
- candidate graph edges can be suggested from semantic proximity
- vector databases can provide fast retrieval over larger OKF repositories

This should enhance OKF, not make OKF dependent on a proprietary service.
In the knowledge-network model, embeddings are a way to discover candidate
connections. They are not the connection itself. The durable connection is only
created when a reviewed relation is written back to canonical frontmatter.

## Boundaries

The core OKF crate must continue to work without:

- network access
- a Voyage AI account
- an API key
- a vector database
- generated embedding artifacts

The Voyage AI layer must not write API keys, raw embeddings, or unreviewed
provider state into canonical OKF Markdown documents. An explicitly accepted
canonical relation retains the provider/model provenance needed to audit its
origin.

## Local Configuration

All Voyage-specific environment variables use the `OKF_VOYAGE_` prefix.

Expected initial variables:

```env
OKF_VOYAGE_API_KEY=...
OKF_VOYAGE_MODEL=voyage-3-large
OKF_VOYAGE_INDEX_ROOT=.okf-voyage
OKF_VOYAGE_BATCH_SIZE=32
OKF_VOYAGE_TIMEOUT_SECONDS=30
OKF_VOYAGE_TPM_LIMIT=3000000
OKF_VOYAGE_RPM_LIMIT=2000
```

`.env` must remain local and must not be committed.

Users who want Voyage AI support for their own OKF projects must provide their
own Voyage AI credentials in their own local `.env` file.

## Derived Data Location

Voyage-derived artifacts should live outside the canonical OKF document tree,
for example:

```text
.okf-voyage/
target/okf-voyage/
external vector database
```

`.okf-voyage/` is ignored by Git and should be treated as disposable derived
state.

The current HTTP workflow uses a SQLite working database below that directory:

```text
.okf-voyage/okf.sqlite
```

SQLite stores derived inventory, chunks, embeddings, candidate edges, review
state, and audit rows for applied suggestions. It is not the canonical OKF
source and can be deleted and rebuilt from Markdown documents and, where
needed, fresh Voyage API calls.

SQLite is authoritative within this rebuildable derived layer. The TSV files
are a compatibility mirror carrying the generation recorded in
`file-index-generation`. Indexing synchronizes document inventory, chunks,
embeddings, and valid suggestions in one SQLite transaction and replaces the
mirror only after that commit. Status reports expose schema health, both
generations, mirror consistency, and recovery requirements.

The authenticated, token-free `POST /api/v1/voyage/rebuild` action retains
only embeddings whose chunk hashes still match current OKF documents, rebuilds
SQLite, and repairs the file mirror. Missing embeddings are reported and can
later be created through the explicit token-spending index action. Deleting
the complete derived directory cannot delete or rewrite canonical Markdown.

SQLite uses WAL, a busy timeout, incremental schema migrations, integrity
triggers, and transaction-time checks. Removed or changed chunks invalidate
their embeddings and suggestions instead of leaving dangling derived edges.

## Implemented Library Surface

The Rust crate exposes the first optional analysis surface as `okf::voyage`.
Hosts can use it to:

- load `OKF_VOYAGE_*` configuration
- run an explicit connectivity check
- inventory OKF documents without spending tokens
- create deterministic Markdown chunks
- estimate token and request usage before API calls
- store embeddings in `.okf-voyage/`
- search a local vector backend
- create AI-marked suggested graph edges
- accept or deny suggestions

Normal `Repository::open` does not call Voyage AI.

`OKF_VOYAGE_TIMEOUT_SECONDS` is applied to both connection establishment and
the complete HTTP exchange of each provider request. A delayed connection or
response therefore ends with the stable `timeout` report code instead of
waiting indefinitely. DNS, TLS, process-launch, generic transport, HTTP, and
structured provider failures have separate safe report codes. Provider HTTP
status and bounded provider error details remain available for diagnosis; API
keys and raw response bodies do not.

Before an index is changed, OKF verifies that every successful batch contains
exactly one finite, non-empty vector per input and that dimensions agree with
the other batches and the existing index. A failed or partial provider run
does not mutate the in-memory or persisted file/SQLite indexes. TPM/RPM
planning, minute-window pacing, and the single retry after HTTP 429 remain in
effect.

Only one indexing request per resolved index root may run inside an
`okf-http` server process. A concurrent request for the same root receives
HTTP 409. Once an authenticated indexing request has been accepted, v1 does
not expose mid-job cancellation: client disconnects do not turn a partial job
into a commit. Graceful server shutdown stops accepting new work and waits for
the in-flight operation to finish or fail; each provider exchange remains
bounded by the configured timeout.

## HTTP and Browser Review Surface

`okf-http` is the recommended local host surface for the OKF browser and the
Voyage workflow. It serves static browser assets from the configured browser
root, exposes OKF documents through explicit routes, and provides API endpoints
for planning, execution, semantic search, review, and applying accepted
relations.

The normal browser startup path during repository development is:

```bash
cargo run -p okf-http -- --browser-root docs-browser 8003
```

Python's `http.server` is not a supported workflow for Voyage-backed OKF
analysis because it cannot provide the required OKF API endpoints.

The local documentation browser exposes a prominent **Run OKF AI analysis**
button and an **OKF AI** mode. The static browser can:

- prepare an analysis/review workflow from loaded documents
- show AI-derived candidate edges
- support accept all, accept one, deny one, and deny all review actions
- persist suggestions and every review decision in SQLite
- export accepted suggestions as a durable JSON artifact
- display accepted review edges and applied canonical relations distinctly in
  the graph view

The default read-only browser disables Voyage and review actions. After an
explicit loopback-only `okf-http --local-editor` start, the browser asks for the
private session token only when an explicit Voyage or review action is
requested and does not persist that token. Its
analysis button performs token planning, asks for confirmation before changed
chunks are embedded, indexes, and creates a durable similarity review set.
Semantic search runs only from its explicit button.

Voyage-derived review rows always include review-set ID, provider, model,
generation method, AI marker, source and target chunks, score, timestamp, and
status. Browser-only metadata prefilters remain visibly separate, use the
`browser_graph_prefilter` method, and cannot be accepted as Voyage results.

Permanent writes are handled by explicit host/API actions. Accepted AI-derived
relations are applied as structured frontmatter metadata; Markdown body links
remain an optional later projection. Canonical provenance includes the accepted
similarity score, while raw vectors remain only in SQLite.

## Suggested Graph Edges

AI-generated graph edges are suggestions, not documented truth.

Suggested edges should carry provenance metadata such as:

- provider
- model
- generation method
- AI-derived marker
- source document or chunk
- target document or chunk
- score or confidence
- creation timestamp
- status, for example `suggested`, `accepted`, or `rejected`

Only accepted edges become part of canonical OKF documents. They are stored as
explicit `relations` frontmatter metadata rather than hidden vector state;
Markdown body links remain an optional later projection.

## Layering

The preferred long-term layering is:

```text
OKF source documents
  ↓
okf repository/parser
  ↓
optional okf-voyage analysis
  ↓
optional vector index/database
  ↓
semantic search and suggested graph edges
```

This keeps OKF open and portable while allowing richer analysis where users
choose to enable it.
