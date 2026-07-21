---
title: OKF HTTP API v1
type: Reference
kind: knowledge-reference
topic: okf-http
status: active
updated: 2026-06-30
tags: [okf, http, api, schema, compatibility]
---

# OKF HTTP API v1

Document summaries include `okf_uri` for mounted roots, `directory_path`, and
`navigation_class`. Clients must build directory navigation from
`source_relative_path` or `directory_path`, not from `topic`, `type`, or
`kind`. Root-level documents use `navigation_class: root-document`; documents
inside real directories use `nested-document`.

The stable namespace is `/api/v1`. Successful JSON responses use:

```json
{"api_version":"v1","data":{}}
```

Errors use a stable machine-readable code:

```json
{"api_version":"v1","error":{"code":"bad_request","message":"..."}}
```

Every response carries `X-OKF-API-Version: v1` and `X-Request-ID`. API
responses also carry `Cache-Control: no-store`. Logs use the same request ID
and never include query strings, API keys, session tokens, or document bodies.

## Compatibility Policy

Fields and endpoints in v1 are additive within the v1 lifecycle. Existing
field meanings and error codes are not changed incompatibly. A breaking change
requires `/api/v2`. The former `/api/okf/...` aliases remain temporarily
available with `Deprecation`, `Sunset`, and successor-version headers; new
clients must use `/api/v1`.

## Privacy

Default responses expose logical OKF paths, mount names, root indices, and
browser routes, but not filesystem paths. Trusted local debugging can opt in
with `--expose-physical-paths`. This may add `source_path`, root `path`/`spec`,
SQLite `path`, and Voyage index paths.

## Read-only Schemas

`GET /api/v1/documents` returns `roots`, `diagnostics`, and `documents`.
Document entries contain title, type, kind, topic, status, updated,
`logical_path`, `source_relative_path`, `browser_path`, `root_index`, optional
`root_mount`, and `is_plan`.

`GET /api/v1/document?path=<logical-path>` returns `document`, `frontmatter`,
`planning`, and Markdown `source`.

Raw content routes `/okf-docs/{mount}/{path}` and the compatibility
`/okf-root/{index}/{path}` route are repository-authorized, not general static
file mounts. Every request revalidates bounded admission, containment,
regular-file status, portable paths, and collisions. Markdown uses fixed
`text/markdown; charset=utf-8`. CSV additionally requires one valid declaration
in the owning `index.md`, valid structure, and fixed
`text/csv; charset=utf-8`. Both use `nosniff`. Hidden, JavaScript, unsupported,
undeclared, malformed, symlinked, colliding, and rejected paths do not expose
contents.

`GET /api/v1/graph` returns `nodes` and `edges`. Node paths are logical paths.

`GET /api/v1/voyage/status` is strictly read-only. It reports provider and
model configuration, file-index embedding count, and optional existing SQLite
counts. Its `integrity` object reports the authoritative backend, schema state,
SQLite and file generations, mirror consistency, recovery need, and concrete
violations. It does not create, migrate, repair, or synchronize SQLite and
spends no tokens.

`GET /api/v1/health` returns `{"status":"ok","mode":"read-only"}` or
`{"status":"ok","mode":"local-editor"}` inside the v1 envelope. In
authenticated TLS mode, its optional `tls` object contains only the protocol,
supported SAN display values, validity interval, and SHA-256 certificate
fingerprint. It never contains certificate/key paths, private-key material, or
session authority.

`GET /api/v1/access/session` is available in every mode. It reports the mode,
whether local pairing remains available, whether the request carries a valid
session cookie, its scope, and its remaining lifetime. It returns neither the
session ID nor the CSRF value.

## Local Pairing and Sessions

Local-editor mode generates a one-time 12-digit pairing code at startup. The
code expires after five minutes, is throttled after failed attempts, and is
consumed by the first successful exchange:

```http
POST /api/v1/access/pair
Host: 127.0.0.1:8003
Origin: http://127.0.0.1:8003
Content-Type: application/json

{"code":"1234-5678-9012"}
```

