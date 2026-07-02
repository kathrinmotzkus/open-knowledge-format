use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use okf::{
    build_root_proposal, AdmissionLimits, DocumentRoot, ProposalConflictCode, ProposalError,
    ProposalTreeState, ProposalValidation, RootProposalContext, RootProposalKind,
    RootProposalRequest, RootProposalStore,
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("okf-proposal-{name}-{unique}"));
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

fn write_fixture(root: &Path, id: &str) {
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(
        root.join("index.md"),
        format!("---\nokf_root_id: urn:okf:root:{id}\ntype: Index\n---\n# Root\n"),
    )
    .unwrap();
    fs::write(root.join("concept.md"), "# Pending concept\n").unwrap();
    fs::write(root.join("nested/topic.md"), "# Nested topic\n").unwrap();
    fs::write(root.join("data.csv"), "name,value\none,1\n").unwrap();
    fs::write(root.join("ignored.txt"), "not admitted\n").unwrap();
}

fn request(root: &Path, kind: RootProposalKind) -> RootProposalRequest {
    RootProposalRequest {
        root: root.to_path_buf(),
        mount: Some("knowledge".to_string()),
        kind,
        limits: AdmissionLimits::default(),
        context: RootProposalContext::default(),
    }
}

#[test]
fn unchanged_fixture_has_deterministic_snapshot_and_separate_operation_proposals() {
    let fixture = TestDirectory::new("deterministic");
    write_fixture(fixture.path(), "01JZY3PROPOSALAAAAAAAAAAA");

    let registration =
        build_root_proposal(request(fixture.path(), RootProposalKind::Registration)).unwrap();
    let registration_again =
        build_root_proposal(request(fixture.path(), RootProposalKind::Registration)).unwrap();
    let initialization = build_root_proposal(request(
        fixture.path(),
        RootProposalKind::SourceInitialization,
    ))
    .unwrap();

    assert_eq!(registration, registration_again);
    assert_eq!(
        registration.snapshot_digest(),
        initialization.snapshot_digest()
    );
    assert_ne!(
        registration.proposal_digest(),
        initialization.proposal_digest()
    );
    assert!(registration.source_changes().is_empty());
    let registration_change = registration.registration_change().unwrap();
    assert_eq!(registration_change.mount.as_deref(), Some("knowledge"));
    assert!(registration_change.enabled);
    assert_eq!(registration_change.priority, 0);
    assert!(!registration_change.check_for_changes);
    assert_eq!(
        registration_change.canonical_path,
        fixture.path().canonicalize().unwrap()
    );
    assert!(!initialization.source_changes().is_empty());
    assert!(initialization.registration_change().is_none());
    assert!(registration.tree().iter().any(|entry| {
        entry.path() == "concept.md" && entry.state() == ProposalTreeState::PendingMetadata
    }));
    assert!(registration.tree().iter().any(|entry| {
        entry.path() == "nested/index.md" && entry.state() == ProposalTreeState::MissingIndex
    }));
    assert!(registration.tree().iter().any(|entry| {
        entry.path() == "data.csv" && entry.state() == ProposalTreeState::UndeclaredResource
    }));
    assert!(!fixture.path().join("nested/index.md").exists());
    assert_eq!(registration.summary().accepted_files, 4);
    assert_eq!(registration.summary().rejected_entries, 1);
}

#[test]
fn malformed_mounts_fail_before_scanning() {
    let mut proposal_request = request(
        Path::new("does-not-need-to-exist"),
        RootProposalKind::Registration,
    );
    proposal_request.mount = Some("../unsafe".to_string());
    assert!(matches!(
        build_root_proposal(proposal_request),
        Err(ProposalError::InvalidMount(_))
    ));
}

