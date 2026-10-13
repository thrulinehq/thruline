//! T-3-2b — an execution recorded for a Change reaches it, a Change says which
//! folders it covers, and a Change bound on this build is never compared
//! against nothing (`B3.5`, `B3.7`, `B3.13`, `B4.4`, `B4.6`, `B11.2`, `Q3`,
//! `Q4`, `L1.1`).
//!
//! The three walls T-3-2 hit on the ordinary path, each with the test that
//! would find it again:
//!
//! 1. a capture made by `thruline check` could never reach a Change, because
//!    the session it named was not a row in `sessions`;
//! 2. nothing in the product could say which files a Change covers, so the
//!    scope check refused every session with `not_applicable_no_scope`;
//! 3. the predicate was not applied at all when a Change had not written down
//!    what it is about, so every item read `applies: null`.
//!
//! Every negative control here is the failure the fix could reintroduce: a run
//! for no Change that attaches itself anyway, a run for another Change read
//! across, a session outside the covered folders counted, and a widened scope
//! nobody accepted.

use thruline_core::change_evidence::{coverage, derive_links};
use thruline_core::change_link::{
    self, applicability, check_session, Applicability, NewCheckSession,
};
use thruline_core::change_propose::{
    self, scope_for_paths, Acceptance, Attribution, ProfileEnvironment, ScopeChoice, Selection,
};
use thruline_core::examined_inputs::{self, SubjectKind};
use thruline_core::ledger::Ledger;

const AT: &str = "2026-02-01T12:00:00Z";

struct Fixture {
    _dir: tempfile::TempDir,
    ledger: Ledger,
    /// The project's folder, as the store names it.
    root: std::path::PathBuf,
}

