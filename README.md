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

The project is licensed under Apache License 2.0. See [LICENSE](LICENSE).

