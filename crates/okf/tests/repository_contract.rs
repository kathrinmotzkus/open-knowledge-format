use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use okf::{
    Diagnostic, DocumentKind, DocumentQuery, DocumentRoot, OkfError, PlanningHeadings, Repository,
    RepositoryOptions,
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new(label: &str) -> Self {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("okf-contract-{}-{label}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("create test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, content: impl AsRef<[u8]>) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create document parent");
        }
        fs::write(path, content).expect("write test document");
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn reads_complete_frontmatter_and_preserves_additional_fields() {
    let root = TestDirectory::new("frontmatter");
    root.write(
        "document.md",
        "---\ntitle: Complete\ntype: Reference\nkind: knowledge-document\ntopic: testing\n\
         status: active\nupdated: 2026-06-22\ncustom: retained\n---\n# Ignored\n",
    );

    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");
    let document = &repository.documents()[0];

    assert_eq!(document.title(), "Complete");
    assert_eq!(
        document.document_type().map(DocumentKind::as_str),
        Some("Reference")
    );
    assert_eq!(
        document.kind().map(DocumentKind::as_str),
        Some("knowledge-document")
    );
    assert_eq!(document.topic(), Some("testing"));
    assert_eq!(document.status(), Some("active"));
    assert_eq!(document.updated(), Some("2026-06-22"));
    assert_eq!(
        document.frontmatter().get("custom").map(String::as_str),
        Some("retained")
    );
}

#[test]
fn bundled_knowledge_documents_have_required_type_metadata() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/knowledge");
    let repository = Repository::open([DocumentRoot::new(&root)]).expect("bundled repository");
    let missing = repository
        .diagnostics()
        .iter()
        .filter_map(|diagnostic| match diagnostic {
            Diagnostic::MissingDocumentType { relative_path, .. } => Some(relative_path),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "documents missing type metadata: {missing:?}"
    );
    let reserved = repository
        .documents()
        .iter()
        .filter(|document| {
            document.relative_path().file_name().is_some_and(|name| {
                name == std::ffi::OsStr::new("index.md") || name == std::ffi::OsStr::new("log.md")
            })
        })
        .collect::<Vec<_>>();
    assert!(
        !reserved.is_empty(),
        "expected bundled documentation to contain reserved navigation documents"
    );
}

#[test]
fn reads_multiline_canonical_relations_with_ai_provenance() {
    let root = TestDirectory::new("canonical-relations");
    root.write(
        "source.md",
        "---\r\ntitle: Source\r\ntype: Concept\r\ncustom: retained\r\nrelations: [\r\n  {\r\n    \"type\": \"ai_suggested_edge\",\r\n    \"target\": \"target.md\",\r\n    \"suggestion_id\": \"suggestion-1\",\r\n    \"source_chunk\": \"source.md#1-a\",\r\n    \"target_chunk\": \"target.md#1-b\",\r\n    \"provider\": \"voyage-ai\",\r\n    \"model\": \"voyage-test\",\r\n    \"generation_method\": \"embedding_similarity\",\r\n    \"ai_generated\": true,\r\n    \"score\": 0.91,\r\n    \"created_at\": \"123456\",\r\n    \"status\": \"accepted\"\r\n  }\r\n]\r\n---\r\n# Source\r\n",
    );
    root.write(
        "target.md",
        "---\ntitle: Target\ntype: Concept\n---\n# Target\n",
    );

    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");
    let source = repository
        .find(DocumentQuery::Exact("source.md".to_string()))
        .expect("source document");
    let relation = source.relations().first().expect("canonical relation");

    assert_eq!(
        source.frontmatter().get("custom").map(String::as_str),
        Some("retained")
    );
    assert_eq!(relation.relation_type(), "ai_suggested_edge");
    assert_eq!(relation.target(), Path::new("target.md"));
    assert_eq!(relation.suggestion_id(), Some("suggestion-1"));
    assert_eq!(relation.provider(), Some("voyage-ai"));
    assert_eq!(relation.model(), Some("voyage-test"));
    assert_eq!(relation.generation_method(), Some("embedding_similarity"));
    assert!(relation.ai_generated());
    assert_eq!(relation.score(), Some("0.91"));
    assert_eq!(relation.status(), Some("accepted"));
}

#[test]
fn accepts_documents_without_frontmatter_and_uses_h1_title() {
    let root = TestDirectory::new("h1");
    root.write("guide.md", "# Human title\n\nBody\n");

    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");
    let document = &repository.documents()[0];

    assert_eq!(document.title(), "Human title");
    assert!(document.frontmatter().is_empty());
    assert_eq!(document.relative_path(), Path::new("guide.md"));
}

#[test]
fn reports_missing_document_type_without_rejecting_the_document() {
    let root = TestDirectory::new("missing-type");
    root.write(
        "guide.md",
        "---\ntitle: Guide\nkind: legacy\n---\n# Guide\n",
    );

    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");

    assert_eq!(repository.documents().len(), 1);
    assert_eq!(repository.documents()[0].title(), "Guide");
    assert!(repository.documents()[0].document_type().is_none());
    assert!(matches!(
        repository.diagnostics(),
        [Diagnostic::MissingDocumentType {
            relative_path,
            root: diagnostic_root,
        }] if relative_path == Path::new("guide.md")
            && diagnostic_root == root.path()
    ));
}

#[test]
fn uses_filename_when_frontmatter_and_h1_are_absent() {
    let root = TestDirectory::new("filename");
    root.write("fallback-name.md", "Body without a heading\n");

    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");

    assert_eq!(repository.documents()[0].title(), "fallback-name");
}

#[test]
fn recursively_discovers_markdown_and_ignores_other_files() {
    let root = TestDirectory::new("recursive");
    root.write("top.md", "# Top\n");
    root.write("nested/deeper/document.md", "# Nested\n");
    root.write("nested/deeper/ignored.txt", "# Not Markdown\n");

    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");
    let paths = repository
        .documents()
        .iter()
        .map(|document| document.relative_path().to_path_buf())
        .collect::<Vec<_>>();

    assert_eq!(
        paths,
        [
            PathBuf::from("nested/deeper/document.md"),
            PathBuf::from("top.md")
        ]
    );
}

#[test]
fn loads_multiple_roots_in_priority_order() {
    let first = TestDirectory::new("multiple-first");
    let second = TestDirectory::new("multiple-second");
    first.write("first.md", "# First\n");
    second.write("second.md", "# Second\n");

    let repository = Repository::open([
        DocumentRoot::new(first.path()),
        DocumentRoot::new(second.path()),
    ])
    .expect("repository");

    assert_eq!(repository.roots().len(), 2);
    assert_eq!(repository.documents()[0].root(), first.path());
    assert_eq!(repository.documents()[1].root(), second.path());
}

#[test]
fn mounted_roots_keep_equal_relative_paths_in_separate_namespaces() {
    let first = TestDirectory::new("mounted-first");
    let second = TestDirectory::new("mounted-second");
    fs::write(first.path().join("index.md"), "# SCQL\n").expect("SCQL index");
    fs::write(second.path().join("index.md"), "# OKF\n").expect("OKF index");

    let repository = Repository::open([
        DocumentRoot::mounted("scql", first.path()),
        DocumentRoot::mounted("okf", second.path()),
    ])
    .expect("mounted repository");

    assert_eq!(
        repository
            .documents()
            .iter()
            .map(|document| document.relative_path().to_path_buf())
            .collect::<Vec<_>>(),
        [
            PathBuf::from("scql/index.md"),
            PathBuf::from("okf/index.md")
        ]
    );
    assert!(repository.diagnostics().is_empty());
}

#[test]
fn roots_with_the_same_mount_still_shadow_by_priority() {
    let first = TestDirectory::new("mounted-shadow-first");
    let second = TestDirectory::new("mounted-shadow-second");
    fs::write(
        first.path().join("guide.md"),
        "---\ntype: Guide\n---\n# First\n",
    )
    .expect("first guide");
    fs::write(
        second.path().join("guide.md"),
        "---\ntype: Guide\n---\n# Second\n",
    )
    .expect("second guide");

    let repository = Repository::open([
        DocumentRoot::mounted("product", first.path()),
        DocumentRoot::mounted("product", second.path()),
    ])
    .expect("mounted repository");

    assert_eq!(repository.documents().len(), 1);
    assert_eq!(
        repository.documents()[0].relative_path(),
        Path::new("product/guide.md")
    );
    assert_eq!(repository.documents()[0].title(), "First");
    assert!(matches!(
        repository.diagnostics(),
        [Diagnostic::ShadowedDocument { relative_path, .. }]
            if relative_path == Path::new("product/guide.md")
    ));
}

#[test]
fn earlier_root_wins_relative_path_collisions() {
    let first = TestDirectory::new("collision-first");
    let second = TestDirectory::new("collision-second");
    first.write("same.md", "---\ntype: Reference\n---\n# Selected\n");
    second.write("same.md", "---\ntype: Reference\n---\n# Shadowed\n");

    let repository = Repository::open([
        DocumentRoot::new(first.path()),
        DocumentRoot::new(second.path()),
    ])
    .expect("repository");

    assert_eq!(repository.documents().len(), 1);
    assert_eq!(repository.documents()[0].title(), "Selected");
    assert!(matches!(
        repository.diagnostics(),
        [Diagnostic::ShadowedDocument {
            relative_path,
            selected_root,
            shadowed_root,
        }] if relative_path == Path::new("same.md")
            && selected_root == first.path()
            && shadowed_root == second.path()
    ));
}

#[test]
fn missing_optional_root_is_reported_but_does_not_block_loading() {
    let root = TestDirectory::new("missing-optional");
    root.write("available.md", "---\ntype: Reference\n---\n# Available\n");
    let missing = root.path().join("missing");

    let repository =
        Repository::open([DocumentRoot::new(&missing), DocumentRoot::new(root.path())])
            .expect("repository");

    assert_eq!(repository.documents().len(), 1);
    assert!(matches!(
        repository.diagnostics(),
        [Diagnostic::MissingRoot { root }] if root == &missing
    ));
}

#[test]
fn rejects_empty_or_entirely_unusable_root_lists() {
    assert!(matches!(
        Repository::open(Vec::<DocumentRoot>::new()),
        Err(OkfError::NoRoots)
    ));

    let root = TestDirectory::new("unusable");
    assert!(matches!(
        Repository::open([DocumentRoot::new(root.path().join("missing"))]),
        Err(OkfError::NoUsableRoots)
    ));
}

#[test]
fn extracts_english_and_german_planning_sections() {
    let root = TestDirectory::new("planning-languages");
    root.write(
        "english.md",
        "# English\n\n## Completed Decisions\n- done\n\
         ## Active Open Questions\n- open\n## Deferred / Later\n- later\n",
    );
    root.write(
        "german.md",
        "# German\n\n## Erledigte Grundsatzentscheidungen\n- erledigt\n\
         ## Aktiv offen\n- offen\n## Später / bewusst verschoben\n- spaeter\n",
    );

    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");
    let english = repository
        .find(DocumentQuery::Exact("english".to_string()))
        .expect("English");
    let german = repository
        .find(DocumentQuery::Exact("german".to_string()))
        .expect("German");

    assert_eq!(english.planning().completed, ["- done"]);
    assert_eq!(english.planning().open, ["- open"]);
    assert_eq!(english.planning().deferred, ["- later"]);
    assert_eq!(german.planning().completed, ["- erledigt"]);
    assert_eq!(german.planning().open, ["- offen"]);
    assert_eq!(german.planning().deferred, ["- spaeter"]);
}

#[test]
fn supports_custom_planning_headings() {
    let root = TestDirectory::new("custom-headings");
    root.write(
        "custom.md",
        "---\ntype: Plan\n---\n# Custom\n\n## Finished\n- done\n## Questions\n- open\n## Parking Lot\n- later\n",
    );
    let options = RepositoryOptions {
        planning_headings: PlanningHeadings {
            completed: vec!["Finished".to_string()],
            open: vec!["Questions".to_string()],
            deferred: vec!["Parking Lot".to_string()],
        },
        ..RepositoryOptions::default()
    };

    let repository = Repository::open_with_options([DocumentRoot::new(root.path())], options)
        .expect("repository");
    let planning = repository.documents()[0].planning();

    assert_eq!(planning.completed, ["- done"]);
    assert_eq!(planning.open, ["- open"]);
    assert_eq!(planning.deferred, ["- later"]);
}

#[test]
fn classifies_plans_by_kind_and_configured_directory() {
    let root = TestDirectory::new("plans");
    root.write(
        "by-kind.md",
        "---\ntype: Plan\nkind: knowledge-plan\n---\n# Kind Plan\n",
    );
    root.write(
        "plans/by-directory.md",
        "---\ntype: Plan\n---\n# Directory Plan\n",
    );
    root.write("ordinary.md", "---\ntype: Reference\n---\n# Ordinary\n");

    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");
    let plans = repository
        .plans()
        .map(|document| document.title())
        .collect::<Vec<_>>();

    assert_eq!(plans, ["Kind Plan", "Directory Plan"]);
    assert!(!repository
        .find(DocumentQuery::Exact("ordinary".to_string()))
        .expect("ordinary")
        .is_plan());
}

#[test]
fn supports_exact_and_partial_document_resolution() {
    let root = TestDirectory::new("queries");
    root.write(
        "nested/architecture.md",
        "---\ntitle: System Architecture\ntopic: internals\n---\n# Ignored\n",
    );
    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");

    assert_eq!(
        repository
            .find(DocumentQuery::Exact("nested/architecture.md".to_string()))
            .expect("path")
            .title(),
        "System Architecture"
    );
    assert_eq!(
        repository
            .find(DocumentQuery::Exact("architecture".to_string()))
            .expect("stem")
            .title(),
        "System Architecture"
    );
    assert_eq!(
        repository
            .find(DocumentQuery::Exact("System Architecture".to_string()))
            .expect("title")
            .title(),
        "System Architecture"
    );
    assert_eq!(
        repository
            .find(DocumentQuery::Partial("internal".to_string()))
            .expect("topic")
            .title(),
        "System Architecture"
    );
}

#[test]
fn reports_empty_unknown_and_ambiguous_queries() {
    let root = TestDirectory::new("query-errors");
    root.write("one.md", "---\ntitle: Shared Alpha\n---\n# One\n");
    root.write("two.md", "---\ntitle: Shared Beta\n---\n# Two\n");
    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");

    assert!(matches!(
        repository.find(DocumentQuery::Exact("  ".to_string())),
        Err(OkfError::EmptyQuery)
    ));
    assert!(matches!(
        repository.find(DocumentQuery::Exact("missing".to_string())),
        Err(OkfError::NotFound { query }) if query == "missing"
    ));
    assert!(matches!(
        repository.find(DocumentQuery::Partial("Shared".to_string())),
        Err(OkfError::Ambiguous { matches, .. }) if matches.len() == 2
    ));
}

#[test]
fn rejects_invalid_utf8_documents() {
    let root = TestDirectory::new("invalid-utf8");
    root.write("invalid.md", [0xff, 0xfe, 0xfd]);

    assert!(matches!(
        Repository::open([DocumentRoot::new(root.path())]),
        Err(OkfError::InvalidUtf8 { path }) if path.ends_with("invalid.md")
    ));
}

#[cfg(unix)]
#[test]
fn reports_document_read_errors() {
    use std::os::unix::fs::symlink;

    let root = TestDirectory::new("read-error");
    symlink(
        root.path().join("missing-target"),
        root.path().join("broken.md"),
    )
    .expect("broken document symlink");

    assert!(matches!(
        Repository::open([DocumentRoot::new(root.path())]),
        Err(OkfError::ReadDocument { path, .. }) if path.ends_with("broken.md")
    ));
}