impl Fixture {
    /// One project, one transcript session that edited a file, one Change.
    /// Nothing is accepted and nothing is bound: every test below says what it
    /// adds.
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        // The project's folder is a real folder on this disk, because that is
        // what a registered project is: binding observes it, and a store that
        // names a folder nobody can see is its own test below.
        let root = dir.path().join("project");
        std::fs::create_dir_all(root.join("src/upload")).expect("mkdir");
        let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
        let conn = ledger.conn();
        conn.execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('p1',?1,'codex','Synthetic','2026-01-01T00:00:00Z')",
            rusqlite::params![root.display().to_string()],
        )
        .expect("project");
        conn.execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('s1','p1','/tmp/synthetic/s1.jsonl','2026-01-01T00:00:00Z')",
            [],
        )
        .expect("session");
        conn.execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('e1','p1','s1',1,'2026-01-01T00:10:00Z','tool_use','Edit',?1,
                    '/tmp/synthetic/s1.jsonl',1,'aa','','bb')",
            rusqlite::params![format!(
                "{{\"file_path\":\"{}/src/upload/retry.rs\"}}",
                root.display()
            )],
        )
        .expect("event");
        conn.execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                 request_source)
             VALUES('c1','p1','2026-01-02T09:00:00Z','Retry failed uploads',
                    'Two retry requests for the same delivery do not enqueue duplicate work. \
                     The test suite passes on the packages that changed.',
                    'sam','person')",
            [],
        )
        .expect("change");
        change_link::decide(
            &ledger,
            "c1",
            "s1",
            thruline_core::change::AssociationProvenance::Confirmed,
            "sam",
            "this is the retry work",
            "2026-01-02T10:00:00Z",
        )
        .expect("confirm");
        Fixture {
            _dir: dir,
            ledger,
            root,
        }
    }

    fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// A second Change in the same project, for the "run for another Change"
    /// control.
    fn second_change(&self) {
        self.ledger
            .conn()
            .execute(
                "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                     request_source)
                 VALUES('c2','p1','2026-01-02T09:00:00Z','Something else','','sam','person')",
                [],
            )
            .expect("second change");
    }

    /// One `evidence_bindings` row, as the recorder writes them.
    fn add_binding(&self, id: &str, session: &str, class: &str, exit: i64) {
        let seq: i64 = self
            .ledger
            .conn()
            .query_row(
                "SELECT COALESCE(MAX(seq),-1)+1 FROM evidence_bindings",
                [],
                |r| r.get(0),
            )
            .expect("seq");
        self.ledger
            .conn()
            .execute(
                "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
                     scope_key, root, repository_identity, pre_identity, post_identity,
                     exit_code, term_signal, bound_at, source_session, doc, doc_sha256)
                 VALUES(?1,?2,'2026-02-01T11:00:00Z',1,?3,?4,'/tmp/synthetic','repo',
                        'pre','post',?5,NULL,'2026-02-01T11:00:00Z',?6,
                        '{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10},
                          \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10}}','ddd')",
                rusqlite::params![
                    id,
                    seq,
                    class,
                    format!("bash:/tmp/synthetic:cargo test --workspace"),
                    exit,
                    session,
                ],
            )
            .expect("binding");
    }

    /// The whole ordinary path in one call: accept what counts as done and the
    /// folders it covers, through the one act a person performs.
    fn accept_the_ordinary_way(&self) -> thruline_core::change_propose::AcceptanceOutcome {
        change_propose::accept(
            &self.ledger,
            "c1",
            &Acceptance {
                selection: Selection::All,
                scope: ScopeChoice::AsProposed,
                replacing: false,
                edits: Default::default(),
                actor: "sam".to_string(),
                rationale: "this is what I want checked".to_string(),
                accepted_at: "2026-02-01T10:00:00Z".to_string(),
            },
            &ProfileEnvironment::empty(),
        )
        .expect("accept")
    }

    /// The write `thruline check --for-change` makes.
    fn record_check(&self, session: &str, binding: &str, change: &str) -> Result<String, String> {
        change_link::record_check_session(
            &self.ledger,
            &NewCheckSession {
                session_id: session.to_string(),
                project_id: "p1".to_string(),
                binding_id: binding.to_string(),
                change_id: change.to_string(),
                ran_in: "/tmp/synthetic".to_string(),
                actor: "sam".to_string(),
                recorded_at: AT.to_string(),
            },
        )
        .map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Wall 1 — a check run reaches the Change it was run for
// ---------------------------------------------------------------------------

/// The card's first acceptance: a check run from the CLI for a Change appears
/// under its criterion, through T-3-1's mapping, with no manual step.
#[test]
fn a_check_run_recorded_for_a_change_appears_under_its_criterion() {
    let f = Fixture::new();
    f.accept_the_ordinary_way();
    f.add_binding("eb1", "cli:check:2026-02-01T11:00:00Z", "test", 0);
    f.record_check("cli:check:2026-02-01T11:00:00Z", "eb1", "c1")
        .expect("the check session is recorded");

    // Nothing else happens in between: this is what opening the Verify view
    // does, and it is the only step.
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &ProfileEnvironment::empty()).expect("coverage");
    let found: Vec<&str> = view
        .criteria
        .iter()
        .flat_map(|c| c.items.iter())
        .map(|i| i.evidence_id.as_str())
        .collect();
    assert!(
        found.contains(&"eb1"),
        "a check run recorded for this Change reached no criterion: {found:?}"
    );
}

/// The negative control, and the one that matters: a run made for no Change
/// stays in the record and attaches to nothing.
#[test]
fn a_check_run_for_no_change_stays_discoverable_and_attaches_to_nothing() {
    let f = Fixture::new();
    f.accept_the_ordinary_way();
    f.add_binding("eb_loose", "cli:check:2026-02-01T11:30:00Z", "test", 0);
    // No `record_check`: no session row, no association, no check-session row.

    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &ProfileEnvironment::empty()).expect("coverage");
    for criterion in &view.criteria {
        for item in &criterion.items {
            assert_ne!(
                item.evidence_id, "eb_loose",
                "a run made for no Change reached a criterion"
            );
        }
    }
    assert!(
        !view.candidates.iter().any(|c| c.evidence_id == "eb_loose"),
        "a run made for no Change entered a Change's candidate pool"
    );
    // Still recorded, still readable: `thruline evidence` reads this row.
    let rows: i64 = f
        .ledger()
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM evidence_bindings WHERE id='eb_loose'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(rows, 1, "the capture itself was not recorded");
}

