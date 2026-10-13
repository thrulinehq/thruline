//! C44 E2: real CLI over authored sealed recordings; no founder data.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;
use thruline_core::ingest::ingest;
use thruline_core::Ledger;

const PROJECT: &str = "-Users-synthetic-prefix";
const SESSION: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const RECORD: &str = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"authored prefix control\"},\"sessionId\":\"aaaaaaaa-0000-4000-8000-000000000001\",\"uuid\":\"prefix-user\"}";

struct Fixture {
    _temp: TempDir,
    home: PathBuf,
    db: PathBuf,
    source: PathBuf,
}

impl Fixture {
    fn new(newline: bool) -> Self {
        let temp = tempfile::tempdir().expect("authored fixture");
        let home = temp.path().join("home");
        fs::create_dir_all(&home).expect("home");
        let project = temp.path().join("projects").join(PROJECT);
        fs::create_dir_all(&project).expect("project");
        let source = project.join(format!("{SESSION}.jsonl"));
        fs::write(
            &source,
            format!("{RECORD}{}", if newline { "\n" } else { "" }),
        )
        .expect("source");
        let db = temp.path().join("ledger.db");
        let mut ledger = Ledger::open(&db).expect("ledger");
        let report = ingest(&mut ledger, &[project], &temp.path().join("mirrors"))
            .expect("production ingest");
        assert_eq!(report.claude_sealed, 1, "{report:?}");
        assert!(report.new_events > 0);
        drop(ledger);
        Self {
            _temp: temp,
            home,
            db,
            source,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_thruline"))
            .env("HOME", &self.home)
            .env("THRULINE_HOME", self.home.join(".thruline"))
            .arg("--db")
            .arg(&self.db)
            .args(args)
            .output()
            .expect("native CLI")
    }

    fn verify(&self) -> Output {
        self.run(&["verify", "--project", PROJECT])
    }

    fn append(&self, bytes: &[u8]) {
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .open(&self.source)
            .expect("authored source")
            .write_all(bytes)
            .expect("append");
    }
}

#[test]
fn a_matching_recorded_prefix_that_grew_exits_zero_and_names_unrecorded_bytes() {
    let f = Fixture::new(true);
    let added = b"a new malformed row not recorded\n";
    f.append(added);
    let out = f.verify();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("GREW AFTER RECORDING"), "{text}");
    assert!(
        text.contains(&format!("{} bytes not recorded", added.len())),
        "{text}"
    );
}

#[test]
fn recorded_eof_inside_a_now_longer_physical_line_is_not_a_raw_line_mismatch() {
    let f = Fixture::new(false);
    f.append(b" extension of the old final line\n");
    let out = f.verify();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("GREW AFTER RECORDING"));
}

#[test]
fn source_gone_is_unverifiable_even_when_the_sealed_copy_is_intact() {
    let f = Fixture::new(true);
    fs::remove_file(&f.source).expect("authored removal");
    let out = f.verify();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("SOURCE GONE"), "{text}");
    assert!(text.contains("sealed copy intact"), "{text}");
    assert!(!text.contains("CHAIN DAMAGED"));
}

#[test]
fn a_real_recorded_prefix_rewrite_stays_an_integrity_failure() {
    let f = Fixture::new(true);
    let content = fs::read_to_string(&f.source).expect("authored bytes");
    fs::write(&f.source, content.replace("authored", "rewrote")).expect("rewrite");
    let out = f.verify();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("PREFIX MISMATCH"));
}

#[test]
fn truncation_is_a_real_prefix_mismatch_even_when_a_mirror_survives() {
    let f = Fixture::new(true);
    fs::write(&f.source, b"{\"type\":").expect("truncate");
    let out = f.verify();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("PREFIX MISMATCH"));
}

#[test]
fn source_gone_and_growth_do_not_refuse_a_schema_upgrade() {
    for gone in [false, true] {
        let f = Fixture::new(true);
        if gone {
            fs::remove_file(&f.source).expect("gone");
        } else {
            f.append(b"not part of this recording\n");
        }
        let conn = rusqlite::Connection::open(&f.db).expect("synthetic DB");
        conn.execute("DELETE FROM schema_migrations", [])
            .expect("pending");
        drop(conn);
        let out = f.run(&["upgrade"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let ledger = Ledger::open(&f.db).expect("upgraded");
        assert!(ledger.took_schema_fast_path());
    }
}
