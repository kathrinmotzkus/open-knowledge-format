---
title: OKF as a Knowledge Network
type: Concept
kind: knowledge-document
topic: okf
status: active
updated: 2026-07-15
tags: [okf, knowledge-network, agents, relations, semantic-search]
---

# OKF as a Knowledge Network

OKF is not only a file format for storing notes. This project treats OKF as a
way to build durable, reviewable knowledge networks from ordinary files.

The starting point is intentionally modest: Markdown documents, YAML-like
frontmatter, directory structure, and explicit document roots. That matters
because a knowledge system should not become unusable when a server, database,
AI provider, or UI is unavailable. A person should still be able to open a
file, read it, review a diff, and move the repository with normal filesystem
tools.

The network appears when these plain files receive stable structure:

- a document belongs to one configured knowledge root
- a root may have a logical mount name
- a normal document declares a `type`
- documents can carry stable identities independent from their current path
- accepted relations can be stored as structured frontmatter
- related documents can be discovered through exact metadata, links, and
  optional semantic analysis

The canonical layer remains Markdown plus frontmatter. Derived layers such as
SQLite indexes, embedding vectors, suggestion review sets, search results, and
browser graph projections can always be rebuilt or reviewed. They improve how
the network is explored, but they do not replace the source documents.

## Relation to Agent-Oriented OKF

The upstream OKF proof of concept emphasizes a portable, vendor-neutral format
that humans, tools, and agents can produce or consume. That is compatible with
this project, but the focus here is slightly different.

This implementation does not try to be an agent framework. It provides the
repository, identity, metadata, relation, admission, and review layer that
agents, browsers, search tools, editors, and host applications can build on.

An AI agent can use OKF documents as context. A crawler or export pipeline can
produce OKF documents. A semantic-analysis layer can suggest edges. A browser
can show the graph. But OKF itself is the shared knowledge substrate below
those tools.

This distinction is important:

- AI may suggest relationships, but suggestions are not canonical truth.
- Human-readable Markdown remains the primary artifact.
- Accepted relations are explicit and reviewable.
- Provider-specific embeddings stay in derived storage, not in canonical
  documents.
- A repository stays useful without a model provider, browser server, or
  database.

In short, OKF can help agents receive better context and instructions, but it
does so by making knowledge structured, connected, and auditable first.

## What `okf` Provides

The `okf` Rust library is the lower layer of the network. It focuses on
filesystem-backed repository behavior:

- opening one or more ordered document roots
- keeping mounted roots logically separate
- discovering Markdown documents recursively
- parsing and preserving frontmatter
- reporting missing `type` metadata without destroying readability
- resolving exact and partial document queries
- detecting collisions, shadowing, and missing roots
- representing stable root, document, and portable-path identities
- exposing accepted frontmatter relations
- planning safe onboarding and initialization changes

That makes `okf` useful to applications that do not want a browser at all.
A CLI, editor plugin, static site generator, RAG system, graph processor, or
custom agent runtime can use the library directly.

## What `okf-http` Adds

`okf-http` is the local workbench for the same knowledge network. It serves the
browser and adds guarded workflows around actions that can reveal sensitive
paths, change configuration, mutate source files, or spend provider tokens.

It provides:

- local read-only browsing without login
- browser installation from the packaged binary
- protected document-root onboarding
- preview of admitted and rejected files
- optional initialization proposals for missing OKF metadata
- opt-in change monitoring
- API and browser flows for Voyage-backed semantic analysis
- review of AI-derived edge suggestions
- explicit application of accepted relations to frontmatter
- optional local HTTPS and persistent accounts for protected workflows

Reading remains simple. Mutation remains deliberate. That split is central to
the design: users should be able to explore knowledge easily without giving
the browser a blank cheque to rewrite files or spend API tokens.

## Canonical and Derived Knowledge

OKF separates canonical knowledge from derived knowledge.

Canonical knowledge is the part that survives without the runtime:

- Markdown body text
- frontmatter metadata
- document and root identities
- accepted structured relations
- declared resources
- reviewed index documents

Derived knowledge is generated from the canonical layer:

- inventories
- search indexes
- SQLite review state
- embedding vectors
- similarity scores
- suggested edges
- browser graph layouts
- monitoring baselines

Derived state may be very useful. It can make a repository searchable,
rankable, explorable, and agent-friendly. But deleting derived state must not
delete canonical knowledge. When provider calls are needed to recreate derived
state, the user should see that cost before it is spent.

## Why This Matters

Knowledge projects often begin as folders of useful text. Over time, they
become hard to navigate:

- related notes are not linked
- copied files lose their origin
- equivalent names differ only by case or Unicode spelling
- generated metadata is mixed with human-written truth
- search finds words but not concepts
- AI output is hard to audit
- a tool-specific database becomes the only place where relationships exist

OKF addresses those problems by making the network visible and portable. The
project does not require every user to adopt the same editor, database, model,
or server. It only requires that the durable knowledge layer remains readable,
structured, and reviewable.

That is why this project invests in secure onboarding, stable identities,
frontmatter relations, local browsing, optional semantic analysis, and explicit
review. Those are not extras around the format; they are the practical
conditions that let ordinary document folders become reliable knowledge
networks.
