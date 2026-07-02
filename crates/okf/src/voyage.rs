use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{frontmatter, Document, Repository};

pub const DEFAULT_MODEL: &str = "voyage-3-large";
pub const DEFAULT_INDEX_ROOT: &str = ".okf-voyage";
pub const DEFAULT_TPM_LIMIT: usize = 3_000_000;
pub const DEFAULT_RPM_LIMIT: usize = 2_000;
pub const DEFAULT_BATCH_SIZE: usize = 32;
pub const PROVIDER: &str = "voyage-ai";
const VOYAGE_EMBEDDINGS_ENDPOINT: &str = "https://api.voyageai.com/v1/embeddings";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoyageConfig {
    api_key: Option<String>,
    model: String,
    index_root: PathBuf,
    batch_size: usize,
    timeout: Duration,
    tpm_limit: usize,
    rpm_limit: usize,
}

impl Default for VoyageConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            model: DEFAULT_MODEL.to_string(),
            index_root: PathBuf::from(DEFAULT_INDEX_ROOT),
            batch_size: DEFAULT_BATCH_SIZE,
            timeout: Duration::from_secs(30),
            tpm_limit: DEFAULT_TPM_LIMIT,
            rpm_limit: DEFAULT_RPM_LIMIT,
        }
    }
}

