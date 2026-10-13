//! Exact-tier, zero-egress acceptance harness for the optional language layer.
//!
//! The ordinary test binds the production client to a test-owned loopback
//! socket. The ignored test is the only path that may contact the fixed local
//! Ollama service, and only when the operator supplies an exact tag + digest.

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::params;
use serde_json::Value;
use thruline_core::annotate::{
    active_language_shadow_projection, extract_language_claims_with_memory, MemoryPressurePoll,
};
use thruline_core::chain::sha256_hex;
use thruline_core::config::Config;
use thruline_core::derive::{derive_project, fingerprint};
use thruline_core::export::export_project;
use thruline_core::ingest::ingest;
use thruline_core::language::{
    immutable_model_identity, Annotator, ClassificationOutcome, LanguageError, MemoryPressureProbe,
    MemoryPressureState, MemoryPressureThresholds, OllamaAnnotator, OllamaOptions,
    ServiceAvailability,
};
use thruline_core::Ledger;

const PROJECT: &str = "-synthetic-language-tier-acceptance";
const SESSION: &str = "60000000-0000-4000-8000-000000000000";
const GENERATED_AT: &str = "2026-08-26T00:01:00Z";
const FIXTURE: &[u8] = include_bytes!("fixtures/language_tier_acceptance.jsonl");
const WRAPPER: &str = include_str!("../../../scripts/verify-language-tiers.sh");
const EXACT_TIERS: [&str; 3] = ["qwen3:0.6b", "qwen3:4b", "qwen3:8b"];

struct CountingAnnotator<'a> {
    inner: &'a dyn Annotator,
    calls: AtomicU64,
}

impl<'a> CountingAnnotator<'a> {
    fn new(inner: &'a dyn Annotator) -> Self {
        Self {
            inner,
            calls: AtomicU64::new(0),
        }
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::Relaxed)
    }
}

impl Annotator for CountingAnnotator<'_> {
    fn classify(&self, message: &str) -> ClassificationOutcome {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.classify(message)
    }
}

struct Scratch {
    _dir: tempfile::TempDir,
    ledger: Ledger,
}

