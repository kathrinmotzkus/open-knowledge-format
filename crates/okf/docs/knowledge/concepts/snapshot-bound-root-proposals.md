---
title: Snapshot-Bound Root Proposals
type: Concept
kind: knowledge-concept
topic: okf-document-roots
status: active
updated: 2026-07-02
tags: [okf, document-roots, proposals, snapshots, security]
---

# Snapshot-Bound Root Proposals

OKF root onboarding uses a read-only proposal as the review boundary between an
arbitrary directory and persistent configuration or canonical source changes.
Creating a proposal does not write configuration, Markdown, relations, SQLite,
or Voyage data and does not call an AI provider.

## Proposal Types

Registration and source initialization are separate operations:

- A registration proposal describes the exact root identity, mount, canonical
  physical path, enabled state, and monitoring default that could later enter
  `config.toml`, including the proposed root priority.
- A source-initialization proposal carries the deterministic `index.md` and
  frontmatter changes produced by compliance analysis.

Both proposal types may share the same filesystem snapshot digest, but they
have different proposal digests. Consent to one operation never implies
consent to the other.

## Snapshot Content

The snapshot records every encountered path in deterministic order, its file
kind and size, and a SHA-256 content hash for every bounded candidate file that
the admission scanner reads. It also includes rejected paths and reasons,
admission status, inspected entry count, and inspected candidate bytes. Empty
directories therefore remain visible to addition, removal, and rename checks.

The proposal digest additionally binds the canonical root, mount, operation,
trusted scan limits, override state, configured-root context, conflicts, and
the proposed operation-specific changes. Filesystem timestamps are not used as
proof of equality.

## Review Tree and Conflicts

The proposal tree contains accepted Markdown and CSV, rejected entries and
reason codes, pending Markdown metadata, missing nested indexes, declared and
undeclared resources, and conflicts. Conflict analysis covers portable-path
case or Unicode collisions, missing or invalid root identity, duplicate stable
identity, mount reuse, logical-path shadowing, and unavailable comparison
roots.

The summary reports counts, candidate bytes, trusted limits, declarative
writeability, and active process, `.env`, and CLI override effects. Physical
paths are sensitive and must only be exposed by a host after `roots.propose`
authorization.

## Lifetime and Revalidation

`RootProposalStore` keeps proposals in expiring process memory outside the
document root and canonical OKF data. Before any later confirmation, the host
must call validation. Validation performs a fresh bounded scan and rebuilds
the proposal context. A changed, added, removed, renamed, retyped, newly
colliding, or newly shadowed entry invalidates the whole proposal. An expired,
missing, unreadable, or otherwise non-reproducible proposal cannot authorize a
partial operation.

HTTP endpoints and browser rendering consume this core model in the later API
and workflow phase; they must not weaken its expiration or digest checks.
