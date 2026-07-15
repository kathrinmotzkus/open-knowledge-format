use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose,
};
use rustls::crypto::ring;
use rustls::pki_types::{
    CertificateDer, PrivateKeyDer, PrivatePkcs1KeyDer, PrivatePkcs8KeyDer, PrivateSec1KeyDer,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use x509_parser::extensions::GeneralName;
use x509_parser::parse_x509_certificate;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalTlsCommand {
    Init,
    Status,
    Verify,
    Renew,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalTlsPaths {
    directory: PathBuf,
    ca_certificate: PathBuf,
    ca_private_key: PathBuf,
    server_certificate_chain: PathBuf,
    server_private_key: PathBuf,
}

impl LocalTlsPaths {
    pub fn from_environment() -> Result<Self, String> {
        let state_home = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
            .ok_or_else(|| {
                "cannot determine private state directory: set XDG_STATE_HOME or HOME".to_string()
            })?;
        Ok(Self::new(state_home.join("okf/tls")))
    }

    pub fn new(directory: impl Into<PathBuf>) -> Self {
        let directory = directory.into();
        Self {
            ca_certificate: directory.join("ca-cert.pem"),
            ca_private_key: directory.join("ca-key.pem"),
            server_certificate_chain: directory.join("server-chain.pem"),
            server_private_key: directory.join("server-key.pem"),
            directory,
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn ca_certificate(&self) -> &Path {
        &self.ca_certificate
    }

    pub fn server_certificate_chain(&self) -> &Path {
        &self.server_certificate_chain
    }

    pub fn server_private_key(&self) -> &Path {
        &self.server_private_key
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocalTlsStatus {
    pub initialized: bool,
    pub valid: bool,
    pub ca_certificate_sha256: Option<String>,
    pub ca_not_after: Option<String>,
    pub server_certificate_sha256: Option<String>,
    pub server_not_after: Option<String>,
    pub subject_alt_names: Vec<String>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct TlsFiles {
    certificate: PathBuf,
    private_key: PathBuf,
}

impl std::fmt::Debug for TlsFiles {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TlsFiles")
            .field("certificate", &self.certificate)
            .field("private_key", &"[REDACTED PATH]")
            .finish()
    }
}

impl TlsFiles {
    pub fn new(certificate: impl Into<PathBuf>, private_key: impl Into<PathBuf>) -> Self {
        Self {
            certificate: certificate.into(),
            private_key: private_key.into(),
        }
    }

    pub fn certificate(&self) -> &Path {
        &self.certificate
    }

    pub fn private_key(&self) -> &Path {
        &self.private_key
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TlsStatus {
    protocol: &'static str,
    subject_alt_names: Vec<String>,
    not_before: String,
    not_after: String,
    certificate_sha256: String,
}

pub struct PreparedTls {
    config: RustlsConfig,
    status: TlsStatus,
}

impl std::fmt::Debug for PreparedTls {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedTls")
            .field("config", &"[RUSTLS CONFIGURED]")
            .field("status", &self.status)
            .finish()
    }
}

impl PreparedTls {
    pub fn config(&self) -> RustlsConfig {
        self.config.clone()
    }

    pub fn status(&self) -> &TlsStatus {
        &self.status
    }
}

pub fn prepare_tls(files: &TlsFiles, expected_host: &str) -> Result<PreparedTls, String> {
    validate_private_key_permissions(files.private_key())?;
    let certificate_pem = read_regular_file(files.certificate(), "TLS certificate")?;
    let private_key_pem = read_regular_file(files.private_key(), "TLS private key")?;

    let certificates = certificate_chain_from_pem(&certificate_pem, "TLS certificate")?;
    let leaf = certificates
        .first()
        .expect("certificate_chain_from_pem guarantees at least one certificate");
    let leaf_der = leaf.as_ref().to_vec();

    let (_, certificate) = parse_x509_certificate(&leaf_der)
        .map_err(|error| format!("failed to parse leaf X.509 certificate: {error}"))?;
    if !certificate.validity().is_valid() {
        return Err(format!(
            "TLS certificate is not currently valid (valid from {} through {})",
            certificate.validity().not_before,
            certificate.validity().not_after
        ));
    }

    let sans = certificate
        .subject_alternative_name()
        .map_err(|error| format!("failed to parse TLS certificate SAN extension: {error}"))?
        .ok_or_else(|| "TLS certificate has no subjectAltName extension".to_string())?;
    let subject_alt_names = sans
        .value
        .general_names
        .iter()
        .filter_map(display_supported_san)
        .collect::<Vec<_>>();
    if !san_matches_host(&sans.value.general_names, expected_host) {
        return Err(format!(
            "TLS certificate subjectAltName does not cover configured host {expected_host}"
        ));
    }

    let private_key = private_key_from_pem(&private_key_pem, "TLS private-key")?;

    let provider = Arc::new(ring::default_provider());
    let mut server_config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| format!("failed to select safe TLS protocol versions: {error}"))?
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .map_err(|error| format!("TLS certificate and private key are incompatible: {error}"))?;
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    let fingerprint = Sha256::digest(&leaf_der);
    let certificate_sha256 = fingerprint
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":");
    let status = TlsStatus {
        protocol: "TLS",
        subject_alt_names,
        not_before: certificate.validity().not_before.to_string(),
        not_after: certificate.validity().not_after.to_string(),
        certificate_sha256,
    };

    Ok(PreparedTls {
        config: RustlsConfig::from_config(Arc::new(server_config)),
        status,
    })
}

fn read_regular_file(path: &Path, label: &str) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect {label} {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{label} is not a regular file: {}", path.display()));
    }
    fs::read(path).map_err(|error| format!("failed to read {label} {}: {error}", path.display()))
}

#[cfg(unix)]
fn validate_private_key_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "failed to inspect TLS private key {}: {error}",
            path.display()
        )
    })?;
    let mode = metadata.mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(format!(
            "TLS private key permissions are too broad ({mode:03o}); require owner-only access such as 600"
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_key_permissions(path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "failed to inspect TLS private key {}: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "TLS private key is not a regular file: {}",
            path.display()
        ));
    }
    Ok(())
}

