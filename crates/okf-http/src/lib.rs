use std::collections::BTreeSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use axum::{Extension, Router};
use okf::voyage::{
    check_connectivity, chunk_repository, embed_changed_chunks, inventory, suggest_edges, Chunk,
    ConnectivityReport, CurlVoyageTransport, EmbeddedChunk, InventoryDocument, LocalIndex,
    SearchResult, SuggestedEdge, SuggestedEdgeStatus, TokenPlan, VectorBackend, VoyageConfig,
    VoyageTransport, PROVIDER,
};
use okf::{
    browser_config_path, build_initialization_plan, build_root_proposal, configuration_revision,
    confirm_root_registration, confirm_source_initialization, format_document_root_spec,
    generate_root_id, import_document_roots, load_browser_config, merge_document_roots,
    parse_document_root_spec, remove_registered_root, save_browser_config, update_registered_root,
    AdmissionLimits, BrowserRoot, ConfigurationRevision, DocumentRoot, InitializationOptions,
    ProposalConflictCode, ProposalValidation, RootConfigurationUpdate, RootId, RootProposal,
    RootProposalContext, RootProposalKind, RootProposalRequest, RootProposalStore,
    TransactionError,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap as Map;
use zeroize::Zeroizing;

pub const BROWSER_ROOT_ENV_KEY: &str = "OKF_BROWSER_ROOT";
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const ROOTS_ENV_KEY: &str = "OKF_DOCUMENT_ROOTS";
pub const SESSION_TOKEN_ENV_KEY: &str = "OKF_HTTP_SESSION_TOKEN";
pub const TRUSTED_PROXY_TOKEN_ENV_KEY: &str = "OKF_HTTP_TRUSTED_PROXY_TOKEN";
pub const API_VERSION: &str = "v1";
const PAIRED_BROWSER_CAPABILITIES: &[&str] = &[
    "content.initialize",
    "content.write",
    "derived.rebuild",
    "review.decide",
    "roots.configure",
    "roots.propose",
    "session.logout",
    "session.recover",
    "voyage.spend",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerMode {
    ReadOnly,
    LocalEditor,
    AuthenticatedTls,
}

impl ServerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::LocalEditor => "local-editor",
            Self::AuthenticatedTls => "authenticated-tls",
        }
    }
}

mod api;
mod auth;
mod cli;
mod monitoring;
mod relations;
mod routing;
mod security;
mod static_files;
mod storage;
mod tls;
mod users;
mod voyage;
use api::{Envelope as ApiEnvelope, ErrorBody as ApiError, ErrorEnvelope as ApiErrorEnvelope};
use auth::{LocalAuthState, PairingError, PersistentAuthState};
use monitoring::RootMonitor;
use relations::{merge_relations_into_frontmatter, relation_from_suggestion};
use security::{
    authorize_pairing_request, authorize_sensitive_request, session_cookie, CSRF_TOKEN_HEADER,
    SESSION_COOKIE_NAME,
};
#[cfg(test)]
use security::{REQUEST_ID_HEADER, SESSION_TOKEN_HEADER};
#[cfg(test)]
use static_files::resolve_admitted_document_file;
use static_files::{
    content_type_for_path, is_allowed_repo_file, resolve_admitted_file, resolve_static_file,
    root_mount_name, ResolvedOkfFile, StaticFileError,
};
pub use tls::{
    initialize_local_tls, local_tls_status, prepare_tls, renew_local_tls, verify_local_tls,
    LocalTlsCommand, LocalTlsPaths, LocalTlsStatus, PreparedTls, TlsFiles, TlsStatus,
};
pub use users::{read_password, UserAuthorization, UserCommand, UserStore, UserSummary};
const BROWSER_MANIFEST: &str = ".okf-browser-assets.json";
const REQUIRED_BROWSER_ASSETS: &[&str] = &[
    "app.js",
    "index.html",
    "security.js",
    "styles.css",
    "vendor/cytoscape.min.js",
];
const BROWSER_ASSETS: &[(&str, &[u8])] = &[
    ("README.md", include_bytes!("../browser/README.md")),
    ("app.js", include_bytes!("../browser/app.js")),
    ("index.html", include_bytes!("../browser/index.html")),
    ("security.js", include_bytes!("../browser/security.js")),
    ("styles.css", include_bytes!("../browser/styles.css")),
    (
        "vendor/cytoscape.min.js",
        include_bytes!("../browser/vendor/cytoscape.min.js"),
    ),
];

#[derive(Clone, Eq, PartialEq)]
pub struct ServerConfig {
    mode: ServerMode,
    pairing_code: Option<String>,
    tls: Option<TlsFiles>,
    host: String,
    port: u16,
    browser_root: PathBuf,
    roots: Vec<DocumentRoot>,
    environment_roots_active: bool,
    env_file: PathBuf,
    scanlab_compat: bool,
    session_token: String,
    remote_access: bool,
    trusted_proxy: Option<Box<TrustedProxyConfig>>,
    expose_physical_paths: bool,
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerConfig")
            .field("mode", &self.mode)
            .field(
                "pairing_code",
                &self.pairing_code.as_ref().map(|_| "[REDACTED]"),
            )
            .field("tls", &self.tls.as_ref().map(|_| "configured"))
            .field("host", &self.host)
            .field("port", &self.port)
            .field("browser_root", &self.browser_root)
            .field("roots", &self.roots)
            .field("environment_roots_active", &self.environment_roots_active)
            .field("env_file", &self.env_file)
            .field("scanlab_compat", &self.scanlab_compat)
            .field("session_token", &"[REDACTED]")
            .field("remote_access", &self.remote_access)
            .field(
                "trusted_proxy",
                &self
                    .trusted_proxy
                    .as_ref()
                    .map(|proxy| &proxy.public_origin),
            )
            .field("expose_physical_paths", &self.expose_physical_paths)
            .finish()
    }
}

impl ServerConfig {
    pub fn mode(&self) -> ServerMode {
        self.mode
    }

    pub fn pairing_code(&self) -> Option<&str> {
        self.pairing_code.as_deref()
    }

    pub fn tls(&self) -> Option<&TlsFiles> {
        self.tls.as_ref()
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn browser_root(&self) -> &Path {
        &self.browser_root
    }

    pub fn roots(&self) -> &[DocumentRoot] {
        &self.roots
    }

    pub fn environment_roots_active(&self) -> bool {
        self.environment_roots_active
    }

    pub fn scanlab_compat(&self) -> bool {
        self.scanlab_compat
    }

    pub fn session_token(&self) -> &str {
        &self.session_token
    }

    pub fn remote_access(&self) -> bool {
        self.remote_access
    }

    pub fn trusted_proxy(&self) -> Option<&TrustedProxyConfig> {
        self.trusted_proxy.as_deref()
    }

    pub fn expose_physical_paths(&self) -> bool {
        self.expose_physical_paths
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct TrustedProxyConfig {
    public_origin: String,
    public_authority: String,
    token: String,
}

impl fmt::Debug for TrustedProxyConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrustedProxyConfig")
            .field("public_origin", &self.public_origin)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl TrustedProxyConfig {
    pub fn public_origin(&self) -> &str {
        &self.public_origin
    }

    pub fn public_authority(&self) -> &str {
        &self.public_authority
    }

    pub(crate) fn token(&self) -> &str {
        &self.token
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliAction {
    Run(ServerConfig),
    InstallBrowser(BrowserInstallConfig),
    Tls(LocalTlsCommand),
    User(UserCommand),
    ImportEnvRoots(PathBuf),
    Version,
    Help,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserInstallConfig {
    browser_root: PathBuf,
    force: bool,
}

impl BrowserInstallConfig {
    pub fn browser_root(&self) -> &Path {
        &self.browser_root
    }

    pub fn force(&self) -> bool {
        self.force
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserInstallReport {
    pub browser_root: PathBuf,
    pub installed_files: usize,
    pub changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliError(String);

impl CliError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CliError {}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawArgs {
    local_editor: bool,
    authenticated: bool,
    tls_certificate: Option<PathBuf>,
    tls_private_key: Option<PathBuf>,
    host: String,
    port: Option<u16>,
    browser_root: Option<PathBuf>,
    roots: Vec<OsString>,
    additional_roots: Vec<OsString>,
    install_browser: bool,
    force: bool,
    scanlab_compat: bool,
    trusted_proxy: bool,
    public_origin: Option<String>,
    session_token: Option<OsString>,
    expose_physical_paths: bool,
}

impl Default for RawArgs {
    fn default() -> Self {
        Self {
            local_editor: false,
            authenticated: false,
            tls_certificate: None,
            tls_private_key: None,
            host: DEFAULT_HOST.to_string(),
            port: None,
            browser_root: None,
            roots: Vec::new(),
            additional_roots: Vec::new(),
            install_browser: false,
            force: false,
            scanlab_compat: false,
            trusted_proxy: false,
            public_origin: None,
            session_token: None,
            expose_physical_paths: false,
        }
    }
}

struct CliSources {
    env_roots: Option<OsString>,
    dotenv_roots: Option<OsString>,
    env_browser_root: Option<OsString>,
    dotenv_browser_root: Option<OsString>,
    defaults: Vec<DocumentRoot>,
    env_session_token: Option<OsString>,
    env_proxy_token: Option<OsString>,
    config_file: PathBuf,
}

pub fn parse_cli(args: impl IntoIterator<Item = OsString>) -> Result<CliAction, CliError> {
    let config_file = browser_config_path(
        env::var_os("XDG_CONFIG_HOME").as_deref().map(Path::new),
        env::var_os("HOME").as_deref().map(Path::new),
    )
    .map_err(|error| CliError::new(error.to_string()))?;
    let browser_roots = load_browser_config(&config_file)
        .map_err(|error| CliError::new(error.to_string()))?
        .enabled_document_roots();
    parse_cli_internal(
        args,
        CliSources {
            env_roots: env::var_os(ROOTS_ENV_KEY),
            dotenv_roots: dotenv_value(ROOTS_ENV_KEY),
            env_browser_root: env::var_os(BROWSER_ROOT_ENV_KEY),
            dotenv_browser_root: dotenv_value(BROWSER_ROOT_ENV_KEY),
            defaults: browser_roots,
            env_session_token: env::var_os(SESSION_TOKEN_ENV_KEY),
            env_proxy_token: env::var_os(TRUSTED_PROXY_TOKEN_ENV_KEY),
            config_file,
        },
    )
}

pub fn parse_cli_with_sources(
    args: impl IntoIterator<Item = OsString>,
    env_roots: Option<OsString>,
    dotenv_roots: Option<OsString>,
    env_browser_root: Option<OsString>,
    dotenv_browser_root: Option<OsString>,
) -> Result<CliAction, CliError> {
    parse_cli_with_sources_and_security(
        args,
        env_roots,
        dotenv_roots,
        env_browser_root,
        dotenv_browser_root,
        None,
    )
}

pub fn parse_cli_with_sources_and_security(
    args: impl IntoIterator<Item = OsString>,
    env_roots: Option<OsString>,
    dotenv_roots: Option<OsString>,
    env_browser_root: Option<OsString>,
    dotenv_browser_root: Option<OsString>,
    env_session_token: Option<OsString>,
) -> Result<CliAction, CliError> {
    parse_cli_internal(
        args,
        CliSources {
            env_roots,
            dotenv_roots,
            env_browser_root,
            dotenv_browser_root,
            defaults: Vec::new(),
            env_session_token,
            env_proxy_token: None,
            config_file: PathBuf::from("config.toml"),
        },
    )
}

pub fn parse_cli_with_sources_and_proxy(
    args: impl IntoIterator<Item = OsString>,
    env_roots: Option<OsString>,
    dotenv_roots: Option<OsString>,
    env_browser_root: Option<OsString>,
    dotenv_browser_root: Option<OsString>,
    env_session_token: Option<OsString>,
    env_proxy_token: Option<OsString>,
) -> Result<CliAction, CliError> {
    parse_cli_internal(
        args,
        CliSources {
            env_roots,
            dotenv_roots,
            env_browser_root,
            dotenv_browser_root,
            defaults: Vec::new(),
            env_session_token,
            env_proxy_token,
            config_file: PathBuf::from("config.toml"),
        },
    )
}

pub fn parse_cli_with_sources_and_defaults(
    args: impl IntoIterator<Item = OsString>,
    env_roots: Option<OsString>,
    dotenv_roots: Option<OsString>,
    env_browser_root: Option<OsString>,
    dotenv_browser_root: Option<OsString>,
    defaults: Vec<DocumentRoot>,
) -> Result<CliAction, CliError> {
    parse_cli_internal(
        args,
        CliSources {
            env_roots,
            dotenv_roots,
            env_browser_root,
            dotenv_browser_root,
            defaults,
            env_session_token: None,
            env_proxy_token: None,
            config_file: PathBuf::from("config.toml"),
        },
    )
}

fn parse_cli_internal(
    args: impl IntoIterator<Item = OsString>,
    sources: CliSources,
) -> Result<CliAction, CliError> {
    let CliSources {
        env_roots,
        dotenv_roots,
        env_browser_root,
        dotenv_browser_root,
        defaults,
        env_session_token,
        env_proxy_token,
        config_file,
    } = sources;
    let args = args.into_iter().collect::<Vec<_>>();
    if args.len() == 1 && args[0] == OsStr::new("--version") {
        return Ok(CliAction::Version);
    }
    if args
        .first()
        .is_some_and(|argument| argument == OsStr::new("tls"))
    {
        return parse_tls_action(&args[1..]);
    }
    if args
        .first()
        .is_some_and(|argument| argument == OsStr::new("user"))
    {
        return parse_user_action(&args[1..]);
    }
    if args
        .first()
        .is_some_and(|argument| argument == OsStr::new("roots"))
    {
        if args.len() == 2 && args[1] == OsStr::new("import-env") {
            return Ok(CliAction::ImportEnvRoots(config_file));
        }
        return Err(CliError::new("root management requires: roots import-env"));
    }
    let raw = match parse_raw_args(args)? {
        Some(raw) => raw,
        None => return Ok(CliAction::Help),
    };
    if raw.install_browser {
        if raw.port.is_some() {
            return Err(CliError::new(
                "--install-browser does not accept a port argument",
            ));
        }
        if !raw.roots.is_empty()
            || !raw.additional_roots.is_empty()
            || raw.host != DEFAULT_HOST
            || raw.scanlab_compat
            || raw.trusted_proxy
            || raw.public_origin.is_some()
            || raw.session_token.is_some()
            || raw.expose_physical_paths
            || raw.local_editor
            || raw.authenticated
            || raw.tls_certificate.is_some()
            || raw.tls_private_key.is_some()
        {
            return Err(CliError::new(
                "--install-browser accepts only --browser-root and --force",
            ));
        }
        return Ok(CliAction::InstallBrowser(BrowserInstallConfig {
            browser_root: configured_browser_root(
                raw.browser_root,
                env_browser_root,
                dotenv_browser_root,
            ),
            force: raw.force,
        }));
    }
    if raw.force {
        return Err(CliError::new("--force requires --install-browser"));
    }
    let Some(port) = raw.port else {
        return Err(CliError::new("missing required port argument"));
    };
    let explicit_session_token = raw.session_token.or(env_session_token);
    let remote_host = !is_loopback_host(&raw.host);
    if raw.local_editor && raw.authenticated {
        return Err(CliError::new(
            "--local-editor and --authenticated are mutually exclusive",
        ));
    }
    let mode = match (raw.local_editor, raw.authenticated) {
        (true, false) => ServerMode::LocalEditor,
        (false, true) => ServerMode::AuthenticatedTls,
        (false, false) => ServerMode::ReadOnly,
        (true, true) => unreachable!("mutually exclusive modes checked above"),
    };
    let tls = match (raw.tls_certificate, raw.tls_private_key) {
        (Some(certificate), Some(private_key)) if mode == ServerMode::AuthenticatedTls => {
            Some(TlsFiles::new(certificate, private_key))
        }
        (None, None) if mode == ServerMode::AuthenticatedTls => {
            return Err(CliError::new(
                "--authenticated requires --tls-cert and --tls-key",
            ));
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(CliError::new(
                "--tls-cert and --tls-key must be provided together",
            ));
        }
        (Some(_), Some(_)) => {
            return Err(CliError::new(
                "TLS certificate options require --authenticated",
            ));
        }
        (None, None) => None,
    };
    if mode == ServerMode::LocalEditor && remote_host {
        return Err(CliError::new(
            "--local-editor requires a loopback bind host",
        ));
    }
    if raw.expose_physical_paths && mode != ServerMode::LocalEditor {
        return Err(CliError::new(
            "--expose-physical-paths requires --local-editor",
        ));
    }
    if remote_host {
        return Err(CliError::new(
            "non-loopback binding is not supported; keep okf-http on loopback and expose a reverse proxy",
        ));
    }
    if raw.trusted_proxy && mode != ServerMode::AuthenticatedTls {
        return Err(CliError::new(
            "--trusted-proxy requires authenticated TLS for the proxy-to-OKF connection",
        ));
    }
    let trusted_proxy = if raw.trusted_proxy {
        let public_origin = raw.public_origin.ok_or_else(|| {
            CliError::new("--trusted-proxy requires --public-origin https://host[:port]")
        })?;
        let public_authority = public_origin
            .strip_prefix("https://")
            .expect("validated public origin")
            .to_string();
        let token = parse_proxy_token(env_proxy_token.ok_or_else(|| {
            CliError::new(format!(
                "--trusted-proxy requires {TRUSTED_PROXY_TOKEN_ENV_KEY}"
            ))
        })?)?;
        Some(Box::new(TrustedProxyConfig {
            public_origin,
            public_authority,
            token,
        }))
    } else {
        if raw.public_origin.is_some() {
            return Err(CliError::new(
                "--public-origin is valid only with --trusted-proxy",
            ));
        }
        None
    };
    let session_token = match explicit_session_token {
        Some(token) => parse_session_token(token)?,
        None => generate_session_token()?,
    };
    let pairing_code = if mode == ServerMode::LocalEditor {
        Some(auth::generate_pairing_code().map_err(CliError::new)?)
    } else {
        None
    };

    let environment_roots_active = env_roots.is_some();
    let environment_roots = parse_configured_roots(env_roots)?;
    let dotenv_roots = parse_configured_roots(dotenv_roots)?;
    let cli_roots = parse_root_specs(raw.roots)?;
    let additional_roots = parse_root_specs(raw.additional_roots)?;

    Ok(CliAction::Run(ServerConfig {
        mode,
        pairing_code,
        tls,
        host: raw.host,
        port,
        browser_root: configured_browser_root(
            raw.browser_root,
            env_browser_root,
            dotenv_browser_root,
        ),
        roots: merge_document_roots(
            defaults,
            dotenv_roots,
            environment_roots,
            cli_roots,
            additional_roots,
        ),
        environment_roots_active,
        env_file: config_file,
        scanlab_compat: raw.scanlab_compat,
        session_token,
        remote_access: false,
        trusted_proxy,
        expose_physical_paths: raw.expose_physical_paths,
    }))
}

fn parse_tls_action(args: &[OsString]) -> Result<CliAction, CliError> {
    if args.len() != 1 {
        return Err(CliError::new(
            "TLS management requires exactly one command: init, status, verify, or renew",
        ));
    }
    let command = match args[0].to_str() {
        Some("init") => LocalTlsCommand::Init,
        Some("status") => LocalTlsCommand::Status,
        Some("verify") => LocalTlsCommand::Verify,
        Some("renew") => LocalTlsCommand::Renew,
        Some(command) => return Err(CliError::new(format!("unknown TLS command: {command}"))),
        None => return Err(CliError::new("TLS command must be valid UTF-8")),
    };
    Ok(CliAction::Tls(command))
}

fn parse_user_action(args: &[OsString]) -> Result<CliAction, CliError> {
    let strings = args
        .iter()
        .map(|value| {
            value
                .to_str()
                .map(str::to_string)
                .ok_or_else(|| CliError::new("user command arguments must be valid UTF-8"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let command = strings.first().map(String::as_str).ok_or_else(|| {
        CliError::new(
            "user management requires add, passwd, disable, remove, list, grant, or revoke",
        )
    })?;
    let password_stdin = strings.iter().any(|value| value == "--password-stdin");
    if strings
        .iter()
        .skip(1)
        .any(|value| value.starts_with('-') && value != "--password-stdin")
    {
        return Err(CliError::new("unknown user-management option"));
    }
    let positional = strings
        .iter()
        .skip(1)
        .filter(|value| value.as_str() != "--password-stdin")
        .collect::<Vec<_>>();
    let action =
        match (command, positional.as_slice(), password_stdin) {
            ("add", [name], password_stdin) => UserCommand::Add {
                name: (*name).clone(),
                password_stdin,
            },
            ("passwd", [name], password_stdin) => UserCommand::Passwd {
                name: (*name).clone(),
                password_stdin,
            },
            ("disable", [name], false) => UserCommand::Disable {
                name: (*name).clone(),
            },
            ("remove", [name], false) => UserCommand::Remove {
                name: (*name).clone(),
            },
            ("list", [], false) => UserCommand::List,
            ("grant", [name, role], false) => UserCommand::Grant {
                name: (*name).clone(),
                role: (*role).clone(),
            },
            ("revoke", [name, role], false) => UserCommand::Revoke {
                name: (*name).clone(),
                role: (*role).clone(),
            },
            _ => return Err(CliError::new(
                "invalid user command syntax; --password-stdin is accepted only by add and passwd",
            )),
        };
    Ok(CliAction::User(action))
}

fn parse_raw_args(args: impl IntoIterator<Item = OsString>) -> Result<Option<RawArgs>, CliError> {
    let mut parsed = RawArgs::default();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        if arg == OsStr::new("-h") || arg == OsStr::new("--help") {
            return Ok(None);
        }
        if arg == OsStr::new("--version") {
            return Err(CliError::new("--version does not accept other arguments"));
        }

        if arg == OsStr::new("--install-browser") {
            parsed.install_browser = true;
            continue;
        }
        if arg == OsStr::new("--force") {
            parsed.force = true;
            continue;
        }
        if arg == OsStr::new("--scanlab-compat") {
            parsed.scanlab_compat = true;
            continue;
        }
        if arg == OsStr::new("--local-editor") {
            parsed.local_editor = true;
            continue;
        }
        if arg == OsStr::new("--authenticated") {
            parsed.authenticated = true;
            continue;
        }
        if arg == OsStr::new("--trusted-proxy") {
            parsed.trusted_proxy = true;
            continue;
        }
        if arg == OsStr::new("--expose-physical-paths") {
            parsed.expose_physical_paths = true;
            continue;
        }

        if let Some(value) = split_option_value(&arg, "--session-token=") {
            parsed.session_token = Some(value);
            continue;
        }

        if let Some(value) = split_option_value(&arg, "--public-origin=") {
            parsed.public_origin = Some(parse_public_origin(value)?);
            continue;
        }
        if arg == OsStr::new("--public-origin") {
            let value = args
                .next()
                .ok_or_else(|| CliError::new("--public-origin requires a value"))?;
            parsed.public_origin = Some(parse_public_origin(value)?);
            continue;
        }
        if arg == OsStr::new("--session-token") {
            parsed.session_token = Some(
                args.next()
                    .ok_or_else(|| CliError::new("--session-token requires a value"))?,
            );
            continue;
        }

        if let Some(value) = split_option_value(&arg, "--tls-cert=") {
            parsed.tls_certificate = Some(parse_required_path(value, "--tls-cert")?);
            continue;
        }
        if arg == OsStr::new("--tls-cert") {
            let value = args
                .next()
                .ok_or_else(|| CliError::new("--tls-cert requires a value"))?;
            parsed.tls_certificate = Some(parse_required_path(value, "--tls-cert")?);
            continue;
        }
        if let Some(value) = split_option_value(&arg, "--tls-key=") {
            parsed.tls_private_key = Some(parse_required_path(value, "--tls-key")?);
            continue;
        }
        if arg == OsStr::new("--tls-key") {
            let value = args
                .next()
                .ok_or_else(|| CliError::new("--tls-key requires a value"))?;
            parsed.tls_private_key = Some(parse_required_path(value, "--tls-key")?);
            continue;
        }

        if let Some(value) = split_option_value(&arg, "--host=") {
            parsed.host = parse_host(value)?;
            continue;
        }
        if arg == OsStr::new("--host") {
            let value = args
                .next()
                .ok_or_else(|| CliError::new("--host requires a value"))?;
            parsed.host = parse_host(value)?;
            continue;
        }

        if let Some(value) = split_option_value(&arg, "--browser-root=") {
            parsed.browser_root = Some(parse_browser_root(value)?);
            continue;
        }
        if arg == OsStr::new("--browser-root") {
            let value = args
                .next()
                .ok_or_else(|| CliError::new("--browser-root requires a value"))?;
            parsed.browser_root = Some(parse_browser_root(value)?);
            continue;
        }

        if let Some(value) = split_option_value(&arg, "--root=") {
            parsed.roots.push(value);
            continue;
        }
        if arg == OsStr::new("--root") {
            let value = args
                .next()
                .ok_or_else(|| CliError::new("--root requires a value"))?;
            parsed.roots.push(value);
            continue;
        }

        if let Some(value) = split_option_value(&arg, "--add-root=") {
            parsed.additional_roots.push(value);
            continue;
        }
        if arg == OsStr::new("--add-root") {
            let value = args
                .next()
                .ok_or_else(|| CliError::new("--add-root requires a value"))?;
            parsed.additional_roots.push(value);
            continue;
        }

        if arg.to_string_lossy().starts_with('-') {
            return Err(CliError::new(format!(
                "unknown option: {}",
                arg.to_string_lossy()
            )));
        }

        if parsed.port.is_some() {
            return Err(CliError::new("only one port argument is allowed"));
        }
        parsed.port = Some(parse_port(&arg)?);
    }

    Ok(Some(parsed))
}

fn split_option_value(arg: &OsStr, prefix: &str) -> Option<OsString> {
    let value = arg.to_string_lossy();
    value
        .strip_prefix(prefix)
        .map(|value| OsString::from(value.to_string()))
}

fn parse_required_path(value: OsString, option: &str) -> Result<PathBuf, CliError> {
    if value.is_empty() {
        return Err(CliError::new(format!("{option} requires a non-empty path")));
    }
    Ok(PathBuf::from(value))
}

fn parse_host(value: OsString) -> Result<String, CliError> {
    let mut value = value.to_string_lossy().trim().to_string();
    if value.is_empty() {
        return Err(CliError::new("host must not be empty"));
    }
    if value.starts_with('[') && value.ends_with(']') {
        value = value[1..value.len() - 1].to_string();
    }
    if value.parse::<std::net::IpAddr>().is_ok() {
        return Ok(value);
    }
    if value.contains("://")
        || value.contains(':')
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_whitespace)
        || value.split('.').any(|label| {
            label.is_empty()
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
    {
        return Err(CliError::new(format!("invalid bind host: {value}")));
    }
    Ok(value)
}

fn parse_public_origin(value: OsString) -> Result<String, CliError> {
    let value = value
        .to_str()
        .ok_or_else(|| CliError::new("public origin must be valid UTF-8"))?
        .trim();
    let Some(authority) = value.strip_prefix("https://") else {
        return Err(CliError::new(
            "public origin must use https:// and contain only host and optional port",
        ));
    };
    if authority.is_empty()
        || authority.contains(['/', '?', '#', '@'])
        || authority.chars().any(char::is_whitespace)
    {
        return Err(CliError::new(
            "public origin must contain only an HTTPS host and optional port",
        ));
    }
    let authority = authority
        .parse::<axum::http::uri::Authority>()
        .map_err(|_| CliError::new("public origin has an invalid authority"))?;
    if authority.host().is_empty() {
        return Err(CliError::new("public origin has no host"));
    }
    Ok(format!("https://{authority}"))
}

fn parse_proxy_token(value: OsString) -> Result<String, CliError> {
    let token = value
        .into_string()
        .map_err(|_| CliError::new("trusted proxy token must be valid UTF-8"))?;
    if token.len() < 32
        || token.len() > 512
        || token
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err(CliError::new(
            "trusted proxy token must contain 32-512 non-whitespace characters",
        ));
    }
    Ok(token)
}

fn is_loopback_host(host: &str) -> bool {
    cli::is_loopback_host(host)
}

fn parse_session_token(value: OsString) -> Result<String, CliError> {
    cli::parse_session_token(value).map_err(CliError::new)
}

fn generate_session_token() -> Result<String, CliError> {
    cli::generate_session_token().map_err(CliError::new)
}

fn parse_browser_root(value: OsString) -> Result<PathBuf, CliError> {
    if value.is_empty() {
        return Err(CliError::new("browser root must not be empty"));
    }
    Ok(PathBuf::from(value))
}

fn parse_port(value: &OsStr) -> Result<u16, CliError> {
    let port = value
        .to_string_lossy()
        .parse::<u16>()
        .map_err(|_| CliError::new(format!("invalid port: {}", value.to_string_lossy())))?;
    if port < 1024 {
        return Err(CliError::new(format!(
            "port {port} is privileged; choose an unprivileged port >= 1024"
        )));
    }
    Ok(port)
}

fn parse_configured_roots(value: Option<OsString>) -> Result<Option<Vec<DocumentRoot>>, CliError> {
    value
        .map(|value| {
            if value.is_empty() {
                Ok(Vec::new())
            } else {
                parse_root_specs(env::split_paths(&value).map(PathBuf::into_os_string))
            }
        })
        .transpose()
}

fn configured_browser_root(
    cli_browser_root: Option<PathBuf>,
    env_browser_root: Option<OsString>,
    dotenv_browser_root: Option<OsString>,
) -> PathBuf {
    cli_browser_root
        .or_else(|| env_browser_root.or(dotenv_browser_root).map(PathBuf::from))
        .unwrap_or_else(default_browser_root)
}

fn default_browser_root() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("docs-browser")
}

fn configured_root_state_dir(config_file: &Path) -> PathBuf {
    let standard_config = browser_config_path(
        env::var_os("XDG_CONFIG_HOME").as_deref().map(Path::new),
        env::var_os("HOME").as_deref().map(Path::new),
    )
    .ok();
    if standard_config.as_deref() == Some(config_file) {
        if let Some(state) = env::var_os("XDG_STATE_HOME") {
            return PathBuf::from(state).join("okf");
        }
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(".local/state/okf");
        }
    }
    config_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("state")
}

fn parse_root_specs(
    specs: impl IntoIterator<Item = OsString>,
) -> Result<Vec<DocumentRoot>, CliError> {
    specs
        .into_iter()
        .map(|spec| {
            parse_document_root_spec(&spec).map_err(|error| {
                CliError::new(format!("invalid document root {:?}: {error}", spec))
            })
        })
        .collect()
}

fn dotenv_value(key: &str) -> Option<OsString> {
    let source = fs::read_to_string(".env").ok()?;
    dotenv_value_from_source(&source, key)
}

pub fn dotenv_value_from_source(source: &str, key: &str) -> Option<OsString> {
    source.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| {
            OsString::from(
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            )
        })
    })
}

fn persistent_roots_from_file(path: &Path) -> Result<Vec<DocumentRoot>, String> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    let Some(value) = dotenv_value_from_source(&source, ROOTS_ENV_KEY) else {
        return Ok(Vec::new());
    };
    if value.is_empty() {
        return Ok(Vec::new());
    }
    parse_root_specs(env::split_paths(&value).map(PathBuf::into_os_string))
        .map_err(|error| error.to_string())
}

pub fn import_env_roots(config_file: &Path) -> Result<usize, String> {
    let roots = persistent_roots_from_file(Path::new(".env"))?;
    let imported = import_document_roots(&roots).map_err(|error| error.to_string())?;
    let mut config = load_browser_config(config_file).map_err(|error| error.to_string())?;
    let existing = config
        .roots()
        .iter()
        .map(|root| root.root_id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    let next_priority = config
        .roots()
        .iter()
        .map(|root| root.priority)
        .max()
        .unwrap_or(-100)
        + 100;
    let additions = imported
        .roots()
        .iter()
        .filter(|root| !existing.contains(root.root_id.as_str()))
        .cloned()
        .enumerate()
        .map(|(index, mut root)| {
            root.priority = next_priority + index as i64 * 100;
            root
        })
        .collect::<Vec<BrowserRoot>>();
    let count = additions.len();
    config.roots_mut().extend(additions);
    save_browser_config(config_file, &config).map_err(|error| error.to_string())?;
    Ok(count)
}

fn atomic_write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or(".env");
    let temporary = parent.join(format!(
        ".{file_name}.okf-{}-{unique}.tmp",
        std::process::id()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| format!("failed to create {}: {error}", temporary.display()))?;
    let operation = (|| -> std::io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if let Err(error) = operation {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "failed to update {} atomically: {error}",
            path.display()
        ));
    }
    Ok(())
}

pub fn help_text() -> &'static str {
    "Usage: okf-http [--local-editor | --authenticated --tls-cert <path> --tls-key <path>] [--host <loopback-host>] [--browser-root <path>] [--root <path>|--root <mount>=<path>] [--add-root <spec>] [--scanlab-compat] [--expose-physical-paths] [--session-token <token> | --trusted-proxy --public-origin <https-origin>] <port>\n\
       okf-http tls <init|status|verify|renew>\n\
  okf-http user <add|passwd|disable|remove|list|grant|revoke> ...\n\
  okf-http roots import-env\n\
\n\
Start the OKF HTTP browser and API server.\n\
\n\
Arguments:\n\
  <port>                  Required unprivileged port, >= 1024.\n\
\n\
Options:\n\
  --install-browser       Provision the packaged browser into the configured browser root; no port is used.\n\
  --force                 With --install-browser, replace user-modified packaged assets.\n\
  --host <host>           Loopback bind host. Default: 127.0.0.1. Intranet and Internet access must use a reverse proxy.\n\
  --browser-root <path>   Serve the OKF browser from this directory. Default on Linux: ~/docs-browser.\n\
  --root <spec>           Add a document root for this process. Use <path> or <mount>=<path>. May be repeated.\n\
  --add-root <spec>       Append a lower-priority fallback without replacing persistent roots with the same mount.\n\
  --scanlab-compat        Enable scanlab's legacy document and repository-file routes.\n\
  --local-editor          Enable protected mutation and token-spending routes on loopback. Default: read-only.\n\
  --authenticated         Start HTTPS with explicit certificate files on loopback.\n\
  --tls-cert <path>       PEM certificate chain for --authenticated mode.\n\
  --tls-key <path>        Owner-only PEM private key for --authenticated mode.\n\
  --trusted-proxy         Trust one loopback reverse proxy only when it injects OKF_HTTP_TRUSTED_PROXY_TOKEN. Backend HTTPS remains mandatory.\n\
  --public-origin <url>   Exact external HTTPS origin used only with --trusted-proxy, for example https://knowledge.example.\n\
  --session-token <token> Set the API session token. Prefer OKF_HTTP_SESSION_TOKEN to avoid process-list exposure.\n\
  --expose-physical-paths Include local filesystem paths in API responses for trusted debugging. Requires --local-editor.\n\
  --version              Show the okf-http package version.\n\
  -h, --help              Show this help text.\n\
\n\
Browser root:\n\
  Persistent browser root configuration is read from OKF_BROWSER_ROOT in .env or the process environment.\n\
  CLI --browser-root overrides persistent browser-root configuration for the current process.\n\
  Run okf-http --install-browser to provision the packaged assets. Existing modified assets require --force.\n\
\n\
Document roots:\n\
  Persistent roots are read from OKF_DOCUMENT_ROOTS in .env or the process environment.\n\
  CLI --root values replace persistent roots with the same mount for the current process and have highest priority.\n\
  CLI --add-root values are appended as lower-priority fallbacks.\n\
  Browser/API root changes are stored in ~/.config/okf/config.toml (or XDG_CONFIG_HOME).\n\
  .env remains operator-managed; use `okf-http roots import-env` for an explicit one-time import.\n\
\n\
Persistent users:\n\
  `user add <name>` and `user passwd <name>` read and confirm passwords from a protected terminal.\n\
  Add `--password-stdin` only for controlled automation; the password is read as one line and is never accepted as a CLI argument.\n\
  Roles are `editor`, `voyage`, and `admin`. Credentials are stored as Argon2id hashes in private XDG state.\n\
\n\
Security:\n\
  Read-only mode is the default and does not mount mutation or token-spending routes.\n\
  Local editor mode is loopback-only. Pair in the browser with the one-time terminal code.\n\
  Authenticated mode refuses startup unless certificate, key, SAN, validity, and key permissions pass validation.\n\
  okf-http always binds to loopback for supported local, intranet, and Internet deployments.\n\
  Expose intranet and Internet sites through a reverse proxy instead of binding okf-http to a non-loopback address.\n\
  Trusted-proxy mode is loopback-only, requires HTTPS on both hops, rejects direct backend requests, and validates unambiguous forwarded host/proto fields.\n\
  Trusted-proxy deployments require login for document reads and disable remote document-root management.\n\
  `tls init` creates an optional local CA and localhost certificate under the private XDG state directory without changing any trust store.\n"
}

pub fn url_host(host: &str) -> String {
    if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

#[derive(Deserialize, Serialize)]
struct BrowserAssetManifest {
    version: String,
    assets: Map<String, String>,
}

pub fn validate_browser_root(root: &Path) -> Result<(), String> {
    for relative in REQUIRED_BROWSER_ASSETS {
        let path = root.join(relative);
        if !path.is_file() {
            return Err(format!("missing browser asset: {}", path.display()));
        }
    }
    Ok(())
}

pub fn install_browser_assets(
    config: &BrowserInstallConfig,
) -> Result<BrowserInstallReport, String> {
    let root = &config.browser_root;
    if root.exists() && !root.is_dir() {
        return Err(format!(
            "browser root exists but is not a directory: {}",
            root.display()
        ));
    }
    let expected = browser_asset_manifest();
    let old_manifest = load_browser_manifest(root).ok();
    let mut modified = Vec::new();
    let mut already_current = root.is_dir();
    for (relative, bytes) in BROWSER_ASSETS {
        let path = root.join(relative);
        match fs::read(&path) {
            Ok(current) => {
                let current_hash = content_hash(&current);
                if current_hash != content_hash(bytes) {
                    already_current = false;
                }
                let expected_old = old_manifest
                    .as_ref()
                    .and_then(|manifest| manifest.assets.get(*relative));
                let unmodified =
                    expected_old.map_or(current == *bytes, |hash| hash == &current_hash);
                if !unmodified {
                    modified.push((*relative).to_string());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                already_current = false;
            }
            Err(error) => {
                return Err(format!("failed to read {}: {error}", path.display()));
            }
        }
    }
    if already_current
        && old_manifest
            .as_ref()
            .is_some_and(|manifest| manifest.version == env!("CARGO_PKG_VERSION"))
    {
        return Ok(BrowserInstallReport {
            browser_root: root.clone(),
            installed_files: BROWSER_ASSETS.len(),
            changed: false,
        });
    }
    if !modified.is_empty() && !config.force {
        return Err(format!(
            "browser assets were modified: {}; rerun with --force to replace packaged assets",
            modified.join(", ")
        ));
    }

    let parent = root.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = root
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("docs-browser");
    let staging = parent.join(format!(".{name}.okf-stage-{}-{unique}", std::process::id()));
    let backup = parent.join(format!(
        ".{name}.okf-backup-{}-{unique}",
        std::process::id()
    ));
    if root.is_dir() {
        copy_directory_tree(root, &staging)?;
    } else {
        fs::create_dir(&staging)
            .map_err(|error| format!("failed to create {}: {error}", staging.display()))?;
    }
    let staged = (|| -> Result<(), String> {
        for (relative, bytes) in BROWSER_ASSETS {
            let path = staging.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
            }
            fs::write(&path, bytes)
                .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        }
        let manifest = serde_json::to_vec_pretty(&expected)
            .map_err(|error| format!("failed to encode browser manifest: {error}"))?;
        fs::write(staging.join(BROWSER_MANIFEST), manifest)
            .map_err(|error| format!("failed to write browser manifest: {error}"))?;
        Ok(())
    })();
    if let Err(error) = staged {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    if root.is_dir() {
        fs::rename(root, &backup)
            .map_err(|error| format!("failed to prepare browser upgrade: {error}"))?;
        if let Err(error) = fs::rename(&staging, root) {
            let _ = fs::rename(&backup, root);
            let _ = fs::remove_dir_all(&staging);
            return Err(format!("failed to activate browser assets: {error}"));
        }
        fs::remove_dir_all(&backup)
            .map_err(|error| format!("failed to remove browser backup: {error}"))?;
    } else {
        fs::rename(&staging, root)
            .map_err(|error| format!("failed to activate browser assets: {error}"))?;
    }

    Ok(BrowserInstallReport {
        browser_root: root.clone(),
        installed_files: BROWSER_ASSETS.len(),
        changed: true,
    })
}

fn browser_asset_manifest() -> BrowserAssetManifest {
    BrowserAssetManifest {
        version: env!("CARGO_PKG_VERSION").to_string(),
        assets: BROWSER_ASSETS
            .iter()
            .map(|(path, bytes)| ((*path).to_string(), content_hash(bytes)))
            .collect(),
    }
}

fn load_browser_manifest(root: &Path) -> Result<BrowserAssetManifest, String> {
    let path = root.join(BROWSER_MANIFEST);
    let source =
        fs::read(&path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_slice(&source)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn content_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn copy_directory_tree(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir(target)
        .map_err(|error| format!("failed to create {}: {error}", target.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("failed to read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read directory entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", entry.path().display()))?;
        let destination = target.join(entry.file_name());
        if file_type.is_symlink() {
            return Err(format!(
                "browser root contains unsupported symlink: {}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            copy_directory_tree(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &destination).map_err(|error| {
                format!(
                    "failed to copy {} to {}: {error}",
                    entry.path().display(),
                    destination.display()
                )
            })?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct AppState {
    mode: ServerMode,
    tls_status: Option<TlsStatus>,
    local_auth: LocalAuthState,
    persistent_auth: PersistentAuthState,
    users: Option<UserStore>,
    remote_access: bool,
    trusted_proxy: Option<TrustedProxyConfig>,
    expected_host: String,
    browser_root: PathBuf,
    document_roots: Vec<DocumentRoot>,
    repo_root: PathBuf,
    environment_roots_active: bool,
    env_file: PathBuf,
    root_proposals: Arc<Mutex<RootProposalStore>>,
    root_state_dir: PathBuf,
    root_monitor: Option<RootMonitor>,
    index_jobs: IndexJobRegistry,
    session_token: String,
    expose_physical_paths: bool,
}

#[derive(Clone, Debug, Default)]
struct IndexJobRegistry {
    active: Arc<Mutex<BTreeSet<PathBuf>>>,
}

#[derive(Debug)]
struct IndexJobGuard {
    registry: IndexJobRegistry,
    key: PathBuf,
}

impl IndexJobRegistry {
    fn try_begin(&self, index_root: &Path) -> Option<IndexJobGuard> {
        let key = index_job_key(index_root);
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !active.insert(key.clone()) {
            return None;
        }
        Some(IndexJobGuard {
            registry: self.clone(),
            key,
        })
    }
}

impl Drop for IndexJobGuard {
    fn drop(&mut self) {
        self.registry
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.key);
    }
}

fn index_job_key(index_root: &Path) -> PathBuf {
    if let Ok(canonical) = index_root.canonicalize() {
        return canonical;
    }
    let absolute = if index_root.is_absolute() {
        index_root.to_path_buf()
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(index_root)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[derive(Clone, Debug)]
struct InternalErrorDetail(String);

pub fn app(config: ServerConfig) -> Router {
    assert_ne!(
        config.mode,
        ServerMode::AuthenticatedTls,
        "authenticated TLS routers require validated TLS material"
    );
    build_app(config, None, None)
}

pub fn app_with_prepared_tls(config: ServerConfig, prepared_tls: &PreparedTls) -> Router {
    assert_eq!(
        config.mode,
        ServerMode::AuthenticatedTls,
        "validated TLS material is only valid for authenticated TLS mode"
    );
    build_app(config, Some(prepared_tls.status().clone()), None)
}

pub fn app_with_prepared_tls_and_users(
    config: ServerConfig,
    prepared_tls: &PreparedTls,
    users: UserStore,
) -> Router {
    assert_eq!(
        config.mode,
        ServerMode::AuthenticatedTls,
        "validated TLS material is only valid for authenticated TLS mode"
    );
    build_app(config, Some(prepared_tls.status().clone()), Some(users))
}

fn build_app(
    config: ServerConfig,
    tls_status: Option<TlsStatus>,
    users: Option<UserStore>,
) -> Router {
    let scanlab_compat = config.scanlab_compat;
    let expected_host = format!("{}:{}", url_host(&config.host), config.port);
    let local_auth = LocalAuthState::new(config.pairing_code);
    let root_state_dir = configured_root_state_dir(&config.env_file);
    let root_monitor = RootMonitor::open(&root_state_dir).ok();
    let state = AppState {
        mode: config.mode,
        tls_status,
        local_auth,
        persistent_auth: PersistentAuthState::new(),
        users,
        remote_access: config.remote_access,
        trusted_proxy: config.trusted_proxy.map(|proxy| *proxy),
        expected_host,
        browser_root: config.browser_root,
        document_roots: config.roots,
        repo_root: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        environment_roots_active: config.environment_roots_active,
        env_file: config.env_file,
        root_proposals: Arc::new(Mutex::new(RootProposalStore::new(Duration::from_secs(
            15 * 60,
        )))),
        root_state_dir,
        root_monitor: root_monitor.clone(),
        index_jobs: IndexJobRegistry::default(),
        session_token: config.session_token,
        expose_physical_paths: config.expose_physical_paths,
    };
    if let Some(monitor) = root_monitor {
        if let Ok(browser_config) = load_browser_config(&state.env_file) {
            let roots = browser_config
                .roots()
                .iter()
                .filter(|root| root.enabled && root.check_for_changes)
                .cloned()
                .collect::<Vec<_>>();
            if !roots.is_empty() {
                let _ = tokio::runtime::Handle::try_current().map(|handle| {
                    handle.spawn_blocking(move || {
                        for root in roots {
                            if let Err(error) = monitor.scan(&root) {
                                eprintln!(
                                    "OKF change detection failed for root {}: {error}",
                                    root.root_id
                                );
                            }
                        }
                    });
                });
            }
        }
    }
    let router = routing::router(scanlab_compat, config.mode);

    router
        .layer(Extension(state.clone()))
        .layer(middleware::from_fn(security::add_security_headers))
        .with_state(state)
}

#[cfg(any())]
async fn add_security_headers(request: Request<Body>, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let legacy_api = path.starts_with("/api/okf/");
    let api_request = path.starts_with("/api/");
    let request_id = format!(
        "okf-{:016x}",
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let started = Instant::now();
    let mut response = next.run(request).await;
    let status = response.status();
    let internal_error = response
        .extensions()
        .get::<InternalErrorDetail>()
        .map(|detail| detail.0.clone());
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; font-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; object-src 'none'"
            .parse()
            .expect("valid CSP"),
    );
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    headers.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    headers.insert(
        "permissions-policy",
        "camera=(), microphone=(), geolocation=(), payment=(), usb=()"
            .parse()
            .unwrap(),
    );
    headers.insert("x-okf-api-version", API_VERSION.parse().unwrap());
    headers.insert(REQUEST_ID_HEADER, request_id.parse().unwrap());
    if api_request {
        headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    }
    if legacy_api {
        headers.insert("deprecation", "true".parse().unwrap());
        headers.insert("sunset", "Wed, 31 Dec 2026 23:59:59 GMT".parse().unwrap());
        headers.insert(
            header::LINK,
            "</api/v1/>; rel=\"successor-version\"".parse().unwrap(),
        );
    }
    eprintln!(
        "{}",
        serde_json::json!({
            "event": "http_request",
            "request_id": request_id,
            "method": method,
            "path": path,
            "status": status.as_u16(),
            "duration_ms": started.elapsed().as_millis(),
            "error": internal_error,
        })
    );
    response
}

#[cfg(any())]
fn authorize_sensitive_request_legacy(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), Response> {
    let provided = headers
        .get(SESSION_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !constant_time_eq(provided.as_bytes(), state.session_token.as_bytes()) {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            format!("missing or invalid {SESSION_TOKEN_HEADER}"),
        ));
    }

    if let Some(site) = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
    {
        if !matches!(site, "same-origin" | "none") {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                "cross-site sensitive request rejected",
            ));
        }
    }

    if let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        let host = headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let matches_host = origin.eq_ignore_ascii_case(&format!("http://{host}"))
            || origin.eq_ignore_ascii_case(&format!("https://{host}"));
        if host.is_empty() || !matches_host {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                "request origin does not match the OKF HTTP origin",
            ));
        }
    }

    Ok(())
}

#[cfg(any())]
fn constant_time_eq_legacy(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

#[derive(Deserialize)]
struct PairingRequest {
    code: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct PasswordChangeRequest {
    current_password: String,
    new_password: String,
    confirm_password: String,
}

#[derive(Deserialize)]
struct RevokeUserSessionsRequest {
    username: String,
}

async fn api_session_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let status = state.local_auth.session_status(session_cookie(&headers));
    let persistent = session_cookie(&headers).and_then(|session_id| {
        state
            .users
            .as_ref()
            .and_then(|users| state.persistent_auth.status(session_id, users))
    });
    api_response(ApiSessionStatus {
        mode: state.mode.as_str(),
        pairing_available: state.local_auth.pairing_available(),
        authenticated: status.authenticated || persistent.is_some(),
        expires_in_seconds: status
            .expires_in_seconds
            .or_else(|| persistent.as_ref().map(|status| status.expires_in_seconds)),
        scope: status
            .authenticated
            .then(|| "local-editor".to_string())
            .or_else(|| persistent.as_ref().map(|_| "persistent-user".to_string())),
        username: persistent.as_ref().map(|status| status.username.clone()),
        capabilities: persistent
            .as_ref()
            .map(|status| status.capabilities.clone())
            .unwrap_or_default(),
    })
}

async fn api_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Response {
    if state.mode != ServerMode::AuthenticatedTls {
        return api_error(StatusCode::NOT_FOUND, "persistent login is unavailable");
    }
    if let Some(response) = authorize_pairing_request(&state, &headers) {
        return response;
    }
    let Some(users) = state.users.clone() else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistent authentication state is unavailable",
        );
    };
    let username = request.username;
    let password = Zeroizing::new(request.password);
    let authentication =
        tokio::task::spawn_blocking(move || users.authenticate_authorization(&username, &password))
            .await;
    let authorization = match authentication {
        Ok(Ok(Some(authorization))) => authorization,
        Ok(Ok(None)) => return api_error(StatusCode::UNAUTHORIZED, "login failed"),
        Ok(Err(error)) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("authentication failed internally: {error}"),
            )
        }
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("authentication task failed: {error}"),
            )
        }
    };
    let username = authorization.name.clone();
    let capabilities = authorization.capabilities.clone();
    let grant = match state.persistent_auth.login(authorization) {
        Ok(grant) => grant,
        Err(_) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to establish persistent session",
            )
        }
    };
    let mut response = api_response(ApiLoginGrant {
        authenticated: true,
        username,
        capabilities,
        csrf_header: CSRF_TOKEN_HEADER,
        csrf_token: grant.csrf_token,
        expires_in_seconds: grant.expires_in_seconds,
    });
    response.headers_mut().insert(
        header::SET_COOKIE,
        secure_session_cookie(&grant.session_id, grant.expires_in_seconds)
            .parse()
            .expect("random session ID forms a valid cookie"),
    );
    response
}