fn synthetic_project() -> Scratch {
    let fixture_text = std::str::from_utf8(FIXTURE).expect("synthetic fixture is UTF-8");
    assert!(!fixture_text.contains("/Users/"));
    assert!(!fixture_text.contains("/home/"));
    assert!(!fixture_text.contains("THRULINE_TIER_"));
    let dir = tempfile::tempdir().expect("synthetic acceptance tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJECT);
    fs::create_dir_all(&project).expect("create synthetic project");
    fs::write(project.join(format!("{SESSION}.jsonl")), FIXTURE)
        .expect("write committed synthetic fixture");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open temp ledger");
    let ingest_report = ingest(
        &mut ledger,
        &[PathBuf::from(&root)],
        &dir.path().join("mirrors"),
    )
    .expect("ingest synthetic project");
    assert!(ingest_report.new_events > 0, "synthetic fixture is empty");
    Scratch { _dir: dir, ledger }
}

fn acceptance_config() -> Config {
    let mut config = Config::default();
    // Keep the production memory-pressure probe while ensuring this tiny
    // operator harness only pauses at genuinely exhausted-memory levels.
    config.language.pause_below_available_mb = 1;
    config.language.resume_above_available_mb = 2;
    config
}

#[derive(Debug)]
struct AcceptanceMetrics {
    calls: u64,
    attempts: u64,
    duration_ms: u128,
    events: u64,
    candidates: u64,
    classified: u64,
    uncertain: u64,
    accepted: u64,
    demoted: u64,
    inserted: u64,
    replay_reused: u64,
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn classification_row_count(ledger: &Ledger) -> u64 {
    let count: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM annotations WHERE layer='classification'",
            [],
            |row| row.get(0),
        )
        .expect("classification count");
    u64::try_from(count).expect("nonnegative classification count")
}

fn validate_stored_manifest(
    ledger: &Ledger,
    model_identity: &str,
    events: u64,
    candidates: u64,
    accepted: u64,
) {
    let manifest_id: String = ledger
        .conn()
        .query_row(
            "SELECT manifest_annotation_id FROM claim_projection_heads WHERE project_id=?1",
            params![PROJECT],
            |row| row.get(0),
        )
        .expect("active language manifest id");
    let (event_id, layer, model, prompt_sha256, output_json, output_sha256, created_at): (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    ) = ledger
        .conn()
        .query_row(
            "SELECT event_id, layer, model, prompt_sha256, output_json, output_sha256, created_at
             FROM annotations WHERE id=?1",
            params![manifest_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .expect("stored active manifest");
    assert_eq!(layer, "classification");
    assert_eq!(model, model_identity);
    assert!(is_lower_sha256(&prompt_sha256));
    assert!(is_lower_sha256(&output_sha256));
    assert_eq!(output_sha256, sha256_hex(output_json.as_bytes()));
    assert!(created_at.starts_with("20") && created_at.ends_with('Z'));

    let manifest: Value = serde_json::from_str(&output_json).expect("manifest JSON");
    let object = manifest.as_object().expect("manifest object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "accepted",
            "candidates",
            "claim_set_sha256",
            "complete",
            "events",
            "extractor",
            "members",
            "prompt_revision",
            "truth_bearing",
            "type",
        ]
    );
    assert_eq!(manifest["type"], "classification_run");
    assert_eq!(manifest["complete"], true);
    assert_eq!(manifest["truth_bearing"], false);
    assert_eq!(manifest["events"], events);
    assert_eq!(manifest["candidates"], candidates);
    assert_eq!(manifest["accepted"], accepted);
    assert!(is_lower_sha256(
        manifest["claim_set_sha256"]
            .as_str()
            .expect("claim set digest")
    ));
    assert_eq!(
        manifest["prompt_revision"],
        thruline_core::language::PROMPT_REVISION
    );

    let members = manifest["members"].as_array().expect("manifest members");
    assert_eq!(members.len() as u64, events);
    let mut member_ids = std::collections::BTreeSet::new();
    for member in members {
        let member = member.as_object().expect("member object");
        let mut member_keys: Vec<&str> = member.keys().map(String::as_str).collect();
        member_keys.sort_unstable();
        assert_eq!(member_keys, vec!["annotation_id", "event_id"]);
        let annotation_id = member["annotation_id"].as_str().expect("annotation id");
        let member_event = member["event_id"].as_str().expect("event id");
        assert!(member_ids.insert(annotation_id));
        let (stored_event, stored_layer, stored_model, stored_output, stored_hash): (
            String,
            String,
            String,
            String,
            String,
        ) = ledger
            .conn()
            .query_row(
                "SELECT event_id, layer, model, output_json, output_sha256
                 FROM annotations WHERE id=?1",
                params![annotation_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("stored manifest member");
        assert_eq!(stored_event, member_event);
        assert_eq!(stored_layer, "classification");
        assert_eq!(stored_model, model_identity);
        assert_eq!(stored_hash, sha256_hex(stored_output.as_bytes()));
        let stored: Value = serde_json::from_str(&stored_output).expect("stored event JSON");
        assert_eq!(stored["type"], "event_classification");
        assert!(stored["results"].is_array());
    }
    assert!(members.iter().any(|member| member["event_id"] == event_id));
}

/// **Always ready, so this measures the language tier and not the machine.**
///
/// `extract_language_claims` installs the *system* memory probe, which made this
/// acceptance test's outcome depend on how much RAM happened to be free while it
/// ran. That was invisible while the pause loop was unbounded — the test simply
/// hung — and once the loop gained a deadline it would have become a flake
/// instead: red on a busy machine, green on an idle one, about nothing either
/// way.
///
/// The pause policy has its own tests in `language_pipeline.rs`, driven by
/// scripted pressure. This file is about the tier's acceptance criteria, so it
/// holds memory constant.
struct AlwaysReady;

impl MemoryPressureProbe for AlwaysReady {
    fn sample(
        &self,
        _previous: MemoryPressureState,
        _thresholds: MemoryPressureThresholds,
    ) -> std::result::Result<MemoryPressureState, LanguageError> {
        Ok(MemoryPressureState::Ready)
    }
}

impl MemoryPressurePoll for AlwaysReady {
    fn wait_before_retry(&self) -> std::result::Result<(), LanguageError> {
        // Unreachable while `sample` is always `Ready`; a sleep here would be a
        // wait this test can never need.
        Ok(())
    }
}

fn run_acceptance(
    ledger: &Ledger,
    provider: &dyn Annotator,
    model: &str,
    digest: &str,
) -> Result<AcceptanceMetrics, &'static str> {
    let model_identity = immutable_model_identity(model, digest).expect("immutable model identity");
    let counting = CountingAnnotator::new(provider);
    let config = acceptance_config();
    let started = Instant::now();
    let first = extract_language_claims_with_memory(
        ledger,
        PROJECT,
        &config,
        &counting,
        &model_identity,
        &AlwaysReady,
        &AlwaysReady,
    )
    .expect("production language annotation path");
    let duration_ms = started.elapsed().as_millis();
    assert!(first.classification_events > 0);
    assert!(first.classification_candidates > 0);
    assert_eq!(counting.calls(), first.classification_candidates);
    assert_eq!(
        first.classification_annotations_inserted,
        first.classification_events + 1,
        "each event and the completed run manifest are stored once"
    );
    assert_eq!(first.classification_annotations_reused, 0);
    assert_eq!(
        classification_row_count(ledger),
        first.classification_events + 1
    );

    validate_stored_manifest(
        ledger,
        &model_identity,
        first.classification_events,
        first.classification_candidates,
        first.classification_accepted,
    );
    let projection = active_language_shadow_projection(ledger, PROJECT)
        .expect("validate active language run")
        .expect("active language run");
    assert!(!projection.truth_bearing);
    assert_eq!(projection.scope, "at_export");
    assert_eq!(projection.run.model, model);
    assert_eq!(projection.run.model_digest, digest);
    assert_eq!(projection.coverage.events, first.classification_events);
    assert_eq!(
        projection.coverage.candidates,
        first.classification_candidates
    );
    if projection.coverage.classified == 0 {
        return Err("exact tier produced no provider-valid classified result");
    }
    assert_eq!(
        projection.items.len() as u64,
        first.classification_candidates
    );

    let graph = derive_project(ledger, PROJECT).expect("derive annotated synthetic project");
    let graph_fingerprint = fingerprint(&graph);
    let exported = export_project(ledger, &graph, GENERATED_AT).expect("export annotated project");
    assert_eq!(exported["language_annotations"]["truth_bearing"], false);
    assert_eq!(exported["language_annotations"]["scope"], "at_export");
    assert_eq!(
        exported["language_annotations"],
        serde_json::to_value(&projection).expect("projection JSON")
    );
    let public_language = exported["language_annotations"].to_string();
    assert!(!public_language.contains("confidence"));
    assert!(!public_language.contains("system_prompt"));
    assert!(!public_language.contains("Target sentence"));
    for node in exported["nodes"].as_array().expect("exported nodes") {
        let node_text = node.to_string();
        assert!(!node_text.contains(model));
        assert!(!node_text.contains(digest));
        assert!(!node_text.contains("truth_bearing"));
    }

    let calls_before_replay = counting.calls();
    let rows_before_replay = classification_row_count(ledger);
    let second = extract_language_claims_with_memory(
        ledger,
        PROJECT,
        &config,
        &counting,
        &model_identity,
        &AlwaysReady,
        &AlwaysReady,
    )
    .expect("frozen language replay");
    assert_eq!(counting.calls(), calls_before_replay);
    assert_eq!(second.classification_attempts, 0);
    assert_eq!(second.classification_annotations_inserted, 0);
    assert_eq!(
        second.classification_annotations_reused,
        first.classification_events + 1
    );
    assert_eq!(classification_row_count(ledger), rows_before_replay);
    let replay_projection = active_language_shadow_projection(ledger, PROJECT)
        .expect("validate replayed active run")
        .expect("replayed active run");
    assert_eq!(replay_projection, projection);
    let replay_graph = derive_project(ledger, PROJECT).expect("derive frozen replay");
    assert_eq!(fingerprint(&replay_graph), graph_fingerprint);
    assert_eq!(
        export_project(ledger, &replay_graph, GENERATED_AT).expect("export frozen replay"),
        exported
    );

    Ok(AcceptanceMetrics {
        calls: calls_before_replay,
        attempts: first.classification_attempts,
        duration_ms,
        events: first.classification_events,
        candidates: first.classification_candidates,
        classified: projection.coverage.classified,
        uncertain: projection.coverage.uncertain,
        accepted: first.classification_accepted,
        demoted: first.classification_demoted,
        inserted: first.classification_annotations_inserted,
        replay_reused: second.classification_annotations_reused,
    })
}

struct MockOllama {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockOllama {
    fn start(model: &str, digest: &str) -> Self {
        Self::start_with_content(
            model,
            digest,
            "{\"category\":\"finding\",\"confidence\":0.91,\"speech_act\":\"none\",\"label\":\"Synthetic record\"}",
        )
    }

    fn start_with_content(model: &str, digest: &str, chat_content: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock Ollama");
        listener
            .set_nonblocking(true)
            .expect("nonblocking mock listener");
        let address = listener.local_addr().expect("mock address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_requests = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let model = model.to_string();
        let digest = digest.to_string();
        let chat_content = chat_content.to_string();
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_nonblocking(false)
                            .expect("blocking mock connection");
                        let Some(path) = read_request_path(&mut stream) else {
                            continue;
                        };
                        thread_requests
                            .lock()
                            .expect("request trace")
                            .push(path.clone());
                        let body = match path.as_str() {
                            "/api/tags" => serde_json::json!({
                                "models": [{"name": model, "digest": digest}]
                            })
                            .to_string(),
                            "/v1/chat/completions" => serde_json::json!({
                                "choices": [{
                                    "message": {
                                        "content": chat_content
                                    }
                                }]
                            })
                            .to_string(),
                            "/api/generate" => "{}".to_string(),
                            _ => "{}".to_string(),
                        };
                        write_json_response(&mut stream, &body);
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("mock Ollama accept failed: {error}"),
                }
            }
        });
        Self {
            address,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}/v1", self.address)
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("request trace").clone()
    }
}

impl Drop for MockOllama {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            thread.join().expect("mock Ollama thread");
        }
    }
}

