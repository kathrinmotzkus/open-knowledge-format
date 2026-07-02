---
title: OKF Local Access, Authentication, and TLS Plan
type: Plan
kind: knowledge-plan
topic: okf-access-security
status: completed
updated: 2026-07-01
tags: [okf, okf-http, access, authentication, authorization, pairing, tls, ca]
---

# OKF Local Access, Authentication, and TLS Plan

## Purpose

This plan defines a low-friction security model for `okf-http`. Reading local
OKF knowledge remains possible without authentication. Mutating configuration,
canonical files, derived state, or paid provider state requires an explicit
local pairing session or an authenticated account with the required
capability.

This plan must be implemented before the
[Secure OKF Document Root Onboarding Plan](secure-document-root-onboarding-plan.md).
Root onboarding consumes the access and authorization contracts established
here instead of inventing a separate login mechanism.

## Design Principle

Security requirements increase with capability:

1. Reading is simple and anonymous on a loopback-only server.
2. Local editing uses an ephemeral pairing session without passwords, TLS
   setup, root privileges, or external system tools.
3. Persistent accounts require HTTPS.
4. Remote operation requires explicit authenticated TLS deployment and is not
   enabled by local setup accidentally.

An inexperienced Markdown author must be able to install `okf-http`, start it,
and read documents in a browser without configuring certificates, users, or
operating-system trust stores.

## Accepted Decisions

- Anonymous read access remains available for ordinary document browsing,
  navigation, graph viewing, and token-free local search.
- Anonymous access never permits configuration changes, canonical writes,
  source initialization, review decisions, derived-state mutation, user
  management, or Voyage API calls.
- The default server binds only to `127.0.0.1` and starts in read-only mode.
- Local editing is an explicit startup mode and uses a short-lived one-time
  pairing code.
- Pairing codes are entered into the browser body, never placed in a URL,
  query string, fragment, log, or persistent browser storage.
- Local pairing does not create a persistent user or password.
- Pairing sessions expire, are server-side, and are invalidated on restart.
- Persistent password authentication is unavailable over plain HTTP, including
  loopback HTTP.
- HTTPS and a local OKF CA are optional for reading and local pairing.
- TLS keys, user credentials, and sessions are not stored in `.env`,
  `config.toml`, canonical OKF documents, browser storage, or Voyage SQLite.
- TLS certificate generation uses Rust code and must not require `openssl` or
  another external command.
- Installing a CA into an operating-system or browser trust store is always an
  explicit user action. `okf-http` never silently requests root privileges.
- Existing Origin, Host, Fetch Metadata, CSRF, CSP, and path protections remain
  mandatory with or without HTTPS.
- Authentication answers who may act; previews and confirmations still define
  exactly what may change.

## Access Modes

### Read-Only Mode

Default command shape:

```text
okf-http 8003
```

Properties:

- loopback-only HTTP is allowed
- no login is required for read endpoints
- all mutation and token-spending routes are disabled, not merely hidden
- no pairing code, account database, certificate, or trust-store modification
  is required

### Local Editor Mode

Proposed command shape:

```text
okf-http --local-editor 8003
```

Properties:

- loopback-only HTTP is allowed
- anonymous read access remains available
- the terminal displays a one-time pairing code
- successful pairing creates an ephemeral editor session
- mutation and paid actions still require their operation-specific previews and
  confirmations
- server restart invalidates the session and creates a new pairing boundary

### Authenticated TLS Mode

Proposed command shape:

```text
okf-http --authenticated 8003
```

Properties:

- valid TLS certificate and private key are mandatory
- anonymous read access remains available by default
- persistent users can log in and receive explicit capabilities
- remote binding remains a separate explicit action with stricter checks

## Authorization Capabilities

Use capabilities rather than hard-coding route checks around role names:

- `content.read`
- `roots.propose`
- `roots.configure`
- `content.initialize`
- `content.write`
- `review.decide`
- `derived.rebuild`
- `voyage.spend`
- `users.manage`
- `security.manage`

Initial role mappings may include:

- `editor`: proposal, root, canonical content, review, and derived-state
  capabilities
- `voyage`: explicit paid-provider capability
- `admin`: user and security management plus editor capabilities

Anonymous clients receive only `content.read`. A local pairing session receives
the documented local-editor capability set. Persistent users receive the union
of their assigned roles. Paid Voyage access is never implied by read access.

## Persistent Security State

