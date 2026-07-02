use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::{Component, Path};

use unicode_normalization::UnicodeNormalization;

const ROOT_ID_PREFIX: &str = "urn:okf:root:";
const DOCUMENT_ID_PREFIX: &str = "urn:okf:document:";

/// A portable identity stored as `okf_root_id` in the bundle-root `index.md`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RootId(String);

impl RootId {
    pub fn parse(value: impl Into<String>) -> Result<Self, IdentityError> {
        parse_identity(value.into(), ROOT_ID_PREFIX).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RootId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A portable identity stored as `okf_document_id` in concept frontmatter.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn parse(value: impl Into<String>) -> Result<Self, IdentityError> {
        parse_identity(value.into(), DOCUMENT_ID_PREFIX).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn parse_identity(value: String, prefix: &str) -> Result<String, IdentityError> {
    let Some(token) = value.strip_prefix(prefix) else {
        return Err(IdentityError::InvalidPrefix);
    };
    if !(16..=128).contains(&token.len()) {
        return Err(IdentityError::InvalidLength);
    }
    if !token
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(IdentityError::InvalidCharacter);
    }
    Ok(value)
}

/// A validated relative UTF-8 path and its separate collision key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortablePath {
    display: String,
    comparison_key: String,
}

impl PortablePath {
    /// Validates an already portable `/`-separated path.
    pub fn parse(value: impl Into<String>) -> Result<Self, IdentityError> {
        let display = value.into();
        validate_portable_string(&display)?;
        Ok(Self {
            comparison_key: comparison_key(&display),
            display,
        })
    }

    /// Converts a platform path without rewriting any UTF-8 component.
    pub fn from_path(path: &Path) -> Result<Self, IdentityError> {
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(value) => {
                    let value = value.to_str().ok_or(IdentityError::NonUtf8)?;
                    validate_component(value)?;
                    components.push(value);
                }
                Component::CurDir => return Err(IdentityError::CurrentDirectory),
                Component::ParentDir => return Err(IdentityError::ParentDirectory),
                Component::RootDir | Component::Prefix(_) => {
                    return Err(IdentityError::AbsoluteOrPrefixed)
                }
            }
        }
        if components.is_empty() {
            return Err(IdentityError::EmptyPath);
        }
        Self::parse(components.join("/"))
    }

    pub fn as_str(&self) -> &str {
        &self.display
    }

    pub fn comparison_key(&self) -> &str {
        &self.comparison_key
    }
}

impl fmt::Display for PortablePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.display)
    }
}

/// Paths that cannot coexist in one portable OKF identity namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathCollision {
    comparison_key: String,
    paths: Vec<PortablePath>,
}

impl PathCollision {
    pub fn comparison_key(&self) -> &str {
        &self.comparison_key
    }

    pub fn paths(&self) -> &[PortablePath] {
        &self.paths
    }
}

pub fn detect_path_collisions(paths: impl IntoIterator<Item = PortablePath>) -> Vec<PathCollision> {
    let mut grouped = BTreeMap::<String, Vec<PortablePath>>::new();
    for path in paths {
        grouped
            .entry(path.comparison_key.clone())
            .or_default()
            .push(path);
    }
    grouped
        .into_iter()
        .filter_map(|(comparison_key, paths)| {
            (paths.len() > 1).then_some(PathCollision {
                comparison_key,
                paths,
            })
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityError {
    EmptyPath,
    EmptyComponent,
    AbsoluteOrPrefixed,
    CurrentDirectory,
    ParentDirectory,
    NonUtf8,
    Backslash,
    ControlCharacter,
    InvalidPrefix,
    InvalidLength,
    InvalidCharacter,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyPath => "path is empty",
            Self::EmptyComponent => "path contains an empty component",
            Self::AbsoluteOrPrefixed => "path is absolute or platform-prefixed",
            Self::CurrentDirectory => "path contains a current-directory component",
            Self::ParentDirectory => "path contains a parent-directory component",
            Self::NonUtf8 => "path is not valid UTF-8",
            Self::Backslash => "portable paths must use forward slashes",
            Self::ControlCharacter => "path contains a control character",
            Self::InvalidPrefix => "identity has the wrong namespace prefix",
            Self::InvalidLength => "identity token length must be between 16 and 128 bytes",
            Self::InvalidCharacter => "identity token contains a non-portable character",
        };
        formatter.write_str(message)
    }
}

impl Error for IdentityError {}

fn validate_portable_string(value: &str) -> Result<(), IdentityError> {
    if value.is_empty() {
        return Err(IdentityError::EmptyPath);
    }
    if value.starts_with('/') || looks_like_windows_prefix(value) {
        return Err(IdentityError::AbsoluteOrPrefixed);
    }
    if value.contains('\\') {
        return Err(IdentityError::Backslash);
    }
    for component in value.split('/') {
        if component.is_empty() {
            return Err(IdentityError::EmptyComponent);
        }
        if component == "." {
            return Err(IdentityError::CurrentDirectory);
        }
        if component == ".." {
            return Err(IdentityError::ParentDirectory);
        }
        validate_component(component)?;
    }
    Ok(())
}

fn validate_component(value: &str) -> Result<(), IdentityError> {
    if value.chars().any(char::is_control) {
        return Err(IdentityError::ControlCharacter);
    }
    Ok(())
}

fn looks_like_windows_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn comparison_key(value: &str) -> String {
    value
        .split('/')
        .map(|component| {
            let normalized = component.nfkc().collect::<String>();
            unicode_case_fold(&normalized)
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn unicode_case_fold(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            // Unicode default case-fold expansions that `to_lowercase` cannot
            // express through a single source character.
            '\u{00df}' | '\u{1e9e}' => "ss".chars().collect::<Vec<_>>(),
            // Final sigma and normal sigma share one case-fold identity.
            '\u{03c2}' => "\u{03c3}".chars().collect::<Vec<_>>(),
            _ => character.to_lowercase().collect::<Vec<_>>(),
        })
        .collect::<String>()
        .nfkc()
        .collect()
}