async fn api_session_refresh(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize_pairing_request(&state, &headers) {
        return response;
    }
    let Some(session_id) = session_cookie(&headers) else {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "missing or expired local editor session",
        );
    };
    let status = state.local_auth.session_status(Some(session_id));
    if let Some(csrf_token) = status.csrf_token {
        return api_response(ApiSessionRefresh {
            authenticated: true,
            scope: "local-editor".to_string(),
            username: None,
            capabilities: PAIRED_BROWSER_CAPABILITIES
                .iter()
                .map(ToString::to_string)
                .collect(),
            expires_in_seconds: status.expires_in_seconds.unwrap_or_default(),
            csrf_header: CSRF_TOKEN_HEADER,
            csrf_token,
        });
    }
    let persistent = state
        .users
        .as_ref()
        .and_then(|users| state.persistent_auth.status(session_id, users));
    let Some(persistent) = persistent else {
        return api_error(StatusCode::UNAUTHORIZED, "missing or expired session");
    };
    api_response(ApiSessionRefresh {
        authenticated: true,
        scope: "persistent-user".to_string(),
        username: Some(persistent.username),
        capabilities: persistent.capabilities,
        expires_in_seconds: persistent.expires_in_seconds,
        csrf_header: CSRF_TOKEN_HEADER,
        csrf_token: persistent.csrf_token,
    })
}

async fn api_pair_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PairingRequest>,
) -> Response {
    if let Some(response) = authorize_pairing_request(&state, &headers) {
        return response;
    }
    let grant = match state.local_auth.pair(&request.code) {
        Ok(grant) => grant,
        Err(PairingError::Invalid | PairingError::Unavailable) => {
            return api_error(StatusCode::UNAUTHORIZED, "pairing failed");
        }
        Err(PairingError::RateLimited) => {
            return api_error(
                StatusCode::TOO_MANY_REQUESTS,
                "pairing temporarily unavailable",
            );
        }
        Err(PairingError::Randomness) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to establish local editor session",
            );
        }
    };
    let cookie = format!(
        "{SESSION_COOKIE_NAME}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        grant.session_id, grant.expires_in_seconds
    );
    let mut response = api_response(ApiPairingGrant {
        authenticated: true,
        capabilities: PAIRED_BROWSER_CAPABILITIES
            .iter()
            .map(ToString::to_string)
            .collect(),
        csrf_header: CSRF_TOKEN_HEADER,
        csrf_token: grant.csrf_token,
        expires_in_seconds: grant.expires_in_seconds,
    });
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie
            .parse()
            .expect("random session ID forms a valid cookie"),
    );
    response
}

async fn api_session_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    let Some(session_id) = session_cookie(&headers) else {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "logout requires a paired local editor session",
        );
    };
    let persistent_user = state.users.as_ref().and_then(|users| {
        state
            .persistent_auth
            .status(session_id, users)
            .map(|status| status.username)
    });
    state.local_auth.logout(session_id);
    state.persistent_auth.logout(session_id);
    if let (Some(users), Some(username)) = (state.users.as_ref(), persistent_user.as_deref()) {
        let _ = users.record_security_event("session_logout", Some(username));
    }
    let mut response = api_response(ApiLogoutResponse {
        authenticated: false,
    });
    response.headers_mut().insert(
        header::SET_COOKIE,
        expired_session_cookie(state.mode == ServerMode::AuthenticatedTls)
            .parse()
            .expect("valid expired session cookie"),
    );
    response
}

async fn api_password_change(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PasswordChangeRequest>,
) -> Response {
    let Some(session_id) = session_cookie(&headers) else {
        return api_error(StatusCode::UNAUTHORIZED, "missing persistent session");
    };
    let Some(users) = state.users.clone() else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistent users unavailable",
        );
    };
    let Some(session) = state.persistent_auth.status(session_id, &users) else {
        return api_error(StatusCode::UNAUTHORIZED, "invalid or expired session");
    };
    let current = Zeroizing::new(request.current_password);
    let new_password = Zeroizing::new(request.new_password);
    let confirmation = Zeroizing::new(request.confirm_password);
    if *new_password != *confirmation {
        return api_error(StatusCode::BAD_REQUEST, "new passwords do not match");
    }
    let username = session.username;
    let result = tokio::task::spawn_blocking({
        let users = users.clone();
        let username = username.clone();
        move || {
            if !users.authenticate(&username, &current)? {
                return Ok::<bool, String>(false);
            }
            users.change_password(&username, &new_password)?;
            Ok(true)
        }
    })
    .await;
    match result {
        Ok(Ok(true)) => {}
        Ok(Ok(false)) => return api_error(StatusCode::UNAUTHORIZED, "password change failed"),
        Ok(Err(error)) => return api_error(StatusCode::BAD_REQUEST, error),
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("password-change task failed: {error}"),
            )
        }
    }
    state.persistent_auth.revoke_user(&username);
    let mut response = api_response(ApiLogoutResponse {
        authenticated: false,
    });
    response.headers_mut().insert(
        header::SET_COOKIE,
        expired_session_cookie(true)
            .parse()
            .expect("valid expired secure session cookie"),
    );
    response
}

async fn api_sessions_revoke(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session_id) = session_cookie(&headers) else {
        return api_error(StatusCode::UNAUTHORIZED, "missing persistent session");
    };
    let Some(users) = state.users.as_ref() else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistent users unavailable",
        );
    };
    let Some(session) = state.persistent_auth.status(session_id, users) else {
        return api_error(StatusCode::UNAUTHORIZED, "invalid or expired session");
    };
    state.persistent_auth.revoke_user(&session.username);
    let _ = users.record_security_event("sessions_revoked", Some(&session.username));
    let mut response = api_response(ApiLogoutResponse {
        authenticated: false,
    });
    response.headers_mut().insert(
        header::SET_COOKIE,
        expired_session_cookie(true).parse().unwrap(),
    );
    response
}

async fn api_users_list(State(state): State<AppState>) -> Response {
    let Some(users) = state.users.as_ref() else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistent users unavailable",
        );
    };
    match users.list_users() {
        Ok(users) => api_response(users),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn api_sessions_revoke_user(
    State(state): State<AppState>,
    Json(request): Json<RevokeUserSessionsRequest>,
) -> Response {
    let Some(users) = state.users.as_ref() else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistent users unavailable",
        );
    };
    let Some(user) = users
        .current_authorization(&request.username)
        .ok()
        .flatten()
    else {
        return api_error(StatusCode::NOT_FOUND, "user does not exist or is disabled");
    };
    state.persistent_auth.revoke_user(&user.name);
    if let Err(error) = users.record_security_event("admin_sessions_revoked", Some(&user.name)) {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    api_response(serde_json::json!({"revoked": true}))
}

fn secure_session_cookie(session_id: &str, max_age: u64) -> String {
    format!(
        "{SESSION_COOKIE_NAME}={session_id}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={max_age}"
    )
}

fn expired_session_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE_NAME}=; HttpOnly; {}SameSite=Strict; Path=/; Max-Age=0",
        if secure { "Secure; " } else { "" }
    )
}

async fn api_config_roots(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    match tokio::task::spawn_blocking(move || config_roots_response(&state, false)).await {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("root configuration task failed: {error}"),
        ),
    }
}

async fn api_config_root_deprecated() -> Response {
    api_error_named(
        StatusCode::GONE,
        "root_api_superseded",
        "direct root mutation is retired; create and confirm a snapshot-bound proposal",
    )
}

async fn api_root_configuration(State(state): State<AppState>) -> Response {
    match tokio::task::spawn_blocking(move || root_configuration_response(&state)).await {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("root configuration task failed: {error}"),
        ),
    }
}

async fn api_root_monitoring_status(State(state): State<AppState>) -> Response {
    match tokio::task::spawn_blocking(move || root_monitoring_status_response(&state)).await {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("root monitoring status task failed: {error}"),
        ),
    }
}

fn root_monitoring_status_response(state: &AppState) -> Response {
    let Some(monitor) = state.root_monitor.as_ref() else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "root monitoring state unavailable",
        );
    };
    let config = match load_browser_config(&state.env_file) {
        Ok(config) => config,
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    match monitor.status(config.roots()) {
        Ok(roots) => api_response(serde_json::json!({ "roots": roots })),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn api_root_monitoring_check(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    match tokio::task::spawn_blocking(move || root_monitoring_check_response(&state, &id)).await {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("root monitoring scan task failed: {error}"),
        ),
    }
}

fn root_monitoring_check_response(state: &AppState, id: &str) -> Response {
    let Some(monitor) = state.root_monitor.as_ref() else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "root monitoring state unavailable",
        );
    };
    let root = match monitored_browser_root(state, id) {
        Ok(root) => root,
        Err(response) => return *response,
    };
    match monitor.scan(&root) {
        Ok(pending) => api_response(pending),
        Err(error) => api_error_named(StatusCode::CONFLICT, "root_scan_failed", error),
    }
}

async fn api_root_monitoring_pending(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let Some(monitor) = state.root_monitor.as_ref() else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "root monitoring state unavailable",
        );
    };
    match monitor.pending(&id) {
        Ok(Some(pending)) => api_response(pending),
        Ok(None) => api_error_named(
            StatusCode::NOT_FOUND,
            "pending_changes_not_found",
            "no pending change set exists for this root",
        ),
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn api_root_monitoring_accept(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<RootChangeReviewRequest>,
) -> Response {
    if confirmation_header(&headers, "x-okf-change-review") != Some("accept") {
        return api_error_named(
            StatusCode::PRECONDITION_REQUIRED,
            "change_review_confirmation_required",
            "accepting pending changes requires X-OKF-Change-Review: accept",
        );
    }
    match tokio::task::spawn_blocking(move || {
        let Some(monitor) = state.root_monitor.as_ref() else {
            return api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "root monitoring state unavailable",
            );
        };
        let root = match monitored_browser_root(&state, &id) {
            Ok(root) => root,
            Err(response) => return *response,
        };
        match monitor.accept(&root, &request.snapshot_digest) {
            Ok(reviewed) => api_response(serde_json::json!({
                "accepted": true,
                "snapshot_digest": reviewed.snapshot_digest,
                "change_count": reviewed.changes.len()
            })),
            Err(error) => api_error_named(StatusCode::CONFLICT, "stale_change_snapshot", error),
        }
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn api_root_monitoring_dismiss(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<RootChangeReviewRequest>,
) -> Response {
    if confirmation_header(&headers, "x-okf-change-review") != Some("dismiss") {
        return api_error_named(
            StatusCode::PRECONDITION_REQUIRED,
            "change_review_confirmation_required",
            "dismissing pending changes requires X-OKF-Change-Review: dismiss",
        );
    }
    let Some(monitor) = state.root_monitor.as_ref() else {
        return api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "root monitoring state unavailable",
        );
    };
    match monitor.dismiss(&id, &request.snapshot_digest) {
        Ok(()) => api_response(serde_json::json!({
            "dismissed": true,
            "baseline_changed": false
        })),
        Err(error) => api_error_named(StatusCode::CONFLICT, "stale_change_snapshot", error),
    }
}

fn monitored_browser_root(state: &AppState, id: &str) -> Result<BrowserRoot, Box<Response>> {
    let config = load_browser_config(&state.env_file).map_err(|error| {
        Box::new(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        ))
    })?;
    let root = config
        .roots()
        .iter()
        .find(|root| root.root_id.to_string() == id)
        .cloned()
        .ok_or_else(|| {
            Box::new(api_error(
                StatusCode::NOT_FOUND,
                "configured root not found",
            ))
        })?;
    if !root.enabled || !root.check_for_changes {
        return Err(Box::new(api_error_named(
            StatusCode::CONFLICT,
            "root_monitoring_disabled",
            "change detection is not enabled for this root",
        )));
    }
    Ok(root)
}

async fn api_root_proposal_create(
    State(state): State<AppState>,
    Json(request): Json<RootProposalCreateRequest>,
) -> Response {
    match tokio::task::spawn_blocking(move || create_root_proposal_response(&state, request)).await
    {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("root proposal task failed: {error}"),
        ),
    }
}

async fn api_root_proposal_details(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let mut store = match state.root_proposals.lock() {
        Ok(store) => store,
        Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock poisoned"),
    };
    let Some((proposal, remaining)) = store.get_with_remaining(&id, SystemTime::now()) else {
        return api_error_named(
            StatusCode::NOT_FOUND,
            "proposal_missing",
            "proposal does not exist or has expired",
        );
    };
    api_response(api_root_proposal(&id, proposal, remaining.as_secs()))
}

async fn api_root_register(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<RootRegistrationRequest>,
) -> Response {
    match tokio::task::spawn_blocking(move || {
        api_root_register_blocking(state, id, headers, request)
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("root registration task failed: {error}"),
        ),
    }
}

fn api_root_register_blocking(
    state: AppState,
    id: String,
    headers: HeaderMap,
    request: RootRegistrationRequest,
) -> Response {
    if confirmation_header(&headers, "x-okf-config-write") != Some("confirm") {
        return api_error_named(
            StatusCode::PRECONDITION_REQUIRED,
            "configuration_confirmation_required",
            "root registration requires X-OKF-Config-Write: confirm",
        );
    }
    let revision = match ConfigurationRevision::parse(request.expected_revision) {
        Ok(revision) => revision,
        Err(error) => return transaction_error_response(error),
    };
    let mut store = match state.root_proposals.lock() {
        Ok(store) => store,
        Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock poisoned"),
    };
    if store
        .get(&id, SystemTime::now())
        .is_some_and(|proposal| proposal.proposal_digest() != request.proposal_digest)
    {
        return api_error_named(
            StatusCode::CONFLICT,
            "proposal_digest_mismatch",
            "proposal digest does not match the reviewed proposal",
        );
    }
    let monitoring_seed = store.get(&id, SystemTime::now()).and_then(|proposal| {
        proposal
            .registration_change()
            .filter(|change| change.check_for_changes)
            .map(|change| (change.root_id.to_string(), proposal.inventory().clone()))
    });
    let result = confirm_root_registration(
        &state.env_file,
        &mut store,
        &id,
        SystemTime::now(),
        &revision,
    );
    audit_root_action("root_registered", &id, result.is_ok());
    match result {
        Ok(report) => {
            if let (Some(monitor), Some((root_id, inventory))) =
                (state.root_monitor.as_ref(), monitoring_seed)
            {
                if let Err(error) = monitor.seed_reviewed_baseline(&root_id, &inventory) {
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("root registered but monitoring baseline failed: {error}"),
                    );
                }
            }
            api_response(ApiRootMutation {
                changed: report.changed,
                revision: report.revision.as_str().to_string(),
                restart_required: report.changed,
            })
        }
        Err(error) => transaction_error_response(error),
    }
}

async fn api_root_initialization_plan(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<RootInitializationOptionsRequest>,
) -> Response {
    match tokio::task::spawn_blocking(move || {
        api_root_initialization_plan_blocking(state, id, request)
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("root initialization planning task failed: {error}"),
        ),
    }
}

fn api_root_initialization_plan_blocking(
    state: AppState,
    id: String,
    request: RootInitializationOptionsRequest,
) -> Response {
    let mut store = match state.root_proposals.lock() {
        Ok(store) => store,
        Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock poisoned"),
    };
    let proposal = match store.validate(&id, SystemTime::now()) {
        Ok(ProposalValidation::Fresh(proposal)) => proposal,
        Ok(ProposalValidation::Stale) => {
            return api_error_named(StatusCode::CONFLICT, "proposal_stale", "proposal is stale")
        }
        Ok(ProposalValidation::Expired) => {
            return api_error_named(StatusCode::CONFLICT, "proposal_expired", "proposal expired")
        }
        Ok(ProposalValidation::Missing) => {
            return api_error_named(
                StatusCode::NOT_FOUND,
                "proposal_missing",
                "proposal is missing",
            )
        }
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    if proposal.proposal_digest() != request.proposal_digest {
        return api_error_named(
            StatusCode::CONFLICT,
            "proposal_digest_mismatch",
            "proposal digest does not match the reviewed proposal",
        );
    }
    let options = InitializationOptions {
        resource_types: request.resource_types,
    };
    match build_initialization_plan(&proposal, &options) {
        Ok(plan) => api_response(api_initialization_plan(&plan)),
        Err(error) => transaction_error_response(error),
    }
}

async fn api_root_initialize(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<RootInitializationRequest>,
) -> Response {
    match tokio::task::spawn_blocking(move || {
        api_root_initialize_blocking(state, id, headers, request)
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("root initialization task failed: {error}"),
        ),
    }
}

fn api_root_initialize_blocking(
    state: AppState,
    id: String,
    headers: HeaderMap,
    request: RootInitializationRequest,
) -> Response {
    if confirmation_header(&headers, "x-okf-source-write") != Some("confirm") {
        return api_error_named(
            StatusCode::PRECONDITION_REQUIRED,
            "source_confirmation_required",
            "source initialization requires X-OKF-Source-Write: confirm",
        );
    }
    let mut store = match state.root_proposals.lock() {
        Ok(store) => store,
        Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock poisoned"),
    };
    if store
        .get(&id, SystemTime::now())
        .is_some_and(|proposal| proposal.proposal_digest() != request.proposal_digest)
    {
        return api_error_named(
            StatusCode::CONFLICT,
            "proposal_digest_mismatch",
            "proposal digest does not match the reviewed proposal",
        );
    }
    let initialized_root = store
        .get(&id, SystemTime::now())
        .map(|proposal| proposal.canonical_root().to_path_buf());
    let options = InitializationOptions {
        resource_types: request.resource_types,
    };
    let result = confirm_source_initialization(
        &state.root_state_dir,
        &mut store,
        &id,
        SystemTime::now(),
        &options,
        &request.plan_digest,
    );
    audit_root_action("root_initialized", &id, result.is_ok());
    match result {
        Ok(report) => {
            if let (Some(monitor), Some(path)) = (state.root_monitor.as_ref(), initialized_root) {
                if let Ok(config) = load_browser_config(&state.env_file) {
                    if let Some(root) = config
                        .roots()
                        .iter()
                        .find(|root| root.enabled && root.check_for_changes && root.path == path)
                    {
                        if let Err(error) = monitor.replace_reviewed_baseline(root) {
                            return api_error(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!(
                                    "source initialized but monitoring baseline failed: {error}"
                                ),
                            );
                        }
                    }
                }
            }
            api_response(ApiInitializationResult {
                changed_files: report.changed_files,
                recovered_interrupted_operation: report.recovered_interrupted_operation,
                git_dirty_before: report.git_before.as_ref().is_some_and(|git| git.dirty),
                git_dirty_after: report.git_after.as_ref().is_some_and(|git| git.dirty),
                final_diffs: report.final_diffs,
            })
        }
        Err(error) => transaction_error_response(error),
    }
}

async fn api_root_update(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
    Json(request): Json<RootUpdateRequest>,
) -> Response {
    match tokio::task::spawn_blocking(move || api_root_update_blocking(state, id, headers, request))
        .await
    {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("root update task failed: {error}"),
        ),
    }
}

fn api_root_update_blocking(
    state: AppState,
    id: String,
    headers: HeaderMap,
    request: RootUpdateRequest,
) -> Response {
    if confirmation_header(&headers, "x-okf-config-write") != Some("confirm") {
        return api_error_named(
            StatusCode::PRECONDITION_REQUIRED,
            "configuration_confirmation_required",
            "root update requires X-OKF-Config-Write: confirm",
        );
    }
    let root_id = match RootId::parse(id.clone()) {
        Ok(root_id) => root_id,
        Err(_) => return api_error(StatusCode::BAD_REQUEST, "invalid root identity"),
    };
    let revision = match ConfigurationRevision::parse(request.expected_revision) {
        Ok(revision) => revision,
        Err(error) => return transaction_error_response(error),
    };
    let update = RootConfigurationUpdate {
        mount: if request.clear_mount {
            Some(None)
        } else {
            request.mount.map(Some)
        },
        enabled: request.enabled,
        priority: request.priority,
        check_for_changes: request.check_for_changes,
    };
    let result = update_registered_root(&state.env_file, &root_id, &revision, &update);
    audit_root_action("root_updated", &id, result.is_ok());
    match result {
        Ok(report) => api_response(ApiRootMutation {
            changed: report.changed,
            revision: report.revision.as_str().to_string(),
            restart_required: report.changed,
        }),
        Err(error) => transaction_error_response(error),
    }
}

async fn api_root_remove(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Query(request): Query<RootRemovalRequest>,
    headers: HeaderMap,
) -> Response {
    match tokio::task::spawn_blocking(move || api_root_remove_blocking(state, id, request, headers))
        .await
    {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("root removal task failed: {error}"),
        ),
    }
}

fn api_root_remove_blocking(
    state: AppState,
    id: String,
    request: RootRemovalRequest,
    headers: HeaderMap,
) -> Response {
    if confirmation_header(&headers, "x-okf-config-write") != Some("confirm") {
        return api_error_named(
            StatusCode::PRECONDITION_REQUIRED,
            "configuration_confirmation_required",
            "root removal requires X-OKF-Config-Write: confirm",
        );
    }
    let root_id = match RootId::parse(id.clone()) {
        Ok(root_id) => root_id,
        Err(_) => return api_error(StatusCode::BAD_REQUEST, "invalid root identity"),
    };
    let revision = match ConfigurationRevision::parse(request.expected_revision) {
        Ok(revision) => revision,
        Err(error) => return transaction_error_response(error),
    };
    let result = remove_registered_root(&state.env_file, &root_id, &revision);
    audit_root_action("root_removed", &id, result.is_ok());
    match result {
        Ok(report) => {
            if let Some(monitor) = state.root_monitor.as_ref() {
                if let Err(error) = monitor.remove(&id) {
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("root removed but monitoring cleanup failed: {error}"),
                    );
                }
            }
            api_response(ApiRootMutation {
                changed: report.changed,
                revision: report.revision.as_str().to_string(),
                restart_required: report.changed,
            })
        }
        Err(error) => transaction_error_response(error),
    }
}

