//! §5g — the classifier is constitutionally blind to source and prose.
//!
//! Holdout-2's three failures (A46) re-expressed as permanent dev regressions,
//! plus the property they were symptoms of. All three were one bug — use/mention
//! confusion — and the fix has two layers, tested separately here because they
//! fail differently:
//!
//! 1. **Provenance** (structural): a non-executed span returns `Unknown` before
//!    any character is read. No wording can defeat this and no wording is
//!    needed to trigger it.
//! 2. **Modality and source guards** (within executed output): future,
//!    conditional and mention markers abstain, and shell source quoted inside a
//!    result is source rather than output.
//!
//! Layer 2 exists because layer 1 cannot cover everything: `cat harness.sh`
//! genuinely produces executed output whose text contains `printf 'FINAL: %d
//! pass'`. Provenance is honest about that span; the guard is what declines it.

use thruline_core::premise::{classify_span, Outcome, SpanKind};

fn abstains(kind: SpanKind, text: &str) -> bool {
    matches!(classify_span(kind, text), Outcome::Unknown)
}

/* ── layer 1: provenance, by construction ────────────────────────────── */

#[test]
fn command_source_is_never_outcome_evidence() {
    // A46 trap 3's family. Every one of these is a real grammar mined from the
    // corpus (308 command-source spans carry count shapes), and every one is a
    // plan to print rather than a printed result.
    for src in [
        "printf 'FINAL: %d pass, %d fail\\n' \"$ok\" \"$bad\"",
        "echo '# pass 31' >> \"$report\"",
        "cat <<'EOF' > summary.txt\n=== final: 31 pass, 0 fail ===\nEOF",
        "npm test 2>&1 | grep -E '^# (pass|fail) [0-9]+' | tail -4",
        // The unambiguous case: a genuine summary sitting in tool_input.
        // Provenance must decline it even though the text is a perfect result.
        "test result: ok. 47 passed; 0 failed; 0 ignored",
    ] {
        assert!(
            abstains(SpanKind::CommandSource, src),
            "command source was read as a result: {src:?}"
        );
    }
}

#[test]
fn prose_is_never_outcome_evidence() {
    // A46 traps 1 and 2's family.
    for prose in [
        "Next I'll re-run the gate; it should show 32 pass, 0 fail once the clamp lands.",
        "An earlier report mentioned the counts (44 pass, 0 fail); no run is attached.",
        "4. **Verify** — `swift test` (61/61), then tag and push.",
        // Again the unambiguous case: perfect result text, wrong provenance.
        "# tests 26\n# pass 26\n# fail 0",
    ] {
        assert!(
            abstains(SpanKind::ProseContext, prose),
            "prose was read as a result: {prose:?}"
        );
    }
}

#[test]
fn provenance_decides_before_wording_does() {
    // The property, stated so a future refactor cannot satisfy the tests above
    // by getting clever about strings: the SAME text classifies from executed
    // output and abstains from the other two spans. If this ever fails, the
    // rule has been reimplemented as a heuristic.
    let real = "test result: ok. 47 passed; 0 failed; 0 ignored";
    assert!(
        !abstains(SpanKind::ExecutedOutput, real),
        "the classifier stopped reading real executed output"
    );
    assert!(abstains(SpanKind::CommandSource, real));
    assert!(abstains(SpanKind::ProseContext, real));
}

/* ── layer 2: guards within executed output ──────────────────────────── */

#[test]
fn holdout2_trap1_a_future_plan_is_not_an_observed_run() {
    // "Next … should show 32 pass" — the counts are real characters and the
    // run they describe has not happened.
    assert!(abstains(
        SpanKind::ExecutedOutput,
        "usage: gate.sh [--full]\n  a full run should show 32 pass, 0 fail"
    ));
    assert!(abstains(
        SpanKind::ExecutedOutput,
        "Plan: will execute 47 tests across 6 suites."
    ));
    assert!(abstains(
        SpanKind::ExecutedOutput,
        "expected 19 passed; run the suite to confirm"
    ));
}

#[test]
fn holdout2_trap2_a_mention_is_not_a_result() {
    // Use versus mention. The sentence is true; it is evidence that a report
    // existed, not evidence of a run.
    assert!(abstains(
        SpanKind::ExecutedOutput,
        "the previous log mentioned 58 pass and 0 fail; nothing was executed here"
    ));
    assert!(abstains(
        SpanKind::ExecutedOutput,
        "As reported in RELEASE.md, the gate stood at 108 pass / 0 fail."
    ));
}

#[test]
fn holdout2_trap3_shell_source_in_a_result_is_still_source() {
    // `cat harness.sh` — provenance says executed output, and it is telling the
    // truth. The format specifier is the structural tell: a real runner prints
    // values, never `%d`.
    assert!(abstains(
        SpanKind::ExecutedOutput,
        "  ok=$((ok+1))\n  printf 'FINAL: %d pass, %d fail\\n' \"$ok\" \"$bad\"\n  exit $rc"
    ));
    assert!(abstains(
        SpanKind::ExecutedOutput,
        "good() { printf 'PASS: %s\\n' \"$1\"; }\nbad()  { printf 'FAIL: %s\\n' \"$1\"; fails=$((fails+1)); }"
    ));
}

/* ── the counterweight ───────────────────────────────────────────────── */

#[test]
fn the_guards_do_not_silence_real_runs() {
    // Precision work that also blinded the classifier would be worthless. Each
    // of these must still classify, and with the right numbers — §5e ranks a
    // wrong count below an abstention, but an abstention on everything is a
    // product that has stopped working.
    let cases: [(&str, Option<u32>, Option<u32>); 6] = [
        ("test result: ok. 47 passed; 0 failed; 0 ignored", Some(47), Some(0)),
        ("# tests 26\n# pass 26\n# fail 0", Some(26), Some(0)),
        (
            "✔ Test run with 61 tests in 7 suites passed after 4.802 seconds.",
            Some(61),
            Some(0),
        ),
        ("=== final: 58 pass, 0 fail ===", Some(58), Some(0)),
        (
            "PASS: index opens\nPASS: index rebuilds\nFAIL: eviction leaves a page\nPASS: replay is idempotent",
            Some(3),
            Some(1),
        ),
        // A real summary that happens to sit beside a speculative sentence.
        // The guard is per line, so the summary survives its neighbour.
        (
            "note: a rerun should be faster\ntest result: ok. 12 passed; 0 failed",
            Some(12),
            Some(0),
        ),
    ];
    for (text, want_p, want_f) in cases {
        match classify_span(SpanKind::ExecutedOutput, text) {
            Outcome::Tests { passed, failed } => {
                assert_eq!(passed, want_p, "wrong pass count for {text:?}");
                assert_eq!(failed, want_f, "wrong fail count for {text:?}");
            }
            other => panic!("real run went unread as {other:?}: {text:?}"),
        }
    }
}
