//! Real controlling-terminal coverage. This entire module is absent outside cfg(test).
use super::*;
use std::cell::Cell;
use std::io::IsTerminal;

thread_local! {
    static PRESENCE_STUB: Cell<bool> = const { Cell::new(false) };
    static DEVICE_UID: Cell<Option<u32>> = const { Cell::new(None) };
    static LEDGER_SWAP: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn presence_stub_enabled() -> bool {
    PRESENCE_STUB.with(Cell::get)
}
pub(super) fn device_uid_override() -> Option<u32> {
    DEVICE_UID.with(Cell::get)
}
pub(super) fn presence_callback() {
    LEDGER_SWAP.with(|selected| {
        if let Some(path) = selected.borrow().as_ref() {
            let saved = path.with_extension("original-db");
            fs::rename(path, &saved).expect("test-only presence ledger replacement");
            std::os::unix::fs::symlink(&saved, path).expect("test-only presence ledger symlink");
        }
    });
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
    os.environ['THRULINE_C33_UNIT_MODE'] = mode
    os.environ['THRULINE_C33_UNIT_STATUS'] = status_path
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
    let executable = std::env::current_exe().expect("unit test binary");
    let output = Command::new("python3")
        .arg("-c")
        .arg(HARNESS)
        .arg(executable)
        .arg(mode)
        .arg("decisions::authority::bridge::terminal_pty_tests::c33_terminal_child")
        .output()
        .expect("real PTY harness");
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "PTY {mode} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn confirmation_succeeds_on_real_owned_controlling_terminal() {
    run("success");
}
#[test]
fn confirmation_refuses_piped_stdin_without_writing() {
    run("stdin_pipe");
}
#[test]
fn confirmation_refuses_foreign_terminal_owner_without_writing() {
    run("wrong_uid");
}
#[test]
fn confirmation_refuses_other_stdout_terminal_without_writing() {
    run("stdout_other_tty");
}
#[test]
fn confirmation_keeps_agent_context_refusal_without_writing() {
    run("agent");
}
#[test]
fn confirmation_keeps_exact_typed_challenge_without_writing() {
    run("wrong_answer");
}

#[test]
fn supersession_succeeds_on_real_owned_controlling_terminal() {
    run("supersede");
}
#[test]
fn confirmation_refuses_noncontrolling_tty_without_writing() {
    run("not_controlling");
}
#[test]
fn confirmation_refuses_background_group_without_writing() {
    run("background");
}
#[test]
fn confirmation_refuses_memory_ledger_without_writing() {
    run("memory_ledger");
}
#[test]
fn confirmation_refuses_ledger_symlink_without_writing() {
    run("ledger_symlink");
}
#[test]
fn confirmation_rechecks_ledger_after_presence_before_writing() {
    run("ledger_swap");
}
#[test]
fn presence_and_stat_hooks_are_entirely_inside_test_only_cfg() {
    let parent = include_str!("bridge.rs");
    assert!(parent.contains("#[cfg(all(test, target_os = \"macos\"))]\n#[path = \"bridge_terminal_tests.rs\"]\nmod terminal_pty_tests;"));
    assert!(parent.contains("#[cfg(test)]\n    if terminal_pty_tests::presence_stub_enabled()"));
    let production = parent
        .split("#[cfg(all(test, target_os = \"macos\"))]\n#[path = \"bridge_terminal_tests.rs\"]")
        .next()
        .expect("production source prefix");
    assert!(!production.contains("THRULINE_C33_UNIT_MODE"));
    assert!(!production.contains("THRULINE_C33_UNIT_STATUS"));
    assert!(!include_str!("../../Cargo.toml").contains("c33"));
}

#[test]
#[ignore = "private child entry point, executed only by the real PTY parent tests"]
fn c33_terminal_child() {
    use crate::decisions::params::{
        CheckParams, PathPresentParams, VisualMatchParams, VisualScreen,
    };
    use crate::decisions::storage::{
        self, AcceptanceInput, DecisionSource, NewDecision, SourceKind,
    };
    let mode = std::env::var("THRULINE_C33_UNIT_MODE").expect("unit harness mode");
    let status = std::env::var("THRULINE_C33_UNIT_STATUS").expect("unit harness result");
    let result = std::panic::catch_unwind(|| {
        if mode != "stdin_pipe" {
            assert!(std::io::stdin().is_terminal());
        }
        assert!(std::io::stdout().is_terminal());
        assert_eq!(
            agent_context(),
            mode == "agent",
            "production agent guard remains active"
        );
        let session = rustix::process::getsid(None).expect("fixture session");
        let tty_session = rustix::termios::tcgetsid(std::io::stdin());
        if mode == "not_controlling" {
            assert!(tty_session.is_err() || tty_session.unwrap() != session);
        } else if mode != "stdin_pipe" {
            assert_eq!(
                tty_session.expect("fixture controlling tty session"),
                session
            );
            let foreground =
                rustix::termios::tcgetpgrp(std::io::stdin()).expect("fixture tty foreground group");
            assert_eq!(
                foreground == rustix::process::getpgrp(),
                mode != "background",
                "background test must retain ctty but use another process group"
            );
        }
        PRESENCE_STUB.with(|flag| flag.set(true));
        if mode == "wrong_uid" {
            DEVICE_UID.with(|uid| uid.set(Some(rustix::process::getuid().as_raw() + 1)));
        }
        let directory = tempfile::tempdir().expect("throwaway ledger directory");
        let db_path = directory.path().join("ledger.db");
        let mut ledger = if mode == "memory_ledger" {
            Ledger::open_in_memory().expect("in-memory test ledger")
        } else {
            Ledger::open(&db_path).expect("throwaway ledger")
        };
        ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('c33',?1,'codex','2026-10-07T00:00:00Z')", [directory.path().to_str().expect("temp path")]).expect("test project");
        let span = "Keep the gate file";
        let decision_params = if mode == "supersede" {
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
                project_id: "c33".into(),
                source_anchor: "c33-test-only-source".into(),
                span: span.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some("c33-test-only-source".into()),
                    raw_file: "AGENTS.md".into(),
                    raw_line: 1,
                    span_start: 0,
                    span_end: span.len() as u64,
                    span: span.into(),
                    actor: Some("fixture".into()),
                    recorded_at: Some("2026-10-07T00:00:00Z".into()),
                }],
                author_actor: "fixture".into(),
                recorded_at: "2026-10-07T00:00:00Z".into(),
                params: decision_params.clone(),
            },
        )
        .expect("test decision");
        let revision = if let CheckParams::VisualMatch(mut updated) = decision_params {
            updated.screens[0].reference_path = "docs/reference/approval.new.png".into();
            updated.screens[0].reference_sha256 = "1".repeat(64);
            storage::append_revision(
                &ledger,
                storage::RevisionInput {
                    decision_id: id.clone(),
                    span: span.into(),
                    sources: vec![DecisionSource {
                        source_kind: SourceKind::RuleFile,
                        event_id: None,
                        file_anchor: Some("c33-test-only-source".into()),
                        raw_file: "AGENTS.md".into(),
                        raw_line: 1,
                        span_start: 0,
                        span_end: span.len() as u64,
                        span: span.into(),
                        actor: Some("fixture".into()),
                        recorded_at: Some("2026-10-07T00:00:00Z".into()),
                    }],
                    author_actor: "fixture".into(),
                    recorded_at: "2026-10-07T00:00:00Z".into(),
                    params: CheckParams::VisualMatch(updated),
                },
            )
            .expect("isolated visual successor")
        } else {
            1
        };
        let act = storage::accept(
            &ledger,
            AcceptanceInput {
                decision_id: id,
                revision,
                actor: "fixture".into(),
                at: "2026-10-07T00:01:00Z".into(),
                because: "Confirm isolated terminal fixture".into(),
                severity: "warns".into(),
                applies_from_version: "test-version".into(),
            },
        )
        .expect("inert test act");
        install_schema_for_write(&ledger).expect("test bridge schema");
        let count = |ledger: &Ledger, table: &str| -> i64 {
            ledger
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .expect("row count")
        };
        let before = [
            count(&ledger, "events"),
            count(&ledger, "sessions"),
            count(&ledger, "decision_person_acts"),
            count(&ledger, "decision_bridge_proofs"),
        ];
        if mode == "ledger_symlink" {
            let saved = db_path.with_extension("original-db");
            fs::rename(&db_path, &saved).expect("isolated database rename");
            std::os::unix::fs::symlink(saved, &db_path).expect("isolated database symlink");
        }
        if mode == "ledger_swap" {
            LEDGER_SWAP.with(|selected| *selected.borrow_mut() = Some(db_path.clone()));
        }
        let confirmation = if mode == "supersede" {
            confirm_terminal_supersession(
                &mut ledger,
                &act,
                SupersessionInput {
                    predecessor_revision: 1,
                    screen: "Approve-Sheet".into(),
                },
            )
        } else {
            confirm_terminal(&mut ledger, PersonActKind::Accept, &act)
        };
        if matches!(mode.as_str(), "success" | "supersede") {
            assert!(
                confirmation.is_ok(),
                "real owned PTY confirmation: {confirmation:?}"
            );
            assert!(verified(&ledger, PersonActKind::Accept, &act).expect("verify test receipt"));
            assert_eq!(
                [
                    count(&ledger, "events"),
                    count(&ledger, "sessions"),
                    count(&ledger, "decision_person_acts"),
                    count(&ledger, "decision_bridge_proofs")
                ],
                [before[0] + 1, before[1] + 1, before[2] + 1, before[3] + 1]
            );
        } else {
            let error = confirmation.expect_err("terminal must refuse").to_string();
            let expected = match mode.as_str() {
                "stdin_pipe" => "interactive terminal",
                "wrong_uid" => "terminal and process account",
                "stdout_other_tty" => "same terminal device",
                "agent" => "agent or automated context",
                "wrong_answer" => "typed confirmation did not match",
                "not_controlling" => "session controlling terminal",
                "background" => "foreground process group",
                "memory_ledger" => "file-backed ledger",
                "ledger_symlink" => "ledger and terminal account owners differ",
                "ledger_swap" => "ledger changed during terminal confirmation",
                _ => panic!("unknown test mode"),
            };
            assert!(error.contains(expected), "{mode}: {error}");
            assert_eq!(
                [
                    count(&ledger, "events"),
                    count(&ledger, "sessions"),
                    count(&ledger, "decision_person_acts"),
                    count(&ledger, "decision_bridge_proofs")
                ],
                before
            );
            assert!(!verified(&ledger, PersonActKind::Accept, &act)
                .expect("refused test act stays inert"));
        }
    });
    fs::write(status, if result.is_ok() { "PASS" } else { "FAIL" }).expect("unit harness result");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
