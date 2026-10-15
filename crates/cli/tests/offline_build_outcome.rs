//! OFFLINE-BUILD-OUTCOME (D-051) — a build that could not fetch its
//! dependencies is not a failure of the project's code, through the compiled
//! binary and a real `cargo`.
//!
//! The defect: every run the sheet starts is offline (ADV-run-ledger-3), so a
//! dependency not already on this machine fails the build with exit 101 —
//! the same exit code a compile error gives. The coverage layer read the exit
//! code alone and called the run `NotProven`: the project's code "failed" a
//! check that never reached it.
//!
//! The founder's rule: an environmental build failure (no network,
//! dependencies not fetchable) records the product's couldn't-check outcome —
//! `Observed::Couldnt`, whose word on every surface is **Recorded**; a compile
//! error in the project's own code still records a failure (`NotProven`).
//!
//! Every run here is a real `cargo test` with networking off, a `CARGO_HOME`
//! that is an empty directory inside this test's own temporary folder (so no
//! dependency is ever cached, and nothing outside the folder is read or
//! written by cargo's registry), and the caller's shared `CARGO_TARGET_DIR`
//! outside the repository (so the build does not move the files the run is
//! examining). Nothing is fetched: offline is the point.

use std::path::{Path, PathBuf};
use std::process::Command;

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

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(base: &Path, cwd: &Path, db: &Path, args: &[&str]) -> Run {
    run_with_env(base, cwd, db, args, &[])
}

/// OFFLINE-KILL-GUARD-R3 (R-2026-09-28-26): the owned fetch takes cargo and
/// rustc only from `$CARGO_HOME/bin`, `$RUSTUP_HOME/toolchains` or a system
/// directory. This harness points `CARGO_HOME` (and `HOME`) at its own
/// folder, so a rustup proxy in the caller's real `~/.cargo/bin` is, by that
/// ruling, an arbitrary PATH entry. Install the same rustup binary here the way
/// rustup lays it out — `bin/rustup` (a hard link, or a copy across volumes)
/// with `cargo` and `rustc` linked to it — and put that directory first on
/// PATH. The toolchain it resolves is the caller's `$RUSTUP_HOME` one. A cargo
/// that is not a rustup proxy is left where it is.
fn trusted_path(cargo_home: &Path) -> std::ffi::OsString {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let Some(cargo) = std::env::split_paths(&path)
        .map(|p| p.join("cargo"))
        .find(|p| p.is_file())
        .and_then(|p| std::fs::canonicalize(p).ok())
    else {
        return path;
    };
    // Homebrew-style layouts resolve `cargo` to a file called `rustup-init`; it
    // is installed below under the name `rustup`, which is what makes it the
    // proxy rather than the installer.
    if !matches!(
        cargo.file_stem().and_then(std::ffi::OsStr::to_str),
        Some("rustup" | "rustup-init")
    ) {
        return path;
    }
    let bin = cargo_home.join("bin");
    std::fs::create_dir_all(&bin).expect("cargo home bin");
    let rustup = bin.join("rustup");
    if !rustup.is_file() {
        std::fs::hard_link(&cargo, &rustup)
            .or_else(|_| std::fs::copy(&cargo, &rustup).map(|_| ()))
            .expect("install rustup into the harness CARGO_HOME");
        #[cfg(unix)]
        for proxy in ["cargo", "rustc"] {
            std::os::unix::fs::symlink("rustup", bin.join(proxy)).expect("rustup proxy");
        }
    }
    std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&path)))
        .expect("harness PATH")
}

