---
title: Open Knowledge Format Documentation
type: Index
kind: knowledge-index
topic: okf
status: active
updated: 2026-07-15
---

# Open Knowledge Format Documentation

OKF is a small, application-independent model for discovering, querying, and
connecting human-maintained Markdown knowledge. This project treats OKF as a
knowledge-networking format: canonical knowledge remains readable Markdown and
frontmatter, while roots, identities, relations, search indexes, and reviewed
AI suggestions make the knowledge navigable by people, tools, and agents.

## Start Here

- [Getting started with standalone OKF](getting-started.md)
- [OKF as a knowledge network](concepts/knowledge-networking.md)
- [Format and document model](concepts/format-and-document-model.md)
- [Document root onboarding baseline](concepts/root-onboarding-baseline.md)
- [Snapshot-bound root proposals](concepts/snapshot-bound-root-proposals.md)
- [Transactional root registration and initialization](concepts/transactional-root-initialization.md)
- [Repositories and document roots](repository/document-roots.md)
- [Queries and planning sections](repository/queries-and-planning.md)
- [Embedding OKF](integration/embedding.md)
- [OKF HTTP API v1](integration/http-api-v1.md)
- [OKF HTTP threat model](security/http-threat-model.md)
- [Document-root access and authentication](security/document-root-access.md)
- [Voyage AI analysis layer](integration/voyage-ai-analysis.md)
- [Migrating from scanlab to standalone OKF](integration/standalone-migration.md)
- [OKF platform support](references/platform-support.md)
- [OKF CI-to-VM handoff](references/ci-to-vm-handoff.md)
- [OKF VM test report template](references/vm-test-report-template.md)
- [Voyage AI integration plan](plans/voyage-ai-integration-plan.md)
- [OKF HTTP server plan](plans/okf-http-server-plan.md)
- [OKF community readiness remediation plan](plans/okf-community-readiness-plan.md)
- [OKF versioning and release roadmap](plans/versioning-roadmap.md)
- [Post-extraction release follow-up plan](plans/post-extraction-release-follow-up-plan.md)
- [OKF platform, CI, and release artifacts plan](plans/platform-ci-and-release-artifacts-plan.md)
- [Local access, authentication, and TLS plan](plans/local-access-authentication-and-tls-plan.md)
- [Secure document root onboarding plan](plans/secure-document-root-onboarding-plan.md)
- [OKF public site mode and wimux.org migration plan](plans/okf-public-site-mode-and-wimux-migration-plan.md)
- [OKF VM test integration plan](plans/okf-vm-test-integration-plan.md)
- [Upstream knowledge-catalog index](references/upstream-knowledge-catalog-index.md)
- [Community release checklist](references/release-checklist.md)

The crate-level API overview is available in
[`../../README.md`](../../README.md).

Community participation and private vulnerability reporting are documented in
[`../../CONTRIBUTING.md`](../../CONTRIBUTING.md) and
[`../../SECURITY.md`](../../SECURITY.md).

This `index.md` is a reserved navigation document. Reserved `index.md` and
`log.md` files do not require the `type` field that normal OKF concept
documents require.

## Scope

OKF owns document discovery, metadata, planning-section extraction, logical
document identities, lookup, and repository diagnostics.

Host applications own:

- which directories are document roots
- root priority
- mount names
- installation and environment policy
- rendering and user interfaces
- application-specific indexing or query languages
