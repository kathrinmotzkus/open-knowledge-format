use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};

use crate::DocumentRoot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RootSpecError {
    Empty,
    EmptyMount,
    EmptyPath,
    InvalidMount(String),
}

impl fmt::Display for RootSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("document root must not be empty"),
            Self::EmptyMount => formatter.write_str("document root mount must not be empty"),
            Self::EmptyPath => formatter.write_str("document root path must not be empty"),
            Self::InvalidMount(mount) => write!(
                formatter,
                "invalid document root mount {mount:?}; use ASCII letters, digits, '-' or '_'"
            ),
        }
    }
}

impl std::error::Error for RootSpecError {}

pub fn is_valid_mount_name(mount: &str) -> bool {
    !mount.is_empty()
        && mount != "."
        && mount != ".."
        && mount
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

pub fn parse_document_root_spec(spec: impl AsRef<OsStr>) -> Result<DocumentRoot, RootSpecError> {
    let spec = spec.as_ref();
    if spec.is_empty() {
        return Err(RootSpecError::Empty);
    }
    let Some(value) = spec.to_str() else {
        return Ok(DocumentRoot::new(PathBuf::from(spec)));
    };
    let Some((mount, path)) = value.split_once('=') else {
        return Ok(DocumentRoot::new(value));
    };
    if mount.is_empty() {
        return Err(RootSpecError::EmptyMount);
    }
    if path.is_empty() {
        return Err(RootSpecError::EmptyPath);
    }
    if !is_valid_mount_name(mount) {
        return Err(RootSpecError::InvalidMount(mount.to_string()));
    }
    Ok(DocumentRoot::mounted(mount, path))
}

pub fn format_document_root_spec(root: &DocumentRoot) -> OsString {
    match root.mount() {
        Some(mount) => {
            let mut value = mount.as_os_str().to_os_string();
            value.push("=");
            value.push(root.path());
            value
        }
        None => root.path().as_os_str().to_os_string(),
    }
}

pub fn merge_document_roots(
    defaults: Vec<DocumentRoot>,
    dotenv: Option<Vec<DocumentRoot>>,
    environment: Option<Vec<DocumentRoot>>,
    cli_overrides: Vec<DocumentRoot>,
    cli_additions: Vec<DocumentRoot>,
) -> Vec<DocumentRoot> {
    merge_document_roots_with_browser(
        defaults,
        Vec::new(),
        dotenv,
        environment,
        cli_overrides,
        cli_additions,
    )
}

pub fn merge_document_roots_with_browser(
    defaults: Vec<DocumentRoot>,
    browser: Vec<DocumentRoot>,
    dotenv: Option<Vec<DocumentRoot>>,
    environment: Option<Vec<DocumentRoot>>,
    cli_overrides: Vec<DocumentRoot>,
    cli_additions: Vec<DocumentRoot>,
) -> Vec<DocumentRoot> {
    let base = environment.or(dotenv).unwrap_or({
        if browser.is_empty() {
            defaults
        } else {
            browser
        }
    });
    let overridden_mounts = cli_overrides
        .iter()
        .filter_map(|root| root.mount().map(Path::to_path_buf))
        .collect::<BTreeSet<_>>();

    deduplicate_document_roots(
        cli_overrides
            .into_iter()
            .chain(base.into_iter().filter(|root| {
                root.mount()
                    .is_none_or(|mount| !overridden_mounts.contains(mount))
            }))
            .chain(cli_additions),
    )
}

pub fn deduplicate_document_roots(
    roots: impl IntoIterator<Item = DocumentRoot>,
) -> Vec<DocumentRoot> {
    let mut seen = BTreeSet::new();
    roots
        .into_iter()
        .filter(|root| {
            seen.insert((
                root.mount().map(Path::to_path_buf),
                root.path().to_path_buf(),
            ))
        })
        .collect()
}