fn run_with_env(
    base: &Path,
    cwd: &Path,
    db: &Path,
    args: &[&str],
    environment: &[(&str, &str)],
) -> Run {
    let cargo_home = base.join("cargo-home");
    std::fs::create_dir_all(&cargo_home).expect("cargo home");
    let out = Command::new(bin())
        .env("PATH", trusted_path(&cargo_home))
        .env("HOME", base)
        .env("THRULINE_HOME", base.join(".thruline"))
        // The child `cargo` inherits these three and nothing else of ours.
        .env("CARGO_HOME", &cargo_home)
        // HOME is the harness's own folder, so rustup's default home would be
        // empty there. Give the child the caller's toolchains (the caller's
        // RUSTUP_HOME, else its ~/.rustup) in this test's environment only.
        .envs(
            std::env::var_os("RUSTUP_HOME")
                .or_else(|| {
                    std::env::var_os("HOME").map(|h| Path::new(&h).join(".rustup").into_os_string())
                })
                .map(|home| ("RUSTUP_HOME", home)),
        )
        .env(
            "CARGO_TARGET_DIR",
            std::env::var_os("CARGO_TARGET_DIR").expect("caller must provide shared target"),
        )
        .envs(environment.iter().copied())
        .current_dir(cwd)
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("run thruline");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// The three projects the ruling names.
enum Project {
    /// Needs a dependency that is not on this machine.
    NeedsADependency,
    /// Its own `src` does not type-check. No dependencies.
    TypeError,
    /// Both: needs a dependency that is not here AND its own `src` does not
    /// type-check.
    Both,
}

const LIB_OK: &str = "pub fn answer() -> u32 {\n    42\n}\n";
const LIB_TYPE_ERROR: &str = "pub fn answer() -> u32 {\n    \"not a number\"\n}\n";

/// A repository holding a real Cargo project, and a store whose one project
/// points at it, holding one recorded session that edited a file under `src`.
fn fixture(base: &Path, project: Project) -> (PathBuf, PathBuf) {
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join("src")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    let (dependency, lib) = match project {
        Project::NeedsADependency => ("serde = \"1\"\n", LIB_OK),
        Project::TypeError => ("", LIB_TYPE_ERROR),
        Project::Both => ("serde = \"1\"\n", LIB_TYPE_ERROR),
    };
    std::fs::write(
        repo.join("Cargo.toml"),
        format!(
            "[package]\nname = \"offline_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n{dependency}"
        ),
    )
    .expect("manifest");
    if dependency.is_empty() {
        // The lockfile cargo itself writes for a package with no
        // dependencies, committed so the build writes nothing into the tree.
        std::fs::write(
            repo.join("Cargo.lock"),
            "# This file is automatically @generated by Cargo.\n\
             # It is not intended for manual editing.\n\
             version = 4\n\n\
             [[package]]\n\
             name = \"offline_fixture\"\n\
             version = \"0.1.0\"\n",
        )
        .expect("lockfile");
    }
    std::fs::write(repo.join("src/lib.rs"), lib).expect("lib");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "seed"]);
    std::fs::write(
        repo.join("s.jsonl"),
        format!("{{\"type\":\"user\",\"cwd\":\"{}\"}}\n", repo.display()),
    )
    .expect("transcript");
    // The transcript is not part of the project; keep it out of the tree the
    // run examines the same way a real session log lives elsewhere.
    std::fs::write(repo.join(".git/info/exclude"), "s.jsonl\n").expect("exclude");

    let db = base.join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('cccccccc-0000-4000-8000-00000000p001', ?1, 'claude_code', 'Synthetic',
                    '2026-01-01T00:00:00Z')",
            rusqlite::params![repo.display().to_string()],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('cccccccc-0000-4000-8000-00000000s001',
                    'cccccccc-0000-4000-8000-00000000p001', ?1, '2026-01-02T09:30:00Z')",
            rusqlite::params![repo.join("s.jsonl").display().to_string()],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, role, text,
                 raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-0','cccccccc-0000-4000-8000-00000000p001',
                    'cccccccc-0000-4000-8000-00000000s001',1,'2026-01-02T09:30:00Z',
                    'user_msg','user','Two retry requests for the same delivery do not
                     enqueue duplicate work.', ?1, 1, 'x', '', '')",
            rusqlite::params![repo.join("s.jsonl").display().to_string()],
        )
        .expect("ask");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-1','cccccccc-0000-4000-8000-00000000p001',
                    'cccccccc-0000-4000-8000-00000000s001',2,'2026-01-02T09:31:00Z',
                    'tool_use','Edit', ?1, ?2, 2, 'x', '', '')",
            rusqlite::params![
                format!(
                    "{{\"file_path\":\"{}\"}}",
                    repo.join("src/lib.rs").display()
                ),
                repo.join("s.jsonl").display().to_string()
            ],
        )
        .expect("event");
    (repo, db)
}

fn change_id(out: &Run) -> String {
    out.stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("chg_"))
        .unwrap_or_else(|| panic!("no Change id was printed: {}", out.stdout))
        .to_string()
}

/// Open a Change, confirm the work, accept one test-worded line.
fn up_to_accepted(base: &Path, repo: &Path, db: &Path) -> String {
    let opened = run(
        base,
        repo,
        db,
        &[
            "change",
            "new",
            "--title",
            "Retry failed uploads",
            "--purpose",
            "Two retry requests for the same delivery do not enqueue duplicate work.",
            "--requester",
            "sam",
        ],
    );
    assert_eq!(opened.code, 0, "{}", opened.stderr);
    let change = change_id(&opened);
    let confirmed = run(
        base,
        repo,
        db,
        &[
            "change",
            "associate",
            &change,
            "cccccccc-0000-4000-8000-00000000s001",
            "--confirm",
            "--actor",
            "sam",
            "--reason",
            "this is the work",
        ],
    );
    assert_eq!(confirmed.code, 0, "{}", confirmed.stderr);
    let proposed = run(base, repo, db, &["change", "propose", &change]);
    assert_eq!(proposed.code, 0, "{}", proposed.stderr);
    let accepted = run(
        base,
        repo,
        db,
        &[
            "change",
            "accept",
            &change,
            "--criterion",
            "1",
            "--edit",
            "1=The duplicate-retry test passes.",
            "--scope",
            "src/**",
            "--actor",
            "sam",
            "--rationale",
            "this is what I want checked",
        ],
    );
    assert_eq!(accepted.code, 0, "{}", accepted.stderr);
    change
}

/// Run `cargo test` offline for the Change, and hand back the coverage state of
/// the one accepted line, the observed result of the run under it, and what the
/// run printed (for the failure message).
fn checked(project: Project) -> (String, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path(), project);
    let change = up_to_accepted(dir.path(), &repo, &db);
    let out = run(
        dir.path(),
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "cargo",
            "test",
        ],
    );
    // The wrapper passes the command's own exit code through: cargo's 101.
    assert_eq!(
        out.code, 101,
        "cargo did not fail as expected: {}",
        out.stderr
    );

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    thruline_core::change_evidence::derive_links(&ledger, &change, "2026-02-01T12:00:00Z")
        .expect("derive");
    let view = thruline_core::change_evidence::coverage(
        &ledger,
        &change,
        &thruline_core::change_propose::ProfileEnvironment::default(),
    )
    .expect("coverage");
    let first = view.criteria.first().expect("one accepted line");
    let observed = first
        .items
        .first()
        .map(|i| format!("{:?}", i.observed))
        .unwrap_or_else(|| "no item".to_string());
    (format!("{:?}", first.state), observed, out.stderr)
}

/// **The defect, as a test.** A project that needs a dependency this machine
/// does not have, built offline, never reached its own code. It is the
/// couldn't-check outcome — Recorded, settles nothing — and never a failure.
#[test]
fn an_offline_build_missing_a_dependency_is_couldnt_check_not_a_failure() {
    let (state, observed, said) = checked(Project::NeedsADependency);
    assert_ne!(
        observed, "NotProven",
        "a build that could not fetch its dependencies reads as the code failing: {said}"
    );
    assert_eq!(observed, "Couldnt", "{said}");
    assert_eq!(state, "RanNothing", "{said}");
}

