//! C1.2 — the lifecycle appender pack (A168).
//!
//! Event-driven capture of Claude Code hook deliveries: the vendor's own
//! process invokes a momentary `thruline sensor append <Event>` that appends
//! one row and exits. No daemon, no poller, nothing survives the call.
//!
//! Three rules give the module its shape:
//!
//! 1. **The probe is the contract.** The six [`PINNED_EVENTS`] and the
//!    [`SPINE`] keys are exactly what A166 measured on the installed binary
//!    (2.1.227), pinned by `tests/c1_hooks_probe.rs`. [`DORMANT_EVENTS`]
//!    ships wired but dormant — registered in the hook set, recorded if it
//!    ever arrives, its unconfirmed status stated rather than assumed.
//! 2. **Verbatim or nothing.** The payload is stored byte-for-byte as
//!    received, content-addressed, on its own machine-scoped hash chain.
//!    A payload that fails to parse, or arrives without its spine, is still
//!    recorded — flagged in the row, never dropped (Rule 12).
//! 3. **Observations feed nothing.** These are Layer-1 rows. No status, no
//!    verdict, no attention item derives from them until C8's shadow gate;
//!    the containment test walks core for leaks.

use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::Value;
use ulid::Ulid;

use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::error::Result;
use crate::ledger::Ledger;

/// The events measured key-for-key on this machine: six at A166, PreCompact
/// at A170 (trigger=manual fired live; exit-2 veto measured to block
/// compaction — a power this sensor never uses, its exit-0 absolute stands).
pub const PINNED_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "SessionEnd",
    "PreCompact",
];

/// Wired but never observed. The registration exists so a first delivery is
/// captured the moment it happens; nothing is claimed about it until then.
pub const DORMANT_EVENTS: &[&str] = &["StopFailure"];

/// The sentence the install plan prints beside every dormant event.
pub const DORMANT_NOTE: &str = "registered dormant — never observed on this machine \
     (Claude Code 2.1.227, A166); delivery unconfirmed, and nothing downstream reads it";

/// The keys every measured delivery carried (A166). A row missing any of
/// these is recorded with `spine_complete = false` — a fact about the
/// delivery, not a reason to drop it.
pub const SPINE: &[&str] = &["session_id", "transcript_path", "hook_event_name"];

/// The full install set: pinned first, dormant after.
pub fn install_set() -> Vec<&'static str> {
    PINNED_EVENTS
        .iter()
        .chain(DORMANT_EVENTS)
        .copied()
        .collect()
}

/// The chain scope. One machine-wide chain rather than per-session: a hook
/// can fire before its session row exists anywhere, and the chain must not
/// depend on a field the payload might lack.
const CHAIN_SCOPE: &str = "sensor";

/// One appended delivery, as stored.
#[derive(Debug, Clone, Serialize)]
pub struct SensorRecord {
    pub id: String,
    pub seq: i64,
    pub received_at: String,
    pub hook_event: String,
    pub recognized: bool,
    pub parse_ok: bool,
    pub spine_complete: bool,
    pub session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub cwd: Option<String>,
    pub payload_sha256: String,
    pub chain_hash: String,
}

