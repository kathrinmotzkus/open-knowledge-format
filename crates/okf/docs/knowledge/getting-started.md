---
title: Getting Started with Standalone OKF
type: Guide
kind: knowledge-document
topic: okf
status: active
updated: 2026-06-30
tags: [okf, installation, browser, voyage-ai, quickstart]
---

# Getting Started with Standalone OKF

This guide starts the standalone OKF browser from a source checkout. Normal
browsing is local and token-free. Voyage connectivity, indexing, and semantic
search are optional explicit actions that may consume provider tokens.

## Quick Path: Local Read-Only Browsing

If you only want to view Markdown knowledge folders in the browser:

```bash
./install.sh --release
okf-http --local-editor 8003
```

Open the printed browser URL, pair the browser with the one-time code from the
terminal, and add a document root in **Document roots**. Registering a root
stores browser-managed configuration in `~/.config/okf/config.toml` on Linux.
After registration, restart in read-only mode:

```bash
okf-http 8003
```

Reading documents does not require login, does not call Voyage AI, and does
not spend provider tokens.

## 1. Install the Binary and Browser

From the repository root:

```bash
./install.sh --release
```

The installer builds `okf-http`, installs the binary, and provisions the
packaged browser for the current user. On Linux the packaged frontend is
installed in `~/docs-browser`. After this installer path, you normally do not
need to run `okf-http --install-browser` separately.

If you install only the binary manually, for example with
`cargo install --path crates/okf-http`, provision or refresh the browser assets
with:

```bash
okf-http --install-browser
```

The browser installer protects locally modified packaged files; use `--force`
only when those files should be replaced.

## 2. Configure Document Roots

OKF has three configuration layers:

1. Browser-managed roots live in `~/.config/okf/config.toml` on Linux, or under
   `$XDG_CONFIG_HOME/okf/config.toml` when XDG configuration is redirected.
2. Operator roots from a private `.env` or process-level
   `OKF_DOCUMENT_ROOTS` override browser-managed roots.
3. CLI roots from `--root` and `--add-root` apply only to the current process
   and have the highest explicit startup priority.

The browser API creates the owner-private `config.toml` file; it never writes
`.env`.

Operators may continue to configure one or more higher-precedence roots in a
private `.env` using platform path-list syntax. On Linux:

```env
OKF_DOCUMENT_ROOTS=project=/absolute/path/to/project/docs/knowledge:reference=/absolute/path/to/reference/knowledge
```

`project` and `reference` are logical mount names. Mounted roots keep document
identities stable even when physical directory names differ. An unmounted
entry is also supported:

```env
OKF_DOCUMENT_ROOTS=/absolute/path/to/knowledge
```

Do not commit `.env`. A process-level `OKF_DOCUMENT_ROOTS` replaces the `.env`
value, and either operator source replaces browser-managed roots. Repeated
`--root` values have highest priority and replace configured roots sharing
their mount for that process. `--add-root` values are appended as lower-priority
fallbacks. Existing `.env` roots can be deliberately copied into the browser
configuration with `okf-http roots import-env`; this console-only command does
not modify `.env` or source documents. See
[OKF document roots](repository/document-roots.md) for the complete precedence
rules.

Every normal concept document needs non-empty `type` frontmatter. Reserved
navigation/history files named `index.md` or `log.md` are exempt.

To add an arbitrary existing directory through the browser, start
`okf-http --local-editor 8003`, pair the browser, and open **Document roots**.
Enter the path as text; OKF does not expose a filesystem browser. **Preview
root** shows every admitted and rejected entry before registration. Registering
read-only changes only `config.toml`. **Register and initialize OKF** opens a
second review with exact source diffs and requires a separate confirmation.

The optional change-monitoring checkbox stores only the preference in
configuration. A reviewed registration seeds a private SQLite baseline. Later
startup checks are token-free, and **Check now** is the only additional scan
trigger. Pending changes appear under **Details** and remain unavailable until
their unchanged digest is accepted. Dismissal clears the notice without moving
the baseline.