fn display_supported_san(name: &GeneralName<'_>) -> Option<String> {
    match name {
        GeneralName::DNSName(value) => Some(format!("DNS:{value}")),
        GeneralName::IPAddress(bytes) => ip_from_san(bytes).map(|value| format!("IP:{value}")),
        _ => None,
    }
}

fn san_matches_host(names: &[GeneralName<'_>], expected_host: &str) -> bool {
    let expected_host = expected_host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(expected_host);
    if let Ok(expected_ip) = expected_host.parse::<IpAddr>() {
        return names.iter().any(|name| {
            matches!(name, GeneralName::IPAddress(bytes) if ip_from_san(bytes) == Some(expected_ip))
        });
    }
    names.iter().any(
        |name| matches!(name, GeneralName::DNSName(value) if value.eq_ignore_ascii_case(expected_host)),
    )
}

fn ip_from_san(bytes: &[u8]) -> Option<IpAddr> {
    match bytes {
        [a, b, c, d] => Some(IpAddr::V4(Ipv4Addr::new(*a, *b, *c, *d))),
        bytes if bytes.len() == 16 => {
            let octets: [u8; 16] = bytes.try_into().ok()?;
            Some(IpAddr::V6(Ipv6Addr::from(octets)))
        }
        _ => None,
    }
}

pub fn initialize_local_tls(paths: &LocalTlsPaths) -> Result<LocalTlsStatus, String> {
    let managed_files = [
        &paths.ca_certificate,
        &paths.ca_private_key,
        &paths.server_certificate_chain,
        &paths.server_private_key,
    ];
    if managed_files.iter().any(|path| path.exists()) {
        return Err(format!(
            "local TLS state already exists in {}; use `okf-http tls status`, `verify`, or `renew`",
            paths.directory.display()
        ));
    }
    create_private_directory(&paths.directory)?;

    let (ca_certificate, ca_key) = generate_ca()?;
    let (server_certificate, server_key) = generate_server_certificate(&ca_certificate, &ca_key)?;
    let chain = format!("{}{}", server_certificate.pem(), ca_certificate.pem());

    let result = (|| {
        write_new_file(
            &paths.ca_private_key,
            ca_key.serialize_pem().as_bytes(),
            true,
        )?;
        write_new_file(
            &paths.ca_certificate,
            ca_certificate.pem().as_bytes(),
            false,
        )?;
        write_new_file(
            &paths.server_private_key,
            server_key.serialize_pem().as_bytes(),
            true,
        )?;
        write_new_file(&paths.server_certificate_chain, chain.as_bytes(), false)?;
        verify_local_tls(paths)
    })();
    if result.is_err() {
        for path in managed_files {
            let _ = fs::remove_file(path);
        }
    }
    result
}

pub fn renew_local_tls(paths: &LocalTlsPaths) -> Result<LocalTlsStatus, String> {
    validate_state_directory(paths)?;
    validate_private_key_permissions(&paths.ca_private_key)?;
    let ca_pem = fs::read_to_string(&paths.ca_certificate).map_err(|error| {
        format!(
            "failed to read local CA certificate {}: {error}",
            paths.ca_certificate.display()
        )
    })?;
    let ca_key_pem = fs::read_to_string(&paths.ca_private_key)
        .map_err(|error| format!("failed to read local CA private key: {error}"))?;
    let ca_key = KeyPair::from_pem(&ca_key_pem)
        .map_err(|error| format!("failed to parse local CA private key: {error}"))?;
    let ca_params = CertificateParams::from_ca_cert_pem(&ca_pem)
        .map_err(|error| format!("failed to parse local CA certificate: {error}"))?;
    let ca_certificate = ca_params
        .self_signed(&ca_key)
        .map_err(|error| format!("failed to reconstruct local CA: {error}"))?;
    verify_ca_certificate(&ca_pem)?;

    let (server_certificate, server_key) = generate_server_certificate(&ca_certificate, &ca_key)?;
    let chain = format!("{}{}", server_certificate.pem(), ca_pem);
    verify_issued_certificate(server_certificate.pem().as_bytes(), ca_pem.as_bytes())?;
    atomic_replace_pair(
        &paths.server_private_key,
        server_key.serialize_pem().as_bytes(),
        &paths.server_certificate_chain,
        chain.as_bytes(),
    )?;
    verify_local_tls(paths)
}

pub fn local_tls_status(paths: &LocalTlsPaths) -> Result<LocalTlsStatus, String> {
    if !paths.directory.exists() {
        return Ok(LocalTlsStatus {
            initialized: false,
            valid: false,
            ca_certificate_sha256: None,
            ca_not_after: None,
            server_certificate_sha256: None,
            server_not_after: None,
            subject_alt_names: Vec::new(),
        });
    }
    match inspect_local_tls(paths) {
        Ok(mut status) => {
            status.valid = verify_local_tls(paths).is_ok();
            Ok(status)
        }
        Err(error) => Err(error),
    }
}

pub fn verify_local_tls(paths: &LocalTlsPaths) -> Result<LocalTlsStatus, String> {
    validate_state_directory(paths)?;
    validate_private_key_permissions(&paths.ca_private_key)?;
    let ca_pem = fs::read(&paths.ca_certificate).map_err(|error| {
        format!(
            "failed to read local CA certificate {}: {error}",
            paths.ca_certificate.display()
        )
    })?;
    let ca_der = first_certificate_der(&ca_pem, "local CA certificate")?;
    let (_, ca) = parse_x509_certificate(&ca_der)
        .map_err(|error| format!("failed to parse local CA certificate: {error}"))?;
    if !ca.validity().is_valid() {
        return Err("local CA certificate is not currently valid".to_string());
    }
    ca.verify_signature(None)
        .map_err(|error| format!("local CA self-signature is invalid: {error}"))?;

    let prepared = prepare_tls(
        &TlsFiles::new(&paths.server_certificate_chain, &paths.server_private_key),
        "localhost",
    )?;
    for host in ["127.0.0.1", "::1"] {
        prepare_tls(
            &TlsFiles::new(&paths.server_certificate_chain, &paths.server_private_key),
            host,
        )?;
    }
    let server_pem = fs::read(&paths.server_certificate_chain)
        .map_err(|error| format!("failed to read local server certificate chain: {error}"))?;
    let server_der = first_certificate_der(&server_pem, "local server certificate")?;
    let (_, server) = parse_x509_certificate(&server_der)
        .map_err(|error| format!("failed to parse local server certificate: {error}"))?;
    if server.issuer() != ca.subject() {
        return Err("local server certificate was not issued by the configured OKF CA".to_string());
    }
    server
        .verify_signature(Some(ca.public_key()))
        .map_err(|error| format!("local server certificate signature is invalid: {error}"))?;

    let mut status = inspect_local_tls(paths)?;
    status.valid = true;
    status.subject_alt_names = prepared.status.subject_alt_names.clone();
    Ok(status)
}

fn generate_ca() -> Result<(rcgen::Certificate, KeyPair), String> {
    let key = KeyPair::generate().map_err(|error| format!("failed to generate CA key: {error}"))?;
    let mut params = CertificateParams::default();
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, "OKF Local CA");
    name.push(DnType::OrganizationName, "Open Knowledge Format");
    params.distinguished_name = name;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.not_before = time::OffsetDateTime::now_utc() - time::Duration::days(1);
    params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(3650);
    let certificate = params
        .self_signed(&key)
        .map_err(|error| format!("failed to generate CA certificate: {error}"))?;
    Ok((certificate, key))
}

