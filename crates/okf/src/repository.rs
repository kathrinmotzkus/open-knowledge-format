use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::discovery;
use crate::frontmatter;
use crate::planning;
use crate::query;
use crate::{
    CanonicalRelation, DeclaredResource, Diagnostic, Document, DocumentId, DocumentKind,
    DocumentQuery, DocumentRoot, OkfError, RepositoryOptions, RootId,
};

#[derive(Debug)]
pub struct Repository {
    roots: Vec<DocumentRoot>,
    options: RepositoryOptions,
    documents: Vec<Document>,
    diagnostics: Vec<Diagnostic>,
}

impl Repository {
    pub fn open(roots: impl IntoIterator<Item = DocumentRoot>) -> Result<Self, OkfError> {
        Self::open_with_options(roots, RepositoryOptions::default())
    }

    pub fn open_with_options(
        roots: impl IntoIterator<Item = DocumentRoot>,
        options: RepositoryOptions,
    ) -> Result<Self, OkfError> {
        let roots = roots.into_iter().collect::<Vec<_>>();
        if roots.is_empty() {
            return Err(OkfError::NoRoots);
        }

        let mut documents = Vec::new();
        let mut diagnostics = Vec::new();
        let mut selected_paths = BTreeMap::<PathBuf, PathBuf>::new();
        let mut usable_roots = 0usize;

        for root in &roots {
            if !root.path().is_dir() {
                diagnostics.push(Diagnostic::MissingRoot {
                    root: root.path().to_path_buf(),
                });
                continue;
            }
            usable_roots += 1;
            let root_id = parse_root_id(root.path());
            for path in discovery::markdown_files(root.path())? {
                let source_relative_path = path
                    .strip_prefix(root.path())
                    .unwrap_or(&path)
                    .to_path_buf();
                let relative_path = root.logical_path(&source_relative_path);
                if let Some(selected_root) = selected_paths.get(&relative_path) {
                    diagnostics.push(Diagnostic::ShadowedDocument {
                        relative_path,
                        selected_root: selected_root.clone(),
                        shadowed_root: root.path().to_path_buf(),
                    });
                    continue;
                }
                let document = parse_document(
                    root.path(),
                    root_id.clone(),
                    &source_relative_path,
                    &relative_path,
                    &path,
                    &options,
                )?;
                if !is_reserved_document_path(&relative_path) && document.document_type().is_none()
                {
                    diagnostics.push(Diagnostic::MissingDocumentType {
                        relative_path: relative_path.clone(),
                        root: root.path().to_path_buf(),
                    });
                }
                selected_paths.insert(relative_path, root.path().to_path_buf());
                documents.push(document);
            }
        }

        if usable_roots == 0 {
            return Err(OkfError::NoUsableRoots);
        }

        documents.sort_by(|left, right| {
            root_priority(&roots, left.root())
                .cmp(&root_priority(&roots, right.root()))
                .then_with(|| left.relative_path().cmp(right.relative_path()))
        });

        Ok(Self {
            roots,
            options,
            documents,
            diagnostics,
        })
    }

    pub fn roots(&self) -> &[DocumentRoot] {
        &self.roots
    }

    pub fn options(&self) -> &RepositoryOptions {
        &self.options
    }

    pub fn documents(&self) -> &[Document] {
        &self.documents
    }

    pub fn plans(&self) -> impl Iterator<Item = &Document> {
        self.documents.iter().filter(|document| document.is_plan())
    }

