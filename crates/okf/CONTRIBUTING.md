# Contributing to OKF

Thank you for helping make OKF useful outside scanlab. Contributions should
preserve the central boundary: canonical knowledge is portable Markdown plus
frontmatter; indexes, vectors, review state, and provider operations are
optional derived layers.

## Before Opening a Change

- Use English for new documentation. Existing German documents may continue to
  be maintained in German.
- Discuss large format, API, security, storage, or dependency changes before
  implementation.
- Do not commit `.env`, API keys, `.okf-voyage/`, SQLite/WAL files, session
  tokens, editor lock files, generated browser installations, or private local
  paths.
- Report vulnerabilities according to [SECURITY.md](SECURITY.md), not in a
  public issue.

## Development Setup

From the repository root:

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
```

Ordinary tests must be offline and must not spend Voyage tokens. The live
Voyage test remains ignored by default and may be run only with the contributor's
own API key and explicit acceptance of provider cost.

Check browser JavaScript after frontend changes:

```bash
node --check crates/okf-http/browser/app.js
```

Keep the packaged browser source and repository development copy synchronized:

```bash
cmp crates/okf-http/browser/app.js docs-browser/app.js
cmp crates/okf-http/browser/index.html docs-browser/index.html
cmp crates/okf-http/browser/security.js docs-browser/security.js
cmp crates/okf-http/browser/styles.css docs-browser/styles.css
```

Before a community release, run the consolidated offline gate after fetching
locked dependencies once:

```bash
cargo fetch --locked
env -u OKF_VOYAGE_API_KEY -u VOYAGE_API_KEY scripts/check-okf-release.sh
```

The complete procedure is in the
[OKF community release checklist](docs/knowledge/references/release-checklist.md).

## Documentation and Format Changes

Every normal document below `docs/knowledge` needs frontmatter with a non-empty
`type`. `index.md` and `log.md` are reserved navigation/history exceptions.
Unknown document types and metadata must remain usable.

Changes to canonical `relations`, public Rust APIs, `/api/v1`, CLI precedence,
mount semantics, security controls, or SQLite migrations require focused
contract tests and corresponding documentation updates.

## Pull Requests

A focused change should explain:

- the user-visible problem and resulting behavior
- compatibility or migration impact
- security and token-cost impact
- tests run
- documentation changed

Keep unrelated refactors separate. Preserve existing user work in Markdown and
frontmatter, and prefer deterministic fixtures over network-dependent tests.

Contributions are licensed under Apache-2.0 as described in [LICENSE](LICENSE).
