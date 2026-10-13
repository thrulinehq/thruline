//! Ruby findings cannot reach `fail`, and the cap tracks the measurement.
//!
//! # The ruling
//!
//! Operator, received 2026-09-06 01:10 IST, on Ruby's measured ambiguity of
//! 0.1275% against the
//! 0.1% budget:
//!
//! > *"ship Ruby, but every Ruby finding is capped at warn severity — never
//! > fail — until ambiguity is under 0.1%, with the cap and the measurement
//! > recorded beside it. No per-language budget. A detector over budget may
//! > inform; it may not accuse."*
//!
//! # Why this is a mutation test and not a spot check
//!
//! The merge plan's rule: before citing a green as evidence, show the
//! harness can reach the change. A test that asserts "these three Ruby fixtures
//! come out `warn`" proves the cap holds for three fixtures. It does not prove
//! the cap holds — a new detector added tomorrow at `fail` severity would pass
//! it, because the test never looked at that detector.
//!
//! So the cap is checked **by construction**: every detector this crate can
//! emit is driven, for every language, and the cap is asserted against
//! `LangPack::severity_cap` rather than against a list of expected values. A
//! detector added without thought about the cap fails here.

use thruline_langpacks::measured::{measurement, AMBIGUITY_BUDGET_PCT};
use thruline_langpacks::pack::{PackLanguage, Severity};
use thruline_langpacks::{diff_test_file_with_symbols, pack_for};

/// Base/head pairs chosen to fire as many distinct detectors as possible.
///
/// Not a fixture directory: these are minimal in-line sources so the test
/// exercises the CAP, not the families. The families already prove the
/// detectors fire.
fn ruby_pairs() -> Vec<(&'static str, String, String)> {
    let base = "class T < Minitest::Test\n\
                \x20 def test_a\n\
                \x20   assert_equal 9.0, Shipping.rate(2)\n\
                \x20   assert_equal 12.0, Shipping.rate(3)\n\
                \x20 end\n\
                \x20 def test_b\n\
                \x20   assert_equal 29.0, Shipping.rate(10)\n\
                \x20 end\n\
                end\n";
    vec![
        (
            "assertion_removed",
            base.to_string(),
            "class T < Minitest::Test\n  def test_a\n    Shipping.rate(2)\n  end\n  def test_b\n    assert_equal 29.0, Shipping.rate(10)\n  end\nend\n".to_string(),
        ),
        (
            "assertion_weakened",
            base.to_string(),
            "class T < Minitest::Test\n  def test_a\n    assert Shipping.rate(2)\n    assert Shipping.rate(3)\n  end\n  def test_b\n    assert_equal 29.0, Shipping.rate(10)\n  end\nend\n".to_string(),
        ),
        (
            "test_deleted",
            base.to_string(),
            "class T < Minitest::Test\n  def test_a\n    assert_equal 9.0, Shipping.rate(2)\n    assert_equal 12.0, Shipping.rate(3)\n  end\nend\n".to_string(),
        ),
        (
            "assertion_swallowed",
            base.to_string(),
            "class T < Minitest::Test\n  def test_a\n    begin\n      assert_equal 9.0, Shipping.rate(2)\n      assert_equal 12.0, Shipping.rate(3)\n    rescue StandardError\n    end\n  end\n  def test_b\n    assert_equal 29.0, Shipping.rate(10)\n  end\nend\n".to_string(),
        ),
        (
            "skip_marker_added",
            base.to_string(),
            "class T < Minitest::Test\n  def test_a\n    skip \"flaky\"\n    assert_equal 9.0, Shipping.rate(2)\n    assert_equal 12.0, Shipping.rate(3)\n  end\n  def test_b\n    assert_equal 29.0, Shipping.rate(10)\n  end\nend\n".to_string(),
        ),
    ]
}

