---
type: Reference
title: Upstream Knowledge Catalog Index
description: Local index of the downloaded Open Knowledge Format knowledge-catalog repository.
kind: knowledge-reference
topic: okf
status: active
updated: 2026-06-23
resource: upstream-open-knowledge-format/knowledge-catalog
tags: [okf, upstream, catalog, reference]
---

# Upstream Knowledge Catalog Index

This document indexes the locally downloaded `knowledge-catalog` repository so
the scanlab OKF implementation can use it as a reference without importing the
repository contents directly.

The source checkout is external to this repository. Individual developers may
place it anywhere and configure it as an OKF root when needed; this portable
index deliberately does not record a private filesystem path.

## Top-Level Categories

| Path | Role | Notes for scanlab OKF |
| --- | --- | --- |
| `README.md` | Repository overview | Entry point for the full knowledge-catalog project. |
| `okf/` | Format specification, Python reference agent, generated bundles, tests | Primary reference for OKF semantics and conformance. |
| `okf/SPEC.md` | OKF v0.1 specification | Defines bundle structure, concept documents, required `type`, index files, log files, links, citations, and conformance. |
| `okf/bundles/` | Example OKF bundles | Concrete examples for datasets, tables, and references. Useful as test material and fixture inspiration. |
| `okf/src/reference_agent/` | Python reference producer/consumer implementation | Useful for behavior comparisons, but not a dependency target for the Rust crate. |
| `okf/tests/` | Reference behavior tests | Useful for identifying expected parser, index, bundle, viewer, and web-ingestion behavior. |
| `okf/samples/` | Seeded bundle-generation samples | Shows how the reference agent creates bundles from seed inputs. |
| `samples/` | Standalone discovery/enrichment samples | Useful for future agent workflows, not part of the core format contract. |
| `toolbox/` | TypeScript tooling and demos | Useful for sync/enrichment ideas and OKF-to-tooling workflows. |

## File Inventory Snapshot

The local checkout contains:

- 131 Markdown files outside `.git`
- 78 Markdown files under `okf/bundles`
- 17 Markdown files under `toolbox/mdcode/demo/okf/catalog`
- 8 Markdown files under `samples/enrichment/sample/docs`

The `okf/bundles` directory currently contains three ready-made example
bundles:

- `crypto_bitcoin`
- `ga4`
- `stackoverflow`

These bundles are the most useful external fixture candidates because they are
already shaped as OKF bundles.

## OKF v0.1 Rules Confirmed by the Catalog

The specification describes an OKF bundle as a directory tree of Markdown files
with YAML frontmatter.

Reserved filenames:

- `index.md` is a directory listing.
- `log.md` is an optional update history.
- Every other `.md` file is a concept document.

Concept documents must contain YAML frontmatter and must have a non-empty
`type` field. The required and recommended frontmatter shape is:

```yaml
---
type: <Type name>
title: <Optional display name>
description: <Optional one-line summary>
resource: <Optional canonical URI for the underlying asset>
tags: [<tag>, <tag>]
timestamp: <ISO 8601 datetime>
---
```

Important conformance detail: consumers must tolerate unknown `type` values and
unknown additional frontmatter keys. That makes `type` required for documents
but open-ended in vocabulary.

## Observed Concept Types

The example bundles under `okf/bundles` currently use these concept `type`
values:

| Type | Count | Example |
| --- | ---: | --- |
| `BigQuery Dataset` | 3 | `okf/bundles/stackoverflow/datasets/stackoverflow.md` |
| `BigQuery Table` | 21 | `okf/bundles/stackoverflow/tables/tags.md` |
| `Reference` | 41 | `okf/bundles/stackoverflow/references/tag_synonyms.md` |

The specification also names additional example types such as `API Endpoint`,
`Metric`, and `Playbook`. These are examples, not a central registry.

## Consequences for scanlab OKF

Our current OKF documents mostly use `kind`, which is scanlab's existing
internal classification field. OKF v0.1 requires `type`.

Recommended migration direction:

1. Keep reading `kind` for backward compatibility inside scanlab.
2. Add `type` to every non-reserved Markdown file that is loaded through OKF
   document roots.
3. Teach the Rust OKF crate to expose `type` as the primary OKF concept type.
4. Treat `kind` as a local compatibility/legacy classification unless we decide
   to retire it later.
5. Do not require a fixed enum of type names; validate presence and non-empty
   value, not central registration.
6. Keep `index.md` and `log.md` special: they are not normal concept documents.

## Candidate Type Vocabulary for scanlab Documents

This vocabulary is intentionally descriptive rather than exhaustive:

| Existing document family | Candidate `type` |
| --- | --- |
| Architecture documents | `Architecture` |
| Concepts | `Concept` |
| Mapping documents | `Mapping` |
| Plans | `Plan` |
| Playground/research notes | `Research Note` |
| Future/deferred notes | `Roadmap Note` |
| SCQL language documentation | `Language Reference` |
| SCQL object model documents | `Object Model` |
| OKF crate documentation | `Reference` |
| README-style entry points | `Guide` |

The exact names can evolve. The important part for conformance is that every
concept document has a non-empty `type`.

## Useful Follow-Up Questions

- Should the Rust crate reject missing `type` only in strict mode, or should it
  emit diagnostics while continuing to load documents?
- Should `kind` become an alias for `type`, or should both fields remain
  distinct?
- Should scanlab's existing `knowledge-plan` kind map to `type: Plan`?
- Should OKF expose reserved files (`index.md`, `log.md`) as documents,
  metadata-only entries, or a separate repository navigation layer?
- Should we add an external fixture root for `okf/bundles` in tests, or keep a
  small copied fixture to avoid depending on the local download path?
