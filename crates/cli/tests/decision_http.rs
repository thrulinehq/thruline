#[path = "../../core/tests/support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};

use serde_json::{json, Value};
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::decisions::params::{
    parse_params, CheckParams, Surface, TextAbsentParams, PARAMS_SCHEMA_VERSION,
};
use thruline_core::decisions::storage::{self, DecisionSource, NewDecision, SourceKind};
use thruline_core::ledger::Ledger;

struct Machine {
    dir: tempfile::TempDir,
    hq_id: String,
    decision_id: String,
    proposed_id: String,
}

fn c35_commit_rules(machine: &Machine) -> String {
    let root = machine.dir.path().join("repo");
    std::fs::write(
        root.join("AGENTS.md"),
        "Remove obsolete_marker everywhere.\nOnly touch human_scope.\n",
    )
    .unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        "rules",
    ]);
    git(&["rev-parse", "HEAD"])
}

#[test]
fn c35_history_and_person_work_routes_are_typed_and_do_not_append() {
    let machine = Machine::new();
    let version = c35_commit_rules(&machine);
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).unwrap();
    let counts = || {
        ["events", "decision_person_acts", "decision_verdicts"].map(|table| {
            ledger
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap()
        })
    };
    let before = counts();
    let server = Server::start(&machine);
    for (route, rows) in [
        ("conflicts", "conflicts"),
        ("revivals", "revivals"),
        ("open-threads", "threads"),
    ] {
        let query = if route == "open-threads" {
            String::new()
        } else {
            format!("?version={version}")
        };
        let (status, body) = server.request(
            "GET",
            &format!(
                "/api/hq/projects/{}/decisions/{route}{query}",
                machine.hq_id
            ),
            false,
            None,
        );
        assert_eq!(status, 200, "{route}: {body}");
        assert_eq!(body["schema_version"], 1, "{body}");
        assert_eq!(body["project_id"], machine.hq_id, "{body}");
        assert!(body[rows].is_array(), "{body}");
        assert!(body["missing_evidence"].is_array(), "{body}");
        if route != "open-threads" {
            assert_eq!(body["version_id"], version, "{body}");
        }
    }
    let (status, body) = server.request(
        "GET",
        &format!("/api/hq/projects/{}/decisions/proposals", machine.hq_id),
        false,
        None,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["notes_source_state"], "not_recorded", "{body}");
    assert_eq!(counts(), before);
    assert_eq!(
        ledger
            .conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='decision_rule_file_notes'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn c35_rule_import_cli_and_hq_keep_exact_parameter_needed_sources() {
    let machine = Machine::new();
    let version = c35_commit_rules(&machine);
    let db = machine.home().join(".thruline/ledger.db");
    let ledger = Ledger::open(&db).unwrap();
    let acts_before: i64 = ledger
        .conn()
        .query_row("SELECT count(*) FROM decision_person_acts", [], |row| {
            row.get(0)
        })
        .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .env("THRULINE_HOME", machine.home().join(".thruline"))
        .args(["--db"])
        .arg(&db)
        .args(["decide", "--project=project-1", "--root"])
        .arg(machine.dir.path().join("repo"))
        .args(["--json", "rules", "--version"])
        .arg(&version)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["version_id"], version, "{result}");
    assert_eq!(result["proposals"].as_array().unwrap().len(), 2, "{result}");
    let server = Server::start(&machine);
    let (status, body) = server.request(
        "GET",
        &format!("/api/hq/projects/{}/decisions/proposals", machine.hq_id),
        false,
        None,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["notes_source_state"], "recorded", "{body}");
    let note = body["notes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|note| note["code"] == "D1")
        .unwrap();
    assert_eq!(note["state"], "needs_parameters");
    assert_eq!(note["span"], "Only touch human_scope");
    assert_eq!(note["source"]["raw_line"], 2);
    assert_eq!(note["anchor"]["version_id"], version);
    assert!(note["params"].is_null() && note["decision_id"].is_null());
    assert_eq!(
        ledger
            .conn()
            .query_row("SELECT count(*) FROM decision_person_acts", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        acts_before
    );
}

#[test]
fn c35_deferred_projection_and_exact_missing_requirement_reach_cli_and_hq() {
    use thruline_core::decisions::deferred::{DeferredCondition, DeferredDecisionParams};
    use thruline_core::decisions::params::PathPresentParams;
    let machine = Machine::new();
    let version = c35_commit_rules(&machine);
    let db = machine.home().join(".thruline/ledger.db");
    let ledger = Ledger::open(&db).unwrap();
    let id = storage::create(
        &ledger,
        NewDecision {
            project_id: "project-1".into(),
            source_anchor: "deferred-source".into(),
            span: "After the date, keep AGENTS.md".into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::WorkPrompt,
                event_id: Some("deferred-source".into()),
                file_anchor: None,
                raw_file: machine
                    .dir
                    .path()
                    .join("person-session.jsonl")
                    .to_string_lossy()
                    .into(),
                raw_line: 1,
                span_start: 0,
                span_end: 29,
                span: "After the date, keep AGENTS.md".into(),
                actor: Some("person".into()),
                recorded_at: None,
            }],
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::Deferred(DeferredDecisionParams {
                condition: DeferredCondition::Date {
                    at_utc: "2026-10-08T00:00:00Z".into(),
                },
                action: Box::new(CheckParams::PathPresent(PathPresentParams {
                    path: "AGENTS.md".into(),
                    sha256: None,
                })),
            }),
        },
    )
    .unwrap();
    let acceptance = storage::accept(
        &ledger,
        storage::AcceptanceInput {
            decision_id: id.clone(),
            revision: 1,
            actor: "person".into(),
            at: "2026-10-07T00:00:02Z".into(),
            because: "Keep this deferred check".into(),
            severity: "warns".into(),
            applies_from_version: version.clone(),
        },
    )
    .unwrap();
    PersonRecorder::bridge_only(&ledger, "project-1").bind_bridge_acceptance(&ledger, &acceptance);
    let output = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .env("THRULINE_HOME", machine.home().join(".thruline"))
        .arg("--db")
        .arg(&db)
        .args(["decisions", "--project=project-1", "--root"])
        .arg(machine.dir.path().join("repo"))
        .args(["--json", "check", "--id"])
        .arg(&id)
        .args(["--version"])
        .arg(&version)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli: Value = serde_json::from_slice(&output.stdout).unwrap();
    let row = &cli["verdicts"][0];
    assert_eq!(row["record_projection"]["lifecycle"], "open", "{row}");
    assert_eq!(row["record_projection"]["due_state"], "unknown", "{row}");
    let (basis, required): (String, String) = ledger
        .conn()
        .query_row(
            "SELECT basis_json,checkable_if FROM decision_verdicts WHERE id=?1",
            [row["verdict_id"].as_str().unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let basis: Value = serde_json::from_str(&basis).unwrap();
    assert_eq!(row["record_projection"], basis["record_projection"]);
    assert_eq!(row["checkable_if"], required);
    let server = Server::start(&machine);
    let (status, body) = server.request(
        "GET",
        &format!(
            "/api/hq/projects/{}/decisions/{id}?version={version}",
            machine.hq_id
        ),
        false,
        None,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["decision"]["kind_code"], "H1");
    assert_eq!(
        body["verdict"]["record_projection"],
        row["record_projection"]
    );
    assert_eq!(body["verdict"]["checkable_if"], required);
}

#[test]
fn c35_list_reports_dormant_gate_and_available_confirmation_transport() {
    let machine = Machine::new();
    let server = Server::start(&machine);
    let (status, body) = server.request(
        "GET",
        &format!("/api/hq/projects/{}/decisions", machine.hq_id),
        false,
        None,
    );
    assert_eq!(status, 200);
    assert_eq!(body["approval_gate"]["active"], false, "{body}");
    assert!(body["approval_gate"]["reason"].is_string(), "{body}");
    assert_eq!(
        body["confirmation_transport"]["browser_available"], false,
        "{body}"
    );
    assert_eq!(
        body["confirmation_transport"]["available"], "terminal",
        "{body}"
    );
    let accepted = body["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == machine.decision_id)
        .unwrap();
    assert_eq!(accepted["blocks_approval"], false, "{accepted}");
}

#[test]
fn c35_verdict_blocking_matches_policy_authority_and_waiver() {
    let machine = Machine::new();
    let decision_id = machine.proposed_id.clone();
    let repo = machine.dir.path().join("repo");
    std::fs::create_dir(repo.join(".thruline")).unwrap();
    std::fs::write(
        repo.join(".thruline/policy.yml"),
        "version: 1\ndecisions:\n  require_hold: blocking\n  on_unverifiable: block\n",
    )
    .unwrap();
    std::fs::write(repo.join("README.md"), "fixture\n").unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        "fixture",
    ]);
    let version = git(&["rev-parse", "HEAD"]);
    let tree = git(&["rev-parse", "HEAD^{tree}"]);
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).unwrap();
    let acceptance = storage::accept(
        &ledger,
        storage::AcceptanceInput {
            decision_id: decision_id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T01:00:00Z".into(),
            because: "blocking choice".into(),
            severity: "blocks".into(),
            applies_from_version: version.clone(),
        },
    )
    .unwrap();
    PersonRecorder::bridge_only(&ledger, "project-1").bind_bridge_acceptance(&ledger, &acceptance);
    storage::record_verdict(
        &ledger,
        storage::VerdictInput {
            decision_id: decision_id.clone(),
            revision: 1,
            version_id: version.clone(),
            tree_hash: tree,
            verdict: "unverifiable".into(),
            reason: Some("recorded run missing".into()),
            checkable_if: Some("record the named run".into()),
            findings: json!([]),
            surfaces: json!([]),
            check_impl_version: thruline_core::decisions::engine::CHECK_IMPL_VERSION.into(),
            tool_versions: json!({}),
            computed_at: "2026-10-07T01:05:00Z".into(),
            basis: json!({"checked_places":0}),
        },
    )
    .unwrap();
    let server = Server::start(&machine);
    let path = format!(
        "/api/hq/projects/{}/decisions/{}?version={}",
        machine.hq_id, decision_id, version
    );
    let (_, blocked) = server.request("GET", &path, false, None);
    assert_eq!(blocked["verdict"]["blocks_approval"], true, "{blocked}");
    std::fs::write(
        repo.join(".thruline/policy.yml"),
        "version: 1\ndecisions:\n  require_hold: blocking\n  on_unverifiable: warn\n",
    )
    .unwrap();
    // A working tree move cannot make an old result claim authority for this version.
    let (_, warn) = server.request("GET", &path, false, None);
    assert_eq!(warn["verdict"]["blocks_approval"], false, "{warn}");
    git(&["checkout", "--", ".thruline/policy.yml"]);
    let waiver = storage::waive(
        &ledger,
        storage::WaiverInput {
            decision_id: decision_id.clone(),
            version_id: version,
            actor: "founder".into(),
            at: "2026-10-07T01:10:00Z".into(),
            because: "one version".into(),
            expires_version: None,
            expires_at: Some("2030-01-01T00:00:00Z".into()),
        },
    )
    .unwrap();
    PersonRecorder::bridge_only(&ledger, "project-1").bind_bridge_waiver(&ledger, &waiver);
    let (_, waived) = server.request("GET", &path, false, None);
    assert_eq!(waived["verdict"]["blocks_approval"], false, "{waived}");
}

