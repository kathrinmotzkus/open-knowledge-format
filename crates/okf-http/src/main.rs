use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use okf_http::{
    app, app_with_prepared_tls_and_users, help_text, import_env_roots, initialize_local_tls,
    install_browser_assets, local_tls_status, parse_cli, prepare_tls, renew_local_tls, url_host,
    validate_browser_root, verify_local_tls, CliAction, LocalTlsCommand, LocalTlsPaths, ServerMode,
    UserCommand, UserStore,
};

#[tokio::main]
async fn main() -> ExitCode {
    let action = match parse_cli(env::args_os().skip(1)) {
        Ok(action) => action,
        Err(error) => {
            eprintln!("error: {error}\n");
            eprintln!("{}", help_text());
            return ExitCode::from(2);
        }
    };

    match action {
        CliAction::Help => {
            print!("{}", help_text());
            ExitCode::SUCCESS
        }
        CliAction::InstallBrowser(config) => match install_browser_assets(&config) {
            Ok(report) => {
                if report.changed {
                    println!(
                        "installed {} OKF browser assets in {}",
                        report.installed_files,
                        report.browser_root.display()
                    );
                } else {
                    println!(
                        "OKF browser assets are already current in {}",
                        report.browser_root.display()
                    );
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::from(1)
            }
        },
        CliAction::Tls(command) => run_tls_command(command),
        CliAction::User(command) => run_user_command(command),
        CliAction::ImportEnvRoots(path) => match import_env_roots(&path) {
            Ok(count) => {
                println!("imported {count} document root(s) into {}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::from(1)
            }
        },
        CliAction::Run(config) => {
            let prepared_tls = if config.mode() == ServerMode::AuthenticatedTls {
                let Some(files) = config.tls() else {
                    eprintln!("error: authenticated TLS mode has no certificate configuration");
                    return ExitCode::from(1);
                };
                match prepare_tls(files, config.host()) {
                    Ok(prepared) => Some(prepared),
                    Err(error) => {
                        eprintln!("error: TLS validation failed: {error}");
                        return ExitCode::from(1);
                    }
                }
            } else {
                None
            };
            let user_store = if config.mode() == ServerMode::AuthenticatedTls {
                match UserStore::from_environment() {
                    Ok(store) => Some(store),
                    Err(error) => {
                        eprintln!("error: failed to initialize persistent authentication: {error}");
                        return ExitCode::from(1);
                    }
                }
            } else {
                None
            };
            println!(
                "{}",
                serde_json::json!({
                    "event": "server_starting",
                    "mode": config.mode().as_str(),
                    "host": config.host(),
                    "port": config.port(),
                    "roots": config.roots().len(),
                    "remote_access": config.remote_access(),
                    "trusted_proxy": config.trusted_proxy().map(|proxy| proxy.public_origin()),
                    "physical_paths_exposed": config.expose_physical_paths(),
                    "tls": prepared_tls.as_ref().map(|prepared| prepared.status()),
                    "browser_root": config.expose_physical_paths().then(|| config.browser_root().display().to_string()),
                })
            );
            for (index, root) in config.roots().iter().enumerate() {
                println!(
                    "{}",
                    serde_json::json!({
                        "event": "document_root",
                        "index": index,
                        "mount": root.mount().map(|mount| mount.to_string_lossy()),
                        "usable": root.path().is_dir(),
                        "path": config.expose_physical_paths().then(|| root.path().display().to_string()),
                    })
                );
                if !root.path().is_dir() {
                    eprintln!("warning: document root {index} is missing or not a directory");
                } else if let Err(error) = std::fs::read_dir(root.path()) {
                    eprintln!("warning: document root {index} is not readable: {error}");
                }
            }
            if config.remote_access() {
                eprintln!("warning: explicit remote authenticated-TLS binding is enabled");
            }
            if let Some(proxy) = config.trusted_proxy() {
                eprintln!(
                    "warning: trusted reverse-proxy mode is enabled for {} and direct backend requests will be rejected",
                    proxy.public_origin()
                );
            }
            if let Err(error) = validate_browser_root(config.browser_root()) {
                eprintln!("error: {error}");
                eprintln!(
                    "provision the packaged browser with: okf-http --install-browser --browser-root {:?}",
                    config.browser_root()
                );
                return ExitCode::from(1);
            }

            let _token_file = if config.mode() == ServerMode::LocalEditor {
                if let Some(pairing_code) = config.pairing_code() {
                    println!("local-editor-pairing-code {pairing_code}");
                    println!("local-editor-pairing-expires-in-seconds 300");
                }
                let token_file =
                    match SessionTokenFile::create(config.port(), config.session_token()) {
                        Ok(token_file) => token_file,
                        Err(error) => {
                            eprintln!(
                                "error: failed to create private session-token file: {error}"
                            );
                            return ExitCode::from(1);
                        }
                    };
                println!("session-token-file {}", token_file.path().display());
                Some(token_file)
            } else {
                println!("read-only mode: mutation and token-spending routes are disabled");
                None
            };

            let bind_target = (config.host(), config.port());
            let display_target = format!("{}:{}", url_host(config.host()), config.port());
            let server_result = if let Some(prepared) = prepared_tls {
                let bind_address = match tokio::net::lookup_host(bind_target).await {
                    Ok(mut addresses) => match addresses.next() {
                        Some(address) => address,
                        None => {
                            eprintln!("error: bind host {display_target} resolved to no addresses");
                            return ExitCode::from(1);
                        }
                    },
                    Err(error) => {
                        eprintln!("error: failed to resolve bind host {display_target}: {error}");
                        return ExitCode::from(1);
                    }
                };
                let handle = axum_server::Handle::new();
                let shutdown_handle = handle.clone();
                tokio::spawn(async move {
                    shutdown_signal().await;
                    shutdown_handle.graceful_shutdown(Some(Duration::from_secs(30)));
                });
                println!("serving OKF browser at https://{display_target}/docs-browser/index.html");
                let router = app_with_prepared_tls_and_users(
                    config,
                    &prepared,
                    user_store.expect("authenticated mode initialized a user store"),
                );
                axum_server::tls_rustls::bind_rustls(bind_address, prepared.config())
                    .handle(handle)
                    .serve(router.into_make_service())
                    .await
            } else {
                let listener = match tokio::net::TcpListener::bind(bind_target).await {
                    Ok(listener) => listener,
                    Err(error) => {
                        eprintln!("error: failed to bind {display_target}: {error}");
                        return ExitCode::from(1);
                    }
                };
                println!("serving OKF browser at http://{display_target}/docs-browser/index.html");
                axum::serve(listener, app(config))
                    .with_graceful_shutdown(shutdown_signal())
                    .await
            };
            match server_result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("error: server failed: {error}");
                    ExitCode::from(1)
                }
            }
        }
    }
}

