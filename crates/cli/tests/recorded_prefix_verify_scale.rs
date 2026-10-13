//! Unavailable receipts keep exact coverage without repeating every event.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::Ledger;

struct Fixture {
    temp: tempfile::TempDir,
    ledger: Ledger,
    previous: String,
    sequence: i64,
}

impl Fixture {
    fn new(raw: &Path, raw_lines: i64) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(temp.path().join("ledger.db")).unwrap();
        ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('scale-project',?1,'claude_code','2026-10-08T00:00:00Z')", [temp.path().to_str().unwrap()]).unwrap();
        ledger.conn().execute("INSERT INTO sessions(id,project_id,file_path,raw_line_count) VALUES('scale-session','scale-project',?1,?2)", rusqlite::params![raw.to_str().unwrap(),raw_lines]).unwrap();
        Self {
            temp,
            ledger,
            previous: genesis_prev_hash("scale-project"),
            sequence: 0,
        }
    }

    fn event(&mut self, path: &Path, line: i64, digest: &str) {
        self.sequence += 1;
        let next = chain_hash(&self.previous, digest, "scale-session", self.sequence);
        self.ledger.conn().execute("INSERT INTO events(id,project_id,session_id,seq,kind,role,text,is_sidechain,raw_file,raw_line,raw_sha256,prev_hash,chain_hash) VALUES(?1,'scale-project','scale-session',?2,'user_msg','user','authored synthetic receipt',0,?3,?4,?5,?6,?7)", rusqlite::params![format!("scale-event-{}",self.sequence),self.sequence,path.to_str().unwrap(),line,digest,self.previous,next]).unwrap();
        self.previous = next;
    }

    fn verify(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_thruline"))
            .arg("--db")
            .arg(self.temp.path().join("ledger.db"))
            .args(["verify", "--json"])
            .env("HOME", self.temp.path())
            .env("THRULINE_HOME", self.temp.path().join("state"))
            .current_dir(self.temp.path())
            .output()
            .unwrap()
    }
}

fn row(doc: &Value) -> &Value {
    assert_eq!(doc["shape"], "thruline.verify");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["verification"]["complete"], true);
    doc["projects"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
}

#[test]
fn many_unavailable_event_receipts_have_exact_counted_noncontiguous_coverage() {
    let source = tempfile::tempdir().unwrap();
    let missing = source.path().join("absent.jsonl");
    let mut fixture = Fixture::new(&missing, 1534);
    for n in 0..512 {
        fixture.event(
            &missing,
            1 + n * 3,
            &sha256_hex(format!("authored-{n}").as_bytes()),
        );
    }
    let out = fixture.verify();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    let report = row(&doc);
    assert_eq!(report["events_checked"], 512);
    assert_eq!(report["chain_matches"], 512);
    assert_eq!(report["raw_matches"], 0);
    assert_eq!(report["sources"][0]["state"], "SOURCE_GONE");
    assert_eq!(
        report["unavailable_raw_receipts"],
        json!([{
            "session_id":"scale-session",
            "file":missing.to_str().unwrap(),
            "event_receipt_count":512,
            "min_recorded_line":1,
            "max_recorded_line":1534,
            "reason":"recorded raw receipt cannot be answered from source or a verified seal"
        }])
    );
    assert!(
        out.stdout.len() < 32 * 1024,
        "unnecessary per-event output: {} bytes",
        out.stdout.len()
    );
}

#[test]
fn counted_unavailable_receipts_preserve_each_independent_known_mismatch() {
    let source = tempfile::tempdir().unwrap();
    let present = source.path().join("present.jsonl");
    let missing: PathBuf = source.path().join("absent.jsonl");
    let lines: Vec<_> = (0..5)
        .map(|n| format!("authored source line {n}"))
        .collect();
    std::fs::write(&present, format!("{}\n", lines.join("\n"))).unwrap();
    let mut fixture = Fixture::new(&present, 5);
    let wrong = sha256_hex(b"different recorded bytes");
    for (n, line) in lines.iter().enumerate() {
        let expected = if n == 2 {
            wrong.clone()
        } else {
            sha256_hex(line.as_bytes())
        };
        fixture.event(&present, n as i64 + 1, &expected);
    }
    fixture.event(&missing, 999, &sha256_hex(b"missing-first"));
    fixture.event(&missing, 1003, &sha256_hex(b"missing-second"));
    let out = fixture.verify();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    let report = row(&doc);
    assert_eq!(report["events_checked"], 7);
    assert_eq!(report["chain_matches"], 7);
    assert_eq!(report["raw_matches"], 4);
    assert_eq!(
        report["raw_mismatches"],
        json!([{
            "file":present.to_str().unwrap(),"line":3,
            "stored_sha256":wrong,"found_sha256":sha256_hex(lines[2].as_bytes())
        }])
    );
    assert_eq!(
        report["unavailable_raw_receipts"][0]["event_receipt_count"],
        2
    );
    assert_eq!(
        report["unavailable_raw_receipts"][0]["min_recorded_line"],
        999
    );
    assert_eq!(
        report["unavailable_raw_receipts"][0]["max_recorded_line"],
        1003
    );
}
