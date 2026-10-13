//! Display-boundary controls use authored canaries, never actual credentials.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::TempDir;
use thruline_core::ingest::ingest;
use thruline_core::redact::Masked;
use thruline_core::Ledger;

const CANARY_A: &str = concat!("sk-", "TLC44DisplayCanaryAlpha12345Beta67890Gamma");
const CANARY_B: &str = concat!("sk-", "TLC44DisplayCanaryDelta12345Beta67890Gamma");

struct Authored {
    _temp: TempDir,
    home: PathBuf,
    db: PathBuf,
    projects: Vec<String>,
}
impl Authored {
    fn new(projects: Vec<String>) -> Self {
        let temp = tempfile::tempdir().expect("authored private fixture");
        let home = temp.path().join("home");
        fs::create_dir_all(&home).expect("authored HOME");
        let db = temp.path().join("ledger.db");
        let mut roots = Vec::new();
        for (index, project) in projects.iter().enumerate() {
            let root = temp.path().join("projects").join(project);
            assert!(
                fs::create_dir_all(&root).is_ok(),
                "authored directory failed"
            );
            let sid = format!("aaaaaaaa-0000-4000-8000-{:012}", index + 1);
            let line = serde_json::json!({ "type":"user", "message": {"role":"user","content":"authored display control"}, "sessionId":sid, "uuid":format!("display-user-{index}") });
            assert!(
                fs::write(root.join(format!("{sid}.jsonl")), format!("{line}\n")).is_ok(),
                "authored source failed"
            );
            roots.push(root);
        }
        let mut ledger = Ledger::open(&db).expect("authored ledger");
        let result = ingest(&mut ledger, &roots, &temp.path().join("mirrors"));
        assert!(
            result.is_ok(),
            "authored ingestion failed; captured data withheld"
        );
        drop(ledger);
        Self {
            _temp: temp,
            home,
            db,
            projects,
        }
    }
    fn run(&self, json: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_thruline"));
        command
            .env("HOME", &self.home)
            .env("THRULINE_HOME", self.home.join(".thruline"))
            .arg("--db")
            .arg(&self.db)
            .arg("verify");
        if json {
            command.arg("--json");
        }
        command.output().expect("authored native CLI")
    }
    fn no_canaries(out: &Output) {
        let mut captured = out.stdout.clone();
        captured.extend_from_slice(&out.stderr);
        for canary in [CANARY_A, CANARY_B] {
            assert!(
                !captured
                    .windows(canary.len())
                    .any(|bytes| bytes == canary.as_bytes()),
                "display leaked an authored canary; captured bytes withheld"
            );
        }
    }
}

#[test]
fn native_json_and_text_mask_project_labels_locators_and_preserve_distinct_bindings() {
    let f = Authored::new(vec![
        format!("-Users-display_{CANARY_A}"),
        format!("-Users-display_{CANARY_B}"),
    ]);
    for json in [true, false] {
        let out = f.run(json);
        assert_eq!(
            out.status.code(),
            Some(0),
            "authored verification failed; captured bytes withheld"
        );
        Authored::no_canaries(&out);
        if json {
            let doc: serde_json::Value =
                serde_json::from_slice(&out.stdout).expect("one JSON document");
            let projects = doc["projects"].as_object().expect("project mapping");
            assert_eq!(projects.len(), 2);
            let bindings: std::collections::BTreeSet<_> = projects
                .values()
                .map(|p| p["project_identity_sha256"].as_str().expect("identity SHA"))
                .collect();
            assert_eq!(bindings.len(), 2);
            for native in &f.projects {
                assert!(
                    bindings.contains(thruline_core::chain::sha256_hex(native.as_bytes()).as_str())
                );
            }
        }
    }
}

#[test]
fn a_masked_project_key_collision_with_a_safe_native_key_cannot_coalesce_rows() {
    let native = format!("-Users-display_{CANARY_A}");
    let masked = Masked::mask(&native).into_text();
    assert!(masked != native, "authored detector fixture did not match");
    let collision = format!(
        "{masked}#{}",
        thruline_core::chain::sha256_hex(native.as_bytes())
    );
    assert_eq!(
        Masked::mask(&collision).text(),
        collision,
        "collision control is not a safe displayed identifier"
    );
    let f = Authored::new(vec![native, collision]);
    let out = f.run(true);
    assert_eq!(
        out.status.code(),
        Some(0),
        "authored verification failed; captured bytes withheld"
    );
    Authored::no_canaries(&out);
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("one JSON document");
    assert_eq!(
        doc["projects"].as_object().expect("project mapping").len(),
        2
    );
}

