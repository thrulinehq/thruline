use super::{Plan, Result};
use anyhow::Context;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
use thruline_core::evidence_binding::StreamCapture;
use thruline_verify::integrations::RunState;
#[path = "worker_broker_container.rs"]
mod broker_container;
#[path = "worker_fs.rs"]
mod filesystem;
pub struct Stream {
    pub bytes: Vec<u8>,
    pub total: u64,
    pub complete: bool,
}
impl Stream {
    pub fn capture(&self) -> StreamCapture {
        if self.complete {
            StreamCapture::Captured {
                sha256: thruline_core::chain::sha256_hex(&self.bytes),
                bytes: self.total,
            }
        } else {
            StreamCapture::Truncated {
                reason: "capture interrupted or output budget exceeded".into(),
            }
        }
    }
}
pub struct Outcome {
    pub stdout: Stream,
    pub stderr: Stream,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub state: RunState,
    pub duration_ms: u64,
    pub sampled_peak_rss_bytes: Option<u64>,
    pub resource_error: Option<String>,
    pub isolation: Option<serde_json::Value>,
}
fn read(src: impl Read + Send + 'static, limit: u64) -> mpsc::Receiver<Stream> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut s = Stream {
            bytes: vec![],
            total: 0,
            complete: false,
        };
        let mut src = src;
        let mut b = [0u8; 16384];
        loop {
            match src.read(&mut b) {
                Ok(0) => {
                    s.complete = true;
                    break;
                }
                Ok(n) => {
                    s.total += n as u64;
                    if s.total > limit {
                        break;
                    }
                    s.bytes.extend_from_slice(&b[..n]);
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(s);
    });
    rx
}
fn terminate_group(pid: u32) {
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
pub struct Prepared {
    pub argv: Vec<String>,
    lease: Option<super::container::Lease>,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    scratch: Option<tempfile::TempDir>,
    native_isolation: Option<serde_json::Value>,
    broker_container: Option<broker_container::Owner>,
}
pub fn prepare(plan: &Plan) -> Result<Prepared> {
    // AI-REVIEW-OUT-OF-LAUNCH: the execution chokepoint. A plan accepted by
    // a build with AI review never runs in one without it.
    anyhow::ensure!(
        !super::withheld(&plan.pack),
        "{}",
        super::AI_REVIEW_NOT_IN_BUILD
    );
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(unix)]
    {
        signal_hook::flag::register(signal_hook::consts::SIGINT, cancelled.clone())?;
        signal_hook::flag::register(signal_hook::consts::SIGTERM, cancelled.clone())?;
    }
    let mut scratch = None;
    let (lease, argv) = if let Some(spec) = &plan.container {
        let (lease, argv) = super::container::Lease::prepare(plan, spec)?;
        (Some(lease), argv)
    } else {
        let private = tempfile::Builder::new()
            .prefix("thruline-native-")
            .tempdir()?;
        let mut argv = actual_argv(plan)?;
        let split = argv
            .iter()
            .position(|s| s == "--")
            .context("worker delimiter missing")?;
        argv.splice(
            split..split,
            [
                "--write-path".into(),
                private.path().canonicalize()?.display().to_string(),
            ],
        );
        scratch = Some(private);
        (None, argv)
    };
    let native_isolation = if lease.is_none() {
        let paths = |flag: &str| {
            argv.windows(2)
                .filter(|p| p[0] == flag)
                .map(|p| p[1].clone())
                .collect::<Vec<_>>()
        };
        Some(serde_json::json!({
            "backend":"macos-seatbelt-native-v1",
            "read_paths":paths("--read-path"),
            "write_paths":paths("--write-path"),
            "filesystem":"content denied by default; explicit selected inputs, prepared runtime and system libraries readable; only managed outputs and private scratch writable",
            "metadata":"filesystem metadata and literal root directory readable; file content scope remains separate",
            "network":if plan.local_model_port.is_some() { "registered localhost model endpoint only" } else { "denied" },
            "scratch":"ephemeral; retained reports belong in managed output paths",
            "cache_home":if plan.pack == "betterleaks" { "Fresh private scratch; embedded WASM compilation never reads the user's cache" } else { "No worker cache-home override" },
            "limits":"Seatbelt filesystem/network boundary; not a VM or proof about every macOS IPC service"
        }))
    } else {
        None
    };
    Ok(Prepared {
        argv,
        lease,
        cancelled,
        scratch,
        native_isolation,
        broker_container: None,
    })
}

