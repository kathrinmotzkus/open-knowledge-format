---
title: OKF HTTP Threat Model
type: Reference
kind: knowledge-reference
topic: okf-security
status: active
updated: 2026-07-01
tags: [okf, okf-http, security, threat-model, authentication, tls]
---

# OKF HTTP Threat Model

`okf-http` is a local knowledge browser with APIs that can read user-selected
documents, spend provider tokens, modify derived databases, and write canonical
OKF metadata. Its default network boundary is loopback, but loopback is shared
by the machine and is not an operating-system sandbox.

This document separates the current 0.2 implementation from the target access
model in the
[Local Access, Authentication, and TLS Plan](../plans/local-access-authentication-and-tls-plan.md).
The route capability matrix, read-only/local-editor enforcement, ephemeral
local pairing, browser pairing UX, Rust-native TLS, guided local certificates,
the persistent credential CLI, HTTPS browser login, and capability-bound
persistent sessions are implemented now.

## Protected Assets

- Browser assets under the configured browser root
- Markdown documents and declared resources under configured OKF roots
- Physical root paths and local directory structure
- `.env`, including document roots and Voyage credentials
- Browser-managed configuration under the XDG configuration directory
- The SQLite working index and its review/audit state
- Pairing secrets and local editor sessions
- Persistent users, password hashes, roles, and security audit state
- TLS CA and server private keys
- Voyage API quota and returned embeddings
- Canonical OKF frontmatter modified by accepted relations

## Trust Boundaries

### Anonymous Reader

An anonymous reader may receive browser assets, admitted OKF content, graph and
document metadata, health status, and token-free local results. Anonymous
reading is a product requirement, not a temporary exception.

Anonymous authority never includes configuration writes, canonical writes,
review decisions, derived-state mutation, provider calls, physical path
disclosure, user management, or security management.

### Local Paired Editor

A paired editor is an explicitly authorized browser session on a loopback-only
local-editor server. Pairing is ephemeral and does not prove the operating
system identity of the person at the browser. It raises the HTTP application
boundary but does not protect against a compromised account or privileged local
process.

### Persistent HTTPS User

A persistent user authenticates only through HTTPS and receives explicit
capabilities. TLS protects transport and server identity only when the client
actually trusts the intended certificate or CA. A bypassed certificate warning
does not provide the same identity assurance.

### Host Operator

The operator controls process arguments, environment values, `.env`, bind
addresses, certificates, filesystem permissions, and reverse-proxy policy.
Operator overrides can intentionally expose additional information and must
never be mistaken for safe browser defaults.

## Route Access Classes

Every productive Axum route is registered through the classified route registry
in `crates/okf-http/src/routing.rs`. A route contract contains its HTTP method,
path, access class, allowed future modes, required capabilities, and legacy
status. Registration without a class is not available through that registry.

| Access class | Meaning | Target modes |
|---|---|---|
| Anonymous read | No state change and no provider cost | read-only, local editor, authenticated TLS |
| Authentication bootstrap | Establishes or recovers local or persistent session proof | local editor or authenticated TLS, route-dependent |
| Authenticated sensitive read | Reads physical/configuration/review state | local editor, authenticated TLS |
| Local editor mutation | Changes host configuration | local editor, authenticated TLS |
| Canonical write | Changes canonical OKF source | local editor, authenticated TLS |
| Derived-state mutation | Changes rebuildable SQLite/index/review state | local editor, authenticated TLS |
| Token spending | Can call a paid provider | local editor, authenticated TLS with `voyage.spend` |
| Security administration | Changes users, roles, sessions, CA, or TLS policy | authenticated TLS only |

Security-administration routes are TLS-only and require `users.manage`,
`security.manage`, or a narrowly scoped self-service capability. They are not
available in read-only or plain-HTTP modes.

## Current Route Inventory

The `/api/okf/...` rows below are deprecated aliases for the corresponding
`/api/v1/...` routes and have the same classification.

