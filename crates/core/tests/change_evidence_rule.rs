//! T-3-1 — the criterion ↔ evidence rule, its negative controls, and the
//! coverage states (`B4.6`, `B5.6`, `B7.5`–`B7.7`, `B10.2`, `Q3`).
//!
//! Every test here is one acceptance criterion of the card. The two that matter
//! most are the ones that assert an *absence*: an unrelated run in a confirmed
//! session must not appear under a criterion, and a test command that exited 0
//! must not satisfy one. Both are the failure the card exists to prevent, and
//! both fail loudly if the rule is loosened.

use rusqlite::Connection;
use thruline_core::change_evidence::{
    act, candidates, class_of_category, coverage, derive_links, known_bases, link_row_count,
    links_for, subject_tokens, tokens_of, Candidate, CoverageState, EvidenceKind, LinkAct,
    LinkBasis, Observed, RunClass,
};
use thruline_core::change_propose::ProfileEnvironment;
use thruline_core::ledger::Ledger;

const AT: &str = "2026-02-01T12:00:00Z";

/// The four criteria every fixture below is judged against, in order.
///
/// 1 is about a named subject and no class; 2 names the tests; 3 names the
/// build; 4 is the restart criterion, which nothing in the record bears on and
/// which the card predeclares must read "no test ran for this".
const CRITERIA: [&str; 4] = [
    "The importer keeps a malformed line when it cannot parse it.",
    "The test suite passes on the packages that changed.",
    "The build stays clean under clippy after the change.",
    "HQ reopens where you left it after a restart.",
];

struct Fixture {
    _dir: tempfile::TempDir,
    ledger: Ledger,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
        // A real folder, because a changed path that is absolute only becomes
        // scope under a root the capture established (T-3-2b). The recorded
        // command keys below deliberately keep their synthetic spelling: they
        // are opaque strings the rule tokenises, not paths it resolves, and
        // keeping them fixed keeps the served snapshot deterministic.
        let root = dir
            .path()
            .canonicalize()
            .unwrap_or_else(|_| dir.path().to_path_buf())
            .display()
            .to_string();
        seed(&ledger, &root);
        Fixture { _dir: dir, ledger }
    }

    fn ledger(&self) -> &Ledger {
        &self.ledger
    }
}

fn seed(ledger: &Ledger, root: &str) {
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p1',?1,'claude_code','Synthetic','2026-01-01T00:00:00Z')",
        rusqlite::params![root],
    )
    .expect("project");
    conn.execute(
        "INSERT INTO workspace_identities(project_id, repo_id, basis, resolved_root,
             resolved_via, sessions_sampled, observed_at)
         VALUES('p1','repo','git',?1,'recorded_cwd',1,'2026-01-01T00:00:00Z')",
        rusqlite::params![root],
    )
    .expect("workspace identity");
    for (id, path) in [
        ("s1", "crates/core/src/importer.rs"),
        ("s2", "crates/core/src/other.rs"),
    ] {
        conn.execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1,'p1',?2,'2026-01-01T00:00:00Z')",
            rusqlite::params![id, format!("/tmp/synthetic/{id}.jsonl")],
        )
        .expect("session");
        conn.execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES(?1,'p1',?2,1,'2026-01-01T00:10:00Z','tool_use','Edit',?3,
                    '/tmp/synthetic/x.jsonl',1,'aa','','bb')",
            rusqlite::params![
                format!("e_{id}"),
                id,
                format!("{{\"file_path\":\"{root}/{path}\"}}")
            ],
        )
        .expect("event");
    }
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
             request_source)
         VALUES('c1','p1','2026-01-02T09:00:00Z','Keep malformed lines',
                'The importer should keep what it cannot parse.','sam','person')",
        [],
    )
    .expect("change");
    accept_scope(ledger, &CRITERIA, &["cargo-test", "clippy"]);
    // s1 is confirmed by a person; s2 is a machine suggestion and stays one.
    conn.execute(
        "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
             basis, actor, asserted_at, reason)
         VALUES('a1','c1','s1','confirmed','user','sam','2026-01-02T10:00:00Z','mine')",
        [],
    )
    .expect("confirm");
    conn.execute(
        "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
             basis, actor, asserted_at, reason)
         VALUES('a2','c1','s2','suggested','timing',NULL,'2026-01-02T10:00:00Z',NULL)",
        [],
    )
    .expect("suggest");
}

