use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::{DocumentId, PortablePath, RootId};

/// An ordered root directory containing OKF documents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentRoot {
    path: PathBuf,
    mount: Option<PathBuf>,
}

impl DocumentRoot {
    /// Creates a document root from a filesystem path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            mount: None,
        }
    }

    /// Creates a document root under a logical namespace.
    ///
    /// Mounted roots keep physical filesystem layout separate from document
    /// identity. For example, mounting `crates/scql/docs/knowledge` as `scql`
    /// exposes its `index.md` as `scql/index.md`.
    pub fn mounted(mount: impl Into<PathBuf>, path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            mount: Some(mount.into()),
        }
    }

    /// Returns the configured filesystem path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the optional logical namespace.
    pub fn mount(&self) -> Option<&Path> {
        self.mount.as_deref()
    }

    pub(crate) fn logical_path(&self, relative_path: &Path) -> PathBuf {
        match &self.mount {
            Some(mount) => mount.join(relative_path),
            None => relative_path.to_path_buf(),
        }
    }
}

impl<P> From<P> for DocumentRoot
where
    P: Into<PathBuf>,
{
    fn from(path: P) -> Self {
        Self::new(path)
    }
}

/// An open document-kind identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DocumentKind(String);

impl DocumentKind {
    /// Creates an open document-kind value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the underlying kind label.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DocumentKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<String> for DocumentKind {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for DocumentKind {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Configurable headings used to classify planning sections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanningHeadings {
    pub completed: Vec<String>,
    pub open: Vec<String>,
    pub deferred: Vec<String>,
}

impl Default for PlanningHeadings {
    fn default() -> Self {
        Self {
            completed: vec![
                "Completed Decisions".to_string(),
                "Erledigte Grundsatzentscheidungen".to_string(),
            ],
            open: vec![
                "Active Open Questions".to_string(),
                "Open Questions".to_string(),
                "Aktiv offen".to_string(),
            ],
            deferred: vec![
                "Deferred / Later".to_string(),
                "Später / bewusst verschoben".to_string(),
            ],
        }
    }
}

/// Planning items extracted from an OKF document.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlanningSections {
    pub completed: Vec<String>,
    pub open: Vec<String>,
    pub deferred: Vec<String>,
}

/// A canonical relation stored in OKF frontmatter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalRelation {
    relation_type: String,
    target: PathBuf,
    suggestion_id: Option<String>,
    source_chunk: Option<String>,
    target_chunk: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    generation_method: Option<String>,
    ai_generated: bool,
    score: Option<String>,
    created_at: Option<String>,
    status: Option<String>,
}

/// A non-Markdown resource declared by its owning directory's `index.md`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredResource {
    path: PathBuf,
    resource_type: String,
    media_type: String,
}