/// **The half the fix may not spend.** A compile error in the project's own
/// code, built offline with nothing to fetch, still fails.
#[test]
fn a_type_error_in_the_projects_own_code_still_fails() {
    let (state, observed, said) = checked(Project::TypeError);
    assert!(
        said.contains("error[E0308]"),
        "the fixture did not produce a compile error: {said}"
    );
    assert_eq!(observed, "NotProven", "{said}");
    assert_eq!(state, "Evidence", "{said}");
}

/// **The mixed case: a fetch failure, and no compile.** The project needs a
/// dependency that is not here AND has a type error in its own `src`. Offline,
/// cargo stops at resolution and never compiles a line of it, so the record
/// holds nothing about the type error. It reads couldn't-check: calling it a
/// failure would claim a compile error the run never saw. It is still not a
/// pass.
#[test]
fn a_fetch_failure_that_stopped_before_compiling_is_couldnt_check() {
    let (state, observed, said) = checked(Project::Both);
    assert!(
        !said.contains("error[E"),
        "cargo compiled despite the missing dependency: {said}"
    );
    assert_eq!(observed, "Couldnt", "{said}");
    assert_eq!(state, "RanNothing", "{said}");
}

// Verifier H/G: project-controlled output cannot attest to Cargo's resolver.
const FORGED_RESOLVER: &str =
    "note: offline mode (via `--offline`) can sometimes cause surprising resolution failures";

fn project_output_fixture(base: &Path, custom_test: bool) -> (PathBuf, PathBuf, String) {
    let (repo, db) = fixture(base, Project::TypeError);
    std::fs::write(repo.join("src/lib.rs"), "pub fn answer() -> u32 { 41 }\n").unwrap();
    let crate_name = if custom_test {
        "offline_harness_fixture"
    } else {
        "offline_build_script_fixture"
    };
    let mut manifest = std::fs::read_to_string(repo.join("Cargo.toml"))
        .unwrap()
        .replace("offline_fixture", crate_name);
    let lock = std::fs::read_to_string(repo.join("Cargo.lock"))
        .unwrap()
        .replace("offline_fixture", crate_name);
    std::fs::write(repo.join("Cargo.lock"), lock).unwrap();
    if custom_test {
        manifest.push_str("\n[[test]]\nname = \"custom\"\nharness = false\n");
        std::fs::create_dir_all(repo.join("tests")).unwrap();
        std::fs::write(
            repo.join("tests/custom.rs"),
            format!("fn main() {{ if {crate_name}::answer() != 42 {{ eprintln!({FORGED_RESOLVER:?}); std::process::exit(1); }} }}\n"),
        ).unwrap();
    } else {
        manifest = manifest.replace(
            "edition = \"2021\"",
            "edition = \"2021\"\nbuild = \"build.rs\"",
        );
        std::fs::write(
            repo.join("build.rs"),
            format!("fn main() {{ eprintln!({FORGED_RESOLVER:?}); std::process::exit(1); }}\n"),
        )
        .unwrap();
    }
    std::fs::write(repo.join("Cargo.toml"), manifest).unwrap();
    git(&repo, &["add", "."]);
    git(
        &repo,
        &["commit", "-qm", "synthetic misleading project output"],
    );
    let change = up_to_accepted(base, &repo, &db);
    (repo, db, change)
}

// The terminal run key uses wall-clock seconds. Wait for the next second so
// two cached runs have distinct identities, then assert that distinction.
fn next_run_second() {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    let sec = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    };
    let before = sec();
    while sec() == before {
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assert_project_failure(
    base: &Path,
    repo: &Path,
    db: &Path,
    change: &str,
    quiet: bool,
    expected_exit: i32,
) -> (String, Run) {
    let ledger = thruline_core::ledger::Ledger::open(db).unwrap();
    let before = thruline_core::run_execution::all(&ledger, Some(change)).unwrap();
    let mut args = vec![
        "check",
        "--for-change",
        change,
        "--actor",
        "sam",
        "--",
        "cargo",
        "test",
    ];
    if quiet {
        args.push("-q");
    }
    let out = run(base, repo, db, &args);
    assert_eq!(out.code, expected_exit, "{}", out.stderr);
    assert!(
        out.stderr.contains(FORGED_RESOLVER),
        "fixture did not spoof Cargo: {}",
        out.stderr
    );
    let after = thruline_core::run_execution::all(&ledger, Some(change)).unwrap();
    let added: Vec<_> = after
        .iter()
        .filter(|r| !before.iter().any(|old| old.run_key == r.run_key))
        .collect();
    assert_eq!(
        added.len(),
        1,
        "each invocation needs its own recorded run: {after:?}"
    );
    let execution = added[0];
    assert_eq!(
        execution.state,
        thruline_core::run_execution::ExecutionState::Completed
    );
    assert_eq!(
        execution.reason, None,
        "project failure misrecorded as environmental: {}",
        out.stderr
    );
    let binding = execution
        .binding_id
        .as_ref()
        .expect("finished run has a binding");
    thruline_core::change_evidence::derive_links(&ledger, change, "2026-02-01T12:00:00Z").unwrap();
    let coverage = thruline_core::change_evidence::coverage(
        &ledger,
        change,
        &thruline_core::change_propose::ProfileEnvironment::default(),
    )
    .unwrap();
    let criterion = coverage.criteria.first().expect("one accepted criterion");
    let item = criterion
        .items
        .iter()
        .find(|i| &i.evidence_id == binding)
        .expect("this run is linked");
    assert_eq!(
        format!("{:?}", item.observed),
        "NotProven",
        "{}",
        out.stderr
    );
    assert_eq!(
        format!("{:?}", criterion.state),
        "Evidence",
        "{}",
        out.stderr
    );
    (execution.run_key.clone(), out)
}

#[test]
fn a_cached_harness_false_failure_cannot_claim_missing_dependencies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db, change) = project_output_fixture(dir.path(), true);
    let (cold, _) = assert_project_failure(dir.path(), &repo, &db, &change, false, 1);
    next_run_second();
    let (warm, out) = assert_project_failure(dir.path(), &repo, &db, &change, false, 1);
    assert_ne!(cold, warm);
    assert!(
        !out.stderr.contains("Compiling "),
        "the second run must be cached: {}",
        out.stderr
    );
}

