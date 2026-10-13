//! Read-only review gives the person the decision words and closed parameters
//! behind an action digest before any OS or TTY confirmation is requested.

use thruline_core::chain::sha256_hex;
use thruline_core::decisions::authority::bridge::{self, SupersessionInput};
use thruline_core::decisions::authority::PersonActKind;
use thruline_core::decisions::params::{
    CheckParams, PathPresentParams, VisualMatchParams, VisualScreen,
};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, RevisionInput, SourceKind, WaiverInput,
};
use thruline_core::Ledger;

fn source(span: &str) -> DecisionSource {
    DecisionSource {
        source_kind: SourceKind::RuleFile,
        event_id: None,
        file_anchor: Some("review-source".into()),
        raw_file: "AGENTS.md".into(),
        raw_line: 1,
        span_start: 0,
        span_end: span.len() as u64,
        span: span.into(),
        actor: Some("person".into()),
        recorded_at: Some("2026-10-07T00:00:00Z".into()),
    }
}

fn ledger() -> Ledger {
    let ledger = Ledger::open_in_memory().expect("scratch ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES('review','/scratch','codex','2026-10-07T00:00:00Z')",
            [],
        )
        .expect("project");
    ledger
}

fn counts(ledger: &Ledger) -> (i64, i64, i64, bool) {
    let count = |table: &str| {
        ledger
            .conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("authority count")
    };
    let bridge_exists: bool = ledger
        .conn()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='decision_bridge_proofs')",
            [],
            |row| row.get(0),
        )
        .expect("bridge schema state");
    (
        count("sessions"),
        count("events"),
        count("decision_person_acts"),
        bridge_exists,
    )
}

fn accept(ledger: &Ledger, decision_id: &str, revision: u32) -> String {
    storage::accept(
        ledger,
        AcceptanceInput {
            decision_id: decision_id.into(),
            revision,
            actor: "person".into(),
            at: "2026-10-07T00:01:00Z".into(),
            because: "Review exact rule".into(),
            severity: "blocks".into(),
            applies_from_version: "a".repeat(40),
        },
    )
    .expect("pending acceptance")
}

#[test]
fn acceptance_review_shows_text_kind_full_params_and_stored_byte_digest_without_authority() {
    let ledger = ledger();
    let decision = storage::create(
        &ledger,
        NewDecision {
            project_id: "review".into(),
            source_anchor: "review-source".into(),
            span: "Keep the waiver-list rule file".into(),
            sources: vec![source("Keep the waiver-list rule file")],
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::PathPresent(PathPresentParams {
                path: "policy/waivers.yml".into(),
                sha256: None,
            }),
        },
    )
    .expect("decision");
    let act = accept(&ledger, &decision, 1);
    let before = counts(&ledger);
    let review = bridge::review_terminal_act(&ledger, PersonActKind::Accept, &act, None)
        .expect("read-only review");
    let stored: String = ledger
        .conn()
        .query_row(
            "SELECT params_json FROM decision_revisions WHERE decision_id=?1 AND rev=1",
            [&decision],
            |row| row.get(0),
        )
        .expect("stored params");
    assert_eq!(review["decision"]["text"], "Keep the waiver-list rule file");
    assert_eq!(review["decision"]["kind"], "path_present");
    assert_eq!(review["decision"]["revision"], 1);
    assert_eq!(
        review["decision"]["params"],
        serde_json::from_str::<serde_json::Value>(&stored).unwrap()
    );
    assert_eq!(
        review["decision"]["params_sha256"],
        sha256_hex(stored.as_bytes())
    );
    assert_eq!(
        review["confirmation"]["thruline_decision_act"]["params_sha256"],
        review["decision"]["params_sha256"]
    );
    assert_eq!(
        counts(&ledger),
        before,
        "review must create no authority evidence"
    );
}