impl Machine {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let store = home.join(".thruline");
        let project = dir.path().join("repo");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(&project)
            .status()
            .unwrap()
            .success());
        let ledger = Ledger::open(store.join("ledger.db")).unwrap();
        ledger.conn().execute(
            "INSERT INTO projects(id,root_path,source,display_name,created_at) VALUES(?1,?2,'claude_code','Fixture','2026-10-07T00:00:00Z')",
            ("project-1", project.to_str().unwrap()),
        ).unwrap();
        let raw_file = dir.path().join("person-session.jsonl");
        std::fs::write(&raw_file, "").unwrap();
        ledger.conn().execute(
            "INSERT INTO sessions(id,project_id,file_path,started_at) VALUES('http-person-session','project-1',?1,'2026-10-07T00:00:00Z')",
            [raw_file.to_str().unwrap()],
        ).unwrap();
        let make = |anchor: &str, span: &str| {
            storage::create(
                &ledger,
                NewDecision {
                    project_id: "project-1".into(),
                    source_anchor: anchor.into(),
                    span: span.into(),
                    sources: vec![DecisionSource {
                        source_kind: SourceKind::WorkPrompt,
                        event_id: Some(anchor.into()),
                        file_anchor: None,
                        raw_file: "recording.jsonl".into(),
                        raw_line: 1,
                        span_start: 0,
                        span_end: span.len() as u64,
                        span: span.into(),
                        actor: Some("person".into()),
                        recorded_at: Some("2026-10-07T00:00:00Z".into()),
                    }],
                    author_actor: "person".into(),
                    recorded_at: "2026-10-07T00:00:00Z".into(),
                    params: CheckParams::TextAbsent(TextAbsentParams {
                        test_allowlist: Vec::new(),
                        json_field_allowlist: Vec::new(),
                        conditional_path_allowlist: Vec::new(),
                        span_allowlist: Vec::new(),
                        terms: vec!["preview".into()],
                        synonyms: vec![],
                        surfaces: vec![Surface::UiCopy],
                        allowlist: vec![],
                    }),
                },
            )
            .unwrap()
        };
        let decision_id = make("event-1", "Remove preview copy");
        let proposed_id = make("event-2", "No preview labels");
        let acceptance_id = storage::accept(
            &ledger,
            storage::AcceptanceInput {
                decision_id: decision_id.clone(),
                revision: 1,
                actor: "founder".into(),
                at: "2026-10-07T00:01:00Z".into(),
                because: "standing choice".into(),
                severity: "warns".into(),
                applies_from_version: "version-1".into(),
            },
        )
        .unwrap();
        let mut machine = Self {
            dir,
            hq_id: String::new(),
            decision_id,
            proposed_id,
        };
        PersonRecorder::bridge_only(&ledger, "project-1")
            .bind_bridge_acceptance(&ledger, &acceptance_id);
        // HQ grouping still needs a source event with the recorded cwd. This
        // fixture event is deliberately not a valid person action.
        machine.person_action(&ledger, json!({"fixture_root_only": true}));
        let hq_project = thruline_core::hq::hq_projects(&ledger).unwrap().remove(0);
        assert_eq!(hq_project.root, project.to_str().unwrap());
        assert_eq!(hq_project.members[0].root_basis, "recorded_cwd");
        machine.hq_id = hq_project.id;
        machine
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn person_action(&self, ledger: &Ledger, action: Value) -> String {
        let raw_file = self.dir.path().join("person-session.jsonl");
        let seq: i64 = ledger
            .conn()
            .query_row(
                "SELECT COALESCE(MAX(seq),0)+1 FROM events WHERE session_id='http-person-session'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let prev_hash: String = if seq == 1 {
            genesis_prev_hash("project-1")
        } else {
            ledger.conn().query_row(
                "SELECT chain_hash FROM events WHERE session_id='http-person-session' AND seq=?1",
                [seq - 1],
                |row| row.get(0),
            ).unwrap()
        };
        let text = json!({"thruline_decision_act":action}).to_string();
        let raw = json!({"type":"user","sessionId":"http-person-session",
            "cwd":self.dir.path().join("repo"),
            "message":{"role":"user","content":text}})
        .to_string();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&raw_file)
            .unwrap();
        writeln!(file, "{raw}").unwrap();
        let raw_sha = sha256_hex(raw.as_bytes());
        let next_hash = chain_hash(&prev_hash, &raw_sha, "http-person-session", seq);
        let event_id = format!("http-person-event-{seq}");
        ledger
            .conn()
            .execute(
                "INSERT INTO events(id,project_id,session_id,seq,ts,kind,role,text,is_sidechain,
               raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
             VALUES(?1,'project-1','http-person-session',?2,'2026-10-07T00:02:00Z',
               'user_msg','user',?3,0,?4,?2,?5,?6,?7)",
                rusqlite::params![
                    event_id,
                    seq,
                    text,
                    raw_file.to_str(),
                    raw_sha,
                    prev_hash,
                    next_hash
                ],
            )
            .unwrap();
        event_id
    }
}

