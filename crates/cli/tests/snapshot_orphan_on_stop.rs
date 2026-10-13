//! SNAPSHOT-ORPHAN-ON-STOP: real HQ shutdown while a snapshot child waits.
//! Synthetic owned HOME/repository only. Both git and ps leaders spawn a
//! never-ending descendant. SIGINT/SIGTERM must stop HQ and the entire group
//! before the existing 20-second snapshot deadline could rescue the test.
//! Cleanup runs only after the absence assertion, including on baseline panic.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

const CLIENT_WAITS: Duration = Duration::from_secs(50);

struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    fn new() -> Machine {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo/src")).expect("repo");
        std::fs::create_dir_all(dir.path().join("repo/.git")).expect("git");
        std::fs::create_dir_all(dir.path().join("bin")).expect("bin");
        Machine { dir }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }

    fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    fn bin(&self) -> PathBuf {
        self.dir.path().join("bin")
    }

    fn pids(&self, program: &str) -> PathBuf {
        self.dir.path().join(format!("{program}.pids"))
    }

    /// The leader and its descendant never exit by themselves. The marker is
    /// complete only after both exist; cleanup must signal their whole group.
    fn stand_in(&self, program: &str, real: &Path, stall_on: &str) {
        let script = format!(
            "#!/bin/sh\ncase \" $* \" in\n  *\" {stall_on} \"*)\n    echo $$ > '{pids}'\n    /bin/sh -c 'while :; do /bin/sleep 3600; done' &\n    echo $! >> '{pids}'\n    wait\n    ;;\nesac\nexec '{real}' \"$@\"\n",
            pids = self.pids(program).display(),
            real = real.display(),
        );
        let path = self.bin().join(program);
        std::fs::write(&path, script).expect("stand-in");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    fn record(&self, session: &str, asked_for: &str, touched: &str, day: &str) {
        let cwd = self.repo().display().to_string();
        let mangled: String = cwd
            .chars()
            .map(|c| match c {
                '/' | '.' | '_' | ' ' => '-',
                other => other,
            })
            .collect();
        let dir = self.home().join(".claude/projects").join(mangled);
        std::fs::create_dir_all(&dir).expect("project dir");
        let lines = [
            format!(
                r#"{{"type":"user","uuid":"{session}-e1","parentUuid":null,"isSidechain":false,"sessionId":"{session}","timestamp":"{day}T09:00:00.100Z","cwd":"{cwd}","message":{{"role":"user","content":"{asked_for}"}}}}"#
            ),
            format!(
                r#"{{"type":"assistant","uuid":"{session}-e2","parentUuid":"{session}-e1","isSidechain":false,"sessionId":"{session}","timestamp":"{day}T09:01:00.100Z","cwd":"{cwd}","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"Edit","input":{{"file_path":"{cwd}/{touched}","old_string":"a","new_string":"b"}}}}]}}}}"#
            ),
        ];
        std::fs::write(
            dir.join(format!("{session}.jsonl")),
            lines.join("\n") + "\n",
        )
        .expect("recording");
    }

    fn ui(&self) -> Command {
        let path = format!(
            "{}:{}",
            self.bin().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Command::new(thruline());
        cmd.args(["ui", "--no-open", "--port", "0"])
            .env("HOME", self.home())
            .env("THRULINE_HOME", self.home().join(".thruline"))
            .env("PATH", path)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.repo());
        cmd
    }
}

/// The real program, found on this process's own `PATH`.
fn real(program: &str) -> PathBuf {
    let path = std::env::var_os("PATH").expect("PATH");
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("{program} is not on PATH"))
}

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(mut cmd: Command) -> Server {
        // Owned by a `Server` from the moment it exists, so a panic below
        // still kills and reaps it (`Drop`).
        let mut server = Server {
            child: cmd
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("thruline ui"),
            port: 0,
        };
        let stdout = server.child.stdout.take().expect("stdout");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = rx
                .recv_timeout(left)
                .expect("`thruline ui` printed an address");
            if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:") {
                server.port = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .expect("port");
                return server;
            }
        }
    }

    fn request(&self, head: &str, body: &str) -> (u16, serde_json::Value) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        socket
            .set_read_timeout(Some(CLIENT_WAITS))
            .expect("timeout");
        socket.write_all(head.as_bytes()).expect("head");
        socket.write_all(body.as_bytes()).expect("body");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("header terminator");
        let status = String::from_utf8_lossy(&raw[..split])
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.parse().ok())
            .expect("status");
        let json = serde_json::from_slice(&raw[split + 4..]).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    fn get(&self, path: &str) -> serde_json::Value {
        let head = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        );
        self.request(&head, "").1
    }

    fn post(&self, path: &str, body: &str) -> (u16, serde_json::Value) {
        let head = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n",
            p = self.port,
            n = body.len()
        );
        self.request(&head, body)
    }
}

