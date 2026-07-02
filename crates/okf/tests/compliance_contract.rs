use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use okf::{
    analyze_compliance, scan_document_root, AdmissionLimits, ComplianceDiagnosticCode,
    ComplianceProposal, CsvWarning, DirectoryComplianceStatus, DocumentRoot,
    MarkdownComplianceStatus, Repository, ResourceComplianceStatus,
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("okf-compliance-{name}-{unique}"));
        fs::create_dir_all(&path).expect("create fixture root");
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn classifies_markdown_and_proposes_metadata_without_writing_sources() {
    let root = TestDirectory::new("markdown");
    fs::write(root.path().join("index.md"), "# Index\n").expect("index");
    fs::write(
        root.path().join("canonical.md"),
        "---\ntype: Reference\ncustom: keep\n---\n# Canonical\n",
    )
    .expect("canonical");
    fs::write(
        root.path().join("missing-frontmatter.md"),
        "# Existing title\n",
    )
    .expect("missing");
    fs::write(
        root.path().join("missing-type.md"),
        "---\ncustom: keep\n---\n# Missing type\n",
    )
    .expect("missing type");
    fs::write(
        root.path().join("invalid.md"),
        "---\ntype: Reference\n# no closing delimiter\n",
    )
    .expect("invalid");
    fs::write(
        root.path().join("conflict.md"),
        "---\ntype: A\ntype: B\n---\n# Conflict\n",
    )
    .expect("conflict");
    fs::create_dir(root.path().join("nested")).expect("nested");
    fs::write(
        root.path().join("nested/document.md"),
        "---\ntype: Guide\n---\n# Nested\n",
    )
    .expect("nested document");
    fs::write(root.path().join("nested/data.csv"), "name,value\na,1\n").expect("nested resource");

    let before = fs::read(root.path().join("missing-type.md")).expect("before");
    let inventory = scan_document_root(root.path(), AdmissionLimits::default()).expect("scan");
    let report = analyze_compliance(&inventory);
    let report_again = analyze_compliance(&inventory);
    assert_eq!(report, report_again, "proposals must be deterministic");
    assert!(report.can_confirm());
    assert_eq!(
        fs::read(root.path().join("missing-type.md")).unwrap(),
        before
    );

    let status = |path: &str| {
        report
            .markdown()
            .iter()
            .find(|item| item.path().as_str() == path)
            .unwrap()
            .status()
    };
    assert_eq!(status("canonical.md"), &MarkdownComplianceStatus::Canonical);
    assert_eq!(status("index.md"), &MarkdownComplianceStatus::Reserved);
    assert_eq!(
        status("missing-frontmatter.md"),
        &MarkdownComplianceStatus::MissingFrontmatter
    );
    assert!(matches!(
        status("missing-type.md"),
        MarkdownComplianceStatus::MissingRequiredFields { .. }
    ));
    assert_eq!(
        status("invalid.md"),
        &MarkdownComplianceStatus::InvalidMetadata
    );
    assert_eq!(
        status("conflict.md"),
        &MarkdownComplianceStatus::ConflictingMetadata
    );

    let nested = report
        .directories()
        .iter()
        .find(|item| item.path() == "nested")
        .unwrap();
    assert_eq!(nested.status(), DirectoryComplianceStatus::MissingIndex);
    assert!(report.proposals().iter().any(|proposal| matches!(
        proposal,
        ComplianceProposal::CreateIndex { path, content }
            if path.as_str() == "nested/index.md"
                && content.contains("document.md")
                && content.contains("\"path\":\"data.csv\"")
    )));
    assert!(report.proposals().iter().any(|proposal| matches!(
        proposal,
        ComplianceProposal::MergeFrontmatter { path, fields }
            if path.as_str() == "missing-type.md"
                && fields.get("type").map(String::as_str) == Some("Concept")
                && !fields.contains_key("custom")
    )));
}