impl VoyageConfig {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| env::var(key).ok())
    }

    pub fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        let api_key = lookup("OKF_VOYAGE_API_KEY").filter(|value| !value.trim().is_empty());
        let mut config = Self {
            api_key,
            ..Self::default()
        };
        if let Some(model) = lookup("OKF_VOYAGE_MODEL").filter(|value| !value.trim().is_empty()) {
            config.model = model;
        }
        if let Some(index_root) =
            lookup("OKF_VOYAGE_INDEX_ROOT").filter(|value| !value.trim().is_empty())
        {
            config.index_root = PathBuf::from(index_root);
        }
        if let Some(batch_size) = lookup_usize(&mut lookup, "OKF_VOYAGE_BATCH_SIZE") {
            config.batch_size = batch_size.max(1);
        }
        if let Some(timeout) = lookup_usize(&mut lookup, "OKF_VOYAGE_TIMEOUT_SECONDS") {
            config.timeout = Duration::from_secs(timeout.max(1) as u64);
        }
        if let Some(limit) = lookup_usize(&mut lookup, "OKF_VOYAGE_TPM_LIMIT") {
            config.tpm_limit = limit.max(1);
        }
        if let Some(limit) = lookup_usize(&mut lookup, "OKF_VOYAGE_RPM_LIMIT") {
            config.rpm_limit = limit.max(1);
        }
        config
    }

    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    pub fn redacted_api_key(&self) -> &str {
        if self.api_key.is_some() {
            "<redacted>"
        } else {
            "<missing>"
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn index_root(&self) -> &Path {
        &self.index_root
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn tpm_limit(&self) -> usize {
        self.tpm_limit
    }

    pub fn rpm_limit(&self) -> usize {
        self.rpm_limit
    }
}

fn lookup_usize(lookup: &mut impl FnMut(&str) -> Option<String>, key: &str) -> Option<usize> {
    lookup(key)?.trim().parse().ok()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectivityReport {
    pub success: bool,
    pub http_status: Option<u16>,
    pub api_error_code: Option<String>,
    pub message: String,
    pub tokens_used: Option<usize>,
}

impl ConnectivityReport {
    pub fn success(http_status: u16, tokens_used: Option<usize>) -> Self {
        Self {
            success: true,
            http_status: Some(http_status),
            api_error_code: None,
            message: "Voyage AI connectivity check succeeded".to_string(),
            tokens_used,
        }
    }

    pub fn failure(
        http_status: Option<u16>,
        api_error_code: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            success: false,
            http_status,
            api_error_code,
            message: message.into(),
            tokens_used: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingResponse {
    pub report: ConnectivityReport,
    pub embeddings: Vec<Vec<f32>>,
}

impl EmbeddingResponse {
    pub fn success(
        http_status: u16,
        tokens_used: Option<usize>,
        embeddings: Vec<Vec<f32>>,
    ) -> Self {
        Self {
            report: ConnectivityReport::success(http_status, tokens_used),
            embeddings,
        }
    }

    pub fn failure(
        http_status: Option<u16>,
        api_error_code: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            report: ConnectivityReport::failure(http_status, api_error_code, message),
            embeddings: Vec::new(),
        }
    }
}

pub trait VoyageTransport {
    fn embed_vectors(&self, config: &VoyageConfig, inputs: &[String]) -> EmbeddingResponse;
}

pub trait VoyagePacer {
    fn wait(&self, duration: Duration);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadSleepPacer;

impl VoyagePacer for ThreadSleepPacer {
    fn wait(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CurlVoyageTransport;

impl VoyageTransport for CurlVoyageTransport {
    fn embed_vectors(&self, config: &VoyageConfig, inputs: &[String]) -> EmbeddingResponse {
        self.embed_vectors_with_program(config, inputs, OsStr::new("curl"))
    }
}

impl CurlVoyageTransport {
    fn embed_vectors_with_program(
        &self,
        config: &VoyageConfig,
        inputs: &[String],
        program: &OsStr,
    ) -> EmbeddingResponse {
        let Some(api_key) = config.api_key() else {
            return EmbeddingResponse::failure(
                None,
                Some("missing_api_key".to_string()),
                "OKF_VOYAGE_API_KEY is not configured",
            );
        };
        if inputs.is_empty() {
            return EmbeddingResponse::failure(
                None,
                Some("empty_input".to_string()),
                "no embedding input was provided",
            );
        }

        let request_body = embedding_request_body(config.model(), inputs);
        let output = voyage_curl_command(
            program,
            config.timeout(),
            api_key,
            request_body,
            VOYAGE_EMBEDDINGS_ENDPOINT,
        )
        .output();

        let output = match output {
            Ok(output) => output,
            Err(error) => {
                return EmbeddingResponse::failure(
                    None,
                    Some("process_launch_error".to_string()),
                    format!("could not start the Voyage HTTP transport: {error}"),
                );
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (body, status) = split_curl_status(&stdout);
        if !output.status.success() {
            return curl_transport_failure(&output, status);
        }
        if status == Some(200) {
            return parse_embedding_response(body, inputs.len());
        }

        provider_http_failure(body, status)
    }
}

fn curl_transport_failure(output: &Output, status: Option<u16>) -> EmbeddingResponse {
    let exit_code = output.status.code();
    let (code, fallback) = curl_exit_error(exit_code);
    let stderr = safe_detail(&String::from_utf8_lossy(&output.stderr));
    EmbeddingResponse::failure(
        status,
        Some(code.to_string()),
        if stderr.is_empty() {
            fallback.to_string()
        } else {
            stderr
        },
    )
}

fn curl_exit_error(exit_code: Option<i32>) -> (&'static str, &'static str) {
    match exit_code {
        Some(6) => ("dns_error", "Voyage AI host name resolution failed"),
        Some(28) => ("timeout", "Voyage AI request timed out"),
        Some(35 | 51 | 53 | 58 | 59 | 60 | 64 | 66 | 77 | 80 | 82 | 83 | 90 | 91) => (
            "tls_error",
            "Voyage AI TLS negotiation or certificate validation failed",
        ),
        _ => ("transport_error", "Voyage AI transport failed"),
    }
}

fn voyage_curl_command(
    program: &OsStr,
    timeout: Duration,
    api_key: &str,
    request_body: String,
    endpoint: &str,
) -> Command {
    let timeout = timeout.as_secs().max(1).to_string();
    let mut command = Command::new(program);
    command
        .arg("--http1.1")
        .arg("-sS")
        .arg("--connect-timeout")
        .arg(&timeout)
        .arg("--max-time")
        .arg(&timeout)
        .arg("-w")
        .arg("\n%{http_code}")
        .arg(endpoint)
        .arg("-H")
        .arg(format!("Authorization: Bearer {api_key}"))
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("-d")
        .arg(request_body);
    command
}

fn provider_http_failure(body: &str, status: Option<u16>) -> EmbeddingResponse {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    let provider_code = parsed.as_ref().and_then(provider_error_code);
    let detail = parsed.as_ref().and_then(provider_error_message);
    let code = provider_code.clone().unwrap_or_else(|| {
        if detail.is_some() {
            "provider_error"
        } else {
            "http_error"
        }
        .to_string()
    });
    let message = detail.unwrap_or_else(|| match status {
        Some(status) => format!("Voyage AI returned HTTP status {status}"),
        None => "Voyage AI returned an invalid HTTP response".to_string(),
    });
    EmbeddingResponse::failure(status, Some(code), message)
}

fn parse_embedding_response(body: &str, expected_count: usize) -> EmbeddingResponse {
    let value = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(value) => value,
        Err(_) => {
            return EmbeddingResponse::failure(
                Some(200),
                Some("malformed_provider_response".to_string()),
                "Voyage AI returned malformed JSON",
            );
        }
    };
    let Some(data) = value.get("data").and_then(serde_json::Value::as_array) else {
        return EmbeddingResponse::failure(
            Some(200),
            Some("malformed_provider_response".to_string()),
            "Voyage AI response does not contain an embedding data array",
        );
    };
    let mut indexed = Vec::with_capacity(data.len());
    for (position, item) in data.iter().enumerate() {
        let index = item
            .get("index")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(position);
        let Some(values) = item.get("embedding").and_then(serde_json::Value::as_array) else {
            return EmbeddingResponse::failure(
                Some(200),
                Some("malformed_provider_response".to_string()),
                "Voyage AI response contains an embedding without a numeric vector",
            );
        };
        let mut vector = Vec::with_capacity(values.len());
        for value in values {
            let Some(value) = value.as_f64() else {
                return EmbeddingResponse::failure(
                    Some(200),
                    Some("malformed_provider_response".to_string()),
                    "Voyage AI response contains a non-numeric embedding value",
                );
            };
            let value = value as f32;
            if !value.is_finite() {
                return EmbeddingResponse::failure(
                    Some(200),
                    Some("malformed_provider_response".to_string()),
                    "Voyage AI response contains a non-finite embedding value",
                );
            }
            vector.push(value);
        }
        indexed.push((index, vector));
    }
    indexed.sort_by_key(|(index, _)| *index);
    if indexed.len() != expected_count
        || indexed
            .iter()
            .enumerate()
            .any(|(expected, (actual, _))| expected != *actual)
    {
        return EmbeddingResponse::failure(
            Some(200),
            Some("embedding_count_mismatch".to_string()),
            format!(
                "Voyage AI returned {} embeddings for {expected_count} inputs",
                indexed.len()
            ),
        );
    }
    let embeddings = indexed
        .into_iter()
        .map(|(_, vector)| vector)
        .collect::<Vec<_>>();
    if let Err(message) = validate_embedding_dimensions(&embeddings, None) {
        return EmbeddingResponse::failure(
            Some(200),
            Some("embedding_dimension_mismatch".to_string()),
            message,
        );
    }
    let tokens_used = value
        .get("usage")
        .and_then(|usage| usage.get("total_tokens"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok());
    EmbeddingResponse::success(200, tokens_used, embeddings)
}

fn provider_error_code(value: &serde_json::Value) -> Option<String> {
    value
        .get("code")
        .or_else(|| value.get("error").and_then(|error| error.get("code")))
        .and_then(serde_json::Value::as_str)
        .map(safe_detail)
        .filter(|value| !value.is_empty())
}

fn provider_error_message(value: &serde_json::Value) -> Option<String> {
    value
        .get("message")
        .or_else(|| value.get("detail"))
        .or_else(|| value.get("error").and_then(|error| error.get("message")))
        .and_then(serde_json::Value::as_str)
        .map(safe_detail)
        .filter(|value| !value.is_empty())
}

fn safe_detail(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || *character == ' ')
        .take(512)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod transport_tests {
    use super::*;

    #[test]
    fn curl_uses_configured_connect_and_total_timeout() {
        let command = voyage_curl_command(
            OsStr::new("curl"),
            Duration::from_secs(7),
            "test-key",
            "{}".to_string(),
            VOYAGE_EMBEDDINGS_ENDPOINT,
        );
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["--connect-timeout", "7"]));
        assert!(arguments.windows(2).any(|pair| pair == ["--max-time", "7"]));
    }

    #[test]
    fn total_timeout_stops_a_delayed_http_response() {
        use std::net::TcpListener;
        use std::thread;
        use std::time::Instant;

        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("bind delayed test endpoint: {error}"),
        };
        let address = listener.local_addr().expect("test endpoint address");
        thread::spawn(move || {
            if let Ok((_stream, _peer)) = listener.accept() {
                thread::sleep(Duration::from_secs(2));
            }
        });
        let mut command = voyage_curl_command(
            OsStr::new("curl"),
            Duration::from_secs(1),
            "test-key",
            "{}".to_string(),
            &format!("http://{address}/embeddings"),
        );
        command.env("NO_PROXY", "127.0.0.1");
        let started = Instant::now();

        let output = command.output().expect("execute curl");

        assert_eq!(output.status.code(), Some(28));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn curl_exit_codes_have_distinct_safe_categories() {
        assert_eq!(curl_exit_error(Some(6)).0, "dns_error");
        assert_eq!(curl_exit_error(Some(28)).0, "timeout");
        assert_eq!(curl_exit_error(Some(60)).0, "tls_error");
        assert_eq!(curl_exit_error(Some(7)).0, "transport_error");
    }

    #[test]
    fn missing_transport_process_has_a_process_launch_error() {
        let config = VoyageConfig::from_lookup(|key| {
            (key == "OKF_VOYAGE_API_KEY").then(|| "test-key".to_string())
        });

        let response = CurlVoyageTransport.embed_vectors_with_program(
            &config,
            &["ping".to_string()],
            OsStr::new("okf-definitely-missing-curl-command"),
        );

        assert_eq!(
            response.report.api_error_code.as_deref(),
            Some("process_launch_error")
        );
    }

    #[test]
    fn malformed_success_response_is_rejected() {
        let response = parse_embedding_response("{not-json", 1);

        assert!(!response.report.success);
        assert_eq!(
            response.report.api_error_code.as_deref(),
            Some("malformed_provider_response")
        );
    }

    #[test]
    fn provider_status_and_safe_details_are_preserved() {
        let response = provider_http_failure(
            r#"{"code":"rate_limit_exceeded","detail":"slow down"}"#,
            Some(429),
        );

        assert_eq!(response.report.http_status, Some(429));
        assert_eq!(
            response.report.api_error_code.as_deref(),
            Some("rate_limit_exceeded")
        );
        assert_eq!(response.report.message, "slow down");
    }

    #[test]
    fn unstructured_http_failure_has_http_error_category() {
        let response = provider_http_failure("gateway unavailable", Some(503));

        assert_eq!(response.report.http_status, Some(503));
        assert_eq!(
            response.report.api_error_code.as_deref(),
            Some("http_error")
        );
    }
}

pub fn check_connectivity(
    config: &VoyageConfig,
    transport: &impl VoyageTransport,
) -> ConnectivityReport {
    transport
        .embed_vectors(config, &["ping".to_string()])
        .report
}

pub fn embed_changed_chunks(
    config: &VoyageConfig,
    chunks: &[Chunk],
    index: &mut LocalIndex,
    transport: &impl VoyageTransport,
) -> ConnectivityReport {
    embed_changed_chunks_with_pacer(config, chunks, index, transport, &ThreadSleepPacer)
}

pub fn embed_changed_chunks_with_pacer(
    config: &VoyageConfig,
    chunks: &[Chunk],
    index: &mut LocalIndex,
    transport: &impl VoyageTransport,
    pacer: &impl VoyagePacer,
) -> ConnectivityReport {
    let mut expected_dimension = match existing_embedding_dimension(index) {
        Ok(dimension) => dimension,
        Err(message) => {
            return ConnectivityReport::failure(
                None,
                Some("existing_index_dimension_mismatch".to_string()),
                message,
            );
        }
    };
    let changed = chunks
        .iter()
        .filter(|chunk| !index.has_current_embedding(config, chunk))
        .collect::<Vec<_>>();
    let mut total_tokens = 0usize;
    let mut generated = Vec::<(String, Vec<f32>)>::new();
    let mut window = RateLimitWindow::default();

    for batch in changed.chunks(config.batch_size()) {
        let batch_estimated_tokens = batch.iter().map(|chunk| chunk.estimated_tokens).sum();
        if batch_estimated_tokens > config.tpm_limit() {
            return ConnectivityReport::failure(
                None,
                Some("batch_exceeds_tpm_limit".to_string()),
                format!(
                    "one Voyage batch is estimated at {batch_estimated_tokens} tokens, which exceeds the configured TPM limit of {}",
                    config.tpm_limit()
                ),
            );
        }
        if window.would_exceed(config, batch_estimated_tokens) {
            pacer.wait(Duration::from_secs(60));
            window.reset();
        }

        let inputs = batch
            .iter()
            .map(|chunk| chunk.content.clone())
            .collect::<Vec<_>>();
        let mut response = transport.embed_vectors(config, &inputs);
        if response.report.http_status == Some(429) {
            pacer.wait(Duration::from_secs(60));
            window.reset();
            response = transport.embed_vectors(config, &inputs);
        }
        if !response.report.success {
            return response.report;
        }
        if response.embeddings.len() != batch.len() {
            return ConnectivityReport::failure(
                response.report.http_status,
                Some("embedding_count_mismatch".to_string()),
                format!(
                    "Voyage AI returned {} embeddings for {} inputs",
                    response.embeddings.len(),
                    batch.len()
                ),
            );
        }
        let dimension =
            match validate_embedding_dimensions(&response.embeddings, expected_dimension) {
                Ok(dimension) => dimension,
                Err(message) => {
                    return ConnectivityReport::failure(
                        response.report.http_status,
                        Some("embedding_dimension_mismatch".to_string()),
                        message,
                    );
                }
            };
        expected_dimension = Some(dimension);
        total_tokens += response.report.tokens_used.unwrap_or_default();
        window.record(batch_estimated_tokens);
        for (chunk, embedding) in batch.iter().zip(response.embeddings) {
            generated.push((chunk.id.clone(), embedding));
        }
    }

    index.rebuild_with_embeddings(config, chunks, generated);
    ConnectivityReport::success(200, Some(total_tokens))
}

fn existing_embedding_dimension(index: &LocalIndex) -> Result<Option<usize>, String> {
    let Some(first) = index.embeddings.first() else {
        return Ok(None);
    };
    let expected = first.embedding.len();
    if expected == 0
        || index.embeddings.iter().any(|entry| {
            entry.embedding.len() != expected
                || entry.embedding.iter().any(|value| !value.is_finite())
        })
    {
        return Err(
            "the existing Voyage index contains inconsistent embedding dimensions".to_string(),
        );
    }
    Ok(Some(expected))
}

fn validate_embedding_dimensions(
    embeddings: &[Vec<f32>],
    expected: Option<usize>,
) -> Result<usize, String> {
    let dimension = expected
        .or_else(|| embeddings.first().map(Vec::len))
        .unwrap_or_default();
    if dimension == 0 {
        return Err("Voyage AI returned an empty embedding vector".to_string());
    }
    if embeddings.iter().any(|embedding| {
        embedding.len() != dimension || embedding.iter().any(|value| !value.is_finite())
    }) {
        return Err(format!(
            "Voyage AI returned inconsistent embedding dimensions; expected {dimension}"
        ));
    }
    Ok(dimension)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct RateLimitWindow {
    requests: usize,
    estimated_tokens: usize,
}

impl RateLimitWindow {
    fn would_exceed(&self, config: &VoyageConfig, next_estimated_tokens: usize) -> bool {
        self.requests.saturating_add(1) > config.rpm_limit()
            || self.estimated_tokens.saturating_add(next_estimated_tokens) > config.tpm_limit()
    }

    fn record(&mut self, estimated_tokens: usize) {
        self.requests += 1;
        self.estimated_tokens += estimated_tokens;
    }

    fn reset(&mut self) {
        self.requests = 0;
        self.estimated_tokens = 0;
    }
}

fn split_curl_status(output: &str) -> (&str, Option<u16>) {
    let Some((body, status)) = output.rsplit_once('\n') else {
        return (output, None);
    };
    (
        body,
        status
            .trim()
            .parse()
            .ok()
            .filter(|status| (100..=599).contains(status)),
    )
}

fn embedding_request_body(model: &str, inputs: &[String]) -> String {
    let input = inputs
        .iter()
        .map(|value| format!("\"{}\"", escape_json(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"model\":\"{}\",\"input\":[{}]}}",
        escape_json(model),
        input
    )
}

fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            value => vec![value],
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryDocument {
    pub logical_path: PathBuf,
    pub physical_path: PathBuf,
    pub title: String,
    pub document_type: Option<String>,
    pub kind: Option<String>,
    pub topic: Option<String>,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub bytes: u64,
    pub content_hash: String,
    pub estimated_tokens: usize,
}

pub fn inventory(repository: &Repository) -> Result<Vec<InventoryDocument>, io::Error> {
    repository
        .documents()
        .iter()
        .filter(|document| !is_reserved_path(document.relative_path()))
        .map(inventory_document)
        .collect()
}

fn inventory_document(document: &Document) -> Result<InventoryDocument, io::Error> {
    let physical_path = document.physical_path();
    let source = fs::read_to_string(&physical_path)?;
    Ok(InventoryDocument {
        logical_path: document.relative_path().to_path_buf(),
        physical_path,
        title: document.title().to_string(),
        document_type: document.document_type().map(ToString::to_string),
        kind: document.kind().map(ToString::to_string),
        topic: document.topic().map(ToString::to_string),
        status: document.status().map(ToString::to_string),
        tags: parse_tags(document.frontmatter().get("tags").map(String::as_str)),
        bytes: source.len() as u64,
        content_hash: stable_hash(&source),
        estimated_tokens: estimate_tokens(&source),
    })
}

fn parse_tags(value: Option<&str>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|tag| tag.trim().trim_matches('"').to_string())
        .filter(|tag| !tag.is_empty())
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chunk {
    pub id: String,
    pub document_path: PathBuf,
    pub title: String,
    pub document_type: Option<String>,
    pub kind: Option<String>,
    pub topic: Option<String>,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub heading_path: Vec<String>,
    pub content: String,
    pub content_hash: String,
    pub estimated_tokens: usize,
}

pub fn chunk_repository(repository: &Repository) -> Result<Vec<Chunk>, io::Error> {
    let mut chunks = Vec::new();
    for document in repository
        .documents()
        .iter()
        .filter(|document| !is_reserved_path(document.relative_path()))
    {
        chunks.extend(chunk_document(document)?);
    }
    Ok(chunks)
}

pub fn chunk_document(document: &Document) -> Result<Vec<Chunk>, io::Error> {
    let source = fs::read_to_string(document.physical_path())?;
    let (metadata, body) = frontmatter::parse(&source);
    let sections = markdown_sections(body);
    let tags = parse_tags(metadata.get("tags").map(String::as_str));
    let mut chunks = Vec::new();
    for (index, section) in sections.into_iter().enumerate() {
        let content = section.content.trim().to_string();
        if content.is_empty() {
            continue;
        }
        let hash = stable_hash(&content);
        let id = format!(
            "{}#{}-{}",
            document.relative_path().to_string_lossy(),
            index + 1,
            hash
        );
        chunks.push(Chunk {
            id,
            document_path: document.relative_path().to_path_buf(),
            title: document.title().to_string(),
            document_type: document.document_type().map(ToString::to_string),
            kind: document.kind().map(ToString::to_string),
            topic: document.topic().map(ToString::to_string),
            status: document.status().map(ToString::to_string),
            tags: tags.clone(),
            heading_path: section.heading_path,
            estimated_tokens: estimate_tokens(&content),
            content_hash: hash,
            content,
        });
    }
    Ok(chunks)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MarkdownSection {
    heading_path: Vec<String>,
    content: String,
}

fn markdown_sections(body: &str) -> Vec<MarkdownSection> {
    let mut sections = Vec::new();
    let mut headings = Vec::<String>::new();
    let mut current = String::new();
    let mut current_headings = Vec::<String>::new();

    for line in body.lines() {
        if let Some((level, title)) = markdown_heading(line) {
            if !current.trim().is_empty() {
                sections.push(MarkdownSection {
                    heading_path: current_headings.clone(),
                    content: current.clone(),
                });
                current.clear();
            }
            headings.truncate(level.saturating_sub(1));
            headings.push(title.to_string());
            current_headings = headings.clone();
        }
        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        sections.push(MarkdownSection {
            heading_path: current_headings,
            content: current,
        });
    }

    if sections.is_empty() && !body.trim().is_empty() {
        sections.push(MarkdownSection {
            heading_path: Vec::new(),
            content: body.to_string(),
        });
    }

    sections
}

fn markdown_heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let title = line.get(hashes..)?.strip_prefix(' ')?.trim();
    if title.is_empty() {
        None
    } else {
        Some((hashes, title))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenPlan {
    pub documents: usize,
    pub chunks: usize,
    pub estimated_tokens: usize,
    pub estimated_requests: usize,
    pub cached_chunks: usize,
    pub changed_chunks: usize,
    pub model: String,
    pub tpm_limit: usize,
    pub rpm_limit: usize,
}

impl TokenPlan {
    pub fn from_chunks(
        config: &VoyageConfig,
        chunks: &[Chunk],
        index: Option<&LocalIndex>,
    ) -> Self {
        let cached_chunks = chunks
            .iter()
            .filter(|chunk| index.is_some_and(|index| index.has_current_embedding(config, chunk)))
            .count();
        let changed_chunks = chunks.len().saturating_sub(cached_chunks);
        let estimated_tokens = chunks
            .iter()
            .filter(|chunk| !index.is_some_and(|index| index.has_current_embedding(config, chunk)))
            .map(|chunk| chunk.estimated_tokens)
            .sum();
        let estimated_requests = if changed_chunks == 0 {
            0
        } else {
            changed_chunks.div_ceil(config.batch_size())
        };
        let documents = chunks
            .iter()
            .map(|chunk| chunk.document_path.clone())
            .collect::<BTreeSet<_>>()
            .len();

        Self {
            documents,
            chunks: chunks.len(),
            estimated_tokens,
            estimated_requests,
            cached_chunks,
            changed_chunks,
            model: config.model().to_string(),
            tpm_limit: config.tpm_limit(),
            rpm_limit: config.rpm_limit(),
        }
    }

    pub fn within_limits(&self) -> bool {
        self.estimated_tokens <= self.tpm_limit && self.estimated_requests <= self.rpm_limit
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddedChunk {
    pub chunk: Chunk,
    pub provider: String,
    pub model: String,
    pub embedding: Vec<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocalIndex {
    pub embeddings: Vec<EmbeddedChunk>,
    pub suggestions: Vec<SuggestedEdge>,
}

impl LocalIndex {
    pub fn load(root: impl AsRef<Path>) -> Result<Self, io::Error> {
        let root = root.as_ref();
        let embeddings = load_embeddings(root.join("embeddings.tsv"))?;
        let suggestions = load_suggestions(root.join("suggestions.tsv"))?;
        Ok(Self {
            embeddings,
            suggestions,
        })
    }

    pub fn save(&self, root: impl AsRef<Path>) -> Result<(), io::Error> {
        let root = root.as_ref();
        fs::create_dir_all(root)?;
        save_embeddings(root.join("embeddings.tsv"), &self.embeddings)?;
        save_suggestions(root.join("suggestions.tsv"), &self.suggestions)?;
        Ok(())
    }

    pub fn chunk_hashes(&self) -> BTreeMap<String, String> {
        self.embeddings
            .iter()
            .map(|entry| (entry.chunk.id.clone(), entry.chunk.content_hash.clone()))
            .collect()
    }

    fn has_current_embedding(&self, config: &VoyageConfig, chunk: &Chunk) -> bool {
        self.embeddings.iter().any(|entry| {
            entry.chunk.id == chunk.id
                && entry.chunk.content_hash == chunk.content_hash
                && entry.provider == PROVIDER
                && entry.model == config.model()
        })
    }

    pub fn rebuild_with_embeddings(
        &mut self,
        config: &VoyageConfig,
        chunks: &[Chunk],
        embeddings: Vec<(String, Vec<f32>)>,
    ) {
        let live_ids = chunks
            .iter()
            .map(|chunk| chunk.id.clone())
            .collect::<BTreeSet<_>>();
        self.embeddings
            .retain(|entry| live_ids.contains(&entry.chunk.id));
        for (chunk_id, embedding) in embeddings {
            if let Some(chunk) = chunks.iter().find(|chunk| chunk.id == chunk_id) {
                if let Some(existing) = self
                    .embeddings
                    .iter_mut()
                    .find(|entry| entry.chunk.id == chunk.id)
                {
                    existing.chunk = chunk.clone();
                    existing.model = config.model().to_string();
                    existing.embedding = embedding;
                } else {
                    self.embeddings.push(EmbeddedChunk {
                        chunk: chunk.clone(),
                        provider: PROVIDER.to_string(),
                        model: config.model().to_string(),
                        embedding,
                    });
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub chunk_id: String,
    pub document_path: PathBuf,
    pub score: f32,
    pub provider: String,
    pub model: String,
}

pub trait VectorBackend {
    fn search(&self, query_embedding: &[f32], limit: usize) -> Vec<SearchResult>;
}

impl VectorBackend for LocalIndex {
    fn search(&self, query_embedding: &[f32], limit: usize) -> Vec<SearchResult> {
        let mut results = self
            .embeddings
            .iter()
            .map(|entry| SearchResult {
                chunk_id: entry.chunk.id.clone(),
                document_path: entry.chunk.document_path.clone(),
                score: cosine_similarity(query_embedding, &entry.embedding),
                provider: entry.provider.clone(),
                model: entry.model.clone(),
            })
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
        });
        results.truncate(limit);
        results
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SuggestedEdgeStatus {
    Suggested,
    Accepted,
    Denied,
}

impl SuggestedEdgeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Suggested => "suggested",
            Self::Accepted => "accepted",
            Self::Denied => "denied",
        }
    }
}

impl fmt::Display for SuggestedEdgeStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SuggestedEdge {
    pub id: String,
    pub review_set_id: String,
    pub provider: String,
    pub model: String,
    pub generation_method: String,
    pub ai_generated: bool,
    pub source_chunk: String,
    pub target_chunk: String,
    pub score: f32,
    pub created_at: String,
    pub status: SuggestedEdgeStatus,
}

impl SuggestedEdge {
    pub fn human_label(&self) -> String {
        format!(
            "AI-derived {} edge {} -> {} ({:.3})",
            self.generation_method, self.source_chunk, self.target_chunk, self.score
        )
    }
}

pub fn suggest_edges(index: &LocalIndex, threshold: f32) -> Vec<SuggestedEdge> {
    let mut suggestions = Vec::new();
    for left_index in 0..index.embeddings.len() {
        for right_index in (left_index + 1)..index.embeddings.len() {
            let left = &index.embeddings[left_index];
            let right = &index.embeddings[right_index];
            if left.chunk.document_path == right.chunk.document_path {
                continue;
            }
            let score = cosine_similarity(&left.embedding, &right.embedding);
            if score < threshold {
                continue;
            }
            let source = left.chunk.id.clone();
            let target = right.chunk.id.clone();
            let id = stable_hash(&format!("{source}\n{target}\n{score:.6}"));
            suggestions.push(SuggestedEdge {
                id,
                review_set_id: "unassigned".to_string(),
                provider: PROVIDER.to_string(),
                model: left.model.clone(),
                generation_method: "embedding_similarity".to_string(),
                ai_generated: true,
                source_chunk: source,
                target_chunk: target,
                score,
                created_at: unix_timestamp_string(),
                status: SuggestedEdgeStatus::Suggested,
            });
        }
    }
    suggestions
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewAction {
    AcceptAll,
    AcceptOne(String),
    DenyOne(String),
    DenyAll,
}

pub fn apply_review_action(suggestions: &mut [SuggestedEdge], action: ReviewAction) {
    match action {
        ReviewAction::AcceptAll => {
            for suggestion in suggestions {
                suggestion.status = SuggestedEdgeStatus::Accepted;
            }
        }
        ReviewAction::AcceptOne(id) => {
            if let Some(suggestion) = suggestions.iter_mut().find(|edge| edge.id == id) {
                suggestion.status = SuggestedEdgeStatus::Accepted;
            }
        }
        ReviewAction::DenyOne(id) => {
            if let Some(suggestion) = suggestions.iter_mut().find(|edge| edge.id == id) {
                suggestion.status = SuggestedEdgeStatus::Denied;
            }
        }
        ReviewAction::DenyAll => {
            for suggestion in suggestions {
                suggestion.status = SuggestedEdgeStatus::Denied;
            }
        }
    }
}

pub fn accepted_edges_markdown(suggestions: &[SuggestedEdge]) -> String {
    let accepted = suggestions
        .iter()
        .filter(|suggestion| suggestion.status == SuggestedEdgeStatus::Accepted)
        .collect::<Vec<_>>();
    if accepted.is_empty() {
        return String::new();
    }
    let mut output = String::from("## Accepted AI-Suggested OKF Edges\n\n");
    for suggestion in accepted {
        output.push_str(&format!(
            "- `{}` -> `{}` ({}, score {:.3}, provider {}, model {})\n",
            suggestion.source_chunk,
            suggestion.target_chunk,
            suggestion.generation_method,
            suggestion.score,
            suggestion.provider,
            suggestion.model
        ));
    }
    output
}

fn save_embeddings(path: PathBuf, embeddings: &[EmbeddedChunk]) -> Result<(), io::Error> {
    let mut output = String::new();
    for entry in embeddings {
        output.push_str(&tsv_escape(&entry.chunk.id));
        output.push('\t');
        output.push_str(&tsv_escape(&entry.chunk.document_path.to_string_lossy()));
        output.push('\t');
        output.push_str(&tsv_escape(&entry.chunk.content_hash));
        output.push('\t');
        output.push_str(&tsv_escape(&entry.provider));
        output.push('\t');
        output.push_str(&tsv_escape(&entry.model));
        output.push('\t');
        output.push_str(
            &entry
                .embedding
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push('\n');
    }
    atomic_write(&path, output.as_bytes())
}

fn load_embeddings(path: PathBuf) -> Result<Vec<EmbeddedChunk>, io::Error> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let source = fs::read_to_string(path)?;
    Ok(source
        .lines()
        .filter_map(|line| {
            let fields = split_tsv(line);
            if fields.len() != 6 {
                return None;
            }
            let embedding = fields[5]
                .split(',')
                .filter_map(|value| value.parse::<f32>().ok())
                .collect::<Vec<_>>();
            Some(EmbeddedChunk {
                chunk: Chunk {
                    id: fields[0].clone(),
                    document_path: PathBuf::from(&fields[1]),
                    title: String::new(),
                    document_type: None,
                    kind: None,
                    topic: None,
                    status: None,
                    tags: Vec::new(),
                    heading_path: Vec::new(),
                    content: String::new(),
                    content_hash: fields[2].clone(),
                    estimated_tokens: 0,
                },
                provider: fields[3].clone(),
                model: fields[4].clone(),
                embedding,
            })
        })
        .collect())
}

fn save_suggestions(path: PathBuf, suggestions: &[SuggestedEdge]) -> Result<(), io::Error> {
    let mut output = String::new();
    for suggestion in suggestions {
        output.push_str(
            &[
                tsv_escape(&suggestion.id),
                tsv_escape(&suggestion.review_set_id),
                tsv_escape(&suggestion.provider),
                tsv_escape(&suggestion.model),
                tsv_escape(&suggestion.generation_method),
                suggestion.ai_generated.to_string(),
                tsv_escape(&suggestion.source_chunk),
                tsv_escape(&suggestion.target_chunk),
                suggestion.score.to_string(),
                tsv_escape(&suggestion.created_at),
                suggestion.status.to_string(),
            ]
            .join("\t"),
        );
        output.push('\n');
    }
    atomic_write(&path, output.as_bytes())
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), io::Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("okf-index");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        use std::io::Write;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn load_suggestions(path: PathBuf) -> Result<Vec<SuggestedEdge>, io::Error> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let source = fs::read_to_string(path)?;
    Ok(source
        .lines()
        .filter_map(|line| {
            let fields = split_tsv(line);
            if fields.len() != 10 && fields.len() != 11 {
                return None;
            }
            let offset = usize::from(fields.len() == 11);
            Some(SuggestedEdge {
                id: fields[0].clone(),
                review_set_id: if offset == 1 {
                    fields[1].clone()
                } else {
                    "legacy".to_string()
                },
                provider: fields[offset + 1].clone(),
                model: fields[offset + 2].clone(),
                generation_method: fields[offset + 3].clone(),
                ai_generated: fields[offset + 4] == "true",
                source_chunk: fields[offset + 5].clone(),
                target_chunk: fields[offset + 6].clone(),
                score: fields[offset + 7].parse().ok()?,
                created_at: fields[offset + 8].clone(),
                status: match fields[offset + 9].as_str() {
                    "accepted" => SuggestedEdgeStatus::Accepted,
                    "denied" => SuggestedEdgeStatus::Denied,
                    _ => SuggestedEdgeStatus::Suggested,
                },
            })
        })
        .collect())
}

fn tsv_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn split_tsv(line: &str) -> Vec<String> {
    line.split('\t')
        .map(|value| {
            value
                .replace("\\n", "\n")
                .replace("\\t", "\t")
                .replace("\\\\", "\\")
        })
        .collect()
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let dot = left
        .iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}

fn is_reserved_path(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some("index.md" | "log.md")
    )
}

fn estimate_tokens(source: &str) -> usize {
    source.chars().count().div_ceil(4).max(1)
}

fn stable_hash(source: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in source.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
