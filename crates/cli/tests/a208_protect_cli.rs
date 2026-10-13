//! A208 — the protected byte layer through the compiled binary (MANDATE-v4
//! §14, §33 snapshot family, §34): `protect`/`snapshot` gain the store,
//! `snapshot --verify` re-hashes it, and `restore` materializes it — with
//! the over-working-tree double gate exercised on both its refusal arms and
//! its gated success, on real scratch repositories.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

fn thruline(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_thruline"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("thruline runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A committed repo with staged, unstaged, and untracked state.
fn seeded_repo(base: &Path) -> PathBuf {
    let root = base.join("repo");
    fs::create_dir_all(&root).expect("mkdir");
    let root = fs::canonicalize(&root).expect("canonical");
    git(&root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("base.txt"), "base\n").expect("write");
    fs::write(root.join(".gitignore"), "scratchpile/\n").expect("write");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "seed"]);
    fs::write(root.join("staged.txt"), "index version\n").expect("write");
    git(&root, &["add", "staged.txt"]);
    fs::write(root.join("staged.txt"), "worktree version\n").expect("write");
    fs::write(root.join("base.txt"), "edited\n").expect("write");
    fs::write(root.join("loose.txt"), "untracked\n").expect("write");
    root
}

fn observation_count(db: &Path, kind: &str) -> i64 {
    let conn = rusqlite::Connection::open(db).expect("open db");
    conn.query_row(
        "SELECT COUNT(*) FROM observations WHERE observation_type = ?1",
        [kind],
        |r| r.get(0),
    )
    .expect("count")
}

#[test]
fn protect_captures_the_byte_layer_and_is_idempotent_over_an_unchanged_tree() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    let db = tmp.path().join("ledger.db");

    let run = |n: u32| {
        let out_path = tmp.path().join(format!("capsule-{n}.json"));
        thruline(
            &db,
            &[
                "protect",
                "--project",
                "none",
                "--root",
                root.to_str().expect("utf8"),
                "--out",
                out_path.to_str().expect("utf8"),
            ],
        )
    };
    let first = run(1);
    assert!(first.status.success(), "{first:?}");
    let text = stdout(&first);
    assert!(
        text.contains("preserved byte-exact") && text.contains("encrypted at rest"),
        "the byte layer is announced: {text}"
    );
    assert!(
        text.contains("manifest     TLSNAP_V1"),
        "the manifest digest is printed: {text}"
    );
    assert!(
        text.contains("observed     SNAPSHOT_CREATED"),
        "the observation receipt is printed: {text}"
    );
    assert_eq!(observation_count(&db, "SNAPSHOT_CREATED"), 1);

    // The stored C2 doc carries the lineage envelope, hash-covered.
    let conn = rusqlite::Connection::open(&db).expect("open db");
    let doc: String = conn
        .query_row("SELECT doc FROM workspace_snapshots LIMIT 1", [], |r| {
            r.get(0)
        })
        .expect("doc");
    let doc: serde_json::Value = serde_json::from_str(&doc).expect("json");
    assert_eq!(doc["protected"]["store_version"], "TLSNAP_V1");
    assert!(doc["protected"]["complete"].as_bool().expect("bool"));
    drop(conn);

    // §33 idempotency at the product surface: an unchanged tree re-protects
    // to zero new blobs and a NEW observation.
    let second = run(2);
    assert!(second.status.success(), "{second:?}");
    let text2 = stdout(&second);
    assert!(
        text2.contains("0 new blobs"),
        "an unchanged tree writes nothing: {text2}"
    );
    assert_eq!(
        observation_count(&db, "SNAPSHOT_CREATED"),
        2,
        "each protection is its own observation"
    );
}

