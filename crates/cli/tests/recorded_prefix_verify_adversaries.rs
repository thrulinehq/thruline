//! C44 E2: independent corruption, availability and JSON contract controls.

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

fn doc(f: &Fixture, expected: i32) -> serde_json::Value {
    let out = f.run(&["verify", "--project", PROJECT, "--json"]);
    assert_eq!(out.status.code(), Some(expected), "{out:?}");
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("one JSON document");
    assert_eq!(value["shape"], "thruline.verify");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["exit_code"], expected);
    value
}

fn payload(f: &Fixture) -> PathBuf {
    let conn = rusqlite::Connection::open(&f.db).expect("authored DB");
    let authority: String = conn
        .query_row("SELECT authority_key FROM ledger_authority", [], |r| {
            r.get(0)
        })
        .expect("authority identity");
    let path: String = conn
        .query_row(
            "SELECT mirror_payload_path FROM session_generations",
            [],
            |r| r.get(0),
        )
        .expect("authored receipt");
    f.db.parent()
        .expect("DB parent")
        .join("mirrors/ledgers")
        .join(authority)
        .join(path)
}

#[test]
fn json_intact_and_growth_bind_exact_recorded_and_unrecorded_bytes() {
    let f = Fixture::new(false);
    let before = doc(&f, 0);
    let source = &before["projects"][PROJECT]["sources"][0];
    assert_eq!(source["state"], "INTACT");
    assert_eq!(source["sealed_copy"], "INTACT");
    assert_eq!(source["recorded_bytes"], RECORD.len());
    assert_eq!(source["prefix_sha256"], source["recorded_sha256"]);
    let extra = b"an unrecorded physical-line extension\n";
    f.append(extra);
    let after = doc(&f, 0);
    let source = &after["projects"][PROJECT]["sources"][0];
    assert_eq!(source["state"], "GREW_AFTER_RECORDING");
    assert_eq!(source["unrecorded_bytes"], extra.len());
    assert_eq!(
        source["recorded_sha256"],
        before["projects"][PROJECT]["sources"][0]["recorded_sha256"]
    );
    assert_eq!(source["current_bytes"], RECORD.len() + extra.len());
    assert!(after["projects"][PROJECT]["raw_mismatches"]
        .as_array()
        .expect("mismatches")
        .is_empty());
}

#[test]
fn source_gone_json_retains_intact_seal_and_chain_separately() {
    let f = Fixture::new(true);
    fs::remove_file(&f.source).expect("authored removal");
    let value = doc(&f, 2);
    let project = &value["projects"][PROJECT];
    assert_eq!(project["sources"][0]["state"], "SOURCE_GONE");
    assert_eq!(project["sources"][0]["sealed_copy"], "INTACT");
    assert!(
        project["raw_matches_via_mirror"]
            .as_u64()
            .expect("mirror matches")
            > 0
    );
    assert!(project["chain_mismatches"]
        .as_array()
        .expect("chain mismatches")
        .is_empty());
    assert!(project["digest_mismatches"]
        .as_array()
        .expect("digest mismatches")
        .is_empty());
    assert_eq!(
        project["swept_files_preserved"]
            .as_array()
            .expect("verified mirror preservation")
            .len(),
        1
    );
    assert!(project["missing_files"]
        .as_array()
        .expect("unanswered files")
        .is_empty());
}

#[test]
fn corrupted_sealed_payload_is_failure_even_with_an_intact_original() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(true);
    let path = payload(&f);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("synthetic permission");
    fs::write(&path, b"authored corrupt sealed copy\n").expect("synthetic corruption");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).expect("restore private mode");
    let value = doc(&f, 1);
    assert_eq!(value["projects"][PROJECT]["sources"][0]["state"], "INTACT");
    assert_eq!(
        value["projects"][PROJECT]["sources"][0]["sealed_copy"],
        "CORRUPT"
    );
    assert!(!value["projects"][PROJECT]["digest_mismatches"]
        .as_array()
        .expect("mismatches")
        .is_empty());
}