fn read_request_path(stream: &mut TcpStream) -> Option<String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("mock read timeout");
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 2048];
    let header_end = loop {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..read]);
        assert!(bytes.len() <= 64 * 1024, "mock request headers are bounded");
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).ok()?;
    let request_line = headers.lines().next()?.to_string();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while bytes.len().saturating_sub(header_end) < content_length {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    request_line.split_whitespace().nth(1).map(str::to_string)
}

fn write_json_response(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("write mock response");
}

fn require_exact_probe(provider: &OllamaAnnotator, model: &str, digest: &str) {
    match provider.probe() {
        ServiceAvailability::Available {
            models,
            selected_model_present: true,
            selected_model_digest: Some(probed_digest),
        } => {
            assert_eq!(probed_digest, digest, "selected model digest changed");
            assert!(models.iter().any(|installed| installed == model));
        }
        availability => panic!("exact model is unavailable: {availability:?}"),
    }
}

#[test]
fn synthetic_full_path_uses_only_the_test_owned_loopback_and_replays_without_calls() {
    let model = EXACT_TIERS[0];
    let digest = "a".repeat(64);
    let mock = MockOllama::start(model, &digest);
    assert_ne!(
        mock.address.port(),
        11434,
        "test must not contact installed Ollama"
    );
    let provider = OllamaAnnotator::new(OllamaOptions {
        base_url: mock.base_url(),
        model_override: Some(model.to_string()),
        connect_timeout: Duration::from_secs(2),
        request_timeout: Duration::from_secs(5),
        ..OllamaOptions::default()
    })
    .expect("mock-bound production provider");
    require_exact_probe(&provider, model, &digest);
    let scratch = synthetic_project();
    let metrics = run_acceptance(&scratch.ledger, &provider, model, &digest)
        .expect("mock exact tier acceptance");
    assert!(metrics.calls > 0);
    let requests = mock.requests();
    assert_eq!(requests.first().map(String::as_str), Some("/api/tags"));
    assert_eq!(
        requests
            .iter()
            .filter(|path| path.as_str() == "/v1/chat/completions")
            .count() as u64,
        metrics.calls
    );
    assert_eq!(
        requests
            .iter()
            .filter(|path| path.as_str() == "/api/generate")
            .count() as u64,
        metrics.calls
    );
    assert!(requests.iter().all(|path| matches!(
        path.as_str(),
        "/api/tags" | "/v1/chat/completions" | "/api/generate"
    )));
}

