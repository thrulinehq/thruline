//! C1 cannot obtain a display clock from Git or the host.
use std::path::Path;
use thruline_core::decisions::checks_visual::visual_inputs::{
    file_digest, load_fixture_manifest_at, valid_utc_second, VisualFixtureManifest,
};

fn write_manifest(db: &Path, path: &Path, clock: &str) {
    let value = VisualFixtureManifest {
        schema_version: 1,
        record_path: None,
        record_sha256: file_digest(db).unwrap(),
        record_bytes: std::fs::metadata(db).unwrap().len(),
        capture_clock: clock.into(),
        clock_basis: "explicit_fixture".into(),
        clock_witness: None,
        fixture_description: "Explicit empty synthetic ledger for capture tests".into(),
    };
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn missing_manifest_or_clock_is_unverifiable_without_a_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.db");
    std::fs::write(&db, b"synthetic record").unwrap();
    let path = db.with_extension("visual-fixture.json");
    assert_eq!(
        load_fixture_manifest_at(&db, &path).unwrap_err(),
        "record fixture manifest is missing"
    );
    write_manifest(&db, &path, "");
    assert!(load_fixture_manifest_at(&db, &path)
        .unwrap_err()
        .contains("clock"));
}

#[test]
fn explicit_record_fixture_clock_is_stable_and_binds_the_exact_record() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.db");
    std::fs::write(&db, b"synthetic record").unwrap();
    let path = db.with_extension("visual-fixture.json");
    write_manifest(&db, &path, "2026-10-06T23:42:17Z");
    let first = load_fixture_manifest_at(&db, &path).unwrap();
    let second = load_fixture_manifest_at(&db, &path).unwrap();
    assert_eq!(first.manifest.capture_clock, second.manifest.capture_clock);
    assert_eq!(first.manifest_sha256, second.manifest_sha256);
    std::fs::write(&db, b"changed record__").unwrap();
    assert!(load_fixture_manifest_at(&db, &path)
        .unwrap_err()
        .contains("does not match"));
}

#[test]
fn missing_witness_or_a_noncanonical_clock_is_unverifiable() {
    assert!(valid_utc_second("2026-10-06T23:42:17Z"));
    for invalid in [
        "2026-10-06T23:42:17.000Z",
        "2026-02-30T00:00:00Z",
        "2026-10-06T23:42:17+00:00",
        "",
    ] {
        assert!(!valid_utc_second(invalid), "{invalid}");
    }
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.db");
    std::fs::write(&db, b"synthetic record").unwrap();
    let path = db.with_extension("visual-fixture.json");
    write_manifest(&db, &path, "2026-10-06T23:42:17Z");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["clock_basis"] = serde_json::json!("record_event_ceiling");
    std::fs::write(&path, value.to_string()).unwrap();
    assert_eq!(
        load_fixture_manifest_at(&db, &path).unwrap_err(),
        "record fixture clock witness is missing"
    );
}

#[cfg(unix)]
#[test]
fn a_public_or_symlinked_manifest_is_unverifiable() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.db");
    std::fs::write(&db, b"synthetic record").unwrap();
    let path = db.with_extension("visual-fixture.json");
    write_manifest(&db, &path, "2026-10-06T23:42:17Z");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(load_fixture_manifest_at(&db, &path)
        .unwrap_err()
        .contains("private"));
    let link = dir.path().join("link.json");
    symlink(&path, &link).unwrap();
    assert!(load_fixture_manifest_at(&db, &link)
        .unwrap_err()
        .contains("invalid"));
}

