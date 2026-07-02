---
title: Voyage AI Integration Plan for OKF
type: Plan
kind: knowledge-plan
topic: okf
status: draft
updated: 2026-06-24
tags: [okf, voyage-ai, embeddings, vectors, rate-limits]
---

# Voyage AI Integration Plan for OKF

This plan describes how to add optional Voyage AI analysis to OKF without
making Voyage AI part of the canonical OKF file format.

Historical planning input: the active Voyage AI billing period had used exactly
0 tokens when this plan was written. This is not a statement about current
usage.

## Implementation Status

The first library-level implementation is in place in `okf::voyage`.

Implemented:

- `OKF_VOYAGE_*` configuration loading with redacted API-key handling
- `.okf-voyage/` Git ignore policy
- explicit connectivity checks with HTTP status and provider error reporting
- repository inventory without API calls
- deterministic Markdown chunking
- token/request dry-run planning
- local file-backed derived index storage
- incremental embedding updates for changed chunks
- minimal Voyage embedding transport via `curl --http1.1`
- local semantic search over stored vectors
- AI-marked suggested graph edges
- review actions: accept all, accept one, deny one, deny all
- accepted-edge review rendering and canonical relation graph projection
- rate-limit-aware embedding execution with conservative TPM/RPM pacing
- SQLite working/index database at `.okf-voyage/okf.sqlite`
- `okf-http` API endpoints for status, planning, explicit token-spending
  execution, semantic search, review state, and applying accepted relations
- docs-browser OKF AI panel for starting analysis, persisting review decisions
  in SQLite, exporting accepted changes, explicitly applying canonical
  relations, and visualizing review/canonical edges in the graph
- unit tests for offline behavior
- ignored live test for explicit token-spending connectivity checks

Still intentionally host-owned or future work:

- user-facing commands
- broader user-facing commands beyond `okf-http`
- replacing the current isolated `curl` transport with a dedicated HTTP client
  if the crate later accepts an HTTP dependency
- external vector database backends

The browser and `okf-http` provide a tested local workflow:
planning is token-free, execution is explicit, derived state is stored in
SQLite, accepted suggestions can be reviewed, and accepted AI-derived relations
can be written to canonical OKF Markdown as structured frontmatter metadata.

## Goals

- Keep OKF source documents plain, portable, and usable without network access.
- Add Voyage AI as an optional analysis layer for embeddings and semantic
  discovery.
- Store all Voyage-derived data outside canonical OKF documents.
- Make token usage predictable before sending requests.
- Support rebuilds that only re-embed changed content.
- Prepare for future vector databases without requiring one initially.
- Keep AI-generated graph edges marked as suggestions until accepted.

## Non-Goals

- Do not require Voyage AI for normal OKF loading or querying.
- Do not store API keys, raw embeddings, or unreviewed provider state in OKF
  Markdown files. Applied relations retain auditable provider provenance.
- Do not automatically call Voyage AI during normal `Repository::open`.
- Do not treat inferred semantic edges as documented truth.
- Do not introduce a central registry of OKF document types.

## Rate-Limit Assumptions

The original `voyage-3-large` planning values supplied for this project were:

- 3,000,000 tokens per minute
- 2,000 requests per minute

For this project, the document count is small and file sizes are known before
indexing. That means the indexer can estimate token and request usage before it
sends any request.

The implemented dry-run plan reports:

- document count
- chunk count
- estimated input tokens
- estimated request count
- selected model
- configured limits
- cached chunks
- changed chunks
- expected token cost for this run

## Environment Configuration

All Voyage-specific variables use the `OKF_VOYAGE_` prefix:

```env
OKF_VOYAGE_API_KEY=...
OKF_VOYAGE_MODEL=voyage-3-large
OKF_VOYAGE_INDEX_ROOT=.okf-voyage
OKF_VOYAGE_BATCH_SIZE=...
OKF_VOYAGE_TIMEOUT_SECONDS=...
OKF_VOYAGE_TPM_LIMIT=3000000
OKF_VOYAGE_RPM_LIMIT=2000
```

Provider limits can change. Users must configure values appropriate for their
current Voyage account instead of treating historical planning values as a
provider guarantee.