#[test]
fn declared_resources_are_typed_and_csv_analysis_reports_safe_metadata() {
    let root = TestDirectory::new("resources");
    fs::write(
        root.path().join("index.md"),
        concat!(
            "---\n",
            "resources: [",
            "{\"path\":\"data.csv\",\"type\":\"Dataset\",\"media_type\":\"text/csv; charset=utf-8\"},",
            "{\"path\":\"missing.csv\",\"type\":\"Dataset\",\"media_type\":\"text/csv; charset=utf-8\"}",
            ", {\"path\":\"duplicate.csv\",\"type\":\"Dataset\",\"media_type\":\"text/csv; charset=utf-8\"}",
            ", {\"path\":\"duplicate.csv\",\"type\":\"Dataset\",\"media_type\":\"text/csv; charset=utf-8\"}",
            "]\n---\n# Index\n"
        ),
    ).expect("index");
    fs::write(
        root.path().join("data.csv"),
        "name;value\nalpha;1\nbeta;=2+2\n",
    )
    .expect("declared csv");
    fs::write(root.path().join("other.csv"), "a,b\n1,2\n").expect("undeclared csv");
    fs::write(root.path().join("duplicate.csv"), "a,b\n1,2\n").expect("duplicate csv");

    let inventory = scan_document_root(root.path(), AdmissionLimits::default()).expect("scan");
    let report = analyze_compliance(&inventory);
    let data = report
        .resources()
        .iter()
        .find(|item| item.path().as_str() == "data.csv")
        .unwrap();
    assert_eq!(data.status(), ResourceComplianceStatus::Declared);
    assert_eq!(data.resource_type(), Some("Dataset"));
    assert_eq!(data.media_type(), Some("text/csv; charset=utf-8"));
    let csv = data.csv().expect("CSV analysis");
    assert_eq!(csv.delimiter(), Some(';'));
    assert_eq!(csv.encoding(), "utf-8");
    assert_eq!((csv.rows(), csv.columns()), (3, 2));
    assert!(csv.valid_structure());
    assert_eq!(
        csv.warnings(),
        &[CsvWarning::SpreadsheetFormula { row: 3, column: 2 }]
    );

    let other = report
        .resources()
        .iter()
        .find(|item| item.path().as_str() == "other.csv")
        .unwrap();
    assert_eq!(other.status(), ResourceComplianceStatus::Undeclared);
    let missing = report
        .resources()
        .iter()
        .find(|item| item.path().as_str() == "missing.csv")
        .unwrap();
    assert_eq!(missing.status(), ResourceComplianceStatus::MissingFile);
    let duplicate = report
        .resources()
        .iter()
        .find(|item| item.path().as_str() == "duplicate.csv")
        .unwrap();
    assert_eq!(
        duplicate.status(),
        ResourceComplianceStatus::DuplicateDeclaration
    );

    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");
    let index = repository
        .documents()
        .iter()
        .find(|document| document.filename() == "index.md")
        .unwrap();
    assert_eq!(index.resources().len(), 4);
    assert_eq!(index.resources()[0].path(), Path::new("data.csv"));
}

#[test]
fn malformed_csv_and_resource_declarations_are_diagnostics_not_writes() {
    let root = TestDirectory::new("invalid-resource");
    fs::write(root.path().join("index.md"), "---\nresources: [{\"path\":\"nested/data.csv\",\"type\":\"\",\"media_type\":\"text/html\"}]\n---\n# Index\n").expect("invalid index");
    fs::write(root.path().join("broken.csv"), "a,b\n1\n").expect("broken csv");
    let inventory = scan_document_root(root.path(), AdmissionLimits::default()).expect("scan");
    let report = analyze_compliance(&inventory);
    let codes = report
        .diagnostics()
        .iter()
        .map(|item| item.code())
        .collect::<Vec<_>>();
    assert!(codes.contains(&ComplianceDiagnosticCode::InvalidResourceDeclaration));
    assert!(codes.contains(&ComplianceDiagnosticCode::InvalidCsvStructure));
    assert!(codes.contains(&ComplianceDiagnosticCode::UndeclaredResource));
    let repository = Repository::open([DocumentRoot::new(root.path())]).expect("repository");
    let index = repository
        .documents()
        .iter()
        .find(|document| document.filename() == "index.md")
        .unwrap();
    assert!(index.resources().is_empty());
}

#[test]
fn incomplete_admission_never_produces_a_confirmable_compliance_report() {
    let root = TestDirectory::new("incomplete");
    fs::write(root.path().join("a.md"), "# A\n").expect("a");
    fs::write(root.path().join("b.md"), "# B\n").expect("b");
    let inventory = scan_document_root(
        root.path(),
        AdmissionLimits {
            max_entries: 1,
            ..AdmissionLimits::default()
        },
    )
    .expect("limited scan");
    let report = analyze_compliance(&inventory);
    assert!(!report.can_confirm());
    assert!(report
        .diagnostics()
        .iter()
        .any(|item| item.code() == ComplianceDiagnosticCode::IncompleteAdmission));
}