fn run_user_command(command: UserCommand) -> ExitCode {
    let store = match UserStore::from_environment() {
        Ok(store) => store,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(1);
        }
    };
    let result = match &command {
        UserCommand::Add {
            name,
            password_stdin,
        } => okf_http::read_password(*password_stdin).and_then(|password| {
            store
                .add_user(name, &password)
                .map(|user| serde_json::json!(user))
        }),
        UserCommand::Passwd {
            name,
            password_stdin,
        } => okf_http::read_password(*password_stdin).and_then(|password| {
            store
                .change_password(name, &password)
                .map(|()| serde_json::json!({"name": name}))
        }),
        UserCommand::Disable { name } => store
            .disable_user(name)
            .map(|()| serde_json::json!({"name": name})),
        UserCommand::Remove { name } => store
            .remove_user(name)
            .map(|()| serde_json::json!({"name": name})),
        UserCommand::List => store.list_users().map(|users| serde_json::json!(users)),
        UserCommand::Grant { name, role } => store
            .grant_role(name, role)
            .map(|()| serde_json::json!({"name": name, "role": role})),
        UserCommand::Revoke { name, role } => store
            .revoke_role(name, role)
            .map(|()| serde_json::json!({"name": name, "role": role})),
    };
    match result {
        Ok(data) => {
            let event = match command {
                UserCommand::Add { .. } => "user_added",
                UserCommand::Passwd { .. } => "password_changed",
                UserCommand::Disable { .. } => "user_disabled",
                UserCommand::Remove { .. } => "user_removed",
                UserCommand::List => "users_listed",
                UserCommand::Grant { .. } => "role_granted",
                UserCommand::Revoke { .. } => "role_revoked",
            };
            println!("{}", serde_json::json!({"event": event, "data": data}));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: user operation failed: {error}");
            ExitCode::from(1)
        }
    }
}

fn run_tls_command(command: LocalTlsCommand) -> ExitCode {
    let paths = match LocalTlsPaths::from_environment() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(1);
        }
    };
    let result = match command {
        LocalTlsCommand::Init => initialize_local_tls(&paths),
        LocalTlsCommand::Status => local_tls_status(&paths),
        LocalTlsCommand::Verify => verify_local_tls(&paths),
        LocalTlsCommand::Renew => renew_local_tls(&paths),
    };
    match result {
        Ok(status) => {
            println!(
                "{}",
                serde_json::json!({
                    "event": match command {
                        LocalTlsCommand::Init => "tls_initialized",
                        LocalTlsCommand::Status => "tls_status",
                        LocalTlsCommand::Verify => "tls_verified",
                        LocalTlsCommand::Renew => "tls_renewed",
                    },
                    "state_directory": paths.directory(),
                    "ca_certificate": paths.ca_certificate(),
                    "server_certificate_chain": paths.server_certificate_chain(),
                    "server_private_key": paths.server_private_key(),
                    "status": status,
                })
            );
            if matches!(command, LocalTlsCommand::Init | LocalTlsCommand::Renew) {
                eprintln!(
                    "The CA certificate was not installed into any trust store. Import only {} into a user or browser trust store if you choose to trust it; never import a private-key file.",
                    paths.ca_certificate().display()
                );
                eprintln!(
                    "Start local HTTPS with: okf-http --authenticated --tls-cert {:?} --tls-key {:?} 8443",
                    paths.server_certificate_chain(),
                    paths.server_private_key()
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: local TLS operation failed: {error}");
            ExitCode::from(1)
        }
    }
}

struct SessionTokenFile {
    path: PathBuf,
}

impl SessionTokenFile {
    fn create(port: u16, token: &str) -> io::Result<Self> {
        let directory = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .unwrap_or_else(env::temp_dir);
        let path = directory.join(format!("okf-http-{port}-{}.token", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(token.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(Self { path })
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for SessionTokenFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    eprintln!("{}", serde_json::json!({"event": "server_shutdown"}));
}
