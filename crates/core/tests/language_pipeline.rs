//! M4.2 integration: model judgment categorizes language and stops there.

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use thruline_core::annotate::{
    active_language_shadow_projection, extract_heuristic_claims,
    extract_language_claims_with_memory, score_claims, AnnotateReport, MemoryPressurePoll,
};
use thruline_core::chain::sha256_hex;
use thruline_core::classification_store::{self, NewClassification};
use thruline_core::config::Config;
use thruline_core::derive::{derive_project, derive_project_with, fingerprint, NodeState};
use thruline_core::export::export_project;
use thruline_core::ingest::ingest;
use thruline_core::language::{
    Annotator, ClassificationOutcome, LanguageCategory, LanguageError, LanguageOutput,
    MemoryPressureProbe, MemoryPressureState, MemoryPressureThresholds, NullAnnotator, SpeechAct,
};
use thruline_core::nli::{EntailmentScorer, Scores};
use thruline_core::Ledger;
use thruline_core::Result;

const PROJ: &str = "-Users-dev-language-pipeline";
const SESSION: &str = "cccccccc-1111-4222-8333-444444444444";

fn uuid(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

fn text_event(n: u32, parent: u32, text: &str) -> String {
    serde_json::json!({
        "type": "assistant",
        "uuid": uuid(n),
        "parentUuid": uuid(parent),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-07-31T00:00:{n:02}.000Z"),
        "message": {
            "role": "assistant",
            "content": [{ "type": "text", "text": text }]
        }
    })
    .to_string()
}

fn user_event(n: u32, parent: Option<u32>, text: &str) -> String {
    serde_json::json!({
        "type": "user",
        "uuid": uuid(n),
        "parentUuid": parent.map(uuid),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-07-31T00:00:{n:02}.000Z"),
        "message": { "role": "user", "content": text }
    })
    .to_string()
}

fn tool_event(n: u32, parent: u32, command: &str) -> String {
    serde_json::json!({
        "type": "assistant",
        "uuid": uuid(n),
        "parentUuid": uuid(parent),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-07-31T00:00:{n:02}.000Z"),
        "message": {
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": "tool-1",
                "name": "Bash",
                "input": { "command": command }
            }]
        }
    })
    .to_string()
}

fn result_event(n: u32, parent: u32, result: &str) -> String {
    serde_json::json!({
        "type": "user",
        "uuid": uuid(n),
        "parentUuid": uuid(parent),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-07-31T00:00:{n:02}.000Z"),
        "message": {
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": "tool-1",
                "content": result
            }]
        }
    })
    .to_string()
}

fn setup() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("mkdir");
    let transcript = [
        user_event(1, None, "report"),
        text_event(
            2,
            1,
            "All 12 parser tests pass. Build not saved? This might be useful.",
        ),
        user_event(3, Some(2), "next"),
        text_event(4, 3, "Update `docs/guide.md` next."),
    ]
    .join("\n")
        + "\n";
    fs::write(project.join(format!("{SESSION}.jsonl")), transcript).expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("ledger");
    ingest(
        &mut ledger,
        &[PathBuf::from(&root)],
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    (dir, ledger)
}

fn setup_contract(result: &str) -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("mkdir");
    let transcript = [
        user_event(1, None, "run the parser suite"),
        tool_event(2, 1, "cargo test parser"),
        result_event(3, 2, result),
        text_event(4, 3, "All 12 parser tests pass."),
    ]
    .join("\n")
        + "\n";
    fs::write(project.join(format!("{SESSION}.jsonl")), transcript).expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("ledger");
    ingest(
        &mut ledger,
        &[PathBuf::from(&root)],
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    (dir, ledger)
}

fn setup_single(text: &str) -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("mkdir");
    let transcript = [user_event(1, None, "report"), text_event(2, 1, text)].join("\n") + "\n";
    fs::write(project.join(format!("{SESSION}.jsonl")), transcript).expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("ledger");
    ingest(
        &mut ledger,
        &[PathBuf::from(&root)],
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    (dir, ledger)
}

#[derive(Debug, Clone, Copy)]
enum PressureStep {
    State(MemoryPressureState),
    Failure(&'static str),
}

struct ScriptedPressure {
    steps: Mutex<VecDeque<PressureStep>>,
    fallback: MemoryPressureState,
    samples: AtomicUsize,
    trace: Option<Arc<Mutex<Vec<String>>>>,
}

impl ScriptedPressure {
    fn new(steps: impl IntoIterator<Item = PressureStep>, fallback: MemoryPressureState) -> Self {
        Self {
            steps: Mutex::new(steps.into_iter().collect()),
            fallback,
            samples: AtomicUsize::new(0),
            trace: None,
        }
    }

    fn clear() -> Self {
        Self::new([], MemoryPressureState::Ready)
    }

    fn with_trace(mut self, trace: Arc<Mutex<Vec<String>>>) -> Self {
        self.trace = Some(trace);
        self
    }

    fn samples(&self) -> usize {
        self.samples.load(Ordering::Relaxed)
    }
}

impl MemoryPressureProbe for ScriptedPressure {
    fn sample(
        &self,
        _previous: MemoryPressureState,
        _thresholds: MemoryPressureThresholds,
    ) -> std::result::Result<MemoryPressureState, LanguageError> {
        self.samples.fetch_add(1, Ordering::Relaxed);
        let step = self
            .steps
            .lock()
            .expect("scripted pressure lock")
            .pop_front()
            .unwrap_or(PressureStep::State(self.fallback));
        if let (Some(trace), PressureStep::State(state)) = (&self.trace, step) {
            trace
                .lock()
                .expect("pressure trace lock")
                .push(format!("sample:{state:?}"));
        }
        match step {
            PressureStep::State(state) => Ok(state),
            PressureStep::Failure(message) => {
                Err(LanguageError::MemoryPressure(message.to_string()))
            }
        }
    }
}

struct ScriptedPoll {
    calls: AtomicUsize,
    fail_on_call: Option<usize>,
    trace: Option<Arc<Mutex<Vec<String>>>>,
}

impl ScriptedPoll {
    fn immediate() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            fail_on_call: None,
            trace: None,
        }
    }

    fn cancelling(call: usize) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            fail_on_call: Some(call),
            trace: None,
        }
    }

    fn with_trace(mut self, trace: Arc<Mutex<Vec<String>>>) -> Self {
        self.trace = Some(trace);
        self
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl MemoryPressurePoll for ScriptedPoll {
    fn wait_before_retry(&self) -> std::result::Result<(), LanguageError> {
        let call = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(trace) = &self.trace {
            trace
                .lock()
                .expect("poll trace lock")
                .push("poll".to_string());
        }
        if self.fail_on_call == Some(call) {
            return Err(LanguageError::MemoryPressure(
                "scripted wait cancelled while pressure remains warning".to_string(),
            ));
        }
        Ok(())
    }
}

/// A **liveness** bound for the two hand-offs in
/// `concurrent_scripted_pressure_is_isolated_per_pipeline`, not a speed bound.
/// Both ends of that hand-off are deterministic channels: the release is sent
/// only after the *other* pipeline has finished, so two seconds was a bound on
/// how fast this machine can build a ledger and run a whole extraction while a
/// release build competes for it. Ordering is enforced by the channels; this
/// number only says that a pipeline which has not moved in this long is
/// wedged. (TIMING-TESTS-OWNER, family of R-2026-09-24-27.)
const PIPELINE_HANDOFF_LIVENESS: Duration = Duration::from_secs(120);

struct BlockingPoll {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    calls: AtomicUsize,
}

impl MemoryPressurePoll for BlockingPoll {
    fn wait_before_retry(&self) -> std::result::Result<(), LanguageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.entered.send(()).map_err(|error| {
            LanguageError::MemoryPressure(format!("test pause signal failed: {error}"))
        })?;
        self.release
            .lock()
            .map_err(|_| LanguageError::MemoryPressure("test release lock poisoned".to_string()))?
            .recv_timeout(PIPELINE_HANDOFF_LIVENESS)
            .map_err(|error| {
                LanguageError::MemoryPressure(format!("test pause release failed: {error}"))
            })
    }
}

