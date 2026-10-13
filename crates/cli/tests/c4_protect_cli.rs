//! C4.1 + C4.5 — protect's contract, and the zero-destructive-actions guardrail
//! extended across the recovery path (A175).

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

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

#[test]
fn protect_preserves_both_artifacts_and_mutates_nothing() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).expect("mkdir");
    git(&root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("a.txt"), "one\n").expect("write");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "seed"]);
    // The uncommitted work protect exists to preserve.
    std::fs::write(root.join("a.txt"), "two\n").expect("modify");
    std::fs::write(root.join("loose.txt"), "unsaved\n").expect("write");

    let db = temp.path().join("ledger.db");
    {
        let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
        ledger
            .conn()
            .execute_batch(
                "INSERT INTO projects(id, root_path, source, display_name, created_at)
                   VALUES ('p1','/tmp/p1','claude_code','p1','2026-08-13T00:00:00Z');
                 INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
                   VALUES ('s1','p1','/t/s1.jsonl','2026-08-13T00:00:00Z',0);
                 INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                                    raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                   VALUES ('e1','p1','s1',0,'2026-08-13T00:00:01Z','user_msg','do the thing',
                           '/t/s1.jsonl',1,'x','g','c1');",
            )
            .expect("seed");
    }

    let before = std::fs::read_to_string(root.join("a.txt")).expect("read");
    let head_before = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("head");

    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "protect",
            "--project",
            "p1",
            "--root",
            root.to_str().unwrap(),
            "--out",
            temp.path().join("capsule.json").to_str().unwrap(),
        ])
        .output()
        .expect("run protect");
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);

    assert!(
        text.contains("nothing was committed, stashed, or launched"),
        "protect must state what it did not do: {text}"
    );
    assert!(
        text.contains("1 changed") && text.contains("1 untracked"),
        "the preserved counts carry their nouns: {text}"
    );
    // Both artifacts exist and are real.
    let capsule: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(temp.path().join("capsule.json")).expect("capsule file"),
    )
    .expect("capsule json");
    assert_eq!(capsule["capsule_version"], 1);
    assert!(
        capsule["workspace_snapshot"]["doc_sha256"].is_string(),
        "the capsule must cite the snapshot it was compiled beside"
    );
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let snaps: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM workspace_snapshots", [], |r| r.get(0))
        .expect("count");
    assert_eq!(snaps, 1, "the snapshot is stored, not just printed");

    // And the workspace is untouched: same bytes, same HEAD, still dirty.
    assert_eq!(
        std::fs::read_to_string(root.join("a.txt")).expect("read"),
        before
    );
    assert!(
        root.join("loose.txt").exists(),
        "protect deleted untracked work"
    );
    let head_after = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("head");
    assert_eq!(head_before.stdout, head_after.stdout, "protect moved HEAD");
    for line in text.lines() {
        assert_eq!(
            thruline_core::voice::banned_in_output(line),
            None,
            "engine lexicon reached protect's surface: {line:?}"
        );
    }
}

#[test]
fn c45_the_recovery_path_can_never_mutate_a_workspace() {
    // C4.5's guardrail, extended from C2's module walker to every function
    // that serves the recovery verbs. Zero destructive actions is a target of
    // zero forever, and a target nothing checks is a wish.
    let main = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .expect("read main.rs");
    let start = main.find("fn run_protect").expect("run_protect exists");
    let end = main[start..]
        .find("\n/// C3.4")
        .map(|i| start + i)
        .expect("the protect function ends before the capsule one");
    let body = &main[start..end];
    for verb in [
        "\"commit\"",
        "\"stash\"",
        "\"reset\"",
        "\"checkout\"",
        "\"clean\"",
        "\"push\"",
        "\"restore\"",
        "\"rm\"",
        "\"kill\"",
        "\"-9\"",
        "Command::new(\"git\")",
        "Command::new(\"kill\")",
    ] {
        assert!(
            !body.contains(verb),
            "the protect path names `{verb}` — the recovery loop reads, it never runs"
        );
    }
    // It must also not spawn anything at all: a launcher is C4.2's preview,
    // and even there ThruLine prints the command rather than running it.
    assert!(
        !body.contains("Command::new"),
        "the protect path spawns a process; ThruLine prepares and prints, the human launches"
    );
}
