//! C46 Settings: the run permission held for each exact recorded worktree.
//! The HQ project groups ledger members for display; it is never itself a
//! permission scope. Writes resolve opaque folder identities on the server.

use super::*;
use thruline_core::hq as core_hq;
use thruline_core::run_permission::{self as rp, Standing};

const SCHEMA_VERSION: u32 = 1;
const PREFIX: &str = "/api/hq/projects/";

pub(super) fn owns(path: &str) -> bool {
    let Some(rest) = path.strip_prefix(PREFIX) else {
        return false;
    };
    matches!(
        rest.split('/').collect::<Vec<_>>().as_slice(),
        [_, "run-permissions"] | [_, "run-permissions", _, "revoke"]
    )
}

fn json<T: Serialize>(stream: &mut TcpStream, status: u16, value: &T) -> Result<()> {
    respond(
        stream,
        status,
        "application/json; charset=utf-8",
        &serde_json::to_vec(value)?,
    )
}

fn refuse(stream: &mut TcpStream, status: u16, error: &str) -> Result<()> {
    json(stream, status, &serde_json::json!({"error": error}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalRequest {
    expected_row: String,
}

fn hex_id(id: &str, len: usize) -> bool {
    id.len() == len && id.bytes().all(|byte| byte.is_ascii_hexdigit())
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
    if !query.is_empty() {
        return refuse(stream, 400, "unknown_query_parameter");
    }
    if headers.malformed {
        return refuse(stream, 400, "malformed_request");
    }
    let rest = path.strip_prefix(PREFIX).expect("owned permission route");
    let parts: Vec<_> = rest.split('/').collect();
    match parts.as_slice() {
        [project_id, "run-permissions"] => {
            if method != "GET" {
                return refuse(stream, 405, "method_not_allowed");
            }
            if !hex_id(project_id, 16) {
                return refuse(stream, 400, "malformed_request");
            }
            let Some(ledger) = open_ledger_for_read(state, stream)? else {
                return Ok(());
            };
            let Some(project) = core_hq::find_project(&ledger, project_id)? else {
                return refuse(stream, 404, "no_such_project");
            };
            get(&ledger, state, &project, stream)
        }
        [project_id, "run-permissions", folder_id, "revoke"] => {
            if method != "POST" {
                return refuse(stream, 405, "method_not_allowed");
            }
            if !hex_id(project_id, 16) || !hex_id(folder_id, 64) {
                return refuse(stream, 400, "malformed_request");
            }
            // The same guards as the Change permission writes, before a body
            // is read or a store is opened.
            if !run_route::origin_is_ours(state, headers) {
                return refuse(stream, 403, "same_origin_required");
            }
            if state.run_route.token().is_none() {
                return refuse(stream, 503, "run_token_unavailable");
            }
            if !run_route::token_is_ours(state, headers) {
                return refuse(stream, 403, "run_token_required");
            }
            let body = match read_interpret_body(reader, headers) {
                Ok(body) => body,
                Err(_) => return refuse(stream, 400, "malformed_request"),
            };
            let Ok(request) = serde_json::from_slice::<HistoricalRequest>(&body) else {
                return refuse(stream, 400, "malformed_request");
            };
            if request.expected_row.trim().is_empty() {
                return refuse(stream, 400, "malformed_request");
            }
            let Some(ledger) = open_ledger_for_read(state, stream)? else {
                return Ok(());
            };
            let Some(project) = core_hq::find_project(&ledger, project_id)? else {
                return refuse(stream, 404, "no_such_project");
            };
            run_route::revoke_historical(
                state,
                &ledger,
                &project,
                folder_id,
                &request.expected_row,
                stream,
            )
        }
        _ => refuse(stream, 404, "no_such_route"),
    }
}

struct FolderEntry {
    folder: String,
    current_change: Option<String>,
    standing: Standing,
}

/// Only read Change IDs for this HQ project's ledger members. Inspect the
/// recorded worktree root for those Changes, not every Change in the store.
fn folder_entries(ledger: &Ledger, project: &core_hq::HqProject) -> Result<Vec<FolderEntry>> {
    let members: BTreeSet<String> = project
        .members
        .iter()
        .map(|member| member.ledger_project_id.clone())
        .collect();
    let ids: Vec<&str> = members.iter().map(String::as_str).collect();
    let marks = vec!["?"; ids.len()].join(",");
    let mut current = BTreeMap::<String, String>::new();
    let mut changes = ledger.conn().prepare(&format!(
        "SELECT id FROM changes WHERE project_id IN ({marks}) ORDER BY id"
    ))?;
    let change_ids = changes
        .query_map(rusqlite::params_from_iter(ids.iter().copied()), |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in change_ids {
        let root = thruline_core::change_decision::worktree_root(ledger, &id)?
            .filter(|root| run_route::permission_root_usable(root, false));
        if let Some(root) = root {
            current.entry(root.display().to_string()).or_insert(id);
        }
    }
    let mut folders: BTreeSet<String> = current.keys().cloned().collect();
    let mut recorded = ledger.conn().prepare(&format!(
        "SELECT DISTINCT project_folder FROM run_permissions \
         WHERE project_id IN ({marks}) ORDER BY project_folder"
    ))?;
    for folder in recorded.query_map(rusqlite::params_from_iter(ids.iter().copied()), |row| {
        row.get::<_, String>(0)
    })? {
        folders.insert(folder?);
    }
    let mut out = Vec::with_capacity(folders.len());
    for folder in folders {
        let standing = rp::standing(ledger, &folder)?;
        let latest_member = match &standing {
            Standing::Never => false,
            Standing::Granted(row) | Standing::Revoked(row) => members.contains(&row.project_id),
        };
        if !current.contains_key(&folder) && !latest_member {
            continue;
        }
        out.push(FolderEntry {
            current_change: current.get(&folder).cloned(),
            folder,
            standing,
        });
    }
    Ok(out)
}

fn state_word(state: &run_route::ConsentState) -> &'static str {
    match state {
        run_route::ConsentState::Given { .. } => "given",
        run_route::ConsentState::Never => "never",
        run_route::ConsentState::Revoked => "revoked",
        run_route::ConsentState::FileMissing => "file_missing",
        run_route::ConsentState::NotInRecord => "not_in_record",
    }
}

fn label(folder: &str, project: &core_hq::HqProject, id: &str) -> String {
    if folder == project.root {
        return "Project folder".to_string();
    }
    if core_hq::is_under(folder, &project.root) {
        if let Ok(relative) = Path::new(folder).strip_prefix(&project.root) {
            return thruline_core::redact::mask(&relative.display().to_string()).0;
        }
    }
    format!("Other worktree · {}", &id[..8])
}

fn get(
    ledger: &Ledger,
    state: &State,
    project: &core_hq::HqProject,
    stream: &mut TcpStream,
) -> Result<()> {
    let policy = match crate::exclusion_serve::policy(ledger, &project.root) {
        Ok(policy) => policy,
        Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    let entries = folder_entries(ledger, project)?;
    let mut raw_by_id = BTreeMap::new();
    let mut rows = Vec::with_capacity(entries.len());
    for entry in entries {
        let id = run_route::folder_id(&entry.folder);
        raw_by_id.insert(id.clone(), entry.folder.clone());
        let consent = run_route::consent_for_standing(
            &state.db,
            Path::new(&entry.folder),
            entry.standing.clone(),
        );
        let latest = match &entry.standing {
            Standing::Never => None,
            Standing::Granted(row) | Standing::Revoked(row) => Some(row),
        };
        let can_revoke = entry.current_change.is_some()
            || run_route::permission_root_usable(Path::new(&entry.folder), true);
        let can_grant = entry.current_change.is_some()
            && run_route::permission_grant_ready(Path::new(&entry.folder));
        let action = match (&consent, entry.current_change.is_some(), &entry.standing) {
            (run_route::ConsentState::Given { .. }, _, _) if can_revoke => Some("revoke"),
            (run_route::ConsentState::Revoked, true, _) if can_grant => Some("grant"),
            (run_route::ConsentState::Revoked, false, _) => None,
            (run_route::ConsentState::Never, _, _) => None,
            (
                run_route::ConsentState::FileMissing | run_route::ConsentState::NotInRecord,
                true,
                _,
            ) if can_grant => Some("grant"),
            (
                run_route::ConsentState::FileMissing | run_route::ConsentState::NotInRecord,
                false,
                Standing::Granted(_),
            ) if can_revoke => Some("revoke"),
            _ => None,
        };
        let (actor, at) = match &consent {
            run_route::ConsentState::Given { grant } => (
                Some(thruline_core::redact::mask(&grant.actor).0),
                Some(grant.recorded_at.clone()),
            ),
            _ => (None, None),
        };
        let latest_row = latest.map(|row| row.id.clone());
        let historical = entry.current_change.is_none();
        rows.push(serde_json::json!({
            "id": id,
            "folder": entry.folder,
            "state": state_word(&consent),
            "actor": actor,
            "at": at,
            "change_id": entry.current_change,
            "latest_row": latest_row,
            "historical": historical,
            "action": action,
        }));
    }
    let mut served = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "project": project.id.as_str(),
        "folders": rows,
    });
    if crate::exclusion_serve::checked_apply(&mut served, &policy).is_err() {
        return refuse(stream, 503, "exclusion_policy_unavailable");
    }
    for row in served["folders"].as_array_mut().expect("folder array") {
        let id = row["id"].as_str().expect("folder identity");
        let raw = raw_by_id.get(id).expect("folder identity held");
        if row["folder"].as_str() == Some(raw.as_str()) {
            row["folder"] = serde_json::Value::String(label(raw, project, id));
        }
    }
    json(stream, 200, &served)
}

pub(super) enum HistoricalTarget {
    NotFound,
    Moved,
    Found { folder: String, project_id: String },
}

/// Re-resolve an opaque historical target while the permission-act lock is
/// held. A current Change must use its own guarded address instead.
pub(super) fn historical_target(
    ledger: &Ledger,
    project: &core_hq::HqProject,
    id: &str,
) -> Result<HistoricalTarget> {
    let mut found = None;
    for entry in folder_entries(ledger, project)? {
        if run_route::folder_id(&entry.folder) != id {
            continue;
        }
        if found.is_some() {
            return Ok(HistoricalTarget::Moved);
        }
        if entry.current_change.is_some() {
            return Ok(HistoricalTarget::Moved);
        }
        let Standing::Granted(grant) = entry.standing else {
            return Ok(HistoricalTarget::Moved);
        };
        if !project
            .members
            .iter()
            .any(|member| member.ledger_project_id == grant.project_id)
        {
            return Ok(HistoricalTarget::NotFound);
        }
        found = Some((entry.folder, grant.project_id));
    }
    Ok(match found {
        Some((folder, project_id)) => HistoricalTarget::Found { folder, project_id },
        None => HistoricalTarget::NotFound,
    })
}