/// A run recorded for another Change is refused where it does not belong, with
/// its own sentence rather than a silence.
#[test]
fn a_check_run_for_another_change_is_refused_and_says_which_refusal_it_is() {
    let f = Fixture::new();
    f.second_change();
    f.accept_the_ordinary_way();
    f.add_binding("eb2", "cli:check:2026-02-01T11:10:00Z", "test", 0);
    f.record_check("cli:check:2026-02-01T11:10:00Z", "eb2", "c2")
        .expect("recorded for the other Change");
    // A person then confirms that same session onto this Change by hand.
    change_link::decide(
        f.ledger(),
        "c1",
        "cli:check:2026-02-01T11:10:00Z",
        thruline_core::change::AssociationProvenance::Confirmed,
        "sam",
        "I think this belongs here too",
        AT,
    )
    .expect("confirm");

    let verdict = applicability(f.ledger(), "c1", "cli:check:2026-02-01T11:10:00Z").expect("gate");
    assert!(!verdict.applies(), "{verdict:?}");
    assert_eq!(verdict.slug(), "not_applicable_check_for_another_change");
    assert!(
        thruline_core::voice::association_line(verdict.slug()).is_some(),
        "the refusal has no sentence"
    );
}

/// A Change that covers nothing counts nothing — including an execution run for
/// it. The check arm sits *after* the scope test, not around it.
#[test]
fn a_check_run_is_not_counted_for_a_change_that_covers_nothing() {
    let f = Fixture::new();
    f.add_binding("eb3", "cli:check:2026-02-01T11:20:00Z", "test", 0);
    f.record_check("cli:check:2026-02-01T11:20:00Z", "eb3", "c1")
        .expect("recorded");
    let verdict = applicability(f.ledger(), "c1", "cli:check:2026-02-01T11:20:00Z").expect("gate");
    assert_eq!(verdict.slug(), "not_applicable_no_scope", "{verdict:?}");
}

/// The write is one act with a name on it, and without a name nothing at all
/// is written — not the session, not the row, not the link.
#[test]
fn recording_a_check_session_needs_a_name_and_writes_nothing_without_one() {
    let f = Fixture::new();
    f.add_binding("eb4", "cli:check:2026-02-01T11:40:00Z", "test", 0);
    let refused = change_link::record_check_session(
        f.ledger(),
        &NewCheckSession {
            session_id: "cli:check:2026-02-01T11:40:00Z".to_string(),
            project_id: "p1".to_string(),
            binding_id: "eb4".to_string(),
            change_id: "c1".to_string(),
            ran_in: "/tmp/synthetic".to_string(),
            actor: "   ".to_string(),
            recorded_at: AT.to_string(),
        },
    );
    assert!(refused.is_err(), "an unnamed act was accepted");
    let sessions: i64 = f
        .ledger()
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE id='cli:check:2026-02-01T11:40:00Z'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(sessions, 0, "a refused act still wrote a session row");
}

/// The session the capture names and the session the store holds are one id:
/// the binding is never rewritten to point somewhere else.
#[test]
fn the_session_row_carries_the_id_the_binding_already_named() {
    let f = Fixture::new();
    f.add_binding("eb5", "cli:check:2026-02-01T11:50:00Z", "test", 0);
    f.record_check("cli:check:2026-02-01T11:50:00Z", "eb5", "c1")
        .expect("recorded");
    let named: String = f
        .ledger()
        .conn()
        .query_row(
            "SELECT source_session FROM evidence_bindings WHERE id='eb5'",
            [],
            |r| r.get(0),
        )
        .expect("read");
    let session = check_session(f.ledger(), &named)
        .expect("read")
        .expect("row");
    assert_eq!(session.session_id, named);
    assert_eq!(session.binding_id, "eb5");
    assert_eq!(session.change_id, "c1");
}

// ---------------------------------------------------------------------------
// Wall 2 — accepting writes what the Change covers
// ---------------------------------------------------------------------------

