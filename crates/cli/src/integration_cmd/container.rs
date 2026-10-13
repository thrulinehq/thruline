//! Optional local Docker broker. Only a pinned, already provisioned image runs.
//! Workloads get no network, writable host mount, Docker socket, extra capability,
//! or host PID namespace. The CLI is a trusted broker, not part of the workload.
use super::*;
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub docker: PathBuf,
    pub socket: PathBuf,
    pub image: String,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
pub fn validate(spec: &Spec) -> Result<()> {
    anyhow::ensure!(
        spec.docker.is_absolute() && spec.socket.is_absolute(),
        "Docker and its local socket must be absolute paths"
    );
    anyhow::ensure!(
        spec.image.strip_prefix("sha256:").is_some_and(digest),
        "An immutable local image SHA-256 is required; image tags and pulls are not accepted"
    );
    for (k, v) in &spec.environment {
        anyhow::ensure!(
            !k.is_empty() && !k.contains(['=', '\0']) && !v.contains('\0'),
            "invalid container environment"
        );
        anyhow::ensure!(
            !["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
                .iter()
                .any(|s| k.to_ascii_uppercase().contains(s)),
            "credentials cannot be persisted in a container plan"
        );
    }
    Ok(())
}
fn command(spec: &Spec) -> Command {
    let mut c = Command::new(&spec.docker);
    c.args(["--host", &format!("unix://{}", spec.socket.display())]);
    c.env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/local/bin")
        .stdin(Stdio::null());
    c
}
/// Bounded metadata/cleanup calls; never `docker prune` or an unscoped stop.
fn bounded(spec: &Spec, args: &[&str]) -> Result<Vec<u8>> {
    let mut child = command(spec)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child.stdout.take().context("container metadata pipe")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut b = Vec::new();
        let result = stdout.take(65537).read_to_end(&mut b).map(|_| b);
        let _ = tx.send(result);
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("local container metadata/cleanup timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    anyhow::ensure!(status.success(), "local container metadata/cleanup failed");
    let bytes = rx.recv_timeout(Duration::from_secs(1))??;
    anyhow::ensure!(bytes.len() <= 65536, "container metadata exceeded budget");
    Ok(bytes)
}
pub fn identity(spec: &Spec) -> Result<String> {
    validate(spec)?;
    let image = bounded(
        spec,
        &["image", "inspect", &spec.image, "--format", "{{.Id}}"],
    )
    .context(
        "Provision the pinned local verifier image and start Docker before running this check",
    )?;
    anyhow::ensure!(
        std::str::from_utf8(&image)?.trim() == spec.image,
        "local image identity differs from the accepted plan"
    );
    let version = bounded(spec, &["version", "--format", "{{json .Server}}"])?;
    // Server build identity, not dynamic resource/account state.
    let v: Value = engine::parse_json(&version)?;
    hash_json(&(
        hash_file(&spec.docker)?,
        &spec.image,
        v.get("Version"),
        v.get("GitCommit"),
        v.get("Os"),
        v.get("Arch"),
    ))
}
pub struct Lease {
    spec: Spec,
    dir: tempfile::TempDir,
    closed: bool,
}
impl Lease {
    pub fn prepare(plan: &Plan, spec: &Spec) -> Result<(Self, Vec<String>)> {
        validate(spec)?;
        anyhow::ensure!(
            !plan.root.to_string_lossy().contains([',', '\n', '\r']),
            "container bind path contains an unsupported separator"
        );
        anyhow::ensure!(
            matches!(plan.report, ReportSource::Stdout | ReportSource::Stderr),
            "container reports must be streamed; host file writes are not mounted"
        );
        let dir = tempfile::Builder::new()
            .prefix("thruline-check-")
            .tempdir()?;
        let name = dir
            .path()
            .file_name()
            .context("container owner ID")?
            .to_string_lossy()
            .into_owned();
        let mut argv = vec![
            spec.docker.display().to_string(),
            "--host".into(),
            format!("unix://{}", spec.socket.display()),
            "create".into(),
            "--pull=never".into(),
            "--name".into(),
            name.clone(),
            "--label".into(),
            format!("thruline.owner={name}"),
            "--cidfile".into(),
            dir.path().join("container.cid").display().to_string(),
            "--network=none".into(),
            "--read-only".into(),
            "--cap-drop=ALL".into(),
            "--security-opt=no-new-privileges".into(),
            "--user=1000:1000".into(),
            "--pids-limit=128".into(),
            "--cpus=1".into(),
            format!("--memory={}m", plan.memory_mb),
            format!("--memory-swap={}m", plan.memory_mb),
            format!("--ulimit=cpu={}:{}", plan.cpu_seconds, plan.cpu_seconds),
            "--shm-size=128m".into(),
            "--tmpfs=/tmp:rw,nosuid,nodev,size=536870912,mode=1777".into(),
            "--workdir=/tmp".into(),
            "--mount".into(),
            format!("type=bind,src={},dst=/input,readonly", plan.root.display()),
        ];
        for (k, v) in &spec.environment {
            argv.extend(["--env".into(), format!("{k}={v}")]);
        }
        // Override any image ENTRYPOINT; the accepted argv is the workload.
        argv.extend([
            "--entrypoint".into(),
            plan.argv[0].clone(),
            spec.image.clone(),
        ]);
        argv.extend(plan.argv[1..].iter().cloned());
        let mut lease = Self {
            spec: spec.clone(),
            dir,
            closed: false,
        };
        let args: Vec<&str> = argv[3..].iter().map(String::as_str).collect();
        if let Err(e) = bounded(spec, &args) {
            let cleanup = lease.finish();
            if let Err(cleanup) = cleanup {
                bail!("Isolated container creation failed ({e}); no workload was started. Cleanup could not be confirmed: {cleanup}");
            }
            return Err(e).context(
                "The isolated verifier container could not be created; no workload was started",
            );
        }
        let id = lease.id()?;
        let start = vec![
            spec.docker.display().to_string(),
            "--host".into(),
            format!("unix://{}", spec.socket.display()),
            "start".into(),
            "--attach".into(),
            id,
        ];
        Ok((lease, start))
    }
    fn id(&self) -> Result<String> {
        let id = std::fs::read_to_string(self.dir.path().join("container.cid"))?;
        anyhow::ensure!(digest(id.trim()), "container ownership file is invalid");
        Ok(id.trim().into())
    }
    pub fn finish(&mut self) -> Result<Value> {
        if self.closed {
            bail!("container lease already closed");
        }
        let id = match self.id() {
            Ok(id) => id,
            Err(_) if !self.dir.path().join("container.cid").exists() => {
                // A timed-out create may have reached the daemon before the CID
                // file was written. Inspect only our random name and verify its
                // owner label. Absence/daemon failure is unknown, not a cleanup claim.
                let name = self
                    .dir
                    .path()
                    .file_name()
                    .context("container owner ID")?
                    .to_string_lossy();
                let bytes = bounded(&self.spec, &["inspect", "--format", "{{json .}}", &name])
                    .context("container creation outcome is unknown; inspect the named ThruLine worker when the local daemon is available")?;
                let value: Value = engine::parse_json(&bytes)?;
                anyhow::ensure!(
                    value
                        .pointer("/Config/Labels/thruline.owner")
                        .and_then(Value::as_str)
                        == Some(name.as_ref()),
                    "refusing cleanup of a container without this worker's owner label"
                );
                let id = value
                    .get("Id")
                    .and_then(Value::as_str)
                    .context("owned container identity missing")?;
                anyhow::ensure!(digest(id), "owned container identity invalid");
                id.to_string()
            }
            Err(e) => return Err(e),
        };
        let observed = bounded(&self.spec, &["inspect", "--format", "{{json .State}}", &id]);
        let removed = bounded(&self.spec, &["rm", "--force", &id]);
        // Mark closed only after successful removal; Drop retries its owned ID.
        removed.context(
            "The owned verifier container could not be removed; do not treat this run as complete",
        )?;
        self.closed = true;
        let state: Value =
            engine::parse_json(&observed.context("container final state unavailable")?)?;
        Ok(
            json!({"image":self.spec.image,"container_id":id,"created":true,"state":state,
            "cleanup":"owned container removed", "network":"none; container loopback only",
            "memory_limit":"cgroup enforced; host CLI RSS is not container RSS",
            "filesystem":"read-only root and project; private bounded /tmp"}),
        )
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.finish();
        }
    }
}
