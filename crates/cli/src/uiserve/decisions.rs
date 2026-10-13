//! Local Decisions HTTP routes. Reads use stored rows; only explicit POSTs
//! append a person act or run a closed, accepted check.

use super::*;

use std::collections::VecDeque;
use std::process::Command;
use std::sync::OnceLock;

use serde_json::{json, Value};
use thruline_core::decisions::authority::bridge;
use thruline_core::decisions::authority::{self, PersonActKind};
use thruline_core::decisions::engine;
use thruline_core::decisions::gate;
use thruline_core::decisions::params::{parse_params, PARAMS_SCHEMA_VERSION};
use thruline_core::decisions::storage::{self, AcceptanceInput, RetirementInput, WaiverInput};
use thruline_core::decisions::why::proof_chain;
use thruline_core::hq::{self as core_hq, HqProject};
use thruline_core::ledger::Ledger;
use thruline_core::redact::Masked;

const SCHEMA_VERSION: u32 = 1;
const PERSON_ACT_SCHEMA_VERSION: u32 = 2;
const BODY_LIMIT: usize = 32 * 1024;
const JOB_LIMIT: usize = 64;
static JOBS: OnceLock<Mutex<VecDeque<(String, Value)>>> = OnceLock::new();
static CONFIRMATION_TOKEN: OnceLock<Option<String>> = OnceLock::new();
static NATIVE_BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareBody {
    schema_version: u32,
    action: String,
    revision: Option<u32>,
    actor: String,
    because: String,
    severity: Option<String>,
    applies_from_version: Option<String>,
    superseded_by: Option<String>,
    version_id: Option<String>,
    expires_version: Option<String>,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeSelection {
    predecessor_revision: u32,
    screen: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeReviewBody {
    schema_version: u32,
    action: String,
    act_row_id: String,
    selection: Option<NativeSelection>,
    expected_review_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptBody {
    schema_version: u32,
    event_id: String,
    revision: u32,
    actor: String,
    because: String,
    severity: String,
    applies_from_version: String,
    params: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetireBody {
    schema_version: u32,
    event_id: String,
    actor: String,
    because: String,
    superseded_by: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaiveBody {
    schema_version: u32,
    event_id: String,
    actor: String,
    because: String,
    version_id: String,
    expires_version: Option<String>,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckBody {
    schema_version: u32,
    version_id: String,
    decision_ids: Option<Vec<String>>,
    all: Option<bool>,
    replay: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InlineBody {
    schema_version: u32,
    version_id: String,
}

struct StoredWaiver {
    id: String,
    actor: String,
    reason: String,
    at: String,
    expires_version: Option<String>,
    expires_at: Option<String>,
}

pub(super) fn owns(path: &str) -> bool {
    if path == "/api/hq/decisions/check" || path == "/api/hq/decisions/confirmation-token" {
        return true;
    }
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    parts.len() >= 5 && parts[..3] == ["api", "hq", "projects"] && parts[4] == "decisions"
}

pub(super) fn handle(
    state: &Arc<State>,
    method: &str,
    path: &str,
    query: &str,
    headers: &RequestHeaders,
    reader: &mut BufReader<TcpStream>,
    stream: &mut TcpStream,
) -> Result<()> {
    if headers.malformed {
        return error(stream, 400, "malformed_request", None);
    }
    if method == "GET" && !no_body(headers) {
        return error(stream, 400, "malformed_request", None);
    }
    if path == "/api/hq/decisions/confirmation-token" {
        if method != "GET" || !query.is_empty() || !no_body(headers) {
            return error(stream, 400, "malformed_request", None);
        }
        if headers
            .sec_fetch_site
            .as_deref()
            .is_some_and(|site| !matches!(site, "same-origin" | "none"))
            || (headers.origin_count > 0 && !origin_is_ours(state, headers))
        {
            return error(stream, 403, "same_origin_required", None);
        }
        return match CONFIRMATION_TOKEN.get_or_init(super::run_route::mint_token) {
            Some(token) => json(stream, 200, &json!({"schema_version":1,"token":token})),
            None => error(stream, 503, "confirmation_token_unavailable", None),
        };
    }
    if path == "/api/hq/decisions/check" {
        if method != "GET" {
            return error(stream, 405, "method_not_allowed", None);
        }
        return job_status(query, headers, stream);
    }
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let Some(project_id) = parts.get(3).and_then(|part| decode(part)) else {
        return error(stream, 400, "malformed_request", None);
    };
    let ledger = match Ledger::open(&state.db) {
        Ok(ledger) => ledger,
        Err(_) => return error(stream, 503, "store_unavailable", None),
    };
    let project = match core_hq::find_project(&ledger, &project_id) {
        Ok(Some(project)) => project,
        Ok(None) => return error(stream, 404, "project_not_found", None),
        Err(_) => return error(stream, 503, "store_unavailable", None),
    };
    match (method, parts.as_slice()) {
        ("GET", [_, _, _, _, "decisions"]) if query.is_empty() => {
            read_list(&ledger, &project, stream)
        }
        ("GET", [_, _, _, _, "decisions", "proposals"]) if query.is_empty() => {
            read_proposals(&ledger, &project, stream)
        }
        ("GET", [_, _, _, _, "decisions", "conflicts" | "revivals"]) => {
            read_history(&ledger, &project, parts[5], query, stream)
        }
        ("GET", [_, _, _, _, "decisions", "open-threads"]) if query.is_empty() => {
            read_open_threads(&ledger, &project, stream)
        }
        ("GET", [_, _, _, _, "decisions", "defaults"]) if query.is_empty() => json(
            stream,
            200,
            &crate::decide_cmd::defaults_document(&project.id),
        ),
        ("GET", [_, _, _, _, "decisions", decision]) => {
            let Some(decision) = decode(decision) else {
                return error(stream, 400, "malformed_request", None);
            };
            read_detail(&ledger, &project, &decision, query, stream)
        }
        ("POST", [_, _, _, _, "decisions", "check"]) if query.is_empty() => {
            let Some(body) = request_body(state, headers, reader, stream)? else {
                return Ok(());
            };
            start_check(state, &ledger, &project, &body, stream)
        }
        ("POST", [_, _, _, _, "decisions", "inline"]) if query.is_empty() => {
            let Some(body) = request_body(state, headers, reader, stream)? else {
                return Ok(());
            };
            let request: InlineBody = match serde_json::from_value(body) {
                Ok(request) => request,
                Err(_) => return error(stream, 400, "invalid_inline_body", None),
            };
            let immutable = (7..=64).contains(&request.version_id.len())
                && request
                    .version_id
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit());
            if request.schema_version != SCHEMA_VERSION
                || (request.version_id != "HEAD" && !immutable)
            {
                return error(stream, 400, "invalid_inline_body", None);
            }
            match crate::decide_cmd::inline_sweep_projects(
                &state.db,
                &project_member_ids(&project),
                Path::new(&project.root),
                &request.version_id,
                false,
            ) {
                Ok(result) => json(stream, 200, &mask_value(&result)),
                Err(_) => error(stream, 503, "inline_check_unavailable", None),
            }
        }
        ("POST", [_, _, _, _, "decisions", decision, action]) if query.is_empty() => {
            let Some(decision) = decode(decision) else {
                return error(stream, 400, "malformed_request", None);
            };
            if !belongs(&ledger, &project, &decision)? {
                return error(stream, 404, "decision_not_found", None);
            }
            let Some(body) = request_body(state, headers, reader, stream)? else {
                return Ok(());
            };
            if matches!(*action, "prepare" | "review" | "native-confirm")
                && !confirmation_token_is_ours(headers)
            {
                return error(stream, 403, "confirmation_token_required", None);
            }
            match *action {
                "prepare" => prepare_native(&ledger, &project, &decision, &body, stream),
                "review" => native_review(&ledger, &project, &decision, &body, stream),
                "native-confirm" => {
                    start_native_confirmation(state, &ledger, &project, &decision, &body, stream)
                }
                "accept" => accept(&ledger, &project, &decision, &body, stream),
                "retire" => retire(&ledger, &project, &decision, &body, stream),
                "waive" => waive(&ledger, &project, &decision, &body, stream),
                _ => error(stream, 404, "no_such_route", None),
            }
        }
        (_, [_, _, _, _, "decisions"])
        | (_, [_, _, _, _, "decisions", "proposals"])
        | (_, [_, _, _, _, "decisions", "defaults"])
        | (_, [_, _, _, _, "decisions", "check"])
        | (_, [_, _, _, _, "decisions", _])
        | (_, [_, _, _, _, "decisions", _, _]) => {
            if !query.is_empty() {
                error(stream, 400, "unknown_query_parameter", None)
            } else {
                error(stream, 405, "method_not_allowed", None)
            }
        }
        _ => error(stream, 404, "no_such_route", None),
    }
}

fn json(stream: &mut TcpStream, status: u16, body: &Value) -> Result<()> {
    respond(
        stream,
        status,
        "application/json; charset=utf-8",
        &serde_json::to_vec(body)?,
    )
}

fn error(stream: &mut TcpStream, status: u16, name: &str, detail: Option<&str>) -> Result<()> {
    json(
        stream,
        status,
        &json!({"schema_version": SCHEMA_VERSION, "error": name, "detail": detail.map(|s| Masked::mask(s).into_text())}),
    )
}

fn decode(raw: &str) -> Option<String> {
    let decoded = percent_decode(raw)?;
    (!decoded.is_empty() && !decoded.contains('/') && decoded != "." && decoded != "..")
        .then_some(decoded)
}

fn no_body(headers: &RequestHeaders) -> bool {
    headers.content_lengths.is_empty()
        && headers.content_types.is_empty()
        && !headers.transfer_encoding
}

fn origin_is_ours(state: &State, headers: &RequestHeaders) -> bool {
    headers.origin_count == 1
        && (headers.origin == format!("http://{}", state.origin)
            || headers.origin == format!("http://localhost:{}", port_of(&state.origin)))
}

fn confirmation_token_is_ours(headers: &RequestHeaders) -> bool {
    let Some(Some(ours)) = CONFIRMATION_TOKEN.get() else {
        return false;
    };
    let [given] = headers.run_tokens.as_slice() else {
        return false;
    };
    given.len() == ours.len()
        && given
            .bytes()
            .zip(ours.bytes())
            .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

fn request_body(
    state: &State,
    headers: &RequestHeaders,
    reader: &mut BufReader<TcpStream>,
    stream: &mut TcpStream,
) -> Result<Option<Value>> {
    if !origin_is_ours(state, headers) {
        error(stream, 403, "same_origin_required", None)?;
        return Ok(None);
    }
    let bytes = match read_json_body(reader, headers, BODY_LIMIT) {
        Ok(bytes) => bytes,
        Err(BodyProblem::BadRequest) => {
            error(stream, 400, "malformed_request", None)?;
            return Ok(None);
        }
        Err(BodyProblem::TooLarge) => {
            error(stream, 413, "body_too_large", None)?;
            return Ok(None);
        }
    };
    let body = match serde_json::from_slice(&bytes) {
        Ok(Value::Object(map)) => Value::Object(map),
        _ => {
            error(stream, 400, "invalid_json_body", None)?;
            return Ok(None);
        }
    };
    Ok(Some(body))
}

fn members(project: &HqProject) -> impl Iterator<Item = &str> {
    project
        .members
        .iter()
        .map(|member| member.ledger_project_id.as_str())
}

fn belongs(ledger: &Ledger, project: &HqProject, decision: &str) -> Result<bool> {
    for member in members(project) {
        let found: i64 = ledger.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM decisions WHERE id=?1 AND project_id=?2)",
            [decision, member],
            |row| row.get(0),
        )?;
        if found != 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn stored_ids(ledger: &Ledger, project: &HqProject) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for member in members(project) {
        let mut stmt = ledger
            .conn()
            .prepare("SELECT id FROM decisions WHERE project_id=?1 ORDER BY id")?;
        let rows = stmt.query_map([member], |row| row.get::<_, String>(0))?;
        for row in rows {
            out.push(row?);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn accepted_ids(ledger: &Ledger, project: &HqProject) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    for member in members(project) {
        ids.extend(
            storage::accepted_for_project(ledger, member)?
                .into_iter()
                .map(|row| row.id),
        );
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn read_list(ledger: &Ledger, project: &HqProject, stream: &mut TcpStream) -> Result<()> {
    let current = git_current(&project.root);
    let mut decisions = Vec::new();
    for id in stored_ids(ledger, project)? {
        if let Some(value) = decision_view(ledger, project, &id, &current, false)? {
            decisions.push(value);
        }
    }
    json(
        stream,
        200,
        &json!({"schema_version": SCHEMA_VERSION, "project_id": &project.id,
          "current_version": current.exact(), "decisions": decisions,
          "approval_gate": project_approval_gate(ledger, project)?,
          "confirmation_transport": confirmation_transport(ledger)}),
    )
}

fn project_approval_gate(ledger: &Ledger, project: &HqProject) -> Result<Value> {
    let mut states = Vec::new();
    for member in members(project) {
        let state = gate::approval_gate_state(ledger, member)?;
        states.push(json!({"project_id": member, "active": state.active, "reason": state.reason}));
    }
    let active = states.iter().any(|state| state["active"] == true);
    Ok(json!({"active": active,
        "reason": if active { None } else { Some("Decisions approval is dormant in this record.") },
        "members": states}))
}

fn confirmation_transport(ledger: &Ledger) -> Value {
    let mut native = bridge::native_click_availability(ledger);
    native.reason = native
        .reason
        .map(|reason| Masked::mask(&reason).into_text());
    json!({
        "available": if native.available { "native_click" } else if cfg!(target_os = "macos") { "terminal" } else { "unavailable" },
        "browser_available": native.available,
        "reason": native.reason,
        "native": native,
        "preparation_is_person_act": false,
        "requires_presence": true
    })
}

fn read_proposals(ledger: &Ledger, project: &HqProject, stream: &mut TcpStream) -> Result<()> {
    let mut proposals = Vec::new();
    let mut notes = Vec::new();
    let mut notes_source_state = "not_recorded";
    for member in members(project) {
        let report = thruline_core::decisions::rule_files::notes_for_project(ledger, member)?;
        if report.state == "recorded" {
            notes_source_state = "recorded";
        }
        notes.extend(report.notes);
    }
    for id in stored_ids(ledger, project)? {
        if let Some(value) =
            decision_view(ledger, project, &id, &CurrentVersion::Unavailable, true)?
        {
            proposals.push(value);
        }
    }
    json(
        stream,
        200,
        &json!({"schema_version": SCHEMA_VERSION, "project_id": &project.id, "proposals": proposals,
            "notes_source_state": notes_source_state, "notes": mask_value(&serde_json::to_value(notes)?)}),
    )
}

/// These bounded projections read source evidence and Git objects; they do not
/// run recorded commands, append verdicts, or make a person act.
fn read_history(
    ledger: &Ledger,
    project: &HqProject,
    route: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    let selected = if query.is_empty() {
        git_current(&project.root)
    } else if let Some(raw) = query.strip_prefix("version=") {
        let Some(version) = decode(raw) else {
            return error(stream, 400, "invalid_version", None);
        };
        let Some(version) = git_version(&project.root, &version) else {
            return error(stream, 404, "version_not_found", None);
        };
        CurrentVersion::Exact(version)
    } else {
        return error(stream, 400, "unknown_query_parameter", None);
    };
    let Some(version) = selected.exact() else {
        return error(stream, 409, "version_unverifiable", None);
    };
    let Some(commit) = version["commit_oid"].as_str() else {
        return error(stream, 409, "version_unverifiable", None);
    };
    let mut reports = Vec::new();
    let mut rows = Vec::new();
    let mut missing = Vec::new();
    for member in members(project) {
        let report = if route == "conflicts" {
            serde_json::to_value(thruline_core::decisions::conflict::project_conflicts(
                ledger,
                member,
                Path::new(&project.root),
                commit,
            )?)?
        } else {
            serde_json::to_value(thruline_core::decisions::revival::project_revivals(
                ledger,
                member,
                Path::new(&project.root),
                commit,
            )?)?
        };
        rows.extend(report[route].as_array().into_iter().flatten().cloned());
        missing.extend(
            report["missing_evidence"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned(),
        );
        reports.push(report);
    }
    let mut body = json!({"schema_version":SCHEMA_VERSION, "project_id":project.id,
        "version_id":commit, "tree_hash":version["tree_hash"], "members":reports,
        "missing_evidence":missing});
    body[route] = Value::Array(rows);
    json(stream, 200, &mask_value(&body))
}

fn read_open_threads(ledger: &Ledger, project: &HqProject, stream: &mut TcpStream) -> Result<()> {
    let mut rows = Vec::new();
    let mut missing = Vec::new();
    let mut reports = Vec::new();
    let mut observed_edit_threads = 0_u64;
    for member in members(project) {
        let report = thruline_core::decisions::person_threads::open_for_project(ledger, member)?;
        observed_edit_threads = observed_edit_threads.saturating_add(report.observed_edit_threads);
        rows.extend(report.threads.iter().cloned());
        missing.extend(report.missing.iter().cloned());
        reports.push(json!({"project_id":member, "report":report}));
    }
    json(
        stream,
        200,
        &mask_value(&json!({"schema_version":SCHEMA_VERSION,
        "project_id":project.id, "threads":rows, "observed_edit_threads":observed_edit_threads,
        "members":reports, "missing_evidence":missing})),
    )
}

fn read_detail(
    ledger: &Ledger,
    project: &HqProject,
    decision: &str,
    query: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    if !belongs(ledger, project, decision)? {
        return error(stream, 404, "decision_not_found", None);
    }
    let selected = if query.is_empty() {
        git_current(&project.root)
    } else if let Some(raw) = query.strip_prefix("version=") {
        let Some(version) = decode(raw) else {
            return error(stream, 400, "invalid_version", None);
        };
        let Some(version) = git_version(&project.root, &version) else {
            return error(
                stream,
                if version.starts_with("wt-") { 409 } else { 404 },
                if version.starts_with("wt-") {
                    "version_unverifiable"
                } else {
                    "version_not_found"
                },
                None,
            );
        };
        CurrentVersion::Exact(version)
    } else {
        return error(stream, 400, "unknown_query_parameter", None);
    };
    let Some(value) = decision_view(ledger, project, decision, &selected, false)? else {
        return error(stream, 404, "decision_not_found", None);
    };
    let verdict = value["current_verdict"].clone();
    let chain = match selected.exact().and_then(|version| {
        Some((
            version["version_id"].as_str()?,
            version["tree_hash"].as_str()?,
        ))
    }) {
        Some((version_id, tree_hash)) => proof_chain(
            ledger,
            verdict["id"].as_str().unwrap_or(decision),
            version_id,
            tree_hash,
        )?
        .map(|proof| serde_json::to_value(proof.chain))
        .transpose()?
        .unwrap_or_else(|| fallback_chain(&value, selected.exact())),
        None => fallback_chain(&value, selected.exact()),
    };
    json(
        stream,
        200,
        &json!({"schema_version": SCHEMA_VERSION, "decision": value,
            "selected_version": selected.exact(), "verdict": verdict, "chain": chain,
            "approval_gate": project_approval_gate(ledger, project)?,
            "confirmation_transport": confirmation_transport(ledger)}),
    )
}

/// Proposals and projects without an immutable version have no accepted proof.
fn fallback_chain(decision: &Value, version: Option<&Value>) -> Value {
    let decided = decision["lifecycle"] != "proposed";
    let stale = decision["current_verdict"]["state"] == "stale";
    let basis_version = decision["current_verdict"]["version"]["version_id"].as_str();
    let source_ids: Vec<String> = decision["sources"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|source| {
            source["event_id"]
                .as_str()
                .or_else(|| source["file_anchor"].as_str())
                .map(str::to_owned)
        })
        .collect();
    let finding_ids: Vec<String> = decision["current_verdict"]["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|finding| finding["id"].as_str().map(str::to_owned))
        .collect();
    let labels = ["DECIDED", "CHECK", "RAN ON", "FOUND", "SINCE", "SO"];
    let sentences = [
        "A person accepted this decision".to_owned(),
        "The check kind and parameters are recorded".to_owned(),
        basis_version.map_or_else(
            || "The named version has no recorded check".to_owned(),
            |id| format!("The stored check ran on {id}"),
        ),
        if stale {
            "Findings belong to the stored check".to_owned()
        } else {
            "Findings from that check are not recorded".to_owned()
        },
        if stale {
            "The working tree has changed after the stored check".to_owned()
        } else {
            "Later edit attribution is not recorded here".to_owned()
        },
        if stale {
            "Stored verdict is stale; check an exact new version".to_owned()
        } else {
            "No current verdict is recorded".to_owned()
        },
    ];
    let steps: Vec<Value> = (0..6)
        .map(|index| {
            let state = match index {
                0 | 1 => decided,
                2 | 3 => stale,
                _ => false,
            };
            let state = if stale && index >= 4 {
                "moved"
            } else if state {
                "holds"
            } else {
                "missing"
            };
            json!({
                "number": index + 1,
                "label": labels[index],
                "state": state,
                "sentence": &sentences[index],
                "source_ids": if index == 0 { source_ids.clone() } else { Vec::new() },
                "finding_ids": if index == 3 { finding_ids.clone() } else { Vec::new() },
                "version_id": version.and_then(|value| value["version_id"].as_str()).or(basis_version),
            })
        })
        .collect();
    Value::Array(steps)
}

enum CurrentVersion {
    Exact(Value),
    Dirty(Value),
    Unavailable,
}

impl CurrentVersion {
    fn exact(&self) -> Option<&Value> {
        match self {
            Self::Exact(version) => Some(version),
            Self::Dirty(_) | Self::Unavailable => None,
        }
    }

    fn head(&self) -> Option<&Value> {
        match self {
            Self::Exact(version) | Self::Dirty(version) => Some(version),
            Self::Unavailable => None,
        }
    }

    fn stale_reason(&self) -> Option<&'static str> {
        match self {
            Self::Exact(_) => Some("later_commit"),
            Self::Dirty(_) => Some("working_tree_dirty"),
            Self::Unavailable => None,
        }
    }
}

fn git_current(root: &str) -> CurrentVersion {
    let out = thruline_core::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .ok();
    let Some(out) = out.filter(|output| output.status.success()) else {
        return CurrentVersion::Unavailable;
    };
    let Some(head) = git_version(root, "HEAD") else {
        return CurrentVersion::Unavailable;
    };
    if out.stdout.is_empty() {
        CurrentVersion::Exact(head)
    } else {
        CurrentVersion::Dirty(head)
    }
}

fn git_version(root: &str, version: &str) -> Option<Value> {
    let immutable_id =
        (7..=64).contains(&version.len()) && version.bytes().all(|byte| byte.is_ascii_hexdigit());
    if version != "HEAD" && !immutable_id {
        return None;
    }
    let commit = git_oid(root, &format!("{version}^{{commit}}"))?;
    if immutable_id && !commit.starts_with(version) {
        return None;
    }
    let tree = git_oid(root, &format!("{commit}^{{tree}}"))?;
    let version_id = if version == "HEAD" { &commit } else { version };
    Some(
        json!({"version_id": version_id, "kind": "commit", "tree_hash": tree, "commit_oid": commit, "captured_at": null}),
    )
}

fn git_oid(root: &str, revision: &str) -> Option<String> {
    let out = thruline_core::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["rev-parse", "--verify", "--end-of-options", revision])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let oid = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (oid.len() == 40 || oid.len() == 64)
        .then_some(oid)
        .filter(|s| s.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn git_is_ancestor(root: &str, ancestor: &str, descendant: &str) -> bool {
    thruline_core::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(root)
        .status()
        .is_ok_and(|status| status.success())
}

fn mask_value(value: &Value) -> Value {
    match value {
        Value::String(s) => Value::String(Masked::mask(s).into_text()),
        Value::Array(xs) => Value::Array(xs.iter().map(mask_value).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), mask_value(v)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn decision_view(
    ledger: &Ledger,
    project: &HqProject,
    decision: &str,
    selected_version: &CurrentVersion,
    proposal_only: bool,
) -> Result<Option<Value>> {
    let latest_revision: Option<u32> = ledger.conn().query_row(
        "SELECT max(rev) FROM decision_revisions WHERE decision_id=?1",
        [decision],
        |row| row.get(0),
    )?;
    let Some(latest_revision) = latest_revision else {
        return Ok(None);
    };
    let accepted = storage::verified_acceptance_for_decision(ledger, decision)?;
    let accepted_version = accepted
        .as_ref()
        .map(|row| row.applies_from_version.clone());
    let retirement = storage::verified_retirement_for_decision(ledger, decision)?;
    let accepted_revision = accepted.as_ref().map(|row| row.revision);
    if proposal_only && accepted_revision.is_some_and(|rev| latest_revision <= rev) {
        return Ok(None);
    }
    let revision = if proposal_only {
        latest_revision
    } else {
        accepted_revision.unwrap_or(latest_revision)
    };
    let lifecycle = if proposal_only || accepted.is_none() {
        "proposed"
    } else if let Some(retirement) = &retirement {
        if retirement.superseded_by.is_some() {
            "superseded"
        } else {
            "retired"
        }
    } else {
        "accepted"
    };
    let (kind, params_schema_version, params_json, span, sources_json): (String, u32, String, String, String) = ledger.conn().query_row(
        "SELECT kind,params_schema_version,params_json,text,sources_json FROM decision_revisions WHERE decision_id=?1 AND rev=?2",
        (decision, revision),
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    )?;
    let params: Value = serde_json::from_str(&params_json)?;
    let sources: Value = serde_json::from_str(&sources_json)?;
    let current_verdict = if lifecycle != "proposed" {
        verdict_view(ledger, &project.root, decision, revision, selected_version)?
    } else {
        None
    };
    let (accepted_by, accepted_at, acceptance_reason, severity) = if lifecycle != "proposed" {
        (
            accepted.as_ref().map(|row| row.accepted_by.clone()),
            accepted.as_ref().map(|row| row.accepted_at.clone()),
            accepted.as_ref().map(|row| row.acceptance_reason.clone()),
            accepted.as_ref().map(|row| row.severity.clone()),
        )
    } else {
        (None, None, None, None)
    };
    let owner: String = ledger.conn().query_row(
        "SELECT project_id FROM decisions WHERE id=?1",
        [decision],
        |row| row.get(0),
    )?;
    let blocks_approval = match selected_version
        .exact()
        .and_then(|version| version["version_id"].as_str())
    {
        Some(version) if lifecycle == "accepted" => gate::decision_blocks_approval(
            ledger,
            &owner,
            decision,
            version,
            &thruline_core::ingest::iso8601_utc_now(),
        )?,
        _ => false,
    };
    Ok(Some(json!({
        "id": decision, "project_id": &project.id, "revision": revision,
        "lifecycle": lifecycle, "span": Masked::mask(&span).into_text(),
        "sources": mask_value(&sources),
        "kind": kind, "kind_code": kind_code(&kind),
        "params_schema_version": params_schema_version, "params": mask_value(&params),
        "params_sha256": thruline_core::chain::sha256_hex(params_json.as_bytes()),
        "severity": severity,
        "accepted_by": accepted_by.map(|actor| Masked::mask(&actor).into_text()),
        "accepted_at": accepted_at,
        "acceptance_reason": acceptance_reason.map(|reason| Masked::mask(&reason).into_text()),
        "applies_from_version": if lifecycle == "proposed" { None } else { accepted_version },
        "current_verdict": current_verdict, "blocks_approval": blocks_approval
    })))
}

fn kind_code(kind: &str) -> Option<&'static str> {
    match kind {
        "text_absent" => Some("A1"),
        "symbol_absent" => Some("A2"),
        "ui_copy_absent" => Some("A5"),
        "path_present" => Some("B1"),
        "test_present_and_runs" => Some("B2"),
        "feature_default_preserved" => Some("B6"),
        "edits_within_paths" => Some("D1"),
        "paths_untouched" => Some("D2"),
        "command_passes" => Some("E1"),
        "term_everywhere" => Some("G1"),
        "forbidden_words" => Some("G2"),
        "visual_match" => Some("C1"),
        "file_equals" => Some("C2"),
        "golden_output" => Some("C4"),
        "deferred_decision" => Some("H1"),
        _ => None,
    }
}

fn finding_message(finding: &Value) -> String {
    let surface = finding["surface"].as_str().unwrap_or("check");
    let path = finding["path"].as_str().unwrap_or("");
    let line = finding["line"].as_u64().unwrap_or(0);
    let location = if path.is_empty() {
        surface.to_owned()
    } else if line > 0 {
        format!("{surface}: {path}:{line}")
    } else {
        format!("{surface}: {path}")
    };
    Masked::mask(&location)
        .into_text()
        .chars()
        .take(160)
        .collect()
}

fn verdict_view(
    ledger: &Ledger,
    root: &str,
    decision: &str,
    revision: u32,
    selected: &CurrentVersion,
) -> Result<Option<Value>> {
    type VerdictRow = (
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
    );
    macro_rules! from_row {
        ($row:expr) => {{
            let row = $row;
            (
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
                row.get(11)?,
            )
        }};
    }
    let exact: Option<VerdictRow> = if let Some(version) = selected.exact() {
        let Some(version_id) = version["version_id"].as_str() else {
            return Ok(None);
        };
        let Some(tree_hash) = version["tree_hash"].as_str() else {
            return Ok(None);
        };
        let mut stmt = ledger.conn().prepare(
            "SELECT id,version_id,tree_hash,verdict,computed_at,reason,checkable_if,findings_json,surfaces_json,check_impl_version,tool_versions_json,basis_json
             FROM decision_verdicts WHERE decision_id=?1 AND rev=?2 AND tree_hash=?3 AND version_id=?4 ORDER BY rowid DESC LIMIT 1",
        )?;
        let mut rows = stmt.query((decision, revision, tree_hash, version_id))?;
        match rows.next()? {
            Some(row) => Some(from_row!(row)),
            None => None,
        }
    } else {
        None
    };
    let (row, version, stale_reason) = if let Some(row) = exact {
        (row, selected.exact().cloned(), None)
    } else if let Some(head) = selected.head() {
        let Some(head_commit) = head["commit_oid"].as_str() else {
            return Ok(None);
        };
        let mut stmt = ledger.conn().prepare(
            "SELECT id,version_id,tree_hash,verdict,computed_at,reason,checkable_if,findings_json,surfaces_json,check_impl_version,tool_versions_json,basis_json
             FROM decision_verdicts WHERE decision_id=?1 AND rev=?2 ORDER BY rowid DESC",
        )?;
        let rows = stmt.query_map((decision, revision), |row| {
            Ok::<VerdictRow, _>(from_row!(row))
        })?;
        let mut prior = None;
        for row in rows {
            let candidate = row?;
            let Some(basis) = git_version(root, &candidate.1) else {
                continue;
            };
            let Some(basis_commit) = basis["commit_oid"].as_str() else {
                continue;
            };
            if basis["tree_hash"].as_str() != Some(candidate.2.as_str())
                || !git_is_ancestor(root, basis_commit, head_commit)
            {
                continue;
            }
            prior = Some((candidate, basis));
            break;
        }
        let Some((row, basis)) = prior else {
            return Ok(None);
        };
        (row, Some(basis), selected.stale_reason())
    } else {
        return Ok(None);
    };
    let (
        id,
        version_id,
        _tree_hash,
        state,
        computed_at,
        reason,
        checkable_if,
        findings_json,
        surfaces_json,
        implementation,
        tools_json,
        basis_json,
    ) = row;
    let basis: Value = serde_json::from_str(&basis_json)?;
    let Some(checked_places) = basis["checked_places"].as_u64() else {
        return Ok(None);
    };
    let visual = basis["visual"].as_array().cloned().unwrap_or_default();
    let broken_visual: Vec<&Value> = visual
        .iter()
        .filter(|screen| screen["state"] == "broken")
        .collect();
    let raw_findings: Value = serde_json::from_str(&findings_json)?;
    let findings: Vec<Value> = raw_findings
        .as_array()
        .map(|items| {
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let screen = broken_visual.get(index).copied();
                    json!({
                        "id": format!("{id}-{}", index + 1), "surface": item["surface"],
                        "path": mask_value(&item["path"]), "line": item["line"],
                        "span": mask_value(&item["span"]),
                        "message": finding_message(item), "edit_id": null, "session_id": null,
                        "thread_id": null,
                        "introduced_at": null, "present_since_version": null,
                        "screen": screen.map(|value| mask_value(&value["screen"])),
                        "difference_pct": screen.map(|value| value["difference_pct"].clone()),
                        "diff_image_path": screen.map(|value| mask_value(&value["diff_image_path"])),
                        "reference_hash": screen.map(|value| value["reference_sha256"].clone())
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let raw_surfaces: Value = serde_json::from_str(&surfaces_json)?;
    let Some(surfaces) = raw_surfaces.as_array().and_then(|items| {
        items
            .iter()
            .map(|item| {
                Some(json!({"surface": item["surface"], "state": item["state"],
                "finding_count": item["finding_count"],
                "checked_places": item["checked_places"].as_u64()?, "reason": null}))
            })
            .collect::<Option<Vec<_>>>()
    }) else {
        return Ok(None);
    };
    let tools: Value = serde_json::from_str(&tools_json)?;
    let last_held: Option<String> = {
        let mut stmt = ledger.conn().prepare(
            "SELECT version_id FROM decision_verdicts WHERE decision_id=?1 AND rev=?2 AND verdict='holds' ORDER BY rowid DESC LIMIT 1",
        )?;
        let mut rows = stmt.query((decision, revision))?;
        match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        }
    };
    let waiver: Option<StoredWaiver> = if stale_reason.is_some() {
        None
    } else {
        let mut stmt = ledger.conn().prepare(
            "SELECT id,actor,reason,waived_at,expires_version,expires_at FROM decision_waivers WHERE decision_id=?1 AND version_id=?2 ORDER BY rowid DESC",
        )?;
        let rows = stmt.query_map((decision, &version_id), |row| {
            Ok(StoredWaiver {
                id: row.get(0)?,
                actor: row.get(1)?,
                reason: row.get(2)?,
                at: row.get(3)?,
                expires_version: row.get(4)?,
                expires_at: row.get(5)?,
            })
        })?;
        let mut active = None;
        let now = thruline_core::ingest::iso8601_utc_now();
        for row in rows {
            let candidate = row?;
            if candidate.at.as_str() > now.as_str()
                || candidate.expires_version.as_deref() == Some(version_id.as_str())
                || candidate
                    .expires_at
                    .as_deref()
                    .is_some_and(|expiry| expiry <= now.as_str())
            {
                continue;
            }
            if authority::bridge::verified(ledger, PersonActKind::Waive, &candidate.id)? {
                active = Some(candidate);
                break;
            }
        }
        active
    };
    let waiver = waiver.map(|waiver| {
        json!({
            "id": waiver.id, "actor": Masked::mask(&waiver.actor).into_text(),
            "because": Masked::mask(&waiver.reason).into_text(), "at": waiver.at,
            "version_id": version_id,
            "expires_version": waiver.expires_version, "expires_at": waiver.expires_at
        })
    });
    let owner: String = ledger.conn().query_row(
        "SELECT project_id FROM decisions WHERE id=?1",
        [decision],
        |row| row.get(0),
    )?;
    let blocks_approval = stale_reason.is_none()
        && gate::decision_blocks_approval(
            ledger,
            &owner,
            decision,
            &version_id,
            &thruline_core::ingest::iso8601_utc_now(),
        )?;
    Ok(Some(json!({
        "id": id, "decision_id": decision, "revision": revision,
        "version": version, "state": if stale_reason.is_some() { "stale" } else { &state },
        "basis_state": state, "stale_reason": stale_reason,
        "reason": reason.map(|text| Masked::mask(&text).into_text()),
        "checkable_if": checkable_if.map(|text| Masked::mask(&text).into_text()),
        "record_projection": mask_value(&basis["record_projection"]),
        "finding_count": findings.len(), "findings": findings, "surfaces": surfaces,
        "visual": mask_value(&Value::Array(visual)),
        "not_looked_at": mask_value(&basis.get("not_looked_at").cloned().unwrap_or_else(|| json!([]))),
        "checked_places": checked_places, "check_impl_version": implementation,
        "tool_versions": tools, "computed_at": computed_at,
        "last_held_version": last_held, "waived": waiver,
        "blocks_approval": blocks_approval
    })))
}

fn project_member_ids(project: &HqProject) -> Vec<String> {
    members(project).map(str::to_owned).collect()
}

fn prepare_native(
    ledger: &Ledger,
    project: &HqProject,
    decision: &str,
    body: &Value,
    stream: &mut TcpStream,
) -> Result<()> {
    let request: PrepareBody = match serde_json::from_value(body.clone()) {
        Ok(request) => request,
        Err(_) => return error(stream, 400, "invalid_prepare_body", None),
    };
    if request.schema_version != 1
        || request.actor.trim().is_empty()
        || request.because.trim().is_empty()
    {
        return error(stream, 400, "invalid_prepare_body", None);
    }
    let result = match request.action.as_str() {
        "accept" => {
            if request.superseded_by.is_some()
                || request.version_id.is_some()
                || request.expires_version.is_some()
                || request.expires_at.is_some()
                || !matches!(request.severity.as_deref(), Some("blocks" | "warns"))
            {
                return error(stream, 400, "invalid_prepare_body", None);
            }
            let Some(revision) = request.revision else {
                return error(stream, 400, "revision_required", None);
            };
            let Some(version) = request
                .applies_from_version
                .as_deref()
                .and_then(|id| git_version(&project.root, id))
            else {
                return error(stream, 400, "exact_version_required", None);
            };
            storage::accept(
                ledger,
                AcceptanceInput {
                    decision_id: decision.into(),
                    revision,
                    actor: request.actor,
                    at: thruline_core::ingest::iso8601_utc_now(),
                    because: request.because,
                    severity: request.severity.unwrap(),
                    applies_from_version: version["commit_oid"].as_str().unwrap().into(),
                },
            )
        }
        "retire" => {
            if request.revision.is_some()
                || request.severity.is_some()
                || request.applies_from_version.is_some()
                || request.version_id.is_some()
                || request.expires_version.is_some()
                || request.expires_at.is_some()
            {
                return error(stream, 400, "invalid_prepare_body", None);
            }
            if let Some(target) = &request.superseded_by {
                let owner = |id: &str| {
                    ledger.conn().query_row(
                        "SELECT project_id FROM decisions WHERE id=?1",
                        [id],
                        |r| r.get::<_, String>(0),
                    )
                };
                if !belongs(ledger, project, target)? || owner(decision)? != owner(target)? {
                    return error(stream, 400, "same_member_successor_required", None);
                }
            }
            storage::retire(
                ledger,
                RetirementInput {
                    decision_id: decision.into(),
                    actor: request.actor,
                    at: thruline_core::ingest::iso8601_utc_now(),
                    because: request.because,
                    superseded_by: request.superseded_by,
                },
            )
        }
        "waive" => {
            if request.revision.is_some()
                || request.severity.is_some()
                || request.applies_from_version.is_some()
                || request.superseded_by.is_some()
            {
                return error(stream, 400, "invalid_prepare_body", None);
            }
            let Some(version) = request
                .version_id
                .as_deref()
                .and_then(|id| git_version(&project.root, id))
            else {
                return error(stream, 400, "exact_version_required", None);
            };
            storage::waive(
                ledger,
                WaiverInput {
                    decision_id: decision.into(),
                    actor: request.actor,
                    at: thruline_core::ingest::iso8601_utc_now(),
                    because: request.because,
                    version_id: version["commit_oid"].as_str().unwrap().into(),
                    expires_version: request.expires_version,
                    expires_at: request.expires_at,
                },
            )
        }
        _ => return error(stream, 400, "invalid_prepare_body", None),
    };
    match result {
        Ok(id) => json(
            stream,
            200,
            &json!({"schema_version":1,"action":request.action,
            "act_row_id":id,"state":"pending_bridge","preparation_is_person_act":false}),
        ),
        Err(_) => error(stream, 409, "preparation_conflict", None),
    }
}

fn native_request(
    body: &Value,
) -> Option<(
    NativeReviewBody,
    PersonActKind,
    Option<bridge::SupersessionInput>,
)> {
    let request: NativeReviewBody = serde_json::from_value(body.clone()).ok()?;
    if request.schema_version != 1 || request.act_row_id.is_empty() {
        return None;
    }
    let action = match request.action.as_str() {
        "accept" if request.selection.is_none() => PersonActKind::Accept,
        "retire" if request.selection.is_none() => PersonActKind::Retire,
        "waive" if request.selection.is_none() => PersonActKind::Waive,
        "supersede" if request.selection.is_some() => PersonActKind::Accept,
        _ => return None,
    };
    let selection = request
        .selection
        .as_ref()
        .map(|s| bridge::SupersessionInput {
            predecessor_revision: s.predecessor_revision,
            screen: s.screen.clone(),
        });
    Some((request, action, selection))
}

fn act_belongs(ledger: &Ledger, decision: &str, action: PersonActKind, act: &str) -> Result<bool> {
    let table = match action {
        PersonActKind::Accept => "decision_acceptances",
        PersonActKind::Retire => "decision_retirements",
        PersonActKind::Waive => "decision_waivers",
    };
    Ok(ledger.conn().query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1 AND decision_id=?2)"),
        (act, decision),
        |r| r.get(0),
    )?)
}

fn native_review(
    ledger: &Ledger,
    _project: &HqProject,
    decision: &str,
    body: &Value,
    stream: &mut TcpStream,
) -> Result<()> {
    let Some((request, action, selection)) = native_request(body) else {
        return error(stream, 400, "invalid_native_body", None);
    };
    if request.expected_review_sha256.is_some() {
        return error(stream, 400, "invalid_native_body", None);
    }
    if !act_belongs(ledger, decision, action, &request.act_row_id)? {
        return error(stream, 404, "act_not_found", None);
    }
    match bridge::review_terminal_act(ledger, action, &request.act_row_id, selection.as_ref()) {
        Ok(review) => json(
            stream,
            200,
            &json!({"schema_version":1,
            "review_sha256":thruline_core::chain::sha256_hex(&serde_json::to_vec(&review)?),
            "review":review,"preparation_is_person_act":false}),
        ),
        Err(_) => error(stream, 409, "review_unavailable", None),
    }
}

struct NativeSlot;
impl Drop for NativeSlot {
    fn drop(&mut self) {
        NATIVE_BUSY.store(false, Ordering::Release);
    }
}

fn start_native_confirmation(
    state: &State,
    ledger: &Ledger,
    project: &HqProject,
    decision: &str,
    body: &Value,
    stream: &mut TcpStream,
) -> Result<()> {
    let Some((request, action, selection)) = native_request(body) else {
        return error(stream, 400, "invalid_native_body", None);
    };
    let Some(expected) = request.expected_review_sha256.as_deref().filter(|sha| {
        sha.len() == 64
            && sha
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    }) else {
        return error(stream, 400, "review_sha256_required", None);
    };
    if !act_belongs(ledger, decision, action, &request.act_row_id)? {
        return error(stream, 404, "act_not_found", None);
    }
    let availability = bridge::native_click_availability(ledger);
    if !availability.available {
        return error(
            stream,
            409,
            "native_confirmation_unavailable",
            availability.reason.as_deref(),
        );
    }
    let review = match bridge::review_terminal_act(
        ledger,
        action,
        &request.act_row_id,
        selection.as_ref(),
    ) {
        Ok(review) => review,
        Err(_) => return error(stream, 409, "review_unavailable", None),
    };
    if expected != thruline_core::chain::sha256_hex(&serde_json::to_vec(&review)?) {
        return error(stream, 409, "review_moved", None);
    }
    if NATIVE_BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return error(stream, 409, "native_confirmation_busy", None);
    }
    let slot = NativeSlot;
    let id = ulid::Ulid::generate().to_string();
    let initial = json!({"schema_version":1,"job_id":id,"kind":"native_confirmation",
        "project_id":project.id,"decision_id":decision,"act_row_id":request.act_row_id,
        "state":"running","receipt_id":null,"errors":[],"progress":{"label":"Review in the native window","unit":"person_action","completed":0,"total":1}});
    // Native jobs have a process-wide slot; do not coalesce them with a running check.
    let mut registration = initial.clone();
    registration["project_id"] = Value::Null;
    let db = state.db.clone();
    let project = project.clone();
    let expected = expected.to_owned();
    let act = request.act_row_id;
    let result = submit_job(registration, move |id| {
        let _slot = slot;
        mutate_job(&id, |job| job["project_id"] = json!(project.id));
        let result = Ledger::open(&db)
            .map_err(anyhow::Error::from)
            .and_then(|mut ledger| {
                let receipt =
                    bridge::confirm_native_click(&mut ledger, action, &act, selection, &expected)?;
                let derived_error = write_derived(&ledger, &project)
                    .err()
                    .map(|error| Masked::mask(&error.to_string()).into_text());
                Ok((receipt, derived_error))
            });
        mutate_job(&id, |job| match result {
            Ok((receipt, derived_error)) => {
                job["state"] = json!("complete");
                job["receipt_id"] = json!(receipt);
                job["derived_written"] = json!(derived_error.is_none());
                job["errors"] = json!(derived_error.into_iter().collect::<Vec<_>>());
                job["progress"]["completed"] = json!(1);
            }
            Err(error) => {
                job["state"] = json!("failed");
                job["errors"] = json!([Masked::mask(&error.to_string()).into_text()]);
            }
        });
    });
    match result {
        Ok(_) => json(stream, 202, &initial),
        Err(_) => error(stream, 503, "job_store_unavailable", None),
    }
}

fn write_derived(ledger: &Ledger, project: &HqProject) -> Result<()> {
    storage::write_derived_file_for_projects(
        ledger,
        &project.id,
        &project_member_ids(project),
        Path::new(&project.root),
    )?;
    Ok(())
}

fn accept(
    ledger: &Ledger,
    project: &HqProject,
    decision: &str,
    body: &Value,
    stream: &mut TcpStream,
) -> Result<()> {
    let request: AcceptBody = match serde_json::from_value(body.clone()) {
        Ok(value) => value,
        Err(_) => return error(stream, 400, "invalid_accept_body", None),
    };
    if request.schema_version != PERSON_ACT_SCHEMA_VERSION
        || request.event_id.trim().is_empty()
        || request.actor.trim().is_empty()
        || request.because.trim().is_empty()
        || request.applies_from_version.trim().is_empty()
        || !matches!(request.severity.as_str(), "blocks" | "warns")
    {
        return error(stream, 400, "invalid_accept_body", None);
    }
    if !authority::verified_user_event(ledger, &request.event_id)? {
        return error(stream, 409, "person_confirmation_unavailable", None);
    }
    let latest: Option<u32> = ledger.conn().query_row(
        "SELECT max(rev) FROM decision_revisions WHERE decision_id=?1",
        [decision],
        |row| row.get(0),
    )?;
    if latest != Some(request.revision) {
        return error(stream, 409, "revision_conflict", None);
    }
    let stored: (String, u32, String) = ledger.conn().query_row(
        "SELECT kind,params_schema_version,params_json FROM decision_revisions WHERE decision_id=?1 AND rev=?2",
        (decision, request.revision), |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    let expected = match parse_params(&stored.0, stored.1, &stored.2) {
        Ok(value) => serde_json::to_value(value)?,
        Err(_) => return error(stream, 503, "stored_params_unavailable", None),
    };
    let request_params = match parse_params(
        &stored.0,
        PARAMS_SCHEMA_VERSION,
        &request.params.to_string(),
    ) {
        Ok(value) => serde_json::to_value(value)?,
        Err(_) => return error(stream, 400, "invalid_params", None),
    };
    if request_params != expected {
        return error(stream, 409, "params_conflict", None);
    }
    let row_id = match storage::accept(
        ledger,
        AcceptanceInput {
            decision_id: decision.into(),
            revision: request.revision,
            actor: request.actor,
            at: thruline_core::ingest::iso8601_utc_now(),
            because: request.because,
            severity: request.severity,
            applies_from_version: request.applies_from_version,
        },
    ) {
        Ok(id) => id,
        Err(_) => return error(stream, 409, "acceptance_conflict", None),
    };
    if authority::record(ledger, PersonActKind::Accept, &row_id, &request.event_id).is_err() {
        return error(stream, 409, "person_confirmation_mismatch", None);
    }
    if write_derived(ledger, project).is_err() {
        return error(stream, 503, "derived_copy_unavailable", None);
    }
    let value = decision_view(
        ledger,
        project,
        decision,
        &git_current(&project.root),
        false,
    )?;
    json(
        stream,
        200,
        &json!({"schema_version": SCHEMA_VERSION, "id": row_id, "state":"pending_bridge", "decision": value}),
    )
}

fn retire(
    ledger: &Ledger,
    project: &HqProject,
    decision: &str,
    body: &Value,
    stream: &mut TcpStream,
) -> Result<()> {
    let request: RetireBody = match serde_json::from_value(body.clone()) {
        Ok(value) => value,
        Err(_) => return error(stream, 400, "invalid_retire_body", None),
    };
    if request.schema_version != PERSON_ACT_SCHEMA_VERSION
        || request.event_id.trim().is_empty()
        || request.actor.trim().is_empty()
        || request.because.trim().is_empty()
    {
        return error(stream, 400, "invalid_retire_body", None);
    }
    if !authority::verified_user_event(ledger, &request.event_id)? {
        return error(stream, 409, "person_confirmation_unavailable", None);
    }
    let row_id = match storage::retire(
        ledger,
        RetirementInput {
            decision_id: decision.into(),
            actor: request.actor,
            at: thruline_core::ingest::iso8601_utc_now(),
            because: request.because,
            superseded_by: request.superseded_by,
        },
    ) {
        Ok(id) => id,
        Err(_) => return error(stream, 409, "retirement_conflict", None),
    };
    if authority::record(ledger, PersonActKind::Retire, &row_id, &request.event_id).is_err() {
        return error(stream, 409, "person_confirmation_mismatch", None);
    }
    if write_derived(ledger, project).is_err() {
        return error(stream, 503, "derived_copy_unavailable", None);
    }
    let value = decision_view(
        ledger,
        project,
        decision,
        &git_current(&project.root),
        false,
    )?;
    json(
        stream,
        200,
        &json!({"schema_version": SCHEMA_VERSION, "id": row_id, "state":"pending_bridge", "decision": value}),
    )
}

fn waive(
    ledger: &Ledger,
    project: &HqProject,
    decision: &str,
    body: &Value,
    stream: &mut TcpStream,
) -> Result<()> {
    let request: WaiveBody = match serde_json::from_value(body.clone()) {
        Ok(value) => value,
        Err(_) => return error(stream, 400, "invalid_waive_body", None),
    };
    if request.schema_version != PERSON_ACT_SCHEMA_VERSION
        || request.event_id.trim().is_empty()
        || request.actor.trim().is_empty()
        || request.because.trim().is_empty()
        || request.version_id.trim().is_empty()
    {
        return error(stream, 400, "invalid_waive_body", None);
    }
    if !authority::verified_user_event(ledger, &request.event_id)? {
        return error(stream, 409, "person_confirmation_unavailable", None);
    }
    let row_id = match storage::waive(
        ledger,
        WaiverInput {
            decision_id: decision.into(),
            version_id: request.version_id,
            actor: request.actor,
            at: thruline_core::ingest::iso8601_utc_now(),
            because: request.because,
            expires_version: request.expires_version,
            expires_at: request.expires_at,
        },
    ) {
        Ok(id) => id,
        Err(_) => return error(stream, 409, "waiver_conflict", None),
    };
    if authority::record(ledger, PersonActKind::Waive, &row_id, &request.event_id).is_err() {
        return error(stream, 409, "person_confirmation_mismatch", None);
    }
    let value = decision_view(
        ledger,
        project,
        decision,
        &git_current(&project.root),
        false,
    )?;
    json(
        stream,
        200,
        &json!({"schema_version": SCHEMA_VERSION, "id": row_id, "state":"pending_bridge", "decision": value}),
    )
}

fn start_check(
    state: &State,
    ledger: &Ledger,
    project: &HqProject,
    body: &Value,
    stream: &mut TcpStream,
) -> Result<()> {
    let request: CheckBody = match serde_json::from_value(body.clone()) {
        Ok(value) => value,
        Err(_) => return error(stream, 400, "invalid_check_body", None),
    };
    if request.schema_version != SCHEMA_VERSION || request.version_id.trim().is_empty() {
        return error(stream, 400, "invalid_check_body", None);
    }
    if request.all == Some(true) && request.decision_ids.is_some() {
        return error(stream, 400, "select_all_or_ids", None);
    }
    if request.all != Some(true) && request.decision_ids.as_ref().is_none_or(Vec::is_empty) {
        return error(stream, 400, "selection_required", None);
    }
    // A visual or run-backed decision may execute a local tool. Use the
    // project's existing recorded grant and sealed cache before any check.
    match super::run_route::consent(ledger, &state.db, Path::new(&project.root))? {
        super::run_route::ConsentState::Given { .. } => {}
        super::run_route::ConsentState::Never | super::run_route::ConsentState::FileMissing => {
            return error(stream, 403, "permission_required", None)
        }
        super::run_route::ConsentState::Revoked => {
            return error(stream, 403, "permission_revoked", None)
        }
        super::run_route::ConsentState::NotInRecord => {
            return error(stream, 403, "permission_not_in_record", None)
        }
    }
    let accepted = accepted_ids(ledger, project)?;
    let selection = request.decision_ids.clone();
    if let Some(ids) = &selection {
        if ids.len() > 256 || ids.iter().any(|id| id.is_empty()) {
            return error(stream, 400, "invalid_selection", None);
        }
        for id in ids {
            if !belongs(ledger, project, id)? {
                return error(stream, 404, "decision_not_found", None);
            }
            if !accepted.contains(id) {
                return error(stream, 409, "decision_not_accepted", None);
            }
        }
    }
    let id = ulid::Ulid::generate().to_string();
    let at = thruline_core::ingest::iso8601_utc_now();
    let replay = request.replay.unwrap_or(false);
    let ids = selection.unwrap_or(accepted);
    let total = ids.len();
    let work: Vec<(String, String)> = ids
        .into_iter()
        .map(|decision| {
            let owner = ledger.conn().query_row(
                "SELECT project_id FROM decisions WHERE id=?1",
                [&decision],
                |row| row.get(0),
            )?;
            Ok((decision, owner))
        })
        .collect::<Result<_>>()?;
    let job = json!({"schema_version":SCHEMA_VERSION,"job_id":id,"project_id":project.id,
        "state":"running","version_id":request.version_id,"total":total,"completed":0,
        "decision_ids":work.iter().map(|(id,_)| id).collect::<Vec<_>>(),
        "verdict_ids":[],"errors":[],"progress":{"completed":0,"total":total,
        "label":format!("0 of {total} decisions"),"unit":"decisions"}});
    let db = state.db.clone();
    let root = project.root.clone();
    let version = request.version_id;
    let result = submit_job(job, move |id| {
        let Ok(ledger) = Ledger::open(&db) else {
            mutate_job(&id, |job| {
                job["state"] = json!("failed");
                job["errors"] = json!(["store unavailable for accepted check"]);
            });
            return;
        };
        let mut verdict_ids = Vec::new();
        let mut errors = Vec::new();
        if work.is_empty() {
            errors.push("no accepted decisions were selected".to_owned());
        }
        for (index, (decision, owner)) in work.into_iter().enumerate() {
            let progress_id = id.clone();
            let observer = Arc::new(
                move |progress: thruline_core::decisions::checks_visual::VisualProgress| {
                    mutate_job(&progress_id, |job| {
                        job["progress"] = json!({
                            "completed":progress.completed,"total":progress.total,
                            "label":Masked::mask(&progress.label).into_text(),"unit":"screens",
                        })
                    });
                },
            );
            let result = thruline_core::decisions::checks_visual::with_visual_progress_observer(
                observer,
                || {
                    engine::check_decision_version(
                        &ledger,
                        &owner,
                        Path::new(&root),
                        &version,
                        &at,
                        replay,
                        &decision,
                    )
                },
            );
            match result {
                Ok(results) => {
                    for result in results {
                        if let Some(id) = result.verdict_id {
                            verdict_ids.push(id);
                        } else {
                            errors.push(
                                Masked::mask(result.outcome.reason.as_deref().unwrap_or(
                                    "check result could not be recorded for the named version",
                                ))
                                .into_text(),
                            );
                        }
                    }
                }
                Err(_) => errors.push("accepted check could not complete".to_owned()),
            }
            mutate_job(&id, |job| {
                job["completed"] = json!(index + 1);
                job["verdict_ids"] = json!(verdict_ids);
                job["errors"] = json!(errors);
                job["progress"] = json!({"completed":index + 1,"total":total,
                    "label":format!("{} of {total} decisions",index + 1),"unit":"decisions"});
            });
        }
        mutate_job(&id, |job| {
            job["state"] = json!(if errors.is_empty() {
                "complete"
            } else {
                "failed"
            });
            job["errors"] = json!(errors);
        });
    });
    match result {
        Ok(job) => json(stream, 202, &job),
        Err(_) => error(stream, 503, "job_store_unavailable", None),
    }
}

/// Register before spawning, so an explicit request returns without waiting
/// for a renderer/build. Completed rows can be evicted; active progress cannot.
fn submit_job(initial: Value, run: impl FnOnce(String) + Send + 'static) -> Result<Value> {
    let id = initial["job_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("job id missing"))?
        .to_owned();
    {
        let jobs = JOBS.get_or_init(|| Mutex::new(VecDeque::new()));
        let mut jobs = jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("job store unavailable"))?;
        if let Some(project) = initial["project_id"].as_str() {
            if let Some((_, running)) = jobs.iter().find(|(_, job)| {
                job["project_id"].as_str() == Some(project)
                    && job["state"] == "running"
                    && job["kind"] == initial["kind"]
            }) {
                return Ok(running.clone());
            }
        }
        while jobs.len() >= JOB_LIMIT {
            let Some(index) = jobs.iter().position(|(_, job)| job["state"] != "running") else {
                return Err(anyhow::anyhow!("all job slots are active"));
            };
            jobs.remove(index);
        }
        jobs.push_back((id.clone(), initial.clone()));
    }
    let worker_id = id.clone();
    let spawn = std::thread::Builder::new()
        .name("decisions-check".into())
        .spawn(move || {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(worker_id.clone())))
                .is_err()
            {
                mutate_job(&worker_id, |job| {
                    job["state"] = json!("failed");
                    job["errors"] = json!(["accepted check worker could not complete"]);
                });
            }
        });
    if spawn.is_err() {
        mutate_job(&id, |job| {
            job["state"] = json!("failed");
            job["errors"] = json!(["accepted check worker could not start"]);
        });
        return Err(anyhow::anyhow!("check worker could not start"));
    }
    Ok(initial)
}

fn mutate_job(id: &str, change: impl FnOnce(&mut Value)) {
    let Some(jobs) = JOBS.get() else { return };
    let Ok(mut jobs) = jobs.lock() else { return };
    if let Some((_, job)) = jobs.iter_mut().find(|(key, _)| key == id) {
        change(job);
    }
}

fn stored_job(id: &str) -> Option<Value> {
    JOBS.get()?
        .lock()
        .ok()?
        .iter()
        .find(|(key, _)| key == id)
        .map(|(_, job)| job.clone())
}

fn job_status(query: &str, headers: &RequestHeaders, stream: &mut TcpStream) -> Result<()> {
    if !no_body(headers) {
        return error(stream, 400, "malformed_request", None);
    }
    let Some(raw) = query.strip_prefix("job_id=") else {
        return error(stream, 400, "job_id_required", None);
    };
    let Some(id) = decode(raw) else {
        return error(stream, 400, "invalid_job_id", None);
    };
    match stored_job(&id) {
        Some(job) => json(stream, 200, &job),
        None => error(stream, 404, "job_not_found", None),
    }
}

#[cfg(test)]
mod background_job_tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn explicit_check_returns_while_worker_waits_and_progress_remains_readable() {
        let id = ulid::Ulid::generate().to_string();
        let (release, wait) = mpsc::channel();
        let (started, observed) = mpsc::channel();
        let initial = json!({"job_id":id,"state":"running","total":1,"completed":0,
            "progress":{"completed":0,"total":14,"label":"0 of 14 screens"}});
        let response = submit_job(initial, move |id| {
            mutate_job(&id, |job| {
                job["progress"] = json!({"completed":6,"total":14,
                "label":"6 of 14 screens"})
            });
            started.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(2)).unwrap();
            mutate_job(&id, |job| job["state"] = json!("complete"));
        })
        .unwrap();
        assert_eq!(response["state"], "running");
        observed.recv_timeout(Duration::from_secs(2)).unwrap();
        let live = stored_job(&id).unwrap();
        assert_eq!(live["state"], "running");
        assert_eq!(live["progress"]["label"], "6 of 14 screens");
        release.send(()).unwrap();
        for _ in 0..100 {
            if stored_job(&id).unwrap()["state"] == "complete" {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("completed worker must finish its readable job");
    }

    #[test]
    fn worker_panic_is_a_failed_job_with_a_plain_error() {
        let id = ulid::Ulid::generate().to_string();
        submit_job(json!({"job_id":id,"state":"running","errors":[]}), |_| {
            panic!("synthetic worker failure")
        })
        .unwrap();
        for _ in 0..100 {
            let job = stored_job(&id).unwrap();
            if job["state"] == "failed" {
                assert_eq!(job["errors"][0], "accepted check worker could not complete");
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("panicked worker must not leave a running job");
    }
}
