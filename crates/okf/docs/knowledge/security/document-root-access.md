---
title: Document Root Access and Authentication
type: Guide
kind: knowledge-document
topic: okf-security
status: active
updated: 2026-07-02
---

# Document Root Access and Authentication

Reading OKF documents remains available without authentication. Adding,
changing, monitoring, initializing, or removing document roots changes trusted
server configuration or source files and therefore requires an authenticated
session with the corresponding capability.

## Read-only startup

Starting `okf-http <port>` serves documents anonymously in read-only mode. It
does not expose root-management forms and cannot create an authenticated
editor session. Restart with an explicitly protected editing mode when root
configuration is required.

## Local editor pairing

For a loopback-only local session, start:

```bash
okf-http --local-editor <port>
```

The terminal prints a short-lived one-time pairing code. Use **Pair browser**,
enter that code, and review the session capabilities shown by the browser.
Pairing credentials and CSRF state remain server-side or in protected cookies;
the browser does not persist them in local storage.

## Account login over HTTPS

An authenticated HTTPS deployment uses accounts created from the console.
Select **Sign in**, then enter the account credentials. Root-management
controls appear only after the server confirms the authenticated session.
Roles and capabilities still determine which actions are allowed.

## Root onboarding remains review-gated

Authentication is permission to request an operation, not permission for OKF
to change arbitrary files automatically. A new root is scanned first. Hidden,
unsupported, unsafe, or escaping files are rejected. The complete proposal and
all conflicts must be reviewed before registration. Source initialization is a
separate confirmation showing the exact planned changes.

Browser-managed roots are stored in `~/.config/okf/config.toml`. The browser
never edits `.env`; process and `.env` roots remain operator-controlled
overrides.

