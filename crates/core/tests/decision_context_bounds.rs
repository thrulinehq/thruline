//! Isolated authored-path rejection checks; no real record is opened.
#![cfg(unix)]
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use thruline_core::{
    decisions::{authority::bridge, context},
    Ledger,
};
#[path = "support/person_action.rs"]
mod person_action;
use person_action::PersonRecorder;

fn fixture() -> (tempfile::TempDir, Ledger, PersonRecorder) {
    let temp = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(temp.path().join("ledger.db")).unwrap();
    ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('bounds','/authored/bounds','codex','2026-10-08T00:00:00Z')",[]).unwrap();
    bridge::install_schema_for_write(&ledger).unwrap();
    ledger.conn().execute_batch("INSERT INTO decisions(id,project_id,created_at) VALUES('standing','bounds','now'); INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at) VALUES('standing',1,'path_present',1,'{\"kind\":\"path_present\",\"path\":\"retained.rs\",\"sha256\":null}','Keep retained.rs','[]','fixture','now'); INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,severity,applies_from_version) VALUES('accept-standing','standing',1,'fixture','now','exact authored scope','warns','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa')").unwrap();
    let mut person = PersonRecorder::bridge_only(&ledger, "bounds");
    person.bind_bridge_acceptance(&ledger, "accept-standing");
    assert_eq!(
        context::read(&ledger, &["bounds".into()])
            .unwrap()
            .decisions
            .len(),
        1
    );
    (temp, ledger, person)
}

fn subprocess_rejects(temp: &tempfile::TempDir) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "context_bounds_worker", "--nocapture"])
        .env("THRULINE_CONTEXT_BOUNDS_DB", temp.path().join("ledger.db"))
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "isolated receipt rejection failed");
            return;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("receipt rejection did not finish within the isolated two-second bound");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn fifo_receipt_is_rejected_without_waiting_for_a_writer() {
    let (temp, _ledger, person) = fixture();
    std::fs::remove_file(person.raw_file()).unwrap();
    make_fifo(person.raw_file());
    subprocess_rejects(&temp);
}
#[test]
fn symlink_to_special_receipt_is_rejected_without_following_it() {
    let (temp, _ledger, person) = fixture();
    let special = temp.path().join("authored-special-source");
    make_fifo(&special);
    std::fs::remove_file(person.raw_file()).unwrap();
    std::os::unix::fs::symlink(&special, person.raw_file()).unwrap();
    subprocess_rejects(&temp);
}
#[test]
fn oversized_receipt_never_grants_standing_or_extends_the_read_bound() {
    use std::io::Write;
    let (temp, _ledger, person) = fixture();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(person.raw_file())
        .unwrap();
    file.write_all(&vec![b'x'; 2 * 1024 * 1024]).unwrap();
    subprocess_rejects(&temp);
}
#[test]
fn context_bounds_worker() {
    let Some(db) = std::env::var_os("THRULINE_CONTEXT_BOUNDS_DB") else {
        return;
    };
    let ledger = Ledger::open(db).unwrap();
    let count = || {
        ledger.conn().query_row("SELECT (SELECT count(*) FROM decision_person_acts),(SELECT count(*) FROM decision_bridge_proofs),(SELECT count(*) FROM decision_verdicts)",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?))).unwrap()
    };
    let before = count();
    let read = context::read(&ledger, &["bounds".into()]).unwrap();
    assert!(read.decisions.is_empty());
    assert!(read.notes.iter().any(|n| n.state == "unverifiable"));
    assert_eq!(before, count());
}

fn make_fifo(path: &std::path::Path) {
    assert!(Command::new("mkfifo")
        .args(["-m", "600"])
        .arg(path)
        .status()
        .unwrap()
        .success());
}
