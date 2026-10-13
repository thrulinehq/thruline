//! `thruline bench` — BUILD-SPEC §12's harness and Overclaim Index.
//!
//! # This command's most useful output is still a refusal, for a new reason
//!
//! The first version of this file said no §12 source publishes labelled
//! trajectories, and printed that finding to the user. **It was wrong**, and
//! wrong in a way worth keeping visible: five sources were probed, all of them
//! happened to be harness-only, and the result was generalised into a claim
//! about the field. `sierra-research/tau2-bench` publishes
//! `data/tau2/results/final/` — real runs, each simulation carrying the
//! benchmark's own `reward_info`. Third-party trajectories, third-party oracle,
//! class 1 by §12's amendment, and downloadable today.
//!
//! So this command loads what is present and **measures** rather than asserting.
//! What the measurement says is that τ2-bench contains no executed code checks
//! bound to a git tree, which is the only evidence §6's verifiers read, so a
//! catch rate over it would be a number about detectors that never ran. That is
//! a finding this command computes from the corpus on disk every time it runs,
//! not a sentence someone has to remember to update.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use thruline_bench::tau2;

pub struct Args {
    /// Where downloaded corpora live.
    pub fixtures: Option<PathBuf>,
    pub html: bool,
    pub json: bool,
}

/// The §12 sources, and what a probe of each found.
///
/// Kept in code rather than in prose so `thruline bench` can print it, and so a
/// future corpus arriving means deleting a row rather than remembering to.
const SOURCES: &[(&str, &str)] = &[
    (
        "tau2-bench",
        "CLASS 1 and downloadable: data/tau2/results/final/ publishes 27 result \
         files carrying messages, tool_calls, reward and reward_info. Fetch with \
         fixtures/bench/download.sh",
    ),
    (
        "terminal-bench-1",
        "third-party harness, tasks and oracle (run-tests.sh); no published runs. \
         The CLASS 2 candidate, because its tasks execute commands",
    ),
    (
        "impossiblebench",
        "harness only — 33 files of agents, scorers and mutators; its README says \
         the logs come from running evaluations yourself",
    ),
    (
        "impossible_swebench / impossible_livecodebench",
        "reachable and unmodified TASK sets (problem_statement, test_patch, \
         FAIL_TO_PASS, impossible_type); no trajectories and no cheat labels",
    ),
    (
        "agentlens-bench",
        "HTTP 401 from the dataset API — gated or absent from this machine",
    ),
    (
        "evilgenie",
        "HTTP 404 — the repository named in §12 is not there",
    ),
    (
        "appworld",
        "reachable and ships zero trajectory files: harness only",
    ),
];

/// What loading a corpus found. Every field is measured from the bytes.
struct Loaded {
    file: String,
    /// Computed here, **not** checked against a fetch record. Printed so a
    /// reader can perform the check this command cannot.
    digest: String,
    trajectories: usize,
    labelled: usize,
    tool_calls: usize,
    code_check_executions: usize,
    near_misses: Vec<(String, usize)>,
}

fn load_tau2(dir: &std::path::Path) -> Vec<Result<Loaded, String>> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    for path in files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        // **Unpinned, and reported as such.** The class-1 evidence is the
        // digest recorded at fetch, and this command has no fetch record —
        // passing the file's own digest in would make the check an identity
        // wearing a verification's name. So the digest is computed, printed,
        // and left for the reader to compare against `download.sh`.
        match tau2::load_unpinned(&path) {
            Ok((corpus, digest)) => {
                let census = tau2::evidence_census(&corpus);
                out.push(Ok(Loaded {
                    file: name,
                    digest,
                    trajectories: corpus.trajectories.len(),
                    labelled: corpus
                        .trajectories
                        .iter()
                        .filter(|t| t.label.label().is_some())
                        .count(),
                    tool_calls: census.tool_calls,
                    code_check_executions: census.code_check_executions,
                    near_misses: census.near_misses.into_iter().collect(),
                }));
            }
            Err(e) => out.push(Err(format!("{name}: {e}"))),
        }
    }
    out
}

