# OKF

`okf` is a filesystem-backed knowledge-document library for Markdown
repositories. The crates.io package name is `okf-open-knowledge-format`; the
Rust library crate is still named `okf`, so downstream code can continue to
write `use okf::...`.

Add it to a Rust project with the package name and import it through the
library crate name:

```toml
[dependencies]
okf = { package = "okf-open-knowledge-format", version = "0.3" }
```

```rust
use okf::{DocumentRoot, Repository};
```

It provides:

- ordered physical document roots
- optional logical mount names for independent document namespaces
- recursive Markdown discovery
- YAML-like frontmatter extraction
- document title, type, compatibility kind, topic, status, and update metadata
- planning-section extraction
- exact and partial document lookup
- diagnostics for missing roots, missing document types, and shadowed documents
- optional Voyage AI analysis helpers for explicit embedding/index workflows
- a secure, write-free Markdown/CSV admission inventory with structured
  rejection reasons and bounded scanning
- deterministic, snapshot-bound root proposals with expiring revalidation
- revision-bound root registration and journaled, recoverable multi-file
  initialization

OKF does not own application paths. A host application selects the roots and
passes them to `Repository`.

```rust
# fn open() -> Result<Repository, okf::OkfError> {
Repository::open([
    DocumentRoot::mounted("product", "docs/knowledge"),
    DocumentRoot::mounted("library", "crates/library/docs/knowledge"),
])
# }
```

Mounted roots separate logical document identities from physical filesystem
layout. In the example, two files named `index.md` become
`product/index.md` and `library/index.md`.

The complete documentation starts at
[`docs/knowledge/index.md`](docs/knowledge/index.md).

Portable onboarding identities are distinct from filesystem paths and mount
names. `RootId` reads `okf_root_id` from the bundle-root `index.md`,
`DocumentId` reads `okf_document_id` from concept frontmatter, and
`PortablePath` preserves the exact UTF-8 display path while exposing a separate
Unicode-aware collision key. Existing repositories without IDs remain readable.

New users should begin with
[Getting Started with Standalone OKF](docs/knowledge/getting-started.md).
See [CONTRIBUTING.md](CONTRIBUTING.md) for development rules and
[SECURITY.md](SECURITY.md) for private vulnerability reporting.
Release history and planned compatibility milestones are documented in
[CHANGELOG.md](CHANGELOG.md) and the
[versioning roadmap](docs/knowledge/plans/versioning-roadmap.md).

## Browser and HTTP Workflow

The OKF browser is served by the separate `okf-http` binary.

From crates.io:

```bash
cargo install okf-http --locked
okf-http --install-browser
okf-http 8003
```

Then open:

```text
http://127.0.0.1:8003/docs-browser/index.html
```

In a repository development checkout, start it with:

```bash
cargo run -p okf-http -- --browser-root docs-browser 8003
```

Then open:

```text
http://127.0.0.1:8003/docs-browser/index.html
```

For normal user installations, browser assets are expected below
`~/docs-browser` unless `OKF_BROWSER_ROOT` or `--browser-root` points
elsewhere. Browser-managed roots come from the versioned XDG
`okf/config.toml`; `OKF_DOCUMENT_ROOTS` and optional `--root` arguments remain
higher-precedence operator controls. Browser assets and document roots are
connected only through explicit `okf-http` routes, not by serving a repository
root wholesale.

Hosts can build separate registration and source-initialization previews with
`build_root_proposal`. Their review trees combine secure admission, compliance
changes, resource state, identity and shadowing conflicts, trusted limits,
writeability, and override effects. `RootProposalStore` retains proposals only
as expiring derived process state and re-scans before later confirmation; no
preview writes files or invokes Voyage AI.

After explicit confirmation, hosts can apply registration through an atomic
configuration revision or apply a separately approved initialization plan.
Initialization preserves unrelated frontmatter, source permissions and line
endings, stages rollback material outside the root, and recovers interrupted
multi-file operations from a synced journal.

## Optional Voyage AI Analysis

`okf::voyage` provides an optional analysis layer for hosts that want semantic
search or AI-suggested graph edges. It is explicit and derived-data oriented:

- normal repository loading never calls Voyage AI
- `OKF_VOYAGE_*` environment variables configure the optional layer
- `.okf-voyage/okf.sqlite` is treated as rebuildable local index/work data
- SQLite is authoritative derived state; generation-marked TSV snapshots are
  a recoverable compatibility mirror
- `okf-http` exposes read-only integrity status and a token-free rebuild action
- live Voyage tests are ignored by default
- suggested edges are marked as AI-derived until reviewed
- accepted AI-derived relations are written as structured frontmatter metadata
  only through an explicit host action

The crate has no dependency on scanlab or SCQL.