#[test]
fn dated_record_witness_rounds_up_and_is_checked_against_the_frozen_file() {
    use thruline_core::decisions::checks_visual::visual_inputs::RecordClockWitness;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE events(id TEXT,project_id TEXT,session_id TEXT,seq INTEGER,raw_line INTEGER,ts TEXT,raw_sha256 TEXT,chain_hash TEXT);").unwrap();
    let witness = RecordClockWitness {
        event_id: "event-fixture".into(),
        project_id: "project-fixture".into(),
        session_id: "session-fixture".into(),
        seq: 3,
        raw_line: 4,
        event_timestamp: "2026-10-06T23:42:16.900Z".into(),
        raw_sha256: "a".repeat(64),
        chain_hash: "b".repeat(64),
    };
    conn.execute(
        "INSERT INTO events VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        rusqlite::params![
            witness.event_id,
            witness.project_id,
            witness.session_id,
            witness.seq,
            witness.raw_line,
            witness.event_timestamp,
            witness.raw_sha256,
            witness.chain_hash
        ],
    )
    .unwrap();
    drop(conn);
    let path = db.with_extension("visual-fixture.json");
    write_manifest(&db, &path, "2026-10-06T23:42:17Z");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["clock_basis"] = serde_json::json!("record_event_ceiling");
    value["clock_witness"] = serde_json::to_value(witness).unwrap();
    std::fs::write(&path, value.to_string()).unwrap();
    assert_eq!(
        load_fixture_manifest_at(&db, &path)
            .unwrap()
            .manifest
            .capture_clock,
        "2026-10-06T23:42:17Z"
    );
    value["capture_clock"] = serde_json::json!("2026-10-06T23:42:16Z");
    std::fs::write(&path, value.to_string()).unwrap();
    assert!(load_fixture_manifest_at(&db, &path)
        .unwrap_err()
        .contains("does not bound"));
    value["capture_clock"] = serde_json::json!("2026-10-06T23:42:17Z");
    value["clock_witness"]["raw_line"] = serde_json::json!(5);
    std::fs::write(&path, value.to_string()).unwrap();
    assert!(load_fixture_manifest_at(&db, &path)
        .unwrap_err()
        .contains("does not match"));
}

#[test]
fn immutable_reads_and_vacuum_do_not_create_source_sidecars_or_allow_writes() {
    use thruline_core::decisions::checks_visual::visual_inputs::open_frozen_record;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("frozen.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TABLE fixture(value TEXT); INSERT INTO fixture VALUES('fixed');")
        .unwrap();
    drop(conn);
    let source = open_frozen_record(&db).unwrap();
    assert_eq!(
        source
            .query_row("SELECT value FROM fixture", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "fixed"
    );
    assert!(source
        .execute("INSERT INTO fixture VALUES('mutable')", [])
        .is_err());
    let snapshot = dir.path().join("snapshot.db");
    source
        .execute("VACUUM main INTO ?1", [snapshot.to_string_lossy().as_ref()])
        .unwrap();
    assert!(snapshot.is_file());
    assert!(!dir.path().join("frozen.db-wal").exists());
    assert!(!dir.path().join("frozen.db-shm").exists());
}

#[test]
fn cached_fixture_requires_unchanged_record_manifest_and_empty_wal() {
    use thruline_core::decisions::checks_visual::visual_cache::{key, VisualCache};
    use thruline_core::decisions::checks_visual::visual_inputs::{
        load_cached_fixture_manifest_at, FrozenVisualInput,
    };
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ledger.db");
    std::fs::write(&db, b"synthetic record").unwrap();
    let path = db.with_extension("visual-fixture.json");
    write_manifest(&db, &path, "2026-10-06T23:42:17Z");
    let cache = VisualCache::at(dir.path().join("cache")).unwrap();
    let first = load_cached_fixture_manifest_at(&db, &path, &cache).unwrap();
    let identity = key(&(path.clone(), first.manifest_sha256.clone(), db.clone()));
    assert!(
        cache
            .load::<FrozenVisualInput>("fixture", &identity)
            .is_some(),
        "a full digest verification must leave a metadata-bound receipt"
    );
    let second = load_cached_fixture_manifest_at(&db, &path, &cache).unwrap();
    assert_eq!(first.record_stamp, second.record_stamp);
    std::fs::write(format!("{}-wal", db.display()), b"unbound new work").unwrap();
    assert!(load_cached_fixture_manifest_at(&db, &path, &cache).is_err());
    std::fs::remove_file(format!("{}-wal", db.display())).unwrap();
    std::fs::write(&db, b"different record").unwrap();
    assert!(load_cached_fixture_manifest_at(&db, &path, &cache).is_err());
}