The endpoint exists only in local-editor mode. It accepts no GET, URL, query,
fragment, or cookie representation of the code. Success sets an `HttpOnly`,
`SameSite=Strict` session cookie and returns `csrf_token`, `csrf_header`, and
`expires_in_seconds`; the session identifier is never returned in JSON.
Protected requests send the CSRF value in `X-OKF-CSRF-Token`. Exact Host and
Origin values and same-origin Fetch Metadata are required. Sessions expire
after 30 minutes and are held only in process memory.

After a page reload, `POST /api/v1/access/session/refresh` recovers the CSRF
value for an existing valid cookie. It requires exact Host and Origin plus
same-origin Fetch Metadata, returns no session identifier, and is unavailable
outside local-editor and authenticated TLS modes.

`POST /api/v1/access/logout` requires the session cookie and CSRF header,
revokes the server-side session, and expires the cookie. Restarting
`okf-http` invalidates every local pairing session. These new endpoints have no
deprecated `/api/okf` aliases.

Authenticated TLS mode additionally exposes `POST /api/v1/access/login` with a
JSON username and password. The route is not mounted on HTTP. Success returns
the username, effective capabilities, CSRF value, and expiry while setting a
`Secure`, `HttpOnly`, `SameSite=Strict` cookie; neither password nor session ID
is returned. Failed, disabled, removed, and unknown accounts share the same
public failure response.

Persistent sessions use the same CSRF, Host, Origin, and Fetch Metadata checks
as paired sessions. Effective capabilities are recalculated from current roles
and guarded by a per-user authorization revision, so password changes,
disablement, removal, grants, and revocations invalidate existing sessions.
TLS-only account routes are:

- `POST /api/v1/access/password`: verify and change the current user's password
- `POST /api/v1/access/sessions/revoke`: revoke all current-user sessions
- `GET /api/v1/access/users`: list users with `users.manage`
- `POST /api/v1/access/sessions/revoke-user`: revoke a user's sessions with
  `security.manage`

### Reverse-Proxy Boundary

Network-facing operation should be deployed through a reverse proxy while
`okf-http` remains bound to loopback. Trusted-proxy mode is configured with
`--trusted-proxy`, an exact `--public-origin https://...`, and
`OKF_HTTP_TRUSTED_PROXY_TOKEN`. The proxy-to-OKF hop remains HTTPS.

Trusted proxy requests must present a backend Host matching the OKF listener,
the secret `X-OKF-Trusted-Proxy-Token`, and exactly one matching
`X-Forwarded-Proto: https` and `X-Forwarded-Host`. Ambiguous/repeated values,
the standard `Forwarded` header, downgrade, direct backend access, and incorrect
Origin values are rejected. Forwarded client IP is never an identity source.

In trusted-proxy deployments, ordinary OKF content APIs need a valid persistent
session even though the same routes remain anonymous in local loopback modes.
Remote `/config/roots` access is forbidden. Health reports the deployment
boundary and whether anonymous document reads are enabled without exposing the
proxy token.

## Planning and Action Schemas

`POST /api/v1/voyage/plan` scans documents and returns configuration,
estimated token/request counts, cache counts, rate limits, optional read-only
storage counts, `spends_tokens: false`, and a message. It does not synchronize
SQLite.

The following endpoints are mounted in explicit local-editor and authenticated
TLS modes. They require the capability declared by the route matrix plus the
session cookie and `X-OKF-CSRF-Token`, or the transitional automation token
where that token is still allowed:

- `POST /api/v1/voyage/check`: connectivity `report` and optional `index`
- `POST /api/v1/voyage/index`: embedding report and index summary
- `POST /api/v1/voyage/rebuild`: token-free derived-index rebuild and integrity
  report
- `POST /api/v1/voyage/search`: `report` and ranked `results`
- `POST /api/v1/suggestions/generate`: create a durable similarity review set
- `GET /api/v1/suggestions?review_set=<id>`: list a review set with provenance
- `POST /api/v1/suggestions/<id>/accept` and `/deny`: review one suggestion
- `POST /api/v1/review-sets/<id>/accept-all` and `/deny-all`: review a defined
  set