fn accept_scope(ledger: &Ledger, criteria: &[&str], profile: &[&str]) {
    let json = |v: &[&str]| serde_json::to_string(v).expect("json");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES('c1',1,'2026-01-02T09:30:00Z',?1,'[]',?2,'[]',?3,'digest')",
            rusqlite::params![json(criteria), json(&["crates/**"]), json(profile)],
        )
        .expect("scope");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
                 accepted_at, rationale)
             VALUES('c1',1,'sam','2026-01-02T09:40:00Z','this is what counts')",
            [],
        )
        .expect("acceptance");
}

/// One `evidence_bindings` row, as the recorder writes them.
#[allow(clippy::too_many_arguments)]
fn add_run(
    ledger: &Ledger,
    id: &str,
    session: &str,
    class: &str,
    invocation: &str,
    exit: Option<i64>,
    signal: Option<i64>,
    complete: bool,
) {
    let capture = if complete {
        "{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10}"
    } else {
        "{\"kind\":\"truncated\",\"reason\":\"over the cap\"}"
    };
    let doc = format!("{{\"stdout\":{capture},\"stderr\":{capture}}}");
    let seq: i64 = ledger
        .conn()
        .query_row(
            "SELECT COALESCE(MAX(seq),-1)+1 FROM evidence_bindings",
            [],
            |r| r.get(0),
        )
        .expect("seq");
    ledger
        .conn()
        .execute(
            "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
                 scope_key, root, repository_identity, pre_identity, post_identity,
                 exit_code, term_signal, bound_at, source_session, doc, doc_sha256)
             VALUES(?1,?2,'2026-01-03T09:00:00Z',1,?3,?4,'/tmp/synthetic','repo',
                    'pre','post',?5,?6,?7,?8,?9,'ddd')",
            rusqlite::params![
                id,
                seq,
                class,
                format!("bash:/tmp/synthetic:{invocation}"),
                exit,
                signal,
                format!("2026-01-03T0{}:00:00Z", (seq % 9) + 1),
                session,
                doc,
            ],
        )
        .expect("binding");
}

fn env_without_tools() -> ProfileEnvironment {
    ProfileEnvironment::empty()
}

fn env_with_tools() -> ProfileEnvironment {
    let mut env = ProfileEnvironment::empty();
    env.tools_present.insert("cargo".to_string());
    env.tools_present.insert("cargo-clippy".to_string());
    env
}

fn state_of(view: &thruline_core::change_evidence::CoverageView, index: u32) -> CoverageState {
    view.criteria
        .iter()
        .find(|c| c.index == index)
        .map(|c| c.state)
        .unwrap_or_else(|| panic!("no criterion {index}"))
}

// ---------------------------------------------------------------------------
// The rule, per basis
// ---------------------------------------------------------------------------

#[test]
fn the_profile_declares_a_check_for_the_criterions_class() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_test",
        "s1",
        "test",
        "cargo test",
        Some(0),
        None,
        true,
    );
    let report = derive_links(f.ledger(), "c1", AT).expect("derive");
    assert!(report.inserted > 0, "the declared link must be written");

    let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    let two = view
        .criteria
        .iter()
        .find(|c| c.index == 2)
        .expect("criterion 2");
    let bases: Vec<LinkBasis> = two.items.iter().map(|i| i.basis).collect();
    assert_eq!(bases, vec![LinkBasis::Declared], "{:?}", two.items);
    assert_eq!(two.items[0].detail.as_deref(), Some("cargo-test"));

    // And it declared it for THAT criterion only: criterion 4 names no class.
    let four = view
        .criteria
        .iter()
        .find(|c| c.index == 4)
        .expect("criterion 4");
    assert!(four.items.is_empty(), "{:?}", four.items);
}

#[test]
fn the_evidences_own_typed_name_carries_the_criterions_subject() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_named",
        "s1",
        "unclassified",
        "cargo run --bin importer -- fixtures/malformed.jsonl",
        Some(0),
        None,
        true,
    );
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    let one = view
        .criteria
        .iter()
        .find(|c| c.index == 1)
        .expect("criterion 1");
    assert_eq!(one.items.len(), 1, "{:?}", one.items);
    assert_eq!(one.items[0].basis, LinkBasis::Named);
    assert_eq!(one.items[0].detail.as_deref(), Some("importer"));
}