struct Server {
    child: Child,
    port: u16,
    // The CLI prints another startup line after its address; keep its pipe open.
    _stdout: BufReader<ChildStdout>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(machine: &Machine) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_thruline"));
        command
            .args(["ui", "--no-open", "--port", "0"])
            .env("HOME", machine.home())
            .env("THRULINE_HOME", machine.home().join(".thruline"))
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(machine.dir.path().join("repo"));
        Self::start_command(&mut command)
    }

    fn start_command(command: &mut Command) -> Self {
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        loop {
            let mut line = String::new();
            let Ok(read) = stdout.read_line(&mut line) else {
                break;
            };
            if read == 0 {
                break;
            }
            if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:") {
                let Ok(port) = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                else {
                    continue;
                };
                return Self {
                    child,
                    port,
                    _stdout: stdout,
                };
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("UI server did not report a port");
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        origin: bool,
        body: Option<&Value>,
    ) -> (u16, Value) {
        self.request_with_headers(method, path, origin, body, "")
    }

    fn request_with_headers(
        &self,
        method: &str,
        path: &str,
        origin: bool,
        body: Option<&Value>,
        extra: &str,
    ) -> (u16, Value) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        let encoded = body.map(Value::to_string).unwrap_or_default();
        let origin = if origin {
            format!("Origin: http://127.0.0.1:{}\r\n", self.port)
        } else {
            String::new()
        };
        let content = if body.is_some() {
            format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                encoded.len()
            )
        } else {
            String::new()
        };
        socket.write_all(format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{origin}{content}{extra}Connection: close\r\n\r\n{encoded}", self.port).as_bytes()).unwrap();
        let mut bytes = Vec::new();
        socket.read_to_end(&mut bytes).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let status = text[9..12].parse().unwrap();
        let value = serde_json::from_str(text.split_once("\r\n\r\n").unwrap().1).unwrap();
        (status, value)
    }
}

