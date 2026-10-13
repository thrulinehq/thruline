//! The Inbox's one read and append-only hide action. The browser names a
//! thread, row kind and the digest it actually read; the server resolves the
//! event anchor and recomputes the digest from its current HQ index.

use super::*;
use thruline_core::inbox_hide as core_hide;

pub(super) const BODY_MAX: usize = 512;

pub(super) fn read(state: &State, project: &str, stream: &mut TcpStream) -> Result<()> {
    let Some((ledger, index, _)) = find_with_fingerprint(state, project, stream)? else {
        return Ok(());
    };
    let acts = match core_hide::last_acts(&ledger) {
        Ok(acts) => acts,
        Err(e) => return failed(state, stream, e),
    };
    json(
        stream,
        200,
        &serde_json::json!({
            "schema_version": core_hq::SCHEMA_VERSION,
            "project": project,
            "rows": core_hide::current_rows(&index, now_ms(), &acts),
        }),
    )
}

pub(super) fn write(
    state: &State,
    project: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        act: String,
        row_kind: String,
        thread_id: String,
        expected_sha256: String,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return refuse(stream, 400, "malformed_request");
    };
    let (Some(act), Some(kind)) = (
        core_hide::Act::parse(&request.act),
        core_hide::RowKind::parse(&request.row_kind),
    ) else {
        return refuse(stream, 400, "malformed_request");
    };
    if request.thread_id.trim().is_empty()
        || request.expected_sha256.len() != 64
        || !request
            .expected_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return refuse(stream, 400, "malformed_request");
    }
    let Some((ledger, index, fp)) = find_with_fingerprint(state, project, stream)? else {
        return Ok(());
    };
    if let Err(e) = ledger.wait_when_busy(BUSY_WAIT) {
        return if e.is_busy() {
            store_busy(stream, BUSY_WAIT)
        } else {
            failed(state, stream, e)
        };
    }
    let actor = crate::check::account_name();
    let at = thruline_core::ingest::iso8601_utc_now();
    let result = core_hide::keep(
        &ledger,
        &index,
        core_hide::KeepRequest {
            index_fp: &fp,
            thread_id: &request.thread_id,
            kind,
            expected_sha256: &request.expected_sha256,
            now_ms: now_ms(),
            act,
            actor: &actor,
            at: &at,
        },
    );
    match result {
        Ok(Ok(row)) => json(
            stream,
            200,
            &serde_json::json!({
                "schema_version": core_hq::SCHEMA_VERSION,
                "project": project,
                "row": row,
            }),
        ),
        Ok(Err(reason)) => json(
            stream,
            match reason {
                core_hide::Refusal::NoSuchRow => 404,
                core_hide::Refusal::NoActor => 400,
                _ => 409,
            },
            &serde_json::json!({ "error": reason.code(), "message": reason.plain() }),
        ),
        Err(e) if e.is_busy() => store_busy(stream, BUSY_WAIT),
        Err(e) => failed(state, stream, e),
    }
}
