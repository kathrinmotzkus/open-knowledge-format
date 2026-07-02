use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    load_browser_config, save_browser_config, BrowserRoot, ComplianceProposal, ProposalValidation,
    RootProposal, RootProposalKind, RootProposalStore,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigurationRevision(String);

impl ConfigurationRevision {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, TransactionError> {
        let value = value.into();
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(TransactionError::Configuration(
                "configuration revision must be a 64-character hexadecimal digest".to_string(),
            ));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationReport {
    pub changed: bool,
    pub revision: ConfigurationRevision,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RootConfigurationUpdate {
    pub mount: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub priority: Option<i64>,
    pub check_for_changes: Option<bool>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InitializationOptions {
    pub resource_types: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFileChange {
    path: String,
    before: Option<String>,
    after: String,
    diff: String,
}

impl SourceFileChange {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn before(&self) -> Option<&str> {
        self.before.as_deref()
    }
    pub fn after(&self) -> &str {
        &self.after
    }
    pub fn diff(&self) -> &str {
        &self.diff
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitWorktreeStatus {
    pub root: PathBuf,
    pub dirty: bool,
    pub entries: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializationPlan {
    root: PathBuf,
    proposal_digest: String,
    plan_digest: String,
    changes: Vec<SourceFileChange>,
    git: Option<GitWorktreeStatus>,
}

impl InitializationPlan {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn proposal_digest(&self) -> &str {
        &self.proposal_digest
    }
    pub fn plan_digest(&self) -> &str {
        &self.plan_digest
    }
    pub fn changes(&self) -> &[SourceFileChange] {
        &self.changes
    }
    pub fn git(&self) -> Option<&GitWorktreeStatus> {
        self.git.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializationReport {
    pub changed_files: Vec<String>,
    pub recovered_interrupted_operation: bool,
    pub git_before: Option<GitWorktreeStatus>,
    pub git_after: Option<GitWorktreeStatus>,
    pub final_diffs: Vec<String>,
}

#[derive(Debug)]
pub enum TransactionError {
    ProposalMissing,
    ProposalExpired,
    ProposalStale,
    WrongProposalKind,
    ProposalNotConfirmable,
    RevisionConflict {
        expected: String,
        actual: String,
    },
    PlanConflict {
        expected: String,
        actual: String,
    },
    ExistingField {
        path: String,
        field: String,
    },
    InvalidResourceType(String),
    ResourceNotProposed(String),
    UnsafePath(PathBuf),
    Configuration(String),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    Journal(String),
}

impl fmt::Display for TransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProposalMissing => formatter.write_str("proposal is missing"),
            Self::ProposalExpired => formatter.write_str("proposal has expired"),
            Self::ProposalStale => formatter.write_str("proposal snapshot is stale"),
            Self::WrongProposalKind => formatter.write_str("proposal has the wrong operation kind"),
            Self::ProposalNotConfirmable => formatter.write_str("proposal is not confirmable"),
            Self::RevisionConflict { expected, actual } => write!(
                formatter,
                "configuration revision conflict: expected {expected}, found {actual}"
            ),
            Self::PlanConflict { expected, actual } => write!(
                formatter,
                "initialization plan conflict: expected {expected}, found {actual}"
            ),
            Self::ExistingField { path, field } => {
                write!(
                    formatter,
                    "refusing to overwrite existing field {field:?} in {path}"
                )
            }
            Self::InvalidResourceType(path) => {
                write!(
                    formatter,
                    "resource {path} requires a non-empty explicit type"
                )
            }
            Self::ResourceNotProposed(path) => {
                write!(
                    formatter,
                    "resource {path} is not an undeclared CSV in this proposal"
                )
            }
            Self::UnsafePath(path) => {
                write!(formatter, "unsafe transaction path {}", path.display())
            }
            Self::Configuration(detail) | Self::Journal(detail) => formatter.write_str(detail),
            Self::Io {
                operation, path, ..
            } => write!(formatter, "cannot {operation} {}", path.display()),
        }
    }
}

impl Error for TransactionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn configuration_revision(path: &Path) -> Result<ConfigurationRevision, TransactionError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(source) => return Err(io_error("read", path, source)),
    };
    let mut hash = Sha256::new();
    hash.update(b"okf-config-revision-v1");
    hash.update(bytes.len().to_le_bytes());
    hash.update(bytes);
    Ok(ConfigurationRevision(format!("{:x}", hash.finalize())))
}

pub fn confirm_root_registration(
    config_path: &Path,
    store: &mut RootProposalStore,
    proposal_id: &str,
    now: SystemTime,
    expected_revision: &ConfigurationRevision,
) -> Result<RegistrationReport, TransactionError> {
    let initial = fresh_proposal(store, proposal_id, now)?;
    if initial.kind() != RootProposalKind::Registration {
        return Err(TransactionError::WrongProposalKind);
    }
    ensure_external_path(config_path, initial.canonical_root())?;
    drop(initial);
    let _lock = FileLock::acquire(&config_path.with_extension("lock"))?;
    let actual = configuration_revision(config_path)?;
    if &actual != expected_revision {
        return Err(TransactionError::RevisionConflict {
            expected: expected_revision.0.clone(),
            actual: actual.0,
        });
    }
    let proposal = fresh_proposal(store, proposal_id, now)?;
    if proposal.kind() != RootProposalKind::Registration {
        return Err(TransactionError::WrongProposalKind);
    }
    if !proposal.can_confirm() {
        return Err(TransactionError::ProposalNotConfirmable);
    }
    let change = proposal
        .registration_change()
        .ok_or(TransactionError::ProposalNotConfirmable)?;
    let mut config = load_browser_config(config_path)
        .map_err(|error| TransactionError::Configuration(error.to_string()))?;
    if let Some(existing) = config
        .roots()
        .iter()
        .find(|root| root.root_id == change.root_id)
    {
        let exact = existing.mount == change.mount
            && existing.path == change.canonical_path
            && existing.enabled == change.enabled
            && existing.priority == change.priority
            && existing.check_for_changes == change.check_for_changes;
        if exact {
            return Ok(RegistrationReport {
                changed: false,
                revision: actual,
            });
        }
        return Err(TransactionError::Configuration(format!(
            "root identity {} is already configured differently",
            change.root_id
        )));
    }
    config.roots_mut().push(BrowserRoot {
        root_id: change.root_id,
        mount: change.mount,
        path: change.canonical_path,
        enabled: change.enabled,
        priority: change.priority,
        check_for_changes: change.check_for_changes,
    });
    save_browser_config(config_path, &config)
        .map_err(|error| TransactionError::Configuration(error.to_string()))?;
    Ok(RegistrationReport {
        changed: true,
        revision: configuration_revision(config_path)?,
    })
}

pub fn update_registered_root(
    config_path: &Path,
    root_id: &crate::RootId,
    expected_revision: &ConfigurationRevision,
    update: &RootConfigurationUpdate,
) -> Result<RegistrationReport, TransactionError> {
    if update
        .mount
        .as_ref()
        .and_then(|mount| mount.as_deref())
        .is_some_and(|mount| !crate::is_valid_mount_name(mount))
    {
        return Err(TransactionError::Configuration(
            "root mount is invalid".to_string(),
        ));
    }
    let _lock = FileLock::acquire(&config_path.with_extension("lock"))?;
    let actual = configuration_revision(config_path)?;
    if &actual != expected_revision {
        return Err(TransactionError::RevisionConflict {
            expected: expected_revision.0.clone(),
            actual: actual.0,
        });
    }
    let mut config = load_browser_config(config_path)
        .map_err(|error| TransactionError::Configuration(error.to_string()))?;
    let root = config
        .roots_mut()
        .iter_mut()
        .find(|root| &root.root_id == root_id)
        .ok_or_else(|| TransactionError::Configuration("root is not configured".to_string()))?;
    ensure_external_path(config_path, &root.path)?;
    let before = root.clone();
    if let Some(mount) = &update.mount {
        root.mount = mount.clone();
    }
    if let Some(enabled) = update.enabled {
        root.enabled = enabled;
    }
    if let Some(priority) = update.priority {
        root.priority = priority;
    }
    if let Some(check_for_changes) = update.check_for_changes {
        root.check_for_changes = check_for_changes;
    }
    if *root == before {
        return Ok(RegistrationReport {
            changed: false,
            revision: actual,
        });
    }
    save_browser_config(config_path, &config)
        .map_err(|error| TransactionError::Configuration(error.to_string()))?;
    Ok(RegistrationReport {
        changed: true,
        revision: configuration_revision(config_path)?,
    })
}

pub fn remove_registered_root(
    config_path: &Path,
    root_id: &crate::RootId,
    expected_revision: &ConfigurationRevision,
) -> Result<RegistrationReport, TransactionError> {
    let _lock = FileLock::acquire(&config_path.with_extension("lock"))?;
    let actual = configuration_revision(config_path)?;
    if &actual != expected_revision {
        return Err(TransactionError::RevisionConflict {
            expected: expected_revision.0.clone(),
            actual: actual.0,
        });
    }
    let mut config = load_browser_config(config_path)
        .map_err(|error| TransactionError::Configuration(error.to_string()))?;
    let index = config
        .roots()
        .iter()
        .position(|root| &root.root_id == root_id)
        .ok_or_else(|| TransactionError::Configuration("root is not configured".to_string()))?;
    ensure_external_path(config_path, &config.roots()[index].path)?;
    config.roots_mut().remove(index);
    save_browser_config(config_path, &config)
        .map_err(|error| TransactionError::Configuration(error.to_string()))?;
    Ok(RegistrationReport {
        changed: true,
        revision: configuration_revision(config_path)?,
    })
}

pub fn build_initialization_plan(
    proposal: &RootProposal,
    options: &InitializationOptions,
) -> Result<InitializationPlan, TransactionError> {
    if proposal.kind() != RootProposalKind::SourceInitialization {
        return Err(TransactionError::WrongProposalKind);
    }
    if !proposal.can_confirm() {
        return Err(TransactionError::ProposalNotConfirmable);
    }
    let root = proposal.canonical_root().to_path_buf();
    let mut pending = BTreeMap::<String, (Option<String>, String)>::new();
    for change in proposal.source_changes() {
        match change {
            ComplianceProposal::CreateIndex { path, content } => {
                let target = safe_target(&root, path.as_str())?;
                if target.exists() {
                    return Err(TransactionError::ExistingField {
                        path: path.as_str().to_string(),
                        field: "index.md".to_string(),
                    });
                }
                pending.insert(path.as_str().to_string(), (None, content.clone()));
            }
            ComplianceProposal::MergeFrontmatter { path, fields } => {
                let target = safe_target(&root, path.as_str())?;
                let before = fs::read_to_string(&target)
                    .map_err(|source| io_error("read", &target, source))?;
                let after = merge_missing_frontmatter(&before, fields, path.as_str())?;
                pending.insert(path.as_str().to_string(), (Some(before), after));
            }
        }
    }
    if proposal.root_id().is_none() {
        let proposed = proposal
            .proposed_root_id()
            .ok_or(TransactionError::ProposalNotConfirmable)?;
        let index = "index.md".to_string();
        let target = safe_target(&root, &index)?;
        let (before, current) = match pending.remove(&index) {
            Some(value) => value,
            None => {
                let source = fs::read_to_string(&target)
                    .map_err(|source| io_error("read", &target, source))?;
                (Some(source.clone()), source)
            }
        };
        let after = merge_missing_frontmatter(
            &current,
            &BTreeMap::from([("okf_root_id".to_string(), proposed.to_string())]),
            &index,
        )?;
        pending.insert(index, (before, after));
    }
    append_resource_declarations(proposal, options, &root, &mut pending)?;
    let changes = pending
        .into_iter()
        .filter_map(|(path, (before, after))| {
            if before.as_deref() == Some(after.as_str()) {
                None
            } else {
                Some(SourceFileChange {
                    diff: exact_diff(&path, before.as_deref(), &after),
                    path,
                    before,
                    after,
                })
            }
        })
        .collect::<Vec<_>>();
    let plan_digest = initialization_plan_digest(proposal.proposal_digest(), &changes);
    Ok(InitializationPlan {
        root: root.clone(),
        proposal_digest: proposal.proposal_digest().to_string(),
        plan_digest,
        changes,
        git: git_status(&root),
    })
}

pub fn confirm_source_initialization(
    state_root: &Path,
    store: &mut RootProposalStore,
    proposal_id: &str,
    now: SystemTime,
    options: &InitializationOptions,
    expected_plan_digest: &str,
) -> Result<InitializationReport, TransactionError> {
    let initial = fresh_proposal(store, proposal_id, now)?;
    if initial.kind() != RootProposalKind::SourceInitialization {
        return Err(TransactionError::WrongProposalKind);
    }
    ensure_external_path(state_root, initial.canonical_root())?;
    drop(initial);
    create_private_directory(state_root)?;
    let _lock = FileLock::acquire(&state_root.join("initialization.lock"))?;
    let recovered = recover_interrupted_operations(state_root)?;
    let proposal = fresh_proposal(store, proposal_id, now)?;
    let plan = build_initialization_plan(&proposal, options)?;
    if plan.plan_digest != expected_plan_digest {
        return Err(TransactionError::PlanConflict {
            expected: expected_plan_digest.to_string(),
            actual: plan.plan_digest,
        });
    }
    if plan.changes.is_empty() {
        return Ok(InitializationReport {
            changed_files: Vec::new(),
            recovered_interrupted_operation: recovered,
            git_before: plan.git.clone(),
            git_after: git_status(&plan.root),
            final_diffs: Vec::new(),
        });
    }
    let operation = prepare_operation(state_root, &plan)?;
    if let Err(error) = apply_operation(&operation) {
        let rollback = rollback_operation(&operation.directory);
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(TransactionError::Journal(format!(
                "initialization failed ({error}); rollback also failed ({rollback_error})"
            ))),
        };
    }
    mark_committed(&operation.directory)?;
    let changed_files = plan
        .changes
        .iter()
        .map(|change| change.path.clone())
        .collect::<Vec<_>>();
    let final_diffs = plan
        .changes
        .iter()
        .map(|change| change.diff.clone())
        .collect::<Vec<_>>();
    fs::remove_dir_all(&operation.directory)
        .map_err(|source| io_error("remove committed operation", &operation.directory, source))?;
    Ok(InitializationReport {
        changed_files,
        recovered_interrupted_operation: recovered,
        git_before: plan.git,
        git_after: git_status(&plan.root),
        final_diffs,
    })
}

fn fresh_proposal(
    store: &mut RootProposalStore,
    id: &str,
    now: SystemTime,
) -> Result<Box<RootProposal>, TransactionError> {
    match store
        .validate(id, now)
        .map_err(|error| TransactionError::Journal(error.to_string()))?
    {
        ProposalValidation::Fresh(proposal) => Ok(proposal),
        ProposalValidation::Stale => Err(TransactionError::ProposalStale),
        ProposalValidation::Expired => Err(TransactionError::ProposalExpired),
        ProposalValidation::Missing => Err(TransactionError::ProposalMissing),
    }
}

fn merge_missing_frontmatter(
    source: &str,
    fields: &BTreeMap<String, String>,
    path: &str,
) -> Result<String, TransactionError> {
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let opening_len = if source.starts_with("---\r\n") {
        5
    } else if source.starts_with("---\n") {
        4
    } else {
        let mut output = String::from("---");
        output.push_str(newline);
        for (key, value) in fields {
            output.push_str(key);
            output.push_str(": ");
            output.push_str(&yaml_scalar(value));
            output.push_str(newline);
        }
        output.push_str("---");
        output.push_str(newline);
        if !source.is_empty() {
            output.push_str(newline);
            output.push_str(source);
        }
        return Ok(output);
    };
    let closing = find_closing_frontmatter(source, opening_len)
        .ok_or_else(|| TransactionError::Journal(format!("invalid frontmatter in {path}")))?;
    let header = &source[opening_len..closing];
    let existing = top_level_keys(header);
    for key in fields.keys() {
        if existing.contains(key) {
            return Err(TransactionError::ExistingField {
                path: path.to_string(),
                field: key.clone(),
            });
        }
    }
    let mut output = String::new();
    output.push_str(&source[..closing]);
    if !output.ends_with(['\n', '\r']) {
        output.push_str(newline);
    }
    for (key, value) in fields {
        output.push_str(key);
        output.push_str(": ");
        output.push_str(&yaml_scalar(value));
        output.push_str(newline);
    }
    output.push_str(&source[closing..]);
    Ok(output)
}

fn append_resource_declarations(
    proposal: &RootProposal,
    options: &InitializationOptions,
    root: &Path,
    pending: &mut BTreeMap<String, (Option<String>, String)>,
) -> Result<(), TransactionError> {
    let undeclared = proposal
        .compliance()
        .resources()
        .iter()
        .filter(|resource| {
            resource.status() == crate::ResourceComplianceStatus::Undeclared
                && resource.csv().is_some_and(|csv| csv.valid_structure())
        })
        .map(|resource| resource.path().as_str().to_string())
        .collect::<BTreeSet<_>>();
    let mut by_index = BTreeMap::<String, Vec<(String, String)>>::new();
    for (path, resource_type) in &options.resource_types {
        if !undeclared.contains(path) {
            return Err(TransactionError::ResourceNotProposed(path.clone()));
        }
        if resource_type.trim().is_empty() {
            return Err(TransactionError::InvalidResourceType(path.clone()));
        }
        let index = path.rsplit_once('/').map_or_else(
            || "index.md".to_string(),
            |(directory, _)| format!("{directory}/index.md"),
        );
        by_index.entry(index).or_default().push((
            path.rsplit('/').next().unwrap_or(path).to_string(),
            resource_type.clone(),
        ));
    }
    for (index, declarations) in by_index {
        let target = safe_target(root, &index)?;
        let (before, current) = match pending.remove(&index) {
            Some(value) => value,
            None => {
                let source = fs::read_to_string(&target)
                    .map_err(|source| io_error("read", &target, source))?;
                (Some(source.clone()), source)
            }
        };
        let after = add_resources_field(&current, &declarations, &index)?;
        pending.insert(index, (before, after));
    }
    Ok(())
}

fn add_resources_field(
    source: &str,
    declarations: &[(String, String)],
    path: &str,
) -> Result<String, TransactionError> {
    let mut values = Vec::<serde_json::Value>::new();
    let (metadata, _) = crate::frontmatter::parse(source);
    if let Some(existing) = metadata.get("resources") {
        values = serde_json::from_str(existing).map_err(|_| TransactionError::ExistingField {
            path: path.to_string(),
            field: "resources".to_string(),
        })?;
    }
    let mut existing_paths = values
        .iter()
        .filter_map(|value| value.get("path").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    for (resource_path, resource_type) in declarations {
        if existing_paths.insert(resource_path.clone()) {
            values.push(serde_json::json!({
                "path": resource_path,
                "type": resource_type,
                "media_type": "text/csv; charset=utf-8"
            }));
        }
    }
    let serialized = serde_json::to_string(&values)
        .map_err(|error| TransactionError::Journal(error.to_string()))?;
    replace_or_insert_frontmatter_field(source, "resources", &serialized, path)
}

fn replace_or_insert_frontmatter_field(
    source: &str,
    key: &str,
    value: &str,
    path: &str,
) -> Result<String, TransactionError> {
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let opening_len = if source.starts_with("---\r\n") {
        5
    } else if source.starts_with("---\n") {
        4
    } else {
        return merge_missing_frontmatter(
            source,
            &BTreeMap::from([(key.to_string(), value.to_string())]),
            path,
        );
    };
    let closing = find_closing_frontmatter(source, opening_len)
        .ok_or_else(|| TransactionError::Journal(format!("invalid frontmatter in {path}")))?;
    let header = &source[opening_len..closing];
    let lines = header.split_inclusive('\n').collect::<Vec<_>>();
    let mut output_header = String::new();
    let mut index = 0;
    let mut replaced = false;
    while index < lines.len() {
        let line = lines[index];
        let top_level = !line.starts_with([' ', '\t']);
        let current_key = top_level
            .then(|| line.split_once(':').map(|(name, _)| name.trim()))
            .flatten();
        if current_key == Some(key) {
            if replaced {
                return Err(TransactionError::ExistingField {
                    path: path.to_string(),
                    field: key.to_string(),
                });
            }
            output_header.push_str(key);
            output_header.push_str(": ");
            output_header.push_str(value);
            output_header.push_str(newline);
            replaced = true;
            index += 1;
            while index < lines.len()
                && (lines[index].starts_with(' ') || lines[index].starts_with('\t'))
            {
                index += 1;
            }
            continue;
        }
        output_header.push_str(line);
        index += 1;
    }
    if !replaced {
        if !output_header.is_empty() && !output_header.ends_with(['\n', '\r']) {
            output_header.push_str(newline);
        }
        output_header.push_str(key);
        output_header.push_str(": ");
        output_header.push_str(value);
        output_header.push_str(newline);
    }
    Ok(format!(
        "{}{}{}",
        &source[..opening_len],
        output_header,
        &source[closing..]
    ))
}

fn find_closing_frontmatter(source: &str, opening_len: usize) -> Option<usize> {
    let mut offset = opening_len;
    for segment in source[opening_len..].split_inclusive('\n') {
        if segment.trim_end_matches(['\r', '\n']) == "---" {
            return Some(offset);
        }
        offset += segment.len();
    }
    None
}

fn top_level_keys(header: &str) -> BTreeSet<String> {
    header
        .lines()
        .filter(|line| !line.starts_with([' ', '\t']))
        .filter_map(|line| line.split_once(':').map(|(key, _)| key.trim().to_string()))
        .collect()
}

fn yaml_scalar(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || " -_./".contains(character))
    {
        value.to_string()
    } else {
        serde_json::to_string(value).expect("JSON string is a YAML scalar")
    }
}

fn exact_diff(path: &str, before: Option<&str>, after: &str) -> String {
    let mut output = format!("--- a/{path}\n+++ b/{path}\n");
    if let Some(before) = before {
        for line in before.lines() {
            output.push('-');
            output.push_str(line);
            output.push('\n');
        }
    }
    for line in after.lines() {
        output.push('+');
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn initialization_plan_digest(proposal_digest: &str, changes: &[SourceFileChange]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"okf-initialization-plan-v1");
    hash.update(proposal_digest.as_bytes());
    for change in changes {
        hash.update(change.path.len().to_le_bytes());
        hash.update(change.path.as_bytes());
        hash.update(change.before.as_deref().unwrap_or("").as_bytes());
        hash.update(change.after.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

#[derive(Debug, Serialize, Deserialize)]
struct OperationJournal {
    version: u32,
    status: String,
    root: PathBuf,
    entries: Vec<JournalEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JournalEntry {
    path: String,
    had_original: bool,
    before_hash: String,
    after_hash: String,
    permissions: Option<u32>,
}

struct PreparedOperation {
    directory: PathBuf,
}

fn prepare_operation(
    state_root: &Path,
    plan: &InitializationPlan,
) -> Result<PreparedOperation, TransactionError> {
    let directory = state_root.join(format!("operation-{}", &plan.plan_digest[..24]));
    if directory.exists() {
        rollback_operation(&directory)?;
    }
    let result = prepare_operation_files(&directory, plan);
    if result.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    result
}

fn prepare_operation_files(
    directory: &Path,
    plan: &InitializationPlan,
) -> Result<PreparedOperation, TransactionError> {
    create_private_directory(directory)?;
    create_private_directory(&directory.join("staged"))?;
    create_private_directory(&directory.join("backup"))?;
    let mut entries = Vec::new();
    for (index, change) in plan.changes.iter().enumerate() {
        let target = safe_target(&plan.root, &change.path)?;
        let current = match fs::read(&target) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => return Err(io_error("read", &target, source)),
        };
        if current.as_deref() != change.before.as_deref().map(str::as_bytes) {
            return Err(TransactionError::ProposalStale);
        }
        let staged = directory.join("staged").join(index.to_string());
        write_private(&staged, change.after.as_bytes())?;
        if let Some(bytes) = &current {
            write_private(&directory.join("backup").join(index.to_string()), bytes)?;
        }
        entries.push(JournalEntry {
            path: change.path.clone(),
            had_original: current.is_some(),
            before_hash: bytes_hash(current.as_deref().unwrap_or_default()),
            after_hash: bytes_hash(change.after.as_bytes()),
            permissions: source_permissions(&target),
        });
    }
    write_journal(
        directory,
        &OperationJournal {
            version: 1,
            status: "prepared".to_string(),
            root: plan.root.clone(),
            entries,
        },
    )?;
    Ok(PreparedOperation {
        directory: directory.to_path_buf(),
    })
}

fn apply_operation(operation: &PreparedOperation) -> Result<(), TransactionError> {
    let mut journal = read_journal(&operation.directory)?;
    journal.status = "applying".to_string();
    write_journal(&operation.directory, &journal)?;
    for (index, entry) in journal.entries.iter().enumerate() {
        let target = safe_target(&journal.root, &entry.path)?;
        let current = match fs::read(&target) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => return Err(io_error("revalidate", &target, source)),
        };
        let expected_before = if entry.had_original {
            Some(entry.before_hash.as_str())
        } else {
            None
        };
        if current.as_deref().map(bytes_hash).as_deref() != expected_before {
            return Err(TransactionError::ProposalStale);
        }
        let staged = fs::read(operation.directory.join("staged").join(index.to_string()))
            .map_err(|source| io_error("read staged file", &target, source))?;
        atomic_replace(&target, &staged, entry.permissions)?;
    }
    for entry in &journal.entries {
        let target = safe_target(&journal.root, &entry.path)?;
        let bytes = fs::read(&target).map_err(|source| io_error("verify", &target, source))?;
        if bytes_hash(&bytes) != entry.after_hash {
            return Err(TransactionError::Journal(format!(
                "verification failed for {}",
                entry.path
            )));
        }
    }
    Ok(())
}

fn mark_committed(directory: &Path) -> Result<(), TransactionError> {
    let mut journal = read_journal(directory)?;
    journal.status = "committed".to_string();
    write_journal(directory, &journal)
}

pub fn recover_interrupted_operations(state_root: &Path) -> Result<bool, TransactionError> {
    if !state_root.exists() {
        return Ok(false);
    }
    let mut recovered = false;
    for entry in fs::read_dir(state_root).map_err(|source| io_error("read", state_root, source))? {
        let entry = entry.map_err(|source| io_error("read", state_root, source))?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("operation-")
        {
            continue;
        }
        let directory = entry.path();
        let journal = read_journal(&directory)?;
        if journal.status == "committed" {
            fs::remove_dir_all(&directory)
                .map_err(|source| io_error("remove committed operation", &directory, source))?;
        } else {
            rollback_operation(&directory)?;
            recovered = true;
        }
    }
    Ok(recovered)
}

fn rollback_operation(directory: &Path) -> Result<(), TransactionError> {
    let journal = read_journal(directory)?;
    for (index, entry) in journal.entries.iter().enumerate().rev() {
        let target = safe_target(&journal.root, &entry.path)?;
        let current = match fs::read(&target) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => return Err(io_error("read rollback target", &target, source)),
        };
        let current_hash = current.as_deref().map(bytes_hash);
        if entry.had_original && current_hash.as_deref() == Some(entry.before_hash.as_str()) {
            continue;
        }
        if !entry.had_original && current.is_none() {
            continue;
        }
        if current_hash.as_deref() != Some(entry.after_hash.as_str()) {
            return Err(TransactionError::Journal(format!(
                "refusing rollback because {} has an unexpected state",
                entry.path
            )));
        }
        if entry.had_original {
            let backup = fs::read(directory.join("backup").join(index.to_string()))
                .map_err(|source| io_error("read rollback", &target, source))?;
            if bytes_hash(&backup) != entry.before_hash {
                return Err(TransactionError::Journal(format!(
                    "rollback backup verification failed for {}",
                    entry.path
                )));
            }
            atomic_replace(&target, &backup, entry.permissions)?;
        } else if target.exists() {
            fs::remove_file(&target).map_err(|source| io_error("remove", &target, source))?;
        }
    }
    fs::remove_dir_all(directory).map_err(|source| io_error("remove operation", directory, source))
}

fn write_journal(directory: &Path, journal: &OperationJournal) -> Result<(), TransactionError> {
    let bytes = serde_json::to_vec_pretty(journal)
        .map_err(|error| TransactionError::Journal(error.to_string()))?;
    let path = directory.join("journal.json");
    let temporary = directory.join("journal.json.tmp");
    write_private(&temporary, &bytes)?;
    fs::rename(&temporary, &path).map_err(|source| io_error("replace journal", &path, source))?;
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|source| io_error("sync", directory, source))
}

fn read_journal(directory: &Path) -> Result<OperationJournal, TransactionError> {
    let path = directory.join("journal.json");
    let bytes = fs::read(&path).map_err(|source| io_error("read journal", &path, source))?;
    let journal: OperationJournal = serde_json::from_slice(&bytes)
        .map_err(|error| TransactionError::Journal(error.to_string()))?;
    if journal.version != 1 {
        return Err(TransactionError::Journal(format!(
            "unsupported operation journal version {}",
            journal.version
        )));
    }
    Ok(journal)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), TransactionError> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|source| io_error("write", path, source))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| io_error("write", path, source))
}

fn atomic_replace(
    path: &Path,
    bytes: &[u8],
    permissions: Option<u32>,
) -> Result<(), TransactionError> {
    let parent = path
        .parent()
        .ok_or_else(|| TransactionError::UnsafePath(path.to_path_buf()))?;
    let temporary = parent.join(format!(
        ".okf-init-{}-{}",
        std::process::id(),
        bytes_hash(bytes)
    ));
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|source| io_error("remove", &temporary, source))?;
    }
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        #[cfg(unix)]
        if let Some(mode) = permissions {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))?;
        }
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok::<(), std::io::Error>(())
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&temporary);
        return Err(io_error("replace", path, source));
    }
    Ok(())
}