pub fn run(args: Args) -> Result<ExitCode> {
    let dir = args
        .fixtures
        .unwrap_or_else(|| PathBuf::from("fixtures/bench"));
    let present: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();

    if present.is_empty() {
        if args.json {
            crate::json_out::print(
                &crate::json_out::BENCH,
                serde_json::json!({
                    "ok": false,
                    "reason": "no labelled corpus is downloaded",
                    "fixtures_dir": dir,
                    "sources": SOURCES
                        .iter()
                        .map(|(n, f)| serde_json::json!({ "source": n, "finding": f }))
                        .collect::<Vec<_>>(),
                }),
            )?;
        } else {
            println!("No labelled corpus is present under {}.", dir.display());
            println!();
            println!("§12's sources, as probed:");
            for (name, finding) in SOURCES {
                println!("  {name}");
                println!("      {finding}");
            }
            println!();
            println!(
                "Nothing is generated here. A corpus written by the party being\n\
                 measured reports on the fixtures, not on the detector — so this\n\
                 command names what is missing instead of filling it in."
            );
        }
        // 3, the policy-failure code: a benchmark that cannot run has not
        // passed. Distinguished from 1, which would mean this command broke.
        return Ok(ExitCode::from(3));
    }

    // A corpus is present. Load it and measure whether it carries the evidence
    // §6's verifiers read, because that is what decides whether a number over
    // it would mean anything.
    let loaded = if present.iter().any(|p| p == "tau2") {
        load_tau2(&dir.join("tau2"))
    } else {
        Vec::new()
    };
    let ok: Vec<&Loaded> = loaded.iter().filter_map(|r| r.as_ref().ok()).collect();
    let executions: usize = ok.iter().map(|l| l.code_check_executions).sum();
    let trajectories: usize = ok.iter().map(|l| l.trajectories).sum();
    let tool_calls: usize = ok.iter().map(|l| l.tool_calls).sum();

    if args.json {
        crate::json_out::print(
            &crate::json_out::BENCH,
            serde_json::json!({
                "ok": false,
                "reason": if ok.is_empty() {
                    "a corpus directory is present and no loader read a file from it"
                } else {
                    "the corpus carries no executed code checks, which is the only \
                     evidence §6's verifiers read"
                },
                "fixtures_dir": dir,
                "present": present,
                "corpora": ok.iter().map(|l| serde_json::json!({
                    "file": l.file,
                    "sha256": l.digest,
                    "digest_pinned": false,
                    "trajectories": l.trajectories,
                    "labelled": l.labelled,
                    "tool_calls": l.tool_calls,
                    "code_check_executions": l.code_check_executions,
                    "near_misses": l.near_misses.iter()
                        .map(|(n, c)| serde_json::json!({ "tool": n, "count": c }))
                        .collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "errors": loaded.iter().filter_map(|r| r.as_ref().err()).collect::<Vec<_>>(),
            }),
        )?;
    } else {
        println!(
            "Found {} corpus directory(ies) under {}: {}",
            present.len(),
            dir.display(),
            present.join(", ")
        );
        println!();
        for r in &loaded {
            match r {
                Ok(l) => println!(
                    "  {}\n      sha256 {} (computed here, not checked against a fetch \
                     record)\n      {} trajectories, {} labelled by the oracle, {} tool \
                     calls, {} code-check executions",
                    l.file,
                    l.digest,
                    l.trajectories,
                    l.labelled,
                    l.tool_calls,
                    l.code_check_executions
                ),
                Err(e) => println!("  {e}"),
            }
        }
        if !ok.is_empty() {
            println!();
            println!(
                "{trajectories} trajectories and {tool_calls} tool calls carry \
                 {executions} executed code checks."
            );
            let near: Vec<String> = ok
                .iter()
                .flat_map(|l| l.near_misses.iter())
                .map(|(n, c)| format!("{n}×{c}"))
                .collect();
            if !near.is_empty() {
                println!(
                    "  near misses (execution-sounding names that are fixed domain APIs): {}",
                    near.join(", ")
                );
            }
            println!();
            println!(
                "Every §6 verifier — tests_pass, build_ok, lint_ok, typecheck_ok,\n\
                 behavior_unchanged — decides from an executed code check bound to a\n\
                 git tree. A catch rate over a corpus with none would be a number\n\
                 about detectors that never ran, so none is reported."
            );
        }
    }
    let _ = args.html;
    Ok(ExitCode::from(3))
}