#[test]
fn server_stdout_stays_open_after_the_port_announcement() {
    let mut command = Command::new("python3");
    command
        .args([
            "-u",
            "-c",
            "import sys\nprint('ThruLine is at http://127.0.0.1:1')\nsys.stdin.buffer.read(1)\nprint('Press Ctrl+C to stop HQ')\n",
        ])
        .stdin(Stdio::piped());
    let mut server = Server::start_command(&mut command);
    // The child cannot print its next startup line until start_command has returned.
    server.child.stdin.take().unwrap().write_all(b"x").unwrap();
    assert!(
        server.child.wait().unwrap().success(),
        "the server's stdout pipe closed immediately after announcing its port"
    );
}

#[test]
fn c35_native_preparation_and_review_are_scoped_token_guarded_and_inert() {
    let machine = Machine::new();
    let version = c35_commit_rules(&machine);
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).unwrap();
    let counts = || {
        ["decision_person_acts", "decision_bridge_proofs", "events"].map(|table| {
            ledger
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap()
        })
    };
    let before = counts();
    let server = Server::start(&machine);
    let path = format!(
        "/api/hq/projects/{}/decisions/{}/prepare",
        machine.hq_id, machine.proposed_id
    );
    let body = json!({"schema_version":1,"action":"accept","revision":1,"actor":"founder",
        "because":"standing choice","severity":"blocks","applies_from_version":version});
    let (status, _) = server.request("POST", &path, false, Some(&body));
    assert_eq!(status, 403);
    let (status, _) = server.request("POST", &path, true, Some(&body));
    assert_eq!(status, 403, "a same-origin body is not a launch token");
    let (status, token_body) =
        server.request("GET", "/api/hq/decisions/confirmation-token", false, None);
    assert_eq!(status, 200);
    let token = token_body["token"].as_str().unwrap();
    let headers = format!("X-ThruLine-Run-Token: {token}\r\n");
    let (status, prepared) =
        server.request_with_headers("POST", &path, true, Some(&body), &headers);
    assert_eq!(status, 200, "{prepared}");
    assert_eq!(prepared["state"], "pending_bridge");
    assert_eq!(prepared["preparation_is_person_act"], false);
    let act = prepared["act_row_id"].as_str().unwrap();
    assert!(
        !thruline_core::decisions::authority::bridge::verified_read_only(
            ledger.conn(),
            thruline_core::decisions::authority::PersonActKind::Accept,
            act,
            std::time::Duration::from_secs(1)
        )
        .unwrap()
    );
    let request = json!({"schema_version":1,"action":"accept","act_row_id":act});
    let review_path = path.replace("/prepare", "/review");
    let (status, review) =
        server.request_with_headers("POST", &review_path, true, Some(&request), &headers);
    assert_eq!(status, 200, "{review}");
    assert_eq!(review["review"]["decision_id"], machine.proposed_id);
    assert_eq!(
        review["review_sha256"],
        sha256_hex(&serde_json::to_vec(&review["review"]).unwrap())
    );
    let mut forged = request.clone();
    forged["expected_review_sha256"] = review["review_sha256"].clone();
    forged["presence_verified"] = json!(true);
    let native_path = path.replace("/prepare", "/native-confirm");
    let (status, _) =
        server.request_with_headers("POST", &native_path, true, Some(&forged), &headers);
    assert_eq!(status, 400);
    forged.as_object_mut().unwrap().remove("presence_verified");
    let (status, refused) =
        server.request_with_headers("POST", &native_path, true, Some(&forged), &headers);
    assert_eq!(
        status, 409,
        "non-TTY server cannot produce native authority: {refused}"
    );
    assert_eq!(counts(), before);
    let foreign_path = format!(
        "/api/hq/projects/{}/decisions/{}/review",
        machine.hq_id, machine.decision_id
    );
    let (status, _) =
        server.request_with_headers("POST", &foreign_path, true, Some(&request), &headers);
    assert_eq!(
        status, 404,
        "act cannot be reviewed through another decision"
    );
}

#[test]
fn decisions_list_and_detail_use_stored_rows_without_inventing_a_verdict() {
    let machine = Machine::new();
    let server = Server::start(&machine);
    let (status, list) = server.request(
        "GET",
        &format!("/api/hq/projects/{}/decisions", machine.hq_id),
        false,
        None,
    );
    assert_eq!(status, 200);
    assert_eq!(list["schema_version"], 1);
    let accepted = list["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == machine.decision_id)
        .unwrap();
    assert_eq!(accepted["lifecycle"], "accepted");
    assert!(accepted["current_verdict"].is_null());
    let proposed = list["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == machine.proposed_id)
        .unwrap();
    assert_eq!(proposed["lifecycle"], "proposed");
    let path = format!(
        "/api/hq/projects/{}/decisions/{}",
        machine.hq_id, machine.decision_id
    );
    let (status, detail) = server.request("GET", &path, false, None);
    assert_eq!(status, 200);
    assert!(detail["verdict"].is_null());
    assert_eq!(detail["chain"].as_array().unwrap().len(), 6);
}

#[test]
fn decision_views_expose_the_hash_of_exact_stored_parameter_bytes() {
    let machine = Machine::new();
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).unwrap();
    let original: String = ledger
        .conn()
        .query_row(
            "SELECT params_json FROM decision_revisions WHERE decision_id=?1 AND rev=1",
            [&machine.proposed_id],
            |row| row.get(0),
        )
        .unwrap();
    let parsed: Value = serde_json::from_str(&original).unwrap();
    let stored = format!(" \n{}\n ", serde_json::to_string_pretty(&parsed).unwrap());
    let digest = sha256_hex(stored.as_bytes());
    assert_ne!(digest, sha256_hex(parsed.to_string().as_bytes()));
    ledger
        .conn()
        .execute(
            "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,
             text,sources_json,author_actor,recorded_at)
         SELECT decision_id,2,kind,params_schema_version,?2,text,sources_json,
             author_actor,recorded_at FROM decision_revisions WHERE decision_id=?1 AND rev=1",
            (&machine.proposed_id, &stored),
        )
        .unwrap();
    let server = Server::start(&machine);
    let base = format!("/api/hq/projects/{}/decisions", machine.hq_id);
    for (path, key) in [
        (base.clone(), "decisions"),
        (format!("{base}/proposals"), "proposals"),
    ] {
        let (status, body) = server.request("GET", &path, false, None);
        assert_eq!(status, 200);
        let decision = body[key]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == machine.proposed_id)
            .unwrap();
        assert_eq!(decision["revision"], 2);
        assert_eq!(decision["params_sha256"], digest);
    }
    let (status, detail) = server.request(
        "GET",
        &format!("{base}/{}", machine.proposed_id),
        false,
        None,
    );
    assert_eq!(status, 200);
    assert_eq!(detail["decision"]["params_sha256"], digest);
    assert_ne!(
        digest,
        sha256_hex(detail["decision"]["params"].to_string().as_bytes())
    );
}