fn extract_language_claims(
    ledger: &Ledger,
    project_id: &str,
    cfg: &Config,
    annotator: &dyn Annotator,
    model_tag: &str,
) -> Result<AnnotateReport> {
    let pressure = ScriptedPressure::clear();
    let poll = ScriptedPoll::immediate();
    extract_language_claims_with_memory(
        ledger, project_id, cfg, annotator, model_tag, &pressure, &poll,
    )
}

struct FixtureAnnotator {
    calls: AtomicUsize,
    targets: Mutex<Vec<String>>,
    trace: Option<Arc<Mutex<Vec<String>>>>,
}

impl FixtureAnnotator {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            targets: Mutex::new(Vec::new()),
            trace: None,
        }
    }

    fn traced(trace: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            targets: Mutex::new(Vec::new()),
            trace: Some(trace),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    fn targets(&self) -> Vec<String> {
        self.targets.lock().expect("targets lock").clone()
    }
}

impl Annotator for FixtureAnnotator {
    fn classify(&self, message: &str) -> ClassificationOutcome {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let target = message
            .lines()
            .find_map(|line| line.strip_prefix("Target sentence: "))
            .unwrap_or("");
        self.targets
            .lock()
            .expect("targets lock")
            .push(target.to_string());
        if let Some(trace) = &self.trace {
            trace
                .lock()
                .expect("provider trace lock")
                .push(format!("provider:{target}"));
        }
        let (category, confidence) = if target.contains("All 12 parser tests pass") {
            (LanguageCategory::CompletedClaim, 0.97)
        } else if target.contains("Build not saved?") {
            (LanguageCategory::Question, 0.99)
        } else if target.contains("Update `docs/guide.md` next") {
            // Deliberate disagreement with the heuristic proposal.
            (LanguageCategory::Finding, 0.96)
        } else {
            (LanguageCategory::Uncertain, 0.40)
        };
        ClassificationOutcome::Classified {
            output: LanguageOutput::new(category, confidence).expect("valid fixture output"),
            attempts: 1,
        }
    }
}

struct RefusalAnnotator;

impl Annotator for RefusalAnnotator {
    fn classify(&self, _message: &str) -> ClassificationOutcome {
        ClassificationOutcome::Classified {
            output: LanguageOutput::with_shadow(
                LanguageCategory::Proposal,
                0.99,
                SpeechAct::Refusal,
                Some("Decline guide update"),
            )
            .expect("valid shadow output"),
            attempts: 1,
        }
    }
}

struct NeverContradicts;

impl EntailmentScorer for NeverContradicts {
    fn score(&mut self, _premise: &str, _hypothesis: &str) -> Result<Scores> {
        Ok(Scores {
            entailment: 0.30,
            neutral: 0.70,
            contradiction: 0.0,
        })
    }
}

fn classification_rows(ledger: &Ledger) -> i64 {
    ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM annotations WHERE layer = 'classification'",
            [],
            |row| row.get(0),
        )
        .expect("classification row count")
}

fn receipt_model(name: &str) -> String {
    format!("{name}@sha256:{}", "a".repeat(64))
}

struct ActiveLanguageManifest {
    event_id: String,
    model: String,
    output: serde_json::Value,
}