#[test]
fn the_subject_token_comes_from_the_subject_and_short_words_are_dropped() {
    let tokens = subject_tokens("The importer keeps a malformed line when it cannot parse it.");
    assert!(tokens.contains(&"importer".to_string()), "{tokens:?}");
    assert!(
        !tokens.iter().any(|t| t.chars().count() < 4),
        "a token that matches every command line is a timing basis in disguise: {tokens:?}"
    );
    assert!(
        !tokens.contains(&"the".to_string()),
        "stopwords never link: {tokens:?}"
    );
}

// ---------------------------------------------------------------------------
// The negative controls
// ---------------------------------------------------------------------------

/// B.11 — the whole point of the card. A green run in a session the person
/// confirmed, that has nothing to do with any criterion, must not appear under
/// one.
#[test]
fn an_unrelated_run_in_a_confirmed_session_does_not_link() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_unrelated",
        "s1",
        "unclassified",
        "ls -la",
        Some(0),
        None,
        true,
    );
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    for criterion in &view.criteria {
        assert!(
            criterion.items.is_empty(),
            "criterion {} took an unrelated run: {:?}",
            criterion.index,
            criterion.items
        );
    }
    assert_eq!(link_row_count(f.ledger(), "c1").expect("count"), 0);
}

/// The same instant, the same session, the same everything except a reason —
/// timing is not a basis and the store has no word for it.
#[test]
fn timing_is_not_a_basis_the_store_will_accept() {
    assert!(!known_bases().contains("timing"));
    let f = Fixture::new();
    // The database's own words, read back. A CHECK that gained a fifth basis
    // would not be caught by the insert below — a `timing` row still trips the
    // actor rule — so the vocabulary is asserted against the schema itself.
    let sql: String = f
        .ledger()
        .conn()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table'
              AND name='change_criterion_evidence'",
            [],
            |row| row.get(0),
        )
        .expect("the table must be there");
    assert!(
        sql.contains("basis IN ('declared','named','manual','not-bearing')"),
        "the basis vocabulary changed: {sql}"
    );
    assert!(!sql.contains("'timing'"), "timing became a basis: {sql}");
    let err = f
        .ledger()
        .conn()
        .execute(
            "INSERT INTO change_criterion_evidence(id, change_id, scope_version,
                 criterion_index, evidence_kind, evidence_id, session_id, basis,
                 detail, actor, asserted_at, reason)
             VALUES('x','c1',1,1,'check_run','eb','s1','timing',NULL,NULL,?1,NULL)",
            rusqlite::params![AT],
        )
        .expect_err("the database must refuse a timing basis");
    assert!(
        err.to_string().to_lowercase().contains("constraint"),
        "{err}"
    );
}

#[test]
fn a_run_from_work_that_was_only_suggested_never_links() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_s2",
        "s2",
        "test",
        "cargo test",
        Some(0),
        None,
        true,
    );
    let pool = candidates(f.ledger(), "c1").expect("candidates");
    assert!(
        pool.is_empty(),
        "a suggestion is not a confirmation: {pool:?}"
    );
    derive_links(f.ledger(), "c1", AT).expect("derive");
    assert_eq!(link_row_count(f.ledger(), "c1").expect("count"), 0);
}

