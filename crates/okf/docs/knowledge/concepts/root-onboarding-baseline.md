---
title: Document Root Onboarding Baseline
type: Reference
kind: knowledge-reference
topic: okf-document-roots
status: active
updated: 2026-07-01
tags: [okf, document-roots, compatibility, resources, limits, security]
---

# Document Root Onboarding Baseline

This reference separates the upstream OKF v0.1 format from stricter OKF
onboarding policy and local extensions. The comparison was made against the
downloaded upstream `okf/SPEC.md`, example bundles, the local format reference,
and the current Rust document model. The external checkout is evidence for the
audit but is not a build or runtime dependency.

## Compatibility Layers

| Rule or field | Classification | Compatibility behavior |
|---|---|---|
| UTF-8 Markdown concept documents | OKF v0.1 | Required for concepts. |
| YAML frontmatter on non-reserved documents | OKF v0.1 | Required for a conformant producer; consumers remain best-effort. |
| Non-empty `type` | OKF v0.1 | Required and open-ended; unknown values are accepted. |
| `title`, `description`, `resource`, `tags`, `timestamp` | OKF v0.1 | Optional and preserved when present. `resource` is a URI identifying an underlying asset, not permission to serve a local file. |
| `index.md`, `log.md` | OKF v0.1 reserved names | Optional upstream. Their reserved semantics are retained. |
| An `index.md` in every admitted directory | Local onboarding profile | Stricter producer policy for reviewed roots. Its absence is an onboarding diagnostic, not a claim that upstream OKF requires it. |
| `kind`, `topic`, `status`, `updated` | Local extensions | Preserved as unknown producer fields. `kind` remains a compatibility classification and never replaces `type`. |
| Structured `relations` metadata | Local extension | Preserved and exposed by OKF tooling. Upstream v0.1 defines Markdown links, not this relation schema. |
| CSV files and resource declarations | Planned local extension | CSV is eligible for admission only after Phase 4 defines its declaring document and metadata contract. A bare CSV is not an OKF concept. |

Unknown frontmatter keys and unknown `type` values must survive parsing and
round-tripping. A local extension must use optional metadata, must not alter the
meaning of an upstream field, and must remain ignorable by a consumer that only
implements OKF v0.1. Strict onboarding diagnostics do not make the underlying
best-effort reader reject an otherwise useful partial bundle.

## Onboarding Terms

- **Configured**: a physical root is present in an effective trusted source of
  root configuration. Configuration alone does not make every file servable.
- **Admitted**: a file passed the current allowlist, path, file-kind, limit,
  collision, and snapshot checks and belongs to the approved inventory.
- **OKF-compatible**: the admitted Markdown hierarchy satisfies upstream OKF
  v0.1 plus the declared local onboarding profile, including the stricter
  per-directory `index.md` rule. Compatibility does not imply user approval to
  modify source files.
- **Pending**: an observed item or change has been classified and presented in
  a snapshot-bound proposal but has not been accepted or rejected by a user.
- **Rejected**: an item is excluded by policy, conflict, limit, or an explicit
  user decision. Rejected items are neither served nor persisted as admitted
  content.
- **Canonical**: source Markdown and approved resource declarations that carry
  portable OKF knowledge. Configuration, previews, inventories, SQLite state,
  embeddings, sessions, and filesystem paths are not canonical knowledge.

## Trusted Scan Limits

The initial defaults for one proposed root are:

| Limit | Default |
|---|---:|
| Maximum admitted size of one file | 8 MiB |
| Maximum filesystem entries inspected | 10,000 |
| Maximum directory nesting below the root | 32 components |
| Maximum total bytes inspected for candidate regular files | 512 MiB |

Limits apply before content parsing and include rejected candidate entries
where their metadata or bytes had to be inspected. Crossing any limit stops the
scan, marks the proposal incomplete, reports the limit name, configured value,
and observed lower bound, and prevents registration, initialization, or source
mutation from that proposal. No partial inventory may be confirmed.

Only the host operator may change limits through future CLI or operator-owned
configuration. Browser-managed `~/.config/okf/config.toml`, document
frontmatter, query parameters, request bodies, and imported roots cannot raise
them. Values must be positive, bounded by implementation hard ceilings, and
validated before scanning. `AdmissionLimits` exposes these defaults and hard
ceilings as a typed OKF-core scanner contract. Host integration and persistent
operator overrides remain later work; browser input cannot supply this type.

## Closed Serving Gap

The former HTTP resolver confined a request to a configured physical root but
served any regular file below that root. Phase 5 replaced it with admission-
and compliance-authorized resolution. The original generated `.txt` regression
fixture now receives `404 Not Found` as a permanent, non-quarantined test.
