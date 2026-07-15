# Open Knowledge Format

Open Knowledge Format (OKF) is a practical format and toolchain for turning
ordinary knowledge folders into structured, searchable, and reviewable
knowledge repositories.

Many people already write useful knowledge in Markdown files, research notes,
project documentation, CSV tables, and small folder trees. The hard part begins
later: documents move, similar names collide, links become implicit, metadata
is missing, and semantic relationships are difficult to discover without
locking the work into one application. OKF is meant to solve that layer.

This project treats OKF primarily as a knowledge-networking format. It is a way
to connect Markdown documents, frontmatter metadata, stable identities,
relations, semantic search, and reviewed AI suggestions without making one
database, web service, or model provider the owner of the knowledge.

The format keeps human-readable Markdown at the center, but adds enough
structure for software to understand a repository safely:

- document roots can come from different folders without merging them
  physically
- every normal document can declare its `type` in frontmatter
- root and document identities can stay stable even when paths change
- hidden, unsafe, or unsupported files can be rejected before they become part
  of the knowledge base
- semantic analysis can suggest relationships, while canonical changes remain
  reviewable and explicit

This implementation provides OKF as a Rust library plus a local browser server.
The library reads and validates filesystem-backed OKF repositories. The HTTP
server serves a local browser, manages document roots through a protected
workflow, and can optionally connect derived semantic-analysis data such as
Voyage AI embeddings. Reading is local and token-free by default; editing,
configuration changes, and paid AI calls must be started explicitly.

OKF can help AI agents receive better context and produce better work, but it
is not itself an agent framework. The durable layer is the knowledge network:
plain files, explicit metadata, reviewable relations, and rebuildable derived
indexes. Agents, browsers, search tools, and host applications can all build
on that shared layer.

This repository contains the independently versioned OKF workspace.

- [`okf`](crates/okf/README.md) is the filesystem-backed document model and
  repository library. Its crates.io package name is
  `okf-open-knowledge-format`; the Rust library crate is still imported as
  `okf`.
- [`okf-http`](crates/okf-http/README.md) provides the local HTTP server,
  browser, protected editing workflows, and optional semantic-analysis APIs.
- [`docs-browser`](docs-browser/README.md) contains the development copy of
  the browser assets embedded in `okf-http` releases.

The complete knowledge documentation starts at
[`crates/okf/docs/knowledge/index.md`](crates/okf/docs/knowledge/index.md).
For the conceptual positioning, start with
[`OKF as a Knowledge Network`](crates/okf/docs/knowledge/concepts/knowledge-networking.md).

## Development

The workspace requires Rust 1.85 or newer.

```bash
cargo test --workspace --locked
scripts/check-okf-release.sh
```

For a local development server:

```bash
cargo run -p okf-http -- --browser-root docs-browser 8003
```

Then open `http://127.0.0.1:8003/docs-browser/index.html`.

## Installation

From crates.io:

```bash
cargo install okf-http --locked
okf-http --install-browser
okf-http 8003
```

Then open `http://127.0.0.1:8003/docs-browser/index.html`.

From a GitHub source checkout, run the installer as your regular user:

```bash
./install.sh --release
```

It builds and installs `/usr/local/bin/okf-http` with sudo and provisions the
packaged browser for the current user without sudo. After this installer path,
you normally do not need to run `okf-http --install-browser` separately. Use
`--prefix`, `--no-browser`, or `--force-browser` when required;
`./install.sh --help` documents every option.

If you install only the binary manually, provision or refresh the browser
assets with:

```bash
okf-http --install-browser
```

For the one-time migration from the former scanlab-owned installation, use:

```bash
./install.sh --release --remove-legacy-scanlab --force-browser
```

The explicit migration flag removes the old `/usr/local/bin/okf-http` and
`/usr/local/share/scanlab/component-docs/okf` before installing the standalone
binary. It is never performed implicitly.

The project is licensed under Apache License 2.0. See [LICENSE](LICENSE).
