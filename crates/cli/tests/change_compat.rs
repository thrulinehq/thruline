//! T-2-1 — the compatibility contract of the Change migration, at the process
//! level (`B11.4`, `B3.15`, A168/A193).
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** The first two tests are that sentence. The third is
//! the one that matters more: after the upgrade, the same store answers every
//! question byte for byte the way it did before — the same claims, the same
//! explanation, the same sessions, the same export, the same deep link.
//!
//! Every store here is created under a temporary `HOME` inside a tempdir. No
//! test in this file can reach `~/.thruline` or any other real store: `HOME`
//! and `THRULINE_HOME` are overridden on every child, and `--db` is explicit.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::tempdir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// 0700, which the mirror root is required to be before `ingest` will use it.
fn private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
}

fn core_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures")
        .join(name)
}

/// Every child runs with `HOME` inside the test's own tempdir, so nothing can
/// open a real store by falling back to a default.
fn run(home: &Path, db: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(bin());
    command
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .arg("--db")
        .arg(db)
        .args(args)
        .stdin(Stdio::null());
    command.output().expect("running the CLI")
}

/// A transcript root holding one synthetic session, ingested into `db`.
fn ingest_a_session(home: &Path, db: &Path) -> PathBuf {
    let root = home.join("projects");
    let project = root.join("-Users-dev-demo");
    std::fs::create_dir_all(&project).expect("mkdir");
    std::fs::copy(
        core_fixture("claude_code_sessions.jsonl"),
        project.join("cccccccc-0000-4000-8000-000000000001.jsonl"),
    )
    .expect("copy the fixture");
    // The mirror root must already be private (0700), and it is created here
    // rather than defaulted so nothing in this test can write under a real
    // `~/.thruline`.
    let mirrors = home.join(".thruline/mirrors");
    std::fs::create_dir_all(&mirrors).expect("mkdir mirrors");
    private(&mirrors);
    private(&home.join(".thruline"));
    let out = run(
        home,
        db,
        &[
            "ingest",
            "--apply",
            "--source",
            "claude-code",
            "--mirror-root",
            mirrors.to_str().unwrap_or_default(),
            root.to_str().unwrap_or_default(),
        ],
    );
    assert!(
        out.status.success(),
        "ingest failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    root
}

/// A store carrying the schema this task supersedes, stamped with that
/// schema's own fingerprint — not an empty file and not a store with its
/// fingerprint deleted, but the thing a person actually has on disk the moment
/// they install this binary.
fn store_on_the_previous_schema(dir: &Path) -> PathBuf {
    let db = dir.join("ledger.db");
    let sql = std::fs::read_to_string(core_fixture("schema-head.sql"))
        .expect("the previous schema fixture must be readable");
    let conn = rusqlite::Connection::open(&db).expect("open");
    conn.execute_batch(&sql).expect("apply the previous schema");
    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations(version) VALUES(?1)",
        ["schema_sha256_744652141e5feb3e3a754582ab7f08899941251774aa4d032c7a0b5d96773be3"],
    )
    .expect("stamp the previous schema's own fingerprint");
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('cccccccc-0000-4000-8000-00000000p001', '/tmp/synthetic-project',
                'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("a store with something in it");
    drop(conn);
    db
}

fn fingerprints(db: &Path) -> i64 {
    let conn = rusqlite::Connection::open(db).expect("open");
    conn.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .expect("count")
}

/// The verbs that open the CORE ledger, which is the store this migration
/// changes. Each inherits the refusal from `Ledger::open`, without any of them
/// having had to remember it.
///
/// `claims` and `explain` are deliberately **not** here, and the reason is a
/// measurement rather than an oversight: on this route they read the execution
/// store (`--exec-db`) and never open the core ledger, so a core-schema change
/// neither refuses them nor alters their answer. That they are byte-identical
/// across this change is established by `scripts/hq-invariance.sh`, which
/// captures `claims --json`, `explain --json` and every receipt for the spool
/// fixtures and diffs two refs.
fn reading_verbs(export_out: &Path) -> Vec<Vec<String>> {
    vec![
        vec!["now".into()],
        vec!["projects".into()],
        vec!["verify".into()],
        vec!["rebuild".into()],
        // `--out` is always explicit: `export --all` defaults to
        // `ui/public/fixtures`, and a test that let it default would write into
        // the repository.
        vec![
            "export".into(),
            "--all".into(),
            "--out".into(),
            export_out.display().to_string(),
        ],
    ]
}

#[test]
fn every_reading_verb_refuses_with_exit_3_until_the_upgrade_runs() {
    let dir = tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let db = store_on_the_previous_schema(dir.path());

    for verb in reading_verbs(&home.join("export-never-written")) {
        let args: Vec<&str> = verb.iter().map(|s| s.as_str()).collect();
        let out = run(&home, &db, &args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            out.status.code(),
            Some(3),
            "`{}` must exit 3 on a store whose schema moved. stderr: {stderr}",
            verb.join(" ")
        );
        assert!(
            stderr.contains("schema upgrade pending") && stderr.contains("thruline upgrade"),
            "`{}` must name the state and the one command that fixes it: {stderr}",
            verb.join(" ")
        );
        assert_eq!(
            stderr.lines().filter(|l| !l.trim().is_empty()).count(),
            1,
            "`{}` must refuse in one line, not an error report: {stderr}",
            verb.join(" ")
        );
    }

    assert_eq!(
        fingerprints(&db),
        1,
        "a refused command must not stamp, drop or pay for a schema"
    );
}

/// The sensor is someone else's hook, on someone else's timeout. A schema
/// change may not make it fail, print, or lose what it was handed.
#[test]
fn the_sensor_stays_silent_and_exits_zero_on_a_store_whose_schema_moved() {
    let dir = tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let db = store_on_the_previous_schema(dir.path());

    let mut child = Command::new(bin())
        .env("HOME", &home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .arg("--db")
        .arg(&db)
        .args(["sensor", "append", "SessionStart"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the sensor");
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(br#"{"session_id":"t21-probe","hook_event_name":"SessionStart"}"#)
            .expect("write the payload");
    }
    let out = child.wait_with_output().expect("the sensor exits");
    assert_eq!(
        out.status.code(),
        Some(0),
        "the sensor must exit 0 whatever the schema's state: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        "the sensor must print nothing on stdout"
    );
}

/// `thruline upgrade` pays it, and the store ends up carrying exactly this
/// binary's schema plus the Change tables — with the project row it already had
/// still there. `B3.15`: old rows are preserved, never migrated away from.
#[test]
fn the_upgrade_adds_the_change_tables_and_keeps_every_existing_row() {
    let dir = tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let db = store_on_the_previous_schema(dir.path());

    let out = run(&home, &db, &["upgrade"]);
    assert!(
        out.status.success(),
        "upgrade failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let conn = rusqlite::Connection::open(&db).expect("open");
    for table in [
        "changes",
        "change_scope_revisions",
        "change_scope_acceptances",
        "change_implementations",
        "change_session_associations",
    ] {
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .expect("query");
        assert_eq!(found, 1, "{table} must exist after the upgrade");
    }
    let projects: i64 = conn
        .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))
        .expect("count");
    assert_eq!(projects, 1, "the row that was there must still be there");
    let changes: i64 = conn
        .query_row("SELECT COUNT(*) FROM changes", [], |r| r.get(0))
        .expect("count");
    assert_eq!(
        changes, 0,
        "the migration invents no Change for historical work (B3.16)"
    );
    drop(conn);

    // And every verb works again.
    let out = run(&home, &db, &["projects"]);
    assert!(out.status.success(), "{out:?}");
}

/// The one field in the whole export that is not derived from the record: the
/// moment the reading was taken. Two exports of an unchanged store straddling a
/// UTC second differ in it and in nothing else, which made this test
/// probabilistic — it failed roughly one run in a dozen under load, on
/// unmodified files. Pinning it with `export --generated-at` makes the
/// comparison deterministic **without narrowing it**: the stamp is still
/// compared, byte for byte, along with every other byte in the tree, and
/// `the_byte_identity_guard_still_watches_the_stamp_it_pins` proves the guard
/// would see it change. The flag reaches this field and nothing else, and the
/// default is still the wall clock —
/// `the_export_stamps_the_moment_it_ran_unless_it_is_asked_not_to`.
const PINNED_STAMP: &str = "2026-09-17T00:00:00Z";

/// `export --all` into `dir`. `stamp` pins `generated_at`; `None` leaves the
/// export to stamp the moment it ran, which is what every person-facing run
/// does.
fn export_all_into(home: &Path, db: &Path, dir: &Path, stamp: Option<&str>) -> Output {
    let dir = dir.to_str().unwrap_or_default().to_string();
    let mut args: Vec<&str> = vec!["export", "--all", "--out", &dir];
    if let Some(stamp) = stamp {
        args.push("--generated-at");
        args.push(stamp);
    }
    run(home, db, &args)
}

/// Every file under `root`, relative path and bytes, in a deterministic order.
fn listing(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let relative = path
                    .strip_prefix(root)
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                found.push((relative, bytes));
            }
        }
    }
    found.sort();
    found
}