Persistent authentication and TLS state is separate from canonical and derived
knowledge:

```text
~/.local/state/okf/
  auth.sqlite
  tls/
    ca-cert.pem
    ca-key.pem
    server-cert.pem
    server-key.pem
```

Respect `XDG_STATE_HOME`; use `~/.local/state/okf` as the Linux fallback.

- directories use mode `0700`
- private keys and credential databases use mode `0600`
- authentication schema migrations are incremental and tested
- `auth.sqlite` is persistent security state and is never deleted by Voyage or
  derived-index rebuilds
- secrets are never returned by an API or included in packages

## Non-Goals

- Do not require authentication merely to read local OKF documents.
- Do not require HTTPS for the default read-only or ephemeral local-pairing
  modes.
- Do not automatically install a CA into a system or browser trust store.
- Do not build a general identity provider or enterprise SSO implementation in
  the first release.
- Do not expose password or CA-private-key management through anonymous browser
  routes.
- Do not place passwords on command lines, where they enter shell history or
  process listings.
- Do not treat TLS as a replacement for authorization, CSRF protection, safe
  rendering, or explicit mutation confirmation.

## Phase 1: Threat Model and Route Capability Matrix

- Inventory every existing HTTP route and classify it as:
  - anonymous read
  - authenticated read of sensitive physical state
  - local editor mutation
  - canonical write
  - derived-state mutation
  - token-spending action
  - security administration
- Define which routes exist in each access mode.
- Extend the threat model for:
  - other local operating-system users
  - malicious local processes
  - malicious websites targeting loopback
  - browser extensions and same-origin XSS
  - DNS rebinding and Host-header attacks
  - pairing-code guessing and replay
  - session fixation, theft, and CSRF
  - certificate substitution and private-key exposure
  - accidental non-loopback binding
- Document residual risk: loopback application security is not an
  operating-system sandbox.
- Add tests that enumerate all routes and fail when a new sensitive route has
  no declared capability.

Completion criteria:

- Every route has one explicit access classification.
- Anonymous reading is clearly separated from every mutation and paid action.
- Adding an unclassified route fails tests or CI.
- Threats and residual risks are documented without overstating loopback or TLS
  protection.

Status: implemented. All productive Axum routes, including deprecated API
aliases and opt-in scanlab compatibility routes, are registered through one
classified route registry. Each contract records method, path, access class,
target modes, required capabilities, and legacy status. Runtime validation and
unit tests reject duplicate routes, sensitive read-only exposure,
token-spending routes without `voyage.spend`, canonical writes without
`content.write`, and non-TLS security administration. The expanded HTTP threat
model inventories the route groups and covers local principals, loopback web
attacks, rebinding, XSS, pairing, sessions, TLS identity, key protection, and
remote-binding mistakes. Mode enforcement remains Phase 2 work.

## Phase 2: Explicit Read-Only and Local-Editor Modes

- Make read-only the default operation mode.
- Add an explicit local-editor startup flag or subcommand.
- Do not mount mutation or token-spending routes in read-only mode, or return a
  stable mode-disabled response before request bodies are processed.
- Preserve anonymous document, graph, metadata, health, and token-free local
  search endpoints.
- Keep physical filesystem paths private in anonymous responses.
- Refuse local-editor mode on a non-loopback bind.
- Report the active mode clearly at startup and through a non-sensitive status
  endpoint.
- Preserve compatibility for automation through a documented migration period;
  do not silently grant old startup tokens broader rights.

Completion criteria:

- A default server supports complete read-only browser use without login.
- No request can mutate state or spend tokens in read-only mode.
- Local-editor mode cannot bind remotely.
- Route-matrix tests cover both modes.

Status: implemented. `okf-http` now starts in `read-only` mode unless
`--local-editor` is explicit. The classified route registry mounts only
anonymous read contracts in the default mode; valid old session tokens cannot
restore omitted sensitive routes. Local-editor mode is restricted to loopback,
retains the transitional session-token boundary, and is the only mode that can
enable physical-path debugging. The health API and startup event expose the
active mode, the read-only process creates no token file, and the packaged
browser disables protected controls while leaving document and graph reading
available. Ephemeral pairing is implemented in Phase 3 below.

## Phase 3: Ephemeral Pairing and Local Sessions

- Generate a cryptographically random one-time pairing secret at local-editor
  startup.