fn active_language_manifest(ledger: &Ledger) -> ActiveLanguageManifest {
    let (event_id, model, output): (String, String, String) = ledger
        .conn()
        .query_row(
            "SELECT a.event_id, a.model, a.output_json
               FROM claim_projection_heads h
               JOIN annotations a ON a.id = h.manifest_annotation_id
              WHERE h.project_id = ?1",
            [PROJ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("active language manifest");
    ActiveLanguageManifest {
        event_id,
        model,
        output: serde_json::from_str(&output).expect("manifest JSON"),
    }
}

fn activate_manifest(
    ledger: &Ledger,
    anchor: &str,
    model: &str,
    prompt: &str,
    output: &serde_json::Value,
) -> String {
    let output_json = serde_json::to_string(output).expect("variant manifest JSON");
    let stored = classification_store::store(
        ledger,
        NewClassification {
            event_id: anchor,
            model,
            prompt,
            output_json: &output_json,
            confidence: 1.0,
            created_at: "2026-08-26T00:00:00Z",
        },
    )
    .expect("store manifest variant")
    .annotation;
    ledger
        .conn()
        .execute(
            "UPDATE claim_projection_heads SET manifest_annotation_id = ?1 WHERE project_id = ?2",
            [&stored.id, PROJ],
        )
        .expect("activate manifest variant");
    stored.id
}

fn rewrite_classification_output(
    ledger: &Ledger,
    annotation_id: &str,
    mutate: impl FnOnce(&mut serde_json::Value),
) {
    let raw: String = ledger
        .conn()
        .query_row(
            "SELECT output_json FROM annotations WHERE id = ?1",
            [annotation_id],
            |row| row.get(0),
        )
        .expect("classification output");
    let mut output: serde_json::Value = serde_json::from_str(&raw).expect("classification JSON");
    mutate(&mut output);
    let output_json = serde_json::to_string(&output).expect("mutated classification JSON");
    let output_sha256 = sha256_hex(output_json.as_bytes());
    ledger
        .conn()
        .execute("DROP TRIGGER classification_annotations_no_update", [])
        .expect("drop update guard in disposable corruption fixture");
    ledger
        .conn()
        .execute(
            "UPDATE annotations SET output_json = ?1, output_sha256 = ?2 WHERE id = ?3",
            rusqlite::params![output_json, output_sha256, annotation_id],
        )
        .expect("rewrite self-consistent corrupt classification output");
}

#[test]
fn clear_pressure_proceeds_without_pause_and_persists_once() {
    let (_dir, ledger) = setup_single("All 12 parser tests pass.");
    let cfg = Config::default();
    let provider = FixtureAnnotator::new();
    let pressure = ScriptedPressure::new(
        [PressureStep::State(MemoryPressureState::Ready)],
        MemoryPressureState::Ready,
    );
    let poll = ScriptedPoll::immediate();

    let report = extract_language_claims_with_memory(
        &ledger,
        PROJ,
        &cfg,
        &provider,
        "fixture:clear",
        &pressure,
        &poll,
    )
    .expect("clear pressure extraction");

    assert_eq!(provider.calls(), 1);
    assert_eq!(pressure.samples(), 1);
    assert_eq!(poll.calls(), 0);
    assert_eq!(report.classification_memory_pauses, 0);
    assert_eq!(report.classification_candidates, 1);
    assert_eq!(report.claims_written, 1);
    assert_eq!(classification_rows(&ledger), 2, "event plus manifest");
}

#[test]
fn warning_then_clear_keeps_work_queued_and_resumes_once() {
    let (_dir, ledger) = setup_single("All 12 parser tests pass.");
    let cfg = Config::default();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let provider = FixtureAnnotator::traced(Arc::clone(&trace));
    let pressure = ScriptedPressure::new(
        [
            PressureStep::State(MemoryPressureState::Paused),
            PressureStep::State(MemoryPressureState::Paused),
            PressureStep::State(MemoryPressureState::Ready),
        ],
        MemoryPressureState::Ready,
    )
    .with_trace(Arc::clone(&trace));
    let poll = ScriptedPoll::immediate().with_trace(Arc::clone(&trace));

    let report = extract_language_claims_with_memory(
        &ledger,
        PROJ,
        &cfg,
        &provider,
        "fixture:warning-clear",
        &pressure,
        &poll,
    )
    .expect("warning clears");

    assert_eq!(provider.calls(), 1);
    assert_eq!(pressure.samples(), 3);
    assert_eq!(poll.calls(), 2);
    assert_eq!(report.classification_memory_pauses, 1);
    assert_eq!(classification_rows(&ledger), 2, "no duplicate write");
    assert_eq!(
        *trace.lock().expect("trace lock"),
        vec![
            "sample:Paused",
            "poll",
            "sample:Paused",
            "poll",
            "sample:Ready",
            "provider:All 12 parser tests pass.",
        ],
        "provider must remain behind every warning observation"
    );
}

#[test]
fn sustained_warning_cancels_deterministically_without_provider_or_annotation() {
    let (_dir, ledger) = setup_single("All 12 parser tests pass.");
    let cfg = Config::default();
    let provider = FixtureAnnotator::new();
    let pressure = ScriptedPressure::new(
        [PressureStep::State(MemoryPressureState::Paused)],
        MemoryPressureState::Paused,
    );
    let poll = ScriptedPoll::cancelling(3);

    let error = extract_language_claims_with_memory(
        &ledger,
        PROJ,
        &cfg,
        &provider,
        "fixture:sustained-warning",
        &pressure,
        &poll,
    )
    .expect_err("scripted cancellation must end the sustained warning test");

    assert!(
        error.to_string().contains("pressure remains warning"),
        "{error}"
    );
    assert_eq!(provider.calls(), 0);
    assert_eq!(pressure.samples(), 3);
    assert_eq!(poll.calls(), 3);
    assert_eq!(classification_rows(&ledger), 0);
}

#[test]
fn pressure_return_after_multiple_polls_resumes_and_persists_exactly_once() {
    let (_dir, ledger) = setup_single("All 12 parser tests pass.");
    let cfg = Config::default();
    let provider = FixtureAnnotator::new();
    let pressure = ScriptedPressure::new(
        [
            PressureStep::State(MemoryPressureState::Paused),
            PressureStep::State(MemoryPressureState::Paused),
            PressureStep::State(MemoryPressureState::Paused),
            PressureStep::State(MemoryPressureState::Ready),
        ],
        MemoryPressureState::Ready,
    );
    let poll = ScriptedPoll::immediate();

    let report = extract_language_claims_with_memory(
        &ledger,
        PROJ,
        &cfg,
        &provider,
        "fixture:multiple-polls",
        &pressure,
        &poll,
    )
    .expect("pressure clears after multiple polls");

    assert_eq!(pressure.samples(), 4);
    assert_eq!(poll.calls(), 3);
    assert_eq!(report.classification_memory_pauses, 1);
    assert_eq!(provider.calls(), 1);
    assert_eq!(classification_rows(&ledger), 2);
}

#[test]
fn pressure_probe_failure_is_fail_closed_and_visible() {
    let (_dir, ledger) = setup_single("All 12 parser tests pass.");
    let cfg = Config::default();
    let provider = FixtureAnnotator::new();
    let pressure = ScriptedPressure::new(
        [PressureStep::Failure("scripted probe failure")],
        MemoryPressureState::Ready,
    );
    let poll = ScriptedPoll::immediate();

    let error = extract_language_claims_with_memory(
        &ledger,
        PROJ,
        &cfg,
        &provider,
        "fixture:probe-failure",
        &pressure,
        &poll,
    )
    .expect_err("probe failure must not become clear pressure");

    assert!(
        error.to_string().contains("scripted probe failure"),
        "{error}"
    );
    assert_eq!(provider.calls(), 0);
    assert_eq!(pressure.samples(), 1);
    assert_eq!(poll.calls(), 0);
    assert_eq!(classification_rows(&ledger), 0);
}

#[test]
fn queued_candidates_resume_in_canonical_order_without_loss_or_duplicates() {
    let (_dir, ledger) = setup();
    let cfg = Config::default();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let provider = FixtureAnnotator::traced(Arc::clone(&trace));
    let pressure = ScriptedPressure::new(
        [
            PressureStep::State(MemoryPressureState::Paused),
            PressureStep::State(MemoryPressureState::Paused),
            PressureStep::State(MemoryPressureState::Ready),
        ],
        MemoryPressureState::Ready,
    )
    .with_trace(Arc::clone(&trace));
    let poll = ScriptedPoll::immediate().with_trace(Arc::clone(&trace));

    let report = extract_language_claims_with_memory(
        &ledger,
        PROJ,
        &cfg,
        &provider,
        "fixture:queued",
        &pressure,
        &poll,
    )
    .expect("queued candidates resume");

    assert_eq!(report.classification_candidates, 4);
    assert_eq!(report.classification_memory_pauses, 1);
    assert_eq!(provider.calls(), 4);
    assert_eq!(
        provider.targets(),
        vec![
            "All 12 parser tests pass.",
            "Build not saved?",
            "This might be useful.",
            "Update `docs/guide.md` next.",
        ]
    );
    assert_eq!(classification_rows(&ledger), 3, "two events plus manifest");
    assert_eq!(
        *trace.lock().expect("trace lock"),
        vec![
            "sample:Paused",
            "poll",
            "sample:Paused",
            "poll",
            "sample:Ready",
            "provider:All 12 parser tests pass.",
            "sample:Ready",
            "provider:Build not saved?",
            "sample:Ready",
            "provider:This might be useful.",
            "sample:Ready",
            "provider:Update `docs/guide.md` next.",
        ],
        "every queued provider call must retain its own preceding pressure gate"
    );
}

#[test]
fn concurrent_scripted_pressure_is_isolated_per_pipeline() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let warning = std::thread::spawn(move || {
        let (_dir, ledger) = setup_single("All 12 parser tests pass.");
        let provider = FixtureAnnotator::new();
        let pressure = ScriptedPressure::new(
            [
                PressureStep::State(MemoryPressureState::Paused),
                PressureStep::State(MemoryPressureState::Ready),
            ],
            MemoryPressureState::Ready,
        );
        let poll = BlockingPoll {
            entered: entered_tx,
            release: Mutex::new(release_rx),
            calls: AtomicUsize::new(0),
        };
        let result = extract_language_claims_with_memory(
            &ledger,
            PROJ,
            &Config::default(),
            &provider,
            "fixture:concurrent-warning",
            &pressure,
            &poll,
        );
        (
            result,
            provider.calls(),
            pressure.samples(),
            poll.calls.load(Ordering::Relaxed),
        )
    });

    let entered = entered_rx.recv_timeout(PIPELINE_HANDOFF_LIVENESS);
    let (_dir, ledger) = setup_single("All 12 parser tests pass.");
    let provider = FixtureAnnotator::new();
    let pressure = ScriptedPressure::clear();
    let poll = ScriptedPoll::immediate();
    let clear = extract_language_claims_with_memory(
        &ledger,
        PROJ,
        &Config::default(),
        &provider,
        "fixture:concurrent-clear",
        &pressure,
        &poll,
    );
    let released = release_tx.send(());
    let (warning_result, warning_calls, warning_samples, warning_polls) =
        warning.join().expect("warning thread");

    entered.expect("warning pipeline reached its deterministic pause");
    released.expect("warning pipeline release");
    clear.expect("clear pipeline must finish while the other pipeline is paused");
    warning_result.expect("warning pipeline resumes after release");
    assert_eq!(
        (provider.calls(), pressure.samples(), poll.calls()),
        (1, 1, 0)
    );
    assert_eq!((warning_calls, warning_samples, warning_polls), (1, 2, 1));
}

#[test]
fn agreement_promotes_while_disagreement_uncertainty_and_questions_stay_safe() {
    let (_dir, ledger) = setup();
    let cfg = Config::default();
    let provider = FixtureAnnotator::new();
    let report = extract_language_claims(&ledger, PROJ, &cfg, &provider, "fixture:1b")
        .expect("language extraction");

    assert_eq!(report.classification_candidates, 4);
    assert_eq!(report.classification_accepted, 1);
    assert_eq!(report.classification_demoted, 1);
    assert_eq!(report.classification_uncertain, 1);
    assert_eq!(report.claims_written, 1);
    assert_eq!(provider.calls(), 4);

    let claims: Vec<(String, String)> = {
        let mut stmt = ledger
            .conn()
            .prepare("SELECT text, kind FROM claims ORDER BY text")
            .expect("claims");
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query")
            .collect::<std::result::Result<_, _>>()
            .expect("rows")
    };
    assert_eq!(
        claims,
        vec![(
            "All 12 parser tests pass.".to_string(),
            "marker".to_string()
        )]
    );

    let graph = derive_project_with(&ledger, PROJ, &cfg).expect("derive");
    assert!(graph
        .nodes
        .iter()
        .any(|node| node.claim_text.contains("All 12 parser tests pass")));
    assert!(graph
        .nodes
        .iter()
        .all(|node| !node.claim_text.contains("Update `docs/guide.md` next")));

    let first = fingerprint(&graph);
    let second = fingerprint(&derive_project_with(&ledger, PROJ, &cfg).expect("derive again"));
    assert_eq!(
        first, second,
        "a complete frozen language projection must derive byte-identically"
    );
}

#[test]
fn complete_run_manifest_names_every_frozen_event_annotation_exactly_once() {
    let (_dir, ledger) = setup();
    let report = extract_language_claims(
        &ledger,
        PROJ,
        &Config::default(),
        &FixtureAnnotator::new(),
        "fixture:manifest-members",
    )
    .expect("language extraction");

    let raw: String = ledger
        .conn()
        .query_row(
            "SELECT output_json FROM annotations
             WHERE layer='classification' AND output_json LIKE '%classification_run%'",
            [],
            |row| row.get(0),
        )
        .expect("run manifest");
    let manifest: serde_json::Value = serde_json::from_str(&raw).expect("manifest JSON");
    assert_eq!(manifest["truth_bearing"], false);
    assert_eq!(
        manifest["prompt_revision"],
        thruline_core::language::PROMPT_REVISION
    );
    let members = manifest["members"].as_array().expect("member array");
    assert_eq!(members.len() as u64, report.classification_events);

    let mut event_ids = std::collections::BTreeSet::new();
    let mut annotation_ids = std::collections::BTreeSet::new();
    for member in members {
        let event_id = member["event_id"].as_str().expect("event id");
        let annotation_id = member["annotation_id"].as_str().expect("annotation id");
        assert!(event_ids.insert(event_id), "duplicate event membership");
        assert!(
            annotation_ids.insert(annotation_id),
            "duplicate annotation membership"
        );
        let bound_event: String = ledger
            .conn()
            .query_row(
                "SELECT event_id FROM annotations
                 WHERE id=?1 AND layer='classification'",
                [annotation_id],
                |row| row.get(0),
            )
            .expect("member annotation");
        assert_eq!(bound_event, event_id);
    }
}

#[test]
fn active_shadow_projection_exposes_coverage_provenance_and_receipts_but_no_confidence() {
    let (_dir, ledger) = setup();
    let model = format!("fixture:manifest@sha256:{}", "a".repeat(64));
    let report = extract_language_claims(
        &ledger,
        PROJ,
        &Config::default(),
        &FixtureAnnotator::new(),
        &model,
    )
    .expect("language extraction");
    let projection = active_language_shadow_projection(&ledger, PROJ)
        .expect("projection")
        .expect("active language layer");

    assert!(!projection.truth_bearing);
    assert_eq!(projection.scope, "at_export");
    assert_eq!(projection.run.model, "fixture:manifest");
    assert_eq!(projection.run.model_digest, "a".repeat(64));
    assert_eq!(projection.coverage.events, report.classification_events);
    assert_eq!(
        projection.coverage.candidates,
        report.classification_candidates
    );
    assert_eq!(
        projection.coverage.classified + projection.coverage.uncertain,
        projection.coverage.candidates
    );
    assert_eq!(
        projection.items.len() as u64,
        projection.coverage.candidates
    );
    assert!(projection.items.iter().all(|item| {
        let receipt_project: String = ledger
            .conn()
            .query_row(
                "SELECT project_id FROM events WHERE id=?1",
                [&item.event_id],
                |row| row.get(0),
            )
            .expect("item event receipt");
        receipt_project == PROJ
            && item.span[0] < item.span[1]
            && item.prompt_sha256.len() == 64
            && item.output_sha256.len() == 64
    }));

    let serialized = serde_json::to_string(&projection).expect("serialize projection");
    for forbidden in ["confidence", "reason", "raw_file", "prompt\""] {
        assert!(
            !serialized.contains(forbidden),
            "ordinary shadow export leaked {forbidden:?}: {serialized}"
        );
    }

    let graph = derive_project(&ledger, PROJ).expect("derive active language projection");
    let exported = export_project(&ledger, &graph, "2026-08-26T00:00:00Z")
        .expect("export active language projection");
    assert_eq!(
        exported["language_annotations"],
        serde_json::to_value(&projection).expect("projection JSON"),
        "the export must carry exactly the independently validated shadow projection"
    );
    assert_eq!(exported["language_annotations"]["truth_bearing"], false);
    assert_eq!(exported["language_annotations"]["scope"], "at_export");
    let exported_serialized =
        serde_json::to_string(&exported["language_annotations"]).expect("exported shadow JSON");
    for forbidden in ["confidence", "reason", "raw_file", "prompt\""] {
        assert!(
            !exported_serialized.contains(forbidden),
            "ordinary export leaked {forbidden:?}: {exported_serialized}"
        );
    }
}

#[test]
fn unsafe_language_model_tags_fail_before_provider_or_persistence() {
    let (_dir, ledger) = setup_single("All 12 parser tests pass.");
    let provider = FixtureAnnotator::new();
    let unsafe_models = [
        "/Users/private/model".to_string(),
        "provider supplied prose".to_string(),
        "ghp_1234567890abcdefghijkl".to_string(),
        format!("fixture@sha256:bad@sha256:{}", "a".repeat(64)),
    ];

    for model in unsafe_models {
        let error = extract_language_claims(&ledger, PROJ, &Config::default(), &provider, &model)
            .expect_err("unsafe model identity");
        assert!(error.to_string().contains("bounded safe tag"), "{error}");
        assert!(!error.to_string().contains(&model));
    }
    assert_eq!(provider.calls(), 0);
    assert_eq!(classification_rows(&ledger), 0);
}

#[test]
fn active_projection_rejects_a_member_from_a_different_prompt_receipt() {
    let (_dir, ledger) = setup();
    let model = receipt_model("fixture:member-prompt");
    extract_language_claims(
        &ledger,
        PROJ,
        &Config::default(),
        &FixtureAnnotator::new(),
        &model,
    )
    .expect("language extraction");
    let mut manifest = active_language_manifest(&ledger);
    let member = manifest.output["members"][0].clone();
    let member_id = member["annotation_id"].as_str().expect("member id");
    let event_id = member["event_id"].as_str().expect("member event");
    let output_json: String = ledger
        .conn()
        .query_row(
            "SELECT output_json FROM annotations WHERE id = ?1",
            [member_id],
            |row| row.get(0),
        )
        .expect("member output");
    let alternate = classification_store::store(
        &ledger,
        NewClassification {
            event_id,
            model: &manifest.model,
            prompt: "valid shape, wrong event prompt",
            output_json: &output_json,
            confidence: 0.5,
            created_at: "2026-08-26T00:00:00Z",
        },
    )
    .expect("alternate prompt annotation")
    .annotation;
    manifest.output["members"][0]["annotation_id"] = alternate.id.into();
    activate_manifest(
        &ledger,
        &manifest.event_id,
        &manifest.model,
        "member-prompt manifest variant",
        &manifest.output,
    );

    let error =
        active_language_shadow_projection(&ledger, PROJ).expect_err("member prompt mismatch");
    assert!(
        error.to_string().contains("canonical input receipt"),
        "{error}"
    );
}

#[test]
fn active_projection_rejects_wrong_run_prompt_and_noncanonical_annotation_id() {
    {
        let (_dir, ledger) = setup();
        let model = receipt_model("fixture:run-prompt");
        extract_language_claims(
            &ledger,
            PROJ,
            &Config::default(),
            &FixtureAnnotator::new(),
            &model,
        )
        .expect("language extraction");
        let manifest = active_language_manifest(&ledger);
        activate_manifest(
            &ledger,
            &manifest.event_id,
            &manifest.model,
            "wrong classification-run prompt",
            &manifest.output,
        );
        let error =
            active_language_shadow_projection(&ledger, PROJ).expect_err("run prompt mismatch");
        assert!(
            error.to_string().contains("canonical run receipt"),
            "{error}"
        );
    }

    {
        let (_dir, ledger) = setup();
        let model = receipt_model("fixture:member-id");
        extract_language_claims(
            &ledger,
            PROJ,
            &Config::default(),
            &FixtureAnnotator::new(),
            &model,
        )
        .expect("language extraction");
        let mut manifest = active_language_manifest(&ledger);
        let old_id = manifest.output["members"][0]["annotation_id"]
            .as_str()
            .expect("member id")
            .to_string();
        let false_id = format!("a{}", "f".repeat(31));
        ledger
            .conn()
            .execute("DROP TRIGGER classification_annotations_no_update", [])
            .expect("drop update guard in disposable corruption fixture");
        ledger
            .conn()
            .execute(
                "UPDATE annotations SET id = ?1 WHERE id = ?2",
                [&false_id, &old_id],
            )
            .expect("rewrite member id");
        manifest.output["members"][0]["annotation_id"] = false_id.into();
        activate_manifest(
            &ledger,
            &manifest.event_id,
            &manifest.model,
            "member-id manifest variant",
            &manifest.output,
        );
        let error =
            active_language_shadow_projection(&ledger, PROJ).expect_err("noncanonical member id");
        assert!(
            error.to_string().contains("canonical input receipt"),
            "{error}"
        );
    }

    {
        let (_dir, ledger) = setup();
        let model = receipt_model("fixture:run-id");
        extract_language_claims(
            &ledger,
            PROJ,
            &Config::default(),
            &FixtureAnnotator::new(),
            &model,
        )
        .expect("language extraction");
        let canonical_id: String = ledger
            .conn()
            .query_row(
                "SELECT manifest_annotation_id FROM claim_projection_heads WHERE project_id = ?1",
                [PROJ],
                |row| row.get(0),
            )
            .expect("manifest id");
        let false_id = format!("a{}", "e".repeat(31));
        ledger
            .conn()
            .execute("DROP TRIGGER classification_annotations_no_replace", [])
            .expect("drop insert guard in disposable corruption fixture");
        ledger
            .conn()
            .execute("DROP INDEX idx_classification_annotation_key", [])
            .expect("drop receipt key in disposable corruption fixture");
        ledger
            .conn()
            .execute(
                "INSERT INTO annotations(
                    id, event_id, layer, model, prompt_sha256, output_json,
                    output_sha256, confidence, created_at
                 )
                 SELECT ?1, event_id, layer, model, prompt_sha256, output_json,
                        output_sha256, confidence, created_at
                   FROM annotations WHERE id = ?2",
                [&false_id, &canonical_id],
            )
            .expect("clone manifest under noncanonical id");
        ledger
            .conn()
            .execute(
                "UPDATE claim_projection_heads SET manifest_annotation_id = ?1
                  WHERE project_id = ?2",
                [&false_id, PROJ],
            )
            .expect("activate noncanonical manifest id");
        let error =
            active_language_shadow_projection(&ledger, PROJ).expect_err("noncanonical run id");
        assert!(
            error.to_string().contains("canonical run receipt"),
            "{error}"
        );
    }
}

