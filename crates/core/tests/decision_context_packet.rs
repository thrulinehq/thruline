//! Synthetic confirmation fixtures through the real packet compiler.

use rusqlite::params;
use serde_json::json;
use thruline_core::config::Config;
use thruline_core::decisions::authority::bridge;
use thruline_core::decisions::storage;
use thruline_core::packet::{self, Binding, Budget, Scope, Section};
use thruline_core::Ledger;

#[path = "support/person_action.rs"]
mod person_action;
use person_action::PersonRecorder;

fn fixture() -> (tempfile::TempDir, Ledger, PersonRecorder) {
    let store = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(store.path().join("ledger.db")).unwrap();
    ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('context','/authored/context','codex','2026-10-08T00:00:00Z')", []).unwrap();
    bridge::install_schema_for_write(&ledger).unwrap();
    let person = PersonRecorder::bridge_only(&ledger, "context");
    (store, ledger, person)
}

fn pending(ledger: &Ledger, id: &str, rev: u32) -> String {
    if rev == 1 {
        ledger.conn().execute("INSERT INTO decisions(id,project_id,created_at) VALUES(?1,'context','2026-10-08T00:00:00Z')", [id]).unwrap();
    }
    let rule = json!({"kind":"path_present","path":"retained.txt","sha256":null});
    ledger.conn().execute("INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at) VALUES(?1,?2,'path_present',1,?3,'Keep retained.txt','[]','fixture','2026-10-08T00:01:00Z')", params![id,rev,rule.to_string()]).unwrap();
    let act = format!("accept-{id}-{rev}");
    ledger.conn().execute("INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,severity,applies_from_version) VALUES(?1,?2,?3,'fixture','2026-10-08T00:02:00Z','authored scope','blocks','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa')", params![act,id,rev]).unwrap();
    act
}

fn compile(ledger: &Ledger, budget: Budget) -> packet::Packet {
    packet::compile_request(
        ledger,
        &Scope::Project("context".into()),
        Some("continue the work"),
        &budget,
        &Config::default(),
    )
    .unwrap()
}

#[test]
fn bridge_verified_rules_cross_sessions_and_remain_budgeted_and_retained() {
    let (_store, mut ledger, mut person) = fixture();
    let accepted = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &accepted);
    let pending_only = pending(&ledger, "pending-only", 1);
    assert!(!bridge::verified(
        &ledger,
        thruline_core::decisions::authority::PersonActKind::Accept,
        &pending_only
    )
    .unwrap());
    let packet = compile(&ledger, Budget::default());
    let line = packet
        .lines
        .iter()
        .find(|line| {
            line.origin
                .as_str()
                .starts_with("decision:context/standing")
        })
        .expect("standing decision in production packet");
    assert_eq!(line.section, Section::RequiredCompletionEvidence);
    assert!(line.text.contains("path_present"));
    assert!(line.reason.contains("receipt"));
    assert!(!packet.render().contains("decision:context/pending-only"));
    let issued = packet
        .retain(&mut ledger, Some("authored-successor"), 1)
        .unwrap();
    assert!(packet::handoff(&ledger, &issued)
        .unwrap()
        .unwrap()
        .contains("path_present"));
    let small = compile(&ledger, Budget { tokens: 0 });
    assert!(small.exclusions.iter().any(|line| line
        .origin
        .as_str()
        .starts_with("decision:context/standing")));
    assert!(!small.lines.iter().any(|line| line
        .origin
        .as_str()
        .starts_with("decision:context/standing")));
}

#[test]
fn decision_bytes_change_binding_even_when_event_heads_do_not_move() {
    let (_store, ledger, mut person) = fixture();
    let act = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &act);
    let before = Binding::compute(&ledger, &["context".into()]).unwrap();
    let _pending_later = pending(&ledger, "standing", 2);
    let after = Binding::compute(&ledger, &["context".into()]).unwrap();
    assert_eq!(
        before.heads, after.heads,
        "this test changes decisions only"
    );
    assert_ne!(
        before.digest, after.digest,
        "decision state belongs to the packet binding"
    );
    let current = compile(&ledger, Budget::default());
    assert!(current.lines.iter().any(|line| line
        .origin
        .as_str()
        .starts_with("decision:context/standing")
        && line.text.contains("revision 1")));
}

