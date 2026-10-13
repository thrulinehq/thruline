//! C30 R-LEDGER — objections: "This is wrong…", a person's reason against one
//! of the agent's statements, kept beside the statement.
//!
//! The sentences these tests hold:
//!
//!   1. an objection keeps the reason exactly as typed and the SHA-256 of the
//!      sentence the project's own index gives for the id, computed by the
//!      module, never supplied by a caller, and serves the words masked;
//!   2. an unknown statement, an empty reason, a reason over 2,000 characters
//!      and a name-less act are refused, and a refusal writes nothing; the
//!      statement ids the project serves are exactly the ones it accepts;
//!   3. the thread view serves `objections[]` on the claim that was objected
//!      to and on no other, and with those keys taken out the view is
//!      byte-identical to the view without any objection — the verdict, the
//!      counts, `whose_move` and the project view do not move;
//!   4. `matches_sentence` is false when the stored digest is not the digest of
//!      the sentence the extractor gives now, so an objection is never carried
//!      onto a different sentence;
//!   5. a `withdrawn` row (the table can say it; no route does) takes its
//!      objection out of what is served;
//!   6. nothing but the thread view's reader names the table — the verdict
//!      rule, the speaker and the stop gate do not.
//!
//! Every id is synthetic or comes from the committed HQ fixtures; no private
//! content is used.

mod scratch;

use std::path::{Path, PathBuf};

use rusqlite::params;
use scratch::TempTree;
use serde_json::Value;
use thruline_core::chain::sha256_hex;
use thruline_core::hq;
use thruline_core::ingest::ingest;
use thruline_core::statement_objection::{self as objection, ClaimRef, Refusal, MAX_REASON_CHARS};
use thruline_core::Ledger;

const NOW: &str = "2026-10-07T00:00:00Z";
const AT: &str = "2026-10-07T12:00:00Z";
const CANARY: &str = "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq")
}

struct World {
    _tmp: TempTree,
    ledger: Ledger,
    index: hq::Index,
    now: i64,
    /// A thread that holds claims, and one that does not hold the first.
    thread: String,
    other_thread: String,
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
    let now = hq::parse_ms(NOW).expect("instant");
    let threads = index.view(now).threads;
    let with_claims: Vec<String> = threads
        .iter()
        .filter(|t| {
            !index
                .thread_view(&ledger, &t.id, now)
                .expect("view")
                .expect("thread")
                .claims
                .is_empty()
        })
        .map(|t| t.id.clone())
        .collect();
    assert!(
        with_claims.len() >= 2,
        "the fixture needs two threads that hold claims: one to object on, one to stay as it was"
    );
    World {
        _tmp: tmp,
        ledger,
        index,
        now,
        thread: with_claims[0].clone(),
        other_thread: with_claims[1].clone(),
    }
}

impl World {
    fn thread_json(&self, thread: &str) -> Value {
        serde_json::to_value(
            self.index
                .thread_view(&self.ledger, thread, self.now)
                .expect("view")
                .expect("thread"),
        )
        .expect("json")
    }
    fn thread_bytes(&self, thread: &str) -> Vec<u8> {
        serde_json::to_vec(
            &self
                .index
                .thread_view(&self.ledger, thread, self.now)
                .expect("view")
                .expect("thread"),
        )
        .expect("json")
    }
    /// The id of the first claim of `thread`, and the sentence it gives.
    fn first_claim(&self, thread: &str) -> (String, ClaimRef) {
        let view = self.thread_json(thread);
        let id = view["claims"][0]["id"]
            .as_str()
            .expect("claim id")
            .to_string();
        let claim = self.index.claim_sentence(&id).expect("the index serves it");
        (id, claim)
    }
    fn rows(&self) -> i64 {
        self.ledger
            .conn()
            .query_row("SELECT COUNT(*) FROM statement_objections", [], |r| {
                r.get(0)
            })
            .expect("count")
    }
    fn object(&self, claim: &ClaimRef, reason: &str) -> objection::ObjectionItem {
        objection::object(&self.ledger, claim, "sam", reason, AT)
            .expect("the ledger answers")
            .expect("kept")
    }
}

/// `value` with every `objections` key removed from every claim of a thread view.
fn without_objections(mut view: Value) -> Value {
    if let Some(claims) = view["claims"].as_array_mut() {
        for claim in claims {
            if let Some(map) = claim.as_object_mut() {
                map.remove("objections");
            }
        }
    }
    view
}

/* ── 1. the reason as typed, the digest the module computed ────────────── */

