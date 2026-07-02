use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use okf::voyage::{
    accepted_edges_markdown, apply_review_action, check_connectivity, chunk_repository,
    embed_changed_chunks, embed_changed_chunks_with_pacer, inventory, suggest_edges,
    ConnectivityReport, CurlVoyageTransport, EmbeddingResponse, LocalIndex, ReviewAction,
    TokenPlan, VectorBackend, VoyageConfig, VoyagePacer, VoyageTransport, DEFAULT_INDEX_ROOT,
    DEFAULT_MODEL,
};
use okf::{DocumentRoot, Repository};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new(label: &str) -> Self {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("okf-voyage-{}-{label}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("create test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, content).expect("write file");
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn repository_fixture() -> (TestDirectory, Repository) {
    let directory = TestDirectory::new("repository");
    directory.write(
        "docs/alpha.md",
        "---\ntitle: Alpha\ntype: Concept\nkind: knowledge-document\ntopic: demo\ntags: [one, two]\n---\n# Alpha\n\nAlpha body.\n\n## Details\n\nShared detail.\n",
    );
    directory.write(
        "docs/beta.md",
        "---\ntitle: Beta\ntype: Concept\nkind: knowledge-document\ntopic: demo\n---\n# Beta\n\nBeta body.\n\n## Details\n\nShared detail.\n",
    );
    directory.write("docs/index.md", "# Index\n\n- [Alpha](alpha.md)\n");
    let repository = Repository::open([DocumentRoot::mounted(
        "knowledge",
        directory.path().join("docs"),
    )])
    .expect("repository");
    (directory, repository)
}

#[test]
fn loads_voyage_config_from_okf_prefixed_environment_values() {
    let values = BTreeMap::from([
        ("OKF_VOYAGE_API_KEY", "secret"),
        ("OKF_VOYAGE_MODEL", "voyage-test"),
        ("OKF_VOYAGE_INDEX_ROOT", ".custom-voyage"),
        ("OKF_VOYAGE_BATCH_SIZE", "7"),
        ("OKF_VOYAGE_TIMEOUT_SECONDS", "9"),
        ("OKF_VOYAGE_TPM_LIMIT", "123"),
        ("OKF_VOYAGE_RPM_LIMIT", "45"),
    ]);

    let config = VoyageConfig::from_lookup(|key| values.get(key).map(ToString::to_string));

    assert!(config.has_api_key());
    assert_eq!(config.redacted_api_key(), "<redacted>");
    assert_eq!(config.model(), "voyage-test");
    assert_eq!(config.index_root(), Path::new(".custom-voyage"));
    assert_eq!(config.batch_size(), 7);
    assert_eq!(config.timeout().as_secs(), 9);
    assert_eq!(config.tpm_limit(), 123);
    assert_eq!(config.rpm_limit(), 45);
}

#[test]
fn defaults_do_not_require_voyage_credentials() {
    let config = VoyageConfig::from_lookup(|_| None);

    assert!(!config.has_api_key());
    assert_eq!(config.redacted_api_key(), "<missing>");
    assert_eq!(config.model(), DEFAULT_MODEL);
    assert_eq!(config.index_root(), Path::new(DEFAULT_INDEX_ROOT));
}

#[derive(Clone, Debug)]
struct FakeTransport {
    report: ConnectivityReport,
}

impl VoyageTransport for FakeTransport {
    fn embed_vectors(&self, _config: &VoyageConfig, _inputs: &[String]) -> EmbeddingResponse {
        EmbeddingResponse {
            report: self.report.clone(),
            embeddings: Vec::new(),
        }
    }
}

#[test]
fn connectivity_reports_provider_error_without_secrets() {
    let config = VoyageConfig::from_lookup(|key| {
        (key == "OKF_VOYAGE_API_KEY").then(|| "super-secret".to_string())
    });
    let report = check_connectivity(
        &config,
        &FakeTransport {
            report: ConnectivityReport::failure(
                Some(429),
                Some("rate_limit_exceeded".to_string()),
                "too many requests",
            ),
        },
    );

    assert!(!report.success);
    assert_eq!(report.http_status, Some(429));
    assert_eq!(
        report.api_error_code.as_deref(),
        Some("rate_limit_exceeded")
    );
    assert!(!report.message.contains("super-secret"));
}

#[derive(Clone, Debug)]
struct CountingTransport;

impl VoyageTransport for CountingTransport {
    fn embed_vectors(&self, _config: &VoyageConfig, inputs: &[String]) -> EmbeddingResponse {
        EmbeddingResponse::success(
            200,
            Some(inputs.len()),
            inputs
                .iter()
                .enumerate()
                .map(|(index, _)| vec![index as f32 + 1.0, 1.0])
                .collect(),
        )
    }
}

#[derive(Debug, Default)]
struct RecordingPacer {
    waits: Cell<usize>,
    durations: RefCell<Vec<Duration>>,
}

impl VoyagePacer for RecordingPacer {
    fn wait(&self, duration: Duration) {
        self.waits.set(self.waits.get() + 1);
        self.durations.borrow_mut().push(duration);
    }
}

#[derive(Debug, Default)]
struct RetryAfterRateLimitTransport {
    calls: Cell<usize>,
}

#[derive(Debug, Default)]
struct PartialFailureTransport {
    calls: Cell<usize>,
}

impl VoyageTransport for PartialFailureTransport {
    fn embed_vectors(&self, _config: &VoyageConfig, inputs: &[String]) -> EmbeddingResponse {
        let call = self.calls.get();
        self.calls.set(call + 1);
        if call == 0 {
            EmbeddingResponse::success(
                200,
                Some(inputs.len()),
                inputs.iter().map(|_| vec![1.0, 0.0]).collect(),
            )
        } else {
            EmbeddingResponse::failure(
                None,
                Some("timeout".to_string()),
                "Voyage AI request timed out",
            )
        }
    }
}

#[derive(Debug)]
struct FixedEmbeddingTransport {
    embeddings: Vec<Vec<f32>>,
}

impl VoyageTransport for FixedEmbeddingTransport {
    fn embed_vectors(&self, _config: &VoyageConfig, _inputs: &[String]) -> EmbeddingResponse {
        EmbeddingResponse::success(200, None, self.embeddings.clone())
    }
}

impl VoyageTransport for RetryAfterRateLimitTransport {
    fn embed_vectors(&self, _config: &VoyageConfig, inputs: &[String]) -> EmbeddingResponse {
        let call = self.calls.get();
        self.calls.set(call + 1);
        if call == 0 {
            return EmbeddingResponse::failure(
                Some(429),
                Some("rate_limit_exceeded".to_string()),
                "rate limited",
            );
        }
        EmbeddingResponse::success(
            200,
            Some(inputs.len()),
            inputs.iter().map(|_| vec![1.0, 0.0]).collect(),
        )
    }
}

#[test]
fn inventory_excludes_reserved_files_and_uses_logical_and_physical_paths() {
    let (directory, repository) = repository_fixture();

    let documents = inventory(&repository).expect("inventory");

    assert_eq!(documents.len(), 2);
    assert_eq!(
        documents[0].logical_path,
        PathBuf::from("knowledge/alpha.md")
    );
    assert_eq!(
        documents[0].physical_path,
        directory.path().join("docs/alpha.md")
    );
    assert_eq!(documents[0].document_type.as_deref(), Some("Concept"));
    assert_eq!(documents[0].tags, ["one", "two"]);
    assert!(documents[0].bytes > 0);
    assert!(documents[0].estimated_tokens > 0);
}

#[test]
fn chunking_is_stable_and_preserves_metadata() {
    let (_directory, repository) = repository_fixture();

    let first = chunk_repository(&repository).expect("first chunks");
    let second = chunk_repository(&repository).expect("second chunks");

    assert_eq!(first, second);
    assert!(first.len() >= 4);
    assert!(first.iter().any(|chunk| chunk.heading_path == ["Alpha"]));
    assert!(first
        .iter()
        .any(|chunk| chunk.heading_path == ["Alpha", "Details"]));
    assert!(first
        .iter()
        .all(|chunk| chunk.document_type.as_deref() == Some("Concept")));
}

#[test]
fn token_plan_reports_cached_and_changed_chunks_before_spending_tokens() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let config = VoyageConfig::default();
    let mut index = LocalIndex::default();
    index.rebuild_with_embeddings(
        &config,
        &chunks[..1],
        vec![(chunks[0].id.clone(), vec![1.0, 0.0])],
    );

    let plan = TokenPlan::from_chunks(&config, &chunks, Some(&index));

    assert_eq!(plan.documents, 2);
    assert_eq!(plan.chunks, chunks.len());
    assert_eq!(plan.cached_chunks, 1);
    assert_eq!(plan.changed_chunks, chunks.len() - 1);
    assert!(plan.estimated_tokens > 0);
    assert!(plan.within_limits());
}

