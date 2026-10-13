//! T-3-6-F1 (`Q1`) — the count is read from the run's own line, or not at all.
//!
//! The rule under test is small and load-bearing: it decides whether a line
//! that counts as done reads "Proven" or "it checked nothing", so everything
//! it must NOT do is asserted here beside what it must.

use thruline_core::test_tally::{parse_result_line, Scanner, TestTally};

fn scan(chunks: &[&str]) -> TestTally {
    let mut s = Scanner::new();
    for c in chunks {
        s.feed(c.as_bytes());
    }
    s.finish()
}

#[test]
fn the_shapes_libtest_actually_prints_are_read() {
    let ok = parse_result_line(
        "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
    )
    .expect("a pass line is a count");
    assert_eq!((ok.passed, ok.failed, ok.ignored, ok.suites), (3, 0, 0, 1));

    let failed = parse_result_line(
        "test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out",
    )
    .expect("a failure line is a count too");
    assert_eq!((failed.passed, failed.failed), (1, 2));
    assert!(
        !failed.counts_a_suite(),
        "a failure never counts as a suite"
    );

    let filtered = parse_result_line(
        "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out",
    )
    .expect("an empty run still states its count");
    assert!(
        !filtered.counts_a_suite(),
        "zero passes is a run that checked nothing, not a pass"
    );
}

#[test]
fn a_line_that_is_not_a_count_is_never_a_zero() {
    for line in [
        "running 3 tests",
        "test result:",
        "test result: ok. some passed; no failed",
        "Compiling thruline-core v0.1.0",
        "the word passed appears here and so does failed",
        "",
    ] {
        assert_eq!(
            parse_result_line(line),
            None,
            "{line:?} was read as a count"
        );
    }
}

#[test]
fn several_suites_in_one_run_are_one_run() {
    let t = scan(&[
        "running 2 tests\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
        "running 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
    ]);
    assert_eq!((t.passed, t.failed, t.suites), (3, 0, 2));
    assert!(t.counts_a_suite());
}

#[test]
fn one_failing_suite_beside_two_passing_ones_is_not_a_pass() {
    let t = scan(&[
        "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
        "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n",
        "test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
    ]);
    assert_eq!((t.passed, t.failed, t.suites), (7, 1, 3));
    assert!(!t.counts_a_suite());
}

#[test]
fn the_line_is_read_however_the_stream_is_chopped_up() {
    let whole = "test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
    for cut in 1..whole.len() {
        let t = scan(&[&whole[..cut], &whole[cut..]]);
        assert_eq!(t.passed, 4, "a chunk boundary at {cut} lost the count");
        assert_eq!(t.suites, 1);
    }
}

#[test]
fn a_last_line_with_no_newline_after_it_still_counts() {
    let t = scan(&["test result: ok. 9 passed; 0 failed"]);
    assert_eq!((t.passed, t.suites), (9, 1));
}

#[test]
fn a_stream_with_no_count_in_it_stores_nothing() {
    let t = scan(&["all good!\nfinished\n"]);
    assert_eq!(t.suites, 0);
    assert!(!t.counts_a_suite());
    assert_eq!(t, TestTally::default());
}

#[test]
fn an_enormous_single_line_cannot_grow_the_wrapper_and_is_not_a_count() {
    let mut huge = "x".repeat(2_000_000);
    huge.push_str("test result: ok. 3 passed; 0 failed");
    huge.push('\n');
    let t = scan(&[&huge]);
    assert_eq!(
        t.suites, 0,
        "a line past the cap is dropped, never grown and never counted"
    );
    // And the scanner keeps working on the next line, which is the point of
    // dropping rather than stopping.
    let mut s = Scanner::new();
    s.feed(huge.as_bytes());
    s.feed(b"test result: ok. 1 passed; 0 failed\n");
    assert_eq!(s.finish().passed, 1);
}

#[test]
fn bytes_that_are_not_utf8_are_not_a_count_and_do_not_panic() {
    let mut s = Scanner::new();
    s.feed(&[0xff, 0xfe, b'\n']);
    s.feed(b"test result: ok. 2 passed; 0 failed\n");
    let t = s.finish();
    assert_eq!((t.passed, t.suites), (2, 1));
}
