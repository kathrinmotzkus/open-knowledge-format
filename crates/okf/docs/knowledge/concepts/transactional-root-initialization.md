---
title: Transactional Root Registration and Initialization
type: Concept
kind: knowledge-concept
topic: okf-document-roots
status: active
updated: 2026-07-02
tags: [okf, document-roots, transactions, recovery, configuration]
---

# Transactional Root Registration and Initialization

OKF treats root registration and source initialization as independent consent
decisions. Both consume an unexpired snapshot proposal, but neither operation
implicitly authorizes the other.

## Registration Transaction

`confirm_root_registration` accepts only a registration proposal and an exact
configuration revision. It acquires an advisory cross-process lock, validates
the proposal again, verifies the expected revision, and atomically replaces
the versioned XDG `config.toml`. The planned root identity, canonical path,
mount, priority, enabled state, and monitoring preference must match the
proposal.

An identical retry is idempotent. A changed configuration revision or a stable
identity already registered with different values fails without writing. The
configuration path and lock are required to remain outside the document root,
so registration cannot modify source files.

## Initialization Plan

`build_initialization_plan` renders exact before and after content plus a
human-readable diff for every proposed file. It merges only missing
frontmatter fields, preserves unknown metadata and existing body content,
retains CRLF or LF line endings where practical, and refuses to overwrite an
existing field blindly. Missing nested indexes are created only when the
snapshot proposed them.

CSV resource declarations require an explicit non-empty type supplied in
`InitializationOptions`. They are merged into the owning `index.md` with the
fixed CSV media type. A new root without identity requires a preapproved random
`proposed_root_id`; its exact value is visible in the plan before confirmation.

The plan includes read-only Git worktree status. OKF invokes only `git
rev-parse` and `git status`; it never stages, resets, commits, or otherwise
changes Git state.

## Journaled Multi-File Commit

`confirm_source_initialization` requires the exact plan digest and keeps all
staging files, backups, and the versioned operation journal in a private state
directory outside the document root. On Linux, a host should normally choose
`$XDG_STATE_HOME/okf`, falling back to `~/.local/state/okf`.

The operation follows this sequence:

1. acquire the cross-process initialization lock;
2. recover any older incomplete journal;
3. validate the snapshot and rebuild the exact plan;
4. stage all new bytes and verified backups outside the root;
5. persist and sync the journal;
6. recheck each source immediately before atomic replacement;
7. preserve existing permissions and verify every resulting content hash;
8. mark the journal committed and remove rollback material.

Failure before the committed marker restores every already changed original
and removes newly created files. Recovery refuses to overwrite an unexpected
third-party state. A committed journal left only because cleanup was
interrupted is removed without rollback.

Initialization rejects transaction state inside the document root and has no
API for changing `config.toml`, relations, SQLite, or Voyage state. Final diffs
and Git status are returned to the host after a verified commit.
