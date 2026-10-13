//! The synthetic claim-bearing sessions, folded and judged by the real binary.
//!
//! `crates/core/tests/fixtures/claims/` holds five spool files written so a
//! fixture session carries claims. Its README gives the conventions and the
//! reasoning behind every expectation below. Each test here rebuilds
//! `scripts/hq-invariance.sh`'s work repository byte for byte (one fixed commit
//! of one `README`), folds a copy of one fixture with `thruline fold --spool`,
//! and pins what `claims --json` and `explain --json` say about it.
//!
//! **What is pinned is the verdict record, not its prose:** claim kind,
//! verdict, verifier, the §6.1 binding, how much evidence is cited, the finding
//! detectors, the promises, the stop classification and both exit codes.
//! Reasons are matched on the fragment that states the rule, so rewording a
//! sentence is not mistaken for a verdict change.
//!
//! The last test captures fixture 01 in two separate roots and requires the
//! same fold report, claims, explanation and receipt chain head. Every id on
//! this route is derived from the evidence, so two captures of one fixture must
//! not differ.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use base64::Engine as _;
use serde_json::{json, Value};

/// The work repository's tree, re-derived from the bytes below. Every exec
/// line in the fixtures names it as its `input_tree_hash`.
const TREE: &str = "c4eeb65ef9132009ad33aeb19abd8785cad50d8c";

/// `scripts/hq-invariance.sh`'s work repository, byte for byte. Change one and
/// the tree changes, and no fixture check binds any more.
const WORK_REPO_FILE: &str = "README";
const WORK_REPO_TEXT: &str =
    "hq-invariance work repository: one fixed commit, so receipts name a non-empty tree.\n";
const WORK_REPO_MESSAGE: &str = "hq-invariance work tree";
const WORK_REPO_IDENTITY: &[(&str, &str)] = &[
    ("GIT_AUTHOR_NAME", "hq-invariance"),
    ("GIT_AUTHOR_EMAIL", "hq-invariance@invalid"),
    ("GIT_AUTHOR_DATE", "2000-01-01T00:00:00 +0000"),
    ("GIT_COMMITTER_NAME", "hq-invariance"),
    ("GIT_COMMITTER_EMAIL", "hq-invariance@invalid"),
    ("GIT_COMMITTER_DATE", "2000-01-01T00:00:00 +0000"),
];

/// What one fixture session must produce.
struct Want {
    file: &'static str,
    session: &'static str,
    claims: &'static [WantClaim],
    /// Promise kinds, in order, each expected `open`.
    promises: &'static [&'static str],
    stopped: &'static str,
    /// A fragment `why_it_stopped.summary` must carry, where the stop names a
    /// cause.
    stop_cites: Option<&'static str>,
    /// `claims` and `explain` both exit 3 when any claim is unsupported.
    exit: i32,
}

struct WantClaim {
    kind: &'static str,
    verdict: &'static str,
    verifier: &'static str,
    /// `None`: the verdict record carries no §6.1 binding (`null`).
    binding: Option<&'static str>,
    /// How many check ids the verdict cites.
    evidence: usize,
    /// `(detector, severity)`, in order.
    findings: &'static [(&'static str, &'static str)],
    /// Fragments that must each appear in some reason.
    reasons: &'static [&'static str],
}