#[test]
fn missing_original_and_seal_are_unverifiable_not_a_mismatch() {
    let f = Fixture::new(true);
    use std::os::unix::fs::PermissionsExt;
    let path = payload(&f);
    let parent = path.parent().expect("authored generation directory");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .expect("synthetic removal permission");
    fs::remove_file(&path).expect("synthetic seal payload removal");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o500))
        .expect("restore private generation mode");
    fs::remove_file(&f.source).expect("synthetic source removal");
    let value = doc(&f, 2);
    let project = &value["projects"][PROJECT];
    assert_eq!(project["sources"][0]["state"], "SOURCE_GONE");
    assert_eq!(project["sources"][0]["sealed_copy"], "MISSING");
    assert!(project["swept_files_preserved"]
        .as_array()
        .expect("preserved files")
        .is_empty());
    assert!(!project["missing_files"]
        .as_array()
        .expect("missing files")
        .is_empty());
    assert!(project["raw_mismatches"]
        .as_array()
        .expect("raw mismatches")
        .is_empty());
    assert!(project["digest_mismatches"]
        .as_array()
        .expect("digest mismatches")
        .is_empty());
}

#[test]
fn chain_corruption_dominates_source_gone_exit_two() {
    let f = Fixture::new(true);
    fs::remove_file(&f.source).expect("synthetic source removal");
    let conn = rusqlite::Connection::open(&f.db).expect("synthetic DB");
    let trigger: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='events_no_update'",
            [],
            |r| r.get(0),
        )
        .expect("trigger");
    conn.execute_batch("DROP TRIGGER events_no_update")
        .expect("synthetic tamper setup");
    conn.execute("UPDATE events SET chain_hash=?1", ["0".repeat(64)])
        .expect("synthetic tamper");
    conn.execute_batch(&trigger).expect("restore exact schema");
    drop(conn);
    let value = doc(&f, 1);
    assert_eq!(
        value["projects"][PROJECT]["sources"][0]["state"],
        "SOURCE_GONE"
    );
    assert!(!value["projects"][PROJECT]["chain_mismatches"]
        .as_array()
        .expect("chain mismatches")
        .is_empty());
}

#[test]
fn missing_byte_s_retains_an_observed_legacy_digest_discrepancy_as_unverifiable() {
    let f = Fixture::new(true);
    let conn = rusqlite::Connection::open(&f.db).expect("synthetic DB");
    conn.execute("DELETE FROM session_generation_heads", [])
        .expect("model legacy metadata");
    conn.execute("UPDATE sessions SET raw_digest=?1", ["0".repeat(64)])
        .expect("known bad legacy digest");
    drop(conn);
    let value = doc(&f, 2);
    assert_eq!(
        value["projects"][PROJECT]["sources"][0]["state"],
        "UNVERIFIABLE"
    );
    assert_eq!(
        value["projects"][PROJECT]["sources"][0]["recorded_bytes"],
        serde_json::Value::Null
    );
    assert!(value["projects"][PROJECT]["digest_mismatches"]
        .as_array()
        .expect("digest mismatches")
        .is_empty());
    assert!(value["projects"][PROJECT]["unverifiable"]
        .as_array()
        .expect("reasons")
        .iter()
        .any(|r| r
            .as_str()
            .is_some_and(|s| s.contains("raw_digest") && s.contains("byte-boundary"))));
}

#[test]
fn legacy_prefix_line_digest_excludes_later_complete_lines_but_is_unverifiable_without_s() {
    let f = Fixture::new(true);
    let conn = rusqlite::Connection::open(&f.db).expect("synthetic DB");
    conn.execute("DELETE FROM session_generation_heads", [])
        .expect("model legacy metadata");
    drop(conn);
    f.append(b"not recorded in the legacy row\n");
    let value = doc(&f, 2);
    assert_eq!(
        value["projects"][PROJECT]["sources"][0]["state"],
        "UNVERIFIABLE"
    );
    assert!(value["projects"][PROJECT]["digest_mismatches"]
        .as_array()
        .expect("digest mismatches")
        .is_empty());
}

