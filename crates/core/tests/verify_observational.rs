//! Store replacement must compare verification verdicts without editing the
//! rows it promises to preserve.

use std::fs;
use std::path::{Path, PathBuf};

use thruline_core::ingest::{
    ingest, ingest_codex, verify, verify_observational, verify_observational_with_mirror_root,
    CodexIngestOptions,
};
use thruline_core::ledger::Ledger;

type MetaRow = (
    i64,
    String,
    String,
    i64,
    Option<String>,
    Option<i64>,
    Option<i64>,
);

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_code_sessions.jsonl")
}

fn scratch_project(root: &Path) -> PathBuf {
    let project = root.join("-Users-dev-demo");
    fs::create_dir_all(&project).expect("create scratch project");
    let session = project.join("cccccccc-1111-4222-8333-444444444444.jsonl");
    fs::copy(fixture(), &session).expect("copy fixture into scratch project");
    session
}

fn ingest_scratch(db: &Path, root: &Path) -> Ledger {
    let mut ledger = Ledger::open(db).expect("open scratch ledger");
    ingest(
        &mut ledger,
        &[root.to_path_buf()],
        &db.parent().unwrap().join("mirrors"),
    )
    .expect("ingest scratch fixture");
    ledger
}

fn metadata_rows(ledger: &Ledger) -> Vec<MetaRow> {
    let mut statement = ledger
        .conn()
        .prepare(
            "SELECT rowid,project_id,head_hash,event_count,verified_at,chain_intact,raw_files_missing
             FROM ledger_meta ORDER BY project_id",
        )
        .expect("prepare metadata census");
    statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })
        .expect("query metadata census")
        .map(|row| row.expect("metadata row"))
        .collect()
}

#[test]
fn observational_pass_keeps_metadata_and_matches_recorded_verdict() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let root = dir.path().join("projects");
    scratch_project(&root);
    let ledger = ingest_scratch(&dir.path().join("ledger.db"), &root);
    ledger
        .conn()
        .execute(
            "UPDATE ledger_meta SET verified_at='2000-01-01T00:00:00Z',
                                    chain_intact=0,raw_files_missing=7",
            [],
        )
        .expect("seed visibly stale verification metadata");
    let before = metadata_rows(&ledger);

    ledger
        .conn()
        .execute_batch("PRAGMA query_only=ON")
        .expect("enforce read-only verification");
    let observed = verify_observational(&ledger, None).expect("observe verification");
    assert!(observed.ok(), "{observed:#?}");
    assert_eq!(
        metadata_rows(&ledger),
        before,
        "observation edited a kept row"
    );

    ledger
        .conn()
        .execute_batch("PRAGMA query_only=OFF")
        .expect("restore scratch write access");
    let recorded = verify(&ledger, None).expect("record verification");
    assert_eq!(observed.projects, recorded.projects);
    let after = metadata_rows(&ledger);
    assert_ne!(after, before, "ordinary verify must record its verdict");
    assert_ne!(after[0].4.as_deref(), Some("2000-01-01T00:00:00Z"));
    assert_eq!(after[0].5, Some(1));
    assert_eq!(after[0].6, Some(0));
}

#[test]
fn observational_failure_does_not_clear_an_older_success() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let root = dir.path().join("projects");
    let session = scratch_project(&root);
    let ledger = ingest_scratch(&dir.path().join("ledger.db"), &root);
    assert!(verify(&ledger, None).expect("baseline verify").ok());
    let before = metadata_rows(&ledger);
    assert!(before[0].4.is_some(), "baseline success must be recorded");

    fs::write(&session, b"changed fixture contents\n").expect("change scratch source");
    ledger
        .conn()
        .execute_batch("PRAGMA query_only=ON")
        .expect("enforce read-only verification");
    let observed = verify_observational(&ledger, None).expect("observe failed verification");
    assert!(!observed.ok(), "changed source must fail");
    assert_eq!(
        metadata_rows(&ledger),
        before,
        "observation cleared success"
    );

    ledger
        .conn()
        .execute_batch("PRAGMA query_only=OFF")
        .expect("restore scratch write access");
    let recorded = verify(&ledger, None).expect("record failed verification");
    assert_eq!(observed.projects, recorded.projects);
    assert!(
        metadata_rows(&ledger)[0].4.is_none(),
        "ordinary verify clears success"
    );
}

