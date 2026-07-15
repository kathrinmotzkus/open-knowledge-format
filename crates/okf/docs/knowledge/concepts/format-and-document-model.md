---
title: OKF Format and Document Model
type: Reference
kind: knowledge-document
topic: okf
status: active
updated: 2026-07-15
---

# OKF Format and Document Model

An OKF repository is a collection of UTF-8 Markdown documents discovered below
one or more ordered roots.

The format is intentionally ordinary at the file level and network-oriented at
the repository level. A single document must remain readable in a plain text
editor. A configured repository can then treat many such documents as a
connected knowledge space through roots, logical paths, stable identities,
frontmatter metadata, declared resources, and reviewed relations.

## Markdown

Every file with the `.md` extension is a document. Discovery is recursive.
Other file types are ignored by the repository.

Documents may begin with a simple frontmatter block:

```text
---
title: Example
type: Reference
kind: knowledge-document
topic: documentation
status: active
updated: 2026-06-23
---
```

For normal concept documents, OKF v0.1 requires a non-empty `type` field.
Reserved files such as `index.md` and `log.md` are special navigation/history
files and are not normal concept documents.

Unknown `type` values and unknown additional frontmatter fields are preserved.
OKF does not enforce a closed metadata vocabulary.

The upstream format and scanlab/OKF extensions are distinguished in the
[document root onboarding baseline](root-onboarding-baseline.md). In
particular, `kind`, structured `relations`, CSV resource admission, and the
requirement for an `index.md` in every onboarded directory are local extensions
or policies, not requirements of upstream OKF v0.1.

## Canonical Relations

Reviewed relations may be stored in the `relations` frontmatter field as a
JSON array. This keeps canonical machine-readable truth in the Markdown file
while the browser supplies the human-facing graph projection. For example:

```text
relations: [
  {
    "type": "ai_suggested_edge",
    "target": "okf/target.md",
    "suggestion_id": "suggestion-1",
    "provider": "voyage-ai",
    "model": "voyage-4",
    "generation_method": "embedding_similarity",
    "ai_generated": true,
    "score": 0.91,
    "status": "accepted"
  }
]
```

`Document::relations()` exposes valid relation objects as typed
`CanonicalRelation` values. Unknown frontmatter fields and existing relation
objects are retained when `okf-http` explicitly applies accepted suggestions.
Raw embedding vectors are never copied into canonical Markdown.

## Declared Resources

Non-Markdown files are not concept documents. The local onboarding profile
currently permits CSV only when the file passed admission and its owning
directory's `index.md` declares it in a `resources` JSON array:

```text
---
resources: [
  {
    "path": "ports.csv",
    "type": "Data Resource",
    "media_type": "text/csv; charset=utf-8"
  }
]
---
```

`path` is exactly one portable filename in the same directory as that
`index.md`; it cannot name a child directory, hidden file, absolute path, or
URL. `type` is a non-empty, open-ended resource classification. `media_type`
is fixed to `text/csv; charset=utf-8`. Nested directories own their resources
through their own `index.md`. Duplicate, missing, malformed, unsupported, or
undeclared resources are diagnostics and are not servable.

`DeclaredResource` exposes valid declarations through the Rust repository API.
The source CSV remains canonical data; delimiter, row/column statistics, and
warnings are derived review information. CSV validation reports UTF-8 encoding,
detects comma, semicolon, or tab delimiters, checks quote and column structure,
and warns when a cell begins with `=`, `+`, `-`, or `@`. Browser rendering must
escape every cell as text and must never evaluate HTML, Markdown, links, or
spreadsheet formulas. Downloads retain the original bytes and warning state.

Every admitted directory containing a document or resource requires an exact
lowercase `index.md`. This is a stricter local onboarding profile than upstream
OKF v0.1, where indexes remain optional. Empty directories and directories
whose contents were all rejected do not require or receive an index proposal.

## Titles

The title is selected in this order:

1. the `title` frontmatter field
2. the first level-one Markdown heading
3. the filename stem

## Document Types and Compatibility Kinds

`type` is the primary OKF concept classification. The Rust crate exposes it
through `Document::document_type()`.

