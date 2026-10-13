//! T-4-2 — the terminal door says what the screen says.
//!
//! `thruline change finding` prints the seven standing labels and the refusal
//! sentences from `finding_standing.rs`; the screen prints them from
//! `HQ_COPY.change.review.standing`. Two tables for one sentence is how two
//! doors come to disagree, so this test reads the copy file's checked-in
//! golden (`ui/src/workspace/hq-copy.golden.json`) and holds the Rust mirror
//! equal to it, word for word.

use thruline_core::finding_standing::{part_words, refusal_line, Standing, REFUSAL_CODES};

fn golden() -> serde_json::Map<String, serde_json::Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/src/workspace/hq-copy.golden.json");
    let text = std::fs::read_to_string(&path).expect("the copy golden must be readable");
    serde_json::from_str(&text).expect("the copy golden is JSON")
}

fn at<'a>(g: &'a serde_json::Map<String, serde_json::Value>, key: &str) -> &'a str {
    g.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("the copy file has no {key}"))
}

#[test]
fn the_seven_labels_are_the_copy_files_labels() {
    let g = golden();
    for s in Standing::ALL {
        assert_eq!(
            s.label(),
            at(&g, &format!("change.review.standing.labels.{}", s.as_str())),
            "{s:?}"
        );
    }
}

#[test]
fn every_refusal_sentence_is_the_copy_files_sentence() {
    let g = golden();
    for code in REFUSAL_CODES {
        assert_eq!(
            refusal_line(code),
            at(&g, &format!("change.review.standing.refused.{code}")),
            "{code}"
        );
    }
    assert_eq!(
        refusal_line("not-a-code"),
        at(&g, "change.review.standing.refused.other")
    );
}

#[test]
fn the_missing_parts_are_named_in_the_copy_files_words() {
    let g = golden();
    for part in ["actor", "reason", "authority"] {
        assert_eq!(
            part_words(part),
            at(&g, &format!("change.review.standing.parts.{part}"))
        );
    }
}

#[test]
fn the_labels_never_borrow_a_state_word() {
    // The Rust side of `(t42)` in hq-copy.test.js, so a label changed only
    // here is caught here.
    for s in Standing::ALL {
        let l = s.label().to_lowercase();
        for w in [
            "proven",
            "not proven",
            "couldn't check",
            "loose thread",
            "fog",
            "proof",
            // The Change screen's ruled state words (ADV-review-input-3).
            "in progress",
            "recorded",
            "needs a re-check",
        ] {
            assert!(!l.contains(w), "{s:?} label borrows {w:?}");
        }
    }
    // And no refusal sentence the terminal door prints borrows a ruled one.
    for code in REFUSAL_CODES {
        let line = refusal_line(code).to_lowercase();
        for w in ["in progress", "needs a re-check", "not proven"] {
            assert!(!line.contains(w), "{code} borrows {w:?}: {line}");
        }
    }
}

/// ADV-review-input-2 (adversary F4): the sentence both doors print for a
/// part over the limit is the copy file's, word for word, and the number in
/// it is the engine's `MAX_PART_CHARS`, written as a person reads it. The
/// screen's side of the same number is `review-model.test.js`.
#[test]
fn the_too_long_sentence_is_the_copy_files_and_states_the_engines_limit() {
    let g = golden();
    assert!(REFUSAL_CODES.contains(&"too_long"), "{REFUSAL_CODES:?}");
    let line = refusal_line("too_long");
    assert_eq!(line, at(&g, "change.review.standing.refused.too_long"));
    let n = thruline_core::finding_standing::MAX_PART_CHARS;
    let written = format!("{},{:03}", n / 1000, n % 1000);
    assert!(line.contains(&format!("{written} characters")), "{line}");
    assert_eq!(part_words("reason"), "the reason");
}

/// FIXED-WORDING (R-2026-09-28-9): the lines the terminal door prints beside
/// a fix are the copy file's, word for word. The golden renders each template
/// with the copy test's sample values (`path` = `src/app/main.rs`, `line` = 2,
/// `n` = 9), so the Rust templates are filled with the same ones here.
#[test]
fn the_fixed_line_and_the_re_check_list_are_the_copy_files_words() {
    use thruline_core::finding_standing::{
        fixed_sentence, FINDING_AT, FINDING_IN, RECHECK_CHECK_RUN, RECHECK_MANY, RECHECK_NONE,
        RECHECK_NO_PLACE, RECHECK_ONE, RECHECK_UNREADABLE,
    };
    let g = golden();
    assert_eq!(
        fixed_sentence(Some("src/app/main.rs"), Some(2)),
        at(&g, "change.review.standing.fixed_at_line")
    );
    assert_eq!(
        fixed_sentence(Some("src/app/main.rs"), None),
        at(&g, "change.review.standing.fixed_at_file")
    );
    assert_eq!(
        fixed_sentence(None, None),
        at(&g, "change.review.standing.fixed_no_place")
    );
    assert_eq!(
        fixed_sentence(None, Some(2)),
        at(&g, "change.review.standing.fixed_no_place")
    );
    for (mine, key) in [
        (RECHECK_NONE, "recheck_none"),
        (RECHECK_ONE, "recheck_one"),
        (RECHECK_NO_PLACE, "recheck_no_place"),
        (RECHECK_CHECK_RUN, "recheck_check_run"),
        (RECHECK_UNREADABLE, "recheck_unreadable"),
    ] {
        assert_eq!(
            mine,
            at(&g, &format!("change.review.standing.{key}")),
            "{key}"
        );
    }
    assert_eq!(
        RECHECK_MANY.replace("{n}", "9"),
        at(&g, "change.review.standing.recheck_many")
    );
    assert_eq!(
        FINDING_AT
            .replace("{path}", "src/app/main.rs")
            .replace("{line}", "2"),
        at(&g, "change.review.finding_at")
    );
    assert_eq!(
        FINDING_IN.replace("{path}", "src/app/main.rs"),
        at(&g, "change.review.finding_in")
    );
    // The founder's sentence, as ruled, is what a fix with a place says.
    assert!(fixed_sentence(Some("a.py"), Some(7))
        .starts_with("Fixed: the re-check didn't raise this finding at a.py"));
}