fn config_roots_response(state: &AppState, restart_required: bool) -> Response {
    let persistent_config = match load_browser_config(&state.env_file) {
        Ok(config) => config,
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let persistent = persistent_config.enabled_document_roots();
    let process_override_active = env::var_os(ROOTS_ENV_KEY).is_some();
    let dotenv_override_active = dotenv_value(ROOTS_ENV_KEY).is_some();
    api_response(ApiRootConfiguration {
        effective: state
            .document_roots
            .iter()
            .map(|root| api_root_config_entry(root, state.expose_physical_paths))
            .collect(),
        persistent: persistent_config
            .roots()
            .iter()
            .map(|root| api_browser_root_config_entry(root, state.expose_physical_paths))
            .collect(),
        environment_override_active: process_override_active,
        dotenv_override_active,
        operator_override_active: state.environment_roots_active
            || process_override_active
            || dotenv_override_active
            || state.document_roots != persistent,
        restart_required,
    })
}

fn root_configuration_response(state: &AppState) -> Response {
    let config = match load_browser_config(&state.env_file) {
        Ok(config) => config,
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let revision = match configuration_revision(&state.env_file) {
        Ok(revision) => revision,
        Err(error) => return transaction_error_response(error),
    };
    let enabled = config.enabled_document_roots();
    api_response(ApiManagedRootConfiguration {
        revision: revision.as_str().to_string(),
        roots: config
            .roots()
            .iter()
            .map(|root| api_browser_root_config_entry(root, true))
            .collect(),
        process_override_active: env::var_os(ROOTS_ENV_KEY).is_some(),
        dotenv_override_active: dotenv_value(ROOTS_ENV_KEY).is_some(),
        effective_differs_from_browser_configuration: state.document_roots != enabled,
        restart_required: false,
    })
}

fn create_root_proposal_response(state: &AppState, request: RootProposalCreateRequest) -> Response {
    let kind = match request.operation.as_str() {
        "registration" => RootProposalKind::Registration,
        "source_initialization" => RootProposalKind::SourceInitialization,
        _ => {
            return api_error_named(
                StatusCode::BAD_REQUEST,
                "invalid_proposal_operation",
                "operation must be registration or source_initialization",
            )
        }
    };
    let config = match load_browser_config(&state.env_file) {
        Ok(config) => config,
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let priority = request.priority.unwrap_or_else(|| {
        config
            .roots()
            .iter()
            .map(|root| root.priority)
            .max()
            .unwrap_or(-100)
            + 100
    });
    let proposed_root_id = if kind == RootProposalKind::SourceInitialization {
        match generate_root_id() {
            Ok(root_id) => Some(root_id),
            Err(error) => {
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to generate root identity: {error}"),
                )
            }
        }
    } else {
        None
    };
    let dotenv_override_active = dotenv_value(ROOTS_ENV_KEY).is_some();
    let process_override_active = env::var_os(ROOTS_ENV_KEY).is_some();
    let browser_roots = config.enabled_document_roots();
    let proposal = match build_root_proposal(RootProposalRequest {
        root: PathBuf::from(request.path),
        mount: request.mount,
        kind,
        limits: AdmissionLimits::default(),
        context: RootProposalContext {
            configured_roots: state.document_roots.clone(),
            proposed_root_id,
            registration_enabled: request.enabled.unwrap_or(true),
            registration_priority: priority,
            check_for_changes: request.check_for_changes.unwrap_or(false),
            process_override_active,
            dotenv_override_active,
            cli_override_active: !process_override_active
                && !dotenv_override_active
                && state.document_roots != browser_roots,
        },
    }) {
        Ok(proposal) => proposal,
        Err(error) => return proposal_error_response(error),
    };
    let mut store = match state.root_proposals.lock() {
        Ok(store) => store,
        Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "proposal lock poisoned"),
    };
    let id = store.insert(proposal, SystemTime::now());
    let (proposal, remaining) = store
        .get_with_remaining(&id, SystemTime::now())
        .expect("inserted proposal");
    audit_root_action("root_proposal_created", &id, true);
    api_response(api_root_proposal(&id, proposal, remaining.as_secs()))
}

fn api_root_proposal(
    id: &str,
    proposal: &RootProposal,
    expires_in_seconds: u64,
) -> ApiRootProposal {
    let registration = proposal
        .registration_change()
        .map(|change| ApiRegistrationChange {
            root_id: change.root_id.to_string(),
            mount: change.mount,
            canonical_path: change.canonical_path.display().to_string(),
            enabled: change.enabled,
            priority: change.priority,
            check_for_changes: change.check_for_changes,
        });
    ApiRootProposal {
        id: id.to_string(),
        operation: match proposal.kind() {
            RootProposalKind::Registration => "registration",
            RootProposalKind::SourceInitialization => "source_initialization",
        },
        canonical_root: proposal.canonical_root().display().to_string(),
        mount: proposal.mount().map(str::to_string),
        root_id: proposal.root_id().map(ToString::to_string),
        proposed_root_id: proposal.proposed_root_id().map(ToString::to_string),
        snapshot_digest: proposal.snapshot_digest().to_string(),
        proposal_digest: proposal.proposal_digest().to_string(),
        confirmable: proposal.can_confirm(),
        expires_in_seconds,
        registration,
        summary: ApiRootProposalSummary {
            accepted_files: proposal.summary().accepted_files,
            rejected_entries: proposal.summary().rejected_entries,
            inspected_entries: proposal.summary().inspected_entries,
            total_candidate_bytes: proposal.summary().total_candidate_bytes,
            writable: proposal.summary().writable,
            process_override_active: proposal.summary().process_override_active,
            dotenv_override_active: proposal.summary().dotenv_override_active,
            cli_override_active: proposal.summary().cli_override_active,
            max_file_bytes: proposal.limits().max_file_bytes,
            max_entries: proposal.limits().max_entries,
            max_depth: proposal.limits().max_depth,
            max_total_bytes: proposal.limits().max_total_bytes,
        },
        tree: proposal
            .tree()
            .iter()
            .map(|entry| ApiProposalTreeEntry {
                path: entry.path().to_string(),
                state: entry.state().code(),
                detail: entry.detail().map(str::to_string),
            })
            .collect(),
        conflicts: proposal
            .conflicts()
            .iter()
            .map(|conflict| ApiProposalConflict {
                code: conflict.code().code(),
                path: conflict.path().map(str::to_string),
                detail: conflict.detail().to_string(),
                blocking: conflict.code().blocks_registration()
                    || (proposal.kind() == RootProposalKind::Registration
                        && conflict.code() == ProposalConflictCode::MissingRootIdentity),
            })
            .collect(),
    }
}

fn api_initialization_plan(plan: &okf::InitializationPlan) -> ApiInitializationPlan {
    ApiInitializationPlan {
        proposal_digest: plan.proposal_digest().to_string(),
        plan_digest: plan.plan_digest().to_string(),
        canonical_root: plan.root().display().to_string(),
        git: plan.git().map(|git| ApiGitStatus {
            worktree: git.root.display().to_string(),
            dirty: git.dirty,
            entries: git.entries.clone(),
        }),
        changes: plan
            .changes()
            .iter()
            .map(|change| ApiSourceFileChange {
                path: change.path().to_string(),
                before: change.before().map(str::to_string),
                after: change.after().to_string(),
                diff: change.diff().to_string(),
            })
            .collect(),
    }
}

fn confirmation_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn audit_root_action(action: &str, proposal_or_root_id: &str, success: bool) {
    eprintln!(
        "{}",
        serde_json::json!({
            "event": "root_management",
            "action": action,
            "reference": proposal_or_root_id,
            "success": success,
        })
    );
}

fn proposal_error_response(error: okf::ProposalError) -> Response {
    match error {
        okf::ProposalError::Admission(_) | okf::ProposalError::InvalidMount(_) => api_error_named(
            StatusCode::BAD_REQUEST,
            "proposal_invalid",
            error.to_string(),
        ),
        okf::ProposalError::Canonicalize(_) => api_error_named(
            StatusCode::BAD_REQUEST,
            "root_unavailable",
            error.to_string(),
        ),
        okf::ProposalError::UnstableSnapshot => {
            api_error_named(StatusCode::CONFLICT, "proposal_unstable", error.to_string())
        }
    }
}

fn transaction_error_response(error: TransactionError) -> Response {
    match error {
        TransactionError::ProposalMissing => {
            api_error_named(StatusCode::NOT_FOUND, "proposal_missing", error.to_string())
        }
        TransactionError::ProposalExpired => {
            api_error_named(StatusCode::CONFLICT, "proposal_expired", error.to_string())
        }
        TransactionError::ProposalStale => {
            api_error_named(StatusCode::CONFLICT, "proposal_stale", error.to_string())
        }
        TransactionError::RevisionConflict { .. } => {
            api_error_named(StatusCode::CONFLICT, "revision_conflict", error.to_string())
        }
        TransactionError::PlanConflict { .. } => {
            api_error_named(StatusCode::CONFLICT, "plan_conflict", error.to_string())
        }
        TransactionError::WrongProposalKind => api_error_named(
            StatusCode::BAD_REQUEST,
            "wrong_proposal_kind",
            error.to_string(),
        ),
        TransactionError::ProposalNotConfirmable
        | TransactionError::ExistingField { .. }
        | TransactionError::InvalidResourceType(_)
        | TransactionError::ResourceNotProposed(_)
        | TransactionError::Configuration(_) => api_error_named(
            StatusCode::UNPROCESSABLE_ENTITY,
            "operation_not_confirmable",
            error.to_string(),
        ),
        TransactionError::UnsafePath(_) => {
            api_error_named(StatusCode::FORBIDDEN, "unsafe_path", error.to_string())
        }
        TransactionError::Io { .. } | TransactionError::Journal(_) => {
            api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
    }
}

async fn health() -> &'static str {
    "okf-http"
}

async fn api_health(State(state): State<AppState>) -> Response {
    api_response(ApiHealth {
        status: "ok",
        mode: state.mode.as_str(),
        tls: state.tls_status,
        remote_access: state.remote_access,
        trusted_proxy_origin: state
            .trusted_proxy
            .as_ref()
            .map(|proxy| proxy.public_origin.clone()),
        anonymous_document_reads: !state.remote_access && state.trusted_proxy.is_none(),
    })
}

async fn redirect_to_browser() -> Redirect {
    Redirect::temporary("/docs-browser/index.html")
}

async fn serve_browser_asset(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    serve_static_file_response(
        &state.browser_root,
        &path,
        "path escapes browser root\n",
        "browser asset not found\n",
    )
    .await
}

async fn serve_okf_document(
    State(state): State<AppState>,
    AxumPath((mount, path)): AxumPath<(String, String)>,
) -> Response {
    serve_document_from_mount(&state, &mount, &path).await
}

async fn serve_unmounted_okf_document(
    State(state): State<AppState>,
    AxumPath((index, path)): AxumPath<(usize, String)>,
) -> Response {
    let Some(root) = state.document_roots.get(index) else {
        return (StatusCode::NOT_FOUND, "document root not found\n").into_response();
    };
    if root.mount().is_some() {
        return (StatusCode::NOT_FOUND, "document root not found\n").into_response();
    }
    if !monitor_allows_document(&state, root, &path) {
        return (
            StatusCode::CONFLICT,
            "document change is pending explicit review\n",
        )
            .into_response();
    }
    let root = root.path().to_path_buf();
    let resolved = tokio::task::spawn_blocking(move || resolve_admitted_file(&root, &path)).await;
    match resolved {
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "document lookup failed\n",
        )
            .into_response(),
        Ok(Ok(file)) => serve_okf_file_response(&file, "document not found\n").await,
        Ok(Err(StaticFileError::EscapesRoot)) => {
            (StatusCode::FORBIDDEN, "path escapes document root\n").into_response()
        }
        Ok(Err(StaticFileError::Missing)) => {
            (StatusCode::NOT_FOUND, "document not found\n").into_response()
        }
    }
}

async fn serve_legacy_scanlab_document(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    serve_document_from_mount(&state, "scanlab", &path).await
}

async fn serve_legacy_scql_document(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    serve_document_from_mount(&state, "scql", &path).await
}

async fn serve_legacy_okf_document(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    serve_document_from_mount(&state, "okf", &path).await
}

async fn serve_document_from_mount(state: &AppState, mount: &str, path: &str) -> Response {
    if !okf::is_valid_mount_name(mount) {
        return (StatusCode::FORBIDDEN, "path escapes document root\n").into_response();
    }
    for root in state
        .document_roots
        .iter()
        .filter(|root| root_mount_name(root).as_deref() == Some(mount))
    {
        if !monitor_allows_document(state, root, path) {
            return (
                StatusCode::CONFLICT,
                "document change is pending explicit review\n",
            )
                .into_response();
        }
        let root_path = root.path().to_path_buf();
        let request_path = path.to_string();
        match tokio::task::spawn_blocking(move || resolve_admitted_file(&root_path, &request_path))
            .await
        {
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "document lookup failed\n",
                )
                    .into_response()
            }
            Ok(Ok(file)) => return serve_okf_file_response(&file, "document not found\n").await,
            Ok(Err(StaticFileError::EscapesRoot)) => {
                return (StatusCode::FORBIDDEN, "path escapes document root\n").into_response()
            }
            Ok(Err(StaticFileError::Missing)) => {}
        }
    }
    (StatusCode::NOT_FOUND, "document not found\n").into_response()
}

fn monitor_allows_document(state: &AppState, root: &DocumentRoot, relative_path: &str) -> bool {
    let Some(monitor) = state.root_monitor.as_ref() else {
        return true;
    };
    let Ok(config) = load_browser_config(&state.env_file) else {
        return false;
    };
    let canonical = root
        .path()
        .canonicalize()
        .unwrap_or_else(|_| root.path().to_path_buf());
    let Some(configured) = config.roots().iter().find(|candidate| {
        candidate.enabled
            && candidate.check_for_changes
            && candidate
                .path
                .canonicalize()
                .unwrap_or_else(|_| candidate.path.clone())
                == canonical
    }) else {
        return true;
    };
    monitor
        .allows(&configured.root_id.to_string(), root.path(), relative_path)
        .unwrap_or(false)
}

fn monitor_allows_physical_document(state: &AppState, physical_path: &Path) -> bool {
    state.document_roots.iter().any(|root| {
        physical_path
            .strip_prefix(root.path())
            .ok()
            .is_some_and(|relative| {
                monitor_allows_document(state, root, &relative.to_string_lossy())
            })
    })
}

async fn serve_repo_file(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    if !is_allowed_repo_file(&path) {
        return (StatusCode::NOT_FOUND, "repository file not found\n").into_response();
    }
    serve_static_file_response(
        &state.repo_root,
        &path,
        "path escapes repository file allowlist\n",
        "repository file not found\n",
    )
    .await
}

async fn api_documents(State(state): State<AppState>) -> Response {
    match tokio::task::spawn_blocking(move || api_documents_blocking(&state)).await {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("document inventory task failed: {error}"),
        ),
    }
}

fn api_documents_blocking(state: &AppState) -> Response {
    let repository = match open_repository(state) {
        Ok(repository) => repository,
        Err(error) => return api_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };

    api_response(OkfDocumentsResponse {
        roots: state
            .document_roots
            .iter()
            .enumerate()
            .map(|(index, root)| api_root(root, index, state.expose_physical_paths))
            .collect(),
        diagnostics: repository
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                api_diagnostic(
                    diagnostic,
                    &state.document_roots,
                    state.expose_physical_paths,
                )
            })
            .collect(),
        documents: repository
            .documents()
            .iter()
            .filter(|document| monitor_allows_physical_document(state, &document.physical_path()))
            .map(|document| {
                api_document_summary(document, &state.document_roots, state.expose_physical_paths)
            })
            .collect(),
    })
}

#[derive(Deserialize)]
struct DocumentQueryParams {
    path: String,
}

async fn api_document(
    State(state): State<AppState>,
    Query(query): Query<DocumentQueryParams>,
) -> Response {
    match tokio::task::spawn_blocking(move || api_document_blocking(&state, &query.path)).await {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("document lookup task failed: {error}"),
        ),
    }
}

fn api_document_blocking(state: &AppState, query_path: &str) -> Response {
    let repository = match open_repository(state) {
        Ok(repository) => repository,
        Err(error) => return api_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };

    let Some(path) = normalize_api_document_path(query_path) else {
        return api_error(StatusCode::BAD_REQUEST, "invalid document path");
    };
    let Some(document) = repository.documents().iter().find(|document| {
        document.relative_path() == path
            && monitor_allows_physical_document(state, &document.physical_path())
    }) else {
        return api_error(StatusCode::NOT_FOUND, "document not found");
    };

    let source = match std::fs::read_to_string(document.physical_path()) {
        Ok(source) => source,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to read document source: {error}"),
            );
        }
    };

    api_response(OkfDocumentResponse {
        document: api_document_summary(
            document,
            &state.document_roots,
            state.expose_physical_paths,
        ),
        frontmatter: document.frontmatter().clone(),
        planning: ApiPlanningSections {
            completed: document.planning().completed.clone(),
            open: document.planning().open.clone(),
            deferred: document.planning().deferred.clone(),
        },
        source,
    })
}

async fn api_graph(State(state): State<AppState>) -> Response {
    match tokio::task::spawn_blocking(move || api_graph_blocking(&state)).await {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("graph extraction task failed: {error}"),
        ),
    }
}

fn api_graph_blocking(state: &AppState) -> Response {
    let repository = match open_repository(state) {
        Ok(repository) => repository,
        Err(error) => return api_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let document_paths = repository
        .documents()
        .iter()
        .filter(|document| monitor_allows_physical_document(state, &document.physical_path()))
        .map(|document| document.relative_path().to_path_buf())
        .collect::<BTreeSet<_>>();
    let mut topic_nodes = BTreeSet::new();
    let mut edge_ids = BTreeSet::new();

    for document in repository
        .documents()
        .iter()
        .filter(|document| monitor_allows_physical_document(state, &document.physical_path()))
    {
        let document_id = logical_path_string(document.relative_path());
        nodes.push(ApiGraphNode {
            id: document_id.clone(),
            node_type: "document".to_string(),
            title: Some(document.title().to_string()),
            topic: document.topic().map(str::to_string),
            kind: document.kind().map(|kind| kind.as_str().to_string()),
            document_type: document
                .document_type()
                .map(|kind| kind.as_str().to_string()),
            path: Some(document_id.clone()),
        });

        if let Some(topic) = document.topic() {
            let topic_id = format!("topic:{topic}");
            topic_nodes.insert(topic_id.clone());
            let edge_id = format!("{document_id}->{topic_id}");
            if edge_ids.insert(edge_id.clone()) {
                edges.push(ApiGraphEdge {
                    id: edge_id,
                    edge_type: "has_topic".to_string(),
                    source: document_id.clone(),
                    target: topic_id,
                    provenance: None,
                });
            }
        }

        let source = std::fs::read_to_string(document.physical_path()).unwrap_or_default();
        for target in extract_markdown_link_targets(&source, document.relative_path()) {
            if !document_paths.contains(&target) {
                continue;
            }
            let target_id = logical_path_string(&target);
            let edge_id = format!("{document_id}->{target_id}");
            if edge_ids.insert(edge_id.clone()) {
                edges.push(ApiGraphEdge {
                    id: edge_id,
                    edge_type: "links_to".to_string(),
                    source: document_id.clone(),
                    target: target_id,
                    provenance: None,
                });
            }
        }

        for relation in document.relations() {
            if !document_paths.contains(relation.target()) {
                continue;
            }
            let target_id = logical_path_string(relation.target());
            let relation_id = relation
                .suggestion_id()
                .map(|id| format!("canonical:{id}"))
                .unwrap_or_else(|| {
                    format!(
                        "canonical:{document_id}->{target_id}:{}",
                        relation.relation_type()
                    )
                });
            if edge_ids.insert(relation_id.clone()) {
                edges.push(ApiGraphEdge {
                    id: relation_id,
                    edge_type: "canonical_relation".to_string(),
                    source: document_id.clone(),
                    target: target_id,
                    provenance: Some(ApiRelationProvenance {
                        relation_type: relation.relation_type().to_string(),
                        suggestion_id: relation.suggestion_id().map(str::to_string),
                        source_chunk: relation.source_chunk().map(str::to_string),
                        target_chunk: relation.target_chunk().map(str::to_string),
                        provider: relation.provider().map(str::to_string),
                        model: relation.model().map(str::to_string),
                        generation_method: relation.generation_method().map(str::to_string),
                        ai_generated: relation.ai_generated(),
                        score: relation.score().map(str::to_string),
                        created_at: relation.created_at().map(str::to_string),
                        status: relation.status().map(str::to_string),
                    }),
                });
            }
        }
    }

    for topic_id in topic_nodes {
        nodes.push(ApiGraphNode {
            id: topic_id.clone(),
            node_type: "topic".to_string(),
            title: Some(topic_id.trim_start_matches("topic:").to_string()),
            topic: None,
            kind: None,
            document_type: None,
            path: None,
        });
    }

    api_response(OkfGraphResponse { nodes, edges })
}

async fn api_voyage_status(State(state): State<AppState>) -> Response {
    let expose_physical_paths = state.expose_physical_paths;
    match tokio::task::spawn_blocking(move || {
        let config = voyage_config_from_environment();
        let index = LocalIndex::load(config.index_root()).map_err(|error| error.to_string())?;
        let storage = api_read_only_storage(config.index_root(), expose_physical_paths)
            .map_err(|error| error.to_string())?;
        let integrity = inspect_index_integrity(config.index_root())?;
        Ok::<_, String>(ApiVoyageStatusResponse {
            provider: PROVIDER.to_string(),
            configured: api_voyage_configuration(
                &config,
                storage.as_ref().and_then(|summary| summary.path.clone()),
                expose_physical_paths,
            ),
            file_index_embeddings: index.embeddings.len(),
            storage,
            integrity,
            read_only: true,
            spends_tokens: false,
        })
    })
    .await
    {
        Ok(Ok(status)) => api_response(status),
        Ok(Err(error)) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Voyage status task failed: {error}"),
        ),
    }
}

async fn api_voyage_plan(State(state): State<AppState>) -> Response {
    match tokio::task::spawn_blocking(move || voyage_plan_response_blocking(&state)).await {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Voyage planning task failed: {error}"),
        ),
    }
}

fn voyage_plan_response_blocking(state: &AppState) -> Response {
    let repository = match open_repository(state) {
        Ok(repository) => repository,
        Err(error) => return api_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };

    let config = voyage_config_from_environment();
    let documents = match inventory(&repository) {
        Ok(documents) => documents,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to inventory OKF documents: {error}"),
            );
        }
    };
    let chunks = match chunk_repository(&repository) {
        Ok(chunks) => chunks,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to chunk OKF documents: {error}"),
            );
        }
    };
    let index = match load_existing_local_index(config.index_root()) {
        Ok(index) => Some(index),
        Err(error) => {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, error);
        }
    };
    let token_plan = TokenPlan::from_chunks(&config, &chunks, index.as_ref());
    let storage = match api_read_only_storage(config.index_root(), state.expose_physical_paths) {
        Ok(storage) => storage,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to inspect SQLite working index: {error}"),
            );
        }
    };

    api_response(ApiVoyagePlanResponse {
        provider: PROVIDER.to_string(),
        configured: api_voyage_configuration(
            &config,
            storage.as_ref().and_then(|summary| summary.path.clone()),
            state.expose_physical_paths,
        ),
        plan: ApiVoyageTokenPlan {
            documents: token_plan.documents,
            document_inventory_count: documents.len(),
            chunks: token_plan.chunks,
            estimated_tokens: token_plan.estimated_tokens,
            estimated_requests: token_plan.estimated_requests,
            cached_chunks: token_plan.cached_chunks,
            changed_chunks: token_plan.changed_chunks,
            tpm_limit: token_plan.tpm_limit,
            rpm_limit: token_plan.rpm_limit,
            within_limits: token_plan.within_limits(),
            model: token_plan.model,
        },
        storage,
        spends_tokens: false,
        message: voyage::planning_message(&config).to_string(),
    })
}

async fn api_voyage_check(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    let config = voyage_config_from_environment();
    let report = match tokio::task::spawn_blocking(move || {
        check_connectivity(&config, &CurlVoyageTransport)
    })
    .await
    {
        Ok(report) => report,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Voyage connectivity task failed: {error}"),
            );
        }
    };

    voyage_report_response(report, None)
}

async fn api_voyage_index(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    let config = voyage_config_from_environment();
    let Some(job) = state.index_jobs.try_begin(config.index_root()) else {
        return api_error(
            StatusCode::CONFLICT,
            "an indexing operation is already running for this Voyage index root",
        );
    };
    match tokio::task::spawn_blocking(move || {
        let _job = job;
        api_voyage_index_blocking(&state, config)
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Voyage indexing task failed: {error}"),
        ),
    }
}

fn api_voyage_index_blocking(state: &AppState, config: VoyageConfig) -> Response {
    let repository = match open_repository(state) {
        Ok(repository) => repository,
        Err(error) => return api_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let chunks = match chunk_repository(&repository) {
        Ok(chunks) => chunks,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to chunk OKF documents: {error}"),
            );
        }
    };
    let documents = match inventory(&repository) {
        Ok(documents) => documents,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to inventory OKF documents: {error}"),
            );
        }
    };
    let existing_index = match load_existing_local_index(config.index_root()) {
        Ok(index) => index,
        Err(error) => {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, error);
        }
    };
    let before_plan = TokenPlan::from_chunks(&config, &chunks, Some(&existing_index));
    let index_root = config.index_root().to_path_buf();
    let mut index = existing_index;
    let report = embed_changed_chunks(&config, &chunks, &mut index, &CurlVoyageTransport);
    if let Err(error) =
        persist_successful_voyage_index(&report, &documents, &chunks, &index, &index_root)
    {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    let after_cached_chunks = index.embeddings.len();
    voyage_report_response(
        report,
        Some(ApiVoyageIndexSummary {
            chunks: before_plan.chunks,
            changed_chunks_before: before_plan.changed_chunks,
            cached_chunks_before: before_plan.cached_chunks,
            cached_chunks_after: after_cached_chunks,
            estimated_tokens_before: before_plan.estimated_tokens,
            estimated_requests_before: before_plan.estimated_requests,
        }),
    )
}

fn persist_successful_voyage_index(
    report: &ConnectivityReport,
    documents: &[InventoryDocument],
    chunks: &[Chunk],
    index: &LocalIndex,
    index_root: &Path,
) -> Result<(), String> {
    if !report.success {
        return Ok(());
    }
    let sqlite = SqliteWorkingIndex::open(index_root)
        .map_err(|error| format!("failed to open SQLite working index: {error}"))?;
    let generation = sqlite
        .sync_repository_index(documents, chunks, index)
        .map_err(|error| format!("failed to commit SQLite index transaction: {error}"))?;
    mirror_sqlite_generation(&sqlite, index_root, generation)
}

fn mirror_sqlite_generation(
    sqlite: &SqliteWorkingIndex,
    index_root: &Path,
    generation: i64,
) -> Result<(), String> {
    let committed = sqlite
        .load_local_index()
        .map_err(|error| format!("failed to reload committed SQLite index: {error}"))?;
    committed.save(index_root).map_err(|error| {
        format!(
            "SQLite generation {generation} committed, but the file index mirror failed: {error}; run the Voyage rebuild action"
        )
    })?;
    atomic_write_private(
        &index_root.join("file-index-generation"),
        format!("{generation}\n").as_bytes(),
    )
    .map_err(|error| {
        format!(
            "SQLite generation {generation} committed, but its file mirror marker failed: {error}; run the Voyage rebuild action"
        )
    })
}

fn load_existing_local_index(index_root: &Path) -> Result<LocalIndex, String> {
    let sqlite_path = index_root.join("okf.sqlite");
    if sqlite_path.is_file() {
        let connection =
            Connection::open_with_flags(&sqlite_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|error| format!("failed to open existing SQLite index: {error}"))?;
        connection
            .busy_timeout(SQLITE_BUSY_TIMEOUT)
            .map_err(|error| format!("failed to configure existing SQLite index: {error}"))?;
        let generation = connection
            .query_row(
                "SELECT generation FROM index_state WHERE singleton = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .unwrap_or(None)
            .unwrap_or(0);
        let sqlite_index = load_local_index_from_connection(&connection)
            .map_err(|error| format!("failed to load existing SQLite index: {error}"))?;
        if generation > 0
            || !sqlite_index.embeddings.is_empty()
            || !sqlite_index.suggestions.is_empty()
        {
            return Ok(sqlite_index);
        }
    }
    LocalIndex::load(index_root).map_err(|error| format!("failed to load Voyage index: {error}"))
}

fn inspect_index_integrity(index_root: &Path) -> Result<ApiIndexIntegrity, String> {
    let sqlite_path = index_root.join("okf.sqlite");
    let file_index = LocalIndex::load(index_root)
        .map_err(|error| format!("failed to inspect file index mirror: {error}"))?;
    let marker_path = index_root.join("file-index-generation");
    let file_generation = if marker_path.is_file() {
        fs::read_to_string(&marker_path)
            .ok()
            .and_then(|value| value.trim().parse::<i64>().ok())
    } else {
        None
    };
    if !sqlite_path.is_file() {
        let recovery_required = !file_index.embeddings.is_empty()
            || !file_index.suggestions.is_empty()
            || file_generation.is_some();
        return Ok(ApiIndexIntegrity {
            authoritative_backend: "sqlite",
            schema_current: false,
            sqlite_generation: None,
            file_generation,
            file_mirror_consistent: !recovery_required,
            recovery_required,
            violations: recovery_required
                .then(|| "sqlite_index_missing".to_string())
                .into_iter()
                .collect(),
        });
    }

    let connection = Connection::open_with_flags(&sqlite_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("failed to inspect SQLite index: {error}"))?;
    connection
        .busy_timeout(SQLITE_BUSY_TIMEOUT)
        .map_err(|error| format!("failed to configure SQLite integrity inspection: {error}"))?;
    let schema_version = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .unwrap_or(None)
        .unwrap_or(0);
    let schema_current = schema_version == CURRENT_SQLITE_SCHEMA_VERSION;
    let sqlite_generation = if schema_current {
        connection
            .query_row(
                "SELECT generation FROM index_state WHERE singleton = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| format!("failed to inspect SQLite generation: {error}"))?
    } else {
        None
    };
    let sqlite_index = load_local_index_from_connection(&connection)
        .map_err(|error| format!("failed to inspect SQLite index contents: {error}"))?;
    let mut violations = if schema_current {
        sqlite_integrity_violations(&connection)
            .map_err(|error| format!("failed to verify SQLite integrity: {error}"))?
    } else {
        vec![format!(
            "schema_upgrade_required:{schema_version}->{CURRENT_SQLITE_SCHEMA_VERSION}"
        )]
    };
    let snapshots_match = local_index_snapshots_match(&sqlite_index, &file_index);
    let generations_match = sqlite_generation.is_some() && sqlite_generation == file_generation;
    let file_mirror_consistent = schema_current && snapshots_match && generations_match;
    if !snapshots_match {
        violations.push("file_mirror_content_mismatch".to_string());
    }
    if !generations_match {
        violations.push("file_mirror_generation_mismatch".to_string());
    }
    Ok(ApiIndexIntegrity {
        authoritative_backend: "sqlite",
        schema_current,
        sqlite_generation,
        file_generation,
        file_mirror_consistent,
        recovery_required: !violations.is_empty(),
        violations,
    })
}

async fn api_voyage_rebuild(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    let config = voyage_config_from_environment();
    let Some(job) = state.index_jobs.try_begin(config.index_root()) else {
        return api_error(
            StatusCode::CONFLICT,
            "an indexing operation is already running for this Voyage index root",
        );
    };
    match tokio::task::spawn_blocking(move || {
        let _job = job;
        api_voyage_rebuild_blocking(&state, config)
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Voyage rebuild task failed: {error}"),
        ),
    }
}

fn api_voyage_rebuild_blocking(state: &AppState, config: VoyageConfig) -> Response {
    let repository = match open_repository(state) {
        Ok(repository) => repository,
        Err(error) => return api_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let documents = match inventory(&repository) {
        Ok(documents) => documents,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to inventory OKF documents: {error}"),
            );
        }
    };
    let chunks = match chunk_repository(&repository) {
        Ok(chunks) => chunks,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to chunk OKF documents: {error}"),
            );
        }
    };
    let existing = match load_existing_local_index(config.index_root()) {
        Ok(index) => index,
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    let live_chunks = chunks
        .iter()
        .map(|chunk| (chunk.id.clone(), chunk))
        .collect::<Map<_, _>>();
    let embeddings = existing
        .embeddings
        .into_iter()
        .filter_map(|mut embedding| {
            let chunk = live_chunks.get(&embedding.chunk.id)?;
            if chunk.content_hash != embedding.chunk.content_hash {
                return None;
            }
            embedding.chunk = (*chunk).clone();
            Some(embedding)
        })
        .collect::<Vec<_>>();
    let embedded_ids = embeddings
        .iter()
        .map(|embedding| embedding.chunk.id.clone())
        .collect::<BTreeSet<_>>();
    let suggestions = existing
        .suggestions
        .into_iter()
        .filter(|suggestion| {
            suggestion.model != "pending-voyage-ai"
                && embedded_ids.contains(&suggestion.source_chunk)
                && embedded_ids.contains(&suggestion.target_chunk)
        })
        .collect::<Vec<_>>();
    let rebuilt = LocalIndex {
        embeddings,
        suggestions,
    };
    let report = ConnectivityReport::success(200, Some(0));
    if let Err(error) =
        persist_successful_voyage_index(&report, &documents, &chunks, &rebuilt, config.index_root())
    {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    let integrity = match inspect_index_integrity(config.index_root()) {
        Ok(integrity) => integrity,
        Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    api_response(ApiVoyageRebuildResponse {
        documents: documents.len(),
        chunks: chunks.len(),
        retained_embeddings: rebuilt.embeddings.len(),
        missing_embeddings: chunks.len().saturating_sub(rebuilt.embeddings.len()),
        integrity,
        spends_tokens: false,
    })
}

async fn api_suggestions_generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<GenerateSuggestionsRequest>,
) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    let threshold = request.threshold.unwrap_or(0.82);
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
        return api_error(StatusCode::BAD_REQUEST, "threshold must be between 0 and 1");
    }
    let config = voyage_config_from_environment();
    let Some(job) = state.index_jobs.try_begin(config.index_root()) else {
        return api_error(
            StatusCode::CONFLICT,
            "an index or review operation is already running for this Voyage index root",
        );
    };
    match tokio::task::spawn_blocking(move || {
        let _job = job;
        let sqlite = match SqliteWorkingIndex::open(config.index_root()) {
            Ok(sqlite) => sqlite,
            Err(error) => {
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to open SQLite working index: {error}"),
                );
            }
        };
        let (review_set, suggestions, generation) =
            match sqlite.create_similarity_review_set(threshold) {
                Ok(result) => result,
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    return api_error(
                        StatusCode::PRECONDITION_REQUIRED,
                        "Voyage embeddings are required before generating suggestions",
                    );
                }
                Err(error) => {
                    return api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("failed to generate suggestion review set: {error}"),
                    );
                }
            };
        if let Err(error) = mirror_sqlite_generation(&sqlite, config.index_root(), generation) {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, error);
        }
        api_response(ApiSuggestionsResponse {
            review_set,
            suggestions: suggestions.iter().map(api_suggestion).collect(),
        })
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("suggestion generation task failed: {error}"),
        ),
    }
}

async fn api_suggestions_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SuggestionListQuery>,
) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    match tokio::task::spawn_blocking(move || {
        let config = voyage_config_from_environment();
        let sqlite = match SqliteWorkingIndex::open_existing(config.index_root()) {
            Ok(sqlite) => sqlite,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return api_error(StatusCode::NOT_FOUND, "suggestion index not found");
            }
            Err(error) => {
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to open SQLite working index: {error}"),
                );
            }
        };
        let review_set = match sqlite.review_set(query.review_set.as_deref()) {
            Ok(review_set) => review_set,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return api_error(StatusCode::NOT_FOUND, "suggestion review set not found");
            }
            Err(error) => {
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to load suggestion review set: {error}"),
                );
            }
        };
        let suggestions = match sqlite.suggestions_for_review_set(&review_set.id) {
            Ok(suggestions) => suggestions,
            Err(error) => {
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to load suggestions: {error}"),
                );
            }
        };
        api_response(ApiSuggestionsResponse {
            review_set,
            suggestions: suggestions.iter().map(api_suggestion).collect(),
        })
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("suggestion listing task failed: {error}"),
        ),
    }
}

async fn api_suggestion_accept(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    api_set_suggestion_status(state, headers, id, SuggestedEdgeStatus::Accepted).await
}

async fn api_suggestion_deny(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    api_set_suggestion_status(state, headers, id, SuggestedEdgeStatus::Denied).await
}

async fn api_set_suggestion_status(
    state: AppState,
    headers: HeaderMap,
    id: String,
    status: SuggestedEdgeStatus,
) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    let config = voyage_config_from_environment();
    let Some(job) = state.index_jobs.try_begin(config.index_root()) else {
        return api_error(
            StatusCode::CONFLICT,
            "another index or review operation is active",
        );
    };
    match tokio::task::spawn_blocking(move || {
        let _job = job;
        let sqlite = match SqliteWorkingIndex::open(config.index_root()) {
            Ok(sqlite) => sqlite,
            Err(error) => {
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
            }
        };
        let status_name = status.as_str().to_string();
        let (review_set_id, generation) = match sqlite.set_suggestion_status(&id, status) {
            Ok(result) => result,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return api_error(StatusCode::NOT_FOUND, "suggestion not found");
            }
            Err(error) => {
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
            }
        };
        if let Err(error) = mirror_sqlite_generation(&sqlite, config.index_root(), generation) {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, error);
        }
        api_response(ApiReviewMutationResponse {
            review_set_id,
            changed: 1,
            status: status_name,
            durable: true,
        })
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn api_review_set_accept_all(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    api_set_review_set_status(state, headers, id, SuggestedEdgeStatus::Accepted).await
}

async fn api_review_set_deny_all(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    api_set_review_set_status(state, headers, id, SuggestedEdgeStatus::Denied).await
}

async fn api_set_review_set_status(
    state: AppState,
    headers: HeaderMap,
    id: String,
    status: SuggestedEdgeStatus,
) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    let config = voyage_config_from_environment();
    let Some(job) = state.index_jobs.try_begin(config.index_root()) else {
        return api_error(
            StatusCode::CONFLICT,
            "another index or review operation is active",
        );
    };
    match tokio::task::spawn_blocking(move || {
        let _job = job;
        let sqlite = match SqliteWorkingIndex::open(config.index_root()) {
            Ok(sqlite) => sqlite,
            Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        };
        let status_name = status.as_str().to_string();
        let (changed, generation) = match sqlite.set_review_set_status(&id, status) {
            Ok(result) => result,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return api_error(StatusCode::NOT_FOUND, "suggestion review set not found");
            }
            Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        };
        if let Err(error) = mirror_sqlite_generation(&sqlite, config.index_root(), generation) {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, error);
        }
        api_response(ApiReviewMutationResponse {
            review_set_id: id,
            changed,
            status: status_name,
            durable: true,
        })
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn api_suggestions_import(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(artifact): Json<ImportReviewArtifact>,
) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    let unique_ids = artifact
        .accepted_edges
        .iter()
        .map(|edge| edge.id.as_str())
        .collect::<BTreeSet<_>>();
    if artifact.artifact_type != "okf-ai-edge-review"
        || artifact.review_set_id.trim().is_empty()
        || artifact.accepted_edges.is_empty()
        || unique_ids.len() != artifact.accepted_edges.len()
        || artifact.accepted_edges.iter().any(|edge| {
            !edge.ai_generated
                || !edge.score.is_finite()
                || !(0.0..=1.0).contains(&edge.score)
                || edge.model == "pending-voyage-ai"
                || edge.id.trim().is_empty()
                || edge.provider.trim().is_empty()
                || edge.model.trim().is_empty()
                || edge.generation_method.trim().is_empty()
                || edge.source_chunk.trim().is_empty()
                || edge.target_chunk.trim().is_empty()
                || edge.created_at.trim().is_empty()
        })
    {
        return api_error(StatusCode::BAD_REQUEST, "invalid OKF AI review artifact");
    }
    let config = voyage_config_from_environment();
    let Some(job) = state.index_jobs.try_begin(config.index_root()) else {
        return api_error(
            StatusCode::CONFLICT,
            "another index or review operation is active",
        );
    };
    match tokio::task::spawn_blocking(move || {
        let _job = job;
        let sqlite = match SqliteWorkingIndex::open(config.index_root()) {
            Ok(sqlite) => sqlite,
            Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        };
        let stored = match sqlite.suggestions_for_review_set(&artifact.review_set_id) {
            Ok(suggestions) => suggestions,
            Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        };
        if !imported_edges_match(&artifact.accepted_edges, &stored) {
            return api_error(
                StatusCode::BAD_REQUEST,
                "review artifact does not match persisted suggestions",
            );
        }
        let ids = artifact
            .accepted_edges
            .iter()
            .map(|edge| edge.id.clone())
            .collect::<Vec<_>>();
        let (changed, generation) = match sqlite
            .accept_imported_suggestions(&artifact.review_set_id, &ids)
        {
            Ok(result) => result,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return api_error(StatusCode::NOT_FOUND, "review set or suggestion not found");
            }
            Err(error) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        };
        if let Err(error) = mirror_sqlite_generation(&sqlite, config.index_root(), generation) {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, error);
        }
        api_response(ApiReviewMutationResponse {
            review_set_id: artifact.review_set_id,
            changed,
            status: "accepted".to_string(),
            durable: true,
        })
    })
    .await
    {
        Ok(response) => response,
        Err(error) => api_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

fn api_suggestion(suggestion: &SuggestedEdge) -> ApiSuggestion {
    ApiSuggestion {
        id: suggestion.id.clone(),
        review_set_id: suggestion.review_set_id.clone(),
        provider: suggestion.provider.clone(),
        model: suggestion.model.clone(),
        generation_method: suggestion.generation_method.clone(),
        ai_generated: suggestion.ai_generated,
        source_chunk: suggestion.source_chunk.clone(),
        target_chunk: suggestion.target_chunk.clone(),
        source_document: chunk_document_id(&suggestion.source_chunk),
        target_document: chunk_document_id(&suggestion.target_chunk),
        score: suggestion.score,
        created_at: suggestion.created_at.clone(),
        status: suggestion.status.as_str().to_string(),
    }
}

fn chunk_document_id(chunk_id: &str) -> String {
    chunk_id
        .rsplit_once('#')
        .map_or(chunk_id, |(document, _)| document)
        .to_string()
}

fn imported_edges_match(imported: &[ImportReviewEdge], stored: &[SuggestedEdge]) -> bool {
    let stored = stored
        .iter()
        .map(|suggestion| (suggestion.id.as_str(), suggestion))
        .collect::<Map<_, _>>();
    imported.iter().all(|edge| {
        stored.get(edge.id.as_str()).is_some_and(|suggestion| {
            suggestion.provider == edge.provider
                && suggestion.model == edge.model
                && suggestion.generation_method == edge.generation_method
                && suggestion.ai_generated == edge.ai_generated
                && suggestion.source_chunk == edge.source_chunk
                && suggestion.target_chunk == edge.target_chunk
                && (suggestion.score - edge.score).abs() <= f32::EPSILON
                && suggestion.created_at == edge.created_at
        })
    })
}

fn local_index_snapshots_match(left: &LocalIndex, right: &LocalIndex) -> bool {
    let embedding_signature = |index: &LocalIndex| {
        let mut values = index
            .embeddings
            .iter()
            .map(|entry| {
                (
                    entry.chunk.id.clone(),
                    entry.chunk.content_hash.clone(),
                    entry.provider.clone(),
                    entry.model.clone(),
                    vector_to_json(&entry.embedding),
                )
            })
            .collect::<Vec<_>>();
        values.sort();
        values
    };
    embedding_signature(left) == embedding_signature(right) && left.suggestions == right.suggestions
}

#[derive(Deserialize)]
struct VoyageSearchRequest {
    query: String,
    limit: Option<usize>,
}

async fn api_voyage_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<VoyageSearchRequest>,
) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    let query = request.query.trim().to_string();
    if query.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "search query must not be empty");
    }
    let limit = request.limit.unwrap_or(10).clamp(1, 50);
    match tokio::task::spawn_blocking(move || api_voyage_search_blocking(query, limit)).await {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Voyage search task failed: {error}"),
        ),
    }
}