/// Every way two export trees differ, as sentences. Empty means byte-identical:
/// same files, same names, same sizes, same bytes, no field excluded from the
/// comparison. This is the guard itself, factored out so the negative controls
/// can point it at a tree that *should* differ and watch it say so.
fn differences(before: &Path, after: &Path) -> Vec<String> {
    let listed_before = listing(before);
    let listed_after = listing(after);
    let mut found = Vec::new();
    if listed_before.is_empty() {
        found.push("the export produced no files, so comparing them proves nothing".to_string());
        return found;
    }
    if listed_before.len() != listed_after.len() {
        found.push(format!(
            "the export changed shape: {} file(s) before, {} after",
            listed_before.len(),
            listed_after.len()
        ));
        return found;
    }
    for ((name_before, bytes_before), (name_after, bytes_after)) in
        listed_before.iter().zip(listed_after.iter())
    {
        if name_before != name_after {
            found.push(format!(
                "the export changed its file names: {name_before} became {name_after}"
            ));
            continue;
        }
        if bytes_before.len() != bytes_after.len() {
            found.push(format!(
                "{name_before} changed size: {} bytes before, {} after",
                bytes_before.len(),
                bytes_after.len()
            ));
            continue;
        }
        if bytes_before != bytes_after {
            found.push(format!("{name_before} is not byte-identical"));
        }
    }
    found
}

