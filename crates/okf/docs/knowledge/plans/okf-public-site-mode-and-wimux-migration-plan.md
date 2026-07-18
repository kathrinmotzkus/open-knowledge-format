---
title: OKF Public Site Mode and wimux.org Migration Plan
type: Plan
kind: knowledge-plan
topic: okf-public-site-mode
status: active
updated: 2026-07-18
tags: [okf, okf-http, public-site, intranet, wimux, nginx, django, migration]
---

# OKF Public Site Mode and wimux.org Migration Plan

## Purpose

OKF should be able to serve controlled public websites and intranet portals
directly from OKF documents. Public internet deployments must be immutable from
the web surface. Intranet deployments may optionally expose protected editing
and administration workflows through the security layers already implemented in
`okf-http`. wimux.org should eventually move away from its current Django
database-backed content model and become a public reference deployment for OKF
itself.

The current Django synchronization command is a useful transition step. It
proves that curated OKF Markdown documents can become website content, but it
should not be the final architecture. The long-term target is:

```text
OKF Markdown roots
  -> okf-http site/publish mode
  -> NGINX reverse proxy
  -> wimux.org or an intranet site
```

## Design Principles

- OKF Markdown and frontmatter are the editorial source of truth.
- The public server must not be an editing server.
- Public and intranet rendering must expose only explicitly released content.
- Public internet site mode must never mount mutating APIs.
- Internal site mode may mount protected mutation and administration APIs only
  when explicitly enabled and guarded by the existing OKF authentication,
  TLS, CSRF, origin, capability, confirmation-header, and revision checks.
- Protected internal editing should require a deliberate reverse-proxy
  deployment profile instead of being enabled on a directly exposed listener.
- NGINX may add another security layer, but `okf-http` itself must be safe
  before the reverse proxy is considered.
- wimux.org should demonstrate that OKF is usable not only locally, but also in
  productive public or intranet publishing environments.

## Target Modes

### Local Workbench Mode

Used by authors and maintainers.

- document browsing
- document-root management
- source initialization
- review workflows
- accepted relation writes
- optional Voyage AI / semantic analysis
- local-editor pairing or authenticated HTTPS when mutation is enabled

### Public Site Mode

Used for public websites such as wimux.org.

- only read-only rendering routes
- only documents with public visibility
- no document-root management
- no local-editor pairing
- no login UI
- no Voyage execution
- no canonical writes
- no SQLite mutation routes
- no physical-path exposure
- no `.env` or local configuration exposure

### Internal Site Mode

Used for intranet portals. Internal mode starts from the same rendering model as
public site mode, but it may deliberately enable protected administrative or
editor workflows.

- may include both `public` and `internal` documents
- may be read-only by default
- may enable protected root management, review, accepted-relation writes, or
  source initialization only through explicit flags
- should require a trusted reverse proxy for protected editing workflows
- must use the existing `okf-http` security layers for every mutation:
  - authenticated HTTPS or an approved local-editor/internal-auth mode
  - CSRF and origin checks
  - route capability checks
  - exact revision/digest checks
  - explicit confirmation headers for writes
  - privacy-safe audit output
- may sit behind NGINX, SSO, Basic Auth, client certificates, VPN, or network
  access control
- must never treat intranet visibility as permission to bypass OKF's own
  authorization model

## Visibility Model

The current transitional website documents use `public: true`. This should be
generalized into an explicit visibility model:

```yaml
visibility: private
visibility: internal
visibility: public
```

Suggested interpretation:

- `private`: never served in site mode
- `internal`: can be served in intranet mode
- `public`: can be served on the public internet

For compatibility, `public: true` may initially be treated as
`visibility: public`.

Site mode should accept a maximum visibility:

```bash
okf-http --site --visibility public 8080
okf-http --site --visibility internal 8080
```

An `internal` site may include `public` and `internal` documents. A `public`
site includes only `public` documents.

## Website Frontmatter

Website rendering needs a small, explicit metadata vocabulary:

```yaml
type: WebsitePage
status: published
visibility: public
language: de
website:
  section: software
  slug: okf
  title: OKF - Open Knowledge Format
  short_title: OKF
  teaser: Local-first knowledge networking for Markdown repositories.
  order: 10
  show_in_navigation: true
  parent: okf
```

The exact vocabulary can evolve, but the first stable version should cover:

- page type
- language
- slug
- title
- short title
- teaser
- navigation order
- parent/section relationship
- visibility
- status badges such as `draft`, `published`, or `deprecated`

## Public Security Requirements

`okf-http --site --visibility public` must guarantee:

- mutation routes are not mounted
- root-management routes are not mounted
- local-editor pairing routes are not mounted
- persistent user-management routes are not mounted
- Voyage execution routes are not mounted
- accepted-relation write routes are not mounted
- source-initialization routes are not mounted
- physical-path debug routes are unavailable
- hidden files are never served
- traversal and symlink escape protection remains active
- only admitted file types are served
- only selected visibility levels are rendered
- rejected or private documents do not appear in navigation, search, sitemap,
  feeds, or raw document routes

The deployment should additionally support read-only filesystem roots. On a
production server, the OKF source checkout should be mounted or deployed
read-only wherever possible.

## Internal Security Requirements

`okf-http --site --visibility internal` may expose protected write workflows
only when explicitly requested. Internal deployments must not rely on "it is
only the intranet" as a security boundary.

If internal mutation is enabled, `okf-http` must preserve the same safety model
used by local-editor and authenticated HTTPS modes:

- protected editing should be available only when `okf-http` is started in an
  explicit trusted reverse-proxy profile
- direct internal listeners should remain read-only unless a later threat model
  explicitly approves another deployment shape
- mutating routes are absent unless enabled by explicit CLI/configuration
- users or sessions need the relevant capabilities
- source and configuration writes require confirmation headers
- proposal, plan, revision, and digest checks remain mandatory
- root admission still rejects hidden files, symlink escapes, traversal, and
  unsafe file types
- physical paths are visible only to authorized users with the required
  capability
- Voyage or other token-spending actions remain separately gated
- public documents must not inherit broader write permissions just because an
  internal admin interface exists

## Phase 1: Stabilize Website Source Metadata

- Keep the current transitional `public: true` documents working.
- Add and document `visibility`.
- Define the first website metadata vocabulary.
- Update the wimux.org OKF website source documents to use both
  `visibility: public` and `public: true` during migration.
- Add examples for public, internal, and private documents.

Completion criteria:

- OKF can distinguish public, internal, and private website documents.
- Transitional documents remain compatible with the existing Django sync.

## Phase 2: Add Site Inventory to OKF

- Add a library-level site inventory builder in `okf`.
- It should read document roots and return only documents allowed for a target
  visibility.
- It should calculate stable slugs, parent relationships, navigation order,
  language, status, and canonical OKF URI.
- It should report diagnostics for duplicate slugs, missing parents, missing
  titles, unsupported languages, hidden files, and visibility conflicts.

Completion criteria:

- Site inventory can be tested without HTTP.
- Private documents cannot accidentally appear in public inventory.

## Phase 2b: Build Deployable OKF Source Bundles

Local OKF workstations may mount document roots from many unrelated filesystem
locations. Public and intranet deployments should not serve those scattered
private paths directly. Instead, OKF should be able to create a deliberate
deployable bundle.

- Add an OKF bundle/export workflow, tentatively:

```bash
okf-http bundle --visibility public --output ./site-okf-bundle
okf-http bundle --visibility internal --output ./intranet-okf-bundle
```

- Use the same named-root structure that the browser navigation already shows.
- Write every accepted source root into a named top-level directory.
- Preserve OKF-compatible subdirectory structure below each named root.
- Do not copy hidden files, rejected files, private files, unsupported file
  formats, local databases, `.env` files, credentials, editor state, or other
  files outside the active visibility boundary.
- Prevent path traversal and symlink escape during export.
- Detect and report naming collisions before writing the bundle.
- Use a stable logical URI scheme for bundled documents, for example:

```text
okf://<root-name>/<subdirectory>/<document>
```

- Preserve provenance metadata so a bundled document can still be traced back
  to its logical source root without exposing private absolute filesystem paths
  on a public server.
- Allow the site server to run from a bundle root instead of live workstation
  roots.

Completion criteria:

- A public bundle can be generated from several local OKF roots.
- The generated bundle contains only files allowed for public deployment.
- Bundle paths match the names and hierarchy shown in the OKF browser.
- `okf-http --site` can serve from the generated bundle without needing access
  to the original local source paths.

## Phase 2c: Deploy Bundles with rsync over SSH

OKF should avoid FTP-based deployment. Public and intranet deployments should
start with `rsync` over SSH because it is widely available, incremental,
scriptable, and compatible with ordinary server administration.

- Define the default server-side storage layout:

```text
/var/lib/okf/sites/<sitename>/
  releases/
    <release-id>/
  current -> releases/<release-id>
```

- Treat `<sitename>` as an explicit deployment name, not as an inferred local
  directory name.
- Upload generated OKF bundles to a new release directory with `rsync` over
  SSH.
- Do not rsync directly into the active `current` directory.
- After upload, validate the uploaded release on the server if possible.
- Switch `current` atomically only after the uploaded release is complete.
- Keep at least one previous release for rollback.
- Document the expected ownership and permissions for
  `/var/lib/okf/sites/<sitename>`.
- Make `okf-http --site` serve the `current` symlink:

```bash
okf-http --site --bundle /var/lib/okf/sites/<sitename>/current --visibility public 8080
```

- Document that SSH access, server users, and firewall rules are outside OKF's
  direct control but must be configured by the server administrator.

Completion criteria:

- A generated OKF bundle can be deployed to
  `/var/lib/okf/sites/<sitename>/releases/<release-id>` with `rsync` over SSH.
- The active deployment is selected through the `current` symlink.
- Rollback can be performed by repointing `current` to the previous release.
- No FTP-based deployment path is required or recommended.

## Phase 3: Add Immutable Public Site Mode

- Add a CLI mode, tentatively:

```bash
okf-http --site --visibility public --theme default 8080
```

- In immutable public site mode, mount only:
  - page rendering
  - static theme assets
  - public search/index JSON if enabled
  - health/status endpoints that do not reveal local paths
  - sitemap and feed endpoints
- Do not mount browser root management, mutation APIs, Voyage execution, or
  local-editor APIs.

Completion criteria:

- A public site can be served with no mutation routes present.
- Tests prove that known mutating paths return 404 or 410 in public site mode.

## Phase 3b: Add Protected Internal Site Mode

- Extend site mode for intranet deployments:

```bash
okf-http --site --visibility internal --theme default 8080
okf-http --site --visibility internal --trusted-proxy --enable-protected-editor 8080
```

- Internal mode should start read-only.
- Protected mutation routes may be mounted only behind explicit flags.
- Protected mutation routes should also require the trusted reverse-proxy
  deployment profile.
- Reuse existing OKF security mechanisms instead of inventing a weaker
  intranet-only path.
- Support reverse-proxy deployment with SSO or network-level access control,
  but keep OKF's own capability and confirmation checks active.

Completion criteria:

- Internal read-only mode behaves like public mode plus internal visibility.
- Internal protected mode can enable selected mutation workflows without
  bypassing authentication, CSRF, origin, capability, revision, digest, and
  confirmation-header checks.
- Internal protected mode cannot be enabled accidentally on an ordinary direct
  listener.

## Phase 4: Theme System

- Introduce a small theme contract for `okf-http` site mode.
- Start with:

```text
themes/
  default/
  wimux/
```

- A theme should provide:
  - base layout
  - document/page template
  - project/index template
  - navigation template
  - CSS
  - optional JavaScript
- Theme assets must be static and must not execute privileged OKF operations.

Completion criteria:

- `okf-http --site --theme wimux` can render the existing wimux.org visual
  structure without Django.

## Phase 5: Port the wimux.org Design

