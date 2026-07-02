use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use toml_dom::{Document as TomlDocument, Table, Value};

use crate::{DocumentRoot, RootId};

pub const BROWSER_CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserConfig {
    schema_version: u32,
    roots: Vec<BrowserRoot>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            schema_version: BROWSER_CONFIG_SCHEMA_VERSION,
            roots: Vec::new(),
        }
    }
}

impl BrowserConfig {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub fn roots(&self) -> &[BrowserRoot] {
        &self.roots
    }
    pub fn roots_mut(&mut self) -> &mut Vec<BrowserRoot> {
        &mut self.roots
    }

    pub fn enabled_document_roots(&self) -> Vec<DocumentRoot> {
        let mut roots = self
            .roots
            .iter()
            .enumerate()
            .filter(|(_, root)| root.enabled)
            .collect::<Vec<_>>();
        roots.sort_by_key(|(index, root)| (root.priority, *index));
        roots
            .into_iter()
            .map(|(_, root)| root.document_root())
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserRoot {
    pub root_id: RootId,
    pub mount: Option<String>,
    pub path: PathBuf,
    pub enabled: bool,
    pub priority: i64,
    pub check_for_changes: bool,
}

impl BrowserRoot {
    pub fn document_root(&self) -> DocumentRoot {
        match &self.mount {
            Some(mount) => DocumentRoot::mounted(mount, &self.path),
            None => DocumentRoot::new(&self.path),
        }
    }
}

#[derive(Debug)]
pub enum BrowserConfigError {
    MissingHome,
    RelativeXdgPath,
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        detail: String,
    },
    UnsupportedSchema(u32),
    InvalidField(String),
    DuplicateRootId(String),
    MissingRootIdentity(PathBuf),
    UnsafePath(PathBuf),
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for BrowserConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHome => {
                f.write_str("HOME is unavailable and XDG_CONFIG_HOME was not provided")
            }
            Self::RelativeXdgPath => f.write_str("XDG_CONFIG_HOME must be an absolute path"),
            Self::Read { path, .. } => {
                write!(f, "cannot read OKF configuration {}", path.display())
            }
            Self::Parse { path, detail } => {
                write!(f, "invalid OKF configuration {}: {detail}", path.display())
            }
            Self::UnsupportedSchema(version) => {
                write!(f, "unsupported OKF configuration schema {version}")
            }
            Self::InvalidField(field) => write!(f, "invalid OKF configuration field {field}"),
            Self::DuplicateRootId(id) => write!(f, "duplicate configured root identity {id}"),
            Self::MissingRootIdentity(path) => {
                write!(f, "root {} has no valid okf_root_id", path.display())
            }
            Self::UnsafePath(path) => write!(f, "unsafe configuration path {}", path.display()),
            Self::Write { path, .. } => {
                write!(f, "cannot write OKF configuration {}", path.display())
            }
        }
    }
}

impl Error for BrowserConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn browser_config_path(
    xdg: Option<&Path>,
    home: Option<&Path>,
) -> Result<PathBuf, BrowserConfigError> {
    let directory = match xdg {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(_) => return Err(BrowserConfigError::RelativeXdgPath),
        None => home.ok_or(BrowserConfigError::MissingHome)?.join(".config"),
    };
    Ok(directory.join("okf/config.toml"))
}

pub fn load_browser_config(path: &Path) -> Result<BrowserConfig, BrowserConfigError> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BrowserConfig::default())
        }
        Err(source) => {
            return Err(BrowserConfigError::Read {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    let document = TomlDocument::parse(&source).map_err(|error| BrowserConfigError::Parse {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })?;
    let version = integer(document.root(), "schema_version")?;
    let version = u32::try_from(version)
        .map_err(|_| BrowserConfigError::InvalidField("schema_version".into()))?;
    if version > BROWSER_CONFIG_SCHEMA_VERSION {
        return Err(BrowserConfigError::UnsupportedSchema(version));
    }
    let roots = match document.root().get("roots") {
        None => Vec::new(),
        Some(Value::Array(array)) => array
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let Value::Table(table) = value else {
                    return Err(BrowserConfigError::InvalidField(format!("roots[{index}]")));
                };
                parse_root(table, index, version)
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(BrowserConfigError::InvalidField("roots".into())),
    };
    validate_roots(&roots)?;
    Ok(BrowserConfig {
        schema_version: BROWSER_CONFIG_SCHEMA_VERSION,
        roots,
    })
}

fn parse_root(
    table: &Table,
    index: usize,
    version: u32,
) -> Result<BrowserRoot, BrowserConfigError> {
    let field = |name: &str| format!("roots[{index}].{name}");
    let root_id = RootId::parse(string(table, "root_id")?.to_string())
        .map_err(|_| BrowserConfigError::InvalidField(field("root_id")))?;
    let mount = optional_string(table, "mount")?;
    if mount
        .as_deref()
        .is_some_and(|mount| !crate::is_valid_mount_name(mount))
    {
        return Err(BrowserConfigError::InvalidField(field("mount")));
    }
    let path = PathBuf::from(string(table, "path")?);
    if !path.is_absolute() {
        return Err(BrowserConfigError::InvalidField(field("path")));
    }
    Ok(BrowserRoot {
        root_id,
        mount,
        path,
        enabled: if version == 0 {
            true
        } else {
            boolean(table, "enabled")?
        },
        priority: if version == 0 {
            index as i64 * 100
        } else {
            integer(table, "priority")?
        },
        check_for_changes: if version == 0 {
            false
        } else {
            boolean(table, "check_for_changes")?
        },
    })
}

pub fn import_document_roots(roots: &[DocumentRoot]) -> Result<BrowserConfig, BrowserConfigError> {
    let mut imported = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let path = root
            .path()
            .canonicalize()
            .map_err(|source| BrowserConfigError::Read {
                path: root.path().to_path_buf(),
                source,
            })?;
        let root_id = read_root_id(&path)
            .ok_or_else(|| BrowserConfigError::MissingRootIdentity(path.clone()))?;
        imported.push(BrowserRoot {
            root_id,
            mount: root
                .mount()
                .map(|value| value.to_string_lossy().into_owned()),
            path,
            enabled: true,
            priority: index as i64 * 100,
            check_for_changes: false,
        });
    }
    validate_roots(&imported)?;
    Ok(BrowserConfig {
        schema_version: BROWSER_CONFIG_SCHEMA_VERSION,
        roots: imported,
    })
}

fn read_root_id(root: &Path) -> Option<RootId> {
    let source = fs::read_to_string(root.join("index.md")).ok()?;
    let opening = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))?;
    for line in opening.lines().take_while(|line| *line != "---") {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim() == "okf_root_id" {
            return RootId::parse(value.trim().trim_matches('"').to_string()).ok();
        }
    }
    None
}