fn api_voyage_search_blocking(query: String, limit: usize) -> Response {
    let config = voyage_config_from_environment();
    let index = match load_existing_local_index(config.index_root()) {
        Ok(index) => index,
        Err(error) => {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, error);
        }
    };
    let sqlite = match SqliteWorkingIndex::open(config.index_root()) {
        Ok(sqlite) => sqlite,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to open SQLite working index: {error}"),
            );
        }
    };
    let sqlite_embedding_count = match sqlite.embedding_count() {
        Ok(count) => count,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to inspect SQLite embeddings: {error}"),
            );
        }
    };
    if sqlite_embedding_count == 0 && index.embeddings.is_empty() {
        return api_error(
            StatusCode::PRECONDITION_REQUIRED,
            "Voyage index is empty; run /api/okf/voyage/index first",
        );
    }

    let outcome = (move || {
        let response = CurlVoyageTransport.embed_vectors(&config, &[query]);
        if !response.report.success {
            return (response.report, Vec::new());
        }
        let Some(query_embedding) = response.embeddings.first() else {
            return (
                ConnectivityReport::failure(
                    Some(502),
                    Some("missing_embedding".to_string()),
                    "Voyage AI did not return a query embedding",
                ),
                Vec::new(),
            );
        };
        let results = if sqlite_embedding_count > 0 {
            sqlite.search(query_embedding, limit)
        } else {
            index.search(query_embedding, limit)
        };
        (response.report, results)
    })();

    let (report, results) = outcome;
    let status = voyage_report_status(&report);
    api_response_with_status(
        status,
        ApiVoyageSearchResponse {
            report: api_connectivity_report(&report),
            results: results.iter().map(api_search_result).collect(),
        },
    )
}

#[derive(Deserialize)]
struct ApplyEdgesRequest {
    dry_run: Option<bool>,
}

async fn api_edges_apply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ApplyEdgesRequest>,
) -> Response {
    if let Some(response) = authorize_sensitive_request(&state, &headers) {
        return response;
    }
    match tokio::task::spawn_blocking(move || api_edges_apply_blocking(&state, request)).await {
        Ok(response) => response,
        Err(error) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("relation apply task failed: {error}"),
        ),
    }
}

fn api_edges_apply_blocking(state: &AppState, request: ApplyEdgesRequest) -> Response {
    let config = voyage_config_from_environment();
    api_edges_apply_with_config(state, request, &config)
}

fn api_edges_apply_with_config(
    state: &AppState,
    request: ApplyEdgesRequest,
    config: &VoyageConfig,
) -> Response {
    let sqlite = match SqliteWorkingIndex::open(config.index_root()) {
        Ok(sqlite) => sqlite,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to open SQLite working index: {error}"),
            );
        }
    };
    api_edges_apply_with_sqlite(state, request, &sqlite)
}

fn api_edges_apply_with_sqlite(
    state: &AppState,
    request: ApplyEdgesRequest,
    sqlite: &SqliteWorkingIndex,
) -> Response {
    let suggestions = match sqlite.accepted_suggestions() {
        Ok(suggestions) => suggestions,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to load accepted edge suggestions: {error}"),
            );
        }
    };
    if suggestions.is_empty() {
        return api_response(ApiApplyEdgesResponse {
            dry_run: request.dry_run.unwrap_or(false),
            applied: 0,
            skipped: 0,
            files: Vec::new(),
            message: "No accepted AI-derived suggestions are ready to apply.".to_string(),
        });
    }

    let repository = match open_repository(state) {
        Ok(repository) => repository,
        Err(error) => return api_error(StatusCode::SERVICE_UNAVAILABLE, error),
    };
    let chunks = match chunk_repository(&repository) {
        Ok(chunks) => chunks,
        Err(error) => {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to validate current OKF chunks: {error}"),
            );
        }
    };
    let current_chunks = chunks
        .into_iter()
        .map(|chunk| (chunk.id, logical_path_string(&chunk.document_path)))
        .collect::<Map<_, _>>();
    let current_documents = repository
        .documents()
        .iter()
        .map(|document| {
            (
                logical_path_string(document.relative_path()),
                document.physical_path(),
            )
        })
        .collect::<Map<_, _>>();

    let dry_run = request.dry_run.unwrap_or(false);
    let mut grouped = Map::<String, Vec<AcceptedSuggestion>>::new();
    for suggestion in suggestions {
        grouped
            .entry(suggestion.source_document.clone())
            .or_default()
            .push(suggestion);
    }

    let mut applied = 0usize;
    let mut skipped = 0usize;
    let mut files = Vec::new();

    for (source_document, suggestions) in grouped {
        let Some(path) = current_documents.get(&source_document).cloned() else {
            skipped += suggestions.len();
            files.push(ApiAppliedFile {
                logical_path: source_document,
                source_path: None,
                applied: 0,
                audited: 0,
                recovered: 0,
                skipped: suggestions.len(),
                changed: false,
                error: Some("source document is not served by a configured OKF mount".to_string()),
            });
            continue;
        };
        let mut valid = Vec::new();
        let mut stale_ids = Vec::new();
        for suggestion in suggestions {
            if suggestion_is_current(&suggestion, &current_chunks, &current_documents) {
                valid.push(suggestion);
            } else {
                stale_ids.push(suggestion.id);
            }
        }
        let invalid = stale_ids.len();
        if valid.is_empty() {
            skipped += invalid;
            files.push(ApiAppliedFile {
                logical_path: source_document,
                source_path: state
                    .expose_physical_paths
                    .then(|| path.display().to_string()),
                applied: 0,
                audited: 0,
                recovered: 0,
                skipped: invalid,
                changed: false,
                error: Some(format!(
                    "source or target document/chunk changed before apply: {}",
                    stale_ids.join(", ")
                )),
            });
            continue;
        }
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                skipped += valid.len() + invalid;
                files.push(ApiAppliedFile {
                    logical_path: source_document,
                    source_path: state
                        .expose_physical_paths
                        .then(|| path.display().to_string()),
                    applied: 0,
                    audited: 0,
                    recovered: 0,
                    skipped: valid.len() + invalid,
                    changed: false,
                    error: Some(format!("failed to read source document: {error}")),
                });
                continue;
            }
        };
        let relations = valid
            .iter()
            .map(relation_from_suggestion)
            .collect::<Vec<_>>();
        let merged = match merge_relations_into_frontmatter(&source, relations) {
            Ok(merged) => merged,
            Err(error) => {
                skipped += valid.len() + invalid;
                files.push(ApiAppliedFile {
                    logical_path: source_document,
                    source_path: state
                        .expose_physical_paths
                        .then(|| path.display().to_string()),
                    applied: 0,
                    audited: 0,
                    recovered: 0,
                    skipped: valid.len() + invalid,
                    changed: false,
                    error: Some(format!("failed to edit relations metadata: {error}")),
                });
                continue;
            }
        };
        let added = merged.added_ids.len();
        let existing = merged.existing_ids.len();
        if dry_run {
            applied += added;
            skipped += invalid + existing;
            files.push(ApiAppliedFile {
                logical_path: source_document,
                source_path: state
                    .expose_physical_paths
                    .then(|| path.display().to_string()),
                applied: added,
                audited: 0,
                recovered: 0,
                skipped: invalid + existing,
                changed: false,
                error: stale_warning(&stale_ids),
            });
            continue;
        }
        if added > 0 {
            if let Err(error) = atomic_replace_preserving_permissions(&path, &merged.source) {
                skipped += valid.len() + invalid;
                files.push(ApiAppliedFile {
                    logical_path: source_document,
                    source_path: state
                        .expose_physical_paths
                        .then(|| path.display().to_string()),
                    applied: 0,
                    audited: 0,
                    recovered: 0,
                    skipped: valid.len() + invalid,
                    changed: false,
                    error: Some(format!(
                        "failed to atomically write source document: {error}"
                    )),
                });
                continue;
            }
        }
        let represented_ids = merged
            .added_ids
            .union(&merged.existing_ids)
            .cloned()
            .collect::<BTreeSet<_>>();
        let represented = valid
            .iter()
            .filter(|suggestion| represented_ids.contains(&suggestion.id))
            .cloned()
            .collect::<Vec<_>>();
        if let Err(error) = sqlite.record_applied_relations(&represented, &path) {
            applied += added;
            skipped += invalid + existing;
            files.push(ApiAppliedFile {
                logical_path: source_document,
                source_path: state
                    .expose_physical_paths
                    .then(|| path.display().to_string()),
                applied: added,
                audited: 0,
                recovered: 0,
                skipped: invalid + existing,
                changed: added > 0,
                error: Some(format!(
                    "canonical document is valid, but SQLite audit recording failed; retry apply to recover: {error}"
                )),
            });
            continue;
        }
        applied += added;
        skipped += invalid;
        files.push(ApiAppliedFile {
            logical_path: source_document,
            source_path: state
                .expose_physical_paths
                .then(|| path.display().to_string()),
            applied: added,
            audited: represented.len(),
            recovered: existing,
            skipped: invalid,
            changed: added > 0,
            error: stale_warning(&stale_ids),
        });
    }

    api_response(ApiApplyEdgesResponse {
        dry_run,
        applied,
        skipped,
        files,
        message: if dry_run {
            "Dry run complete; no OKF files were modified.".to_string()
        } else {
            "Accepted AI-derived relations were applied to OKF frontmatter.".to_string()
        },
    })
}

fn suggestion_is_current(
    suggestion: &AcceptedSuggestion,
    chunks: &Map<String, String>,
    documents: &Map<String, PathBuf>,
) -> bool {
    chunks
        .get(&suggestion.source_chunk)
        .is_some_and(|document| document == &suggestion.source_document)
        && chunks
            .get(&suggestion.target_chunk)
            .is_some_and(|document| document == &suggestion.target_document)
        && documents
            .get(&suggestion.source_document)
            .is_some_and(|source| std::fs::File::open(source).is_ok())
        && documents
            .get(&suggestion.target_document)
            .is_some_and(|target| std::fs::File::open(target).is_ok())
}

fn stale_warning(stale_ids: &[String]) -> Option<String> {
    (!stale_ids.is_empty()).then(|| {
        format!(
            "some suggestions became stale before apply: {}",
            stale_ids.join(", ")
        )
    })
}

fn atomic_replace_preserving_permissions(path: &Path, source: &str) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "document has no parent directory",
        )
    })?;
    let permissions = std::fs::metadata(path)?.permissions();
    let mut temporary = None;
    for attempt in 0..100u32 {
        let candidate = parent.join(format!(
            ".{}.okf-{}-{attempt}.tmp",
            path.file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("document"),
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    let Some((temporary_path, mut file)) = temporary else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate a temporary relation file",
        ));
    };
    let outcome = (|| {
        file.set_permissions(permissions)?;
        file.write_all(source.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary_path, path)?;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    outcome
}

async fn serve_static_file_response(
    root: &Path,
    path: &str,
    escape_message: &'static str,
    missing_message: &'static str,
) -> Response {
    let root = root.to_path_buf();
    let path = path.to_string();
    let file = match tokio::task::spawn_blocking(move || resolve_static_file(&root, &path)).await {
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "file lookup failed\n").into_response();
        }
        Ok(Ok(file)) => file,
        Ok(Err(StaticFileError::EscapesRoot)) => {
            return (StatusCode::FORBIDDEN, escape_message).into_response();
        }
        Ok(Err(StaticFileError::Missing)) => {
            return (StatusCode::NOT_FOUND, missing_message).into_response();
        }
    };
    serve_resolved_file_response(&file, missing_message).await
}

async fn serve_resolved_file_response(file: &Path, missing_message: &'static str) -> Response {
    match tokio::fs::read(&file).await {
        Ok(contents) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type_for_path(file))
            .body(Body::from(contents))
            .expect("static response should be valid"),
        Err(_) => (StatusCode::NOT_FOUND, missing_message).into_response(),
    }
}

async fn serve_okf_file_response(
    file: &ResolvedOkfFile,
    missing_message: &'static str,
) -> Response {
    match tokio::fs::read(&file.path).await {
        Ok(contents) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, file.content_type)
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .body(Body::from(contents))
            .expect("OKF response should be valid"),
        Err(_) => (StatusCode::NOT_FOUND, missing_message).into_response(),
    }
}

#[cfg(any())]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StaticFileError {
    EscapesRoot,
    Missing,
}

#[cfg(any())]
fn resolve_static_file_legacy(root: &Path, request_path: &str) -> Result<PathBuf, StaticFileError> {
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
    let file = root.join(requested);
    let file = file.canonicalize().map_err(|_| StaticFileError::Missing)?;
    if !file.starts_with(&root) {
        return Err(StaticFileError::EscapesRoot);
    }
    if !file.is_file() {
        return Err(StaticFileError::Missing);
    }
    Ok(file)
}

#[cfg(any())]
fn resolve_document_file_legacy(
    roots: &[DocumentRoot],
    mount: &str,
    request_path: &str,
) -> Result<PathBuf, StaticFileError> {
    if !is_safe_mount_name(mount) {
        return Err(StaticFileError::EscapesRoot);
    }

    let mut saw_matching_mount = false;
    for root in roots {
        if root_mount_name(root).as_deref() != Some(mount) {
            continue;
        }
        saw_matching_mount = true;
        match resolve_static_file(root.path(), request_path) {
            Ok(file) => return Ok(file),
            Err(StaticFileError::EscapesRoot) => return Err(StaticFileError::EscapesRoot),
            Err(StaticFileError::Missing) => {}
        }
    }

    if saw_matching_mount {
        Err(StaticFileError::Missing)
    } else {
        Err(StaticFileError::Missing)
    }
}

#[cfg(any())]
fn root_mount_name_legacy(root: &DocumentRoot) -> Option<String> {
    root.mount().and_then(|mount| {
        let value = mount.to_string_lossy();
        is_safe_mount_name(&value).then(|| value.to_string())
    })
}

#[cfg(any())]
fn is_safe_mount_name_legacy(value: &str) -> bool {
    is_valid_mount_name(value)
}

#[cfg(any())]
fn is_allowed_repo_file_legacy(path: &str) -> bool {
    matches!(path, "README.md" | "README.de.md" | "HOSTS.md")
}

#[cfg(any())]
fn content_type_for_path_legacy(path: &Path) -> &'static str {
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

#[cfg(any())]
#[derive(Serialize)]
struct ApiEnvelopeLegacy<T> {
    api_version: &'static str,
    data: T,
}

#[cfg(any())]
#[derive(Serialize)]
struct ApiErrorEnvelopeLegacy {
    api_version: &'static str,
    error: ApiError,
}

#[cfg(any())]
#[derive(Serialize)]
struct ApiErrorLegacy {
    code: &'static str,
    message: String,
}

#[derive(Serialize)]
struct OkfDocumentsResponse {
    roots: Vec<ApiRoot>,
    diagnostics: Vec<ApiDiagnostic>,
    documents: Vec<ApiDocumentSummary>,
}

#[derive(Serialize)]
struct ApiHealth {
    status: &'static str,
    mode: &'static str,
    tls: Option<TlsStatus>,
    remote_access: bool,
    trusted_proxy_origin: Option<String>,
    anonymous_document_reads: bool,
}

#[derive(Serialize)]
struct ApiSessionStatus {
    mode: &'static str,
    pairing_available: bool,
    authenticated: bool,
    expires_in_seconds: Option<u64>,
    scope: Option<String>,
    username: Option<String>,
    capabilities: Vec<String>,
}

#[derive(Serialize)]
struct ApiSessionRefresh {
    authenticated: bool,
    scope: String,
    username: Option<String>,
    capabilities: Vec<String>,
    expires_in_seconds: u64,
    csrf_header: &'static str,
    csrf_token: String,
}

#[derive(Serialize)]
struct ApiLoginGrant {
    authenticated: bool,
    username: String,
    capabilities: Vec<String>,
    csrf_header: &'static str,
    csrf_token: String,
    expires_in_seconds: u64,
}

#[derive(Serialize)]
struct ApiPairingGrant {
    authenticated: bool,
    capabilities: Vec<String>,
    csrf_header: &'static str,
    csrf_token: String,
    expires_in_seconds: u64,
}

#[derive(Serialize)]
struct ApiLogoutResponse {
    authenticated: bool,
}

#[derive(Serialize)]
struct OkfDocumentResponse {
    document: ApiDocumentSummary,
    frontmatter: Map<String, String>,
    planning: ApiPlanningSections,
    source: String,
}

#[derive(Serialize)]
struct OkfGraphResponse {
    nodes: Vec<ApiGraphNode>,
    edges: Vec<ApiGraphEdge>,
}

#[derive(Serialize)]
struct ApiVoyagePlanResponse {
    provider: String,
    configured: ApiVoyageConfiguration,
    plan: ApiVoyageTokenPlan,
    storage: Option<ApiSqliteStorageSummary>,
    spends_tokens: bool,
    message: String,
}

#[derive(Serialize)]
struct ApiVoyageStatusResponse {
    provider: String,
    configured: ApiVoyageConfiguration,
    file_index_embeddings: usize,
    storage: Option<ApiSqliteStorageSummary>,
    integrity: ApiIndexIntegrity,
    read_only: bool,
    spends_tokens: bool,
}

#[derive(Serialize)]
struct ApiIndexIntegrity {
    authoritative_backend: &'static str,
    schema_current: bool,
    sqlite_generation: Option<i64>,
    file_generation: Option<i64>,
    file_mirror_consistent: bool,
    recovery_required: bool,
    violations: Vec<String>,
}

#[derive(Serialize)]
struct ApiVoyageRebuildResponse {
    documents: usize,
    chunks: usize,
    retained_embeddings: usize,
    missing_embeddings: usize,
    integrity: ApiIndexIntegrity,
    spends_tokens: bool,
}

#[derive(Deserialize)]
struct GenerateSuggestionsRequest {
    threshold: Option<f32>,
}

#[derive(Deserialize)]
struct SuggestionListQuery {
    review_set: Option<String>,
}

#[derive(Serialize)]
struct ApiSuggestionsResponse {
    review_set: ReviewSetRecord,
    suggestions: Vec<ApiSuggestion>,
}

#[derive(Serialize)]
struct ApiSuggestion {
    id: String,
    review_set_id: String,
    provider: String,
    model: String,
    generation_method: String,
    ai_generated: bool,
    source_chunk: String,
    target_chunk: String,
    source_document: String,
    target_document: String,
    score: f32,
    created_at: String,
    status: String,
}

#[derive(Serialize)]
struct ApiReviewMutationResponse {
    review_set_id: String,
    changed: usize,
    status: String,
    durable: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportReviewArtifact {
    #[serde(rename = "type")]
    artifact_type: String,
    review_set_id: String,
    accepted_edges: Vec<ImportReviewEdge>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportReviewEdge {
    id: String,
    provider: String,
    model: String,
    generation_method: String,
    ai_generated: bool,
    source_chunk: String,
    target_chunk: String,
    score: f32,
    created_at: String,
}

#[derive(Serialize)]
struct ApiVoyageConfiguration {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    index_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sqlite_path: Option<String>,
    batch_size: usize,
    timeout_seconds: u64,
    has_api_key: bool,
    api_key_status: String,
}

#[derive(Serialize)]
struct ApiVoyageTokenPlan {
    documents: usize,
    document_inventory_count: usize,
    chunks: usize,
    estimated_tokens: usize,
    estimated_requests: usize,
    cached_chunks: usize,
    changed_chunks: usize,
    tpm_limit: usize,
    rpm_limit: usize,
    within_limits: bool,
    model: String,
}

#[derive(Serialize)]
struct ApiConnectivityReport {
    success: bool,
    http_status: Option<u16>,
    api_error_code: Option<String>,
    message: String,
    tokens_used: Option<usize>,
}

#[derive(Serialize)]
struct ApiVoyageIndexResponse {
    report: ApiConnectivityReport,
    index: Option<ApiVoyageIndexSummary>,
}

#[derive(Serialize)]
struct ApiVoyageIndexSummary {
    chunks: usize,
    changed_chunks_before: usize,
    cached_chunks_before: usize,
    cached_chunks_after: usize,
    estimated_tokens_before: usize,
    estimated_requests_before: usize,
}

#[derive(Serialize)]
struct ApiVoyageSearchResponse {
    report: ApiConnectivityReport,
    results: Vec<ApiVoyageSearchResult>,
}

#[derive(Serialize)]
struct ApiVoyageSearchResult {
    chunk_id: String,
    document_path: String,
    score: f32,
    provider: String,
    model: String,
}

#[derive(Serialize)]
struct ApiApplyEdgesResponse {
    dry_run: bool,
    applied: usize,
    skipped: usize,
    files: Vec<ApiAppliedFile>,
    message: String,
}

#[derive(Serialize)]
struct ApiAppliedFile {
    logical_path: String,
    source_path: Option<String>,
    applied: usize,
    audited: usize,
    recovered: usize,
    skipped: usize,
    changed: bool,
    error: Option<String>,
}

#[derive(Serialize)]
struct ApiSqliteStorageSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    schema_version: i64,
    documents: i64,
    chunks: i64,
    embeddings: i64,
}

#[derive(Clone, Debug)]
struct AcceptedSuggestion {
    id: String,
    provider: String,
    model: String,
    generation_method: String,
    ai_generated: bool,
    source_chunk: String,
    target_chunk: String,
    score: f32,
    created_at: String,
    source_document: String,
    target_document: String,
}

#[derive(Clone, Debug, Serialize)]
struct ReviewSetRecord {
    id: String,
    provider: String,
    model: String,
    generation_method: String,
    threshold: f32,
    index_generation: i64,
    created_at: String,
    status: String,
}

#[derive(Serialize)]
struct ApiRoot {
    id: String,
    mount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_path: Option<String>,
}

#[derive(Serialize)]
struct ApiRootConfiguration {
    effective: Vec<ApiRootConfigEntry>,
    persistent: Vec<ApiRootConfigEntry>,
    environment_override_active: bool,
    dotenv_override_active: bool,
    operator_override_active: bool,
    restart_required: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootProposalCreateRequest {
    path: String,
    mount: Option<String>,
    operation: String,
    enabled: Option<bool>,
    priority: Option<i64>,
    check_for_changes: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootRegistrationRequest {
    proposal_digest: String,
    expected_revision: String,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RootInitializationOptionsRequest {
    proposal_digest: String,
    #[serde(default)]
    resource_types: Map<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootInitializationRequest {
    proposal_digest: String,
    plan_digest: String,
    #[serde(default)]
    resource_types: Map<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootChangeReviewRequest {
    snapshot_digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootUpdateRequest {
    expected_revision: String,
    mount: Option<String>,
    #[serde(default)]
    clear_mount: bool,
    enabled: Option<bool>,
    priority: Option<i64>,
    check_for_changes: Option<bool>,
}

#[derive(Deserialize)]
struct RootRemovalRequest {
    expected_revision: String,
}

#[derive(Serialize)]
struct ApiManagedRootConfiguration {
    revision: String,
    roots: Vec<ApiRootConfigEntry>,
    process_override_active: bool,
    dotenv_override_active: bool,
    effective_differs_from_browser_configuration: bool,
    restart_required: bool,
}

#[derive(Serialize)]
struct ApiRootProposal {
    id: String,
    operation: &'static str,
    canonical_root: String,
    mount: Option<String>,
    root_id: Option<String>,
    proposed_root_id: Option<String>,
    snapshot_digest: String,
    proposal_digest: String,
    confirmable: bool,
    expires_in_seconds: u64,
    registration: Option<ApiRegistrationChange>,
    summary: ApiRootProposalSummary,
    tree: Vec<ApiProposalTreeEntry>,
    conflicts: Vec<ApiProposalConflict>,
}

#[derive(Serialize)]
struct ApiRegistrationChange {
    root_id: String,
    mount: Option<String>,
    canonical_path: String,
    enabled: bool,
    priority: i64,
    check_for_changes: bool,
}

#[derive(Serialize)]
struct ApiRootProposalSummary {
    accepted_files: usize,
    rejected_entries: usize,
    inspected_entries: u64,
    total_candidate_bytes: u64,
    writable: bool,
    process_override_active: bool,
    dotenv_override_active: bool,
    cli_override_active: bool,
    max_file_bytes: u64,
    max_entries: u64,
    max_depth: u64,
    max_total_bytes: u64,
}

#[derive(Serialize)]
struct ApiProposalTreeEntry {
    path: String,
    state: &'static str,
    detail: Option<String>,
}

#[derive(Serialize)]
struct ApiProposalConflict {
    code: &'static str,
    path: Option<String>,
    detail: String,
    blocking: bool,
}

#[derive(Serialize)]
struct ApiInitializationPlan {
    proposal_digest: String,
    plan_digest: String,
    canonical_root: String,
    git: Option<ApiGitStatus>,
    changes: Vec<ApiSourceFileChange>,
}

#[derive(Serialize)]
struct ApiGitStatus {
    worktree: String,
    dirty: bool,
    entries: Vec<String>,
}

#[derive(Serialize)]
struct ApiSourceFileChange {
    path: String,
    before: Option<String>,
    after: String,
    diff: String,
}

#[derive(Serialize)]
struct ApiRootMutation {
    changed: bool,
    revision: String,
    restart_required: bool,
}

#[derive(Serialize)]
struct ApiInitializationResult {
    changed_files: Vec<String>,
    recovered_interrupted_operation: bool,
    git_dirty_before: bool,
    git_dirty_after: bool,
    final_diffs: Vec<String>,
}

#[derive(Serialize)]
struct ApiRootConfigEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    root_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spec: Option<String>,
    mount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    usable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_for_changes: Option<bool>,
}

#[derive(Serialize)]
struct ApiDiagnostic {
    diagnostic_type: String,
    logical_path: Option<String>,
    root_index: Option<usize>,
    selected_root_index: Option<usize>,
    shadowed_root_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shadowed_root: Option<String>,
}

#[derive(Serialize)]
struct ApiDocumentSummary {
    title: String,
    #[serde(rename = "type")]
    document_type: Option<String>,
    kind: Option<String>,
    topic: Option<String>,
    status: Option<String>,
    updated: Option<String>,
    logical_path: String,
    okf_uri: Option<String>,
    source_relative_path: String,
    directory_path: String,
    navigation_class: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_path: Option<String>,
    browser_path: String,
    root_index: usize,
    root_mount: Option<String>,
    is_plan: bool,
}

#[derive(Serialize)]
struct ApiPlanningSections {
    completed: Vec<String>,
    open: Vec<String>,
    deferred: Vec<String>,
}

#[derive(Serialize)]
struct ApiGraphNode {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    title: Option<String>,
    topic: Option<String>,
    kind: Option<String>,
    #[serde(rename = "document_type")]
    document_type: Option<String>,
    path: Option<String>,
}

#[derive(Serialize)]
struct ApiGraphEdge {
    id: String,
    #[serde(rename = "type")]
    edge_type: String,
    source: String,
    target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ApiRelationProvenance>,
}

#[derive(Serialize)]
struct ApiRelationProvenance {
    relation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_chunk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_chunk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_method: Option<String>,
    ai_generated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

fn api_error(status: StatusCode, error: impl Into<String>) -> Response {
    let internal = error.into();
    let message = if status.is_server_error() {
        "The OKF HTTP server could not complete the request.".to_string()
    } else {
        internal.clone()
    };
    let mut response = (
        status,
        Json(ApiErrorEnvelope {
            api_version: API_VERSION,
            error: ApiError {
                code: api::error_code(status),
                message,
            },
        }),
    )
        .into_response();
    response
        .extensions_mut()
        .insert(InternalErrorDetail(internal));
    response
}

fn api_error_named(status: StatusCode, code: &'static str, error: impl Into<String>) -> Response {
    let internal = error.into();
    let message = if status.is_server_error() {
        "The OKF HTTP server could not complete the request.".to_string()
    } else {
        internal.clone()
    };
    let mut response = (
        status,
        Json(ApiErrorEnvelope {
            api_version: API_VERSION,
            error: ApiError { code, message },
        }),
    )
        .into_response();
    response
        .extensions_mut()
        .insert(InternalErrorDetail(internal));
    response
}

#[cfg(any())]
fn api_error_code_legacy(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "bad_request",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::CONFLICT => "conflict",
        StatusCode::PRECONDITION_REQUIRED => "precondition_required",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        StatusCode::BAD_GATEWAY => "provider_error",
        StatusCode::SERVICE_UNAVAILABLE => "service_unavailable",
        _ if status.is_server_error() => "internal_error",
        _ => "request_failed",
    }
}

fn api_response<T: Serialize>(data: T) -> Response {
    Json(ApiEnvelope {
        api_version: API_VERSION,
        data,
    })
    .into_response()
}

fn api_response_with_status<T: Serialize>(status: StatusCode, data: T) -> Response {
    (
        status,
        Json(ApiEnvelope {
            api_version: API_VERSION,
            data,
        }),
    )
        .into_response()
}

fn voyage_report_response(
    report: ConnectivityReport,
    index: Option<ApiVoyageIndexSummary>,
) -> Response {
    api_response_with_status(
        voyage_report_status(&report),
        ApiVoyageIndexResponse {
            report: api_connectivity_report(&report),
            index,
        },
    )
}

fn voyage_report_status(report: &ConnectivityReport) -> StatusCode {
    if report.success {
        return StatusCode::OK;
    }
    match report.api_error_code.as_deref() {
        Some("missing_api_key" | "empty_input") => StatusCode::BAD_REQUEST,
        Some("timeout") => StatusCode::GATEWAY_TIMEOUT,
        Some("process_launch_error") => StatusCode::SERVICE_UNAVAILABLE,
        _ => match report.http_status {
            Some(400) => StatusCode::BAD_REQUEST,
            Some(401) => StatusCode::UNAUTHORIZED,
            Some(403) => StatusCode::FORBIDDEN,
            Some(404) => StatusCode::NOT_FOUND,
            Some(408) => StatusCode::REQUEST_TIMEOUT,
            Some(429) => StatusCode::TOO_MANY_REQUESTS,
            Some(500..=599) => StatusCode::BAD_GATEWAY,
            _ => StatusCode::BAD_GATEWAY,
        },
    }
}

fn api_connectivity_report(report: &ConnectivityReport) -> ApiConnectivityReport {
    ApiConnectivityReport {
        success: report.success,
        http_status: report.http_status,
        api_error_code: report.api_error_code.clone(),
        message: report.message.clone(),
        tokens_used: report.tokens_used,
    }
}

fn api_search_result(result: &SearchResult) -> ApiVoyageSearchResult {
    ApiVoyageSearchResult {
        chunk_id: result.chunk_id.clone(),
        document_path: logical_path_string(&result.document_path),
        score: result.score,
        provider: result.provider.clone(),
        model: result.model.clone(),
    }
}

#[derive(Clone, Debug)]
struct SqliteWorkingIndex {
    path: PathBuf,
}

const CURRENT_SQLITE_SCHEMA_VERSION: i64 = 4;
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

impl SqliteWorkingIndex {
    fn open(index_root: &Path) -> rusqlite::Result<Self> {
        std::fs::create_dir_all(index_root)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let index = Self {
            path: index_root.join("okf.sqlite"),
        };
        index.migrate()?;
        Ok(index)
    }

    fn open_existing(index_root: &Path) -> rusqlite::Result<Self> {
        let path = index_root.join("okf.sqlite");
        if !path.is_file() {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        let index = Self { path };
        let connection = index.connection()?;
        let version = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get::<_, Option<i64>>(0)
            })?
            .unwrap_or(0);
        if version != CURRENT_SQLITE_SCHEMA_VERSION {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(index)
    }

    #[cfg(test)]
    fn path(&self) -> &Path {
        &self.path
    }

    fn connection(&self) -> rusqlite::Result<Connection> {
        let connection = Connection::open(&self.path)?;
        configure_sqlite_connection(&connection)?;
        Ok(connection)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        let mut connection = self.connection()?;
        let journal_mode: String =
            connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            connection.execute_batch("PRAGMA journal_mode = WAL;")?;
        }
        connection.execute_batch("PRAGMA synchronous = FULL;")?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            ",
        )?;
        let current = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get::<_, Option<i64>>(0)
            })?
            .unwrap_or(0);
        if current > CURRENT_SQLITE_SCHEMA_VERSION {
            return Err(rusqlite::Error::InvalidQuery);
        }
        for version in (current + 1)..=CURRENT_SQLITE_SCHEMA_VERSION {
            let transaction = connection.transaction()?;
            match version {
                1 => transaction.execute_batch(
                    "
            CREATE TABLE IF NOT EXISTS documents (
                logical_path TEXT PRIMARY KEY,
                physical_path TEXT NOT NULL,
                title TEXT NOT NULL,
                document_type TEXT,
                kind TEXT,
                topic TEXT,
                status TEXT,
                content_hash TEXT NOT NULL,
                estimated_tokens INTEGER NOT NULL,
                bytes INTEGER NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS chunks (
                id TEXT PRIMARY KEY,
                document_path TEXT NOT NULL,
                title TEXT NOT NULL,
                document_type TEXT,
                kind TEXT,
                topic TEXT,
                status TEXT,
                heading_path_json TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                estimated_tokens INTEGER NOT NULL,
                content TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS embeddings (
                chunk_id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                dimensions INTEGER NOT NULL,
                vector_json TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS suggested_edges (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                generation_method TEXT NOT NULL,
                ai_generated INTEGER NOT NULL,
                source_chunk TEXT NOT NULL,
                target_chunk TEXT NOT NULL,
                score REAL NOT NULL,
                created_at TEXT NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
                    ",
                )?,
                2 => transaction.execute_batch(
                    "
            CREATE TABLE IF NOT EXISTS applied_relations (
                suggestion_id TEXT PRIMARY KEY,
                source_document TEXT NOT NULL,
                target_document TEXT NOT NULL,
                source_path TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                generation_method TEXT NOT NULL,
                ai_generated INTEGER NOT NULL,
                score REAL NOT NULL,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
                    ",
                )?,
                3 => transaction.execute_batch(
                    "
            ALTER TABLE chunks ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]';
            CREATE TABLE index_state (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                generation INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            INSERT INTO index_state (singleton, generation) VALUES (1, 0);

            DELETE FROM embeddings
             WHERE chunk_id NOT IN (SELECT id FROM chunks);
            DELETE FROM suggested_edges
             WHERE source_chunk NOT IN (SELECT id FROM chunks)
                OR target_chunk NOT IN (SELECT id FROM chunks);
            DELETE FROM applied_relations
             WHERE suggestion_id NOT IN (SELECT id FROM suggested_edges);

            CREATE TRIGGER embeddings_require_chunk_insert
            BEFORE INSERT ON embeddings
            WHEN NOT EXISTS (
                SELECT 1 FROM chunks
                 WHERE id = NEW.chunk_id AND content_hash = NEW.content_hash
            )
            BEGIN
                SELECT RAISE(ABORT, 'embedding requires matching chunk and content hash');
            END;
            CREATE TRIGGER embeddings_require_chunk_update
            BEFORE UPDATE ON embeddings
            WHEN NOT EXISTS (
                SELECT 1 FROM chunks
                 WHERE id = NEW.chunk_id AND content_hash = NEW.content_hash
            )
            BEGIN
                SELECT RAISE(ABORT, 'embedding requires matching chunk and content hash');
            END;
            CREATE TRIGGER suggestions_require_chunks_insert
            BEFORE INSERT ON suggested_edges
            WHEN NOT EXISTS (SELECT 1 FROM chunks WHERE id = NEW.source_chunk)
              OR NOT EXISTS (SELECT 1 FROM chunks WHERE id = NEW.target_chunk)
            BEGIN
                SELECT RAISE(ABORT, 'suggestion requires source and target chunks');
            END;
            CREATE TRIGGER suggestions_require_chunks_update
            BEFORE UPDATE ON suggested_edges
            WHEN NOT EXISTS (SELECT 1 FROM chunks WHERE id = NEW.source_chunk)
              OR NOT EXISTS (SELECT 1 FROM chunks WHERE id = NEW.target_chunk)
            BEGIN
                SELECT RAISE(ABORT, 'suggestion requires source and target chunks');
            END;
            CREATE TRIGGER applied_relations_require_suggestion_insert
            BEFORE INSERT ON applied_relations
            WHEN NOT EXISTS (SELECT 1 FROM suggested_edges WHERE id = NEW.suggestion_id)
            BEGIN
                SELECT RAISE(ABORT, 'applied relation requires suggestion');
            END;
            CREATE TRIGGER applied_relations_require_suggestion_update
            BEFORE UPDATE ON applied_relations
            WHEN NOT EXISTS (SELECT 1 FROM suggested_edges WHERE id = NEW.suggestion_id)
            BEGIN
                SELECT RAISE(ABORT, 'applied relation requires suggestion');
            END;
            CREATE TRIGGER chunks_remove_derived_rows
            AFTER DELETE ON chunks
            BEGIN
                DELETE FROM embeddings WHERE chunk_id = OLD.id;
                DELETE FROM suggested_edges
                 WHERE source_chunk = OLD.id OR target_chunk = OLD.id;
            END;
            CREATE TRIGGER suggestions_remove_applied_rows
            AFTER DELETE ON suggested_edges
            BEGIN
                DELETE FROM applied_relations WHERE suggestion_id = OLD.id;
            END;
                    ",
                )?,
                4 => transaction.execute_batch(
                    "
            DELETE FROM suggested_edges WHERE model = 'pending-voyage-ai';
            CREATE TABLE review_sets (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                generation_method TEXT NOT NULL,
                threshold REAL NOT NULL,
                index_generation INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'open',
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            INSERT INTO review_sets (
                id, provider, model, generation_method, threshold,
                index_generation, created_at, status
            ) VALUES (
                'legacy', 'unknown', 'unknown', 'legacy_import', 0.0,
                0, CURRENT_TIMESTAMP, 'legacy'
            );
            ALTER TABLE suggested_edges
                ADD COLUMN review_set_id TEXT NOT NULL DEFAULT 'legacy';
            CREATE INDEX suggested_edges_review_set_status
                ON suggested_edges (review_set_id, status, score DESC, id);
            CREATE TRIGGER suggestions_require_review_set_insert
            BEFORE INSERT ON suggested_edges
            WHEN NOT EXISTS (SELECT 1 FROM review_sets WHERE id = NEW.review_set_id)
            BEGIN
                SELECT RAISE(ABORT, 'suggestion requires review set');
            END;
            CREATE TRIGGER suggestions_require_review_set_update
            BEFORE UPDATE ON suggested_edges
            WHEN NOT EXISTS (SELECT 1 FROM review_sets WHERE id = NEW.review_set_id)
            BEGIN
                SELECT RAISE(ABORT, 'suggestion requires review set');
            END;
                    ",
                )?,
                _ => unreachable!(),
            }
            transaction.execute(
                "INSERT INTO schema_migrations (version) VALUES (?1)",
                [version],
            )?;
            transaction.commit()?;
        }
        Ok(())
    }

    fn sync_repository_index(
        &self,
        documents: &[InventoryDocument],
        chunks: &[Chunk],
        index: &LocalIndex,
    ) -> rusqlite::Result<i64> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let live_chunk_ids = chunks
            .iter()
            .map(|chunk| chunk.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut invalidated_suggestion_chunks = BTreeSet::new();
        transaction.execute_batch(
            "
            CREATE TEMP TABLE IF NOT EXISTS okf_live_documents (
                logical_path TEXT PRIMARY KEY
            ) WITHOUT ROWID;
            CREATE TEMP TABLE IF NOT EXISTS okf_live_chunks (
                id TEXT PRIMARY KEY
            ) WITHOUT ROWID;
            CREATE TEMP TABLE IF NOT EXISTS okf_live_suggestions (
                id TEXT PRIMARY KEY
            ) WITHOUT ROWID;
            DELETE FROM okf_live_documents;
            DELETE FROM okf_live_chunks;
            DELETE FROM okf_live_suggestions;
            ",
        )?;

        for document in documents {
            let logical_path = logical_path_string(&document.logical_path);
            transaction.execute(
                "INSERT INTO okf_live_documents (logical_path) VALUES (?1)",
                [&logical_path],
            )?;
            transaction.execute(
                "INSERT INTO documents (
                    logical_path, physical_path, title, document_type, kind, topic, status,
                    content_hash, estimated_tokens, bytes, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, CURRENT_TIMESTAMP)
                ON CONFLICT(logical_path) DO UPDATE SET
                    physical_path = excluded.physical_path,
                    title = excluded.title,
                    document_type = excluded.document_type,
                    kind = excluded.kind,
                    topic = excluded.topic,
                    status = excluded.status,
                    content_hash = excluded.content_hash,
                    estimated_tokens = excluded.estimated_tokens,
                    bytes = excluded.bytes,
                    updated_at = CURRENT_TIMESTAMP",
                params![
                    logical_path,
                    document.physical_path.display().to_string(),
                    document.title,
                    document.document_type,
                    document.kind,
                    document.topic,
                    document.status,
                    document.content_hash,
                    document.estimated_tokens as i64,
                    document.bytes as i64,
                ],
            )?;
        }

        for chunk in chunks {
            let previous_hash = transaction
                .query_row(
                    "SELECT content_hash FROM chunks WHERE id = ?1",
                    [&chunk.id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if previous_hash
                .as_deref()
                .is_some_and(|hash| hash != chunk.content_hash)
            {
                invalidated_suggestion_chunks.insert(chunk.id.as_str());
                transaction.execute(
                    "DELETE FROM suggested_edges
                      WHERE source_chunk = ?1 OR target_chunk = ?1",
                    [&chunk.id],
                )?;
            }
            transaction.execute("INSERT INTO okf_live_chunks (id) VALUES (?1)", [&chunk.id])?;
            transaction.execute(
                "INSERT INTO chunks (
                    id, document_path, title, document_type, kind, topic, status,
                    heading_path_json, content_hash, estimated_tokens, content, tags_json,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                          CURRENT_TIMESTAMP)
                ON CONFLICT(id) DO UPDATE SET
                    document_path = excluded.document_path,
                    title = excluded.title,
                    document_type = excluded.document_type,
                    kind = excluded.kind,
                    topic = excluded.topic,
                    status = excluded.status,
                    heading_path_json = excluded.heading_path_json,
                    content_hash = excluded.content_hash,
                    estimated_tokens = excluded.estimated_tokens,
                    content = excluded.content,
                    tags_json = excluded.tags_json,
                    updated_at = CURRENT_TIMESTAMP",
                params![
                    chunk.id,
                    logical_path_string(&chunk.document_path),
                    chunk.title,
                    chunk.document_type,
                    chunk.kind,
                    chunk.topic,
                    chunk.status,
                    serde_json::to_string(&chunk.heading_path).unwrap_or_else(|_| "[]".to_string()),
                    chunk.content_hash,
                    chunk.estimated_tokens as i64,
                    chunk.content,
                    serde_json::to_string(&chunk.tags).unwrap_or_else(|_| "[]".to_string()),
                ],
            )?;
        }

        transaction.execute(
            "DELETE FROM chunks WHERE id NOT IN (SELECT id FROM okf_live_chunks)",
            [],
        )?;
        transaction.execute(
            "DELETE FROM documents
              WHERE logical_path NOT IN (SELECT logical_path FROM okf_live_documents)",
            [],
        )?;
        transaction.execute("DELETE FROM embeddings", [])?;
        for embedding in &index.embeddings {
            transaction.execute(
                "INSERT INTO embeddings (
                    chunk_id, provider, model, dimensions, vector_json, content_hash,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP)",
                params![
                    embedding.chunk.id,
                    embedding.provider,
                    embedding.model,
                    embedding.embedding.len() as i64,
                    vector_to_json(&embedding.embedding),
                    embedding.chunk.content_hash,
                ],
            )?;
        }

        for suggestion in &index.suggestions {
            if suggestion.model == "pending-voyage-ai"
                || !live_chunk_ids.contains(suggestion.source_chunk.as_str())
                || !live_chunk_ids.contains(suggestion.target_chunk.as_str())
                || invalidated_suggestion_chunks.contains(suggestion.source_chunk.as_str())
                || invalidated_suggestion_chunks.contains(suggestion.target_chunk.as_str())
            {
                continue;
            }
            transaction.execute(
                "INSERT OR IGNORE INTO review_sets (
                    id, provider, model, generation_method, threshold,
                    index_generation, created_at, status
                ) VALUES (?1, ?2, ?3, ?4, 0.0,
                          (SELECT generation FROM index_state WHERE singleton = 1),
                          ?5, 'recovered')",
                params![
                    suggestion.review_set_id,
                    suggestion.provider,
                    suggestion.model,
                    suggestion.generation_method,
                    suggestion.created_at,
                ],
            )?;
            transaction.execute(
                "INSERT INTO okf_live_suggestions (id) VALUES (?1)",
                [&suggestion.id],
            )?;
            transaction.execute(
                "INSERT INTO suggested_edges (
                    id, review_set_id, provider, model, generation_method, ai_generated,
                    source_chunk, target_chunk, score, created_at, status, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                          CURRENT_TIMESTAMP)
                ON CONFLICT(id) DO UPDATE SET
                    review_set_id = excluded.review_set_id,
                    provider = excluded.provider,
                    model = excluded.model,
                    generation_method = excluded.generation_method,
                    ai_generated = excluded.ai_generated,
                    source_chunk = excluded.source_chunk,
                    target_chunk = excluded.target_chunk,
                    score = excluded.score,
                    created_at = excluded.created_at,
                    status = excluded.status,
                    updated_at = CURRENT_TIMESTAMP",
                params![
                    suggestion.id,
                    suggestion.review_set_id,
                    suggestion.provider,
                    suggestion.model,
                    suggestion.generation_method,
                    if suggestion.ai_generated { 1 } else { 0 },
                    suggestion.source_chunk,
                    suggestion.target_chunk,
                    suggestion.score as f64,
                    suggestion.created_at,
                    suggestion.status.as_str(),
                ],
            )?;
        }
        transaction.execute(
            "DELETE FROM suggested_edges
              WHERE id NOT IN (SELECT id FROM okf_live_suggestions)",
            [],
        )?;

        let violations = sqlite_integrity_violations(&transaction)?;
        if !violations.is_empty() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        transaction.execute(
            "UPDATE index_state
                SET generation = generation + 1,
                    updated_at = CURRENT_TIMESTAMP
              WHERE singleton = 1",
            [],
        )?;
        let generation = transaction.query_row(
            "SELECT generation FROM index_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(generation)
    }

    fn load_local_index(&self) -> rusqlite::Result<LocalIndex> {
        load_local_index_from_connection(&self.connection()?)
    }

    #[cfg(test)]
    fn generation(&self) -> rusqlite::Result<i64> {
        self.connection()?.query_row(
            "SELECT generation FROM index_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
    }

    #[cfg(test)]
    fn integrity_violations(&self) -> rusqlite::Result<Vec<String>> {
        sqlite_integrity_violations(&self.connection()?)
    }

    #[cfg(test)]
    fn sync_inventory(
        &self,
        documents: &[InventoryDocument],
        chunks: &[Chunk],
    ) -> rusqlite::Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM documents", [])?;
        transaction.execute("DELETE FROM chunks", [])?;
        for document in documents {
            transaction.execute(
                "INSERT INTO documents (
                    logical_path, physical_path, title, document_type, kind, topic, status,
                    content_hash, estimated_tokens, bytes
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    logical_path_string(&document.logical_path),
                    document.physical_path.display().to_string(),
                    document.title,
                    document.document_type,
                    document.kind,
                    document.topic,
                    document.status,
                    document.content_hash,
                    document.estimated_tokens as i64,
                    document.bytes as i64,
                ],
            )?;
        }
        for chunk in chunks {
            transaction.execute(
                "INSERT INTO chunks (
                    id, document_path, title, document_type, kind, topic, status,
                    heading_path_json, content_hash, estimated_tokens, content
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    chunk.id,
                    logical_path_string(&chunk.document_path),
                    chunk.title,
                    chunk.document_type,
                    chunk.kind,
                    chunk.topic,
                    chunk.status,
                    serde_json::to_string(&chunk.heading_path).unwrap_or_else(|_| "[]".to_string()),
                    chunk.content_hash,
                    chunk.estimated_tokens as i64,
                    chunk.content,
                ],
            )?;
        }
        transaction.commit()
    }

    #[cfg(test)]
    fn sync_embeddings(&self, embeddings: &[EmbeddedChunk]) -> rusqlite::Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM embeddings", [])?;
        for embedding in embeddings {
            transaction.execute(
                "INSERT INTO embeddings (
                    chunk_id, provider, model, dimensions, vector_json, content_hash
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    embedding.chunk.id,
                    embedding.provider,
                    embedding.model,
                    embedding.embedding.len() as i64,
                    vector_to_json(&embedding.embedding),
                    embedding.chunk.content_hash,
                ],
            )?;
        }
        transaction.commit()
    }

    fn embedding_count(&self) -> rusqlite::Result<usize> {
        let connection = self.connection()?;
        let count = connection.query_row("SELECT COUNT(*) FROM embeddings", [], |row| {
            row.get::<_, i64>(0)
        })?;
        Ok(count as usize)
    }

    fn create_similarity_review_set(
        &self,
        threshold: f32,
    ) -> rusqlite::Result<(ReviewSetRecord, Vec<SuggestedEdge>, i64)> {
        let index = self.load_local_index()?;
        let Some(first) = index.embeddings.first() else {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        };
        if index
            .embeddings
            .iter()
            .any(|embedding| embedding.provider != first.provider || embedding.model != first.model)
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let created_at = unix_timestamp_string();
        let review_set = ReviewSetRecord {
            id: format!(
                "review-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            provider: first.provider.clone(),
            model: first.model.clone(),
            generation_method: "embedding_similarity".to_string(),
            threshold,
            index_generation: self.generation_value()?,
            created_at,
            status: "open".to_string(),
        };
        let mut suggestions = suggest_edges(&index, threshold);
        for suggestion in &mut suggestions {
            suggestion.id = format!("{}:{}", review_set.id, suggestion.id);
            suggestion.review_set_id = review_set.id.clone();
            suggestion.provider = review_set.provider.clone();
            suggestion.model = review_set.model.clone();
            suggestion.generation_method = review_set.generation_method.clone();
        }

        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO review_sets (
                id, provider, model, generation_method, threshold,
                index_generation, created_at, status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                review_set.id,
                review_set.provider,
                review_set.model,
                review_set.generation_method,
                review_set.threshold as f64,
                review_set.index_generation,
                review_set.created_at,
                review_set.status,
            ],
        )?;
        for suggestion in &suggestions {
            transaction.execute(
                "INSERT INTO suggested_edges (
                    id, review_set_id, provider, model, generation_method, ai_generated,
                    source_chunk, target_chunk, score, created_at, status
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    suggestion.id,
                    suggestion.review_set_id,
                    suggestion.provider,
                    suggestion.model,
                    suggestion.generation_method,
                    if suggestion.ai_generated { 1 } else { 0 },
                    suggestion.source_chunk,
                    suggestion.target_chunk,
                    suggestion.score as f64,
                    suggestion.created_at,
                    suggestion.status.as_str(),
                ],
            )?;
        }
        let generation = increment_index_generation(&transaction)?;
        transaction.commit()?;
        Ok((review_set, suggestions, generation))
    }