#[test]
fn embedding_client_updates_only_changed_chunks() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let config = VoyageConfig::default();
    let mut index = LocalIndex::default();
    index.rebuild_with_embeddings(
        &config,
        &chunks[..1],
        vec![(chunks[0].id.clone(), vec![99.0, 99.0])],
    );

    let report = embed_changed_chunks(&config, &chunks, &mut index, &CountingTransport);

    assert!(report.success);
    assert_eq!(index.embeddings.len(), chunks.len());
    assert_eq!(index.embeddings[0].embedding, vec![99.0, 99.0]);
}

#[test]
fn changing_embedding_model_invalidates_cached_chunks() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let old_config = VoyageConfig::from_lookup(|key| {
        (key == "OKF_VOYAGE_MODEL").then(|| "voyage-old".to_string())
    });
    let new_config = VoyageConfig::from_lookup(|key| {
        (key == "OKF_VOYAGE_MODEL").then(|| "voyage-new".to_string())
    });
    let mut index = LocalIndex::default();
    index.rebuild_with_embeddings(
        &old_config,
        &chunks,
        chunks
            .iter()
            .map(|chunk| (chunk.id.clone(), vec![9.0, 9.0]))
            .collect(),
    );

    let plan = TokenPlan::from_chunks(&new_config, &chunks, Some(&index));
    let report = embed_changed_chunks(&new_config, &chunks, &mut index, &CountingTransport);

    assert_eq!(plan.changed_chunks, chunks.len());
    assert!(report.success);
    assert!(index
        .embeddings
        .iter()
        .all(|embedding| embedding.model == "voyage-new"));
}

