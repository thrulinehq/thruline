//! HQ Next — what a person did, kept as record (C30 R-LEDGER). This file is the
//! home of the routes that append it; the first is notes, the second seconds,
//! the third told-the-agent, the fourth objections.
//!
//!   - `POST /api/changes/<id>/notes` — keep one note on a Change (row 21 of
//!     the write-address table in CLAUDE.md).
//!   - `POST /api/changes/<id>/seconds` — a second person puts their name on
//!     the version the standing approval names (row 22).
//!   - `POST /api/changes/<id>/seconds/withdraw` — that person takes their own
//!     second back (row 23).
//!   - `POST /api/hq/projects/<id>/threads/<tid>/told-agent` — a person copied
//!     a message for the agent, said they pasted it, or took that back (row 24).
//!     Dispatched from `hq::handle`; nothing is sent anywhere.
//!   - `POST /api/hq/projects/<id>/objections` — a person says one of the
//!     agent's statements is wrong, and why (row 25). Dispatched from
//!     `hq::handle`; changes no verdict.
//!
//! A note is words, and a second is a name. Nothing this module writes is read
//! by a verdict, a coverage count, readiness, or the choice of which approval
//! stands, and no route here asks whether approving is open: both are free
//! (C34 ruling Q2). Each route calls the same core function its terminal door
//! calls (`change_note::add` and `thruline change note`;
//! `change_decision_second::second` / `withdraw` and `thruline change second`).
//!
//! The Approve note does not come through this route: `note` rides on the
//! existing `POST …/decisions` and `…/decisions/withdraw` requests and is
//! appended by the act itself, inside its transaction (`change_decision`).

use super::*;
use thruline_core::change_note::{self, NoteProblem, Refusal, MAX_NOTE_CHARS};

/// The body cap of the three routes that can carry a note, sized from the one
/// limit both doors hold instead of the 512 bytes the other action routes keep:
/// the note at `MAX_NOTE_CHARS` characters, every character written the longest
/// way JSON can write it (a `\uXXXX\uXXXX` surrogate pair, twelve bytes), plus
/// room for the fields nobody types at length. A body over even this could not
/// hold a note within the limit, so it is refused with the same code,
/// `too_long`.
pub(super) const BODY_MAX: usize = MAX_NOTE_CHARS * 12 + 4096;

fn refuse(stream: &mut TcpStream, status: u16, error: &str) -> Result<()> {
    respond(
        stream,
        status,
        "application/json; charset=utf-8",
        serde_json::to_vec(&serde_json::json!({ "error": error }))?.as_slice(),
    )
}

/// A successful write can echo a person's note, copied draft, objection or
/// changed-file list. Keep the stored act intact, but apply today's path
/// exclusions before any of those display bytes reach the socket.
fn presented_success(
    stream: &mut TcpStream,
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
    value: serde_json::Value,
) -> Result<()> {
    let raw = serde_json::to_vec(&value)?;
    match crate::exclusion_serve::bytes(&raw, policy) {
        Ok(body) => respond(stream, 200, "application/json; charset=utf-8", &body),
        Err(_) => refuse(stream, 503, "exclusion_policy_unavailable"),
    }
}

/// `409 too_long` naming the field that is over its limit.
pub(super) fn too_long(stream: &mut TcpStream, field: &str, limit: usize) -> Result<()> {
    respond(
        stream,
        409,
        "application/json; charset=utf-8",
        serde_json::to_vec(&serde_json::json!({
            "error": "too_long",
            "field": field,
            "limit": limit,
        }))?
        .as_slice(),
    )
}

/// One JSON body of at most [`BODY_MAX`] bytes, or the refusal already sent:
/// `400 malformed_request` for a body that is not framed as the action routes
/// require, `413 too_long` for one larger than a note within the limit could
/// make. `None` means the answer has been written and the route is done.
pub(super) fn read_body(
    reader: &mut BufReader<TcpStream>,
    headers: &RequestHeaders,
    stream: &mut TcpStream,
) -> Result<Option<Vec<u8>>> {
    read_body_capped(reader, headers, stream, BODY_MAX)
}