- `POST /api/v1/suggestions/import`: validate an exported artifact against
  existing suggestions and import accepted statuses
- `POST /api/v1/edges/apply`: apply summary and per-logical-document results
- root configuration writes

## Action and Token-Cost Matrix

| Action | Editor authority | Provider tokens | Persistent effect |
| --- | --- | --- | --- |
| Browse documents/graph | No; available read-only | No | None |
| Voyage status/plan | No; available read-only | No | None |
| Voyage connectivity check | Paired local editor or transitional token | Yes, tiny embedding request | None |
| Voyage index | Paired local editor or transitional token | Yes for changed chunks | SQLite and file mirror |
| Semantic search | Paired local editor or transitional token | Yes for each query embedding | None beyond provider usage |
| Generate/review suggestions | Paired local editor or transitional token | No | SQLite review state |
| Apply accepted relations | Paired local editor or transitional token | No | Canonical Markdown and SQLite audit |
| Rebuild derived state | Paired local editor or transitional token | No | SQLite and file mirror |
| Preview document root | Paired local editor or persistent HTTPS `roots.propose` | No | Expiring process-memory proposal |
| Register/update/remove root | `roots.configure`, exact revision/digest, confirmation header | No | Versioned XDG `config.toml`; restart required |
| Plan source initialization | `content.initialize`, current proposal digest | No | None |
| Initialize source | `content.initialize`, exact plan digest, source confirmation | No | Canonical Markdown through recoverable journal |

The browser shows an estimate and asks for confirmation before indexing changed
chunks. Direct API clients are responsible for calling the token-free plan
endpoint and obtaining user approval before invoking a token-spending endpoint.
The API never performs such approval on a client's behalf.

`GET /api/v1/health` reports the active `read-only` or `local-editor` mode.
Read-only mode does not mount sensitive read, mutation, canonical-write,
derived-state, or token-spending routes. Supplying an old startup token cannot
grant those routes in read-only mode.

Root management uses only `/api/v1/roots/...`:

| Method and path | Purpose |
| --- | --- |
| `GET /api/v1/roots/configuration` | Configuration revision, stable roots and override state |
| `POST /api/v1/roots/proposals` | Create registration or source-initialization proposal |
| `GET /api/v1/roots/proposals/<id>` | Read cached proposal tree and remaining lifetime |
| `POST /api/v1/roots/proposals/<id>/registration` | Register the exact reviewed root |
| `POST /api/v1/roots/proposals/<id>/initialization/plan` | Render exact source before/after bytes and diffs |
| `POST /api/v1/roots/proposals/<id>/initialization` | Apply the separately confirmed source plan |
| `PUT /api/v1/roots/<root-id>` | Change mount, enabled state, priority, or monitoring preference |
| `DELETE /api/v1/roots/<root-id>` | Remove configuration without deleting source documents |
| `GET /api/v1/roots/monitoring` | Read cached monitoring status without scanning |
| `POST /api/v1/roots/<root-id>/monitoring/check` | Run one explicit token-free root scan |
| `GET /api/v1/roots/<root-id>/monitoring/pending` | Read the cached exact change set and admission inventory |
| `POST /api/v1/roots/<root-id>/monitoring/accept` | Revalidate and atomically accept the snapshot baseline |
| `POST /api/v1/roots/<root-id>/monitoring/dismiss` | Clear the notice without changing the baseline |

Proposal and configuration responses may contain physical paths and therefore
require `roots.propose`. Mutations require same-origin session and CSRF checks,
operation capabilities, explicit confirmation headers, exact proposal and plan
digests, and configuration revisions. Structured conflict codes distinguish
missing, expired, stale, unstable, digest-mismatched and concurrently modified
state. API audit events contain only action, stable/proposal reference, and
success state—not source contents, physical paths, credentials, or secrets.