- Present a human-enterable code with sufficient entropy, a short expiration,
  strict attempt limits, and one-time use.
- Never accept the code from a URL, query parameter, fragment, cookie, or GET
  request.
- Exchange a valid code for a random server-side session.
- Store only a verifier or safely bounded pairing state while pairing is
  pending.
- Rotate session identifiers after pairing and invalidate them at logout,
  expiration, or restart.
- Use `HttpOnly` and `SameSite=Strict` cookies plus a separate CSRF token or
  equivalent bound custom-header mechanism.
- Validate exact Origin and Host values and reject cross-site Fetch Metadata.
- Rate-limit failed pairing attempts without creating an unlimited local denial
  of service.
- Keep pairing and sessions entirely independent from persistent user storage.

Completion criteria:

- A code can be used once and cannot be replayed.
- Guessing, expired, malformed, cross-origin, and fixed-session attempts fail.
- Pairing secrets and session identifiers never appear in logs or URLs.
- Restart and logout invalidate local editor authority.

Status: implemented. Local-editor startup now generates an unbiased random
12-digit pairing code held only as bounded in-memory state. The code expires
after five minutes, is accepted only by the local-editor-only JSON POST
endpoint, is throttled after failed attempts, and is consumed on success. The
server creates a random 30-minute in-memory session, sends its identifier only
as an `HttpOnly`, `SameSite=Strict` cookie, and returns a separate CSRF token.
Protected paired requests require the cookie, CSRF header, exact listener Host
and Origin, and non-cross-site Fetch Metadata. Logout, expiry, and restart
invalidate authority. Router tests cover the authentication-bootstrap class;
HTTP tests cover malformed and cross-origin input, fixation, replay, missing
CSRF, successful authority, logout, and restart invalidation. The old startup
token remains a temporary parallel compatibility path for existing automation;
the browser migration is completed in Phase 4.

## Phase 4: Browser Pairing and Session UX

- Keep read-only navigation immediately available without a login wall.
- Show editing controls only when local-editor mode is available.
- Add a pairing dialog that submits the code in a protected POST body.
- Show session scope and expiration without exposing tokens.
- Add explicit logout and session-expired behavior.
- Keep credentials and session material out of `localStorage`,
  `sessionStorage`, URLs, exported files, and document content.
- Require re-confirmation for destructive, canonical, and token-spending
  operations even during a valid editor session.
- Ensure malicious rendered Markdown cannot invoke paired actions.

Completion criteria:

- Anonymous users can continue reading while another browser session is paired.
- Pairing, expiration, and logout are understandable and keyboard-accessible.
- Browser security tests prove that session material is not persisted or
  rendered.

Status: implemented. Anonymous document and graph browsing still starts
immediately without a login wall. In local-editor mode, the browser exposes a
native, labeled pairing dialog that accepts and clears the one-time code,
displays only session scope and remaining lifetime, and provides explicit
logout and expiration behavior. The `HttpOnly` cookie remains inaccessible to
JavaScript; the CSRF value stays only in memory and is recovered after reload
through a Host-, Origin-, Fetch-Metadata-, and cookie-protected POST. No
credential enters URLs, DOM text, browser storage, exports, or document
content. Protected controls remain disabled until pairing, while provider-cost,
bulk-review, and canonical-write actions retain separate confirmations.
Rendered Markdown can link only to admitted document routes or safe external
schemes and cannot target access or mutation APIs. Browser security tests cover
pairing-code handling, protected-route rejection, absence of legacy token and
storage use, and accessible dialog structure.

## Phase 5: Rust-Native TLS Foundation

- Add a Rust-native TLS implementation without external `openssl` commands or
  system-library runtime dependencies.
- Support explicit certificate and private-key paths.
- Validate certificate/key matching, SANs, validity period, file permissions,
  and parse errors before binding the listener.
- Support `localhost`, `127.0.0.1`, and `::1` SANs for local certificates.
- Refuse authenticated mode when TLS is absent, invalid, expired, or insecurely
  permissioned.
- Keep read-only and local-editor HTTP operation available on strict loopback.
- Never fall back silently from requested HTTPS to HTTP.
- Expose only non-secret TLS status information.

Completion criteria:

- HTTPS starts with valid user-provided credentials and fails safely otherwise.
- Persistent login endpoints cannot be mounted on HTTP.
- The default read-only installation gains no external system dependency.
- TLS tests use temporary certificates and do not modify trust stores.