#[test]
fn verified_retirement_removes_rules_without_new_acts_on_read() {
    let (_store, ledger, mut person) = fixture();
    let act = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &act);
    assert!(compile(&ledger, Budget::default())
        .render()
        .contains("decision:context/standing"));
    let retirement = storage::retire(
        &ledger,
        storage::RetirementInput {
            decision_id: "standing".into(),
            actor: "fixture".into(),
            at: "2026-10-08T00:03:00Z".into(),
            because: "finished authored work".into(),
            superseded_by: None,
        },
    )
    .unwrap();
    assert!(
        compile(&ledger, Budget::default())
            .render()
            .contains("decision:context/standing"),
        "pending retirement is inert"
    );
    person.bind_bridge_retirement(&ledger, &retirement);
    let before: (i64,i64) = ledger.conn().query_row("SELECT (SELECT count(*) FROM decision_person_acts),(SELECT count(*) FROM decision_bridge_proofs)", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert!(!compile(&ledger, Budget::default())
        .render()
        .contains("decision:context/standing"));
    assert_eq!(before, ledger.conn().query_row("SELECT (SELECT count(*) FROM decision_person_acts),(SELECT count(*) FROM decision_bridge_proofs)", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap());
}

#[test]
fn changed_raw_confirmation_invalidates_same_head_binding_and_keeps_handoff_frozen() {
    let (_store, mut ledger, mut person) = fixture();
    let act = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &act);
    let old = compile(&ledger, Budget::default());
    let issue = old.retain(&mut ledger, Some("first-client"), 10).unwrap();
    let frozen = packet::handoff(&ledger, &issue).unwrap().unwrap();
    let counts: (i64,i64) = ledger.conn().query_row("SELECT (SELECT count(*) FROM decision_person_acts),(SELECT count(*) FROM decision_bridge_proofs)", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
    std::fs::write(person.raw_file(), "changed authored confirmation source\n").unwrap();
    let new = compile(&ledger, Budget::default());
    assert_eq!(new.binding.heads, old.binding.heads);
    assert_ne!(new.binding.digest, old.binding.digest);
    assert!(!new.render().contains("decision:context/standing"));
    assert_eq!(packet::handoff(&ledger, &issue).unwrap().unwrap(), frozen);
    let context = thruline_core::decisions::context::read(&ledger, &["context".into()]).unwrap();
    assert!(context
        .notes
        .iter()
        .any(|note| note.decision_id == "standing" && note.state == "unverifiable"));
    assert_eq!(counts, ledger.conn().query_row("SELECT (SELECT count(*) FROM decision_person_acts),(SELECT count(*) FROM decision_bridge_proofs)", [], |r| Ok((r.get(0)?,r.get(1)?))).unwrap());
}

#[test]
fn pending_revision_changes_context_since_and_never_uses_unconfirmed_words() {
    let (_store, mut ledger, mut person) = fixture();
    let act = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &act);
    let old = compile(&ledger, Budget::default());
    old.retain(&mut ledger, Some("first-client"), 10).unwrap();
    pending(&ledger, "standing", 2);
    assert!(matches!(
        packet::context_since(
            &ledger,
            &Scope::Project("context".into()),
            &old.binding.digest,
            &Config::default()
        )
        .unwrap(),
        packet::ContextAnswer::Changed { .. }
    ));
    ledger
        .conn()
        .execute_batch("DROP TRIGGER decision_revisions_no_update")
        .unwrap();
    ledger.conn().execute("UPDATE decision_revisions SET text='Unconfirmed source instructions' WHERE decision_id='standing' AND rev=1", []).unwrap();
    let new = compile(&ledger, Budget::default());
    assert!(new.render().contains("decision:context/standing"));
    assert!(!new.render().contains("Unconfirmed source instructions"));
    assert_ne!(new.binding.digest, old.binding.digest);
}

#[test]
fn changed_accepted_params_legacy_confirmation_and_cross_project_rows_are_inert() {
    let (_store, ledger, mut person) = fixture();
    let act = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &act);
    let legacy = pending(&ledger, "legacy", 1);
    let mut legacy_person = PersonRecorder::new(&ledger, "context");
    legacy_person.bind_legacy_acceptance(&ledger, &legacy);
    assert!(!compile(&ledger, Budget::default())
        .render()
        .contains("decision:context/legacy"));
    ledger
        .conn()
        .execute_batch("DROP TRIGGER decision_revisions_no_update")
        .unwrap();
    let params = json!({"kind":"path_present","path":"changed.txt","sha256":null});
    ledger
        .conn()
        .execute(
            "UPDATE decision_revisions SET params_json=?1 WHERE decision_id='standing' AND rev=1",
            [params.to_string()],
        )
        .unwrap();
    assert!(!compile(&ledger, Budget::default())
        .render()
        .contains("decision:context/standing"));
    assert!(
        thruline_core::decisions::context::read(&ledger, &["elsewhere".into()])
            .unwrap()
            .decisions
            .is_empty()
    );
}

