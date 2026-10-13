//! Exact, read-only binding for a person retiring one accepted decision for another.
use crate::{
    chain::sha256_hex,
    decisions::{authority::PersonActKind, params::parse_params},
    error::{CoreError, Result},
    ledger::Ledger,
    redact::{resembles_known_credential, Masked},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

struct Accepted {
    id: String,
    decision: String,
    project: String,
    revision: u32,
    kind: String,
    schema: u32,
    bytes: String,
    version: String,
    order: i64,
}

fn latest(conn: &Connection, decision: &str, deadline: Instant) -> Result<Accepted> {
    let mut query=conn.prepare("SELECT a.id,d.project_id,a.rev,r.kind,r.params_schema_version,r.params_json,a.applies_from_version,a.rowid FROM decision_acceptances a JOIN decisions d ON d.id=a.decision_id JOIN decision_revisions r ON r.decision_id=a.decision_id AND r.rev=a.rev WHERE a.decision_id=?1 ORDER BY a.rowid DESC LIMIT 257")?;
    let rows = query
        .query_map([decision], |r| {
            Ok(Accepted {
                id: r.get(0)?,
                project: r.get(1)?,
                decision: decision.into(),
                revision: r.get(2)?,
                kind: r.get(3)?,
                schema: r.get(4)?,
                bytes: r.get(5)?,
                version: r.get(6)?,
                order: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > 256 {
        return Err(CoreError::Config(
            "supersession acceptance census exceeds its bound".into(),
        ));
    }
    for accepted in rows {
        if Instant::now() >= deadline {
            break;
        }
        if super::verified_on_connection(conn, PersonActKind::Accept, &accepted.id, Some(deadline))?
        {
            parse_params(&accepted.kind, accepted.schema, &accepted.bytes)?;
            return Ok(accepted);
        }
    }
    Err(CoreError::Config(
        "supersession needs readable current person acceptance evidence".into(),
    ))
}

fn add_side(metadata: &mut Value, prefix: &str, side: &Accepted) -> Result<()> {
    let object = metadata
        .as_object_mut()
        .ok_or_else(|| CoreError::Integrity("supersession metadata is not an object".into()))?;
    for (key, value) in [
        ("decision_id", json!(side.decision)),
        ("acceptance_id", json!(side.id)),
        ("revision", json!(side.revision)),
        ("kind", json!(side.kind)),
        ("params_schema_version", json!(side.schema)),
        ("params_sha256", json!(sha256_hex(side.bytes.as_bytes()))),
        ("applies_from_version", json!(side.version)),
    ] {
        object.insert(format!("{prefix}_{key}"), value);
    }
    Ok(())
}

pub(super) fn derive(
    conn: &Connection,
    retirement: &str,
    deadline: Option<Instant>,
) -> Result<Option<Value>> {
    let until = deadline.map_or_else(
        || Instant::now() + Duration::from_secs(5),
        |d| d.min(Instant::now() + Duration::from_secs(5)),
    );
    let row:Option<(String,String,Option<String>)>=conn.query_row("SELECT d.project_id,r.decision_id,r.superseded_by FROM decision_retirements r JOIN decisions d ON d.id=r.decision_id WHERE r.id=?1",[retirement],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((project, predecessor, successor)) = row else {
        return Err(CoreError::Config(
            "supersession retirement is unavailable".into(),
        ));
    };
    let Some(successor) = successor else {
        return Ok(None);
    };
    if predecessor == successor {
        return Err(CoreError::Config(
            "supersession requires different decisions".into(),
        ));
    }
    let old = latest(conn, &predecessor, until)?;
    let new = latest(conn, &successor, until)?;
    if old.project != project
        || new.project != project
        || new.order <= old.order
        || Instant::now() >= until
    {
        return Err(CoreError::Config(
            "supersession requires a later current acceptance in the same project".into(),
        ));
    }
    let mut metadata =
        json!({"schema_version":1,"kind":"general_decision_supersession","project_id":project});
    add_side(&mut metadata, "predecessor", &old)?;
    add_side(&mut metadata, "successor", &new)?;
    Ok(Some(metadata))
}

/// The review displays both exact stored revisions, with the existing safe
/// parameter-review checks. These fields add no confirmation authority.
pub(super) fn review_details(ledger: &Ledger, metadata: &Value) -> Result<Value> {
    let mut details = json!({});
    for prefix in ["predecessor", "successor"] {
        let id = metadata[format!("{prefix}_decision_id")]
            .as_str()
            .ok_or_else(|| CoreError::Integrity("supersession has no decision ID".into()))?;
        let revision = metadata[format!("{prefix}_revision")]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| CoreError::Integrity("supersession has no revision".into()))?;
        let (kind,schema,bytes,text):(String,u32,String,String)=ledger.conn().query_row("SELECT kind,params_schema_version,params_json,text FROM decision_revisions WHERE decision_id=?1 AND rev=?2",params![id,revision],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        let version = metadata[format!("{prefix}_applies_from_version")]
            .as_str()
            .ok_or_else(|| CoreError::Integrity("supersession has no applicable version".into()))?;
        let project = metadata["project_id"]
            .as_str()
            .ok_or_else(|| CoreError::Integrity("supersession has no project".into()))?;
        let parsed = parse_params(&kind, schema, &bytes)?;
        if metadata[format!("{prefix}_params_sha256")].as_str()
            != Some(sha256_hex(bytes.as_bytes()).as_str())
            || Masked::mask(&text).into_text() != text
            || resembles_known_credential(&text)
            || !crate::decisions::review_params::safe_for_terminal(
                ledger, project, &parsed, &bytes, version,
            )?
        {
            return Err(CoreError::Config(
                "supersession review contains changed or masked values".into(),
            ));
        }
        details[prefix] = json!({"decision_id":id,"acceptance_id":metadata[format!("{prefix}_acceptance_id")],"revision":revision,"kind":kind,"params_schema_version":schema,"params":serde_json::from_str::<Value>(&bytes)?,"params_sha256":sha256_hex(bytes.as_bytes()),"text":text,"applies_from_version":version});
    }
    Ok(details)
}

#[cfg(test)]
#[path = "general_supersession_mask_tests.rs"]
mod mask_tests;
