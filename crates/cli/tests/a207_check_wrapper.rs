//! A207 (MANDATE-v4 §13 / §33) — the runner wrapper, end to end.
//!
//! The compiled production binary, real temporary git repositories, a real
//! bare remote, real commands, fresh SQLite files. The wrapper's contract
//! under test: exit codes pass through unmodified in every path, streams
//! pass through byte-identical while being digested, pipeline exit status
//! survives shell mode, a pending migration refuses before the child ever
//! runs, wrapper failure is distinguished from child failure, and a REAL
//! captured `git push` still derives no remote fact.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::tempdir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

fn seeded_repo(base: &Path) -> PathBuf {
    let repo = base.join("repo");
    std::fs::create_dir_all(&repo).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("base.txt"), "base\n").expect("write");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "seed"]);
    repo
}

struct Run {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    code: Option<i32>,
}

fn check(db: &Path, cwd: &Path, extra: &[&str], command: &[&str]) -> Run {
    let mut args: Vec<&str> = vec!["--db", db.to_str().expect("db"), "check"];
    args.extend_from_slice(extra);
    args.push("--");
    args.extend_from_slice(command);
    let out = Command::new(bin())
        .args(&args)
        .current_dir(cwd)
        .output()
        .expect("run thruline check");
    Run {
        stdout: out.stdout,
        stderr: out.stderr,
        code: out.status.code(),
    }
}

fn table_counts(db: &Path) -> (i64, i64) {
    let conn = rusqlite::Connection::open(db).expect("open");
    let obs: i64 = conn
        .query_row("SELECT COUNT(*) FROM observations", [], |r| r.get(0))
        .expect("count");
    let bindings: i64 = conn
        .query_row("SELECT COUNT(*) FROM evidence_bindings", [], |r| r.get(0))
        .expect("count");
    (obs, bindings)
}

