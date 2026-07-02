use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use crate::{
    analyze_compliance, is_valid_mount_name, scan_document_root, AdmissionError,
    AdmissionInventory, AdmissionLimits, AdmittedFormat, ComplianceProposal, ComplianceReport,
    DirectoryComplianceStatus, DocumentRoot, MarkdownComplianceStatus, RejectionReason,
    ResourceComplianceStatus, RootId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootProposalKind {
    Registration,
    SourceInitialization,
}

impl RootProposalKind {
    fn code(self) -> &'static str {
        match self {
            Self::Registration => "registration",
            Self::SourceInitialization => "source_initialization",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootProposalContext {
    pub configured_roots: Vec<DocumentRoot>,
    pub proposed_root_id: Option<RootId>,
    pub registration_enabled: bool,
    pub registration_priority: i64,
    pub check_for_changes: bool,
    pub process_override_active: bool,
    pub dotenv_override_active: bool,
    pub cli_override_active: bool,
}

impl Default for RootProposalContext {
    fn default() -> Self {
        Self {
            configured_roots: Vec::new(),
            proposed_root_id: None,
            registration_enabled: true,
            registration_priority: 0,
            check_for_changes: false,
            process_override_active: false,
            dotenv_override_active: false,
            cli_override_active: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootProposalRequest {
    pub root: PathBuf,
    pub mount: Option<String>,
    pub kind: RootProposalKind,
    pub limits: AdmissionLimits,
    pub context: RootProposalContext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalTreeState {
    AcceptedMarkdown,
    AcceptedCsv,
    Rejected,
    PendingMetadata,
    MissingIndex,
    DeclaredResource,
    UndeclaredResource,
    Conflict,
}

impl ProposalTreeState {
    pub fn code(self) -> &'static str {
        match self {
            Self::AcceptedMarkdown => "accepted_markdown",
            Self::AcceptedCsv => "accepted_csv",
            Self::Rejected => "rejected",
            Self::PendingMetadata => "pending_metadata",
            Self::MissingIndex => "missing_index",
            Self::DeclaredResource => "declared_resource",
            Self::UndeclaredResource => "undeclared_resource",
            Self::Conflict => "conflict",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposalTreeEntry {
    path: String,
    state: ProposalTreeState,
    detail: Option<String>,
}

impl ProposalTreeEntry {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn state(&self) -> ProposalTreeState {
        self.state
    }
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalConflictCode {
    MissingRootIdentity,
    InvalidRootIdentity,
    DuplicateRootIdentity,
    CaseOrUnicodeCollision,
    MountCollision,
    ShadowedPath,
    ExistingRootUnavailable,
}

impl ProposalConflictCode {
    pub fn code(self) -> &'static str {
        match self {
            Self::MissingRootIdentity => "missing_root_identity",
            Self::InvalidRootIdentity => "invalid_root_identity",
            Self::DuplicateRootIdentity => "duplicate_root_identity",
            Self::CaseOrUnicodeCollision => "case_or_unicode_collision",
            Self::MountCollision => "mount_collision",
            Self::ShadowedPath => "shadowed_path",
            Self::ExistingRootUnavailable => "existing_root_unavailable",
        }
    }

    pub fn blocks_registration(self) -> bool {
        matches!(
            self,
            Self::InvalidRootIdentity | Self::DuplicateRootIdentity | Self::CaseOrUnicodeCollision
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProposalConflict {
    code: ProposalConflictCode,
    path: Option<String>,
    detail: String,
}

impl ProposalConflict {
    pub fn code(&self) -> ProposalConflictCode {
        self.code
    }
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootProposalSummary {
    pub accepted_files: usize,
    pub rejected_entries: usize,
    pub inspected_entries: u64,
    pub total_candidate_bytes: u64,
    pub writable: bool,
    pub process_override_active: bool,
    pub dotenv_override_active: bool,
    pub cli_override_active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationChange {
    pub root_id: RootId,
    pub mount: Option<String>,
    pub canonical_path: PathBuf,
    pub enabled: bool,
    pub priority: i64,
    pub check_for_changes: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootProposal {
    request: RootProposalRequest,
    canonical_root: PathBuf,
    root_id: Option<RootId>,
    inventory: AdmissionInventory,
    compliance: ComplianceReport,
    tree: Vec<ProposalTreeEntry>,
    conflicts: Vec<ProposalConflict>,
    summary: RootProposalSummary,
    snapshot_digest: String,
    proposal_digest: String,
    confirmable: bool,
}

impl RootProposal {
    pub fn kind(&self) -> RootProposalKind {
        self.request.kind
    }
    pub fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }
    pub fn mount(&self) -> Option<&str> {
        self.request.mount.as_deref()
    }
    pub fn root_id(&self) -> Option<&RootId> {
        self.root_id.as_ref()
    }
    pub fn proposed_root_id(&self) -> Option<&RootId> {
        self.request.context.proposed_root_id.as_ref()
    }
    pub fn inventory(&self) -> &AdmissionInventory {
        &self.inventory
    }
    pub fn compliance(&self) -> &ComplianceReport {
        &self.compliance
    }
    pub fn tree(&self) -> &[ProposalTreeEntry] {
        &self.tree
    }
    pub fn conflicts(&self) -> &[ProposalConflict] {
        &self.conflicts
    }
    pub fn summary(&self) -> &RootProposalSummary {
        &self.summary
    }
    pub fn limits(&self) -> AdmissionLimits {
        self.request.limits
    }
    pub fn snapshot_digest(&self) -> &str {
        &self.snapshot_digest
    }
    pub fn proposal_digest(&self) -> &str {
        &self.proposal_digest
    }
    pub fn can_confirm(&self) -> bool {
        self.confirmable
    }
    pub fn source_changes(&self) -> &[ComplianceProposal] {
        if self.kind() == RootProposalKind::SourceInitialization {
            self.compliance.proposals()
        } else {
            &[]
        }
    }
    pub fn registration_change(&self) -> Option<RegistrationChange> {
        if self.kind() != RootProposalKind::Registration {
            return None;
        }
        Some(RegistrationChange {
            root_id: self.root_id.clone()?,
            mount: self.request.mount.clone(),
            canonical_path: self.canonical_root.clone(),
            enabled: self.request.context.registration_enabled,
            priority: self.request.context.registration_priority,
            check_for_changes: self.request.context.check_for_changes,
        })
    }
}

#[derive(Debug)]
pub enum ProposalError {
    Admission(AdmissionError),
    Canonicalize(std::io::Error),
    InvalidMount(String),
    UnstableSnapshot,
}

impl fmt::Display for ProposalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admission(error) => write!(formatter, "cannot scan proposal root: {error}"),
            Self::Canonicalize(_) => formatter.write_str("cannot canonicalize proposal root"),
            Self::InvalidMount(mount) => write!(formatter, "invalid proposal mount {mount:?}"),
            Self::UnstableSnapshot => {
                formatter.write_str("document root changed while the proposal was built")
            }
        }
    }
}

impl Error for ProposalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Admission(error) => Some(error),
            Self::Canonicalize(error) => Some(error),
            Self::InvalidMount(_) | Self::UnstableSnapshot => None,
        }
    }
}

impl From<AdmissionError> for ProposalError {
    fn from(value: AdmissionError) -> Self {
        Self::Admission(value)
    }
}

pub fn build_root_proposal(request: RootProposalRequest) -> Result<RootProposal, ProposalError> {
    if let Some(mount) = request
        .mount
        .as_deref()
        .filter(|mount| !is_valid_mount_name(mount))
    {
        return Err(ProposalError::InvalidMount(mount.to_string()));
    }
    let proposal = build_once(request.clone())?;
    let verification = scan_document_root(&proposal.canonical_root, request.limits)?;
    if inventory_digest(&verification) != proposal.snapshot_digest {
        return Err(ProposalError::UnstableSnapshot);
    }
    Ok(proposal)
}

fn build_once(request: RootProposalRequest) -> Result<RootProposal, ProposalError> {
    let canonical_root = request
        .root
        .canonicalize()
        .map_err(ProposalError::Canonicalize)?;
    let inventory = scan_document_root(&canonical_root, request.limits)?;
    let snapshot_digest = inventory_digest(&inventory);
    let compliance = analyze_compliance(&inventory);
    let root_id = root_identity(&canonical_root).ok().flatten();
    let mut conflicts = proposal_conflicts(&request, &canonical_root, &inventory, root_id.as_ref());
    conflicts.sort_by(|left, right| {
        (left.code.code(), left.path.as_deref(), &left.detail).cmp(&(
            right.code.code(),
            right.path.as_deref(),
            &right.detail,
        ))
    });
    let tree = proposal_tree(&inventory, &compliance, &conflicts);
    let summary = RootProposalSummary {
        accepted_files: inventory.accepted().len(),
        rejected_entries: inventory.rejected().len(),
        inspected_entries: inventory.inspected_entries(),
        total_candidate_bytes: inventory.inspected_bytes(),
        writable: is_declaratively_writable(&canonical_root),
        process_override_active: request.context.process_override_active,
        dotenv_override_active: request.context.dotenv_override_active,
        cli_override_active: request.context.cli_override_active,
    };
    let proposal_digest = proposal_digest(
        &request,
        &canonical_root,
        &snapshot_digest,
        &conflicts,
        compliance.proposals(),
    );
    let confirmable = inventory.can_confirm()
        && compliance.can_confirm()
        && !conflicts.iter().any(|conflict| {
            conflict.code.blocks_registration()
                || (request.kind == RootProposalKind::Registration
                    && conflict.code == ProposalConflictCode::MissingRootIdentity)
                || (request.kind == RootProposalKind::SourceInitialization
                    && conflict.code == ProposalConflictCode::MissingRootIdentity
                    && request.context.proposed_root_id.is_none())
        });
    Ok(RootProposal {
        request,
        canonical_root,
        root_id,
        inventory,
        compliance,
        tree,
        conflicts,
        summary,
        snapshot_digest,
        proposal_digest,
        confirmable,
    })
}

fn proposal_tree(
    inventory: &AdmissionInventory,
    compliance: &ComplianceReport,
    conflicts: &[ProposalConflict],
) -> Vec<ProposalTreeEntry> {
    let mut tree = Vec::new();
    for file in inventory.accepted() {
        tree.push(ProposalTreeEntry {
            path: file.path().as_str().to_string(),
            state: match file.format() {
                AdmittedFormat::Markdown => ProposalTreeState::AcceptedMarkdown,
                AdmittedFormat::Csv => ProposalTreeState::AcceptedCsv,
            },
            detail: None,
        });
    }
    for entry in inventory.rejected() {
        tree.push(ProposalTreeEntry {
            path: entry.display_path().to_string(),
            state: ProposalTreeState::Rejected,
            detail: Some(entry.reason().code().to_string()),
        });
    }
    for markdown in compliance.markdown() {
        if !matches!(
            markdown.status(),
            MarkdownComplianceStatus::Canonical | MarkdownComplianceStatus::Reserved
        ) {
            tree.push(ProposalTreeEntry {
                path: markdown.path().as_str().to_string(),
                state: ProposalTreeState::PendingMetadata,
                detail: Some(format!("{:?}", markdown.status()).to_ascii_lowercase()),
            });
        }
    }
    for directory in compliance.directories() {
        if directory.status() == DirectoryComplianceStatus::MissingIndex {
            tree.push(ProposalTreeEntry {
                path: directory.index_path().as_str().to_string(),
                state: ProposalTreeState::MissingIndex,
                detail: None,
            });
        }
    }
    for resource in compliance.resources() {
        if matches!(
            resource.status(),
            ResourceComplianceStatus::Declared | ResourceComplianceStatus::Undeclared
        ) {
            tree.push(ProposalTreeEntry {
                path: resource.path().as_str().to_string(),
                state: if resource.status() == ResourceComplianceStatus::Declared {
                    ProposalTreeState::DeclaredResource
                } else {
                    ProposalTreeState::UndeclaredResource
                },
                detail: resource.resource_type().map(str::to_string),
            });
        }
    }
    for conflict in conflicts {
        tree.push(ProposalTreeEntry {
            path: conflict.path.clone().unwrap_or_default(),
            state: ProposalTreeState::Conflict,
            detail: Some(conflict.code.code().to_string()),
        });
    }
    tree.sort_by(|left, right| {
        (&left.path, left.state.code(), left.detail.as_deref()).cmp(&(
            &right.path,
            right.state.code(),
            right.detail.as_deref(),
        ))
    });
    tree
}

fn proposal_conflicts(
    request: &RootProposalRequest,
    canonical_root: &Path,
    inventory: &AdmissionInventory,
    root_id: Option<&RootId>,
) -> Vec<ProposalConflict> {
    let mut conflicts = Vec::new();
    match root_identity(canonical_root) {
        Ok(None) => conflict(
            &mut conflicts,
            ProposalConflictCode::MissingRootIdentity,
            Some("index.md"),
            "root index has no okf_root_id",
        ),
        Err(()) => conflict(
            &mut conflicts,
            ProposalConflictCode::InvalidRootIdentity,
            Some("index.md"),
            "root index contains an invalid okf_root_id",
        ),
        Ok(Some(_)) => {}
    }
    for rejected in inventory.rejected() {
        if rejected.reason() == &RejectionReason::PathCollision {
            conflict(
                &mut conflicts,
                ProposalConflictCode::CaseOrUnicodeCollision,
                Some(rejected.display_path()),
                "path collides after case folding or Unicode normalization",
            );
        }
    }
    let candidate_paths = inventory
        .accepted()
        .iter()
        .map(|file| file.path().as_str())
        .collect::<BTreeSet<_>>();
    for configured in &request.context.configured_roots {
        if configured.path().canonicalize().ok().as_deref() == Some(canonical_root) {
            continue;
        }
        if configured.mount().and_then(Path::to_str) == request.mount.as_deref() {
            conflict(
                &mut conflicts,
                ProposalConflictCode::MountCollision,
                None,
                format!("mount is already used by {}", configured.path().display()),
            );
        }
        if let Some(candidate_id) = root_id {
            if root_identity(configured.path()).ok().flatten().as_ref() == Some(candidate_id) {
                conflict(
                    &mut conflicts,
                    ProposalConflictCode::DuplicateRootIdentity,
                    Some("index.md"),
                    format!(
                        "root identity is already configured at {}",
                        configured.path().display()
                    ),
                );
            }
        }
        if configured.mount().and_then(Path::to_str) != request.mount.as_deref() {
            continue;
        }
        match scan_document_root(configured.path(), request.limits) {
            Ok(existing) => {
                for path in existing
                    .accepted()
                    .iter()
                    .map(|file| file.path().as_str())
                    .filter(|path| candidate_paths.contains(path))
                {
                    conflict(
                        &mut conflicts,
                        ProposalConflictCode::ShadowedPath,
                        Some(path),
                        format!(
                            "logical path also exists in {}",
                            configured.path().display()
                        ),
                    );
                }
            }
            Err(_) => conflict(
                &mut conflicts,
                ProposalConflictCode::ExistingRootUnavailable,
                None,
                format!(
                    "cannot compare configured root {}",
                    configured.path().display()
                ),
            ),
        }
    }
    conflicts
}

fn conflict(
    output: &mut Vec<ProposalConflict>,
    code: ProposalConflictCode,
    path: Option<&str>,
    detail: impl Into<String>,
) {
    output.push(ProposalConflict {
        code,
        path: path.map(str::to_string),
        detail: detail.into(),
    });
}

fn root_identity(root: &Path) -> Result<Option<RootId>, ()> {
    let Ok(source) = fs::read_to_string(root.join("index.md")) else {
        return Ok(None);
    };
    let Some(frontmatter) = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))
    else {
        return Ok(None);
    };
    let mut closed = false;
    let mut values = Vec::new();
    for line in frontmatter.lines() {
        if line == "---" {
            closed = true;
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim() == "okf_root_id" {
            values.push(value.trim().trim_matches('"').to_string());
        }
    }
    if !closed || values.len() > 1 {
        return Err(());
    }
    values.pop().map(RootId::parse).transpose().map_err(|_| ())
}

fn inventory_digest(inventory: &AdmissionInventory) -> String {
    let mut hash = Sha256::new();
    encode(&mut hash, "okf-root-snapshot-v1");
    for entry in inventory.snapshot() {
        encode(&mut hash, entry.path());
        encode(&mut hash, entry.kind().code());
        encode(&mut hash, &entry.size().unwrap_or_default().to_string());
        encode(&mut hash, entry.content_hash().unwrap_or(""));
        encode(
            &mut hash,
            &entry.permissions().unwrap_or_default().to_string(),
        );
    }
    for rejected in inventory.rejected() {
        encode(&mut hash, rejected.display_path());
        encode(&mut hash, rejected.reason().code());
    }
    encode(&mut hash, &inventory.inspected_entries().to_string());
    encode(&mut hash, &inventory.inspected_bytes().to_string());
    encode(&mut hash, &format!("{:?}", inventory.status()));
    format!("{:x}", hash.finalize())
}

fn proposal_digest(
    request: &RootProposalRequest,
    canonical_root: &Path,
    snapshot_digest: &str,
    conflicts: &[ProposalConflict],
    changes: &[ComplianceProposal],
) -> String {
    let mut hash = Sha256::new();
    encode(&mut hash, "okf-root-proposal-v1");
    encode(&mut hash, request.kind.code());
    encode(&mut hash, &canonical_root.to_string_lossy());
    encode(&mut hash, request.mount.as_deref().unwrap_or(""));
    encode(&mut hash, snapshot_digest);
    encode(&mut hash, &format!("{:?}", request.limits));
    encode(&mut hash, &format!("{:?}", request.context));
    for conflict in conflicts {
        encode(&mut hash, conflict.code.code());
        encode(&mut hash, conflict.path.as_deref().unwrap_or(""));
        encode(&mut hash, &conflict.detail);
    }
    if request.kind == RootProposalKind::SourceInitialization {
        for change in changes {
            encode(&mut hash, &format!("{change:?}"));
        }
    }
    format!("{:x}", hash.finalize())
}

fn encode(hash: &mut Sha256, value: &str) {
    hash.update(value.len().to_le_bytes());
    hash.update(value.as_bytes());
}

pub fn generate_root_id() -> std::io::Result<RootId> {
    let mut random = [0_u8; 16];
    getrandom::getrandom(&mut random).map_err(|error| std::io::Error::other(error.to_string()))?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut token = String::with_capacity(random.len() * 2);
    for byte in random {
        token.push(HEX[(byte >> 4) as usize] as char);
        token.push(HEX[(byte & 0x0f) as usize] as char);
    }
    RootId::parse(format!("urn:okf:root:{token}"))
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(unix)]
fn is_declaratively_writable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o222 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_declaratively_writable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| !metadata.permissions().readonly())
        .unwrap_or(false)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalValidation {
    Fresh(Box<RootProposal>),
    Stale,
    Expired,
    Missing,
}

#[derive(Clone, Debug)]
struct StoredProposal {
    proposal: RootProposal,
    expires_at: SystemTime,
}

#[derive(Debug)]
pub struct RootProposalStore {
    ttl: Duration,
    next_id: u64,
    entries: BTreeMap<String, StoredProposal>,
}

impl RootProposalStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            next_id: 0,
            entries: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, proposal: RootProposal, now: SystemTime) -> String {
        self.remove_expired(now);
        self.next_id = self.next_id.wrapping_add(1);
        let mut hash = Sha256::new();
        encode(&mut hash, proposal.proposal_digest());
        hash.update(self.next_id.to_le_bytes());
        let id = format!("proposal-{:x}", hash.finalize());
        let expires_at = now.checked_add(self.ttl).unwrap_or(SystemTime::UNIX_EPOCH);
        self.entries.insert(
            id.clone(),
            StoredProposal {
                proposal,
                expires_at,
            },
        );
        id
    }

    pub fn get(&mut self, id: &str, now: SystemTime) -> Option<&RootProposal> {
        self.remove_expired(now);
        self.entries.get(id).map(|entry| &entry.proposal)
    }

    pub fn get_with_remaining(
        &mut self,
        id: &str,
        now: SystemTime,
    ) -> Option<(&RootProposal, Duration)> {
        self.remove_expired(now);
        self.entries.get(id).map(|entry| {
            (
                &entry.proposal,
                entry
                    .expires_at
                    .duration_since(now)
                    .unwrap_or(Duration::ZERO),
            )
        })
    }

    pub fn validate(
        &mut self,
        id: &str,
        now: SystemTime,
    ) -> Result<ProposalValidation, ProposalError> {
        let Some(stored) = self.entries.get(id).cloned() else {
            return Ok(ProposalValidation::Missing);
        };
        if now >= stored.expires_at {
            self.entries.remove(id);
            return Ok(ProposalValidation::Expired);
        }
        let current = match build_root_proposal(stored.proposal.request.clone()) {
            Ok(current) => current,
            Err(_) => {
                self.entries.remove(id);
                return Ok(ProposalValidation::Stale);
            }
        };
        if current.snapshot_digest() != stored.proposal.snapshot_digest()
            || current.proposal_digest() != stored.proposal.proposal_digest()
        {
            self.entries.remove(id);
            return Ok(ProposalValidation::Stale);
        }
        Ok(ProposalValidation::Fresh(Box::new(current)))
    }

    pub fn remove_expired(&mut self, now: SystemTime) {
        self.entries.retain(|_, entry| now < entry.expires_at);
    }
}
