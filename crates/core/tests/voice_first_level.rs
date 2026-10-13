//! §5v's teeth.
//!
//! Two tests, and neither is a formality:
//!
//! * `no_first_level_string_the_engine_can_emit_carries_engine_lexicon` walks
//!   every first-level string reachable from the presentation authority — every
//!   verdict, currency, proof currentness, abstention, all twelve readiness
//!   states, and every attention sentence and suggested action for every kind
//!   crossed with every readiness — and fails naming the offender.
//! * `the_ui_mirror_has_not_drifted` asserts the checked-in UI table is byte
//!   identical to the Rust table, which is what makes "one voice" a mechanical
//!   fact rather than a convention two codebases are trusted to keep.

use std::fs;
use std::path::PathBuf;

use thruline_core::attention::AttentionKind;
use thruline_core::capture::ProofApplicability;
use thruline_core::capture_product::CaptureReadiness;
use thruline_core::derive::{Currency, NodeState};
use thruline_core::voice;

const READINESS_STATES: [CaptureReadiness; 12] = [
    CaptureReadiness::NotAvailable,
    CaptureReadiness::BoundedOnly,
    CaptureReadiness::Disabled,
    CaptureReadiness::AvailableNotInstalled,
    CaptureReadiness::DegradedSourceVersion,
    CaptureReadiness::DegradedConfigurationDrift,
    CaptureReadiness::DegradedKeyOrIdentity,
    CaptureReadiness::InstalledUnverified,
    CaptureReadiness::DegradedSourcePairing,
    CaptureReadiness::UnsupportedProfile,
    CaptureReadiness::CaptureError,
    CaptureReadiness::AssuredReady,
];

const KINDS: [AttentionKind; 4] = [
    AttentionKind::Contradiction,
    AttentionKind::Staleness,
    AttentionKind::CaptureAttention,
    AttentionKind::AllClear,
];

fn check(label: &str, text: &str, count: &mut usize) {
    if let Some(term) = voice::banned_in(text) {
        panic!("engine lexicon {term:?} reached a first-level string.\n  where: {label}\n  text:  {text:?}");
    }
    *count += 1;
}

#[test]
fn no_first_level_string_the_engine_can_emit_carries_engine_lexicon() {
    let mut n = 0usize;

    for state in [
        NodeState::Recorded,
        NodeState::Supported,
        NodeState::Pending,
        NodeState::Unknown,
        NodeState::Unsupported,
        NodeState::Dropped,
        NodeState::Contradicted,
    ] {
        let t = voice::node_state(state);
        check("node badge word", t.word, &mut n);
        check("node sentence", t.sentence, &mut n);
        check("chip label", &voice::chip_label(t.word), &mut n);
    }

    for c in [
        Currency::Current,
        Currency::Superseded,
        Currency::OutOfScope,
    ] {
        let t = voice::currency(c);
        check("currency word", t.word, &mut n);
        check("currency sentence", t.sentence, &mut n);
    }

    for p in [
        ProofApplicability::Current,
        ProofApplicability::Stale,
        ProofApplicability::Unknown,
    ] {
        let t = voice::proof_currentness(p);
        check("proof word", t.word, &mut n);
        check("proof sentence", t.sentence, &mut n);
        check("proof chip", &voice::chip_label(t.word), &mut n);
    }

    for r in READINESS_STATES {
        let v = voice::readiness(r);
        check("readiness title", v.title, &mut n);
        check("readiness detail", v.detail, &mut n);
    }

    // Every attention sentence and action, for every kind, crossed with every
    // readiness state and with/without numbers. This is the combination a user
    // actually meets, so it is the combination the ban has to hold over.
    for kind in KINDS {
        for readiness in READINESS_STATES.map(Some).into_iter().chain([None]) {
            for numbers in [
                (None, None),
                (Some("33/33 tests passed"), Some("31 passed, 2 failed")),
            ] {
                let s = voice::attention_sentence(kind, numbers.0, numbers.1, readiness);
                check("attention sentence", &s, &mut n);
                let a = voice::attention_action(kind, readiness);
                check("attention action", &a, &mut n);
            }
        }
    }

    // Every claim state's level-two line, and L3's own copy. New surfaces come
    // with new strings; the walk grows with them or it stops meaning anything.
    for t in voice::CLAIM_STORIES {
        check("claim story", voice::claim_story(t.slug), &mut n);
    }
    check(
        "claim story fallback",
        voice::claim_story("no-such-state"),
        &mut n,
    );
    for at_widest in [true, false] {
        check("trace foot", voice::trace_foot(at_widest), &mut n);
    }
    check("trace scope", voice::trace_scope(), &mut n);
    for day in ["Jul 29", ""] {
        check(
            "trace caption",
            &voice::trace_caption(voice::verdict("contradicted").unwrap().sentence, day),
            &mut n,
        );
    }

    for count in [0u64, 1, 2, 17] {
        check("headline", &voice::headline("thruline", count), &mut n);
        check(
            "count phrase",
            &voice::count_phrase(count, "thing worth a look", "things worth a look"),
            &mut n,
        );
    }

    // Compositions built from the table, including the doctrine's example.
    let words: Vec<&str> = voice::VERDICTS.iter().map(|t| t.word).collect();
    let parts: Vec<(u64, &str)> = words
        .iter()
        .enumerate()
        .map(|(i, w)| (i as u64, *w))
        .collect();
    check("composition", &voice::composition(&parts), &mut n);

    assert!(
        n >= 200,
        "the walk covered only {n} strings — it is not walking what it claims to"
    );
}

#[test]
fn the_ui_mirror_has_not_drifted() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/lib/voice.data.js");
    let on_disk = fs::read_to_string(&path).expect("read the UI voice mirror");
    assert_eq!(
        on_disk,
        voice::mirror_module(),
        "\nui/src/lib/voice.data.js has drifted from crates/core/src/voice.rs.\n\
         The Rust table is the authority. Regenerate with:\n\
           THRULINE_BLESS_VOICE=1 cargo test -p thruline-core --test voice_first_level\n"
    );
}

#[test]
fn bless_the_mirror() {
    // Not a test so much as the generator, gated so a normal run never writes.
    if std::env::var("THRULINE_BLESS_VOICE").as_deref() != Ok("1") {
        return;
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/lib/voice.data.js");
    fs::write(&path, voice::mirror_module()).expect("write the UI voice mirror");
}