const WANT: &[Want] = &[
    // M8's scripted fake: three asks, one done and claimed with proof, one
    // claimed with nothing behind it, one never mentioned.
    Want {
        file: "01-three-asks-one-done-one-claimed-one-untouched.jsonl",
        session: "cccccccc-0001-4000-8000-000000000000",
        claims: &[
            WantClaim {
                kind: "build_ok",
                verdict: "supported",
                verifier: "build_ok@1",
                binding: Some("ran_on_final_tree"),
                evidence: 1,
                findings: &[],
                reasons: &["ran on the final tree", "`cargo build` exited 0"],
            },
            WantClaim {
                kind: "tests_pass",
                verdict: "unverifiable",
                verifier: "tests_pass@1",
                binding: None,
                evidence: 0,
                findings: &[],
                reasons: &["no test check was captured in this session"],
            },
        ],
        promises: &[],
        stopped: "agent_finished",
        stop_cites: None,
        exit: 0,
    },
    Want {
        file: "02-supported-lint-claim.jsonl",
        session: "cccccccc-0002-4000-8000-000000000000",
        claims: &[WantClaim {
            kind: "lint_ok",
            verdict: "supported",
            verifier: "lint_ok@1",
            binding: Some("ran_on_final_tree"),
            evidence: 1,
            findings: &[],
            reasons: &[
                "ran on the final tree",
                "`cargo clippy --all-targets` exited 0",
            ],
        }],
        promises: &[],
        stopped: "agent_finished",
        stop_cites: None,
        exit: 0,
    },
    Want {
        file: "03-contradicted-tests-claim.jsonl",
        session: "cccccccc-0003-4000-8000-000000000000",
        claims: &[WantClaim {
            kind: "tests_pass",
            verdict: "unsupported",
            verifier: "tests_pass@1",
            binding: None,
            evidence: 0,
            findings: &[("test_run_failed", "fail")],
            reasons: &["a captured test run failed: `cargo test` exited 101"],
        }],
        promises: &[],
        stopped: "error",
        stop_cites: Some("Exit code 101"),
        exit: 3,
    },
    Want {
        file: "04-insufficient-evidence-tests-claim.jsonl",
        session: "cccccccc-0004-4000-8000-000000000000",
        claims: &[WantClaim {
            kind: "tests_pass",
            verdict: "unverifiable",
            verifier: "tests_pass@1",
            binding: Some("ran_on_final_tree"),
            evidence: 1,
            findings: &[],
            reasons: &[
                "ran on the final tree",
                "the runner's output could not be parsed",
            ],
        }],
        promises: &[],
        stopped: "agent_finished",
        stop_cites: None,
        exit: 0,
    },
    Want {
        file: "05-open-promise-beside-supported-build.jsonl",
        session: "cccccccc-0005-4000-8000-000000000000",
        claims: &[WantClaim {
            kind: "build_ok",
            verdict: "supported",
            verifier: "build_ok@1",
            binding: Some("ran_on_final_tree"),
            evidence: 1,
            findings: &[],
            reasons: &["ran on the final tree", "`cargo build` exited 0"],
        }],
        promises: &["follow_up"],
        stopped: "agent_finished",
        stop_cites: None,
        exit: 0,
    },
];

macro_rules! args {
    ($($a:expr),* $(,)?) => { vec![$(OsString::from($a)),*] };
}

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/claims")
}

fn want(file: &str) -> &'static Want {
    WANT.iter()
        .find(|w| w.file == file)
        .unwrap_or_else(|| panic!("no expectations for {file}"))
}

/// `git` with no system or user configuration, the work repository's fixed
/// identity and date, and its home and temp directory inside `root`.
fn git(root: &Path, cwd: &Path, args: Vec<OsString>) -> String {
    let mut command = Command::new("git");
    command
        .current_dir(cwd)
        .args(&args)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", root.join("git-home"))
        .env("TMPDIR", root.join("tmp"))
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0");
    for &(key, value) in WORK_REPO_IDENTITY {
        command.env(key, value);
    }
    let out = command
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn json_of(what: &str, out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{what} printed no JSON ({e}); exit {:?}\nstdout: {}\nstderr: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn str_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

fn detectors(claim: &Value) -> Vec<(&str, &str)> {
    claim["findings"]
        .as_array()
        .map(|f| {
            f.iter()
                .map(|x| (str_at(x, "detector"), str_at(x, "severity")))
                .collect()
        })
        .unwrap_or_default()
}

fn promise_kinds(doc: &Value) -> Vec<(&str, &str)> {
    doc["promises"]
        .as_array()
        .map(|p| {
            p.iter()
                .map(|x| (str_at(x, "kind"), str_at(x, "status")))
                .collect()
        })
        .unwrap_or_default()
}

fn chain_heads(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                if key == "event_chain_head" {
                    if let Some(head) = v.as_str() {
                        out.push(head.to_string());
                    }
                }
                chain_heads(v, out);
            }
        }
        Value::Array(items) => {
            for v in items {
                chain_heads(v, out);
            }
        }
        _ => {}
    }
}

