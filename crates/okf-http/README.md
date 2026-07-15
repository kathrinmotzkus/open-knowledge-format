# okf-http

`okf-http` is the HTTP server binary for OKF browser and API workflows. It is
the recommended local way to serve the OKF browser; Python's `http.server` is
only useful as historical troubleshooting context and does not provide the OKF
APIs.

For a complete first installation, configuration, browser, Voyage, review,
apply, and rebuild walkthrough, see
[Getting Started with Standalone OKF](../okf/docs/knowledge/getting-started.md).
Release history is recorded in [CHANGELOG.md](CHANGELOG.md). Compatibility
milestones shared with the OKF ecosystem are documented in the
[OKF versioning roadmap](../okf/docs/knowledge/plans/versioning-roadmap.md).

## Usage

From a source checkout, the repository installer builds the binary and keeps
browser installation in the invoking user account:

```bash
./install.sh --release
```

Provision the packaged browser once before the first normal start:

```bash
okf-http --install-browser
okf-http 8003
```

The ordinary start is read-only: document, graph, metadata, health, and
token-free planning routes remain available without authentication, while
mutation and token-spending routes are not mounted. To enable protected editor
APIs and one-time local pairing, start explicitly with:

```bash
okf-http --local-editor 8003
```

On Linux this installs into `~/docs-browser`. Use `--browser-root <path>` with
the install command to choose another location. Re-running the command is
idempotent. If a packaged asset was edited locally, the installer refuses to
replace it unless `--force` is supplied; unrelated user files are preserved.

```bash
okf-http --install-browser --force
```

Server examples:

```bash
okf-http 8003
okf-http --browser-root ~/docs-browser 8003
okf-http --root okf=crates/okf/docs/knowledge 8003
okf-http --root okf=./primary --add-root okf=/usr/local/share/okf 8003
okf-http --local-editor --root okf=crates/okf/docs/knowledge 8003
```

During repository development, when the browser assets still live in the
workspace, this is usually the most convenient command:

```bash
cargo run -p okf-http -- --browser-root docs-browser 8003
```

The default bind host is `127.0.0.1`. The port is required and must be
unprivileged (`>= 1024`).

The default browser root is `~/docs-browser`. Use `--browser-root <path>` or
`OKF_BROWSER_ROOT` when developing against another browser directory.

If required browser assets are missing at startup, `okf-http` exits with a
provisioning command instead of serving a broken page.

Browser-managed document roots are stored in
`$XDG_CONFIG_HOME/okf/config.toml`, falling back to
`~/.config/okf/config.toml`. Process-level or `.env`
`OKF_DOCUMENT_ROOTS` remains an operator override. A mounted `--root` replaces
configured roots with the same mount for the current process and has highest
priority. `--add-root` appends an explicit lower-priority fallback. Malformed
mount specifications are rejected.

In local-editor and authenticated local HTTPS modes, snapshot-bound root
management is available through:

```text
GET    /api/v1/roots/configuration
POST   /api/v1/roots/proposals
GET    /api/v1/roots/proposals/<proposal-id>
POST   /api/v1/roots/proposals/<proposal-id>/registration
POST   /api/v1/roots/proposals/<proposal-id>/initialization/plan
POST   /api/v1/roots/proposals/<proposal-id>/initialization
PUT    /api/v1/roots/<root-id>
DELETE /api/v1/roots/<root-id>?expected_revision=<revision>
GET    /api/v1/roots/monitoring
POST   /api/v1/roots/<root-id>/monitoring/check
GET    /api/v1/roots/<root-id>/monitoring/pending
POST   /api/v1/roots/<root-id>/monitoring/accept
POST   /api/v1/roots/<root-id>/monitoring/dismiss
```

Physical paths and proposals require a paired local session or persistent HTTPS
account with `roots.propose`; the transitional startup token is deliberately
insufficient. Registration, updates, and removal additionally require
`roots.configure`, the reviewed proposal or configuration revision, and
`X-OKF-Config-Write: confirm`. Source planning and writes require
`content.initialize`, an unexpired proposal and exact plan digest; writing also
requires `X-OKF-Source-Write: confirm`.

