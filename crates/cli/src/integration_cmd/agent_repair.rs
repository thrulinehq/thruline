//! Native local-agent proposals and persisted continuation use bounded workers.
//! Repair state is an artifact, never an inferred correctness verdict.
use super::*;

const BROKER: &str = include_str!("../../../../scripts/integrations/local-agents/agent-repair.py");

fn captured(db: &Path, root: &Path, recipe: &str) -> Result<Vec<Value>> {
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
            v.binding.source_adapter == format!("thruline-agent-repair:{recipe}")
                && Path::new(&v.binding.root) == root
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
            value["binding_id"] = json!(saved.id);
            value["artifact"] = json!(path);
            rows.push(value);
        }
    }
    Ok(rows)
}

pub(crate) fn action(
    db: &Path,
    root: &Path,
    recipe: &str,
    action: &str,
    digest: &str,
) -> Result<Value> {
    // AI-REVIEW-OUT-OF-LAUNCH-R5: the withheld review is not a repair agent
    // this build runs. Before any store is made or attempt recorded.
    anyhow::ensure!(
        !super::withheld(recipe),
        "{}",
        super::AI_REVIEW_NOT_IN_BUILD
    );
    anyhow::ensure!(
        ["preview", "resume", "verify", "apply", "list"].contains(&action),
        "Unknown repair action"
    );
    anyhow::ensure!(
        ["aider", "openhands-sdk", "pr-agent", "mini-swe-agent"].contains(&recipe)
            && super::shipped(recipe),
        "Unsupported local repair agent"
    );
    let root = root.canonicalize()?;
    let store = db.with_extension("integration-agent-repairs");
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
    let recorded = captured(db, &root, recipe)?;
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
                if matches!(
                    value.get("state").and_then(Value::as_str),
                    Some("unavailable" | "rejected")
                ) && value.get("root").and_then(Value::as_str) == root.to_str()
                    && value.get("recipe").and_then(Value::as_str) == Some(recipe)
                {
                    repairs.push(value);
                }
            }
        }
        return Ok(json!({"schema":"thruline.agent-repair-list.v1","repairs":repairs}));
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
        let path: PathBuf = std::env::var_os("THRULINE_AGENT_REPAIR_CONFIG")
            .context("No independent agent repair oracle is configured. Set THRULINE_AGENT_REPAIR_CONFIG to reviewed, hash-pinned protected oracles and native driver identities.")?
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
            let value = json!({"schema":"thruline.agent-repair.v1", "state":"unavailable",
                "error":error.to_string(), "attempt":attempt, "digest":digest, "root":root, "recipe":recipe});
            fs::write(attempt.join("result.json"), serde_json::to_vec(&value)?)?;
            return Ok(value);
        }
    };
    let source_capture = if ["preview", "resume"].contains(&action) {
        match native_capture(db, &root, recipe, digest) {
            Ok(capture) => Some(capture),
            Err(error) => {
                let value = json!({"schema":"thruline.agent-repair.v1", "state":"rejected",
                    "error":error.to_string(), "attempt":attempt, "digest":digest, "root":root, "recipe":recipe});
                fs::write(attempt.join("result.json"), serde_json::to_vec(&value)?)?;
                return Ok(value);
            }
        }
    } else {
        None
    };
    let broker = private_write(&artifact_dir(db), BROKER.as_bytes())?.canonicalize()?;
    let request = json!({"root":root,"store":store,"attempt":attempt,"action":action,
        "digest":digest,"recipe":recipe,"source_binding_id":digest,"capture":source_capture,"config":config,"candidate_sha256":hash_file(&std::env::current_exe()?)?, "prior_meta_sha256":prior_meta});
    let request_file = attempt.join("request.json");
    fs::write(&request_file, serde_json::to_vec(&request)?)?;
    let mut plan = Plan {
        schema: "thruline.integration-plan.v1".into(),
        pack: "agent-repair".into(),
        format: "thruline.agent-repair.v1".into(),
        support: None,
        root: root.clone(),
        argv: vec![
            config["python"]["path"].as_str().context("Python path missing")?.into(),
            "-I".into(), broker.to_string_lossy().into_owned(), request_file.to_string_lossy().into_owned(),
        ],
        container: None,
        local_model_port: if action == "resume" { Some(11435) } else { None },
        writable_paths: vec![],
        criterion: Criterion {
            id: "native-agent-proposal-repair".into(), revision: "v1".into(),
            requirement: "Inspect captured native proposals or resume a persisted local conversation; apply only after protected independent oracle checks pass".into(),
            success: "Every frozen named oracle case passes on the staged proposal, previously passing controls remain passing, and source/config/patch identities remain current".into(),
            accepted_by: "explicit local repair action with operator-configured independent oracle".into(),
            condition: engine::Condition::NoObservations,
        },
        acceptance: None, approved_setup: None, rules: vec![], runtime: vec![],
        dependencies: vec![], dataset: vec![],
        environment: BTreeMap::from([("PATH".into(), "/usr/bin:/bin".into()), ("PYTHONDONTWRITEBYTECODE".into(), "1".into())]),
        input_closure_complete: false, timeout_seconds: 700, output_limit_bytes: 8 * 1024 * 1024,
        cpu_seconds: 600, memory_mb: 6144, report: ReportSource::Stdout,
    };
    if let Some(runtime) = config.pointer(&format!("/recipes/{recipe}/container_runtime")) {
        let path = PathBuf::from(
            runtime["path"]
                .as_str()
                .context("Linux oracle runtime path missing")?,
        );
        anyhow::ensure!(
            path.is_absolute()
                && !path.starts_with(&root)
                && runtime["sha256"].as_str() == Some(hash_file(&path)?.as_str()),
            "Linux oracle runtime changed or belongs to reviewed source"
        );
        plan.environment.insert(
            "THRULINE_CONTAINER_RUNTIME".into(),
            path.display().to_string(),
        );
    }
    let pre = wsid::observe(&root, Scope::everything())?;
    let fingerprint = match &pre {
        WObservation::Complete(fp) => fp,
        _ => bail!("repair cannot bind an incomplete workspace observation"),
    };
    let started = thruline_core::ingest::iso8601_utc_now();
    let mut prepared = worker::prepare_broker(&plan)?;
    // macOS rejects nested sandbox_apply. This embedded, hash-bound filesystem
    // broker executes only fixed local Git operations. Every native-agent/oracle child
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
        source_adapter: format!("thruline-agent-repair:{recipe}"),
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
        json!({"schema":"thruline.agent-repair.v1", "state":if outcome.state == RunState::Completed { "failed".to_string() } else { format!("{:?}",outcome.state).to_lowercase() },
            "error":"Repair did not finish within its execution/capture contract. Original and attempted artifacts remain available.",
            "diagnostics":thruline_core::redact::mask_recorded(&String::from_utf8_lossy(&outcome.stderr.bytes)).0,
            "attempt":attempt,"digest":digest})
    };
    result["transport"] = json!({"run_state":outcome.state,"exit_code":outcome.exit_code,
        "duration_ms":outcome.duration_ms,"sampled_peak_rss_bytes":outcome.sampled_peak_rss_bytes,
        "resource_error":outcome.resource_error,"candidate_sha256":request["candidate_sha256"],
        "broker_sha256":sha256_hex(BROKER.as_bytes()),"attempt":attempt});
    result["root"] = json!(root);
    result["recipe"] = json!(recipe);
    let managed_refs = retain_protocol_artifacts(db, &store, &mut result);
    if let Err(error) = &managed_refs {
        result["state"] = json!(artifact_failure_state(result["state"].as_str()));
        result["artifact_capture_error"] = json!(error.to_string());
        binding
            .wrapper_defects
            .push(format!("agent repair artifact capture: {error}"));
    }
    let transport_bytes = serde_json::to_vec(&result)?;
    let artifact = private_write(&artifact_dir(db), &transport_bytes)?;
    binding.artifacts.push(ArtifactDigest {
        path: artifact.to_string_lossy().into_owned(),
        sha256: Some(sha256_hex(&transport_bytes)),
    });
    if let Ok(references) = managed_refs {
        binding.artifacts.extend(references);
    }
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