/// The card's second acceptance: the ordinary path yields a non-empty allowed
/// scope, and the gate then admits the session it was read off.
#[test]
fn the_ordinary_path_writes_the_folders_and_the_gate_then_admits_the_work() {
    let f = Fixture::new();
    let outcome = f.accept_the_ordinary_way();
    assert!(
        !outcome.allowed_scope.is_empty(),
        "accepting wrote no folders: {outcome:?}"
    );
    assert_eq!(outcome.allowed_scope, vec!["src/upload/**".to_string()]);
    assert_eq!(outcome.scope_attribution, Attribution::AsProposed);

    let verdict = applicability(f.ledger(), "c1", "s1").expect("gate");
    assert!(
        verdict.applies(),
        "the gate still refuses the work: {verdict:?}"
    );
    assert_eq!(verdict.slug(), "applies_in_scope");
}

/// A session that touched only files outside the covered folders is refused,
/// with T-2-2's own sentence.
#[test]
fn a_session_outside_the_covered_folders_is_refused_with_the_scope_sentence() {
    let f = Fixture::new();
    f.accept_the_ordinary_way();
    f.ledger()
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('s2','p1','/tmp/synthetic/s2.jsonl','2026-01-01T00:00:00Z')",
            [],
        )
        .expect("session");
    f.ledger()
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('e2','p1','s2',1,'2026-01-01T00:20:00Z','tool_use','Edit',?1,
                    '/tmp/synthetic/s2.jsonl',1,'aa','','bb')",
            rusqlite::params![format!(
                "{{\"file_path\":\"{}/docs/readme.md\"}}",
                f.root.display()
            )],
        )
        .expect("event");
    change_link::decide(
        f.ledger(),
        "c1",
        "s2",
        thruline_core::change::AssociationProvenance::Confirmed,
        "sam",
        "and this",
        AT,
    )
    .expect("confirm");

    let verdict = applicability(f.ledger(), "c1", "s2").expect("gate");
    assert!(!verdict.applies(), "{verdict:?}");
    assert_eq!(verdict.slug(), "not_applicable_outside_scope");
    match verdict {
        Applicability::NotApplicable { outside_paths, .. } => {
            assert_eq!(outside_paths, vec!["docs/readme.md".to_string()])
        }
        other => panic!("{other:?}"),
    }
}

/// A person who edits the folders before accepting owns the list, and the
/// record says so.
#[test]
fn folders_a_person_typed_are_attributed_to_them_and_not_to_us() {
    let f = Fixture::new();
    let outcome = change_propose::accept(
        f.ledger(),
        "c1",
        &Acceptance {
            selection: Selection::All,
            scope: ScopeChoice::Paths(vec!["src/**".to_string()]),
            replacing: false,
            edits: Default::default(),
            actor: "sam".to_string(),
            rationale: "the whole of src, not just the one folder".to_string(),
            accepted_at: "2026-02-01T10:00:00Z".to_string(),
        },
        &ProfileEnvironment::empty(),
    )
    .expect("accept");
    assert_eq!(outcome.allowed_scope, vec!["src/**".to_string()]);
    assert_eq!(outcome.scope_attribution, Attribution::YourWords);
}

/// The folders are read off the record and nothing else: a directory, dedup, a
/// nested folder dropped under the one that already covers it, and a file at
/// the top named exactly rather than widened.
#[test]
fn the_folders_are_the_ones_the_work_touched_and_never_a_wildcard() {
    assert_eq!(
        scope_for_paths(&[
            "src/upload/retry.rs".to_string(),
            "src/upload/mod.rs".to_string(),
            "README.md".to_string(),
        ]),
        vec!["README.md".to_string(), "src/upload/**".to_string()]
    );
    // A folder already covered by a shorter entry is dropped rather than
    // repeated, so accepting twice cannot grow the list.
    assert_eq!(
        scope_for_paths(&["src/a.rs".to_string(), "src/deep/b.rs".to_string()]),
        vec!["src/**".to_string()]
    );
    // Nothing becomes `**`.
    assert!(!scope_for_paths(&["a.rs".to_string()]).contains(&"**".to_string()));
    assert!(scope_for_paths(&[]).is_empty());
}

// ---------------------------------------------------------------------------
// Wall 3 — a Change bound on this build is never compared against nothing
// ---------------------------------------------------------------------------