Status: implemented. The workspace now uses Rust 1.85 consistently;
`okf-http` keeps Axum 0.8.9 and uses maintained `axum-server` 0.8 with Rustls
and the ring provider. `--authenticated` requires
explicit PEM certificate and private-key paths. Before binding, startup checks
regular files, owner-only Unix key permissions, PEM/X.509 parsing, current
validity, exact DNS/IP SAN coverage for the configured host, certificate/key
compatibility, and safe TLS protocol defaults. `localhost`, `127.0.0.1`, and
`::1` SANs are supported. Validation failure terminates startup without an HTTP
fallback. Plain HTTP is now loopback-only; remote TLS additionally requires a
concrete address, explicit `--allow-remote`, and explicit transitional
automation authority. Health and startup output expose only SANs, validity,
protocol, and certificate fingerprint. Temporary-certificate tests cover valid
HTTPS, plain-HTTP refusal, malformed and expired certificates, wrong SAN,
mismatched keys, and insecure key permissions without touching trust stores.

## Phase 6: Guided Local CA and Certificate Setup

- Add guided commands such as:

  ```text
  okf-http tls init
  okf-http tls status
  okf-http tls verify
  okf-http tls renew
  ```

- Generate an OKF-local CA and server certificate in Rust.
- Store CA and server private keys under the private XDG state directory.
- Display certificate fingerprints and expiration dates.
- Provide platform- and browser-specific trust instructions, including a
  user-profile path where supported without root privileges.
- Clearly report when enterprise policy prevents certificate installation.
- Never invoke `sudo`, alter a trust store, or request root privileges
  automatically.
- Allow users to skip local CA setup and continue in read-only or local-editor
  mode.
- Permit externally supplied enterprise or personal certificates instead of
  the OKF CA.
- Define renewal and recovery behavior if state is lost or corrupted.

Completion criteria:

- Certificate generation needs no external executable or root privilege.
- A user can stop at every trust-store step without breaking read-only access.
- Renewal preserves the trusted CA when possible and never overwrites external
  certificates.
- Private keys never enter APIs, logs, Git, packages, or browser assets.

Status: implemented. `okf-http tls init`, `status`, `verify`, and `renew`
manage an optional Rust-generated local CA and a server certificate covering
`localhost`, `127.0.0.1`, and `::1`. Managed state lives below
`$XDG_STATE_HOME/okf/tls` or `~/.local/state/okf/tls`; Unix directories and
keys are protected with modes `0700` and `0600`. Status output contains only
paths, SANs, fingerprints, expiration dates, and validity. Initialization
refuses existing or partial state. Verification checks the CA self-signature,
server issuer/signature, SANs, validity, key matches, and permissions. Renewal
stages a new server key/certificate pair with rollback, verifies it against the
existing CA before activation, and never touches external certificates.
Commands never invoke `sudo`, external executables, or trust-store APIs.
Documentation gives optional per-user/browser trust instructions and explicit
recovery behavior; users can always stop and retain loopback read-only or
local-editor HTTP.

## Phase 7: Persistent Users and Credential CLI

- Add CLI commands with an extensible structure:

  ```text
  okf-http user add <name>
  okf-http user passwd <name>
  okf-http user disable <name>
  okf-http user remove <name>
  okf-http user list
  okf-http user grant <name> <role>
  okf-http user revoke <name> <role>
  ```

- Read passwords twice from a protected terminal prompt; never accept them as a
  normal command-line argument.
- Optionally support a carefully documented password-stdin path for automation.
- Store only a memory-hard password hash with a random salt and upgradeable
  parameters.
- Normalize and validate usernames without creating case-confusable duplicate
  accounts.
- Provide CLI-only bootstrap and recovery so browser access is never required
  to repair authentication.
- Add bounded login failure delays and audit events without exposing whether a
  username exists.
- Keep user administration unavailable to ephemeral paired editors unless an
  explicit future policy grants it.

Completion criteria:

- Passwords never appear in process arguments, logs, APIs, or plaintext state.
- Disabled and removed users cannot create new sessions.
- Hash parameters can be upgraded on a successful later login.
- Credential and migration tests pass against temporary private state.