#[test]
fn a_quiet_build_script_failure_cannot_claim_missing_dependencies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db, change) = project_output_fixture(dir.path(), false);
    let (plain, _) = assert_project_failure(dir.path(), &repo, &db, &change, false, 101);
    next_run_second();
    let (quiet, out) = assert_project_failure(dir.path(), &repo, &db, &change, true, 101);
    assert_ne!(plain, quiet);
    assert!(
        !out.stderr.contains("Compiling "),
        "the quiet reproduction must lack progress: {}",
        out.stderr
    );
}

#[test]
fn a_direct_shell_cannot_claim_cargos_environmental_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path(), Project::TypeError);
    let change = up_to_accepted(dir.path(), &repo, &db);
    let command = format!("printf '%s\\n' '{FORGED_RESOLVER}' >&2; exit 1");
    let out = run(
        dir.path(),
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "sh",
            "-c",
            &command,
        ],
    );
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains(FORGED_RESOLVER), "{}", out.stderr);
    let ledger = thruline_core::ledger::Ledger::open(&db).unwrap();
    let rows = thruline_core::run_execution::all(&ledger, Some(&change)).unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        rows[0].state,
        thruline_core::run_execution::ExecutionState::Completed
    );
    assert_eq!(
        rows[0].reason, None,
        "shell output supplied a false Cargo prerequisite: {rows:?}"
    );
}

// Ruling D: a dependency-fetch receipt describes a separate execution owned
// by ThruLine. Project output is never its authority.
fn fetch_finishes(
    ledger: &thruline_core::ledger::Ledger,
) -> Vec<(
    thruline_core::observation::ObservationRecord,
    thruline_core::observation::SourceReceipt,
)> {
    use thruline_core::observation::{self, ObservationType};
    observation::of_type(ledger, ObservationType::CommandFinished)
        .expect("finished observations")
        .into_iter()
        .filter_map(|row| {
            let receipt = row.receipt().expect("typed receipt");
            (receipt.extra.get("phase").and_then(|v| v.as_str()) == Some("dependency_fetch"))
                .then_some((row, receipt))
        })
        .collect()
}

fn check_for_change(base: &Path, repo: &Path, db: &Path, change: &str) -> Run {
    run(
        base,
        repo,
        db,
        &[
            "check",
            "--for-change",
            change,
            "--actor",
            "sam",
            "--",
            "cargo",
            "test",
        ],
    )
}

fn assert_config_receipt(receipt: &thruline_core::observation::SourceReceipt, config: &Path) {
    let configurations = receipt.extra["project_configuration"]
        .as_array()
        .expect("fetch names the project configuration that steered it");
    let canonical = std::fs::canonicalize(config).expect("canonical config");
    let entry = configurations
        .iter()
        .find(|entry| {
            entry["path"]
                .as_str()
                .and_then(|path| std::fs::canonicalize(path).ok())
                .as_ref()
                == Some(&canonical)
        })
        .unwrap_or_else(|| {
            panic!(
                "missing project configuration {}: {receipt:?}",
                config.display()
            )
        });
    assert_eq!(
        entry["sha256"],
        thruline_core::chain::sha256_hex(&std::fs::read(config).unwrap())
    );
    assert!(
        !entry["applied"]
            .as_array()
            .expect("applied config keys")
            .is_empty(),
        "source configuration was not attributed: {entry}"
    );
}

