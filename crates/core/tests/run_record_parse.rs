//! T-3-6-F3 (`Q2`, `B16.8`, `B16.10`) — what a failing run SAID, and what
//! class of evidence it was declared to be.
//!
//! The rule under test, stated once: **the failing assertion's own words are
//! read out of the stream at record time, and an absent line is never filled
//! in.** A failure whose words were not captured is not a failure with no
//! words, and the screens say so in those words.
//!
//! The class is **declared**, never derived: nothing here reads a branch name,
//! and a word that is not one of the four is refused rather than coerced.

use thruline_core::run_record::{EvidenceClass, FailureScanner, RunRecord, MAX_FAILURE_LINE};

/// Exactly the bytes `cargo test` printed on case `Q2`, including the blank
/// lines and the backtrace note that must NOT be kept.
const Q2_OUTPUT: &str = "running 2 tests\n\
test a_sent_delivery_refuses_a_retry ... ok\n\
test two_retry_requests_for_the_same_delivery_enqueue_one_job ... FAILED\n\
\n\
failures:\n\
\n\
---- two_retry_requests_for_the_same_delivery_enqueue_one_job stdout ----\n\
\n\
thread 'two_retry_requests_for_the_same_delivery_enqueue_one_job' (23369770) panicked at tests/duplicate_retry.rs:28:5:\n\
assertion `left == right` failed: a second retry request while one is pending must return the same job, not a new one\n\
  left: \"job_18d7bc73572f5c20_0002\"\n\
 right: \"job_18d7bc73573157f0_0003\"\n\
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\
\n\
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";

fn scan(text: &str) -> Option<String> {
    let mut scanner = FailureScanner::new();
    scanner.feed(text.as_bytes());
    scanner.finish()
}

#[test]
fn the_failing_assertions_own_words_are_read_out_of_the_stream() {
    let line = scan(Q2_OUTPUT).expect("the stream carried a failure block");
    // What a person needs: the sentence the assertion carried, and the two
    // values it printed. This is the fact case `Q2` said appeared nowhere.
    assert!(
        line.contains("must return the same job, not a new one"),
        "the assertion's message: {line}"
    );
    assert!(line.contains("job_18d7bc73572f5c20_0002"), "left: {line}");
    assert!(line.contains("job_18d7bc73573157f0_0003"), "right: {line}");
    // And NOT the backtrace note: that is libtest's advice to whoever is at a
    // terminal, not the test's own words about what it found.
    assert!(!line.contains("RUST_BACKTRACE"), "{line}");
    // Nor the panic header itself, which names a file and a line number — the
    // technical layer's business, not a sentence a person reads first.
    assert!(!line.contains("panicked at"), "{line}");
}

#[test]
fn a_stream_with_no_failure_yields_no_line_rather_than_an_empty_one() {
    let passing = "running 2 tests\ntest a ... ok\ntest b ... ok\n\n\
                   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
    assert_eq!(scan(passing), None);
    assert_eq!(scan(""), None);
    // An absent line is stored as nothing at all, so there is no row to read
    // as "the failure said nothing".
    assert!(RunRecord::default().is_empty());
}

#[test]
fn only_the_first_failure_block_is_kept() {
    let two = "thread 'a' panicked at src/lib.rs:1:1:\nfirst thing broke\n\n\
               thread 'b' panicked at src/lib.rs:2:2:\nsecond thing broke\n";
    let line = scan(two).expect("a block");
    assert!(line.contains("first thing broke"), "{line}");
    assert!(!line.contains("second thing broke"), "{line}");
}

#[test]
fn a_block_is_bounded_and_is_cut_on_a_character_boundary() {
    // A panic message of multibyte characters, longer than the cap it is
    // kept under and shorter than the line the scanner will assemble at all.
    let long = "é".repeat(400);
    let text = format!("thread 'a' panicked at src/lib.rs:1:1:\n{long}\n");
    let line = scan(&text).expect("a block");
    assert!(line.len() <= MAX_FAILURE_LINE, "{} bytes", line.len());
    // It is valid UTF-8 by construction (it is a String), and it carries no
    // mark this product invented to say it was cut.
    assert!(!line.contains('…'), "no invented ellipsis: {line}");
    assert!(!line.contains("..."), "no invented ellipsis: {line}");
}

#[test]
fn a_line_too_long_to_assemble_is_dropped_rather_than_grown() {
    // A run that prints a megabyte on one line cannot make the wrapper the
    // problem: the line is dropped, which reads as the record holding no line
    // from the failure — an absence, which is what it is.
    let huge = "x".repeat(200_000);
    let text = format!("thread 'a' panicked at src/lib.rs:1:1:\n{huge}\n");
    assert_eq!(scan(&text), None);
}

#[test]
fn the_scanner_is_the_same_whatever_the_chunking() {
    let whole = scan(Q2_OUTPUT).expect("a block");
    let mut scanner = FailureScanner::new();
    for byte in Q2_OUTPUT.as_bytes() {
        scanner.feed(&[*byte]);
    }
    assert_eq!(scanner.finish().expect("a block"), whole);
}

#[test]
fn a_last_line_with_no_newline_after_it_is_still_read() {
    let line = scan("thread 'a' panicked at src/lib.rs:1:1:\nit broke").expect("a block");
    assert_eq!(line, "it broke");
}

#[test]
fn a_class_is_one_of_four_and_a_word_from_outside_is_refused() {
    assert_eq!(EvidenceClass::parse("seeded"), Some(EvidenceClass::Seeded));
    assert_eq!(
        EvidenceClass::parse("naturally_occurring"),
        Some(EvidenceClass::NaturallyOccurring)
    );
    // Not coerced, not defaulted, not stored as "unknown".
    assert_eq!(EvidenceClass::parse("SEEDED"), None);
    assert_eq!(EvidenceClass::parse("ordinary"), None);
    assert_eq!(EvidenceClass::parse(""), None);
    assert_eq!(EvidenceClass::every().len(), 4);
    // Each class has its own copy-file slug: a screen cannot render two
    // classes with one sentence.
    let slugs: std::collections::BTreeSet<_> =
        EvidenceClass::every().iter().map(|c| c.slug()).collect();
    assert_eq!(slugs.len(), 4);
}