| Method and route | Access class | Capabilities |
|---|---|---|
| `GET /`, `/docs-browser[/...]` | Anonymous read | `content.read` |
| `GET /okf-docs/{mount}/...`, `/okf-root/{index}/...` | Anonymous read | `content.read` |
| `GET /api/v1/documents`, `/document`, `/graph` | Anonymous read | `content.read` |
| `GET /api/v1/voyage/status` | Anonymous read | `content.read` |
| `POST /api/v1/voyage/plan` | Anonymous read | `content.read` |
| `GET /api/v1/health`, `/health` | Anonymous read | `content.read` |
| `GET /api/v1/access/session` | Anonymous read | `content.read` |
| `POST /api/v1/access/pair` | Authentication bootstrap | `session.pair` |
| `POST /api/v1/access/login` | Authentication bootstrap | `session.login` |
| `POST /api/v1/access/session/refresh` | Authentication bootstrap | `session.recover` |
| `POST /api/v1/access/logout` | Local editor mutation | `session.logout` |
| `POST /api/v1/access/password` | Security administration | `password.change` |
| `POST /api/v1/access/sessions/revoke` | Security administration | `session.logout` |
| `GET /api/v1/access/users` | Security administration | `users.manage` |
| `POST /api/v1/access/sessions/revoke-user` | Security administration | `security.manage` |
| `GET /api/v1/config/roots` | Authenticated sensitive read | `roots.propose` |
| `GET /api/v1/roots/configuration`, `/roots/proposals/{id}` | Authenticated sensitive read | `roots.propose` |
| `POST /api/v1/roots/proposals` | Authenticated sensitive read | `roots.propose` |
| `POST /api/v1/roots/proposals/{id}/initialization/plan` | Authenticated sensitive read | `content.initialize` |
| `GET /api/v1/suggestions` | Authenticated sensitive read | `review.decide` |
| `POST /api/v1/roots/proposals/{id}/registration` | Local editor mutation | `roots.configure` |
| `PUT`, `DELETE /api/v1/roots/{root-id}` | Local editor mutation | `roots.configure` |
| `POST /api/v1/roots/proposals/{id}/initialization` | Local editor source mutation | `content.initialize` |
| Legacy `POST`, `PUT`, `DELETE /api/v1/config/roots...` | Retired (`410 Gone`) | `roots.configure` gate before retirement response |
| `POST /api/v1/voyage/rebuild` | Derived-state mutation | `derived.rebuild` |
| `POST /api/v1/suggestions/generate`, `/import` | Derived-state mutation | `review.decide` |
| `POST /api/v1/suggestions/{id}/accept`, `/deny` | Derived-state mutation | `review.decide` |
| `POST /api/v1/review-sets/{id}/accept-all`, `/deny-all` | Derived-state mutation | `review.decide` |
| `POST /api/v1/voyage/check`, `/search` | Token spending | `voyage.spend` |
| `POST /api/v1/voyage/index` | Token spending and derived output | `voyage.spend`, `derived.rebuild` |
| `POST /api/v1/edges/apply` | Canonical write | `content.write` |

When `--scanlab-compat` is enabled, `GET /docs/...`, the two legacy crate
knowledge routes, and the small `/repo-files/...` allowlist are classified as
legacy anonymous reads. They are absent otherwise.

The `--expose-physical-paths` debug option can add physical paths to metadata
responses and therefore requires explicit local-editor mode.

## Main Threats and Required Controls

### Other Local Operating-System Users

Any local user or process may attempt to connect to a TCP loopback listener.
Read access is intentionally public locally, but mutation requires pairing or
authentication. Private runtime and security files use owner-only permissions.
This does not prevent a privileged user from inspecting another process.

### Malicious Local Processes

A process running as the same user may read accessible files, inspect process
state where the operating system permits it, automate browser input, or race
configuration changes. Session protection is not a defense against full local
account compromise. File permissions, process isolation, short-lived sessions,
revision checks, and explicit confirmations limit accidental and lower-privilege
abuse.

### Malicious Websites Targeting Loopback

Remote websites can cause browsers to send some requests to loopback even
though the same-origin policy limits reading responses. Sensitive routes require
non-simple requests, exact Origin and Host validation, Fetch Metadata checks,
CSRF protection, and explicit authority. GET routes must remain free of side
effects.

### DNS Rebinding and Host Confusion