fn generate_server_certificate(
    ca_certificate: &rcgen::Certificate,
    ca_key: &KeyPair,
) -> Result<(rcgen::Certificate, KeyPair), String> {
    let key =
        KeyPair::generate().map_err(|error| format!("failed to generate server key: {error}"))?;
    let mut params = CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])
    .map_err(|error| format!("failed to configure local certificate SANs: {error}"))?;
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, "OKF Local Server");
    params.distinguished_name = name;
    params.not_before = time::OffsetDateTime::now_utc() - time::Duration::hours(1);
    params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(365);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    let certificate = params
        .signed_by(&key, ca_certificate, ca_key)
        .map_err(|error| format!("failed to generate local server certificate: {error}"))?;
    Ok((certificate, key))
}

fn inspect_local_tls(paths: &LocalTlsPaths) -> Result<LocalTlsStatus, String> {
    let ca_pem = fs::read(&paths.ca_certificate).map_err(|error| {
        format!(
            "failed to read local CA certificate {}: {error}",
            paths.ca_certificate.display()
        )
    })?;
    let server_pem = fs::read(&paths.server_certificate_chain)
        .map_err(|error| format!("failed to read local server certificate chain: {error}"))?;
    let ca_der = first_certificate_der(&ca_pem, "local CA certificate")?;
    let server_der = first_certificate_der(&server_pem, "local server certificate")?;
    let (_, ca) = parse_x509_certificate(&ca_der)
        .map_err(|error| format!("failed to parse local CA certificate: {error}"))?;
    let (_, server) = parse_x509_certificate(&server_der)
        .map_err(|error| format!("failed to parse local server certificate: {error}"))?;
    let sans = server
        .subject_alternative_name()
        .map_err(|error| format!("failed to parse local server SANs: {error}"))?
        .map(|extension| {
            extension
                .value
                .general_names
                .iter()
                .filter_map(display_supported_san)
                .collect()
        })
        .unwrap_or_default();
    Ok(LocalTlsStatus {
        initialized: true,
        valid: false,
        ca_certificate_sha256: Some(fingerprint(&ca_der)),
        ca_not_after: Some(ca.validity().not_after.to_string()),
        server_certificate_sha256: Some(fingerprint(&server_der)),
        server_not_after: Some(server.validity().not_after.to_string()),
        subject_alt_names: sans,
    })
}

