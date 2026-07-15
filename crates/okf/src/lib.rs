//! Core document model and repository API for the Open Knowledge Format.
//!
//! OKF discovers and parses Markdown documents from ordered filesystem roots.
//! It is independent from scanlab and SCQL.

mod admission;
mod browser_config;
mod compliance;
mod config;
mod discovery;
mod document;
mod error;
mod frontmatter;
mod identity;
mod planning;
mod proposal;
mod query;
mod repository;
mod transaction;
mod uri;
pub mod voyage;

pub use admission::{
    scan_document_root, AdmissionError, AdmissionInventory, AdmissionLimits, AdmissionStatus,
    AdmittedFile, AdmittedFormat, LimitKind, RejectedEntry, RejectionReason, SnapshotEntry,
    SnapshotEntryKind,
};
pub use browser_config::{
    browser_config_path, import_document_roots, load_browser_config, save_browser_config,
    BrowserConfig, BrowserConfigError, BrowserRoot,
};
pub use compliance::{
    analyze_compliance, ComplianceDiagnostic, ComplianceDiagnosticCode, ComplianceProposal,
    ComplianceReport, CsvAnalysis, CsvWarning, DirectoryCompliance, DirectoryComplianceStatus,
    MarkdownCompliance, MarkdownComplianceStatus, ResourceCompliance, ResourceComplianceStatus,
};

pub use config::{
    deduplicate_document_roots, format_document_root_spec, is_valid_mount_name,
    merge_document_roots, merge_document_roots_with_browser, parse_document_root_spec,
    RootSpecError,
};
pub use document::{
    CanonicalRelation, DeclaredResource, Diagnostic, Document, DocumentKind, DocumentRoot,
    PlanningHeadings, PlanningSections, RepositoryOptions,
};
pub use error::OkfError;
pub use identity::{
    detect_path_collisions, DocumentId, IdentityError, PathCollision, PortablePath, RootId,
};
pub use proposal::{
    build_root_proposal, generate_root_id, ProposalConflict, ProposalConflictCode, ProposalError,
    ProposalTreeEntry, ProposalTreeState, ProposalValidation, RegistrationChange, RootProposal,
    RootProposalContext, RootProposalKind, RootProposalRequest, RootProposalStore,
    RootProposalSummary,
};
pub use query::DocumentQuery;
pub use repository::Repository;
pub use transaction::{
    build_initialization_plan, configuration_revision, confirm_root_registration,
    confirm_source_initialization, recover_interrupted_operations, remove_registered_root,
    update_registered_root, ConfigurationRevision, GitWorktreeStatus, InitializationOptions,
    InitializationPlan, InitializationReport, RegistrationReport, RootConfigurationUpdate,
    SourceFileChange, TransactionError,
};
pub use uri::{OkfUri, OkfUriError};
