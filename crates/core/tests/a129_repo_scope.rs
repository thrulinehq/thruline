//! A129: `repo_scope` tells three shapes apart instead of collapsing them.
//!
//! The defect this pins down was a fail-open. `repo_scope` matched its argument
//! against `workspace_identities.repo_id` — a 64-hex digest — by exact string
//! equality. Callers pass a filesystem path. A path matched nothing, and the
//! function returned `Ok` with an empty project list, which compiled into a
//! packet whose every section read "the source was queried and is empty".
//!
//! On the live ledger that packet reported zero projects while two million
//! events sat underneath it. The measurement was never made; the packet said it
//! was made and came back zero. One test per shape, because the shapes are what
//! the fix is about.

use std::path::Path;
use std::process::Command;

use thruline_core::ledger::Ledger;
use thruline_core::model::{ProjectRef, Source};
use thruline_core::packet::{repo_scope, Scope};
use thruline_core::workspace::{repo_identity_at, Basis, RootVia, WorkspaceIdentityRow};

/// Run git with an isolated config, so the developer's own global config (or a
/// CI runner's absence of one) cannot change the result.
fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=ThruLine Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git should be runnable in tests");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn seed_repo(dir: &Path, remote: &str) {
    std::fs::create_dir_all(dir).expect("create repo dir");
    git(dir, &["init", "-q"]);
    std::fs::write(dir.join("README.md"), "seed\n").expect("write file");
    git(dir, &["add", "README.md"]);
    git(dir, &["commit", "-q", "-m", "seed"]);
    git(dir, &["remote", "add", "origin", remote]);
}

/// `workspace_identities.project_id` is a foreign key, so a project row has to
/// exist before its identity can. Vendor matters here: the cross-vendor case is
/// the one the packet has to get right.
fn seed_project(ledger: &Ledger, id: &str, root: &str, source: Source) {
    ledger
        .upsert_project(
            &ProjectRef {
                id: id.to_string(),
                root_path: root.to_string(),
                source,
                display_name: None,
            },
            "2026-08-07T00:00:00Z",
        )
        .expect("seed project");
}

fn row(project_id: &str, repo_id: &str, root: &str) -> WorkspaceIdentityRow {
    WorkspaceIdentityRow {
        project_id: project_id.to_string(),
        repo_id: Some(repo_id.to_string()),
        basis: Basis::Git.as_str().to_string(),
        root_commits_json: "[]".to_string(),
        remote_fingerprints_json: "[]".to_string(),
        resolved_root: Some(root.to_string()),
        resolved_via: Some(RootVia::RecordedCwd.as_str().to_string()),
        sessions_sampled: 3,
        observed_at: "2026-08-07T00:00:00Z".to_string(),
    }
}

/// Shape 1 — a path resolves.
///
/// This is the shape that was broken, and it is the shape every caller uses.
/// Two projects entered the same repo from different vendors; scoping by the
/// path must return both.
#[test]
fn a_path_resolves_to_its_repo_and_spans_every_project_in_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("market");
    seed_repo(&repo, "https://example.invalid/org/market.git");

    let repo_id = repo_identity_at(&repo)
        .expect("identity")
        .repo_id
        .expect("a seeded repo is resolvable");

    let ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    let root = repo.to_string_lossy().to_string();
    // Two vendors, one directory — the cross-vendor grouping the packet exists
    // to report.
    seed_project(&ledger, "-r-market", &root, Source::ClaudeCode);
    seed_project(&ledger, "project_abc", &root, Source::Codex);
    seed_project(&ledger, "-r-other", "/elsewhere", Source::ClaudeCode);
    ledger
        .record_workspace_identity(&row("-r-market", &repo_id, &root))
        .expect("record claude row");
    ledger
        .record_workspace_identity(&row("project_abc", &repo_id, &root))
        .expect("record codex row");
    // A different repo must not leak in.
    ledger
        .record_workspace_identity(&row("-r-other", &"f".repeat(64), "/elsewhere"))
        .expect("record unrelated row");

    let scope = repo_scope(&ledger, &root).expect("a resolvable path is not an error");
    let Scope::Repo {
        repo_id: got,
        project_ids,
    } = scope
    else {
        panic!("a repo argument must produce a repo scope");
    };
    assert_eq!(got, repo_id, "the path resolved to the repo's own id");
    assert_eq!(
        project_ids,
        vec!["-r-market".to_string(), "project_abc".to_string()],
        "both vendors' projects are in scope, sorted"
    );
}

/// Shape 2 — an unknown repo id is an error, not an empty answer.
///
/// A digest nothing on this ledger carries cannot be measured. Returning an
/// empty scope would assert a zero we never counted.
#[test]
fn an_unknown_repo_id_is_an_error_not_an_empty_scope() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    seed_project(&ledger, "-r-known", "/known", Source::ClaudeCode);
    ledger
        .record_workspace_identity(&row("-r-known", &"a".repeat(64), "/known"))
        .expect("record");

    let unknown = "b".repeat(64);
    let err = repo_scope(&ledger, &unknown)
        .expect_err("an unnameable repo id must fail loud, not answer zero");
    let msg = err.to_string();
    assert!(
        msg.contains(&unknown) && msg.contains("unknown repo id"),
        "the error names the id it could not resolve: {msg}"
    );

    // The known id, by contrast, still resolves.
    let scope = repo_scope(&ledger, &"a".repeat(64)).expect("a known repo id resolves");
    assert_eq!(
        scope.project_ids(),
        vec!["-r-known".to_string()],
        "the known id is unaffected by the new error path"
    );
}

/// Shape 3 — a repo that resolves but carries no projects is a legitimate zero.
///
/// This is the case the fix must NOT turn into an error. The question was
/// asked, the repo was identified, and the honest answer is that nothing has
/// been ingested from it yet.
#[test]
fn a_resolvable_repo_with_no_projects_is_a_legitimate_empty_scope() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("never-ingested");
    seed_repo(&repo, "https://example.invalid/org/never-ingested.git");

    let repo_id = repo_identity_at(&repo)
        .expect("identity")
        .repo_id
        .expect("a seeded repo is resolvable");

    let ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    // Some other repo has been backfilled; this one never has.
    seed_project(&ledger, "-r-other", "/elsewhere", Source::ClaudeCode);
    ledger
        .record_workspace_identity(&row("-r-other", &"c".repeat(64), "/elsewhere"))
        .expect("record");

    let scope = repo_scope(&ledger, &repo.to_string_lossy())
        .expect("a resolvable repo with no projects is not an error");
    let Scope::Repo {
        repo_id: got,
        project_ids,
    } = scope
    else {
        panic!("a repo argument must produce a repo scope");
    };
    assert_eq!(got, repo_id, "the repo was identified");
    assert!(
        project_ids.is_empty(),
        "and honestly reports zero projects: {project_ids:?}"
    );
}

/// A path that is not a repo at all is an error too — we cannot say *which*
/// repo was asked about, so we must not answer for one.
#[test]
fn an_unresolvable_path_is_an_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    let plain = tmp.path().join("not-a-repo");
    std::fs::create_dir_all(&plain).expect("mkdir");

    let err = repo_scope(&ledger, &plain.to_string_lossy())
        .expect_err("an unidentifiable path must fail loud");
    assert!(
        err.to_string().contains("not a resolvable repo"),
        "the error says why: {err}"
    );
}