The server validates HTTP Host and request Origin against its configured
origin. A hostname resolving to loopback is not automatically trusted.
Non-loopback binding remains explicit. Forwarding headers are rejected outside
trusted-proxy mode. In trusted-proxy mode the loopback HTTPS backend requires a
separate proxy token, exact public HTTPS origin, one forwarded host/protocol
value, and rejects direct backend access, downgrade, `Forwarded`, and ambiguous
client-address chains.

### Browser Extensions and Same-Origin XSS

An extension or same-origin script may act with the browser's authority.
Markdown is untrusted display content: generated HTML is escaped, dangerous URL
schemes remain inert, raw active content is rejected, and a restrictive CSP
limits script execution. Session material is never placed in browser storage or
rendered content. TLS cannot repair same-origin XSS.

### Pairing Guessing, Disclosure, and Replay

Pairing codes require cryptographic randomness, short expiry, strict attempt
limits, one-time exchange, and server-side session rotation. Codes never enter
URLs or logs. A successful pairing remains an application capability, not proof
of physical presence or operating-system identity.

### Session Fixation, Theft, and CSRF

Sessions are generated by the server, rotated after authority changes, bound to
CSRF defenses, short-lived, revocable, and invalidated on local-editor restart.
Persistent login uses HTTPS-only cookies. Logout, account disablement, and role
changes revoke authority according to the access plan.

### Certificate Substitution and Private-Key Exposure

HTTPS authenticates the intended server only when the client validates the
intended certificate chain. The current explicit private-key path must be
owner-only on Unix, never crosses an API boundary, and is validated before
startup. The guided Phase 6 setup places generated keys in owner-only XDG
state. CA installation remains an explicit user action and may be prohibited
by browser or enterprise policy. Browser warning bypasses, loss of CA state,
and untrusted replacement certificates remain documented residual risks.

### Accidental Non-Loopback Binding

Read-only and local-editor modes bind only to loopback. Local editor mode
refuses non-loopback binding. For intranet and Internet deployments, `okf-http`
remains a loopback backend behind a reverse proxy. HTTPS never turns the OKF
process itself into the public network boundary.

### Remote Anonymous Reads and Root Administration

Local read-only, local-editor, and authenticated local TLS operation retain
anonymous document reading. Trusted-proxy deployments default to authenticated
document/graph/API reads while keeping only browser assets, health, session
status, and login bootstrap public. Remote document-root inspection,
initialization, and mutation are denied even to authenticated editors until the
separate root-onboarding policy explicitly authorizes them.

### Path and Content Escape

Browser assets and OKF documents are served from separate canonical roots.
Plain, URL-encoded, double-encoded, absolute, and symlink traversal attempts
cannot escape either root. Document routes re-run bounded admission and
compliance checks for every request. They resolve only exact admitted Markdown
or structurally valid CSV declared by the owning directory's `index.md`.
Hidden, unsupported, executable, non-regular, symlinked, colliding,
undeclared, malformed, or incomplete-scan content receives no successful
response. Browser assets remain outside this scanner on their packaged root.

### Browser-Managed Root Proposals

A browser request must never become a general filesystem browser. Physical
root paths, rejected entries, ownership details, and diagnostics may disclose
usernames, project names, or confidential directory structure. Root proposal
and preview APIs therefore require `roots.propose`, return only the minimum
review information, and remain unavailable to anonymous and remote clients.
The browser cannot write `.env`; persistent browser configuration is isolated
in the owner-private, versioned XDG `okf/config.toml` file.

The HTTP implementation exposes proposal details only through versioned local
routes guarded by `roots.propose`. The transitional automation token is not an
accepted root-management authority. Network-facing trusted-proxy deployments
reject proposal, configuration, and initialization capabilities before handler
execution. Mutations additionally bind paired or persistent sessions to CSRF,
same-origin, exact proposal/plan digests, explicit confirmation headers, and
configuration revisions. Audit records omit physical paths and document
contents.

### Source Mutation and Confirmation Scope

Registering a root, generating missing metadata, creating `index.md`, and
rewriting source documents are distinct operations. Authentication alone is
not consent to alter source files. Every source mutation requires an explicit
operation-specific confirmation and must preserve permissions and unknown
metadata or fail atomically. Read-only registration remains possible where the
source is not writable or the user declines initialization.