/// Fixed product source-snapshot child. Only this internal plan may add the
/// ordinary Git object store to write scope; the source/index/refs stay read-only.
pub(super) fn prepare_semgrep_snapshot(plan: &Plan) -> Result<Prepared> {
    let exe = std::env::current_exe()?.canonicalize()?;
    anyhow::ensure!(
        plan.pack == "semgrep-snapshot"
            && plan.container.is_none()
            && plan.local_model_port.is_none()
            && plan.writable_paths.is_empty()
            && plan.argv
                == vec![
                    exe.display().to_string(),
                    "--db".into(),
                    "/dev/null".into(),
                    "integration".into(),
                    "semgrep-snapshot".into(),
                    "--root".into(),
                    plan.root.display().to_string()
                ],
        "Only the fixed current-candidate Semgrep snapshot child may prepare Git objects"
    );
    let objects = plan.root.join(".git/objects");
    anyhow::ensure!(
        objects.canonicalize()? == objects && objects.is_dir(),
        "Git objects must be an ordinary bound directory"
    );
    let mut prepared = prepare(plan)?;
    let split = prepared
        .argv
        .iter()
        .position(|s| s == "--")
        .context("snapshot worker delimiter missing")?;
    prepared.argv.splice(
        split..split,
        ["--write-path".into(), objects.display().to_string()],
    );
    if let Some(ref mut isolation) = prepared.native_isolation {
        isolation["write_paths"]
            .as_array_mut()
            .context("worker write scope missing")?
            .push(serde_json::json!(objects));
        isolation["snapshot_exception"]=serde_json::json!("Current candidate's bounded Git source snapshot may create ordinary Git objects. Source files, real index, refs, configuration and unrelated host files remain read-only; network denied.");
    }
    Ok(prepared)
}