fn verify_ca_certificate(pem: &str) -> Result<(), String> {
    let der = first_certificate_der(pem.as_bytes(), "local CA certificate")?;
    let (_, certificate) = parse_x509_certificate(&der)
        .map_err(|error| format!("failed to parse local CA certificate: {error}"))?;
    if !certificate.validity().is_valid() {
        return Err("local CA certificate is not currently valid".to_string());
    }
    certificate
        .verify_signature(None)
        .map_err(|error| format!("local CA self-signature is invalid: {error}"))
}

fn verify_issued_certificate(certificate_pem: &[u8], ca_pem: &[u8]) -> Result<(), String> {
    let certificate_der = first_certificate_der(certificate_pem, "new server certificate")?;
    let ca_der = first_certificate_der(ca_pem, "local CA certificate")?;
    let (_, certificate) = parse_x509_certificate(&certificate_der)
        .map_err(|error| format!("failed to parse new server certificate: {error}"))?;
    let (_, ca) = parse_x509_certificate(&ca_der)
        .map_err(|error| format!("failed to parse local CA certificate: {error}"))?;
    certificate
        .verify_signature(Some(ca.public_key()))
        .map_err(|_| {
            "local CA certificate and private key do not match; renewal was not written".to_string()
        })
}

fn first_certificate_der(pem: &[u8], label: &str) -> Result<Vec<u8>, String> {
    certificate_chain_from_pem(pem, label)?
        .into_iter()
        .next()
        .map(|certificate| certificate.as_ref().to_vec())
        .ok_or_else(|| format!("{label} file contains no certificate"))
}