#[test]
fn active_projection_reconstructs_admission_before_trusting_claim_set() {
    let (_dir, ledger) = setup();
    let model = receipt_model("fixture:claim-binding");
    extract_language_claims(
        &ledger,
        PROJ,
        &Config::default(),
        &FixtureAnnotator::new(),
        &model,
    )
    .expect("language extraction");
    let manifest = active_language_manifest(&ledger);
    let member_id = manifest.output["members"][0]["annotation_id"]
        .as_str()
        .expect("member id")
        .to_string();
    rewrite_classification_output(&ledger, &member_id, |output| {
        output["results"][0]["category"] = "narration".into();
    });

    let error = active_language_shadow_projection(&ledger, PROJ)
        .expect_err("classification/claim mismatch");
    assert!(
        error.to_string().contains("classified claim projection"),
        "{error}"
    );
}

#[test]
fn active_projection_rejects_wrong_anchor_and_partial_or_reordered_members() {
    {
        let (_dir, ledger) = setup();
        let model = receipt_model("fixture:wrong-anchor");
        extract_language_claims(
            &ledger,
            PROJ,
            &Config::default(),
            &FixtureAnnotator::new(),
            &model,
        )
        .expect("language extraction");
        let manifest = active_language_manifest(&ledger);
        let wrong_anchor = manifest.output["members"][1]["event_id"]
            .as_str()
            .expect("second event")
            .to_string();
        activate_manifest(
            &ledger,
            &wrong_anchor,
            &manifest.model,
            "wrong-anchor manifest variant",
            &manifest.output,
        );
        let error =
            active_language_shadow_projection(&ledger, PROJ).expect_err("wrong manifest anchor");
        assert!(
            error.to_string().contains("exact project event frontier"),
            "{error}"
        );
    }

    for mutation in ["partial", "reordered", "duplicate"] {
        let (_dir, ledger) = setup();
        let model = receipt_model(&format!("fixture:{mutation}"));
        extract_language_claims(
            &ledger,
            PROJ,
            &Config::default(),
            &FixtureAnnotator::new(),
            &model,
        )
        .expect("language extraction");
        let mut manifest = active_language_manifest(&ledger);
        let members = manifest.output["members"]
            .as_array_mut()
            .expect("manifest members");
        match mutation {
            "partial" => {
                members.pop();
            }
            "reordered" => members.reverse(),
            "duplicate" => members[1] = members[0].clone(),
            _ => unreachable!(),
        }
        activate_manifest(
            &ledger,
            &manifest.event_id,
            &manifest.model,
            &format!("{mutation} manifest variant"),
            &manifest.output,
        );
        let error = active_language_shadow_projection(&ledger, PROJ)
            .expect_err("invalid manifest membership");
        let message = error.to_string();
        assert!(
            message.contains("exact project event frontier")
                || message.contains("duplicate membership"),
            "{mutation}: {message}"
        );
    }
}

