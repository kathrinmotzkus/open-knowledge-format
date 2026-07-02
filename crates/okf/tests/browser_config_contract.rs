use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use okf::{
    browser_config_path, import_document_roots, load_browser_config, save_browser_config,
    BrowserConfig, BrowserConfigError, BrowserRoot, DocumentRoot, RootId,
};

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("okf-browser-config-{name}-{unique}"));
        fs::create_dir_all(&path).unwrap();
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

fn root(id: &str, path: &Path, priority: i64) -> BrowserRoot {
    BrowserRoot {
        root_id: RootId::parse(format!("urn:okf:root:{id}")).unwrap(),
        mount: Some(id[..3].to_ascii_lowercase()),
        path: path.to_path_buf(),
        enabled: true,
        priority,
        check_for_changes: false,
    }
}

#[test]
fn xdg_path_is_stable_and_never_depends_on_working_directory() {
    assert_eq!(
        browser_config_path(
            Some(Path::new("/tmp/custom-xdg")),
            Some(Path::new("/synthetic-user-home"))
        )
        .unwrap(),
        Path::new("/tmp/custom-xdg/okf/config.toml")
    );
    assert_eq!(
        browser_config_path(None, Some(Path::new("/synthetic-user-home"))).unwrap(),
        Path::new("/synthetic-user-home/.config/okf/config.toml")
    );
    assert!(matches!(
        browser_config_path(Some(Path::new("relative")), None),
        Err(BrowserConfigError::RelativeXdgPath)
    ));
}

#[test]
fn schema_one_roundtrips_privately_atomically_and_orders_enabled_roots() {
    let fixture = TestDirectory::new("roundtrip");
    let path = fixture.path().join("xdg/okf/config.toml");
    let mut config = BrowserConfig::default();
    config.roots_mut().push(root(
        "01JZY3ROOTAAAAAAAAAAAAAAA",
        Path::new("/knowledge/later"),
        200,
    ));
    config.roots_mut().push(root(
        "01JZY3ROOTBBBBBBBBBBBBBBB",
        Path::new("/knowledge/first"),
        10,
    ));
    let mut disabled = root("01JZY3ROOTCCCCCCCCCCCCCCC", Path::new("/knowledge/off"), 0);
    disabled.enabled = false;
    disabled.check_for_changes = true;
    config.roots_mut().push(disabled);
    save_browser_config(&path, &config).expect("save");
    let loaded = load_browser_config(&path).expect("load");
    assert_eq!(loaded, config);
    assert_eq!(
        loaded
            .enabled_document_roots()
            .iter()
            .map(|root| root.path())
            .collect::<Vec<_>>(),
        vec![Path::new("/knowledge/first"), Path::new("/knowledge/later")]
    );
    assert!(!fixture.path().join("xdg/okf/.config.toml.tmp-0").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn schema_zero_migrates_incrementally_and_future_or_malformed_schemas_fail() {
    let fixture = TestDirectory::new("migration");
    let path = fixture.path().join("config.toml");
    fs::write(
        &path,
        concat!(
            "schema_version = 0\n",
            "[[roots]]\nroot_id = \"urn:okf:root:01JZY3ROOTAAAAAAAAAAAAAAA\"\n",
            "mount = \"old\"\npath = \"/knowledge/old\"\n"
        ),
    )
    .unwrap();
    let migrated = load_browser_config(&path).expect("migrate");
    assert_eq!(migrated.schema_version(), 1);
    assert!(migrated.roots()[0].enabled);
    assert_eq!(migrated.roots()[0].priority, 0);
    assert!(!migrated.roots()[0].check_for_changes);

    fs::write(&path, "schema_version = 99\nroots = []\n").unwrap();
    assert!(matches!(
        load_browser_config(&path),
        Err(BrowserConfigError::UnsupportedSchema(99))
    ));
    fs::write(&path, "schema_version = nope\n").unwrap();
    assert!(matches!(
        load_browser_config(&path),
        Err(BrowserConfigError::Parse { .. })
    ));
}

#[test]
fn console_import_requires_portable_root_identity_and_never_changes_sources() {
    let fixture = TestDirectory::new("import");
    let root_path = fixture.path().join("knowledge");
    fs::create_dir(&root_path).unwrap();
    let index = root_path.join("index.md");
    let source = "---\nokf_root_id: urn:okf:root:01JZY3ROOTAAAAAAAAAAAAAAA\n---\n# Index\n";
    fs::write(&index, source).unwrap();
    let imported =
        import_document_roots(&[DocumentRoot::mounted("docs", &root_path)]).expect("import");
    assert_eq!(imported.roots()[0].mount.as_deref(), Some("docs"));
    assert_eq!(fs::read_to_string(index).unwrap(), source);

    let missing = fixture.path().join("missing-id");
    fs::create_dir(&missing).unwrap();
    fs::write(missing.join("index.md"), "# Index\n").unwrap();
    assert!(matches!(
        import_document_roots(&[DocumentRoot::new(&missing)]),
        Err(BrowserConfigError::MissingRootIdentity(_))
    ));
}

#[cfg(unix)]
#[test]
fn symlinked_configuration_files_are_rejected() {
    use std::os::unix::fs::symlink;
    let fixture = TestDirectory::new("symlink");
    let target = fixture.path().join("target.toml");
    let link = fixture.path().join("config.toml");
    fs::write(&target, "untouched").unwrap();
    symlink(&target, &link).unwrap();
    assert!(matches!(
        save_browser_config(&link, &BrowserConfig::default()),
        Err(BrowserConfigError::UnsafePath(_))
    ));
    assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
}
