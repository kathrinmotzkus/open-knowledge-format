use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use okf::{
    build_initialization_plan, build_root_proposal, configuration_revision,
    confirm_root_registration, confirm_source_initialization, load_browser_config,
    recover_interrupted_operations, remove_registered_root, update_registered_root,
    AdmissionLimits, InitializationOptions, ProposalValidation, RootConfigurationUpdate, RootId,
    RootProposalContext, RootProposalKind, RootProposalRequest, RootProposalStore,
    TransactionError,
};
use sha2::{Digest, Sha256};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("okf-transaction-{name}-{unique}"));
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

fn write_fixture(root: &Path) {
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(
        root.join("index.md"),
        "---\nokf_root_id: urn:okf:root:01JZY3TRANSACTIONAAAAAAAA\nunknown: keep\n---\n# Root\n",
    )
    .unwrap();
    fs::write(
        root.join("concept.md"),
        "---\r\nunknown: preserve\r\n---\r\n# Concept\r\n",
    )
    .unwrap();
    fs::write(root.join("nested/topic.md"), "# Nested\n").unwrap();
    fs::write(root.join("data.csv"), "name,value\none,1\n").unwrap();
}

fn proposal(root: &Path, kind: RootProposalKind, priority: i64) -> okf::RootProposal {
    build_root_proposal(RootProposalRequest {
        root: root.to_path_buf(),
        mount: Some("knowledge".to_string()),
        kind,
        limits: AdmissionLimits::default(),
        context: RootProposalContext {
            registration_priority: priority,
            check_for_changes: true,
            ..RootProposalContext::default()
        },
    })
    .unwrap()
}

fn stored(proposal: okf::RootProposal, now: SystemTime) -> (RootProposalStore, String) {
    let mut store = RootProposalStore::new(Duration::from_secs(300));
    let id = store.insert(proposal, now);
    (store, id)
}

#[test]
fn registration_is_revision_bound_idempotent_and_never_mutates_sources() {
    let fixture = TestDirectory::new("registration");
    let root = fixture.path().join("root");
    fs::create_dir(&root).unwrap();
    write_fixture(&root);
    let config = fixture.path().join("config/okf/config.toml");
    let source_before = fs::read(root.join("concept.md")).unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(100);
    let (mut store, id) = stored(proposal(&root, RootProposalKind::Registration, 400), now);
    let revision = configuration_revision(&config).unwrap();

    let report = confirm_root_registration(&config, &mut store, &id, now, &revision).unwrap();
    assert!(report.changed);
    let loaded = load_browser_config(&config).unwrap();
    assert_eq!(loaded.roots().len(), 1);
    assert_eq!(loaded.roots()[0].priority, 400);
    assert!(loaded.roots()[0].check_for_changes);
    assert_eq!(fs::read(root.join("concept.md")).unwrap(), source_before);

    let repeated =
        confirm_root_registration(&config, &mut store, &id, now, &report.revision).unwrap();
    assert!(!repeated.changed);
    assert!(matches!(
        confirm_root_registration(&config, &mut store, &id, now, &revision),
        Err(TransactionError::RevisionConflict { .. })
    ));
    let updated = update_registered_root(
        &config,
        &loaded.roots()[0].root_id,
        &report.revision,
        &RootConfigurationUpdate {
            priority: Some(900),
            check_for_changes: Some(false),
            ..RootConfigurationUpdate::default()
        },
    )
    .unwrap();
    assert_eq!(
        load_browser_config(&config).unwrap().roots()[0].priority,
        900
    );
    let removed =
        remove_registered_root(&config, &loaded.roots()[0].root_id, &updated.revision).unwrap();
    assert!(removed.changed);
    assert!(load_browser_config(&config).unwrap().roots().is_empty());
    assert_eq!(fs::read(root.join("concept.md")).unwrap(), source_before);
}