/// **The cap, by construction.** Not a list of expected severities.
#[test]
fn a_ruby_finding_cannot_reach_fail() {
    let pack = pack_for(PackLanguage::Ruby);
    let cap = pack.severity_cap().expect(
        "Ruby is capped; if the cap was lifted, MEASUREMENTS.toml must show it under budget",
    );
    assert_eq!(cap, Severity::Warn, "the cap is `warn`, per the ruling");

    let mut fired = 0usize;
    for (name, base, head) in ruby_pairs() {
        let findings =
            diff_test_file_with_symbols("t_test.rb", &base, &head, pack, &["rate".to_string()])
                .expect("diff");
        assert!(
            !findings.is_empty(),
            "`{name}` produced no findings at all, so it proves nothing about the cap"
        );
        for f in &findings {
            fired += 1;
            assert!(
                f.severity() <= cap,
                "a Ruby finding reached {:?}, above the cap: {} — {}",
                f.severity(),
                f.detector,
                f.detail
            );
            assert_ne!(
                f.severity(),
                Severity::Fail,
                "a Ruby finding is `fail`: {} — a detector over budget may \
                 inform, it may not accuse",
                f.detector
            );
        }
    }
    assert!(
        fired >= 5,
        "only {fired} findings were produced across five detector-driving pairs; \
         the cap is being asserted against almost nothing"
    );
}

/// The other three languages are NOT capped, so the cap is a Ruby fact rather
/// than a change to what the engine can emit.
///
/// Without this, a bug that capped everything would pass the test above and
/// silently disarm `assertion_removed` for Java, C# and Go.
#[test]
fn the_cap_is_ruby_only_and_the_others_still_reach_fail() {
    for language in [PackLanguage::Java, PackLanguage::CSharp, PackLanguage::Go] {
        let pack = pack_for(language);
        assert!(
            pack.severity_cap().is_none(),
            "{:?} grew a severity cap. If that is deliberate it needs its own \
             measurement and its own line in MEASUREMENTS.toml, because a cap \
             is a statement that the language's classifier is not trustworthy \
             for `fail`.",
            language
        );
    }

    // And prove one of them really does still reach `fail`, so "uncapped" is
    // not merely an unset flag.
    let go = pack_for(PackLanguage::Go);
    let base = "func TestA(t *testing.T) {\n\tif got := Rate(2); got != 9.0 {\n\t\tt.Errorf(\"x\")\n\t}\n}\n";
    let head = "func TestA(t *testing.T) {\n\tRate(2)\n}\n";
    let findings = diff_test_file_with_symbols("a_test.go", base, head, go, &[]).expect("diff");
    assert!(
        findings.iter().any(|f| f.severity() == Severity::Fail),
        "no Go finding reached `fail`, so this test cannot tell a cap from an \
         engine that stopped emitting `fail` at all: {findings:?}"
    );
}

/// The cap and the measurement must not drift apart.
///
/// The cap exists *because* Ruby is over budget. If Ruby is measured under
/// budget and the cap is still set, the crate is under-reporting for no
/// recorded reason; if Ruby is over budget and the cap is gone, it is accusing
/// on a classifier the budget says is not trustworthy for that.
#[test]
fn the_cap_and_the_measurement_agree() {
    let ruby = measurement("ruby").expect("ruby is measured");
    let capped = pack_for(PackLanguage::Ruby).severity_cap().is_some();
    let over_budget = ruby.ambiguity_pct >= AMBIGUITY_BUDGET_PCT;
    assert_eq!(
        capped,
        over_budget,
        "Ruby is {} budget ({:.4}% against {AMBIGUITY_BUDGET_PCT}%) and the cap \
         is {}. The ruling ties them together: capped until ambiguity is under \
         budget, and not one moment longer.",
        if over_budget { "OVER" } else { "under" },
        ruby.ambiguity_pct,
        if capped { "set" } else { "not set" }
    );
}

// **Retired at core `18d94dc`.**
//
// `the_special_casing_escalation_reads_pre_cap_findings_and_must_keep_doing_so`
// stood here. It guarded a statement order in `crates/cli` — that
// `test_side_failed` was computed from findings as EMITTED, before
// `tests_pass::escalate` ran — because that ordering was the only thing keeping
// `special_casing::escalate_with_test_findings` from escalating §6.8 findings
// off a capped Ruby finding.
//
// The operator ruled the structural shape and set the retirement condition:
// *"Keep G's ordering test until the structural fix lands, then retire it with
// the fix, not before."* It landed at `18d94dc`: `Finding` carries its language
// and a private `cap`, `raised_to` is the only way a severity increases, and
// both raising sites go through the value. There is no order left to get wrong,
// so the guard has nothing to guard.
//
// Retired here, in the commit that adopts `Finding::from_pack` — which is what
// actually made the fix take effect for this crate. Not earlier.
