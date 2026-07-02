use std::path::{Path, PathBuf};

use okf::{
    format_document_root_spec, merge_document_roots, merge_document_roots_with_browser,
    parse_document_root_spec, DocumentRoot, RootSpecError,
};

#[test]
fn browser_roots_are_below_dotenv_and_above_host_defaults() {
    let defaults = vec![DocumentRoot::mounted("default", "default")];
    let browser = vec![DocumentRoot::mounted("browser", "browser")];
    assert_eq!(
        merge_document_roots_with_browser(
            defaults.clone(),
            browser.clone(),
            None,
            None,
            Vec::new(),
            Vec::new()
        ),
        browser
    );
    let dotenv = vec![DocumentRoot::mounted("dotenv", "dotenv")];
    assert_eq!(
        merge_document_roots_with_browser(
            defaults,
            browser,
            Some(dotenv.clone()),
            None,
            Vec::new(),
            Vec::new()
        ),
        dotenv
    );
}

#[test]
fn parses_and_formats_mounted_and_unmounted_roots() {
    let mounted = parse_document_root_spec("okf=docs/knowledge").expect("mounted root");
    assert_eq!(mounted.mount(), Some(Path::new("okf")));
    assert_eq!(mounted.path(), Path::new("docs/knowledge"));
    assert_eq!(format_document_root_spec(&mounted), "okf=docs/knowledge");

    let plain = parse_document_root_spec("/srv/knowledge").expect("plain root");
    assert_eq!(plain.mount(), None);
    assert_eq!(plain.path(), Path::new("/srv/knowledge"));
}

#[test]
fn rejects_malformed_mount_specs() {
    assert_eq!(
        parse_document_root_spec("=docs"),
        Err(RootSpecError::EmptyMount)
    );
    assert_eq!(
        parse_document_root_spec("okf="),
        Err(RootSpecError::EmptyPath)
    );
    assert!(matches!(
        parse_document_root_spec("bad/mount=docs"),
        Err(RootSpecError::InvalidMount(_))
    ));
}

#[test]
fn cli_mounts_replace_persistent_mounts_and_additions_remain_fallbacks() {
    let defaults = vec![DocumentRoot::mounted("default", "default-docs")];
    let dotenv = Some(vec![
        DocumentRoot::mounted("okf", "old-primary"),
        DocumentRoot::mounted("okf", "old-fallback"),
        DocumentRoot::mounted("scql", "scql-docs"),
    ]);
    let environment = Some(vec![DocumentRoot::mounted("okf", "env-primary")]);
    let roots = merge_document_roots(
        defaults,
        dotenv,
        environment,
        vec![DocumentRoot::mounted("okf", "cli-primary")],
        vec![DocumentRoot::mounted("okf", "cli-fallback")],
    );

    assert_eq!(
        roots
            .iter()
            .map(|root| root.path().to_path_buf())
            .collect::<Vec<_>>(),
        vec![PathBuf::from("cli-primary"), PathBuf::from("cli-fallback")]
    );
}