/// Append one hook delivery. Total: malformed payloads, unknown event names,
/// and spineless deliveries are all recorded with their defect flagged in the
/// row. The only error out of here is the database itself refusing.
pub fn append(
    ledger: &Ledger,
    hook_event: &str,
    payload: &[u8],
    received_at: &str,
) -> Result<SensorRecord> {
    let text = String::from_utf8_lossy(payload).into_owned();
    let parsed: Option<Value> = serde_json::from_str(&text).ok();
    let parse_ok = parsed.is_some();
    let field = |key: &str| -> Option<String> {
        parsed
            .as_ref()
            .and_then(|v| v.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let spine_complete = parsed.as_ref().is_some_and(|v| {
        SPINE
            .iter()
            .all(|k| v.get(k).and_then(Value::as_str).is_some())
    });
    let record = SensorRecord {
        id: Ulid::generate().to_string(),
        seq: 0, // assigned inside the transaction below
        received_at: received_at.to_string(),
        hook_event: hook_event.to_string(),
        recognized: PINNED_EVENTS.contains(&hook_event) || DORMANT_EVENTS.contains(&hook_event),
        parse_ok,
        spine_complete,
        session_id: field("session_id"),
        transcript_path: field("transcript_path"),
        cwd: field("cwd"),
        payload_sha256: sha256_hex(payload),
        chain_hash: String::new(), // computed inside the transaction below
    };

    let conn = ledger.conn();
    // Two hooks can land back to back (Stop, then SessionEnd); the appender
    // waits briefly rather than losing the later one to SQLITE_BUSY.
    conn.busy_timeout(std::time::Duration::from_millis(3_000))?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<SensorRecord> {
        let head: Option<(String, i64)> = conn
            .query_row(
                "SELECT chain_hash, seq FROM sensor_events ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (prev_hash, seq) = match head {
            Some((hash, last)) => (hash, last + 1),
            None => (genesis_prev_hash(CHAIN_SCOPE), 0),
        };
        let chain = chain_hash(&prev_hash, &record.payload_sha256, CHAIN_SCOPE, seq);
        conn.execute(
            "INSERT INTO sensor_events(id, seq, received_at, hook_event, recognized, parse_ok,
                spine_complete, session_id, transcript_path, cwd, payload, payload_sha256,
                prev_hash, chain_hash)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            rusqlite::params![
                record.id,
                seq,
                record.received_at,
                record.hook_event,
                record.recognized,
                record.parse_ok,
                record.spine_complete,
                record.session_id,
                record.transcript_path,
                record.cwd,
                text,
                record.payload_sha256,
                prev_hash,
                chain,
            ],
        )?;
        Ok(SensorRecord {
            seq,
            chain_hash: chain,
            ..record
        })
    })();
    match outcome {
        Ok(stored) => {
            conn.execute_batch("COMMIT")?;
            Ok(stored)
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// Re-fold the whole chain and compare it link by link. Returns the number of
/// rows checked; any mismatch is an error naming the seq that broke.
pub fn verify_chain(ledger: &Ledger) -> Result<u64> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT seq, payload, payload_sha256, prev_hash, chain_hash
         FROM sensor_events ORDER BY seq",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut prev = genesis_prev_hash(CHAIN_SCOPE);
    let mut checked = 0u64;
    for row in rows {
        let (seq, payload, stored_sha, stored_prev, stored_chain) = row?;
        let sha = sha256_hex(payload.as_bytes());
        let expect = chain_hash(&prev, &sha, CHAIN_SCOPE, seq);
        if sha != stored_sha || stored_prev != prev || stored_chain != expect {
            return Err(crate::error::CoreError::Integrity(format!(
                "sensor chain does not add up at seq {seq}"
            )));
        }
        prev = stored_chain;
        checked += 1;
    }
    Ok(checked)
}

/// One recorded delivery, exactly as the appender stored it.
///
/// Read-only, and deliberately a plain owned struct rather than a borrow of the
/// row: the one consumer is the §4.1 bridge, which has to hold every field
/// while it writes a spool line, and a lifetime here would buy nothing.
///
/// `session_id`, `transcript_path` and `cwd` are `Option` because the table
/// declares them nullable — a delivery whose spine did not arrive is still
/// recorded (Rule 12, and `spine_complete` says so). A consumer must decide
/// what to do about that rather than being handed a fabricated value.
#[derive(Debug, Clone)]
pub struct Delivery {
    pub seq: i64,
    pub received_at: String,
    pub hook_event: String,
    pub recognized: bool,
    pub parse_ok: bool,
    pub spine_complete: bool,
    pub session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub cwd: Option<String>,
    /// The vendor's hook payload, byte-for-byte as received.
    pub payload: String,
    pub payload_sha256: String,
}

/// Every delivery recorded after `after_seq`, oldest first.
///
/// `seq` is the append-only, gapless per-machine ordinal the chain is built on,
/// so `after_seq` is a durable watermark: a caller that has consumed up to `n`
/// resumes at `n` and cannot miss or repeat a row. Pass `0` for all of them.
///
/// This reads; it never writes. The `sensor_events` triggers forbid update and
/// delete, so the rows behind a watermark are immutable by construction and a
/// resumed read is a pure function of the watermark.
pub fn since(ledger: &Ledger, after_seq: i64) -> Result<Vec<Delivery>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT seq, received_at, hook_event, recognized, parse_ok, spine_complete,
                session_id, transcript_path, cwd, payload, payload_sha256
         FROM sensor_events WHERE seq > ?1 ORDER BY seq",
    )?;
    let rows = stmt.query_map([after_seq], |r| {
        Ok(Delivery {
            seq: r.get(0)?,
            received_at: r.get(1)?,
            hook_event: r.get(2)?,
            recognized: r.get::<_, i64>(3)? != 0,
            parse_ok: r.get::<_, i64>(4)? != 0,
            spine_complete: r.get::<_, i64>(5)? != 0,
            session_id: r.get(6)?,
            transcript_path: r.get(7)?,
            cwd: r.get(8)?,
            payload: r.get(9)?,
            payload_sha256: r.get(10)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// The spill sidecar beside a ledger: deliveries that could not reach the
/// chain (a busy timeout against a long writer, a pending migration) and were
/// appended here verbatim by the CLI appender instead of being lost.
pub fn spill_path(db: &std::path::Path) -> std::path::PathBuf {
    db.with_file_name("sensor-spill.jsonl")
}

/// How many spilled deliveries name this session.
///
/// This is the A201 spill guard's one question. A spilled PostToolUse row is
/// a delivery that happened and is NOT on the chain, so an evidence scan over
/// chained rows alone can read an honestly-evidenced claim as unproven — the
/// false-accusation vector. Any count above zero for the session under audit
/// must degrade a would-be no-proof verdict to silence.
///
/// Two deliberate asymmetries, both in the silence-safe direction:
///
/// * A **missing or unreadable file is zero spills known** — the guard exists
///   to stop accusations, and a guard that failed closed would silence every
///   machine whose spill file it cannot read, forever. Fail-open here cannot
///   fail-accuse, because the chained scan still runs and evidenced claims
///   still bind.
/// * A spill line whose payload cannot be parsed, or names no session, does
///   **not** count against any session. The CLI writer embeds the vendor's
///   payload verbatim, and a payload without a session id would have been
///   equally unattributable on the chain; counting it against every session
///   would mute the speaker machine-wide on one malformed line, permanently.
pub fn spilled_for_session(spill: &std::path::Path, session_id: &str) -> u64 {
    let Ok(text) = std::fs::read_to_string(spill) else {
        return 0;
    };
    let mut count = 0u64;
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let named = entry
            .get("payload")
            .and_then(Value::as_str)
            .and_then(|p| serde_json::from_str::<Value>(p).ok())
            .and_then(|p| {
                p.get("session_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
        if named.as_deref() == Some(session_id) {
            count += 1;
        }
    }
    count
}

/// Whether a SessionEnd delivery is on record for this session — the doctor's
/// close check. A fact about the rows, never a status.
pub fn has_close(ledger: &Ledger, session_id: &str) -> Result<bool> {
    let n: i64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM sensor_events WHERE hook_event='SessionEnd' AND session_id=?1",
        [session_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Row counts per hook event, for receipts and the ops surface. A count, not
/// a meaning (RB7): what arrived, never what it implies.
pub fn summary(ledger: &Ledger) -> Result<Vec<(String, u64)>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT hook_event, COUNT(*) FROM sensor_events GROUP BY hook_event ORDER BY hook_event",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (event, n) = row?;
        out.push((event, n.max(0) as u64));
    }
    Ok(out)
}
