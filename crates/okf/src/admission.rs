use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{detect_path_collisions, PortablePath};

pub const DEFAULT_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub const DEFAULT_MAX_ENTRIES: u64 = 10_000;
pub const DEFAULT_MAX_DEPTH: u64 = 32;
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

const HARD_MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const HARD_MAX_ENTRIES: u64 = 1_000_000;
const HARD_MAX_DEPTH: u64 = 256;
const HARD_MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdmissionLimits {
    pub max_file_bytes: u64,
    pub max_entries: u64,
    pub max_depth: u64,
    pub max_total_bytes: u64,
}

impl Default for AdmissionLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_depth: DEFAULT_MAX_DEPTH,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
        }
    }
}

impl AdmissionLimits {
    pub fn validate(self) -> Result<Self, AdmissionError> {
        validate_limit("max_file_bytes", self.max_file_bytes, HARD_MAX_FILE_BYTES)?;
        validate_limit("max_entries", self.max_entries, HARD_MAX_ENTRIES)?;
        validate_limit("max_depth", self.max_depth, HARD_MAX_DEPTH)?;
        validate_limit(
            "max_total_bytes",
            self.max_total_bytes,
            HARD_MAX_TOTAL_BYTES,
        )?;
        Ok(self)
    }
}

fn validate_limit(name: &'static str, value: u64, maximum: u64) -> Result<(), AdmissionError> {
    if value == 0 || value > maximum {
        return Err(AdmissionError::InvalidLimit {
            name,
            value,
            maximum,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmittedFormat {
    Markdown,
    Csv,
}

impl AdmittedFormat {
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Markdown => "text/markdown; charset=utf-8",
            Self::Csv => "text/csv; charset=utf-8",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedFile {
    path: PortablePath,
    format: AdmittedFormat,
    size: u64,
    content_hash: String,
}

impl AdmittedFile {
    pub fn path(&self) -> &PortablePath {
        &self.path
    }

    pub fn format(&self) -> AdmittedFormat {
        self.format
    }

    pub fn media_type(&self) -> &'static str {
        self.format.media_type()
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RejectedEntry {
    display_path: String,
    portable_path: Option<PortablePath>,
    reason: RejectionReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotEntryKind {
    Directory,
    RegularFile,
    Symlink,
    Other,
    Hidden,
    Unreadable,
    InvalidPath,
}

impl SnapshotEntryKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::RegularFile => "regular_file",
            Self::Symlink => "symlink",
            Self::Other => "other",
            Self::Hidden => "hidden",
            Self::Unreadable => "unreadable",
            Self::InvalidPath => "invalid_path",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotEntry {
    path: String,
    kind: SnapshotEntryKind,
    size: Option<u64>,
    content_hash: Option<String>,
    permissions: Option<u32>,
}

impl SnapshotEntry {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn kind(&self) -> SnapshotEntryKind {
        self.kind
    }
    pub fn size(&self) -> Option<u64> {
        self.size
    }
    pub fn content_hash(&self) -> Option<&str> {
        self.content_hash.as_deref()
    }
    pub fn permissions(&self) -> Option<u32> {
        self.permissions
    }
}

impl RejectedEntry {
    pub fn display_path(&self) -> &str {
        &self.display_path
    }

    pub fn portable_path(&self) -> Option<&PortablePath> {
        self.portable_path.as_ref()
    }

    pub fn reason(&self) -> &RejectionReason {
        &self.reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RejectionReason {
    HiddenPath,
    NonUtf8Path,
    InvalidPortablePath,
    Symlink,
    Socket,
    Fifo,
    Device,
    NonRegularFile,
    UnsupportedExtension,
    Executable,
    BinarySignature,
    InvalidUtf8,
    NulByte,
    SuspiciousControlCharacter,
    PathCollision,
    ReadFailed,
}

impl RejectionReason {
    pub fn code(&self) -> &'static str {
        match self {
            Self::HiddenPath => "hidden_path",
            Self::NonUtf8Path => "non_utf8_path",
            Self::InvalidPortablePath => "invalid_portable_path",
            Self::Symlink => "symlink",
            Self::Socket => "socket",
            Self::Fifo => "fifo",
            Self::Device => "device",
            Self::NonRegularFile => "non_regular_file",
            Self::UnsupportedExtension => "unsupported_extension",
            Self::Executable => "executable",
            Self::BinarySignature => "binary_signature",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::NulByte => "nul_byte",
            Self::SuspiciousControlCharacter => "suspicious_control_character",
            Self::PathCollision => "path_collision",
            Self::ReadFailed => "read_failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    FileBytes,
    Entries,
    Depth,
    TotalBytes,
}

impl LimitKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::FileBytes => "max_file_bytes",
            Self::Entries => "max_entries",
            Self::Depth => "max_depth",
            Self::TotalBytes => "max_total_bytes",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdmissionStatus {
    Complete,
    LimitExceeded {
        kind: LimitKind,
        configured: u64,
        observed_at_least: u64,
        path: String,
    },
}

impl AdmissionStatus {
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmissionInventory {
    root: PathBuf,
    accepted: Vec<AdmittedFile>,
    rejected: Vec<RejectedEntry>,
    status: AdmissionStatus,
    inspected_entries: u64,
    inspected_bytes: u64,
    snapshot: Vec<SnapshotEntry>,
}

impl AdmissionInventory {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn accepted(&self) -> &[AdmittedFile] {
        &self.accepted
    }

    pub fn rejected(&self) -> &[RejectedEntry] {
        &self.rejected
    }

    pub fn status(&self) -> &AdmissionStatus {
        &self.status
    }

    pub fn inspected_entries(&self) -> u64 {
        self.inspected_entries
    }

    pub fn inspected_bytes(&self) -> u64 {
        self.inspected_bytes
    }

    pub fn snapshot(&self) -> &[SnapshotEntry] {
        &self.snapshot
    }

    /// Incomplete inventories are never confirmable.
    pub fn can_confirm(&self) -> bool {
        self.status.is_complete()
    }
}

#[derive(Debug)]
pub enum AdmissionError {
    InvalidLimit {
        name: &'static str,
        value: u64,
        maximum: u64,
    },
    RootUnavailable {
        root: PathBuf,
        source: io::Error,
    },
    RootNotDirectory {
        root: PathBuf,
    },
    HiddenRoot {
        root: PathBuf,
    },
    RootSymlink {
        root: PathBuf,
    },
    ReadDirectory {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimit {
                name,
                value,
                maximum,
            } => write!(
                formatter,
                "invalid admission limit {name}={value}; expected 1..={maximum}"
            ),
            Self::RootUnavailable { root, .. } => {
                write!(formatter, "cannot inspect document root {}", root.display())
            }
            Self::RootNotDirectory { root } => {
                write!(
                    formatter,
                    "document root is not a directory: {}",
                    root.display()
                )
            }
            Self::HiddenRoot { root } => {
                write!(
                    formatter,
                    "hidden document roots are not admitted: {}",
                    root.display()
                )
            }
            Self::RootSymlink { root } => {
                write!(
                    formatter,
                    "symlink document roots are not admitted: {}",
                    root.display()
                )
            }
            Self::ReadDirectory { path, .. } => {
                write!(formatter, "cannot inspect directory {}", path.display())
            }
        }
    }
}

impl Error for AdmissionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RootUnavailable { source, .. } | Self::ReadDirectory { source, .. } => {
                Some(source)
            }
            _ => None,
        }
    }
}

pub fn scan_document_root(
    root: impl AsRef<Path>,
    limits: AdmissionLimits,
) -> Result<AdmissionInventory, AdmissionError> {
    let root = root.as_ref().to_path_buf();
    let limits = limits.validate()?;
    let metadata =
        fs::symlink_metadata(&root).map_err(|source| AdmissionError::RootUnavailable {
            root: root.clone(),
            source,
        })?;
    if metadata.file_type().is_symlink() {
        return Err(AdmissionError::RootSymlink { root });
    }
    if !metadata.is_dir() {
        return Err(AdmissionError::RootNotDirectory { root });
    }
    if root
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
    {
        return Err(AdmissionError::HiddenRoot { root });
    }

    let mut scanner = Scanner {
        root: root.clone(),
        limits,
        accepted: Vec::new(),
        rejected: Vec::new(),
        status: AdmissionStatus::Complete,
        inspected_entries: 0,
        inspected_bytes: 0,
        snapshot: Vec::new(),
    };
    scanner.walk(&root, 0)?;
    scanner.reject_collisions();
    scanner
        .accepted
        .sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
    scanner
        .rejected
        .sort_by(|left, right| left.display_path.cmp(&right.display_path));
    scanner
        .snapshot
        .sort_by(|left, right| left.path.cmp(&right.path));
    Ok(AdmissionInventory {
        root,
        accepted: scanner.accepted,
        rejected: scanner.rejected,
        status: scanner.status,
        inspected_entries: scanner.inspected_entries,
        inspected_bytes: scanner.inspected_bytes,
        snapshot: scanner.snapshot,
    })
}

struct Scanner {
    root: PathBuf,
    limits: AdmissionLimits,
    accepted: Vec<AdmittedFile>,
    rejected: Vec<RejectedEntry>,
    status: AdmissionStatus,
    inspected_entries: u64,
    inspected_bytes: u64,
    snapshot: Vec<SnapshotEntry>,
}

impl Scanner {
    fn walk(&mut self, directory: &Path, depth: u64) -> Result<bool, AdmissionError> {
        let entries = fs::read_dir(directory).map_err(|source| AdmissionError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let mut entries = entries.collect::<Result<Vec<_>, _>>().map_err(|source| {
            AdmissionError::ReadDirectory {
                path: directory.to_path_buf(),
                source,
            }
        })?;
        entries.sort_by_key(fs::DirEntry::file_name);

        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(&self.root).unwrap_or(&path);
            let display = relative.to_string_lossy().replace('\\', "/");
            self.inspected_entries = self.inspected_entries.saturating_add(1);
            if self.inspected_entries > self.limits.max_entries {
                self.exceed(
                    LimitKind::Entries,
                    self.limits.max_entries,
                    self.inspected_entries,
                    display,
                );
                return Ok(true);
            }
            let entry_depth = depth.saturating_add(1);
            if entry_depth > self.limits.max_depth {
                self.exceed(
                    LimitKind::Depth,
                    self.limits.max_depth,
                    entry_depth,
                    display,
                );
                return Ok(true);
            }

            let portable = match PortablePath::from_path(relative) {
                Ok(path) => Some(path),
                Err(crate::IdentityError::NonUtf8) => {
                    self.snapshot(display.clone(), SnapshotEntryKind::InvalidPath, None, None);
                    self.reject(display, None, RejectionReason::NonUtf8Path);
                    continue;
                }
                Err(_) => {
                    self.snapshot(display.clone(), SnapshotEntryKind::InvalidPath, None, None);
                    self.reject(display, None, RejectionReason::InvalidPortablePath);
                    continue;
                }
            };
            if relative
                .components()
                .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
            {
                self.snapshot(display.clone(), SnapshotEntryKind::Hidden, None, None);
                self.reject(display, portable, RejectionReason::HiddenPath);
                continue;
            }

            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    self.snapshot(display.clone(), SnapshotEntryKind::Unreadable, None, None);
                    self.reject(display, portable, RejectionReason::ReadFailed);
                    continue;
                }
            };
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                self.snapshot(
                    display.clone(),
                    SnapshotEntryKind::Symlink,
                    None,
                    metadata_permissions(&metadata),
                );
                self.reject(display, portable, RejectionReason::Symlink);
                continue;
            }
            if file_type.is_dir() {
                self.snapshot(
                    display,
                    SnapshotEntryKind::Directory,
                    None,
                    metadata_permissions(&metadata),
                );
                if self.walk(&path, entry_depth)? {
                    return Ok(true);
                }
                continue;
            }
            if !file_type.is_file() {
                self.snapshot(
                    display.clone(),
                    SnapshotEntryKind::Other,
                    None,
                    metadata_permissions(&metadata),
                );
                self.reject(display, portable, classify_non_regular(&file_type));
                continue;
            }
            self.snapshot(
                display.clone(),
                SnapshotEntryKind::RegularFile,
                Some(metadata.len()),
                metadata_permissions(&metadata),
            );
            if is_executable(&metadata) {
                self.reject(display, portable, RejectionReason::Executable);
                continue;
            }
            let Some(format) = admitted_format(&path) else {
                self.reject(display, portable, RejectionReason::UnsupportedExtension);
                continue;
            };
            if metadata.len() > self.limits.max_file_bytes {
                self.exceed(
                    LimitKind::FileBytes,
                    self.limits.max_file_bytes,
                    metadata.len(),
                    display,
                );
                return Ok(true);
            }
            let next_total = self.inspected_bytes.saturating_add(metadata.len());
            if next_total > self.limits.max_total_bytes {
                self.exceed(
                    LimitKind::TotalBytes,
                    self.limits.max_total_bytes,
                    next_total,
                    display,
                );
                return Ok(true);
            }
            self.inspected_bytes = next_total;
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(_) => {
                    self.reject(display, portable, RejectionReason::ReadFailed);
                    continue;
                }
            };
            let content_hash = format!("{:x}", Sha256::digest(&bytes));
            if let Some(entry) = self.snapshot.last_mut() {
                entry.content_hash = Some(content_hash.clone());
            }
            if has_binary_signature(&bytes) {
                self.reject(display, portable, RejectionReason::BinarySignature);
                continue;
            }
            if bytes.contains(&0) {
                self.reject(display, portable, RejectionReason::NulByte);
                continue;
            }
            let Ok(text) = std::str::from_utf8(&bytes) else {
                self.reject(display, portable, RejectionReason::InvalidUtf8);
                continue;
            };
            if text
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
            {
                self.reject(
                    display,
                    portable,
                    RejectionReason::SuspiciousControlCharacter,
                );
                continue;
            }
            self.accepted.push(AdmittedFile {
                path: portable.expect("regular UTF-8 path was validated"),
                format,
                size: metadata.len(),
                content_hash,
            });
        }
        Ok(false)
    }

    fn reject(
        &mut self,
        display_path: String,
        portable_path: Option<PortablePath>,
        reason: RejectionReason,
    ) {
        self.rejected.push(RejectedEntry {
            display_path,
            portable_path,
            reason,
        });
    }

    fn snapshot(
        &mut self,
        path: String,
        kind: SnapshotEntryKind,
        size: Option<u64>,
        permissions: Option<u32>,
    ) {
        self.snapshot.push(SnapshotEntry {
            path,
            kind,
            size,
            content_hash: None,
            permissions,
        });
    }

    fn exceed(&mut self, kind: LimitKind, configured: u64, observed: u64, path: String) {
        self.status = AdmissionStatus::LimitExceeded {
            kind,
            configured,
            observed_at_least: observed,
            path,
        };
    }

    fn reject_collisions(&mut self) {
        let collisions = detect_path_collisions(
            self.accepted
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
        );
        if collisions.is_empty() {
            return;
        }
        let keys = collisions
            .into_iter()
            .map(|collision| collision.comparison_key().to_string())
            .collect::<Vec<_>>();
        let mut retained = Vec::new();
        let accepted = std::mem::take(&mut self.accepted);
        for file in accepted {
            if keys.iter().any(|key| key == file.path.comparison_key()) {
                self.reject(
                    file.path.as_str().to_string(),
                    Some(file.path),
                    RejectionReason::PathCollision,
                );
            } else {
                retained.push(file);
            }
        }
        self.accepted = retained;
    }
}

fn admitted_format(path: &Path) -> Option<AdmittedFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "md" | "markdown" => Some(AdmittedFormat::Markdown),
        "csv" => Some(AdmittedFormat::Csv),
        _ => None,
    }
}

fn has_binary_signature(bytes: &[u8]) -> bool {
    const SIGNATURES: &[&[u8]] = &[
        b"\x7fELF",
        b"MZ",
        b"%PDF-",
        b"\x89PNG\r\n\x1a\n",
        b"PK\x03\x04",
        b"\x1f\x8b",
        b"\0asm",
    ];
    SIGNATURES
        .iter()
        .any(|signature| bytes.starts_with(signature))
        || bytes.starts_with(b"#!")
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn metadata_permissions(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(metadata.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn metadata_permissions(metadata: &fs::Metadata) -> Option<u32> {
    Some(u32::from(metadata.permissions().readonly()))
}

fn classify_non_regular(file_type: &fs::FileType) -> RejectionReason {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if file_type.is_socket() {
            return RejectionReason::Socket;
        }
        if file_type.is_fifo() {
            return RejectionReason::Fifo;
        }
        if file_type.is_block_device() || file_type.is_char_device() {
            return RejectionReason::Device;
        }
    }
    RejectionReason::NonRegularFile
}
