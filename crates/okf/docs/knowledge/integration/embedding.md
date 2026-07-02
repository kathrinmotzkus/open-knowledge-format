---
title: Embedding OKF
type: Guide
kind: knowledge-document
topic: okf
status: active
updated: 2026-06-23
---

# Embedding OKF

OKF is a library, not a command-line application. The embedding host owns
filesystem policy and presentation.

```rust
use okf::{DocumentQuery, DocumentRoot, Repository};

# fn example() -> Result<(), okf::OkfError> {
let repository = Repository::open([
    DocumentRoot::mounted("application", "docs/knowledge"),
    DocumentRoot::mounted("component", "crates/component/docs/knowledge"),
])?;

let document = repository.find(DocumentQuery::Partial("document roots".into()))?;
println!("{}: {}", document.relative_path().display(), document.title());
# Ok(())
# }
```

## Configuration Policy

Environment variables, `.env` files, command-line arguments, installation
prefixes, and fallback directories belong to the host application.

OKF deliberately does not know paths such as:

- `docs/`
- `/usr/local/share/...`
- a particular project's crate layout

## Integration with Query Layers

Other crates may adapt OKF documents to richer query systems. The SCQL crate,
for example, provides an OKF adapter without making OKF depend on SCQL.

The dependency direction is:

```text
host application -> query adapter -> okf
```

OKF remains usable directly without a query language or terminal frontend.

## Optional AI Analysis

Hosts that want semantic search can opt into `okf::voyage`. This is separate
from normal OKF embedding in an application:

- OKF documents remain the source of truth.
- Voyage AI configuration uses `OKF_VOYAGE_*`.
- Derived embeddings live outside canonical Markdown documents.
- Live API checks are explicit and opt-in.

See [Voyage AI analysis layer](voyage-ai-analysis.md) and the
[Voyage AI integration plan](../plans/voyage-ai-integration-plan.md).