    pub fn find(&self, query: DocumentQuery) -> Result<&Document, OkfError> {
        let (query, partial) = query.into_parts();
        let query = query.trim().trim_matches('"').to_string();
        if query.is_empty() {
            return Err(OkfError::EmptyQuery);
        }
        let matches = self
            .documents
            .iter()
            .filter(|document| query::matches(document, &query, partial))
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [document] => Ok(*document),
            [] => Err(OkfError::NotFound { query }),
            _ => Err(OkfError::Ambiguous {
                query,
                matches: matches
                    .iter()
                    .map(|document| document.relative_path().to_path_buf())
                    .collect(),
            }),
        }
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

fn parse_document(
    root: &Path,
    root_id: Option<RootId>,
    source_relative_path: &Path,
    relative_path: &Path,
    path: &Path,
    options: &RepositoryOptions,
) -> Result<Document, OkfError> {
    let bytes = fs::read(path).map_err(|source| OkfError::ReadDocument {
        path: path.to_path_buf(),
        source,
    })?;
    let source =
        String::from_utf8(bytes).map_err(|_| OkfError::InvalidUtf8 { path: path.into() })?;
    let (frontmatter, body) = frontmatter::parse(&source);
    let title = frontmatter
        .get("title")
        .cloned()
        .or_else(|| frontmatter::first_h1(body))
        .unwrap_or_else(|| {
            relative_path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("document")
                .to_string()
        });
    let document_type = frontmatter.get("type").cloned().map(DocumentKind::new);
    let kind = frontmatter.get("kind").cloned().map(DocumentKind::new);
    let document_id = frontmatter
        .get("okf_document_id")
        .and_then(|value| DocumentId::parse(value.clone()).ok());
    let relations = frontmatter
        .get("relations")
        .and_then(|value| serde_json::from_str::<Vec<serde_json::Value>>(value).ok())
        .unwrap_or_default()
        .iter()
        .filter_map(CanonicalRelation::from_json)
        .collect();
    let resources = frontmatter
        .get("resources")
        .and_then(|value| serde_json::from_str::<Vec<serde_json::Value>>(value).ok())
        .unwrap_or_default()
        .iter()
        .filter_map(DeclaredResource::from_json)
        .collect();
    let planning = planning::extract(body, &options.planning_headings);
    let is_plan = kind
        .as_ref()
        .is_some_and(|kind| options.plan_kinds.contains(kind))
        || is_in_plan_directory(relative_path, &options.plan_directories);

    Ok(Document {
        root: root.to_path_buf(),
        root_id,
        document_id,
        source_relative_path: source_relative_path.to_path_buf(),
        relative_path: relative_path.to_path_buf(),
        title,
        document_type,
        kind,
        frontmatter,
        planning,
        relations,
        resources,
        is_plan,
    })
}

fn parse_root_id(root: &Path) -> Option<RootId> {
    let source = fs::read_to_string(root.join("index.md")).ok()?;
    let (frontmatter, _) = frontmatter::parse(&source);
    RootId::parse(frontmatter.get("okf_root_id")?.clone()).ok()
}

fn is_in_plan_directory(path: &Path, plan_directories: &[PathBuf]) -> bool {
    let parent_components = path
        .parent()
        .map(|parent| parent.components().collect::<Vec<_>>())
        .unwrap_or_default();
    plan_directories.iter().any(|directory| {
        let directory_components = directory.components().collect::<Vec<_>>();
        !directory_components.is_empty()
            && parent_components
                .windows(directory_components.len())
                .any(|window| window == directory_components.as_slice())
    })
}

fn is_reserved_document_path(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some("index.md" | "log.md")
    )
}

fn root_priority(roots: &[DocumentRoot], root: &Path) -> usize {
    roots
        .iter()
        .position(|candidate| candidate.path() == root)
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temporary_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("okf-{label}-{unique}"));
        fs::create_dir_all(&path).expect("temporary directory");
        path
    }

    #[test]
    fn loads_markdown_and_planning_sections() {
        let root = temporary_directory("load");
        fs::create_dir_all(root.join("plans")).expect("plans directory");
        fs::write(
            root.join("plans/example.md"),
            "---\ntitle: Example\ntype: Plan\nkind: knowledge-plan\ntopic: demo\n---\n\
             # Ignored\n\n## Completed Decisions\n\n- done\n\n\
             ## Active Open Questions\n\n- open\n\n## Deferred / Later\n\n- later\n",
        )
        .expect("document");

        let repository = Repository::open([DocumentRoot::new(&root)]).expect("repository");
        let document = repository
            .find(DocumentQuery::Exact("example".to_string()))
            .expect("document");

        assert_eq!(document.title(), "Example");
        assert_eq!(document.topic(), Some("demo"));
        assert!(document.is_plan());
        assert_eq!(document.planning().completed, ["- done"]);
        assert_eq!(document.planning().open, ["- open"]);
        assert_eq!(document.planning().deferred, ["- later"]);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn earlier_root_shadows_the_same_relative_path() {
        let first = temporary_directory("first");
        let second = temporary_directory("second");
        fs::write(
            first.join("same.md"),
            "---\ntype: Reference\n---\n# First\n",
        )
        .expect("first");
        fs::write(
            second.join("same.md"),
            "---\ntype: Reference\n---\n# Second\n",
        )
        .expect("second");

        let repository = Repository::open([DocumentRoot::new(&first), DocumentRoot::new(&second)])
            .expect("repository");

        assert_eq!(repository.documents().len(), 1);
        assert_eq!(repository.documents()[0].title(), "First");
        assert!(matches!(
            repository.diagnostics(),
            [Diagnostic::ShadowedDocument { .. }]
        ));
        fs::remove_dir_all(first).expect("cleanup");
        fs::remove_dir_all(second).expect("cleanup");
    }

    #[test]
    fn missing_roots_are_diagnostics_when_one_root_is_usable() {
        let root = temporary_directory("usable");
        fs::write(
            root.join("document.md"),
            "---\ntype: Reference\n---\n# Document\n",
        )
        .expect("document");
        let missing = root.join("missing");

        let repository = Repository::open([DocumentRoot::new(&root), DocumentRoot::new(&missing)])
            .expect("repository");

        assert_eq!(repository.documents().len(), 1);
        assert!(matches!(
            repository.diagnostics(),
            [Diagnostic::MissingRoot { .. }]
        ));
        fs::remove_dir_all(root).expect("cleanup");
    }
}
