use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use okf::{
    scan_document_root, AdmissionError, AdmissionLimits, AdmissionStatus, AdmittedFormat,
    LimitKind, RejectionReason,
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        Self::new_under(std::env::temp_dir(), name)
    }

    #[cfg(unix)]
    fn new_short(name: &str) -> Self {
        Self::new_under(PathBuf::from("/tmp"), name)
    }

    fn new_under(base: PathBuf, name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = base.join(format!("okf-admission-{name}-{unique}"));
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

fn rejection_map(inventory: &okf::AdmissionInventory) -> BTreeMap<&str, &RejectionReason> {
    inventory
        .rejected()
        .iter()
        .map(|entry| (entry.display_path(), entry.reason()))
        .collect()
}

#[test]
fn admits_only_supported_regular_utf8_text_and_reports_stable_reasons() {
    let root = TestDirectory::new("formats");
    fs::write(root.path().join("one.md"), "# One\n").expect("markdown");
    fs::write(root.path().join("two.markdown"), "# Two\n").expect("long markdown");
    fs::write(root.path().join("THREE.MD"), "# Three\n").expect("upper markdown");
    fs::write(root.path().join("data.csv"), "name,value\nalpha,1\n").expect("csv");
    fs::write(root.path().join("script.js"), "alert(1)\n").expect("unsupported");
    fs::write(root.path().join("fake.md"), b"%PDF-1.7\n").expect("binary signature");
    fs::write(root.path().join("nul.csv"), b"a,b\n\0,1\n").expect("nul");
    fs::write(root.path().join("invalid.md"), [0xff, 0xfe]).expect("invalid UTF-8");
    fs::write(root.path().join("control.md"), "safe\u{1b}unsafe\n").expect("control");

    let inventory = scan_document_root(root.path(), AdmissionLimits::default()).expect("scan");
    assert!(inventory.status().is_complete());
    assert!(inventory.can_confirm());
    let accepted = inventory
        .accepted()
        .iter()
        .map(|file| (file.path().as_str(), file.format(), file.media_type()))
        .collect::<Vec<_>>();
    assert_eq!(
        accepted,
        vec![
            (
                "THREE.MD",
                AdmittedFormat::Markdown,
                "text/markdown; charset=utf-8"
            ),
            ("data.csv", AdmittedFormat::Csv, "text/csv; charset=utf-8"),
            (
                "one.md",
                AdmittedFormat::Markdown,
                "text/markdown; charset=utf-8"
            ),
            (
                "two.markdown",
                AdmittedFormat::Markdown,
                "text/markdown; charset=utf-8"
            ),
        ]
    );
    let rejected = rejection_map(&inventory);
    assert_eq!(
        rejected["script.js"],
        &RejectionReason::UnsupportedExtension
    );
    assert_eq!(rejected["fake.md"], &RejectionReason::BinarySignature);
    assert_eq!(rejected["nul.csv"], &RejectionReason::NulByte);
    assert_eq!(rejected["invalid.md"], &RejectionReason::InvalidUtf8);
    assert_eq!(
        rejected["control.md"],
        &RejectionReason::SuspiciousControlCharacter
    );
    assert_eq!(RejectionReason::HiddenPath.code(), "hidden_path");
}

#[test]
fn hidden_paths_and_collisions_never_enter_the_accepted_inventory() {
    let root = TestDirectory::new("hidden-collision");
    fs::write(root.path().join(".env"), "SECRET=fixture\n").expect("hidden file");
    fs::create_dir(root.path().join(".ssh")).expect("hidden directory");
    fs::write(root.path().join(".ssh/id.md"), "# Never scanned\n").expect("nested hidden");
    fs::create_dir(root.path().join("Docs")).expect("Docs");
    let collision_fixture_available = fs::create_dir(root.path().join("docs")).is_ok();
    if collision_fixture_available {
        fs::write(root.path().join("Docs/Überblick.md"), "# One\n").expect("first collision");
        fs::write(root.path().join("docs/U\u{308}BERBLICK.md"), "# Two\n")
            .expect("second collision");
    }

    let inventory = scan_document_root(root.path(), AdmissionLimits::default()).expect("scan");
    let rejected = rejection_map(&inventory);
    assert_eq!(rejected[".env"], &RejectionReason::HiddenPath);
    assert_eq!(rejected[".ssh"], &RejectionReason::HiddenPath);
    assert!(!rejected.contains_key(".ssh/id.md"));
    if collision_fixture_available {
        assert!(inventory.accepted().is_empty());
        assert_eq!(
            rejected["Docs/Überblick.md"],
            &RejectionReason::PathCollision
        );
        assert_eq!(
            rejected["docs/U\u{308}BERBLICK.md"],
            &RejectionReason::PathCollision
        );
    } else {
        assert_eq!(
            inventory
                .accepted()
                .iter()
                .map(|file| file.path().as_str())
                .collect::<Vec<_>>(),
            Vec::<&str>::new()
        );
    }
}

#[cfg(unix)]
#[test]
fn symlinks_sockets_executables_and_non_utf8_names_are_rejected() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::os::unix::net::UnixListener;

    let root = TestDirectory::new_short("unix-kinds");
    fs::write(root.path().join("target.md"), "# Target\n").expect("target");
    symlink("target.md", root.path().join("inside-link.md")).expect("inside symlink");
    let socket_path = root.path().join("service.md");
    let listener = match UnixListener::bind(&socket_path) {
        Ok(listener) => Some(listener),
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                && std::env::var_os("OKF_REQUIRE_SOCKET_E2E").is_none() =>
        {
            None
        }
        Err(error) => panic!("socket fixture is required: {error}"),
    };
    let executable = root.path().join("executable.md");
    fs::write(&executable, "# not executable content\n").expect("executable fixture");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).expect("chmod executable");
    let non_utf8 = root
        .path()
        .join(OsString::from_vec(b"bad-\xff.md".to_vec()));
    fs::write(&non_utf8, "# Non UTF-8 path\n").expect("non UTF-8 path");

    let inventory = scan_document_root(root.path(), AdmissionLimits::default()).expect("scan");
    let rejected = rejection_map(&inventory);
    assert_eq!(rejected["inside-link.md"], &RejectionReason::Symlink);
    if listener.is_some() {
        assert_eq!(rejected["service.md"], &RejectionReason::Socket);
    }
    assert_eq!(rejected["executable.md"], &RejectionReason::Executable);
    assert!(inventory
        .rejected()
        .iter()
        .any(|entry| entry.reason() == &RejectionReason::NonUtf8Path));
    assert_eq!(
        inventory
            .accepted()
            .iter()
            .map(|file| file.path().as_str())
            .collect::<Vec<_>>(),
        vec!["target.md"]
    );
}