`.env` remains local and ignored by Git.

When `.okf-voyage/` is introduced, it should also be ignored by Git.

## Proposed Layers

```text
okf
  canonical documents, roots, metadata, diagnostics

okf-voyage
  optional Voyage AI client, chunking, token planning, embeddings

okf-index
  optional local/vector index storage and semantic retrieval

host applications
  commands, UI, review workflows, accepted-edge policy
```

The exact crate names can change, but the boundaries should not: core OKF must
not depend on Voyage AI.

## Phase 1: Configuration and Safety

- Add `.okf-voyage/` to `.gitignore`.
- Keep `OKF_VOYAGE_API_KEY` in `.env`.
- Define a small configuration loader for Voyage-specific settings.
- Ensure all logs redact the API key.
- Add a connectivity check that sends one tiny request only when explicitly
  asked.

Completion criteria:

- The API key is never printed.
- Missing Voyage configuration does not affect normal OKF behavior.
- A connectivity check can report success/failure without embedding the whole
  repository.
- On failure, the connectivity check reports the HTTP status code and, when
  available, the provider's API error code/message without printing secrets or
  request bodies.

Status: implemented in `VoyageConfig`, `CurlVoyageTransport`, and
`check_connectivity`.

## Phase 2: Repository Inventory and Token Planning

- Scan configured OKF document roots.
- Exclude reserved files such as `index.md` and `log.md` unless a later policy
  intentionally includes them.
- Compute file size and content hash for each candidate document.
- Estimate tokens per document and per chunk before making API requests.
- Produce a dry-run plan.

Completion criteria:

- A user can see estimated token and request usage before spending tokens.
- The plan distinguishes unchanged cached content from content that needs new
  embeddings.

Status: implemented in `inventory`, `chunk_repository`, and `TokenPlan`.

## Phase 3: Chunking Policy

- Define deterministic Markdown chunking.
- Prefer semantic boundaries:
  - document
  - heading section
  - paragraph/list/table block
- Preserve enough metadata for each chunk:
  - document logical path
  - document title
  - `type`
  - `kind`
  - topic/status/tags when available
  - heading path
  - content hash

Completion criteria:

- Re-running the chunker on unchanged documents produces stable chunk IDs.
- Small documents can remain one chunk.
- Large documents split predictably without losing context.

Status: implemented in deterministic Markdown chunking for OKF documents.

## Phase 4: Local Derived Index

- Create `.okf-voyage/` as a derived-data root.
- Store index metadata, chunk hashes, model name, embedding dimensions, and
  generated vectors.
- Treat this directory as disposable cache.
- Add rebuild logic:
  - skip unchanged chunks
  - delete stale chunks
  - re-embed changed chunks

Completion criteria:

- A full rebuild can be deleted and recreated from OKF source documents.
- Incremental rebuilds only spend tokens for changed chunks.

Status: implemented in `LocalIndex` and `embed_changed_chunks`.

## Phase 5: Voyage Embedding Client

- Implement the minimal embedding request path.
- Use `OKF_VOYAGE_MODEL`, defaulting to `voyage-3-large` while configurable.
- Batch chunks conservatively.
- Rate-limit requests according to configured or known TPM/RPM.
- Retry only safe transient failures.
- Record provider/model/provenance in derived metadata.

Completion criteria:

- The client embeds a small planned batch successfully.
- Token/request usage stays within the dry-run plan.
- Failures do not corrupt the local index.

Status: implemented as an isolated `curl --http1.1` transport plus an opt-in
ignored live test. Normal tests do not call Voyage AI.

Rate-limit status: implemented in the shared OKF Voyage library. Changed chunks
are embedded in configurable batches. Before each batch, OKF checks whether the
next request would exceed the configured TPM or RPM window. If so, it waits for
a fresh minute window before continuing. If a single batch is estimated to
exceed the configured TPM limit, OKF fails before calling Voyage AI. If Voyage
AI returns `429`, OKF waits for a fresh minute window and retries that batch
once.

## Phase 6: Semantic Search

- Add a local semantic lookup path over `.okf-voyage/`.
- Return ranked chunks/documents with scores and provenance.
- Keep exact OKF lookup separate from semantic lookup.