/// Internal trusted-broker entry: macOS cannot nest sandbox_apply. Callers must
/// bind/validate the embedded or registered broker and sandbox every workload
/// child. This changes no public plan/CLI permission and retains group limits.
pub fn prepare_broker(plan: &Plan) -> Result<Prepared> {
    // AI-REVIEW-OUT-OF-LAUNCH: the execution chokepoint. A plan accepted by
    // a build with AI review never runs in one without it.
    anyhow::ensure!(
        !super::withheld(&plan.pack),
        "{}",
        super::AI_REVIEW_NOT_IN_BUILD
    );
    anyhow::ensure!(
        [
            "ruff-repair",
            "native-repair",
            "agent-repair",
            "aider",
            "openhands-sdk",
            "pr-agent",
            "mini-swe-agent",
            "sonarqube",
            "megalinter"
        ]
        .contains(&plan.pack.as_str())
            && plan.container.is_none(),
        "this pack has no trusted broker execution path"
    );
    let mut prepared = prepare(plan)?;
    // Fixed shell program only; every executable/argument remains a distinct
    // argv element. No target content is interpolated into shell source. exec
    // preserves the broker PID/PGID, and the hard CPU limit is inherited.
    prepared.argv = vec![
        "/bin/sh".into(),
        "-c".into(),
        "ulimit -t \"$1\" || exit 125; shift; exec \"$@\"".into(),
        "thruline-trusted-broker".into(),
        plan.cpu_seconds.to_string(),
    ];
    prepared.argv.extend(plan.argv.clone());
    prepared.scratch = None;
    let container_runtime = plan
        .argv
        .windows(2)
        .find(|pair| pair[0] == "--container-runtime")
        .map(|pair| pair[1].as_str())
        .or_else(|| {
            plan.environment
                .get("THRULINE_CONTAINER_RUNTIME")
                .map(String::as_str)
        });
    if let Some(path) = container_runtime {
        anyhow::ensure!(
            [
                "aider",
                "openhands-sdk",
                "pr-agent",
                "mini-swe-agent",
                "agent-repair",
                "megalinter"
            ]
            .contains(&plan.pack.as_str()),
            "Linux namespace ownership requires an explicit registered native recipe"
        );
        prepared.broker_container = Some(broker_container::Owner::prepare(std::path::Path::new(
            path,
        ))?);
    }
    prepared.native_isolation = Some(if plan.pack == "sonarqube" {
        serde_json::json!({"backend":"trusted-local-docker-service-broker", "note":"Registered broker uses local Docker API and verifies fixed container/image/internal-network identity. This is not native filesystem confinement. Broker termination alone does not prove asynchronous server-task termination."})
    } else if plan.pack == "megalinter" {
        serde_json::json!({"backend":"trusted-megalinter-child-isolation", "note":"Registered broker owns analyzer/oracle child sandboxes; exact native profiles are retained in its artifacts. Optional explicitly selected localhost Advisor traffic is separate from deterministic analyzer observations."})
    } else {
        serde_json::json!({"backend":"trusted-broker-child-isolation", "note":"The registered broker owns workload child sandboxes; native outer Seatbelt is not nested."})
    });
    Ok(prepared)
}
pub fn execute(
    plan: &Plan,
    prepared: &mut Prepared,
    started: impl FnOnce(u32) -> Result<()>,
) -> Result<Outcome> {
    let cancelled = &prepared.cancelled;
    let start = Instant::now();
    let actual = &prepared.argv;
    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(Outcome {
            stdout: Stream {
                bytes: vec![],
                total: 0,
                complete: true,
            },
            stderr: Stream {
                bytes: vec![],
                total: 0,
                complete: true,
            },
            exit_code: None,
            signal: None,
            state: RunState::Cancelled,
            duration_ms: 0,
            sampled_peak_rss_bytes: None,
            resource_error: None,
            isolation: prepared
                .lease
                .as_mut()
                .map(|l| l.finish())
                .transpose()?
                .or_else(|| prepared.native_isolation.clone()),
        });
    }
    let mut cmd = Command::new(&actual[0]);
    cmd.args(&actual[1..])
        .current_dir(&plan.root)
        .env_clear()
        .envs(&plan.environment)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        // -B disables writes but still loads cached bytecode. This non-directory
        // prefix forces source imports, including inherited Python subprocesses.
        .env("PYTHONPYCACHEPREFIX", "/dev/null")
        .env("PYTHONNOUSERSITE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(owner) = &prepared.broker_container {
        owner.environment(&mut cmd, plan);
    }
    if let Some(scratch) = &prepared.scratch {
        cmd.env("TMPDIR", scratch.path())
            .env("TMP", scratch.path())
            .env("TEMP", scratch.path());
        if plan.pack == "betterleaks" {
            // The embedded RE2/Wazero runtime uses os.UserCacheDir. Give it a
            // fresh derived cache inside this worker's declared writable scope.
            cmd.env("HOME", scratch.path())
                .env("XDG_CACHE_HOME", scratch.path());
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            return Ok(Outcome {
                stdout: Stream {
                    bytes: vec![],
                    total: 0,
                    complete: true,
                },
                stderr: Stream {
                    bytes: vec![],
                    total: 0,
                    complete: true,
                },
                exit_code: None,
                signal: None,
                state: RunState::FailedToStart,
                duration_ms: start.elapsed().as_millis() as u64,
                sampled_peak_rss_bytes: None,
                resource_error: Some(e.to_string()),
                isolation: prepared
                    .lease
                    .as_mut()
                    .map(|l| l.finish())
                    .transpose()?
                    .or_else(|| prepared.native_isolation.clone()),
            })
        }
    };
    let pid = child.id();
    let mut owned = OwnedProcesses::new(pid);
    if let Err(e) = started(pid) {
        owned.signal_all();
        terminate_group(pid);
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    let out = read(
        child.stdout.take().context("stdout pipe unavailable")?,
        plan.output_limit_bytes,
    );
    let err = read(
        child.stderr.take().context("stderr pipe unavailable")?,
        plan.output_limit_bytes,
    );
    let (mut stdout, mut stderr) = (None, None);
    let mut state = RunState::Completed;
    let mut peak = None;
    let mut resource_error = None;
    let mut sample_at = Instant::now();
    let status = loop {
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            state = RunState::Cancelled;
            // This named trusted broker owns separately sandboxed native child
            // groups and must freeze partial reports before its process exits.
            // Untrusted workloads and resource violations retain hard kills.
            if plan.pack == "megalinter" && prepared.native_isolation.is_some() {
                let _ = Command::new("/bin/kill")
                    .args(["-TERM", "--", &format!("-{pid}")])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let deadline = Instant::now() + Duration::from_millis(900);
                while Instant::now() < deadline {
                    if child.try_wait()?.is_some() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            owned.signal_all();
            terminate_group(pid);
            let _ = child.kill();
            break child.wait()?;
        }
        if Instant::now() >= sample_at {
            sample_at = Instant::now() + Duration::from_millis(200);
            match owned.sample() {
                Ok(Some(rss)) => {
                    peak = Some(peak.unwrap_or(0u64).max(rss));
                    if rss > plan.memory_mb * 1024 * 1024 {
                        state = RunState::ResourceLimit;
                        owned.signal_all();
                        terminate_group(pid);
                        let _ = child.kill();
                        break child.wait()?;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    resource_error = Some(e.to_string());
                    state = RunState::CaptureFailed;
                    owned.signal_all();
                    terminate_group(pid);
                    let _ = child.kill();
                    break child.wait()?;
                }
            }
        }
        if stdout.is_none() {
            stdout = out.try_recv().ok();
        }
        if stderr.is_none() {
            stderr = err.try_recv().ok();
        }
        if stdout.as_ref().is_some_and(|s| !s.complete)
            || stderr.as_ref().is_some_and(|s| !s.complete)
        {
            state = RunState::OutputLimit;
            owned.signal_all();
            terminate_group(pid);
            let _ = child.kill();
            break child.wait()?;
        }
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() >= Duration::from_secs(plan.timeout_seconds) {
            state = RunState::TimedOut;
            owned.signal_all();
            terminate_group(pid);
            let _ = child.kill();
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // SDK terminals may call setsid. Preserve PID/start-time ownership across
    // those groups and reparenting instead of counting/killing only our PGID.
    let termination = owned.finish(&mut child);
    if termination["observed_processes_terminated"] != true {
        resource_error = Some("Worker termination could not be established for every observed owned process; inspect the process-tree evidence".into());
        state = RunState::CaptureFailed;
    }
    let missing = || Stream {
        bytes: vec![],
        total: 0,
        complete: false,
    };
    let stdout = stdout.unwrap_or_else(|| {
        out.recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|_| missing())
    });
    let stderr = stderr.unwrap_or_else(|| {
        err.recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|_| missing())
    });
    if state == RunState::Completed && (!stdout.complete || !stderr.complete) {
        state = RunState::CaptureFailed;
    }
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    };
    #[cfg(not(unix))]
    let signal = None;
    if signal.is_some() && state == RunState::Completed {
        state = RunState::CaptureFailed;
    }
    let mut isolation = match prepared.lease.as_mut().map(|l| l.finish()).transpose() {
        Ok(value) => value.or_else(|| prepared.native_isolation.clone()),
        Err(e) => {
            state = RunState::CaptureFailed;
            resource_error = Some(format!("Container cleanup or final state failed: {e}"));
            Some(
                serde_json::json!({"cleanup":"failed; inspect the owned container", "error":e.to_string()}),
            )
        }
    };
    let details = isolation.get_or_insert_with(|| serde_json::json!({}));
    details["process_tree"] = termination;
    if let Some(owner) = &mut prepared.broker_container {
        match owner.finish() {
            Ok(namespace) => {
                if namespace["namespaces"].as_array().is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row.pointer("/state/OOMKilled")
                            .and_then(serde_json::Value::as_bool)
                            == Some(true)
                    })
                }) {
                    state = RunState::ResourceLimit;
                    resource_error =
                        Some("Native agent namespace exceeded its container memory limit".into());
                }
                details["native_namespaces"] = namespace;
            }
            Err(error) => {
                state = RunState::CaptureFailed;
                resource_error = Some(format!(
                    "Native agent namespace termination unestablished: {error}"
                ));
                details["native_namespaces"] =
                    serde_json::json!({"termination_established":false,"error":error.to_string()});
            }
        }
    }
    if isolation
        .as_ref()
        .and_then(|v| v.pointer("/state/OOMKilled"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        state = RunState::ResourceLimit;
        resource_error = Some("The verifier exceeded its container memory limit.".into());
    }
    if state == RunState::Completed && prepared.lease.is_some() {
        if let Some(value) = &isolation {
            let final_state = value
                .pointer("/state/Status")
                .and_then(serde_json::Value::as_str);
            let final_exit = value
                .pointer("/state/ExitCode")
                .and_then(serde_json::Value::as_i64);
            if final_state != Some("exited")
                || final_exit != status.code().map(i64::from)
                || final_exit.is_some_and(|n| n >= 128)
            {
                state = RunState::CaptureFailed;
                resource_error = Some("The container did not record an ordinary, matching exit. Inspect the isolated execution state; no passing result is admitted.".into());
            }
        }
    }
    Ok(Outcome {
        stdout,
        stderr,
        exit_code: status.code(),
        signal,
        state,
        duration_ms: start.elapsed().as_millis().min(u64::MAX as u128) as u64,
        sampled_peak_rss_bytes: peak,
        isolation,
        resource_error,
    })
}

