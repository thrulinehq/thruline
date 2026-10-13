//! C30 R-LEDGER — telling the agent: a person's copy-for-the-agent and
//! "mark as pasted" acts, kept as record.
//!
//! The sentences these tests hold:
//!
//!   1. a copy keeps the message exactly as written, with the SHA-256 of those
//!      bytes (computed here, never sent), and serves it masked;
//!   2. an empty message, a message over 8,000 characters, a name-less act, an
//!      unknown thread, a mark with no copy and an undo with no mark are
//!      refused, and a refusal writes nothing;
//!   3. a mark refers to a copy on the SAME thread and an undo to a standing
//!      mark on it; a copy cannot be marked twice, a mark cannot be undone
//!      twice, and a copy can be marked again after its mark was undone;
//!   4. the last act of a thread, and whether the thread has had work recorded
//!      since, are read at serve time and never stored — an act stamped to the
//!      second does not count a row logged in that same second as later work;
//!   5. the acts reach the project view through ONE field, `told_agent`: with
//!      the field taken out the view is byte-identical to the view without any
//!      act, and a thread with no act serves no such key;
//!   6. the table has three acts and none that could say a message was sent;
//!   7. the Change that tracks a thread is found by the thread's first work
//!      prompt, the same match "Track this work as a Change" makes.
//!
//! Every id is synthetic or comes from the committed HQ fixtures; no private
//! content is used.

mod scratch;

use std::path::{Path, PathBuf};

use rusqlite::params;
use scratch::TempTree;
use thruline_core::change::{insert_change, ChangeRequest, NewChange, RequestSource};
use thruline_core::change_agent_message::{
    self as told, Act, Anchor, Refusal, ToldAct, MAX_MESSAGE_CHARS,
};
use thruline_core::hq;
use thruline_core::ingest::ingest;
use thruline_core::Ledger;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq")
}

struct World {
    _tmp: TempTree,
    ledger: Ledger,
    project: hq::HqProject,
    thread: String,
    anchor: Anchor,
    ledger_project_id: String,
    other_thread: String,
    other_anchor: Anchor,
}

fn world() -> World {
    let tmp = TempTree::new();
    let mut ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        &[fixtures().join("claude/-work-app")],
        &tmp.path().join("mirrors"),
    )
    .expect("claude ingest");
    let project = hq::hq_projects(&ledger).expect("projects").remove(0);
    let index = hq::index(&ledger, &project).expect("index");
    let view = index.view(hq::parse_ms("2026-10-07T00:00:00Z").expect("instant"));
    assert!(
        view.threads.len() >= 2,
        "the fixture needs two threads to prove an act stays on its own thread"
    );
    let anchor_of = |id: &str| {
        let identity = index.thread_identity(id).expect("identity");
        (
            identity.ledger_project_id.clone(),
            Anchor {
                thread_id: identity.id,
                thread_event_id: identity.first_prompt_event_id,
                change_id: None,
                version_id: identity.current_version_id,
            },
        )
    };
    let (ledger_project_id, anchor) = anchor_of(&view.threads[0].id);
    let (_, other_anchor) = anchor_of(&view.threads[1].id);
    World {
        _tmp: tmp,
        ledger,
        project,
        thread: view.threads[0].id.clone(),
        anchor,
        ledger_project_id,
        other_thread: view.threads[1].id.clone(),
        other_anchor,
    }
}

const AT: &str = "2026-10-07T12:00:00Z";

fn rows(ledger: &Ledger) -> i64 {
    ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_agent_messages", [], |r| {
            r.get(0)
        })
        .expect("count")
}

fn copy(w: &World, message: &str, at: &str) -> told::ActView {
    told::copied(&w.ledger, &w.anchor, "sam", message, at)
        .expect("the ledger answers")
        .expect("a copy with words is kept")
}

/* ── 1. a copy is kept as written, with the digest of those bytes ──────── */