#[test]
fn context_read_without_bridge_schema_never_installs_it() {
    let store = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(store.path().join("ledger.db")).unwrap();
    ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('context','/authored/context','codex','2026-10-08T00:00:00Z')", []).unwrap();
    pending(&ledger, "pending", 1);
    let before: i64 = ledger
        .conn()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='decision_bridge_proofs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let context = thruline_core::decisions::context::read(&ledger, &["context".into()]).unwrap();
    assert!(context.decisions.is_empty());
    assert_eq!(
        before,
        ledger
            .conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='decision_bridge_proofs'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap()
    );
}

#[test]
fn production_mcp_packet_carries_standing_check_and_polls_same_head_changes() {
    let (store, ledger, mut person) = fixture();
    let act = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &act);
    let before: (i64,i64,i64) = ledger.conn().query_row("SELECT (SELECT count(*) FROM decision_person_acts),(SELECT count(*) FROM decision_bridge_proofs),(SELECT count(*) FROM decision_verdicts)", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    let successor =
        thruline_core::mcp::Server::new("authored-successor", store.path().join("ledger.db"))
            .unwrap();
    let packet = successor
        .call(
            "get_context_packet",
            &json!({"project":"context","task":"continue the work"}),
        )
        .unwrap();
    let markdown = packet["packet_md"].as_str().unwrap();
    assert!(markdown.contains("decision:context/standing") && markdown.contains("path_present"));
    let position = packet["chain_position"].as_str().unwrap();
    let poll = successor
        .call(
            "check_context",
            &json!({"project":"context","chain_position":position}),
        )
        .unwrap();
    assert_eq!(poll["current"], true);
    pending(&ledger, "standing", 2);
    let poll = successor
        .call(
            "check_context",
            &json!({"project":"context","chain_position":position}),
        )
        .unwrap();
    assert_eq!(poll["status"], "changed");
    assert_eq!(poll["current"], false);
    assert_eq!(before, ledger.conn().query_row("SELECT (SELECT count(*) FROM decision_person_acts),(SELECT count(*) FROM decision_bridge_proofs),(SELECT count(*) FROM decision_verdicts)", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap());
}

#[test]
fn parameter_note_changes_binding_without_becoming_an_accepted_instruction() {
    let (_store, ledger, _person) = fixture();
    let before = Binding::compute(&ledger, &["context".into()]).unwrap();
    let repo = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let result = std::process::Command::new("git")
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap().trim().to_string()
    };
    run(&["init", "-q"]);
    std::fs::write(repo.path().join("CLAUDE.md"), "Only touch pending_scope.\n").unwrap();
    run(&["add", "CLAUDE.md"]);
    run(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        "parameter note",
    ]);
    let version = run(&["rev-parse", "HEAD"]);
    thruline_core::decisions::rule_files::ingest(
        &ledger,
        "context",
        repo.path(),
        &version,
        "2026-10-08T00:05:00Z",
    )
    .unwrap();
    let after = Binding::compute(&ledger, &["context".into()]).unwrap();
    assert_eq!(after.heads, before.heads);
    assert_ne!(after.digest, before.digest);
    let packet = compile(&ledger, Budget::default());
    assert!(!packet
        .lines
        .iter()
        .any(|line| line.origin.as_str().starts_with("decision:")));
    assert!(!packet.render().contains("pending_scope"));
}

#[test]
fn lost_superseding_receipt_restores_predecessor_in_production_context() {
    let (_store, ledger, mut person) = fixture();
    let old = pending(&ledger, "predecessor", 1);
    person.bind_bridge_acceptance(&ledger, &old);
    let new = pending(&ledger, "successor", 1);
    person.bind_bridge_acceptance(&ledger, &new);
    let successor_source = person.raw_file().to_owned();
    let retirement = storage::retire(
        &ledger,
        storage::RetirementInput {
            decision_id: "predecessor".into(),
            actor: "fixture".into(),
            at: "2026-10-08T00:05:00Z".into(),
            because: "replace the exact predecessor".into(),
            superseded_by: Some("successor".into()),
        },
    )
    .unwrap();
    person.bind_bridge_retirement(&ledger, &retirement);
    let before = compile(&ledger, Budget::default());
    assert!(!before.render().contains("decision:context/predecessor"));
    assert!(before.render().contains("decision:context/successor"));
    std::fs::write(
        successor_source,
        "changed authored successor confirmation\n",
    )
    .unwrap();
    let after = compile(&ledger, Budget::default());
    assert_eq!(after.binding.heads, before.binding.heads);
    assert_ne!(after.binding.digest, before.binding.digest);
    assert!(
        after.render().contains("decision:context/predecessor"),
        "retirement cannot hide the predecessor when its successor evidence is absent"
    );
    assert!(!after.render().contains("decision:context/successor"));
}