Status: implemented. The extensible `okf-http user` CLI supports `add`,
`passwd`, `disable`, `remove`, `list`, `grant`, and `revoke`. Interactive
password entry is read twice without terminal echo; the explicit
`--password-stdin` automation path reads one line and passwords are never CLI
arguments. ASCII-only lowercase normalization prevents case-only and Unicode
confusable duplicate identities. The initial roles are `editor`, `voyage`, and
`admin`, ready for the capability mapping in Phase 8.

Persistent credentials live in owner-only `$XDG_STATE_HOME/okf/auth.sqlite`
with an incremental SQLite schema. Only Argon2id hashes with random salts and
upgradeable parameters are stored. Successful authentication transparently
upgrades older supported parameters. Unknown, removed, disabled, and
wrong-password attempts all return the same boolean result, perform a password
hash verification, use a bounded minimum failure delay, and record an audit
event without identifying nonexistent accounts. CLI changes are transactional
and audited. Tests cover lifecycle, roles, normalization, plaintext absence,
disablement/removal, migration rejection, delay, and hash upgrades in temporary
private state. Persistent browser login and session issuance remain Phase 8;
ephemeral paired editors have no user-administration API.

## Phase 8: HTTPS Login, Roles, and Authorization

- Add login only to authenticated TLS mode.
- Keep ordinary reading anonymous unless a future explicit private-reading
  deployment mode is designed separately.
- Create short-lived server-side sessions using secure cookies appropriate for
  HTTPS.
- Enforce capabilities in shared middleware used by all sensitive routes.
- Require `voyage.spend` separately from editor rights.
- Require `users.manage` and `security.manage` for administrative operations.
- Preserve operation-specific proposal digests and confirmation headers after
  authorization succeeds.
- Add logout, session revocation, password change, disabled-user, and role-change
  invalidation behavior.
- Audit security-relevant actions without logging passwords, session tokens,
  private paths unnecessarily, or document contents.

Completion criteria:

- Password login is impossible over HTTP.
- Anonymous reading still works in authenticated local mode.
- Every sensitive route enforces the capability matrix centrally.
- Role removal and account disablement revoke existing authority predictably.

Status: implemented. Persistent login is mounted only in authenticated TLS
mode; all document reads remain anonymous. Successful login creates a
short-lived in-memory server session with a `Secure`, `HttpOnly`,
`SameSite=Strict` cookie and an in-memory browser CSRF token. Role mappings are
capability-based: `editor` grants content/root/review/derived actions,
`voyage` independently grants `voyage.spend`, and `admin` adds
`users.manage`/`security.manage` without implicitly granting paid-provider
access.

Every productive route receives its declared contract through shared
middleware before its handler runs. Existing proposal digests and confirmation
headers remain additional operation-specific checks. TLS-only endpoints cover
login, logout, CSRF recovery, self-service password change, self-session
revocation, user listing, and administrative session revocation. Persistent
sessions carry a user authorization revision; password changes, role grants or
revocations, account disablement, and removal invalidate them on the next
request. Security actions are audited without passwords, tokens, document
contents, or unnecessary private paths. Unit, browser, router, and real Rustls
binary tests verify anonymous reading, HTTP login absence, secure cookie flags,
capability separation, revocation, and disabled-user behavior.

## Phase 9: Remote and Enterprise Deployment Boundary

- Keep non-loopback binding an explicit opt-in.
- Require authenticated TLS mode for direct remote binding.
- Support externally managed certificate/key files and deployment behind an
  authenticated TLS reverse proxy with a documented trusted-proxy model.
- Do not treat the local OKF CA as an automatic enterprise trust solution.
- Document how corporate CA policy, browser management, and restricted user
  privileges affect setup.
- Refuse ambiguous forwarded identity, insecure proxy headers, and HTTP
  downgrade.
- Define whether anonymous read access is acceptable remotely; default to the
  safer deployment policy without changing the simple local read contract.
- Keep remote root management disabled until the root-onboarding threat model
  explicitly permits it.

Completion criteria:

- Local defaults cannot accidentally expose the server remotely.
- Direct remote password authentication always uses validated TLS.
- Reverse-proxy trust is explicit and testable.
- Enterprise setup does not require changing canonical OKF documents.

Status: implemented. Direct non-loopback binding still requires a concrete
address, validated authenticated TLS, `--allow-remote`, and explicit startup
authority. A separate trusted-proxy mode binds the OKF backend only to
loopback, retains HTTPS on the proxy-to-OKF hop, requires an exact public HTTPS
origin and a deployment secret from `OKF_HTTP_TRUSTED_PROXY_TOKEN`, and rejects
direct backend requests. Forwarded protocol/host values must be singular and
exact; HTTP downgrade, `Forwarded`, repeated/comma-separated fields, wrong
Origin, and ambiguous client identity are refused. Forwarded addresses never
become authentication identity.