Completion criteria:

- Exact OKF queries still work without Voyage.
- Semantic search clearly reports that results come from the derived Voyage
  index.

Status: implemented through the `VectorBackend` trait and `LocalIndex`
semantic search results with provider/model provenance.

## Phase 7: Suggested Graph Edges

- Compute candidate edges from semantic similarity.
- Store them as derived suggestions, not canonical OKF truth.
- Mark every AI-generated or AI-assisted suggestion explicitly as AI-derived.
- Include provenance:
  - provider
  - model
  - generation method, for example `embedding_similarity`
  - AI label, for example `ai_generated: true`
  - source chunk
  - target chunk
  - score
  - created timestamp
  - status

Completion criteria:

- Suggested edges can be listed and reviewed.
- Suggested edges are clearly marked as AI-derived in both machine-readable
  metadata and human-facing output.
- No suggested edge modifies OKF Markdown automatically.

Status: implemented through `suggest_edges` and AI-derived suggestion metadata.

## Phase 8: Review and Acceptance Workflow

- Define how a suggested edge can become canonical.
- Treat AI as the right tool for discovering candidate relationships at this
  stage, while keeping acceptance under explicit user control.
- Provide review actions:
  - accept all suggestions in the current review set
  - accept a single suggestion
  - deny a single suggestion
  - deny all suggestions in the current review set
- Store explicitly applied relations as structured OKF frontmatter metadata.
- Keep rejected suggestions recorded only in derived state.
- Expose the workflow prominently in the OKF website/browser.
- Persist accepted suggestions in SQLite and provide export and explicit
  canonical-application paths.

Completion criteria:

- Humans can accept/reject suggestions.
- The review interface supports `accept all`, `accept <entry>`,
  `deny <entry>`, and `deny all`.
- Accepted edges have a clear export/apply path so a host tool can make them
  visible in normal OKF files.
- The origin of accepted AI suggestions remains auditable.
- The OKF browser has a prominent button to start AI analysis and review.
- Accepted suggestions survive restart in SQLite and can be exported or
  explicitly applied to canonical frontmatter.

Status: review actions, canonical graph rendering, a docs-browser review panel,
SQLite review state, and `okf-http` apply support are implemented. Accepted
suggestions are stored in SQLite review state, can be exported as JSON, and are
explicitly applied to OKF files as structured frontmatter relations. The graph
projects canonical relations with AI provenance. Raw vectors remain in SQLite.

## Phase 9: Optional Vector Database Backend

- Keep `.okf-voyage/` as the first storage backend.
- Add a backend abstraction before introducing external databases.
- Evaluate candidates such as PostgreSQL/pgvector, Qdrant, LanceDB, or SQLite
  vector extensions.

Completion criteria:

- OKF can use local file-backed embeddings without a database.
- A future database backend can be added without changing canonical OKF
  documents.

Status: implemented through the file-backed `LocalIndex` and the `VectorBackend`
trait.

## Phase 10: Tests

- Unit-test chunking and hash stability without network access.
- Unit-test rate-limit planning with fixed fixtures.
- Unit-test cache invalidation.
- Keep live Voyage tests opt-in and ignored by default.
- Never run live Voyage tests in ordinary CI unless explicitly configured.

Completion criteria:

- Normal test runs do not spend Voyage tokens.
- Live tests require `OKF_VOYAGE_API_KEY`.
- Live tests use tiny inputs and print token-safe summaries only.

Status: implemented in `tests/voyage_contract.rs`; the live test is ignored by
default.

## Decisions

- Reserved `index.md` and `log.md` files are navigation/history documents and
  are skipped by Voyage inventory and embedding.
- SQLite is the authoritative local derived index. Generation-marked TSV files
  are a compatibility/recovery mirror, not a second source of truth.
- Semantic search belongs to the optional `okf::voyage` layer and is exposed by
  `okf-http`. Query-language integrations such as SCQL may adapt that API but
  do not own OKF semantic search.
- Explicitly applied relations use canonical `relations` frontmatter metadata.
  Human-readable Markdown body links remain an optional later projection.

## Open Questions

- Should German and English documents share language-aware metadata in chunks?