#[test]
fn concurrent_tabs_and_server_instances_cannot_reuse_one_configuration_revision() {
    let fixture = TestDirectory::new("concurrent-revision");
    let root = fixture.path().join("root");
    fs::create_dir(&root).unwrap();
    write_fixture(&root);
    let config = fixture.path().join("config.toml");
    let now = UNIX_EPOCH + Duration::from_secs(150);
    let (mut store, id) = stored(proposal(&root, RootProposalKind::Registration, 100), now);
    let initial = configuration_revision(&config).unwrap();
    let registered = confirm_root_registration(&config, &mut store, &id, now, &initial).unwrap();
    let root_id = load_browser_config(&config).unwrap().roots()[0]
        .root_id
        .clone();

    let shared_revision = registered.revision.clone();
    let first_tab = update_registered_root(
        &config,
        &root_id,
        &shared_revision,
        &RootConfigurationUpdate {
            priority: Some(200),
            ..RootConfigurationUpdate::default()
        },
    )
    .unwrap();
    assert!(first_tab.changed);
    let second_tab = update_registered_root(
        &config,
        &root_id,
        &shared_revision,
        &RootConfigurationUpdate {
            priority: Some(300),
            ..RootConfigurationUpdate::default()
        },
    );
    assert!(matches!(
        second_tab,
        Err(TransactionError::RevisionConflict { .. })
    ));
    assert_eq!(
        load_browser_config(&config).unwrap().roots()[0].priority,
        200
    );
}