`kind` is scanlab's older compatibility classification. The Rust crate still
exposes it through `Document::kind()` so existing documents and plan detection
continue to work during migration.

The default repository options recognize `knowledge-plan` as a plan kind.

## Logical Paths

Each document has:

- a physical root
- a logical relative path

For an unmounted root, the logical path is the filesystem-relative path.
For a mounted root, the mount name is prepended. A physical file
`docs/knowledge/index.md` mounted as `product` therefore has the logical path
`product/index.md`.

Logical paths are stable application-facing identities. They prevent unrelated
roots from colliding merely because both contain common names such as
`index.md`.

Logical paths are locators, not stable knowledge identities. Mount names,
physical root order, browser routes, percent-encoded URLs, and numeric HTTP
root indexes may all change without changing the represented knowledge.

## Stable Identities

### Logical document URIs

Documents in a mounted root have a stable logical URI:

```text
okf://<mount>/<source-relative-document-path>
```

For example, `security/http-threat-model.md` in the `okf` mount is identified
as `okf://okf/security/http-threat-model.md`. UTF-8 and spaces are
percent-encoded. This is an OKF URI rather than a filesystem URL or formal URN:
the mount is its authority, the path is relative to that mount, and no physical
directory is disclosed. Unmounted roots do not receive such a URI until they
have a stable logical mount.

The navigation hierarchy follows the same structure. A root-level document is
classified as `root-document`; a document below one or more real directories
is `nested-document`. Frontmatter values such as `topic`, `type`, and `kind`
remain metadata and never create synthetic folders.

The local onboarding profile assigns two opaque URN values:

- `okf_root_id: urn:okf:root:<token>` in frontmatter of the bundle-root
  `index.md`
- `okf_document_id: urn:okf:document:<token>` in every non-reserved concept
  document

Tokens contain 16–128 ASCII letters, digits, dots, underscores, or hyphens.
Writers generate them from cryptographically secure randomness and never from
paths, titles, document contents, mount names, or root order. They are copied
unchanged on rename or move. A copied document must receive a new document ID;
duplicate IDs inside one configured knowledge space are blocking onboarding
conflicts. Reserved `index.md` and `log.md` files do not receive document IDs.

Existing repositories remain readable without either field. Migration first
previews random IDs and metadata-only edits, then writes them only after an
explicit source-mutation confirmation. It never renames files or rewrites
spaces, case, or Unicode spelling. `RootId`, `DocumentId`, and `PortablePath`
represent these contracts in the public Rust API.

Canonical relations will migrate to an identity-first target containing root
and document IDs while retaining a logical target path as a human-readable and
legacy fallback. Identity resolution wins when IDs are present. This lets a
relation survive root reordering, mount changes, moves, and renames; a missing
ID target becomes a broken relation rather than silently binding to a new file
that happens to reuse the old path.

## Portable Path Comparison

`PortablePath` preserves the exact UTF-8 display path with `/` separators and
spaces unchanged. It rejects absolute and platform-prefixed paths, empty,
`.` and `..` components, backslashes, non-UTF-8 components, NUL, and other
control characters. Percent encoding exists only at the HTTP boundary.

A separate comparison key applies Unicode NFKC normalization and case folding
component by component. The key is used only for collision detection and is
never shown as, or written over, a filename. Exact, case-only, compatibility,
and normalization-equivalent collisions block onboarding even on a
case-sensitive filesystem.

## Onboarding and Monitoring State

The filesystem is not canonical merely because a path is configured. Secure
onboarding first builds a bounded snapshot containing admitted and rejected
entries, compliance diagnostics, collisions, and source proposals. Registration
persists only the root configuration. Optional initialization writes only the
separately reviewed metadata and index changes.

For opted-in monitoring, SQLite stores a rebuildable accepted inventory and
pending comparison snapshots; it never becomes canonical knowledge. Added or
content-modified paths are withheld until the exact current digest is accepted.
Accepting advances only the monitoring baseline because the user or editor
already changed the source. Dismissing a notice advances nothing. Neither
detection nor review creates relations, embeddings, or Voyage calls.
