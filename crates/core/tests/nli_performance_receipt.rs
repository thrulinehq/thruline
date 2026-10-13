//! Reproducible, provisioned-model NLI performance receipt (P0-M3F).
//!
//! Ordinary tests in this target parse and validate only committed synthetic
//! input and receipt shapes. The production scorer is exercised solely by the
//! explicitly ignored `provisioned_model_performance_receipt` gate, through
//! `scripts/measure-provisioned-nli.sh`. That wrapper requires a checkpointed
//! source tree, an external Cargo target, and the exact operator-provisioned
//! default A18 receipt. Neither layer downloads, starts, deletes, or contacts a
//! model service.

use std::fs;
#[cfg(feature = "ort")]
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thruline_core::model_supply::{
    verify_model_dir, ModelDirectoryState, A18_MODEL_RECEIPT_DIGEST,
};

const CORPUS_SCHEMA: &str = "thruline-nli-performance-corpus-v1";
const CORPUS_PROFILE: &str = "synthetic-production-shape-v1";
const RECEIPT_SCHEMA: &str = "thruline-nli-performance-receipt-v1";
const RSS_CEILING_BYTES: u64 = 1_073_741_824;
const CORPUS_BYTES: &[u8] = include_bytes!("fixtures/nli-performance-corpus-v1.json");
const CORPUS_SHA256: &str = "6408b5d3607fbd268d416f863dd5d7f05cf5098fdb74c980646a8f60ac8be30a";
const WRAPPER: &str = include_str!("../../../scripts/measure-provisioned-nli.sh");
const HARNESS_SOURCE: &str = include_str!("nli_performance_receipt.rs");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SyntheticCorpus {
    schema: String,
    profile: String,
    repetitions: u32,
    pairs: Vec<SyntheticPair>,
    stress_pair: StressPair,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SyntheticPair {
    premise: String,
    hypothesis: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StressPair {
    premise_prefix: String,
    repeated_token: String,
    repeat_count: u32,
    hypothesis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MachineBasis {
    os: String,
    arch: String,
    cpu_brand: String,
    kernel_version: String,
    logical_cpus: u32,
    total_memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NliPerformanceReceipt {
    schema: String,
    model_content_digest: String,
    code_digest: String,
    machine: MachineBasis,
    profile: String,
    corpus_digest: String,
    scored_pairs: u64,
    throughput_pairs_per_second_milli: u64,
    peak_rss_bytes: u64,
    truncated_pairs: u64,
}

fn lowercase_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn safe_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && !value.chars().any(char::is_control)
        && ![
            "/Users/",
            "/home/",
            "C:\\Users\\",
            "BEGIN PRIVATE KEY",
            "BEGIN OPENSSH",
            "sk-",
        ]
        .iter()
        .any(|marker| value.contains(marker))
}

fn parse_corpus(bytes: &[u8]) -> Result<SyntheticCorpus, String> {
    let corpus: SyntheticCorpus =
        serde_json::from_slice(bytes).map_err(|error| format!("invalid corpus JSON: {error}"))?;
    if corpus.schema != CORPUS_SCHEMA {
        return Err("unexpected corpus schema".to_string());
    }
    if corpus.profile != CORPUS_PROFILE {
        return Err("unexpected corpus profile".to_string());
    }
    if !(1..=64).contains(&corpus.repetitions) {
        return Err("corpus repetitions must be within 1..=64".to_string());
    }
    if !(8..=64).contains(&corpus.pairs.len()) {
        return Err("corpus must contain 8..=64 ordinary pairs".to_string());
    }
    for pair in &corpus.pairs {
        if !safe_text(&pair.premise, 2_048) || !safe_text(&pair.hypothesis, 512) {
            return Err("corpus contains unsafe or unbounded pair text".to_string());
        }
    }
    if !safe_text(&corpus.stress_pair.premise_prefix, 512)
        || !safe_text(&corpus.stress_pair.repeated_token, 64)
        || !safe_text(&corpus.stress_pair.hypothesis, 512)
        || !(513..=2_048).contains(&corpus.stress_pair.repeat_count)
    {
        return Err("corpus stress pair is unsafe or outside its bounds".to_string());
    }
    let stress_bytes = corpus.stress_pair.premise_prefix.len().saturating_add(
        corpus
            .stress_pair
            .repeated_token
            .len()
            .saturating_mul(corpus.stress_pair.repeat_count as usize),
    );
    if stress_bytes > 65_536 {
        return Err("expanded stress premise exceeds 64 KiB".to_string());
    }
    let total = (corpus.pairs.len() + 1).saturating_mul(corpus.repetitions as usize);
    if total > 4_096 {
        return Err("expanded corpus exceeds 4096 pairs".to_string());
    }
    Ok(corpus)
}

fn expanded_pairs(corpus: &SyntheticCorpus) -> Vec<(String, String)> {
    let stress_premise = format!(
        "{}{}",
        corpus.stress_pair.premise_prefix,
        corpus
            .stress_pair
            .repeated_token
            .repeat(corpus.stress_pair.repeat_count as usize)
    );
    let mut one_pass: Vec<(String, String)> = corpus
        .pairs
        .iter()
        .map(|pair| (pair.premise.clone(), pair.hypothesis.clone()))
        .collect();
    one_pass.push((stress_premise, corpus.stress_pair.hypothesis.clone()));
    let mut expanded = Vec::with_capacity(one_pass.len() * corpus.repetitions as usize);
    for _ in 0..corpus.repetitions {
        expanded.extend(one_pass.iter().cloned());
    }
    expanded
}

fn validate_receipt(
    receipt: &NliPerformanceReceipt,
    expected_pairs: u64,
    expected_truncated_pairs: u64,
) -> Result<(), String> {
    if receipt.schema != RECEIPT_SCHEMA {
        return Err("unexpected receipt schema".to_string());
    }
    let expected_model = format!("sha256:{A18_MODEL_RECEIPT_DIGEST}");
    if receipt.model_content_digest != expected_model {
        return Err("model content digest does not name the closed A18 receipt".to_string());
    }
    let Some(code) = receipt.code_digest.strip_prefix("sha256:") else {
        return Err("code digest has no sha256 prefix".to_string());
    };
    if !lowercase_hex(code, 16) {
        return Err("code digest is not the canonical engine digest".to_string());
    }
    let Some(corpus) = receipt.corpus_digest.strip_prefix("sha256:") else {
        return Err("corpus digest has no sha256 prefix".to_string());
    };
    if corpus != CORPUS_SHA256 || !lowercase_hex(corpus, 64) {
        return Err("corpus digest does not name the committed synthetic input".to_string());
    }
    if receipt.profile != "release" {
        return Err("performance receipts require a release profile".to_string());
    }
    if !matches!(receipt.machine.os.as_str(), "macos" | "linux")
        || !matches!(receipt.machine.arch.as_str(), "aarch64" | "x86_64")
        || !safe_text(&receipt.machine.cpu_brand, 128)
        || !safe_text(&receipt.machine.kernel_version, 128)
        || !(1..=1_024).contains(&receipt.machine.logical_cpus)
        || receipt.machine.total_memory_bytes < 1_073_741_824
    {
        return Err("machine basis is unsupported or incomplete".to_string());
    }
    if receipt.scored_pairs != expected_pairs
        || receipt.throughput_pairs_per_second_milli == 0
        || receipt.peak_rss_bytes == 0
        || receipt.peak_rss_bytes > RSS_CEILING_BYTES
        || receipt.peak_rss_bytes > receipt.machine.total_memory_bytes
        || receipt.truncated_pairs != expected_truncated_pairs
    {
        return Err("aggregate measurement is incomplete or impossible".to_string());
    }
    Ok(())
}

#[cfg(feature = "ort")]
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn committed_synthetic_corpus_is_strict_bounded_and_content_addressed() {
    let corpus = parse_corpus(CORPUS_BYTES).expect("committed synthetic corpus");
    assert_eq!(sha256(CORPUS_BYTES), CORPUS_SHA256);
    let pairs = expanded_pairs(&corpus);
    assert_eq!(pairs.len(), 128);
    assert_eq!(
        pairs.first().expect("first pair").1,
        "All 128 tests passed."
    );
    assert!(pairs.iter().any(|(premise, _)| premise.len() > 10_000));

    let all_text = pairs
        .iter()
        .flat_map(|(premise, hypothesis)| [premise.as_str(), hypothesis.as_str()])
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "/Users/",
        "/home/",
        "C:\\Users\\",
        "BEGIN PRIVATE KEY",
        "sk-",
    ] {
        assert!(
            !all_text.contains(forbidden),
            "forbidden corpus marker {forbidden}"
        );
    }

    let mut value: serde_json::Value =
        serde_json::from_slice(CORPUS_BYTES).expect("fixture is JSON");
    value
        .as_object_mut()
        .expect("corpus object")
        .insert("unknown".to_string(), serde_json::json!(true));
    assert!(parse_corpus(&serde_json::to_vec(&value).expect("mutated JSON")).is_err());
}

#[test]
fn performance_receipt_schema_is_closed_and_basis_fields_fail_loud() {
    let corpus = parse_corpus(CORPUS_BYTES).expect("committed synthetic corpus");
    let pairs = expanded_pairs(&corpus).len() as u64;
    let receipt = NliPerformanceReceipt {
        schema: RECEIPT_SCHEMA.to_string(),
        model_content_digest: format!("sha256:{A18_MODEL_RECEIPT_DIGEST}"),
        code_digest: "sha256:0123456789abcdef".to_string(),
        machine: MachineBasis {
            os: "macos".to_string(),
            arch: "aarch64".to_string(),
            cpu_brand: "Apple M4 Pro".to_string(),
            kernel_version: "Darwin 25.1.0".to_string(),
            logical_cpus: 10,
            total_memory_bytes: 17_179_869_184,
        },
        profile: "release".to_string(),
        corpus_digest: format!("sha256:{CORPUS_SHA256}"),
        scored_pairs: pairs,
        throughput_pairs_per_second_milli: 55_125,
        peak_rss_bytes: 536_870_912,
        truncated_pairs: 8,
    };
    validate_receipt(&receipt, pairs, 8).expect("valid receipt basis");

    let encoded = serde_json::to_vec(&receipt).expect("serialize receipt");
    let decoded: NliPerformanceReceipt =
        serde_json::from_slice(&encoded).expect("round-trip receipt");
    assert_eq!(decoded, receipt);
    let encoded_text = String::from_utf8(encoded).expect("receipt is UTF-8");
    assert!(!encoded_text.contains("All 128 tests passed"));
    assert!(!encoded_text.contains("/Users/"));

    let mut unknown = serde_json::to_value(&receipt).expect("receipt value");
    unknown
        .as_object_mut()
        .expect("receipt object")
        .insert("elapsed_seconds".to_string(), serde_json::json!(2.0));
    assert!(serde_json::from_value::<NliPerformanceReceipt>(unknown).is_err());

    for invalid in [
        NliPerformanceReceipt {
            code_digest: "sha256:ABCDEF0123456789".to_string(),
            ..receipt.clone()
        },
        NliPerformanceReceipt {
            profile: "debug".to_string(),
            ..receipt.clone()
        },
        NliPerformanceReceipt {
            corpus_digest: format!("sha256:{}", "0".repeat(64)),
            ..receipt.clone()
        },
        NliPerformanceReceipt {
            scored_pairs: pairs - 1,
            ..receipt.clone()
        },
        NliPerformanceReceipt {
            truncated_pairs: pairs + 1,
            ..receipt.clone()
        },
        NliPerformanceReceipt {
            peak_rss_bytes: RSS_CEILING_BYTES + 1,
            ..receipt.clone()
        },
    ] {
        assert!(validate_receipt(&invalid, pairs, 8).is_err());
    }
    let at_rss_ceiling = NliPerformanceReceipt {
        peak_rss_bytes: RSS_CEILING_BYTES,
        ..receipt
    };
    validate_receipt(&at_rss_ceiling, pairs, 8).expect("exact 1 GiB RSS ceiling is accepted");
}

#[test]
fn missing_model_receipt_fails_closed_without_home_or_filesystem_mutation() {
    let temp = tempfile::tempdir().expect("temporary root");
    let absent = temp.path().join("not-provisioned");
    let before = fs::read_dir(temp.path()).expect("empty temp root").count();
    let status = verify_model_dir(&absent).expect("missing receipt is a status");
    let after = fs::read_dir(temp.path())
        .expect("unchanged temp root")
        .count();

    assert_eq!(status.state, ModelDirectoryState::Missing);
    assert!(!status.ready);
    assert_eq!(status.unexpected_entries, 0);
    assert_eq!(before, after);
    assert!(!absent.exists());
}

#[test]
fn operator_wrapper_is_external_target_zero_network_and_evidence_only() {
    for required in [
        "set -euo pipefail",
        "CARGO_TARGET_DIR",
        "must be outside the repository",
        "CARGO_NET_OFFLINE=true",
        "ORT_OFFLINE=1",
        "rust-toolchain.toml",
        "crates/cli/build.rs",
        "find -L \"$REPO_ROOT/crates/core/src\" \"$REPO_ROOT/crates/cli/src\"",
        "-type f -name '*.rs' -print0",
        "while IFS= read -r -d '' source_path",
        "relative_path=${source_path#\"$REPO_ROOT/\"}",
        "ls-files --error-unmatch -- \"$relative_path\"",
        "model status --dir \"$MODEL_DIR\"",
        ".codex/evidence/nli-performance-receipt-v1.json",
        "THRULINE_NLI_PERF_LIVE=1",
        "provisioned_model_performance_receipt",
        "--ignored --exact",
    ] {
        assert!(
            WRAPPER.contains(required),
            "missing wrapper contract: {required}"
        );
    }
    for forbidden in [
        "curl ",
        "wget ",
        "ollama ",
        "model import",
        "cargo install",
        "rm -",
    ] {
        assert!(
            !WRAPPER.contains(forbidden),
            "wrapper contains forbidden action: {forbidden}"
        );
    }

    let sampled = HARNESS_SOURCE
        .rfind("sampler_peak.fetch_max")
        .expect("RSS sample update");
    let signaled = HARNESS_SOURCE
        .rfind(".send(())")
        .expect("sampler readiness signal");
    let waited = HARNESS_SOURCE
        .rfind("ready_rx")
        .expect("main readiness wait");
    let model_open = HARNESS_SOURCE
        .rfind("OrtNli::open(&model_dir)")
        .expect("production model open");
    assert!(
        sampled < signaled && signaled < waited && waited < model_open,
        "the first successful RSS sample must precede model loading"
    );
}

#[cfg(feature = "ort")]
#[test]
#[ignore = "operator-only: exact provisioned A18 model, release build, and external target required"]
fn provisioned_model_performance_receipt() {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::sync_channel;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use sysinfo::{get_current_pid, ProcessRefreshKind, ProcessesToUpdate, System};
    use thruline_core::model_supply::A18_MODEL_RECEIPT_ID;
    use thruline_core::nli::{EntailmentScorer, OrtNli};

    assert_eq!(
        std::env::var("THRULINE_NLI_PERF_LIVE").as_deref(),
        Ok("1"),
        "operator-only measurement refused: use scripts/measure-provisioned-nli.sh"
    );
    assert_eq!(
        thruline_core::perf_basis::build_profile(),
        "release",
        "operator-only measurement requires a release build"
    );

    let repo = workspace_root().canonicalize().expect("workspace root");
    let target = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR")
            .expect("operator-only measurement requires CARGO_TARGET_DIR"),
    )
    .canonicalize()
    .expect("external CARGO_TARGET_DIR must exist");
    assert!(
        !target.starts_with(&repo),
        "operator-only measurement requires an external CARGO_TARGET_DIR"
    );
    let output = PathBuf::from(
        std::env::var_os("THRULINE_NLI_PERF_OUTPUT")
            .expect("operator-only measurement requires the fixed evidence output"),
    );
    let expected_output = repo.join(".codex/evidence/nli-performance-receipt-v1.json");
    assert_eq!(
        output, expected_output,
        "measurement output must use the ignored evidence path"
    );
    assert!(
        !output.exists(),
        "measurement refuses to overwrite an existing receipt"
    );

    let model_dir = OrtNli::default_dir().expect("default model directory");
    let status = verify_model_dir(&model_dir).expect("verify default model receipt");
    assert!(status.ready, "exact default A18 model receipt is not ready");

    let corpus = parse_corpus(CORPUS_BYTES).expect("committed synthetic corpus");
    let pairs = expanded_pairs(&corpus);
    let stop = Arc::new(AtomicBool::new(false));
    let peak = Arc::new(AtomicU64::new(0));
    let (ready_tx, ready_rx) = sync_channel(0);
    let sampler_stop = Arc::clone(&stop);
    let sampler_peak = Arc::clone(&peak);
    let sampler = thread::spawn(move || -> Result<(), String> {
        let mut ready_tx = Some(ready_tx);
        let pid = get_current_pid().map_err(str::to_string)?;
        let pids = [pid];
        let mut system = System::new();
        loop {
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&pids),
                false,
                ProcessRefreshKind::nothing().with_memory(),
            );
            let rss = system
                .process(pid)
                .ok_or_else(|| "measurement process disappeared".to_string())?
                .memory();
            sampler_peak.fetch_max(rss, Ordering::Relaxed);
            if let Some(ready_tx) = ready_tx.take() {
                ready_tx
                    .send(())
                    .map_err(|_| "measurement readiness receiver disappeared".to_string())?;
            }
            if sampler_stop.load(Ordering::Acquire) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(2));
        }
    });

    ready_rx
        .recv()
        .expect("RSS sampler must succeed before model loading");
    let mut nli = OrtNli::open(&model_dir).expect("load production scorer from verified bytes");
    assert_eq!(nli.model_tag(), A18_MODEL_RECEIPT_ID);
    let started = Instant::now();
    for (premise, hypothesis) in &pairs {
        nli.score(premise, hypothesis)
            .expect("score committed synthetic pair");
    }
    let elapsed = started.elapsed();
    stop.store(true, Ordering::Release);
    sampler
        .join()
        .expect("RSS sampler thread")
        .expect("RSS samples");

    let mut code_entries =
        thruline_core::perf_basis::engine_sources(&repo).expect("read complete engine sources");
    let code_digest = thruline_core::perf_basis::code_digest(&mut code_entries);
    let elapsed_nanos = elapsed.as_nanos().max(1);
    let throughput_milli = u64::try_from(
        (nli.pairs_scored() as u128)
            .saturating_mul(1_000_000_000)
            .saturating_mul(1_000)
            / elapsed_nanos,
    )
    .expect("bounded throughput");
    let mut system = System::new_all();
    system.refresh_memory();
    let cpu_brand = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_string())
        .filter(|brand| !brand.is_empty())
        .expect("machine CPU brand");
    let receipt = NliPerformanceReceipt {
        schema: RECEIPT_SCHEMA.to_string(),
        model_content_digest: format!("sha256:{A18_MODEL_RECEIPT_DIGEST}"),
        code_digest,
        machine: MachineBasis {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_brand,
            kernel_version: System::kernel_long_version(),
            logical_cpus: u32::try_from(
                thread::available_parallelism()
                    .expect("logical CPU count")
                    .get(),
            )
            .expect("logical CPU count fits u32"),
            total_memory_bytes: system.total_memory(),
        },
        profile: "release".to_string(),
        corpus_digest: format!("sha256:{}", sha256(CORPUS_BYTES)),
        scored_pairs: nli.pairs_scored(),
        throughput_pairs_per_second_milli: throughput_milli,
        peak_rss_bytes: peak.load(Ordering::Relaxed),
        truncated_pairs: nli.pairs_truncated(),
    };
    validate_receipt(&receipt, pairs.len() as u64, u64::from(corpus.repetitions))
        .expect("complete performance receipt");

    fs::create_dir_all(output.parent().expect("evidence parent"))
        .expect("create ignored evidence directory");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .expect("create receipt without overwrite");
    let mut encoded = serde_json::to_vec_pretty(&receipt).expect("serialize aggregate receipt");
    encoded.push(b'\n');
    file.write_all(&encoded).expect("write aggregate receipt");
    file.sync_all().expect("sync aggregate receipt");
    println!(
        "NLI receipt: {} pairs · {:.3} pairs/s · {} peak RSS bytes · {} truncated",
        receipt.scored_pairs,
        receipt.throughput_pairs_per_second_milli as f64 / 1_000.0,
        receipt.peak_rss_bytes,
        receipt.truncated_pairs,
    );
}