#[test]
fn source_symlink_is_unverifiable_without_reading_its_target() {
    let f = Fixture::new(true);
    let target = f.source.with_extension("authored-target");
    fs::rename(&f.source, &target).expect("synthetic move");
    std::os::unix::fs::symlink(&target, &f.source).expect("synthetic symlink");
    let value = doc(&f, 2);
    assert_eq!(
        value["projects"][PROJECT]["sources"][0]["state"],
        "UNVERIFIABLE"
    );
    assert_eq!(
        value["projects"][PROJECT]["sources"][0]["prefix_sha256"],
        serde_json::Value::Null
    );
}

#[test]
fn source_fifo_is_refused_without_blocking_or_opening_a_live_store() {
    use std::time::{Duration, Instant};
    let f = Fixture::new(true);
    fs::remove_file(&f.source).expect("synthetic removal");
    let status = Command::new("/usr/bin/mkfifo")
        .arg(&f.source)
        .status()
        .expect("synthetic FIFO");
    assert!(status.success());
    let mut child = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .env("HOME", &f.home)
        .env("THRULINE_HOME", f.home.join(".thruline"))
        .arg("--db")
        .arg(&f.db)
        .args(["verify", "--project", PROJECT, "--json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("native CLI");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().expect("child status").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop blocked synthetic child");
            let _ = child.wait();
            panic!("FIFO source blocked beyond the bounded synthetic control");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().expect("native output");
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(
        value["projects"][PROJECT]["sources"][0]["state"],
        "UNVERIFIABLE"
    );
}

#[test]
fn a_legacy_unterminated_line_append_is_unverifiable_and_retains_the_observed_discrepancy() {
    let f = Fixture::new(false);
    let conn = rusqlite::Connection::open(&f.db).expect("synthetic DB");
    conn.execute("DELETE FROM session_generation_heads", [])
        .expect("model legacy metadata");
    drop(conn);
    f.append(b" extension of the old final line\n");
    let value = doc(&f, 2);
    let project = &value["projects"][PROJECT];
    assert_eq!(project["sources"][0]["state"], "UNVERIFIABLE");
    assert!(project["raw_mismatches"]
        .as_array()
        .expect("raw mismatches")
        .is_empty());
    assert!(project["digest_mismatches"]
        .as_array()
        .expect("digest mismatches")
        .is_empty());
    let reasons = project["unverifiable"].as_array().expect("reasons");
    assert!(reasons.iter().any(|r| r
        .as_str()
        .is_some_and(|s| s.contains("raw receipt discrepancy") && s.contains("byte-boundary"))));
    assert!(reasons.iter().any(|r| r
        .as_str()
        .is_some_and(|s| s.contains("raw_digest") && s.contains("byte-boundary"))));
}

#[test]
fn a_legacy_earlier_line_with_a_later_recorded_line_is_a_real_mismatch() {
    let f = Fixture::new(true);
    let second = RECORD
        .replace("authored prefix control", "second bounded control")
        .replace("prefix-user", "second-user");
    f.append(format!("{second}\n").as_bytes());
    let mut ledger = Ledger::open(&f.db).expect("authored ledger");
    let report = ingest(
        &mut ledger,
        &[f.source.parent().expect("project").to_path_buf()],
        &f.db.parent().expect("DB parent").join("mirrors"),
    )
    .expect("second production ingest");
    assert!(report.new_events > 0, "{report:?}");
    drop(ledger);
    let conn = rusqlite::Connection::open(&f.db).expect("synthetic DB");
    conn.execute("DELETE FROM session_generation_heads", [])
        .expect("model legacy metadata");
    drop(conn);
    let content = fs::read_to_string(&f.source).expect("authored bytes");
    fs::write(&f.source, content.replace("authored", "rewrote"))
        .expect("bounded first-line rewrite");
    let value = doc(&f, 1);
    let alarms = value["projects"][PROJECT]["raw_mismatches"]
        .as_array()
        .expect("alarms");
    assert!(alarms.iter().any(|alarm| alarm["line"] == 1));
}

#[test]
fn contradictory_accepted_generation_counts_are_a_real_mismatch_not_unavailable_bytes() {
    let f = Fixture::new(true);
    let conn = rusqlite::Connection::open(&f.db).expect("synthetic DB");
    let trigger: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='session_generations_no_update'",
            [],
            |r| r.get(0),
        )
        .expect("trigger");
    conn.execute_batch("DROP TRIGGER session_generations_no_update")
        .expect("synthetic tamper setup");
    conn.execute("UPDATE session_generations SET total_envelopes=0", [])
        .expect("synthetic contradictory counts");
    conn.execute_batch(&trigger).expect("restore exact schema");
    drop(conn);
    let value = doc(&f, 1);
    assert!(!value["projects"][PROJECT]["digest_mismatches"]
        .as_array()
        .expect("mismatches")
        .is_empty());
}