#[test]
fn waiver_review_shows_the_named_version_and_underlying_rule_without_claiming_activation() {
    let ledger = ledger();
    let decision = storage::create(
        &ledger,
        NewDecision {
            project_id: "review".into(),
            source_anchor: "review-source".into(),
            span: "Keep the waiver-list rule file".into(),
            sources: vec![source("Keep the waiver-list rule file")],
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::PathPresent(PathPresentParams {
                path: "policy/waivers.yml".into(),
                sha256: None,
            }),
        },
    )
    .expect("decision");
    accept(&ledger, &decision, 1);
    let waiver = storage::waive(
        &ledger,
        WaiverInput {
            decision_id: decision,
            version_id: "a".repeat(40),
            actor: "person".into(),
            at: "2026-10-07T00:02:00Z".into(),
            because: "Allow this exact version".into(),
            expires_version: None,
            expires_at: None,
        },
    )
    .expect("pending waiver");
    let before = counts(&ledger);
    let review = bridge::review_terminal_act(&ledger, PersonActKind::Waive, &waiver, None)
        .expect("waiver review");
    assert_eq!(review["action"], "waive");
    assert_eq!(
        review["confirmation"]["thruline_decision_act"]["version_id"],
        "a".repeat(40)
    );
    assert_eq!(review["decision"]["kind"], "path_present");
    assert_eq!(review["decision"]["params"]["path"], "policy/waivers.yml");
    assert_eq!(
        review["decision"]["revision_basis"],
        "latest_recorded_acceptance_pending_bridge"
    );
    assert_eq!(counts(&ledger), before);
}

#[test]
fn terminal_review_refuses_a_masked_parameter_before_it_can_be_printed() {
    let ledger = ledger();
    let decision = storage::create(
        &ledger,
        NewDecision {
            project_id: "review".into(),
            source_anchor: "review-source".into(),
            span: "Keep a selected file".into(),
            sources: vec![source("Keep a selected file")],
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::PathPresent(PathPresentParams {
                path: "policy/ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e".into(),
                sha256: None,
            }),
        },
    )
    .expect("decision");
    let act = accept(&ledger, &decision, 1);
    let before = counts(&ledger);
    let error = bridge::review_terminal_act(&ledger, PersonActKind::Accept, &act, None)
        .expect_err("masked values cannot be shown as full review parameters");
    assert!(error.to_string().contains("masked values"));
    assert_eq!(counts(&ledger), before);
}

#[test]
fn terminal_review_refuses_bare_credential_prefixes_in_parameters_without_authority() {
    for bare in [
        "sk-",
        "sk-ant-",
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "AKIA",
        "ASIA",
        "AIza",
        "eyJ",
        "xoxb-",
    ] {
        let ledger = ledger();
        let decision = storage::create(
            &ledger,
            NewDecision {
                project_id: "review".into(),
                source_anchor: "review-source".into(),
                span: "Keep a selected file".into(),
                sources: vec![source("Keep a selected file")],
                author_actor: "person".into(),
                recorded_at: "2026-10-07T00:00:00Z".into(),
                params: CheckParams::PathPresent(PathPresentParams {
                    path: format!("policy/{bare}"),
                    sha256: None,
                }),
            },
        )
        .expect("synthetic decision");
        let act = accept(&ledger, &decision, 1);
        let before = counts(&ledger);
        let error = bridge::review_terminal_act(&ledger, PersonActKind::Accept, &act, None)
            .expect_err("a bare credential prefix must not reach the terminal review");
        assert!(
            error.to_string().contains("masked values"),
            "{bare}: {error}"
        );
        assert_eq!(counts(&ledger), before);
    }
}

