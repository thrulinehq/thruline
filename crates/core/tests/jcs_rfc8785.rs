//! RFC 8785 conformance for `thruline_core::spec::canonical`.
//!
//! Every hash in BUILD-SPEC §3.1 is taken over the bytes that module produces,
//! so "is our canonicalizer correct?" is not a library-quality question — it is
//! the question of whether two ThruLine installations agree about what an event
//! *is*. The spec's instruction was explicit: verify against the RFC 8785 test
//! vectors and commit those as tests.
//!
//! Three independent lines of evidence, because one is not enough for a
//! function that defines identity:
//!
//! 1. **The reference vectors, as bytes.** `fixtures/jcs/{input,output,outhex}`
//!    is the `cyberphone/json-canonicalization` test corpus (see the fixture
//!    README for provenance). The `outhex` comparison is the load-bearing one:
//!    comparing `String`s would pass on output that differs by a BOM or by
//!    Unicode normalisation, and this is precisely the domain where that
//!    happens.
//! 2. **Differential testing against a second implementation.**
//!    `serde_json_canonicalizer` is an independently written RFC 8785
//!    canonicalizer. Two implementations agreeing with the reference *and* with
//!    each other on generated input is much stronger than either alone.
//! 3. **The properties that make it a canonicalization at all** — idempotence,
//!    and insensitivity to input key order and whitespace.

use std::path::{Path, PathBuf};

use thruline_core::spec::canonical::{canonical_bytes, canonical_string};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jcs")
}

/// The six reference documents.
const VECTORS: [&str; 6] = [
    "arrays",
    "french",
    "structures",
    "unicode",
    "values",
    "weird",
];