#[test]
fn native_auxiliary_sql_decode_unavailability_is_two_and_known_mismatch_is_one() {
    for observation in [true, false] {
        let f = Authored::new(vec!["-Users-auxiliary".into()]);
        let conn = rusqlite::Connection::open(&f.db).expect("authored DB");
        if observation {
            conn.execute("INSERT INTO observations(id,seq,recorded_at,schema_version,observation_type,source_adapter,source_session,repository_identity,observed_at,completeness,source_receipt,prev_hash,chain_hash) VALUES('authored-observation',0,'2026-10-08T00:00:00Z','opaque-version','WORKSPACE_OBSERVED','authored','authored','authored','2026-10-08T00:00:00Z','unknown','{}','zero','zero')", []).expect("authored uninterpretable machine row");
        } else {
            conn.execute_batch("PRAGMA ignore_check_constraints=ON; INSERT INTO run_permissions(id,seq,act,project_id,project_folder,actor,recorded_at,content_sha256,prev_hash,chain_hash) VALUES('authored-permission','opaque-seq','granted','authored','/authored','synthetic','2026-10-08T00:00:00Z','zero','zero','zero')").expect("authored uninterpretable authority fixture");
        }
        drop(conn);
        let out = f.run(true);
        assert_eq!(
            out.status.code(),
            Some(2),
            "uninterpretable auxiliary receipt was called a mismatch; captured bytes withheld"
        );
        let doc: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("one JSON document");
        assert_eq!(
            doc[if observation {
                "observations"
            } else {
                "permissions"
            }]["state"],
            "UNVERIFIABLE"
        );
    }
    let f = Authored::new(vec!["-Users-auxiliary".into()]);
    let conn = rusqlite::Connection::open(&f.db).expect("authored DB");
    conn.execute("INSERT INTO run_permissions(id,seq,act,project_id,project_folder,actor,recorded_at,content_sha256,prev_hash,chain_hash) VALUES('authored-permission',0,'granted','authored','/authored','synthetic','2026-10-08T00:00:00Z','zero','zero','zero')", []).expect("authored contradictory chain fixture");
    drop(conn);
    let out = f.run(true);
    assert_eq!(
        out.status.code(),
        Some(1),
        "known auxiliary mismatch was weakened; captured bytes withheld"
    );
}

#[test]
fn native_auxiliary_error_strings_are_masked_in_json_and_text() {
    let f = Authored::new(vec!["-Users-auxiliary".into()]);
    let conn = rusqlite::Connection::open(&f.db).expect("authored DB");
    conn.execute_batch("PRAGMA ignore_check_constraints=ON")
        .expect("authored malformed row setup");
    conn.execute("INSERT INTO run_permissions(id,seq,act,project_id,project_folder,actor,recorded_at,content_sha256,prev_hash,chain_hash) VALUES('authored-permission',0,?1,'authored','/authored','synthetic','2026-10-08T00:00:00Z','zero','zero','zero')", [CANARY_A]).expect("authored invalid closed-act fixture");
    drop(conn);
    for json in [true, false] {
        Authored::no_canaries(&f.run(json));
    }
}

#[test]
fn schema_pending_remains_the_distinct_global_refusal_three() {
    let f = Authored::new(vec!["-Users-schema".into()]);
    let conn = rusqlite::Connection::open(&f.db).expect("authored DB");
    conn.execute("DELETE FROM schema_migrations", [])
        .expect("authored pending migration");
    drop(conn);
    let out = f.run(true);
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
}

fn unreadable_generation_metadata(f: &Authored, project: &str) {
    let conn = rusqlite::Connection::open(&f.db).expect("authored DB");
    let trigger: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='session_generations_no_update'",
            [],
            |row| row.get(0),
        )
        .expect("authored schema trigger");
    conn.execute_batch("DROP TRIGGER session_generations_no_update")
        .expect("authored malformed metadata setup");
    conn.execute(
        "UPDATE session_generations SET payload_bytes=?1 WHERE project_id=?2",
        [CANARY_A, project],
    )
    .expect("authored SQL type failure");
    conn.execute_batch(&trigger).expect("restore exact schema");
}

