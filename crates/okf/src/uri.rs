use std::fmt;
use std::path::{Component, Path};

use crate::is_valid_mount_name;

/// Stable logical URI for a document in a mounted OKF root.
///
/// The authority is the configured mount name and the URI path is the UTF-8
/// source-relative document path. Physical filesystem paths are never exposed.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OkfUri(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OkfUriError {
    InvalidMount,
    InvalidPath,
}

impl OkfUri {
    pub fn from_mount_and_path(mount: &str, path: &Path) -> Result<Self, OkfUriError> {
        if !is_valid_mount_name(mount) {
            return Err(OkfUriError::InvalidMount);
        }
        let path = portable_path(path)?;
        Ok(Self(format!("okf://{mount}/{}", encode_path(&path))))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OkfUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for OkfUriError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMount => formatter.write_str("invalid OKF URI mount"),
            Self::InvalidPath => formatter.write_str("invalid OKF URI document path"),
        }
    }
}

impl std::error::Error for OkfUriError {}

fn portable_path(path: &Path) -> Result<String, OkfUriError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(OkfUriError::InvalidPath);
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(OkfUriError::InvalidPath);
        }
    }
    path.to_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(OkfUriError::InvalidPath)
}

fn encode_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(&mut encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounted_document_uri_is_logical_and_percent_encoded() {
        let uri =
            OkfUri::from_mount_and_path("okf", Path::new("security/Überblick mit Leerzeichen.md"))
                .unwrap();
        assert_eq!(
            uri.as_str(),
            "okf://okf/security/%C3%9Cberblick%20mit%20Leerzeichen.md"
        );
    }

    #[test]
    fn document_uri_rejects_physical_and_traversing_paths() {
        assert_eq!(
            OkfUri::from_mount_and_path("okf", Path::new("/etc/passwd")),
            Err(OkfUriError::InvalidPath)
        );
        assert_eq!(
            OkfUri::from_mount_and_path("okf", Path::new("../secret.md")),
            Err(OkfUriError::InvalidPath)
        );
    }
}
