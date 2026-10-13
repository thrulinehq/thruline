//! Native deterministic remedies use the bounded worker and existing command bindings.
//! Repair state is an artifact, never an inferred correctness verdict.
use super::*;

const BROKER: &str = include_str!("../../../../scripts/integrations/native-repair.py");

fn docker_control(attempt: &Path, args: &[&str]) -> Result<(bool, Vec<u8>, Vec<u8>)> {
    let out = tempfile::NamedTempFile::new_in(attempt)?;
    let err = tempfile::NamedTempFile::new_in(attempt)?;
    let mut child = std::process::Command::new("/usr/local/bin/docker")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(out.reopen()?)
        .stderr(err.reopen()?)
        .spawn()?;
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > std::time::Duration::from_secs(10)
            || out.as_file().metadata()?.len() + err.as_file().metadata()?.len() > 1024 * 1024
        {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "container cleanup exceeded its time/output bound; native job is not accounted for"
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    Ok((
        status.success(),
        bounded_file(out.path(), 1024 * 1024)?,
        bounded_file(err.path(), 1024 * 1024)?,
    ))
}

fn cleanup_container(attempt: &Path) -> Result<Value> {
    let expected = sha256_hex(attempt.to_string_lossy().as_bytes());
    let filter = format!("label=org.thruline.native-repair-attempt={expected}");
    // Creation and execution are separate: no workload starts until its CID
    // has been recorded. A label lookup also accounts for cancelled creation.
    let (success, out, err) = docker_control(
        attempt,
        &["ps", "--all", "--no-trunc", "--quiet", "--filter", &filter],
    )?;
    anyhow::ensure!(
        success,
        "owned container inventory unavailable: {}",
        String::from_utf8_lossy(&err)
    );
    let mut ids: std::collections::BTreeSet<String> = std::str::from_utf8(&out)?
        .lines()
        .map(str::to_owned)
        .collect();
    let file = attempt.join("cargo-fix.cid");
    if file.exists() {
        anyhow::ensure!(
            !fs::symlink_metadata(&file)?.file_type().is_symlink(),
            "native repair container record cannot be a symlink"
        );
        let bytes = bounded_file(&file, 128)?;
        ids.insert(std::str::from_utf8(&bytes)?.trim().to_owned());
    }
    anyhow::ensure!(ids.len() <= 4, "unexpected owned repair container count");
    let mut results = Vec::new();
    for id in ids {
        anyhow::ensure!(
            id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid native repair container id"
        );
        let (success, out, err) = docker_control(attempt, &["inspect", &id])?;
        if !success {
            let error = String::from_utf8_lossy(&err);
            anyhow::ensure!(
                error.to_ascii_lowercase().contains("no such object")
                    || error.to_ascii_lowercase().contains("no such container"),
                "native repair container final state is unavailable: {error}"
            );
            results.push(json!({"container_id":id,"already_removed":true}));
            continue;
        }
        let value = engine::parse_json(&out)?;
        anyhow::ensure!(
            value
                .pointer("/0/Config/Labels/org.thruline.native-repair-attempt")
                .and_then(Value::as_str)
                == Some(expected.as_str()),
            "container does not belong to this repair attempt"
        );
        let (removed, _, _) = docker_control(attempt, &["rm", "-f", &id])?;
        anyhow::ensure!(removed, "native repair container cleanup failed");
        results.push(
            json!({"container_id":id,"removed":true,"state_before_cleanup":value[0]["State"]}),
        );
    }
    Ok(json!({"owned_container_inventory_checked":true,"containers":results}))
}

fn recover_attempt_artifacts(
    plan: &Plan,
    broker: &Path,
    request_file: &Path,
    request: &Value,
    attempt: &Path,
    original: &worker::Outcome,
    cleanup: &Value,
) -> Result<(Value, Value)> {
    // A read-only collector cannot establish termination. Require the original
    // worker's measured cleanup and the owned-container control result first.
    anyhow::ensure!(
        original
            .isolation
            .as_ref()
            .and_then(|v| v.pointer("/process_tree/observed_processes_terminated"))
            == Some(&json!(true)),
        "Native worker termination was not confirmed before artifact recovery"
    );
    let pack = request["config"]["recipe"]
        .as_str()
        .context("recovery recipe missing")?;
    if pack == "semgrep"
        || pack == "osv-scanner"
        || (pack == "clippy" && request.pointer("/config/selection/fix_container").is_some())
    {
        anyhow::ensure!(
            cleanup["owned_container_inventory_checked"] == true,
            "Owned native container termination was not confirmed before recovery"
        );
    }
    anyhow::ensure!(
        hash_file(broker)? == sha256_hex(BROKER.as_bytes()),
        "Captured broker changed before recovery"
    );
    let request_bytes = bounded_file(request_file, 1024 * 1024)?;
    anyhow::ensure!(
        engine::parse_json(&request_bytes)? == *request,
        "Repair request changed before recovery"
    );
    let request_sha = sha256_hex(&request_bytes);
    let mut recovery = plan.clone();
    recovery.root = attempt.to_owned();
    recovery.argv = vec![
        plan.argv[0].clone(),
        "-I".into(),
        broker.display().to_string(),
        "--capture-attempt".into(),
        request_file.display().to_string(),
        request_sha.clone(),
    ];
    recovery.runtime = vec![broker.to_owned(), request_file.to_owned()];
    recovery.timeout_seconds = 15;
    recovery.cpu_seconds = 10;
    recovery.memory_mb = 256;
    recovery.output_limit_bytes = 8 * 1024 * 1024;
    recovery.writable_paths.clear();
    // No broker bypass here: the entire collector runs with read-only attempt
    // contents, no network and only the standard private scratch write grant.
    let mut prepared = worker::prepare(&recovery)?;
    let actual = prepared.argv.clone();
    let outcome = worker::execute(&recovery, &mut prepared, |_| Ok(()))?;
    fs::write(attempt.join("recovery.stdout"), &outcome.stdout.bytes)?;
    fs::write(attempt.join("recovery.stderr"), &outcome.stderr.bytes)?;
    let transport = json!({"schema":"thruline.native-repair-recovery-transport.v1","capture_only":true,
        "argv":actual,"worker_state":outcome.state,"exit_code":outcome.exit_code,
        "duration_ms":outcome.duration_ms,"sampled_peak_rss_bytes":outcome.sampled_peak_rss_bytes,
        "isolation":outcome.isolation,"request_sha256":request_sha,"broker_sha256":sha256_hex(BROKER.as_bytes()),
        "stdout":{"sha256":sha256_hex(&outcome.stdout.bytes),"bytes":outcome.stdout.bytes.len()},
        "stderr":{"sha256":sha256_hex(&outcome.stderr.bytes),"bytes":outcome.stderr.bytes.len()},
        "termination_basis":{"original_worker":original.isolation,"container_cleanup":cleanup}});
    fs::write(
        attempt.join("recovery-transport.json"),
        serde_json::to_vec(&transport)?,
    )?;
    anyhow::ensure!(
        outcome.state == RunState::Completed
            && outcome.exit_code == Some(0)
            && outcome.stdout.complete
            && outcome.stderr.complete,
        "Bounded capture-only recovery failed; command/streams retained"
    );
    let mut value = engine::parse_json(&outcome.stdout.bytes)?;
    anyhow::ensure!(
        value["schema"] == "thruline.native-repair-artifact-recovery.v1"
            && value["capture_only"] == true
            && value["attempt"] == json!(attempt)
            && value["request_sha256"] == request_sha
            && value["broker_sha256"] == sha256_hex(BROKER.as_bytes())
            && value["attempt_artifacts"]["schema"]
                == "thruline.native-repair-attempt-artifacts.v1",
        "Unexpected capture-only recovery response"
    );
    let artifacts = value
        .as_object_mut()
        .context("recovery object missing")?
        .remove("attempt_artifacts")
        .context("recovered native manifest missing")?;
    value["transport"] = transport;
    Ok((artifacts, value))
}

fn seal_native_artifacts(db: &Path, root: &Path, store: &Path, result: &mut Value) -> Result<()> {
    use std::io::Read;
    let destination = artifact_dir(db).canonicalize()?;
    let attempts = store.join("attempts").canonicalize()?;
    let mut total = 0_u64;
    let mut count = 0_usize;
    for pointer in [
        "/attempt_artifacts",
        "/original_detector/native",
        "/native_fix/native",
        "/verification/remaining_detector/native",
        "/verification/base/native",
        "/verification/changed/native",
    ] {
        let Some(native) = result.pointer_mut(pointer) else {
            continue;
        };
        if !matches!(
            native["schema"].as_str(),
            Some(
                "thruline.native-repair-attempt-artifacts.v1"
                    | "thruline.clippy-capture.v1"
                    | "thruline.clippy-capture.v2"
                    | "thruline.openrewrite-capture.v1"
                    | "thruline.openrewrite-module-capture.v1"
                    | "thruline.semgrep-repair-capture.v1"
                    | "thruline.opengrep-repair-capture.v1"
                    | "thruline.osv-guided-repair-capture.v1"
                    | "thruline.repair-oracle-capture.v1"
            )
        ) {
            continue;
        }
        let rows = native
            .get_mut("artifacts")
            .and_then(Value::as_array_mut)
            .context("Known native envelope has no artifact manifest")?;
        for row in rows {
            count += 1;
            anyhow::ensure!(
                count <= 4096,
                "Native repair exceeds 4096 artifact references"
            );
            let source = PathBuf::from(
                row["path"]
                    .as_str()
                    .context("Native artifact path missing")?,
            );
            let expected = row["sha256"]
                .as_str()
                .context("Native artifact SHA missing")?
                .to_owned();
            let managed = source.starts_with(&destination);
            let boundary = if managed { &destination } else { &attempts };
            let relative = source
                .strip_prefix(boundary)
                .context("Native artifact escaped the selected repair store")?;
            let mut cursor = boundary.clone();
            for component in relative.components() {
                anyhow::ensure!(
                    matches!(component, std::path::Component::Normal(_)),
                    "Native artifact traversal refused"
                );
                cursor.push(component);
                anyhow::ensure!(
                    !fs::symlink_metadata(&cursor)?.file_type().is_symlink(),
                    "Native artifact symlink refused"
                );
            }
            anyhow::ensure!(
                source.is_absolute() && source.canonicalize()? == source,
                "Native artifact path is not canonical"
            );
            if !managed {
                let owner = relative
                    .components()
                    .next()
                    .context("Native artifact attempt missing")?;
                let request = engine::parse_json(&bounded_file(
                    &attempts.join(owner).join("request.json"),
                    65536,
                )?)?;
                anyhow::ensure!(
                    request["root"].as_str() == root.to_str(),
                    "Native artifact belongs to another source root"
                );
            }
            #[cfg(unix)]
            let file = {
                use rustix::fs::{open, Mode, OFlags};
                fs::File::from(open(
                    &source,
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                    Mode::empty(),
                )?)
            };
            #[cfg(not(unix))]
            let file = fs::File::open(&source)?;
            let meta = file.metadata()?;
            anyhow::ensure!(
                meta.is_file() && meta.len() <= 8 * 1024 * 1024,
                "Native repair attachment is not a bounded regular file"
            );
            total = total
                .checked_add(meta.len())
                .context("Native attachment size overflow")?;
            anyhow::ensure!(
                total <= 64 * 1024 * 1024,
                "Native repair attachments exceed 64 MiB"
            );
            let mut bytes = Vec::new();
            file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            anyhow::ensure!(
                bytes.len() as u64 == meta.len()
                    && row["bytes"].as_u64() == Some(meta.len())
                    && sha256_hex(&bytes) == expected,
                "Native repair attachment differs from its captured identity"
            );
            let saved = private_write(&destination, &bytes)?;
            row["original_path"] = json!(source);
            row["path"] = json!(saved);
        }
    }
    result["native_attachment_seal"] = json!({"complete":result["attempt_artifacts"].is_null() || result["attempt_artifacts"]["complete"] == true,
            "references":count,"bytes":total,"store":destination});
    Ok(())
}

fn capture_attempt_logs(db: &Path, attempt: &Path) -> (Vec<ArtifactDigest>, Vec<Value>) {
    use std::io::Read;
    let mut artifacts = vec![];
    let mut gaps = vec![];
    let entries = match fs::read_dir(attempt) {
        Ok(entries) => entries,
        Err(error) => {
            return (
                artifacts,
                vec![json!({"path":attempt,"reason":error.to_string()})],
            );
        }
    };
    let mut total = 0_u64;
    for (index, entry) in entries.enumerate() {
        if index >= 4096 {
            gaps.push(
                json!({"path":attempt,"reason":"Top-level attempt log entry budget exceeded"}),
            );
            break;
        }
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(error) => {
                gaps.push(json!({"path":attempt,"reason":error.to_string()}));
                continue;
            }
        };
        if !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("stdout" | "stderr" | "json" | "sb" | "cid")
        ) {
            continue;
        }
        let captured = (|| -> Result<ArtifactDigest> {
            anyhow::ensure!(
                fs::symlink_metadata(&path)?.file_type().is_file(),
                "Attempt log must be a regular file, not a symlink or special file"
            );
            #[cfg(unix)]
            let file = {
                use rustix::fs::{open, Mode, OFlags};
                fs::File::from(open(
                    &path,
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                    Mode::empty(),
                )?)
            };
            #[cfg(not(unix))]
            let file = fs::File::open(&path)?;
            let meta = file.metadata()?;
            anyhow::ensure!(
                meta.is_file() && meta.len() <= 8 * 1024 * 1024,
                "Attempt log exceeds the bounded regular-file contract"
            );
            anyhow::ensure!(
                total + meta.len() <= 64 * 1024 * 1024,
                "Attempt logs exceed 64 MiB"
            );
            let mut bytes = vec![];
            file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            anyhow::ensure!(
                bytes.len() as u64 == meta.len(),
                "Attempt log changed during capture"
            );
            let saved = private_write(&artifact_dir(db), &bytes)?;
            total += bytes.len() as u64;
            Ok(ArtifactDigest {
                path: saved.to_string_lossy().into_owned(),
                sha256: Some(sha256_hex(&bytes)),
            })
        })();
        match captured {
            Ok(artifact) => artifacts.push(artifact),
            Err(error) => gaps.push(json!({"path":path,"reason":error.to_string()})),
        }
    }
    (artifacts, gaps)
}

