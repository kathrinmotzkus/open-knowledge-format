---
title: OKF Document Roots
type: Reference
kind: knowledge-document
topic: okf
status: active
updated: 2026-07-01
---

# OKF Document Roots

`Repository` opens an ordered list of `DocumentRoot` values.

## Unmounted Roots

```rust
use okf::DocumentRoot;

let root = DocumentRoot::new("docs");
```

This preserves the historical behavior: a file below the root is identified
only by its path relative to that root.

## Mounted Roots

```rust
use okf::DocumentRoot;

let root = DocumentRoot::mounted("scql", "crates/scql/docs/knowledge");
```

Mounted roots add a logical namespace without changing the physical directory.

Mount names are application data. OKF does not prescribe names such as
`scanlab`, `scql`, or `okf`.

## Priority and Shadowing

Roots are evaluated in their configured order.

If two roots expose the same logical path, the earlier root wins and the later
document produces a `Diagnostic::ShadowedDocument`.

This is useful for local-over-installed priority:

```rust
use okf::DocumentRoot;

let roots = [
    DocumentRoot::mounted("product", "docs"),
    DocumentRoot::mounted("product", "/usr/local/share/product/docs"),
];
```

Both roots share the same namespace, so a repository-local document can shadow
its installed counterpart.

Different mount names never shadow each other solely because their internal
relative paths are equal.

Root order, mount names, and the numeric indexes used by legacy
`/okf-root/{index}/...` HTTP routes are runtime routing details only. They are
not portable knowledge identities and must never be written into canonical
relations. A portable root carries `okf_root_id` in its root `index.md`, while
concept documents carry `okf_document_id`; changing configuration order or a
mount does not change either identity.

Missing roots are non-fatal diagnostics as long as at least one configured
root is usable.

## `okf-http` Configuration Precedence

The `okf` library itself receives an already ordered `DocumentRoot` list and
does not read environment variables. The `okf-http` host constructs that list
in this exact order:

1. The base is the process-level `OKF_DOCUMENT_ROOTS` value when it is defined;
   otherwise `.env`; otherwise enabled browser roots from the versioned XDG
   configuration; otherwise host defaults.
2. Repeated CLI `--root` entries are placed before the base. A mounted CLI root
   removes base entries with the same mount for that process. An unmounted CLI
   root is simply a higher-priority root.
3. Repeated `--add-root` entries are appended after the base as lower-priority
   fallbacks.
4. Exact duplicate mount/path pairs are removed while preserving the first
   occurrence.

Within the resulting list, the first document with a logical path wins. A
process environment value overrides the entire `.env` root list, including
when the process value is intentionally empty.

Root specifications use either `<path>` or `<mount>=<path>`. Multiple
persistent specifications use the operating system's path-list separator
(`:` on Linux). Mount names must be one safe logical path component.

The local-editor API modifies only `$XDG_CONFIG_HOME/okf/config.toml`, with
`~/.config/okf/config.toml` as the Linux fallback. The schema stores stable root
identity, mount, canonical physical path, enabled state, priority, and the
`check_for_changes` preference. The directory and file are owner-private and
the file is replaced atomically. These routes are absent in default read-only
mode and require a paired session with `roots.configure` plus
`X-OKF-Config-Write: confirm`. Changes become active after restart and cannot
override process or `.env` operator configuration.

The browser never writes `.env`. Operators can explicitly copy compatible
`.env` roots into XDG configuration with `okf-http roots import-env`; all roots
must already carry a valid `okf_root_id` in their root `index.md`. The command
does not modify `.env` or source files.

## Browser Onboarding and Change Review

The browser accepts only a path text field plus mount, priority, and monitoring
preference. It cannot enumerate parent directories. Registration requires a
current bounded proposal and explicit acknowledgement of its complete tree.
Source initialization is a second transaction with exact diffs and a separate
confirmation header.

For `check_for_changes = true`, a reviewed registration seeds an owner-private
SQLite baseline. Startup performs one asynchronous token-free comparison and
the browser's **Check now** action performs later scans. Page refresh reads
cached status only. Added, modified, deleted, rename-candidate, hidden,
rejected, metadata-invalid, and conflicting entries remain pending. Added and
modified documents are excluded from HTTP document inventories and routes until
the same snapshot digest is revalidated and accepted. Dismissal does not move
the baseline. Removing a root deletes configuration and rebuildable monitoring
rows, never source files.

Browser-root precedence is independent: CLI `--browser-root`, then process
`OKF_BROWSER_ROOT`, then `.env`, then `~/docs-browser` on Linux.