#[test]
fn snapshot_verify_rehashes_everything_and_a_tampered_blob_fails_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    let db = tmp.path().join("ledger.db");
    let out = thruline(&db, &["snapshot", "--root", root.to_str().expect("utf8")]);
    assert!(out.status.success(), "{out:?}");
    let id = parse_banner_id(&stdout(&out));

    let verify = thruline(&db, &["snapshot", "--verify", &id]);
    assert!(verify.status.success(), "{verify:?}");
    let text = stdout(&verify);
    assert!(text.contains("SNAPSHOT VERIFIED"), "{text}");
    assert!(text.contains("re-hashed"), "{text}");
    assert_eq!(observation_count(&db, "SNAPSHOT_VERIFIED"), 1);

    // Flip one byte of one sealed blob: verification must fail loudly and
    // append no health observation.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let blobs = tmp.path().join("protected").join("blobs");
        let shard = fs::read_dir(&blobs)
            .expect("blobs")
            .next()
            .expect("a shard")
            .expect("entry")
            .path();
        let victim = fs::read_dir(&shard)
            .expect("shard")
            .next()
            .expect("a blob")
            .expect("entry")
            .path();
        fs::set_permissions(&victim, fs::Permissions::from_mode(0o600)).expect("unlock");
        let mut bytes = fs::read(&victim).expect("read");
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        fs::write(&victim, &bytes).expect("tamper");
        fs::set_permissions(&victim, fs::Permissions::from_mode(0o400)).expect("relock");

        let broken = thruline(&db, &["snapshot", "--verify", &id]);
        assert!(
            !broken.status.success(),
            "a tampered store must fail verification: {broken:?}"
        );
        assert_eq!(
            observation_count(&db, "SNAPSHOT_VERIFIED"),
            1,
            "no observation of health is appended for a failed pass"
        );
    }
}

fn parse_banner_id(text: &str) -> String {
    text.lines()
        .find(|l| l.starts_with("WORKSPACE SNAPSHOT "))
        .map(|l| l.trim_start_matches("WORKSPACE SNAPSHOT ").to_string())
        .expect("banner id")
}

#[test]
fn restore_materializes_a_byte_exact_tree_in_a_safe_directory() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    let db = tmp.path().join("ledger.db");
    let out = thruline(&db, &["snapshot", "--root", root.to_str().expect("utf8")]);
    assert!(out.status.success(), "{out:?}");
    let id = parse_banner_id(&stdout(&out));

    let target = tmp.path().join("safe");
    let restore = thruline(
        &db,
        &["restore", &id, "--to", target.to_str().expect("utf8")],
    );
    assert!(restore.status.success(), "{restore:?}");
    let text = stdout(&restore);
    assert!(
        text.contains("byte-exact, into a separate safe directory"),
        "{text}"
    );
    assert!(
        text.contains("the active workspace was not touched"),
        "{text}"
    );
    assert_eq!(
        fs::read(target.join("base.txt")).expect("restored"),
        fs::read(root.join("base.txt")).expect("source"),
        "worktree bytes round-trip"
    );
    assert_eq!(
        fs::read(target.join(".git/index")).expect("restored index"),
        fs::read(root.join(".git/index")).expect("source index"),
        "the index — the staged/worktree duality's carrier — round-trips"
    );
    // The restored directory is a standalone repository in the same state.
    let status = Command::new("git")
        .arg("-C")
        .arg(&target)
        .args(["status", "--porcelain"])
        .output()
        .expect("git");
    let status_src = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["status", "--porcelain"])
        .output()
        .expect("git");
    assert_eq!(
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status_src.stdout),
        "the restored repository reports the same porcelain state"
    );

    // A second restore to the same default-shaped target refuses: the safe
    // path never writes into a non-empty directory.
    let again = thruline(
        &db,
        &["restore", &id, "--to", target.to_str().expect("utf8")],
    );
    assert!(!again.status.success(), "a non-empty target must refuse");
}