#[cfg(unix)]
#[test]
fn r2_failed_fetch_runs_no_project_code_and_never_counts_toward_approval() {
    use std::os::unix::fs::PermissionsExt;
    use thruline_core::{
        change_evidence, change_readiness, observation::ReceiptTerminal, run_execution,
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path(), Project::NeedsADependency);
    let build_marker = dir.path().join("build-ran");
    let compiler_marker = dir.path().join("project-compiler-ran");
    std::fs::write(
        repo.join("build.rs"),
        format!(
            "fn main() {{ std::fs::write({:?}, \"ran\").unwrap(); }}\n",
            build_marker.to_str().unwrap()
        ),
    )
    .unwrap();
    let compiler = repo.join("compiler.sh");
    std::fs::write(
        &compiler,
        format!(
            "#!/bin/sh\nprintf ran > {:?}\nexit 97\n",
            compiler_marker.to_str().unwrap()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::create_dir(repo.join(".cargo")).unwrap();
    let config = repo.join(".cargo/config.toml");
    std::fs::write(&config, format!(
        "[source.crates-io]\nreplace-with = \"unavailable-local\"\n[source.unavailable-local]\ndirectory = \"missing-vendor\"\n[build]\nrustc = {:?}\n",
        compiler.to_str().unwrap())).unwrap();
    git(&repo, &["add", "."]);
    git(
        &repo,
        &["commit", "-qm", "synthetic fetch with executable config"],
    );
    let change = up_to_accepted(dir.path(), &repo, &db);
    let out = check_for_change(dir.path(), &repo, &db, &change);
    assert_eq!(out.code, 101, "{}", out.stderr);
    assert!(
        !compiler_marker.exists(),
        "the environment step executed project-configured compiler code"
    );
    assert!(
        !build_marker.exists(),
        "project build ran despite fetch failure"
    );
    let ledger = thruline_core::ledger::Ledger::open(&db).unwrap();
    let finishes = fetch_finishes(&ledger);
    assert_eq!(
        finishes.len(),
        1,
        "one actual fetch is recorded: {finishes:?}"
    );
    assert!(
        matches!(&finishes[0].1.terminal, ReceiptTerminal::ProcessExecution { command, exit_code: Some(101) } if command.contains("fetch"))
    );
    assert_config_receipt(&finishes[0].1, &config);
    let config_rows = finishes[0].1.extra["project_configuration"]
        .as_array()
        .unwrap();
    assert!(
        config_rows
            .iter()
            .any(|entry| entry["excluded"].as_array().is_some_and(|keys| keys
                .iter()
                .any(|key| key
                    .as_str()
                    .is_some_and(|key| key == "build" || key.starts_with("build."))))),
        "executable config exclusions must be explicit: {config_rows:?}"
    );
    let executions = run_execution::all(&ledger, Some(&change)).unwrap();
    assert_eq!(executions.len(), 1);
    assert_eq!(
        executions[0].reason,
        Some(run_execution::Reason::PrerequisiteAbsent)
    );
    let binding = thruline_core::evidence_binding::load(
        &ledger,
        executions[0]
            .binding_id
            .as_ref()
            .expect("failed fetch binding"),
    )
    .unwrap()
    .unwrap();
    assert!(
        binding.binding.argv.iter().any(|arg| arg == "fetch"),
        "record claimed the requested build ran: {:?}",
        binding.binding.argv
    );
    assert!(!binding.binding.argv.iter().any(|arg| arg == "test"));
    change_evidence::derive_links(&ledger, &change, "2026-02-01T12:00:00Z").unwrap();
    let coverage = change_evidence::coverage(
        &ledger,
        &change,
        &thruline_core::change_propose::ProfileEnvironment::default(),
    )
    .unwrap();
    let criterion = coverage.criteria.first().unwrap();
    assert_eq!(criterion.state, change_evidence::CoverageState::RanNothing);
    assert!(!criterion.items.is_empty());
    assert!(criterion
        .items
        .iter()
        .all(|item| item.observed == change_evidence::Observed::Couldnt));
    let readiness = change_readiness::readiness(&ledger, &change, &coverage).unwrap();
    assert!(
        !readiness.ready,
        "Couldn't check enabled approval: {readiness:?}"
    );
    let obligation = readiness
        .obligations
        .iter()
        .find(|obligation| obligation.criterion_index == Some(1))
        .expect("criterion obligation");
    assert_eq!(obligation.outcome, change_readiness::Outcome::NeedsEvidence);
    assert!(!readiness
        .rests_on
        .iter()
        .any(|item| item.obligation_id == obligation.id));
}

#[test]
fn r2_successful_fetch_precedes_spoofing_build_and_is_not_a_second_check() {
    use thruline_core::observation::{self, ObservationType, ReceiptTerminal};
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db, change) = project_output_fixture(dir.path(), false);
    let (run_key, _) = assert_project_failure(dir.path(), &repo, &db, &change, false, 101);
    let ledger = thruline_core::ledger::Ledger::open(&db).unwrap();
    let finishes = fetch_finishes(&ledger);
    assert_eq!(
        finishes.len(),
        1,
        "a separate successful fetch must be recorded"
    );
    assert!(
        matches!(&finishes[0].1.terminal, ReceiptTerminal::ProcessExecution { command, exit_code: Some(0) } if command.contains("fetch"))
    );
    let starts = observation::of_type(&ledger, ObservationType::CommandStarted).unwrap();
    let build = starts.iter().find(|row| {
        let receipt = row.receipt().unwrap();
        receipt.extra.get("phase").and_then(|v| v.as_str()) != Some("dependency_fetch")
            && matches!(&receipt.terminal, ReceiptTerminal::ProcessExecution { command, .. } if command.contains("cargo test"))
    }).expect("actual project build start");
    assert!(
        finishes[0].0.seq < build.seq,
        "project code started before the fetch step finished"
    );
    let binding_count: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM evidence_bindings WHERE source_session=?1",
            [&run_key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        binding_count, 1,
        "successful fetch must not supply passing check evidence"
    );
}

#[cfg(unix)]
#[test]
fn r2_successful_fetch_never_runs_the_project_compiler_hook() {
    use std::os::unix::fs::PermissionsExt;
    use thruline_core::observation::ReceiptTerminal;
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path(), Project::TypeError);
    let marker = dir.path().join("compiler-cargo-homes");
    let compiler = repo.join("compiler.sh");
    std::fs::write(
        &compiler,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$CARGO_HOME\" >> {:?}\nexec rustc \"$@\"\n",
            marker.to_str().unwrap()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::create_dir(repo.join(".cargo")).unwrap();
    let config = repo.join(".cargo/config.toml");
    std::fs::write(
        &config,
        format!("[build]\nrustc = {:?}\n", compiler.to_str().unwrap()),
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "synthetic compiler phase marker"]);
    let change = up_to_accepted(dir.path(), &repo, &db);
    let out = check_for_change(dir.path(), &repo, &db, &change);
    assert_eq!(out.code, 101, "{}", out.stderr);
    assert!(out.stderr.contains("E0308"), "{}", out.stderr);
    let observed = std::fs::read_to_string(&marker).expect("project build used its compiler hook");
    let expected_home = dir.path().join("cargo-home");
    assert!(
        !observed.trim().is_empty(),
        "compiler marker must be nonempty"
    );
    for home in observed.lines() {
        assert_eq!(
            std::fs::canonicalize(home).expect("observed Cargo home"),
            std::fs::canonicalize(&expected_home).unwrap(),
            "the dependency-fetch phase executed the project compiler hook: {observed}"
        );
    }
    let ledger = thruline_core::ledger::Ledger::open(&db).unwrap();
    let finishes = fetch_finishes(&ledger);
    assert_eq!(finishes.len(), 1, "the fixture reached a successful fetch");
    assert!(matches!(
        finishes[0].1.terminal,
        ReceiptTerminal::ProcessExecution {
            exit_code: Some(0),
            ..
        }
    ));
}