Local modes preserve anonymous reading. Direct remote and trusted-proxy modes
expose only browser/login bootstrap and health anonymously; document, graph,
and content API reads require a persistent account. Root inspection,
initialization, and mutation are denied remotely pending the independent
root-onboarding policy. External personal or enterprise certificates remain
supported, while the OKF local CA is explicitly not presented as enterprise
PKI. Deployment guidance covers managed browser trust, service secret storage,
users without root privileges, and proxy header replacement. No deployment
setting changes canonical Markdown. CLI, router, forwarding, capability, and
real TLS tests make the boundary explicit and testable.

## Phase 10: Tests, Documentation, Migration, and Release Gates

- Add unit, integration, browser, and standalone tests for all access modes.
- Test anonymous read access as a permanent product contract.
- Test that every mutation and Voyage action fails anonymously and in read-only
  mode.
- Test pairing entropy handling, expiry, attempt limits, replay, CSRF, Origin,
  Host, logout, and restart invalidation.
- Test TLS parsing, mismatch, expiration, SANs, permissions, renewal, and
  no-downgrade behavior.
- Test password hashing, user lifecycle, roles, session revocation, database
  migrations, and recovery.
- Ensure tests never install trust roots, need root privileges, or make Voyage
  calls.
- Update installation, getting-started, HTTP API, threat model, root onboarding,
  security policy, changelogs, and release checklist.
- Add CI gates that reject:
  - authenticated HTTP login
  - anonymous mutation routes
  - unclassified routes
  - secrets in responses or packages
  - browser-persisted credentials
  - silent CA installation or HTTP downgrade
- Verify MSRV and supported-platform builds with Rust-native TLS dependencies.

Completion criteria:

- A new user can install and read Markdown without authentication, TLS setup,
  root privileges, or external TLS software.
- A local editor can pair and mutate only through explicit protected actions.
- Persistent accounts work only through HTTPS.
- Full release gates pass without trust-store changes, Voyage credentials, or
  network access beyond local test listeners.

Status: implemented. Unit and router integration tests cover the permanent
anonymous-read contract, route classification, read-only route omission,
one-time pairing, entropy shape, expiry, attempt throttling, replay, CSRF,
Host/Origin checks, logout, and restart invalidation. Persistent HTTPS tests
cover Argon2id credentials, roles and capabilities, user lifecycle, session
revocation, live account changes, schema migration, and recovery behavior.
Rustls tests reject malformed, mismatched, expired, future-dated, wrong-SAN,
and insecurely permissioned credentials, exercise renewal, and prove that no
HTTP downgrade occurs. Trusted-proxy tests reject bypass, ambiguous forwarding,
wrong public authority, and downgraded protocol metadata.

Standalone tests install the embedded browser into temporary user-owned paths
and exercise both anonymous HTTP reading and authenticated HTTPS without
scanlab. Browser tests reject persisted credentials and protected API targets
disguised as Markdown links. The release gate runs locked workspace tests
offline, strict Clippy, formatting, browser security checks, synchronized asset
checks, and package-content inspection. It refuses to run with Voyage or proxy
credentials, never installs trust roots, needs no root privileges or external
TLS tools, and permits only local test listeners. CI repeats these gates on
Linux and tests the supported workspace on Linux and macOS, with standalone
`okf` and `okf-http` checks at Rust 1.85. Installation, getting-started, HTTP
API, threat model, root onboarding, security policy, changelog, and release
checklist now describe the tested boundary.

## Dependency Order

1. Threats and route classification define the boundary.
2. Explicit modes make anonymous reading safe by disabling mutation.
3. Pairing provides dependency-free local editor authority.
4. Browser UX exposes pairing without storing credentials.
5. TLS exists before persistent login is possible.
6. Guided CA setup remains optional and non-privileged.
7. Persistent users are created through a safe CLI.
8. Authorization connects users and sessions to capabilities.
9. Remote deployment receives a separate stricter boundary.
10. Release gates preserve anonymous reading and protected mutation together.

The secure document root onboarding work starts only after this plan's local
read, pairing, session, TLS, and capability contracts are implemented and
tested.
