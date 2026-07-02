---
title: OKF Versioning and Release Roadmap
type: Plan
kind: knowledge-plan
topic: okf-versioning
status: active
updated: 2026-07-02
tags: [okf, versioning, releases, semver, compatibility]
---

# OKF Versioning and Release Roadmap

## Purpose

OKF is expected to leave the scanlab repository and mature independently. This
roadmap defines the compatibility surfaces that lead to version 1.0.0 without
binding OKF releases to scanlab releases.

## Versioning Policy

The `okf` library and `okf-http` binary use Semantic Versioning and have
independent versions. They both release 0.3.0 for the completed format,
authentication, TLS, and secure-root-onboarding milestone, but lockstep
releases are not a long-term requirement.

Before 1.0.0:

- minor releases may contain documented breaking changes
- patch releases must remain compatible within their minor release line
- format, API, HTTP, and database compatibility changes must be identified
  separately in release notes
- every released schema or persistent format must have a tested migration path
- every release receives a dated changelog section and matching Git tag

## Planned OKF Milestones

### 0.2.0: Standalone Extraction

Status: completed as the internal standalone extraction baseline. The larger
community-facing security and onboarding work is released as 0.3.0.

- move OKF to its own repository while preserving relevant Git history
- update repository metadata, links, package policy, and CI
- publish or distribute `okf` and `okf-http` independently from scanlab
- retain Markdown plus frontmatter as canonical knowledge

### 0.3.0: Format and Relation Contract

Status: release prepared on 2026-07-02. The planned identity and format work
was completed together with the local-access/authentication/TLS and secure
document-root onboarding plans. This release therefore also includes the
versioned root API, browser review workflow, SQLite monitoring baselines,
restricted serving, migration coverage, and release gates originally expected
across later milestones. Those contracts remain pre-1.0 and can still be
refined in subsequent minor releases.

- stabilize required document metadata and reserved documents
- stabilize logical identities, mounted roots, and relation metadata
- define format validation, diagnostics, and compatibility rules
- implement the identity and format foundations from the
  [secure document root onboarding plan](secure-document-root-onboarding-plan.md)

### 0.4.0: Repository and Search APIs

- stabilize discovery, lookup, query, and repository error contracts
- stabilize derived storage boundaries and recovery behavior
- document performance and concurrency expectations

### 0.5.0: Analysis Provider Boundary

- define a provider-neutral embedding and semantic-analysis interface
- keep Voyage AI optional and physically separated from canonical OKF data
- stabilize provenance, suggestion, review, and acceptance metadata

### 0.6.0: HTTP and Browser Contract

- stabilize the versioned HTTP API
- define browser installation and upgrade behavior
- stabilize authentication and authorization for mutations and paid actions
- preserve anonymous local reading while implementing the
  [local access, authentication, and TLS plan](local-access-authentication-and-tls-plan.md)

### 0.9.0: Release Candidate

- freeze the candidate 1.0 format and public API
- publish migration guidance for all earlier supported formats
- run community feedback and compatibility testing without planned redesigns

### 1.0.0: Stable OKF Contract

Version 1.0.0 requires:

- stable canonical document and relation formats
- stable public Rust APIs or an explicit deprecation policy
- a versioned and documented HTTP API
- tested migrations for persistent schemas and canonical metadata
- provider-independent operation without Voyage AI or scanlab
- complete release, security, contribution, and recovery documentation

OKF may continue gaining features after 1.0.0. The milestone means that users
can rely on its compatibility contracts, not that development has ended.

## Repository Separation

Repository separation was completed on 2026-07-02. `okf`, `okf-http`, browser
assets, CI, release tooling, and OKF documentation now live in the independent
`open-knowledge-format` workspace. Package metadata and links point to the new
repository. scanlab consumes `okf` through a normal declared dependency and
retains only host-specific integration and configuration.