#[test]
fn a_finding_or_an_observation_is_never_linked_by_the_rule() {
    // Only a check run has a typed name field the rule may read. Stated as a
    // property of `links_for`, over a candidate list a store cannot argue with.
    let pool = vec![
        Candidate {
            execution: thruline_core::run_execution::ExecutionState::NotRun,
            failure_line: None,
            evidence_class: None,
            kind: EvidenceKind::Finding,
            id: "sf1".into(),
            session_id: "s1".into(),
            name: "importer".into(),
            class: RunClass::Test,
            observed: Observed::Loose,
            checked_nothing: false,
            recorded_at: AT.into(),
        },
        Candidate {
            execution: thruline_core::run_execution::ExecutionState::NotRun,
            failure_line: None,
            evidence_class: None,
            kind: EvidenceKind::Observation,
            id: "ob1".into(),
            session_id: "s1".into(),
            name: "importer".into(),
            class: RunClass::Test,
            observed: Observed::Couldnt,
            checked_nothing: true,
            recorded_at: AT.into(),
        },
    ];
    let criteria: Vec<String> = CRITERIA.iter().map(|c| c.to_string()).collect();
    let profile = vec!["cargo-test".to_string()];
    assert!(links_for(&criteria, &profile, &pool).is_empty());
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

#[test]
fn the_rule_replays_to_the_same_rows() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_test",
        "s1",
        "test",
        "cargo test",
        Some(0),
        None,
        true,
    );
    add_run(
        f.ledger(),
        "eb_named",
        "s1",
        "unclassified",
        "cargo run --bin importer",
        Some(0),
        None,
        true,
    );
    let first = derive_links(f.ledger(), "c1", AT).expect("derive");
    let rows_after_first = dump(f.ledger());
    // A different clock, deliberately: a rule's output must not depend on when
    // it was asked.
    let second = derive_links(f.ledger(), "c1", "2027-09-09T09:09:09Z").expect("derive again");
    assert_eq!(second.inserted, 0, "a replay must write nothing new");
    assert_eq!(second.considered, first.considered);
    assert_eq!(dump(f.ledger()), rows_after_first);
}

fn dump(ledger: &Ledger) -> Vec<String> {
    let conn: &Connection = ledger.conn();
    let mut statement = conn
        .prepare(
            "SELECT id, criterion_index, evidence_kind, evidence_id, basis,
                    COALESCE(detail,''), COALESCE(actor,''), asserted_at
               FROM change_criterion_evidence ORDER BY id",
        )
        .expect("prepare");
    statement
        .query_map([], |row| {
            Ok(format!(
                "{}|{}|{}|{}|{}|{}|{}|{}",
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .expect("query")
        .filter_map(|r| r.ok())
        .collect()
}

// ---------------------------------------------------------------------------
// Coverage states
// ---------------------------------------------------------------------------

#[test]
fn a_criterion_with_nothing_behind_it_says_no_test_ran_for_this() {
    let f = Fixture::new();
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    assert_eq!(state_of(&view, 4), CoverageState::None);
    assert_eq!(
        view.criteria[3].state_slug, "coverage_no_test_ran",
        "the sentence is looked up, never built in the engine"
    );
}

#[test]
fn a_criterion_whose_check_cannot_run_here_says_which_tool_is_missing() {
    let f = Fixture::new();
    // Nothing installed. The accepted profile names `cargo-test`, whose tool is
    // `cargo`, and the criterion about the tests has nothing linked.
    let view = coverage(f.ledger(), "c1", &env_without_tools()).expect("coverage");
    assert_eq!(state_of(&view, 2), CoverageState::AbsentTool);
    let two = &view.criteria[1];
    assert_eq!(two.absent_tool.as_deref(), Some("cargo"));
    assert_eq!(two.state_slug, "coverage_tool_absent");
    // And a criterion of no class never borrows that sentence.
    assert_eq!(state_of(&view, 4), CoverageState::None);
}

/// `Q3`, and the arm that looks like a win. A test command that exited 0 is a
/// command that exited 0; this store holds no count of what it ran, so it
/// cannot rule out zero tests or every test skipped.
#[test]
fn a_test_run_that_exited_zero_satisfies_nothing() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_test",
        "s1",
        "test",
        "cargo test",
        Some(0),
        None,
        true,
    );
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    assert_eq!(state_of(&view, 2), CoverageState::RanNothing);
    let two = &view.criteria[1];
    assert_eq!(two.state_slug, "coverage_ran_checked_nothing");
    assert_eq!(two.items.len(), 1, "the run is SHOWN, not hidden");
    assert!(!two.items[0].bears);
    assert_eq!(two.items[0].observed, Observed::Couldnt);
    assert_eq!(two.items[0].note_slug, Some("item_checked_nothing"));
}

#[test]
fn a_run_that_was_interrupted_or_lost_its_output_satisfies_nothing() {
    for (id, exit, signal, complete) in [
        ("eb_signal", None, Some(9), true),
        ("eb_noexit", None, None, true),
        ("eb_truncated", Some(0), None, false),
    ] {
        let f = Fixture::new();
        add_run(
            f.ledger(),
            id,
            "s1",
            "build",
            "cargo clippy",
            exit,
            signal,
            complete,
        );
        derive_links(f.ledger(), "c1", AT).expect("derive");
        let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
        assert_eq!(state_of(&view, 3), CoverageState::RanNothing, "{id}");
    }
}

#[test]
fn a_build_check_that_exited_zero_is_evidence_and_a_failing_one_is_a_problem() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_lint",
        "s1",
        "build",
        "cargo clippy",
        Some(0),
        None,
        true,
    );
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    assert_eq!(state_of(&view, 3), CoverageState::Evidence);
    assert_eq!(view.criteria[2].items[0].observed, Observed::Proven);

    let g = Fixture::new();
    add_run(
        g.ledger(),
        "eb_lint",
        "s1",
        "build",
        "cargo clippy",
        Some(101),
        None,
        true,
    );
    derive_links(g.ledger(), "c1", AT).expect("derive");
    let bad = coverage(g.ledger(), "c1", &env_with_tools()).expect("coverage");
    assert_eq!(state_of(&bad, 3), CoverageState::Evidence);
    assert_eq!(bad.criteria[2].items[0].observed, Observed::NotProven);
}