#[test]
fn active_projection_rejects_stale_or_unsafe_run_provenance() {
    {
        let (_dir, ledger) = setup();
        let model = receipt_model("fixture:old-revision");
        extract_language_claims(
            &ledger,
            PROJ,
            &Config::default(),
            &FixtureAnnotator::new(),
            &model,
        )
        .expect("language extraction");
        let mut manifest = active_language_manifest(&ledger);
        manifest.output["prompt_revision"] = "obsolete-language-prompt".into();
        activate_manifest(
            &ledger,
            &manifest.event_id,
            &manifest.model,
            "old-revision manifest variant",
            &manifest.output,
        );
        let error =
            active_language_shadow_projection(&ledger, PROJ).expect_err("old prompt revision");
        assert!(error.to_string().contains("malformed"), "{error}");
    }

    {
        let (_dir, ledger) = setup();
        let model = receipt_model("fixture:unsafe-extractor");
        extract_language_claims(
            &ledger,
            PROJ,
            &Config::default(),
            &FixtureAnnotator::new(),
            &model,
        )
        .expect("language extraction");
        let mut manifest = active_language_manifest(&ledger);
        let unsafe_extractor = "/Users/private/provider prose";
        manifest.output["extractor"] = unsafe_extractor.into();
        activate_manifest(
            &ledger,
            &manifest.event_id,
            &manifest.model,
            "unsafe-extractor manifest variant",
            &manifest.output,
        );
        ledger
            .conn()
            .execute(
                "UPDATE claim_projection_heads SET extractor = ?1 WHERE project_id = ?2",
                [unsafe_extractor, PROJ],
            )
            .expect("coordinate unsafe head extractor");
        let error = active_language_shadow_projection(&ledger, PROJ).expect_err("unsafe extractor");
        assert!(error.to_string().contains("extractor identity"), "{error}");
        assert!(!error.to_string().contains(unsafe_extractor));
    }

    {
        let (_dir, ledger) = setup();
        let model = receipt_model("fixture:unsafe-created-at");
        extract_language_claims(
            &ledger,
            PROJ,
            &Config::default(),
            &FixtureAnnotator::new(),
            &model,
        )
        .expect("language extraction");
        let manifest_id: String = ledger
            .conn()
            .query_row(
                "SELECT manifest_annotation_id FROM claim_projection_heads WHERE project_id = ?1",
                [PROJ],
                |row| row.get(0),
            )
            .expect("manifest id");
        ledger
            .conn()
            .execute("DROP TRIGGER classification_annotations_no_update", [])
            .expect("drop update guard in disposable corruption fixture");
        ledger
            .conn()
            .execute(
                "UPDATE annotations SET created_at = '/Users/private/secret' WHERE id = ?1",
                [&manifest_id],
            )
            .expect("rewrite manifest timestamp");
        let error = active_language_shadow_projection(&ledger, PROJ).expect_err("unsafe timestamp");
        assert!(error.to_string().contains("malformed"), "{error}");
        assert!(!error.to_string().contains("/Users/private"));
    }

    {
        let (_dir, ledger) = setup();
        let model = receipt_model("fixture:safe-source");
        extract_language_claims(
            &ledger,
            PROJ,
            &Config::default(),
            &FixtureAnnotator::new(),
            &model,
        )
        .expect("language extraction");
        let manifest = active_language_manifest(&ledger);
        let unsafe_model = format!("/Users/private/model@sha256:{}", "b".repeat(64));
        activate_manifest(
            &ledger,
            &manifest.event_id,
            &unsafe_model,
            "unsafe-model manifest variant",
            &manifest.output,
        );
        let error =
            active_language_shadow_projection(&ledger, PROJ).expect_err("unsafe stored model");
        assert!(
            error.to_string().contains("bounded immutable model"),
            "{error}"
        );
        assert!(!error.to_string().contains("/Users/private"));
    }
}

