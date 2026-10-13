//! Entirely private, test-only native transport fixtures.
use super::*;
use std::cell::RefCell;
thread_local! { static MODE: RefCell<String> = const { RefCell::new(String::new()) }; static DB: RefCell<Option<PathBuf>> = const { RefCell::new(None) }; }
pub(super) fn native_fixture_result(expected: &Value) -> Option<Result<Vec<u8>>> {
    let mode = MODE.with(|v| v.borrow().clone());
    if mode.is_empty() {
        return None;
    }
    if mode == "cancel" || mode == "timeout" {
        return Some(Err(CoreError::Config(format!("native review {mode}"))));
    }
    if mode == "tuple_changed" {
        DB.with(|p| { let p=p.borrow(); let conn=rusqlite::Connection::open(p.as_ref().expect("fixture db")).expect("fixture connection"); conn.execute_batch("DROP TRIGGER decision_acceptances_no_update; UPDATE decision_acceptances SET rationale='Changed after displayed review'").expect("fixture tuple mutation"); });
    }
    if mode == "ledger_swap" {
        DB.with(|p| {
            let p = p.borrow();
            let p = p.as_ref().expect("fixture path");
            let saved = p.with_extension("saved");
            fs::rename(p, &saved).expect("fixture rename");
            std::os::unix::fs::symlink(saved, p).expect("fixture symlink");
        });
    }
    let mut result = expected.clone();
    if mode == "wrong_nonce" {
        result["nonce"] = json!("different");
    }
    if mode == "wrong_hash" {
        result["review_sha256"] = json!("f".repeat(64));
    }
    Some(Ok(serde_json::to_vec(&result).expect("fixture result")))
}
const HARNESS: &str = r#"
import os, sys, pty, fcntl, termios, select, time, re, tempfile, signal, json
exe, mode, test = sys.argv[1:]
master, slave = pty.openpty()
status_fd, status_path = tempfile.mkstemp(prefix='thruline-c33-test-status-')
os.close(status_fd)
pid_path = status_path + '.pid'
first = os.fork()
if first == 0:
    child = os.fork()
    if child:
        os._exit(0)
    # Unit fixture only: orphan before exec so production agent ancestry rules
    # can run unchanged. No public release executable is accepted by this rig.
    while os.getppid() != 1:
        time.sleep(.01)
    os.setsid()
    if mode != 'not_controlling':
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    leader = os.getpid()
    if mode == 'background':
        worker = os.fork()
        if worker:
            with os.fdopen(os.open(pid_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as out:
                json.dump({'leader':leader,'worker':worker}, out)
            _, status = os.waitpid(worker, 0)
            os._exit(os.waitstatus_to_exitcode(status))
        os.setpgid(0, 0)
    else:
        with os.fdopen(os.open(pid_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as out:
            json.dump({'leader':leader,'worker':leader}, out)
    os.dup2(slave, 0)
    os.dup2(slave, 1)
    os.dup2(slave, 2)
    os.close(master)
    if slave > 2:
        os.close(slave)
    if mode == 'stdin_pipe':
        r, w = os.pipe()
        os.close(w)
        os.dup2(r, 0)
        if r > 2:
            os.close(r)
    if mode == 'stdout_other_tty':
        other_master, other_slave = pty.openpty()
        os.set_inheritable(other_master, True)
        os.dup2(other_slave, 1)
        if other_slave > 2:
            os.close(other_slave)
        # Keep other_master open so stdout remains a real different tty.
    for key in list(os.environ):
        if key.startswith(('CODEX_', 'CLAUDE_', 'THRULINE_AGENT_', 'THRULINE_MODEL_')) or key in ('CLAUDECODE', 'OPENAI_AGENT', 'GITHUB_ACTIONS', 'CI'):
            del os.environ[key]
    if mode == 'agent':
        os.environ['THRULINE_AGENT_CONTEXT'] = '1'
    os.environ['THRULINE_NATIVE_UNIT_MODE'] = mode
    os.environ['THRULINE_NATIVE_UNIT_STATUS'] = status_path
    os.execv(exe, [exe, '--exact', test, '--ignored', '--nocapture', '--test-threads=1'])
os.waitpid(first, 0)
os.close(slave)
data = bytearray()
sent = False
child_ids = None
eof = False
deadline = time.monotonic() + 20
try:
    while time.monotonic() < deadline:
        if child_ids is None and os.path.exists(pid_path):
            try:
                child_ids = json.load(open(pid_path))
            except json.JSONDecodeError:
                pass
        ready, _, _ = select.select([master], [], [], .1)
        if ready:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                eof = True
                break
            if not chunk:
                eof = True
                break
            data.extend(chunk)
            challenge = re.search(rb'Type exactly: (CONFIRM [A-Z0-9]+ [a-f0-9]{12})', data)
            if challenge and not sent:
                answer = challenge.group(1) if mode != 'wrong_answer' else b'wrong answer'
                os.write(master, answer + b'\n')
                sent = True
    status = open(status_path).read()
    if not eof and child_ids is not None:
        # Only these two groups were created by this rig. The background
        # worker group is distinct; its session leader must be reaped too.
        for pid in dict.fromkeys([child_ids['worker'], child_ids['leader']]):
            try:
                os.killpg(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
    print(data.decode(errors='replace'))
    print('C33 unit mode=' + mode + ' status=' + repr(status) + ' typed=' + str(sent) + ' eof=' + str(eof))
    if status != 'PASS' or not eof:
        sys.exit(1)
finally:
    os.close(master)
    for path in (status_path, pid_path):
        try:
            os.unlink(path)
        except FileNotFoundError:
            pass
"#;
fn run(mode: &str) {
    let output = Command::new("python3")
        .arg("-c")
        .arg(HARNESS)
        .arg(std::env::current_exe().expect("unit binary"))
        .arg(mode)
        .arg("decisions::authority::bridge::native_click::tests::native_child")
        .output()
        .expect("PTY fixture");
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn native_exact_review_succeeds_on_owned_foreground_pty() {
    run("success");
}
#[test]
fn native_visual_supersession_keeps_exact_reference_pins() {
    run("visual");
}
#[test]
fn native_review_refuses_piped_stdin() {
    run("stdin_pipe");
}
#[test]
fn native_review_refuses_other_stdout() {
    run("stdout_other_tty");
}
#[test]
fn native_review_refuses_agent_context() {
    run("agent");
}
#[test]
fn native_review_refuses_noncontrolling_terminal() {
    run("not_controlling");
}
#[test]
fn native_review_refuses_background_group() {
    run("background");
}
#[test]
fn native_review_refuses_memory_ledger() {
    run("memory_ledger");
}
#[test]
fn native_review_refuses_ledger_symlink() {
    run("ledger_symlink");
}
#[test]
fn native_review_rechecks_ledger_after_presence() {
    run("ledger_swap");
}
#[test]
fn native_changed_tuple_creates_no_authority() {
    run("tuple_changed");
}
#[test]
fn native_cancel_creates_no_authority() {
    run("cancel");
}
#[test]
fn native_timeout_creates_no_authority() {
    run("timeout");
}
#[test]
fn native_wrong_nonce_creates_no_authority() {
    run("wrong_nonce");
}
#[test]
fn native_wrong_result_hash_creates_no_authority() {
    run("wrong_hash");
}
#[test]
fn native_stale_caller_review_creates_no_authority() {
    run("stale_review");
}
#[test]
fn native_result_refuses_extra_fields() {
    let expected = json!({"schema_version":1,"transport":"native_review_click","result":"confirmed","nonce":"fixture","review_sha256":"a".repeat(64),"tuple_sha256":"b".repeat(64),"account_uid":501});
    let extra = json!({"result":"confirmed","injected":true});
    assert!(validate_native_result(&serde_json::to_vec(&extra).expect("JSON"), &expected).is_err());
}
#[test]
fn native_result_refuses_missing_fields() {
    assert!(validate_native_result(b"{}", &json!({"result":"confirmed"})).is_err());
}
#[test]
fn native_result_refuses_non_json() {
    assert!(validate_native_result(b"confirmed", &json!({"result":"confirmed"})).is_err());
}
#[test]
fn native_result_refuses_duplicate_fields() {
    let expected = json!({"schema_version":1,"transport":"native_review_click","result":"confirmed","nonce":"fixture","review_sha256":"a".repeat(64),"tuple_sha256":"b".repeat(64),"account_uid":501});
    let bytes = serde_json::to_string(&expected).expect("JSON");
    let duplicated = bytes.replacen("{", "{\"result\":\"cancelled\",", 1);
    assert!(validate_native_result(duplicated.as_bytes(), &expected).is_err());
}
#[test]
fn native_result_accepts_only_exact_result() {
    let expected = json!({"schema_version":1,"transport":"native_review_click","result":"confirmed","nonce":"fixture","review_sha256":"a".repeat(64),"tuple_sha256":"b".repeat(64),"account_uid":501});
    assert!(
        validate_native_result(&serde_json::to_vec(&expected).expect("JSON"), &expected).is_ok()
    );
}
#[test]
#[ignore = "private PTY child, executed only by parent tests"]
fn native_child() {
    use crate::decisions::{
        params::{CheckParams, PathPresentParams, VisualMatchParams, VisualScreen},
        storage::{self, AcceptanceInput, DecisionSource, NewDecision, SourceKind},
    };
    let mode = std::env::var("THRULINE_NATIVE_UNIT_MODE").expect("fixture mode");
    let status = std::env::var("THRULINE_NATIVE_UNIT_STATUS").expect("fixture status");
    let result = std::panic::catch_unwind(|| {
        let dir = tempfile::tempdir().expect("fixture directory");
        let db = dir.path().join("ledger.db");
        let mut ledger = if mode == "memory_ledger" {
            Ledger::open_in_memory().expect("memory ledger")
        } else {
            Ledger::open(&db).expect("fixture ledger")
        };
        ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('native-fixture',?1,'codex','2026-10-08T00:00:00Z')",[dir.path().to_str().expect("fixture path")]).expect("project");
        let span = "Keep the gate file";
        let decision_params = if mode == "visual" {
            CheckParams::VisualMatch(VisualMatchParams {
                screens: vec![VisualScreen {
                    screen: "Approve-Sheet".into(),
                    route: "#/approve-sheet".into(),
                    scenario: "design".into(),
                    legacy_route: None,
                    legacy_scenario: None,
                    reference_path: "docs/reference/approval.png".into(),
                    reference_sha256: "0".repeat(64),
                    viewport_width: 1512,
                    viewport_height: 920,
                    full_page: false,
                    mask_rectangles: Vec::new(),
                }],
                tolerance_pct: 0.5,
            })
        } else {
            CheckParams::PathPresent(PathPresentParams {
                path: "src/gate.rs".into(),
                sha256: None,
            })
        };

        let id = storage::create(
            &ledger,
            NewDecision {
                project_id: "native-fixture".into(),
                source_anchor: "fixture".into(),
                span: span.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some("fixture".into()),
                    raw_file: "AGENTS.md".into(),
                    raw_line: 1,
                    span_start: 0,
                    span_end: span.len() as u64,
                    span: span.into(),
                    actor: Some("fixture".into()),
                    recorded_at: Some("2026-10-08T00:00:00Z".into()),
                }],
                author_actor: "fixture".into(),
                recorded_at: "2026-10-08T00:00:00Z".into(),
                params: decision_params.clone(),
            },
        )
        .expect("decision");
        let revision = if let CheckParams::VisualMatch(mut updated) = decision_params {
            updated.screens[0].reference_path = "docs/reference/approval.new.png".into();
            updated.screens[0].reference_sha256 = "1".repeat(64);
            let sources: String = ledger
                .conn()
                .query_row(
                    "SELECT sources_json FROM decision_revisions WHERE decision_id=?1 AND rev=1",
                    [&id],
                    |r| r.get(0),
                )
                .expect("fixture sources");
            storage::append_revision(
                &ledger,
                storage::RevisionInput {
                    decision_id: id.clone(),
                    span: span.into(),
                    sources: serde_json::from_str(&sources).expect("sources"),
                    author_actor: "fixture".into(),
                    recorded_at: "2026-10-08T00:00:30Z".into(),
                    params: CheckParams::VisualMatch(updated),
                },
            )
            .expect("visual successor")
        } else {
            1
        };
        let act = storage::accept(
            &ledger,
            AcceptanceInput {
                decision_id: id,
                revision,
                actor: "fixture".into(),
                at: "2026-10-08T00:01:00Z".into(),
                because: "Confirm isolated native fixture".into(),
                severity: "warns".into(),
                applies_from_version: "fixture-version".into(),
            },
        )
        .expect("inert acceptance");
        install_schema_for_write(&ledger).expect("fixture schema");
        let selection = (mode == "visual").then(|| SupersessionInput {
            predecessor_revision: 1,
            screen: "Approve-Sheet".into(),
        });
        let review = review_terminal_act(&ledger, PersonActKind::Accept, &act, selection.as_ref())
            .expect("review");
        let hash = if mode == "stale_review" {
            "0".repeat(64)
        } else {
            sha256_hex(&serde_json::to_vec(&review).expect("JSON"))
        };
        MODE.with(|v| *v.borrow_mut() = mode.clone());
        DB.with(|v| *v.borrow_mut() = Some(db.clone()));
        if mode == "ledger_symlink" {
            let saved = db.with_extension("saved");
            fs::rename(&db, &saved).expect("rename");
            std::os::unix::fs::symlink(saved, &db).expect("symlink");
        }
        let availability = native_click_availability(&ledger);
        if matches!(
            mode.as_str(),
            "success"
                | "visual"
                | "cancel"
                | "timeout"
                | "wrong_nonce"
                | "wrong_hash"
                | "stale_review"
                | "ledger_swap"
                | "tuple_changed"
        ) {
            assert!(availability.available, "preflight: {availability:?}");
        } else {
            assert!(!availability.available);
        }
        let count = |table: &str| -> i64 {
            ledger
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .expect("count")
        };
        let before = [
            count("events"),
            count("sessions"),
            count("decision_person_acts"),
            count("decision_bridge_proofs"),
        ];
        let result =
            confirm_native_click(&mut ledger, PersonActKind::Accept, &act, selection, &hash);
        if mode == "success" || mode == "visual" {
            assert!(result.is_ok(), "native confirmation: {result:?}");
            assert!(verified(&ledger, PersonActKind::Accept, &act).expect("verification"));
        } else {
            let error = result.expect_err("must refuse").to_string();
            let fragment = match mode.as_str() {
                "stdin_pipe" => "interactive terminal",
                "stdout_other_tty" => "same terminal device",
                "agent" => "agent or automated context",
                "not_controlling" => "session controlling terminal",
                "background" => "foreground process group",
                "memory_ledger" => "file-backed ledger",
                "ledger_symlink" => "ledger and terminal account owners differ",
                "ledger_swap" => "ledger changed during native confirmation",
                "cancel" => "cancel",
                "timeout" => "timeout",
                "wrong_nonce" | "wrong_hash" => "native result",
                "stale_review" => "review SHA",
                "tuple_changed" => "reviewed decision changed",
                _ => panic!("unknown mode"),
            };
            assert!(error.contains(fragment), "{mode}: {error}");
            let count = |table: &str| -> i64 {
                ledger
                    .conn()
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                    .expect("count")
            };
            assert_eq!(
                [
                    count("events"),
                    count("sessions"),
                    count("decision_person_acts"),
                    count("decision_bridge_proofs")
                ],
                before
            );
            assert!(!dir.path().join("decision-confirmations").exists());
        }
    });
    fs::write(status, if result.is_ok() { "PASS" } else { "FAIL" }).expect("fixture status");
    if let Err(p) = result {
        std::panic::resume_unwind(p);
    }
}

#[test]
fn native_test_hook_has_no_production_escape() {
    let source = include_str!("native_click.rs");
    assert!(source.contains(
        "#[cfg(test)]\n    if let Some(result) = tests::native_fixture_result(expected)"
    ));
    assert!(source
        .contains("#[cfg(all(test, target_os = \"macos\"))]\n#[path = \"native_click_tests.rs\"]"));
    assert!(!source.contains("THRULINE_NATIVE_UNIT_MODE"));
    assert!(!include_str!("native_review.swift").contains("THRULINE_NATIVE_UNIT_"));
    assert!(!include_str!("../../Cargo.toml").contains("native-click-stub"));
}

#[test]
fn native_envelope_requires_explicit_supersession_field() {
    let value =
        json!({"schema_version":1,"kind":"native_review_click","review_sha256":"a".repeat(64)});
    assert!(parse_envelope(&value.to_string()).is_err());
}
#[test]
fn native_runtime_timeout_kills_and_reaps_child() {
    let started = Instant::now();
    let error = bounded_status(
        Command::new("/bin/sleep").arg("2"),
        Duration::from_millis(50),
    )
    .expect_err("actual timeout");
    assert!(error.to_string().contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(1));
}
