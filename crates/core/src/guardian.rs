//! C8 — Guardian phase 1. SHADOW MODE (A184).
//!
//! Conservative detectors over **measured signals only**: RB0's taxonomy
//! (A148) and C1's hook rows (A166–A170). Nothing here is a verdict; every
//! output is a labeled candidate carrying a reason code and the evidence rows
//! it stands on, and no primary surface may reach this module until a class
//! graduates by adjudication.
//!
//! **A false stall alarm in week one kills the product.** So the phase-1 rule
//! is not "detect as much as possible" — it is *emit nothing a row cannot
//! carry*, and say plainly which classes have no rows at all.
//!
//! # What the corpus actually supports (measured 2026-08-14)
//!
//! | class | status | why |
//! |---|---|---|
//! | crash candidate | implementable | SessionStart with no SessionEnd, aged past a stated window |
//! | compaction pressure | implementable | repeated PreCompact within a window |
//! | detached tool | **not separable** | a DENIED tool produces the identical Pre-without-Post shape (A166) |
//! | API / rate limit | **no signal** | StopFailure has never fired on this machine (A166); 0 rows |
//! | waiting on you | **no signal** | Notification has never fired (A166); 0 rows |
//!
//! Two of the blueprint's five phase-1 classes have no signal at all, and a
//! third cannot be told apart from an ordinary permission denial. Building
//! them anyway would mean inventing the evidence, which is the failure this
//! product exists to refuse.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::ledger::Ledger;

/// Age before a quiet session may be called a crash candidate. Phase-aware and
/// deliberately generous: a compile, a long test run, or a person at lunch can
/// legitimately be quiet. Assumption-grade, stated rather than buried.
pub const CRASH_QUIET_MINUTES: i64 = 60;

/// PreCompact deliveries within one session before it reads as pressure.
pub const COMPACTION_PRESSURE_THRESHOLD: u64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    CrashCandidate,
    CompactionPressure,
    DetachedTool,
    ApiLimit,
    WaitingOnYou,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::CrashCandidate => "crash_candidate",
            Class::CompactionPressure => "compaction_pressure",
            Class::DetachedTool => "detached_tool",
            Class::ApiLimit => "api_limit",
            Class::WaitingOnYou => "waiting_on_you",
        }
    }
}

/// Whether a class can be derived at all today, and if not, why not. An
/// unbuildable class is NAMED rather than silently absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Readiness {
    /// Rows can carry it. Still shadow until it graduates.
    Implementable,
    /// The signal exists in the vendor's documentation but has never been
    /// observed on this machine, so nothing can be built on it.
    NoSignalObserved { why: String },
    /// Rows exist but cannot distinguish this class from an innocent one.
    NotSeparable { why: String },
}

/// One labeled candidate. Never a verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub class: Class,
    pub session_id: String,
    /// A stable machine code a person can look up.
    pub reason_code: &'static str,
    /// The rows this stands on. A candidate with no evidence does not exist.
    pub evidence: Vec<String>,
    /// The rule that produced it, in words, including its assumption.
    pub basis: String,
}

/// The phase-1 class table with each class's readiness. This IS the honest
/// half of the deliverable: it says what cannot be built and why.
pub fn readiness() -> Vec<(Class, Readiness)> {
    vec![
        (Class::CrashCandidate, Readiness::Implementable),
        (Class::CompactionPressure, Readiness::Implementable),
        (
            Class::DetachedTool,
            Readiness::NotSeparable {
                why: "a PreToolUse with no matching PostToolUse is the shape of a DENIED tool as \
                      well as a detached one — measured at A166, where a denied call produced \
                      exactly this pattern. Emitting it would accuse an ordinary permission \
                      denial of being a failure."
                    .to_string(),
            },
        ),
        (
            Class::ApiLimit,
            Readiness::NoSignalObserved {
                why: "StopFailure has never fired on this machine (A166: registered, accepted by \
                      settings validation, never delivered). 0 rows, so nothing can be derived."
                    .to_string(),
            },
        ),
        (
            Class::WaitingOnYou,
            Readiness::NoSignalObserved {
                why: "Notification has never fired on this machine (A166), including on a \
                      headless tool denial. 0 rows."
                    .to_string(),
            },
        ),
    ]
}

/// Minutes since the epoch for an ISO-8601 UTC timestamp. Same arithmetic as
/// `session_status`, reused rather than re-derived.
fn minutes(ts: &str) -> Option<i64> {
    let date = ts.get(0..10)?;
    let time = ts.get(11..16)?;
    let mut p = date.split('-');
    let (y, mo, d) = (
        p.next()?.parse::<i64>().ok()?,
        p.next()?.parse::<i64>().ok()?,
        p.next()?.parse::<i64>().ok()?,
    );
    let mut hm = time.split(':');
    let (h, mi) = (
        hm.next()?.parse::<i64>().ok()?,
        hm.next()?.parse::<i64>().ok()?,
    );
    let y_adj = if mo <= 2 { y - 1 } else { y };
    let era = y_adj.div_euclid(400);
    let yoe = y_adj - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 1440 + h * 60 + mi)
}