fn safe_target(root: &Path, logical: &str) -> Result<PathBuf, TransactionError> {
    let portable = crate::PortablePath::parse(logical.to_string())
        .map_err(|_| TransactionError::UnsafePath(PathBuf::from(logical)))?;
    let target = root.join(portable.as_str());
    let parent = target
        .parent()
        .ok_or_else(|| TransactionError::UnsafePath(target.clone()))?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|source| io_error("canonicalize", parent, source))?;
    if !canonical_parent.starts_with(root) {
        return Err(TransactionError::UnsafePath(target));
    }
    Ok(target)
}

fn ensure_external_path(path: &Path, root: &Path) -> Result<(), TransactionError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| io_error("read working directory for", path, source))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    if normalized.starts_with(root) {
        return Err(TransactionError::UnsafePath(path.to_path_buf()));
    }
    let mut ancestor = normalized.as_path();
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| TransactionError::UnsafePath(path.to_path_buf()))?;
    }
    let canonical = ancestor
        .canonicalize()
        .map_err(|source| io_error("canonicalize", ancestor, source))?;
    if canonical.starts_with(root) {
        return Err(TransactionError::UnsafePath(path.to_path_buf()));
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), TransactionError> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(TransactionError::UnsafePath(path.to_path_buf()));
    }
    fs::create_dir_all(path).map_err(|source| io_error("create directory", path, source))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|source| io_error("set permissions", path, source))?;
    }
    Ok(())
}