use rustix::process::{
    kill_process, kill_process_group, test_kill_process, test_kill_process_group, Pid, Signal,
};

const STOP_BOUND: Duration = Duration::from_secs(10);

fn pid(raw: u32) -> Pid {
    Pid::from_raw(i32::try_from(raw).expect("pid fits")).expect("nonzero pid")
}

fn process_alive(raw: u32) -> bool {
    test_kill_process(pid(raw)) != Err(rustix::io::Errno::SRCH)
}

fn group_alive(raw: u32) -> bool {
    test_kill_process_group(pid(raw)) != Err(rustix::io::Errno::SRCH)
}

struct OwnedGroup {
    marker: PathBuf,
    armed: bool,
}

impl Drop for OwnedGroup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let raw = std::fs::read_to_string(&self.marker).ok().and_then(|text| {
            text.lines()
                .next()
                .and_then(|line| line.parse::<u32>().ok())
        });
        if let Some(raw) = raw {
            let _ = kill_process_group(pid(raw), Signal::KILL);
            let until = Instant::now() + Duration::from_secs(5);
            while group_alive(raw) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
            eprintln!(
                "fixture cleanup of owned group {raw}: absent={}",
                !group_alive(raw)
            );
        }
    }
}

/// Send the real write but keep its connection open while it blocks in capture.
fn begin_change(machine: &Machine, server: &Server) -> TcpStream {
    let doc = server.get("/api/first-use");
    let here = machine.repo().display().to_string();
    let key = doc["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .find(|c| c["label"] == here)
        .and_then(|c| c["key"].as_str())
        .unwrap_or_else(|| panic!("no candidate for {here}: {doc}"));
    let (status, picked) = server.post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#));
    assert_eq!(status, 200, "{picked}");
    let project = picked["project"].as_str().expect("project");
    let body = format!(r#"{{"project":"{project}"}}"#);
    let mut socket = TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
    socket
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("write timeout");
    write!(socket, "POST /api/first-use/change HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}", p=server.port, n=body.len()).expect("change request");
    socket
}

fn stop_during_snapshot(program: &str, argument: &str, signal: Signal) {
    let machine = Machine::new();
    machine.record("cccccccc-0000-4000-8000-00000000t001", "Fix the retry loop. When an upload fails three times, the record shows a single failure row.", "src/uploader.rs", "2026-01-09");
    machine.stand_in(program, &real(program), argument);
    let mut group = OwnedGroup {
        marker: machine.pids(program),
        armed: true,
    };
    let mut server = Server::start(machine.ui());
    let _request = begin_change(&machine, &server);
    let waiting_since = Instant::now();
    let pids = loop {
        let pids: Vec<u32> = std::fs::read_to_string(&group.marker)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.parse().ok())
            .collect();
        if pids.len() == 2 {
            break pids;
        }
        assert!(
            waiting_since.elapsed() < Duration::from_secs(10),
            "{program} never entered its wait"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    for &raw in &pids {
        assert!(
            process_alive(raw),
            "fixture process {raw} was not alive before stop"
        );
        let actual = Command::new(real("ps"))
            .args(["-p", &raw.to_string(), "-o", "pgid="])
            .output()
            .expect("read group");
        assert_eq!(
            String::from_utf8_lossy(&actual.stdout).trim(),
            pids[0].to_string(),
            "fixture is not one owned group"
        );
    }
    let stopped = Instant::now();
    kill_process(pid(server.child.id()), signal).expect("stop HQ");
    loop {
        let exited = server.child.try_wait().expect("HQ status").is_some();
        let absent = !group_alive(pids[0]) && pids.iter().all(|&p| !process_alive(p));
        if exited && absent {
            break;
        }
        assert!(stopped.elapsed() < STOP_BOUND, "HQ stop left {program} group {} behind: HQ exited={exited}, group alive={}, pids={pids:?}", pids[0], group_alive(pids[0]));
        std::thread::sleep(Duration::from_millis(20));
    }
    group.armed = false;
    assert!(
        stopped.elapsed() < thruline_core::bounded_child::TIME_LIMIT,
        "ordinary deadline masked shutdown cleanup"
    );
    // If the interrupted request managed to store anything before HQ exited,
    // it must not turn an interrupted tree/ps read into a fabricated snapshot.
    let ledger = machine.home().join(".thruline/ledger.db");
    let db =
        rusqlite::Connection::open_with_flags(ledger, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("ledger");
    let snapshots: i64 = db
        .query_row("SELECT COUNT(*) FROM workspace_snapshots", [], |r| r.get(0))
        .expect("snapshot count");
    assert_eq!(
        snapshots, 0,
        "an interrupted capture became a stored snapshot"
    );
}

#[test]
fn sigint_stops_the_git_snapshot_group() {
    stop_during_snapshot("git", "--is-inside-work-tree", Signal::INT);
}
#[test]
fn sigterm_stops_the_git_snapshot_group() {
    stop_during_snapshot("git", "--is-inside-work-tree", Signal::TERM);
}
#[test]
fn sigint_stops_the_ps_snapshot_group() {
    stop_during_snapshot("ps", "-axo", Signal::INT);
}
#[test]
fn sigterm_stops_the_ps_snapshot_group() {
    stop_during_snapshot("ps", "-axo", Signal::TERM);
}

/// Run cancellation probes in fresh test processes: closing the production
/// registry is one-way and must not poison unrelated concurrent tests.
fn probe_interruption(program: &str, argument: &str, test_name: &str) {
    const CHILD: &str = "THRULINE_SNAPSHOT_STOP_PROBE";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let marker = root.parent().expect("fixture parent").join("probe.pids");
        let capture = std::thread::spawn(move || {
            thruline_core::snapshot::capture(&root, "2026-09-27T00:00:00Z")
        });
        let until = Instant::now() + Duration::from_secs(10);
        while std::fs::read_to_string(&marker)
            .unwrap_or_default()
            .lines()
            .count()
            != 2
        {
            assert!(
                Instant::now() < until,
                "capture did not reach its waiting child"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut guard = thruline_core::bounded_child::shutdown_guard();
        assert!(guard.stop(), "shutdown did not drain its owned child");
        let error = capture
            .join()
            .expect("capture thread")
            .expect_err("interrupted capture returned a fabricated snapshot");
        assert!(
            matches!(&error, thruline_core::error::CoreError::Io { source, .. } if source.kind() == std::io::ErrorKind::Interrupted),
            "{error}"
        );
        assert!(
            !thruline_core::bounded_child::is_timeout(&error),
            "shutdown was mislabeled as the 20s deadline: {error}"
        );
        let later = marker.with_extension("late");
        let error = thruline_core::bounded_child::output_until(
            Command::new("/usr/bin/touch").arg(&later),
            Instant::now() + Duration::from_secs(5),
        )
        .expect_err("shutdown reopened spawning");
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        assert!(!later.exists(), "a late request spawned after shutdown");
        return;
    }
    {
        let machine = Machine::new();
        machine.stand_in(program, &real(program), argument);
        // The helper knows one marker name regardless of which tool is stalled.
        let path = machine.bin().join(program);
        let script = std::fs::read_to_string(&path)
            .expect("stand-in script")
            .replace(
                &machine.pids(program).display().to_string(),
                &machine.dir.path().join("probe.pids").display().to_string(),
            );
        std::fs::write(path, script).expect("probe marker");
        let mut group = OwnedGroup {
            marker: machine.dir.path().join("probe.pids"),
            armed: true,
        };
        let mut child = Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", test_name, "--nocapture"])
            .env(CHILD, machine.repo())
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    machine.bin().display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .spawn()
            .expect("isolated cancellation probe");
        let until = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.try_wait().expect("probe status") {
                break status;
            }
            if Instant::now() >= until {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{program} interruption probe did not finish");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(
            status.success(),
            "{program} interruption probe failed: {status}"
        );
        let raw: u32 = std::fs::read_to_string(&group.marker)
            .expect("marker")
            .lines()
            .next()
            .expect("leader")
            .parse()
            .expect("pid");
        assert!(!group_alive(raw), "interruption left group {raw}");
        group.armed = false;
    }
}

#[test]
fn interrupted_git_capture_is_unknown() {
    probe_interruption(
        "git",
        "--is-inside-work-tree",
        "interrupted_git_capture_is_unknown",
    );
}

#[test]
fn interrupted_ps_capture_is_unknown() {
    probe_interruption("ps", "-axo", "interrupted_ps_capture_is_unknown");
}