/// Derive every candidate the rows support, at read.
///
/// `now_utc` is the caller's clock: nothing here polls, watches, or survives
/// the call.
pub fn derive(ledger: &Ledger, now_utc: &str) -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
    let now = minutes(now_utc);

    // ── crash candidate ───────────────────────────────────────────────────
    // A session whose hook rows opened and never closed, quiet past the
    // window. The window is what keeps a working session out of the list.
    let mut stmt = ledger.conn().prepare(
        "SELECT s.session_id, MAX(s.received_at), COUNT(*)
         FROM sensor_events s
         WHERE s.session_id IS NOT NULL
           AND EXISTS (SELECT 1 FROM sensor_events a
                        WHERE a.session_id = s.session_id AND a.hook_event = 'SessionStart')
           AND NOT EXISTS (SELECT 1 FROM sensor_events b
                        WHERE b.session_id = s.session_id AND b.hook_event = 'SessionEnd')
         GROUP BY s.session_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (session_id, last_at, deliveries) = row?;
        let quiet_for = match (now, minutes(&last_at)) {
            (Some(n), Some(l)) => n - l,
            _ => continue, // no clock to judge by: abstain rather than guess
        };
        if quiet_for < CRASH_QUIET_MINUTES {
            continue; // still within a working window — not a candidate
        }
        out.push(Candidate {
            class: Class::CrashCandidate,
            session_id: session_id.clone(),
            reason_code: "no-close-and-quiet",
            evidence: vec![
                format!("session {session_id} has a SessionStart and no SessionEnd on record"),
                format!("{deliveries} hook deliveries, last at {last_at}"),
                format!("quiet for {quiet_for} minutes"),
            ],
            basis: format!(
                "a session whose hook rows opened and never closed, quiet longer than the \
                 {CRASH_QUIET_MINUTES}-minute window. The window is assumption-grade and \
                 deliberately generous: a compile can legitimately be quiet."
            ),
        });
    }

    // ── compaction pressure ───────────────────────────────────────────────
    let mut stmt = ledger.conn().prepare(
        "SELECT session_id, COUNT(*), MIN(received_at), MAX(received_at)
         FROM sensor_events WHERE hook_event = 'PreCompact' AND session_id IS NOT NULL
         GROUP BY session_id HAVING COUNT(*) >= ?1",
    )?;
    let rows = stmt.query_map([COMPACTION_PRESSURE_THRESHOLD as i64], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    for row in rows {
        let (session_id, n, first, last) = row?;
        out.push(Candidate {
            class: Class::CompactionPressure,
            session_id: session_id.clone(),
            reason_code: "repeated-compaction",
            evidence: vec![
                format!("{n} pre-compaction deliveries for session {session_id}"),
                format!("first {first} · last {last}"),
            ],
            basis: format!(
                "{COMPACTION_PRESSURE_THRESHOLD} or more compactions recorded in one session. \
                 The vendor compacts when context runs short; repeated compaction is the \
                 measured shape of a session under pressure."
            ),
        });
    }

    // The other three classes emit nothing, by the readiness table above.
    out.sort_by(|a, b| (a.class, &a.session_id).cmp(&(b.class, &b.session_id)));
    Ok(out)
}

/// The denominator the Guardian actually reaches: sessions with hook rows, out
/// of every session on record. Phase 1's most important number, because a
/// detector's reach bounds every claim made about it.
pub fn coverage(ledger: &Ledger) -> Result<(u64, u64)> {
    let covered: i64 = ledger.conn().query_row(
        "SELECT COUNT(DISTINCT session_id) FROM sensor_events WHERE session_id IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    let total: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
    Ok((covered.max(0) as u64, total.max(0) as u64))
}

/// Whether a class may leave shadow.
///
/// **Never, in phase 1** — and the reason is arithmetic rather than caution.
/// NORTHSTAR-v2 reliability rule 4 requires enough independent held-out
/// adjudications (order of ~300) that the lower confidence bound on the
/// false-alarm rate stays under threshold. Returns the bar and the shortfall
/// so a caller reports the gap instead of a vibe.
pub fn graduation_gap(adjudicated: u64) -> (u64, u64) {
    const BAR: u64 = 300;
    (BAR, BAR.saturating_sub(adjudicated))
}