    fn review_set(&self, id: Option<&str>) -> rusqlite::Result<ReviewSetRecord> {
        let connection = self.connection()?;
        if let Some(id) = id {
            connection.query_row(
                "SELECT id, provider, model, generation_method, threshold,
                        index_generation, created_at, status
                   FROM review_sets WHERE id = ?1",
                [id],
                review_set_from_row,
            )
        } else {
            connection.query_row(
                "SELECT id, provider, model, generation_method, threshold,
                        index_generation, created_at, status
                   FROM review_sets WHERE id <> 'legacy'
                  ORDER BY rowid DESC LIMIT 1",
                [],
                review_set_from_row,
            )
        }
    }

    fn suggestions_for_review_set(
        &self,
        review_set_id: &str,
    ) -> rusqlite::Result<Vec<SuggestedEdge>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, review_set_id, provider, model, generation_method, ai_generated,
                    source_chunk, target_chunk, score, created_at, status
               FROM suggested_edges
              WHERE review_set_id = ?1
              ORDER BY score DESC, id",
        )?;
        let rows = statement.query_map([review_set_id], suggested_edge_from_row)?;
        rows.collect()
    }

    fn set_suggestion_status(
        &self,
        suggestion_id: &str,
        status: SuggestedEdgeStatus,
    ) -> rusqlite::Result<(String, i64)> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let review_set_id = transaction.query_row(
            "SELECT review_set_id FROM suggested_edges WHERE id = ?1",
            [suggestion_id],
            |row| row.get::<_, String>(0),
        )?;
        transaction.execute(
            "UPDATE suggested_edges SET status = ?1, updated_at = CURRENT_TIMESTAMP
              WHERE id = ?2",
            params![status.as_str(), suggestion_id],
        )?;
        let generation = increment_index_generation(&transaction)?;
        transaction.commit()?;
        Ok((review_set_id, generation))
    }

    fn set_review_set_status(
        &self,
        review_set_id: &str,
        status: SuggestedEdgeStatus,
    ) -> rusqlite::Result<(usize, i64)> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM review_sets WHERE id = ?1 AND id <> 'legacy')",
            [review_set_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        let changed = transaction.execute(
            "UPDATE suggested_edges SET status = ?1, updated_at = CURRENT_TIMESTAMP
              WHERE review_set_id = ?2",
            params![status.as_str(), review_set_id],
        )?;
        transaction.execute(
            "UPDATE review_sets SET status = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![status.as_str(), review_set_id],
        )?;
        let generation = increment_index_generation(&transaction)?;
        transaction.commit()?;
        Ok((changed, generation))
    }

    fn accept_imported_suggestions(
        &self,
        review_set_id: &str,
        suggestion_ids: &[String],
    ) -> rusqlite::Result<(usize, i64)> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM review_sets WHERE id = ?1 AND id <> 'legacy')",
            [review_set_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        let mut changed = 0;
        for id in suggestion_ids {
            let belongs: bool = transaction.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM suggested_edges WHERE id = ?1 AND review_set_id = ?2
                )",
                params![id, review_set_id],
                |row| row.get(0),
            )?;
            if !belongs {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            changed += transaction.execute(
                "UPDATE suggested_edges
                    SET status = 'accepted', updated_at = CURRENT_TIMESTAMP
                  WHERE id = ?1 AND status <> 'accepted'",
                [id],
            )?;
        }
        let generation = increment_index_generation(&transaction)?;
        transaction.commit()?;
        Ok((changed, generation))
    }

    fn generation_value(&self) -> rusqlite::Result<i64> {
        self.connection()?.query_row(
            "SELECT generation FROM index_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
    }

    fn accepted_suggestions(&self) -> rusqlite::Result<Vec<AcceptedSuggestion>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                s.id, s.provider, s.model, s.generation_method, s.ai_generated,
                s.source_chunk, s.target_chunk, s.score, s.created_at,
                source.document_path, target.document_path
            FROM suggested_edges s
            JOIN chunks source ON source.id = s.source_chunk
            JOIN chunks target ON target.id = s.target_chunk
            LEFT JOIN applied_relations applied ON applied.suggestion_id = s.id
            WHERE s.status = 'accepted'
              AND s.ai_generated = 1
              AND applied.suggestion_id IS NULL
            ORDER BY source.document_path, s.score DESC, s.id
            ",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AcceptedSuggestion {
                id: row.get(0)?,
                provider: row.get(1)?,
                model: row.get(2)?,
                generation_method: row.get(3)?,
                ai_generated: row.get::<_, i64>(4)? != 0,
                source_chunk: row.get(5)?,
                target_chunk: row.get(6)?,
                score: row.get::<_, f64>(7)? as f32,
                created_at: row.get(8)?,
                source_document: row.get(9)?,
                target_document: row.get(10)?,
            })
        })?;
        rows.collect()
    }

    fn record_applied_relations(
        &self,
        suggestions: &[AcceptedSuggestion],
        source_path: &Path,
    ) -> rusqlite::Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        for suggestion in suggestions {
            transaction.execute(
                "INSERT OR IGNORE INTO applied_relations (
                    suggestion_id, source_document, target_document, source_path,
                    provider, model, generation_method, ai_generated, score
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    suggestion.id,
                    suggestion.source_document,
                    suggestion.target_document,
                    source_path.display().to_string(),
                    suggestion.provider,
                    suggestion.model,
                    suggestion.generation_method,
                    if suggestion.ai_generated { 1 } else { 0 },
                    suggestion.score as f64,
                ],
            )?;
        }
        transaction.commit()
    }

    #[cfg(test)]
    fn storage_summary(&self, expose_physical_paths: bool) -> ApiSqliteStorageSummary {
        let counts = self.storage_counts().unwrap_or_default();
        ApiSqliteStorageSummary {
            path: expose_physical_paths.then(|| self.path.display().to_string()),
            schema_version: counts.schema_version,
            documents: counts.documents,
            chunks: counts.chunks,
            embeddings: counts.embeddings,
        }
    }

    #[cfg(any())]
    fn read_only_summary_legacy(
        index_root: &Path,
        expose_physical_paths: bool,
    ) -> rusqlite::Result<Option<ApiSqliteStorageSummary>> {
        let path = index_root.join("okf.sqlite");
        if !path.is_file() {
            return Ok(None);
        }
        let connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let counts = SqliteCounts {
            schema_version: connection
                .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                    row.get::<_, Option<i64>>(0)
                })?
                .unwrap_or(0),
            documents: table_count(&connection, "documents")?,
            chunks: table_count(&connection, "chunks")?,
            embeddings: table_count(&connection, "embeddings")?,
        };
        Ok(Some(ApiSqliteStorageSummary {
            path: expose_physical_paths.then(|| path.display().to_string()),
            schema_version: counts.schema_version,
            documents: counts.documents,
            chunks: counts.chunks,
            embeddings: counts.embeddings,
        }))
    }

    #[cfg(test)]
    fn storage_counts(&self) -> rusqlite::Result<SqliteCounts> {
        let connection = self.connection()?;
        Ok(SqliteCounts {
            schema_version: connection
                .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                    row.get::<_, Option<i64>>(0)
                })?
                .unwrap_or(0),
            documents: table_count(&connection, "documents")?,
            chunks: table_count(&connection, "chunks")?,
            embeddings: table_count(&connection, "embeddings")?,
        })
    }
}

fn increment_index_generation(transaction: &rusqlite::Transaction<'_>) -> rusqlite::Result<i64> {
    transaction.execute(
        "UPDATE index_state
            SET generation = generation + 1,
                updated_at = CURRENT_TIMESTAMP
          WHERE singleton = 1",
        [],
    )?;
    transaction.query_row(
        "SELECT generation FROM index_state WHERE singleton = 1",
        [],
        |row| row.get(0),
    )
}

fn review_set_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewSetRecord> {
    Ok(ReviewSetRecord {
        id: row.get(0)?,
        provider: row.get(1)?,
        model: row.get(2)?,
        generation_method: row.get(3)?,
        threshold: row.get::<_, f64>(4)? as f32,
        index_generation: row.get(5)?,
        created_at: row.get(6)?,
        status: row.get(7)?,
    })
}