/// Applied only in a disposable child, before replacing it with the user's argv.
/// These are per-process CPU and virtual-address limits, not an RSS claim.
pub fn exec_limited(
    cpu: u64,
    memory: u64,
    local_model_port: Option<u16>,
    argv: &[String],
    read_paths: &[std::path::PathBuf],
    write_paths: &[std::path::PathBuf],
) -> Result<std::process::ExitCode> {
    anyhow::ensure!(
        !argv.is_empty() && (1..=3600).contains(&cpu) && (128..=16384).contains(&memory),
        "invalid worker limits"
    );
    #[cfg(unix)]
    {
        use rustix::process::{setrlimit, Resource, Rlimit};
        #[cfg(target_os = "macos")]
        use std::os::unix::process::CommandExt;
        // Only the macOS sandbox profile reads these three.
        #[cfg(not(target_os = "macos"))]
        let _ = (local_model_port, read_paths, write_paths);
        setrlimit(
            Resource::Cpu,
            Rlimit {
                current: Some(cpu),
                maximum: Some(cpu),
            },
        )
        .context("CPU limit could not be applied")?;
        #[cfg(target_os = "linux")]
        {
            let bytes = memory * 1024 * 1024;
            setrlimit(
                Resource::As,
                Rlimit {
                    current: Some(bytes),
                    maximum: Some(bytes),
                },
            )
            .context("virtual memory limit could not be applied")?;
        }
        #[cfg(target_os = "macos")]
        let profile = filesystem::profile(read_paths, write_paths, local_model_port)?;
        #[cfg(target_os = "macos")]
        let error = Command::new("/usr/bin/sandbox-exec")
            .args(["-p", &profile, "--"])
            .args(argv)
            .exec();
        #[cfg(not(target_os = "macos"))]
        let error = std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "a network-denying worker sandbox is not configured on this platform",
        );
        Err(error).context("limited worker could not start the tool")
    }
    #[cfg(not(unix))]
    {
        anyhow::bail!("resource-limited worker unavailable on this platform");
    }
}