pub fn save_browser_config(path: &Path, config: &BrowserConfig) -> Result<(), BrowserConfigError> {
    validate_roots(&config.roots)?;
    let parent = path
        .parent()
        .ok_or_else(|| BrowserConfigError::UnsafePath(path.to_path_buf()))?;
    create_private_directory(parent)?;
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(BrowserConfigError::UnsafePath(path.to_path_buf()));
    }
    let source = serialize(config);
    atomic_write(path, source.as_bytes())
}

fn validate_roots(roots: &[BrowserRoot]) -> Result<(), BrowserConfigError> {
    let mut ids = BTreeSet::new();
    for root in roots {
        if !root.path.is_absolute() {
            return Err(BrowserConfigError::InvalidField("root.path".into()));
        }
        if !ids.insert(root.root_id.as_str()) {
            return Err(BrowserConfigError::DuplicateRootId(
                root.root_id.to_string(),
            ));
        }
    }
    Ok(())
}

fn serialize(config: &BrowserConfig) -> String {
    let mut output = format!("schema_version = {}\n", BROWSER_CONFIG_SCHEMA_VERSION);
    for root in &config.roots {
        output.push_str("\n[[roots]]\nroot_id = ");
        output.push_str(&quote(root.root_id.as_str()));
        output.push('\n');
        if let Some(mount) = &root.mount {
            output.push_str("mount = ");
            output.push_str(&quote(mount));
            output.push('\n');
        }
        output.push_str("path = ");
        output.push_str(&quote(&root.path.to_string_lossy()));
        output.push('\n');
        output.push_str(&format!(
            "enabled = {}\npriority = {}\ncheck_for_changes = {}\n",
            root.enabled, root.priority, root.check_for_changes
        ));
    }
    output
}

fn quote(value: &str) -> String {
    serde_json::to_string(value).expect("JSON string is valid TOML basic string")
}
fn integer(table: &Table, key: &str) -> Result<i64, BrowserConfigError> {
    match table.get(key) {
        Some(Value::Integer(v)) => Ok(*v),
        _ => Err(BrowserConfigError::InvalidField(key.into())),
    }
}
fn boolean(table: &Table, key: &str) -> Result<bool, BrowserConfigError> {
    match table.get(key) {
        Some(Value::Boolean(v)) => Ok(*v),
        _ => Err(BrowserConfigError::InvalidField(key.into())),
    }
}
fn string<'a>(table: &'a Table, key: &str) -> Result<&'a str, BrowserConfigError> {
    match table.get(key) {
        Some(Value::String(v)) => Ok(v),
        _ => Err(BrowserConfigError::InvalidField(key.into())),
    }
}
fn optional_string(table: &Table, key: &str) -> Result<Option<String>, BrowserConfigError> {
    match table.get(key) {
        None => Ok(None),
        Some(Value::String(v)) => Ok(Some(v.clone())),
        _ => Err(BrowserConfigError::InvalidField(key.into())),
    }
}

fn create_private_directory(path: &Path) -> Result<(), BrowserConfigError> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(BrowserConfigError::UnsafePath(path.to_path_buf()));
        }
    } else {
        fs::create_dir_all(path).map_err(|source| BrowserConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            BrowserConfigError::Write {
                path: path.to_path_buf(),
                source,
            }
        })?;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), BrowserConfigError> {
    let parent = path.parent().expect("validated parent");
    let temporary = parent.join(format!(".config.toml.tmp-{}", std::process::id()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        fs::File::open(parent)?.sync_all()?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|source| BrowserConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}