fn native_capture(db: &Path, root: &Path, recipe: &str, binding_id: &str) -> Result<Value> {
    let rows = results(db, root)?;
    let row = rows["runs"]
        .as_array()
        .context("integration receipts unavailable")?
        .iter()
        .find(|row| row["binding_id"].as_str() == Some(binding_id))
        .context("Select a captured integration receipt for this project")?;
    anyhow::ensure!(
        row.pointer("/execution_receipt/root")
            .and_then(Value::as_str)
            == root.to_str(),
        "receipt belongs to another project root"
    );
    anyhow::ensure!(
        row.pointer("/record/plan/root").and_then(Value::as_str) == root.to_str(),
        "record plan belongs to another project root"
    );
    anyhow::ensure!(
        row.pointer("/assessment/state").and_then(Value::as_str) != Some("stale")
            && row
                .pointer("/assessment/changed_inputs")
                .and_then(Value::as_array)
                .is_some_and(|rows| rows.is_empty()),
        "native receipt inputs changed; run a fresh native check"
    );
    anyhow::ensure!(
        row.pointer("/record/admission/captured")
            .and_then(Value::as_bool)
            == Some(true),
        "imported or uncaptured reports cannot authorize repair"
    );
    anyhow::ensure!(
        row.pointer("/record/admission/before").is_some()
            && row.pointer("/record/admission/before") == row.pointer("/record/admission/after"),
        "native receipt inputs changed during execution; run a fresh native check"
    );
    let repository = match recipe {
        "aider" => "R12",
        "openhands-sdk" => "R13",
        "pr-agent" => "R14",
        "mini-swe-agent" => "R15",
        _ => bail!("unknown agent"),
    };
    for artifact in row
        .pointer("/execution_receipt/artifacts")
        .and_then(Value::as_array)
        .context("native artifacts missing")?
    {
        let path = PathBuf::from(artifact["path"].as_str().context("artifact path missing")?);
        anyhow::ensure!(
            path.canonicalize()?.parent() == Some(artifact_dir(db).canonicalize()?.as_path()),
            "native artifact escapes this product store"
        );
        let bytes = bounded_file(&path, 8 * 1024 * 1024)?;
        let hash = sha256_hex(&bytes);
        anyhow::ensure!(
            artifact["sha256"].as_str() == Some(hash.as_str()),
            "native artifact hash mismatch"
        );
        let value = engine::parse_json(&bytes)?;
        if value["schema"].as_str() == Some("thruline.local-agent-capture.v1") {
            anyhow::ensure!(
                value["repository_id"].as_str() == Some(repository)
                    && value.pointer("/source/root").and_then(Value::as_str) == root.to_str(),
                "native capture provider/root mismatch"
            );
            return Ok(json!({"path":path,"sha256":hash,"size":bytes.len()}));
        }
    }
    bail!("No retained native capture was found for this receipt")
}