## 3. Start and Browse

```bash
okf-http 8003
```

The server binds to `127.0.0.1` by default and prints two important lines:

```text
read-only mode: mutation and token-spending routes are disabled
serving OKF browser at http://127.0.0.1:8003/docs-browser/index.html
```

Open the printed browser URL. The document tree is generated from the
configured roots; no project-specific catalog edit is needed. Reading does not
require authentication. The browser reports `read-only` and disables protected
actions.

## 4. Optional Voyage Configuration

To enable semantic analysis, add your own Voyage credentials and limits to the
same uncommitted `.env`:

```env
OKF_VOYAGE_API_KEY=replace-with-your-own-key
OKF_VOYAGE_MODEL=voyage-3-large
OKF_VOYAGE_INDEX_ROOT=.okf-voyage
OKF_VOYAGE_BATCH_SIZE=32
OKF_VOYAGE_TIMEOUT_SECONDS=30
OKF_VOYAGE_TPM_LIMIT=3000000
OKF_VOYAGE_RPM_LIMIT=2000
```

The API key stays server-side. `.okf-voyage/` contains rebuildable SQLite and
compatibility-index data and must not be committed.

## 5. Plan, Index, Review, and Apply

Stop the read-only server and restart it explicitly as a local editor:

```bash
okf-http --local-editor 8003
```

This loopback-only mode prints a one-time pairing code, its five-minute
validity, and a private transitional `session-token-file`. A successful API
pairing creates a 30-minute server-side session; logout, expiry, and restart
revoke it. The server removes the transitional token file during graceful
shutdown.

Open **OKF AI**, select **Pair local editor**, and enter the one-time code from
the terminal. The accessible dialog submits it only in a protected POST body.
The browser then displays the local-editor scope and remaining session time
without rendering either the cookie or CSRF value. **Log out** revokes the
session immediately.

1. Select **Run analysis**. Planning itself is token-free.
2. If changed chunks require embeddings, inspect the displayed token/request
   estimate. Continue only if the provider cost is acceptable.
3. Review the durable SQLite suggestion set with **Accept**, **Deny**,
   **Accept all**, or **Deny all**.
4. Accepted-but-unapplied edges appear dotted and are still derived state.
5. Select **Apply accepted relations** and confirm the canonical write.
6. Applied relations are stored in Markdown frontmatter and appear as solid
   canonical graph edges with AI provenance.

Semantic search is also explicit and embeds each submitted query, so every
search may consume Voyage tokens. Normal document and graph browsing does not.

## 6. Inspect or Rebuild Derived State

Status is read-only and token-free:

```bash
curl http://127.0.0.1:8003/api/v1/voyage/status
```

Existing automation clients may still read the transitional startup token path
printed by the server during the migration period:

```bash
TOKEN=$(cat /private/runtime/path/okf-http-....token)
```

A rebuild changes local derived state but does not call Voyage AI:

```bash
curl -X POST \
  -H "X-OKF-Session-Token: $TOKEN" \
  http://127.0.0.1:8003/api/v1/voyage/rebuild
```

Rebuild reconstructs current inventory and SQLite/file-mirror consistency from
canonical Markdown while retaining only hash-matching embeddings. Missing
embeddings remain missing until a later explicitly confirmed index operation.
Deleting `.okf-voyage/` cannot delete canonical Markdown, but recreating lost
embeddings may consume provider tokens.

## 7. Optional Local HTTPS

Generate an OKF-local CA and a certificate for `localhost`, `127.0.0.1`, and
`::1` without OpenSSL or root privileges:

```bash
okf-http tls init
okf-http tls status
okf-http tls verify
```

The files are stored under `$XDG_STATE_HOME/okf/tls`, or
`~/.local/state/okf/tls` when `XDG_STATE_HOME` is unset. Private keys use mode
`0600` and the directory uses mode `0700` on Unix. The command prints the exact
HTTPS start command. `okf-http tls renew` replaces only the local server key
and certificate while retaining the trusted CA.