#[test]
fn decision_mutations_require_same_origin_and_closed_json() {
    let machine = Machine::new();
    let server = Server::start(&machine);
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).unwrap();
    let params_json: String = ledger
        .conn()
        .query_row(
            "SELECT params_json FROM decision_revisions WHERE decision_id=?1 AND rev=1",
            [&machine.proposed_id],
            |row| row.get(0),
        )
        .unwrap();
    let event_id = machine.person_action(
        &ledger,
        json!({
            "schema_version":1,"action":"accept","decision_id":machine.proposed_id.clone(),
            "revision":1,"params_sha256":sha256_hex(params_json.as_bytes()),
            "severity":"warns","applies_from_version":"version-1",
            "actor":"founder","because":"needed"
        }),
    );
    let path = format!(
        "/api/hq/projects/{}/decisions/{}/accept",
        machine.hq_id, machine.proposed_id
    );
    let body = json!({
        "schema_version": 2, "event_id": event_id,
        "revision": 1, "actor": "founder", "because": "needed",
        "severity": "warns", "applies_from_version": "version-1",
        "params": { "kind": "text_absent", "terms": ["preview"], "synonyms": [], "surfaces": ["ui_copy"], "allowlist": [] }
    });
    assert_eq!(server.request("POST", &path, false, Some(&body)).0, 403);
    let mut old_schema = body.clone();
    old_schema["schema_version"] = json!(1);
    assert_eq!(
        server.request("POST", &path, true, Some(&old_schema)).0,
        400
    );
    let mut missing_event = body.clone();
    missing_event.as_object_mut().unwrap().remove("event_id");
    assert_eq!(
        server.request("POST", &path, true, Some(&missing_event)).0,
        400
    );
    let mut extra = body.clone();
    extra["unexpected"] = json!(true);
    assert_eq!(server.request("POST", &path, true, Some(&extra)).0, 400);
    let wrong_event = machine.person_action(
        &ledger,
        json!({
            "schema_version":1,"action":"accept","decision_id":machine.proposed_id.clone(),
            "revision":1,"params_sha256":sha256_hex(params_json.as_bytes()),
            "severity":"blocks","applies_from_version":"version-1",
            "actor":"founder","because":"needed"
        }),
    );
    let mut mismatch = body.clone();
    mismatch["event_id"] = json!(wrong_event);
    assert_eq!(server.request("POST", &path, true, Some(&mismatch)).0, 409);
    let (status, pending) = server.request(
        "GET",
        &format!(
            "/api/hq/projects/{}/decisions/{}",
            machine.hq_id, machine.proposed_id
        ),
        false,
        None,
    );
    assert_eq!(status, 200);
    assert_eq!(pending["decision"]["lifecycle"], "proposed");
    let (status, accepted) = server.request("POST", &path, true, Some(&body));
    assert_eq!(status, 200);
    assert_eq!(accepted["state"], "pending_bridge");
    assert_eq!(accepted["decision"]["lifecycle"], "proposed");
    let derived = machine.dir.path().join("repo/.thruline/decisions.yml");
    std::fs::remove_file(&derived).unwrap();
    let (retry_status, retried) = server.request("POST", &path, true, Some(&body));
    assert_eq!(retry_status, 200);
    assert_eq!(retried["id"], accepted["id"]);
    assert!(derived.exists(), "retry repairs the derived copy");
    let mut conflicting = body.clone();
    conflicting["because"] = json!("different acceptance");
    assert_eq!(
        server.request("POST", &path, true, Some(&conflicting)).0,
        409
    );
}

#[test]
fn proposals_and_unknown_check_jobs_have_explicit_shapes() {
    let machine = Machine::new();
    let server = Server::start(&machine);
    let (status, proposals) = server.request(
        "GET",
        &format!("/api/hq/projects/{}/decisions/proposals", machine.hq_id),
        false,
        None,
    );
    assert_eq!(status, 200);
    assert_eq!(proposals["proposals"].as_array().unwrap().len(), 1);
    assert_eq!(proposals["proposals"][0]["id"], machine.proposed_id);
    assert_eq!(
        server
            .request(
                "GET",
                "/api/hq/decisions/check?job_id=no-such-job",
                false,
                None
            )
            .0,
        404
    );
}

#[test]
fn explicit_inline_sweep_is_read_only_and_cannot_select_model_proposals() {
    let machine = Machine::new();
    let server = Server::start(&machine);
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).unwrap();
    let counts = || -> (i64, i64) {
        (
            ledger
                .conn()
                .query_row("SELECT COUNT(*) FROM decision_verdicts", [], |r| r.get(0))
                .unwrap(),
            ledger
                .conn()
                .query_row("SELECT COUNT(*) FROM decision_person_acts", [], |r| {
                    r.get(0)
                })
                .unwrap(),
        )
    };
    let before = counts();
    let path = format!("/api/hq/projects/{}/decisions/inline", machine.hq_id);
    let body = json!({"schema_version":1,"version_id":"HEAD"});
    assert_eq!(server.request("POST", &path, false, Some(&body)).0, 403);
    let (status, answer) = server.request("POST", &path, true, Some(&body));
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["diagnostic"], false);
    assert_eq!(answer["budget_ms"], 20_000);
    let rows = answer["projects"].as_array().unwrap()[0]["results"]
        .as_array()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["decision_id"], machine.decision_id);
    assert_ne!(rows[0]["decision_id"], machine.proposed_id);
    assert_eq!(
        counts(),
        before,
        "inline checks cannot append verdicts or person acts"
    );
    assert_eq!(
        server
            .request(
                "POST",
                &path,
                true,
                Some(&json!({"schema_version":1,"version_id":"HEAD","diagnose_proposals":true}))
            )
            .0,
        400
    );
}