// ---------------------------------------------------------------------------
// The human acts
// ---------------------------------------------------------------------------

fn manual(criterion: u32, evidence: &str, basis: LinkBasis, actor: &str, at: &str) -> LinkAct {
    LinkAct {
        criterion_index: criterion,
        kind: EvidenceKind::CheckRun,
        evidence_id: evidence.to_string(),
        basis,
        actor: actor.to_string(),
        reason: "because I looked".to_string(),
        at: at.to_string(),
    }
}

#[test]
fn a_manual_link_needs_a_named_person() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_any",
        "s1",
        "build",
        "cargo clippy",
        Some(0),
        None,
        true,
    );
    let err = act(
        f.ledger(),
        "c1",
        &manual(4, "eb_any", LinkBasis::Manual, "   ", AT),
    )
    .expect_err("an act nobody signed is not an act");
    assert_eq!(
        err.to_string(),
        "We cannot add or remove a link without a name. Say who is doing it."
    );
    assert_eq!(link_row_count(f.ledger(), "c1").expect("count"), 0);
}

#[test]
fn a_person_cannot_hand_link_evidence_this_change_may_not_look_at() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_s2",
        "s2",
        "build",
        "cargo clippy",
        Some(0),
        None,
        true,
    );
    let err = act(
        f.ledger(),
        "c1",
        &manual(4, "eb_s2", LinkBasis::Manual, "sam", AT),
    )
    .expect_err("the gate is the gate");
    assert!(
        err.to_string()
            .starts_with("That piece of work is not confirmed"),
        "{err}"
    );
}

#[test]
fn a_manual_link_reaches_a_criterion_the_rule_would_never_have_found() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_restart",
        "s1",
        "build",
        "cargo run --bin hq -- --smoke",
        Some(0),
        None,
        true,
    );
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let before = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    assert_eq!(state_of(&before, 4), CoverageState::None);

    act(
        f.ledger(),
        "c1",
        &manual(4, "eb_restart", LinkBasis::Manual, "sam", AT),
    )
    .expect("link");
    let after = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    assert_eq!(state_of(&after, 4), CoverageState::Evidence);
    let item = &after.criteria[3].items[0];
    assert_eq!(item.basis, LinkBasis::Manual);
    assert_eq!(item.actor.as_deref(), Some("sam"));
    assert_eq!(item.asserted_at, AT);
}

#[test]
fn unlinking_tombstones_and_keeps_the_history() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_lint",
        "s1",
        "build",
        "cargo clippy",
        Some(0),
        None,
        true,
    );
    derive_links(f.ledger(), "c1", AT).expect("derive");
    assert_eq!(
        state_of(
            &coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage"),
            3
        ),
        CoverageState::Evidence
    );
    let rows_before = link_row_count(f.ledger(), "c1").expect("count");

    act(
        f.ledger(),
        "c1",
        &manual(
            3,
            "eb_lint",
            LinkBasis::NotBearing,
            "sam",
            "2026-02-02T12:00:00Z",
        ),
    )
    .expect("unlink");

    let after = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    let three = &after.criteria[2];
    assert!(three.items.is_empty(), "the link no longer bears");
    assert_eq!(three.removed.len(), 1, "and it is still on the page");
    assert_eq!(three.removed[0].actor.as_deref(), Some("sam"));
    assert_eq!(three.state, CoverageState::None);
    assert_eq!(
        link_row_count(f.ledger(), "c1").expect("count"),
        rows_before + 1,
        "unlinking APPENDS; nothing is deleted"
    );
}