/// The throwaway Ed25519 key `scripts/hq-invariance.sh` signs with: a PKCS#8
/// v0 header and a seed of 32 zero bytes. The label is assembled at run time
/// so this source holds no key banner.
fn zero_seed_signing_pem() -> String {
    const PKCS8_V0_ED25519: [u8; 16] = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    let mut der = PKCS8_V0_ED25519.to_vec();
    der.extend_from_slice(&[0u8; 32]);
    let label = ["PRIVATE", "KEY"].join(" ");
    let body = base64::engine::general_purpose::STANDARD.encode(der);
    format!("-----BEGIN {label}-----\n{body}\n-----END {label}-----\n")
}

/// One isolated root: a home, the work repository, a temp directory, and a
/// store per captured session.
struct Root {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    repo: PathBuf,
    commit: String,
}

/// The three outputs of one fixture's capture.
struct Capture {
    session: String,
    exec_db: PathBuf,
    fold: Output,
    claims: Output,
    explain: Output,
}

impl Root {
    fn new() -> Root {
        let dir = tempfile::tempdir().expect("tempdir");
        // Canonical, so the repository key `fold` registers from `--repo` is
        // the one `receipt` derives (a macOS temp directory is a symlink).
        let root = dir.path().canonicalize().expect("canonical temp root");
        for sub in ["home/.thruline", "git-home", "tmp"] {
            std::fs::create_dir_all(root.join(sub)).expect("root layout");
        }
        let home = root.join("home");
        let repo = root.join("repo");
        git(
            &root,
            &root,
            args![
                "-c",
                "init.defaultBranch=main",
                "init",
                "-q",
                "--template=",
                repo.as_os_str()
            ],
        );
        std::fs::write(repo.join(WORK_REPO_FILE), WORK_REPO_TEXT).expect("README");
        git(&root, &repo, args!["add", "--", WORK_REPO_FILE]);
        git(
            &root,
            &repo,
            args![
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                WORK_REPO_MESSAGE
            ],
        );
        let tree = git(&root, &repo, args!["rev-parse", "HEAD^{tree}"]);
        assert_eq!(
            tree, TREE,
            "the work repository's tree moved; every exec line in the fixtures names {TREE}, \
             so no check would bind and every supported claim would read unverifiable"
        );
        let commit = git(&root, &repo, args!["rev-parse", "HEAD"]);
        Root {
            _dir: dir,
            root,
            home,
            repo,
            commit,
        }
    }