#[test]
fn waiver_and_retirement_are_explicit_actions() {
    let machine = Machine::new();
    let server = Server::start(&machine);
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).unwrap();
    let base = format!("/api/hq/projects/{}/decisions", machine.hq_id);
    let waive = format!("{}/{}/waive", base, machine.decision_id);
    let waive_event = machine.person_action(
        &ledger,
        json!({
            "schema_version":1,"action":"waive","decision_id":machine.decision_id.clone(),
            "version_id":"version-1","actor":"founder","because":"named exception",
            "expires_version":null,"expires_at":null
        }),
    );
    assert_eq!(
        server
            .request(
                "POST",
                &waive,
                true,
                Some(&json!({
                    "schema_version":1,"event_id":waive_event.clone(),
                    "actor":"founder","because":"named exception","version_id":"version-1"
                }))
            )
            .0,
        400
    );
    let (status, answer) = server.request(
        "POST",
        &waive,
        true,
        Some(&json!({
            "schema_version": 2, "event_id": waive_event,
            "actor": "founder", "because": "named exception", "version_id": "version-1"
        })),
    );
    assert_eq!(status, 200);
    assert!(answer["id"].is_string());
    assert_eq!(answer["state"], "pending_bridge");
    assert!(answer["decision"]["verdict"].is_null());
    let retire = format!("{}/{}/retire", base, machine.decision_id);
    let retire_event = machine.person_action(
        &ledger,
        json!({
            "schema_version":1,"action":"retire","decision_id":machine.decision_id.clone(),
            "actor":"founder","because":"choice changed","superseded_by":null
        }),
    );
    assert_eq!(
        server
            .request(
                "POST",
                &retire,
                true,
                Some(&json!({
                    "schema_version":1,"event_id":retire_event.clone(),
                    "actor":"founder","because":"choice changed"
                }))
            )
            .0,
        400
    );
    let (status, answer) = server.request(
        "POST",
        &retire,
        true,
        Some(&json!({
            "schema_version": 2, "event_id": retire_event,
            "actor": "founder", "because": "choice changed"
        })),
    );
    assert_eq!(status, 200);
    assert_eq!(answer["state"], "pending_bridge");
    assert_eq!(answer["decision"]["lifecycle"], "accepted");
}

#[test]
fn five_terminal_receipts_keep_the_recorded_hq_root() {
    // C25 HQ lane need, met on hq-next/ui before the C34 merge:
    // `recent_native_files` (crates/core/src/hq.rs) no longer lets terminal
    // confirmation sessions (`decision-terminal-…`) displace the native
    // recording, so five of them leave the HQ project id where it was
    // (crates/core/tests/hq_threads.rs
    // `terminal_confirmation_logs_never_displace_the_recorded_root`). This test
    // was the diagnostic expectation of the gap; at the merge it asserts the fix.
    let machine = Machine::new();
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).expect("ledger");
    for index in 0..5 {
        let waiver = storage::waive(
            &ledger,
            storage::WaiverInput {
                decision_id: machine.decision_id.clone(),
                version_id: "version-1".into(),
                actor: "founder".into(),
                at: "2026-10-07T00:03:00Z".into(),
                because: format!("fixture exception {index}"),
                expires_version: None,
                expires_at: None,
            },
        )
        .expect("waiver row");
        PersonRecorder::bridge_only(&ledger, "project-1").bind_bridge_waiver(&ledger, &waiver);
    }
    let current = thruline_core::hq::hq_projects(&ledger).expect("HQ projects");
    assert!(
        current.iter().any(|project| project.id == machine.hq_id),
        "five terminal confirmation sessions displaced the recorded native root"
    );
    let server = Server::start(&machine);
    let (status, _) = server.request(
        "GET",
        &format!("/api/hq/projects/{}/decisions", machine.hq_id),
        false,
        None,
    );
    assert_eq!(
        status, 200,
        "the project's decisions are served under its recorded root"
    );
}

#[test]
fn detail_shows_only_a_current_person_verified_waiver() {
    let machine = Machine::new();
    let repo = machine.dir.path().join("repo");
    std::fs::write(repo.join("README.md"), "fixture\n").unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["add", "README.md"]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        "fixture",
    ]);
    let version = git(&["rev-parse", "HEAD"]);
    let tree = git(&["rev-parse", "HEAD^{tree}"]);
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).unwrap();
    storage::record_verdict(
        &ledger,
        storage::VerdictInput {
            decision_id: machine.decision_id.clone(),
            revision: 1,
            version_id: version.clone(),
            tree_hash: tree,
            verdict: "broken".into(),
            reason: Some("preview copy remains".into()),
            checkable_if: None,
            findings: json!([{"surface":"ui_copy","path":"README.md","line":1,"span":"preview"}]),
            surfaces: json!([{"surface":"ui_copy","checked_places":1,"finding_count":1,"state":"broken"}]),
            check_impl_version: "fixture".into(),
            tool_versions: json!({"git":"fixture"}),
            computed_at: "2026-10-07T00:02:00Z".into(),
            basis: json!({"checked_places":1}),
        },
    )
    .unwrap();
    let bind_waiver = |because: &str, expires_at: &str| {
        let row_id = storage::waive(
            &ledger,
            storage::WaiverInput {
                decision_id: machine.decision_id.clone(),
                version_id: version.clone(),
                actor: "founder".into(),
                at: "2026-10-07T00:03:00Z".into(),
                because: because.into(),
                expires_version: None,
                expires_at: Some(expires_at.into()),
            },
        )
        .unwrap();
        PersonRecorder::bridge_only(&ledger, "project-1").bind_bridge_waiver(&ledger, &row_id);
    };
    bind_waiver("expired exception", "2026-10-07T00:04:00Z");
    let server = Server::start(&machine);
    let path = format!(
        "/api/hq/projects/{}/decisions/{}",
        machine.hq_id, machine.decision_id
    );
    let (status, expired) = server.request("GET", &path, false, None);
    assert_eq!(status, 200);
    assert_eq!(expired["verdict"]["state"], "broken");
    assert!(expired["verdict"]["waived"].is_null(), "{expired}");

    bind_waiver("current exception", "2030-01-01T00:00:00Z");
    let (status, current) = server.request("GET", &path, false, None);
    assert_eq!(status, 200);
    assert_eq!(current["verdict"]["waived"]["because"], "current exception");
}