/// Two acts inside one second. The recorded time is whole seconds, so both
/// rows carry the same stamp; the standing answer must still be the one the
/// person did last. Found on the break-and-restore round, where the two acts
/// landed in the same second and the screen showed the link the person had
/// just taken off.
#[test]
fn two_acts_in_one_second_are_resolved_by_what_was_done_last() {
    let f = Fixture::new();
    // The ADVERSARIAL pair, chosen rather than hoped for. The ids are digests,
    // so ordering the rows by id resolves this tie by a hash — and about half
    // the pairs would come out right by luck. This picks an evidence id whose
    // tombstone id sorts BEFORE its link id, which is exactly the half where
    // ordering by id shows the person the link they just took off.
    let mut chosen: Option<String> = None;
    for n in 0..64 {
        let id = format!("eb_{n:02}");
        let link = thruline_core::change_evidence::link_id(
            "c1",
            1,
            4,
            EvidenceKind::CheckRun,
            &id,
            LinkBasis::Manual,
            AT,
        );
        let tomb = thruline_core::change_evidence::link_id(
            "c1",
            1,
            4,
            EvidenceKind::CheckRun,
            &id,
            LinkBasis::NotBearing,
            AT,
        );
        if tomb < link {
            chosen = Some(id);
            break;
        }
    }
    let evidence = chosen.expect("no adversarial pair in 64 tries — that is itself a finding");
    add_run(
        f.ledger(),
        &evidence,
        "s1",
        "build",
        "cargo clippy",
        Some(0),
        None,
        true,
    );
    act(
        f.ledger(),
        "c1",
        &manual(4, &evidence, LinkBasis::Manual, "sam", AT),
    )
    .expect("link");
    act(
        f.ledger(),
        "c1",
        &manual(4, &evidence, LinkBasis::NotBearing, "sam", AT),
    )
    .expect("unlink at the same recorded second");
    let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    assert!(
        view.criteria[3].items.is_empty(),
        "the link the person took off is still standing: {:?}",
        view.criteria[3].items
    );
    assert_eq!(view.criteria[3].removed.len(), 1);
    assert_eq!(view.criteria[3].state, CoverageState::None);
}

#[test]
fn the_rule_does_not_resurrect_a_tombstoned_link() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_lint",
        "s1",
        "build",
        "cargo clippy",
        Some(0),
        None,
        true,
    );
    derive_links(f.ledger(), "c1", AT).expect("derive");
    act(
        f.ledger(),
        "c1",
        &manual(
            3,
            "eb_lint",
            LinkBasis::NotBearing,
            "sam",
            "2026-02-02T12:00:00Z",
        ),
    )
    .expect("unlink");
    let report = derive_links(f.ledger(), "c1", "2026-02-03T12:00:00Z").expect("derive again");
    assert_eq!(report.inserted, 0);
    assert_eq!(report.tombstoned, 1);
    let after = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    assert!(after.criteria[2].items.is_empty());
}

// ---------------------------------------------------------------------------
// The counted things
// ---------------------------------------------------------------------------

#[test]
fn the_link_row_count_is_the_technical_layers_own_number() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_lint",
        "s1",
        "build",
        "cargo clippy",
        Some(0),
        None,
        true,
    );
    add_run(
        f.ledger(),
        "eb_test",
        "s1",
        "test",
        "cargo test",
        Some(0),
        None,
        true,
    );
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    assert_eq!(view.link_rows, link_row_count(f.ledger(), "c1").expect("n"));
    assert_eq!(view.link_rows as usize, dump(f.ledger()).len());
}

#[test]
fn a_change_with_nothing_accepted_has_no_coverage_and_says_so() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('p1','/tmp/synthetic','claude_code','Synthetic','2026-01-01T00:00:00Z')",
            [],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                 request_source)
             VALUES('c1','p1','2026-01-02T09:00:00Z','T','P','sam','person')",
            [],
        )
        .expect("change");
    let view = coverage(&ledger, "c1", &env_with_tools()).expect("coverage");
    assert_eq!(view.scope_version, None);
    assert!(view.criteria.is_empty());
    assert_eq!(view.link_rows, 0);
}

