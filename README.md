# Open Knowledge Format

This repository contains the independently versioned Open Knowledge Format
workspace.

- [`okf`](crates/okf/README.md) is the filesystem-backed document model and
  repository library.
- [`okf-http`](crates/okf-http/README.md) provides the local HTTP server,
  browser, protected editing workflows, and optional semantic-analysis APIs.
- [`docs-browser`](docs-browser/README.md) contains the development copy of
  the browser assets embedded in `okf-http` releases.

The complete knowledge documentation starts at
[`crates/okf/docs/knowledge/index.md`](crates/okf/docs/knowledge/index.md).

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

Run the installer as your regular user:

```bash
./install.sh --release
```

It builds and installs `/usr/local/bin/okf-http` with sudo and provisions the
packaged browser for the current user without sudo. Use `--prefix`,
`--no-browser`, or `--force-browser` when required; `./install.sh --help`
documents every option.

For the one-time migration from the former scanlab-owned installation, use:

```bash
./install.sh --release --remove-legacy-scanlab --force-browser
```

The explicit migration flag removes the old `/usr/local/bin/okf-http` and
`/usr/local/share/scanlab/component-docs/okf` before installing the standalone
binary. It is never performed implicitly.

The project is licensed under Apache License 2.0. See [LICENSE](LICENSE).