fn captured(db: &Path, root: &Path, pack: &str) -> Result<Vec<Value>> {
    if !db.exists() {
        return Ok(vec![]);
    }
    let observation = wsid::observe(root, Scope::everything())?;
    let repo = match observation {
        WObservation::Complete(fp) => fp
            .doc()
            .repo_id
            .clone()
            .context("repository identity unavailable")?,
        _ => bail!("repair history requires a complete workspace identity"),
    };
    let ledger = Ledger::open(db)?;
    let mut rows = vec![];
    for saved in evb::all_for_repo(&ledger, &repo)?
        .into_iter()
        .rev()
        .filter(|v| {
            v.binding.source_adapter == format!("thruline-native-repair:{pack}")
                && Path::new(&v.binding.root).canonicalize().ok().as_deref() == Some(root)
        })
        .take(100)
    {
        if let Some(artifact) = saved.binding.artifacts.first() {
            let path = Path::new(&artifact.path).canonicalize()?;
            anyhow::ensure!(
                path.parent() == Some(artifact_dir(db).canonicalize()?.as_path()),
                "repair receipt artifact is outside its store"
            );
            let bytes = bounded_file(&path, 8 * 1024 * 1024)?;
            anyhow::ensure!(
                artifact.sha256.as_deref() == Some(sha256_hex(&bytes).as_str()),
                "repair receipt artifact changed"
            );
            let mut value = engine::parse_json(&bytes)?;
            anyhow::ensure!(
                value.get("root").and_then(Value::as_str) == root.to_str()
                    && value.get("recipe").and_then(Value::as_str) == Some(pack),
                "native repair receipt root or recipe differs from selected project"
            );
            value["binding_id"] = json!(saved.id);
            value["artifact"] = json!(path);
            rows.push(value);
        }
    }
    Ok(rows)
}