/// Binding writes the record, and the served view then asks the question of
/// every examination instead of leaving it unasked.
#[test]
fn binding_a_change_makes_applies_null_unreachable() {
    let f = Fixture::new();
    f.accept_the_ordinary_way();
    examined_inputs::bind_intended(f.ledger(), "c1", AT)
        .expect("bind")
        .expect("the project has a folder");
    f.add_binding("eb6", "cli:check:2026-02-01T12:10:00Z", "test", 0);
    f.record_check("cli:check:2026-02-01T12:10:00Z", "eb6", "c1")
        .expect("recorded");

    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &ProfileEnvironment::empty()).expect("coverage");
    assert_eq!(view.nothing_bound_slug, None);
    let items: Vec<_> = view
        .criteria
        .iter()
        .flat_map(|c| c.items.iter())
        .filter(|i| i.kind == thruline_core::change_evidence::EvidenceKind::CheckRun)
        .collect();
    assert!(!items.is_empty(), "nothing was linked to compare");
    for item in items {
        assert!(
            item.applies.is_some(),
            "the question was not put for {}: {item:?}",
            item.evidence_id
        );
        assert!(item.applies_slug.is_some());
    }
    assert!(
        examined_inputs::standing(f.ledger(), SubjectKind::Intended, "c1")
            .expect("read")
            .is_some()
    );
}

/// A Change from an earlier build keeps its honest silence, and the view names
/// the one act that ends it.
#[test]
fn a_change_with_nothing_bound_says_so_and_names_the_act() {
    let f = Fixture::new();
    f.accept_the_ordinary_way();
    f.add_binding("eb7", "cli:check:2026-02-01T12:20:00Z", "test", 0);
    f.record_check("cli:check:2026-02-01T12:20:00Z", "eb7", "c1")
        .expect("recorded");
    derive_links(f.ledger(), "c1", AT).expect("derive");
    let view = coverage(f.ledger(), "c1", &ProfileEnvironment::empty()).expect("coverage");
    assert_eq!(view.nothing_bound_slug, Some("inputs_nothing_bound"));
    let line = thruline_core::voice::inputs_line("inputs_nothing_bound").expect("line");
    assert!(
        line.plain
            .contains("not set up to check against your code yet"),
        "the sentence does not say what is missing: {}",
        line.plain
    );
    // And it does not borrow the word the top of the same screen spends on the
    // version of the work (T-3-2b r1): two records, two sentences, no shared
    // noun.
    assert!(
        !line.plain.to_lowercase().contains("version"),
        "the sentence says \"version\", which on this screen names the work: {}",
        line.plain
    );
    for criterion in &view.criteria {
        for item in &criterion.items {
            assert_eq!(
                item.applies, None,
                "a mismatch against nothing was asserted"
            );
        }
    }
}

/// A project folder that is not on this disk is not guessed at: nothing is
/// written, and the Change reads as not bound rather than as examined and
/// unreadable. The two are different facts.
#[test]
fn a_project_folder_that_is_not_here_binds_nothing_rather_than_unknowns() {
    let f = Fixture::new();
    f.ledger()
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('p9','/tmp/a-folder-that-is-not-here','claude_code','Gone',
                    '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("project");
    f.ledger()
        .conn()
        .execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                 request_source)
             VALUES('c9','p9','2026-01-02T09:00:00Z','Elsewhere','','sam','person')",
            [],
        )
        .expect("change");
    assert!(!std::path::Path::new("/tmp/a-folder-that-is-not-here").is_dir());
    assert_eq!(
        examined_inputs::bind_intended(f.ledger(), "c9", AT).expect("bind"),
        None
    );
    assert!(
        examined_inputs::standing(f.ledger(), SubjectKind::Intended, "c9")
            .expect("read")
            .is_none(),
        "a folder nobody can see was recorded as examined"
    );
}