#[test]
fn r2_relative_project_source_config_is_applied_and_recorded_before_compile_failure() {
    use thruline_core::observation::ReceiptTerminal;
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path(), Project::TypeError);
    let dependency = repo.join("vendor/offline_local_dep");
    std::fs::create_dir_all(dependency.join("src")).unwrap();
    std::fs::write(
        dependency.join("Cargo.toml"),
        "[package]\nname = \"offline_local_dep\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(dependency.join("src/lib.rs"), "pub fn local() {}\n").unwrap();
    std::fs::write(
        dependency.join(".cargo-checksum.json"),
        r#"{"files":{},"package":null}"#,
    )
    .unwrap();
    let mut manifest = std::fs::read_to_string(repo.join("Cargo.toml")).unwrap();
    manifest.push_str("offline_local_dep = \"=0.1.0\"\n");
    std::fs::write(repo.join("Cargo.toml"), manifest).unwrap();
    std::fs::write(repo.join("Cargo.lock"), "version = 4\n\n[[package]]\nname = \"offline_fixture\"\nversion = \"0.1.0\"\ndependencies = [\"offline_local_dep\"]\n\n[[package]]\nname = \"offline_local_dep\"\nversion = \"0.1.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n").unwrap();
    std::fs::create_dir(repo.join(".cargo")).unwrap();
    let config = repo.join(".cargo/config.toml");
    std::fs::write(
        &config,
        "[source.crates-io]\nreplace-with = \"local\"\n[source.local]\ndirectory = \"vendor\"\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(
        &repo,
        &["commit", "-qm", "synthetic relative vendored dependency"],
    );
    let change = up_to_accepted(dir.path(), &repo, &db);
    let out = check_for_change(dir.path(), &repo, &db, &change);
    assert_eq!(out.code, 101, "{}", out.stderr);
    assert!(
        out.stderr.contains("error[E0308]"),
        "valid vendored fetch must reach the project compile error: {}",
        out.stderr
    );
    let ledger = thruline_core::ledger::Ledger::open(&db).unwrap();
    let finishes = fetch_finishes(&ledger);
    assert_eq!(finishes.len(), 1);
    assert!(matches!(
        finishes[0].1.terminal,
        ReceiptTerminal::ProcessExecution {
            exit_code: Some(0),
            ..
        }
    ));
    assert_config_receipt(&finishes[0].1, &config);
    let executions = thruline_core::run_execution::all(&ledger, Some(&change)).unwrap();
    assert_eq!(executions.len(), 1);
    assert_ne!(
        executions[0].reason,
        Some(thruline_core::run_execution::Reason::PrerequisiteAbsent)
    );
}

fn fetch_with_missing_archive(named_registry: bool) {
    use thruline_core::{change_evidence, run_execution};
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path(), Project::NeedsADependency);
    let registry = dir.path().join("registry");
    let index = registry.join("index/sy/nt");
    std::fs::create_dir_all(&index).unwrap();
    std::fs::write(
        index.join("synthetic-fetchdep"),
        format!(
            "{}\n",
            serde_json::json!({
                "name":"synthetic-fetchdep", "vers":"0.1.0", "deps":[],
                "cksum":"0".repeat(64), "features":{}, "yanked":false
            })
        ),
    )
    .unwrap();
    let dependency = if named_registry {
        "synthetic-fetchdep = {version = \"=0.1.0\", registry = \"synthetic\"}"
    } else {
        "synthetic-fetchdep = \"=0.1.0\""
    };
    let manifest = std::fs::read_to_string(repo.join("Cargo.toml"))
        .unwrap()
        .replace("serde = \"1\"", dependency);
    std::fs::write(repo.join("Cargo.toml"), manifest).unwrap();
    std::fs::create_dir(repo.join(".cargo")).unwrap();
    let source = if named_registry {
        "[source.synthetic]\nregistry = \"sparse+https://synthetic.invalid/\"\n"
    } else {
        "[source.crates-io]\n"
    };
    std::fs::write(
        repo.join(".cargo/config.toml"),
        format!(
            "{source}replace-with = \"local\"\n[source.local]\nlocal-registry = {:?}\n",
            registry.to_str().unwrap()
        ),
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "commit",
            "-qm",
            "synthetic metadata with absent crate archive",
        ],
    );
    let change = up_to_accepted(dir.path(), &repo, &db);
    let environment = if named_registry {
        vec![(
            "CARGO_REGISTRIES_SYNTHETIC_INDEX",
            "sparse+https://synthetic.invalid/",
        )]
    } else {
        vec![]
    };
    let out = run_with_env(
        dir.path(),
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "cargo",
            "test",
        ],
        &environment,
    );
    assert_eq!(out.code, 101, "{}", out.stderr);
    assert!(
        repo.join("Cargo.lock").is_file(),
        "fetch must resolve the configured registry before the archive download fails: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("synthetic-fetchdep-0.1.0.crate"),
        "failure must be the absent local archive: {}",
        out.stderr
    );
    let ledger = thruline_core::ledger::Ledger::open(&db).unwrap();
    let executions = run_execution::all(&ledger, Some(&change)).unwrap();
    assert_eq!(executions.len(), 1);
    assert_eq!(
        executions[0].reason,
        Some(run_execution::Reason::PrerequisiteAbsent),
        "a fetch-written lockfile cannot relabel failed fetching as project failure"
    );
    change_evidence::derive_links(&ledger, &change, "2026-02-01T12:00:00Z").unwrap();
    let coverage = change_evidence::coverage(
        &ledger,
        &change,
        &thruline_core::change_propose::ProfileEnvironment::default(),
    )
    .unwrap();
    let criterion = coverage.criteria.first().unwrap();
    assert!(!criterion.items.is_empty());
    assert!(criterion
        .items
        .iter()
        .all(|item| item.observed == change_evidence::Observed::Couldnt));
    if named_registry {
        let finishes = fetch_finishes(&ledger);
        assert_eq!(finishes.len(), 1);
        let indexes = finishes[0].1.extra["registry_index_environment"]
            .as_array()
            .expect("registry override attributed");
        assert!(
            indexes
                .iter()
                .any(|entry| entry["name"] == "CARGO_REGISTRIES_SYNTHETIC_INDEX"
                    && entry["sha256"]
                        == thruline_core::chain::sha256_hex(b"sparse+https://synthetic.invalid/")),
            "{indexes:?}"
        );
    }
}