#[test]
fn primary_sql_decode_failure_is_registered_masked_and_explicitly_incomplete() {
    let project = format!("-Users-generation_{CANARY_A}");
    let f = Authored::new(vec![project.clone()]);
    unreadable_generation_metadata(&f, &project);
    for json in [true, false] {
        let out = f.run(json);
        assert_eq!(
            out.status.code(),
            Some(2),
            "availability failure was called corruption; captured bytes withheld"
        );
        Authored::no_canaries(&out);
        if json {
            let doc: serde_json::Value =
                serde_json::from_slice(&out.stdout).expect("registered failure document");
            assert_eq!(doc["shape"], "thruline.verify");
            assert_eq!(doc["schema_version"], 1);
            assert_eq!(doc["verification"]["complete"], false);
            assert_eq!(doc["verification"]["state"], "UNVERIFIABLE");
            assert!(doc["verification"]["reason"].is_string());
            assert_eq!(
                doc["projects"].as_object().expect("partial projects").len(),
                1
            );
            let partial = doc["projects"]
                .as_object()
                .expect("partial projects")
                .values()
                .next()
                .expect("partial project");
            assert_eq!(partial["complete"], false);
        } else {
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(text.contains("verification incomplete"));
            assert!(!text.contains("chain          intact"));
        }
    }
}

#[test]
fn an_aborted_verify_preserves_a_previously_proved_partial_mismatch() {
    let known = "-Users-aa-known";
    let unavailable = "-Users-zz-unavailable";
    let f = Authored::new(vec![known.into(), unavailable.into()]);
    let conn = rusqlite::Connection::open(&f.db).expect("authored DB");
    let trigger: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='events_no_update'",
            [],
            |row| row.get(0),
        )
        .expect("authored trigger");
    conn.execute_batch("DROP TRIGGER events_no_update")
        .expect("authored tamper setup");
    conn.execute(
        "UPDATE events SET chain_hash=?1 WHERE project_id=?2",
        ["0".repeat(64), known.into()],
    )
    .expect("authored known chain mismatch");
    conn.execute_batch(&trigger).expect("restore exact schema");
    drop(conn);
    unreadable_generation_metadata(&f, unavailable);
    let out = f.run(true);
    assert_eq!(
        out.status.code(),
        Some(1),
        "known earlier mismatch was softened; captured bytes withheld"
    );
    Authored::no_canaries(&out);
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("registered partial document");
    assert_eq!(doc["verification"]["complete"], false);
    assert_eq!(doc["verification"]["state"], "MISMATCH");
    assert!(!doc["projects"][known]["chain_mismatches"]
        .as_array()
        .expect("known partial mismatch")
        .is_empty());
    assert_eq!(doc["projects"][unavailable]["complete"], false);
}

#[test]
fn verify_only_ledger_open_failure_is_masked_unverifiable_without_partial_success() {
    let mut f = Authored::new(vec!["-Users-open".into()]);
    let blocked_parent = f.db.parent().expect("authored parent").join(CANARY_A);
    fs::write(&blocked_parent, b"authored non-directory").expect("authored unreadable path");
    f.db = blocked_parent.join("ledger.db");
    for json in [true, false] {
        let out = f.run(json);
        assert_eq!(
            out.status.code(),
            Some(2),
            "ledger availability failure was called corruption; captured bytes withheld"
        );
        Authored::no_canaries(&out);
        if json {
            let doc: serde_json::Value =
                serde_json::from_slice(&out.stdout).expect("registered failure document");
            assert_eq!(doc["verification"]["complete"], false);
            assert_eq!(doc["verification"]["state"], "UNVERIFIABLE");
            assert!(doc["projects"]
                .as_object()
                .expect("unopened report")
                .is_empty());
            assert_eq!(doc["observations"]["state"], "UNVERIFIABLE");
            assert_eq!(doc["permissions"]["state"], "UNVERIFIABLE");
        }
    }
}