fn suggested_edge_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SuggestedEdge> {
    let status: String = row.get(10)?;
    Ok(SuggestedEdge {
        id: row.get(0)?,
        review_set_id: row.get(1)?,
        provider: row.get(2)?,
        model: row.get(3)?,
        generation_method: row.get(4)?,
        ai_generated: row.get::<_, i64>(5)? != 0,
        source_chunk: row.get(6)?,
        target_chunk: row.get(7)?,
        score: row.get::<_, f64>(8)? as f32,
        created_at: row.get(9)?,
        status: match status.as_str() {
            "accepted" => SuggestedEdgeStatus::Accepted,
            "denied" => SuggestedEdgeStatus::Denied,
            _ => SuggestedEdgeStatus::Suggested,
        },
    })
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn configure_sqlite_connection(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
    connection.execute_batch("PRAGMA foreign_keys = ON;")
}

fn load_local_index_from_connection(connection: &Connection) -> rusqlite::Result<LocalIndex> {
    let chunk_columns = connection
        .prepare("PRAGMA table_info(chunks)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect::<BTreeSet<_>>();
    let has_tags = chunk_columns.contains("tags_json");
    let tags_expression = if has_tags { "c.tags_json" } else { "'[]'" };
    let query = format!(
        "SELECT c.id, c.document_path, c.title, c.document_type, c.kind, c.topic, c.status,
                c.heading_path_json, c.content_hash, c.estimated_tokens, c.content,
                {tags_expression}, e.provider, e.model, e.vector_json
           FROM embeddings e
           JOIN chunks c ON c.id = e.chunk_id
          ORDER BY c.id"
    );
    let mut statement = connection.prepare(&query)?;
    let embeddings = statement
        .query_map([], |row| {
            let heading_path_json: String = row.get(7)?;
            let tags_json: String = row.get(11)?;
            let vector_json: String = row.get(14)?;
            Ok(EmbeddedChunk {
                chunk: Chunk {
                    id: row.get(0)?,
                    document_path: PathBuf::from(row.get::<_, String>(1)?),
                    title: row.get(2)?,
                    document_type: row.get(3)?,
                    kind: row.get(4)?,
                    topic: row.get(5)?,
                    status: row.get(6)?,
                    heading_path: serde_json::from_str(&heading_path_json).unwrap_or_default(),
                    content_hash: row.get(8)?,
                    estimated_tokens: row.get::<_, i64>(9)?.max(0) as usize,
                    content: row.get(10)?,
                    tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                },
                provider: row.get(12)?,
                model: row.get(13)?,
                embedding: vector_from_json(&vector_json).unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let suggestion_columns = connection
        .prepare("PRAGMA table_info(suggested_edges)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .collect::<BTreeSet<_>>();
    let review_set_expression = if suggestion_columns.contains("review_set_id") {
        "review_set_id"
    } else {
        "'legacy'"
    };
    let mut statement = connection.prepare(&format!(
        "SELECT id, {review_set_expression}, provider, model, generation_method, ai_generated,
                source_chunk, target_chunk, score, created_at, status
           FROM suggested_edges
          ORDER BY id"
    ))?;
    let suggestions = statement
        .query_map([], |row| {
            let status: String = row.get(10)?;
            Ok(SuggestedEdge {
                id: row.get(0)?,
                review_set_id: row.get(1)?,
                provider: row.get(2)?,
                model: row.get(3)?,
                generation_method: row.get(4)?,
                ai_generated: row.get::<_, i64>(5)? != 0,
                source_chunk: row.get(6)?,
                target_chunk: row.get(7)?,
                score: row.get::<_, f64>(8)? as f32,
                created_at: row.get(9)?,
                status: match status.as_str() {
                    "accepted" => SuggestedEdgeStatus::Accepted,
                    "denied" => SuggestedEdgeStatus::Denied,
                    _ => SuggestedEdgeStatus::Suggested,
                },
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(LocalIndex {
        embeddings,
        suggestions,
    })
}

fn sqlite_integrity_violations(connection: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut violations = Vec::new();
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        violations.push(format!("sqlite:{integrity}"));
    }
    for (label, query) in [
        (
            "chunk_without_document",
            "SELECT COUNT(*) FROM chunks c LEFT JOIN documents d
              ON d.logical_path = c.document_path WHERE d.logical_path IS NULL",
        ),
        (
            "embedding_without_matching_chunk",
            "SELECT COUNT(*) FROM embeddings e LEFT JOIN chunks c
              ON c.id = e.chunk_id AND c.content_hash = e.content_hash WHERE c.id IS NULL",
        ),
        (
            "suggestion_without_chunks",
            "SELECT COUNT(*) FROM suggested_edges s
              LEFT JOIN chunks source ON source.id = s.source_chunk
              LEFT JOIN chunks target ON target.id = s.target_chunk
              WHERE source.id IS NULL OR target.id IS NULL",
        ),
        (
            "suggestion_without_review_set",
            "SELECT COUNT(*) FROM suggested_edges s LEFT JOIN review_sets r
              ON r.id = s.review_set_id WHERE r.id IS NULL",
        ),
        (
            "applied_relation_without_suggestion",
            "SELECT COUNT(*) FROM applied_relations a LEFT JOIN suggested_edges s
              ON s.id = a.suggestion_id WHERE s.id IS NULL",
        ),
    ] {
        let count: i64 = connection.query_row(query, [], |row| row.get(0))?;
        if count > 0 {
            violations.push(format!("{label}:{count}"));
        }
    }
    let mut statement =
        connection.prepare("SELECT chunk_id, dimensions, vector_json FROM embeddings")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (chunk_id, dimensions, vector_json) = row?;
        let vector = vector_from_json(&vector_json).unwrap_or_default();
        if dimensions <= 0
            || vector.len() != dimensions as usize
            || vector.iter().any(|value| !value.is_finite())
        {
            violations.push(format!("invalid_embedding:{chunk_id}"));
        }
    }
    Ok(violations)
}

impl VectorBackend for SqliteWorkingIndex {
    fn search(&self, query_embedding: &[f32], limit: usize) -> Vec<SearchResult> {
        let Ok(connection) = self.connection() else {
            return Vec::new();
        };
        let Ok(mut statement) = connection.prepare(
            "
            SELECT e.chunk_id, c.document_path, e.provider, e.model, e.vector_json
            FROM embeddings e
            JOIN chunks c ON c.id = e.chunk_id
            ",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        }) else {
            return Vec::new();
        };
        let mut results = rows
            .filter_map(Result::ok)
            .filter_map(|(chunk_id, document_path, provider, model, vector_json)| {
                let vector = vector_from_json(&vector_json)?;
                Some(SearchResult {
                    chunk_id,
                    document_path: PathBuf::from(document_path),
                    score: cosine_similarity(query_embedding, &vector),
                    provider,
                    model,
                })
            })
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }
}

#[cfg(test)]
#[derive(Default)]
struct SqliteCounts {
    schema_version: i64,
    documents: i64,
    chunks: i64,
    embeddings: i64,
}

#[cfg(test)]
fn table_count(connection: &Connection, table: &str) -> rusqlite::Result<i64> {
    connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
}

fn vector_to_json(vector: &[f32]) -> String {
    serde_json::to_string(vector).unwrap_or_else(|_| "[]".to_string())
}

fn vector_from_json(source: &str) -> Option<Vec<f32>> {
    serde_json::from_str(source).ok()
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut left_norm = 0.0f32;
    let mut right_norm = 0.0f32;
    for (left, right) in left.iter().zip(right.iter()) {
        dot += left * right;
        left_norm += left * left;
        right_norm += right * right;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

fn open_repository(state: &AppState) -> Result<okf::Repository, String> {
    okf::Repository::open(state.document_roots.clone()).map_err(|error| error.to_string())
}

fn voyage_config_from_environment() -> VoyageConfig {
    VoyageConfig::from_lookup(|key| {
        env::var(key)
            .ok()
            .or_else(|| dotenv_value(key).map(|value| value.to_string_lossy().to_string()))
    })
}

fn api_read_only_storage(
    index_root: &Path,
    expose_physical_paths: bool,
) -> rusqlite::Result<Option<ApiSqliteStorageSummary>> {
    storage::inspect_read_only(index_root).map(|summary| {
        summary.map(|summary| ApiSqliteStorageSummary {
            path: expose_physical_paths.then(|| summary.path.display().to_string()),
            schema_version: summary.schema_version,
            documents: summary.documents,
            chunks: summary.chunks,
            embeddings: summary.embeddings,
        })
    })
}

fn api_voyage_configuration(
    config: &VoyageConfig,
    sqlite_path: Option<String>,
    expose_physical_paths: bool,
) -> ApiVoyageConfiguration {
    ApiVoyageConfiguration {
        model: config.model().to_string(),
        index_root: expose_physical_paths.then(|| config.index_root().display().to_string()),
        sqlite_path: expose_physical_paths.then_some(sqlite_path).flatten(),
        batch_size: config.batch_size(),
        timeout_seconds: config.timeout().as_secs(),
        has_api_key: config.has_api_key(),
        api_key_status: voyage::api_key_status(config).to_string(),
    }
}

fn api_root(root: &DocumentRoot, index: usize, expose_physical_paths: bool) -> ApiRoot {
    ApiRoot {
        id: format!("root-{index}"),
        mount: root.mount().map(logical_path_string),
        source_path: expose_physical_paths.then(|| root.path().display().to_string()),
    }
}

fn api_root_config_entry(root: &DocumentRoot, expose_physical_paths: bool) -> ApiRootConfigEntry {
    ApiRootConfigEntry {
        root_id: None,
        spec: expose_physical_paths.then(|| {
            format_document_root_spec(root)
                .to_string_lossy()
                .to_string()
        }),
        mount: root.mount().map(logical_path_string),
        path: expose_physical_paths.then(|| root.path().display().to_string()),
        usable: fs::read_dir(root.path()).is_ok(),
        enabled: None,
        priority: None,
        check_for_changes: None,
    }
}

fn api_browser_root_config_entry(
    root: &BrowserRoot,
    expose_physical_paths: bool,
) -> ApiRootConfigEntry {
    let document_root = root.document_root();
    let mut entry = api_root_config_entry(&document_root, expose_physical_paths);
    entry.root_id = Some(root.root_id.as_str().to_string());
    entry.enabled = Some(root.enabled);
    entry.priority = Some(root.priority);
    entry.check_for_changes = Some(root.check_for_changes);
    entry
}

fn api_diagnostic(
    diagnostic: &okf::Diagnostic,
    roots: &[DocumentRoot],
    expose_physical_paths: bool,
) -> ApiDiagnostic {
    match diagnostic {
        okf::Diagnostic::MissingRoot { root } => ApiDiagnostic {
            diagnostic_type: "missing_root".to_string(),
            logical_path: None,
            root_index: configured_root_index(roots, root),
            selected_root_index: None,
            shadowed_root_index: None,
            root: expose_physical_paths.then(|| root.display().to_string()),
            selected_root: None,
            shadowed_root: None,
        },
        okf::Diagnostic::MissingDocumentType {
            relative_path,
            root,
        } => ApiDiagnostic {
            diagnostic_type: "missing_document_type".to_string(),
            logical_path: Some(logical_path_string(relative_path)),
            root_index: configured_root_index(roots, root),
            selected_root_index: None,
            shadowed_root_index: None,
            root: expose_physical_paths.then(|| root.display().to_string()),
            selected_root: None,
            shadowed_root: None,
        },
        okf::Diagnostic::ShadowedDocument {
            relative_path,
            selected_root,
            shadowed_root,
        } => ApiDiagnostic {
            diagnostic_type: "shadowed_document".to_string(),
            logical_path: Some(logical_path_string(relative_path)),
            root_index: None,
            selected_root_index: configured_root_index(roots, selected_root),
            shadowed_root_index: configured_root_index(roots, shadowed_root),
            root: None,
            selected_root: expose_physical_paths.then(|| selected_root.display().to_string()),
            shadowed_root: expose_physical_paths.then(|| shadowed_root.display().to_string()),
        },
    }
}

fn configured_root_index(roots: &[DocumentRoot], path: &Path) -> Option<usize> {
    roots.iter().position(|root| root.path() == path)
}

fn api_document_summary(
    document: &okf::Document,
    roots: &[DocumentRoot],
    expose_physical_paths: bool,
) -> ApiDocumentSummary {
    let logical_path = logical_path_string(document.relative_path());
    let source_relative_path = logical_path_string(document.source_relative_path());
    let (root_index, root) = roots
        .iter()
        .enumerate()
        .find(|(_, root)| {
            root.path() == document.root()
                && match root.mount() {
                    Some(mount) => mount.join(document.source_relative_path()),
                    None => document.source_relative_path().to_path_buf(),
                } == document.relative_path()
        })
        .expect("repository document must originate from a configured root");
    let root_mount = root_mount_name(root);
    let okf_uri = root_mount.as_deref().and_then(|mount| {
        okf::OkfUri::from_mount_and_path(mount, document.source_relative_path())
            .ok()
            .map(|uri| uri.to_string())
    });
    let directory_path = document
        .source_relative_path()
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(logical_path_string)
        .unwrap_or_default();
    let navigation_class = if directory_path.is_empty() {
        "root-document"
    } else {
        "nested-document"
    };
    let browser_path = match root_mount.as_deref() {
        Some(mount) => format!(
            "/okf-docs/{}/{}",
            encode_url_path(mount),
            encode_url_path(&source_relative_path)
        ),
        None => format!(
            "/okf-root/{root_index}/{}",
            encode_url_path(&source_relative_path)
        ),
    };
    ApiDocumentSummary {
        title: document.title().to_string(),
        document_type: document
            .document_type()
            .map(|kind| kind.as_str().to_string()),
        kind: document.kind().map(|kind| kind.as_str().to_string()),
        topic: document.topic().map(str::to_string),
        status: document.status().map(str::to_string),
        updated: document.updated().map(str::to_string),
        source_relative_path,
        directory_path,
        navigation_class,
        source_path: expose_physical_paths.then(|| document.physical_path().display().to_string()),
        browser_path,
        root_index,
        root_mount,
        logical_path,
        okf_uri,
        is_plan: document.is_plan(),
    }
}

fn encode_url_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn normalize_api_document_path(path: &str) -> Option<PathBuf> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let without_query = path.split_once('?').map_or(path, |(path, _query)| path);
    let without_hash = without_query
        .split_once('#')
        .map_or(without_query, |(path, _hash)| path);
    let logical = without_hash
        .strip_prefix("/okf-docs/")
        .or_else(|| without_hash.strip_prefix("okf-docs/"))
        .unwrap_or(without_hash)
        .trim_start_matches('/');
    let candidate = Path::new(logical);
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(candidate.to_path_buf())
}

fn extract_markdown_link_targets(source: &str, base_path: &Path) -> Vec<PathBuf> {
    let mut targets = Vec::new();
    let mut rest = source;
    while let Some(open_label) = rest.find('[') {
        rest = &rest[open_label + 1..];
        let Some(close_label) = rest.find("](") else {
            continue;
        };
        let target_start = close_label + 2;
        let Some(close_target) = rest[target_start..].find(')') else {
            continue;
        };
        let target = rest[target_start..target_start + close_target].trim();
        if let Some(target) = normalize_markdown_link_target(target, base_path) {
            targets.push(target);
        }
        rest = &rest[target_start + close_target + 1..];
    }
    targets
}

fn normalize_markdown_link_target(target: &str, base_path: &Path) -> Option<PathBuf> {
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with('#')
    {
        return None;
    }
    let without_fragment = target
        .split_once('#')
        .map_or(target, |(path, _fragment)| path);
    if without_fragment.is_empty() {
        return None;
    }
    let target_path = Path::new(without_fragment);
    let joined = if target_path.is_absolute() {
        normalize_api_document_path(without_fragment)?
    } else {
        base_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(target_path)
    };
    normalize_relative_components(&joined)
}

fn normalize_relative_components(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => normalized.push(value),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!normalized.as_os_str().is_empty()).then_some(normalized)
}

fn logical_path_string(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(any())]
fn relation_from_suggestion_legacy(suggestion: &AcceptedSuggestion) -> serde_json::Value {
    serde_json::json!({
        "type": "ai_suggested_edge",
        "target": suggestion.target_document,
        "source_chunk": suggestion.source_chunk,
        "target_chunk": suggestion.target_chunk,
        "suggestion_id": suggestion.id,
        "provider": suggestion.provider,
        "model": suggestion.model,
        "generation_method": suggestion.generation_method,
        "ai_generated": suggestion.ai_generated,
        "score": suggestion.score,
        "created_at": suggestion.created_at,
        "status": "accepted"
    })
}

#[cfg(any())]
fn merge_relations_into_frontmatter_legacy(
    source: &str,
    new_relations: Vec<serde_json::Value>,
) -> (String, usize) {
    let (frontmatter, body, had_frontmatter) = split_frontmatter(source);
    let mut existing_relations = Vec::<serde_json::Value>::new();
    let mut frontmatter_lines = Vec::<String>::new();

    for line in frontmatter.lines() {
        if let Some(value) = line.trim().strip_prefix("relations:") {
            if let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(value.trim()) {
                existing_relations = values;
            }
        } else {
            frontmatter_lines.push(line.to_string());
        }
    }

    let mut existing_ids = existing_relations
        .iter()
        .filter_map(|relation| relation.get("suggestion_id")?.as_str().map(str::to_string))
        .collect::<BTreeSet<_>>();
    let mut added = 0usize;
    for relation in new_relations {
        let suggestion_id = relation
            .get("suggestion_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !suggestion_id.is_empty() && existing_ids.contains(&suggestion_id) {
            continue;
        }
        if !suggestion_id.is_empty() {
            existing_ids.insert(suggestion_id);
        }
        existing_relations.push(relation);
        added += 1;
    }

    if !existing_relations.is_empty() {
        frontmatter_lines.push(format!(
            "relations: {}",
            serde_json::to_string(&existing_relations).unwrap_or_else(|_| "[]".to_string())
        ));
    }

    let frontmatter_block = format!("---\n{}\n---\n", frontmatter_lines.join("\n"));
    let updated = if had_frontmatter {
        format!("{frontmatter_block}{body}")
    } else if body.is_empty() {
        frontmatter_block
    } else {
        format!("{frontmatter_block}\n{body}")
    };
    (updated, added)
}

#[cfg(any())]
fn split_frontmatter_legacy(source: &str) -> (&str, &str, bool) {
    if !source.starts_with("---\n") {
        return ("", source, false);
    }
    let Some(end) = source[4..].find("\n---\n") else {
        return ("", source, false);
    };
    let end = end + 4;
    let frontmatter = &source[4..end];
    let body = &source[end + "\n---\n".len()..];
    (frontmatter.trim_matches('\n'), body, true)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;

    fn temp_browser_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = env::temp_dir().join(format!("okf-http-{name}-{unique}"));
        fs::create_dir_all(&root).expect("create temp browser root");
        root
    }

    fn authenticated_test_app(name: &str, roles: &[&str]) -> (Router, UserStore, PathBuf) {
        let root = temp_browser_root(name);
        let users = UserStore::open(root.join("state/auth.sqlite")).expect("user store");
        users
            .add_user("alice", "correct horse battery staple")
            .expect("add persistent user");
        for role in roles {
            users.grant_role("alice", role).expect("grant role");
        }
        let config = ServerConfig {
            mode: ServerMode::AuthenticatedTls,
            pairing_code: None,
            tls: Some(TlsFiles::new("certificate.pem", "private-key.pem")),
            host: DEFAULT_HOST.to_string(),
            port: 8443,
            browser_root: root.clone(),
            roots: Vec::new(),
            environment_roots_active: false,
            env_file: root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        };
        (build_app(config, None, Some(users.clone())), users, root)
    }

    async fn persistent_login(router: &Router) -> (String, String, serde_json::Value) {
        let response = router
            .clone()
            .oneshot(
                Request::post("/api/v1/access/login")
                    .header(header::HOST, "127.0.0.1:8443")
                    .header(header::ORIGIN, "https://127.0.0.1:8443")
                    .header("sec-fetch-site", "same-origin")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"username":"alice","password":"correct horse battery staple"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .expect("login response");
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("secure session cookie")
            .to_str()
            .unwrap()
            .to_string();
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("SameSite=Strict"));
        let cookie_pair = cookie.split(';').next().unwrap().to_string();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let csrf = payload["data"]["csrf_token"].as_str().unwrap().to_string();
        (cookie_pair, csrf, payload)
    }

    async fn local_pair(router: &Router, code: &str) -> (String, String) {
        let response = router
            .clone()
            .oneshot(
                Request::post("/api/v1/access/pair")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header("sec-fetch-site", "same-origin")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::json!({"code": code}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let payload: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let csrf = payload["data"]["csrf_token"].as_str().unwrap().to_string();
        (cookie, csrf)
    }

    fn local_authorized_request(
        method: &str,
        uri: impl AsRef<str>,
        cookie: &str,
        csrf: &str,
        body: impl Into<Body>,
    ) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri.as_ref())
            .header(header::HOST, "127.0.0.1:8003")
            .header(header::ORIGIN, "http://127.0.0.1:8003")
            .header("sec-fetch-site", "same-origin")
            .header(header::COOKIE, cookie)
            .header(CSRF_TOKEN_HEADER, csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.into())
            .unwrap()
    }

    fn authorized_request(method: &str, uri: &str, cookie: &str, csrf: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "127.0.0.1:8443")
            .header(header::ORIGIN, "https://127.0.0.1:8443")
            .header("sec-fetch-site", "same-origin")
            .header(header::COOKIE, cookie)
            .header(CSRF_TOKEN_HEADER, csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn persistent_https_sessions_enforce_capabilities_and_live_user_revision() {
        let (router, users, root) = authenticated_test_app("persistent-login", &["editor"]);

        let anonymous = router
            .clone()
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::OK);

        let (cookie, csrf, payload) = persistent_login(&router).await;
        assert_eq!(payload["data"]["username"], "alice");
        assert!(payload["data"]["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|capability| capability == "content.write"));

        let editor = router
            .clone()
            .oneshot(authorized_request(
                "GET",
                "/api/v1/roots/configuration",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(editor.status(), StatusCode::OK);

        let paid = router
            .clone()
            .oneshot(authorized_request(
                "POST",
                "/api/v1/voyage/check",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(paid.status(), StatusCode::FORBIDDEN);

        let user_admin = router
            .clone()
            .oneshot(authorized_request(
                "GET",
                "/api/v1/access/users",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(user_admin.status(), StatusCode::FORBIDDEN);

        users.grant_role("alice", "voyage").unwrap();
        let stale = router
            .clone()
            .oneshot(authorized_request(
                "GET",
                "/api/v1/roots/configuration",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::UNAUTHORIZED);

        users.grant_role("alice", "admin").unwrap();
        let (cookie, csrf, admin_payload) = persistent_login(&router).await;
        for capability in ["voyage.spend", "users.manage", "security.manage"] {
            assert!(admin_payload["data"]["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == capability));
        }
        let user_admin = router
            .clone()
            .oneshot(authorized_request(
                "GET",
                "/api/v1/access/users",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(user_admin.status(), StatusCode::OK);

        let admin_revoke = router
            .clone()
            .oneshot(
                Request::post("/api/v1/access/sessions/revoke-user")
                    .header(header::HOST, "127.0.0.1:8443")
                    .header(header::ORIGIN, "https://127.0.0.1:8443")
                    .header("sec-fetch-site", "same-origin")
                    .header(header::COOKIE, &cookie)
                    .header(CSRF_TOKEN_HEADER, &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"username":"alice"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admin_revoke.status(), StatusCode::OK);
        let revoked = router
            .clone()
            .oneshot(authorized_request(
                "GET",
                "/api/v1/roots/configuration",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
        let (cookie, csrf, _) = persistent_login(&router).await;

        users.revoke_role("alice", "voyage").unwrap();
        let role_revoked = router
            .clone()
            .oneshot(authorized_request(
                "GET",
                "/api/v1/roots/configuration",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(role_revoked.status(), StatusCode::UNAUTHORIZED);
        let (cookie, csrf, _) = persistent_login(&router).await;

        let password_change = router
            .clone()
            .oneshot(
                Request::post("/api/v1/access/password")
                    .header(header::HOST, "127.0.0.1:8443")
                    .header(header::ORIGIN, "https://127.0.0.1:8443")
                    .header("sec-fetch-site", "same-origin")
                    .header(header::COOKIE, &cookie)
                    .header(CSRF_TOKEN_HEADER, &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"current_password":"correct horse battery staple","new_password":"new correct horse battery staple","confirm_password":"new correct horse battery staple"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(password_change.status(), StatusCode::OK);
        assert!(password_change
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Secure"));
        assert!(!users
            .authenticate("alice", "correct horse battery staple")
            .unwrap());
        assert!(users
            .authenticate("alice", "new correct horse battery staple")
            .unwrap());

        users
            .change_password("alice", "correct horse battery staple")
            .unwrap();
        let (cookie, csrf, _) = persistent_login(&router).await;
        users.disable_user("alice").unwrap();
        let disabled = router
            .clone()
            .oneshot(authorized_request(
                "GET",
                "/api/v1/config/roots",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(disabled.status(), StatusCode::UNAUTHORIZED);
        fs::remove_dir_all(root).unwrap();
    }

    fn proxy_request(method: &str, uri: &str) -> axum::http::request::Builder {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "127.0.0.1:8443")
            .header(
                security::TRUSTED_PROXY_TOKEN_HEADER,
                "proxy-secret-token-000000000000000000000000",
            )
            .header("x-forwarded-proto", "https")
            .header("x-forwarded-host", "knowledge.example")
    }

    #[tokio::test]
    async fn trusted_proxy_rejects_bypass_ambiguity_downgrade_and_remote_root_management() {
        let root = temp_browser_root("trusted-proxy");
        let documents = root.join("documents");
        fs::create_dir_all(&documents).unwrap();
        fs::write(
            documents.join("index.md"),
            "---\ntitle: Proxy document\ntype: Guide\n---\n# Proxy document\n",
        )
        .unwrap();
        let users = UserStore::open(root.join("state/auth.sqlite")).unwrap();
        users
            .add_user("alice", "correct horse battery staple")
            .unwrap();
        users.grant_role("alice", "editor").unwrap();
        let config = ServerConfig {
            mode: ServerMode::AuthenticatedTls,
            pairing_code: None,
            tls: Some(TlsFiles::new("certificate.pem", "private-key.pem")),
            host: DEFAULT_HOST.to_string(),
            port: 8443,
            browser_root: root.clone(),
            roots: vec![DocumentRoot::mounted("proxy", &documents)],
            environment_roots_active: false,
            env_file: root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: Some(Box::new(TrustedProxyConfig {
                public_origin: "https://knowledge.example".to_string(),
                public_authority: "knowledge.example".to_string(),
                token: "proxy-secret-token-000000000000000000000000".to_string(),
            })),
            expose_physical_paths: false,
        };
        let router = build_app(config, None, Some(users));

        let bypass = router
            .clone()
            .oneshot(
                Request::get("/health")
                    .header(header::HOST, "127.0.0.1:8443")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bypass.status(), StatusCode::FORBIDDEN);

        let health = router
            .clone()
            .oneshot(proxy_request("GET", "/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);

        let anonymous_documents = router
            .clone()
            .oneshot(
                proxy_request("GET", "/api/v1/documents")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous_documents.status(), StatusCode::UNAUTHORIZED);

        let login = router
            .clone()
            .oneshot(
                proxy_request("POST", "/api/v1/access/login")
                    .header(header::ORIGIN, "https://knowledge.example")
                    .header("sec-fetch-site", "same-origin")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"username":"alice","password":"correct horse battery staple"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let cookie = login
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let body = to_bytes(login.into_body(), usize::MAX).await.unwrap();
        let login: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let csrf = login["data"]["csrf_token"].as_str().unwrap();

        let documents_response = router
            .clone()
            .oneshot(
                proxy_request("GET", "/api/v1/documents")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(documents_response.status(), StatusCode::OK);

        let roots = router
            .clone()
            .oneshot(
                proxy_request("GET", "/api/v1/config/roots")
                    .header(header::ORIGIN, "https://knowledge.example")
                    .header("sec-fetch-site", "same-origin")
                    .header(header::COOKIE, &cookie)
                    .header(CSRF_TOKEN_HEADER, csrf)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(roots.status(), StatusCode::FORBIDDEN);

        for (name, value) in [
            ("x-forwarded-proto", "http"),
            ("x-forwarded-host", "evil.example"),
            ("x-forwarded-for", "192.0.2.1, 192.0.2.2"),
        ] {
            let response = router
                .clone()
                .oneshot(
                    proxy_request("GET", "/health")
                        .header(name, value)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{name}");
        }
        let forwarded = router
            .oneshot(
                proxy_request("GET", "/health")
                    .header("forwarded", "for=192.0.2.1;proto=https")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(forwarded.status(), StatusCode::FORBIDDEN);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn index_job_registry_prevents_duplicate_root_jobs_and_releases_on_drop() {
        let root = temp_browser_root("index-job-registry");
        let registry = IndexJobRegistry::default();

        let first = registry.try_begin(&root).expect("first index job");
        assert!(registry.try_begin(&root.join(".")).is_none());
        drop(first);
        assert!(registry.try_begin(&root).is_some());

        fs::remove_dir_all(root).expect("remove index root");
    }

    #[test]
    #[should_panic(expected = "authenticated TLS routers require validated TLS material")]
    fn authenticated_router_cannot_be_constructed_without_prepared_tls() {
        let root = PathBuf::from("unused-tls-router-proof");
        let _ = app(ServerConfig {
            mode: ServerMode::AuthenticatedTls,
            pairing_code: None,
            tls: Some(TlsFiles::new("certificate.pem", "private-key.pem")),
            host: DEFAULT_HOST.to_string(),
            port: 8443,
            browser_root: root.clone(),
            roots: Vec::new(),
            environment_roots_active: false,
            env_file: root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });
    }

    #[test]
    fn voyage_transport_failures_map_to_specific_http_statuses() {
        let timeout =
            ConnectivityReport::failure(None, Some("timeout".to_string()), "request timed out");
        let launch = ConnectivityReport::failure(
            None,
            Some("process_launch_error".to_string()),
            "curl missing",
        );

        assert_eq!(voyage_report_status(&timeout), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(
            voyage_report_status(&launch),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn failed_voyage_run_does_not_touch_either_persistent_index_backend() {
        let root = temp_browser_root("failed-voyage-persistence");
        fs::remove_dir_all(&root).expect("remove unused index root");
        let report =
            ConnectivityReport::failure(None, Some("timeout".to_string()), "request timed out");
        let index = LocalIndex::default();

        persist_successful_voyage_index(&report, &[], &[], &index, &root)
            .expect("failure path must be a persistence no-op");

        assert!(!root.exists());
    }

    #[test]
    fn resolves_existing_browser_assets_inside_root() {
        let root = temp_browser_root("resolve");
        let index = root.join("index.html");
        fs::write(&index, "<!doctype html>").expect("write index");

        let resolved = resolve_static_file(&root, "index.html").expect("resolve index");
        assert_eq!(resolved, index.canonicalize().expect("canonical index"));

        fs::remove_dir_all(root).expect("remove temp browser root");
    }

    #[test]
    fn missing_browser_assets_return_missing() {
        let root = temp_browser_root("missing");

        assert_eq!(
            resolve_static_file(&root, "missing.js"),
            Err(StaticFileError::Missing)
        );

        fs::remove_dir_all(root).expect("remove temp browser root");
    }

    #[test]
    fn parent_directory_components_are_rejected_before_joining() {
        let root = temp_browser_root("parent-dir");

        assert_eq!(
            resolve_static_file(&root, "../Cargo.toml"),
            Err(StaticFileError::EscapesRoot)
        );

        fs::remove_dir_all(root).expect("remove temp browser root");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escapes_are_rejected_after_canonicalization() {
        use std::os::unix::fs as unix_fs;

        let root = temp_browser_root("symlink-root");
        let outside = temp_browser_root("symlink-outside");
        fs::write(outside.join("secret.txt"), "outside").expect("write outside file");
        unix_fs::symlink(outside.join("secret.txt"), root.join("secret.txt"))
            .expect("create symlink");

        assert_eq!(
            resolve_static_file(&root, "secret.txt"),
            Err(StaticFileError::EscapesRoot)
        );

        fs::remove_dir_all(root).expect("remove temp browser root");
        fs::remove_dir_all(outside).expect("remove outside root");
    }

    #[test]
    fn browser_asset_content_types_cover_current_files() {
        assert_eq!(
            content_type_for_path(Path::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            content_type_for_path(Path::new("app.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type_for_path(Path::new("styles.css")),
            "text/css; charset=utf-8"
        );
    }

    #[test]
    fn packaged_browser_uses_durable_review_apis_without_pending_voyage_placeholders() {
        let source = String::from_utf8_lossy(include_bytes!("../browser/app.js"));
        let html = String::from_utf8_lossy(include_bytes!("../browser/index.html"));

        for endpoint in [
            "/api/v1/voyage/index",
            "/api/v1/voyage/search",
            "/api/v1/suggestions/generate",
            "/api/v1/suggestions",
            "/api/v1/review-sets/",
        ] {
            assert!(
                source.contains(endpoint),
                "missing browser endpoint {endpoint}"
            );
        }
        assert!(source.contains("/api/v1/access/pair"));
        assert!(source.contains("/api/v1/access/session/refresh"));
        assert!(source.contains("/api/v1/access/logout"));
        assert!(source.contains("X-OKF-CSRF-Token"));
        assert!(source.contains("credentials: \"same-origin\""));
        assert!(!source.contains("X-OKF-Session-Token"));
        assert!(source.contains("Local metadata prefilter — not Voyage AI"));
        assert!(!source.contains("pending-voyage-ai"));
        assert!(!source.contains("localStorage"));
        assert!(!source.contains("sessionStorage"));
        assert!(html.contains("id=\"pairing-dialog\""));
        assert!(html.contains("aria-labelledby=\"pairing-title\""));
        assert!(!html.contains("id=\"ai-session-token\""));
        assert!(html.contains("SQLite is the durable review source"));
        assert!(html.contains("id=\"root-access-required\""));
        assert!(html.contains("id=\"root-protected-content\""));
        assert!(source.contains("openRootAccessGuide"));
    }

    #[test]
    fn packaged_browser_installs_upgrades_and_protects_modifications() {
        let parent = temp_browser_root("browser-install");
        let browser_root = parent.join("installed-browser");
        let config = BrowserInstallConfig {
            browser_root: browser_root.clone(),
            force: false,
        };

        let installed = install_browser_assets(&config).expect("install browser");
        assert!(installed.changed);
        assert_eq!(installed.installed_files, BROWSER_ASSETS.len());
        validate_browser_root(&browser_root).expect("valid browser root");
        assert!(browser_root.join(BROWSER_MANIFEST).is_file());

        let unchanged = install_browser_assets(&config).expect("repeat install");
        assert!(!unchanged.changed);

        fs::write(browser_root.join("app.js"), "// user modification")
            .expect("modify browser asset");
        fs::write(browser_root.join("user-note.txt"), "preserve me").expect("write user file");
        let error = install_browser_assets(&config).expect_err("modified asset must be protected");
        assert!(error.contains("--force"));

        let forced = install_browser_assets(&BrowserInstallConfig {
            browser_root: browser_root.clone(),
            force: true,
        })
        .expect("force upgrade");
        assert!(forced.changed);
        assert_eq!(
            fs::read(browser_root.join("app.js")).expect("read installed app"),
            BROWSER_ASSETS
                .iter()
                .find(|(path, _)| *path == "app.js")
                .expect("packaged app")
                .1
        );
        assert_eq!(
            fs::read_to_string(browser_root.join("user-note.txt")).expect("read user file"),
            "preserve me"
        );

        fs::remove_dir_all(parent).expect("remove install fixture");
    }

    #[test]
    fn browser_root_validation_reports_missing_assets() {
        let root = temp_browser_root("browser-validation");
        let error = validate_browser_root(&root).expect_err("empty root is invalid");
        assert!(error.contains("missing browser asset"));
        fs::remove_dir_all(root).expect("remove validation fixture");
    }

    #[test]
    fn packaged_browser_discovers_documents_without_project_catalog_entries() {
        let app = BROWSER_ASSETS
            .iter()
            .find(|(path, _)| *path == "app.js")
            .map(|(_, bytes)| String::from_utf8_lossy(bytes))
            .expect("packaged app.js");
        assert!(app.contains("/api/v1/documents"));
        assert!(!app.contains("scanlab"));
        assert!(!app.contains("SCQL"));
        assert!(!app.contains("/repo-files/"));
        assert!(!app.contains("const DOCS = ["));
        assert!(app.contains("buildNavigationNode"));
        assert!(app.contains("navigationNodeContainsPath"));
        assert!(app.contains("directory.open = revealMatches"));
        assert!(app.contains("tree-directory"));
        assert!(!app.contains("const section = document.topic"));
    }

    #[tokio::test]
    async fn app_serves_docs_browser_index_and_redirects_root() {
        let root = temp_browser_root("smoke-index");
        fs::write(root.join("index.html"), "<!doctype html><title>OKF</title>")
            .expect("write index");

        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: Vec::new(),
            environment_roots_active: false,
            env_file: root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });

        let redirect = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("redirect request"),
            )
            .await
            .expect("redirect response");

        assert_eq!(redirect.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            redirect
                .headers()
                .get(header::LOCATION)
                .expect("redirect location"),
            "/docs-browser/index.html"
        );

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/docs-browser/index.html")
                    .body(Body::empty())
                    .expect("index request"),
            )
            .await
            .expect("index response");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response
            .headers()
            .contains_key(header::CONTENT_SECURITY_POLICY));
        assert_eq!(
            response
                .headers()
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .unwrap(),
            "nosniff"
        );
        assert_eq!(
            response.headers().get(header::X_FRAME_OPTIONS).unwrap(),
            "DENY"
        );
        assert_eq!(
            response.headers().get(header::REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
        assert!(response.headers().contains_key("permissions-policy"));
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .expect("content type"),
            "text/html; charset=utf-8"
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body");
        assert!(String::from_utf8_lossy(&body).contains("<title>OKF</title>"));

        fs::remove_dir_all(root).expect("remove temp browser root");
    }

    #[tokio::test]
    async fn browser_routes_reject_plain_encoded_and_double_encoded_traversal() {
        let root = temp_browser_root("encoded-traversal-root");
        fs::write(root.join("index.html"), "OKF").expect("write index");
        let outside = root.parent().expect("temp parent").join(format!(
            "outside-secret-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::write(&outside, "must-not-be-served").expect("write outside secret");
        let outside_name = outside.file_name().unwrap().to_string_lossy();
        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: Vec::new(),
            environment_roots_active: false,
            env_file: root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });

        for path in [
            format!("/docs-browser/../{outside_name}"),
            format!("/docs-browser/%2e%2e/{outside_name}"),
            format!("/docs-browser/%252e%252e/{outside_name}"),
            format!("/docs-browser/%2E%2E%2F{outside_name}"),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(Body::empty())
                        .expect("traversal request"),
                )
                .await
                .expect("traversal response");
            assert_ne!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("read traversal response");
            assert!(!String::from_utf8_lossy(&body).contains("must-not-be-served"));
        }

        fs::remove_file(outside).expect("remove outside secret");
        fs::remove_dir_all(root).expect("remove browser root");
    }

    #[tokio::test]
    async fn read_only_mode_mounts_reads_and_disables_sensitive_routes() {
        let root = temp_browser_root("read-only-mode");
        fs::write(root.join("index.html"), "OKF").expect("write index");
        let documents = root.join("knowledge");
        fs::create_dir_all(&documents).expect("create document root");
        fs::write(
            documents.join("welcome.md"),
            "---\ntitle: Welcome\ntype: Guide\n---\n# Welcome\n",
        )
        .expect("write document");
        let env_file = root.join(".env");
        let router = app(ServerConfig {
            mode: ServerMode::ReadOnly,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: vec![DocumentRoot::mounted("read-only", &documents)],
            environment_roots_active: false,
            env_file: env_file.clone(),
            scanlab_compat: false,
            session_token: "unused-read-only-token-000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });

        for path in [
            "/api/v1/documents",
            "/api/v1/graph",
            "/api/v1/voyage/status",
            "/api/v1/health",
            "/api/v1/access/session",
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(Body::empty())
                        .expect("read-only request"),
                )
                .await
                .expect("read-only response");
            assert_eq!(response.status(), StatusCode::OK, "{path}");
        }

        let health = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .expect("health request"),
            )
            .await
            .expect("health response");
        let body = to_bytes(health.into_body(), usize::MAX)
            .await
            .expect("health body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("health JSON");
        assert_eq!(payload["data"]["mode"], "read-only");

        for path in [
            "/api/v1/config/roots",
            "/api/v1/roots/configuration",
            "/api/v1/roots/proposals/example",
            "/api/v1/suggestions",
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header(SESSION_TOKEN_HEADER, "unused-read-only-token-000000000000")
                        .body(Body::empty())
                        .expect("disabled sensitive read"),
                )
                .await
                .expect("disabled sensitive response");
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }

        for path in [
            "/api/v1/config/roots",
            "/api/v1/roots/proposals",
            "/api/v1/roots/proposals/example/registration",
            "/api/v1/roots/proposals/example/initialization",
            "/api/v1/roots/example",
            "/api/v1/access/pair",
            "/api/v1/access/session/refresh",
            "/api/v1/access/logout",
            "/api/v1/voyage/check",
            "/api/v1/voyage/index",
            "/api/v1/voyage/rebuild",
            "/api/v1/voyage/search",
            "/api/v1/suggestions/generate",
            "/api/v1/edges/apply",
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(path)
                        .header(SESSION_TOKEN_HEADER, "unused-read-only-token-000000000000")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from("{}"))
                        .expect("disabled request"),
                )
                .await
                .expect("disabled response");
            assert!(
                matches!(
                    response.status(),
                    StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
                ),
                "{path}: {}",
                response.status()
            );
        }
        assert!(!env_file.exists());
        fs::remove_dir_all(root).expect("remove browser root");
    }

    #[tokio::test]
    async fn document_routes_reject_regular_files_outside_the_admitted_inventory() {
        let root = temp_browser_root("unadmitted-regular-file");
        fs::write(root.join("index.html"), "OKF").expect("write browser index");
        let documents = root.join("knowledge");
        fs::create_dir_all(&documents).expect("create document root");
        fs::write(documents.join("generated-private.txt"), "fixture-only\n")
            .expect("write unsupported fixture");

        let router = app(ServerConfig {
            mode: ServerMode::ReadOnly,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: vec![DocumentRoot::mounted("quarantine", &documents)],
            environment_roots_active: false,
            env_file: root.join(".env"),
            scanlab_compat: false,
            session_token: "unused-quarantine-token-00000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/okf-docs/quarantine/generated-private.txt")
                    .body(Body::empty())
                    .expect("unsupported-file request"),
            )
            .await
            .expect("unsupported-file response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        fs::remove_dir_all(root).expect("remove quarantine fixture");
    }

    #[tokio::test]
    async fn document_routes_serve_only_admitted_markdown_and_declared_valid_csv() {
        let root = temp_browser_root("admitted-http-files");
        fs::write(root.join("index.html"), "OKF").expect("write browser index");
        let documents = root.join("knowledge");
        fs::create_dir_all(documents.join(".ssh")).expect("hidden directory");
        fs::create_dir_all(documents.join("Case")).expect("case directory");
        fs::create_dir_all(documents.join("case")).expect("folded case directory");
        fs::write(
            documents.join("index.md"),
            concat!(
                "---\nresources: [",
                "{\"path\":\"declared.csv\",\"type\":\"Dataset\",",
                "\"media_type\":\"text/csv; charset=utf-8\"},",
                "{\"path\":\"broken.csv\",\"type\":\"Dataset\",",
                "\"media_type\":\"text/csv; charset=utf-8\"}",
                "]\n---\n# Index\n"
            ),
        )
        .expect("write index");
        fs::write(
            documents.join("valid.md"),
            "---\ntype: Guide\n---\n# Valid\n",
        )
        .expect("valid Markdown");
        fs::write(documents.join("declared.csv"), "name,value\nalpha,1\n").expect("declared CSV");
        fs::write(documents.join("undeclared.csv"), "name,value\nalpha,1\n")
            .expect("undeclared CSV");
        fs::write(documents.join("broken.csv"), "a,b\n1\n").expect("broken CSV");
        fs::write(documents.join("script.js"), "alert(1)\n").expect("JavaScript");
        fs::write(documents.join(".env"), "SECRET=fixture\n").expect("hidden env");
        fs::write(documents.join(".ssh/id.md"), "# Hidden\n").expect("hidden SSH file");
        fs::write(documents.join("Case/Same.md"), "# First\n").expect("first collision");
        fs::write(documents.join("case/same.md"), "# Second\n").expect("second collision");
        #[cfg(unix)]
        std::os::unix::fs::symlink("valid.md", documents.join("linked.md"))
            .expect("document symlink");

        let router = app(ServerConfig {
            mode: ServerMode::ReadOnly,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: vec![DocumentRoot::mounted("secure", &documents)],
            environment_roots_active: false,
            env_file: root.join(".env"),
            scanlab_compat: false,
            session_token: "unused-secure-route-token-0000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });

        for (path, content_type) in [
            ("/okf-docs/secure/valid.md", "text/markdown; charset=utf-8"),
            ("/okf-docs/secure/declared.csv", "text/csv; charset=utf-8"),
        ] {
            let response = router
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .expect("admitted response");
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            assert_eq!(response.headers()[header::CONTENT_TYPE], content_type);
            assert_eq!(
                response.headers()[header::X_CONTENT_TYPE_OPTIONS],
                "nosniff"
            );
        }

        let mut denied = vec![
            "/okf-docs/secure/undeclared.csv",
            "/okf-docs/secure/broken.csv",
            "/okf-docs/secure/script.js",
            "/okf-docs/secure/.env",
            "/okf-docs/secure/%2eenv",
            "/okf-docs/secure/%252eenv",
            "/okf-docs/secure/.ssh/id.md",
        ];
        let first_case_path = documents.join("Case/Same.md");
        let second_case_path = documents.join("case/same.md");
        if first_case_path.canonicalize().ok() != second_case_path.canonicalize().ok() {
            denied.push("/okf-docs/secure/Case/Same.md");
            denied.push("/okf-docs/secure/case/same.md");
        }
        #[cfg(unix)]
        denied.push("/okf-docs/secure/linked.md");
        for path in denied {
            let response = router
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .expect("denied response");
            assert_ne!(response.status(), StatusCode::OK, "{path}");
        }

        fs::remove_dir_all(root).expect("remove secure route fixture");
    }

    #[tokio::test]
    async fn local_pairing_is_one_time_csrf_bound_and_invalidated_by_logout_and_restart() {
        let root = temp_browser_root("local-pairing");
        fs::write(root.join("index.html"), "OKF").expect("write index");
        let config = ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: Some("1234-5678-9012".to_string()),
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: Vec::new(),
            environment_roots_active: false,
            env_file: root.join(".env"),
            scanlab_compat: false,
            session_token: "transitional-session-token-000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        };
        let router = app(config.clone());

        let fixed_session = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/config/roots")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header(header::COOKIE, "okf_session=attacker-selected")
                    .header(CSRF_TOKEN_HEADER, "attacker-selected")
                    .body(Body::empty())
                    .expect("fixed session request"),
            )
            .await
            .expect("fixed session response");
        assert_eq!(fixed_session.status(), StatusCode::UNAUTHORIZED);

        let cross_origin = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/access/pair")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://evil.example")
                    .header("sec-fetch-site", "cross-site")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"code":"1234-5678-9012"}"#))
                    .expect("cross-origin pairing request"),
            )
            .await
            .expect("cross-origin pairing response");
        assert_eq!(cross_origin.status(), StatusCode::FORBIDDEN);

        let malformed = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/access/pair")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"code":"bad"}"#))
                    .expect("malformed pairing request"),
            )
            .await
            .expect("malformed pairing response");
        assert_eq!(malformed.status(), StatusCode::UNAUTHORIZED);

        let pair = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/access/pair")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header("sec-fetch-site", "same-origin")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"code":"1234-5678-9012"}"#))
                    .expect("pairing request"),
            )
            .await
            .expect("pairing response");
        assert_eq!(pair.status(), StatusCode::OK);
        let set_cookie = pair
            .headers()
            .get(header::SET_COOKIE)
            .expect("session cookie")
            .to_str()
            .expect("cookie text")
            .to_string();
        assert!(set_cookie.contains("HttpOnly"));
        assert!(set_cookie.contains("SameSite=Strict"));
        assert!(!set_cookie.contains("1234-5678-9012"));
        let cookie = set_cookie
            .split(';')
            .next()
            .expect("cookie pair")
            .to_string();
        let session_id = cookie.split_once('=').expect("session cookie value").1;
        assert_eq!(session_id.len(), 64);
        assert_ne!(session_id, "attacker-selected");
        let pair_body = to_bytes(pair.into_body(), usize::MAX)
            .await
            .expect("pairing body");
        let payload: serde_json::Value = serde_json::from_slice(&pair_body).expect("pairing JSON");
        let csrf = payload["data"]["csrf_token"]
            .as_str()
            .expect("CSRF token")
            .to_string();
        assert_eq!(csrf.len(), 64);
        assert_eq!(payload["data"]["csrf_header"], CSRF_TOKEN_HEADER);
        assert!(!String::from_utf8_lossy(&pair_body).contains("1234-5678-9012"));
        assert!(!String::from_utf8_lossy(&pair_body).contains(session_id));

        let status = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/access/session")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .expect("session status request"),
            )
            .await
            .expect("session status response");
        assert_eq!(status.status(), StatusCode::OK);
        let status_body = to_bytes(status.into_body(), usize::MAX)
            .await
            .expect("session status body");
        let status_payload: serde_json::Value =
            serde_json::from_slice(&status_body).expect("session status JSON");
        assert_eq!(status_payload["data"]["authenticated"], true);
        assert_eq!(status_payload["data"]["pairing_available"], false);
        assert_eq!(status_payload["data"]["scope"], "local-editor");
        assert!(status_payload["data"].get("csrf_token").is_none());
        assert!(!String::from_utf8_lossy(&status_body).contains(session_id));

        let cross_origin_refresh = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/access/session/refresh")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://evil.example")
                    .header("sec-fetch-site", "cross-site")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .expect("cross-origin session refresh request"),
            )
            .await
            .expect("cross-origin session refresh response");
        assert_eq!(cross_origin_refresh.status(), StatusCode::FORBIDDEN);

        let refreshed = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/access/session/refresh")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header("sec-fetch-site", "same-origin")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .expect("session refresh request"),
            )
            .await
            .expect("session refresh response");
        assert_eq!(refreshed.status(), StatusCode::OK);
        let refreshed_body = to_bytes(refreshed.into_body(), usize::MAX)
            .await
            .expect("session refresh body");
        let refreshed_payload: serde_json::Value =
            serde_json::from_slice(&refreshed_body).expect("session refresh JSON");
        assert_eq!(refreshed_payload["data"]["csrf_token"], csrf);
        assert_eq!(refreshed_payload["data"]["scope"], "local-editor");
        assert!(!String::from_utf8_lossy(&refreshed_body).contains(session_id));

        let replay = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/access/pair")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"code":"1234-5678-9012"}"#))
                    .expect("pairing replay"),
            )
            .await
            .expect("pairing replay response");
        assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);

        let missing_csrf = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/config/roots")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .expect("request without CSRF"),
            )
            .await
            .expect("missing CSRF response");
        assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);

        let authorized = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/config/roots")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header("sec-fetch-site", "same-origin")
                    .header(header::COOKIE, &cookie)
                    .header(CSRF_TOKEN_HEADER, &csrf)
                    .body(Body::empty())
                    .expect("authorized request"),
            )
            .await
            .expect("authorized response");
        assert_eq!(authorized.status(), StatusCode::OK);

        let restarted = app(config);
        let after_restart = restarted
            .oneshot(
                Request::builder()
                    .uri("/api/v1/config/roots")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header(header::COOKIE, &cookie)
                    .header(CSRF_TOKEN_HEADER, &csrf)
                    .body(Body::empty())
                    .expect("request after restart"),
            )
            .await
            .expect("restart response");
        assert_eq!(after_restart.status(), StatusCode::UNAUTHORIZED);

        let logout = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/access/logout")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header(header::COOKIE, &cookie)
                    .header(CSRF_TOKEN_HEADER, &csrf)
                    .body(Body::empty())
                    .expect("logout request"),
            )
            .await
            .expect("logout response");
        assert_eq!(logout.status(), StatusCode::OK);
        assert!(logout
            .headers()
            .get(header::SET_COOKIE)
            .expect("expired cookie")
            .to_str()
            .expect("expired cookie text")
            .contains("Max-Age=0"));

        let after_logout = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/config/roots")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "http://127.0.0.1:8003")
                    .header(header::COOKIE, &cookie)
                    .header(CSRF_TOKEN_HEADER, &csrf)
                    .body(Body::empty())
                    .expect("request after logout"),
            )
            .await
            .expect("logout invalidation response");
        assert_eq!(after_logout.status(), StatusCode::UNAUTHORIZED);

        fs::remove_dir_all(root).expect("remove pairing fixture");
    }

    #[tokio::test]
    async fn every_token_spending_or_mutating_api_requires_the_session_token() {
        let root = temp_browser_root("sensitive-api-auth");
        fs::write(root.join("index.html"), "OKF").expect("write index");
        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: Vec::new(),
            environment_roots_active: false,
            env_file: root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });
        for (path, body) in [
            ("/api/okf/voyage/check", "{}"),
            ("/api/okf/voyage/index", "{}"),
            ("/api/okf/voyage/rebuild", "{}"),
            ("/api/okf/suggestions/generate", r#"{"threshold":0.8}"#),
            (
                "/api/okf/suggestions/import",
                r#"{"type":"okf-ai-edge-review","review_set_id":"x","accepted_edges":[]}"#,
            ),
            ("/api/okf/suggestions/example/accept", "{}"),
            ("/api/okf/suggestions/example/deny", "{}"),
            ("/api/okf/review-sets/example/accept-all", "{}"),
            ("/api/okf/review-sets/example/deny-all", "{}"),
            ("/api/okf/voyage/search", r#"{"query":"security"}"#),
            ("/api/okf/edges/apply", r#"{"dry_run":true}"#),
            ("/api/okf/config/roots", r#"{"spec":"okf=docs"}"#),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(path)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(body))
                        .expect("sensitive request"),
                )
                .await
                .expect("sensitive response");
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        }
        let list_response = router
            .oneshot(
                Request::builder()
                    .uri("/api/okf/suggestions")
                    .body(Body::empty())
                    .expect("suggestion list request"),
            )
            .await
            .expect("suggestion list response");
        assert_eq!(list_response.status(), StatusCode::UNAUTHORIZED);
        fs::remove_dir_all(root).expect("remove browser root");
    }

    #[tokio::test]
    async fn unmounted_root_inventory_paths_are_routable_and_url_encoded() {
        let browser_root = temp_browser_root("unmounted-browser");
        fs::write(browser_root.join("index.html"), "OKF").expect("write browser index");
        let document_root = temp_browser_root("arbitrary root");
        fs::create_dir_all(document_root.join("Wissen mit Leerzeichen"))
            .expect("create nested document directory");
        fs::write(
            document_root.join("Wissen mit Leerzeichen/Überblick.md"),
            "---\ntype: concept\ntitle: Überblick\ntopic: Einführung\n---\n# Überblick\n",
        )
        .expect("write document");

        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: browser_root.clone(),
            roots: vec![DocumentRoot::new(&document_root)],
            environment_roots_active: false,
            env_file: browser_root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });
        let inventory = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/documents")
                    .body(Body::empty())
                    .expect("inventory request"),
            )
            .await
            .expect("inventory response");
        assert_eq!(inventory.status(), StatusCode::OK);
        let body = to_bytes(inventory.into_body(), usize::MAX)
            .await
            .expect("read inventory");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("parse inventory");
        let browser_path = payload["data"]["documents"][0]["browser_path"]
            .as_str()
            .expect("browser path");
        assert_eq!(
            browser_path,
            "/okf-root/0/Wissen%20mit%20Leerzeichen/%C3%9Cberblick.md"
        );
        assert_eq!(payload["data"]["documents"][0]["root_index"], 0);
        assert!(payload["data"]["documents"][0]["root_mount"].is_null());
        assert!(payload["data"]["documents"][0].get("source_path").is_none());

        let document = router
            .oneshot(
                Request::builder()
                    .uri(browser_path)
                    .body(Body::empty())
                    .expect("document request"),
            )
            .await
            .expect("document response");
        assert_eq!(document.status(), StatusCode::OK);

        fs::remove_dir_all(browser_root).expect("remove browser root");
        fs::remove_dir_all(document_root).expect("remove document root");
    }

    #[tokio::test]
    async fn v1_document_contract_is_complete_versioned_and_private_by_default() {
        let browser_root = temp_browser_root("v1-contract-browser");
        fs::write(browser_root.join("index.html"), "OKF").expect("write browser index");
        let document_root = temp_browser_root("v1-contract-documents");
        fs::write(
            document_root.join("index.md"),
            "---\ntitle: Contract\ntype: Reference\nkind: knowledge-reference\ntopic: api\nstatus: active\nupdated: 2026-06-27\n---\n# Contract\n",
        )
        .expect("write contract document");
        let config = |expose_physical_paths| ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: browser_root.clone(),
            roots: vec![DocumentRoot::mounted("contract", &document_root)],
            environment_roots_active: false,
            env_file: browser_root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths,
        };

        let response = app(config(false))
            .oneshot(
                Request::builder()
                    .uri("/api/v1/documents")
                    .body(Body::empty())
                    .expect("v1 request"),
            )
            .await
            .expect("v1 response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get("x-okf-api-version").unwrap(), "v1");
        assert!(response.headers().contains_key(REQUEST_ID_HEADER));
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read v1 response");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("parse v1 response");
        assert_eq!(
            payload,
            serde_json::json!({
                "api_version": "v1",
                "data": {
                    "roots": [{"id": "root-0", "mount": "contract"}],
                    "diagnostics": [],
                    "documents": [{
                        "title": "Contract",
                        "type": "Reference",
                        "kind": "knowledge-reference",
                        "topic": "api",
                        "status": "active",
                        "updated": "2026-06-27",
                        "logical_path": "contract/index.md",
                        "okf_uri": "okf://contract/index.md",
                        "source_relative_path": "index.md",
                        "directory_path": "",
                        "navigation_class": "root-document",
                        "browser_path": "/okf-docs/contract/index.md",
                        "root_index": 0,
                        "root_mount": "contract",
                        "is_plan": false
                    }]
                }
            })
        );
        assert!(!String::from_utf8_lossy(&body).contains(&document_root.display().to_string()));

        let debug_response = app(config(true))
            .oneshot(
                Request::builder()
                    .uri("/api/v1/documents")
                    .body(Body::empty())
                    .expect("debug request"),
            )
            .await
            .expect("debug response");
        let debug_body = to_bytes(debug_response.into_body(), usize::MAX)
            .await
            .expect("read debug response");
        assert!(String::from_utf8_lossy(&debug_body).contains(&document_root.display().to_string()));

        fs::remove_dir_all(browser_root).expect("remove browser root");
        fs::remove_dir_all(document_root).expect("remove document root");
    }

    #[tokio::test]
    async fn v1_errors_have_stable_codes_and_legacy_routes_are_deprecated() {
        let root = temp_browser_root("v1-error-contract");
        fs::write(root.join("index.html"), "OKF").expect("write browser index");
        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: vec![DocumentRoot::new(&root)],
            environment_roots_active: false,
            env_file: root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });
        let error = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/document?path=..%2FCargo.toml")
                    .body(Body::empty())
                    .expect("error request"),
            )
            .await
            .expect("error response");
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(error.into_body(), usize::MAX)
            .await
            .expect("read error");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).expect("parse error"),
            serde_json::json!({
                "api_version": "v1",
                "error": {
                    "code": "bad_request",
                    "message": "invalid document path"
                }
            })
        );

        let legacy = router
            .oneshot(
                Request::builder()
                    .uri("/api/okf/documents")
                    .body(Body::empty())
                    .expect("legacy request"),
            )
            .await
            .expect("legacy response");
        assert_eq!(legacy.headers().get("deprecation").unwrap(), "true");
        assert!(legacy.headers().contains_key("sunset"));

        fs::remove_dir_all(root).expect("remove root");
    }

    #[tokio::test]
    async fn scanlab_legacy_routes_require_explicit_compatibility_mode() {
        let browser_root = temp_browser_root("compat-browser");
        fs::write(browser_root.join("index.html"), "OKF").expect("write browser index");
        let document_root = temp_browser_root("compat-documents");
        fs::write(
            document_root.join("index.md"),
            "---\ntype: index\n---\n# Index\n",
        )
        .expect("write document");
        let config = |scanlab_compat| ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: browser_root.clone(),
            roots: vec![DocumentRoot::mounted("scanlab", &document_root)],
            environment_roots_active: false,
            env_file: browser_root.join(".env"),
            scanlab_compat,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        };

        let default_response = app(config(false))
            .oneshot(
                Request::builder()
                    .uri("/docs/index.md")
                    .body(Body::empty())
                    .expect("default request"),
            )
            .await
            .expect("default response");
        assert_eq!(default_response.status(), StatusCode::NOT_FOUND);

        let compatibility_response = app(config(true))
            .oneshot(
                Request::builder()
                    .uri("/docs/index.md")
                    .body(Body::empty())
                    .expect("compatibility request"),
            )
            .await
            .expect("compatibility response");
        assert_eq!(compatibility_response.status(), StatusCode::OK);

        fs::remove_dir_all(browser_root).expect("remove browser root");
        fs::remove_dir_all(document_root).expect("remove document root");
    }

    #[tokio::test]
    async fn duplicate_mount_fallback_inventory_paths_resolve_selected_documents() {
        let browser_root = temp_browser_root("fallback-browser");
        fs::write(browser_root.join("index.html"), "OKF").expect("write browser index");
        let primary = temp_browser_root("fallback-primary");
        let fallback = temp_browser_root("fallback-secondary");
        fs::write(
            fallback.join("index.md"),
            "---\ntype: index\ntitle: Fallback\n---\n# Fallback\n",
        )
        .expect("write fallback document");
        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: browser_root.clone(),
            roots: vec![
                DocumentRoot::mounted("knowledge", &primary),
                DocumentRoot::mounted("knowledge", &fallback),
            ],
            environment_roots_active: false,
            env_file: browser_root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });

        let inventory = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/documents")
                    .body(Body::empty())
                    .expect("inventory request"),
            )
            .await
            .expect("inventory response");
        let body = to_bytes(inventory.into_body(), usize::MAX)
            .await
            .expect("read inventory");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("parse inventory");
        assert_eq!(payload["data"]["documents"][0]["root_index"], 1);
        assert_eq!(
            payload["data"]["documents"][0]["browser_path"],
            "/okf-docs/knowledge/index.md"
        );

        let document = router
            .oneshot(
                Request::builder()
                    .uri("/okf-docs/knowledge/index.md")
                    .body(Body::empty())
                    .expect("document request"),
            )
            .await
            .expect("document response");
        assert_eq!(document.status(), StatusCode::OK);

        fs::remove_dir_all(browser_root).expect("remove browser root");
        fs::remove_dir_all(primary).expect("remove primary root");
        fs::remove_dir_all(fallback).expect("remove fallback root");
    }

    #[tokio::test]
    async fn same_physical_root_under_different_mounts_reports_selected_logical_mount() {
        let browser_root = temp_browser_root("duplicate-physical-browser");
        fs::write(browser_root.join("index.html"), "OKF").expect("write browser index");
        let document_root = temp_browser_root("duplicate-physical-documents");
        fs::write(
            document_root.join("index.md"),
            "---\ntype: index\ntitle: Shared\n---\n# Shared\n",
        )
        .expect("write shared document");
        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: browser_root.clone(),
            roots: vec![
                DocumentRoot::mounted("first", &document_root),
                DocumentRoot::mounted("second", &document_root),
            ],
            environment_roots_active: false,
            env_file: browser_root.join(".env"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });

        let inventory = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/documents")
                    .body(Body::empty())
                    .expect("inventory request"),
            )
            .await
            .expect("inventory response");
        assert_eq!(inventory.status(), StatusCode::OK);
        let body = to_bytes(inventory.into_body(), usize::MAX)
            .await
            .expect("read inventory");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("parse inventory");
        let document = &payload["data"]["documents"][0];
        let mount = document["root_mount"].as_str().expect("root mount");
        assert!(document["logical_path"]
            .as_str()
            .expect("logical path")
            .starts_with(&format!("{mount}/")));
        assert!(document["browser_path"]
            .as_str()
            .expect("browser path")
            .starts_with(&format!("/okf-docs/{mount}/")));

        fs::remove_dir_all(browser_root).expect("remove browser root");
        fs::remove_dir_all(document_root).expect("remove document root");
    }

    #[test]
    fn resolves_documents_from_named_mounts() {
        let root = temp_browser_root("document-mount");
        fs::write(root.join("index.md"), "# Mounted").expect("write mounted document");
        let roots = vec![DocumentRoot::mounted("okf", &root)];

        let resolved = resolve_admitted_document_file(&roots, "okf", "index.md")
            .expect("resolve mounted document");
        assert_eq!(
            resolved.path,
            root.join("index.md")
                .canonicalize()
                .expect("canonical document")
        );

        fs::remove_dir_all(root).expect("remove temp document root");
    }

    #[test]
    fn missing_document_mounts_are_not_served_from_repository_root() {
        let root = temp_browser_root("missing-mount");
        fs::write(root.join("index.md"), "# Mounted").expect("write mounted document");
        let roots = vec![DocumentRoot::mounted("okf", &root)];

        assert_eq!(
            resolve_admitted_document_file(&roots, "scanlab", "index.md"),
            Err(StaticFileError::Missing)
        );

        fs::remove_dir_all(root).expect("remove temp document root");
    }

    #[test]
    fn document_mounts_reject_parent_directory_components() {
        let root = temp_browser_root("document-parent-dir");
        fs::write(root.join("index.md"), "# Mounted").expect("write mounted document");
        let roots = vec![DocumentRoot::mounted("okf", &root)];

        assert_eq!(
            resolve_admitted_document_file(&roots, "okf", "../Cargo.toml"),
            Err(StaticFileError::EscapesRoot)
        );

        fs::remove_dir_all(root).expect("remove temp document root");
    }

    #[test]
    fn unsafe_mount_names_are_rejected() {
        let root = temp_browser_root("unsafe-mount");
        let roots = vec![DocumentRoot::mounted("okf", &root)];

        assert_eq!(
            resolve_admitted_document_file(&roots, "../okf", "index.md"),
            Err(StaticFileError::EscapesRoot)
        );

        fs::remove_dir_all(root).expect("remove temp document root");
    }

    #[test]
    fn repository_file_route_has_a_small_allowlist() {
        assert!(is_allowed_repo_file("README.md"));
        assert!(is_allowed_repo_file("README.de.md"));
        assert!(is_allowed_repo_file("HOSTS.md"));
        assert!(!is_allowed_repo_file("Cargo.toml"));
        assert!(!is_allowed_repo_file("docs/index.md"));
    }

    #[test]
    fn api_document_path_accepts_logical_and_browser_paths() {
        assert_eq!(
            normalize_api_document_path("okf/index.md"),
            Some(PathBuf::from("okf/index.md"))
        );
        assert_eq!(
            normalize_api_document_path("/okf-docs/scanlab/knowledge/index.md?x=1#top"),
            Some(PathBuf::from("scanlab/knowledge/index.md"))
        );
    }

    #[test]
    fn api_document_path_rejects_escape_attempts() {
        assert_eq!(normalize_api_document_path("../Cargo.toml"), None);
        assert_eq!(
            normalize_api_document_path("/okf-docs/scanlab/../Cargo.toml"),
            None
        );
    }

    #[test]
    fn markdown_link_targets_resolve_relative_to_logical_document_path() {
        let source = "See [target](../target.md), [section](./detail.md#part), and [web](https://example.invalid).";
        let targets =
            extract_markdown_link_targets(source, Path::new("scanlab/knowledge/current.md"));

        assert_eq!(
            targets,
            vec![
                PathBuf::from("scanlab/target.md"),
                PathBuf::from("scanlab/knowledge/detail.md")
            ]
        );
    }

    #[test]
    fn dotenv_parser_supports_voyage_configuration_keys() {
        let source = r#"
OKF_VOYAGE_MODEL=voyage-test
OKF_VOYAGE_TPM_LIMIT=123
OKF_VOYAGE_RPM_LIMIT=45
"#;

        assert_eq!(
            dotenv_value_from_source(source, "OKF_VOYAGE_MODEL"),
            Some(OsString::from("voyage-test"))
        );
        assert_eq!(
            dotenv_value_from_source(source, "OKF_VOYAGE_TPM_LIMIT"),
            Some(OsString::from("123"))
        );
        assert_eq!(
            dotenv_value_from_source(source, "OKF_VOYAGE_RPM_LIMIT"),
            Some(OsString::from("45"))
        );
    }

    #[tokio::test]
    async fn monitored_routes_withhold_added_and_modified_documents_until_acceptance() {
        let root = temp_browser_root("monitored-serving");
        let documents = root.join("documents");
        fs::create_dir(&documents).unwrap();
        fs::write(
            documents.join("index.md"),
            "---\nokf_root_id: urn:okf:root:01JZY3MONITORAAAAAAAAAAAA\ntype: index\n---\n# Root\n",
        )
        .unwrap();
        fs::write(
            documents.join("note.md"),
            "---\ntitle: Note\ntype: concept\n---\n# Note\n",
        )
        .unwrap();
        let configured = BrowserRoot {
            root_id: RootId::parse("urn:okf:root:01JZY3MONITORAAAAAAAAAAAA").unwrap(),
            mount: Some("knowledge".to_string()),
            path: documents.clone(),
            enabled: true,
            priority: 0,
            check_for_changes: true,
        };
        let mut browser_config = okf::BrowserConfig::default();
        browser_config.roots_mut().push(configured.clone());
        let config_path = root.join("config.toml");
        save_browser_config(&config_path, &browser_config).unwrap();
        let router = app(ServerConfig {
            mode: ServerMode::ReadOnly,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: vec![DocumentRoot::mounted("knowledge", &documents)],
            environment_roots_active: false,
            env_file: config_path,
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });
        let monitor = RootMonitor::open(&root.join("state")).unwrap();
        for _ in 0..100 {
            if monitor.status(std::slice::from_ref(&configured)).unwrap()[0].initialized {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(monitor.status(std::slice::from_ref(&configured)).unwrap()[0].initialized);

        let admitted = router
            .clone()
            .oneshot(
                Request::get("/okf-docs/knowledge/note.md")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(admitted.status(), StatusCode::OK);
        fs::write(
            documents.join("note.md"),
            "---\ntitle: Modified\ntype: concept\n---\n# Modified\n",
        )
        .unwrap();
        let modified = router
            .clone()
            .oneshot(
                Request::get("/okf-docs/knowledge/note.md")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(modified.status(), StatusCode::CONFLICT);
        fs::write(
            documents.join("added.md"),
            "---\ntitle: Added\ntype: concept\n---\n# Added\n",
        )
        .unwrap();
        let added = router
            .oneshot(
                Request::get("/okf-docs/knowledge/added.md")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(added.status(), StatusCode::CONFLICT);
    }

    #[cfg(any())]
    #[tokio::test]
    async fn persistent_root_api_writes_xdg_config_and_never_mutates_dotenv() {
        let root = temp_browser_root("root-config-api");
        let dotenv_source = "OKF_VOYAGE_API_KEY=keep-secret\nOTHER=keep\n";
        fs::write(root.join(".env"), dotenv_source).expect("write dotenv");
        for (directory, root_id) in [
            ("docs", "urn:okf:root:01JZY3DOCSAAAAAAAAAAAAAAA"),
            ("scql-docs", "urn:okf:root:01JZY3SCQLAAAAAAAAAAAAAAA"),
        ] {
            let path = root.join(directory);
            fs::create_dir_all(&path).expect("create document root");
            fs::write(
                path.join("index.md"),
                format!("---\nokf_root_id: {root_id}\ntype: Index\n---\n# Index\n"),
            )
            .expect("write root index");
        }
        let config_path = root.join("config.toml");
        let add_body = serde_json::json!({
            "spec": format!("okf={}", root.join("docs").display())
        })
        .to_string();
        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: Vec::new(),
            environment_roots_active: false,
            env_file: config_path.clone(),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });

        let denied = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/okf/config/roots")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(add_body.clone()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

        let missing_confirmation = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/okf/config/roots")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(SESSION_TOKEN_HEADER, "test-session-token-0000000000000000")
                    .body(Body::from(add_body.clone()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(missing_confirmation.status(), StatusCode::FORBIDDEN);

        for (header_name, header_value) in [
            (header::ORIGIN.as_str(), "https://attacker.invalid"),
            ("sec-fetch-site", "cross-site"),
        ] {
            let cross_site = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/okf/config/roots")
                        .header(header::CONTENT_TYPE, "application/json")
                        .header(SESSION_TOKEN_HEADER, "test-session-token-0000000000000000")
                        .header("x-okf-config-write", "confirm")
                        .header(header_name, header_value)
                        .body(Body::from(r#"{"spec":"attacker=outside"}"#))
                        .expect("cross-site request"),
                )
                .await
                .expect("cross-site response");
            assert_eq!(cross_site.status(), StatusCode::FORBIDDEN);
        }
        assert_eq!(
            fs::read_to_string(root.join(".env")).expect("read unchanged dotenv"),
            dotenv_source
        );
        assert!(!config_path.exists());

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/okf/config/roots")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(SESSION_TOKEN_HEADER, "test-session-token-0000000000000000")
                    .header("x-okf-config-write", "confirm")
                    .body(Body::from(add_body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let config = load_browser_config(&config_path).expect("load browser config");
        assert_eq!(config.roots().len(), 1);
        assert_eq!(
            config.roots()[0].root_id.as_str(),
            "urn:okf:root:01JZY3DOCSAAAAAAAAAAAAAAA"
        );
        assert_eq!(config.roots()[0].mount.as_deref(), Some("okf"));
        assert_eq!(
            fs::read_to_string(root.join(".env")).expect("read unchanged dotenv"),
            dotenv_source
        );

        let edit_body = serde_json::json!({
            "spec": format!("scql={}", root.join("scql-docs").display())
        })
        .to_string();

        let edited = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/okf/config/roots/0")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(SESSION_TOKEN_HEADER, "test-session-token-0000000000000000")
                    .header("x-okf-config-write", "confirm")
                    .body(Body::from(edit_body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(edited.status(), StatusCode::OK);
        let restarted = load_browser_config(&config_path).expect("reload browser config");
        assert_eq!(restarted.roots().len(), 1);
        assert_eq!(
            restarted.roots()[0].root_id.as_str(),
            "urn:okf:root:01JZY3SCQLAAAAAAAAAAAAAAA"
        );
        assert_eq!(restarted.roots()[0].mount.as_deref(), Some("scql"));

        let removed = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/okf/config/roots/0")
                    .header(SESSION_TOKEN_HEADER, "test-session-token-0000000000000000")
                    .header("x-okf-config-write", "confirm")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(removed.status(), StatusCode::OK);
        let removed_config = load_browser_config(&config_path).expect("reload empty config");
        assert!(removed_config.roots().is_empty());
        assert_eq!(
            fs::read_to_string(root.join(".env")).expect("read unchanged dotenv"),
            dotenv_source
        );

        let listed = router
            .oneshot(
                Request::builder()
                    .uri("/api/okf/config/roots")
                    .header(SESSION_TOKEN_HEADER, "test-session-token-0000000000000000")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(listed.status(), StatusCode::OK);

        fs::remove_dir_all(root).expect("remove root");
    }

    #[tokio::test]
    async fn authorized_root_api_is_proposal_bound_revision_safe_and_never_writes_dotenv() {
        let root = temp_browser_root("authorized-root-api");
        fs::write(root.join("index.html"), "OKF").expect("browser index");
        let dotenv_source = "OKF_VOYAGE_API_KEY=keep-secret\nOTHER=keep\n";
        fs::write(root.join(".env"), dotenv_source).expect("dotenv");
        let documents = root.join("documents");
        fs::create_dir(&documents).expect("document root");
        fs::write(
            documents.join("index.md"),
            "---\nokf_root_id: urn:okf:root:01JZY3APIROOTAAAAAAAAAAAA\n---\n# Root\n",
        )
        .expect("root index");
        fs::write(documents.join("concept.md"), "# Concept\n").expect("concept");
        let config_path = root.join("config.toml");
        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: Some("1111-2222-3333".to_string()),
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: Vec::new(),
            environment_roots_active: false,
            env_file: config_path.clone(),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });
        let proposal_body = serde_json::json!({
            "path": documents,
            "mount": "knowledge",
            "operation": "registration",
            "priority": 300,
            "check_for_changes": true
        })
        .to_string();

        let anonymous = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/roots/proposals")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(proposal_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

        let transitional = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/roots/proposals")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(SESSION_TOKEN_HEADER, "test-session-token-0000000000000000")
                    .body(Body::from(proposal_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(transitional.status(), StatusCode::FORBIDDEN);
        let (cookie, csrf) = local_pair(&router, "1111-2222-3333").await;

        let cross_origin = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/roots/proposals")
                    .header(header::HOST, "127.0.0.1:8003")
                    .header(header::ORIGIN, "https://attacker.invalid")
                    .header("sec-fetch-site", "cross-site")
                    .header(header::COOKIE, &cookie)
                    .header(CSRF_TOKEN_HEADER, &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(proposal_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cross_origin.status(), StatusCode::FORBIDDEN);

        let proposal_response = router
            .clone()
            .oneshot(local_authorized_request(
                "POST",
                "/api/v1/roots/proposals",
                &cookie,
                &csrf,
                proposal_body.clone(),
            ))
            .await
            .unwrap();
        assert_eq!(proposal_response.status(), StatusCode::OK);
        let payload: serde_json::Value = serde_json::from_slice(
            &to_bytes(proposal_response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let proposal_id = payload["data"]["id"].as_str().unwrap();
        let proposal_digest = payload["data"]["proposal_digest"].as_str().unwrap();
        assert_eq!(payload["data"]["registration"]["priority"], 300);
        let canonical_documents = documents.canonicalize().expect("canonical documents path");
        assert_eq!(
            payload["data"]["canonical_root"],
            canonical_documents.display().to_string()
        );

        let configuration = router
            .clone()
            .oneshot(local_authorized_request(
                "GET",
                "/api/v1/roots/configuration",
                &cookie,
                &csrf,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(configuration.status(), StatusCode::OK);
        let configuration: serde_json::Value = serde_json::from_slice(
            &to_bytes(configuration.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let revision = configuration["data"]["revision"].as_str().unwrap();
        let registration_body = serde_json::json!({
            "proposal_digest": proposal_digest,
            "expected_revision": revision
        })
        .to_string();

        let missing_confirmation = router
            .clone()
            .oneshot(local_authorized_request(
                "POST",
                format!("/api/v1/roots/proposals/{proposal_id}/registration"),
                &cookie,
                &csrf,
                registration_body.clone(),
            ))
            .await
            .unwrap();
        assert_eq!(
            missing_confirmation.status(),
            StatusCode::PRECONDITION_REQUIRED
        );

        let registered = router
            .clone()
            .oneshot({
                let mut request = local_authorized_request(
                    "POST",
                    format!("/api/v1/roots/proposals/{proposal_id}/registration"),
                    &cookie,
                    &csrf,
                    registration_body,
                );
                request
                    .headers_mut()
                    .insert("x-okf-config-write", "confirm".parse().unwrap());
                request
            })
            .await
            .unwrap();
        assert_eq!(registered.status(), StatusCode::OK);
        let registered: serde_json::Value =
            serde_json::from_slice(&to_bytes(registered.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let registered_revision = registered["data"]["revision"].as_str().unwrap();
        let config = load_browser_config(&config_path).unwrap();
        assert_eq!(config.roots()[0].priority, 300);
        assert!(config.roots()[0].check_for_changes);
        assert_eq!(
            fs::read_to_string(root.join(".env")).unwrap(),
            dotenv_source
        );

        let monitor_uri = "/api/v1/roots/urn:okf:root:01JZY3APIROOTAAAAAAAAAAAA/monitoring";
        let baseline = router
            .clone()
            .oneshot(local_authorized_request(
                "POST",
                format!("{monitor_uri}/check"),
                &cookie,
                &csrf,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(baseline.status(), StatusCode::OK);
        fs::write(
            documents.join("added.md"),
            "---\ntitle: Added\ntype: concept\n---\n# Added\n",
        )
        .unwrap();
        let detected = router
            .clone()
            .oneshot(local_authorized_request(
                "POST",
                format!("{monitor_uri}/check"),
                &cookie,
                &csrf,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(detected.status(), StatusCode::OK);
        let detected: serde_json::Value =
            serde_json::from_slice(&to_bytes(detected.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert!(detected["data"]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|change| change["kind"] == "added" && change["path"] == "added.md"));
        let stale_digest = detected["data"]["snapshot_digest"].as_str().unwrap();
        let details = router
            .clone()
            .oneshot(local_authorized_request(
                "GET",
                format!("{monitor_uri}/pending"),
                &cookie,
                &csrf,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(details.status(), StatusCode::OK);
        fs::write(
            documents.join("added.md"),
            "---\ntitle: Changed\ntype: concept\n---\n# Changed\n",
        )
        .unwrap();
        let stale_accept = router
            .clone()
            .oneshot({
                let mut request = local_authorized_request(
                    "POST",
                    format!("{monitor_uri}/accept"),
                    &cookie,
                    &csrf,
                    serde_json::json!({"snapshot_digest": stale_digest}).to_string(),
                );
                request
                    .headers_mut()
                    .insert("x-okf-change-review", "accept".parse().unwrap());
                request
            })
            .await
            .unwrap();
        assert_eq!(stale_accept.status(), StatusCode::CONFLICT);
        let refreshed = router
            .clone()
            .oneshot(local_authorized_request(
                "POST",
                format!("{monitor_uri}/check"),
                &cookie,
                &csrf,
                Body::empty(),
            ))
            .await
            .unwrap();
        let refreshed: serde_json::Value =
            serde_json::from_slice(&to_bytes(refreshed.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let digest = refreshed["data"]["snapshot_digest"].as_str().unwrap();
        let accepted = router
            .clone()
            .oneshot({
                let mut request = local_authorized_request(
                    "POST",
                    format!("{monitor_uri}/accept"),
                    &cookie,
                    &csrf,
                    serde_json::json!({"snapshot_digest": digest}).to_string(),
                );
                request
                    .headers_mut()
                    .insert("x-okf-change-review", "accept".parse().unwrap());
                request
            })
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::OK);

        let stale_revision_update = router
            .clone()
            .oneshot({
                let mut request = local_authorized_request(
                    "PUT",
                    "/api/v1/roots/urn:okf:root:01JZY3APIROOTAAAAAAAAAAAA",
                    &cookie,
                    &csrf,
                    serde_json::json!({"expected_revision": revision, "priority": 900}).to_string(),
                );
                request
                    .headers_mut()
                    .insert("x-okf-config-write", "confirm".parse().unwrap());
                request
            })
            .await
            .unwrap();
        assert_eq!(stale_revision_update.status(), StatusCode::CONFLICT);

        let updated = router
            .clone()
            .oneshot({
                let mut request = local_authorized_request(
                    "PUT",
                    "/api/v1/roots/urn:okf:root:01JZY3APIROOTAAAAAAAAAAAA",
                    &cookie,
                    &csrf,
                    serde_json::json!({
                        "expected_revision": registered_revision,
                        "priority": 900,
                        "check_for_changes": false
                    })
                    .to_string(),
                );
                request
                    .headers_mut()
                    .insert("x-okf-config-write", "confirm".parse().unwrap());
                request
            })
            .await
            .unwrap();
        assert_eq!(updated.status(), StatusCode::OK);
        let updated: serde_json::Value =
            serde_json::from_slice(&to_bytes(updated.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let updated_revision = updated["data"]["revision"].as_str().unwrap();
        assert_eq!(
            load_browser_config(&config_path).unwrap().roots()[0].priority,
            900
        );

        let removed = router
            .clone()
            .oneshot({
                let mut request = local_authorized_request(
                    "DELETE",
                    format!(
                        "/api/v1/roots/urn:okf:root:01JZY3APIROOTAAAAAAAAAAAA?expected_revision={updated_revision}"
                    ),
                    &cookie,
                    &csrf,
                    Body::empty(),
                );
                request
                    .headers_mut()
                    .insert("x-okf-config-write", "confirm".parse().unwrap());
                request
            })
            .await
            .unwrap();
        assert_eq!(removed.status(), StatusCode::OK);
        assert!(load_browser_config(&config_path)
            .unwrap()
            .roots()
            .is_empty());
        assert!(documents.join("concept.md").exists());

        let retired = router
            .oneshot(local_authorized_request(
                "POST",
                "/api/okf/config/roots",
                &cookie,
                &csrf,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(retired.status(), StatusCode::GONE);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn initialization_api_requires_its_own_plan_confirmation_and_rejects_stale_sources() {
        let root = temp_browser_root("initialization-root-api");
        fs::write(root.join("index.html"), "OKF").unwrap();
        let documents = root.join("documents");
        fs::create_dir(&documents).unwrap();
        fs::write(
            documents.join("index.md"),
            "---\nokf_root_id: urn:okf:root:01JZY3INITAPIAAAAAAAAAAAA\n---\n# Root\n",
        )
        .unwrap();
        fs::write(documents.join("concept.md"), "# Concept\n").unwrap();
        let config_path = root.join("config.toml");
        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: Some("4444-5555-6666".to_string()),
            tls: None,
            host: DEFAULT_HOST.to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: Vec::new(),
            environment_roots_active: false,
            env_file: config_path.clone(),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: false,
            trusted_proxy: None,
            expose_physical_paths: false,
        });
        let proposal_body = serde_json::json!({
            "path": documents,
            "mount": "knowledge",
            "operation": "source_initialization"
        })
        .to_string();
        let (cookie, csrf) = local_pair(&router, "4444-5555-6666").await;

        async fn proposal_and_plan(
            router: &Router,
            proposal_body: &str,
            cookie: &str,
            csrf: &str,
        ) -> (String, String, String) {
            let response = router
                .clone()
                .oneshot(local_authorized_request(
                    "POST",
                    "/api/v1/roots/proposals",
                    cookie,
                    csrf,
                    proposal_body.to_string(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let proposal: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                    .unwrap();
            let id = proposal["data"]["id"].as_str().unwrap().to_string();
            let digest = proposal["data"]["proposal_digest"]
                .as_str()
                .unwrap()
                .to_string();
            let response = router
                .clone()
                .oneshot(local_authorized_request(
                    "POST",
                    format!("/api/v1/roots/proposals/{id}/initialization/plan"),
                    cookie,
                    csrf,
                    serde_json::json!({"proposal_digest": digest}).to_string(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let plan: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                    .unwrap();
            assert!(plan["data"]["changes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|change| change["path"] == "concept.md" && change["diff"].is_string()));
            let plan_digest = plan["data"]["plan_digest"].as_str().unwrap().to_string();
            (id, digest, plan_digest)
        }

        let (stale_id, stale_digest, stale_plan) =
            proposal_and_plan(&router, &proposal_body, &cookie, &csrf).await;
        fs::write(documents.join("concept.md"), "# Changed\n").unwrap();
        let stale = router
            .clone()
            .oneshot({
                let mut request = local_authorized_request(
                    "POST",
                    format!("/api/v1/roots/proposals/{stale_id}/initialization"),
                    &cookie,
                    &csrf,
                    serde_json::json!({
                        "proposal_digest": stale_digest,
                        "plan_digest": stale_plan
                    })
                    .to_string(),
                );
                request
                    .headers_mut()
                    .insert("x-okf-source-write", "confirm".parse().unwrap());
                request
            })
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::CONFLICT);

        let (id, digest, plan_digest) =
            proposal_and_plan(&router, &proposal_body, &cookie, &csrf).await;
        let missing_confirmation = router
            .clone()
            .oneshot(local_authorized_request(
                "POST",
                format!("/api/v1/roots/proposals/{id}/initialization"),
                &cookie,
                &csrf,
                serde_json::json!({
                    "proposal_digest": digest,
                    "plan_digest": plan_digest
                })
                .to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(
            missing_confirmation.status(),
            StatusCode::PRECONDITION_REQUIRED
        );

        let initialized = router
            .clone()
            .oneshot({
                let mut request = local_authorized_request(
                    "POST",
                    format!("/api/v1/roots/proposals/{id}/initialization"),
                    &cookie,
                    &csrf,
                    serde_json::json!({
                        "proposal_digest": digest,
                        "plan_digest": plan_digest
                    })
                    .to_string(),
                );
                request
                    .headers_mut()
                    .insert("x-okf-source-write", "confirm".parse().unwrap());
                request
            })
            .await
            .unwrap();
        assert_eq!(initialized.status(), StatusCode::OK);
        assert!(fs::read_to_string(documents.join("concept.md"))
            .unwrap()
            .contains("type: Concept"));
        assert!(!config_path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn remote_deployments_cannot_create_or_mutate_root_proposals() {
        let root = temp_browser_root("remote-root-api");
        fs::write(root.join("index.html"), "OKF").unwrap();
        let router = app(ServerConfig {
            mode: ServerMode::LocalEditor,
            pairing_code: None,
            tls: None,
            host: "0.0.0.0".to_string(),
            port: 8003,
            browser_root: root.clone(),
            roots: Vec::new(),
            environment_roots_active: false,
            env_file: root.join("config.toml"),
            scanlab_compat: false,
            session_token: "test-session-token-0000000000000000".to_string(),
            remote_access: true,
            trusted_proxy: None,
            expose_physical_paths: false,
        });
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/roots/proposals")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(SESSION_TOKEN_HEADER, "test-session-token-0000000000000000")
                    .body(Body::from(
                        serde_json::json!({
                            "path": root,
                            "operation": "registration"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn voyage_report_status_maps_common_failures() {
        assert_eq!(
            voyage_report_status(&ConnectivityReport::failure(
                None,
                Some("missing_api_key".to_string()),
                "missing"
            )),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            voyage_report_status(&ConnectivityReport::failure(
                Some(429),
                Some("rate_limit_exceeded".to_string()),
                "too many requests"
            )),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            voyage_report_status(&ConnectivityReport::failure(
                Some(503),
                Some("provider_unavailable".to_string()),
                "unavailable"
            )),
            StatusCode::BAD_GATEWAY
        );
    }

    fn inventory_document_fixture(root: &Path, logical_path: &str) -> InventoryDocument {
        InventoryDocument {
            logical_path: PathBuf::from(logical_path),
            physical_path: root.join(logical_path),
            title: "Document".to_string(),
            document_type: Some("Concept".to_string()),
            kind: Some("knowledge-document".to_string()),
            topic: Some("demo".to_string()),
            status: Some("active".to_string()),
            tags: vec!["one".to_string()],
            bytes: 42,
            content_hash: "document-hash".to_string(),
            estimated_tokens: 12,
        }
    }

    fn chunk_fixture(id: &str, document_path: &str, hash: &str, content: &str) -> Chunk {
        Chunk {
            id: id.to_string(),
            document_path: PathBuf::from(document_path),
            title: "Document".to_string(),
            document_type: Some("Concept".to_string()),
            kind: Some("knowledge-document".to_string()),
            topic: Some("demo".to_string()),
            status: Some("active".to_string()),
            tags: vec!["one".to_string()],
            heading_path: vec!["Heading".to_string()],
            content: content.to_string(),
            content_hash: hash.to_string(),
            estimated_tokens: 5,
        }
    }

    fn create_released_sqlite_schema(path: &Path, version: i64) {
        let connection = Connection::open(path).expect("create released schema fixture");
        connection
            .execute_batch(
                "
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE documents (
                    logical_path TEXT PRIMARY KEY, physical_path TEXT NOT NULL,
                    title TEXT NOT NULL, document_type TEXT, kind TEXT, topic TEXT,
                    status TEXT, content_hash TEXT NOT NULL,
                    estimated_tokens INTEGER NOT NULL, bytes INTEGER NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE chunks (
                    id TEXT PRIMARY KEY, document_path TEXT NOT NULL, title TEXT NOT NULL,
                    document_type TEXT, kind TEXT, topic TEXT, status TEXT,
                    heading_path_json TEXT NOT NULL, content_hash TEXT NOT NULL,
                    estimated_tokens INTEGER NOT NULL, content TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE embeddings (
                    chunk_id TEXT PRIMARY KEY, provider TEXT NOT NULL, model TEXT NOT NULL,
                    dimensions INTEGER NOT NULL, vector_json TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE suggested_edges (
                    id TEXT PRIMARY KEY, provider TEXT NOT NULL, model TEXT NOT NULL,
                    generation_method TEXT NOT NULL, ai_generated INTEGER NOT NULL,
                    source_chunk TEXT NOT NULL, target_chunk TEXT NOT NULL,
                    score REAL NOT NULL, created_at TEXT NOT NULL, status TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                INSERT INTO schema_migrations (version) VALUES (1);
                ",
            )
            .expect("create version 1 schema");
        if version >= 2 {
            connection
                .execute_batch(
                    "
                    CREATE TABLE applied_relations (
                        suggestion_id TEXT PRIMARY KEY, source_document TEXT NOT NULL,
                        target_document TEXT NOT NULL, source_path TEXT NOT NULL,
                        provider TEXT NOT NULL, model TEXT NOT NULL,
                        generation_method TEXT NOT NULL, ai_generated INTEGER NOT NULL,
                        score REAL NOT NULL,
                        applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                    );
                    INSERT INTO schema_migrations (version) VALUES (2);
                    ",
                )
                .expect("create version 2 schema");
        }
        if version >= 3 {
            connection
                .execute_batch(
                    "
                    ALTER TABLE chunks ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]';
                    CREATE TABLE index_state (
                        singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                        generation INTEGER NOT NULL DEFAULT 0,
                        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                    );
                    INSERT INTO index_state (singleton, generation) VALUES (1, 0);
                    INSERT INTO schema_migrations (version) VALUES (3);
                    ",
                )
                .expect("create version 3 schema");
        }
    }

    fn embedded_index(chunks: &[Chunk]) -> LocalIndex {
        LocalIndex {
            embeddings: chunks
                .iter()
                .enumerate()
                .map(|(position, chunk)| EmbeddedChunk {
                    chunk: chunk.clone(),
                    provider: "voyage-ai".to_string(),
                    model: "voyage-test".to_string(),
                    embedding: vec![position as f32 + 1.0, 1.0],
                })
                .collect(),
            suggestions: Vec::new(),
        }
    }

    #[test]
    fn sqlite_working_index_creates_and_migrates_database() {
        let root = temp_browser_root("sqlite-create");

        let sqlite = SqliteWorkingIndex::open(&root).expect("open sqlite");
        let summary = sqlite.storage_summary(false);

        assert_eq!(sqlite.path(), &root.join("okf.sqlite"));
        assert!(sqlite.path().is_file());
        assert_eq!(summary.schema_version, CURRENT_SQLITE_SCHEMA_VERSION);
        assert_eq!(summary.documents, 0);
        assert_eq!(summary.chunks, 0);
        assert_eq!(summary.embeddings, 0);

        fs::remove_dir_all(root).expect("remove sqlite root");
    }

    #[test]
    fn every_released_sqlite_schema_migrates_incrementally() {
        for version in [1, 2, 3] {
            let root = temp_browser_root(&format!("sqlite-migrate-v{version}"));
            create_released_sqlite_schema(&root.join("okf.sqlite"), version);
            if version == 2 {
                let connection = Connection::open(root.join("okf.sqlite")).expect("v2 fixture");
                connection
                    .execute_batch(
                        "
                        INSERT INTO documents (
                            logical_path, physical_path, title, content_hash,
                            estimated_tokens, bytes
                        ) VALUES ('okf/doc.md', '/tmp/doc.md', 'Document', 'doc-hash', 1, 1);
                        INSERT INTO chunks (
                            id, document_path, title, heading_path_json, content_hash,
                            estimated_tokens, content
                        ) VALUES ('chunk-a', 'okf/doc.md', 'Document', '[]', 'hash-a', 1, 'alpha');
                        INSERT INTO embeddings (
                            chunk_id, provider, model, dimensions, vector_json, content_hash
                        ) VALUES ('chunk-a', 'voyage-ai', 'voyage-test', 2, '[1.0,0.0]', 'hash-a');
                        ",
                    )
                    .expect("v2 fixture data");
            }
            if version == 3 {
                let connection = Connection::open(root.join("okf.sqlite")).expect("v3 fixture");
                connection
                    .execute_batch(
                        "
                        INSERT INTO documents (
                            logical_path, physical_path, title, content_hash,
                            estimated_tokens, bytes
                        ) VALUES ('okf/doc.md', '/tmp/doc.md', 'Document', 'doc-hash', 1, 1);
                        INSERT INTO chunks (
                            id, document_path, title, heading_path_json, content_hash,
                            estimated_tokens, content, tags_json
                        ) VALUES ('chunk-a', 'okf/doc.md', 'Document', '[]', 'hash-a', 1, 'alpha', '[]');
                        INSERT INTO suggested_edges (
                            id, provider, model, generation_method, ai_generated,
                            source_chunk, target_chunk, score, created_at, status
                        ) VALUES (
                            'pending', 'okf-browser-derived', 'pending-voyage-ai',
                            'browser_graph_prefilter', 1, 'chunk-a', 'chunk-a', 0.5,
                            '1', 'suggested'
                        );
                        ",
                    )
                    .expect("v3 placeholder fixture");
            }

            let sqlite = SqliteWorkingIndex::open(&root).expect("migrate released schema");

            assert_eq!(
                sqlite
                    .storage_counts()
                    .expect("storage counts")
                    .schema_version,
                CURRENT_SQLITE_SCHEMA_VERSION
            );
            assert_eq!(sqlite.generation().expect("generation"), 0);
            assert!(sqlite.integrity_violations().expect("integrity").is_empty());
            if version == 2 {
                assert_eq!(
                    sqlite
                        .load_local_index()
                        .expect("migrated data")
                        .embeddings
                        .len(),
                    1
                );
            }
            if version == 3 {
                assert_eq!(
                    table_count(&sqlite.connection().expect("connection"), "suggested_edges")
                        .expect("placeholder count"),
                    0
                );
            }
            fs::remove_dir_all(root).expect("remove migration root");
        }
    }

    #[test]
    fn direct_index_commit_populates_inventory_search_and_consistent_file_mirror() {
        let root = temp_browser_root("direct-index-commit");
        let document = inventory_document_fixture(&root, "okf/doc.md");
        let chunks = vec![
            chunk_fixture("chunk-a", "okf/doc.md", "hash-a", "alpha"),
            chunk_fixture("chunk-b", "okf/doc.md", "hash-b", "beta"),
        ];
        let index = embedded_index(&chunks);
        let report = ConnectivityReport::success(200, Some(2));

        persist_successful_voyage_index(&report, &[document], &chunks, &index, &root)
            .expect("direct index commit");

        let sqlite = SqliteWorkingIndex::open(&root).expect("restart sqlite");
        let counts = sqlite.storage_counts().expect("counts");
        assert_eq!(
            (counts.documents, counts.chunks, counts.embeddings),
            (1, 2, 2)
        );
        assert_eq!(sqlite.search(&[1.0, 1.0], 10).len(), 2);
        assert!(sqlite.integrity_violations().expect("integrity").is_empty());
        let integrity = inspect_index_integrity(&root).expect("mirror integrity");
        assert!(integrity.file_mirror_consistent);
        assert!(!integrity.recovery_required);
        fs::remove_dir_all(root).expect("remove direct index root");
    }

    #[test]
    fn voyage_suggestions_and_review_state_survive_restart_and_feed_apply_rows() {
        let root = temp_browser_root("persistent-review");
        let documents = vec![
            inventory_document_fixture(&root, "okf/alpha.md"),
            inventory_document_fixture(&root, "okf/beta.md"),
        ];
        let chunks = vec![
            chunk_fixture("okf/alpha.md#1-a", "okf/alpha.md", "hash-a", "alpha"),
            chunk_fixture("okf/beta.md#1-b", "okf/beta.md", "hash-b", "beta"),
        ];
        persist_successful_voyage_index(
            &ConnectivityReport::success(200, Some(2)),
            &documents,
            &chunks,
            &embedded_index(&chunks),
            &root,
        )
        .expect("indexed embeddings");
        let sqlite = SqliteWorkingIndex::open(&root).expect("sqlite");

        let (review_set, suggestions, generation) = sqlite
            .create_similarity_review_set(0.8)
            .expect("generate suggestions");
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().all(|suggestion| {
            suggestion.review_set_id == review_set.id
                && suggestion.provider == "voyage-ai"
                && suggestion.model == "voyage-test"
                && suggestion.generation_method == "embedding_similarity"
                && suggestion.ai_generated
                && suggestion.model != "pending-voyage-ai"
        }));
        mirror_sqlite_generation(&sqlite, &root, generation).expect("suggestion mirror");
        let suggestion_id = suggestions[0].id.clone();
        let (_, generation) = sqlite
            .set_suggestion_status(&suggestion_id, SuggestedEdgeStatus::Accepted)
            .expect("accept suggestion");
        mirror_sqlite_generation(&sqlite, &root, generation).expect("accepted mirror");
        drop(sqlite);

        let restarted = SqliteWorkingIndex::open(&root).expect("restart sqlite");
        let persisted = restarted
            .suggestions_for_review_set(&review_set.id)
            .expect("persisted suggestions");
        assert_eq!(persisted[0].status, SuggestedEdgeStatus::Accepted);
        let apply_rows = restarted.accepted_suggestions().expect("apply rows");
        assert_eq!(apply_rows.len(), 1);
        assert_eq!(apply_rows[0].id, suggestion_id);
        let file_index = LocalIndex::load(&root).expect("file mirror");
        assert_eq!(file_index.suggestions[0].review_set_id, review_set.id);
        assert_eq!(
            file_index.suggestions[0].status,
            SuggestedEdgeStatus::Accepted
        );

        let (changed, generation) = restarted
            .set_review_set_status(&review_set.id, SuggestedEdgeStatus::Denied)
            .expect("deny review set");
        assert_eq!(changed, persisted.len());
        mirror_sqlite_generation(&restarted, &root, generation).expect("denied mirror");
        assert!(restarted
            .suggestions_for_review_set(&review_set.id)
            .expect("denied suggestions")
            .iter()
            .all(|suggestion| suggestion.status == SuggestedEdgeStatus::Denied));
        let stored = restarted
            .suggestions_for_review_set(&review_set.id)
            .expect("stored suggestions");
        let imported = ImportReviewEdge {
            id: stored[0].id.clone(),
            provider: stored[0].provider.clone(),
            model: stored[0].model.clone(),
            generation_method: stored[0].generation_method.clone(),
            ai_generated: stored[0].ai_generated,
            source_chunk: stored[0].source_chunk.clone(),
            target_chunk: stored[0].target_chunk.clone(),
            score: stored[0].score,
            created_at: stored[0].created_at.clone(),
        };
        assert!(imported_edges_match(
            std::slice::from_ref(&imported),
            &stored
        ));
        let mut mismatched = imported;
        mismatched.model = "tampered-model".to_string();
        assert!(!imported_edges_match(&[mismatched], &stored));
        let (changed, generation) = restarted
            .accept_imported_suggestions(&review_set.id, &[stored[0].id.clone()])
            .expect("import accepted review");
        assert_eq!(changed, 1);
        mirror_sqlite_generation(&restarted, &root, generation).expect("import mirror");
        assert_eq!(
            restarted
                .accepted_suggestions()
                .expect("accepted import")
                .len(),
            1
        );
        fs::remove_dir_all(root).expect("remove review root");
    }

    #[test]
    fn browser_review_artifact_schema_is_strict_and_portable() {
        let source = r#"{
            "type":"okf-ai-edge-review",
            "review_set_id":"review-1",
            "accepted_edges":[{
                "id":"edge-1",
                "provider":"voyage-ai",
                "model":"voyage-test",
                "generation_method":"embedding_similarity",
                "ai_generated":true,
                "source_chunk":"okf/a.md#1-a",
                "target_chunk":"okf/b.md#1-b",
                "score":0.9,
                "created_at":"1"
            }]
        }"#;

        let artifact: ImportReviewArtifact = serde_json::from_str(source).expect("valid artifact");

        assert_eq!(artifact.artifact_type, "okf-ai-edge-review");
        assert_eq!(artifact.accepted_edges.len(), 1);
        let with_unknown =
            source.replacen("\"review_set_id\"", "\"unknown\":true,\"review_set_id\"", 1);
        assert!(serde_json::from_str::<ImportReviewArtifact>(&with_unknown).is_err());
    }

    #[test]
    fn failed_sqlite_transaction_keeps_previous_generation_and_file_snapshot() {
        let root = temp_browser_root("sqlite-rollback");
        let document = inventory_document_fixture(&root, "okf/doc.md");
        let chunks = vec![chunk_fixture("chunk-a", "okf/doc.md", "hash-a", "alpha")];
        let report = ConnectivityReport::success(200, Some(1));
        persist_successful_voyage_index(
            &report,
            std::slice::from_ref(&document),
            &chunks,
            &embedded_index(&chunks),
            &root,
        )
        .expect("initial commit");
        let before_embeddings = fs::read(root.join("embeddings.tsv")).expect("file snapshot");
        let before_generation = SqliteWorkingIndex::open(&root)
            .expect("sqlite")
            .generation()
            .expect("generation");
        let ghost = chunk_fixture("ghost", "okf/doc.md", "ghost-hash", "ghost");
        let invalid = embedded_index(&[ghost]);

        let error = persist_successful_voyage_index(&report, &[document], &chunks, &invalid, &root)
            .expect_err("orphan embedding must roll back");

        assert!(error.contains("SQLite index transaction"));
        let sqlite = SqliteWorkingIndex::open(&root).expect("reopen sqlite");
        assert_eq!(sqlite.generation().expect("generation"), before_generation);
        assert_eq!(
            fs::read(root.join("embeddings.tsv")).expect("unchanged file snapshot"),
            before_embeddings
        );
        assert!(
            inspect_index_integrity(&root)
                .expect("integrity")
                .file_mirror_consistent
        );
        fs::remove_dir_all(root).expect("remove rollback root");
    }

    #[test]
    fn damaged_file_mirror_is_reported_and_rebuilt_from_authoritative_sqlite() {
        let root = temp_browser_root("file-mirror-recovery");
        let document = inventory_document_fixture(&root, "okf/doc.md");
        let chunks = vec![chunk_fixture("chunk-a", "okf/doc.md", "hash-a", "alpha")];
        let report = ConnectivityReport::success(200, Some(1));
        persist_successful_voyage_index(
            &report,
            std::slice::from_ref(&document),
            &chunks,
            &embedded_index(&chunks),
            &root,
        )
        .expect("initial mirror");
        fs::write(root.join("embeddings.tsv"), "damaged\n").expect("damage file mirror");

        let damaged = inspect_index_integrity(&root).expect("damaged integrity");
        assert!(damaged.recovery_required);
        assert!(!damaged.file_mirror_consistent);
        let authoritative = load_existing_local_index(&root).expect("authoritative SQLite");
        assert_eq!(authoritative.embeddings.len(), 1);
        persist_successful_voyage_index(&report, &[document], &chunks, &authoritative, &root)
            .expect("repair mirror");

        let repaired = inspect_index_integrity(&root).expect("repaired integrity");
        assert!(!repaired.recovery_required);
        assert!(repaired.file_mirror_consistent);
        fs::remove_dir_all(root).expect("remove recovery root");
    }

    #[test]
    fn token_free_rebuild_restores_deleted_sqlite_from_current_documents_and_file_mirror() {
        let root = temp_browser_root("rebuild-deleted-sqlite");
        let documents_root = root.join("documents");
        let index_root = root.join("derived");
        fs::create_dir_all(&documents_root).expect("document root");
        fs::write(
            documents_root.join("doc.md"),
            "---\ntitle: Document\ntype: Concept\n---\n# Document\n\nAlpha knowledge.\n",
        )
        .expect("document");
        let repository = okf::Repository::open([DocumentRoot::mounted("okf", &documents_root)])
            .expect("repository");
        let chunks = chunk_repository(&repository).expect("chunks");
        embedded_index(&chunks)
            .save(&index_root)
            .expect("legacy file mirror");
        assert!(!index_root.join("okf.sqlite").exists());
        let config = VoyageConfig::from_lookup(|key| match key {
            "OKF_VOYAGE_MODEL" => Some("voyage-test".to_string()),
            "OKF_VOYAGE_INDEX_ROOT" => Some(index_root.display().to_string()),
            _ => None,
        });
        let state = AppState {
            mode: ServerMode::LocalEditor,
            tls_status: None,
            local_auth: LocalAuthState::new(None),
            persistent_auth: PersistentAuthState::new(),
            users: None,
            remote_access: false,
            trusted_proxy: None,
            expected_host: "127.0.0.1:8003".to_string(),
            browser_root: root.clone(),
            document_roots: vec![DocumentRoot::mounted("okf", &documents_root)],
            repo_root: root.clone(),
            environment_roots_active: false,
            env_file: root.join(".env"),
            root_proposals: Arc::new(Mutex::new(RootProposalStore::new(Duration::from_secs(900)))),
            root_state_dir: root.join("state"),
            root_monitor: None,
            index_jobs: IndexJobRegistry::default(),
            session_token: "test-session-token-0000000000000000".to_string(),
            expose_physical_paths: false,
        };

        let response = api_voyage_rebuild_blocking(&state, config);

        assert_eq!(response.status(), StatusCode::OK);
        let sqlite = SqliteWorkingIndex::open(&index_root).expect("rebuilt sqlite");
        assert_eq!(sqlite.embedding_count().expect("embeddings"), chunks.len());
        assert!(
            inspect_index_integrity(&index_root)
                .expect("integrity")
                .file_mirror_consistent
        );
        fs::remove_dir_all(root).expect("remove rebuild root");
    }

    #[test]
    fn stale_chunks_embeddings_suggestions_and_applied_rows_are_removed_together() {
        let root = temp_browser_root("sqlite-stale-derived");
        let document = inventory_document_fixture(&root, "okf/doc.md");
        let chunks = vec![
            chunk_fixture("chunk-a", "okf/doc.md", "hash-a", "alpha"),
            chunk_fixture("chunk-b", "okf/doc.md", "hash-b", "beta"),
        ];
        let mut initial = embedded_index(&chunks);
        initial.suggestions.push(SuggestedEdge {
            id: "edge-a-b".to_string(),
            review_set_id: "legacy".to_string(),
            provider: "voyage-ai".to_string(),
            model: "voyage-test".to_string(),
            generation_method: "embedding_similarity".to_string(),
            ai_generated: true,
            source_chunk: "chunk-a".to_string(),
            target_chunk: "chunk-b".to_string(),
            score: 0.9,
            created_at: "1".to_string(),
            status: SuggestedEdgeStatus::Accepted,
        });
        let report = ConnectivityReport::success(200, Some(2));
        persist_successful_voyage_index(
            &report,
            std::slice::from_ref(&document),
            &chunks,
            &initial,
            &root,
        )
        .expect("initial derived state");
        let sqlite = SqliteWorkingIndex::open(&root).expect("sqlite");
        let accepted = sqlite.accepted_suggestions().expect("accepted suggestions");
        sqlite
            .record_applied_relations(&accepted, &root.join("doc.md"))
            .expect("applied audit row");

        let changed = vec![chunk_fixture(
            "chunk-a",
            "okf/doc.md",
            "hash-a-new",
            "changed alpha",
        )];
        persist_successful_voyage_index(
            &report,
            &[document],
            &changed,
            &LocalIndex {
                embeddings: embedded_index(&changed).embeddings,
                suggestions: initial.suggestions,
            },
            &root,
        )
        .expect("replace stale state");

        let connection = SqliteWorkingIndex::open(&root)
            .expect("reopen sqlite")
            .connection()
            .expect("connection");
        assert_eq!(table_count(&connection, "chunks").expect("chunks"), 1);
        assert_eq!(
            table_count(&connection, "embeddings").expect("embeddings"),
            1
        );
        assert_eq!(
            table_count(&connection, "suggested_edges").expect("suggestions"),
            0
        );
        assert_eq!(
            table_count(&connection, "applied_relations").expect("applied"),
            0
        );
        fs::remove_dir_all(root).expect("remove stale root");
    }

    #[test]
    fn wal_allows_index_commit_while_a_read_transaction_is_open() {
        let root = temp_browser_root("sqlite-wal-read");
        let document = inventory_document_fixture(&root, "okf/doc.md");
        let chunks = vec![chunk_fixture("chunk-a", "okf/doc.md", "hash-a", "alpha")];
        let report = ConnectivityReport::success(200, Some(1));
        persist_successful_voyage_index(
            &report,
            std::slice::from_ref(&document),
            &chunks,
            &embedded_index(&chunks),
            &root,
        )
        .expect("initial index");
        let mut reader =
            Connection::open_with_flags(root.join("okf.sqlite"), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .expect("reader");
        reader
            .busy_timeout(SQLITE_BUSY_TIMEOUT)
            .expect("busy timeout");
        let read_transaction = reader.transaction().expect("read transaction");
        assert_eq!(
            table_count(&read_transaction, "embeddings").expect("read count"),
            1
        );
        let writer_root = root.clone();
        let writer_document = document.clone();
        let writer_chunks = chunks.clone();
        let writer = std::thread::spawn(move || {
            persist_successful_voyage_index(
                &ConnectivityReport::success(200, Some(0)),
                &[writer_document],
                &writer_chunks,
                &embedded_index(&writer_chunks),
                &writer_root,
            )
        });

        writer.join().expect("writer thread").expect("WAL writer");
        assert_eq!(
            table_count(&read_transaction, "embeddings").expect("stable reader"),
            1
        );
        drop(read_transaction);
        let journal_mode: String = reader
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode");
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        drop(reader);
        fs::remove_dir_all(root).expect("remove WAL root");
    }

    #[test]
    fn deleting_all_derived_state_leaves_canonical_markdown_untouched() {
        let root = temp_browser_root("derived-delete");
        let markdown = root.join("doc.md");
        let source = "---\ntitle: Canonical\ntype: Concept\n---\n# Canonical\n";
        fs::write(&markdown, source).expect("write canonical markdown");
        let index_root = root.join("derived");
        let document = inventory_document_fixture(&root, "doc.md");
        let chunks = vec![chunk_fixture("chunk-a", "doc.md", "hash-a", "alpha")];
        persist_successful_voyage_index(
            &ConnectivityReport::success(200, Some(1)),
            &[document],
            &chunks,
            &embedded_index(&chunks),
            &index_root,
        )
        .expect("derived index");

        fs::remove_dir_all(&index_root).expect("delete derived state");

        assert_eq!(
            fs::read_to_string(markdown).expect("canonical source"),
            source
        );
        fs::remove_dir_all(root).expect("remove canonical fixture");
    }

    #[test]
    fn sqlite_status_inspection_is_read_only_and_does_not_create_a_database() {
        let root = temp_browser_root("sqlite-read-only-status");
        let database = root.join("okf.sqlite");
        assert!(!database.exists());
        let summary = api_read_only_storage(&root, false).expect("inspect missing database");
        assert!(summary.is_none());
        assert!(!database.exists());
        fs::remove_dir_all(root).expect("remove sqlite root");
    }

    #[test]
    fn sqlite_working_index_syncs_inventory_and_embeddings() {
        let root = temp_browser_root("sqlite-sync");
        let sqlite = SqliteWorkingIndex::open(&root).expect("open sqlite");
        let document = inventory_document_fixture(&root, "okf/doc.md");
        let chunks = vec![
            chunk_fixture("chunk-a", "okf/doc.md", "hash-a", "alpha"),
            chunk_fixture("chunk-b", "okf/doc.md", "hash-b", "beta"),
        ];

        sqlite
            .sync_inventory(&[document], &chunks)
            .expect("sync inventory");
        sqlite
            .sync_embeddings(&[
                EmbeddedChunk {
                    chunk: chunks[0].clone(),
                    provider: "voyage-ai".to_string(),
                    model: "voyage-test".to_string(),
                    embedding: vec![1.0, 0.0],
                },
                EmbeddedChunk {
                    chunk: chunks[1].clone(),
                    provider: "voyage-ai".to_string(),
                    model: "voyage-test".to_string(),
                    embedding: vec![0.0, 1.0],
                },
            ])
            .expect("sync embeddings");

        let summary = sqlite.storage_summary(false);
        assert_eq!(summary.documents, 1);
        assert_eq!(summary.chunks, 2);
        assert_eq!(summary.embeddings, 2);
        assert_eq!(sqlite.embedding_count().expect("embedding count"), 2);

        fs::remove_dir_all(root).expect("remove sqlite root");
    }

    #[test]
    fn sqlite_working_index_searches_vectors_with_rust_cosine() {
        let root = temp_browser_root("sqlite-search");
        let sqlite = SqliteWorkingIndex::open(&root).expect("open sqlite");
        let document = inventory_document_fixture(&root, "okf/doc.md");
        let chunks = vec![
            chunk_fixture("chunk-a", "okf/doc.md", "hash-a", "alpha"),
            chunk_fixture("chunk-b", "okf/doc.md", "hash-b", "beta"),
        ];
        sqlite
            .sync_inventory(&[document], &chunks)
            .expect("sync inventory");
        sqlite
            .sync_embeddings(&[
                EmbeddedChunk {
                    chunk: chunks[0].clone(),
                    provider: "voyage-ai".to_string(),
                    model: "voyage-test".to_string(),
                    embedding: vec![1.0, 0.0],
                },
                EmbeddedChunk {
                    chunk: chunks[1].clone(),
                    provider: "voyage-ai".to_string(),
                    model: "voyage-test".to_string(),
                    embedding: vec![0.0, 1.0],
                },
            ])
            .expect("sync embeddings");

        let results = sqlite.search(&[0.9, 0.1], 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].chunk_id, "chunk-a");
        assert!(results[0].score > results[1].score);

        fs::remove_dir_all(root).expect("remove sqlite root");
    }

    #[test]
    fn relation_frontmatter_merge_preserves_metadata_and_deduplicates_suggestions() {
        let suggestion = AcceptedSuggestion {
            id: "suggestion-1".to_string(),
            provider: "voyage-ai".to_string(),
            model: "voyage-test".to_string(),
            generation_method: "embedding_similarity".to_string(),
            ai_generated: true,
            source_chunk: "okf/source.md#1-a".to_string(),
            target_chunk: "okf/target.md#1-b".to_string(),
            score: 0.91,
            created_at: "123456".to_string(),
            source_document: "okf/source.md".to_string(),
            target_document: "okf/target.md".to_string(),
        };
        let source = "---\ntitle: Source\ntype: Concept\n---\n# Source\n";

        let merged =
            merge_relations_into_frontmatter(source, vec![relation_from_suggestion(&suggestion)])
                .expect("merge relation");
        let merged_again = merge_relations_into_frontmatter(
            &merged.source,
            vec![relation_from_suggestion(&suggestion)],
        )
        .expect("merge relation again");

        assert_eq!(merged.added_ids.len(), 1);
        assert_eq!(merged_again.added_ids.len(), 0);
        assert_eq!(merged_again.existing_ids.len(), 1);
        assert!(merged.source.contains("title: Source"));
        assert!(merged.source.contains("type: Concept"));
        assert!(merged.source.contains("relations: ["));
        assert!(merged
            .source
            .contains("\"suggestion_id\": \"suggestion-1\""));
        assert!(merged_again
            .source
            .contains("\"suggestion_id\": \"suggestion-1\""));
        assert_eq!(
            merged_again
                .source
                .matches("\"suggestion_id\": \"suggestion-1\"")
                .count(),
            1
        );
    }

    #[test]
    fn relation_frontmatter_merge_preserves_multiline_unknown_and_existing_metadata() {
        let suggestion = AcceptedSuggestion {
            id: "suggestion-new".to_string(),
            provider: "voyage-ai".to_string(),
            model: "voyage-test".to_string(),
            generation_method: "embedding_similarity".to_string(),
            ai_generated: true,
            source_chunk: "okf/source.md#1-a".to_string(),
            target_chunk: "okf/target.md#1-b".to_string(),
            score: 0.91,
            created_at: "123456".to_string(),
            source_document: "okf/source.md".to_string(),
            target_document: "okf/target.md".to_string(),
        };
        let source = "---\r\ntitle: Source\r\nunknown: keep me exactly\r\nrelations: [\r\n  {\r\n    \"type\": \"manual\",\r\n    \"target\": \"okf/manual.md\",\r\n    \"suggestion_id\": \"manual-1\"\r\n  }\r\n]\r\ntags: [one, two]\r\n---\r\n# Source\r\n";

        let merged =
            merge_relations_into_frontmatter(source, vec![relation_from_suggestion(&suggestion)])
                .expect("merge relation");

        assert!(merged.source.contains("unknown: keep me exactly\r\n"));
        assert!(merged.source.contains("tags: [one, two]\r\n"));
        assert!(merged.source.contains("\"suggestion_id\": \"manual-1\""));
        assert!(merged
            .source
            .contains("\"suggestion_id\": \"suggestion-new\""));
        assert!(merged.source.ends_with("# Source\r\n"));
    }

    #[test]
    fn relation_frontmatter_merge_rejects_invalid_existing_relations() {
        let source = "---\ntitle: Source\nrelations: not-json\n---\n# Source\n";
        let result = merge_relations_into_frontmatter(source, Vec::new());

        assert!(matches!(
            result,
            Err(relations::RelationEditError::InvalidRelations(_))
        ));
    }

    #[test]
    fn canonical_apply_validation_rejects_changed_chunks_and_missing_targets() {
        let root = temp_browser_root("relation-validation");
        let source = root.join("source.md");
        let target = root.join("target.md");
        fs::write(&source, "source").expect("source");
        fs::write(&target, "target").expect("target");
        let suggestion = AcceptedSuggestion {
            id: "suggestion-1".to_string(),
            provider: "voyage-ai".to_string(),
            model: "voyage-test".to_string(),
            generation_method: "embedding_similarity".to_string(),
            ai_generated: true,
            source_chunk: "source-chunk".to_string(),
            target_chunk: "target-chunk".to_string(),
            score: 0.9,
            created_at: "123456".to_string(),
            source_document: "okf/source.md".to_string(),
            target_document: "okf/target.md".to_string(),
        };
        let mut chunks = Map::from([
            ("source-chunk".to_string(), "okf/source.md".to_string()),
            ("target-chunk".to_string(), "okf/target.md".to_string()),
        ]);
        let documents = Map::from([
            ("okf/source.md".to_string(), source),
            ("okf/target.md".to_string(), target.clone()),
        ]);

        assert!(suggestion_is_current(&suggestion, &chunks, &documents));
        chunks.insert("target-chunk".to_string(), "okf/changed.md".to_string());
        assert!(!suggestion_is_current(&suggestion, &chunks, &documents));
        chunks.insert("target-chunk".to_string(), "okf/target.md".to_string());
        fs::remove_file(target).expect("remove target");
        assert!(!suggestion_is_current(&suggestion, &chunks, &documents));

        fs::remove_dir_all(root).expect("remove root");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_relation_write_preserves_document_permissions_and_reports_write_failures() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_browser_root("atomic-relations");
        let document = root.join("source.md");
        fs::write(&document, "before").expect("write document");
        fs::set_permissions(&document, fs::Permissions::from_mode(0o640)).expect("set permissions");

        atomic_replace_preserving_permissions(&document, "after").expect("atomic write");
        assert_eq!(
            fs::read_to_string(&document).expect("read document"),
            "after"
        );
        assert_eq!(
            fs::metadata(&document)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
        assert!(atomic_replace_preserving_permissions(&root, "invalid").is_err());

        fs::remove_dir_all(root).expect("remove root");
    }

    #[tokio::test]
    async fn accepted_relation_applies_to_frontmatter_and_projects_into_graph() {
        let root = temp_browser_root("canonical-relation-e2e");
        let documents_root = root.join("documents");
        let index_root = root.join("derived");
        fs::create_dir_all(&documents_root).expect("documents root");
        fs::write(
            documents_root.join("source.md"),
            "---\ntitle: Source\ntype: Concept\nunknown: retained\n---\n# Source\n\nAlpha.\n",
        )
        .expect("source");
        fs::write(
            documents_root.join("target.md"),
            "---\ntitle: Target\ntype: Concept\n---\n# Target\n\nBeta.\n",
        )
        .expect("target");
        let document_root = DocumentRoot::mounted("okf", &documents_root);
        let repository = okf::Repository::open([document_root.clone()]).expect("repository");
        let documents = inventory(&repository).expect("inventory");
        let chunks = chunk_repository(&repository).expect("chunks");
        let source_chunk = chunks
            .iter()
            .find(|chunk| chunk.document_path == Path::new("okf/source.md"))
            .expect("source chunk");
        let target_chunk = chunks
            .iter()
            .find(|chunk| chunk.document_path == Path::new("okf/target.md"))
            .expect("target chunk");
        let sqlite = SqliteWorkingIndex::open(&index_root).expect("sqlite");
        sqlite
            .sync_inventory(&documents, &chunks)
            .expect("sync inventory");
        sqlite
            .connection()
            .expect("connection")
            .execute(
                "INSERT INTO suggested_edges (
                    id, provider, model, generation_method, ai_generated,
                    source_chunk, target_chunk, score, created_at, status
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    "suggestion-e2e",
                    "voyage-ai",
                    "voyage-test",
                    "embedding_similarity",
                    1,
                    source_chunk.id,
                    target_chunk.id,
                    0.91f64,
                    "123456",
                    "accepted"
                ],
            )
            .expect("insert suggestion");
        let state = AppState {
            mode: ServerMode::LocalEditor,
            tls_status: None,
            local_auth: LocalAuthState::new(None),
            persistent_auth: PersistentAuthState::new(),
            users: None,
            remote_access: false,
            trusted_proxy: None,
            expected_host: "127.0.0.1:8003".to_string(),
            browser_root: root.clone(),
            document_roots: vec![document_root],
            repo_root: root.clone(),
            environment_roots_active: false,
            env_file: root.join(".env"),
            root_proposals: Arc::new(Mutex::new(RootProposalStore::new(Duration::from_secs(900)))),
            root_state_dir: root.join("state"),
            root_monitor: None,
            index_jobs: IndexJobRegistry::default(),
            session_token: "test-session-token-0000000000000000".to_string(),
            expose_physical_paths: false,
        };
        let config = VoyageConfig::from_lookup(|key| match key {
            "OKF_VOYAGE_INDEX_ROOT" => Some(index_root.display().to_string()),
            _ => None,
        });

        let audit_lock = sqlite.connection().expect("audit lock connection");
        audit_lock
            .execute_batch("BEGIN IMMEDIATE")
            .expect("hold SQLite write lock");
        let response = api_edges_apply_with_sqlite(
            &state,
            ApplyEdgesRequest {
                dry_run: Some(false),
            },
            &sqlite,
        );
        assert_eq!(response.status(), StatusCode::OK);
        let failed_audit_body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("failed audit body");
        let failed_audit: serde_json::Value =
            serde_json::from_slice(&failed_audit_body).expect("failed audit json");
        assert_eq!(failed_audit["data"]["files"][0]["changed"], true);
        assert!(failed_audit["data"]["files"][0]["error"]
            .as_str()
            .expect("audit error")
            .contains("audit recording failed"));
        audit_lock
            .execute_batch("ROLLBACK")
            .expect("release audit lock");
        assert_eq!(
            sqlite
                .accepted_suggestions()
                .expect("recovery candidates")
                .len(),
            1
        );
        let reopened = okf::Repository::open(state.document_roots.clone()).expect("reopen");
        let source = reopened
            .find(okf::DocumentQuery::Exact("okf/source.md".to_string()))
            .expect("source");
        assert_eq!(
            source.frontmatter().get("unknown").map(String::as_str),
            Some("retained")
        );
        assert_eq!(source.relations().len(), 1);
        assert_eq!(
            source.relations()[0].suggestion_id(),
            Some("suggestion-e2e")
        );

        let graph = api_graph_blocking(&state);
        let body = to_bytes(graph.into_body(), usize::MAX)
            .await
            .expect("graph body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("graph json");
        let canonical = payload["data"]["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .find(|edge| edge["type"] == "canonical_relation")
            .expect("canonical edge");
        assert_eq!(canonical["provenance"]["suggestion_id"], "suggestion-e2e");
        assert_eq!(canonical["provenance"]["provider"], "voyage-ai");
        assert_eq!(canonical["provenance"]["ai_generated"], true);

        let recovered = api_edges_apply_with_config(
            &state,
            ApplyEdgesRequest {
                dry_run: Some(false),
            },
            &config,
        );
        let recovered_body = to_bytes(recovered.into_body(), usize::MAX)
            .await
            .expect("recovery body");
        let recovered_payload: serde_json::Value =
            serde_json::from_slice(&recovered_body).expect("recovery json");
        assert_eq!(recovered_payload["data"]["applied"], 0);
        assert_eq!(recovered_payload["data"]["files"][0]["recovered"], 1);
        assert!(sqlite
            .accepted_suggestions()
            .expect("accepted after recovery")
            .is_empty());
        let canonical_source = fs::read_to_string(documents_root.join("source.md"))
            .expect("canonical source after recovery");
        assert_eq!(
            canonical_source
                .matches("\"suggestion_id\": \"suggestion-e2e\"")
                .count(),
            1
        );

        fs::remove_dir_all(root).expect("remove root");
    }

    #[test]
    fn sqlite_accepted_suggestions_are_auditable_and_not_reapplied() {
        let root = temp_browser_root("sqlite-accepted");
        let sqlite = SqliteWorkingIndex::open(&root).expect("open sqlite");
        let document = inventory_document_fixture(&root, "okf/source.md");
        let target = inventory_document_fixture(&root, "okf/target.md");
        let chunks = vec![
            chunk_fixture("source-chunk", "okf/source.md", "hash-a", "alpha"),
            chunk_fixture("target-chunk", "okf/target.md", "hash-b", "beta"),
        ];
        sqlite
            .sync_inventory(&[document, target], &chunks)
            .expect("sync inventory");
        let connection = sqlite.connection().expect("connection");
        connection
            .execute(
                "INSERT INTO suggested_edges (
                    id, provider, model, generation_method, ai_generated,
                    source_chunk, target_chunk, score, created_at, status
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    "suggestion-1",
                    "voyage-ai",
                    "voyage-test",
                    "embedding_similarity",
                    1,
                    "source-chunk",
                    "target-chunk",
                    0.91f64,
                    "123456",
                    "accepted"
                ],
            )
            .expect("insert suggestion");

        let suggestions = sqlite.accepted_suggestions().expect("accepted suggestions");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].source_document, "okf/source.md");
        assert_eq!(suggestions[0].target_document, "okf/target.md");

        sqlite
            .record_applied_relations(&suggestions, &root.join("source.md"))
            .expect("record applied");
        assert!(sqlite
            .accepted_suggestions()
            .expect("accepted suggestions after audit")
            .is_empty());

        fs::remove_dir_all(root).expect("remove sqlite root");
    }
}
