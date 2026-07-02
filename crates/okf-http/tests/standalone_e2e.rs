use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rcgen::{CertificateParams, KeyPair};
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "okf-http-standalone-e2e-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create standalone fixture");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ServerProcess(Child);

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn clean_command(binary: &str, working_directory: &Path, runtime: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .current_dir(working_directory)
        .env("XDG_RUNTIME_DIR", runtime)
        .env("XDG_STATE_HOME", working_directory.join("state"))
        .env_remove("OKF_BROWSER_ROOT")
        .env_remove("OKF_DOCUMENT_ROOTS")
        .env_remove("OKF_HTTP_SESSION_TOKEN")
        .env_remove("OKF_HTTP_TRUSTED_PROXY_TOKEN")
        .env_remove("OKF_VOYAGE_API_KEY")
        .env_remove("VOYAGE_API_KEY");
    command
}

fn available_port() -> Option<u16> {
    match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(listener) => Some(listener.local_addr().expect("local address").port()),
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                && std::env::var_os("OKF_REQUIRE_SOCKET_E2E").is_none() =>
        {
            eprintln!("socket-restricted environment: packaged install passed; HTTP E2E skipped");
            None
        }
        Err(error) => panic!("reserve local port: {error}"),
    }
}

fn http_get(port: u16, path: &str) -> std::io::Result<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (headers, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    Ok((status, body.to_string()))
}

fn wait_for_server(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if http_get(port, "/api/v1/health").is_ok_and(|(status, _)| status == 200) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("standalone okf-http did not become ready");
}

fn https_get(
    port: u16,
    path: &str,
    certificate: CertificateDer<'static>,
) -> std::io::Result<(u16, String)> {
    let (status, _, body) = https_request(port, "GET", path, "", certificate, "")?;
    Ok((status, body))
}

