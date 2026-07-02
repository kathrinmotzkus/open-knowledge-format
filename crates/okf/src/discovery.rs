use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::OkfError;

pub(crate) fn markdown_files(root: &Path) -> Result<Vec<PathBuf>, OkfError> {
    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), OkfError> {
    let entries = fs::read_dir(directory).map_err(|source| OkfError::ReadRoot {
        root: root.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| OkfError::ReadRoot {
            root: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files)?;
        } else if path.extension() == Some(OsStr::new("md")) {
            files.push(path);
        }
    }
    Ok(())
}