#[test]
fn acceptance_refuses_when_every_model_response_fails_the_schema() {
    let model = EXACT_TIERS[0];
    let digest = "b".repeat(64);
    let mock = MockOllama::start_with_content(
        model,
        &digest,
        "{\"category\":\"invented\",\"confidence\":0.99}",
    );
    let provider = OllamaAnnotator::new(OllamaOptions {
        base_url: mock.base_url(),
        model_override: Some(model.to_string()),
        connect_timeout: Duration::from_secs(2),
        request_timeout: Duration::from_secs(5),
        ..OllamaOptions::default()
    })
    .expect("mock-bound production provider");
    require_exact_probe(&provider, model, &digest);
    let scratch = synthetic_project();
    let refusal = run_acceptance(&scratch.ledger, &provider, model, &digest)
        .expect_err("an all-invalid provider must not pass tier acceptance");
    assert_eq!(
        refusal,
        "exact tier produced no provider-valid classified result"
    );
    let projection = active_language_shadow_projection(&scratch.ledger, PROJECT)
        .expect("validate fail-closed run")
        .expect("fail-closed run is recorded");
    assert_eq!(projection.coverage.classified, 0);
    assert_eq!(
        projection.coverage.uncertain,
        projection.coverage.candidates
    );
    let requests = mock.requests();
    assert_eq!(
        projection.coverage.attempts,
        projection.coverage.candidates * 2,
        "every invalid candidate exhausted the production retry-once path"
    );
    let chat_requests = requests
        .iter()
        .filter(|path| path.as_str() == "/v1/chat/completions")
        .count() as u64;
    assert!(chat_requests > 0);
    assert!(chat_requests <= projection.coverage.attempts);
    assert!(requests.iter().all(|path| matches!(
        path.as_str(),
        "/api/tags" | "/v1/chat/completions" | "/api/generate"
    )));
}