#[derive(Debug, Eq, PartialEq)]
struct PemBlock {
    label: String,
    der: Vec<u8>,
}

fn certificate_chain_from_pem(
    pem: &[u8],
    label: &str,
) -> Result<Vec<CertificateDer<'static>>, String> {
    let certificates = parse_pem_blocks(pem, label)?
        .into_iter()
        .filter(|block| block.label == "CERTIFICATE")
        .map(|block| CertificateDer::from(block.der))
        .collect::<Vec<_>>();
    if certificates.is_empty() {
        return Err(format!("{label} file contains no certificates"));
    }
    Ok(certificates)
}

fn private_key_from_pem(pem: &[u8], label: &str) -> Result<PrivateKeyDer<'static>, String> {
    let mut keys = Vec::new();
    for block in parse_pem_blocks(pem, label)? {
        let key = match block.label.as_str() {
            "PRIVATE KEY" => Some(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(block.der))),
            "RSA PRIVATE KEY" => Some(PrivateKeyDer::Pkcs1(PrivatePkcs1KeyDer::from(block.der))),
            "EC PRIVATE KEY" => Some(PrivateKeyDer::Sec1(PrivateSec1KeyDer::from(block.der))),
            _ => None,
        };
        if let Some(key) = key {
            keys.push(key);
        }
    }
    match keys.len() {
        0 => Err(format!("{label} file contains no supported private key")),
        1 => Ok(keys.remove(0)),
        _ => Err(format!("{label} file contains more than one private key")),
    }
}

fn parse_pem_blocks(pem: &[u8], label: &str) -> Result<Vec<PemBlock>, String> {
    let text =
        std::str::from_utf8(pem).map_err(|_| format!("failed to parse {label} PEM: not UTF-8"))?;
    let mut blocks = Vec::new();
    let mut current_label: Option<String> = None;
    let mut body = String::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(begin_label) = line
            .strip_prefix("-----BEGIN ")
            .and_then(|rest| rest.strip_suffix("-----"))
        {
            if current_label.is_some() {
                return Err(format!(
                    "failed to parse {label} PEM: nested BEGIN boundary"
                ));
            }
            if begin_label.trim().is_empty() {
                return Err(format!("failed to parse {label} PEM: empty BEGIN label"));
            }
            current_label = Some(begin_label.to_string());
            body.clear();
            continue;
        }

        if let Some(end_label) = line
            .strip_prefix("-----END ")
            .and_then(|rest| rest.strip_suffix("-----"))
        {
            let Some(begin_label) = current_label.take() else {
                return Err(format!(
                    "failed to parse {label} PEM: END boundary without matching BEGIN"
                ));
            };
            if begin_label != end_label {
                return Err(format!(
                    "failed to parse {label} PEM: END {end_label} does not match BEGIN {begin_label}"
                ));
            }
            let der = decode_pem_base64(&body, label)?;
            blocks.push(PemBlock {
                label: begin_label,
                der,
            });
            body.clear();
            continue;
        }

        if current_label.is_some() {
            if line.starts_with("-----") {
                return Err(format!(
                    "failed to parse {label} PEM: malformed PEM boundary"
                ));
            }
            body.push_str(line);
        }
    }

    if let Some(open_label) = current_label {
        return Err(format!(
            "failed to parse {label} PEM: missing END {open_label} boundary"
        ));
    }

    Ok(blocks)
}

