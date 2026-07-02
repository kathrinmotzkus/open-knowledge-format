---
title: OKF Queries and Planning Sections
type: Reference
kind: knowledge-document
topic: okf
status: active
updated: 2026-06-23
---

# OKF Queries and Planning Sections

## Document Lookup

`Repository::find` accepts exact or partial `DocumentQuery` values.

Lookup considers:

- logical path
- filename
- filename stem
- title

An empty query, no match, and multiple matches are distinct structured errors.
Applications should preserve that distinction in their user interfaces.

## Planning Sections

OKF can extract completed, open, and deferred items from Markdown sections.
The default headings include English and established German forms.

English defaults include:

- `Completed Decisions`
- `Active Open Questions`
- `Open Questions`
- `Deferred / Later`

Existing German documents remain supported through the default heading aliases.
New OKF documentation should use English headings.

`RepositoryOptions` can replace or extend the recognized headings, plan kinds,
and plan-directory names.

## Plan Classification

A document is a plan when either:

- its kind appears in `RepositoryOptions::plan_kinds`, or
- its logical path is below a configured plan directory

The defaults recognize the kind `knowledge-plan` and directories named
`plans`.