#[test]
fn embedding_client_paces_batches_before_rate_limits_are_hit() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let config = VoyageConfig::from_lookup(|key| match key {
        "OKF_VOYAGE_BATCH_SIZE" => Some("1".to_string()),
        "OKF_VOYAGE_RPM_LIMIT" => Some("1".to_string()),
        _ => None,
    });
    let mut index = LocalIndex::default();
    let pacer = RecordingPacer::default();

    let report =
        embed_changed_chunks_with_pacer(&config, &chunks, &mut index, &CountingTransport, &pacer);

    assert!(report.success);
    assert!(chunks.len() > 1);
    assert_eq!(index.embeddings.len(), chunks.len());
    assert_eq!(pacer.waits.get(), chunks.len() - 1);
    assert!(pacer
        .durations
        .borrow()
        .iter()
        .all(|duration| *duration == Duration::from_secs(60)));
}

#[test]
fn embedding_client_rejects_batches_that_exceed_tpm_before_calling_transport() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let config = VoyageConfig::from_lookup(|key| match key {
        "OKF_VOYAGE_TPM_LIMIT" => Some("1".to_string()),
        _ => None,
    });
    let mut index = LocalIndex::default();
    let pacer = RecordingPacer::default();

    let report =
        embed_changed_chunks_with_pacer(&config, &chunks, &mut index, &CountingTransport, &pacer);

    assert!(!report.success);
    assert_eq!(
        report.api_error_code.as_deref(),
        Some("batch_exceeds_tpm_limit")
    );
    assert_eq!(index.embeddings.len(), 0);
    assert_eq!(pacer.waits.get(), 0);
}