fn decode_pem_base64(input: &str, label: &str) -> Result<Vec<u8>, String> {
    let bytes = input.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return Err(format!("failed to parse {label} PEM: invalid base64 data"));
    }

    let mut decoded = Vec::with_capacity(bytes.len() / 4 * 3);
    let chunks = bytes.chunks_exact(4);
    let chunk_count = chunks.len();
    for (index, chunk) in chunks.enumerate() {
        let is_last = index + 1 == chunk_count;
        let padding = chunk.iter().filter(|byte| **byte == b'=').count();
        if padding > 2
            || chunk[0] == b'='
            || chunk[1] == b'='
            || (chunk[2] == b'=' && chunk[3] != b'=')
            || (padding > 0 && !is_last)
        {
            return Err(format!("failed to parse {label} PEM: invalid base64 data"));
        }

        let first = base64_value(chunk[0], label)?;
        let second = base64_value(chunk[1], label)?;
        let third = if chunk[2] == b'=' {
            0
        } else {
            base64_value(chunk[2], label)?
        };
        let fourth = if chunk[3] == b'=' {
            0
        } else {
            base64_value(chunk[3], label)?
        };

        let combined = ((first as u32) << 18)
            | ((second as u32) << 12)
            | ((third as u32) << 6)
            | fourth as u32;
        decoded.push((combined >> 16) as u8);
        if padding < 2 {
            decoded.push((combined >> 8) as u8);
        }
        if padding == 0 {
            decoded.push(combined as u8);
        }
    }

    Ok(decoded)
}

fn base64_value(byte: u8, label: &str) -> Result<u8, String> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(format!("failed to parse {label} PEM: invalid base64 data")),
    }
}

fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn validate_state_directory(paths: &LocalTlsPaths) -> Result<(), String> {
    let metadata = fs::symlink_metadata(&paths.directory).map_err(|error| {
        format!(
            "failed to inspect local TLS state {}: {error}",
            paths.directory.display()
        )
    })?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("local TLS state path must be a real directory, not a symlink".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 {
            return Err(
                "local TLS state directory must allow owner access only (mode 700)".to_string(),
            );
        }
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "failed to create local TLS state {}: {error}",
            path.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("failed to protect local TLS state: {error}"))?;
    }
    validate_state_directory(&LocalTlsPaths::new(path))
}