/// A lossy project key is not permission to choose a directory that merely
/// spells similarly. Two plausible hyphen expansions stay unbound until a
/// captured cwd or a person's explicit CLI selection establishes the root.
#[test]
fn ambiguous_hyphenated_roots_are_not_reconstructed_for_binding() {
    let f = Fixture::new();
    let base = f._dir.path().join("ambiguous");
    std::fs::create_dir_all(base.join("retry-delivery")).expect("first candidate");
    std::fs::create_dir_all(base.join("retry_delivery")).expect("second candidate");
    let stored = base.join("retry/delivery");
    f.ledger()
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('p-ambiguous',?1,'claude_code','Ambiguous','2026-01-01T00:00:00Z')",
            rusqlite::params![stored.display().to_string()],
        )
        .expect("project");
    f.ledger()
        .conn()
        .execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                 request_source)
             VALUES('c-ambiguous','p-ambiguous','2026-01-02T09:00:00Z','Retry','','sam','person')",
            [],
        )
        .expect("change");

    assert_eq!(
        examined_inputs::bind_intended(f.ledger(), "c-ambiguous", AT).expect("bind"),
        None,
        "a similarly named directory was selected without captured identity"
    );
}

/// Binding observes; it does not claim. No running application and no tools
/// are recorded by the act of opening a Change.
#[test]
fn binding_claims_no_running_application_and_no_tools() {
    let f = Fixture::new();
    examined_inputs::bind_intended(f.ledger(), "c1", AT)
        .expect("bind")
        .expect("recorded");
    let record = examined_inputs::standing(f.ledger(), SubjectKind::Intended, "c1")
        .expect("read")
        .expect("row");
    assert_eq!(record.target, examined_inputs::Target::None);
    assert!(record.tools.is_empty());
}

/// Claude's directory key loses hyphens, but the captured transcript retains
/// the actual cwd. First-use calls this same core binding path without a CLI
/// root, so this is the ordinary captured-project recovery rather than a
/// hand-edited store or a filesystem name guess.
#[test]
fn a_hyphenated_project_binds_from_its_captured_cwd() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("retry-delivery");
    std::fs::create_dir_all(&root).expect("mkdir");
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(root.join("README.md"), "captured root\n").expect("write");
    git(&["add", "."]);
    git(&["commit", "-qm", "seed"]);

    let transcript = dir.path().join("session.jsonl");
    std::fs::write(
        &transcript,
        format!(r#"{{"type":"user","cwd":"{}"}}"#, root.display()) + "\n",
    )
    .expect("transcript");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('hyphen','/tmp/retry/delivery','claude_code','Retry','2026-01-01T00:00:00Z')",
            [],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('s-hyphen','hyphen',?1,'2026-01-01T00:00:00Z')",
            rusqlite::params![transcript.display().to_string()],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester, request_source)
             VALUES('c-hyphen','hyphen','2026-01-02T09:00:00Z','Retry','','sam','person')",
            [],
        )
        .expect("change");

    assert!(examined_inputs::bind_intended(&ledger, "c-hyphen", AT)
        .expect("bind")
        .is_some());
    let canonical_root = root.canonicalize().expect("canonical root");
    let record = examined_inputs::standing(&ledger, SubjectKind::Intended, "c-hyphen")
        .expect("standing")
        .expect("record");
    assert_eq!(
        record.worktree_path.value(),
        Some(canonical_root.to_str().expect("utf8"))
    );
}

/// Two captured roots for one project are conflicting provenance, not a reason
/// to choose the newest transcript. Neither binding nor a changed-file scope
/// may borrow one of them.
#[test]
fn conflicting_captured_roots_leave_binding_and_scope_unresolved() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = dir.path().join("first-worktree");
    let second = dir.path().join("second-worktree");
    std::fs::create_dir_all(first.join("src")).expect("first root");
    std::fs::create_dir_all(&second).expect("second root");
    std::fs::write(first.join("src/retry.rs"), "changed\n").expect("changed file");
    let first_raw = dir.path().join("first.jsonl");
    let second_raw = dir.path().join("second.jsonl");
    std::fs::write(
        &first_raw,
        format!(r#"{{"type":"user","cwd":"{}"}}"#, first.display()) + "\n",
    )
    .expect("first transcript");
    std::fs::write(
        &second_raw,
        format!(r#"{{"type":"user","cwd":"{}"}}"#, second.display()) + "\n",
    )
    .expect("second transcript");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('p-conflict','/missing/lossy/root','claude_code','Conflict','2026-01-01T00:00:00Z')",
            [],
        )
        .expect("project");
    for (id, raw) in [("s-first", &first_raw), ("s-second", &second_raw)] {
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id, project_id, file_path, started_at)
                 VALUES(?1,'p-conflict',?2,'2026-01-01T00:00:00Z')",
                rusqlite::params![id, raw.display().to_string()],
            )
            .expect("session");
    }
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('e-conflict','p-conflict','s-first',1,'2026-01-01T00:10:00Z',
                    'tool_use','Edit',?1,?2,1,'aa','','bb')",
            rusqlite::params![
                format!(
                    "{{\"file_path\":\"{}\"}}",
                    first.join("src/retry.rs").display()
                ),
                first_raw.display().to_string(),
            ],
        )
        .expect("event");
    ledger
        .conn()
        .execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester, request_source)
             VALUES('c-conflict','p-conflict','2026-01-02T09:00:00Z','Retry','','sam','person')",
            [],
        )
        .expect("change");

    assert_eq!(
        thruline_core::workspace::established_project_root(&ledger, "p-conflict", None)
            .expect("resolve"),
        None,
        "conflicting captured roots selected one workspace"
    );
    assert_eq!(
        examined_inputs::bind_intended(&ledger, "c-conflict", AT).expect("bind"),
        None
    );
    assert!(
        change_link::session_changed_paths(&ledger, "s-first")
            .expect("changed paths")
            .is_empty(),
        "an absolute path entered scope after root provenance conflicted"
    );
}

