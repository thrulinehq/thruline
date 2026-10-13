//! B3 reads assertions from immutable Git blobs through the accepted engine.

#[path = "support/person_action.rs"]
mod person_action;

use std::fs;
use std::path::Path;
use std::process::Command;

use person_action::PersonRecorder;
use rusqlite::params;
use serde_json::json;
use tempfile::TempDir;
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_preservation::AssertionFloorParams;
use thruline_core::decisions::engine::check_decision_version;

const TWO: &str = "#[test] fn proof() { assert!(true); assert_eq!(1, 1); }\n";
const ONE: &str = "#[test] fn proof() { assert!(true); }\n";

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git UTF-8")
        .trim()
        .into()
}

fn commit(repo: &Path, label: &str) -> String {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            label,
        ],
    );
    git(repo, &["rev-parse", "HEAD"])
}

#[test]
fn b3_engine_nets_a_move_then_breaks_for_one_removed_assertion() {
    let repo = TempDir::new().expect("repo");
    git(repo.path(), &["init", "-q"]);
    fs::create_dir(repo.path().join("tests")).expect("tests dir");
    fs::write(repo.path().join("tests/old.rs"), TWO).expect("baseline test");
    let baseline = commit(repo.path(), "two assertions");

    git(repo.path(), &["mv", "tests/old.rs", "tests/new.rs"]);
    let moved = commit(repo.path(), "move test with assertions intact");
    fs::write(repo.path().join("tests/new.rs"), ONE).expect("remove assertion");
    let reduced = commit(repo.path(), "remove one assertion");

    let ledger = thruline_core::ledger::Ledger::open(repo.path().join(".git/decision-ledger.db"))
        .expect("ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES('p',?1,'codex','2026-10-07T00:00:00Z')",
            [repo.path().to_string_lossy().as_ref()],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO decisions(id,project_id,created_at) VALUES('b3','p','2026-10-07T00:00:00Z')",
            [],
        )
        .expect("decision");
    let params_json = json!({"kind":"assertion_floor","paths":["tests/**"],"minimum":2,"baseline_version":baseline}).to_string();
    ledger
        .conn()
        .execute(
            "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at)
             VALUES('b3',1,'assertion_floor',1,?1,'Keep two assertions','[]','person','2026-10-07T00:00:00Z')",
            [&params_json],
        )
        .expect("revision");
    ledger
        .conn()
        .execute(
            "INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,severity,applies_from_version)
             VALUES('b3-accept','b3',1,'person','2026-10-07T00:01:00Z','fixture','blocks',?1)",
            params![baseline],
        )
        .expect("acceptance");
    let mut person = PersonRecorder::new(&ledger, "p");
    person.bind_acceptance(&ledger, "b3-accept");

    let verdict = |version: &str| {
        check_decision_version(
            &ledger,
            "p",
            repo.path(),
            version,
            "2026-10-07T00:02:00Z",
            false,
            "b3",
        )
        .expect("B3 check")[0]
            .clone()
    };
    // Earlier engines had no production B3 adapter. That historical result
    // must not suppress a newly implemented exact-tree check.
    let moved_tree = git(repo.path(), &["rev-parse", &format!("{moved}^{{tree}}")]);
    let git_tool = git(repo.path(), &["--version"]);
    thruline_core::decisions::storage::record_verdict(
        &ledger,
        thruline_core::decisions::storage::VerdictInput {
            decision_id: "b3".into(),
            revision: 1,
            version_id: moved.clone(),
            tree_hash: moved_tree,
            verdict: "unverifiable".into(),
            reason: Some("assertion parser unavailable".into()),
            checkable_if: Some("Provide an assertion parser".into()),
            findings: json!([]),
            surfaces: json!([]),
            check_impl_version: "decisions.0.2.c24.4".into(),
            tool_versions: json!({"git": git_tool}),
            computed_at: "2026-10-07T00:01:30Z".into(),
            basis: json!({"checked_places": 0, "visual": []}),
        },
    )
    .expect("historical B3 result");

    let held = verdict(&moved);
    assert!(
        !held.from_cache,
        "an older implementation result needs recomputation"
    );
    assert_eq!(held.state, VerdictState::Holds, "{held:?}");
    assert_eq!(held.outcome.checked_places, 1);
    let broken = verdict(&reduced);
    assert_eq!(broken.state, VerdictState::Broken, "{broken:?}");
    assert!(broken.outcome.findings[0].span.contains("2 to 1"));
    for (version, stored) in [(&moved, &held), (&reduced, &broken)] {
        let replay = check_decision_version(
            &ledger,
            "p",
            repo.path(),
            version,
            "2026-10-07T00:03:00Z",
            true,
            "b3",
        )
        .expect("deterministic B3 replay");
        assert_eq!(replay[0].outcome, stored.outcome);
    }

    // An unsupported selected language and a syntactically malformed blob are
    // evidence gaps, never a zero-assertion pass or a fabricated breakage.
    fs::write(repo.path().join("tests/unknown.rb"), "assert true\n").expect("Ruby test");
    let unsupported = commit(repo.path(), "unsupported selected language");
    assert_eq!(verdict(&unsupported).state, VerdictState::Unverifiable);
    fs::remove_file(repo.path().join("tests/unknown.rb")).expect("remove Ruby test");
    fs::write(
        repo.path().join("tests/component.tsx"),
        "const element = <div />;\ntest('view', () => { expect(element).toBeTruthy(); });\n",
    )
    .expect("TSX test");
    let tsx = commit(repo.path(), "selected TSX assertion");
    let tsx_result = verdict(&tsx);
    assert_eq!(tsx_result.state, VerdictState::Unverifiable);
    assert_eq!(
        tsx_result.outcome.reason.as_deref(),
        Some("TSX assertion parser is unavailable")
    );
    fs::remove_file(repo.path().join("tests/component.tsx")).expect("remove TSX test");
    fs::write(
        repo.path().join("tests/new.rs"),
        "#[test] fn proof( { assert!(true); }\n",
    )
    .expect("malformed Rust test");
    let malformed = commit(repo.path(), "malformed test blob");
    assert_eq!(verdict(&malformed).state, VerdictState::Unverifiable);

    fs::write(repo.path().join("tests/new.rs"), TWO).expect("restore Rust test");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("new.rs", repo.path().join("tests/link.rs"))
            .expect("symlink fixture");
        let symlink = commit(repo.path(), "selected symlink");
        assert_eq!(verdict(&symlink).state, VerdictState::Unverifiable);
        fs::remove_file(repo.path().join("tests/link.rs")).expect("remove symlink");
    }
    fs::write(
        repo.path().join("tests/huge.rs"),
        " ".repeat(2 * 1024 * 1024 + 1),
    )
    .expect("oversized blob");
    let oversized = commit(repo.path(), "selected blob exceeds bound");
    let huge_result = verdict(&oversized);
    assert_eq!(huge_result.state, VerdictState::Unverifiable);
    assert!(
        huge_result
            .outcome
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("byte limit")),
        "{huge_result:?}"
    );

    // The adapter's claimed target tree must equal the named version's tree.
    let wrong_tree = git(repo.path(), &["rev-parse", &format!("{baseline}^{{tree}}")]);
    let params = AssertionFloorParams {
        paths: vec!["tests/**".into()],
        minimum: 2,
        baseline_version: baseline,
    };
    let mismatched = thruline_core::decisions::assertion_adapter::check_at(
        repo.path(),
        &moved,
        &wrong_tree,
        &params,
    );
    assert_eq!(mismatched.state, VerdictState::Unverifiable);
    assert!(mismatched.tree_hash.is_none());
    assert_eq!(
        thruline_core::decisions::assertion_adapter::check_at(
            repo.path(),
            "HEAD",
            &wrong_tree,
            &params
        )
        .state,
        VerdictState::Unverifiable,
        "movable refs cannot be accepted as exact target versions"
    );
}
