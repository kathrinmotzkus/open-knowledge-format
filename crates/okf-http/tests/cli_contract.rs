use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use okf_http::{
    dotenv_value_from_source, help_text, parse_cli_with_sources, parse_cli_with_sources_and_proxy,
    parse_cli_with_sources_and_security, url_host, CliAction, LocalTlsCommand, ServerMode,
    UserCommand, BROWSER_ROOT_ENV_KEY, DEFAULT_HOST, ROOTS_ENV_KEY, SESSION_TOKEN_ENV_KEY,
};

fn os_args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn run_action(values: &[&str]) -> okf_http::ServerConfig {
    match parse_cli_with_sources(os_args(values), None, None, None, None).expect("parse") {
        CliAction::Run(config) => config,
        CliAction::InstallBrowser(_) => panic!("expected run action"),
        CliAction::Tls(_) => panic!("expected run action"),
        CliAction::User(_) => panic!("expected run action"),
        CliAction::ImportEnvRoots(_) => panic!("expected run action"),
        CliAction::Help => panic!("expected run action"),
    }
}

#[test]
fn parses_console_only_env_root_import() {
    let action = parse_cli_with_sources(os_args(&["roots", "import-env"]), None, None, None, None)
        .expect("parse root import");
    assert_eq!(
        action,
        CliAction::ImportEnvRoots(PathBuf::from("config.toml"))
    );
}

#[test]
fn parses_persistent_user_management_without_password_arguments() {
    let action = parse_cli_with_sources(
        os_args(&["user", "add", "Alice", "--password-stdin"]),
        None,
        None,
        None,
        None,
    )
    .expect("parse user add");
    assert_eq!(
        action,
        CliAction::User(UserCommand::Add {
            name: "Alice".to_string(),
            password_stdin: true,
        })
    );
    assert_eq!(
        parse_cli_with_sources(
            os_args(&["user", "grant", "alice", "editor"]),
            None,
            None,
            None,
            None,
        )
        .unwrap(),
        CliAction::User(UserCommand::Grant {
            name: "alice".to_string(),
            role: "editor".to_string(),
        })
    );
    assert!(parse_cli_with_sources(
        os_args(&["user", "disable", "alice", "--password-stdin"]),
        None,
        None,
        None,
        None,
    )
    .is_err());
    assert!(parse_cli_with_sources(
        os_args(&["user", "add", "alice", "secret-on-command-line"]),
        None,
        None,
        None,
        None,
    )
    .is_err());
}

#[test]
fn parses_guided_tls_management_commands_without_a_server_port() {
    for (name, expected) in [
        ("init", LocalTlsCommand::Init),
        ("status", LocalTlsCommand::Status),
        ("verify", LocalTlsCommand::Verify),
        ("renew", LocalTlsCommand::Renew),
    ] {
        let action = parse_cli_with_sources(os_args(&["tls", name]), None, None, None, None)
            .expect("parse TLS command");
        assert_eq!(action, CliAction::Tls(expected));
    }
    assert!(parse_cli_with_sources(os_args(&["tls"]), None, None, None, None).is_err());
    assert!(parse_cli_with_sources(os_args(&["tls", "unknown"]), None, None, None, None).is_err());
    assert!(
        parse_cli_with_sources(os_args(&["tls", "init", "8003"]), None, None, None, None).is_err()
    );
}

#[test]
fn parses_browser_install_and_force_without_a_port() {
    let action = parse_cli_with_sources(
        os_args(&[
            "--install-browser",
            "--browser-root",
            "/tmp/okf-browser",
            "--force",
        ]),
        None,
        None,
        None,
        None,
    )
    .expect("parse install");
    let CliAction::InstallBrowser(config) = action else {
        panic!("expected install action");
    };
    assert_eq!(config.browser_root(), Path::new("/tmp/okf-browser"));
    assert!(config.force());

    assert!(parse_cli_with_sources(
        os_args(&["--install-browser", "8003"]),
        None,
        None,
        None,
        None,
    )
    .is_err());
    assert!(parse_cli_with_sources(os_args(&["--force", "8003"]), None, None, None, None).is_err());
}