- Extract the current wimux.org design from Django templates and CSS.
- Convert it into an OKF theme.
- Preserve:
  - header/banner
  - language switcher
  - project navigation
  - project tiles or equivalent landing page
  - footer
  - responsive behavior
- Remove Django-specific template constructs.

Completion criteria:

- The OKF-rendered site visually matches the current wimux.org design closely
  enough to replace it.

## Phase 6: Language and Translation Strategy

- Move away from database-backed translation state for content pages.
- Treat translated OKF documents as normal versioned files.
- Support one of these patterns:

```text
website/de/okf.md
website/en/okf.md
```

or:

```text
okf.de.md
okf.en.md
```

- Define how language alternates are expressed in frontmatter.
- Evaluate Pootle, Weblate, PO files, or another translation workflow for
  file-based translation management.
- Automatic translation may remain a drafting tool, not the canonical
  production source.

Completion criteria:

- A public page can link to its translated counterpart without Django slugs.
- Translation status is represented in files, not hidden database state.

## Phase 7: Public and Internal Search and Indexing

- Decide which search mode is safe for public/intranet site mode.
- Public mode should avoid exposing private paths, internal drafts, hidden
  files, and rejected documents.
- A generated JSON search index may be sufficient for public wimux.org.
- Intranet mode may use richer indexes, but every result must still respect the
  active user's visibility and capability boundary.

Completion criteria:

- Search only returns documents allowed by the selected visibility and,
  where applicable, the authenticated user's capabilities.
- Search output reveals no local filesystem paths.

## Phase 8: NGINX Deployment Model

- Document a reverse-proxy deployment:

```nginx
server {
    server_name wimux.org www.wimux.org;

    location / {
        proxy_pass http://127.0.0.1:8080;
    }
}
```

- NGINX should own public TLS, compression, caching, and rate limits.
- `okf-http` should bind to loopback in production.
- The OKF source tree or generated OKF bundle should be deployed read-only
  where possible.
- Public deployments should prefer generated bundles over direct access to
  scattered workstation document roots.
- For intranet deployments, NGINX or another trusted reverse proxy may provide
  SSO, Basic Auth, client certificates, or network-level access control. OKF
  protected mutation routes should require this explicit reverse-proxy profile
  and still require OKF-side authorization checks.

Completion criteria:

- A documented public deployment can serve wimux.org through NGINX without
  exposing local editor routes.
- A documented internal deployment can expose protected routes only when the
  admin has explicitly enabled them behind the trusted reverse proxy profile.

## Phase 9: Service Installation Support

Reliable reverse-proxy operation requires `okf-http` to be managed by the
operating system instead of being started manually in a terminal.

- Extend `install.sh` to ask interactively whether `okf-http` should be
  installed as a service.
- If the user answers `Y` / `Yes`, ask which service mode should be installed:
  - public site mode
  - internal read-only mode
  - internal protected mode behind a trusted reverse proxy
  - local workbench mode
- Ask which TCP port `okf-http` should listen on.
- Check during installation whether the selected port appears to be available.
  This check is advisory because the port can become occupied after
  installation or before the service starts.
- If the selected port is already in use, show the conflicting listener when
  possible and ask the user to choose a different port or abort the service
  installation.
- Generate an explicit service command for the selected mode.
- Prefer a user service when root privileges are not available.
- Support a system service when the installer is allowed to use elevated
  privileges.
- Make the selected bind host, port, document roots, browser/theme path, and
  visibility explicit.
- Ask whether the service should serve live configured document roots or a
  generated OKF bundle. Public services should recommend the bundle path.
- For reverse-proxy deployments, bind `okf-http` to loopback unless the admin
  deliberately chooses otherwise.
- Refuse to install protected internal editing as a directly exposed service
  unless a later threat model explicitly allows that deployment.
- Print the matching NGINX reverse-proxy snippet after service installation.
- Tell the user that firewall rules may need to be adjusted separately,
  especially when `okf-http` is meant to be reachable from other machines or
  through a reverse proxy.
- Provide uninstall/disable instructions for the generated service.

Completion criteria:

- `install.sh` can install `okf-http` as a managed service.
- The installer records or prints the exact mode selected by the user.
- The installer records or prints the selected port and reports whether it was
  available at installation time.
- Public and internal reverse-proxy deployments can survive reboot without
  manual shell sessions.
- Protected internal editing can only be selected together with the trusted
  reverse-proxy profile.

## Phase 10: Binary Packaging and APT-Friendly Installation

OKF should not require users to install a Rust toolchain just to run
`okf-http`. Source builds and `cargo install` should remain available for
developers, but ordinary server and desktop users need prebuilt installation
paths.

- Build release binaries for common Linux targets, starting with:
  - `x86_64-unknown-linux-gnu`
  - `aarch64-unknown-linux-gnu`
- Publish release archives through GitHub Releases.
- Add Debian package generation for `okf-http`, initially as locally
  installable `.deb` packages.
- Package names may be user-facing names such as `okf-http`, even when the Rust
  library crate is named `okf-open-knowledge-format`.
- The Debian package should install files into standard locations, for example:

```text
/usr/bin/okf-http
/usr/share/doc/okf-http/
/usr/share/okf/browser/
/usr/share/okf/themes/
/usr/lib/systemd/user/okf-http.service.example
/usr/lib/systemd/system/okf-http.service.example
```

- Keep mutable deployment data outside the package, for example:

```text
/var/lib/okf/sites/<sitename>/
/etc/okf/
~/.config/okf/
```

- Make `install.sh` aware of whether it is running from:
  - a source checkout
  - a release archive
  - an installed package
- Document installation paths for:
  - `cargo install`
  - GitHub release archive
  - local `.deb` package installation
  - future APT repository installation
- Treat an official APT repository as a later step after `.deb` packages are
  stable.
- Document that APT/release installations do not require Rust.

Completion criteria:

- A user can install `okf-http` from a prebuilt release artifact without a Rust
  toolchain.
- A local `.deb` package can install `okf-http` into standard Linux paths.
- The package does not overwrite user content under `/var/lib/okf` or user
  configuration under `~/.config/okf`.
- The package includes enough service examples or installer integration to run
  `okf-http` reliably behind NGINX.

## Phase 11: Parallel Deployment

- Run Django and `okf-http --site` side by side on separate local ports.
- Compare:
  - rendered pages
  - navigation
  - links
  - language alternates
  - sitemap
  - headers
  - performance
  - broken links
- Keep Django as the public service until the OKF site reaches parity.

Completion criteria:

- The OKF-rendered wimux.org preview can be reviewed without changing the
  public DNS or production NGINX route.

## Phase 12: Cutover from Django to OKF

- Freeze Django content updates.
- Ensure all public content exists as OKF documents.
- Run final link and visibility checks.
- Change NGINX upstream from Django to `okf-http --site`.
- Keep a rollback path to Django for the first deployment window.

Completion criteria:

- wimux.org is served by `okf-http --site` behind NGINX.
- Django is no longer required for public project/documentation pages.

## Phase 13: Retire or Repurpose Django

After cutover, decide whether Django remains useful for:

- news publishing
- contact forms
- admin-only workflows
- legacy redirects
- translation tooling

If not, remove it from the deployment. If yes, keep it behind separate routes
and do not use it as the canonical source for OKF content.

Completion criteria:

- The role of Django is explicit and no longer confused with the OKF content
  source of truth.

## Current Transitional State

The current wimux.org Django sync command imports curated OKF website documents
into `Project` and `SubPage` records. It is intentionally a bridge:

```bash
python manage.py sync_okf_content
python manage.py sync_okf_content --apply
```

This bridge should remain useful until `okf-http --site` can render the same
content directly.

## Open Questions

- Should the CLI name be `--site`, `--publish`, or `--public-site`?
- Should visibility use `visibility: public/internal/private`, `public: true`,
  or both during a compatibility period?
- Should theme files be embedded into the binary, read from disk, or support
  both?
- Should public search be generated at startup or served from a prebuilt index?
- Should wimux.org keep a small Django service for news/contact after the
  content cutover?