/// [`read_body`] with the cap of the route that asks: a route whose longest
/// typed part is a name does not accept a note's worth of bytes.
pub(super) fn read_body_capped(
    reader: &mut BufReader<TcpStream>,
    headers: &RequestHeaders,
    stream: &mut TcpStream,
    cap: usize,
) -> Result<Option<Vec<u8>>> {
    match read_json_body(reader, headers, cap) {
        Ok(body) => Ok(Some(body)),
        Err(BodyProblem::BadRequest) => refuse(stream, 400, "malformed_request").map(|()| None),
        Err(BodyProblem::TooLarge) => refuse(stream, 413, "too_long").map(|()| None),
    }
}

/// `POST /api/changes/<id>/notes` — one person keeps one note on a Change.
///
/// The body is `{"body": "<words>"}` and nothing else (an unknown field is
/// `400 malformed_request`). The name on the note is the account ThruLine is
/// running as, exactly as the terminal door's default. Answers:
///
///   - `200 {schema_version, note}` — the note as served (masked), with who and when
///   - `400 malformed_request` — not the shape above
///   - `400 body_required` — nothing but whitespace
///   - `403 same_origin_required` — not from HQ's own page (the caller checks)
///   - `404 no_such_change`
///   - `409 too_long` `{field: "body", limit}` — over 2,000 characters
///   - `413 too_long` — a body larger than a note within the limit could make
///   - `503 upgrade_pending` — the store needs `thruline upgrade`
pub(super) fn add_note(
    state: &State,
    change_id: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        body: String,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return refuse(stream, 400, "malformed_request");
    };
    match change_note::check(&request.body) {
        Ok(()) => {}
        Err(NoteProblem::Empty) => return refuse(stream, 400, "body_required"),
        Err(NoteProblem::TooLong { .. }) => return too_long(stream, "body", MAX_NOTE_CHARS),
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    if thruline_core::change::read_change(&ledger, change_id)?.is_none() {
        return refuse(stream, 404, "no_such_change");
    }
    let policy = match super::change_action_policy(&ledger, change_id) {
        Ok(Some(policy)) => policy,
        _ => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    let actor = crate::check::account_name();
    let at = thruline_core::ingest::iso8601_utc_now();
    match change_note::add(&ledger, change_id, &actor, &request.body, &at)
        .context("keeping the note")?
    {
        Ok(note) => presented_success(
            stream,
            &policy,
            serde_json::json!({
                "schema_version": thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION,
                "note": note,
            }),
        ),
        Err(Refusal::NoSuchChange) => refuse(stream, 404, "no_such_change"),
        // `check` above has already answered both of these; they are here so a
        // refusal the core adds later is a compile error, not a silent 200.
        Err(Refusal::NoActor) => refuse(stream, 400, "actor_required"),
        Err(Refusal::Note(NoteProblem::Empty)) => refuse(stream, 400, "body_required"),
        Err(Refusal::Note(NoteProblem::TooLong { .. })) => too_long(stream, "body", MAX_NOTE_CHARS),
    }
}

/* ── second approvals ──────────────────────────────────────────────────── */

/// The body cap of the two seconds routes, sized from the longest things a
/// person types into them — a name and the version the screen was showing, each
/// at most [`DECISION_ACTOR_MAX_CHARS`] characters — every character written the
/// longest way JSON can write it (twelve bytes), plus room for the field names.
/// A body over this could not hold a name within the limit, so it is refused
/// with `413 too_long`.
pub(super) const SECONDS_BODY_MAX: usize = DECISION_ACTOR_MAX_CHARS * 2 * 12 + 1024;

/// `409` with a code and the sentence a person reads.
fn conflict(stream: &mut TcpStream, body: serde_json::Value) -> Result<()> {
    respond(
        stream,
        409,
        "application/json; charset=utf-8",
        serde_json::to_vec(&body)?.as_slice(),
    )
}

/// The Change's decisions after the act, as `…/decisions` answers them.
fn decisions_after(
    ledger: &thruline_core::ledger::Ledger,
    change_id: &str,
) -> Result<serde_json::Value> {
    let decisions = thruline_core::change_decision::read(ledger, change_id)
        .context("re-reading the decisions after the act")?;
    Ok(serde_json::to_value(decisions)?)
}

/// `POST /api/changes/<id>/seconds` — a second person puts their name on the
/// version the standing approval names (row 22 of the write-address table).
///
/// The body is `{"actor": "<name>", "seen_version": "<version>"}` and nothing
/// else. The name is TYPED, as on the approval routes: ThruLine records it and
/// does not check who is at the keyboard. This route never asks whether
/// approving is open — a second is free (C34 Q2) — and never makes an approval
/// stand. Answers:
///
///   - `200 {schema_version, acted: {outcome: "recorded", second}, decisions}`
///   - `200 {schema_version, acted: {outcome: "moved", seen, now, changed[],
///     message}, decisions}` — the screen was showing another version than the
///     approval's; nothing was recorded
///   - `400 malformed_request` — not the shape above (an unknown field too)
///   - `400 actor_required` — a name with nothing in it
///   - `403 same_origin_required` — not from HQ's own page (the caller checks)
///   - `404 no_such_change`
///   - `409 nothing_to_second {message}` — no approval stands
///   - `409 same_person {with, message}` — the name is the approval's actor or a
///     standing second's, compared trimmed and case-folded; `message` says the
///     name is not checked against anyone
///   - `409 too_long {field, limit}` — a name or version over 200 characters
///   - `413 too_long` — a body larger than a name within the limit could make
///   - `503 upgrade_pending` — the store needs `thruline upgrade`
pub(super) fn add_second(
    state: &State,
    change_id: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    use thruline_core::change_decision::Decide;
    use thruline_core::change_decision_second as seconds;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        actor: String,
        seen_version: String,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return refuse(stream, 400, "malformed_request");
    };
    let actor = request.actor.trim().to_string();
    if actor.is_empty() {
        return refuse(stream, 400, "actor_required");
    }
    if actor.chars().count() > DECISION_ACTOR_MAX_CHARS {
        return too_long(stream, "actor", DECISION_ACTOR_MAX_CHARS);
    }
    if request.seen_version.chars().count() > DECISION_ACTOR_MAX_CHARS {
        return too_long(stream, "seen_version", DECISION_ACTOR_MAX_CHARS);
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    if !seconds::exists(&ledger, change_id).context("looking for the Change")? {
        return refuse(stream, 404, "no_such_change");
    }
    let policy = match super::change_action_policy(&ledger, change_id) {
        Ok(Some(policy)) => policy,
        _ => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    let decide = Decide {
        actor,
        at: thruline_core::ingest::iso8601_utc_now(),
        seen_revision: request.seen_version,
        note: None,
    };
    let schema = thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION;
    match seconds::second(&ledger, change_id, &decide).context("recording the second")? {
        seconds::Seconding::Recorded { second } => presented_success(
            stream,
            &policy,
            serde_json::json!({
                "schema_version": schema,
                "acted": { "outcome": "recorded", "second": second.served(true) },
                "decisions": decisions_after(&ledger, change_id)?,
            }),
        ),
        seconds::Seconding::Moved { seen, now, changed } => presented_success(
            stream,
            &policy,
            serde_json::json!({
                "schema_version": schema,
                "acted": {
                    "outcome": "moved",
                    "seen": seconds::masked_text(&seen),
                    "now": seconds::masked_text(&now),
                    "changed": changed
                        .iter()
                        .map(|path| seconds::masked_text(path))
                        .collect::<Vec<_>>(),
                    "message": seconds::MOVED,
                },
                "decisions": decisions_after(&ledger, change_id)?,
            }),
        ),
        seconds::Seconding::NothingToSecond => conflict(
            stream,
            serde_json::json!({
                "error": "nothing_to_second",
                "message": seconds::NOTHING_TO_SECOND,
            }),
        ),
        seconds::Seconding::SamePerson { with } => {
            let with = seconds::masked_text(&with);
            conflict(
                stream,
                serde_json::json!({
                    "error": "same_person",
                    "with": with,
                    "message": seconds::same_person_sentence(&with),
                }),
            )
        }
    }
}

/// `POST /api/changes/<id>/seconds/withdraw` — a person takes their own second
/// back (row 23 of the write-address table).
///
/// The body is `{"actor": "<name>"}` and nothing else. Answers:
///
///   - `200 {schema_version, acted: {outcome: "recorded", withdrawal}, decisions}`
///   - `400 malformed_request` / `400 actor_required`
///   - `403 same_origin_required` (the caller checks)
///   - `404 no_such_change`
///   - `409 no_second_to_withdraw {message}` — that name has no second standing
///     on the approval that stands
///   - `409 too_long {field: "actor", limit}` / `413 too_long`
///   - `503 upgrade_pending`
pub(super) fn withdraw_second(
    state: &State,
    change_id: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    use thruline_core::change_decision_second as seconds;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        actor: String,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return refuse(stream, 400, "malformed_request");
    };
    let actor = request.actor.trim().to_string();
    if actor.is_empty() {
        return refuse(stream, 400, "actor_required");
    }
    if actor.chars().count() > DECISION_ACTOR_MAX_CHARS {
        return too_long(stream, "actor", DECISION_ACTOR_MAX_CHARS);
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    if !seconds::exists(&ledger, change_id).context("looking for the Change")? {
        return refuse(stream, 404, "no_such_change");
    }
    let policy = match super::change_action_policy(&ledger, change_id) {
        Ok(Some(policy)) => policy,
        _ => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    let at = thruline_core::ingest::iso8601_utc_now();
    match seconds::withdraw(&ledger, change_id, &actor, &at).context("taking the second back")? {
        seconds::Withdrawing::Recorded { withdrawal } => presented_success(
            stream,
            &policy,
            serde_json::json!({
                "schema_version": thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION,
                "acted": { "outcome": "recorded", "withdrawal": withdrawal.served(false) },
                "decisions": decisions_after(&ledger, change_id)?,
            }),
        ),
        seconds::Withdrawing::NoSecondToWithdraw => conflict(
            stream,
            serde_json::json!({
                "error": "no_second_to_withdraw",
                "message": seconds::no_second_to_withdraw_sentence(&seconds::masked_text(&actor)),
            }),
        ),
    }
}

/* ── told the agent ────────────────────────────────────────────────────── */

/// The body cap of the told-the-agent route, sized from the one limit it holds
/// — a message of at most `MAX_MESSAGE_CHARS` characters, every character
/// written the longest way JSON can write it (twelve bytes) — plus room for the
/// act and the id of the row it refers to. A body over this could not hold a
/// message within the limit, so it is refused with `413 too_long`.
pub(super) const TOLD_AGENT_BODY_MAX: usize =
    thruline_core::change_agent_message::MAX_MESSAGE_CHARS * 12 + 4096;

/// `POST /api/hq/projects/<id>/threads/<tid>/told-agent` — a person copied a
/// message for the agent, said they pasted it, or took that back (row 24 of the
/// write-address table). One `change_agent_messages` row. **Nothing is sent
/// anywhere**: the table has no act that could say so, and every answer carries
/// the sentence that says so.
///
/// The body is `{"act": "copied" | "marked_pasted" | "unmarked_pasted",
/// "message"?, "refers_to"?}` and nothing else. `copied` needs `message` (kept
/// as written, at most 8,000 characters; its SHA-256 is computed here) and takes
/// no `refers_to`; the other two take no `message`, and `refers_to` is optional
/// (the latest copy, or the latest standing mark, is meant when it is absent).
/// The thread's anchor event, the version it is at and the Change that tracks it
/// are read by the server from the project's own index; the client sends none of
/// them. The name on the act is the account ThruLine is running as. Free: this
/// route never asks the licence gate (approving a version is the one paid act).
/// Answers:
///
///   - `200 {schema_version, act, told_agent, nothing_sent, message}` — the act
///     as served (masked) and the thread's `told_agent` after it
///   - `400 malformed_request` — not the shape above (an unknown field, a
///     `message` on a mark, a `refers_to` on a copy)
///   - `400 unknown_act` — an act the table does not have
///   - `400 message_required` — a copy with nothing but whitespace, or none
///   - `403 same_origin_required` — not from HQ's own page (the caller checks)
///   - `404 no_such_project` / `404 no_such_thread`
///   - `409 too_long {field: "message", limit}` — over 8,000 characters
///   - `409 no_copy_to_mark` / `already_marked` / `no_mark_to_undo {message}`
///   - `413 too_long` — a body larger than a message within the limit could make
///   - `503 upgrade_pending` — the store needs `thruline upgrade`
///   - `503 store_busy` — another ThruLine process held the write lock for 30 s
pub(super) fn told_agent(
    state: &State,
    project: &str,
    thread: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    use thruline_core::change_agent_message as told;
    use thruline_core::hq as core_hq;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        act: String,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        refers_to: Option<String>,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return refuse(stream, 400, "malformed_request");
    };
    let Some(act) = told::Act::parse(&request.act) else {
        return refuse(stream, 400, "unknown_act");
    };
    match act {
        told::Act::Copied => {
            if request.refers_to.is_some() {
                return refuse(stream, 400, "malformed_request");
            }
            let Some(message) = request.message.as_deref() else {
                return refuse(stream, 400, "message_required");
            };
            match told::check_message(message) {
                Ok(()) => {}
                Err(told::Refusal::TooLong { .. }) => {
                    return too_long(stream, "message", told::MAX_MESSAGE_CHARS)
                }
                Err(_) => return refuse(stream, 400, "message_required"),
            }
        }
        told::Act::MarkedPasted | told::Act::UnmarkedPasted => {
            if request.message.is_some() {
                return refuse(stream, 400, "malformed_request");
            }
        }
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    ledger
        .wait_when_busy(super::hq::BUSY_WAIT)
        .context("waiting for the record's write lock")?;
    let Some(hq_project) =
        core_hq::find_project(&ledger, project).context("looking for the project")?
    else {
        return refuse(stream, 404, "no_such_project");
    };
    let policy = match crate::exclusion_serve::policy(&ledger, &hq_project.root) {
        Ok(policy) => policy,
        Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    let index = super::hq::cached_index(state, &ledger, &hq_project)
        .context("reading the project's threads")?;
    let Some(found) = index.thread_identity(thread) else {
        return refuse(stream, 404, "no_such_thread");
    };
    let change_id = told::tracking_change(
        &ledger,
        &found.ledger_project_id,
        &found.first_prompt_event_id,
    )
    .context("looking for the Change that tracks this thread")?;
    let anchor = told::Anchor {
        thread_id: found.id.clone(),
        thread_event_id: found.first_prompt_event_id.clone(),
        change_id,
        version_id: found.current_version_id.clone(),
    };
    let actor = crate::check::account_name();
    let at = thruline_core::ingest::iso8601_utc_now();
    let refers_to = request.refers_to.as_deref();
    let written = match act {
        told::Act::Copied => told::copied(
            &ledger,
            &anchor,
            &actor,
            request.message.as_deref().unwrap_or_default(),
            &at,
        ),
        told::Act::MarkedPasted => told::marked_pasted(&ledger, &anchor, &actor, refers_to, &at),
        told::Act::UnmarkedPasted => {
            told::unmarked_pasted(&ledger, &anchor, &actor, refers_to, &at)
        }
    };
    let outcome = match written {
        Ok(outcome) => outcome,
        Err(problem) if problem.is_busy() => {
            return super::hq::store_busy(stream, super::hq::BUSY_WAIT)
        }
        Err(problem) => {
            return Err(anyhow::Error::new(problem).context("keeping the told-the-agent act"))
        }
    };
    match outcome {
        Ok(kept) => {
            let after = told::last_acts(&ledger)
                .context("reading the thread's last act")?
                .remove(&found.id)
                .and_then(|last| index.told_agent(&found.id, &last));
            presented_success(
                stream,
                &policy,
                serde_json::json!({
                    "schema_version": core_hq::SCHEMA_VERSION,
                    "act": kept,
                    "told_agent": after,
                    "nothing_sent": true,
                    "message": told::NOTHING_SENT,
                }),
            )
        }
        Err(told::Refusal::NoSuchThread) => refuse(stream, 404, "no_such_thread"),
        Err(told::Refusal::NoActor) => refuse(stream, 400, "actor_required"),
        Err(told::Refusal::EmptyMessage) => refuse(stream, 400, "message_required"),
        Err(told::Refusal::TooLong { .. }) => too_long(stream, "message", told::MAX_MESSAGE_CHARS),
        Err(refusal) => {
            let mark = match &refusal {
                told::Refusal::AlreadyMarked { mark } => Some(mark.clone()),
                _ => None,
            };
            conflict(
                stream,
                serde_json::json!({
                    "error": refusal.code(),
                    "message": refusal.plain(),
                    "mark": mark,
                    "nothing_sent": true,
                }),
            )
        }
    }
}

/* ── objections ────────────────────────────────────────────────────────── */

/// The body cap of the objections route, sized from the one limit it holds — a
/// reason of at most `MAX_REASON_CHARS` characters, every character written the
/// longest way JSON can write it (twelve bytes) — plus room for the statement
/// id and the field names. A body over this could not hold a reason within the
/// limit, so it is refused with `413 too_long`.
pub(super) const OBJECTION_BODY_MAX: usize =
    thruline_core::statement_objection::MAX_REASON_CHARS * 12 + 4096;

/// `POST /api/hq/projects/<id>/objections` — a person says one of the agent's
/// statements is wrong, and why (row 25 of the write-address table). One
/// `statement_objections` row. **It changes no verdict**: the table has no
/// column that could hold one, nothing in the engine reads it, and the answer
/// carries the sentence that says so.
///
/// The body is `{"statement": "<events.id>#<n>", "reason": "<words>"}` and
/// nothing else (an unknown field — a digest, a sentence, a verdict — is
/// `400 malformed_request`). The statement must be one this project's own index
/// serves; the SERVER reads the sentence the extractor gives for it now and
/// keeps its SHA-256, so the client never sends either. The reason is kept
/// exactly as typed. The name on the objection is the account ThruLine is
/// running as. Free: this route never asks the licence gate (approving a
/// version is the one paid act). There is no terminal door and no route to take
/// an objection back (C34 ruling Q16). Answers:
///
///   - `200 {schema_version, statement, objection, message}` — the objection as
///     served (masked), `message` being `KEPT_BESIDE`
///   - `400 malformed_request` — not the shape above
///   - `400 reason_required` — a reason with nothing but whitespace
///   - `403 same_origin_required` — not from HQ's own page (the caller checks)
///   - `404 no_such_project` / `404 no_such_statement`
///   - `409 too_long {field: "reason", limit}` — over 2,000 characters
///   - `413 too_long` — a body larger than a reason within the limit could make
///   - `503 upgrade_pending` — the store needs `thruline upgrade`
///   - `503 store_busy` — another ThruLine process held the write lock for 30 s
pub(super) fn objection(
    state: &State,
    project: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    use thruline_core::hq as core_hq;
    use thruline_core::statement_objection as objections;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        statement: String,
        reason: String,
    }
    let Ok(request) = serde_json::from_slice::<Request>(body) else {
        return refuse(stream, 400, "malformed_request");
    };
    match objections::check_reason(&request.reason) {
        Ok(()) => {}
        Err(objections::Refusal::TooLong { .. }) => {
            return too_long(stream, "reason", objections::MAX_REASON_CHARS)
        }
        Err(_) => return refuse(stream, 400, "reason_required"),
    }
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    ledger
        .wait_when_busy(super::hq::BUSY_WAIT)
        .context("waiting for the record's write lock")?;
    let Some(hq_project) =
        core_hq::find_project(&ledger, project).context("looking for the project")?
    else {
        return refuse(stream, 404, "no_such_project");
    };
    let policy = match crate::exclusion_serve::policy(&ledger, &hq_project.root) {
        Ok(policy) => policy,
        Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    let index = super::hq::cached_index(state, &ledger, &hq_project)
        .context("reading the project's statements")?;
    let Some(claim) = index.claim_sentence(&request.statement) else {
        return refuse(stream, 404, "no_such_statement");
    };
    let actor = crate::check::account_name();
    let at = thruline_core::ingest::iso8601_utc_now();
    let written = match objections::object(&ledger, &claim, &actor, &request.reason, &at) {
        Ok(written) => written,
        Err(problem) if problem.is_busy() => {
            return super::hq::store_busy(stream, super::hq::BUSY_WAIT)
        }
        Err(problem) => return Err(anyhow::Error::new(problem).context("keeping the objection")),
    };
    match written {
        Ok(kept) => presented_success(
            stream,
            &policy,
            serde_json::json!({
                "schema_version": core_hq::SCHEMA_VERSION,
                "statement": claim.statement_id,
                "objection": kept,
                "message": objections::KEPT_BESIDE,
            }),
        ),
        Err(objections::Refusal::NoSuchStatement) => refuse(stream, 404, "no_such_statement"),
        Err(objections::Refusal::NoActor) => refuse(stream, 400, "actor_required"),
        Err(objections::Refusal::EmptyReason) => refuse(stream, 400, "reason_required"),
        Err(objections::Refusal::TooLong { .. }) => {
            too_long(stream, "reason", objections::MAX_REASON_CHARS)
        }
    }
}