#[test]
fn missing_legacy_digest_cannot_hide_independent_generation_accounting_corruption() {
    let f = Fixture::new(true);
    let conn = rusqlite::Connection::open(&f.db).expect("synthetic DB");
    let trigger: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='session_generations_no_update'",
            [],
            |r| r.get(0),
        )
        .expect("trigger");
    conn.execute_batch("DROP TRIGGER session_generations_no_update")
        .expect("synthetic tamper setup");
    conn.execute("UPDATE session_generations SET total_envelopes=0", [])
        .expect("synthetic contradictory counts");
    conn.execute_batch(&trigger).expect("restore exact schema");
    conn.execute("UPDATE sessions SET raw_digest=NULL", [])
        .expect("independently absent legacy digest");
    drop(conn);
    let value = doc(&f, 1);
    let project = &value["projects"][PROJECT];
    assert!(project["unverifiable"]
        .as_array()
        .expect("unverifiable reasons")
        .iter()
        .any(|reason| reason
            .as_str()
            .is_some_and(|text| text.contains("no stored digest"))));
    assert!(!project["digest_mismatches"]
        .as_array()
        .expect("independent generation mismatches")
        .is_empty());
}

#[test]
fn missing_payload_does_not_hide_independent_generation_arithmetic_corruption() {
    accounting_with_unavailable_mirror(false);
}

#[test]
fn missing_mirror_root_does_not_hide_independent_generation_sql_accounting_corruption() {
    accounting_with_unavailable_mirror(true);
}

fn accounting_with_unavailable_mirror(root_missing: bool) {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new(true);
    let conn = rusqlite::Connection::open(&f.db).expect("synthetic DB");
    let trigger: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='session_generations_no_update'",
            [],
            |r| r.get(0),
        )
        .expect("trigger");
    conn.execute_batch("DROP TRIGGER session_generations_no_update")
        .expect("synthetic tamper setup");
    conn.execute(
        if root_missing {
            "UPDATE session_generations SET local_envelopes=0"
        } else {
            "UPDATE session_generations SET total_envelopes=0"
        },
        [],
    )
    .expect("synthetic independently provable accounting contradiction");
    conn.execute_batch(&trigger).expect("restore exact schema");
    drop(conn);
    if root_missing {
        let mirrors = f.db.parent().expect("authored parent").join("mirrors");
        fs::rename(&mirrors, mirrors.with_file_name("unavailable-mirrors"))
            .expect("synthetic unavailable mirror root");
    } else {
        let path = payload(&f);
        let parent = path.parent().expect("authored generation directory");
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .expect("synthetic removal permission");
        fs::remove_file(&path).expect("synthetic payload removal");
        fs::set_permissions(parent, fs::Permissions::from_mode(0o500))
            .expect("restore private generation mode");
    }
    let value = doc(&f, 1);
    let project = &value["projects"][PROJECT];
    assert_eq!(project["sources"][0]["state"], "INTACT");
    assert!(!project["digest_mismatches"]
        .as_array()
        .expect("independent metadata contradictions")
        .is_empty());
}