#[test]
fn r2_fetch_failure_remains_couldnt_check_when_fetch_created_a_lockfile() {
    fetch_with_missing_archive(false);
}

#[test]
fn r2_fetch_preserves_and_records_the_named_registry_index_environment() {
    fetch_with_missing_archive(true);
}

#[test]
fn r2_missing_manifest_is_recorded_by_the_owned_fetch() {
    let dir = tempfile::tempdir().unwrap();
    let (repo, db) = fixture(dir.path(), Project::TypeError);
    std::fs::remove_file(repo.join("Cargo.toml")).unwrap();
    std::fs::remove_file(repo.join("Cargo.lock")).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "synthetic missing manifest"]);
    let change = up_to_accepted(dir.path(), &repo, &db);
    let out = check_for_change(dir.path(), &repo, &db, &change);
    assert_eq!(out.code, 101, "{}", out.stderr);
    let ledger = thruline_core::ledger::Ledger::open(&db).unwrap();
    assert_eq!(fetch_finishes(&ledger).len(), 1);
    let executions = thruline_core::run_execution::all(&ledger, Some(&change)).unwrap();
    assert_eq!(executions.len(), 1);
    assert_eq!(
        executions[0].reason,
        Some(thruline_core::run_execution::Reason::PrerequisiteAbsent)
    );
    let binding =
        thruline_core::evidence_binding::load(&ledger, executions[0].binding_id.as_ref().unwrap())
            .unwrap()
            .unwrap();
    assert!(binding.binding.argv.iter().any(|arg| arg == "fetch"));
    assert!(!binding.binding.argv.iter().any(|arg| arg == "test"));
}

// Defensive target admission under ruling D. The JSON has no backend or
// executable content; these tests make no claim that code execution occurred.
fn custom_target_is_refused_without_an_execution(origin: &str) {
    use thruline_core::{change_evidence, change_readiness, observation, run_execution};
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path(), Project::TypeError);
    std::fs::create_dir(repo.join("targets")).expect("target directory");
    let target = repo.join("targets/custom.json");
    std::fs::write(&target, "{}\n").expect("harmless JSON target");
    if origin == "config" || origin == "config-array" {
        std::fs::create_dir(repo.join(".cargo")).expect("config directory");
        let setting = if origin == "config" {
            "'targets/custom.json'"
        } else {
            "['aarch64-apple-darwin', 'targets/custom.json']"
        };
        std::fs::write(
            repo.join(".cargo/config.toml"),
            format!("[build]\ntarget = {setting}\n"),
        )
        .expect("target config");
    }
    git(&repo, &["add", "."]);
    git(
        &repo,
        &["commit", "-qm", "synthetic custom target admission"],
    );
    let change = up_to_accepted(dir.path(), &repo, &db);
    let mut command = vec!["cargo".to_string(), "test".to_string()];
    let target_text = target.display().to_string();
    match origin {
        "relative" => command.extend(["--target".into(), "targets/custom.json".into()]),
        "absolute" => command.push(format!("--target={target_text}")),
        "shell" => {
            command = vec![
                "sh".into(),
                "-c".into(),
                "CARGO_BUILD_TARGET='targets/custom.json' exec cargo test".into(),
            ]
        }
        "config" | "config-array" | "environment" => {}
        _ => panic!("unknown fixture origin"),
    }
    let mut args = vec!["check", "--for-change", &change, "--actor", "sam", "--"];
    args.extend(command.iter().map(String::as_str));
    let environment = if origin == "environment" {
        vec![("CARGO_BUILD_TARGET", target_text.as_str())]
    } else {
        Vec::new()
    };
    let out = run_with_env(dir.path(), &repo, &db, &args, &environment);
    assert_eq!(
        out.code, 125,
        "unsupported target was executed instead of refused: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("target") && out.stderr.contains("built-in"),
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("custom.json"),
        "refusal did not name the target: {}",
        out.stderr
    );
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
    assert!(run_execution::all(&ledger, Some(&change))
        .expect("executions")
        .is_empty());
    let bindings: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM evidence_bindings", [], |row| {
            row.get(0)
        })
        .expect("bindings");
    assert_eq!(bindings, 0, "a refused target produced approval evidence");
    for kind in [
        observation::ObservationType::CommandStarted,
        observation::ObservationType::CommandFinished,
    ] {
        for row in observation::of_type(&ledger, kind).expect("observations") {
            let receipt = row.receipt().expect("receipt");
            assert_ne!(
                receipt.extra.get("phase").and_then(|value| value.as_str()),
                Some("dependency_fetch"),
                "a refused target started the prerequisite fetch"
            );
        }
    }
    change_evidence::derive_links(&ledger, &change, "2026-02-01T12:00:00Z").expect("links");
    let coverage = change_evidence::coverage(
        &ledger,
        &change,
        &thruline_core::change_propose::ProfileEnvironment::default(),
    )
    .expect("coverage");
    assert!(coverage
        .criteria
        .iter()
        .all(|criterion| criterion.items.is_empty()));
    let readiness = change_readiness::readiness(&ledger, &change, &coverage).expect("readiness");
    assert!(!readiness.ready, "a refused custom target enabled approval");
    let obligation = readiness
        .obligations
        .iter()
        .find(|item| item.criterion_index == Some(1))
        .expect("criterion obligation");
    assert_eq!(obligation.outcome, change_readiness::Outcome::NeedsEvidence);
    assert!(!readiness
        .rests_on
        .iter()
        .any(|item| item.obligation_id == obligation.id));
}