#[cfg(unix)]
#[test]
fn hidden_and_symlink_roots_are_refused_before_traversal() {
    use std::os::unix::fs::symlink;

    let parent = TestDirectory::new("root-policy");
    let hidden = parent.path().join(".ssh");
    fs::create_dir(&hidden).expect("hidden root");
    assert!(matches!(
        scan_document_root(&hidden, AdmissionLimits::default()),
        Err(AdmissionError::HiddenRoot { .. })
    ));

    let real = parent.path().join("real");
    let linked = parent.path().join("linked");
    fs::create_dir(&real).expect("real root");
    symlink(&real, &linked).expect("root symlink");
    assert!(matches!(
        scan_document_root(&linked, AdmissionLimits::default()),
        Err(AdmissionError::RootSymlink { .. })
    ));
}

#[test]
fn every_limit_stops_the_scan_and_makes_the_inventory_unconfirmable() {
    let root = TestDirectory::new("limits");
    fs::write(root.path().join("a.md"), "1234").expect("a");
    fs::write(root.path().join("b.md"), "5678").expect("b");
    fs::create_dir(root.path().join("nested")).expect("nested");
    fs::write(root.path().join("nested/c.md"), "90").expect("nested file");

    let cases = [
        (
            AdmissionLimits {
                max_entries: 1,
                ..AdmissionLimits::default()
            },
            LimitKind::Entries,
        ),
        (
            AdmissionLimits {
                max_depth: 1,
                ..AdmissionLimits::default()
            },
            LimitKind::Depth,
        ),
        (
            AdmissionLimits {
                max_file_bytes: 3,
                ..AdmissionLimits::default()
            },
            LimitKind::FileBytes,
        ),
        (
            AdmissionLimits {
                max_total_bytes: 6,
                ..AdmissionLimits::default()
            },
            LimitKind::TotalBytes,
        ),
    ];

    for (limits, expected_kind) in cases {
        let inventory = scan_document_root(root.path(), limits).expect("limited scan");
        assert!(!inventory.can_confirm());
        assert!(matches!(
            inventory.status(),
            AdmissionStatus::LimitExceeded { kind, .. } if *kind == expected_kind
        ));
    }
}

#[test]
fn invalid_limits_are_rejected_before_filesystem_scanning() {
    let limits = AdmissionLimits {
        max_entries: 0,
        ..AdmissionLimits::default()
    };
    assert!(matches!(
        scan_document_root("does-not-need-to-exist", limits),
        Err(AdmissionError::InvalidLimit {
            name: "max_entries",
            ..
        })
    ));
}

#[test]
fn every_rejection_reason_has_a_stable_machine_code() {
    let reasons = [
        (RejectionReason::HiddenPath, "hidden_path"),
        (RejectionReason::NonUtf8Path, "non_utf8_path"),
        (
            RejectionReason::InvalidPortablePath,
            "invalid_portable_path",
        ),
        (RejectionReason::Symlink, "symlink"),
        (RejectionReason::Socket, "socket"),
        (RejectionReason::Fifo, "fifo"),
        (RejectionReason::Device, "device"),
        (RejectionReason::NonRegularFile, "non_regular_file"),
        (
            RejectionReason::UnsupportedExtension,
            "unsupported_extension",
        ),
        (RejectionReason::Executable, "executable"),
        (RejectionReason::BinarySignature, "binary_signature"),
        (RejectionReason::InvalidUtf8, "invalid_utf8"),
        (RejectionReason::NulByte, "nul_byte"),
        (
            RejectionReason::SuspiciousControlCharacter,
            "suspicious_control_character",
        ),
        (RejectionReason::PathCollision, "path_collision"),
        (RejectionReason::ReadFailed, "read_failed"),
    ];
    for (reason, code) in reasons {
        assert_eq!(reason.code(), code);
    }
}