#[test]
fn embedding_client_retries_once_after_rate_limit_response() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let config = VoyageConfig::from_lookup(|key| match key {
        "OKF_VOYAGE_BATCH_SIZE" => Some(chunks.len().to_string()),
        _ => None,
    });
    let mut index = LocalIndex::default();
    let pacer = RecordingPacer::default();
    let transport = RetryAfterRateLimitTransport::default();

    let report = embed_changed_chunks_with_pacer(&config, &chunks, &mut index, &transport, &pacer);

    assert!(report.success);
    assert_eq!(transport.calls.get(), 2);
    assert_eq!(pacer.waits.get(), 1);
    assert_eq!(index.embeddings.len(), chunks.len());
}

#[test]
fn embedding_client_rejects_wrong_embedding_count_without_mutating_index() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let config = VoyageConfig::from_lookup(|key| match key {
        "OKF_VOYAGE_BATCH_SIZE" => Some(chunks.len().to_string()),
        _ => None,
    });
    let mut index = LocalIndex::default();
    let before = index.clone();
    let transport = FixedEmbeddingTransport {
        embeddings: vec![vec![1.0, 0.0]],
    };

    let report = embed_changed_chunks(&config, &chunks, &mut index, &transport);

    assert!(!report.success);
    assert_eq!(
        report.api_error_code.as_deref(),
        Some("embedding_count_mismatch")
    );
    assert_eq!(index, before);
}

#[test]
fn embedding_client_rejects_inconsistent_dimensions_without_mutating_index() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let config = VoyageConfig::from_lookup(|key| match key {
        "OKF_VOYAGE_BATCH_SIZE" => Some(chunks.len().to_string()),
        _ => None,
    });
    let mut embeddings = vec![vec![1.0, 0.0]; chunks.len()];
    embeddings[1] = vec![1.0, 0.0, 0.0];
    let transport = FixedEmbeddingTransport { embeddings };
    let mut index = LocalIndex::default();
    let before = index.clone();

    let report = embed_changed_chunks(&config, &chunks, &mut index, &transport);

    assert!(!report.success);
    assert_eq!(
        report.api_error_code.as_deref(),
        Some("embedding_dimension_mismatch")
    );
    assert_eq!(index, before);
}

#[test]
fn partial_batch_failure_leaves_index_unchanged() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    assert!(chunks.len() > 1);
    let config = VoyageConfig::from_lookup(|key| match key {
        "OKF_VOYAGE_BATCH_SIZE" => Some("1".to_string()),
        _ => None,
    });
    let mut index = LocalIndex::default();
    let before = index.clone();
    let transport = PartialFailureTransport::default();

    let report = embed_changed_chunks(&config, &chunks, &mut index, &transport);

    assert!(!report.success);
    assert_eq!(report.api_error_code.as_deref(), Some("timeout"));
    assert_eq!(transport.calls.get(), 2);
    assert_eq!(index, before);
}