#[test]
fn a_copy_is_kept_exactly_as_written_with_its_digest_and_served_masked() {
    let w = world();
    let canary = format!("ghp_{}", "TLCANARYaaZZ11bbYY22ccXX33ddWW44e");
    let words = format!("  Please re-run the totals test. {canary}\n\n");
    let kept = told::copied(&w.ledger, &w.anchor, "  sam  ", &words, AT)
        .expect("the ledger answers")
        .expect("kept");

    assert_eq!(kept.act, Act::Copied);
    assert_eq!(kept.actor, "sam", "the name is kept trimmed");
    assert_eq!(kept.recorded_at, AT);
    assert_eq!(kept.refers_to, None);
    assert_eq!(kept.thread_id, w.thread);
    assert_eq!(kept.thread_event_id, w.anchor.thread_event_id);
    assert!(
        !kept.message.as_deref().unwrap_or("").contains(&canary),
        "the answer to the writer is masked: {:?}",
        kept.message
    );

    // The table keeps the words exactly as written, and the digest is the
    // SHA-256 of exactly those bytes — computed by the module, not supplied.
    let (stored, digest): (String, String) = w
        .ledger
        .conn()
        .query_row(
            "SELECT message, message_sha256 FROM change_agent_messages",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored, words);
    assert_eq!(digest, thruline_core::chain::sha256_hex(words.as_bytes()));
    assert_eq!(kept.message_sha256.as_deref(), Some(digest.as_str()));

    // Every reader serves it masked.
    let mut record = thruline_core::redact::MaskRecord::default();
    let read = told::read(&w.ledger, &w.thread, &mut record).expect("read");
    assert_eq!(read.len(), 1);
    assert_eq!(read[0], kept);
    assert!(!read[0].message.as_deref().unwrap_or("").contains(&canary));
}

/* ── 2. refusals write nothing ─────────────────────────────────────────── */

#[test]
fn what_cannot_be_kept_is_refused_and_writes_nothing() {
    let w = world();

    for empty in ["", "   ", "\n\t \n"] {
        assert_eq!(
            told::copied(&w.ledger, &w.anchor, "sam", empty, AT).unwrap(),
            Err(Refusal::EmptyMessage),
            "{empty:?}"
        );
    }
    let over = "x".repeat(MAX_MESSAGE_CHARS + 1);
    assert_eq!(
        told::copied(&w.ledger, &w.anchor, "sam", &over, AT).unwrap(),
        Err(Refusal::TooLong {
            chars: MAX_MESSAGE_CHARS + 1
        })
    );
    let before = rows(&w.ledger);
    assert_eq!(before, 0, "every refusal so far wrote nothing");

    assert_eq!(
        told::copied(&w.ledger, &w.anchor, "   ", "hello", AT).unwrap(),
        Err(Refusal::NoActor)
    );
    let stray = Anchor {
        thread_event_id: "no-such-event".to_string(),
        ..w.anchor.clone()
    };
    assert_eq!(
        told::copied(&w.ledger, &stray, "sam", "hello", AT).unwrap(),
        Err(Refusal::NoSuchThread)
    );
    // A mark with no copy to refer to, and an undo with no mark.
    assert_eq!(
        told::marked_pasted(&w.ledger, &w.anchor, "sam", None, AT).unwrap(),
        Err(Refusal::NoCopyToMark)
    );
    assert_eq!(
        told::unmarked_pasted(&w.ledger, &w.anchor, "sam", None, AT).unwrap(),
        Err(Refusal::NoMarkToUndo)
    );
    assert_eq!(rows(&w.ledger), before, "a refusal appends nothing");

    // Characters, not bytes: 8,000 two-byte characters fit.
    let wide = "é".repeat(MAX_MESSAGE_CHARS);
    assert!(told::copied(&w.ledger, &w.anchor, "sam", &wide, AT)
        .unwrap()
        .is_ok());
    assert_eq!(rows(&w.ledger), 1);

    // Every refusal has a plain sentence that says nothing was saved.
    for refusal in [
        Refusal::NoSuchThread,
        Refusal::NoActor,
        Refusal::EmptyMessage,
        Refusal::TooLong { chars: 9000 },
        Refusal::NoCopyToMark,
        Refusal::AlreadyMarked { mark: "m".into() },
        Refusal::NoMarkToUndo,
    ] {
        assert!(
            refusal.plain().contains("Nothing was saved"),
            "{}",
            refusal.plain()
        );
    }
}

/* ── 3. a mark refers to a copy on its own thread ──────────────────────── */

