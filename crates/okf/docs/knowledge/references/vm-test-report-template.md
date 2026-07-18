---
title: OKF VM Test Report Template
type: Template
status: draft
created: 2026-07-18
updated: 2026-07-18
---

# OKF VM Test Report Template

Use this template for VM-based release artifact tests.

## Summary

- Result: pass / fail / partial
- Tester:
- Date:
- Notes:

## CI Artifact

- Workflow:
- Run ID:
- Commit SHA:
- Git tag, if any:
- Artifact name:
- Checksum file:
- Checksum result:

## Versions

- `okf-http`:
- `okf-open-knowledge-format`:
- Rust target:
- Debian architecture, if applicable:

## VM Profile

- Hypervisor:
- OS:
- OS version:
- Architecture:
- Kernel:
- Package manager:
- Init system:
- Rust installed before test: yes / no
- NGINX installed before test: yes / no
- Firewall enabled before test: yes / no / unknown

## Installation Method

- Method: `.deb` / archive
- Command used:
- Required sudo: yes / no
- Installed binary path:
- Browser asset path:
- Service example path:

## Smoke Checks

- `command -v okf-http`:
- `okf-http --help`:
- Version file:
- Browser assets:
- Service examples:
- Rust not required:

## Service Checks

Only fill this section when service testing is part of the VM run.

- User service tested: yes / no
- System service tested: yes / no
- Bind host:
- Port:
- Port availability checked:
- Service starts:
- Service survives restart:
- Service survives reboot:

## Reverse Proxy Checks

Only fill this section when NGINX or another reverse proxy is part of the VM
run.

- Reverse proxy:
- Proxy bind:
- OKF bind:
- Public URL:
- TLS handled by proxy:
- Public mutation routes unavailable:

## Deployment Checks

Only fill this section when bundle deployment is part of the VM run.

- Site name:
- Bundle artifact:
- Deployed path:
- Current symlink:
- Rollback tested:

## Problems Found

- Problem:
- Evidence:
- Fix or workaround:
- Follow-up needed:

## Final Decision

- Artifact acceptable for release recommendation: yes / no
- Required follow-up before publication:
