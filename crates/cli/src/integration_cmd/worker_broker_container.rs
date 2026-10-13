//! The product owns named native/oracle namespaces even if its trusted Python
//! broker is killed. Workload code never receives the Docker socket or owner.
use super::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(super) struct Owner {
    spec: super::super::container::Spec,
    directory: tempfile::TempDir,
    name: String,
    closed: bool,
}
impl Owner {
    pub fn prepare(path: &Path) -> Result<Self> {
        let value: Value = serde_json::from_slice(&super::super::bounded_file(path, 1024 * 1024)?)?;
        anyhow::ensure!(
            value["schema"] == "thruline.local-agent-container-runtime.v1"
                || value["schema"] == "thruline.megalinter-container-runtime.v1",
            "unsupported local agent container runtime"
        );
        let spec = super::super::container::Spec {
            docker: PathBuf::from(
                value["docker"]["path"]
                    .as_str()
                    .context("Docker executable missing")?,
            ),
            socket: PathBuf::from(
                value["socket"]
                    .as_str()
                    .context("local Docker socket missing")?,
            ),
            image: value["image"]
                .as_str()
                .context("pinned container image missing")?
                .into(),
            environment: BTreeMap::new(),
        };
        anyhow::ensure!(
            super::super::hash_file(&spec.docker)?
                == value["docker"]["sha256"]
                    .as_str()
                    .context("Docker digest missing")?,
            "Docker runtime changed"
        );
        super::super::container::identity(&spec)?;
        let directory = tempfile::Builder::new()
            .prefix("thruline-agent-owner-")
            .tempdir()?;
        let name = directory
            .path()
            .file_name()
            .context("container owner directory")?
            .to_string_lossy()
            .into_owned();
        Ok(Self {
            spec,
            directory,
            name,
            closed: false,
        })
    }
    pub fn environment(&self, command: &mut Command, plan: &Plan) {
        command
            .env("THRULINE_BROKER_CONTAINER_NAME", &self.name)
            .env("THRULINE_BROKER_CONTAINER_OWNER", &self.name)
            .env("THRULINE_BROKER_MEMORY_MB", plan.memory_mb.to_string())
            .env("THRULINE_BROKER_CPU_SECONDS", plan.cpu_seconds.to_string());
    }
    fn call(&self, args: &[&str]) -> Result<(i32, Vec<u8>, Vec<u8>)> {
        let mut child = Command::new(&self.spec.docker)
            .args(["--host", &format!("unix://{}", self.spec.socket.display())])
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin:/usr/local/bin")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let out = read(child.stdout.take().context("Docker stdout")?, 65536);
        let err = read(child.stderr.take().context("Docker stderr")?, 65536);
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if start.elapsed() > Duration::from_secs(5) {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("named agent namespace cleanup timed out");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let out = out.recv_timeout(Duration::from_secs(1))?;
        let err = err.recv_timeout(Duration::from_secs(1))?;
        anyhow::ensure!(
            out.complete && err.complete,
            "Docker ownership metadata exceeded bound"
        );
        Ok((status.code().unwrap_or(-1), out.bytes, err.bytes))
    }
    fn inspect(&self, name: &str) -> Result<Option<Value>> {
        let (code, bytes, stderr) = self.call(&["inspect", name])?;
        if code != 0 {
            anyhow::ensure!(
                String::from_utf8_lossy(&stderr)
                    .to_ascii_lowercase()
                    .contains("no such object")
                    || String::from_utf8_lossy(&stderr)
                        .to_ascii_lowercase()
                        .contains("no such container"),
                "Docker could not establish namespace existence"
            );
            return Ok(None);
        }
        let values: Vec<Value> = serde_json::from_slice(&bytes)?;
        let value = values
            .into_iter()
            .next()
            .context("container identity response empty")?;
        anyhow::ensure!(
            value
                .pointer("/Config/Labels/thruline.owner")
                .and_then(Value::as_str)
                == Some(self.name.as_str())
                && value["Image"].as_str() == Some(self.spec.image.as_str()),
            "named container ownership/image does not match product lease; refusing unrelated kill"
        );
        anyhow::ensure!(
            value
                .pointer("/HostConfig/NetworkMode")
                .and_then(Value::as_str)
                == Some("none")
                && value
                    .pointer("/HostConfig/ReadonlyRootfs")
                    .and_then(Value::as_bool)
                    == Some(true)
                && value.pointer("/HostConfig/PidMode").and_then(Value::as_str) != Some("host")
                && value.pointer("/HostConfig/IpcMode").and_then(Value::as_str) != Some("host")
                && value.pointer("/HostConfig/Privileged").and_then(Value::as_bool) == Some(false)
                && value.pointer("/HostConfig/CapAdd").is_none_or(|caps| caps.is_null() || caps.as_array().is_some_and(Vec::is_empty)),
            "native container lost declared isolation configuration"
        );
        Ok(Some(value))
    }
    pub fn finish(&mut self) -> Result<Value> {
        let start = Instant::now();
        let mut rows = vec![];
        for suffix in ["native", "oracle-before", "oracle-after"] {
            let name = format!("{}-{suffix}", self.name);
            let Some(before) = self.inspect(&name)? else {
                rows.push(json!({"name":name,"created":false}));
                continue;
            };
            if before.pointer("/State/Running").and_then(Value::as_bool) == Some(true) {
                let (code, _, _) = self.call(&["kill", &name])?;
                anyhow::ensure!(code == 0, "owned native namespace kill failed");
            }
            let after = self
                .inspect(&name)?
                .context("owned namespace disappeared before final observation")?;
            anyhow::ensure!(
                after.pointer("/State/Running").and_then(Value::as_bool) == Some(false)
                    && after.pointer("/State/Pid").and_then(Value::as_u64) == Some(0),
                "native namespace termination is unestablished"
            );
            rows.push(json!({"name":name,"created":true,"id":after["Id"],"state":after["State"],"before_state":before["State"],"image":after["Image"],"host_config":after["HostConfig"],"mounts":after["Mounts"]}));
        }
        self.closed = true;
        Ok(
            json!({"backend":"product-owned-linux-agent-namespaces-v1","owner":self.name,"owner_directory":self.directory.path(),"namespaces":rows,"termination_established":true,"cleanup_duration_ms":start.elapsed().as_millis(),"network":"none; native loopback gateway uses only fixed host model mailbox; no host IPC or Docker socket is mounted"}),
        )
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.finish();
        }
    }
}