fn https_request(
    port: u16,
    method: &str,
    path: &str,
    body: &str,
    certificate: CertificateDer<'static>,
    additional_headers: &str,
) -> std::io::Result<(u16, String, String)> {
    let mut roots = RootCertStore::empty();
    roots.add(certificate).map_err(std::io::Error::other)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(std::io::Error::other)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name =
        ServerName::IpAddress("127.0.0.1".parse::<std::net::IpAddr>().unwrap().into());
    let connection =
        ClientConnection::new(Arc::new(config), server_name).map_err(std::io::Error::other)?;
    let tcp = TcpStream::connect(("127.0.0.1", port))?;
    tcp.set_read_timeout(Some(Duration::from_secs(15)))?;
    let mut stream = StreamOwned::new(connection, tcp);
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: https://127.0.0.1:{port}\r\nSec-Fetch-Site: same-origin\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{additional_headers}Connection: close\r\n\r\n{body}",
        body.len()
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (headers, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    Ok((status, headers.to_string(), body.to_string()))
}

fn wait_for_tls_server(port: u16, certificate: &CertificateDer<'static>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if https_get(port, "/api/v1/health", certificate.clone())
            .is_ok_and(|(status, _)| status == 200)
        {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("standalone TLS okf-http did not become ready");
}

#[test]
fn packaged_binary_installs_and_serves_standalone_browser_and_documents() {
    let fixture = TestDirectory::new();
    let browser_root = fixture.path().join("browser");
    let documents_root = fixture.path().join("knowledge");
    let runtime_root = fixture.path().join("runtime");
    fs::create_dir_all(&documents_root).expect("create document root");
    fs::create_dir_all(&runtime_root).expect("create runtime root");
    fs::write(
        documents_root.join("welcome.md"),
        "---\ntitle: Standalone Welcome\ntype: Guide\n---\n# Standalone Welcome\n",
    )
    .expect("write standalone document");

    let binary = env!("CARGO_BIN_EXE_okf-http");
    let install = clean_command(binary, fixture.path(), &runtime_root)
        .arg("--install-browser")
        .arg("--browser-root")
        .arg(&browser_root)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("run packaged browser installer");
    assert!(
        install.status.success(),
        "browser install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );
    assert!(browser_root.join(".okf-browser-assets.json").is_file());
    assert!(browser_root.join("vendor/cytoscape.min.js").is_file());

    let Some(port) = available_port() else {
        return;
    };
    let mut command = clean_command(binary, fixture.path(), &runtime_root);
    let child = command
        .arg("--browser-root")
        .arg(&browser_root)
        .arg("--root")
        .arg(format!("standalone={}", documents_root.display()))
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start packaged standalone server");
    let _server = ServerProcess(child);
    wait_for_server(port);

    let (_, health) = http_get(port, "/api/v1/health").expect("fetch health");
    let health: serde_json::Value = serde_json::from_str(&health).expect("health JSON");
    assert_eq!(health["data"]["mode"], "read-only");
    assert!(
        fs::read_dir(&runtime_root)
            .expect("read runtime directory")
            .all(|entry| !entry
                .expect("runtime entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".token")),
        "read-only startup must not create a session-token file"
    );

    let (browser_status, browser) =
        http_get(port, "/docs-browser/index.html").expect("fetch packaged browser");
    assert_eq!(browser_status, 200);
    assert!(browser.contains("<title>OKF Knowledge Browser</title>"));

    let (documents_status, documents) =
        http_get(port, "/api/v1/documents").expect("fetch standalone documents");
    assert_eq!(documents_status, 200);
    let payload: serde_json::Value = serde_json::from_str(&documents).expect("document JSON");
    assert_eq!(payload["api_version"], "v1");
    assert_eq!(payload["data"]["roots"][0]["mount"], "standalone");
    assert_eq!(
        payload["data"]["documents"][0]["logical_path"],
        "standalone/welcome.md"
    );
    assert_eq!(
        payload["data"]["documents"][0]["browser_path"],
        "/okf-docs/standalone/welcome.md"
    );
    assert!(!documents.contains("scanlab"));

    let (document_status, document) = http_get(
        port,
        payload["data"]["documents"][0]["browser_path"]
            .as_str()
            .expect("browser path"),
    )
    .expect("fetch standalone Markdown");
    assert_eq!(document_status, 200);
    assert!(document.contains("# Standalone Welcome"));
}

#[cfg(unix)]
#[test]
fn packaged_binary_serves_authenticated_mode_only_over_validated_https() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = TestDirectory::new();
    let browser_root = fixture.path().join("browser");
    let runtime_root = fixture.path().join("runtime");
    fs::create_dir_all(&runtime_root).expect("create runtime root");

    let key = KeyPair::generate().expect("generate TLS key");
    let params = CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])
    .expect("TLS certificate params");
    let certificate = params
        .self_signed(&key)
        .expect("self-signed TLS certificate");
    let certificate_path = fixture.path().join("certificate.pem");
    let private_key_path = fixture.path().join("private-key.pem");
    fs::write(&certificate_path, certificate.pem()).expect("write TLS certificate");
    fs::write(&private_key_path, key.serialize_pem()).expect("write TLS key");
    fs::set_permissions(&private_key_path, fs::Permissions::from_mode(0o600))
        .expect("protect TLS key");

    let binary = env!("CARGO_BIN_EXE_okf-http");
    let install = clean_command(binary, fixture.path(), &runtime_root)
        .arg("--install-browser")
        .arg("--browser-root")
        .arg(&browser_root)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("install TLS test browser");
    assert!(install.status.success());

    let mut add_user = clean_command(binary, fixture.path(), &runtime_root)
        .arg("user")
        .arg("add")
        .arg("alice")
        .arg("--password-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start persistent-user creation");
    add_user
        .stdin
        .take()
        .expect("user command stdin")
        .write_all(b"correct horse battery staple\n")
        .expect("write test password");
    let add_user = add_user.wait_with_output().expect("finish user creation");
    assert!(
        add_user.status.success(),
        "user creation failed: {}",
        String::from_utf8_lossy(&add_user.stderr)
    );

    let Some(port) = available_port() else {
        return;
    };
    let child = clean_command(binary, fixture.path(), &runtime_root)
        .arg("--authenticated")
        .arg("--tls-cert")
        .arg(&certificate_path)
        .arg("--tls-key")
        .arg(&private_key_path)
        .arg("--browser-root")
        .arg(&browser_root)
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start authenticated TLS server");
    let _server = ServerProcess(child);
    let trusted_certificate = CertificateDer::from(certificate.der().to_vec());
    wait_for_tls_server(port, &trusted_certificate);

    let (status, health) =
        https_get(port, "/api/v1/health", trusted_certificate.clone()).expect("fetch TLS health");
    assert_eq!(status, 200);
    let health: serde_json::Value = serde_json::from_str(&health).expect("TLS health JSON");
    assert_eq!(health["data"]["mode"], "authenticated-tls");
    assert_eq!(health["data"]["tls"]["protocol"], "TLS");
    assert!(health["data"]["tls"]["certificate_sha256"]
        .as_str()
        .is_some_and(|value| value.len() == 95));
    assert!(!health
        .to_string()
        .contains(private_key_path.to_string_lossy().as_ref()));

    let login_body = r#"{"username":"alice","password":"correct horse battery staple"}"#;
    let (login_status, login_headers, login_response) = https_request(
        port,
        "POST",
        "/api/v1/access/login",
        login_body,
        trusted_certificate,
        "",
    )
    .expect("HTTPS login");
    assert_eq!(login_status, 200, "login response: {login_response}");
    assert!(login_headers.contains("set-cookie: okf_session="));
    assert!(login_headers.contains("HttpOnly"));
    assert!(login_headers.contains("Secure"));
    assert!(login_headers.contains("SameSite=Strict"));
    assert!(!login_response.contains("correct horse battery staple"));

    assert!(
        http_get(port, "/api/v1/health").map_or(true, |(status, _)| status != 200),
        "authenticated mode must not silently serve plain HTTP"
    );
}