    fn run(&self, args: Vec<OsString>) -> Output {
        Command::new(thruline())
            .args(&args)
            .current_dir(&self.root)
            // Nothing inherited: the suite may itself run inside an agent
            // session whose variables would otherwise reach the binary.
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &self.home)
            .env("THRULINE_HOME", self.home.join(".thruline"))
            .env("TMPDIR", self.root.join("tmp"))
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .env("TZ", "UTC")
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap_or_else(|e| panic!("thruline {args:?}: {e}"))
    }

    /// Fold a copy of `file` into its own store, then read claims and explain.
    fn capture(&self, file: &str) -> Capture {
        let session_dir = self
            .root
            .join("sessions")
            .join(format!("claims__{}", file.trim_end_matches(".jsonl")));
        std::fs::create_dir_all(session_dir.join("spool")).expect("spool dir");
        let spool = session_dir.join("spool").join("claude-code.jsonl");
        // A copy, never the committed file: a held-back line writes
        // `<spool>.quarantine` beside whatever was folded.
        std::fs::copy(fixtures_dir().join(file), &spool)
            .unwrap_or_else(|e| panic!("copying {file}: {e}"));
        let exec_db = session_dir.join("store").join("exec.db");

        let fold = self.run(args![
            "fold",
            "--spool",
            spool.as_os_str(),
            "--exec-db",
            exec_db.as_os_str(),
            "--repo",
            self.repo.as_os_str(),
            "--json"
        ]);
        assert!(
            !thruline_capture::fold::quarantine_path(&spool).exists(),
            "{file}: fold held a line back"
        );
        let claims = self.run(args![
            "claims",
            "--last",
            "--exec-db",
            exec_db.as_os_str(),
            "--repo",
            self.repo.as_os_str(),
            "--json"
        ]);
        let session = str_at(&json_of("claims", &claims), "session").to_string();
        assert!(
            !session.is_empty(),
            "{file}: claims --json named no session"
        );
        let explain = self.run(args![
            "explain",
            "--session",
            session.as_str(),
            "--exec-db",
            exec_db.as_os_str(),
            "--repo",
            self.repo.as_os_str(),
            "--json"
        ]);
        Capture {
            session,
            exec_db,
            fold,
            claims,
            explain,
        }
    }

    /// The `event_chain_head` a v0.1 receipt publishes for a captured session.
    fn chain_head(&self, got: &Capture) -> String {
        std::fs::create_dir_all(self.root.join("key")).expect("key dir");
        let key = self.root.join("key").join("signing.pem");
        std::fs::write(&key, zero_seed_signing_pem()).expect("signing key");
        let out = self.run(args![
            "receipt",
            "--predicate-version",
            "0.1",
            "--session",
            got.session.as_str(),
            "--sign-with",
            key.as_os_str(),
            "--exec-db",
            got.exec_db.as_os_str(),
            "--repo",
            self.repo.as_os_str(),
            "--json"
        ]);
        let envelope = json_of("receipt", &out);
        assert_eq!(out.status.code(), Some(0), "receipt: {envelope:#}");
        let payload = envelope["payload"]
            .as_str()
            .unwrap_or_else(|| panic!("the receipt envelope has no payload: {envelope:#}"));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .expect("the DSSE payload is base64");
        let statement: Value =
            serde_json::from_slice(&bytes).expect("the DSSE payload is a JSON statement");
        let mut heads = Vec::new();
        chain_heads(&statement, &mut heads);
        assert_eq!(
            heads.len(),
            1,
            "expected exactly one event_chain_head in the statement: {statement:#}"
        );
        heads.swap_remove(0)
    }

    /// Output with this root's absolute path written out, so two roots compare.
    fn relative(&self, bytes: &[u8]) -> String {
        let root = self.root.to_string_lossy();
        String::from_utf8_lossy(bytes).replace(&*root, "<ROOT>")
    }
}