fn write_new_file(path: &Path, bytes: &[u8], private: bool) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn atomic_replace_pair(
    key_path: &Path,
    key_bytes: &[u8],
    certificate_path: &Path,
    certificate_bytes: &[u8],
) -> Result<(), String> {
    let suffix = format!("okf-renew-{}", std::process::id());
    let key_temporary = key_path.with_extension(format!("{suffix}.tmp"));
    let certificate_temporary = certificate_path.with_extension(format!("{suffix}.tmp"));
    let key_backup = key_path.with_extension(format!("{suffix}.bak"));
    let certificate_backup = certificate_path.with_extension(format!("{suffix}.bak"));
    for path in [
        &key_temporary,
        &certificate_temporary,
        &key_backup,
        &certificate_backup,
    ] {
        if path.exists() {
            return Err(format!(
                "renewal staging path already exists: {}",
                path.display()
            ));
        }
    }
    write_new_file(&key_temporary, key_bytes, true)?;
    if let Err(error) = write_new_file(&certificate_temporary, certificate_bytes, false) {
        let _ = fs::remove_file(&key_temporary);
        return Err(error);
    }

    let replace = (|| -> Result<(), String> {
        fs::rename(key_path, &key_backup)
            .map_err(|error| format!("failed to stage existing server key: {error}"))?;
        fs::rename(certificate_path, &certificate_backup).map_err(|error| {
            let _ = fs::rename(&key_backup, key_path);
            format!("failed to stage existing server certificate: {error}")
        })?;
        if let Err(error) = fs::rename(&key_temporary, key_path) {
            let _ = fs::rename(&key_backup, key_path);
            let _ = fs::rename(&certificate_backup, certificate_path);
            return Err(format!("failed to activate renewed server key: {error}"));
        }
        if let Err(error) = fs::rename(&certificate_temporary, certificate_path) {
            let _ = fs::remove_file(key_path);
            let _ = fs::rename(&key_backup, key_path);
            let _ = fs::rename(&certificate_backup, certificate_path);
            return Err(format!(
                "failed to activate renewed server certificate: {error}"
            ));
        }
        Ok(())
    })();
    let _ = fs::remove_file(&key_temporary);
    let _ = fs::remove_file(&certificate_temporary);
    if replace.is_ok() {
        let _ = fs::remove_file(&key_backup);
        let _ = fs::remove_file(&certificate_backup);
    }
    replace
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{date_time_ymd, CertificateParams, KeyPair};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture {
        root: PathBuf,
        files: TlsFiles,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn fixture(names: &[&str], validity: Option<(i32, i32)>) -> Fixture {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("okf-http-tls-{unique}"));
        fs::create_dir_all(&root).expect("create TLS fixture");
        let key = KeyPair::generate().expect("generate private key");
        let mut params =
            CertificateParams::new(names.iter().map(ToString::to_string).collect::<Vec<_>>())
                .expect("certificate params");
        if let Some((not_before, not_after)) = validity {
            params.not_before = date_time_ymd(not_before, 1, 1);
            params.not_after = date_time_ymd(not_after, 1, 1);
        }
        let certificate = params.self_signed(&key).expect("self-signed certificate");
        let certificate_path = root.join("certificate.pem");
        let private_key_path = root.join("private-key.pem");
        fs::write(&certificate_path, certificate.pem()).expect("write certificate");
        fs::write(&private_key_path, key.serialize_pem()).expect("write private key");
        set_private_key_permissions(&private_key_path, 0o600);
        Fixture {
            root,
            files: TlsFiles::new(certificate_path, private_key_path),
        }
    }

    #[cfg(unix)]
    fn set_private_key_permissions(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set key permissions");
    }

    #[cfg(not(unix))]
    fn set_private_key_permissions(_path: &Path, _mode: u32) {}

    #[test]
    fn valid_local_certificate_supports_dns_ipv4_and_ipv6_sans() {
        let fixture = fixture(&["localhost", "127.0.0.1", "::1"], None);
        for host in ["localhost", "127.0.0.1", "::1", "[::1]"] {
            let prepared = prepare_tls(&fixture.files, host).expect("valid local TLS");
            assert_eq!(prepared.status.protocol, "TLS");
            assert_eq!(prepared.status.certificate_sha256.len(), 95);
        }
    }

    #[test]
    fn local_ca_lifecycle_initializes_verifies_and_renews_without_replacing_ca() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("okf-local-ca-{unique}"));
        let paths = LocalTlsPaths::new(root.clone());

        let initialized = initialize_local_tls(&paths).expect("initialize local TLS");
        assert!(initialized.initialized);
        assert!(initialized.valid);
        assert_eq!(initialized.subject_alt_names.len(), 3);
        let ca_fingerprint = initialized.ca_certificate_sha256.clone();
        let old_server_key = fs::read(paths.server_private_key()).expect("server key");

        let verified = verify_local_tls(&paths).expect("verify local TLS");
        assert_eq!(verified.ca_certificate_sha256, ca_fingerprint);
        let renewed = renew_local_tls(&paths).expect("renew local TLS");
        assert_eq!(renewed.ca_certificate_sha256, ca_fingerprint);
        assert_ne!(
            fs::read(paths.server_private_key()).expect("renewed server key"),
            old_server_key
        );
        assert!(initialize_local_tls(&paths).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(fs::metadata(&root).unwrap().mode() & 0o777, 0o700);
            assert_eq!(
                fs::metadata(paths.server_private_key()).unwrap().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(root).expect("remove local TLS fixture");
    }

    #[test]
    fn local_tls_status_is_safe_before_initialization() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("okf-local-ca-missing-{unique}"));
        let status = local_tls_status(&LocalTlsPaths::new(root)).expect("missing status");
        assert!(!status.initialized);
        assert!(!status.valid);
        assert!(status.ca_certificate_sha256.is_none());
    }

    #[test]
    fn renewal_with_mismatched_ca_key_preserves_active_server_credentials() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("okf-local-ca-mismatch-{unique}"));
        let paths = LocalTlsPaths::new(root.clone());
        initialize_local_tls(&paths).expect("initialize local TLS");
        let old_key = fs::read(&paths.server_private_key).expect("old key");
        let old_chain = fs::read(&paths.server_certificate_chain).expect("old chain");
        let unrelated_key = KeyPair::generate().expect("unrelated key");
        fs::write(&paths.ca_private_key, unrelated_key.serialize_pem()).expect("replace CA key");
        set_private_key_permissions(&paths.ca_private_key, 0o600);

        let error = renew_local_tls(&paths).expect_err("mismatched CA key must fail");
        assert!(error.contains("do not match"));
        assert_eq!(fs::read(&paths.server_private_key).unwrap(), old_key);
        assert_eq!(
            fs::read(&paths.server_certificate_chain).unwrap(),
            old_chain
        );
        fs::remove_dir_all(root).expect("remove local TLS fixture");
    }

    #[test]
    fn certificate_must_cover_configured_host() {
        let fixture = fixture(&["localhost"], None);
        let error = prepare_tls(&fixture.files, "127.0.0.1").expect_err("missing IP SAN");
        assert!(error.contains("subjectAltName"));
        assert!(error.contains("127.0.0.1"));
    }

    #[test]
    fn expired_certificate_is_rejected() {
        let fixture = fixture(&["localhost"], Some((2000, 2001)));
        let error = prepare_tls(&fixture.files, "localhost").expect_err("expired certificate");
        assert!(error.contains("not currently valid"));
    }

    #[test]
    fn not_yet_valid_certificate_is_rejected() {
        let fixture = fixture(&["localhost"], Some((2090, 2091)));
        let error = prepare_tls(&fixture.files, "localhost").expect_err("future certificate");
        assert!(error.contains("not currently valid"));
    }

    #[test]
    fn mismatched_private_key_is_rejected() {
        let primary = fixture(&["localhost"], None);
        let other = fixture(&["localhost"], None);
        fs::copy(other.files.private_key(), primary.files.private_key()).expect("replace key");
        set_private_key_permissions(primary.files.private_key(), 0o600);
        let error = prepare_tls(&primary.files, "localhost").expect_err("mismatched key");
        assert!(error.contains("incompatible"));
    }

    #[cfg(unix)]
    #[test]
    fn broadly_readable_private_key_is_rejected() {
        let fixture = fixture(&["localhost"], None);
        set_private_key_permissions(fixture.files.private_key(), 0o644);
        let error = prepare_tls(&fixture.files, "localhost").expect_err("insecure key mode");
        assert!(error.contains("too broad"));
    }

    #[test]
    fn malformed_certificate_fails_before_listener_startup() {
        let fixture = fixture(&["localhost"], None);
        fs::write(fixture.files.certificate(), "not a certificate").expect("damage certificate");
        let error = prepare_tls(&fixture.files, "localhost").expect_err("malformed certificate");
        assert!(error.contains("contains no certificates"));
    }

    #[test]
    fn certificate_pem_parser_accepts_chains() {
        let first = fixture(&["localhost"], None);
        let second = fixture(&["localhost"], None);
        let chain = format!(
            "{}{}",
            fs::read_to_string(first.files.certificate()).expect("first cert"),
            fs::read_to_string(second.files.certificate()).expect("second cert")
        );

        let certificates =
            certificate_chain_from_pem(chain.as_bytes(), "TLS certificate").expect("chain");

        assert_eq!(certificates.len(), 2);
    }

    #[test]
    fn invalid_certificate_base64_fails_before_listener_startup() {
        let fixture = fixture(&["localhost"], None);
        fs::write(
            fixture.files.certificate(),
            "-----BEGIN CERTIFICATE-----\n!!!!\n-----END CERTIFICATE-----\n",
        )
        .expect("damage certificate");
        let error = prepare_tls(&fixture.files, "localhost").expect_err("invalid base64");
        assert!(error.contains("invalid base64 data"));
    }

    #[test]
    fn malformed_private_key_fails_before_listener_startup() {
        let fixture = fixture(&["localhost"], None);
        fs::write(fixture.files.private_key(), "not a private key").expect("damage key");
        set_private_key_permissions(fixture.files.private_key(), 0o600);
        let error = prepare_tls(&fixture.files, "localhost").expect_err("malformed key");
        assert!(error.contains("contains no supported private key"));
    }

    #[test]
    fn multiple_private_keys_fail_before_listener_startup() {
        let fixture = fixture(&["localhost"], None);
        let private_key = fs::read_to_string(fixture.files.private_key()).expect("private key");
        fs::write(
            fixture.files.private_key(),
            format!("{private_key}{private_key}"),
        )
        .expect("duplicate key");
        set_private_key_permissions(fixture.files.private_key(), 0o600);

        let error = prepare_tls(&fixture.files, "localhost").expect_err("multiple private keys");

        assert!(error.contains("contains more than one private key"));
    }
}