#[test]
fn local_index_roundtrips_and_drops_stale_chunks_on_rebuild() {
    let directory = TestDirectory::new("index");
    let (_fixture_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let config = VoyageConfig::default();
    let mut index = LocalIndex::default();
    index.rebuild_with_embeddings(
        &config,
        &chunks,
        chunks
            .iter()
            .map(|chunk| (chunk.id.clone(), vec![1.0, 0.0]))
            .collect(),
    );

    index.save(directory.path()).expect("save index");
    let loaded = LocalIndex::load(directory.path()).expect("load index");
    assert_eq!(loaded.embeddings.len(), chunks.len());

    let mut rebuilt = loaded;
    rebuilt.rebuild_with_embeddings(
        &config,
        &chunks[..1],
        vec![(chunks[0].id.clone(), vec![0.0, 1.0])],
    );
    assert_eq!(rebuilt.embeddings.len(), 1);
    assert_eq!(rebuilt.embeddings[0].embedding, vec![0.0, 1.0]);
}

#[test]
fn semantic_search_uses_the_vector_backend_without_touching_exact_lookup() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let config = VoyageConfig::default();
    let mut index = LocalIndex::default();
    index.rebuild_with_embeddings(
        &config,
        &chunks[..2],
        vec![
            (chunks[0].id.clone(), vec![1.0, 0.0]),
            (chunks[1].id.clone(), vec![0.0, 1.0]),
        ],
    );

    let results = index.search(&[0.9, 0.1], 1);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].chunk_id, chunks[0].id);
    assert!(repository
        .find(okf::DocumentQuery::Exact("Alpha".to_string()))
        .is_ok());
}

#[test]
fn suggested_edges_are_ai_marked_and_reviewable() {
    let (_directory, repository) = repository_fixture();
    let chunks = chunk_repository(&repository).expect("chunks");
    let config = VoyageConfig::default();
    let mut index = LocalIndex::default();
    index.rebuild_with_embeddings(
        &config,
        &chunks,
        chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| {
                let embedding = if index % 2 == 0 {
                    vec![1.0, 0.0]
                } else {
                    vec![0.95, 0.05]
                };
                (chunk.id.clone(), embedding)
            })
            .collect(),
    );

    let mut suggestions = suggest_edges(&index, 0.9);
    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().all(|edge| edge.ai_generated));
    assert!(suggestions
        .iter()
        .all(|edge| edge.generation_method == "embedding_similarity"));
    assert!(suggestions[0].human_label().contains("AI-derived"));

    let first_id = suggestions[0].id.clone();
    apply_review_action(&mut suggestions, ReviewAction::AcceptOne(first_id.clone()));
    assert_eq!(suggestions[0].status.as_str(), "accepted");
    apply_review_action(&mut suggestions, ReviewAction::DenyOne(first_id));
    assert_eq!(suggestions[0].status.as_str(), "denied");
    apply_review_action(&mut suggestions, ReviewAction::AcceptAll);
    assert!(suggestions
        .iter()
        .all(|edge| edge.status.as_str() == "accepted"));
    assert!(accepted_edges_markdown(&suggestions).contains("Accepted AI-Suggested OKF Edges"));
    apply_review_action(&mut suggestions, ReviewAction::DenyAll);
    assert!(suggestions
        .iter()
        .all(|edge| edge.status.as_str() == "denied"));
}

#[test]
fn optional_vector_backend_can_be_implemented_without_a_database() {
    struct EmptyBackend;

    impl VectorBackend for EmptyBackend {
        fn search(
            &self,
            _query_embedding: &[f32],
            _limit: usize,
        ) -> Vec<okf::voyage::SearchResult> {
            Vec::new()
        }
    }

    assert!(EmptyBackend.search(&[1.0], 10).is_empty());
}

#[test]
#[ignore = "requires OKF_VOYAGE_API_KEY and spends a tiny number of Voyage AI tokens"]
fn live_voyage_connectivity_check_is_opt_in() {
    let config = VoyageConfig::from_env();
    assert!(config.has_api_key(), "OKF_VOYAGE_API_KEY must be set");

    let report = check_connectivity(&config, &CurlVoyageTransport);

    assert!(
        report.success,
        "status {:?}, code {:?}, message {}",
        report.http_status, report.api_error_code, report.message
    );
}