Configuration writes use the private versioned TOML file and never modify
`.env`. Initialization uses private XDG state outside the document root for
locks, staging, rollback, and recovery. Root-management APIs are unavailable in
read-only mode and forbidden for remote or trusted-proxy deployments. The old
direct `/api/v1/config/roots` mutation routes return `410 Gone`; their GET form
remains a deprecated compatibility view.

The browser's **Document roots** view implements the same guarded workflow. It
accepts a path as text, displays the complete bounded proposal tree, rejected
entries, conflicts, limits, and active operator overrides, and requires an
explicit review acknowledgement before registration. **Register read-only**
changes only `config.toml`. **Register and initialize OKF** then creates a
separate source proposal, displays exact diffs, and requires a second source
write confirmation. Proposal filtering is local and never changes admission;
refreshing the browser does not automatically rescan roots.

Roots with `check_for_changes = true` are scanned once asynchronously at
startup. Baseline inventories and pending reviews live in the rebuildable
`root-monitor.sqlite` under the private OKF state directory. Browser refresh
reads cached status only; subsequent scans require **Check now**. Added,
modified, deleted, rename candidates, hidden/rejected entries, metadata errors,
and conflicts remain pending and are excluded from document responses until an
unchanged digest is explicitly accepted. Dismissal clears the current notice
without moving the baseline, so the change can reappear later. Neither action
invokes Voyage or mutates source files, relations, embeddings, or `.env`.
New registrations seed the baseline from their revalidated proposal, and a
separately reviewed source initialization advances it to the exact post-write
inventory; startup initialization is only the migration fallback for older
monitored configuration.

An operator may deliberately import existing `.env` roots from the console:

```bash
okf-http roots import-env
```

Every imported root must have a valid `okf_root_id` in its root `index.md`.
The source documents and `.env` remain unchanged.

If a local firewall or host policy blocks loopback traffic, explicitly choose a
bind host:

```bash
okf-http --host 127.0.0.1 8003
```

`127.0.0.1` remains the default because the OKF browser is a local editing and
analysis surface.

IPv6 bind addresses are accepted directly, for example `--host ::1`; generated
browser URLs use bracketed IPv6 notation.

At local-editor startup, `okf-http` prints a cryptographically random one-time
pairing code. It expires after five minutes and creates a 30-minute server-side
session when submitted to `POST /api/v1/access/pair`. The code is accepted only
in the JSON POST body. The resulting session uses an `HttpOnly`,
`SameSite=Strict` cookie and requires the returned CSRF value in
`X-OKF-CSRF-Token` for every protected request. `POST /api/v1/access/logout`
revokes it; expiry and process restart do the same.

The packaged browser uses this pairing flow and keeps its CSRF value only in
memory. Local-editor mode also writes the transitional random session token to
a private mode-`0600` runtime file for existing automation clients during the
documented migration period. Those clients can send it as:

```text
X-OKF-Session-Token: <startup-token>
```

Paired requests require the listener's exact Host and Origin and reject
cross-site Fetch Metadata. Local-editor mode always refuses a non-loopback bind.

## Authenticated TLS Mode

For optional local HTTPS, create and inspect an OKF-local CA without external
executables or root privileges:

```bash
okf-http tls init
okf-http tls status
okf-http tls verify
okf-http tls renew
```