impl DeclaredResource {
    pub(crate) fn from_json(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        let path = object.get("path")?.as_str()?;
        let resource_type = object.get("type")?.as_str()?.trim();
        let media_type = object.get("media_type")?.as_str()?.trim();
        if path.is_empty()
            || path.contains('/')
            || PortablePath::parse(path).is_err()
            || !Path::new(path)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
            || resource_type.is_empty()
            || media_type != "text/csv; charset=utf-8"
        {
            return None;
        }
        Some(Self {
            path: PathBuf::from(path),
            resource_type: resource_type.to_string(),
            media_type: media_type.to_string(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    pub fn media_type(&self) -> &str {
        &self.media_type
    }
}

impl CanonicalRelation {
    pub(crate) fn from_json(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        Some(Self {
            relation_type: object.get("type")?.as_str()?.to_string(),
            target: PathBuf::from(object.get("target")?.as_str()?),
            suggestion_id: string_field(object, "suggestion_id"),
            source_chunk: string_field(object, "source_chunk"),
            target_chunk: string_field(object, "target_chunk"),
            provider: string_field(object, "provider"),
            model: string_field(object, "model"),
            generation_method: string_field(object, "generation_method"),
            ai_generated: object
                .get("ai_generated")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            score: object.get("score").map(ToString::to_string),
            created_at: string_field(object, "created_at"),
            status: string_field(object, "status"),
        })
    }

    pub fn relation_type(&self) -> &str {
        &self.relation_type
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    pub fn suggestion_id(&self) -> Option<&str> {
        self.suggestion_id.as_deref()
    }

    pub fn source_chunk(&self) -> Option<&str> {
        self.source_chunk.as_deref()
    }

    pub fn target_chunk(&self) -> Option<&str> {
        self.target_chunk.as_deref()
    }

    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn generation_method(&self) -> Option<&str> {
        self.generation_method.as_deref()
    }

    pub fn ai_generated(&self) -> bool {
        self.ai_generated
    }

    pub fn score(&self) -> Option<&str> {
        self.score.as_deref()
    }

    pub fn created_at(&self) -> Option<&str> {
        self.created_at.as_deref()
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }
}

fn string_field(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    object.get(key)?.as_str().map(str::to_string)
}

/// Repository-level OKF conventions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryOptions {
    pub planning_headings: PlanningHeadings,
    pub plan_kinds: Vec<DocumentKind>,
    pub plan_directories: Vec<PathBuf>,
}

impl Default for RepositoryOptions {
    fn default() -> Self {
        Self {
            planning_headings: PlanningHeadings::default(),
            plan_kinds: vec![DocumentKind::new("knowledge-plan")],
            plan_directories: vec![PathBuf::from("plans")],
        }
    }
}

/// A non-fatal condition encountered while loading a repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    MissingRoot {
        root: PathBuf,
    },
    MissingDocumentType {
        relative_path: PathBuf,
        root: PathBuf,
    },
    ShadowedDocument {
        relative_path: PathBuf,
        selected_root: PathBuf,
        shadowed_root: PathBuf,
    },
}

/// One parsed OKF document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    pub(crate) root: PathBuf,
    pub(crate) root_id: Option<RootId>,
    pub(crate) document_id: Option<DocumentId>,
    pub(crate) source_relative_path: PathBuf,
    pub(crate) relative_path: PathBuf,
    pub(crate) title: String,
    pub(crate) document_type: Option<DocumentKind>,
    pub(crate) kind: Option<DocumentKind>,
    pub(crate) frontmatter: BTreeMap<String, String>,
    pub(crate) planning: PlanningSections,
    pub(crate) relations: Vec<CanonicalRelation>,
    pub(crate) resources: Vec<DeclaredResource>,
    pub(crate) is_plan: bool,
}

impl Document {
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the portable bundle identity from the root `index.md`.
    ///
    /// This identity is independent of root ordering, mount name, physical
    /// path, browser URL, and the HTTP runtime root index.
    pub fn root_id(&self) -> Option<&RootId> {
        self.root_id.as_ref()
    }

    /// Returns the portable concept identity from `okf_document_id`.
    pub fn document_id(&self) -> Option<&DocumentId> {
        self.document_id.as_ref()
    }

    /// Returns the physical path relative to the document root.
    ///
    /// For mounted roots this differs from `relative_path()`, which includes
    /// the logical mount namespace.
    pub fn source_relative_path(&self) -> &Path {
        &self.source_relative_path
    }

    /// Returns the physical filesystem path for the document.
    pub fn physical_path(&self) -> PathBuf {
        self.root.join(&self.source_relative_path)
    }

    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }

    pub fn filename(&self) -> &str {
        self.relative_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    }

    pub fn stem(&self) -> &str {
        self.relative_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the OKF concept type from the `type` frontmatter field.
    ///
    /// OKF v0.1 uses `type` as the primary, open-ended classification for
    /// concept documents. Consumers must tolerate unknown values.
    pub fn document_type(&self) -> Option<&DocumentKind> {
        self.document_type.as_ref()
    }

    /// Returns the compatibility classification from the `kind` frontmatter
    /// field.
    ///
    /// `kind` predates the OKF v0.1 `type` requirement in scanlab's knowledge
    /// documents. It is still exposed so existing scanlab metadata and plan
    /// detection keep working while `type` becomes the primary OKF field.
    pub fn kind(&self) -> Option<&DocumentKind> {
        self.kind.as_ref()
    }

    pub fn topic(&self) -> Option<&str> {
        self.frontmatter.get("topic").map(String::as_str)
    }

    pub fn status(&self) -> Option<&str> {
        self.frontmatter.get("status").map(String::as_str)
    }

    pub fn updated(&self) -> Option<&str> {
        self.frontmatter.get("updated").map(String::as_str)
    }

    pub fn frontmatter(&self) -> &BTreeMap<String, String> {
        &self.frontmatter
    }

    pub fn planning(&self) -> &PlanningSections {
        &self.planning
    }

    pub fn relations(&self) -> &[CanonicalRelation] {
        &self.relations
    }

    /// Returns valid resource declarations from an `index.md` frontmatter.
    pub fn resources(&self) -> &[DeclaredResource] {
        &self.resources
    }

    pub fn is_plan(&self) -> bool {
        self.is_plan
    }
}
