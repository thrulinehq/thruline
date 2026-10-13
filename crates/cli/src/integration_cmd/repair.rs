//! Explicit Ruff remedies use the bounded worker and existing command bindings.
//! Repair state is an artifact, never an inferred correctness verdict.
use super::*;

const BROKER: &str = include_str!("../../../../scripts/integrations/ruff-repair.py");

// A read-only display summary of the external setup. The broker revalidates
// the full pinned configuration before every execution; this does not authorize it.
pub(super) fn configuration_summary(root: &Path) -> Value {
    (|| -> Result<Value> {
        let path = PathBuf::from(
            std::env::var_os("THRULINE_REPAIR_CONFIG").context("repair setup missing")?,
        )
        .canonicalize()?;
        anyhow::ensure!(
            path.is_absolute() && !path.starts_with(root),
            "repair setup must be external"
        );
        let value = engine::parse_json(&bounded_file(&path, 65536)?)?;
        let mode = match value["schema"].as_str() {
            Some("thruline.ruff-repair-config.v1") => "legacy-f401",
            Some("thruline.ruff-repair-config.v2") => value["mode"]
                .as_str()
                .filter(|s| ["imports", "format", "notebook-format"].contains(s))
                .context("unknown selected mode")?,
            _ => bail!("unknown repair setup"),
        };
        Ok(json!({"mode":mode,"files":value.get("files"),"setup_only":true}))
    })()
    .unwrap_or(Value::Null)
}

fn captured(db: &Path, root: &Path) -> Result<Vec<Value>> {
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
            v.binding.source_adapter == "thruline-repair:ruff" && Path::new(&v.binding.root) == root
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
                value["root"] == json!(root),
                "Repair receipt belongs to a different project root"
            );
            value["binding_id"] = json!(saved.id);
            value["artifact"] = json!(path);
            rows.push(value);
        }
    }
    Ok(rows)
}

pub(crate) fn action(db: &Path, root: &Path, action: &str, digest: &str) -> Result<Value> {
    anyhow::ensure!(
        ["preview", "verify", "apply", "list"].contains(&action),
        "Unknown repair action"
    );
    let root = root.canonicalize()?;
    let store = db.with_extension("integration-repairs");
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
    let recorded = captured(db, &root)?;
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
        return Ok(json!({"schema":"thruline.ruff-repair-list.v1","repairs":repairs}));
    }
    let prior_meta = recorded
        .iter()
        .filter(|v| v.get("digest").and_then(Value::as_str) == Some(digest))
        .find_map(|v| v.get("meta_sha256").and_then(Value::as_str));
    let attempt = tempfile::Builder::new()
        .prefix(&format!("{action}-"))
        .tempdir_in(&attempts)?
        .keep();
    let configured = (|| -> Result<Value> {
        let path: PathBuf = std::env::var_os("THRULINE_REPAIR_CONFIG")
            .context("No independent behavior oracle is configured. Set THRULINE_REPAIR_CONFIG to a reviewed, hash-pinned external oracle and Ruff runtime configuration.")?
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
        Ok(config)
    })();
    let config = match configured {
        Ok(config) => config,
        Err(error) => {
            let value = json!({"schema":"thruline.ruff-repair.v1", "state":"unavailable",
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
        pack: "ruff-repair".into(),
        format: "thruline.ruff-repair.v1".into(),
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
            id: if config["schema"]=="thruline.ruff-repair-config.v2" { format!("ruff-selected-{}-repair",config["mode"].as_str().unwrap_or("unknown")) } else { "ruff-safe-f401-repair".into() }, revision: if config["schema"]=="thruline.ruff-repair-config.v2" { "v2".into() } else { "v1".into() },
            requirement: "Preview the explicitly configured native Ruff remedy and apply only after the unchanged independent behavior oracle passes".into(),
            success: "Named independent cases pass on base and changed stage; selected native check clears; source/config/patch identities remain current".into(),
            accepted_by: "explicit local repair action with operator-configured independent oracle".into(),
            condition: engine::Condition::NoObservations,
        },
        acceptance: None, approved_setup: None, rules: vec![], runtime: vec![],
        dependencies: vec![], dataset: vec![],
        environment: BTreeMap::from([("PATH".into(), "/usr/bin:/bin".into()), ("PYTHONDONTWRITEBYTECODE".into(), "1".into())]),
        input_closure_complete: false, timeout_seconds: 260, output_limit_bytes: 8 * 1024 * 1024,
        cpu_seconds: 120, memory_mb: 2048, report: ReportSource::Stdout,
    };
    let pre = wsid::observe(&root, Scope::everything())?;
    let fingerprint = match &pre {
        WObservation::Complete(fp) => fp,
        _ => bail!("repair cannot bind an incomplete workspace observation"),
    };
    let started = thruline_core::ingest::iso8601_utc_now();
    let mut prepared = worker::prepare_broker(&plan)?;
    // macOS rejects nested sandbox_apply. This embedded, hash-bound filesystem
    // broker executes only fixed local Git operations. Every Ruff/oracle child
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
        source_adapter: "thruline-repair:ruff".into(),
        source_session: format!("repair:{started}"),
    };
    let ledger = Ledger::open(db)?;
    let outcome = worker::execute(&plan, &mut prepared, |pid| {
        evb::record_command_started(&ledger, &binding, pid)
            .map(|_| ())
            .map_err(Into::into)
    })?;
    fs::write(attempt.join("worker.stdout"), &outcome.stdout.bytes)?;
    fs::write(attempt.join("worker.stderr"), &outcome.stderr.bytes)?;
    let mut result = if outcome.state == RunState::Completed && outcome.exit_code == Some(0) {
        engine::parse_json(&outcome.stdout.bytes).unwrap_or_else(|e| {
            json!({
            "state":"capture_failed", "error":e.to_string(), "attempt":attempt})
        })
    } else {
        json!({"schema":"thruline.ruff-repair.v1", "state":if outcome.state == RunState::Completed { "failed".to_string() } else { format!("{:?}",outcome.state).to_lowercase() },
            "error":"Repair did not finish within its execution/capture contract. Original and attempted artifacts remain available.",
            "diagnostics":thruline_core::redact::mask_recorded(&String::from_utf8_lossy(&outcome.stderr.bytes)).0,
            "attempt":attempt,"digest":digest})
    };
    result["transport"] = json!({"run_state":outcome.state,"exit_code":outcome.exit_code,
        "duration_ms":outcome.duration_ms,"sampled_peak_rss_bytes":outcome.sampled_peak_rss_bytes,
        "resource_error":outcome.resource_error,"candidate_sha256":request["candidate_sha256"],
        "broker_sha256":sha256_hex(BROKER.as_bytes()),"attempt":attempt});
    result["root"] = json!(root);
    let transport_bytes = serde_json::to_vec(&result)?;
    let artifact = private_write(&artifact_dir(db), &transport_bytes)?;
    binding.artifacts.push(ArtifactDigest {
        path: artifact.to_string_lossy().into_owned(),
        sha256: Some(sha256_hex(&transport_bytes)),
    });
    // Retain native process logs in the existing content-addressed artifact store.
    for entry in fs::read_dir(&attempt)? {
        let path = entry?.path();
        if path.is_file()
            && matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("stdout" | "stderr" | "json")
            )
        {
            let bytes = bounded_file(&path, 8 * 1024 * 1024)?;
            let saved = private_write(&artifact_dir(db), &bytes)?;
            binding.artifacts.push(ArtifactDigest {
                path: saved.to_string_lossy().into_owned(),
                sha256: Some(sha256_hex(&bytes)),
            });
        }
    }
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
