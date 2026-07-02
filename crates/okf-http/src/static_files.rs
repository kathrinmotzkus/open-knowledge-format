use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use okf::{
    analyze_compliance, is_valid_mount_name, scan_document_root, AdmissionLimits, AdmittedFormat,
    DocumentRoot, PortablePath, ResourceComplianceStatus,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StaticFileError {
    EscapesRoot,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedOkfFile {
    pub(crate) path: PathBuf,
    pub(crate) content_type: &'static str,
}

pub(crate) fn resolve_static_file(
    root: &Path,
    request_path: &str,
) -> Result<PathBuf, StaticFileError> {
    let requested = Path::new(request_path);
    if requested.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(StaticFileError::EscapesRoot);
    }
    let root = root.canonicalize().map_err(|_| StaticFileError::Missing)?;
    let file = root
        .join(requested)
        .canonicalize()
        .map_err(|_| StaticFileError::Missing)?;
    if !file.starts_with(&root) {
        return Err(StaticFileError::EscapesRoot);
    }
    file.is_file()
        .then_some(file)
        .ok_or(StaticFileError::Missing)
}

pub(crate) fn resolve_admitted_file(
    root: &Path,
    request_path: &str,
) -> Result<ResolvedOkfFile, StaticFileError> {
    let requested =
        PortablePath::parse(request_path.to_string()).map_err(|_| StaticFileError::EscapesRoot)?;
    let inventory = scan_document_root(root, AdmissionLimits::default())
        .map_err(|_| StaticFileError::Missing)?;
    if !inventory.can_confirm() {
        return Err(StaticFileError::Missing);
    }
    let admitted = inventory
        .accepted()
        .iter()
        .find(|file| file.path() == &requested)
        .ok_or(StaticFileError::Missing)?;
    let content_type = match admitted.format() {
        AdmittedFormat::Markdown => "text/markdown; charset=utf-8",
        AdmittedFormat::Csv => {
            let compliance = analyze_compliance(&inventory);
            let resource = compliance
                .resources()
                .iter()
                .find(|resource| resource.path() == &requested)
                .ok_or(StaticFileError::Missing)?;
            if resource.status() != ResourceComplianceStatus::Declared
                || !resource
                    .csv()
                    .is_some_and(|analysis| analysis.valid_structure())
            {
                return Err(StaticFileError::Missing);
            }
            "text/csv; charset=utf-8"
        }
    };
    let path = resolve_static_file(root, requested.as_str())?;
    Ok(ResolvedOkfFile { path, content_type })
}

#[cfg(test)]
pub(crate) fn resolve_admitted_document_file(
    roots: &[DocumentRoot],
    mount: &str,
    request_path: &str,
) -> Result<ResolvedOkfFile, StaticFileError> {
    if !is_valid_mount_name(mount) {
        return Err(StaticFileError::EscapesRoot);
    }
    for root in roots
        .iter()
        .filter(|root| root_mount_name(root).as_deref() == Some(mount))
    {
        match resolve_admitted_file(root.path(), request_path) {
            Ok(file) => return Ok(file),
            Err(StaticFileError::EscapesRoot) => return Err(StaticFileError::EscapesRoot),
            Err(StaticFileError::Missing) => {}
        }
    }
    Err(StaticFileError::Missing)
}

pub(crate) fn root_mount_name(root: &DocumentRoot) -> Option<String> {
    root.mount().and_then(|mount| {
        let value = mount.to_string_lossy();
        is_valid_mount_name(&value).then(|| value.to_string())
    })
}

pub(crate) fn is_allowed_repo_file(path: &str) -> bool {
    matches!(path, "README.md" | "README.de.md" | "HOSTS.md")
}

pub(crate) fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(OsStr::to_str) {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("md") => "text/markdown; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
