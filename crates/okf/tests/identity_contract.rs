use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use okf::{
    detect_path_collisions, DocumentId, DocumentRoot, IdentityError, PortablePath, Repository,
    RootId,
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("okf-identity-{name}-{unique}"));
        fs::create_dir_all(&path).expect("create fixture root");
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

#[test]
fn stable_identity_namespaces_are_typed_and_validated() {
    let root = RootId::parse("urn:okf:root:01JZY3E8QJRM8JVM3PPD5P4G7A").expect("root id");
    let document =
        DocumentId::parse("urn:okf:document:01JZY3G0C5ZX6QM7BQ5B4BP4AY").expect("document id");
    assert_eq!(root.as_str(), "urn:okf:root:01JZY3E8QJRM8JVM3PPD5P4G7A");
    assert_eq!(
        document.as_str(),
        "urn:okf:document:01JZY3G0C5ZX6QM7BQ5B4BP4AY"
    );
    assert_eq!(
        RootId::parse(document.as_str()).unwrap_err(),
        IdentityError::InvalidPrefix
    );
    assert_eq!(
        DocumentId::parse("urn:okf:document:too-short").unwrap_err(),
        IdentityError::InvalidLength
    );
}

#[test]
fn portable_paths_preserve_display_text_and_reject_ambiguous_components() {
    let path = PortablePath::parse("Wissen mit Leerzeichen/Überblick.md").expect("portable path");
    assert_eq!(path.as_str(), "Wissen mit Leerzeichen/Überblick.md");
    assert_eq!(
        PortablePath::from_path(Path::new("Wissen mit Leerzeichen/Überblick.md"))
            .expect("platform path"),
        path
    );

    for (candidate, expected) in [
        ("", IdentityError::EmptyPath),
        ("/absolute.md", IdentityError::AbsoluteOrPrefixed),
        ("C:/prefixed.md", IdentityError::AbsoluteOrPrefixed),
        ("a//b.md", IdentityError::EmptyComponent),
        ("./a.md", IdentityError::CurrentDirectory),
        ("a/../b.md", IdentityError::ParentDirectory),
        ("a\\b.md", IdentityError::Backslash),
        ("a/line\nbreak.md", IdentityError::ControlCharacter),
    ] {
        assert_eq!(PortablePath::parse(candidate).unwrap_err(), expected);
    }
}

#[test]
fn collision_keys_fold_case_and_unicode_without_rewriting_display_paths() {
    let paths = [
        "Hosts/Überblick.md",
        "hosts/U\u{308}BERBLICK.md",
        "Dienste/Straße.md",
        "dienste/STRASSE.md",
        "Greek/οδός.md",
        "greek/ΟΔΌΣ.md",
    ]
    .into_iter()
    .map(|path| PortablePath::parse(path).expect("portable fixture"))
    .collect::<Vec<_>>();

    let collisions = detect_path_collisions(paths);
    assert_eq!(collisions.len(), 3);
    assert!(collisions
        .iter()
        .all(|collision| collision.paths().len() == 2));
    assert!(collisions.iter().any(|collision| {
        collision
            .paths()
            .iter()
            .any(|path| path.as_str() == "Dienste/Straße.md")
    }));
}

#[test]
fn repository_identities_survive_mount_changes_reordering_and_renames() {
    let root = TestDirectory::new("survival");
    fs::write(
        root.path().join("index.md"),
        "---\nokf_root_id: urn:okf:root:01JZY3E8QJRM8JVM3PPD5P4G7A\n---\n# Index\n",
    )
    .expect("write root index");
    let original = root.path().join("Name mit Leerzeichen.md");
    fs::write(
        &original,
        "---\ntype: Reference\nokf_document_id: urn:okf:document:01JZY3G0C5ZX6QM7BQ5B4BP4AY\n---\n# Stable\n",
    )
    .expect("write document");

    let unmounted = Repository::open([DocumentRoot::new(root.path())]).expect("unmounted");
    let document = unmounted
        .documents()
        .iter()
        .find(|document| document.document_id().is_some())
        .expect("identified document");
    assert_eq!(
        document.root_id().map(RootId::as_str),
        Some("urn:okf:root:01JZY3E8QJRM8JVM3PPD5P4G7A")
    );
    assert_eq!(
        document.relative_path(),
        Path::new("Name mit Leerzeichen.md")
    );

    let mounted =
        Repository::open([DocumentRoot::mounted("temporary-mount", root.path())]).expect("mounted");
    let document = mounted
        .documents()
        .iter()
        .find(|document| document.document_id().is_some())
        .expect("mounted identified document");
    assert_eq!(
        document.document_id().map(DocumentId::as_str),
        Some("urn:okf:document:01JZY3G0C5ZX6QM7BQ5B4BP4AY")
    );
    assert_eq!(
        document.relative_path(),
        Path::new("temporary-mount/Name mit Leerzeichen.md")
    );

    let renamed = root.path().join("Neuer Name.md");
    fs::rename(&original, &renamed).expect("rename without content rewrite");
    let reopened = Repository::open([DocumentRoot::new(root.path())]).expect("reopened");
    let document = reopened
        .documents()
        .iter()
        .find(|document| document.document_id().is_some())
        .expect("renamed identified document");
    assert_eq!(
        document.document_id().map(DocumentId::as_str),
        Some("urn:okf:document:01JZY3G0C5ZX6QM7BQ5B4BP4AY")
    );
    assert_eq!(document.relative_path(), Path::new("Neuer Name.md"));
}