#[test]
fn content_add_remove_and_rename_each_invalidate_the_stored_snapshot() {
    for mutation in ["content", "add", "remove", "rename"] {
        let fixture = TestDirectory::new(mutation);
        write_fixture(fixture.path(), "01JZY3STALEAAAAAAAAAAAAAA");
        let proposal =
            build_root_proposal(request(fixture.path(), RootProposalKind::Registration)).unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(100);
        let mut store = RootProposalStore::new(Duration::from_secs(60));
        let id = store.insert(proposal, now);

        match mutation {
            "content" => fs::write(fixture.path().join("concept.md"), "# Changed concept\n"),
            "add" => fs::write(fixture.path().join("added.md"), "# Added\n"),
            "remove" => fs::remove_file(fixture.path().join("concept.md")),
            "rename" => fs::rename(
                fixture.path().join("concept.md"),
                fixture.path().join("renamed.md"),
            ),
            _ => unreachable!(),
        }
        .unwrap();

        assert_eq!(
            store.validate(&id, now + Duration::from_secs(1)).unwrap(),
            ProposalValidation::Stale,
            "mutation {mutation} must invalidate the complete proposal"
        );
    }
}

#[test]
fn proposals_expire_and_never_create_configuration_or_derived_state() {
    let fixture = TestDirectory::new("expiry");
    write_fixture(fixture.path(), "01JZY3EXPIRYAAAAAAAAAAAAA");
    let proposal =
        build_root_proposal(request(fixture.path(), RootProposalKind::Registration)).unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(200);
    let mut store = RootProposalStore::new(Duration::from_secs(5));
    let id = store.insert(proposal, now);

    assert!(matches!(
        store.validate(&id, now + Duration::from_secs(1)).unwrap(),
        ProposalValidation::Fresh(_)
    ));
    assert_eq!(
        store.validate(&id, now + Duration::from_secs(5)).unwrap(),
        ProposalValidation::Expired
    );
    assert!(!fixture.path().join(".okf-voyage").exists());
    assert!(!fixture.path().join("config.toml").exists());
}

#[test]
fn proposal_reports_identity_mount_shadowing_and_case_conflicts() {
    let fixture = TestDirectory::new("conflicts");
    let candidate = fixture.path().join("candidate");
    let existing = fixture.path().join("existing");
    fs::create_dir_all(&candidate).unwrap();
    fs::create_dir_all(&existing).unwrap();
    write_fixture(&candidate, "01JZY3CONFLICTAAAAAAAAAAA");
    write_fixture(&existing, "01JZY3CONFLICTAAAAAAAAAAA");
    fs::write(candidate.join("Name.md"), "# One\n").unwrap();
    fs::write(candidate.join("name.md"), "# Two\n").unwrap();

    let mut proposal_request = request(&candidate, RootProposalKind::Registration);
    proposal_request.context.configured_roots = vec![DocumentRoot::mounted("knowledge", &existing)];
    proposal_request.context.dotenv_override_active = true;
    let proposal = build_root_proposal(proposal_request).unwrap();

    for expected in [
        ProposalConflictCode::DuplicateRootIdentity,
        ProposalConflictCode::CaseOrUnicodeCollision,
        ProposalConflictCode::MountCollision,
        ProposalConflictCode::ShadowedPath,
    ] {
        assert!(proposal
            .conflicts()
            .iter()
            .any(|conflict| conflict.code() == expected));
    }
    assert!(proposal.summary().dotenv_override_active);
    assert!(!proposal.can_confirm());
}

#[cfg(unix)]
#[test]
fn permission_metadata_changes_invalidate_the_snapshot() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = TestDirectory::new("permissions");
    write_fixture(fixture.path(), "01JZY3PERMISSIONAAAAAAAAA");
    let proposal =
        build_root_proposal(request(fixture.path(), RootProposalKind::Registration)).unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(500);
    let mut store = RootProposalStore::new(Duration::from_secs(60));
    let id = store.insert(proposal, now);
    fs::set_permissions(
        fixture.path().join("concept.md"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert_eq!(
        store.validate(&id, now + Duration::from_secs(1)).unwrap(),
        ProposalValidation::Stale
    );
}