#[test]
fn the_category_to_class_table_is_stated_rather_than_guessed() {
    assert_eq!(class_of_category("tests"), RunClass::Test);
    assert_eq!(class_of_category("behaviour"), RunClass::Test);
    assert_eq!(class_of_category("lint"), RunClass::Build);
    assert_eq!(class_of_category("secrets"), RunClass::Build);
    assert_eq!(class_of_category("nothing we know"), RunClass::Unclassified);
}

#[test]
fn tokens_are_lowercase_alphanumeric_and_in_order() {
    assert_eq!(
        tokens_of("bash:/tmp/x:cargo test -p Thruline-Core"),
        vec!["bash", "tmp", "x", "cargo", "test", "p", "thruline", "core"]
    );
}

#[test]
fn the_served_coverage_is_stable() {
    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_lint",
        "s1",
        "build",
        "cargo clippy",
        Some(0),
        None,
        true,
    );
    add_run(
        f.ledger(),
        "eb_test",
        "s1",
        "test",
        "cargo test",
        Some(0),
        None,
        true,
    );
    add_run(
        f.ledger(),
        "eb_named",
        "s1",
        "unclassified",
        "cargo run --bin importer",
        Some(0),
        None,
        true,
    );
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");
    insta::assert_json_snapshot!(view);
}

/// **T-3-4 verifier finding 5 — the non-Completed veto has teeth.**
///
/// A build-class run that exits 0 is Proven by the ordinary rule. The same run
/// whose EXECUTION was interrupted must never be: its last captured line says
/// nothing about whether the check reached the end. Disabling the veto in
/// `change_evidence::check_runs_of` must fail this test, which is the whole
/// point of it existing (A.3).
#[test]
fn an_interrupted_run_that_exited_zero_is_never_evidence_and_never_proven() {
    use thruline_core::change_evidence::{candidates, EvidenceKind, Observed};
    use thruline_core::run_execution as rx;

    let f = Fixture::new();
    let ledger = f.ledger();
    let change_id = "c1".to_string();
    add_run(
        ledger,
        "eb_build",
        "s1",
        "build",
        "cargo build",
        Some(0),
        None,
        true,
    );

    // As the record stands, with no execution row, the run reads as it always
    // did. That baseline is what makes the assertion below about the VETO and
    // not about the fixture.
    let before = candidates(ledger, &change_id).expect("candidates");
    let run = before
        .iter()
        .find(|c| c.kind == EvidenceKind::CheckRun)
        .expect("the fixture has a check run");
    assert_eq!(
        run.observed,
        Observed::Proven,
        "the fixture is a passing run"
    );
    assert!(run.bears());
    let binding = run.id.clone();

    // Now the record says the execution was cut short.
    rx::start(
        ledger,
        &rx::Start {
            run_key: "run-1".into(),
            change_id: Some(change_id.clone()),
            name: "cargo build".into(),
            pid: None,
            process_group: None,
            started_at: "2026-09-22T10:00:00Z".into(),
            examined_inputs_id: None,
        },
    )
    .expect("supervisor row");
    rx::finish(
        ledger,
        &rx::Finish {
            run_key: "run-1".into(),
            state: rx::ExecutionState::Interrupted,
            reason: Some(rx::Reason::HqRestarted),
            actor: None,
            binding_id: Some(binding.clone()),
            cleanup_verified: Some(true),
            survivors: Some(0),
            at: "2026-09-22T10:09:00Z".into(),
        },
    )
    .expect("terminal row");

    let after = candidates(ledger, &change_id).expect("candidates");
    let run = after
        .iter()
        .find(|c| c.kind == EvidenceKind::CheckRun && c.id == binding)
        .expect("the run is still listed — a check that was cut short is a fact");
    assert_eq!(
        run.observed,
        Observed::Couldnt,
        "an interrupted execution can never read Proven, whatever the exit code was"
    );
    assert!(
        !run.bears(),
        "an interrupted execution satisfies nothing it is attached to (`B7.6`)"
    );
    assert_eq!(run.execution, rx::ExecutionState::Interrupted);
}