#[test]
fn a_mark_names_a_copy_on_the_same_thread_and_an_undo_names_a_standing_mark() {
    let w = world();
    let first = copy(&w, "first", "2026-10-07T12:00:00Z");
    let second = copy(&w, "second", "2026-10-07T12:01:00Z");

    // By default the latest copy is the one meant.
    let mark = told::marked_pasted(&w.ledger, &w.anchor, "sam", None, "2026-10-07T12:02:00Z")
        .unwrap()
        .expect("marked");
    assert_eq!(mark.act, Act::MarkedPasted);
    assert_eq!(mark.refers_to.as_deref(), Some(second.id.as_str()));
    assert_eq!(mark.message, None, "a mark carries no text");
    assert_eq!(mark.message_sha256, None);

    // The same copy cannot be marked twice; naming it makes no difference.
    for given in [None, Some(second.id.as_str())] {
        assert_eq!(
            told::marked_pasted(&w.ledger, &w.anchor, "sam", given, AT).unwrap(),
            Err(Refusal::AlreadyMarked {
                mark: mark.id.clone()
            })
        );
    }
    // An earlier copy can be marked by naming it.
    let earlier = told::marked_pasted(
        &w.ledger,
        &w.anchor,
        "sam",
        Some(&first.id),
        "2026-10-07T12:03:00Z",
    )
    .unwrap()
    .expect("the first copy, named");
    assert_eq!(earlier.refers_to.as_deref(), Some(first.id.as_str()));

    // A mark is not a copy, and another thread's copy is not this thread's.
    let foreign = told::copied(&w.ledger, &w.other_anchor, "sam", "elsewhere", AT)
        .unwrap()
        .unwrap();
    for given in [mark.id.as_str(), foreign.id.as_str(), "cam_nothing"] {
        assert_eq!(
            told::marked_pasted(&w.ledger, &w.anchor, "sam", Some(given), AT).unwrap(),
            Err(Refusal::NoCopyToMark),
            "{given}"
        );
    }

    // By default the latest standing mark is the one undone.
    let undo = told::unmarked_pasted(&w.ledger, &w.anchor, "sam", None, "2026-10-07T12:04:00Z")
        .unwrap()
        .expect("undone");
    assert_eq!(undo.act, Act::UnmarkedPasted);
    assert_eq!(undo.refers_to.as_deref(), Some(earlier.id.as_str()));
    // That mark is no longer standing; the older one still is.
    assert_eq!(
        told::unmarked_pasted(&w.ledger, &w.anchor, "sam", Some(&earlier.id), AT).unwrap(),
        Err(Refusal::NoMarkToUndo)
    );
    for given in [first.id.as_str(), foreign.id.as_str(), "cam_nothing"] {
        assert_eq!(
            told::unmarked_pasted(&w.ledger, &w.anchor, "sam", Some(given), AT).unwrap(),
            Err(Refusal::NoMarkToUndo),
            "{given}"
        );
    }
    let undone_too = told::unmarked_pasted(&w.ledger, &w.anchor, "sam", Some(&mark.id), AT)
        .unwrap()
        .expect("the other mark, named");
    assert_eq!(undone_too.refers_to.as_deref(), Some(mark.id.as_str()));
    assert_eq!(
        told::unmarked_pasted(&w.ledger, &w.anchor, "sam", None, AT).unwrap(),
        Err(Refusal::NoMarkToUndo),
        "no standing mark is left"
    );

    // After an undo the same copy can be marked again: a new mark, a new row.
    let again = told::marked_pasted(&w.ledger, &w.anchor, "sam", Some(&first.id), AT)
        .unwrap()
        .expect("marked again");
    assert_ne!(again.id, earlier.id);
}

/* ── 4. the last act, and later work, are read not stored ──────────────── */

#[test]
fn the_last_act_of_each_thread_is_read_in_the_order_it_was_appended() {
    let w = world();
    assert!(told::last_acts(&w.ledger).unwrap().is_empty());

    let c = copy(&w, "hello", "2026-10-07T12:00:00Z");
    let last = told::last_acts(&w.ledger).unwrap();
    assert_eq!(last.len(), 1);
    assert_eq!(
        last[&w.thread],
        ToldAct {
            last_act: Act::Copied,
            at: "2026-10-07T12:00:00Z".to_string(),
            version_id: w.anchor.version_id.clone(),
            act_id: c.id.clone(),
        }
    );

    // Two acts in the same second: the later APPENDED one is the last.
    let m = told::marked_pasted(&w.ledger, &w.anchor, "sam", None, "2026-10-07T12:00:00Z")
        .unwrap()
        .unwrap();
    let u = told::unmarked_pasted(&w.ledger, &w.anchor, "sam", None, "2026-10-07T12:00:00Z")
        .unwrap()
        .unwrap();
    let last = told::last_acts(&w.ledger).unwrap();
    assert_eq!(last[&w.thread].last_act, Act::UnmarkedPasted);
    assert_eq!(last[&w.thread].act_id, u.id);
    assert_ne!(m.id, u.id);

    // Another thread's act is its own entry.
    told::copied(&w.ledger, &w.other_anchor, "sam", "there", AT)
        .unwrap()
        .unwrap();
    let last = told::last_acts(&w.ledger).unwrap();
    assert_eq!(last.len(), 2);
    assert_eq!(last[&w.other_thread].last_act, Act::Copied);
}