#[test]
fn explicit_mirror_root_cannot_fall_back_to_a_different_store() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let root = dir.path().join("projects");
    let session = scratch_project(&root);
    let ledger = ingest_scratch(&dir.path().join("ledger.db"), &root);
    fs::remove_file(&session).expect("remove synthetic vendor source");
    let good = verify_observational_with_mirror_root(&ledger, None, &dir.path().join("mirrors"))
        .expect("explicit sealed mirror");
    assert!(good.ok(), "{good:#?}");
    let absent = dir.path().join("different-mirrors");
    let bad = verify_observational_with_mirror_root(&ledger, None, &absent)
        .expect("wrong explicit mirror root verdict");
    assert!(
        !bad.ok(),
        "explicit root must not fall back to the available sibling mirror"
    );
    assert!(
        !absent.exists(),
        "verification must not create its mirror root"
    );
}

#[test]
fn explicit_codex_mirror_root_isolates_head_and_predecessor_payloads() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let source = dir.path().join("rollout.jsonl");
    let original_db = dir.path().join("original.db");
    let copied_db = dir.path().join("copied.db");
    let original_mirrors = dir.path().join("original-mirrors");
    let copied_mirrors = dir.path().join("copied-mirrors");
    let options = CodexIngestOptions {
        mirror_root: original_mirrors.clone(),
        owner_key: Some("synthetic-isolation-owner".to_owned()),
    };
    let mut bytes = concat!(
        "{\"timestamp\":\"2026-08-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"synthetic-isolation-session\",\"cwd\":\"/synthetic/isolation-project\"}}\n",
        "{\"timestamp\":\"2026-08-01T00:00:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"m1\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"First synthetic answer\"}]}}\n"
    )
    .as_bytes()
    .to_vec();
    fs::write(&source, &bytes).expect("write first generation");
    let mut original = Ledger::open(&original_db).expect("original scratch ledger");
    let first = ingest_codex(&mut original, std::slice::from_ref(&source), &options)
        .expect("ingest first generation");
    assert_eq!(first.accepted_generations, 1);
    bytes.extend_from_slice(b"{\"timestamp\":\"2026-08-01T00:00:02Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"m2\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Second synthetic answer\"}]}}\n");
    fs::write(&source, &bytes).expect("write successor generation");
    let second = ingest_codex(&mut original, std::slice::from_ref(&source), &options)
        .expect("ingest successor generation");
    assert_eq!(second.accepted_generations, 1);
    let project = original.project_ids().expect("projects").remove(0);
    let session = original
        .sessions_of(&project)
        .expect("sessions")
        .remove(0)
        .0;
    let lineage = original
        .accepted_generation_lineage(&session)
        .expect("accepted lineage");
    assert_eq!(lineage.len(), 2);
    let receipt_paths: std::collections::BTreeSet<_> = original
        .session_chain(&session)
        .expect("event receipts")
        .into_iter()
        .map(|link| link.raw_file)
        .collect();
    assert_eq!(
        receipt_paths.len(),
        2,
        "both generations must have receipts"
    );
    let authority = original.mirror_authority_key().expect("mirror authority");
    let original_namespace = original_mirrors.join("ledgers").join(&authority);
    let copied_namespace = copied_mirrors.join("ledgers").join(&authority);
    for generation in &lineage {
        for relative in [
            &generation.mirror_payload_path,
            &generation.mirror_seal_path,
        ] {
            let target = copied_namespace.join(relative);
            fs::create_dir_all(target.parent().expect("mirror parent"))
                .expect("copied mirror directory");
            fs::copy(original_namespace.join(relative), &target)
                .expect("copy synthetic accepted mirror");
        }
    }
    drop(original);
    fs::copy(&original_db, &copied_db).expect("copy closed synthetic ledger");
    let copied_bytes = fs::read(&copied_db).expect("copied ledger bytes");
    let copied = Ledger::open_existing_read_only(&copied_db).expect("read-only copied ledger");
    let metadata_before = metadata_rows(&copied);
    let baseline = verify_observational_with_mirror_root(&copied, None, &copied_mirrors)
        .expect("verify copied mirror before original changes");
    assert!(baseline.ok(), "{baseline:#?}");

    // An intact original cannot answer for either missing copied generation.
    for generation in &lineage {
        let payload = copied_namespace.join(&generation.mirror_payload_path);
        let withheld = payload.with_extension("withheld");
        fs::rename(&payload, &withheld).expect("withhold copied payload");
        let missing = verify_observational_with_mirror_root(&copied, None, &copied_mirrors)
            .expect("verify missing copied generation");
        assert!(
            !missing.ok(),
            "missing copied generation passed: {missing:#?}"
        );
        assert!(
            !missing.projects[&project].raw_mismatches.is_empty(),
            "raw receipts were answered by another generation or the original mirror"
        );
        fs::rename(&withheld, &payload).expect("restore copied payload");
    }

    // Poison the old absolute paths still named by both generations' event
    // receipts and by sessions.file_path. Only the explicit mirror is intact.
    for generation in &lineage {
        let payload = original_namespace.join(&generation.mirror_payload_path);
        let parent = payload.parent().expect("synthetic generation directory");
        let directory_permissions = fs::metadata(parent)
            .expect("synthetic generation metadata")
            .permissions();
        let payload_permissions = fs::metadata(&payload)
            .expect("synthetic payload metadata")
            .permissions();
        let line_count = fs::read_to_string(&payload)
            .expect("synthetic original payload")
            .lines()
            .count();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
                .expect("temporarily allow replacement in synthetic sealed directory");
        }
        fs::remove_file(&payload).expect("remove synthetic original payload");
        fs::write(&payload, "changed original mirror\n".repeat(line_count))
            .expect("poison original payload without triggering missing-line discovery");
        fs::set_permissions(&payload, payload_permissions)
            .expect("restore synthetic payload permissions");
        fs::set_permissions(parent, directory_permissions)
            .expect("restore synthetic sealed directory permissions");
    }
    let original = Ledger::open(&original_db).expect("reopen original scratch ledger");
    assert!(
        !verify(&original, None).expect("ordinary verification").ok(),
        "ordinary verification must still detect changed original mirrors"
    );
    let isolated = verify_observational_with_mirror_root(&copied, None, &copied_mirrors)
        .expect("verify with explicitly isolated mirrors");
    assert_eq!(
        isolated.projects, baseline.projects,
        "explicit mirror verification followed an original absolute payload path"
    );
    assert_eq!(metadata_rows(&copied), metadata_before);
    drop(copied);
    assert_eq!(
        fs::read(&copied_db).expect("copied ledger bytes"),
        copied_bytes
    );
}