// ---------------------------------------------------------------------------
// T-3-2b r1 — what a Change covers is spelled the way the project spells it
// ---------------------------------------------------------------------------

/// One project, one edited file, and the folders that fall out of it.
///
/// The vendor root is deliberately lossy. The captured transcript, rather
/// than that spelling, records the real workspace that makes an absolute tool
/// path eligible for scope.
fn covered_by(folder: &str, edited_relative: &str) -> (Vec<String>, Vec<String>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join(folder);
    let edited = root.join(edited_relative);
    std::fs::create_dir_all(edited.parent().expect("parent")).expect("mkdir");
    std::fs::write(&edited, "changed\n").expect("file");
    let transcript = dir.path().join("s1.jsonl");
    std::fs::write(
        &transcript,
        format!(r#"{{"type":"user","cwd":"{}"}}"#, root.display()) + "\n",
    )
    .expect("transcript");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p1','/missing/lossy/root','claude_code','Synthetic','2026-01-01T00:00:00Z')",
        [],
    )
    .expect("project");
    conn.execute(
        "INSERT INTO sessions(id, project_id, file_path, started_at)
         VALUES('s1','p1',?1,'2026-01-01T00:00:00Z')",
        rusqlite::params![transcript.display().to_string()],
    )
    .expect("session");
    conn.execute(
        "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
             tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
         VALUES('e1','p1','s1',1,'2026-01-01T00:10:00Z','tool_use','Edit',?1,
                ?2,1,'aa','','bb')",
        rusqlite::params![
            format!("{{\"file_path\":\"{}\"}}", edited.display()),
            transcript.display().to_string(),
        ],
    )
    .expect("event");
    let paths = change_link::session_changed_paths(&ledger, "s1").expect("paths");
    let folders = scope_for_paths(&paths);
    (paths, folders)
}

/// The wall this removes, found on a real project: Claude Code names a
/// project's directory by replacing every `/` with `-`, and recovering the path
/// replaces every `-` with `/`, so a project in a folder called
/// `retry-delivery` is stored at a root that is nowhere on the disk. Nothing
/// under it matched, and the one thing this card puts on Sam's screen — the
/// folders a Change covers — read as a full machine path instead of `src/**`.
#[test]
fn a_project_folder_with_a_hyphen_is_covered_as_a_folder_and_not_as_a_machine_path() {
    let (paths, folders) = covered_by("retry-delivery", "src/upload/retry.rs");
    assert_eq!(paths, vec!["src/upload/retry.rs".to_string()]);
    assert_eq!(folders, vec!["src/upload/**".to_string()]);
    assert!(
        !folders.iter().any(|f| f.starts_with('/')),
        "a full path from this machine reached what a person reads: {folders:?}"
    );
}