#[test]
fn r2_custom_target_relative_cli_is_refused_before_fetch() {
    custom_target_is_refused_without_an_execution("relative");
}

#[test]
fn r2_custom_target_absolute_cli_is_refused_before_fetch() {
    custom_target_is_refused_without_an_execution("absolute");
}

#[test]
fn r2_custom_target_environment_is_refused_before_fetch() {
    custom_target_is_refused_without_an_execution("environment");
}

#[test]
fn r2_custom_target_literal_shell_is_refused_before_fetch() {
    custom_target_is_refused_without_an_execution("shell");
}

#[test]
fn r2_custom_target_config_is_refused_before_fetch() {
    custom_target_is_refused_without_an_execution("config");
}

#[test]
fn r2_custom_target_config_array_is_refused_before_fetch() {
    custom_target_is_refused_without_an_execution("config-array");
}

/// **OFFLINE-KILL-GUARD-R4 (P-2026-09-28-4 (1)).** A `cargo` outside the
/// toolchain and system directories is not a reason to refuse the check: the
/// check runs as the person's own command, and ThruLine's own fetch step is
/// skipped. Its failure then reads failed — never couldn't-check, which only a
/// failed owned fetch may authorize (R-2026-09-27-14) — even when what it
/// prints looks like a resolver failure. The record says, in words, that the
/// owned fetch did not run and why: the program, where it was found, and what
/// would allow it.
#[cfg(unix)]
#[test]
fn r4_a_cargo_outside_the_allow_list_runs_without_the_owned_fetch_and_a_failure_reads_failed() {
    use std::os::unix::fs::PermissionsExt;
    use thruline_core::observation::{self, ObservationType};
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path(), Project::NeedsADependency);
    let change = up_to_accepted(dir.path(), &repo, &db);
    let arbitrary = dir.path().join("arbitrary-bin");
    std::fs::create_dir_all(&arbitrary).expect("arbitrary bin");
    let marker = dir.path().join("outside-cargo-ran");
    let cargo = arbitrary.join("cargo");
    std::fs::write(
        &cargo,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\n\
             echo 'error: no matching package named `serde` found' >&2\n\
             echo '{FORGED_RESOLVER}' >&2\nexit 101\n",
            marker.display()
        ),
    )
    .expect("outside cargo");
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let path =
        std::env::join_paths([arbitrary.clone(), "/usr/bin".into(), "/bin".into()]).expect("path");
    let out = run_with_env(
        dir.path(),
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "cargo",
            "test",
        ],
        &[("PATH", path.to_str().expect("utf-8 path"))],
    );
    assert_eq!(
        out.code, 101,
        "the check did not run as the person's own command: {}",
        out.stderr
    );
    // The check ran as `cargo test`; ThruLine never asked this cargo to fetch.
    // (The wrapper's own `cargo --version` probe of the toolchain it examined
    // is not a fetch.)
    let asked = std::fs::read_to_string(&marker).unwrap_or_default();
    assert!(
        asked.lines().any(|line| line == "test"),
        "the outside cargo did not run the check: {asked:?}"
    );
    assert!(
        !asked.lines().any(|line| line.starts_with("fetch")),
        "the outside cargo was asked to fetch: {asked:?}"
    );
    assert!(
        out.stderr.contains("own fetch step did not run"),
        "the terminal was not told the owned fetch was skipped: {}",
        out.stderr
    );

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    assert!(
        fetch_finishes(&ledger).is_empty(),
        "an owned fetch was recorded"
    );
    let reasons: Vec<String> = observation::of_type(&ledger, ObservationType::CommandStarted)
        .expect("started observations")
        .into_iter()
        .filter_map(|row| {
            row.receipt()
                .expect("typed receipt")
                .extra
                .get("dependency_fetch_skipped")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();
    assert_eq!(
        reasons.len(),
        1,
        "the record does not say the fetch was skipped"
    );
    let why = &reasons[0];
    assert!(why.contains("cargo"), "{why}");
    assert!(why.contains(&cargo.display().to_string()), "{why}");
    assert!(
        why.contains("$RUSTUP_HOME/toolchains") && why.contains("$CARGO_HOME/bin"),
        "{why}"
    );

    thruline_core::change_evidence::derive_links(&ledger, &change, "2026-02-01T12:00:00Z")
        .expect("derive");
    let view = thruline_core::change_evidence::coverage(
        &ledger,
        &change,
        &thruline_core::change_propose::ProfileEnvironment::default(),
    )
    .expect("coverage");
    let first = view.criteria.first().expect("one accepted line");
    let observed = first
        .items
        .first()
        .map(|i| format!("{:?}", i.observed))
        .unwrap_or_else(|| "no item".to_string());
    assert_ne!(
        observed, "Couldnt",
        "a skipped fetch authorized couldn't-check"
    );
    assert_eq!(observed, "NotProven", "{}", out.stderr);
}