#[test]
fn terminal_review_refuses_bare_credential_prefixes_in_words_without_authority() {
    for bare in [
        "sk-",
        "sk-ant-",
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "AKIA",
        "ASIA",
        "AIza",
        "eyJ",
        "xoxb-",
    ] {
        let ledger = ledger();
        let words = format!("Keep {bare} in the selected file");
        let decision = storage::create(
            &ledger,
            NewDecision {
                project_id: "review".into(),
                source_anchor: "review-source".into(),
                span: words.clone(),
                sources: vec![source(&words)],
                author_actor: "person".into(),
                recorded_at: "2026-10-07T00:00:00Z".into(),
                params: CheckParams::PathPresent(PathPresentParams {
                    path: "policy/rules.yml".into(),
                    sha256: None,
                }),
            },
        )
        .expect("synthetic decision");
        let act = accept(&ledger, &decision, 1);
        let before = counts(&ledger);
        let error = bridge::review_terminal_act(&ledger, PersonActKind::Accept, &act, None)
            .expect_err("a bare credential prefix must not reach the terminal review");
        assert!(
            error.to_string().contains("masked values"),
            "{bare}: {error}"
        );
        assert_eq!(counts(&ledger), before);
    }
}

#[test]
fn supersession_review_names_old_and_new_reference_and_both_revisions_without_authority() {
    let ledger = ledger();
    let old_screen = VisualScreen {
        screen: "Approve-Sheet".into(),
        route: "/approve".into(),
        scenario: "design".into(),
        legacy_route: None,
        legacy_scenario: None,
        reference_path: "refs/Approve-Sheet.old.png".into(),
        reference_sha256: "a".repeat(64),
        viewport_width: 1512,
        viewport_height: 2708,
        full_page: true,
        mask_rectangles: vec![],
    };
    let decision = storage::create(
        &ledger,
        NewDecision {
            project_id: "review".into(),
            source_anchor: "review-source".into(),
            span: "Keep Approve-Sheet on the pinned reference".into(),
            sources: vec![source("Keep Approve-Sheet on the pinned reference")],
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::VisualMatch(VisualMatchParams {
                screens: vec![old_screen.clone()],
                tolerance_pct: 0.0,
            }),
        },
    )
    .expect("old visual");
    let mut new_screen = old_screen.clone();
    new_screen.reference_path = "refs/Approve-Sheet.new.png".into();
    new_screen.reference_sha256 = "b".repeat(64);
    storage::append_revision(
        &ledger,
        RevisionInput {
            decision_id: decision.clone(),
            span: "Use the new pinned Approve-Sheet reference".into(),
            sources: vec![source("Use the new pinned Approve-Sheet reference")],
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:00:10Z".into(),
            params: CheckParams::VisualMatch(VisualMatchParams {
                screens: vec![new_screen.clone()],
                tolerance_pct: 0.0,
            }),
        },
    )
    .expect("successor");
    let act = accept(&ledger, &decision, 2);
    let before = counts(&ledger);
    let review = bridge::review_terminal_act(
        &ledger,
        PersonActKind::Accept,
        &act,
        Some(&SupersessionInput {
            predecessor_revision: 1,
            screen: "Approve-Sheet".into(),
        }),
    )
    .expect("read-only supersession review");
    assert_eq!(review["action"], "supersede");
    assert_eq!(review["decision"]["revision"], 2);
    assert_eq!(
        review["decision"]["text"],
        "Use the new pinned Approve-Sheet reference"
    );
    assert_eq!(review["supersession"]["predecessor_revision"], 1);
    assert_eq!(review["supersession"]["successor_revision"], 2);
    assert_eq!(
        review["supersession"]["old_reference_path"],
        old_screen.reference_path
    );
    assert_eq!(
        review["supersession"]["new_reference_path"],
        new_screen.reference_path
    );
    assert_eq!(
        review["supersession"]["old_reference_sha256"],
        old_screen.reference_sha256
    );
    assert_eq!(
        review["supersession"]["new_reference_sha256"],
        new_screen.reference_sha256
    );
    assert!(review["summary"]
        .as_str()
        .unwrap()
        .contains("Approve-Sheet"));
    assert!(review["summary"]
        .as_str()
        .unwrap()
        .contains("refs/Approve-Sheet.old.png"));
    assert!(review["summary"]
        .as_str()
        .unwrap()
        .contains("refs/Approve-Sheet.new.png"));
    assert_eq!(
        counts(&ledger),
        before,
        "review must create no authority evidence"
    );
}