#[test]
fn active_projection_rejects_invalid_stored_confidence_and_uncertain_relationships() {
    for mutation in [
        "confidence",
        "attempts_high",
        "attempts_zero_classified",
        "uncertain",
        "provider_reason",
    ] {
        let (_dir, ledger) = setup();
        let model = receipt_model(&format!("fixture:stored-{mutation}"));
        extract_language_claims(
            &ledger,
            PROJ,
            &Config::default(),
            &FixtureAnnotator::new(),
            &model,
        )
        .expect("language extraction");
        let manifest = active_language_manifest(&ledger);
        let member_id = manifest.output["members"][0]["annotation_id"]
            .as_str()
            .expect("member id")
            .to_string();
        rewrite_classification_output(&ledger, &member_id, |output| match mutation {
            "confidence" => output["results"][0]["confidence"] = 2.0.into(),
            "attempts_high" => output["results"][0]["attempts"] = 255.into(),
            "attempts_zero_classified" => output["results"][0]["attempts"] = 0.into(),
            "uncertain" => {
                output["results"][0]["category"] = "uncertain".into();
                output["results"][0]["speech_act"] = "decision".into();
                output["results"][0]["label"] = "Review parser".into();
            }
            "provider_reason" => {
                output["results"][0]["reason"] = "provider supplied prose".into();
            }
            _ => unreachable!(),
        });
        let error = active_language_shadow_projection(&ledger, PROJ)
            .expect_err("invalid stored classification semantics");
        let message = error.to_string();
        assert!(
            message.contains("confidence outside")
                || message.contains("impossible provider attempt count")
                || message.contains("speech act or label")
                || message.contains("provider failure prose"),
            "{mutation}: {message}"
        );
        assert!(!message.contains("provider supplied prose"));
    }
}

#[test]
fn active_projection_preserves_zero_attempt_no_provider_uncertainty() {
    let (_dir, ledger) = setup_single("All 12 parser tests pass.");
    let model = receipt_model("disabled");
    extract_language_claims(&ledger, PROJ, &Config::default(), &NullAnnotator, &model)
        .expect("disabled language projection");

    let projection = active_language_shadow_projection(&ledger, PROJ)
        .expect("resolve disabled projection")
        .expect("active disabled projection");
    assert!(projection.items.iter().all(|item| item.attempts == 0));
    assert_eq!(projection.coverage.accepted_claims, 0);
    assert_eq!(projection.coverage.classified, 0);
    assert_eq!(
        projection.coverage.uncertain,
        projection.coverage.candidates
    );
}

#[test]
fn a_new_event_atomically_inerts_optional_projection_heads_without_erasing_history() {
    let (dir, mut ledger) = setup();
    let cfg = Config::default();
    let language_model = format!("fixture:freshness@sha256:{}", "a".repeat(64));
    let report = extract_language_claims(
        &ledger,
        PROJ,
        &cfg,
        &FixtureAnnotator::new(),
        &language_model,
    )
    .expect("complete language projection");
    score_claims(
        &ledger,
        PROJ,
        &cfg,
        &mut NeverContradicts,
        "freshness-scorer",
        report,
    )
    .expect("complete scorer projection");

    let active_heads = |ledger: &Ledger| -> (Option<String>, String) {
        let claim = ledger
            .conn()
            .query_row(
                "SELECT manifest_annotation_id FROM claim_projection_heads
                 WHERE project_id = ?1",
                [PROJ],
                |row| row.get(0),
            )
            .expect("claim projection head");
        let scorer = ledger
            .conn()
            .query_row(
                "SELECT scorer FROM entailment_projection_heads WHERE project_id = ?1",
                [PROJ],
                |row| row.get(0),
            )
            .expect("entailment projection head");
        (claim, scorer)
    };
    let before_heads = active_heads(&ledger);
    assert!(before_heads.0.is_some());
    assert_eq!(before_heads.1, "freshness-scorer");

    let historical_counts = |ledger: &Ledger| -> (i64, i64, i64) {
        (
            ledger
                .conn()
                .query_row("SELECT COUNT(*) FROM annotations", [], |row| row.get(0))
                .expect("annotation count"),
            ledger
                .conn()
                .query_row("SELECT COUNT(*) FROM claims", [], |row| row.get(0))
                .expect("claim count"),
            ledger
                .conn()
                .query_row("SELECT COUNT(*) FROM evidence_links", [], |row| row.get(0))
                .expect("evidence count"),
        )
    };
    let before_history = historical_counts(&ledger);
    let before_event_count = ledger.event_count(PROJ).expect("event count");
    let before_graph = derive_project(&ledger, PROJ).expect("derive active projection");
    let before_export = export_project(&ledger, &before_graph, "2026-08-26T00:00:00Z")
        .expect("export active projection");
    assert!(before_export.get("language_annotations").is_some());

    let root = dir.path().join("projects");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("idempotent ingest");
    assert_eq!(
        ledger.event_count(PROJ).expect("unchanged count"),
        before_event_count
    );
    assert_eq!(
        active_heads(&ledger),
        before_heads,
        "no INSERT must preserve both heads"
    );

    let source = root.join(PROJ).join(format!("{SESSION}.jsonl"));
    let mut transcript = fs::read_to_string(&source).expect("read transcript");
    transcript.push_str(&user_event(5, Some(4), "new recorded request"));
    transcript.push('\n');
    fs::write(&source, transcript).expect("append one source event");
    ingest(&mut ledger, &[root], &dir.path().join("mirrors")).expect("ingest new event");

    assert_eq!(
        ledger.event_count(PROJ).expect("advanced count"),
        before_event_count + 1
    );
    assert_eq!(
        active_heads(&ledger),
        (None, String::new()),
        "the event INSERT must atomically inert both optional projection heads"
    );
    assert_eq!(
        ledger
            .active_entailment_scorer(PROJ)
            .expect("inert scorer authority"),
        None
    );
    assert_eq!(
        historical_counts(&ledger),
        before_history,
        "freshness invalidation must retain annotation, claim, and evidence history"
    );

    let graph = derive_project(&ledger, PROJ).expect("deterministic derive remains usable");
    let again = derive_project(&ledger, PROJ).expect("deterministic derive remains stable");
    assert_eq!(fingerprint(&graph), fingerprint(&again));
    let exported = export_project(&ledger, &graph, "2026-08-26T00:01:00Z")
        .expect("deterministic export remains usable");
    assert!(
        exported.get("language_annotations").is_none(),
        "an invalidated optional language projection must not cross the export boundary"
    );
}

