//! Which of this crate's four engine bugs `crates/verify` also has.
//!
//! # Why this file exists
//!
//! `thruline-langpacks` was built on `verify::tamper`'s `LangSpec` shape and
//! consumes its `strip_string_literals`, so a bug in this crate's engine is a
//! question about verify's, not an answer. Track Z asked for the answer per
//! bug, with a reproducing fixture, rather than a claim.
//!
//! Measured against verify at core `04d86de` (unchanged through `c37760d`):
//!
//! | # | bug | in verify? |
//! |---|---|---|
//! | 1 | raw comments into the literal stripper, ahead of `body_tokens` | ~~YES~~ — **fixed at `e9f2b84`**, test retired |
//! | 2 | `contains("throw")` exempts `catch (Throwable t)` | ~~YES since R1~~ — **fixed at `189a3b7`**, test retired |
//! | 3 | `defer`/`recover` swallow invisible | **YES for Go**; the Ruby half does not apply, verify has no Ruby |
//! | 4 | honest expects-a-throw refuted at `fail` | **YES** |
//!
//! Bugs 1, 3-Go and 4 are filed as interface requests R6 and R7. Bug 2
//! **was** this crate's alone and is not any more: R1 (`af5d5f8`) gave verify a
//! structural catch path, and its re-throw check is the substring form this
//! crate had and fixed. Filed as R10.
//!
//! # Characterisation, not aspiration
//!
//! Every assertion states what verify does **today**. When verify is fixed
//! these fail and say so — which is correct, because the fix and this file's
//! removal are one change.

use thruline_verify::tamper::{diff_test_file, Language};
use thruline_verify::verdict::{Finding, Severity};

fn fired(fs: &[Finding], detector: &str, severity: Severity) -> bool {
    fs.iter()
        .any(|f| f.detector == detector && f.severity() == severity)
}

// **Bug 1 retired at core `e9f2b84`.**
//
// `bug1_verify_reports_an_honest_rename_as_a_deletion_when_a_comment_has_an_apostrophe`
// stood here. It asserted that a Python rename which also reworded `isn't` in a
// comment came back `test_deleted` at `fail`, because raw comment text reached
// `strip_string_literals` ahead of `body_tokens` and an apostrophe truncated the
// token set.
//
// R6's core half landed and the test failed with exactly the message it was
// written to carry: verify now returns `test_renamed` at `info`. Retired as its
// own message instructed, and R6 marked landed in
// the interface requests.
//
// The control that isolated the cause — the same rename with `is not` instead of
// `isn't` — went with it; it existed to show the apostrophe was the whole
// difference, and there is no longer a difference to attribute.

/// **Bug 4 — verify has it.** A `fail`-severity false accusation against the
/// expects-a-throw idiom.
///
/// verify's swallow rule is a substring one: a `swallow_markers` hit beside one
/// of `pass` / `{}` / `continue`. JavaScript's markers are `catch {}` and
/// `catch (e) {}`, so the honest
/// `try { rate(0); assert.fail('must throw'); } catch (e) {}` matches on both
/// halves and fires.
///
/// It fires on the **transition**, which is the realistic honest edit: a test
/// that did not previously use the idiom and now does.
#[test]
fn bug4_verify_refutes_a_javascript_test_that_gains_the_expects_a_throw_idiom() {
    let base = "it('rejects a weightless parcel', () => {\n\
                \x20 assert.throws(() => rate(0));\n\
                });\n";
    let head = "it('rejects a weightless parcel', () => {\n\
                \x20 try { rate(0); assert.fail('must not be priced'); } catch (e) {}\n\
                });\n";
    let findings = diff_test_file("t.js", base, head, Language::JavaScript).expect("diff");
    assert!(
        fired(&findings, "assertion_swallowed", Severity::Fail),
        "verify no longer refutes this — bug 4 is fixed there; strike it from \
         R7 and delete this test: {findings:?}"
    );
}

/// **Bug 3 — verify has the Go half.**
///
/// verify's substring path genuinely covers the shape it was written for:
/// Python's `except:` + `pass` fires, correctly. What it cannot see is a
/// language whose swallow has no marker in that table — Go's
/// `defer func() { recover() }()`, for which verify ships
/// `swallow_markers: &[]`.
///
/// The Ruby half of this crate's bug 3 does not apply: verify has no Ruby.
#[test]
fn bug3_verify_sees_pythons_except_pass_and_not_gos_defer_recover() {
    let py_base = "def test_rate():\n    assert rate(2) == 9.0\n";
    let py_head = "def test_rate():\n\
                   \x20   try:\n\
                   \x20       assert rate(2) == 9.0\n\
                   \x20   except:\n\
                   \x20       pass\n";
    let py = diff_test_file("t.py", py_base, py_head, Language::Python).expect("diff");
    assert!(
        fired(&py, "assertion_swallowed", Severity::Fail),
        "verify stopped catching Python's `except: pass`, which is the one shape \
         its substring path was built for: {py:?}"
    );

    let go_base = "func TestRate(t *testing.T) {\n\
                   \tif got := Rate(2); got != 9.0 {\n\t\tt.Errorf(\"bad\")\n\t}\n}\n";
    let go_head = "func TestRate(t *testing.T) {\n\
                   \tdefer func() { recover() }()\n\
                   \tif got := Rate(2); got != 9.0 {\n\t\tt.Errorf(\"bad\")\n\t}\n}\n";
    let go = diff_test_file("t.go", go_base, go_head, Language::Go).expect("diff");
    assert!(
        !fired(&go, "assertion_swallowed", Severity::Fail),
        "verify now sees Go's defer/recover swallow — the Go half of bug 3 is \
         fixed there; strike it from R7 and delete this half: {go:?}"
    );
}

// **Bug 2 retired at core `189a3b7`.**
//
// `bug2_verify_now_shares_the_throwable_blind_spot_it_inherited_with_r1` stood
// here. It asserted that R1's new re-throw exemption was the substring form —
// `["raise","throw","rethrow","panic"].any(|k| lower.contains(k))` — under which
// `"Throwable"` contains `"throw"` and `catch (Throwable t) { }`, the broadest
// swallow there is, read as a handler that re-throws and was exempted.
//
// The core took the token-wise form this crate had used since `da7243a`:
// `rethrows` now splits on non-identifier characters and matches whole
// identifiers. The test failed by name on the first run after the merge, which
// is what it was written to do, and is retired here against `189a3b7` in a
// commit that says so.
//
// This entry is the whole life-cycle of one finding in three notes: bug 2 was
// recorded as NOT shared, became shared when R1 grew a re-throw concept and
// inherited the wrong spelling, and is now fixed. The middle note existed only
// because the first one was written as a test rather than a sentence.