fn check(want: &Want) {
    let file = want.file;
    let root = Root::new();
    let got = root.capture(file);

    // --- the fold -------------------------------------------------------------
    let fold = json_of("fold", &got.fold);
    assert_eq!(
        got.fold.status.code(),
        Some(0),
        "{file}: fold did not exit clean: {fold:#}"
    );
    assert_eq!(fold["ok"], true, "{file}: {fold:#}");
    // Eight lines; the Edit adds an fs_change and a snapshot, the Bash result a
    // snapshot: eleven events, and the exec becomes one check.
    for (key, count) in [
        ("sessions", 1),
        ("lines_read", 8),
        ("events_written", 11),
        ("events_skipped", 0),
        ("snapshots_taken", 2),
        ("exec_events", 1),
        ("checks_written", 1),
    ] {
        assert_eq!(fold[key], count, "{file}: fold.{key}: {fold:#}");
    }
    for key in [
        "snapshots_failed",
        "checks_failed",
        "quarantined",
        "sessions_in_multiple_spools",
    ] {
        assert_eq!(fold[key], json!([]), "{file}: fold.{key}: {fold:#}");
    }
    assert_eq!(
        fold["quarantine_write_error"],
        Value::Null,
        "{file}: {fold:#}"
    );

    // --- claims ---------------------------------------------------------------
    let claims = json_of("claims", &got.claims);
    assert_eq!(
        got.claims.status.code(),
        Some(want.exit),
        "{file}: claims exit code: {claims:#}"
    );
    assert_eq!(claims["session"], want.session, "{file}: {claims:#}");
    assert_eq!(claims["final_tree_hash"], TREE, "{file}: {claims:#}");
    assert_eq!(claims["checks"], 1, "{file}: {claims:#}");
    assert_eq!(claims["negated"], json!([]), "{file}: {claims:#}");
    // Commit G p4-fog: the session view's own capture flag, the one that caps
    // every claim in a session at `unverifiable` (§13). Every fixture delivers
    // its Stop after its last activity, and each receipt captured at the
    // narrator-baseline tag says `capture_degraded: false`.
    assert_eq!(
        claims["capture_degraded"],
        json!(false),
        "{file}: {claims:#}"
    );
    let listed = claims["claims"].as_array().cloned().unwrap_or_default();
    let triples: Vec<(&str, &str, &str)> = listed
        .iter()
        .map(|c| {
            (
                str_at(c, "kind"),
                str_at(c, "verdict"),
                str_at(c, "verifier"),
            )
        })
        .collect();
    let wanted: Vec<(&str, &str, &str)> = want
        .claims
        .iter()
        .map(|w| (w.kind, w.verdict, w.verifier))
        .collect();
    assert_eq!(triples, wanted, "{file}: {claims:#}");
    for (c, w) in listed.iter().zip(want.claims) {
        let kind = w.kind;
        assert_eq!(c["extractor"], "rules", "{file} {kind}: {c:#}");
        assert_eq!(
            c["binding"],
            w.binding.map_or(Value::Null, Value::from),
            "{file} {kind}: binding: {c:#}"
        );
        let evidence = c["evidence"].as_array().cloned().unwrap_or_default();
        assert_eq!(evidence.len(), w.evidence, "{file} {kind}: evidence: {c:#}");
        assert!(
            evidence
                .iter()
                .all(|e| e.as_str().is_some_and(|id| id.starts_with("chk_"))),
            "{file} {kind}: the evidence cites something other than a check: {c:#}"
        );
        assert_eq!(
            detectors(c),
            w.findings.to_vec(),
            "{file} {kind}: findings: {c:#}"
        );
        assert_eq!(c["warnings"], json!([]), "{file} {kind}: warnings: {c:#}");
        let reasons: Vec<&str> = c["reasons"]
            .as_array()
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for fragment in w.reasons {
            assert!(
                reasons.iter().any(|r| r.contains(fragment)),
                "{file} {kind}: no reason says {fragment:?}: {reasons:#?}"
            );
        }
    }
    let open: Vec<(&str, &str)> = want.promises.iter().map(|k| (*k, "open")).collect();
    assert_eq!(
        promise_kinds(&claims),
        open,
        "{file}: claims promises: {claims:#}"
    );

    // --- explain --------------------------------------------------------------
    let explain = json_of("explain", &got.explain);
    assert_eq!(
        got.explain.status.code(),
        Some(want.exit),
        "{file}: explain exit code: {explain:#}"
    );
    assert_eq!(explain["session"], want.session, "{file}: {explain:#}");
    let proof = explain["claims_vs_proof"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let pairs: Vec<(&str, &str)> = proof
        .iter()
        .map(|c| (str_at(c, "kind"), str_at(c, "verdict")))
        .collect();
    let wanted: Vec<(&str, &str)> = want.claims.iter().map(|w| (w.kind, w.verdict)).collect();
    assert_eq!(
        pairs, wanted,
        "{file}: explain disagrees with claims: {explain:#}"
    );
    for (c, w) in proof.iter().zip(want.claims) {
        assert_eq!(
            detectors(c),
            w.findings.to_vec(),
            "{file} {}: explain findings: {c:#}",
            w.kind
        );
    }
    let stopped = &explain["why_it_stopped"];
    assert_eq!(stopped["kind"], want.stopped, "{file}: {stopped:#}");
    if let Some(cause) = want.stop_cites {
        let summary = str_at(stopped, "summary");
        assert!(
            summary.contains(cause),
            "{file}: the stop does not cite {cause:?}: {summary}"
        );
    }
    assert_eq!(
        promise_kinds(&explain),
        open,
        "{file}: explain promises: {explain:#}"
    );

    // --- the work repository is untouched ---------------------------------------
    assert_eq!(
        git(
            &root.root,
            &root.repo,
            args!["status", "--porcelain=v1", "--ignored", "-uall"]
        ),
        "",
        "{file}: a command left the work repository unclean"
    );
    assert_eq!(
        git(&root.root, &root.repo, args!["rev-parse", "HEAD"]),
        root.commit,
        "{file}: a command moved the work repository's HEAD"
    );
}

#[test]
fn every_claim_fixture_has_pinned_expectations() {
    let mut on_disk: Vec<String> = std::fs::read_dir(fixtures_dir())
        .expect("crates/core/tests/fixtures/claims")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".jsonl"))
        .collect();
    on_disk.sort();
    let pinned: Vec<String> = WANT.iter().map(|w| w.file.to_string()).collect();
    assert_eq!(
        on_disk, pinned,
        "a claim fixture was added or removed without its expectations"
    );
}

