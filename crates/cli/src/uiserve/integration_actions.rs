//! Explicit same-origin actions for built-in recipes. No arbitrary browser commands.
use super::*;
use std::process::{Command, Stdio};
static RUNNING: Mutex<Option<(String, u32)>> = Mutex::new(None);
pub(super) fn cancel_for_shutdown() -> bool {
    if let Ok(running) = RUNNING.lock() {
        if let Some((_, pid)) = running.as_ref() {
            signal(*pid);
            return true;
        }
    }
    false
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    session: String,
    action: String,
    recipe: String,
    #[serde(default)]
    digest: String,
}
/// The actions this route admits: its whole table, so that the door census
/// (`tests` below, and `tests/integration_door_census.rs`) enumerates the route
/// rather than listing it by hand.
pub(super) const ACTIONS: [&str; 23] = [
    "preview",
    "accept",
    "run",
    "cancel",
    "repair-preview",
    "repair-verify",
    "repair-apply",
    "repair-list",
    "review-status",
    "review-preview",
    "review-accept",
    "review-step",
    "review-retry",
    "review-export",
    "native-repair-preview",
    "native-repair-verify",
    "native-repair-apply",
    "native-repair-list",
    "agent-repair-preview",
    "agent-repair-resume",
    "agent-repair-verify",
    "agent-repair-apply",
    "agent-repair-list",
];
fn session_root(state: &State, session: &str) -> Result<PathBuf> {
    if let Some((root, _)) = review_projects::resolve(state, session)? {
        return Ok(root);
    }
    let store = thruline_ledger::Ledger::open_read_only(&thruline_ledger::Ledger::default_path()?)?;
    let root: String = store
        .conn()
        .query_row(
            "SELECT r.root_path FROM sessions s JOIN repos r ON r.id=s.repo_id WHERE s.id=?1",
            [session],
            |row| row.get(0),
        )
        .context("This session has no recorded project path.")?;
    Ok(PathBuf::from(root).canonicalize()?)
}
fn signal(pid: u32) {
    let _ = Command::new("/bin/kill")
        .args(["-TERM", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
struct Job(std::process::Child);
impl Drop for Job {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            signal(self.0.id());
            for _ in 0..100 {
                if self.0.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
        if let Ok(mut running) = RUNNING.lock() {
            *running = None;
        }
    }
}
fn execute(state: &State, root: &Path, request: &Action) -> Result<serde_json::Value> {
    let mut running = RUNNING
        .lock()
        .map_err(|_| anyhow::anyhow!("worker state unavailable"))?;
    anyhow::ensure!(
        !state.shutting_down.load(Ordering::SeqCst),
        "workspace is shutting down"
    );
    anyhow::ensure!(
        running.is_none(),
        "A check is already running. Wait for its receipt or cancel that check."
    );
    let mut command = Command::new(&state.cli_exe);
    command.arg("--db").arg(&state.db);
    if let Some(action) = request.action.strip_prefix("agent-repair-") {
        command
            .args(["integration", "agent-repair", "--action", action, "--root"])
            .arg(root)
            .args(["--recipe", &request.recipe, "--digest", &request.digest]);
    } else if let Some(action) = request.action.strip_prefix("native-repair-") {
        command
            .args(["integration", "native-repair", "--action", action, "--root"])
            .arg(root)
            .args(["--recipe", &request.recipe, "--digest", &request.digest]);
    } else if let Some(action) = request.action.strip_prefix("repair-") {
        anyhow::ensure!(
            request.recipe == "ruff",
            "Only the supported Ruff remedy is available"
        );
        command
            .args(["integration", "repair", "--action", action, "--root"])
            .arg(root)
            .args(["--digest", &request.digest]);
    } else if request.action == "review-step" {
        command
            .args(["integration", "review", "--action", "step", "--root"])
            .arg(root)
            .args(["--mode", &request.recipe, "--digest", &request.digest]);
    } else {
        command
            .args(["integration", "workflow", "--action", "run", "--root"])
            .arg(root)
            .args(["--recipe", &request.recipe, "--digest", &request.digest]);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if !request.action.contains("repair-") {
        if let Some((_, exec_db)) = review_projects::resolve(state, &request.session)? {
            command.arg("--exec-db").arg(exec_db);
        }
    }
    let mut child = command.spawn()?;
    let out = child.stdout.take().context("missing check output")?;
    let err = child.stderr.take().context("missing check diagnostics")?;
    *running = Some((request.session.clone(), child.id()));
    drop(running);
    let mut job = Job(child);
    let (tx, rx) = std::sync::mpsc::channel();
    for (label, src) in [
        (true, Box::new(out) as Box<dyn Read + Send>),
        (false, Box::new(err) as Box<dyn Read + Send>),
    ] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let r = src
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = tx.send((label, r));
        });
    }
    drop(tx);
    let transport_seconds = if request.action == "review-step"
        || request.action.contains("repair-")
        || (["aider", "openhands-sdk", "pr-agent", "mini-swe-agent"]
            .contains(&request.recipe.as_str())
            && request.action == "run")
    {
        720
    } else {
        360
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(transport_seconds);
    let mut stdout = vec![];
    let mut stderr = vec![];
    let mut closed = 0;
    loop {
        while let Ok((label, result)) = rx.try_recv() {
            let bytes = result?;
            anyhow::ensure!(
                bytes.len() <= 8 * 1024 * 1024,
                "Check response exceeded its transport budget; inspect stored receipts."
            );
            if label {
                stdout = bytes;
            } else {
                stderr = bytes;
            }
            closed += 1;
        }
        if let Some(status) = job.0.try_wait()? {
            if closed == 2 {
                anyhow::ensure!(
                    status.success(),
                    "{}",
                    bounded_stderr(&String::from_utf8_lossy(&stderr))
                );
                return thruline_verify::integrations::parse_json(&stdout)
                    .map_err(|e| anyhow::anyhow!(e));
            }
        }
        anyhow::ensure!(std::time::Instant::now()<deadline,"Check transport timed out. Cancellation was requested; refresh receipts before retrying.");
        std::thread::sleep(Duration::from_millis(20));
    }
}
pub(super) fn handle(state: &State, body: &[u8], stream: &mut TcpStream) -> Result<()> {
    if state.shutting_down.load(Ordering::SeqCst) {
        return respond(
            stream,
            503,
            "application/json",
            br#"{"error":"workspace_is_shutting_down"}"#,
        );
    }
    let request: Action = match (|| -> Result<Action> {
        Ok(serde_json::from_value(
            thruline_verify::integrations::parse_json(body).map_err(anyhow::Error::msg)?,
        )?)
    })() {
        Ok(v) => v,
        Err(_) => {
            return respond(
                stream,
                400,
                "application/json",
                br#"{"error":"Invalid check request."}"#,
            )
        }
    };
    if session_param(&format!("session={}", request.session))
        .ok()
        .flatten()
        .as_deref()
        != Some(request.session.as_str())
        || !ACTIONS.contains(&request.action.as_str())
    {
        return respond(
            stream,
            400,
            "application/json",
            br#"{"error":"Unknown check action or session."}"#,
        );
    }
    // AI-REVIEW-OUT-OF-LAUNCH: the allow-list admits no PR-Agent recipe in a
    // build without AI review, whatever the action. Nothing is spawned.
    if crate::integration_cmd::withheld(&request.recipe) {
        return respond(
            stream,
            400,
            "application/json",
            &serde_json::to_vec(
                &serde_json::json!({"error": crate::integration_cmd::AI_REVIEW_NOT_IN_BUILD}),
            )?,
        );
    }
    // AI-REVIEW-OUT-OF-LAUNCH-R5: and then the allow-list, for every other id.
    // Whatever the action, the route acts on an id the build ships or a review
    // mode, and answers anything else in fixed words that echo none of it, so
    // no spelling of an id is decided on here and none is spawned.
    if !crate::integration_cmd::route_recipe_admitted(&request.recipe) {
        return respond(
            stream,
            400,
            "application/json",
            &serde_json::to_vec(
                &serde_json::json!({"error": crate::integration_cmd::CHECK_NOT_RUN}),
            )?,
        );
    }
    let root = match session_root(state, &request.session) {
        Ok(root) => root,
        Err(_) => {
            return respond(
                stream,
                409,
                "application/json",
                br#"{"error":"Check action could not be completed."}"#,
            )
        }
    };
    // This must precede every action: a policy failure cannot be discovered
    // only after a check has already written a receipt or changed a file.
    let policy = match super::integration_policy_for_session(state, &request.session, Some(&root)) {
        Ok(policy) => policy,
        Err(problem) if super::error_is_store_held(&problem) => {
            return super::store_held(state, stream)
        }
        Err(_) => return super::projection_policy_unavailable(stream, &[]),
    };
    let value = (|| -> Result<serde_json::Value> {
        if request.action == "cancel" {
            let running = RUNNING
                .lock()
                .map_err(|_| anyhow::anyhow!("worker state unavailable"))?;
            let (session, pid) = running
                .as_ref()
                .context("No check started by this HQ is running.")?;
            anyhow::ensure!(
                session == &request.session,
                "This session does not own the running check."
            );
            signal(*pid);
            return Ok(
                serde_json::json!({"cancellation_requested":true,"next":"Wait for the cancellation receipt. The agent session itself is unaffected."}),
            );
        }
        if request.action == "run"
            || request.action == "review-step"
            || request.action.contains("repair-")
        {
            execute(state, &root, &request)
        } else if let Some(action) = request.action.strip_prefix("review-") {
            let (_, exec_db) = review_projects::resolve(state, &request.session)?
                .context("Select a registered project for repository review")?;
            crate::integration_cmd::review::action(
                &state.db,
                &root,
                &exec_db,
                action,
                &request.recipe,
                &request.digest,
                crate::integration_cmd::review::BindingRequest::from_parts(None, None)?,
            )
        } else if let Some((_, exec_db)) = review_projects::resolve(state, &request.session)? {
            crate::integration_cmd::workflow::action_at(
                &state.db,
                &root,
                &request.action,
                &request.recipe,
                &request.digest,
                &exec_db,
            )
        } else {
            crate::integration_cmd::workflow::action(
                &state.db,
                &root,
                &request.action,
                &request.recipe,
                &request.digest,
            )
        }
    })();
    match value {
        Ok(mut value) => {
            if super::present_integration_action(&mut value, &policy).is_err() {
                return super::projection_policy_unavailable(stream, &[]);
            }
            respond(
                stream,
                200,
                "application/json",
                &serde_json::to_vec(&value)?,
            )
        }
        Err(e) => {
            if super::error_is_store_held(&e) {
                return super::store_held(state, stream);
            }
            let body = serde_json::to_vec(&serde_json::json!({
                "error": thruline_core::redact::mask_recorded(&e.to_string()).0
            }))?;
            let served = match crate::exclusion_serve::bytes(&body, &policy) {
                Ok(served) => served,
                Err(_) => return super::projection_policy_unavailable(stream, &[]),
            };
            respond(stream, 409, "application/json", &served)
        }
    }
}

/// AI-REVIEW-OUT-OF-LAUNCH-R6 — the door census, the half that reads the
/// route's own table. `tests/support/integration_doors.json` classifies every
/// action and every field of the body; this fails on any the route admits and
/// the table does not name, and on any the table names that the route no
/// longer has. `crate::integration_cmd::door_census` does the same for the
/// command line, and `tests/integration_door_census.rs` runs the drives.
#[cfg(test)]
mod door_census {
    use super::*;
    use std::collections::BTreeSet;

    const TABLE: &str = include_str!("../../tests/support/integration_doors.json");

    /// The fields of the body, from serde's own account of the struct: an
    /// unknown field is refused by listing the ones it expects.
    fn body_fields() -> BTreeSet<String> {
        let refused = match serde_json::from_str::<Action>(r#"{"zz":1}"#) {
            Ok(_) => panic!("an unknown field is refused"),
            Err(error) => error.to_string(),
        };
        let expected = refused
            .split("expected one of ")
            .nth(1)
            .expect("serde lists the fields it expects");
        expected
            .split('`')
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, name)| name.to_owned())
            .collect()
    }

    #[test]
    fn every_action_and_every_body_field_of_the_route_is_classified() {
        let table: serde_json::Value = serde_json::from_str(TABLE).expect("table json");
        let route = &table["route"];
        let listed: Vec<&str> = route["actions"]
            .as_array()
            .expect("actions")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        let mut problems = Vec::new();
        for action in ACTIONS {
            if !listed.contains(&action) {
                problems.push(format!(
                    "POST /api/integrations/action admits `{action}`, which is not in the door census: \
                     classify it in crates/cli/tests/support/integration_doors.json"
                ));
            }
        }
        for action in &listed {
            if !ACTIONS.contains(action) {
                problems.push(format!(
                    "the door census names the route action `{action}`, which the route no longer admits"
                ));
            }
        }
        let unique: BTreeSet<&&str> = listed.iter().collect();
        if unique.len() != listed.len() {
            problems.push("the door census lists a route action twice".into());
        }

        let fields = body_fields();
        let classified: BTreeSet<String> = route["body"]
            .as_object()
            .expect("body")
            .keys()
            .cloned()
            .collect();
        if fields != classified {
            problems.push(format!(
                "the route's body has the fields {fields:?}; the door census classifies {classified:?}"
            ));
        }
        // The recipe is the id door, and a drive reaches it at every action.
        if route["body"]["recipe"] != "id" {
            problems.push("the body's `recipe` is classified as an id".into());
        }
        let drives = route["drives"].as_array().expect("drives");
        if !drives
            .iter()
            .any(|d| d["via"] == "recipe" && d["every_action"] == true)
        {
            problems.push("no drive hands `recipe` an id at every action".into());
        }
        for drive in drives {
            let via = drive["via"].as_str().unwrap_or_default();
            if !fields.contains(via) {
                problems.push(format!(
                    "a route drive is `via` {via:?}, which is no body field"
                ));
            }
        }
        for (field, class) in route["body"].as_object().expect("body") {
            if ["id", "word", "other_id"].contains(&class.as_str().unwrap_or_default())
                && !drives.iter().any(|d| d["via"] == field.as_str())
            {
                problems.push(format!(
                    "the body field `{field}` is classified {class} and no drive hands it an id"
                ));
            }
        }
        assert!(
            problems.is_empty(),
            "the door census is out of step with the route:\n  {}",
            problems.join("\n  ")
        );
    }
}
