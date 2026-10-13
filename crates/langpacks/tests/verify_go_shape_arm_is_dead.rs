//! `verify::tamper`'s Go `if_statement` arm cannot fire, and what that costs.
//!
//! # The claim, stated so it can be refuted
//!
//! `crates/verify/src/tamper.rs` gives Go:
//!
//! ```text
//! assertion_kinds: &["call_expression", "if_statement"],
//! assertion_call_prefixes: &["t.error", "t.fatal", "assert.", "require.",
//!                            "t.errorf", "t.fatalf"],
//! ```
//!
//! and `collect_assertions` admits a node only when its text **starts with**
//! one of those prefixes (or with `assert`). An `if_statement`'s text starts
//! with `if`. So `if_statement` is in the kind list and can never be admitted:
//! **the arm is dead**.
//!
//! That is not cosmetic. The stdlib shape
//!
//! ```text
//! if got != want {
//!     t.Errorf("Add(1,2) = %d, want %d", got, want)
//! }
//! ```
//!
//! is the majority of real Go test code — the corpus in this crate's
//! `MEASUREMENTS.toml` bears that out. With the `if` arm dead, the comparison
//! `!=` is never seen; what reaches the classifier is the `t.Errorf` call, and
//! `strength_of` on `t.Errorf(…)` with its literals stripped finds no `==`, no
//! ` in `, no `approx` — so it returns `Truthy`, the weakest shape there is.
//!
//! # Why that matters more than "one shape is mis-labelled"
//!
//! `Strength` is ordered, and `diff_test_file` reports `assertion_weakened`
//! when the strongest shape **decreases**. A real Go equality already recorded
//! as `Truthy` is at the floor: nothing can weaken below it. So for Go,
//! `assertion_weakened` — the detector §6.7 escalates to `fail` when an
//! assertion goes vacuous — **cannot fire on a shape change at all**.
//!
//! This is exactly the gap `verify`'s own shape-classifier gate names in its
//! first paragraph: *"Go unmeasured, no corpus on this machine."* It was
//! unmeasured, and it was wrong.
//!
//! # This test is read-only about verify
//!
//! It asserts what verify does **today**, against real Go source. If someone
//! fixes verify, this test fails and says so in its own message — which is the
//! correct outcome: the fix and this file's removal are one change, recorded in
//! the merge plan.

use thruline_verify::tamper::{parse_tests, strength_of, Language, Strength};

/// The stdlib shape, as it appears throughout the Go corpus.
const STDLIB: &str = r#"
package math_test

import "testing"

func TestAdd(t *testing.T) {
	got := Add(1, 2)
	want := 3
	if got != want {
		t.Errorf("Add(1, 2) = %d, want %d", got, want)
	}
}
"#;

#[test]
fn verify_sees_a_real_stdlib_equality_as_the_weakest_possible_shape() {
    let fns = parse_tests(STDLIB, Language::Go).expect("parse");
    let add = fns
        .iter()
        .find(|f| f.name == "TestAdd")
        .expect("verify did not find TestAdd at all");

    assert!(
        !add.assertions.is_empty(),
        "verify found no assertions in a plain stdlib Go test — that is a \
         different and larger defect than the one this file documents"
    );

    let strongest = add.assertions.iter().copied().max().expect("non-empty");
    assert_eq!(
        strongest,
        Strength::Truthy,
        "verify now classifies the stdlib `if got != want {{ t.Errorf }}` shape as \
         {strongest:?} rather than Truthy. If the `if_statement` arm was fixed, \
         this file has served its purpose: delete it and strike the corresponding \
         item from MERGE-PLAN.md."
    );
}

/// The mechanism, isolated: the prefix list cannot admit an `if`.
#[test]
fn the_prefix_list_cannot_admit_an_if_statement() {
    // Reproduces `collect_assertions`' admission rule verbatim against verify's
    // own Go prefixes, so the claim rests on the rule and not on a guess about
    // tree shapes.
    const GO_PREFIXES: &[&str] = &[
        "t.error", "t.fatal", "assert.", "require.", "t.errorf", "t.fatalf",
    ];
    let if_text = "if got != want {\n\tt.Errorf(\"x\")\n}";
    let head = if_text.trim_start().to_ascii_lowercase();
    let admitted = GO_PREFIXES.iter().any(|p| head.starts_with(p)) || head.starts_with("assert");
    assert!(
        !admitted,
        "an `if_statement` is now admitted by verify's Go prefix list, so the \
         `if_statement` kind is live and this file is obsolete"
    );
}

/// The consequence: for Go, a shape change cannot produce `assertion_weakened`.
#[test]
fn a_go_equality_replaced_by_a_truthiness_check_is_not_seen_as_a_weakening() {
    // An unambiguous cheat: the exact-value comparison becomes "not zero".
    let cheat = r#"
package math_test

import "testing"

func TestAdd(t *testing.T) {
	got := Add(1, 2)
	if got == 0 {
		t.Errorf("Add(1, 2) = %d, want non-zero", got)
	}
}
"#;
    let base = parse_tests(STDLIB, Language::Go).expect("parse base");
    let head = parse_tests(cheat, Language::Go).expect("parse head");

    let was = base[0].assertions.iter().copied().max().expect("base");
    let now = head[0].assertions.iter().copied().max().expect("head");
    assert_eq!(
        (was, now),
        (Strength::Truthy, Strength::Truthy),
        "the two shapes now classify differently ({was:?} -> {now:?}), which means \
         verify's Go shape reading has been fixed"
    );
    assert!(
        now >= was,
        "a weakening is `now < was`; both being Truthy is why it cannot fire"
    );
}

/// And the same source read through this crate's pack does see it.
///
/// The point of the comparison: the defect is in the reading, not in the
/// grammar or in what Go source can express.
#[test]
fn this_crates_go_pack_reads_the_same_source_as_an_equality() {
    let pack = thruline_langpacks::pack_for(thruline_langpacks::PackLanguage::Go);
    let fns = thruline_langpacks::parse_tests(STDLIB, pack).expect("parse");
    let add = fns
        .iter()
        .find(|f| f.name == "TestAdd")
        .expect("the Go pack did not find TestAdd");
    let strongest = add.assertions.iter().copied().max().expect("assertions");
    assert_eq!(
        strongest,
        Strength::Equality,
        "the Go pack reads the stdlib shape as {strongest:?}. If this fails, the \
         defect this file documents in verify is ALSO present here, and the \
         comparison that justifies the pack is void."
    );
    // Not a claim about verify — a claim about this crate — but it belongs
    // beside the one above, because a fix that only moved the bug is not a fix.
    let _ = strength_of("t.Errorf(\"x\")");
}