/// The compatibility test proper. One real store, ingested from a fixture; its
/// answers captured; the schema moved under it; the upgrade run; the answers
/// captured again. The two captures must be identical **byte for byte** —
/// claims, explanation, the session list, every deep link, and the export.
#[test]
fn the_same_store_answers_byte_identically_across_the_upgrade() {
    let dir = tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let db = home.join("ledger.db");
    ingest_a_session(&home, &db);

    // `verify` is not compared byte for byte: it prints its own elapsed
    // milliseconds, so two runs of it differ for a reason that has nothing to
    // do with this migration. It is in the refusal list above instead, where
    // what is pinned is its exit code.
    let questions: Vec<Vec<String>> = vec![vec!["projects".into()], vec!["now".into()]];

    let capture = |db: &Path| -> Vec<(String, i32, Vec<u8>)> {
        questions
            .iter()
            .map(|q| {
                let args: Vec<&str> = q.iter().map(|s| s.as_str()).collect();
                let out = run(&home, db, &args);
                (q.join(" "), out.status.code().unwrap_or(-1), out.stdout)
            })
            .collect()
    };

    let before = capture(&db);
    for (question, code, body) in &before {
        assert!(
            !body.is_empty(),
            "`{question}` produced nothing before the upgrade (exit {code})"
        );
    }

    // Export is a directory of files, not a stream: captured separately. The
    // stamp is pinned on both captures so the comparison is about the
    // migration and not about which second the two runs landed in.
    let export_before = home.join("export-before");
    let out = export_all_into(&home, &db, &export_before, Some(PINNED_STAMP));
    assert!(
        out.status.success(),
        "export before: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The schema moves under the store. From here, every verb must refuse.
    {
        let conn = rusqlite::Connection::open(&db).expect("open");
        conn.execute("DELETE FROM schema_migrations", [])
            .expect("clear the fingerprint");
    }
    let refused = run(&home, &db, &["projects"]);
    assert_eq!(refused.status.code(), Some(3));

    let upgraded = run(&home, &db, &["upgrade"]);
    assert!(
        upgraded.status.success(),
        "upgrade: {}",
        String::from_utf8_lossy(&upgraded.stderr)
    );

    let after = capture(&db);
    for ((question, code_before, body_before), (_, code_after, body_after)) in
        before.iter().zip(after.iter())
    {
        assert_eq!(
            code_before, code_after,
            "`{question}` changed its exit code across the upgrade"
        );
        assert_eq!(
            String::from_utf8_lossy(body_before),
            String::from_utf8_lossy(body_after),
            "`{question}` did not answer byte-identically across the upgrade"
        );
    }

    let export_after = home.join("export-after");
    let out = export_all_into(&home, &db, &export_after, Some(PINNED_STAMP));
    assert!(
        out.status.success(),
        "export after: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        !listing(&export_before).is_empty(),
        "the export produced no files, so comparing them proves nothing"
    );
    let differences = differences(&export_before, &export_after);
    assert!(
        differences.is_empty(),
        "the export did not survive the upgrade byte for byte:\n{}",
        differences.join("\n")
    );
}

/// Negative control 1 — the guard still watches the field it pins.
///
/// Pinning `generated_at` would be worthless if it amounted to excluding the
/// field: a field nobody compares is a field the guard no longer watches. So
/// the same store is exported twice with two *different* pinned stamps and
/// nothing else changed, and the comparison must fail, naming `index.json` and
/// the project fixture.
#[test]
fn the_byte_identity_guard_still_watches_the_stamp_it_pins() {
    let dir = tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let db = home.join("ledger.db");
    ingest_a_session(&home, &db);

    let first = home.join("export-first");
    let out = export_all_into(&home, &db, &first, Some(PINNED_STAMP));
    assert!(out.status.success(), "{out:?}");

    let second = home.join("export-second");
    let out = export_all_into(&home, &db, &second, Some("2026-09-17T00:00:01Z"));
    assert!(out.status.success(), "{out:?}");

    let differences = differences(&first, &second);
    assert!(
        differences.iter().any(|d| d.starts_with("index.json")),
        "a one-second change in the stamp must be caught, and it was not: {differences:?}"
    );
}

/// Negative control 2 — one other served field, changed, and the guard says so.
///
/// The project's name is a field a person reads on a surface. Change it in the
/// second tree and nothing else, and the comparison must report exactly that
/// file. Same size, so this also proves the comparison reaches the bytes and
/// does not stop at the length.
#[test]
fn the_byte_identity_guard_catches_one_changed_served_field() {
    let dir = tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let db = home.join("ledger.db");
    ingest_a_session(&home, &db);

    let first = home.join("export-first");
    let out = export_all_into(&home, &db, &first, Some(PINNED_STAMP));
    assert!(out.status.success(), "{out:?}");
    let second = home.join("export-second");
    let out = export_all_into(&home, &db, &second, Some(PINNED_STAMP));
    assert!(out.status.success(), "{out:?}");
    assert!(
        differences(&first, &second).is_empty(),
        "two pinned exports of the same store must start identical"
    );

    // One served field, one character, same length.
    let project_dir = second.join("projects");
    let entry = std::fs::read_dir(&project_dir)
        .expect("read the project fixtures")
        .next()
        .expect("no project fixture to mutate")
        .expect("entry");
    let path = entry.path();
    let text = std::fs::read_to_string(&path).expect("read");
    let needle = "\"name\": \"demo\"";
    assert!(
        text.contains(needle),
        "the fixture no longer carries the field this control mutates"
    );
    std::fs::write(&path, text.replacen(needle, "\"name\": \"demq\"", 1)).expect("write");

    let differences = differences(&first, &second);
    assert!(
        differences.iter().any(|d| d.starts_with("projects/")),
        "a changed served field must be caught, and it was not: {differences:?}"
    );
}

/// The pin is an opt-in, and the default did not move: with no flag, the export
/// still stamps the moment it ran.
#[test]
fn the_export_stamps_the_moment_it_ran_unless_it_is_asked_not_to() {
    /// Seconds since the epoch for a `YYYY-MM-DDTHH:MM:SSZ` stamp.
    /// Howard Hinnant's days_from_civil, the inverse of what the export writes.
    fn epoch_of(stamp: &str) -> Option<i64> {
        let n = |from: usize, to: usize| -> Option<i64> { stamp.get(from..to)?.parse().ok() };
        let (y, m, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
        let (h, mi, sec) = (n(11, 13)?, n(14, 16)?, n(17, 19)?);
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        Some(days * 86_400 + h * 3600 + mi * 60 + sec)
    }

    let dir = tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let db = home.join("ledger.db");
    ingest_a_session(&home, &db);

    let out_dir = home.join("export-default");
    let out = export_all_into(&home, &db, &out_dir, None);
    assert!(out.status.success(), "{out:?}");
    let index = std::fs::read_to_string(out_dir.join("index.json")).expect("read the index");
    let marker = "\"generated_at\": \"";
    let at = index.find(marker).expect("the index carries a stamp") + marker.len();
    let stamp = &index[at..at + 20];

    assert_ne!(
        stamp, PINNED_STAMP,
        "the default must be the clock, not the test's pinned value"
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default();
    let written = epoch_of(stamp).expect("the stamp is the canonical UTC form");
    assert!(
        (now - written).abs() <= 300,
        "the default stamp ({stamp}) is not the moment the export ran ({now} epoch seconds)"
    );

    // And with the flag, it is exactly what was asked for.
    let pinned_dir = home.join("export-pinned");
    let out = export_all_into(&home, &db, &pinned_dir, Some(PINNED_STAMP));
    assert!(out.status.success(), "{out:?}");
    let index = std::fs::read_to_string(pinned_dir.join("index.json")).expect("read the index");
    assert!(
        index.contains(&format!("{marker}{PINNED_STAMP}\"")),
        "the pinned stamp is not the one the export wrote"
    );
}

/// A stamp the export could not have written is refused rather than written.
/// An export carrying a time nothing can parse still looks like an export
/// (Rule 12), so the flag fails loud and writes nothing.
#[test]
fn a_generated_at_that_is_not_a_time_is_refused() {
    let dir = tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    let db = home.join("ledger.db");
    ingest_a_session(&home, &db);

    for bad in [
        "yesterday",
        "2026-09-17",
        "2026-09-17T00:00:00",
        "2026-13-01T00:00:00Z",
        "2026-09-17T24:00:00Z",
        "20260917T000000Z",
    ] {
        let out_dir = home.join(format!("export-{}", bad.len()));
        let out = export_all_into(&home, &db, &out_dir, Some(bad));
        assert!(
            !out.status.success(),
            "`--generated-at {bad}` was accepted and should not have been"
        );
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.contains("is not a time this export can write"),
            "the refusal did not say why: {said}"
        );
        assert!(
            !out_dir.join("index.json").is_file(),
            "a refused export still wrote a file"
        );
    }
}
