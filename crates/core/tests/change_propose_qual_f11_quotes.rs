//! QUAL-F11-LAUNCH-R2 — a semicolon inside quoted text never cuts a sentence,
//! and a fragment with an unclosed quotation is never proposed.
//!
//! Round 1's adversary (`adversary_qual_f11_quote_semicolon`) found that
//! "The service shows 'delivered; Retry disabled' when the job ends." was
//! proposed as `["The service shows 'delivered."]`: the `;` before a capital
//! ended a span although it sat inside an open quotation, and the half with
//! the unclosed quote was offered as something that counts as done while the
//! real sentence never appeared whole.
//!
//! What these tests hold, for every kind of quotation a person types (' ' ,
//! " ", ‘ ’, “ ” and backticks): no span-ending character inside a closed
//! quotation ends a span, and a candidate whose quotation is never closed is
//! kept as a note — never proposed.
//!
//! Every id is synthetic (`cccccccc-…`) and no fixture here holds private
//! content.

use thruline_core::change::{self, ChangeRequest, NewChange, RequestSource};
use thruline_core::change_propose::{self, ProfileEnvironment, ProposalView, ShapeRefusal};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000f0ab";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000cfab";

fn proposals_for(purpose: &str) -> ProposalView {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, '/tmp/synthetic-f11-r2', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            rusqlite::params![PROJECT],
        )
        .expect("insert project");
    change::insert_change(
        &ledger,
        &NewChange {
            id: CHANGE.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Synthetic".to_string(),
                purpose: purpose.to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert the change");
    change_propose::propose(&ledger, CHANGE, &ProfileEnvironment::empty()).expect("propose")
}

fn criterion_texts(view: &ProposalView) -> Vec<String> {
    view.criteria.iter().map(|c| c.text.clone()).collect()
}

fn note_texts(view: &ProposalView) -> Vec<String> {
    view.notes.iter().map(|n| n.text.clone()).collect()
}

/// The ask is one sentence; it must be proposed whole and alone.
fn assert_proposed_whole(ask: &str) {
    let view = proposals_for(ask);
    assert_eq!(
        criterion_texts(&view),
        vec![ask.to_string()],
        "the one sentence of the ask must be proposed whole; notes: {:?}",
        note_texts(&view)
    );
    assert!(
        view.notes.is_empty(),
        "nothing is left over: {:?}",
        note_texts(&view)
    );
}

/// The adversary's reproduction, re-created (red at 9e1268f8).
#[test]
fn a_semicolon_inside_single_quotes_does_not_cut_the_sentence() {
    let ask = "The service shows 'delivered; Retry disabled' when the job ends.";
    let view = proposals_for(ask);
    let texts = criterion_texts(&view);
    assert!(
        !texts.contains(&"The service shows 'delivered.".to_string()),
        "an unclosed-quotation fragment was proposed as a criterion: {texts:?}"
    );
    assert_proposed_whole(ask);
}

#[test]
fn a_semicolon_inside_double_quotes_does_not_cut_the_sentence() {
    assert_proposed_whole("The service shows \"delivered; Retry disabled\" when the job ends.");
}

#[test]
fn a_semicolon_inside_typographic_single_quotes_does_not_cut_the_sentence() {
    assert_proposed_whole("The service shows ‘delivered; Retry disabled’ when the job ends.");
}

#[test]
fn a_semicolon_inside_typographic_double_quotes_does_not_cut_the_sentence() {
    assert_proposed_whole("The service shows “delivered; Retry disabled” when the job ends.");
}

#[test]
fn a_semicolon_inside_backticks_does_not_cut_the_sentence() {
    assert_proposed_whole("The service shows `delivered; Retry disabled` when the job ends.");
}

/// Every other span-ending character is held to the same rule: a full stop,
/// `!` or `?` inside a closed quotation ends nothing.
#[test]
fn no_span_ending_character_inside_a_quotation_ends_a_span() {
    assert_proposed_whole("The page shows 'Saved. Retry later' when the job fails.");
    assert_proposed_whole("The page shows \"Stop! Retry now\" when the job fails.");
    assert_proposed_whole("The page shows “Sent? Check again” when the job fails.");
}

/// A quotation that is opened and never closed is not a sentence anybody can
/// check. It is kept as a note, with the plain reason a fragment carries, and
/// nothing with an unclosed quotation is proposed.
#[test]
fn an_unclosed_quotation_is_kept_as_a_note_never_proposed() {
    for ask in [
        "The page shows 'Saved when the form is sent.",
        "The page shows \"Saved when the form is sent.",
        "The page shows ‘Saved when the form is sent.",
        "The page shows “Saved when the form is sent.",
        "The page shows `Saved when the form is sent.",
    ] {
        let view = proposals_for(ask);
        assert!(
            view.criteria.is_empty(),
            "{ask:?}: an unclosed quotation was proposed: {:?}",
            criterion_texts(&view)
        );
        let note = view
            .notes
            .iter()
            .find(|n| n.refusal == ShapeRefusal::Fragment)
            .unwrap_or_else(|| panic!("{ask:?}: kept as a fragment note: {:?}", note_texts(&view)));
        assert_eq!(note.reason_slug, "note_fragment");
        assert_eq!(format!("{}.", note.text), ask, "kept verbatim");
    }
}

/// The adversary's ask with its closing quote removed: the `;` before a
/// capital ends a span again, and neither half — one of which opens a
/// quotation it never closes — is proposed.
#[test]
fn a_semicolon_cut_never_proposes_the_half_with_an_open_quotation() {
    let view = proposals_for("The service shows 'delivered; Retry disabled when the job ends.");
    for text in criterion_texts(&view) {
        assert!(
            !text.contains('\''),
            "an open quotation was proposed: {text:?}"
        );
    }
    assert!(
        view.notes.iter().any(
            |n| n.refusal == ShapeRefusal::Fragment && n.text == "The service shows 'delivered"
        ),
        "the open-quotation half is kept as a fragment note: {:?}",
        note_texts(&view)
    );
}

/// An apostrophe is not a quotation: a contraction or a plural possessive
/// neither opens nor closes one, so these are proposed as before.
#[test]
fn an_apostrophe_is_not_a_quotation() {
    assert_proposed_whole("When the user's cart is empty, the total shows zero.");
    assert_proposed_whole("When the users' cart is empty, the total shows zero.");
    assert_proposed_whole(
        "When the cart is empty, the total shows zero and the button doesn't appear.",
    );
    assert_proposed_whole("When the user’s cart is empty, the total shows zero.");
}