fn read(part: &str, name: &str, ext: &str) -> String {
    let path = fixtures().join(part).join(format!("{name}.{ext}"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// `outhex` is space- and newline-separated lowercase hex bytes.
fn parse_hex(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| {
            u8::from_str_radix(byte, 16).unwrap_or_else(|e| panic!("bad hex byte {byte:?}: {e}"))
        })
        .collect()
}

#[test]
fn reference_vectors_match_byte_for_byte() {
    for name in VECTORS {
        let input: serde_json::Value =
            serde_json::from_str(&read("input", name, "json")).expect("fixture input parses");
        let got = canonical_bytes(&input).expect("canonicalizable");

        let want_bytes = parse_hex(&read("outhex", name, "txt"));
        assert_eq!(
            got,
            want_bytes,
            "{name}: canonical bytes differ from the RFC 8785 reference vector\n\
             got : {:?}\nwant: {:?}",
            String::from_utf8_lossy(&got),
            String::from_utf8_lossy(&want_bytes),
        );

        // The `output/` files carry a trailing newline that is not part of the
        // canonical form; the hex vectors are the exact bytes.
        let want_text = read("output", name, "json");
        assert_eq!(
            String::from_utf8_lossy(&got),
            want_text.trim_end_matches('\n'),
            "{name}: canonical text differs from the RFC 8785 reference vector"
        );
    }
}

#[test]
fn reference_vectors_are_actually_present_and_non_trivial() {
    // A corpus that silently became empty would let every assertion above pass
    // vacuously. This is the failure mode this repo has been bitten by before
    // (A209-C2 BLOCKING 4: a 173-case corpus that existed only in a scratchpad).
    let mut total = 0usize;
    for name in VECTORS {
        let bytes = parse_hex(&read("outhex", name, "txt"));
        assert!(bytes.len() > 10, "{name}: vector is too small to be real");
        total += bytes.len();
    }
    // Measured, not guessed: the reference corpus is exactly 622 canonical
    // bytes across the six documents. A floor rather than an equality so
    // adding a vector is not a test failure, but shrinkage or an empty corpus
    // is caught loudly.
    assert!(
        total >= 622,
        "the JCS corpus totals only {total} canonical bytes — the measured \
         reference corpus is 622, so this one is truncated or missing files"
    );

    // The corpus must actually exercise the two traps, or "it passes" means
    // nothing. Asserted over the whole corpus rather than over a guessed file:
    // the non-BMP character lives in `weird.json`, not — as the name suggests —
    // in `unicode.json`, which is a Unicode *normalization* case (a bare "Å").
    let corpus: String = VECTORS
        .iter()
        .map(|name| read("output", name, "json"))
        .collect();
    let non_bmp: Vec<char> = corpus.chars().filter(|c| *c as u32 > 0xFFFF).collect();
    assert!(
        !non_bmp.is_empty(),
        "no vector contains a non-BMP character, so the corpus cannot \
         discriminate UTF-16 key order from UTF-8 byte order"
    );

    // Stronger: prove the non-BMP character is used as a KEY and is sorted
    // where UTF-16 puts it, not where UTF-8 would. In `weird.json` the keys
    // run € (U+20AC), 😂 (U+1F602, surrogates D83D DE02), דּ (U+FB33).
    // UTF-16 order is 20AC < D83D < FB33 — 😂 in the middle. UTF-8 / code-point
    // order would put 😂 last.
    let weird = read("output", "weird", "json");
    let euro = weird.find('\u{20ac}').expect("€ key present");
    let smiley = weird.find('\u{1f602}').expect("😂 key present");
    let dalet = weird.find('\u{fb33}').expect("דּ key present");
    assert!(
        euro < smiley && smiley < dalet,
        "the non-BMP key is not in UTF-16 code-unit position — the corpus is \
         not the discriminating one it is relied on to be"
    );

    assert!(
        read("output", "values", "json").contains("e+30"),
        "no vector carries an exponent-form number, so ECMAScript \
         Number::toString is untested"
    );
}

#[test]
fn agrees_with_an_independent_implementation_on_the_reference_corpus() {
    for name in VECTORS {
        let input: serde_json::Value =
            serde_json::from_str(&read("input", name, "json")).expect("fixture input parses");
        let ours = canonical_bytes(&input).expect("canonicalizable");
        let theirs = serde_json_canonicalizer::to_vec(&input).expect("canonicalizable");
        assert_eq!(
            ours, theirs,
            "{name}: our canonicalizer and serde_json_canonicalizer disagree"
        );
    }
}

#[test]
fn agrees_with_an_independent_implementation_on_adversarial_input() {
    // Hand-picked for the places RFC 8785 implementations actually diverge.
    let cases = vec![
        serde_json::json!({}),
        serde_json::json!([]),
        serde_json::json!(null),
        // UTF-16 vs UTF-8 key order: the surrogate range sits below U+E000.
        serde_json::json!({"\u{e000}": 1, "\u{10000}": 2, "\u{ffff}": 3, "\u{10ffff}": 4}),
        // Keys that are prefixes of one another.
        serde_json::json!({"a": 1, "aa": 2, "": 3, "A": 4}),
        // The full JSON escape set plus characters that must NOT be escaped.
        serde_json::json!({"s": "\"\\\u{8}\u{c}\n\r\t\u{0}\u{1f}/<>&\u{7f}"}),
        // Number boundaries where ES switches to exponent form.
        serde_json::json!([
            0.0,
            -0.0,
            1e-6,
            1e-7,
            1e20,
            1e21,
            1e-323,
            5e-324,
            9007199254740991i64,
            -9007199254740991i64,
            1.7976931348623157e308
        ]),
        // Deep nesting, and duplicate-looking values that must stay distinct.
        serde_json::json!({"z": [{"b": [1, {"a": {}}]}, {"a": null}], "": {"": {"": 0}}}),
    ];
    for case in cases {
        let ours = canonical_bytes(&case).expect("canonicalizable");
        let theirs = serde_json_canonicalizer::to_vec(&case).expect("canonicalizable");
        assert_eq!(
            String::from_utf8_lossy(&ours),
            String::from_utf8_lossy(&theirs),
            "implementations disagree on {case}"
        );
    }
}

#[test]
fn canonicalization_is_idempotent_and_input_order_blind() {
    for name in VECTORS {
        let input: serde_json::Value =
            serde_json::from_str(&read("input", name, "json")).expect("fixture input parses");
        let once = canonical_string(&input).expect("canonicalizable");
        let reparsed: serde_json::Value = serde_json::from_str(&once).expect("canonical is JSON");
        let twice = canonical_string(&reparsed).expect("canonicalizable");
        assert_eq!(once, twice, "{name}: canonicalization is not idempotent");
    }

    // Whitespace and member order in the source text must not survive.
    let a: serde_json::Value = serde_json::from_str(r#"{"b":{"y":1,"x":2},"a":[3,4]}"#).expect("a");
    let b: serde_json::Value =
        serde_json::from_str("  {\n \"a\" : [ 3 , 4 ] ,\n \"b\" : { \"x\" : 2 , \"y\" : 1 }\n}  ")
            .expect("b");
    assert_eq!(
        canonical_string(&a).expect("canonicalizable"),
        canonical_string(&b).expect("canonicalizable")
    );
}

/// Differential testing at volume, against the second implementation.
///
/// BUILD-PLAN M0.2's done-when (a): ≥10,000 generated cases in which
/// `serde_jcs` and `serde_json_canonicalizer` must agree byte-for-byte.
///
/// **Floats are excluded from the generator on purpose**, and not to make the
/// test easier: BUILD-PLAN C.4/D2 forbids non-integer numbers inside hashed
/// event payloads for exactly the reason a float-bearing differential test
/// would be exercising — ECMAScript number serialization is where conforming
/// implementations diverge. `spec::canonical`'s own unit tests still pin the
/// float boundaries (`1e-7`, `1e21`, `-0.0`, the RFC's `333333333.33333329`),
/// so the behaviour is covered; it is simply not reachable from an `Event`.
///
/// **Lone surrogates are absent, and cannot be added.** RFC 8785 §3.2.2.2
/// governs them, but a Rust `String` is well-formed UTF-8 by construction and
/// `serde_json` refuses `"\ud800"` at parse time, so no `serde_json::Value`
/// can hold one. That is a real gap in this test's reach and it is stated
/// rather than papered over: a producer emitting a lone surrogate is rejected
/// before canonicalization ever sees it, which is the safe direction, but it
/// is untested here because it is unrepresentable.
mod differential {
    use super::*;
    use proptest::prelude::*;
    /// 2^53 - 1, mirrored as `i64` for the range strategy. Asserted equal to
    /// `spec::event::MAX_SAFE_INTEGER` below so the two cannot drift.
    const SAFE: i64 = 9_007_199_254_740_991;

    /// Strings that reach into the parts of Unicode canonicalization gets wrong.
    fn interesting_string() -> impl Strategy<Value = String> {
        prop_oneof![
            // The JSON escape set, and characters that must NOT be escaped.
            Just("\"\\\u{8}\u{c}\n\r\t\u{0}\u{1f}/<>&\u{7f}".to_string()),
            // Astral plane: encoded as surrogate pairs in UTF-16, so key
            // ordering depends on getting that encoding right.
            "[\u{10000}-\u{10ffff}]{1,4}",
            // The BMP range that sorts AFTER the surrogates in UTF-16 and
            // BEFORE the astral plane in UTF-8 — the discriminating band.
            "[\u{e000}-\u{ffff}]{1,4}",
            // Combining marks and precomposed forms: canonicalization must NOT
            // normalise, so "Å" and "A\u{30a}" have to stay distinct.
            Just("\u{c5}".to_string()),
            Just("A\u{30a}".to_string()),
            "[a-zA-Z0-9 _.:-]{0,12}",
            any::<String>(),
        ]
    }

    fn value() -> impl Strategy<Value = serde_json::Value> {
        let leaf = prop_oneof![
            Just(serde_json::Value::Null),
            any::<bool>().prop_map(serde_json::Value::Bool),
            // Bounded to ±(2^53 - 1). Beyond that, RFC 8785 routes through an
            // IEEE-754 double and rounds — `serde_json` round-trips the exact
            // integer, so `parse(canonicalize(n)) != n` and the fixpoint
            // property genuinely fails. That is not a bug in either
            // canonicalizer; it is a property of RFC 8785, and it is why
            // `spec::event::reject_unhashable_numbers` refuses such numbers
            // rather than hashing them. Found by this test.
            (-SAFE..=SAFE).prop_map(|n| serde_json::json!(n)),
            interesting_string().prop_map(serde_json::Value::String),
        ];
        leaf.prop_recursive(4, 48, 6, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..6).prop_map(serde_json::Value::Array),
                prop::collection::hash_map(interesting_string(), inner, 0..6)
                    .prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
            ]
        })
    }

    #[test]
    fn the_generator_bound_matches_the_model() {
        assert_eq!(
            i128::from(SAFE),
            thruline_core::spec::event::MAX_SAFE_INTEGER,
            "the generator's safe-integer bound has drifted from the event model's"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        #[test]
        fn two_implementations_agree_byte_for_byte(v in value()) {
            let ours = canonical_bytes(&v)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            let theirs = serde_json_canonicalizer::to_vec(&v)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(
                String::from_utf8_lossy(&ours).to_string(),
                String::from_utf8_lossy(&theirs).to_string()
            );
        }

        /// serialize → parse → serialize is a fixpoint, and the parse of the
        /// canonical form is the same value.
        #[test]
        fn canonicalization_is_a_fixpoint(v in value()) {
            let once = canonical_string(&v)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            let reparsed: serde_json::Value = serde_json::from_str(&once)
                .map_err(|e| TestCaseError::fail(format!("{e}: {once}")))?;
            let twice = canonical_string(&reparsed)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(&once, &twice);
            prop_assert_eq!(reparsed, v);
        }
    }
}
