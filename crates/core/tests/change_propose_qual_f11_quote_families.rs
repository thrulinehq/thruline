//! QUAL-F11-LAUNCH-R3 — quotation marks pair across straight and typographic
//! styles, and any fragment with an unmatched quote mark is kept as a note,
//! never proposed.
//!
//! Round 2's adversary (`gates/adversary-QUAL-F11-LAUNCH-R2`, attacks 1–4)
//! found that a quotation opened with one style of mark and closed with the
//! other ('…’, "…”, ‘…') never formed a pair, so a `;` inside it still cut
//! the sentence; and that a stray closing ’ was invisible to the unmatched
//! check, so "The retry counter stores zero’ after the job ends." — half of
//! what somebody wrote, with a dangling quote mark — was proposed.
//!
//! The rule these tests hold:
//! - Two families: single (' ‘ ’) and double (" “ ”). Within a family any
//!   mark closes a quotation that any mark opened.
//! - A single mark with a letter or digit on both sides (don’t, it's, the
//!   service’s) is an apostrophe, not a quote mark.
//! - A single mark straight after an `s` and before a space or the end, with
//!   no single quotation open (the users' cart), is a plural possessive, not
//!   a quote mark.
//! - Every other mark that neither opens nor closes a quotation is unmatched,
//!   and so is a quotation left open: the candidate holding it is kept as a
//!   fragment note, verbatim, and never proposed.
//!
//! Every id is synthetic (`cccccccc-…`) and no fixture here holds private
//! content.

use thruline_core::change::{self, ChangeRequest, NewChange, RequestSource};
use thruline_core::change_propose::{self, ProfileEnvironment, ProposalView, ShapeRefusal};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000f0ac";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000cfac";

fn proposals_for(purpose: &str) -> ProposalView {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, '/tmp/synthetic-f11-r3', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
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

fn assert_proposed_whole(ask: &str) {
    let view = proposals_for(ask);
    assert_eq!(
        criterion_texts(&view),
        vec![ask.to_string()],
        "{ask:?} must be proposed whole; notes: {:?}",
        note_texts(&view)
    );
    assert!(
        view.notes.is_empty(),
        "{ask:?}: nothing is left over: {:?}",
        note_texts(&view)
    );
}

/// The ask holds an unmatched mark: nothing is proposed, and the whole ask is
/// kept verbatim as a fragment note with its plain reason.
fn assert_kept_as_fragment(ask: &str) {
    let view = proposals_for(ask);
    assert!(
        view.criteria.is_empty(),
        "{ask:?}: an unmatched quote mark was proposed: {:?}",
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

/// Attack 1: straight ' opens, typographic ’ closes.
#[test]
fn attack_1_straight_single_open_typographic_close_is_one_sentence() {
    assert_proposed_whole("The service shows 'delivered; Retry disabled’ when the job ends.");
}

/// Attack 2: straight " opens, typographic ” closes.
#[test]
fn attack_2_straight_double_open_typographic_close_is_one_sentence() {
    assert_proposed_whole("The service shows \"delivered; Retry disabled” when the job ends.");
}

/// Attack 3: typographic ‘ opens, straight ' closes.
#[test]
fn attack_3_typographic_single_open_straight_close_is_one_sentence() {
    assert_proposed_whole("The service shows ‘delivered; Retry disabled' when the job ends.");
}

/// Attack 4: the second half of a mismatched quotation had the shape of a
/// criterion and was proposed, dangling ’ and all. The whole sentence is
/// proposed instead, and nothing carrying half a quotation is.
#[test]
fn attack_4_half_a_mismatched_quotation_is_never_proposed() {
    let ask = "The panel shows 'delivered; The retry counter stores zero’ after the job ends.";
    let view = proposals_for(ask);
    let texts = criterion_texts(&view);
    assert!(
        !texts
            .iter()
            .any(|t| t.starts_with("The retry counter stores zero")),
        "half a sentence was proposed: {texts:?}"
    );
    assert_proposed_whole(ask);
}

/// Every opening mark of a family pairs with every closing mark of the same
/// family: nine combinations for singles, nine for doubles.
#[test]
fn any_mark_of_a_family_closes_a_quotation_any_mark_of_it_opened() {
    for family in [['\'', '‘', '’'], ['"', '“', '”']] {
        for open in family {
            for close in family {
                assert_proposed_whole(&format!(
                    "The service shows {open}delivered; Retry disabled{close} when the job ends."
                ));
            }
        }
    }
}

/// Marks of different families never pair: a ' opened and a ” closed leaves
/// both unmatched, so nothing is proposed.
#[test]
fn marks_of_different_families_do_not_pair() {
    assert_kept_as_fragment("The service shows 'delivered when the job ends” twice.");
    assert_kept_as_fragment("The service shows “delivered when the job ends’ twice.");
}

/// A stray closing mark, of either family and either style, with nothing
/// open: the candidate is kept as a note, never proposed — even when it has
/// every other shape of a criterion.
#[test]
fn a_stray_closing_mark_of_either_family_is_never_proposed() {
    for ask in [
        "The retry counter stores zero’ after the job ends.",
        "The retry counter stores zero' after the job ends.",
        "The retry counter stores zero” after the job ends.",
        "The retry counter stores zero\" after the job ends.",
        "The retry counter stores ‘ zero after the job ends.",
        "The retry counter stores ’ zero after the job ends.",
    ] {
        assert_kept_as_fragment(ask);
    }
}

/// A quotation opened by any mark and never closed is kept as a note.
#[test]
fn an_opened_quotation_never_closed_is_never_proposed() {
    for open in ['\'', '‘', '’', '"', '“', '”'] {
        assert_kept_as_fragment(&format!(
            "The retry counter stores {open}zero after the job ends."
        ));
    }
}

/// An apostrophe inside a word, and a plural possessive, are not quote marks
/// — in either style, and inside a quotation as well as outside one.
#[test]
fn an_apostrophe_is_not_a_quote_mark_in_either_style() {
    for ask in [
        "When the cart is empty, the total shows zero and the button don’t appear.",
        "When the cart is empty, the total shows zero and it's greyed out.",
        "When the service’s cart is empty, the total shows zero.",
        "When the service's cart is empty, the total shows zero.",
        "When the users’ cart is empty, the total shows zero.",
        "The page shows 'the service’s state; Retry later' when the job fails.",
        "The page shows ‘it's saved; Retry later’ when the job fails.",
    ] {
        assert_proposed_whole(ask);
    }
}
