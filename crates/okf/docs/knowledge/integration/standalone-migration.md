---
title: Migrating from the scanlab Browser to Standalone OKF
type: Guide
kind: knowledge-document
topic: okf-http
status: active
updated: 2026-07-02
tags: [okf, okf-http, scanlab, migration]
---

# Migrating from the scanlab Browser to Standalone OKF

The standalone browser uses the same canonical Markdown documents. Migration
changes the serving and configuration boundary; it does not require moving or
rewriting existing knowledge files.

## Before Migration

1. Stop the old browser server before changing its executable or browser
   assets.
2. Back up the private `.env` and any `.okf-voyage/` directory you intend to
   preserve.
3. Confirm that every normal concept document has non-empty `type`
   frontmatter. Reserved `index.md` and `log.md` files are exempt.
4. Keep `.env`, API keys, session-token files, SQLite files, and editor lock
   files outside version control.

## Install the Standalone Components

```bash
cargo install --path crates/okf-http
okf-http --install-browser
```

The installed frontend now lives below `~/docs-browser` on Linux instead of
depending on scanlab's repository checkout. Re-run the install command after
upgrades. Use `--force` only to replace modified packaged assets.

## Reuse Existing Knowledge Roots

Keep existing Markdown in place. Existing installations may mount each tree
explicitly in `.env`:

```env
OKF_DOCUMENT_ROOTS=scanlab=/absolute/path/to/scanlab/docs/knowledge:scql=/absolute/path/to/scanlab/crates/scql/docs/knowledge:okf=/absolute/path/to/open-knowledge-format/crates/okf/docs/knowledge
```

The mount names become the first component of logical document identities,
for example `scql/SCQL.md`. Explicit mounts prevent equal relative paths in
independent trees from colliding.

The process environment overrides `.env`, and `.env` overrides browser-managed
XDG configuration. CLI `--root` entries override roots with the same mount for
one run; `--add-root` appends fallbacks. To copy compatible legacy roots into
`~/.config/okf/config.toml`, run `okf-http roots import-env`; the command leaves
`.env` and source documents untouched. Restart the server after persistent root
changes.

Alternatively, pair a local-editor browser and use **Document roots**. Existing
directories are first shown as bounded proposals; registration and source
initialization remain separate confirmations. Enabling change checks seeds the
baseline from that reviewed proposal. Existing version-0 browser configuration
migrates incrementally to the current schema, while `.env`-only installations
remain valid until an operator explicitly imports them.

## Start Without scanlab Compatibility

```bash
okf-http 8003
```

Open the printed `/docs-browser/index.html` URL. The standalone browser obtains
its inventory from `/api/v1/documents` and fetches files through `/okf-docs/`
or `/okf-root/`; it does not need scanlab's former hard-coded paths.

`--scanlab-compat` temporarily restores `/docs/...`,
`/crates/scql/docs/knowledge/...`, and `/crates/okf/docs/knowledge/...` for an
older client. Treat it as a migration adapter, not the standalone default.

## Derived Voyage State

Canonical Markdown is portable; `.okf-voyage/` is rebuildable and local. When
mounts or logical paths change, prefer an authenticated, token-free rebuild:

```bash
okf-http --local-editor 8003
TOKEN=$(cat /path/printed/by/okf-http.token)
curl -X POST \
  -H "X-OKF-Session-Token: $TOKEN" \
  http://127.0.0.1:8003/api/v1/voyage/rebuild
```

The rebuild retains embeddings only when current chunk IDs and content hashes
still match. Re-embedding missing chunks is a separate explicit action and may
consume Voyage tokens.

Review decisions are SQLite state, not browser `localStorage`. Copying only
browser assets never migrates review state. If the old SQLite database is not
retained, generate and review a new suggestion set.

## Verification

- `GET /api/v1/documents` lists every intended mount and document.
- Browser document links remain inside configured OKF routes.
- `GET /api/v1/voyage/status` reports a consistent index or clearly requests a
  rebuild.
- Accepted-but-unapplied edges are dotted; canonical relations are solid and
  retain provenance.
- Normal browsing performs no Voyage API calls.
- Monitored roots show cached status; browser refresh does not rescan them.
- Added or modified monitored files return pending-review status until their
  exact snapshot is accepted.

## Rollback

Stop standalone `okf-http`, restore the previous `.env` and derived directory,
and start the old host. Canonical Markdown requires no reverse conversion.