The startup automation token cannot authorize root management. Trusted-proxy
deployments cannot create proposals or mutate roots even when an authenticated
account exists. Admitted document and declared-resource reads remain anonymous
on the local read-only server. Browser API code never writes `.env`.

Legacy `POST`, `PUT`, and `DELETE` routes below `/api/v1/config/roots` and
`/api/okf/config/roots` return `410 Gone`. The legacy GET remains temporarily
available as a deprecated compatibility response.

The packaged browser exposes this protocol through its **Document roots** tab.
It creates one cached registration proposal only after an explicit preview,
requires acknowledgement of the complete tree, and does not rescan on reload.
Read-only registration and source initialization remain separate: the latter
uses a new source-initialization proposal and exact plan, displays every diff,
and sends the source confirmation only after a second acknowledgement. Tree
filtering never changes the server-side proposal, and rejected entries cannot
be force-admitted.

Change review is enabled per root by `check_for_changes`. The first startup
scan establishes a rebuildable SQLite baseline; later startup scans compare
against it and manual scans run only through `monitoring/check`. Status and
detail reads never scan. Acceptance requires `X-OKF-Change-Review: accept` and
the exact current snapshot digest; dismissal requires the corresponding
`dismiss` value. Pending added or modified paths are withheld from document
routes and inventories. Monitoring is token-free and never mutates source,
canonical relations, embeddings, Voyage data, configuration, or `.env`.

Suggestion generation uses existing Voyage embeddings and spends no provider
tokens. Every suggestion response includes `id`, `review_set_id`, `provider`,
`model`, `generation_method`, `ai_generated`, source/target chunks and
documents, `score`, `created_at`, and `status`. Review mutations are
transactional SQLite operations and update the generation-marked file mirror.

`POST /api/v1/edges/apply` is the only API transition from accepted derived
state to canonical Markdown. It revalidates current source/target documents and
chunks, writes `relations` atomically, and then records the SQLite audit. Each
file result reports `applied`, `audited`, `recovered`, `skipped`, and `changed`.
If a file write succeeded but audit recording failed, repeating the explicit
request repairs the audit row without adding a duplicate relation.

`GET /api/v1/graph` projects canonical relations with a `provenance` object.
AI-derived entries include suggestion ID, provider, model, generation method,
source/target chunk IDs, score, creation timestamp, status, and the AI marker;
raw vectors are never returned.

An import artifact has `type: "okf-ai-edge-review"`, `review_set_id`, and a
non-empty `accepted_edges` array containing the persisted provenance fields.
Import never creates suggestions: every row must exactly match an existing row
in that review set. Duplicate IDs, placeholder models, non-AI rows, malformed
scores, or changed provenance are rejected.

Voyage action failures return the normal v1 data envelope with a non-success
HTTP status and a `report`. Stable `report.api_error_code` values include
`timeout`, `dns_error`, `tls_error`, `process_launch_error`, `transport_error`,
`http_error`, `provider_error`, `malformed_provider_response`,
`embedding_count_mismatch`, and `embedding_dimension_mismatch`. When Voyage
supplies its own safe code, that provider code is retained. `report.http_status`
and a bounded safe provider message are preserved when available.

Only one `/voyage/index` operation may run for a resolved index root in one
server process. `/voyage/rebuild` uses the same lock. A duplicate receives HTTP
409 and the common `conflict` error. Accepted jobs have no v1 cancellation
endpoint and are allowed to finish or fail cleanly during graceful shutdown.

SQLite schema version 4 is the authoritative derived index. Successful index
and rebuild actions commit SQLite first, then atomically replace the
generation-marked TSV mirror. A post-commit mirror failure is returned as an
error and remains visible through status until rebuild repairs it. SQLite uses
WAL and a five-second busy timeout, so local readers can continue while the
single writer commits.

## Error Codes

The common stable codes are `bad_request`, `unauthorized`, `forbidden`,
`not_found`, `conflict`, `precondition_required`, `rate_limited`,
`provider_error`, `service_unavailable`, `internal_error`, and
`request_failed`. Server-side details are logged with the request ID; 5xx
responses return a safe generic message.