#[test]
fn restore_over_working_tree_is_double_gated_and_touches_only_the_preview() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    let db = tmp.path().join("ledger.db");
    let out = thruline(&db, &["snapshot", "--root", root.to_str().expect("utf8")]);
    assert!(out.status.success(), "{out:?}");
    let id = parse_banner_id(&stdout(&out));

    // The tree drifts; an ignored file sits outside the declared scope.
    fs::write(root.join("base.txt"), "drifted\n").expect("drift");
    fs::write(root.join("junk.txt"), "delete me\n").expect("junk");
    fs::create_dir_all(root.join("scratchpile")).expect("ignored dir");
    fs::write(root.join("scratchpile/survivor.txt"), "outside scope\n").expect("survivor");

    let root_arg = root.to_str().expect("utf8");

    // Gate one, missing: refused, and the current identity is offered.
    let refused = thruline(
        &db,
        &["restore", &id, "--over-working-tree", "--to", root_arg],
    );
    assert!(!refused.status.success(), "no acknowledgment, no restore");
    let text = stdout(&refused);
    assert!(text.contains("REFUSED"), "{text}");
    assert!(text.contains("nothing was touched"), "{text}");
    let token = text
        .lines()
        .find(|l| l.trim_start().starts_with("currently"))
        .and_then(|l| l.split_whitespace().nth(1))
        .expect("the current identity is offered")
        .to_string();
    assert!(token.starts_with("wsid1:"), "{token}");
    assert_eq!(
        fs::read_to_string(root.join("base.txt")).expect("read"),
        "drifted\n",
        "refusal touched nothing"
    );

    // Gate one, wrong: refused.
    let wrong = thruline(
        &db,
        &[
            "restore",
            &id,
            "--over-working-tree",
            "--to",
            root_arg,
            "--expect-tree",
            "wsid1:0000000000000000000000000000000000000000000000000000000000000000",
            "--yes",
        ],
    );
    assert!(
        !wrong.status.success(),
        "a wrong acknowledgment must refuse"
    );
    assert!(
        stdout(&wrong).contains("not the state you acknowledged"),
        "{}",
        stdout(&wrong)
    );

    // Both gates passed (--yes is the documented confirmation seam): the
    // preview names everything, the apply does exactly that, and the
    // out-of-scope file survives.
    let gated = thruline(
        &db,
        &[
            "restore",
            &id,
            "--over-working-tree",
            "--to",
            root_arg,
            "--expect-tree",
            &token,
            "--yes",
        ],
    );
    assert!(gated.status.success(), "{gated:?}");
    let text = stdout(&gated);
    assert!(
        text.contains("write  base.txt") && text.contains("delete junk.txt"),
        "the preview names the exact files: {text}"
    );
    assert!(text.contains("exactly as previewed"), "{text}");
    assert!(
        text.contains("the tree now IS the captured workspace identity"),
        "the §34 equality is re-observed and stated: {text}"
    );
    assert_eq!(
        fs::read_to_string(root.join("base.txt")).expect("read"),
        "edited\n",
        "the drifted file returned to the captured bytes"
    );
    assert!(!root.join("junk.txt").exists(), "the named delete happened");
    assert_eq!(
        fs::read_to_string(root.join("scratchpile/survivor.txt")).expect("survivor"),
        "outside scope\n",
        "nothing beyond the preview was touched"
    );
}

#[test]
fn a_digest_only_snapshot_refuses_to_restore_and_says_why() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let not_a_repo = tmp.path().join("plain");
    fs::create_dir_all(&not_a_repo).expect("mkdir");
    fs::write(not_a_repo.join("f.txt"), "x").expect("write");
    let db = tmp.path().join("ledger.db");
    let out = thruline(
        &db,
        &["snapshot", "--root", not_a_repo.to_str().expect("utf8")],
    );
    assert!(out.status.success(), "{out:?}");
    let text = stdout(&out);
    assert!(
        text.contains("NO byte layer"),
        "the missing layer is stated, never silent: {text}"
    );
    let id = parse_banner_id(&text);
    let restore = thruline(&db, &["restore", &id]);
    assert!(!restore.status.success());
    assert!(
        stdout(&restore).contains("no byte layer"),
        "{}",
        stdout(&restore)
    );
}

#[test]
fn the_evidence_help_names_the_stale_exit_contract() {
    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args(["evidence", "--help"])
        .output()
        .expect("help runs");
    assert!(out.status.success());
    assert!(
        stdout(&out).contains("Exits 0 whatever state it reports"),
        "the register-documented exit contract is one --help away: {}",
        stdout(&out)
    );
}