#[test]
fn an_objection_keeps_the_reason_as_typed_and_the_sentences_digest_and_serves_it_masked() {
    let w = world();
    let (id, claim) = w.first_claim(&w.thread);
    let reason = format!("  The test never ran. {CANARY}\n\n");
    let kept = objection::object(&w.ledger, &claim, "  sam  ", &reason, AT)
        .expect("the ledger answers")
        .expect("kept");

    assert_eq!(kept.actor, "sam", "the name is kept trimmed");
    assert_eq!(kept.at, AT);
    assert!(kept.matches_sentence);
    assert!(
        !kept.reason.contains(CANARY),
        "the answer to the writer is masked: {:?}",
        kept.reason
    );

    // The table keeps the words exactly as typed; the digest is the SHA-256 of
    // the sentence the INDEX gave, computed by the module.
    let (act, statement_id, event_id, ordinal, digest, actor, stored, withdraws): (
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
        Option<String>,
    ) = w
        .ledger
        .conn()
        .query_row(
            "SELECT act, statement_id, event_id, claim_ordinal, statement_sha256, actor, reason,
                    withdraws
               FROM statement_objections WHERE id=?1",
            params![kept.id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .expect("the row");
    assert_eq!(act, "objected");
    assert_eq!(statement_id, id);
    assert_eq!(statement_id, format!("{event_id}#{ordinal}"));
    assert_eq!(digest, sha256_hex(claim.sentence.as_bytes()));
    assert_eq!(digest.len(), 64);
    assert_eq!(actor, "sam");
    assert_eq!(stored, reason, "kept exactly as typed");
    assert_eq!(withdraws, None);
    assert_eq!(w.rows(), 1);

    // Served masked in the thread view too, and nothing but a digest of the
    // sentence is kept: the sentence itself is not in the table.
    let view = w.thread_json(&w.thread).to_string();
    assert!(!view.contains(CANARY), "the thread view served the secret");
    let all: String = w
        .ledger
        .conn()
        .query_row(
            "SELECT group_concat(statement_id||statement_sha256||reason, '|') FROM statement_objections",
            [],
            |r| r.get(0),
        )
        .expect("rows");
    assert!(
        !all.contains(&claim.sentence),
        "the table keeps a digest, not the sentence"
    );
}

/* ── 2. refusals write nothing; the ids served are the ids accepted ────── */

#[test]
fn what_cannot_be_kept_is_refused_by_name_and_writes_nothing() {
    let w = world();
    let (_, claim) = w.first_claim(&w.thread);
    let say = |claim: &ClaimRef, actor: &str, reason: &str| {
        objection::object(&w.ledger, claim, actor, reason, AT).expect("the ledger answers")
    };

    assert_eq!(say(&claim, "sam", "").unwrap_err(), Refusal::EmptyReason);
    assert_eq!(
        say(&claim, "sam", " \n\t ").unwrap_err(),
        Refusal::EmptyReason
    );
    assert_eq!(say(&claim, "  ", "wrong").unwrap_err(), Refusal::NoActor);
    let over = "x".repeat(MAX_REASON_CHARS + 1);
    assert_eq!(
        say(&claim, "sam", &over).unwrap_err(),
        Refusal::TooLong {
            chars: MAX_REASON_CHARS + 1
        }
    );
    // A statement whose event is not in this store, and an id that does not
    // spell the event and ordinal beside it.
    let stray = ClaimRef {
        statement_id: "no-such-event#0".to_string(),
        event_id: "no-such-event".to_string(),
        ordinal: 0,
        sentence: "A sentence.".to_string(),
    };
    assert_eq!(
        say(&stray, "sam", "wrong").unwrap_err(),
        Refusal::NoSuchStatement
    );
    let crossed = ClaimRef {
        ordinal: claim.ordinal + 1,
        ..claim.clone()
    };
    assert_eq!(
        say(&crossed, "sam", "wrong").unwrap_err(),
        Refusal::NoSuchStatement
    );
    assert_eq!(w.rows(), 0, "a refusal writes nothing");

    // The limit is inclusive: exactly 2,000 characters is kept, counted in
    // characters and not bytes.
    let edge = "é".repeat(MAX_REASON_CHARS);
    assert!(say(&claim, "sam", &edge).is_ok());
    assert_eq!(w.rows(), 1);

    // Every refusal has a code and a sentence that says nothing was saved.
    for refusal in [
        Refusal::NoSuchStatement,
        Refusal::NoActor,
        Refusal::EmptyReason,
        Refusal::TooLong { chars: 2001 },
    ] {
        assert!(refusal.plain().ends_with("Nothing was saved."));
        assert!(!refusal.code().is_empty());
    }
}

#[test]
fn the_statements_a_project_serves_are_exactly_the_ones_it_accepts() {
    let w = world();
    let served = w.index.statements();
    assert!(!served.claims.is_empty());
    for row in &served.claims {
        let claim = w.index.claim_sentence(&row.id).unwrap_or_else(|| {
            panic!(
                "the statements route serves {} but it is not accepted",
                row.id
            )
        });
        assert_eq!(claim.statement_id, row.id);
        assert!(row.id.starts_with(&claim.event_id));
        assert!(row.id.ends_with(&format!("#{}", claim.ordinal)));
    }
    for thread in w.index.view(w.now).threads {
        for claim in w.thread_json(&thread.id)["claims"]
            .as_array()
            .expect("claims")
        {
            let id = claim["id"].as_str().expect("id");
            let got = w.index.claim_sentence(id).expect("accepted");
            assert_eq!(
                got.sentence,
                claim["text"].as_str().expect("text"),
                "the sentence the index gives is the sentence the thread shows"
            );
        }
    }

    let (id, claim) = w.first_claim(&w.thread);
    let (event, n) = id.rsplit_once('#').expect("an id spells event#n");
    let first_prompt = w
        .index
        .thread_identity(&w.thread)
        .expect("identity")
        .first_prompt_event_id;
    for bad in [
        String::new(),
        "nope".to_string(),
        "#0".to_string(),
        format!("{event}#"),
        format!("{event}#-1"),
        format!("{event}#+{n}"),
        format!("{event}#0{n}"),
        format!("{event}#{}", claim.ordinal + 1000),
        format!("{event}#{n}#{n}"),
        format!("x{event}#{n}"),
        // an event that exists but holds no claim
        format!("{first_prompt}#0"),
    ] {
        assert!(
            w.index.claim_sentence(&bad).is_none(),
            "{bad:?} must not be accepted"
        );
    }
}

/* ── 3. the thread view: beside the verdict, never instead of it ───────── */

#[test]
fn the_thread_view_serves_objections_beside_an_unchanged_verdict() {
    let w = world();
    let (id, claim) = w.first_claim(&w.thread);
    let before = w.thread_json(&w.thread);
    let before_bytes = w.thread_bytes(&w.thread);
    let other_before = w.thread_bytes(&w.other_thread);
    let statements_before = serde_json::to_vec(&w.index.statements()).expect("json");
    let project_before = serde_json::to_vec(&w.index.view(w.now)).expect("json");
    assert!(
        !String::from_utf8_lossy(&before_bytes).contains("objections"),
        "a thread nobody objected to serves no such key"
    );

    let first = w.object(&claim, "The test never ran.");
    let second = objection::object(&w.ledger, &claim, "robin", "Nor did the build.", AT)
        .expect("ledger")
        .expect("kept");

    let after = w.thread_json(&w.thread);
    let claims = after["claims"].as_array().expect("claims");
    for c in claims {
        if c["id"] == id.as_str() {
            let served = c["objections"].as_array().expect("objections[]");
            assert_eq!(served.len(), 2, "both objections, oldest first");
            assert_eq!(served[0]["id"], first.id.as_str());
            assert_eq!(served[0]["actor"], "sam");
            assert_eq!(served[0]["at"], AT);
            assert_eq!(served[0]["reason"], "The test never ran.");
            assert_eq!(served[0]["matches_sentence"], true);
            assert_eq!(served[1]["id"], second.id.as_str());
            assert_eq!(served[1]["actor"], "robin");
            // exactly these five keys and no verdict-shaped one
            let mut keys: Vec<&str> = served[0]
                .as_object()
                .expect("object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            assert_eq!(keys, ["actor", "at", "id", "matches_sentence", "reason"]);
        } else {
            assert!(
                c.get("objections").is_none(),
                "an objection reaches only the statement it was about"
            );
        }
    }

    // The verdict, the basis, every count and the whole rest of the view: with
    // the `objections` keys taken out, the view is the one served before.
    assert_ne!(after, before);
    assert_eq!(without_objections(after.clone()), before);
    assert_eq!(
        w.thread_bytes(&w.other_thread),
        other_before,
        "another thread is byte-identical"
    );
    // The project view, and the statements route, read neither table nor rows.
    assert_eq!(
        serde_json::to_vec(&w.index.view(w.now)).expect("json"),
        project_before
    );
    assert_eq!(
        serde_json::to_vec(&w.index.statements()).expect("json"),
        statements_before
    );
    assert_eq!(
        serde_json::to_vec(&w.index.view_told(w.now, &Default::default())).expect("json"),
        project_before,
        "whose_move and every count are the project view's, objections or none"
    );
}

/* ── 4. the sentence moved: the objection stays with its wording ───────── */

#[test]
fn an_objection_never_follows_a_different_sentence() {
    let w = world();
    let (id, claim) = w.first_claim(&w.thread);
    // Objected to when the extractor gave another wording for this id.
    let older = ClaimRef {
        sentence: format!("{} (as an older extractor read it)", claim.sentence),
        ..claim.clone()
    };
    let stale = w.object(&older, "I meant the older wording.");
    assert!(
        stale.matches_sentence,
        "it matches the sentence it was given"
    );
    let fresh = w.object(&claim, "And this one.");

    let view = w.thread_json(&w.thread);
    let served = view["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["id"] == id.as_str())
        .expect("the claim")["objections"]
        .as_array()
        .expect("objections")
        .clone();
    let by_id = |want: &str| served.iter().find(|o| o["id"] == want).expect("served");
    assert_eq!(
        by_id(&stale.id)["matches_sentence"],
        false,
        "the sentence changed since: say so"
    );
    assert_eq!(by_id(&fresh.id)["matches_sentence"], true);
    assert_eq!(by_id(&stale.id)["reason"], "I meant the older wording.");
}

/* ── 5. a withdrawn row is not an objection that stands ────────────────── */

#[test]
fn a_withdrawn_row_takes_its_objection_out_of_what_is_served() {
    let w = world();
    let (id, claim) = w.first_claim(&w.thread);
    let before = w.thread_bytes(&w.thread);
    let kept = w.object(&claim, "Wrong.");
    let still = w.object(&claim, "Also wrong.");
    assert_ne!(w.thread_bytes(&w.thread), before);

    // No function here appends a withdrawal (C34 Q16); the table can hold one.
    w.ledger
        .conn()
        .execute(
            "INSERT INTO statement_objections(id, act, statement_id, event_id, claim_ordinal,
                                              statement_sha256, actor, reason, recorded_at,
                                              withdraws)
             VALUES('w1','withdrawn',?1,?2,?3,?4,'sam','',?5,?6)",
            params![
                id,
                claim.event_id,
                claim.ordinal as i64,
                objection::sentence_sha256(&claim.sentence),
                AT,
                kept.id,
            ],
        )
        .expect("the table can say it");

    let standing = objection::standing(&w.ledger).expect("standing");
    let ids: Vec<&str> = standing[&id].iter().map(|o| o.id.as_str()).collect();
    assert_eq!(
        ids,
        [still.id.as_str()],
        "only the objection nobody took back"
    );
    let view = w.thread_json(&w.thread);
    let served = view["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["id"] == id.as_str())
        .expect("claim")["objections"]
        .as_array()
        .expect("objections")
        .clone();
    assert_eq!(served.len(), 1);
    assert_eq!(served[0]["id"], still.id.as_str());
    assert_eq!(
        objection::last_by_event(&w.ledger).expect("last")[&claim.event_id],
        "w1",
        "a withdrawal moves the key a cache holds, too"
    );
}

/* ── 6. who may read the table ─────────────────────────────────────────── */

/// Every `.rs` file under `dir`, relative to the workspace root.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn nothing_but_the_thread_views_reader_names_the_table_and_the_verdict_rule_never_does() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    sources(&root.join("core/src"), &mut files);
    sources(&root.join("cli/src"), &mut files);
    let mut naming: Vec<String> = files
        .iter()
        .filter(|p| {
            std::fs::read_to_string(p)
                .map(|t| t.contains("statement_objection"))
                .unwrap_or(false)
        })
        .map(|p| {
            p.canonicalize()
                .expect("path")
                .strip_prefix(root.canonicalize().expect("root"))
                .expect("under the workspace")
                .display()
                .to_string()
        })
        .collect();
    naming.sort();
    assert_eq!(
        naming,
        [
            "cli/src/uiserve/hq.rs",
            "cli/src/uiserve/person_acts.rs",
            "core/src/hq/statements.rs",
            "core/src/hq/view.rs",
            "core/src/ledger.rs",
            "core/src/lib.rs",
            "core/src/statement_objection.rs",
        ],
        "a file that names the objections table must be one of the seven that may \
         (the speaker, the stop gate, the audit, readiness and the verdict rule are not)"
    );

    // The verdict rule lives in statements.rs beside the lookup; the rule's own
    // body must not name an objection.
    let text = std::fs::read_to_string(root.join("core/src/hq/statements.rs")).expect("read");
    let start = text
        .find("pub(crate) fn judge_thread")
        .expect("judge_thread");
    let end = text[start..]
        .find("\nimpl Index")
        .map_or(text.len(), |n| start + n);
    assert!(
        !text[start..end].to_lowercase().contains("objection"),
        "judge_thread must not read an objection"
    );
}