fn repair_runtime_paths(
    root: &Path,
    pack: &str,
    runtime: &Value,
    selection: &Value,
) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = serde_json::from_value(runtime["runtime"].clone())?;
    paths.push(PathBuf::from(
        runtime["executable"]
            .as_str()
            .context("Runtime executable missing")?,
    ));
    if pack == "osv-scanner" {
        let consumer: Vec<PathBuf> =
            serde_json::from_value(selection["guided"]["consumer_runtime"].clone())?;
        anyhow::ensure!(
            !consumer.is_empty() && consumer.len() <= 16,
            "Select a bounded consumer runtime outside source"
        );
        for path in &consumer {
            anyhow::ensure!(
                path.is_absolute() && !path.canonicalize()?.starts_with(root),
                "Consumer runtime must be outside source"
            );
        }
        paths.extend(consumer);
    }
    Ok(paths)
}

pub(crate) fn action(
    db: &Path,
    root: &Path,
    action: &str,
    pack: &str,
    digest: &str,
) -> Result<Value> {
    anyhow::ensure!(
        ["preview", "verify", "apply", "list"].contains(&action),
        "Unknown repair action"
    );
    anyhow::ensure!(
        [
            "clippy",
            "knip",
            "openrewrite",
            "semgrep",
            "opengrep",
            "osv-scanner"
        ]
        .contains(&pack)
            && super::shipped(pack),
        "Unsupported native repair recipe"
    );
    let root = root.canonicalize()?;
    let store = db.with_extension("integration-native-repairs").join(pack);
    anyhow::ensure!(
        !fs::symlink_metadata(&store).is_ok_and(|m| m.file_type().is_symlink()),
        "repair store cannot be a symlink"
    );
    fs::create_dir_all(&store)?;
    let store = store.canonicalize()?;
    anyhow::ensure!(
        !store.starts_with(&root),
        "repair artifacts must be outside the reviewed project"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700))?;
    }
    let attempts = store.join("attempts");
    fs::create_dir_all(&attempts)?;
    let recorded = captured(db, &root, pack)?;
    if action == "list" {
        let mut seen = std::collections::BTreeSet::new();
        let mut repairs: Vec<Value> = recorded
            .into_iter()
            .filter(|row| {
                let identity = row
                    .get("digest")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .or_else(|| row.pointer("/transport/attempt").and_then(Value::as_str))
                    .unwrap_or("");
                seen.insert(identity.to_string())
            })
            .collect();
        for entry in fs::read_dir(&attempts)?.take(100) {
            let file = entry?.path().join("result.json");
            if file.is_file() {
                let value = engine::parse_json(&bounded_file(&file, 8 * 1024 * 1024)?)?;
                if value.get("state").and_then(Value::as_str) == Some("unavailable")
                    && value.get("root").and_then(Value::as_str) == root.to_str()
                {
                    repairs.push(value);
                }
            }
        }
        return Ok(json!({"schema":"thruline.native-repair-list.v1","repairs":repairs}));
    }
    let prior_meta = recorded
        .iter()
        .filter(|v| v.get("digest").and_then(Value::as_str) == Some(digest))
        .find_map(|v| v.get("meta_sha256").and_then(Value::as_str));
    if action == "apply" {
        anyhow::ensure!(
            recorded
                .iter()
                .find(|row| row.get("digest").and_then(Value::as_str) == Some(digest))
                .and_then(|row| row.get("state"))
                .and_then(Value::as_str)
                == Some("verified_for_named_checks"),
            "The latest captured attempt must independently verify this patch before apply"
        );
    }
    let attempt = tempfile::Builder::new()
        .prefix(&format!("{action}-"))
        .tempdir_in(&attempts)?
        .keep();
    let configured = (|| -> Result<Value> {
        let path: PathBuf = std::env::var_os("THRULINE_NATIVE_REPAIR_CONFIG")
            .context("No independent behavior oracle is configured. Set THRULINE_NATIVE_REPAIR_CONFIG to a reviewed, hash-pinned external oracle and selected editable files.")?
            .into();
        anyhow::ensure!(
            path.is_absolute() && !path.canonicalize()?.starts_with(&root),
            "repair configuration must be an absolute path outside the project"
        );
        let config = engine::parse_json(&bounded_file(&path, 65536)?)?;
        let python = PathBuf::from(
            config
                .pointer("/python/path")
                .and_then(Value::as_str)
                .context("repair configuration needs a pinned Python executable")?,
        );
        anyhow::ensure!(
            python.is_absolute()
                && config.pointer("/python/sha256").and_then(Value::as_str)
                    == Some(hash_file(&python)?.as_str()),
            "The repair Python executable does not match its reviewed hash"
        );
        anyhow::ensure!(
            config.get("schema").and_then(Value::as_str)
                == Some("thruline.native-repair-config.v1"),
            "unsupported native repair config"
        );
        let selected = config
            .pointer(&format!("/tools/{pack}"))
            .context("No independent oracle configured for this recipe")?;
        let runtime = workflow::native_repair_runtime(pack)?;
        let runtime_paths = repair_runtime_paths(&root, pack, &runtime, selected)?;
        let runtime_identity = input_read::hash_runtime(&root, &runtime_paths)?;
        Ok(
            json!({"schema":"thruline.native-repair-config.v1","python":config["python"],"recipe":pack,"selection":selected,"runtime":runtime,"runtime_identity":runtime_identity}),
        )
    })();
    let config = match configured {
        Ok(config) => config,
        Err(error) => {
            let value = json!({"schema":"thruline.native-repair.v1", "state":"unavailable", "recipe":pack,
                "error":error.to_string(), "attempt":attempt, "digest":digest, "root":root});
            fs::write(attempt.join("result.json"), serde_json::to_vec(&value)?)?;
            return Ok(value);
        }
    };
    let broker = private_write(&artifact_dir(db), BROKER.as_bytes())?.canonicalize()?;
    let request = json!({"root":root,"store":store,"attempt":attempt,"action":action,
        "digest":digest,"config":config,"candidate_sha256":hash_file(&std::env::current_exe()?)?, "prior_meta_sha256":prior_meta});
    let request_file = attempt.join("request.json");
    fs::write(&request_file, serde_json::to_vec(&request)?)?;
    let plan = Plan {
        schema: "thruline.integration-plan.v1".into(),
        pack: "native-repair".into(),
        format: "thruline.native-repair.v1".into(),
        support: None,
        root: root.clone(),
        argv: vec![
            config["python"]["path"].as_str().context("Python path missing")?.into(),
            "-I".into(), broker.to_string_lossy().into_owned(), request_file.to_string_lossy().into_owned(),
        ],
        container: None,
        local_model_port: None,
        writable_paths: vec![],
        criterion: Criterion {
            id: format!("native-{pack}-repair"), revision: "v1".into(),
            requirement: "Preview the selected native remedy and apply only after the pinned independent behavior oracle passes".into(),
            success: "Named baseline and changed cases pass; the original selected detector clears; source, runtime, oracle and patch identities remain current".into(),
            accepted_by: "explicit local repair action with operator-configured independent oracle".into(),
            condition: engine::Condition::NoObservations,
        },
        acceptance: None, approved_setup: None, rules: vec![], runtime: vec![],
        dependencies: vec![], dataset: vec![],
        environment: BTreeMap::from([("PATH".into(), "/usr/bin:/bin".into()), ("PYTHONDONTWRITEBYTECODE".into(), "1".into())]),
        input_closure_complete: false, timeout_seconds: 680, output_limit_bytes: 8 * 1024 * 1024,
        cpu_seconds: 300, memory_mb: 3072, report: ReportSource::Stdout,
    };
    let pre = wsid::observe(&root, Scope::everything())?;
    let fingerprint = match &pre {
        WObservation::Complete(fp) => fp,
        _ => bail!("repair cannot bind an incomplete workspace observation"),
    };
    let started = thruline_core::ingest::iso8601_utc_now();
    let mut prepared = worker::prepare_broker(&plan)?;
    // macOS rejects nested sandbox_apply. This embedded, hash-bound filesystem
    // broker executes fixed local Git/Docker control operations. Native Cargo and
    // Semgrep fixes run in their pinned network-none containers. Every host analyzer/oracle child
    // enters the stronger network/read/write sandbox in BROKER exactly once.
    // The existing worker still owns group RSS, timeout, capture and termination.
    let mut binding = EvidenceBinding {
        argv: prepared.argv.clone(),
        mode: ExecMode::Direct,
        cwd: fingerprint.doc().root.clone(),
        root: fingerprint.doc().root.clone(),
        repository_identity: fingerprint
            .doc()
            .repo_id
            .clone()
            .context("repository identity unavailable")?,
        scope: evb::classify_command(&plan.argv.join(" "), &fingerprint.doc().root),
        wsid_scope: WsidScopeDecl::everything(),
        pre: IdentityEdge::from_observation(&pre),
        post: IdentityEdge::incomplete("repair running".into()),
        exit_code: None,
        term_signal: None,
        stdout: evb::StreamCapture::Truncated {
            reason: "repair running".into(),
        },
        stderr: evb::StreamCapture::Truncated {
            reason: "repair running".into(),
        },
        artifacts: vec![],
        wrapper_defects: vec![],
        started_at: started.clone(),
        bound_at: started.clone(),
        source_adapter: format!("thruline-native-repair:{pack}"),
        source_session: format!("repair:{started}"),
    };
    let ledger = Ledger::open(db)?;
    let outcome = worker::execute(&plan, &mut prepared, |pid| {
        evb::record_command_started(&ledger, &binding, pid)
            .map(|_| ())
            .map_err(Into::into)
    })?;
    let cleanup = if pack == "semgrep"
        || pack == "osv-scanner"
        || (pack == "clippy" && config.pointer("/selection/fix_container").is_some())
    {
        cleanup_container(&attempt)
    } else {
        Ok(Value::Null)
    };
    fs::write(attempt.join("worker.stdout"), &outcome.stdout.bytes)?;
    fs::write(attempt.join("worker.stderr"), &outcome.stderr.bytes)?;
    let mut result = if outcome.state == RunState::Completed && outcome.exit_code == Some(0) {
        engine::parse_json(&outcome.stdout.bytes).unwrap_or_else(|e| {
            json!({
            "schema":"thruline.native-repair.v1", "state":"capture_failed", "error":e.to_string(), "attempt":attempt})
        })
    } else {
        json!({"schema":"thruline.native-repair.v1", "state":if outcome.state == RunState::Completed { "failed".to_string() } else { format!("{:?}",outcome.state).to_lowercase() },
            "error":"Repair did not finish within its execution/capture contract. Original and attempted artifacts remain available.",
            "diagnostics":thruline_core::redact::mask_recorded(&String::from_utf8_lossy(&outcome.stderr.bytes)).0,
            "attempt":attempt,"digest":digest})
    };
    result["transport"] = json!({"run_state":outcome.state,"exit_code":outcome.exit_code,
        "duration_ms":outcome.duration_ms,"sampled_peak_rss_bytes":outcome.sampled_peak_rss_bytes,
        "resource_error":outcome.resource_error,"candidate_sha256":request["candidate_sha256"],
        "broker_sha256":sha256_hex(BROKER.as_bytes()),"attempt":attempt});
    result["root"] = json!(root);
    result["recipe"] = json!(pack);
    result["container_cleanup"] = match cleanup {
        Ok(value) => value,
        Err(error) => {
            result["state"] = json!("rejected");
            result["error"] = json!(error.to_string());
            json!({"complete":false,"error":error.to_string()})
        }
    };
    let runtime_after = repair_runtime_paths(&root, pack, &config["runtime"], &config["selection"])
        .and_then(|paths| input_read::hash_runtime(&root, &paths));
    if runtime_after.as_ref().ok().map(String::as_str) != config["runtime_identity"].as_str() {
        result["state"] = json!("rejected");
        result["error"] = json!(
            "Native runtime changed during the repair action; retained evidence is not an approved patch"
        );
    }
    result["runtime_identity_before"] = config["runtime_identity"].clone();
    result["runtime_identity_after"] = runtime_after.map(Value::String).unwrap_or(Value::Null);
    let oracle_path = Path::new(
        config["selection"]["oracle"]["path"]
            .as_str()
            .context("Oracle path missing")?,
    );
    let oracle_bytes = bounded_file(oracle_path, 8 * 1024 * 1024)?;
    let oracle_sha = sha256_hex(&oracle_bytes);
    if config["selection"]["oracle"]["sha256"].as_str() != Some(oracle_sha.as_str()) {
        result["state"] = json!("rejected");
        result["error"] = json!("Independent oracle changed during this action");
    }
    let oracle_artifact = private_write(&artifact_dir(db), &oracle_bytes)?;
    result["oracle_artifact"] =
        json!({"path":oracle_artifact,"sha256":oracle_sha,"bytes":oracle_bytes.len()});
    let positive = matches!(
        result["state"].as_str(),
        Some("previewed" | "verified_for_named_checks" | "applied_verified_patch")
    );
    if !positive && result["attempt_artifacts"].is_null() {
        match recover_attempt_artifacts(
            &plan,
            &broker,
            &request_file,
            &request,
            &attempt,
            &outcome,
            &result["container_cleanup"],
        ) {
            Ok((artifacts, recovery)) => {
                result["attempt_artifacts"] = artifacts;
                result["artifact_recovery"] = recovery;
            }
            Err(error) => {
                result["artifact_recovery"] =
                    json!({"capture_only":true,"complete":false,"error":error.to_string()});
                result["attempt_artifacts"] = json!({"schema":"thruline.native-repair-attempt-artifacts.v1", "complete":false,
                    "roots":[], "artifacts":[], "gaps":[{"path":attempt,"reason":format!("Native capture recovery unavailable: {error}")}]});
            }
        }
    }
    if let Err(error) = seal_native_artifacts(db, &root, &store, &mut result) {
        result["native_attachment_seal"] = json!({"complete":false,"error":error.to_string()});
        if positive {
            result["native_state"] = result["state"].clone();
            result["state"] = json!("capture_failed");
            result["error"] = json!(format!("Native attachment could not be sealed: {error}"));
        }
    }
    let (attempt_logs, log_gaps) = capture_attempt_logs(db, &attempt);
    if !log_gaps.is_empty() {
        result["attempt_log_gaps"] = json!(log_gaps);
        result["native_attachment_seal"]["complete"] = json!(false);
        if positive {
            if result["native_state"].is_null() {
                result["native_state"] = result["state"].clone();
            }
            result["state"] = json!("capture_failed");
            result["error"] = json!("Attempt log capture is incomplete; inspect explicit gaps");
        }
    }
    let transport_bytes = serde_json::to_vec(&result)?;
    let artifact = private_write(&artifact_dir(db), &transport_bytes)?;
    binding.artifacts.push(ArtifactDigest {
        path: artifact.to_string_lossy().into_owned(),
        sha256: Some(sha256_hex(&transport_bytes)),
    });
    binding.artifacts.push(ArtifactDigest {
        path: oracle_artifact.to_string_lossy().into_owned(),
        sha256: Some(oracle_sha),
    });
    // Known logs remain opaque; only the primary protocol receipt expands
    // nested references. Unsafe or oversized logs are explicit receipt gaps.
    binding.artifacts.extend(attempt_logs);
    binding.post = wsid::observe(&root, Scope::everything())
        .map(|p| IdentityEdge::from_observation(&p))
        .unwrap_or_else(|error| IdentityEdge::incomplete(error.to_string()));
    binding.exit_code = outcome.exit_code.map(i64::from);
    binding.term_signal = outcome.signal.map(i64::from);
    binding.stdout = outcome.stdout.capture();
    binding.stderr = outcome.stderr.capture();
    binding.bound_at = thruline_core::ingest::iso8601_utc_now();
    if outcome.state != RunState::Completed {
        binding
            .wrapper_defects
            .push(format!("repair worker: {:?}", outcome.state));
    }
    let stored = evb::store(&ledger, &binding)?;
    let (_, event) = evb::record_command_bound(&ledger, stored.stored())?;
    result["binding_id"] = json!(stored.stored().id);
    result["evidence_event"] = json!(event);
    result["artifact"] = json!(artifact);
    fs::write(attempt.join("transport.json"), serde_json::to_vec(&result)?)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumer_runtime_change_is_part_of_repair_identity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let consumer = temp.path().join("consumer");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&consumer).unwrap();
        let root = root.canonicalize().unwrap();
        let executable = temp.path().join("scanner");
        fs::write(&executable, b"scanner bytes").unwrap();
        fs::write(consumer.join("node"), b"original runtime").unwrap();
        let runtime = json!({"executable":executable,"runtime":[]});
        let selection = json!({"guided":{"consumer_runtime":[consumer]}});
        let paths = repair_runtime_paths(&root, "osv-scanner", &runtime, &selection).unwrap();
        let before = input_read::hash_runtime(&root, &paths).unwrap();
        assert_eq!(
            before,
            input_read::hash_runtime(
                &root,
                &repair_runtime_paths(&root, "osv-scanner", &runtime, &selection).unwrap()
            )
            .unwrap()
        );
        fs::write(consumer.join("node"), b"changed runtime").unwrap();
        assert_ne!(
            before,
            input_read::hash_runtime(
                &root,
                &repair_runtime_paths(&root, "osv-scanner", &runtime, &selection).unwrap()
            )
            .unwrap()
        );
        for invalid in [json!([]), json!([root]), json!(["relative-consumer"])] {
            assert!(repair_runtime_paths(
                &root,
                "osv-scanner",
                &runtime,
                &json!({"guided":{"consumer_runtime":invalid}})
            )
            .is_err());
        }
        #[cfg(unix)]
        {
            let alias = temp.path().join("aliased-consumer");
            std::os::unix::fs::symlink(&root, &alias).unwrap();
            assert!(repair_runtime_paths(
                &root,
                "osv-scanner",
                &runtime,
                &json!({"guided":{"consumer_runtime":[alias]}})
            )
            .is_err());
        }
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf, Value) {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("ledger.db");
        fs::create_dir_all(artifact_dir(&db)).unwrap();
        let root = temp.path().join("selected-project");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let store = temp.path().join("repair-store");
        let attempt = store.join("attempts/preview-owned");
        fs::create_dir_all(attempt.join("native")).unwrap();
        fs::write(
            attempt.join("request.json"),
            serde_json::to_vec(&json!({"root":root})).unwrap(),
        )
        .unwrap();
        let native = attempt.join("native/stdout.jsonl");
        fs::write(&native, b"captured compiler bytes\n").unwrap();
        let bytes = fs::read(&native).unwrap();
        let result = json!({"original_detector":{"native":{"schema":"thruline.clippy-capture.v1","artifacts":[{"path":native.canonicalize().unwrap(),"bytes":bytes.len(),"sha256":sha256_hex(&bytes)}]}}});
        (temp, db, root, store.canonicalize().unwrap(), result)
    }

    #[test]
    fn sealed_native_bytes_survive_original_attempt_mutation() {
        let (_temp, db, root, store, mut result) = fixture();
        let source = PathBuf::from(
            result["original_detector"]["native"]["artifacts"][0]["path"]
                .as_str()
                .unwrap(),
        );
        seal_native_artifacts(&db, &root, &store, &mut result).unwrap();
        let row = &result["original_detector"]["native"]["artifacts"][0];
        assert_eq!(row["original_path"], json!(source));
        let saved = PathBuf::from(row["path"].as_str().unwrap());
        assert_eq!(
            saved.parent().unwrap(),
            artifact_dir(&db).canonicalize().unwrap()
        );
        fs::write(source, b"changed after native capture").unwrap();
        assert_eq!(fs::read(saved).unwrap(), b"captured compiler bytes\n");
    }

    #[test]
    fn failed_attempt_seals_raw_bytes_without_promoting_native_state() {
        let (_temp, db, root, store, fixture) = fixture();
        let row = fixture["original_detector"]["native"]["artifacts"][0].clone();
        let source = PathBuf::from(row["path"].as_str().unwrap());
        let mut result = json!({"schema":"thruline.native-repair.v1", "state":"failed",
            "error":"Native metadata was unavailable", "attempt_artifacts":{
            "schema":"thruline.native-repair-attempt-artifacts.v1", "complete":true,
            "roots":[], "gaps":[], "artifacts":[row]}});
        seal_native_artifacts(&db, &root, &store, &mut result).unwrap();
        fs::write(&source, b"changed failed-attempt staging bytes").unwrap();
        let saved = &result["attempt_artifacts"]["artifacts"][0];
        assert_eq!(saved["original_path"], json!(source));
        assert_eq!(
            fs::read(saved["path"].as_str().unwrap()).unwrap(),
            b"captured compiler bytes\n"
        );
        assert_eq!(result["state"], "failed");
        assert_eq!(result["error"], "Native metadata was unavailable");
        assert_eq!(result["native_attachment_seal"]["complete"], true);
    }

    #[test]
    fn incomplete_attempt_retains_available_bytes_and_explicit_gap() {
        let (_temp, db, root, store, fixture) = fixture();
        let mut result = json!({"state":"failed", "attempt_artifacts":{
            "schema":"thruline.native-repair-attempt-artifacts.v1", "complete":false,
            "roots":[], "gaps":[{"reason":"artifact byte budget exceeded"}],
            "artifacts":fixture["original_detector"]["native"]["artifacts"]}});
        seal_native_artifacts(&db, &root, &store, &mut result).unwrap();
        assert_eq!(result["native_attachment_seal"]["references"], 1);
        assert_eq!(result["native_attachment_seal"]["complete"], false);
        assert_eq!(result["state"], "failed");
        assert_eq!(
            result["attempt_artifacts"]["gaps"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    #[cfg(unix)]
    fn unsafe_opaque_logs_are_gaps_without_losing_safe_failure_bytes() {
        let (temp, db, _root, store, _) = fixture();
        let attempt = store.join("attempts/preview-owned");
        let outside = temp.path().join("controlled-secret");
        fs::write(&outside, b"must not be captured").unwrap();
        std::os::unix::fs::symlink(&outside, attempt.join("alias.stdout")).unwrap();
        fs::File::create(attempt.join("oversized.stderr"))
            .unwrap()
            .set_len(8 * 1024 * 1024 + 1)
            .unwrap();
        fs::write(attempt.join("safe.stdout"), b"actual safe failure bytes\n").unwrap();
        let (artifacts, gaps) = capture_attempt_logs(&db, &attempt);
        assert_eq!(gaps.len(), 2);
        let bytes: Vec<_> = artifacts
            .iter()
            .map(|row| fs::read(&row.path).unwrap())
            .collect();
        assert!(bytes.iter().any(|b| b == b"actual safe failure bytes\n"));
        assert!(!bytes.iter().any(|b| b == b"must not be captured"));
    }

    #[test]
    fn semgrep_capture_seals_actual_bytes_and_refuses_changed_capture() {
        let (_temp, db, root, store, mut result) = fixture();
        result["original_detector"]["native"]["schema"] =
            json!("thruline.semgrep-repair-capture.v1");
        let mut corrupted = result.clone();
        corrupted["original_detector"]["native"]["artifacts"][0]["sha256"] = json!("ff".repeat(32));
        assert!(seal_native_artifacts(&db, &root, &store, &mut corrupted).is_err());
        assert_eq!(fs::read_dir(artifact_dir(&db)).unwrap().count(), 0);
        seal_native_artifacts(&db, &root, &store, &mut result).unwrap();
        let record = &result["original_detector"]["native"]["artifacts"][0];
        let original = record["original_path"].as_str().unwrap();
        fs::write(original, b"changed original Semgrep process artifact").unwrap();
        assert_eq!(
            fs::read(record["path"].as_str().unwrap()).unwrap(),
            b"captured compiler bytes\n"
        );
    }

    #[test]
    fn opengrep_capture_seals_actual_bytes_and_refuses_changed_capture() {
        let (_temp, db, root, store, mut result) = fixture();
        result["original_detector"]["native"]["schema"] =
            json!("thruline.opengrep-repair-capture.v1");
        let mut corrupted = result.clone();
        corrupted["original_detector"]["native"]["artifacts"][0]["sha256"] = json!("ff".repeat(32));
        assert!(seal_native_artifacts(&db, &root, &store, &mut corrupted).is_err());
        assert_eq!(fs::read_dir(artifact_dir(&db)).unwrap().count(), 0);
        seal_native_artifacts(&db, &root, &store, &mut result).unwrap();
        let record = &result["original_detector"]["native"]["artifacts"][0];
        let original = record["original_path"].as_str().unwrap();
        fs::write(original, b"changed original Opengrep process artifact").unwrap();
        assert_eq!(
            fs::read(record["path"].as_str().unwrap()).unwrap(),
            b"captured compiler bytes\n"
        );
    }

    #[test]
    fn another_root_or_changed_hash_cannot_enter_native_export() {
        let (_temp, db, root, store, result) = fixture();
        let mut foreign = result.clone();
        fs::write(
            store.join("attempts/preview-owned/request.json"),
            br#"{"root":"/another-project"}"#,
        )
        .unwrap();
        assert!(seal_native_artifacts(&db, &root, &store, &mut foreign)
            .unwrap_err()
            .to_string()
            .contains("another source root"));
        fs::write(
            store.join("attempts/preview-owned/request.json"),
            serde_json::to_vec(&json!({"root":root})).unwrap(),
        )
        .unwrap();
        let mut tampered = result;
        tampered["original_detector"]["native"]["artifacts"][0]["sha256"] = json!("00".repeat(32));
        assert!(seal_native_artifacts(&db, &root, &store, &mut tampered)
            .unwrap_err()
            .to_string()
            .contains("captured identity"));
        assert_eq!(fs::read_dir(artifact_dir(&db)).unwrap().count(), 0);
    }

    #[test]
    #[cfg(unix)]
    fn symlink_native_attachment_is_not_followed() {
        let (_temp, db, root, store, mut result) = fixture();
        let source = PathBuf::from(
            result["original_detector"]["native"]["artifacts"][0]["path"]
                .as_str()
                .unwrap(),
        );
        let actual = source.with_extension("actual");
        fs::rename(&source, &actual).unwrap();
        std::os::unix::fs::symlink(actual, source).unwrap();
        assert!(seal_native_artifacts(&db, &root, &store, &mut result)
            .unwrap_err()
            .to_string()
            .contains("symlink"));
        assert_eq!(fs::read_dir(artifact_dir(&db)).unwrap().count(), 0);
    }
}