/// Only producer-defined protocol locations authorize artifact reads. Model prose
/// is never traversed for paths. Internal proposal metadata remains unchanged;
/// the immutable published EVB contains independently hashed managed copies.
fn retain_protocol_artifacts(
    db: &Path,
    store: &Path,
    value: &mut Value,
) -> Result<Vec<ArtifactDigest>> {
    let managed = artifact_dir(db).canonicalize()?;
    let mut pointers: Vec<String> = [
        "/capture",
        "/proposal",
        "/native_suggestion/response",
        "/resume/invocation",
        "/resume/events",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    for side in ["before", "after"] {
        for name in ["stdout", "stderr", "sandbox_profile", "namespace_command"] {
            pointers.push(format!("/verification/{side}/command/{name}"));
        }
    }
    for prefix in [
        "/resume/persistence_sources",
        "/resume/native_artifacts",
        "/action_artifacts",
    ] {
        if let Some(items) = value.pointer(prefix).and_then(Value::as_array) {
            anyhow::ensure!(
                items.len() <= 4096,
                "repair artifact count exceeds capture bound"
            );
            pointers.extend((0..items.len()).map(|index| format!("{prefix}/{index}")));
        }
    }
    let mut retained = BTreeMap::new();
    let mut total = 0usize;
    for pointer in pointers {
        let Some(item) = value.pointer_mut(&pointer) else {
            continue;
        };
        if item.is_null() {
            continue;
        }
        let original = PathBuf::from(
            item["path"]
                .as_str()
                .context("repair artifact path missing")?,
        );
        anyhow::ensure!(
            original.is_absolute()
                && original
                    .ancestors()
                    .all(|path| !fs::symlink_metadata(path)
                        .is_ok_and(|m| m.file_type().is_symlink())),
            "symlink or relative repair artifact refused"
        );
        let path = original.canonicalize()?;
        anyhow::ensure!(
            path.starts_with(store) || path.starts_with(&managed),
            "repair artifact is outside owned proposal/native stores"
        );
        let bytes = bounded_file(&path, 16 * 1024 * 1024)?;
        let digest = sha256_hex(&bytes);
        anyhow::ensure!(
            item["sha256"].as_str() == Some(digest.as_str())
                && item["size"].as_u64() == Some(bytes.len() as u64),
            "repair artifact bytes differ from their protocol binding"
        );
        if !retained.contains_key(&digest) {
            total += bytes.len();
            anyhow::ensure!(
                total <= 128 * 1024 * 1024,
                "repair artifact closure exceeds capture bound"
            );
            let saved = private_write(&managed, &bytes)?;
            retained.insert(
                digest.clone(),
                ArtifactDigest {
                    path: saved.to_string_lossy().into_owned(),
                    sha256: Some(digest.clone()),
                },
            );
        }
        item["original_path"] = json!(original);
        item["path"] = json!(retained[&digest].path);
    }
    Ok(retained.into_values().collect())
}

/// The state an attempt ends in when its artifacts could not be retained. The
/// worker's own deadline stays the reason: stopping the action at the
/// deadline is what leaves its artifacts partial, so that is not relabelled
/// `capture_failed` (T-4-1 fix 1, R04, `Q11`). The failure itself stays on the
/// record as `artifact_capture_error`.
fn artifact_failure_state(current: Option<&str>) -> &'static str {
    match current {
        Some("timedout") => "timedout",
        _ => "capture_failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_deadline_is_not_relabelled_by_the_artifact_failure_it_caused() {
        assert_eq!(artifact_failure_state(Some("timedout")), "timedout");
        for other in [Some("previewed"), Some("rejected"), Some("failed"), None] {
            assert_eq!(artifact_failure_state(other), "capture_failed");
        }
    }
}