/// The ordinary case is untouched: a root that is exactly the prefix still
/// cuts exactly where it did.
#[test]
fn a_project_folder_with_no_hyphen_is_cut_exactly_where_it_always_was() {
    let (paths, folders) = covered_by("project", "src/upload/retry.rs");
    assert_eq!(paths, vec!["src/upload/retry.rs".to_string()]);
    assert_eq!(folders, vec!["src/upload/**".to_string()]);
}

/// An absolute file path must have a root established from the captured record;
/// a stored path which merely exists is not provenance. The same applies to a
/// sibling workspace whose name happens to share a prefix.
#[test]
fn a_file_outside_the_project_folder_is_never_cut_into_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("retry-delivery");
    let sibling = dir.path().join("retry-delivery-two/src/x.rs");
    std::fs::create_dir_all(sibling.parent().expect("parent")).expect("mkdir");
    std::fs::write(&sibling, "outside\n").expect("file");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p1',?1,'claude_code','Synthetic','2026-01-01T00:00:00Z')",
            rusqlite::params![root.display().to_string()],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
         VALUES('s1','p1','/missing/transcript.jsonl','2026-01-01T00:00:00Z')",
            [],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
             tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
         VALUES('e1','p1','s1',1,'2026-01-01T00:10:00Z','tool_use','Edit',?1,
                '/missing/transcript.jsonl',1,'aa','','bb')",
            rusqlite::params![format!("{{\"file_path\":\"{}\"}}", sibling.display())],
        )
        .expect("event");
    let paths = change_link::session_changed_paths(&ledger, "s1").expect("paths");
    assert!(
        paths.is_empty(),
        "unproven absolute sibling reached scope: {paths:?}"
    );
}

/// Every line `verify` would print about one session: its whole-file digest
/// findings and its missing receipts.
fn verify_findings_naming(ledger: &Ledger, session: &str, file: &str) -> Vec<String> {
    let report = thruline_core::ingest::verify(ledger, None).expect("verify runs");
    let mut out = Vec::new();
    for v in report.projects.values() {
        out.extend(
            v.digest_mismatches
                .iter()
                .filter(|m| m.starts_with(&format!("{session}:")))
                .cloned(),
        );
        out.extend(
            v.missing_files
                .iter()
                .filter(|m| !file.is_empty() && m.as_str() == file)
                .cloned(),
        );
    }
    out
}

/// **ADV-run-ledger-5 (adversary F5, lane B5).** A check run recorded for a
/// Change is a session with a whole-file digest, the way every session is: its
/// row points at the run's own record — one line, written once beside the
/// store — with that file's digest and line count, so `thruline verify`
/// checks it like any transcript. Before this the row pointed at the folder
/// the command ran in with no digest, and verify said `no stored digest` and
/// FAIL after every run. And the check is real, not waved through: a record
/// altered after the fact is named.
#[test]
fn a_check_session_is_verified_through_a_record_file_with_its_digest() {
    let f = Fixture::new();
    let session = "cli:check:2026-02-01T12:10:00Z";
    f.add_binding("eb9", session, "test", 0);
    f.record_check(session, "eb9", "c1").expect("recorded");

    let (file, lines, digest): (String, Option<i64>, Option<String>) = f
        .ledger()
        .conn()
        .query_row(
            "SELECT file_path, raw_line_count, raw_digest FROM sessions WHERE id=?1",
            [session],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("the session row");
    assert_eq!(
        verify_findings_naming(f.ledger(), session, &file),
        Vec::<String>::new(),
        "verify flags a check session the product wrote itself"
    );
    assert_eq!(lines, Some(1));
    assert!(digest.is_some());
    let text = std::fs::read_to_string(&file).expect("the record file is on disk");
    assert!(text.contains(session) && text.contains("eb9"), "{text}");
    // Where the command ran is still what the typed row says it was.
    assert_eq!(
        check_session(f.ledger(), session)
            .expect("read")
            .expect("a check session")
            .ran_in,
        "/tmp/synthetic"
    );

    // Altered after the fact: verify names the session.
    std::fs::write(&file, format!("{text}{{\"added\":\"later\"}}\n")).expect("alter");
    let findings = verify_findings_naming(f.ledger(), session, &file);
    assert_eq!(
        findings,
        vec![format!("{session}: raw_line_count stored 1 / found 2")],
        "an altered check record went unnoticed"
    );
}