#[test]
fn initialization_preserves_metadata_permissions_line_endings_and_config() {
    let fixture = TestDirectory::new("initialization");
    let root = fixture.path().join("root");
    fs::create_dir(&root).unwrap();
    write_fixture(&root);
    let git = Command::new("git")
        .arg("init")
        .arg(&root)
        .output()
        .expect("run git init");
    assert!(git.status.success());
    let config = fixture.path().join("config.toml");
    fs::write(&config, "operator-owned-config\n").unwrap();
    let config_before = fs::read(&config).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("concept.md"), fs::Permissions::from_mode(0o640)).unwrap();
    }
    let now = UNIX_EPOCH + Duration::from_secs(200);
    let (mut store, id) = stored(
        proposal(&root, RootProposalKind::SourceInitialization, 0),
        now,
    );
    let current = match store.validate(&id, now).unwrap() {
        ProposalValidation::Fresh(value) => value,
        other => panic!("unexpected validation: {other:?}"),
    };
    let options = InitializationOptions {
        resource_types: BTreeMap::from([("data.csv".to_string(), "Dataset".to_string())]),
    };
    let plan = build_initialization_plan(&current, &options).unwrap();
    assert!(plan.git().is_some_and(|status| status.dirty));
    assert!(plan
        .changes()
        .iter()
        .any(|change| change.path() == "nested/index.md"));
    assert!(plan
        .changes()
        .iter()
        .any(|change| change.path() == "index.md" && change.diff().contains("resources")));

    let report = confirm_source_initialization(
        &fixture.path().join("state"),
        &mut store,
        &id,
        now,
        &options,
        plan.plan_digest(),
    )
    .unwrap();
    assert_eq!(fs::read(&config).unwrap(), config_before);
    let concept = fs::read_to_string(root.join("concept.md")).unwrap();
    assert!(concept.contains("unknown: preserve\r\n"));
    assert!(concept.contains("type: Concept\r\n"));
    assert!(!concept.replace("\r\n", "").contains('\n'));
    assert!(root.join("nested/index.md").exists());
    let index = fs::read_to_string(root.join("index.md")).unwrap();
    assert!(index.contains("unknown: keep"));
    assert!(index.contains("\"path\":\"data.csv\""));
    assert_eq!(report.changed_files.len(), plan.changes().len());
    assert_eq!(report.final_diffs.len(), plan.changes().len());
    assert!(!fixture.path().join("state").join("operation-").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join("concept.md"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
}

#[cfg(unix)]
#[test]
fn failed_multi_file_apply_rolls_back_every_already_changed_file() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = TestDirectory::new("rollback");
    let root = fixture.path().join("root");
    fs::create_dir(&root).unwrap();
    write_fixture(&root);
    let original_concept = fs::read(root.join("concept.md")).unwrap();
    let original_index = fs::read(root.join("index.md")).unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(300);
    let (mut store, id) = stored(
        proposal(&root, RootProposalKind::SourceInitialization, 0),
        now,
    );
    let current = match store.validate(&id, now).unwrap() {
        ProposalValidation::Fresh(value) => value,
        other => panic!("unexpected validation: {other:?}"),
    };
    let options = InitializationOptions::default();
    let plan = build_initialization_plan(&current, &options).unwrap();
    fs::set_permissions(root.join("nested"), fs::Permissions::from_mode(0o555)).unwrap();

    let result = confirm_source_initialization(
        &fixture.path().join("state"),
        &mut store,
        &id,
        now,
        &options,
        plan.plan_digest(),
    );
    fs::set_permissions(root.join("nested"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(root.join("concept.md")).unwrap(), original_concept);
    assert_eq!(fs::read(root.join("index.md")).unwrap(), original_index);
    assert!(!root.join("nested/index.md").exists());
}

#[test]
fn startup_recovery_restores_an_interrupted_external_journal() {
    let fixture = TestDirectory::new("recovery");
    let root = fixture.path().join("root");
    let state = fixture.path().join("state/operation-test");
    fs::create_dir_all(state.join("backup")).unwrap();
    fs::create_dir_all(state.join("staged")).unwrap();
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("document.md"), "after").unwrap();
    fs::write(state.join("backup/0"), "before").unwrap();
    fs::write(state.join("staged/0"), "after").unwrap();
    let hash = |value: &[u8]| format!("{:x}", Sha256::digest(value));
    fs::write(
        state.join("journal.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1,
            "status": "applying",
            "root": root,
            "entries": [{
                "path": "document.md",
                "had_original": true,
                "before_hash": hash(b"before"),
                "after_hash": hash(b"after"),
                "permissions": null
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    assert!(recover_interrupted_operations(&fixture.path().join("state")).unwrap());
    assert_eq!(fs::read(root.join("document.md")).unwrap(), b"before");
    assert!(!state.exists());
}

#[test]
fn initialization_can_add_a_preapproved_portable_identity_to_a_new_root_index() {
    let fixture = TestDirectory::new("new-root-identity");
    let root = fixture.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("concept.md"), "# Concept\n").unwrap();
    let identity = RootId::parse("urn:okf:root:01JZY3NEWROOTAAAAAAAAAAAA").unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(400);
    let proposal = build_root_proposal(RootProposalRequest {
        root: root.clone(),
        mount: Some("new-root".to_string()),
        kind: RootProposalKind::SourceInitialization,
        limits: AdmissionLimits::default(),
        context: RootProposalContext {
            proposed_root_id: Some(identity.clone()),
            ..RootProposalContext::default()
        },
    })
    .unwrap();
    assert!(proposal.can_confirm());
    let (mut store, id) = stored(proposal, now);
    let current = match store.validate(&id, now).unwrap() {
        ProposalValidation::Fresh(value) => value,
        other => panic!("unexpected validation: {other:?}"),
    };
    let plan = build_initialization_plan(&current, &InitializationOptions::default()).unwrap();
    let index = plan
        .changes()
        .iter()
        .find(|change| change.path() == "index.md")
        .unwrap();
    assert!(index.after().contains(identity.as_str()));
    confirm_source_initialization(
        &fixture.path().join("state"),
        &mut store,
        &id,
        now,
        &InitializationOptions::default(),
        plan.plan_digest(),
    )
    .unwrap();
    assert!(fs::read_to_string(root.join("index.md"))
        .unwrap()
        .contains(identity.as_str()));
}

#[test]
fn transaction_state_and_configuration_are_refused_inside_the_document_root() {
    let fixture = TestDirectory::new("external-state");
    let root = fixture.path().join("root");
    fs::create_dir(&root).unwrap();
    write_fixture(&root);
    let now = UNIX_EPOCH + Duration::from_secs(500);

    let (mut registration_store, registration_id) =
        stored(proposal(&root, RootProposalKind::Registration, 0), now);
    let internal_config = root.join("config.toml");
    let revision = configuration_revision(&internal_config).unwrap();
    assert!(matches!(
        confirm_root_registration(
            &internal_config,
            &mut registration_store,
            &registration_id,
            now,
            &revision
        ),
        Err(TransactionError::UnsafePath(_))
    ));
    assert!(!internal_config.exists());

    let (mut initialization_store, initialization_id) = stored(
        proposal(&root, RootProposalKind::SourceInitialization, 0),
        now,
    );
    let current = match initialization_store
        .validate(&initialization_id, now)
        .unwrap()
    {
        ProposalValidation::Fresh(value) => value,
        other => panic!("unexpected validation: {other:?}"),
    };
    let plan = build_initialization_plan(&current, &InitializationOptions::default()).unwrap();
    assert!(matches!(
        confirm_source_initialization(
            &root.join(".okf-state"),
            &mut initialization_store,
            &initialization_id,
            now,
            &InitializationOptions::default(),
            plan.plan_digest()
        ),
        Err(TransactionError::UnsafePath(_))
    ));
    assert!(!root.join(".okf-state").exists());
}