#[test]
fn moved_or_dirty_tree_projects_a_prior_verdict_as_stale() {
    let machine = Machine::new();
    let repo = machine.dir.path().join("repo");
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    std::fs::write(repo.join("README.md"), "original\n").unwrap();
    git(&["add", "README.md"]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        "first",
    ]);
    let first = git(&["rev-parse", "HEAD"]);
    let first_tree = git(&["rev-parse", "HEAD^{tree}"]);
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).unwrap();
    storage::record_verdict(
        &ledger,
        storage::VerdictInput {
            decision_id: machine.decision_id.clone(),
            revision: 1,
            version_id: first.clone(),
            tree_hash: first_tree,
            verdict: "holds".into(),
            reason: None,
            checkable_if: None,
            findings: json!([]),
            surfaces: json!([{"surface":"ui_copy","checked_places":1,"finding_count":0,"state":"holds"}]),
            check_impl_version: "fixture".into(),
            tool_versions: json!({"git":"fixture"}),
            computed_at: "2026-10-07T00:02:00Z".into(),
            basis: json!({"checked_places":1}),
        },
    )
    .unwrap();
    drop(ledger);
    let server = Server::start(&machine);
    let base = format!("/api/hq/projects/{}/decisions", machine.hq_id);
    let detail_path = format!("{base}/{}", machine.decision_id);
    let (status, exact) = server.request("GET", &detail_path, false, None);
    assert_eq!(status, 200);
    assert_eq!(exact["verdict"]["state"], "holds");
    assert_eq!(exact["verdict"]["basis_state"], "holds");
    assert!(exact["verdict"]["stale_reason"].is_null());

    std::fs::write(repo.join("README.md"), "edited after check\n").unwrap();
    let (status, dirty) = server.request("GET", &detail_path, false, None);
    assert_eq!(status, 200);
    assert!(dirty["selected_version"].is_null());
    assert_eq!(dirty["verdict"]["state"], "stale");
    assert_eq!(dirty["verdict"]["basis_state"], "holds");
    assert_eq!(dirty["verdict"]["version"]["version_id"], first);
    assert_eq!(dirty["verdict"]["stale_reason"], "working_tree_dirty");
    assert_eq!(dirty["chain"][5]["state"], "moved");
    let (status, dirty_list) = server.request("GET", &base, false, None);
    assert_eq!(status, 200);
    assert!(dirty_list["current_version"].is_null());
    let listed = dirty_list["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == machine.decision_id)
        .unwrap();
    assert_eq!(listed["current_verdict"]["state"], "stale");

    git(&["add", "README.md"]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        "second",
    ]);
    let second = git(&["rev-parse", "HEAD"]);
    let (status, moved) = server.request("GET", &detail_path, false, None);
    assert_eq!(status, 200);
    assert_eq!(moved["selected_version"]["version_id"], second);
    assert_eq!(moved["verdict"]["state"], "stale");
    assert_eq!(moved["verdict"]["basis_state"], "holds");
    assert_eq!(moved["verdict"]["version"]["version_id"], first);
    assert_eq!(moved["verdict"]["stale_reason"], "later_commit");
    assert_eq!(moved["chain"][5]["state"], "moved");

    let (status, historical) = server.request(
        "GET",
        &format!("{detail_path}?version={first}"),
        false,
        None,
    );
    assert_eq!(status, 200);
    assert_eq!(historical["verdict"]["state"], "holds");
    assert!(historical["verdict"]["stale_reason"].is_null());
}

