use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::{AdmissionInventory, AdmittedFormat, PortablePath};

const CSV_MEDIA_TYPE: &str = "text/csv; charset=utf-8";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownComplianceStatus {
    Canonical,
    Reserved,
    MissingFrontmatter,
    MissingRequiredFields { fields: Vec<String> },
    InvalidMetadata,
    ConflictingMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownCompliance {
    path: PortablePath,
    status: MarkdownComplianceStatus,
    title: String,
}

impl MarkdownCompliance {
    pub fn path(&self) -> &PortablePath {
        &self.path
    }
    pub fn status(&self) -> &MarkdownComplianceStatus {
        &self.status
    }
    pub fn title(&self) -> &str {
        &self.title
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryComplianceStatus {
    Indexed,
    MissingIndex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryCompliance {
    path: String,
    index_path: PortablePath,
    status: DirectoryComplianceStatus,
}

impl DirectoryCompliance {
    /// Returns an empty string for the root directory.
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn index_path(&self) -> &PortablePath {
        &self.index_path
    }
    pub fn status(&self) -> DirectoryComplianceStatus {
        self.status
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceComplianceStatus {
    Declared,
    Undeclared,
    MissingFile,
    DuplicateDeclaration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceCompliance {
    path: PortablePath,
    resource_type: Option<String>,
    media_type: Option<String>,
    status: ResourceComplianceStatus,
    csv: Option<CsvAnalysis>,
}

impl ResourceCompliance {
    pub fn path(&self) -> &PortablePath {
        &self.path
    }
    pub fn resource_type(&self) -> Option<&str> {
        self.resource_type.as_deref()
    }
    pub fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }
    pub fn status(&self) -> ResourceComplianceStatus {
        self.status
    }
    pub fn csv(&self) -> Option<&CsvAnalysis> {
        self.csv.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CsvAnalysis {
    delimiter: Option<char>,
    encoding: &'static str,
    rows: usize,
    columns: usize,
    valid_structure: bool,
    warnings: Vec<CsvWarning>,
}

impl CsvAnalysis {
    pub fn delimiter(&self) -> Option<char> {
        self.delimiter
    }
    pub fn encoding(&self) -> &'static str {
        self.encoding
    }
    pub fn rows(&self) -> usize {
        self.rows
    }
    pub fn columns(&self) -> usize {
        self.columns
    }
    pub fn valid_structure(&self) -> bool {
        self.valid_structure
    }
    pub fn warnings(&self) -> &[CsvWarning] {
        &self.warnings
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CsvWarning {
    SpreadsheetFormula { row: usize, column: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComplianceDiagnosticCode {
    IncompleteAdmission,
    ReadFailed,
    MissingIndex,
    MissingFrontmatter,
    MissingRequiredField,
    InvalidMetadata,
    ConflictingMetadata,
    UndeclaredResource,
    MissingResource,
    InvalidResourceDeclaration,
    DuplicateResourceDeclaration,
    InvalidCsvStructure,
    SpreadsheetFormula,
}

impl ComplianceDiagnosticCode {
    pub fn code(self) -> &'static str {
        match self {
            Self::IncompleteAdmission => "incomplete_admission",
            Self::ReadFailed => "read_failed",
            Self::MissingIndex => "missing_index",
            Self::MissingFrontmatter => "missing_frontmatter",
            Self::MissingRequiredField => "missing_required_field",
            Self::InvalidMetadata => "invalid_metadata",
            Self::ConflictingMetadata => "conflicting_metadata",
            Self::UndeclaredResource => "undeclared_resource",
            Self::MissingResource => "missing_resource",
            Self::InvalidResourceDeclaration => "invalid_resource_declaration",
            Self::DuplicateResourceDeclaration => "duplicate_resource_declaration",
            Self::InvalidCsvStructure => "invalid_csv_structure",
            Self::SpreadsheetFormula => "spreadsheet_formula",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComplianceDiagnostic {
    code: ComplianceDiagnosticCode,
    path: String,
    detail: String,
}

impl ComplianceDiagnostic {
    pub fn code(&self) -> ComplianceDiagnosticCode {
        self.code
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComplianceProposal {
    CreateIndex {
        path: PortablePath,
        content: String,
    },
    MergeFrontmatter {
        path: PortablePath,
        fields: BTreeMap<String, String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComplianceReport {
    markdown: Vec<MarkdownCompliance>,
    directories: Vec<DirectoryCompliance>,
    resources: Vec<ResourceCompliance>,
    diagnostics: Vec<ComplianceDiagnostic>,
    proposals: Vec<ComplianceProposal>,
    confirmable: bool,
}

impl ComplianceReport {
    pub fn markdown(&self) -> &[MarkdownCompliance] {
        &self.markdown
    }
    pub fn directories(&self) -> &[DirectoryCompliance] {
        &self.directories
    }
    pub fn resources(&self) -> &[ResourceCompliance] {
        &self.resources
    }
    pub fn diagnostics(&self) -> &[ComplianceDiagnostic] {
        &self.diagnostics
    }
    pub fn proposals(&self) -> &[ComplianceProposal] {
        &self.proposals
    }
    pub fn can_confirm(&self) -> bool {
        self.confirmable
    }
}

#[derive(Default)]
struct IndexData {
    declarations: Vec<Declaration>,
    invalid: bool,
}

#[derive(Clone)]
struct Declaration {
    path: String,
    resource_type: String,
    media_type: String,
}

pub fn analyze_compliance(inventory: &AdmissionInventory) -> ComplianceReport {
    let mut markdown = Vec::new();
    let mut diagnostics = Vec::new();
    let mut proposals = Vec::new();
    let mut indexes = BTreeMap::<String, IndexData>::new();
    let mut csv_paths = BTreeSet::new();

    if !inventory.can_confirm() {
        diagnostic(
            &mut diagnostics,
            ComplianceDiagnosticCode::IncompleteAdmission,
            "",
            "admission inventory is incomplete",
        );
    }
    for file in inventory.accepted() {
        if file.format() == AdmittedFormat::Csv {
            csv_paths.insert(file.path().as_str().to_string());
            continue;
        }
        analyze_markdown(
            inventory,
            file.path(),
            &mut markdown,
            &mut diagnostics,
            &mut proposals,
            &mut indexes,
        );
    }
    let directories = analyze_directories(inventory, &mut diagnostics, &mut proposals);
    let resources = analyze_resources(inventory, &csv_paths, &indexes, &mut diagnostics);

    markdown.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));
    diagnostics.sort_by(|a, b| (&a.path, a.code.code()).cmp(&(&b.path, b.code.code())));
    proposals.sort_by(|a, b| proposal_path(a).cmp(proposal_path(b)));
    ComplianceReport {
        markdown,
        directories,
        resources,
        diagnostics,
        proposals,
        confirmable: inventory.can_confirm(),
    }
}

fn analyze_markdown(
    inventory: &AdmissionInventory,
    path: &PortablePath,
    output: &mut Vec<MarkdownCompliance>,
    diagnostics: &mut Vec<ComplianceDiagnostic>,
    proposals: &mut Vec<ComplianceProposal>,
    indexes: &mut BTreeMap<String, IndexData>,
) {
    let Ok(source) = fs::read_to_string(inventory.root().join(path.as_str())) else {
        diagnostic(
            diagnostics,
            ComplianceDiagnosticCode::ReadFailed,
            path.as_str(),
            "admitted Markdown could not be read",
        );
        return;
    };
    let inspected = inspect_frontmatter(&source);
    let title = inspected
        .body
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| title_from_path(path.as_str()));
    let reserved = is_reserved(path.as_str());
    let status = match inspected.state {
        FrontmatterState::Missing if reserved => MarkdownComplianceStatus::Reserved,
        FrontmatterState::Missing => {
            diagnostic(
                diagnostics,
                ComplianceDiagnosticCode::MissingFrontmatter,
                path.as_str(),
                "concept document has no frontmatter",
            );
            propose_fields(proposals, path, &title, true);
            MarkdownComplianceStatus::MissingFrontmatter
        }
        FrontmatterState::Invalid => {
            diagnostic(
                diagnostics,
                ComplianceDiagnosticCode::InvalidMetadata,
                path.as_str(),
                "frontmatter is malformed",
            );
            MarkdownComplianceStatus::InvalidMetadata
        }
        FrontmatterState::Conflicting => {
            diagnostic(
                diagnostics,
                ComplianceDiagnosticCode::ConflictingMetadata,
                path.as_str(),
                "frontmatter contains duplicate keys",
            );
            MarkdownComplianceStatus::ConflictingMetadata
        }
        FrontmatterState::Valid if reserved => {
            if file_name(path.as_str()) == "index.md" {
                capture_index(path, &inspected.values, indexes, diagnostics);
            }
            MarkdownComplianceStatus::Reserved
        }
        FrontmatterState::Valid
            if inspected
                .values
                .get("type")
                .is_none_or(|v| v.trim().is_empty()) =>
        {
            diagnostic(
                diagnostics,
                ComplianceDiagnosticCode::MissingRequiredField,
                path.as_str(),
                "required field type is missing",
            );
            propose_fields(proposals, path, &title, false);
            MarkdownComplianceStatus::MissingRequiredFields {
                fields: vec!["type".to_string()],
            }
        }
        FrontmatterState::Valid => MarkdownComplianceStatus::Canonical,
    };
    output.push(MarkdownCompliance {
        path: path.clone(),
        status,
        title,
    });
}

fn capture_index(
    path: &PortablePath,
    values: &BTreeMap<String, String>,
    indexes: &mut BTreeMap<String, IndexData>,
    diagnostics: &mut Vec<ComplianceDiagnostic>,
) {
    let directory = parent(path.as_str());
    let index = indexes.entry(directory.clone()).or_default();
    let Some(raw) = values.get("resources") else {
        return;
    };
    let Some(items) = serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.as_array().cloned())
    else {
        index.invalid = true;
        diagnostic(
            diagnostics,
            ComplianceDiagnosticCode::InvalidResourceDeclaration,
            path.as_str(),
            "resources must be a JSON array",
        );
        return;
    };
    for item in items {
        let parsed = item.as_object().and_then(|object| {
            Some((
                object.get("path")?.as_str()?,
                object.get("type")?.as_str()?,
                object.get("media_type")?.as_str()?,
            ))
        });
        let Some((resource_path, resource_type, media_type)) = parsed else {
            index.invalid = true;
            continue;
        };
        if !valid_resource_filename(resource_path)
            || resource_type.trim().is_empty()
            || media_type != CSV_MEDIA_TYPE
        {
            index.invalid = true;
            continue;
        }
        index.declarations.push(Declaration {
            path: join(&directory, resource_path),
            resource_type: resource_type.to_string(),
            media_type: media_type.to_string(),
        });
    }
    if index.invalid {
        diagnostic(
            diagnostics,
            ComplianceDiagnosticCode::InvalidResourceDeclaration,
            path.as_str(),
            "one or more resource declarations are invalid",
        );
    }
}

fn analyze_directories(
    inventory: &AdmissionInventory,
    diagnostics: &mut Vec<ComplianceDiagnostic>,
    proposals: &mut Vec<ComplianceProposal>,
) -> Vec<DirectoryCompliance> {
    let mut required = BTreeSet::new();
    let accepted = inventory
        .accepted()
        .iter()
        .map(|file| file.path().as_str())
        .collect::<BTreeSet<_>>();
    for file in inventory.accepted() {
        let mut current = parent(file.path().as_str());
        loop {
            required.insert(current.clone());
            if current.is_empty() {
                break;
            }
            current = parent(&current);
        }
    }
    required
        .into_iter()
        .map(|directory| {
            let path =
                PortablePath::parse(join(&directory, "index.md")).expect("portable index path");
            let status = if accepted.contains(path.as_str()) {
                DirectoryComplianceStatus::Indexed
            } else {
                diagnostic(
                    diagnostics,
                    ComplianceDiagnosticCode::MissingIndex,
                    path.as_str(),
                    "directory containing admitted content requires index.md",
                );
                proposals.push(ComplianceProposal::CreateIndex {
                    path: path.clone(),
                    content: generated_index(inventory, &directory),
                });
                DirectoryComplianceStatus::MissingIndex
            };
            DirectoryCompliance {
                path: directory,
                index_path: path,
                status,
            }
        })
        .collect()
}

fn analyze_resources(
    inventory: &AdmissionInventory,
    csv_paths: &BTreeSet<String>,
    indexes: &BTreeMap<String, IndexData>,
    diagnostics: &mut Vec<ComplianceDiagnostic>,
) -> Vec<ResourceCompliance> {
    let mut declared = BTreeMap::<String, Vec<Declaration>>::new();
    for index in indexes.values() {
        for declaration in &index.declarations {
            declared
                .entry(declaration.path.clone())
                .or_default()
                .push(declaration.clone());
        }
    }
    let mut output = Vec::new();
    for path in csv_paths {
        let values = declared.get(path);
        let (status, resource_type, media_type) = match values {
            None => {
                diagnostic(
                    diagnostics,
                    ComplianceDiagnosticCode::UndeclaredResource,
                    path,
                    "CSV is not declared by its owning index.md",
                );
                (ResourceComplianceStatus::Undeclared, None, None)
            }
            Some(values) if values.len() > 1 => {
                diagnostic(
                    diagnostics,
                    ComplianceDiagnosticCode::DuplicateResourceDeclaration,
                    path,
                    "resource is declared more than once",
                );
                (ResourceComplianceStatus::DuplicateDeclaration, None, None)
            }
            Some(values) => (
                ResourceComplianceStatus::Declared,
                Some(values[0].resource_type.clone()),
                Some(values[0].media_type.clone()),
            ),
        };
        output.push(ResourceCompliance {
            path: PortablePath::parse(path.clone()).expect("admitted portable CSV"),
            resource_type,
            media_type,
            status,
            csv: analyze_csv(inventory, path, diagnostics),
        });
    }
    for (path, values) in declared {
        if csv_paths.contains(&path) {
            continue;
        }
        diagnostic(
            diagnostics,
            ComplianceDiagnosticCode::MissingResource,
            &path,
            "declared CSV is absent from the admitted inventory",
        );
        if let Ok(path) = PortablePath::parse(path) {
            output.push(ResourceCompliance {
                path,
                resource_type: values.first().map(|v| v.resource_type.clone()),
                media_type: values.first().map(|v| v.media_type.clone()),
                status: ResourceComplianceStatus::MissingFile,
                csv: None,
            });
        }
    }
    output.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));
    output
}

fn analyze_csv(
    inventory: &AdmissionInventory,
    path: &str,
    diagnostics: &mut Vec<ComplianceDiagnostic>,
) -> Option<CsvAnalysis> {
    let source = fs::read_to_string(inventory.root().join(path)).ok()?;
    let delimiter = detect_delimiter(&source);
    let (rows, columns, valid, warnings) = parse_csv(&source, delimiter);
    if !valid {
        diagnostic(
            diagnostics,
            ComplianceDiagnosticCode::InvalidCsvStructure,
            path,
            "CSV has unbalanced quotes or inconsistent columns",
        );
    }
    for warning in &warnings {
        let CsvWarning::SpreadsheetFormula { row, column } = warning;
        diagnostic(
            diagnostics,
            ComplianceDiagnosticCode::SpreadsheetFormula,
            path,
            format!("cell {row}:{column} begins with a formula marker"),
        );
    }
    Some(CsvAnalysis {
        delimiter,
        encoding: "utf-8",
        rows,
        columns,
        valid_structure: valid,
        warnings,
    })
}

fn generated_index(inventory: &AdmissionInventory, directory: &str) -> String {
    let mut files = inventory
        .accepted()
        .iter()
        .filter(|file| parent(file.path().as_str()) == directory)
        .collect::<Vec<_>>();
    files.sort_by_key(|file| file.path().as_str());
    let resources = files.iter().filter(|file| file.format() == AdmittedFormat::Csv).map(|file| serde_json::json!({
        "path": file_name(file.path().as_str()), "type": "Data Resource", "media_type": CSV_MEDIA_TYPE
    })).collect::<Vec<_>>();
    let mut output = String::new();
    if !resources.is_empty() {
        output.push_str("---\nresources: ");
        output.push_str(&serde_json::to_string(&resources).expect("resource JSON"));
        output.push_str("\n---\n");
    }
    output.push_str("# Index\n\n");
    for file in files {
        let name = file_name(file.path().as_str());
        if name != "index.md" {
            output.push_str(&format!("- [{name}]({name})\n"));
        }
    }
    output
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum FrontmatterState {
    Missing,
    Valid,
    Invalid,
    Conflicting,
}

struct FrontmatterInspection<'a> {
    state: FrontmatterState,
    values: BTreeMap<String, String>,
    body: &'a str,
}

fn inspect_frontmatter(source: &str) -> FrontmatterInspection<'_> {
    if !(source.starts_with("---\n") || source.starts_with("---\r\n")) {
        return FrontmatterInspection {
            state: FrontmatterState::Missing,
            values: BTreeMap::new(),
            body: source,
        };
    }
    let (values, body) = crate::frontmatter::parse(source);
    if body.len() == source.len() {
        return FrontmatterInspection {
            state: FrontmatterState::Invalid,
            values,
            body,
        };
    }
    let header = &source[..source.len() - body.len()];
    let mut seen = BTreeSet::new();
    let mut invalid = false;
    let mut conflict = false;
    for line in header.lines().skip(1).take_while(|line| *line != "---") {
        if line.trim().is_empty()
            || line.trim_start().starts_with('#')
            || line.starts_with([' ', '\t'])
        {
            continue;
        }
        match line.split_once(':') {
            Some((key, _)) if !key.trim().is_empty() => {
                if !seen.insert(key.trim()) {
                    conflict = true;
                }
            }
            _ => invalid = true,
        }
    }
    let state = if conflict {
        FrontmatterState::Conflicting
    } else if invalid {
        FrontmatterState::Invalid
    } else {
        FrontmatterState::Valid
    };
    FrontmatterInspection {
        state,
        values,
        body,
    }
}

fn parse_csv(source: &str, delimiter: Option<char>) -> (usize, usize, bool, Vec<CsvWarning>) {
    let delimiter = delimiter.unwrap_or(',');
    let mut rows = Vec::<Vec<String>>::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(ch);
            }
        } else if ch == '"' && field.is_empty() {
            quoted = true;
        } else if ch == delimiter {
            row.push(std::mem::take(&mut field));
        } else if ch == '\n' {
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
        } else if ch != '\r' {
            field.push(ch);
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    let columns = rows.first().map_or(0, Vec::len);
    let consistent = rows.iter().all(|row| row.len() == columns);
    let mut warnings = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            if value.trim_start().starts_with(['=', '+', '-', '@']) {
                warnings.push(CsvWarning::SpreadsheetFormula {
                    row: r + 1,
                    column: c + 1,
                });
            }
        }
    }
    (rows.len(), columns, !quoted && consistent, warnings)
}

fn detect_delimiter(source: &str) -> Option<char> {
    let line = source.lines().find(|line| !line.trim().is_empty())?;
    [',', ';', '\t']
        .into_iter()
        .map(|ch| (ch, line.matches(ch).count()))
        .filter(|(_, n)| *n > 0)
        .max_by_key(|(_, n)| *n)
        .map(|(ch, _)| ch)
}

fn propose_fields(
    proposals: &mut Vec<ComplianceProposal>,
    path: &PortablePath,
    title: &str,
    include_title: bool,
) {
    let mut fields = BTreeMap::from([("type".to_string(), "Concept".to_string())]);
    if include_title {
        fields.insert("title".to_string(), title.to_string());
    }
    proposals.push(ComplianceProposal::MergeFrontmatter {
        path: path.clone(),
        fields,
    });
}

fn diagnostic(
    output: &mut Vec<ComplianceDiagnostic>,
    code: ComplianceDiagnosticCode,
    path: impl Into<String>,
    detail: impl Into<String>,
) {
    output.push(ComplianceDiagnostic {
        code,
        path: path.into(),
        detail: detail.into(),
    });
}

fn valid_resource_filename(value: &str) -> bool {
    PortablePath::parse(value).is_ok()
        && !value.contains('/')
        && Path::new(value)
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.eq_ignore_ascii_case("csv"))
}
fn is_reserved(path: &str) -> bool {
    matches!(file_name(path), "index.md" | "log.md")
}
fn parent(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(String::new, |(p, _)| p.to_string())
}
fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, n)| n)
}
fn join(directory: &str, name: &str) -> String {
    if directory.is_empty() {
        name.to_string()
    } else {
        format!("{directory}/{name}")
    }
}
fn title_from_path(path: &str) -> String {
    Path::new(file_name(path))
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("document")
        .to_string()
}
fn proposal_path(value: &ComplianceProposal) -> &str {
    match value {
        ComplianceProposal::CreateIndex { path, .. }
        | ComplianceProposal::MergeFrontmatter { path, .. } => path.as_str(),
    }
}