#[test]
fn later_work_is_whether_the_thread_has_a_row_after_the_act_and_is_never_stored() {
    let w = world();
    let index = hq::index(&w.ledger, &w.project).expect("index");
    let now = hq::parse_ms("2026-10-07T00:00:00Z").unwrap();
    let thread = index
        .view(now)
        .threads
        .into_iter()
        .find(|t| t.id == w.thread)
        .expect("the thread");
    let last = thread.last_activity_at.expect("the thread has activity");
    let last_ms = hq::parse_ms(&last).expect("an instant");
    let secs = |ms: i64| thruline_core::ingest::iso8601_utc(ms.div_euclid(1000));
    let act = |at: String| ToldAct {
        last_act: Act::Copied,
        at,
        version_id: None,
        act_id: "cam_x".to_string(),
    };

    // An act long before the thread's last row: work came after it.
    assert!(
        index
            .told_agent(&w.thread, &act(secs(last_ms - 3_600_000)))
            .expect("served")
            .later_work
    );
    // An act long after the thread's last row: nothing since.
    assert!(
        !index
            .told_agent(&w.thread, &act(secs(last_ms + 3_600_000)))
            .expect("served")
            .later_work
    );
    // An act in the SAME second as the thread's last row is not "before" it: the
    // stamp is to the second, so a row logged in that second is not later work.
    assert!(
        !index
            .told_agent(&w.thread, &act(secs(last_ms)))
            .expect("served")
            .later_work,
        "a row in the act's own second is not later work"
    );
    // One whole second earlier, the row is later.
    assert!(
        index
            .told_agent(&w.thread, &act(secs(last_ms - 1000)))
            .expect("served")
            .later_work
    );
    // An unreadable stamp never claims later work.
    assert!(
        !index
            .told_agent(&w.thread, &act("not a time".to_string()))
            .expect("served")
            .later_work
    );
    assert!(index.told_agent("no-such-thread", &act(secs(0))).is_none());

    // Nothing about it is stored: the table has no such column.
    let columns: Vec<String> = w
        .ledger
        .conn()
        .prepare("SELECT name FROM pragma_table_info('change_agent_messages')")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(!columns.iter().any(|c| c.contains("later")), "{columns:?}");
}

/* ── 5. the acts reach the view through one field ──────────────────────── */

#[test]
fn the_view_with_acts_differs_from_the_view_without_only_by_told_agent() {
    let w = world();
    let index = hq::index(&w.ledger, &w.project).expect("index");
    let now = hq::parse_ms("2026-10-07T00:00:00Z").unwrap();
    let bare = serde_json::to_string(&index.view(now)).unwrap();

    // No acts: byte-identical, and the key is not there.
    let empty = serde_json::to_string(&index.view_told(now, &Default::default())).unwrap();
    assert!(empty == bare, "an empty map is exactly the plain view");
    assert!(!bare.contains("told_agent"));
    let bare: serde_json::Value = serde_json::from_str(&bare).unwrap();

    copy(&w, "hello", "2026-10-07T12:00:00Z");
    told::marked_pasted(&w.ledger, &w.anchor, "sam", None, "2026-10-07T12:01:00Z")
        .unwrap()
        .unwrap();
    let acts = told::last_acts(&w.ledger).unwrap();
    let with = serde_json::to_value(index.view_told(now, &acts)).unwrap();

    let threads = with["threads"].as_array().expect("threads");
    for thread in threads {
        let id = thread["id"].as_str().unwrap();
        if id == w.thread {
            let t = &thread["told_agent"];
            assert_eq!(t["last_act"], "marked_pasted");
            assert_eq!(t["at"], "2026-10-07T12:01:00Z");
            assert_eq!(t["later_work"], false);
            assert_eq!(
                t.as_object().unwrap().keys().cloned().collect::<Vec<_>>(),
                ["act_id", "at", "last_act", "later_work", "version_id"],
                "exactly these keys, and no message text"
            );
        } else {
            assert!(
                thread.get("told_agent").is_none(),
                "a thread with no act serves no told_agent: {id}"
            );
        }
    }

    // With the field taken out, nothing else moved: not whose_move, not a count,
    // not the order, not the project's totals.
    let mut stripped = with.clone();
    for thread in stripped["threads"].as_array_mut().unwrap() {
        thread.as_object_mut().unwrap().remove("told_agent");
    }
    assert!(
        stripped == bare,
        "nothing but told_agent moved: whose_move, counts and order are the same"
    );
    assert!(with != bare, "the field itself is served");
}