Managed files live under `$XDG_STATE_HOME/okf/tls` (normally
`~/.local/state/okf/tls`). No trust store is changed automatically. Import only
the printed public `ca-cert.pem` through the browser or current-user platform
UI if local policy permits it. The full trust, renewal, and recovery workflow
is documented in the [getting-started guide](../okf/docs/knowledge/getting-started.md#7-optional-local-https).

Plain HTTP is always loopback-only. Start HTTPS with explicit PEM files:

```bash
okf-http --authenticated \
  --tls-cert ~/.config/okf/tls/server-chain.pem \
  --tls-key ~/.config/okf/tls/server-key.pem \
  8443
```

The private key must be owner-only on Unix, for example mode `0600`. Before
binding, `okf-http` parses the complete certificate chain and key, verifies
that they match, checks current validity and confirms that the leaf SAN covers
the configured host. DNS `localhost`, IPv4 `127.0.0.1`, and IPv6 `::1` SANs are
supported. Any validation failure aborts startup; HTTPS never falls back to
HTTP.

Remote HTTPS additionally requires a concrete bind address, explicit opt-in,
and an explicit transitional automation token. The certificate must cover the
concrete address:

```bash
export OKF_HTTP_SESSION_TOKEN='replace-with-at-least-32-random-characters'
okf-http --authenticated \
  --host 192.0.2.10 --allow-remote \
  --tls-cert /private/okf/server-chain.pem \
  --tls-key /private/okf/server-key.pem \
  8443
```

Unspecified addresses such as `0.0.0.0` and `::` are rejected because their
identity cannot be represented by the URL and certificate check. In
authenticated TLS mode the browser offers persistent account sign-in while
ordinary document reading remains anonymous.

Physical filesystem paths are omitted from API responses by default. Trusted
local debugging can opt in explicitly by combining `--local-editor` with
`--expose-physical-paths`.

## Persistent User Administration

Persistent users are managed only from the local CLI. Browser pairing cannot
create, change, or remove accounts:

```bash
okf-http user add alice
okf-http user grant alice editor
okf-http user grant alice voyage
okf-http user list
okf-http user passwd alice
okf-http user revoke alice voyage
okf-http user disable alice
okf-http user remove alice
```

`add` and `passwd` prompt twice without terminal echo. They never accept a
password as a positional argument or option value. Controlled automation may
pipe exactly one password line with `--password-stdin`; this shifts protection
of the pipeline and its input to the caller and should not be used from a
shared shell history.

Only ASCII usernames are accepted. They are normalized to lowercase, which
prevents case-only duplicates and Unicode-confusable account names. Initial
roles are `editor`, `voyage`, and `admin`; their capabilities are enforced by
the shared route middleware. `voyage.spend` is never implied by `editor`.

Credentials are stored in `$XDG_STATE_HOME/okf/auth.sqlite`, normally
`~/.local/state/okf/auth.sqlite`, with owner-only permissions. The database
contains Argon2id hashes with random salts, roles, schema version, and security
audit events—never plaintext passwords. Disabled and removed users fail the
same credential check as unknown users. Successful authentication upgrades an
older supported Argon2 parameter set automatically.

`POST /api/v1/access/login` exists only in authenticated TLS mode. It creates a
short-lived server-side session and a `Secure`, `HttpOnly`, `SameSite=Strict`
cookie. The browser keeps the accompanying CSRF token only in memory. Logout,
self-service password change, account disablement, removal, and every role
change invalidate existing authority. Administrators can list users and revoke
sessions through TLS-only capability-checked endpoints; account bootstrap and
recovery remain available from the CLI.

## Remote and Enterprise Deployment

Direct remote binding remains an explicit authenticated-TLS deployment. The
certificate SAN must cover the concrete bind address and `--allow-remote` is
mandatory. Unlike local operation, remote document and graph reads require a
persistent login; browser assets, health, and login bootstrap remain public.
Document-root inspection and mutation are disabled remotely.

For a TLS-terminating enterprise reverse proxy, keep the OKF backend on
loopback and TLS-enable both hops:

```bash
export OKF_HTTP_TRUSTED_PROXY_TOKEN='use-a-random-secret-of-at-least-32-characters'
okf-http --authenticated \
  --trusted-proxy \
  --public-origin https://knowledge.example \
  --tls-cert ~/.local/state/okf/tls/server-chain.pem \
  --tls-key ~/.local/state/okf/tls/server-key.pem \
  8443
```

The proxy must connect to `https://127.0.0.1:8443`, validate the backend
certificate, set backend `Host` to `127.0.0.1:8443`, and replace—not append—
these headers:

```text
X-Forwarded-Proto: https
X-Forwarded-Host: knowledge.example
X-OKF-Trusted-Proxy-Token: <OKF_HTTP_TRUSTED_PROXY_TOKEN>
```

Strip client-supplied `Forwarded`, `X-Forwarded-*`, and
`X-OKF-Trusted-Proxy-Token` before adding trusted values. The backend rejects
direct requests, repeated/comma-separated forwarding values, HTTP downgrade,
wrong public origins, and ambiguous forwarded client identities. It never uses
forwarded client addresses as authentication.

The proxy token is deployment state, not `.env` or canonical OKF content; keep
it in the proxy/service secret facility. The built-in local CA is a convenient
personal backend trust anchor, not an enterprise PKI. Managed environments
should use administrator-provided certificates and browser trust policy.
Neither setup requires modifying Markdown documents, and users without root
rights can still run the loopback-only local modes.

`/` redirects to `/docs-browser/index.html`. Browser assets are served only
from the configured browser root; path traversal and symlink escapes are
rejected.

Configured OKF document roots are served through explicit document routes:

```text
/okf-docs/<mount>/<path>
/okf-root/<root-index>/<path>
```

Mounted roots use `/okf-docs/`; unmounted roots receive a stable index for the
running process under `/okf-root/`. Every `browser_path` returned by
`GET /api/v1/documents` is directly fetchable. The browser obtains its entire
navigation inventory from that API, so adding a standalone OKF repository does
not require editing `app.js`.

These are not static directory mounts. They serve only files from a complete
bounded admission inventory: admitted Markdown or structurally valid CSV
uniquely declared by its owning `index.md`. Hidden files, JavaScript, symlinks,
unsupported or colliding paths, undeclared CSV, and arbitrary regular files
remain unavailable even below a configured root.

The optional `--scanlab-compat` adapter enables scanlab's former browser paths:

```text
/docs/...                       -> scanlab mount
/crates/scql/docs/knowledge/... -> scql mount
/crates/okf/docs/knowledge/...  -> okf mount
```

Repository root files are not served wholesale. Only a small allowlist is
available under `/repo-files/` while `--scanlab-compat` is active. The generic
OKF server does not expose those routes.

Read-only OKF metadata is exposed through the stable v1 JSON endpoints:

```text
GET /api/v1/documents
GET /api/v1/graph
GET /api/v1/document?path=<logical-path-or-/okf-docs-path>
```

These endpoints open the configured OKF document roots through the `okf` crate
and return parsed metadata, diagnostics, graph nodes/edges, and document source
without granting general filesystem access.

All JSON responses use an `api_version: "v1"` envelope, return stable error
codes, and carry a request ID. The compatibility aliases below `/api/okf/`
are deprecated. The complete schema and compatibility policy are documented
in [OKF HTTP API v1](../okf/docs/knowledge/integration/http-api-v1.md).

Voyage AI planning is exposed through non-token-spending endpoints:

```text
GET  /api/v1/voyage/status
POST /api/v1/voyage/plan
POST /api/v1/voyage/rebuild
```

`status` and `plan` report configured model, API-key status, index integrity,
document/chunk counts, estimated tokens, estimated requests, cached/changed
chunks, and TPM/RPM limits. `rebuild` reconstructs SQLite and its file mirror
from current OKF documents and still-valid local embeddings. None of these
actions calls Voyage AI. `rebuild` requires paired editor authority, or the
transitional automation token, because it changes derived state.

Voyage AI execution is exposed only through explicit token-spending `POST`
endpoints:

```text
POST /api/v1/voyage/check
POST /api/v1/voyage/index
POST /api/v1/voyage/search
```

`check` performs a tiny connectivity embedding request. `index` embeds changed
chunks and saves the local `.okf-voyage` index. `search` embeds the provided
query and searches the local index. These endpoints are never called during
normal browsing.

All three endpoints require paired editor authority or the transitional
automation token. The browser asks for confirmation before each provider-cost
operation.

`index` uses the shared OKF Voyage rate-limit executor. Batches are checked
against configured TPM/RPM limits before each provider request; oversized
batches fail before spending tokens, and a provider `429` is retried once after
waiting for a fresh minute window.

`OKF_VOYAGE_TIMEOUT_SECONDS` bounds both connection establishment and the
complete exchange of each individual Voyage request. Transport and provider
failures carry distinct safe report codes. Embedding count and dimensions are
validated before either local index backend is changed, so failed or partial
batches are not persisted. A second index request for the same resolved index
root is rejected with HTTP 409 while the first is active.

Index jobs accepted by v1 run to an atomic provider outcome; there is no
mid-job cancellation endpoint. Disconnecting the browser does not authorize a
partial commit. Graceful server shutdown waits for an in-flight index job,
whose provider calls remain bounded by the configured timeout.

The local working/index database is SQLite at:

```text
.okf-voyage/okf.sqlite
```

It stores derived document inventory, chunks, embeddings, provider/model
metadata, vector dimensions, suggestions, review state, and application audit
rows. Canonical OKF Markdown remains the source of truth. Within derived
state, SQLite is authoritative; `embeddings.tsv` and `suggestions.tsv` are an
atomically replaced compatibility mirror identified by
`file-index-generation`.

An index operation commits inventory, chunks, embeddings, valid suggestions,
and a new generation in one SQLite transaction before replacing the file
mirror. A failed transaction changes neither backend. If SQLite commits but a
mirror replacement is interrupted, `GET /api/v1/voyage/status` reports
`recovery_required` instead of silently accepting the disagreement, and
`POST /api/v1/voyage/rebuild` repairs it from SQLite without provider calls.
Stale embeddings and suggestions are removed when chunks disappear or their
content hashes change.

Connections use WAL mode, integrity triggers, explicit integrity checks, and
a five-second busy timeout. Schema migrations are incremental; the current
schema version is 4 and upgrades released versions 1, 2, and 3 in place.
Vector search currently stores vectors as JSON and computes cosine similarity
in Rust, so no SQLite vector extension is required.

Do not commit `.okf-voyage/` or rely on it as the canonical OKF source. It is a
local working database for indexing, semantic search, candidate edges, review
state, and audit rows.

## Suggestion Review API

Voyage similarity suggestions are generated from existing embeddings without
another provider call:

```text
POST /api/v1/suggestions/generate
GET  /api/v1/suggestions?review_set=<id>
POST /api/v1/suggestions/<id>/accept
POST /api/v1/suggestions/<id>/deny
POST /api/v1/review-sets/<id>/accept-all
POST /api/v1/review-sets/<id>/deny-all
POST /api/v1/suggestions/import
```

All endpoints require paired editor authority or the transitional automation
token. Generated review sets and every decision survive restart in SQLite and
the generation-marked mirror. Provider,
model, method, AI marker, chunks, score, timestamp, and status are returned for
every row. Import validates an artifact against existing suggestions and can
only mark exact matches accepted; it cannot manufacture provenance.

The packaged browser drives planning, explicit indexing, suggestion generation,
semantic search, and review through these APIs. Its cookie is `HttpOnly`; its
CSRF value stays only in memory and is recovered through a protected
same-origin POST after reload. Local metadata prefilters are labeled as
non-Voyage and are not reviewable. SQLite, not browser storage, is the durable
review source.

Accepted AI-derived edge suggestions can be applied explicitly:

```text
POST /api/v1/edges/apply
```

This endpoint also requires paired editor authority or the transitional
automation token.

The endpoint reads accepted suggestions from SQLite review state, writes
structured `relations: [...]` frontmatter metadata to the source OKF document,
and records an audit row in SQLite. Source and target documents/chunks are
revalidated immediately before an atomic, permission-preserving write. A retry
repairs a missing audit row without duplicating an already written relation.
The browser exposes this as the separately confirmed “Apply accepted
relations” action and shows canonical relations from `/api/v1/graph` with AI
provenance. Markdown body links are not generated by this canonical write path,
and raw vectors remain in SQLite.

The full implementation plan lives here:

[`../okf/docs/knowledge/plans/okf-http-server-plan.md`](../okf/docs/knowledge/plans/okf-http-server-plan.md)

The security boundary and residual risks are documented in the
[OKF HTTP threat model](../okf/docs/knowledge/security/http-threat-model.md).

## Source Layout

The server is separated by responsibility:

- `cli.rs`: host and session-token policy helpers
- `routing.rs`: v1 and temporary compatibility routes
- `security.rs`: session authorization, response headers, request IDs, logs
- `tls.rs`: certificate/key validation and Rustls server configuration
- `static_files.rs`: canonical static/document path resolution
- `api.rs`: stable v1 success/error envelopes and error codes
- `storage.rs`: read-only SQLite status inspection
- `voyage.rs`: provider-facing API policy text
- `relations.rs`: canonical relation/frontmatter handling
- `lib.rs`: endpoint orchestration and transactional SQLite index management