#[cfg(unix)]
#[test]
fn private_receipt_permissions_change_the_production_mcp_position() {
    use std::os::unix::fs::PermissionsExt;
    let (store, ledger, mut person) = fixture();
    let act = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &act);
    let server =
        thruline_core::mcp::Server::new("permission-client", store.path().join("ledger.db"))
            .unwrap();
    let packet = server
        .call("get_context_packet", &json!({"project":"context"}))
        .unwrap();
    let position = packet["chain_position"].as_str().unwrap();
    let before = authority_counts(&ledger);
    std::fs::set_permissions(person.raw_file(), std::fs::Permissions::from_mode(0o644)).unwrap();
    let poll = server
        .call(
            "check_context",
            &json!({"project":"context","chain_position":position}),
        )
        .unwrap();
    assert_eq!(
        poll["current"], false,
        "private-mode loss invalidates an issued position even with unchanged bytes"
    );
    assert_eq!(poll["status"], "changed");
    assert!(!compile(&ledger, Budget::default())
        .render()
        .contains("decision:context/standing"));
    assert_eq!(before, authority_counts(&ledger));
    std::fs::set_permissions(person.raw_file(), std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        server
            .call(
                "check_context",
                &json!({"project":"context","chain_position":position})
            )
            .unwrap()["current"],
        true
    );
    std::fs::set_permissions(
        person.raw_file().parent().unwrap(),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert_eq!(
        server
            .call(
                "check_context",
                &json!({"project":"context","chain_position":position})
            )
            .unwrap()["current"],
        false
    );
}

#[cfg(unix)]
#[test]
fn receipt_symlink_changes_binding_without_changing_its_bytes() {
    use std::os::unix::fs::symlink;
    let (_store, ledger, mut person) = fixture();
    let act = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &act);
    let before = Binding::compute(&ledger, &["context".into()]).unwrap();
    let counts = authority_counts(&ledger);
    let target = person.raw_file().with_extension("saved");
    std::fs::rename(person.raw_file(), &target).unwrap();
    symlink(&target, person.raw_file()).unwrap();
    let after = Binding::compute(&ledger, &["context".into()]).unwrap();
    assert_eq!(before.heads, after.heads);
    assert_ne!(
        before.digest, after.digest,
        "same receipt bytes through a symlink are no longer bridge evidence"
    );
    assert!(!compile(&ledger, Budget::default())
        .render()
        .contains("decision:context/standing"));
    assert_eq!(counts, authority_counts(&ledger));
}

#[test]
fn extra_receipt_line_changes_binding_without_changing_the_referenced_line() {
    use std::io::Write;
    let (_store, ledger, mut person) = fixture();
    let act = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &act);
    let before = Binding::compute(&ledger, &["context".into()]).unwrap();
    let counts = authority_counts(&ledger);
    std::fs::OpenOptions::new()
        .append(true)
        .open(person.raw_file())
        .unwrap()
        .write_all(b"extra authored line\n")
        .unwrap();
    let after = Binding::compute(&ledger, &["context".into()]).unwrap();
    assert_eq!(before.heads, after.heads);
    assert_ne!(
        before.digest, after.digest,
        "the bridge requires one complete receipt line"
    );
    assert!(!compile(&ledger, Budget::default())
        .render()
        .contains("decision:context/standing"));
    assert_eq!(counts, authority_counts(&ledger));
}

#[test]
fn changed_normalized_confirmation_invalidates_binding_without_moving_heads() {
    let (_store, ledger, mut person) = fixture();
    let act = pending(&ledger, "standing", 1);
    person.bind_bridge_acceptance(&ledger, &act);
    let before = Binding::compute(&ledger, &["context".into()]).unwrap();
    let counts = authority_counts(&ledger);
    ledger
        .conn()
        .execute_batch("DROP TRIGGER events_no_update")
        .unwrap();
    ledger.conn().execute("UPDATE events SET tool_name='authored corruption' WHERE id=(SELECT event_id FROM decision_person_acts WHERE act_row_id=?1)", [&act]).unwrap();
    let after = Binding::compute(&ledger, &["context".into()]).unwrap();
    assert_eq!(before.heads, after.heads);
    assert_ne!(
        before.digest, after.digest,
        "normalized receipt evidence is part of a position's validity"
    );
    assert!(!compile(&ledger, Budget::default())
        .render()
        .contains("decision:context/standing"));
    assert_eq!(counts, authority_counts(&ledger));
}

fn authority_counts(ledger: &Ledger) -> (i64, i64, i64) {
    ledger.conn().query_row("SELECT (SELECT count(*) FROM decision_person_acts),(SELECT count(*) FROM decision_bridge_proofs),(SELECT count(*) FROM decision_verdicts)", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap()
}