/* ── 6. nothing is sent ────────────────────────────────────────────────── */

#[test]
fn there_is_no_act_that_could_say_a_message_was_sent() {
    for word in ["sent", "send", "delivered", "dispatched", "told", "posted"] {
        assert_eq!(Act::parse(word), None, "{word}");
    }
    assert_eq!(Act::parse("copied"), Some(Act::Copied));
    assert_eq!(Act::parse("marked_pasted"), Some(Act::MarkedPasted));
    assert_eq!(Act::parse("unmarked_pasted"), Some(Act::UnmarkedPasted));
    assert!(told::NOTHING_SENT.starts_with("Nothing was sent."));
    for word in ["lie", "cheat", "violate", "ignore", "hallucinat", "trial"] {
        assert!(!told::NOTHING_SENT.to_lowercase().contains(word));
    }

    // And the store itself refuses one.
    let w = world();
    let c = copy(&w, "hello", AT);
    let err = w
        .ledger
        .conn()
        .execute(
            "INSERT INTO change_agent_messages(id, act, thread_id, thread_event_id, actor,
                                               recorded_at, refers_to)
             VALUES('cam_x', 'sent', ?1, ?2, 'sam', ?3, ?4)",
            params![w.thread, w.anchor.thread_event_id, AT, c.id],
        )
        .expect_err("the table has no such act");
    assert!(err.to_string().contains("CHECK"), "{err}");
    // And a row cannot be edited or removed.
    assert!(w
        .ledger
        .conn()
        .execute("UPDATE change_agent_messages SET actor='x'", [])
        .is_err());
    assert!(w
        .ledger
        .conn()
        .execute("DELETE FROM change_agent_messages", [])
        .is_err());
}

/* ── 7. the Change that tracks the thread ──────────────────────────────── */

#[test]
fn the_change_that_tracks_a_thread_is_found_by_its_first_work_prompt() {
    let w = world();
    assert_eq!(
        told::tracking_change(&w.ledger, &w.ledger_project_id, &w.anchor.thread_event_id).unwrap(),
        None
    );
    insert_change(
        &w.ledger,
        &NewChange {
            id: "chg_tracked".to_string(),
            project_id: w.ledger_project_id.clone(),
            opened_at: "2026-10-07T10:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Tracked from this thread".to_string(),
                purpose: String::new(),
                prompt_event_id: Some(w.anchor.thread_event_id.clone()),
                requester: "you".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("a Change that tracks the thread");
    assert_eq!(
        told::tracking_change(&w.ledger, &w.ledger_project_id, &w.anchor.thread_event_id).unwrap(),
        Some("chg_tracked".to_string())
    );
    // Another thread's anchor, or another project, finds nothing.
    assert_eq!(
        told::tracking_change(
            &w.ledger,
            &w.ledger_project_id,
            &w.other_anchor.thread_event_id
        )
        .unwrap(),
        None
    );
    assert_eq!(
        told::tracking_change(&w.ledger, "other-project", &w.anchor.thread_event_id).unwrap(),
        None
    );

    // The act names the Change when one tracks the thread, and is still kept
    // when none does (the foreign thread above).
    let anchor = Anchor {
        change_id: Some("chg_tracked".to_string()),
        ..w.anchor.clone()
    };
    let kept = told::copied(&w.ledger, &anchor, "sam", "hello", AT)
        .unwrap()
        .unwrap();
    assert_eq!(kept.change_id.as_deref(), Some("chg_tracked"));
    let untracked = told::copied(&w.ledger, &w.other_anchor, "sam", "hello", AT)
        .unwrap()
        .unwrap();
    assert_eq!(untracked.change_id, None);
}
