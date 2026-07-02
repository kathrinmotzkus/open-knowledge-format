# OKF Browser

`docs-browser/` contains the static frontend files for the OKF knowledge
browser. The browser should be served by `okf-http`, not by Python's
`http.server`, because the browser now depends on OKF-specific API routes.

For an installed `okf-http` binary, provision the packaged assets first:

```bash
okf-http --install-browser
okf-http 8003
```

Re-run `--install-browser` after an upgrade. Packaged files modified by the
user are protected; `--force` explicitly replaces them while preserving
unrelated files in the browser directory.

During repository development:

```bash
cargo run -p okf-http -- --browser-root docs-browser 8003
```

Then open:

```text
http://127.0.0.1:8003/docs-browser/index.html
```

For normal user installations on Linux, the default browser root is:

```text
~/docs-browser
```

Use `--browser-root <path>` or `OKF_BROWSER_ROOT` when the browser files live
somewhere else.

## Document Roots

OKF document roots are configured separately from browser assets.

The default `okf-http` start is read-only and requires no login. The browser
reads `/api/v1/health`, displays the active mode, and disables protected AI,
review, and write controls unless the server was started explicitly with
`--local-editor` or authenticated TLS. Read-only mode does not mount those
sensitive routes. Authenticated TLS offers account sign-in while retaining
anonymous local reading.

- Browser-managed roots live in the private, versioned
  `$XDG_CONFIG_HOME/okf/config.toml` file (or `~/.config/okf/config.toml`).
- `OKF_DOCUMENT_ROOTS` in the process environment or `.env` remains a
  higher-precedence, console-managed operator override.
- Temporary roots can be added with repeated `--root` arguments.
- API root management writes only `config.toml`, through an authenticated,
  explicitly confirmed server action. It never writes `.env`. A restart
  activates the changed roots.

The **Document roots** tab provides path, mount, priority, and opt-in change
checking fields without exposing a filesystem browser. A proposal must be
reviewed before registration. Its complete filterable tree keeps rejected
entries and reasons visible and offers no unsafe override. Read-only
registration and OKF source initialization are separate actions; initialization
shows exact diffs in a second dialog and requires a second confirmation. The
browser does not persist proposal or session authority and does not rescan a
path merely because the page is refreshed.

Opted-in roots display cached change status, **Check now**, and **Details**.
Startup performs one asynchronous token-free scan; later scans are explicit.
Details show every classified change and the complete current admission
inventory. Accepting revalidates the exact digest before atomically moving the
SQLite baseline. Dismissing removes only the notice and leaves the baseline
unchanged. Pending new or modified documents are not served, and detection
never writes source, configuration, relations, embeddings, or Voyage state.

The browser never gains access to arbitrary parent directories. Files outside
the browser root and the configured OKF roots must be exposed through explicit
server routes or API endpoints.

Markdown links are restricted to OKF document routes and safe external
`http`, `https`, or `mailto` targets. Active and local schemes remain inert.

Document routes expose only admitted Markdown and valid CSV resources declared
by their owning `index.md`. CSV cells are parsed locally and inserted only
through HTML escaping. Formula markers produce a safety warning; the browser
never evaluates spreadsheet formulas or document-provided markup.

## Voyage and SQLite

Voyage AI actions are explicit:

- status and planning endpoints do not spend tokens
- connectivity, indexing, and semantic search endpoints may spend tokens

Token-spending and mutating endpoints require a paired local-editor session or
an HTTPS account with the corresponding capability. `voyage` remains separate
from `editor`.
The browser submits the one-time terminal code only to
`POST /api/v1/access/pair`, receives the session identifier as an `HttpOnly`,
`SameSite=Strict` cookie, and keeps the separate CSRF value only in JavaScript
memory. Reloading recovers the CSRF value from the same-origin session-status
endpoint; no credential is placed in a URL, DOM text, `localStorage`, or
`sessionStorage`.

The OKF AI panel plans before indexing, confirms estimated token use, generates
durable Voyage review sets, performs explicitly confirmed semantic searches,
and persists accept/deny decisions through `/api/v1`. Bulk review and canonical
writes require additional confirmation. Local metadata prefilters are visibly
marked as non-Voyage and cannot be accepted.

The browser consumes the stable `/api/v1` namespace. Physical filesystem
paths are absent unless the server was started with the explicit trusted-debug
option `--expose-physical-paths`.

Derived Voyage/AI state is stored in:

```text
.okf-voyage/okf.sqlite
```

That SQLite database is a rebuildable local working/index database. Canonical
OKF source remains Markdown plus frontmatter. Raw vectors stay in SQLite;
accepted relations are applied to OKF files only through an explicit API or
host command. The “Apply accepted relations” button asks for confirmation,
writes structured canonical frontmatter through `okf-http`, and reloads the
graph. Solid green edges are canonical relations; dotted pink edges are
accepted review state that has not yet been applied.

## Troubleshooting

`okf-http` binds to `127.0.0.1` by default. If a local firewall or host policy
blocks loopback traffic, try an explicit host:

```bash
okf-http --host 127.0.0.1 8003
```

Plain HTTP is deliberately loopback-only. Non-loopback binding requires
`--authenticated`, matching certificate SANs, explicit certificate/key paths,
`--allow-remote`, a concrete bind address, and explicit authority. Invalid TLS
material aborts startup without falling back to HTTP.

Direct remote and trusted reverse-proxy deployments require account login for
document reads and disable root management. Trusted-proxy mode also requires
loopback backend HTTPS, an exact public origin, and a separate proxy token; see
the `okf-http` README for the required header-replacement contract.
