# OKF Security Policy

## Supported Versions

OKF is currently pre-release software. Security fixes are made on the current
development branch; older snapshots are not supported separately.

## Reporting a Vulnerability

Please report suspected vulnerabilities privately through the repository's
GitHub **Security → Report a vulnerability** function:

<https://github.com/kathrinmotzkus/open-knowledge-format/security/advisories/new>

Do not open a public issue containing exploit details, API keys, session
tokens, private document paths, document contents, or provider responses. If
private advisories are unavailable, contact the maintainer through a private
channel listed on the repository owner's GitHub profile before disclosing
technical details.

Include, where possible:

- affected commit or version
- affected component (`okf`, `okf-http`, browser, or Voyage integration)
- minimal reproduction steps using non-sensitive fixtures
- expected and observed security boundary
- impact and whether network access or authentication is required
- suggested mitigation, if known

Never include a real Voyage API key. Revoke and replace any credential that
may have been exposed.

## Response Process

The maintainer will acknowledge the report privately, reproduce and classify
the issue, prepare a fix and regression test, and coordinate disclosure. No
fixed response-time guarantee is made during the pre-release phase.

## Security Boundary

`okf-http` is loopback-only and read-only by default. Anonymous document and
graph reading is a supported product contract; mutation, canonical writes,
derived-state changes, security administration, and Voyage token spending are
not anonymously available.

Local editing is an explicit loopback-only mode protected by one-time pairing,
an in-memory `HttpOnly` session cookie, CSRF validation, and strict Host and
Origin checks. Persistent accounts and password login are available only in
authenticated HTTPS mode. Direct remote binding additionally requires explicit
remote opt-in. Trusted reverse-proxy deployment keeps TLS on the backend hop,
requires an exact public HTTPS origin and a separate proxy secret, and rejects
ambiguous or downgraded forwarding metadata. Remote document reads require an
account and remote document-root administration remains disabled.

The optional local CA changes only files in the user's private OKF state
directory. OKF never installs a trust root, invokes a system certificate tool,
or requires root privileges. It is not a replacement for organization-managed
PKI.

OKF is not an operating-system sandbox. Markdown and imported paths are
untrusted input. `.env`, `~/.config/okf`, `~/.local/share/okf`,
`.okf-voyage/`, user databases, private keys, session authority, proxy secrets,
and provider credentials are private local state. Do not publish them or place
them in canonical OKF documents.

The detailed model is documented in
[docs/knowledge/security/http-threat-model.md](docs/knowledge/security/http-threat-model.md).