### Stale Preview and Filesystem Races

An attacker or ordinary editor may change a tree after preview but before
confirmation. Every proposal is therefore bound to an exact inventory snapshot
and expires. Confirmation must revalidate identity, metadata, hashes, file
kinds, symlink status, and collision state. A mismatch invalidates the complete
proposal; OKF must not silently apply the unchanged subset.

### Monitored Changes and Baseline Races

Monitoring is opt-in and cannot grant standing write authority. New
registrations seed their baseline from the already revalidated proposal rather
than an unreviewed later restart scan. Reviewed initialization advances it to
the exact post-write inventory. Subsequent startup detection is read-only and
token-free; browser refresh never starts a scan.

Pending added or content-modified paths fail the accepted-baseline hash gate
even before a review decision. Acceptance rescans and requires the exact digest
before one SQLite transaction advances the baseline and removes the pending
row. Dismissal removes only that notice. Detection and review cannot alter
configuration, source, relations, embeddings, Voyage state, or `.env`.

### Oversized and Adversarial Trees

Deep trees, very large files, huge entry counts, sparse files, special files,
and repeated filesystem errors can consume memory, CPU, descriptors, or review
attention. Scanning has operator-owned limits for per-file bytes, inspected
entries, nesting depth, and aggregate candidate bytes. Limit exhaustion is a
closed failure: the proposal is incomplete and cannot be registered or used
for initialization. Browser input and document metadata cannot raise limits.

### Malicious Document and Resource Content

Admission does not make content trustworthy. Markdown remains escaped and
rendered under the restrictive CSP; raw active content, unsafe URL schemes,
and document-root JavaScript remain inert or rejected. CSV is data, never
script. Content type is derived from the admitted resource declaration rather
than a user-controlled response header. Hidden paths, symlinks, non-regular
files, unsupported formats, and undeclared resources are excluded from the
servable inventory.

The normative terminology, compatibility boundary, defaults, and scan failure
behavior are defined in the
[document root onboarding baseline](../concepts/root-onboarding-baseline.md).

### Provider Cost and Canonical Writes

Voyage credentials remain server-side and never enter browser assets, API
responses, logs, Markdown, or SQLite. Provider calls require `voyage.spend` and
an operation-specific confirmation. Canonical writes require `content.write`,
an exact reviewed proposal, atomic source handling, and retained provenance.

## Existing Response Controls

Every response receives a restrictive Content Security Policy, clickjacking,
MIME-sniffing, referrer, and browser-permission headers. API responses use
`Cache-Control: no-store`. Deprecated API aliases carry deprecation metadata.

The current 0.2 local-editor mutation boundary supports ephemeral paired
sessions with server-generated identifiers, `HttpOnly`/`SameSite=Strict`
cookies, a separate CSRF header, exact Host/Origin checks, Fetch Metadata,
expiry, logout, restart invalidation, and operation-specific confirmations.
The pairing code is one-time, five-minute state with bounded attempts and
cooldown. The browser keeps the CSRF value only in memory and can recover it
after reload only through a protected same-origin POST. The transitional
`X-OKF-Session-Token` is no longer used by the browser and remains only for the
documented automation migration period. The default read-only router mounts
neither pairing nor sensitive routes and creates no token file.

## Residual Risk

- Loopback is reachable by other local principals and is not an OS sandbox.
- Anonymous local reading means configured admitted content is intentionally
  visible to local HTTP clients.
- A compromised user account or privileged process can bypass application
  boundaries by reading files directly.
- TLS without certificate validation encrypts traffic but does not reliably
  authenticate the intended server.
- A trusted browser extension can exceed the protections available to ordinary
  web content.
- Debug and operator overrides can deliberately weaken privacy or deployment
  defaults and must be used knowingly.

## Reporting

Report suspected vulnerabilities privately according to
[`../../../SECURITY.md`](../../../SECURITY.md). Never include real API keys,
pairing codes, password hashes, session tokens, private keys, private document
contents, or local filesystem paths in a public issue.