#[test]
fn operator_wrapper_is_fixed_loopback_exact_tier_and_never_manages_models() {
    assert_eq!(WRAPPER.matches("http://").count(), 1);
    assert!(WRAPPER.contains("'http://127.0.0.1:11434/api/tags'"));
    for model in EXACT_TIERS {
        assert!(WRAPPER.contains(model));
    }
    for forbidden in [
        "ollama pull",
        "ollama serve",
        "ollama run",
        "ollama create",
        "curl -L",
        "https://",
        "THRULINE_LANGUAGE_BASE_URL",
    ] {
        assert!(
            !WRAPPER.contains(forbidden),
            "wrapper contains {forbidden:?}"
        );
    }
    assert!(WRAPPER.contains("curl --disable --noproxy '*'"));
    assert!(WRAPPER.contains("--locked --offline --no-default-features"));
    assert!(WRAPPER.contains("--ignored --exact --nocapture --test-threads=1"));
    assert!(WRAPPER.contains("CARGO_TARGET_DIR must be outside the repository"));
}

#[test]
#[ignore = "operator-only: requires exact preinstalled qwen tier on fixed loopback Ollama"]
fn exact_tier_full_project_acceptance() {
    let model = std::env::var("THRULINE_TIER_MODEL").expect("THRULINE_TIER_MODEL is required");
    assert!(
        EXACT_TIERS.contains(&model.as_str()),
        "model tag is not an exact tier"
    );
    let digest = std::env::var("THRULINE_TIER_DIGEST").expect("THRULINE_TIER_DIGEST is required");
    assert!(is_lower_sha256(&digest), "digest must be lowercase SHA-256");
    let provider = OllamaAnnotator::new(OllamaOptions {
        base_url: "http://127.0.0.1:11434/v1".to_string(),
        model_override: Some(model.clone()),
        connect_timeout: Duration::from_secs(2),
        request_timeout: Duration::from_secs(90),
        ..OllamaOptions::default()
    })
    .expect("fixed-loopback production provider");
    require_exact_probe(&provider, &model, &digest);
    let scratch = synthetic_project();
    let metrics = run_acceptance(&scratch.ledger, &provider, &model, &digest)
        .expect("provider-valid exact tier acceptance");
    println!(
        "TIER_ACCEPTANCE model={} digest={} calls={} attempts={} duration_ms={} events={} candidates={} classified={} uncertain={} accepted={} demoted={} inserted={} replay_calls=0 replay_reused={}",
        model,
        digest,
        metrics.calls,
        metrics.attempts,
        metrics.duration_ms,
        metrics.events,
        metrics.candidates,
        metrics.classified,
        metrics.uncertain,
        metrics.accepted,
        metrics.demoted,
        metrics.inserted,
        metrics.replay_reused,
    );
}