#[test]
fn refusal_is_persisted_as_shadow_metadata_and_only_demotes_a_proposal() {
    let (_dir, ledger) = setup_single("I will update `docs/guide.md` next.");
    let report = extract_language_claims(
        &ledger,
        PROJ,
        &Config::default(),
        &RefusalAnnotator,
        "fixture:refusal",
    )
    .expect("language extraction");

    assert_eq!(report.classification_candidates, 1);
    assert_eq!(report.classification_accepted, 0);
    assert_eq!(report.classification_demoted, 1);
    assert_eq!(report.claims_written, 0);
    let output: String = ledger
        .conn()
        .query_row(
            "SELECT output_json FROM annotations
             WHERE layer = 'classification' AND output_json LIKE '%event_classification%'",
            [],
            |row| row.get(0),
        )
        .expect("frozen event output");
    let output: serde_json::Value = serde_json::from_str(&output).expect("stored JSON");
    assert_eq!(output["results"][0]["speech_act"], "refusal");
    assert_eq!(output["results"][0]["label"], "Decline guide update");
    assert!(ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM claims", [], |row| row
            .get::<_, i64>(0))
        .is_ok_and(|count| count == 0));
}

#[test]
fn frozen_event_annotations_are_reused_and_model_swap_appends() {
    let (_dir, ledger) = setup();
    let cfg = Config::default();
    let first = FixtureAnnotator::new();
    extract_language_claims(&ledger, PROJ, &cfg, &first, "fixture:1b").expect("first");
    assert_eq!(first.calls(), 4);

    let replay = FixtureAnnotator::new();
    let replay_report =
        extract_language_claims(&ledger, PROJ, &cfg, &replay, "fixture:1b").expect("cached replay");
    assert_eq!(
        replay.calls(),
        0,
        "frozen inputs must not call the model again"
    );
    assert_eq!(replay_report.classification_annotations_inserted, 0);
    assert_eq!(replay_report.classification_annotations_reused, 3);

    let swapped = FixtureAnnotator::new();
    extract_language_claims(&ledger, PROJ, &cfg, &swapped, "fixture:4b").expect("model swap");
    assert_eq!(swapped.calls(), 4);
    let rows: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM annotations WHERE layer = 'classification'",
            [],
            |row| row.get(0),
        )
        .expect("classification rows");
    assert_eq!(rows, 6, "two event rows + one run manifest per model");
}

#[test]
fn confidence_threshold_is_part_of_the_atomic_projection_identity() {
    let (_dir, ledger) = setup();
    let mut cfg = Config::default();
    let provider = FixtureAnnotator::new();
    let first =
        extract_language_claims(&ledger, PROJ, &cfg, &provider, "fixture:1b").expect("first");
    score_claims(
        &ledger,
        PROJ,
        &cfg,
        &mut NeverContradicts,
        "projection-head-stub",
        first,
    )
    .expect("score first projection");
    assert_eq!(
        ledger.active_entailment_scorer(PROJ).expect("head"),
        Some("projection-head-stub".to_string())
    );
    assert_eq!(provider.calls(), 4);

    cfg.language.confidence_threshold = 0.99;
    let replay = FixtureAnnotator::new();
    let report = extract_language_claims(&ledger, PROJ, &cfg, &replay, "fixture:1b")
        .expect("threshold projection");
    assert_eq!(replay.calls(), 0, "model-visible inputs remain frozen");
    assert_eq!(report.claims_written, 0, "0.97 is below the new threshold");

    let claims: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM claims", [], |row| row.get(0))
        .expect("claims");
    assert_eq!(claims, 0, "the old accepted projection must not survive");
    assert_eq!(
        ledger.active_entailment_scorer(PROJ).expect("cleared head"),
        None,
        "a changed claim projection cannot retain the prior scorer's authority"
    );
    let graph = derive_project_with(&ledger, PROJ, &cfg).expect("derive");
    assert!(
        graph.nodes.is_empty(),
        "a complete zero-claim language projection must suppress heuristic nodes"
    );
}