fn latest_binding_doc(db: &Path) -> serde_json::Value {
    let conn = rusqlite::Connection::open(db).expect("open");
    let doc: String = conn
        .query_row(
            "SELECT doc FROM evidence_bindings ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("one binding row");
    serde_json::from_str(&doc).expect("doc parses")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let mut s = String::with_capacity(64);
    for b in h.finalize() {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[test]
fn exit_codes_pass_through_unmodified_in_every_path() {
    let dir = tempdir().expect("tempdir");
    let repo = seeded_repo(dir.path());
    let db = dir.path().join("ledger.db");

    for (cmd, expected) in [
        (vec!["true"], 0),
        (vec!["false"], 1),
        (vec!["sh", "-c", "exit 42"], 42),
    ] {
        let run = check(&db, &repo, &["--quiet"], &cmd);
        assert_eq!(
            run.code,
            Some(expected),
            "{cmd:?} → stderr: {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    // Signal death: 128 + SIGTERM, the shell convention, with the real
    // signal recorded in the binding rather than a fabricated exit code.
    let run = check(&db, &repo, &["--quiet"], &["sh", "-c", "kill -TERM $$"]);
    assert_eq!(run.code, Some(143), "128 + 15, the documented convention");
    let doc = latest_binding_doc(&db);
    assert_eq!(doc["exit_code"], serde_json::Value::Null);
    assert_eq!(doc["term_signal"], 15);
}

#[test]
fn rapid_and_concurrent_checks_keep_independent_attempt_records() {
    let dir = tempdir().expect("tempdir");
    let repo = seeded_repo(dir.path());
    let db = dir.path().join("ledger.db");

    // Initialize the ledger before concurrent writers arrive. Every following
    // invocation is a new attempt, even when their wall-clock seconds match.
    assert_eq!(check(&db, &repo, &["--quiet"], &["true"]).code, Some(0));
    for expected in [0, 1, 42] {
        let script = format!("exit {expected}");
        let run = check(&db, &repo, &["--quiet"], &["sh", "-c", &script]);
        assert_eq!(
            run.code,
            Some(expected),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
    std::thread::scope(|scope| {
        let jobs: Vec<_> = [0, 1, 42, 7]
            .into_iter()
            .map(|expected| {
                let db = &db;
                let repo = &repo;
                scope.spawn(move || {
                    let script = format!("exit {expected}");
                    let run = check(db, repo, &["--quiet"], &["sh", "-c", &script]);
                    (expected, run)
                })
            })
            .collect();
        for job in jobs {
            let (expected, run) = job.join().expect("check thread");
            assert_eq!(
                run.code,
                Some(expected),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
        }
    });

    let conn = rusqlite::Connection::open(&db).expect("open");
    for state in ["running", "completed"] {
        let (rows, attempts): (i64, i64) = conn.query_row(
            "SELECT count(*), count(DISTINCT run_key) FROM change_run_executions WHERE state=?1",
            [state],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).expect("execution counts");
        assert_eq!(
            (rows, attempts),
            (8, 8),
            "{state} rows must name independent attempts"
        );
    }
    assert_eq!(table_counts(&db).1, 8, "each attempt has its own binding");
}

/// Megabytes on both streams simultaneously: no reordering within a
/// stream, no deadlock, byte-identical passthrough, digests matching both
/// the expected bytes and what the binding recorded.
#[test]
fn megabyte_tee_passes_both_streams_through_byte_identical() {
    let dir = tempdir().expect("tempdir");
    let repo = seeded_repo(dir.path());
    let db = dir.path().join("ledger.db");

    // ~3.1 MB to each stream, interleaved in 4 KiB slabs.
    let script = concat!(
        "import sys\n",
        "out_slab = ('o' * 4095) + '\\n'\n",
        "err_slab = ('e' * 4095) + '\\n'\n",
        "for _ in range(760):\n",
        "    sys.stdout.write(out_slab)\n",
        "    sys.stderr.write(err_slab)\n",
        "sys.stdout.flush(); sys.stderr.flush()\n"
    );
    let expected_out = format!("{}\n", "o".repeat(4095)).repeat(760).into_bytes();
    let expected_err = format!("{}\n", "e".repeat(4095)).repeat(760).into_bytes();

    let run = check(&db, &repo, &["--quiet"], &["python3", "-c", script]);
    assert_eq!(run.code, Some(0));
    assert_eq!(
        run.stdout.len(),
        expected_out.len(),
        "no dropped stdout bytes"
    );
    assert_eq!(
        run.stderr.len(),
        expected_err.len(),
        "no dropped stderr bytes"
    );
    assert_eq!(
        run.stdout, expected_out,
        "stdout passed through byte-identical"
    );
    assert_eq!(
        run.stderr, expected_err,
        "stderr passed through byte-identical"
    );

    let doc = latest_binding_doc(&db);
    assert_eq!(doc["stdout"]["kind"], "captured");
    assert_eq!(doc["stdout"]["sha256"], sha256_hex(&expected_out).as_str());
    assert_eq!(doc["stdout"]["bytes"], expected_out.len() as u64);
    assert_eq!(doc["stderr"]["sha256"], sha256_hex(&expected_err).as_str());
    assert_eq!(doc["stderr"]["bytes"], expected_err.len() as u64);
}

#[test]
fn pipeline_exit_status_survives_shell_mode() {
    let dir = tempdir().expect("tempdir");
    let repo = seeded_repo(dir.path());
    let db = dir.path().join("ledger.db");

    // Without pipefail, `false | cat` exits 0 — the §13/§32 law is that the
    // wrapper must not lose the failure.
    let run = check(&db, &repo, &["--quiet", "--shell"], &["false | cat"]);
    assert_eq!(run.code, Some(1), "the failing stage surfaces");

    let run = check(
        &db,
        &repo,
        &["--quiet", "--shell"],
        &["sh -c 'exit 3' | cat"],
    );
    assert_eq!(run.code, Some(3), "the exact stage status surfaces");

    let doc = latest_binding_doc(&db);
    assert_eq!(doc["mode"], "shell");
    let argv: Vec<String> = doc["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(|v| v.as_str().expect("str").to_string())
        .collect();
    assert!(
        argv.contains(&"pipefail".to_string()),
        "the spawned argv records the pipefail interposition: {argv:?}"
    );
}

/// A REAL push to a REAL bare remote, captured through the wrapper: the
/// execution binds — and still no remote fact exists anywhere in the
/// binding, because a real remote claim requires an independent remote
/// query (later stage; the refusal is what this stage pins).
#[test]
fn a_real_push_through_the_wrapper_derives_no_remote_fact() {
    let dir = tempdir().expect("tempdir");
    let repo = seeded_repo(dir.path());
    let db = dir.path().join("ledger.db");
    let remote = dir.path().join("origin.git");
    git(
        dir.path(),
        &["init", "-q", "--bare", remote.to_str().expect("utf8")],
    );
    git(
        &repo,
        &["remote", "add", "origin", remote.to_str().expect("utf8")],
    );

    let run = check(&db, &repo, &["--quiet"], &["git", "push", "origin", "main"]);
    assert_eq!(
        run.code,
        Some(0),
        "the real push succeeds: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let doc = latest_binding_doc(&db);
    assert_eq!(doc["scope"]["class"], "unclassified");
    assert_eq!(
        doc["exit_code"], 0,
        "the captured execution is real and binds"
    );

    // The refusal, pinned structurally: no key anywhere in the stored
    // document speaks for a remote — the schema has no place for one.
    fn keys_of(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(map) => {
                for (k, val) in map {
                    out.push(k.clone());
                    keys_of(val, out);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    keys_of(item, out);
                }
            }
            _ => {}
        }
    }
    let mut keys = Vec::new();
    keys_of(&doc, &mut keys);
    for banned in ["remote", "ref", "push", "deploy"] {
        assert!(
            !keys.iter().any(|k| k.to_lowercase().contains(banned)),
            "a binding must carry no remote-fact field; found a {banned:?}-like \
             key in {keys:?}"
        );
    }

    // And the pre/post identities bracket cleanly — a push does not touch
    // the worktree, so this binding is even confirmable.
    assert_eq!(doc["pre"]["identity"], doc["post"]["identity"]);

    // The chain holds the full trail and folds.
    let out = Command::new(bin())
        .args(["--db", db.to_str().expect("db"), "verify"])
        .output()
        .expect("verify");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("observations"), "{text}");
}

#[test]
fn wrapper_failure_before_the_spawn_is_125_and_records_nothing() {
    let dir = tempdir().expect("tempdir");
    let repo = seeded_repo(dir.path());
    let db = dir.path().join("ledger.db");

    // Prime the ledger so the counts have a baseline.
    let run = check(&db, &repo, &["--quiet"], &["true"]);
    assert_eq!(run.code, Some(0));
    let before = table_counts(&db);

    // (a) The binary does not exist: the child never ran.
    let run = check(
        &db,
        &repo,
        &["--quiet"],
        &["/nonexistent/definitely-not-a-binary"],
    );
    assert_eq!(run.code, Some(125), "pre-spawn wrapper failure is 125");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("did NOT run"),
        "the refusal says the child never ran: {stderr}"
    );
    assert_eq!(
        table_counts(&db),
        before,
        "nothing may be recorded for a command that never ran"
    );

    // (b) Outside any git worktree: same refusal, same silence.
    let outside = dir.path().join("not-a-repo");
    std::fs::create_dir_all(&outside).expect("mkdir");
    let run = check(&db, &outside, &["--quiet"], &["true"]);
    assert_eq!(run.code, Some(125));
    assert_eq!(table_counts(&db), before);
}

#[test]
fn a_pending_migration_refuses_with_exit_3_before_the_child_runs() {
    let dir = tempdir().expect("tempdir");
    let repo = seeded_repo(dir.path());
    let db = dir.path().join("ledger.db");
    {
        let out = Command::new(bin())
            .args(["--db", db.to_str().expect("db"), "projects"])
            .output()
            .expect("create ledger");
        assert!(out.status.success());
        let conn = rusqlite::Connection::open(&db).expect("open");
        conn.execute("DELETE FROM schema_migrations", [])
            .expect("clearing the fingerprint");
    }

    let marker = repo.join("the-child-ran");
    let marker_arg = format!("touch {}", marker.display());
    let run = check(&db, &repo, &["--quiet"], &["sh", "-c", &marker_arg]);
    assert_eq!(
        run.code,
        Some(3),
        "A193: pending migration is exit 3 — stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("thruline upgrade"),
        "the refusal names the one command that fixes it"
    );
    assert!(
        !marker.exists(),
        "the child must never run when the wrapper refuses pre-spawn"
    );
}

/// Wrapper trouble AFTER the spawn: the child destroys the workspace's
/// .git mid-run. The child's exit code passes through untouched; the
/// binding still lands (its repository identity came from the pre-command
/// observation), with the post edge incomplete and the defect recorded in
/// the hash-covered document.
#[test]
fn post_spawn_wrapper_trouble_keeps_the_childs_code_and_records_the_defect() {
    let dir = tempdir().expect("tempdir");
    let repo = seeded_repo(dir.path());
    let db = dir.path().join("ledger.db");

    let run = check(&db, &repo, &[], &["sh", "-c", "rm -rf .git; exit 9"]);
    assert_eq!(
        run.code,
        Some(9),
        "the child's code survives wrapper-path trouble: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let doc = latest_binding_doc(&db);
    assert_eq!(doc["exit_code"], 9);
    assert!(
        doc["post"]["identity"].is_null(),
        "no post identity can exist — the workspace stopped being a repo"
    );
    let reason = doc["post"]["incomplete_reason"]
        .as_str()
        .expect("the reason is stated");
    assert!(
        reason.contains("post-command observation failed"),
        "the defect is named: {reason}"
    );
    let defects = doc["wrapper_defects"].as_array().expect("defects");
    assert!(
        !defects.is_empty(),
        "the wrapper defect is recorded separately from the child's outcome"
    );

    // The EVIDENCE_BOUND observation says incomplete, not confirmed.
    let conn = rusqlite::Connection::open(&db).expect("open");
    let completeness: String = conn
        .query_row(
            "SELECT completeness FROM observations
              WHERE observation_type = 'EVIDENCE_BOUND' ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("bound row");
    assert_eq!(completeness, "incomplete");
}

/// A recognised test scope through the wrapper, read back through
/// `thruline evidence --json` — the all-tests predicate included.
#[test]
fn a_test_scope_binds_and_the_evidence_verb_reports_it_in_section_9_terms() {
    let dir = tempdir().expect("tempdir");
    let repo = seeded_repo(dir.path());
    let db = dir.path().join("ledger.db");
    std::fs::write(repo.join("test.sh"), "#!/bin/sh\nexit 0\n").expect("write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(repo.join("test.sh"), std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
    }
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "test script"]);

    let run = check(&db, &repo, &["--quiet"], &["./test.sh"]);
    assert_eq!(run.code, Some(0));

    let out = Command::new(bin())
        .args(["--db", db.to_str().expect("db"), "evidence", "--json"])
        .current_dir(&repo)
        .output()
        .expect("evidence");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stable JSON on stdout");
    let bindings = v["bindings"].as_array().expect("bindings");
    assert_eq!(bindings.len(), 1);
    let row = &bindings[0];
    assert_eq!(row["scope_class"], "test");
    assert_eq!(row["establishes_all_tests"], true);
    assert_eq!(row["state"], "CURRENT_STATE_CONFIRMED");
    assert!(row["evaluated_at"].is_string() && row["bound_at"].is_string());

    // Mutate; the report flips to STALE, verbatim, in both surfaces.
    std::fs::write(repo.join("base.txt"), "moved\n").expect("write");
    let out = Command::new(bin())
        .args(["--db", db.to_str().expect("db"), "evidence"])
        .current_dir(&repo)
        .output()
        .expect("evidence");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("STALE"), "{text}");
    assert!(
        !thruline_core::voice::without_paths(&text)
            .to_lowercase()
            .contains("verified"),
        "never \"verified\" language on the new surfaces: {text}"
    );
}