#[test]
fn copied_store_has_same_verdict_until_its_digest_row_changes() {
    let dir = tempfile::tempdir().expect("scratch dir");
    let root = dir.path().join("projects");
    scratch_project(&root);
    let source = dir.path().join("ledger.db");
    let candidate = dir.path().join("ledger.db.removing-test");
    let ledger = ingest_scratch(&source, &root);
    drop(ledger);
    fs::copy(&source, &candidate).expect("copy closed scratch store");

    let source_ledger = Ledger::open(&source).expect("reopen source");
    let candidate_ledger = Ledger::open(&candidate).expect("open copied candidate");
    source_ledger
        .conn()
        .execute_batch("PRAGMA query_only=ON")
        .expect("enforce read-only source verification");
    candidate_ledger
        .conn()
        .execute_batch("PRAGMA query_only=ON")
        .expect("enforce read-only candidate verification");
    let baseline = verify_observational(&source_ledger, None).expect("source verdict");
    let copied = verify_observational(&candidate_ledger, None).expect("candidate verdict");
    assert_eq!(baseline.projects, copied.projects);

    candidate_ledger
        .conn()
        .execute_batch("PRAGMA query_only=OFF")
        .expect("allow deliberate scratch corruption");
    candidate_ledger
        .conn()
        .execute("UPDATE sessions SET raw_digest='wrong-digest'", [])
        .expect("corrupt scratch candidate digest");
    let changed = verify_observational(&candidate_ledger, None).expect("changed verdict");
    assert_ne!(baseline.projects, changed.projects);
    assert!(changed.projects.values().any(|verdict| {
        verdict
            .digest_mismatches
            .iter()
            .any(|message| message.contains("raw_digest"))
    }));
}