#[test]
fn deterministic_fallback_resets_the_language_projection_head() {
    let (_dir, ledger) = setup();
    let cfg = Config::default();
    extract_heuristic_claims(&ledger, PROJ).expect("initial tier-zero extraction");
    let clean_tier_zero =
        fingerprint(&derive_project_with(&ledger, PROJ, &cfg).expect("tier zero"));

    let language =
        extract_language_claims(&ledger, PROJ, &cfg, &FixtureAnnotator::new(), "fixture:1b")
            .expect("language extraction");
    score_claims(
        &ledger,
        PROJ,
        &cfg,
        &mut NeverContradicts,
        "fallback-head-stub",
        language,
    )
    .expect("score language projection");
    assert!(ledger
        .active_entailment_scorer(PROJ)
        .expect("language head")
        .is_some());
    let assisted = fingerprint(&derive_project_with(&ledger, PROJ, &cfg).expect("assisted"));
    assert_ne!(
        clean_tier_zero, assisted,
        "the fixture must exercise demotion"
    );

    extract_heuristic_claims(&ledger, PROJ).expect("fallback extraction");
    let fallback = fingerprint(&derive_project_with(&ledger, PROJ, &cfg).expect("fallback"));
    assert_eq!(
        clean_tier_zero, fallback,
        "an old classification manifest must not remain authoritative after fallback"
    );
    let head: (String, Option<String>) = ledger
        .conn()
        .query_row(
            "SELECT extractor, manifest_annotation_id FROM claim_projection_heads
             WHERE project_id = ?1",
            [PROJ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("projection head");
    assert_eq!(head.0, "marker-v2");
    assert_eq!(head.1, None);
    assert_eq!(
        ledger
            .active_entailment_scorer(PROJ)
            .expect("fallback cleared entailment head"),
        None
    );
}

#[test]
fn language_classification_cannot_change_numeric_verdict_authority() {
    for (result, expected) in [
        (
            "running 12 tests\ntest result: ok. 12 passed; 0 failed",
            NodeState::Supported,
        ),
        (
            "running 12 tests\ntest result: FAILED. 10 passed; 2 failed",
            NodeState::Contradicted,
        ),
    ] {
        let (_dir, ledger) = setup_contract(result);
        let cfg = Config::default();
        let report =
            extract_language_claims(&ledger, PROJ, &cfg, &FixtureAnnotator::new(), "fixture:1b")
                .expect("language extraction");
        assert_eq!(
            report.claims_written, 1,
            "language agreement admits the claim"
        );
        score_claims(
            &ledger,
            PROJ,
            &cfg,
            &mut NeverContradicts,
            "never-contradicts",
            report,
        )
        .expect("score");

        let graph = derive_project_with(&ledger, PROJ, &cfg).expect("derive");
        let verdict = graph
            .nodes
            .iter()
            .flat_map(|node| &node.verdicts)
            .find(|verdict| verdict.text == "All 12 parser tests pass.")
            .expect("numeric verdict");
        assert_eq!(verdict.state, expected, "{}", verdict.reason);
        assert!(
            verdict
                .reason
                .contains("Checked against the latest identity-matched prior run"),
            "verdict did not come from the typed contract: {}",
            verdict.reason
        );
    }
}

#[test]
fn manifest_and_claim_projection_corruption_are_rejected() {
    let (_dir, ledger) = setup();
    let cfg = Config::default();
    extract_language_claims(&ledger, PROJ, &cfg, &FixtureAnnotator::new(), "fixture:1b")
        .expect("language extraction");

    ledger
        .conn()
        .execute(
            "UPDATE claims SET kind = 'finding' WHERE kind = 'marker'",
            [],
        )
        .expect("tamper rebuildable projection");
    let err = derive_project_with(&ledger, PROJ, &cfg).expect_err("claim digest mismatch");
    assert!(err.to_string().contains("does not match manifest"), "{err}");
}

#[test]
fn a_projection_head_cannot_point_at_another_annotation_layer() {
    let (_dir, ledger) = setup();
    let cfg = Config::default();
    extract_language_claims(&ledger, PROJ, &cfg, &FixtureAnnotator::new(), "fixture:1b")
        .expect("language extraction");
    let event_id: String = ledger
        .conn()
        .query_row(
            "SELECT id FROM events WHERE project_id = ?1 ORDER BY seq LIMIT 1",
            [PROJ],
            |row| row.get(0),
        )
        .expect("event");
    ledger
        .conn()
        .execute(
            "INSERT INTO annotations(id,event_id,layer,model,prompt_sha256,output_json,
                 output_sha256,confidence,created_at)
             VALUES('wrong-layer',?1,'entailment','stub','p','{}','o',0.5,
                    '2026-07-31T00:10:00Z')",
            [&event_id],
        )
        .expect("wrong-layer row");
    ledger
        .conn()
        .execute(
            "UPDATE claim_projection_heads SET manifest_annotation_id = 'wrong-layer'
             WHERE project_id = ?1",
            [PROJ],
        )
        .expect("retarget head");
    let err = derive_project_with(&ledger, PROJ, &cfg).expect_err("wrong layer");
    assert!(err.to_string().contains("non-classification"), "{err}");
}

#[test]
fn manifest_output_hash_is_verified_when_derivation_consumes_it() {
    let (_dir, ledger) = setup();
    let cfg = Config::default();
    extract_language_claims(&ledger, PROJ, &cfg, &FixtureAnnotator::new(), "fixture:1b")
        .expect("language extraction");
    let manifest_id: String = ledger
        .conn()
        .query_row(
            "SELECT manifest_annotation_id FROM claim_projection_heads WHERE project_id = ?1",
            [PROJ],
            |row| row.get(0),
        )
        .expect("manifest id");

    // Simulate physical/external corruption by removing the application guard
    // in this disposable database. Production code cannot perform this write.
    ledger
        .conn()
        .execute("DROP TRIGGER classification_annotations_no_update", [])
        .expect("drop guard in corruption fixture");
    ledger
        .conn()
        .execute(
            "UPDATE annotations SET output_json = '{}' WHERE id = ?1",
            [&manifest_id],
        )
        .expect("corrupt manifest bytes");
    let err = derive_project_with(&ledger, PROJ, &cfg).expect_err("hash mismatch");
    assert!(err.to_string().contains("output hash mismatch"), "{err}");
}

/// A poll that returns after a short real sleep, so a deadline test spends
/// wall-clock time without busy-spinning through millions of iterations.
struct SlowPoll {
    calls: AtomicUsize,
}

impl SlowPoll {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl MemoryPressurePoll for SlowPoll {
    fn wait_before_retry(&self) -> std::result::Result<(), LanguageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(20));
        Ok(())
    }
}

/// **The wait has a deadline, and pressure that never clears is not a hang.**
///
/// The loop was `while Paused { poll; resample }` with no bound. Measured
/// consequence: a full workspace test run blocked here for 13.5 minutes on 7.7
/// seconds of CPU while a second process built the same workspace — an ordinary
/// CI condition, where it presents as a hung runner rather than a failed one.
///
/// This drives pressure that never clears and asserts the run **finishes**,
/// that it finishes as a degraded classification rather than a guess, and that
/// the degradation is counted.
#[test]
fn memory_pressure_that_never_clears_ends_the_wait_instead_of_hanging() {
    let (_dir, ledger) = setup_single("All 12 parser tests pass.");
    let mut cfg = Config::default();
    cfg.language.memory_pause_timeout_secs = 1;
    let provider = FixtureAnnotator::new();
    // Paused forever: the fallback is what every sample after the script
    // returns, and the script is empty.
    let pressure = ScriptedPressure::new([], MemoryPressureState::Paused);
    let poll = SlowPoll::new();

    let started = std::time::Instant::now();
    let report = extract_language_claims_with_memory(
        &ledger,
        PROJ,
        &cfg,
        &provider,
        "fixture:pressure-never-clears",
        &pressure,
        &poll,
    )
    .expect("sustained memory pressure degrades the run; it does not fail it");
    let elapsed = started.elapsed();

    // It ended, and it ended near the deadline rather than at some unrelated
    // moment — the bound is doing the work.
    assert!(
        elapsed >= Duration::from_secs(1) && elapsed < Duration::from_secs(20),
        "finished in {elapsed:?}; the deadline is 1s"
    );
    assert!(poll.calls() > 1, "the loop never retried: {}", poll.calls());

    // The pause was recorded, and so was giving up on it. A degraded run
    // nobody can see is the failure this project refuses.
    assert_eq!(report.classification_memory_pauses, 1);
    assert!(
        report.classification_memory_timeouts >= 1,
        "the timeout was not counted: {report:?}"
    );

    // **H5 — the reason's CONTENT, not just its presence.** Replacing the
    // string with `"x"` was green: a stored reason nobody reads is a reason
    // that stops being one. It has to name what happened and what to change.
    let stored = stored_classification_reasons(&ledger);
    assert!(!stored.is_empty(), "no classification row was written");
    let reason = stored
        .iter()
        .find(|r| r.contains("memory"))
        .unwrap_or_else(|| panic!("no reason mentions memory: {stored:?}"));
    assert!(
        reason.contains("gave up"),
        "the reason does not say the wait ended: {reason}"
    );
    assert!(
        reason.contains(&cfg.language.resume_above_available_mb.to_string()),
        "the reason does not name the threshold that was never reached: {reason}"
    );
    assert!(
        reason.contains('s') && reason.chars().any(|c| c.is_ascii_digit()),
        "the reason does not say how long it waited: {reason}"
    );

    // **And the model never ran.** The candidate is uncertain because nobody
    // classified it, which is the existing slot for that, not a guessed
    // category.
    assert_eq!(
        provider.calls(),
        0,
        "the classifier ran despite never getting memory"
    );
    assert_eq!(
        report.classification_uncertain,
        report.classification_candidates
    );
}

/// The deadline cannot be switched off by setting it to zero — that is the same
/// unbounded wait wearing a bound's name.
#[test]
fn a_zero_memory_pause_deadline_is_refused_by_config_validation() {
    let mut cfg = Config::default();
    cfg.language.memory_pause_timeout_secs = 0;
    let err = cfg
        .validate()
        .expect_err("a zero deadline must be refused")
        .to_string();
    assert!(err.contains("memory_pause_timeout_secs"), "{err}");
    assert!(err.contains("unbounded wait"), "{err}");
}

/// Every `reason` string in the stored classification rows.
fn stored_classification_reasons(ledger: &Ledger) -> Vec<String> {
    let conn = ledger.conn();
    let mut stmt = conn
        .prepare("SELECT output_json FROM annotations")
        .expect("prepare");
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .collect::<std::result::Result<Vec<String>, rusqlite::Error>>()
        .expect("rows");
    let mut out = Vec::new();
    for raw in rows {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let Some(results) = v.get("results").and_then(|r| r.as_array()) else {
            continue;
        };
        for r in results {
            if let Some(reason) = r.get("reason").and_then(|x| x.as_str()) {
                out.push(reason.to_string());
            }
        }
    }
    out
}
