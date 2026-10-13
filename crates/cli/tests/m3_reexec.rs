//! M3's done-when, half one (BUILD-SPEC §17):
//!
//! > *a claimed `pytest` run is re-executed at the final tree in a clean
//! > container and the receipt shows `reexecuted: true`*
//!
//! The container half needs a running Docker daemon, so it is `#[ignore]`d and
//! run explicitly; its result is recorded in `REPORT.md` with the image digest
//! it produced. **Everything that does not need a daemon is not ignored**, and
//! that is deliberate: the gate, the derivation rule and the forgery refusals
//! are the parts an adversary attacks, and they must run in every suite.
//!
//! # The construction each test is against
//!
//! - **Forging `reexecuted: true`.** The field is derived at receipt-build time
//!   from the §8.1 `container_image_digest` on a `Check` record, and there is no
//!   other producer. Two controls: a source census that the only assignment is
//!   that lookup, and a runtime test that a session with no such digest yields
//!   `false` while one with it yields `true`.
//! - **Gated, not annotated.** A degraded session must make `reexec` refuse,
//!   and `reexecuted: true` must be unreachable for it.
//! - **A container that is not the tree.** Covered where it can be
//!   demonstrated rather than asserted, in
//!   `crates/reexec/tests/materialize.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "support/qualified_binary.rs"]
mod qualified_binary;
use qualified_binary::thruline;

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn pytest_available() -> bool {
    Command::new("pytest")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

struct Session {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    shim: PathBuf,
    session: String,
}

impl Session {
    /// A repository with a passing pytest suite and a session that ran it
    /// through the shim, so the `exec` event is one this system captured.
    fn record(filter: Option<&str>) -> Session {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path().join("home");
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).expect("repo");
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "a@e.invalid"],
            vec!["config", "user.name", "A"],
        ] {
            let out = Command::new("git")
                .current_dir(&repo)
                .args(&args)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}");
        }
        std::fs::write(
            repo.join("shipping.py"),
            "def rate(weight_kg):\n    return round(2.5 * weight_kg + 4.0, 2)\n",
        )
        .expect("w");
        std::fs::write(
            repo.join("test_shipping.py"),
            "from shipping import rate\n\n\
             def test_rate_small():\n    assert rate(2) == 9.0\n\n\
             def test_rate_large():\n    assert rate(10) == 29.0\n",
        )
        .expect("w");
        // The lockfile that both identifies the ecosystem and keys the
        // dependency layer.
        std::fs::write(repo.join("requirements.txt"), "pytest==9.0.1\n").expect("w");
        std::fs::write(repo.join(".gitignore"), "__pycache__/\n.pytest_cache/\n").expect("w");
        if let Some(f) = filter {
            std::fs::write(repo.join(".gitattributes"), f).expect("w");
        }
        for args in [vec!["add", "-A"], vec!["commit", "-qm", "baseline"]] {
            let out = Command::new("git")
                .current_dir(&repo)
                .args(&args)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}");
        }

        let shim = home.join("shim");
        let session = "33333333-4444-4555-8666-777777777777".to_string();
        let out = Command::new(thruline())
            .args(["shim", "install"])
            .env("THRULINE_HOME", &home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&repo)
            .output()
            .expect("shim install");
        assert!(out.status.success());

        let s = Session {
            _dir: dir,
            home,
            repo,
            shim,
            session,
        };
        s.hook(&format!(
            r#"{{"hook_event_name":"SessionStart","session_id":"{}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
            s.session,
            s.repo.display()
        ));
        if pytest_available() {
            let out = s.bash("pytest -q");
            assert!(
                out.status.success(),
                "the baseline suite must pass: {}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
        s.hook(&format!(
            r#"{{"hook_event_name":"Stop","session_id":"{}","transcript_path":"/t","cwd":"{}","last_assistant_message":"All tests pass.","stop_hook_active":false}}"#,
            s.session,
            s.repo.display()
        ));
        s.fold();
        s
    }

    fn hook(&self, json: &str) {
        use std::io::Write;
        let mut child = Command::new(thruline())
            .args(["hook", "claude-code"])
            .env("THRULINE_HOME", &self.home)
            .current_dir(&self.repo)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("hook");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(json.as_bytes())
            .expect("write");
        assert!(child.wait().expect("wait").success());
    }

    fn bash(&self, command: &str) -> std::process::Output {
        let path = format!(
            "{}:{}",
            self.shim.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new("sh")
            .args(["-c", command])
            .current_dir(&self.repo)
            .env("THRULINE_HOME", &self.home)
            .env("PATH", path)
            .env("CLAUDE_CODE_SESSION_ID", &self.session)
            .output()
            .expect("bash tool")
    }

    fn fold(&self) {
        let out = Command::new(thruline())
            .args(["fold"])
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("fold");
        assert!(
            out.status.success(),
            "fold: {}",
            String::from_utf8_lossy(&out.stdout)
        );
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(thruline())
            .args(args)
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("thruline")
    }

    fn receipt(&self) -> serde_json::Value {
        let out = self.run(&["receipt", "--json"]);
        assert!(
            out.status.success(),
            "receipt: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let envelope: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("the receipt is JSON");
        let payload = envelope["payload"].as_str().expect("payload");
        let bytes = base64_decode(payload);
        serde_json::from_slice(&bytes).expect("the payload is a Statement")
    }
}

fn base64_decode(text: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .expect("base64")
}

// ---------------------------------------------------------------------------
// The gate: refusal, not annotation.
// ---------------------------------------------------------------------------

#[test]
fn a_clean_filter_makes_reexec_refuse_rather_than_launder() {
    // C.7/3. A `.gitattributes` filter driver means the tree hash may not name
    // the bytes that ran, so materialising the tree and running its tests would
    // attest that HONEST code passes, about an artifact containing a cheat.
    // Re-execution must refuse; annotating would leave the laundering in place
    // with a footnote.
    let s = Session::record(Some("*.py filter=hide\n"));
    let out = s.run(&["reexec", "--json"]);
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("a JSON report even on refusal");
    assert_eq!(
        report["refused"]["reason"], "tree_fidelity_broken",
        "{report:#}"
    );
    assert_eq!(
        out.status.code(),
        Some(3),
        "the refusal must be visible in $?"
    );
    assert!(
        report["checks"].as_array().is_none_or(|a| a.is_empty()),
        "a refused re-execution ran something"
    );

    // And the consequence: no receipt for this session can claim re-execution.
    let statement = s.receipt();
    for check in statement["predicate"]["checks"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        assert_eq!(
            check["reexecuted"],
            serde_json::json!(false),
            "a degraded session produced a re-executed check"
        );
    }
}

#[test]
fn an_undegraded_session_is_not_refused_by_the_gate() {
    // The control. A gate that refuses everything passes the test above and is
    // worthless, so the same machinery must let an honest session through — as
    // far as the container runtime, which is where a machine without Docker
    // stops.
    if !pytest_available() {
        eprintln!("SKIPPED: no pytest, so no captured check to re-execute");
        return;
    }
    let s = Session::record(None);
    let out = s.run(&["reexec", "--json"]);
    let text = String::from_utf8_lossy(&out.stdout);
    if let Ok(report) = serde_json::from_str::<serde_json::Value>(&text) {
        assert!(
            report["refused"].is_null(),
            "the gate refused an honest session: {report:#}"
        );
    } else {
        // No JSON means the command failed before producing a report, which is
        // what a missing Docker daemon looks like. That is an environment
        // error (exit 1), never a refusal and never a pass.
        assert_eq!(
            out.status.code(),
            Some(1),
            "a failure that is not a gate refusal must exit 1, not {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("container runtime")
                || String::from_utf8_lossy(&out.stderr).contains("docker"),
            "unexpected failure: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

// ---------------------------------------------------------------------------
// `reexecuted` is derived, and there is no other producer.
// ---------------------------------------------------------------------------

#[test]
fn a_session_that_was_never_re_executed_says_so() {
    if !pytest_available() {
        eprintln!("SKIPPED: no pytest, so the session has no check at all");
        return;
    }
    let s = Session::record(None);
    let statement = s.receipt();
    let checks = statement["predicate"]["checks"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(!checks.is_empty(), "the session recorded no check");
    for check in &checks {
        assert_eq!(check["reexecuted"], serde_json::json!(false));
        assert!(
            check.get("container_image_digest").is_none(),
            "a check that was not re-executed carries a container digest"
        );
    }
    assert_eq!(statement["predicate"]["verifier"]["mode"], "local");
}

#[test]
fn the_only_producer_of_reexecuted_is_the_check_records_digest() {
    // A source census, in the shape this repository already uses for the
    // redaction call sites. The runtime tests show the two answers; this shows
    // there is no THIRD way to reach the field — a flag, an argument, or a
    // future caller that sets it directly.
    let src =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/receipt_cmd.rs"))
            .expect("receipt_cmd.rs");
    let assignments: Vec<&str> = src
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("reexecuted:"))
        .collect();
    assert_eq!(
        assignments.len(),
        1,
        "`reexecuted` is set in {} places: {assignments:?}. It must have exactly one \
         producer — the lookup against `exec` events carrying a container image digest — \
         because every other producer is a way to write a claim where evidence belongs.",
        assignments.len()
    );
    assert_eq!(
        assignments[0], "reexecuted: image_digests.contains_key(&c.id),",
        "the single assignment is no longer the digest lookup"
    );
    // And the digest it is paired with comes from the same map, so the two
    // cannot disagree — which is the state `verify` refuses.
    assert!(
        src.contains("container_image_digest: image_digests.get(&c.id).cloned(),"),
        "the digest no longer comes from the same lookup as the boolean"
    );
    // The map is built from the check record §8.1 puts the field on.
    assert!(
        src.contains(".get(\"reexec\")?"),
        "the digest map no longer reads the check's `reexec` block"
    );
}

#[test]
fn a_re_execution_leaves_the_session_chain_untouched() {
    // The defect this guards, found in my own first version and identical to
    // one this repository has already been bitten by: the fold assigns
    // `seq = ordinal * SPOOL_SEQ_STRIDE` and resumes at `seq <= last_seq`, so
    // ANY foreign writer that advances the chain tail makes the next spool line
    // hit the resume guard and be counted as ALREADY PRESENT. A real hook
    // delivery vanishes and the report calls it idempotence.
    //
    // A re-execution therefore records its digest on the Check row — where
    // §8.1 puts `container_image_digest` anyway — and appends nothing.
    let src =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/reexec_cmd.rs"))
            .expect("reexec_cmd.rs");
    for forbidden in [
        "append_events",
        "EventRow",
        "Payload::Exec",
        "genesis_prev_hash",
    ] {
        assert!(
            !src.contains(forbidden),
            "reexec_cmd.rs reaches {forbidden:?}: sharing the fold's seq space costs a \
             hook delivery, and the fold's resume point is the property its crash-safety \
             and idempotence tests rest on"
        );
    }
    // And behaviourally, on a real session: `reexec` refused here (no Docker in
    // the suite, or the gate), and the chain head is the same either way.
    if !pytest_available() {
        eprintln!("SKIPPED: no pytest, so there is no session with a check to re-execute");
        return;
    }
    let s = Session::record(None);
    let before = s.receipt()["predicate"]["session"].clone();
    let _ = s.run(&["reexec", "--json"]);
    let after = s.receipt()["predicate"]["session"].clone();
    assert_eq!(
        before, after,
        "a re-execution moved the session's chain head or event count"
    );
}

#[test]
fn both_producers_of_an_exec_event_record_their_stdio_shape() {
    // The TTY trap, pinned where it can be checked without a PTY.
    //
    // The M0.8 recorder gives its children a pipe and the container does too,
    // so today the two are comparable. The failure this guards is the day one
    // of them changes: an output comparison across a `pipe` run and a `tty` run
    // reports a digest mismatch that is not one, which in §7's vocabulary reads
    // as tampering. `Comparison::of` conditions on the field, so the field has
    // to exist on both sides.
    let recorder = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../capture/src/exec.rs"),
    )
    .expect("capture/src/exec.rs");
    let reexec =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/reexec_cmd.rs"))
            .expect("reexec_cmd.rs");
    // The re-executed check's `parsed` block is built by `thruline_reexec`, so
    // that is where its `stdio` is written.
    let parsed_block =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../reexec/src/lib.rs"))
            .expect("reexec/src/lib.rs");
    // ...and the view builder is what reads BOTH of them back. A field written
    // by two producers and read by nobody conditions nothing.
    let view =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/claims_cmd.rs"))
            .expect("claims_cmd.rs");
    for (name, src) in [
        ("the recorder", &recorder),
        ("the re-executor", &parsed_block),
        ("the view builder", &view),
    ] {
        assert!(
            src.contains("\"stdio\""),
            "{name} no longer records the stdio shape of the child, so §7's comparison \
             has nothing to condition on and a terminal difference would read as tampering"
        );
    }
    // **The branch must stay reachable.** The comparison used to be handed a
    // literal `"pipe"` for the agent's side, which made both sides equal by
    // construction: the capture-shape branch could never be taken, however the
    // event was recorded. An unreachable branch is a future loosening.
    let comparison = reexec
        .split("Comparison::of(")
        .nth(1)
        .expect("a comparison site");
    let comparison: String = comparison.chars().take(900).collect();
    assert!(
        !comparison.contains("stdio: \"pipe\""),
        "the comparison asserts the agent's stdio shape instead of reading the \
         recorded one, which makes §7's capture-shape branch unreachable"
    );
    assert!(
        comparison.contains("original_stdio"),
        "the comparison no longer reads the agent's recorded stdio shape"
    );
    // And the comparison is not made anywhere else, where it could be made
    // without the condition.
    assert!(
        reexec.contains("Comparison::of("),
        "the re-executor no longer compares through `Comparison::of`"
    );
    assert_eq!(
        reexec.matches("Comparison::of(").count(),
        1,
        "there is more than one comparison site, so one of them can disagree with the other"
    );
}

// ---------------------------------------------------------------------------
// The done-when. Needs a container runtime.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "M3's done-when: requires a running Docker daemon. Run with --ignored."]
fn m3_done_when_a_claimed_pytest_run_is_re_executed_in_a_clean_container() {
    assert!(
        docker_available(),
        "this test was asked for explicitly and there is no Docker daemon — a skip \
         here would report M3's done-when as met without running it"
    );
    assert!(
        pytest_available(),
        "the agent's own run needs a real pytest"
    );

    let s = Session::record(None);
    // Name the locally present image so the run does not depend on a registry,
    // and so the `.thruline/reexec.yml` branch of §7's selection order is the
    // one under test.
    std::fs::create_dir_all(s.repo.join(".thruline")).expect("mkdir");
    std::fs::write(
        s.repo.join(".thruline/reexec.yml"),
        "version: 1\nimage: python:3.13.7-slim-bookworm\n",
    )
    .expect("w");

    let before_head = s.receipt()["predicate"]["session"]["event_chain_head"].clone();
    let out = s.run(&["reexec", "--json", "--timeout-secs", "600"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "reexec --json: {e}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    assert!(report["refused"].is_null(), "{report:#}");
    assert_eq!(out.status.code(), Some(0), "{report:#}");

    let sandbox = &report["sandbox"];
    let digest = sandbox["image_digest"].as_str().expect("an image digest");
    assert!(digest.starts_with("sha256:"), "{digest}");
    assert_eq!(sandbox["network"], serde_json::json!(false));

    let checks = report["checks"].as_array().expect("checks");
    assert!(!checks.is_empty(), "nothing was re-executed");
    let pytest = checks
        .iter()
        .find(|c| c["runner"] == "pytest")
        .unwrap_or_else(|| panic!("no pytest check was re-executed: {report:#}"));
    assert_eq!(pytest["exit_code"], serde_json::json!(0));
    assert_eq!(pytest["parsed"]["tests_total"], serde_json::json!(2));
    assert_eq!(pytest["parsed"]["passed"], serde_json::json!(2));
    assert_eq!(
        pytest["comparison"]["outcome"], "agrees",
        "the agent's run and the re-execution disagree: {pytest:#}"
    );
    // **M3.3's scope limit, made falsifiable.** §7 says "re-run FAILED tests";
    // a command that passes on its first run is never run again, so a
    // pass-then-fail flake is invisible to this build. That is a deliberate
    // limit, not an oversight — running unconditionally would double the cost
    // of every re-execution and the spec does not ask for it — and it is named
    // in `reexec_cmd.rs`'s module docs and in `docs/STATUS.md`.
    //
    // Named in three prose places and held by nothing is this program's
    // signature defect, so it is asserted here: exactly ONE run for a command
    // that passed. If a future change re-runs passing commands, this reddens
    // and the limit has to be un-stated deliberately rather than drifting.
    assert_eq!(
        pytest["runs"].as_array().map(Vec::len),
        Some(1),
        "a command that passed on its first run was run again. That may be an \
         improvement — it would close the pass-then-fail hole — but the limit is \
         written down in two other places and they now disagree: {pytest:#}"
    );
    assert_eq!(
        pytest["deterministic"],
        serde_json::json!(true),
        "one run cannot disagree with itself: {pytest:#}"
    );
    // Exit 0 requires AGREEMENT, not merely the absence of disagreement: a
    // re-execution that could not be compared exits 2, the same three-way
    // distinction `verify-receipt` draws. Asserted here so the honest case is
    // shown to reach 0 rather than 2.
    assert_eq!(report["exit"], serde_json::json!(0), "{report:#}");

    // The receipt, which is what the done-when actually names.
    let statement = s.receipt();
    let reexecuted: Vec<&serde_json::Value> = statement["predicate"]["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .filter(|c| c["reexecuted"] == serde_json::json!(true))
        .collect();
    assert!(
        !reexecuted.is_empty(),
        "the receipt shows no re-executed check: {statement:#}"
    );
    for check in &reexecuted {
        assert_eq!(
            check["container_image_digest"].as_str(),
            Some(digest),
            "the receipt's digest is not the one the container ran under"
        );
    }
    assert_eq!(
        statement["predicate"]["verifier"]["mode"],
        "clean-sandbox-reexec"
    );
    assert_eq!(
        statement["predicate"]["session"]["event_chain_head"], before_head,
        "the re-execution moved the session's chain head, which is how a foreign \
         writer eats the next hook delivery"
    );

    // And it verifies, on this machine, with the re-execution check passing
    // rather than being refused as an unbacked assertion.
    let receipt_path = s.repo.join("receipt.json");
    let out = s.run(&[
        "receipt",
        "--predicate-version",
        "0.1",
        "--out",
        receipt_path.to_str().expect("path"),
    ]);
    assert!(out.status.success());
    let out = s.run(&[
        "verify-receipt",
        receipt_path.to_str().expect("path"),
        "--json",
    ]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("verify json");
    let reexec_check = report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|c| c["name"] == "reexecution")
        .expect("a reexecution check");
    assert_eq!(reexec_check["outcome"], "pass", "{report:#}");
    eprintln!(
        "M3 done-when: image {} digest {digest}",
        sandbox["image"].as_str().unwrap_or("?")
    );
}

#[test]
#[ignore = "requires a running Docker daemon. Run with --ignored."]
fn the_container_has_no_network_even_with_a_dependency_install_step() {
    // §7 "Network off by default" and §0.8 "nothing leaves the machine unless
    // the user enables it". The dependency layer is BUILT with network (that is
    // what installing from a lockfile means) and the run is not — so the
    // interesting case is a run that came through a layer build, which is where
    // an implementation would most plausibly leak the network through.
    assert!(docker_available(), "no Docker daemon");
    assert!(
        pytest_available(),
        "the agent's own run needs a real pytest"
    );
    let s = Session::record(None);
    std::fs::create_dir_all(s.repo.join(".thruline")).expect("mkdir");
    std::fs::write(
        s.repo.join(".thruline/reexec.yml"),
        "version: 1\nimage: python:3.13.7-slim-bookworm\n",
    )
    .expect("w");
    // A test that succeeds only if it can open an outbound connection.
    std::fs::write(
        s.repo.join("test_network.py"),
        "import socket\n\n\
         def test_can_reach_the_internet():\n\
         \x20   socket.create_connection((\"1.1.1.1\", 53), timeout=5)\n",
    )
    .expect("w");
    for args in [vec!["add", "-A"], vec!["commit", "-qm", "net"]] {
        let out = Command::new("git")
            .current_dir(&s.repo)
            .args(&args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@e.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@e.invalid")
            .output()
            .expect("git");
        assert!(out.status.success());
    }
    // Re-snapshot so the final tree contains the new test.
    let _ = s.bash("pytest -q --collect-only");
    // This fixture performed more work after record()'s original Stop. End
    // the extended session explicitly; otherwise the fidelity gate correctly
    // refuses before the network-isolation behavior under test is reached.
    s.hook(
        &serde_json::json!({"hook_event_name":"SessionEnd", "session_id":s.session,
        "cwd":s.repo,"reason":"qualification finished collecting tests"})
        .to_string(),
    );
    s.fold();

    let out = s.run(&[
        "reexec",
        "--mode",
        "full",
        "--json",
        "--timeout-secs",
        "600",
    ]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "reexec --json: {e}\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    let checks = report["checks"].as_array().expect("checks");
    let run = checks
        .first()
        .unwrap_or_else(|| panic!("expected an actual sandbox run: {report:#}"));
    assert_ne!(
        run["exit_code"],
        serde_json::json!(0),
        "a test that requires outbound network PASSED inside the sandbox — the \
         container had a network: {report:#}"
    );
    assert_eq!(report["sandbox"]["network"], serde_json::json!(false));

    // **The discriminator.** A non-zero exit alone proves nothing: a missing
    // pytest, a failed import or a bad working directory all produce one. The
    // tally shows the suite really ran and that EXACTLY the network test
    // failed — the other two passed inside the same container. Had the network
    // been up, all three would have passed.
    assert_eq!(
        run["parsed"]["tests_total"],
        serde_json::json!(3),
        "the suite did not run as expected, so the network claim is unproven: {run:#}"
    );
    assert_eq!(run["parsed"]["passed"], serde_json::json!(2), "{run:#}");
    assert_eq!(run["parsed"]["failed"], serde_json::json!(1), "{run:#}");

    // And the exit code says the run FAILED at the final tree — `--mode full`
    // has nothing to compare against, so a re-run that fails must not exit 0
    // just because no comparison contradicted it.
    assert_eq!(out.status.code(), Some(3), "{report:#}");
    assert!(run["comparison"].is_null(), "{run:#}");
}

#[test]
fn a_re_execution_that_cannot_be_compared_exits_two_and_not_zero() {
    // The collapse this closes, in §7's clothes: without a middle code, a
    // re-execution whose output could not be parsed — or one run against a
    // differently captured original — exits 0, and a CI gate reads 0 as a pass.
    //
    // Tested at the mapping rather than by manufacturing an unparsable run,
    // because the mapping is what a gate reads and the manufacture would be a
    // fixture pretending to be a runner.
    use thruline_reexec::{Comparison, RunFacts};
    let pipe = RunFacts {
        exit_code: 0,
        tally: Some((2, 2, 0)),
        stdio: "pipe".into(),
    };
    let unparsed = RunFacts {
        exit_code: 0,
        tally: None,
        stdio: "pipe".into(),
    };
    assert!(matches!(
        Comparison::of(&pipe, &unparsed),
        Comparison::NotComparable { .. }
    ));
    // And the CLI's own mapping, read from the source so a widening is a
    // visible edit: `NotComparable` reaches 2 and only agreement reaches 0.
    let src =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/reexec_cmd.rs"))
            .expect("reexec_cmd.rs");
    assert!(
        src.contains("Some(Comparison::NotComparable { .. })"),
        "the not-comparable case no longer reaches its own exit code"
    );
    assert!(
        src.contains("|| c.exit_code != 0"),
        "a re-run that failed at the final tree no longer refutes the record"
    );
}

// ---------------------------------------------------------------------------
// A refusal that is not allowed to be swallowed (Rule 12).
// ---------------------------------------------------------------------------

#[test]
fn an_unreadable_reexec_config_refuses_instead_of_degrading_to_no_network() {
    // `.thruline/reexec.yml` chooses the image, the install command and whether
    // the container gets the host network. `reexec_cmd` read it a SECOND time
    // for the network bit and wrote `ReexecConfig::load(&repo).unwrap_or(None)`
    // — a parse error became `None`, which becomes `network: false`.
    //
    // That degrades in the safe direction, which is exactly why it survived: it
    // was invisible. It was also masked, because `image::select` read the same
    // file with `?` one line earlier and refused first. A guard riding a
    // stronger veto is not a guard; it is a guard-shaped absence that becomes
    // live the moment the veto moves. This test names the property directly:
    // **a config this command cannot read is a refusal, never a default.**
    //
    // Witnessed red twice. Once against the code as it stood, with
    // `image::select`'s own read of the config deleted — the mutation that
    // simulates the veto moving, and the whole reason this was a defect rather
    // than a style note. And once against the repaired code, by turning the
    // single surviving `load` back into `unwrap_or(None)`; there is no second
    // reader left for it to ride.
    if !pytest_available() {
        eprintln!("SKIPPED: no pytest, so `reexec` stops before it reads the config");
        return;
    }
    let s = Session::record(None);
    std::fs::create_dir_all(s.repo.join(".thruline")).expect("mkdir");
    std::fs::write(
        s.repo.join(".thruline").join("reexec.yml"),
        "version: 1\nnetwork: bridge\n",
    )
    .expect("w");

    let out = s.run(&["reexec", "--json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "an unparseable reexec.yml was accepted:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("reexec.yml"),
        "the refusal does not name the file it could not read: {stderr}"
    );
    assert!(
        stderr.contains("bridge"),
        "the refusal does not say what was wrong with it: {stderr}"
    );
    // And it must not have gone on to produce a report. A report emitted beside
    // an error is the shape where the caller reads the JSON and never the exit
    // code.
    assert!(
        !stdout.contains("\"tree_hash\""),
        "a report was emitted for a session whose configuration could not be read: {stdout}"
    );
}