No command installs trust automatically. In Firefox, open Settings, Privacy &
Security, Certificates, View Certificates, Authorities, and import only
`ca-cert.pem`. On Windows, the same public CA can be imported into the current
user's Trusted Root Certification Authorities store; on macOS, use the login
keychain rather than the system keychain. Chromium-family browser behavior
depends on its platform packaging and enterprise policy. If importing a CA is
blocked, ask the administrator or continue with ordinary loopback HTTP. Never
import `ca-key.pem` or either server key.

If local TLS state is damaged, `status` or `verify` fails without overwriting
it. Move the complete TLS directory aside before running `init` again; a new
CA has a new fingerprint and must be trusted separately. Loss of the CA private
key prevents renewal but does not affect read-only or local-editor HTTP.

### Existing Certificates

If suitable PEM files already exist, start the authenticated TLS foundation
directly:

```bash
okf-http --authenticated \
  --tls-cert ~/.config/okf/tls/server-chain.pem \
  --tls-key ~/.config/okf/tls/server-key.pem \
  8443
```

If you want to use certificates generated by `okf-http tls init`, prefer the
exact paths printed by that command. Managed certificates live under the XDG
state directory, normally `~/.local/state/okf/tls` on Linux. The example above
uses custom paths only to show that externally managed PEM files are also
accepted.

The certificate SAN must cover the configured host. Local certificates may use
`localhost`, `127.0.0.1`, and `::1`. On Unix, protect the key with `chmod 600`.
The server validates the files before binding and never falls back to HTTP.
External personal or enterprise certificates remain independent of the
OKF-managed state and are never changed by `okf-http tls renew`.

### Persistent HTTPS Sign-In

Create an account from the protected local terminal before starting HTTPS:

```bash
okf-http user add alice
okf-http user grant alice editor
okf-http --authenticated \
  --tls-cert ~/.local/state/okf/tls/server-chain.pem \
  --tls-key ~/.local/state/okf/tls/server-key.pem \
  8443
```

Open `https://127.0.0.1:8443/docs-browser/index.html` and use **Sign in**.
Reading never requires an account. Add the independent `voyage` role only when
the account should be able to trigger paid provider requests; `editor` alone
cannot spend Voyage tokens. Password, role, disablement, and removal changes
invalidate existing sessions. CLI administration remains the recovery path if
browser access is unavailable.

For non-loopback or enterprise reverse-proxy deployment, follow the explicit
[remote and enterprise boundary](../../../okf-http/README.md#remote-and-enterprise-deployment).
Remote content reads require login and remote document-root administration is
disabled. The proxy mode keeps HTTPS on both hops and needs a separately stored
proxy secret; do not put that secret in `.env`, Markdown, or browser assets.

## Security Notes

- Keep the default loopback bind unless remote access is genuinely required.
- Use the default read-only mode whenever no mutation or paid action is needed.
- Local editor mode is explicit and cannot bind to a non-loopback host.
- Never place the API key, pairing code, or session material in browser assets,
  Markdown, shell history, logs, or a committed file.
- Create persistent accounts only with `okf-http user add <name>`. Passwords
  are prompted twice without echo and stored only as Argon2id hashes under the
  private XDG state directory. `--password-stdin` is reserved for controlled
  automation and never accepts a password in process arguments.
- Canonical writes and derived-state mutations are mounted only in protected
  modes and require either a paired local-editor session with its CSRF header
  or the transitional automation token.
- Root configuration writes additionally require
  `X-OKF-Config-Write: confirm` and take effect after restart.
- Plain HTTP is loopback-only. Direct remote operation requires explicit
  authenticated TLS mode, a concrete bind address, matching SAN, and explicit
  remote opt-in.

See the [HTTP threat model](security/http-threat-model.md),
[API v1 reference](integration/http-api-v1.md), and
[Voyage analysis decision](integration/voyage-ai-analysis.md) for details.