#[test]
fn the_three_asks_fake_supports_the_build_and_finds_no_proof_for_the_tests() {
    check(want(
        "01-three-asks-one-done-one-claimed-one-untouched.jsonl",
    ));
}

#[test]
fn a_clean_clippy_run_supports_the_lint_claim() {
    check(want("02-supported-lint-claim.jsonl"));
}

#[test]
fn a_failing_test_run_contradicts_the_tests_claim() {
    check(want("03-contradicted-tests-claim.jsonl"));
}

#[test]
fn a_bound_test_run_whose_output_cannot_be_read_leaves_the_claim_unverifiable() {
    check(want("04-insufficient-evidence-tests-claim.jsonl"));
}

#[test]
fn an_open_promise_sits_beside_a_supported_build() {
    check(want("05-open-promise-beside-supported-build.jsonl"));
}

#[test]
fn two_captures_of_one_fixture_are_identical() {
    let file = "01-three-asks-one-done-one-claimed-one-untouched.jsonl";
    let (a, b) = (Root::new(), Root::new());
    assert_ne!(a.root, b.root, "premise: two separate roots");
    assert_eq!(
        a.commit, b.commit,
        "the work repository's commit depends on where it was built, so its snapshot \
         events would too"
    );
    let (ga, gb) = (a.capture(file), b.capture(file));
    for (what, left, right) in [
        ("fold", &ga.fold, &gb.fold),
        ("claims", &ga.claims, &gb.claims),
        ("explain", &ga.explain, &gb.explain),
    ] {
        assert_eq!(
            left.status.code(),
            right.status.code(),
            "{what}: exit codes differ"
        );
        assert_eq!(
            a.relative(&left.stdout),
            b.relative(&right.stdout),
            "{what} --json differs between two captures of {file}"
        );
    }
    let (ha, hb) = (a.chain_head(&ga), b.chain_head(&gb));
    assert!(!ha.is_empty(), "the receipt published an empty chain head");
    assert_eq!(
        ha, hb,
        "two captures of one fixture published different chain heads"
    );
}