#[test]
fn parses_required_unprivileged_port_with_default_host() {
    let config = run_action(&["8003"]);

    assert_eq!(config.host(), DEFAULT_HOST);
    assert_eq!(config.mode(), ServerMode::ReadOnly);
    assert!(config.pairing_code().is_none());
    assert!(config.tls().is_none());
    assert_eq!(config.port(), 8003);
    assert!(config.browser_root().ends_with("docs-browser"));
    assert!(config.roots().is_empty());
    assert_eq!(config.session_token().len(), 64);
    assert!(!config.remote_access());
    assert!(!config.expose_physical_paths());
    let debug = format!("{config:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains(config.session_token()));
}

#[test]
fn accepts_explicit_host_and_document_roots() {
    let config = run_action(&[
        "--host",
        "192.0.2.10",
        "--authenticated",
        "--tls-cert",
        "/tmp/okf-cert.pem",
        "--tls-key",
        "/tmp/okf-key.pem",
        "--allow-remote",
        "--session-token",
        "remote-session-token-0000000000000000",
        "--browser-root",
        "/tmp/docs-browser",
        "--root",
        "scanlab=docs",
        "--root=crates/okf/docs/knowledge",
        "8003",
    ]);

    assert_eq!(config.host(), "192.0.2.10");
    assert_eq!(config.mode(), ServerMode::AuthenticatedTls);
    assert_eq!(
        config.tls().expect("TLS files").certificate(),
        Path::new("/tmp/okf-cert.pem")
    );
    assert!(config.remote_access());
    assert_eq!(config.port(), 8003);
    assert_eq!(config.browser_root(), Path::new("/tmp/docs-browser"));
    assert_eq!(config.roots().len(), 2);
    assert_eq!(config.roots()[0].mount(), Some(Path::new("scanlab")));
    assert_eq!(config.roots()[0].path(), Path::new("docs"));
    assert_eq!(config.roots()[1].mount(), None);
    assert_eq!(
        config.roots()[1].path(),
        Path::new("crates/okf/docs/knowledge")
    );
}

#[test]
fn scanlab_compatibility_routes_are_opt_in() {
    assert!(!run_action(&["8003"]).scanlab_compat());
    assert!(run_action(&["--scanlab-compat", "8003"]).scanlab_compat());
}

#[test]
fn physical_api_paths_are_debug_opt_in() {
    assert!(!run_action(&["8003"]).expose_physical_paths());
    assert!(parse_cli_with_sources(
        os_args(&["--expose-physical-paths", "8003"]),
        None,
        None,
        None,
        None,
    )
    .is_err());
    assert!(
        run_action(&["--local-editor", "--expose-physical-paths", "8003"]).expose_physical_paths()
    );
}

#[test]
fn local_editor_is_explicit_and_loopback_only() {
    let editor = run_action(&["--local-editor", "8003"]);
    assert_eq!(editor.mode(), ServerMode::LocalEditor);
    let pairing_code = editor.pairing_code().expect("local pairing code");
    assert_eq!(pairing_code.len(), 14);
    assert_eq!(pairing_code.as_bytes()[4], b'-');
    assert_eq!(pairing_code.as_bytes()[9], b'-');
    let debug = format!("{editor:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains(pairing_code));

    let remote = parse_cli_with_sources_and_security(
        os_args(&[
            "--local-editor",
            "--host",
            "0.0.0.0",
            "--allow-remote",
            "--session-token",
            "remote-session-token-0000000000000000",
            "8003",
        ]),
        None,
        None,
        None,
        None,
        None,
    )
    .expect_err("local editor must remain on loopback")
    .to_string();
    assert!(remote.contains("loopback"));
}

#[test]
fn authenticated_mode_requires_complete_tls_configuration() {
    for args in [
        vec!["--authenticated", "8003"],
        vec!["--authenticated", "--tls-cert", "/tmp/cert.pem", "8003"],
        vec![
            "--tls-cert",
            "/tmp/cert.pem",
            "--tls-key",
            "/tmp/key.pem",
            "8003",
        ],
        vec![
            "--local-editor",
            "--authenticated",
            "--tls-cert",
            "/tmp/cert.pem",
            "--tls-key",
            "/tmp/key.pem",
            "8003",
        ],
    ] {
        assert!(
            parse_cli_with_sources(os_args(&args), None, None, None, None).is_err(),
            "{args:?}"
        );
    }

    let config = run_action(&[
        "--authenticated",
        "--tls-cert",
        "/tmp/cert.pem",
        "--tls-key",
        "/tmp/key.pem",
        "8003",
    ]);
    assert_eq!(config.mode(), ServerMode::AuthenticatedTls);
    assert!(config.pairing_code().is_none());
    assert_eq!(
        config.tls().expect("TLS files").private_key(),
        Path::new("/tmp/key.pem")
    );
    let debug = format!("{config:?}");
    assert!(debug.contains("configured"));
    assert!(!debug.contains("/tmp/key.pem"));
}

#[test]
fn combines_persistent_roots_with_cli_roots_for_current_process() {
    let dotenv_roots = env::join_paths([
        PathBuf::from("scanlab=docs"),
        PathBuf::from("okf=crates/okf/docs/knowledge"),
    ])
    .expect("join dotenv roots");

    let action = parse_cli_with_sources(
        os_args(&["--root", "scql=crates/scql/docs/knowledge", "8003"]),
        None,
        Some(dotenv_roots),
        None,
        None,
    )
    .expect("parse");

    let CliAction::Run(config) = action else {
        panic!("expected run action");
    };
    assert_eq!(config.roots().len(), 3);
    assert_eq!(config.roots()[0].mount(), Some(Path::new("scql")));
    assert_eq!(config.roots()[1].mount(), Some(Path::new("scanlab")));
    assert_eq!(config.roots()[2].mount(), Some(Path::new("okf")));
}

#[test]
fn cli_roots_replace_persistent_mounts_and_add_root_keeps_fallbacks() {
    let dotenv_roots = env::join_paths([
        PathBuf::from("okf=old-primary"),
        PathBuf::from("okf=old-fallback"),
        PathBuf::from("scql=scql-docs"),
    ])
    .expect("dotenv roots");
    let action = parse_cli_with_sources(
        os_args(&[
            "--root",
            "okf=cli-primary",
            "--add-root",
            "okf=cli-fallback",
            "8003",
        ]),
        None,
        Some(dotenv_roots),
        None,
        None,
    )
    .expect("parse");
    let CliAction::Run(config) = action else {
        panic!("expected run action");
    };
    assert_eq!(
        config
            .roots()
            .iter()
            .map(|root| root.path().to_path_buf())
            .collect::<Vec<_>>(),
        vec![
            PathBuf::from("cli-primary"),
            PathBuf::from("scql-docs"),
            PathBuf::from("cli-fallback"),
        ]
    );
}

#[test]
fn rejects_malformed_root_specs_and_accepts_ipv6_hosts() {
    let error = parse_cli_with_sources(
        os_args(&["--root", "bad/mount=docs", "8003"]),
        None,
        None,
        None,
        None,
    )
    .expect_err("invalid mount")
    .to_string();
    assert!(error.contains("invalid document root"));

    let ipv6 = run_action(&["--host", "::1", "8003"]);
    assert_eq!(ipv6.host(), "::1");
    assert_eq!(url_host(ipv6.host()), "[::1]");
    assert_eq!(
        run_action(&["--host", "localhost", "8003"]).host(),
        "localhost"
    );
    assert_eq!(
        run_action(&["--host", "127.0.0.1", "8003"]).host(),
        "127.0.0.1"
    );
    assert!(parse_cli_with_sources(
        os_args(&["--host", "0.0.0.0", "8003"]),
        None,
        None,
        None,
        None,
    )
    .is_err());

    let invalid = parse_cli_with_sources(
        os_args(&["--host", "http://localhost", "8003"]),
        None,
        None,
        None,
        None,
    )
    .expect_err("URL is not a host")
    .to_string();
    assert!(invalid.contains("invalid bind host"));
}

#[test]
fn remote_binding_requires_explicit_opt_in_and_authentication() {
    let without_opt_in = parse_cli_with_sources(
        os_args(&["--host", "192.0.2.10", "8003"]),
        None,
        None,
        None,
        None,
    )
    .expect_err("remote host must be rejected")
    .to_string();
    assert!(without_opt_in.contains("--authenticated"));

    let without_token = parse_cli_with_sources(
        os_args(&[
            "--host",
            "192.0.2.10",
            "--authenticated",
            "--tls-cert",
            "/tmp/cert.pem",
            "--tls-key",
            "/tmp/key.pem",
            "--allow-remote",
            "8003",
        ]),
        None,
        None,
        None,
        None,
    )
    .expect_err("remote host requires authentication")
    .to_string();
    assert!(without_token.contains("session-token"));

    let action = parse_cli_with_sources_and_security(
        os_args(&[
            "--host",
            "192.0.2.10",
            "--authenticated",
            "--tls-cert",
            "/tmp/cert.pem",
            "--tls-key",
            "/tmp/key.pem",
            "--allow-remote",
            "8003",
        ]),
        None,
        None,
        None,
        None,
        Some(OsString::from("environment-session-token-000000000000")),
    )
    .expect("authenticated remote bind");
    let CliAction::Run(config) = action else {
        panic!("expected run action");
    };
    assert!(config.remote_access());
    assert_eq!(config.mode(), ServerMode::AuthenticatedTls);
    assert_eq!(
        config.session_token(),
        "environment-session-token-000000000000"
    );

    let unspecified = parse_cli_with_sources_and_security(
        os_args(&[
            "--host",
            "0.0.0.0",
            "--authenticated",
            "--tls-cert",
            "/tmp/cert.pem",
            "--tls-key",
            "/tmp/key.pem",
            "--allow-remote",
            "8003",
        ]),
        None,
        None,
        None,
        None,
        Some(OsString::from("environment-session-token-000000000000")),
    )
    .expect_err("unspecified TLS bind must be rejected")
    .to_string();
    assert!(unspecified.contains("concrete bind address"));
}

#[test]
fn trusted_proxy_requires_loopback_backend_tls_public_origin_and_secret() {
    let args = os_args(&[
        "--authenticated",
        "--tls-cert",
        "/tmp/cert.pem",
        "--tls-key",
        "/tmp/key.pem",
        "--trusted-proxy",
        "--public-origin",
        "https://knowledge.example:8443",
        "8443",
    ]);
    let action = parse_cli_with_sources_and_proxy(
        args.clone(),
        None,
        None,
        None,
        None,
        None,
        Some(OsString::from(
            "proxy-secret-token-000000000000000000000000",
        )),
    )
    .expect("trusted proxy configuration");
    let CliAction::Run(config) = action else {
        panic!("expected run action");
    };
    let proxy = config.trusted_proxy().expect("trusted proxy");
    assert_eq!(proxy.public_origin(), "https://knowledge.example:8443");
    assert_eq!(proxy.public_authority(), "knowledge.example:8443");
    assert!(!config.remote_access());
    assert!(!format!("{config:?}").contains("proxy-secret-token"));

    for invalid in [
        os_args(&[
            "--authenticated",
            "--tls-cert",
            "/tmp/cert.pem",
            "--tls-key",
            "/tmp/key.pem",
            "--trusted-proxy",
            "8443",
        ]),
        os_args(&[
            "--trusted-proxy",
            "--public-origin",
            "https://knowledge.example",
            "8443",
        ]),
        os_args(&[
            "--authenticated",
            "--tls-cert",
            "/tmp/cert.pem",
            "--tls-key",
            "/tmp/key.pem",
            "--trusted-proxy",
            "--public-origin",
            "http://knowledge.example",
            "8443",
        ]),
    ] {
        assert!(parse_cli_with_sources_and_proxy(
            invalid,
            None,
            None,
            None,
            None,
            None,
            Some(OsString::from(
                "proxy-secret-token-000000000000000000000000"
            )),
        )
        .is_err());
    }
    assert!(parse_cli_with_sources_and_proxy(args, None, None, None, None, None, None,).is_err());
}

#[test]
fn process_environment_roots_take_precedence_over_dotenv_roots() {
    let env_roots = env::join_paths([PathBuf::from("env=docs")]).expect("env roots");
    let dotenv_roots = env::join_paths([PathBuf::from("dotenv=docs")]).expect("dotenv roots");

    let action = parse_cli_with_sources(
        os_args(&["8003"]),
        Some(env_roots),
        Some(dotenv_roots),
        None,
        None,
    )
    .expect("parse");

    let CliAction::Run(config) = action else {
        panic!("expected run action");
    };
    assert_eq!(config.roots().len(), 1);
    assert_eq!(config.roots()[0].mount(), Some(Path::new("env")));
}

#[test]
fn rejects_missing_invalid_and_privileged_ports() {
    let missing = parse_cli_with_sources(os_args(&[]), None, None, None, None)
        .expect_err("missing port should fail")
        .to_string();
    assert!(missing.contains("missing required port"));

    let invalid = parse_cli_with_sources(os_args(&["not-a-port"]), None, None, None, None)
        .expect_err("invalid port should fail")
        .to_string();
    assert!(invalid.contains("invalid port"));

    let privileged = parse_cli_with_sources(os_args(&["80"]), None, None, None, None)
        .expect_err("privileged port should fail")
        .to_string();
    assert!(privileged.contains("privileged"));
}

#[test]
fn help_text_documents_host_port_roots_and_env_policy() {
    let action =
        parse_cli_with_sources(os_args(&["--help"]), None, None, None, None).expect("help");
    assert_eq!(action, CliAction::Help);

    let help = help_text();
    assert!(help.contains("127.0.0.1"));
    assert!(help.contains("0.0.0.0"));
    assert!(help.contains("<port>"));
    assert!(help.contains("--root"));
    assert!(help.contains("--add-root"));
    assert!(help.contains("--browser-root"));
    assert!(help.contains("--scanlab-compat"));
    assert!(help.contains("--local-editor"));
    assert!(help.contains("--authenticated"));
    assert!(help.contains("--tls-cert"));
    assert!(help.contains("--tls-key"));
    assert!(help.contains("read-only"));
    assert!(help.contains("--allow-remote"));
    assert!(help.contains("--session-token"));
    assert!(help.contains("--expose-physical-paths"));
    assert!(help.contains(SESSION_TOKEN_ENV_KEY));
    assert!(help.contains(ROOTS_ENV_KEY));
    assert!(help.contains(BROWSER_ROOT_ENV_KEY));
    assert!(help.contains(".env"));
}

#[test]
fn browser_root_comes_from_env_dotenv_or_cli_override() {
    let dotenv_action = parse_cli_with_sources(
        os_args(&["8003"]),
        None,
        None,
        None,
        Some(OsString::from("/tmp/from-dotenv")),
    )
    .expect("parse dotenv browser root");
    let CliAction::Run(dotenv_config) = dotenv_action else {
        panic!("expected run action");
    };
    assert_eq!(dotenv_config.browser_root(), Path::new("/tmp/from-dotenv"));

    let env_action = parse_cli_with_sources(
        os_args(&["8003"]),
        None,
        None,
        Some(OsString::from("/tmp/from-env")),
        Some(OsString::from("/tmp/from-dotenv")),
    )
    .expect("parse env browser root");
    let CliAction::Run(env_config) = env_action else {
        panic!("expected run action");
    };
    assert_eq!(env_config.browser_root(), Path::new("/tmp/from-env"));

    let cli_config = run_action(&["--browser-root", "/tmp/from-cli", "8003"]);
    assert_eq!(cli_config.browser_root(), Path::new("/tmp/from-cli"));
}

#[test]
fn dotenv_parser_reads_persistent_root_configuration() {
    let source = r#"
# comment
OKF_DOCUMENT_ROOTS='scanlab=docs:okf=crates/okf/docs/knowledge'
OTHER=value
"#;

    assert_eq!(
        dotenv_value_from_source(source, ROOTS_ENV_KEY),
        Some(OsString::from("scanlab=docs:okf=crates/okf/docs/knowledge"))
    );
}