#[test]
fn verdict_observer_is_read_only_while_a_writer_holds_the_record() {
    let machine = Machine::new();
    let db = machine.home().join(".thruline/ledger.db");
    let writer = rusqlite::Connection::open(&db).unwrap();
    // A count observer must not initialize a journal or contend for its writer's lock.
    writer
        .pragma_update(None, "journal_mode", "DELETE")
        .unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let observer = VerdictObserver::open(&db);
    assert!(observer.conn().is_readonly("main").unwrap());
    assert_eq!(
        observer
            .conn()
            .pragma_query_value(None, "query_only", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(observer.count(), 0);
    assert_eq!(
        observer
            .conn()
            .execute_batch("CREATE TABLE observer_must_not_write(value TEXT)")
            .unwrap_err()
            .sqlite_error_code(),
        Some(rusqlite::ErrorCode::ReadOnly)
    );
    writer.execute_batch("ROLLBACK").unwrap();
    assert_eq!(observer.count(), 0);
}

struct VerdictObserver {
    conn: rusqlite::Connection,
}

impl VerdictObserver {
    fn open(db: &std::path::Path) -> Self {
        let conn =
            rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        conn.pragma_update(None, "query_only", true).unwrap();
        Self { conn }
    }

    fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }

    fn count(&self) -> i64 {
        self.conn()
            .query_row("SELECT count(*) FROM decision_verdicts", [], |row| {
                row.get(0)
            })
            .unwrap()
    }
}

#[test]
fn check_without_project_run_permission_refuses_before_writing_a_verdict() {
    let machine = Machine::new();
    let db = machine.home().join(".thruline/ledger.db");
    // Pin the read-only observer before the server starts its background warm.
    let observer = VerdictObserver::open(&db);
    let before = observer.count();
    let server = Server::start(&machine);
    let (status, answer) = server.request(
        "POST",
        &format!("/api/hq/projects/{}/decisions/check", machine.hq_id),
        true,
        Some(&json!({"schema_version": 1, "version_id": "HEAD", "all": true})),
    );
    assert_eq!(status, 403, "{answer}");
    assert_eq!(answer["error"], "permission_required", "{answer}");
    let after = observer.count();
    assert_eq!(after, before);
}

#[test]
fn visual_detail_projects_all_fourteen_typed_screen_results() {
    let machine = Machine::new();
    let repo = machine.dir.path().join("repo");
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .expect("git runs");
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8(output.stdout)
            .expect("git text")
            .trim()
            .to_owned()
    };
    git(&["init", "-q"]);
    std::fs::write(repo.join("README.md"), "recorded UI fixture\n").expect("tree");
    git(&["add", "README.md"]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        "fixture",
    ]);
    let version = git(&["rev-parse", "HEAD"]);
    let tree = git(&["rev-parse", &format!("{version}^{{tree}}")]);
    let names = [
        "Inbox",
        "Change-Brief",
        "Every-Statement",
        "What-Changed",
        "Done-Criteria",
        "Repair-Loop",
        "Approve-Sheet",
        "Whole-Story",
        "Record-Over-Time",
        "Settings",
        "Team-View",
        "Onboarding",
        "Command-Ask",
        "Visual-System",
    ];
    let screens: Vec<Value> = names
        .iter()
        .map(|name| {
            json!({
                "name":name,
                "route":format!("/#/design/{name}"),
                "scenario":"design",
                "reference_path":format!("docs/hq-next/reference/{name}.png"),
                "reference_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "width":1512,"height":920,"full_page":false,"mask_rectangles":[]
            })
        })
        .collect();
    let params = json!({"kind":"visual_match","screens":screens,"tolerance_pct":0.5});
    let params = parse_params("visual_match", PARAMS_SCHEMA_VERSION, &params.to_string())
        .expect("closed C1 parameters");
    let ledger = Ledger::open(machine.home().join(".thruline/ledger.db")).expect("ledger");
    let span = "Screens match the agreed design";
    let id = storage::create(
        &ledger,
        NewDecision {
            project_id: "project-1".into(),
            source_anchor: "visual-person-source".into(),
            span: span.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::WorkPrompt,
                event_id: Some("visual-person-source".into()),
                file_anchor: None,
                raw_file: "recording.jsonl".into(),
                raw_line: 7,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
                actor: Some("founder".into()),
                recorded_at: Some("2026-10-07T00:00:00Z".into()),
            }],
            author_actor: "founder".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params,
        },
    )
    .expect("visual proposal");
    let acceptance_id = storage::accept(
        &ledger,
        storage::AcceptanceInput {
            decision_id: id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T00:01:00Z".into(),
            because: "agreed canvas".into(),
            severity: "blocks".into(),
            applies_from_version: version.clone(),
        },
    )
    .expect("person acceptance");
    PersonRecorder::bridge_only(&ledger, "project-1")
        .bind_bridge_acceptance(&ledger, &acceptance_id);
    let visual: Vec<Value> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let rendered = index < 9;
            json!({
                "screen":name,
                "route":format!("/#/design/{name}"),
                "actual_route":rendered.then(||format!("/#/design/{name}")),
                "actual_scenario":rendered.then_some("design"),
                "state":"broken",
                "difference_pct":rendered.then_some(42.25),
                "different_pixels":rendered.then_some(100),
                "checked_pixels":rendered.then_some(240),
                "reference_width":rendered.then_some(1512),
                "reference_height":rendered.then_some(920),
                "capture_width":rendered.then_some(1512),
                "capture_height":rendered.then_some(920),
                "compared_width":rendered.then_some(1512),
                "compared_height":rendered.then_some(920),
                "diff_image_path":rendered.then(||format!("diff/{name}.png")),
                "diff_sha256":rendered.then_some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
                "reference_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "version_id":version,
                "commit_oid":version,
                "tree_hash":tree,
                "renderer_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "reason":(!rendered).then_some("route not found")
            })
        })
        .collect();
    let findings: Vec<Value> = names
        .iter()
        .map(|name| {
            json!({"surface":"ui_copy","path":format!("/#/design/{name}"),"line":0,
                "span":"screen differs or route not found"})
        })
        .collect();
    storage::record_verdict(
        &ledger,
        storage::VerdictInput {
            decision_id: id.clone(),
            revision: 1,
            version_id: version.clone(),
            tree_hash: tree.clone(),
            verdict: "broken".into(),
            reason: Some("14 of 14 screens differ or have no route".into()),
            checkable_if: None,
            findings: json!(findings),
            surfaces: json!([{"surface":"ui_copy","checked_places":14,"finding_count":14,"state":"broken"}]),
            check_impl_version: "visual.fixture".into(),
            tool_versions: json!({"git":"fixture","visual":{"renderer_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}),
            computed_at: "2026-10-07T00:02:00Z".into(),
            basis: json!({"checked_places":14,"visual":visual}),
        },
    )
    .expect("stored visual verdict");
    drop(ledger);

    let server = Server::start(&machine);
    let (status, detail) = server.request(
        "GET",
        &format!("/api/hq/projects/{}/decisions/{id}", machine.hq_id),
        false,
        None,
    );
    assert_eq!(status, 200);
    let verdict = &detail["verdict"];
    assert_eq!(verdict["visual"].as_array().map(Vec::len), Some(14));
    assert_eq!(verdict["finding_count"], 14);
    assert_eq!(verdict["visual"][0]["screen"], "Inbox");
    assert_eq!(verdict["visual"][0]["difference_pct"], 42.25);
    assert_eq!(verdict["visual"][0]["version_id"], version);
    assert_eq!(verdict["visual"][0]["tree_hash"], tree);
    assert_eq!(verdict["visual"][0]["compared_width"], 1512);
    assert_eq!(
        verdict["visual"][0]["diff_sha256"],
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    );
    assert_eq!(verdict["visual"][9]["reason"], "route not found");
    assert!(verdict["visual"][9]["difference_pct"].is_null());
    assert_eq!(verdict["findings"][0]["screen"], "Inbox");
    assert_eq!(verdict["findings"][0]["difference_pct"], 42.25);
    assert_eq!(
        verdict["findings"][0]["reference_hash"],
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
}