#[derive(Clone, serde::Serialize)]
struct ProcessIdentity {
    pid: u32,
    ppid: u32,
    pgid: u32,
    rss_bytes: u64,
    start_time: String,
}
struct OwnedProcesses {
    leader: u32,
    known: BTreeMap<u32, ProcessIdentity>,
    live: BTreeMap<u32, ProcessIdentity>,
    errors: Vec<String>,
    samples: u64,
}
fn process_snapshot() -> Result<BTreeMap<u32, ProcessIdentity>> {
    let output = Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,pgid=,rss=,lstart="])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "cannot observe worker descendants/memory; process inspection unavailable"
    );
    let text = std::str::from_utf8(&output.stdout)?;
    let mut rows = BTreeMap::new();
    for row in text.lines() {
        let mut p = row.split_whitespace();
        let pid = p.next().context("missing process PID")?.parse::<u32>()?;
        let ppid = p.next().context("missing process PPID")?.parse::<u32>()?;
        let pgid = p.next().context("missing process PGID")?.parse::<u32>()?;
        let rss_bytes = p
            .next()
            .context("missing process RSS")?
            .parse::<u64>()?
            .saturating_mul(1024);
        let start_time = p.collect::<Vec<_>>().join(" ");
        anyhow::ensure!(!start_time.is_empty(), "process start identity missing");
        rows.insert(
            pid,
            ProcessIdentity {
                pid,
                ppid,
                pgid,
                rss_bytes,
                start_time,
            },
        );
    }
    Ok(rows)
}
impl OwnedProcesses {
    fn new(leader: u32) -> Self {
        Self {
            leader,
            known: BTreeMap::new(),
            live: BTreeMap::new(),
            errors: vec![],
            samples: 0,
        }
    }
    fn sample(&mut self) -> Result<Option<u64>> {
        let rows = process_snapshot()?;
        let mut owned: BTreeSet<u32> = rows
            .values()
            .filter(|row| {
                self.known
                    .get(&row.pid)
                    .is_some_and(|old| old.start_time == row.start_time)
                    || (row.pgid == self.leader
                        && self
                            .known
                            .get(&row.pid)
                            .is_none_or(|old| old.start_time == row.start_time))
            })
            .map(|row| row.pid)
            .collect();
        loop {
            let next: Vec<u32> = rows
                .values()
                .filter(|row| {
                    owned.contains(&row.ppid)
                        && !owned.contains(&row.pid)
                        && self
                            .known
                            .get(&row.pid)
                            .is_none_or(|old| old.start_time == row.start_time)
                })
                .map(|row| row.pid)
                .collect();
            if next.is_empty() {
                break;
            }
            owned.extend(next);
        }
        self.live = owned
            .into_iter()
            .filter_map(|pid| rows.get(&pid).cloned().map(|row| (pid, row)))
            .collect();
        self.known.extend(self.live.clone());
        self.samples += 1;
        Ok((!self.live.is_empty()).then(|| self.live.values().map(|row| row.rss_bytes).sum()))
    }
    fn signal_all(&mut self) {
        if let Err(error) = self.sample() {
            self.errors.push(error.to_string());
        }
        // Re-sample immediately before signalling and do not signal a PID whose
        // start timestamp changed. Children are signalled before the leader.
        let mut pids: Vec<u32> = self
            .live
            .keys()
            .copied()
            .filter(|pid| *pid != self.leader)
            .collect();
        if self.live.contains_key(&self.leader) {
            pids.push(self.leader);
        }
        for pid in pids {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", "--", &pid.to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    fn finish(&mut self, child: &mut std::process::Child) -> serde_json::Value {
        let start = Instant::now();
        loop {
            let _ = child.try_wait();
            self.signal_all();
            terminate_group(self.leader);
            let _ = child.try_wait();
            if let Err(error) = self.sample() {
                self.errors.push(error.to_string());
                break;
            }
            if self.live.is_empty() || start.elapsed() >= Duration::from_secs(6) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        serde_json::json!({"observed_processes_terminated":self.live.is_empty() && self.errors.is_empty(),
            "remaining_observed_pids":self.live.keys().collect::<Vec<_>>(), "observed_processes":self.known.values().collect::<Vec<_>>(),
            "samples":self.samples,"cleanup_duration_ms":start.elapsed().as_millis(),"errors":self.errors,
            "ownership":"PID, process start timestamp, original PGID and sampled PPID descendants, including observed new sessions",
            "limitation":"Sampled ownership is not PID-namespace containment. A fast double-fork/reparent between samples may evade discovery; this record establishes termination only for observed owned processes."})
    }
}

pub fn actual_argv(plan: &Plan) -> Result<Vec<String>> {
    let mut argv = vec![
        std::env::current_exe()?.to_string_lossy().into_owned(),
        "--db".into(),
        "/dev/null".into(),
        "integration".into(),
        "worker-exec".into(),
        "--cpu-seconds".into(),
        plan.cpu_seconds.to_string(),
        "--memory-mb".into(),
        plan.memory_mb.to_string(),
    ];
    if let Some(port) = plan.local_model_port {
        argv.extend(["--local-model-port".into(), port.to_string()]);
    }
    let mut read_paths = vec![plan.root.canonicalize()?];
    read_paths.extend(
        plan.runtime
            .iter()
            .chain(&plan.rules)
            .chain(&plan.dependencies)
            .chain(&plan.dataset)
            .cloned(),
    );
    if let Some(executable) = plan.argv.first() {
        let path = std::path::Path::new(executable);
        if path.is_absolute() && path.exists() {
            read_paths.push(path.canonicalize()?);
        }
    }
    read_paths.sort();
    read_paths.dedup();
    for path in &read_paths {
        argv.extend(["--read-path".into(), path.display().to_string()]);
    }
    for path in &plan.writable_paths {
        let path = filesystem::writable(path, &read_paths)?;
        argv.extend(["--write-path".into(), path.display().to_string()]);
    }
    argv.push("--".into());
    argv.extend(plan.argv.clone());
    Ok(argv)
}