struct FileLock {
    file: File,
}

impl FileLock {
    fn acquire(path: &Path) -> Result<Self, TransactionError> {
        if let Some(parent) = path.parent() {
            create_private_directory(parent)?;
        }
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(TransactionError::UnsafePath(path.to_path_buf()));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|source| io_error("open lock", path, source))?;
        lock_file(&file).map_err(|source| io_error("lock", path, source))?;
        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = unlock_file(&self.file);
    }
}

#[cfg(unix)]
fn lock_file(file: &File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: flock only borrows this live file descriptor for the syscall.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn unlock_file(file: &File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: flock only borrows this live file descriptor for the syscall.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn lock_file(_: &File) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn unlock_file(_: &File) -> std::io::Result<()> {
    Ok(())
}

fn bytes_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(unix)]
fn source_permissions(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn source_permissions(_: &Path) -> Option<u32> {
    None
}

fn git_status(root: &Path) -> Option<GitWorktreeStatus> {
    let repository = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !repository.status.success() {
        return None;
    }
    let worktree = PathBuf::from(String::from_utf8(repository.stdout).ok()?.trim());
    let status = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["status", "--porcelain", "--"])
        .arg(root)
        .output()
        .ok()?;
    if !status.status.success() {
        return None;
    }
    let entries = String::from_utf8(status.stdout)
        .ok()?
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    Some(GitWorktreeStatus {
        root: worktree,
        dirty: !entries.is_empty(),
        entries,
    })
}

fn io_error(operation: &'static str, path: &Path, source: std::io::Error) -> TransactionError {
    TransactionError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
}