/// **T-3-6-F3 verifier finding — the served class and failure line have teeth.**
///
/// The coder's own report disclosed this hole: `item_view`'s `evidence_class`
/// could be broken to `None` and no Rust test went red, because every fixture
/// held a run with no `check_run_records` row. `B16.10` is a label being TRUE
/// wherever a finding is shown, and a field nothing fences is a field that can
/// quietly stop being served.
///
/// Breaking either line in `change_evidence::item_view` must fail this test.
#[test]
fn a_recorded_class_and_failure_line_reach_the_served_item() {
    use thruline_core::run_record::{record as record_run, EvidenceClass, RunRecord};

    let f = Fixture::new();
    add_run(
        f.ledger(),
        "eb_test",
        "s1",
        "test",
        "cargo test",
        Some(101),
        None,
        true,
    );
    record_run(
        f.ledger(),
        "eb_test",
        &RunRecord {
            failure_line: Some(
                "assertion `left == right` failed: must return the same job".to_string(),
            ),
            evidence_class: Some(EvidenceClass::Seeded),
        },
        AT,
    )
    .expect("the run record is appended beside the binding");
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &env_with_tools()).expect("coverage");

    let item = view
        .criteria
        .iter()
        .flat_map(|c| c.items.iter())
        .find(|i| i.evidence_id == "eb_test")
        .expect("the run is linked to a criterion");

    assert_eq!(
        item.evidence_class,
        Some(EvidenceClass::Seeded),
        "the declared class reaches the screen (B16.10)"
    );
    assert_eq!(item.evidence_class_slug, Some("evidence_class_seeded"));
    let line = item
        .failure_line
        .as_deref()
        .expect("the failing assertion's own words reach the screen");
    assert!(line.contains("must return the same job"), "{line}");
}

// ---------------------------------------------------------------------------
// NAMED-LINK-CWD — the `named` basis reads the command, never the folder
// ---------------------------------------------------------------------------

fn passing_run(id: &str, name: &str) -> Candidate {
    Candidate {
        execution: thruline_core::run_execution::ExecutionState::NotRun,
        failure_line: None,
        evidence_class: None,
        kind: EvidenceKind::CheckRun,
        id: id.into(),
        session_id: "s1".into(),
        name: name.into(),
        class: RunClass::Test,
        observed: Observed::Loose,
        checked_nothing: false,
        recorded_at: AT.into(),
    }
}

/// A run's name is `bash:{cwd}:{cmd}`. A project that lives in a folder called
/// `importer` runs `cargo test` there; nobody said that run covers "the
/// importer keeps a malformed line", and the folder's name is not a statement
/// about what the command checked. No link.
#[test]
fn a_run_is_not_named_by_the_folder_it_ran_in() {
    let criteria = vec![CRITERIA[0].to_string()];
    assert!(subject_tokens(CRITERIA[0]).contains(&"importer".to_string()));
    let pool = vec![
        passing_run("eb_cwd", "bash:/work/importer:cargo test"),
        passing_run("eb_deep", "bash:/home/dev/importer/crates/core:npm test"),
    ];
    let links = links_for(&criteria, &[], &pool);
    assert!(
        links.is_empty(),
        "a folder name linked a run to a line nobody said it covers: {links:?}"
    );
}

/// The control: a command that genuinely names the subject still links, by
/// `named`, on the command's own word — including when the folder shares it,
/// and when the command itself carries a colon.
#[test]
fn a_command_that_names_the_subject_still_links() {
    let criteria = vec![CRITERIA[0].to_string()];
    let pool = vec![
        passing_run("eb_cmd", "bash:/work/elsewhere:cargo test importer"),
        passing_run("eb_both", "bash:/work/importer:cargo test -p importer"),
        passing_run(
            "eb_path",
            "bash:/work/x:cargo test importer::keeps_malformed",
        ),
    ];
    let links = links_for(&criteria, &[], &pool);
    let ids: Vec<&str> = links.iter().map(|l| l.evidence_id.as_str()).collect();
    assert_eq!(ids, vec!["eb_cmd", "eb_both", "eb_path"]);
    for link in &links {
        assert_eq!(link.basis, LinkBasis::Named);
        assert_eq!(link.detail, "importer");
    }
}
