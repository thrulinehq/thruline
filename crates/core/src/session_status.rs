//! RB1 — session status, derived at read. SHADOW MODE.
//!
//! Built strictly on the pinned RB0 taxonomy (docs/recon/RB0-FAILURE-SIGNAL-PROBE.md,
//! `rb0_taxonomy.rs`), and nothing wider:
//!
//! * T1/T2 — vendor-marked clean closes: Claude Code's trailing `last-prompt`
//!   line, Codex's `task_complete`; Codex's `turn_aborted` is a vendor-declared
//!   interruption and needs no inference.
//! * T3 — a missing terminal marker is *interrupted-looking*; age decides
//!   between `active`, `idle`, and the abandonment question.
//! * T4 — nothing may be called `abandoned` before the cross-session lineage
//!   join: Claude Code context exhaustion ends a session and spawns a successor
//!   carrying a continuation preamble, so a session another session continued
//!   is `interrupted`, never `abandoned`.
//! * T6 — environment signatures are ceilings, not statuses: they feed RB2's
//!   diagnosis layer and are deliberately not consulted here.
//! * T7 — the retention class is quarantined exactly as pinned: this module
//!   NEVER stats a raw file, so a vendor sweep cannot masquerade as a failure.
//!
//! No watcher, no poller, no daemon: everything is a function of stored rows
//! and a `now` the caller supplies. Every output is a LABELED HYPOTHESIS —
//! `basis` names the signal class each status stands on — and the whole module
//! is contained by `rb0_taxonomy.rs`'s shadow gate: only the diagnostics
//! surface may name it, and no primary surface (`export`, `now`, `packet`,
//! MCP) may reach it until per-class graduation by adjudication.

use std::collections::BTreeMap;

use crate::error::Result;
use crate::ledger::Ledger;

/// The five states of the RB1 spec, plus the abstention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Idle,
    Completed,
    Interrupted,
    Abandoned,
    Unknown,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Active => "active",
            SessionStatus::Idle => "idle",
            SessionStatus::Completed => "completed",
            SessionStatus::Interrupted => "interrupted",
            SessionStatus::Abandoned => "abandoned",
            SessionStatus::Unknown => "unknown",
        }
    }
}

/// Hypothesis parameters, stated rather than buried. Assumption-grade: the
/// windows are starting points for adjudication, not measured constants.
pub const ACTIVE_WINDOW_MINUTES: i64 = 30;
pub const IDLE_WINDOW_HOURS: i64 = 24;

/// One session's derived status, with the signal class it stands on.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionStatusRow {
    pub session_id: String,
    pub project_id: String,
    pub source: String,
    pub status: SessionStatus,
    /// The taxonomy basis — every status cites its signal class (RB0 rule:
    /// labeled hypotheses with cited signals, never verdicts).
    pub basis: &'static str,
    pub last_ts: Option<String>,
}

/// The classifier inputs, separated from storage so the rule table is a pure
/// function a test can walk without a database.
#[derive(Debug, Clone)]
pub struct TailFacts<'a> {
    pub source: &'a str,
    /// kind of the session's last event.
    pub last_kind: &'a str,
    /// text of the session's last event (vendor terminal markers live here).
    pub last_text: Option<&'a str>,
    /// minutes since the last event, measured against the caller's `now`;
    /// None when the session has no parseable timestamp.
    pub age_minutes: Option<i64>,
    /// T4: does a later session in the same project open with the vendor's
    /// continuation preamble after this session's end?
    pub continued_by_successor: bool,
}

const CLAUDE_TERMINAL: &str = "last-prompt";
const CODEX_COMPLETE: &str = "codex.event_msg.task_complete";
const CODEX_ABORTED: &str = "codex.event_msg.turn_aborted";
/// The Claude Code continuation preamble (T4). Matched by prefix on the
/// successor's first user message — the vendor's own words, not ours.
pub const CONTINUATION_PREFIX: &str = "This session is being continued from a previous";

/// The T4 scan, with the prefix inlined so the planner can prove it against
/// the partial index's WHERE clause. Public to the plan test only.
pub fn t4_continuation_sql() -> String {
    format!(
        "SELECT e.project_id, e.ts FROM events e
         WHERE e.kind='user_msg' AND e.ts IS NOT NULL
           AND e.text LIKE '{CONTINUATION_PREFIX}%'"
    )
}

/// The rule table. Deterministic, total, and cited: every arm names the
/// taxonomy class it stands on.
pub fn classify(facts: &TailFacts) -> (SessionStatus, &'static str) {
    let text = facts.last_text.unwrap_or("");
    // T1/T2 — vendor-marked closes first: they need no inference.
    if facts.source == "claude_code" && facts.last_kind == "meta" && text == CLAUDE_TERMINAL {
        return (SessionStatus::Completed, "T1 vendor terminal marker");
    }
    if facts.source == "codex" && facts.last_kind == "meta" {
        if text == CODEX_COMPLETE {
            return (SessionStatus::Completed, "T1 vendor terminal marker");
        }
        if text == CODEX_ABORTED {
            return (SessionStatus::Interrupted, "T2 vendor-declared abort");
        }
        // The seven odd tails RB0 left unclassified stay unclassified.
        return (SessionStatus::Unknown, "T3 tail unclassified by the probe");
    }
    // T3 — no terminal marker. Age decides, against the caller's clock.
    match facts.age_minutes {
        None => (
            SessionStatus::Unknown,
            "T3 no terminal marker, no timestamp",
        ),
        Some(m) if m <= ACTIVE_WINDOW_MINUTES => {
            (SessionStatus::Active, "T3 no terminal marker, recent")
        }
        Some(m) if m <= IDLE_WINDOW_HOURS * 60 => {
            (SessionStatus::Idle, "T3 no terminal marker, idle window")
        }
        Some(_) if facts.continued_by_successor => (
            SessionStatus::Interrupted,
            // Project-scoped join — deliberately coarse and said so: any later
            // continuation preamble in the project counts. Per-lineage matching
            // needs a raw-namespace probe (A151 batch-2 question).
            "T4 successor continuation in project (coarse join)",
        ),
        Some(_) => (
            SessionStatus::Abandoned,
            "T3 no terminal marker, old, no successor (T4 join checked)",
        ),
    }
}

/// Derive every session's status at read time.
///
/// `now_utc` is the caller's clock in ISO-8601; passing it in keeps the
/// derivation a pure read — there is nothing here a watcher could keep fresher.
pub fn derive_all(ledger: &Ledger, now_utc: &str) -> Result<Vec<SessionStatusRow>> {
    let conn = ledger.conn();

    // T4 first: per project, the start times of continuation successors.
    let mut continuations: BTreeMap<String, Vec<String>> = BTreeMap::new();
    {
        // The LIKE literal is inlined, not bound: the partial index
        // `idx_events_t4_continuation` (ledger.rs) carries this exact
        // predicate, and SQLite's implication prover cannot match a bound
        // parameter against an index WHERE at prepare time. A bound version
        // of this query measured 29 s over the live corpus; the indexed form
        // answers from the ~40 matching rows. `t4_continuation_sql()` exists
        // so the plan test pins index use — a drifted literal silently
        // reverts to the 29 s scan and the test catches it.
        let mut stmt = conn.prepare(&t4_continuation_sql())?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (project, ts) = row?;
            continuations.entry(project).or_default().push(ts);
        }
    }

    let now = parse_minutes(now_utc);
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT s.id, s.project_id,
                COALESCE(ss.source, 'undeclared'),
                e.kind, e.text,
                (SELECT MAX(ts) FROM events e2 WHERE e2.session_id = s.id AND e2.ts IS NOT NULL)
         FROM sessions s
         JOIN events e ON e.session_id = s.id AND e.seq = s.last_seq
         LEFT JOIN source_sessions ss ON ss.session_id = s.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
        ))
    })?;
    for row in rows {
        let (session_id, project_id, source, last_kind, last_text, last_ts) = row?;
        let age_minutes = match (&last_ts, now) {
            (Some(ts), Some(now_m)) => parse_minutes(ts).map(|m| now_m - m),
            _ => None,
        };
        let continued_by_successor = match (&last_ts, continuations.get(&project_id)) {
            (Some(end), Some(starts)) => starts.iter().any(|s| s.as_str() > end.as_str()),
            _ => false,
        };
        let facts = TailFacts {
            source: &source,
            last_kind: &last_kind,
            last_text: last_text.as_deref(),
            age_minutes,
            continued_by_successor,
        };
        let (status, basis) = classify(&facts);
        out.push(SessionStatusRow {
            session_id,
            project_id,
            source,
            status,
            basis,
            last_ts,
        });
    }
    Ok(out)
}

/// Minutes since the epoch for an ISO-8601 UTC timestamp, or None. Minute
/// granularity is all the windows need, and it keeps parsing dependency-free.
fn parse_minutes(ts: &str) -> Option<i64> {
    let date = ts.get(0..10)?;
    let time = ts.get(11..16)?;
    let mut parts = date.split('-');
    let (y, mo, d) = (
        parts.next()?.parse::<i64>().ok()?,
        parts.next()?.parse::<i64>().ok()?,
        parts.next()?.parse::<i64>().ok()?,
    );
    let mut hm = time.split(':');
    let (h, mi) = (
        hm.next()?.parse::<i64>().ok()?,
        hm.next()?.parse::<i64>().ok()?,
    );
    // Days since a fixed epoch, civil-calendar arithmetic (Howard Hinnant's).
    let y_adj = if mo <= 2 { y - 1 } else { y };
    let era = y_adj.div_euclid(400);
    let yoe = y_adj - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 1440 + h * 60 + mi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_t4_scan_answers_from_its_partial_index() {
        // The 29-second lesson (A169): a bound-parameter LIKE cannot be
        // proven against a partial index, so the literal is inlined and this
        // test holds query and index together. If either drifts, the plan
        // falls back to a full scan of every event text and this fails.
        let ledger = crate::ledger::Ledger::open_in_memory().expect("ledger");
        let plan_sql = format!("EXPLAIN QUERY PLAN {}", t4_continuation_sql());
        let mut stmt = ledger.conn().prepare(&plan_sql).expect("prepare");
        let plan: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .expect("plan rows")
            .collect::<std::result::Result<_, _>>()
            .expect("plan text");
        assert!(
            plan.iter()
                .any(|p| p.contains("idx_events_t4_continuation")),
            "the T4 scan no longer uses its partial index — plan: {plan:?}"
        );
    }

    fn facts<'a>(
        source: &'a str,
        kind: &'a str,
        text: Option<&'a str>,
        age: Option<i64>,
        continued: bool,
    ) -> TailFacts<'a> {
        TailFacts {
            source,
            last_kind: kind,
            last_text: text,
            age_minutes: age,
            continued_by_successor: continued,
        }
    }

    #[test]
    fn vendor_terminal_markers_close_a_session() {
        let (s, basis) = classify(&facts(
            "claude_code",
            "meta",
            Some("last-prompt"),
            Some(9999),
            false,
        ));
        assert_eq!(s, SessionStatus::Completed);
        assert!(basis.starts_with("T1"));
        let (s, _) = classify(&facts(
            "codex",
            "meta",
            Some("codex.event_msg.task_complete"),
            Some(9999),
            false,
        ));
        assert_eq!(s, SessionStatus::Completed);
    }

    #[test]
    fn a_codex_abort_is_interrupted_on_the_vendors_word() {
        let (s, basis) = classify(&facts(
            "codex",
            "meta",
            Some("codex.event_msg.turn_aborted"),
            Some(5),
            false,
        ));
        assert_eq!(s, SessionStatus::Interrupted);
        assert!(basis.starts_with("T2"));
    }

    #[test]
    fn the_probe_unclassified_codex_tails_stay_unknown() {
        for tail in [
            "codex.event_msg.token_count",
            "codex.session_meta",
            "codex.event_msg.exec_command_end",
        ] {
            let (s, _) = classify(&facts("codex", "meta", Some(tail), Some(99999), false));
            assert_eq!(s, SessionStatus::Unknown, "{tail} jumped its class");
        }
    }

    #[test]
    fn age_decides_between_active_idle_and_the_abandonment_question() {
        let f = |age, cont| facts("claude_code", "tool_use", None, age, cont);
        assert_eq!(classify(&f(Some(5), false)).0, SessionStatus::Active);
        assert_eq!(classify(&f(Some(120), false)).0, SessionStatus::Idle);
        assert_eq!(
            classify(&f(Some(100_000), false)).0,
            SessionStatus::Abandoned
        );
        assert_eq!(classify(&f(None, false)).0, SessionStatus::Unknown);
    }

    #[test]
    fn nothing_is_abandoned_past_a_continuation_successor() {
        // T4: the lineage join outranks age. A session another session
        // continued was interrupted — the work went on.
        let (s, basis) = classify(&facts(
            "claude_code",
            "assistant_text",
            None,
            Some(100_000),
            true,
        ));
        assert_eq!(s, SessionStatus::Interrupted);
        assert!(basis.starts_with("T4"));
    }

    #[test]
    fn every_basis_labels_its_taxonomy_class() {
        let cases: Vec<TailFacts> = vec![
            facts("claude_code", "meta", Some("last-prompt"), Some(1), false),
            facts(
                "codex",
                "meta",
                Some("codex.event_msg.turn_aborted"),
                Some(1),
                false,
            ),
            facts("codex", "meta", Some("codex.session_meta"), Some(1), false),
            facts("claude_code", "tool_use", None, Some(5), false),
            facts("claude_code", "tool_use", None, Some(120), false),
            facts("claude_code", "tool_use", None, Some(100_000), false),
            facts("claude_code", "tool_use", None, Some(100_000), true),
            facts("claude_code", "tool_use", None, None, false),
        ];
        for f in &cases {
            let (_, basis) = classify(f);
            assert!(
                basis.starts_with("T1")
                    || basis.starts_with("T2")
                    || basis.starts_with("T3")
                    || basis.starts_with("T4"),
                "a status stands on nothing: {basis}"
            );
        }
    }

    #[test]
    fn minute_parsing_is_calendar_correct() {
        let a = parse_minutes("2026-08-08T16:50:00.598Z").unwrap();
        let b = parse_minutes("2026-08-09T16:50:00.000Z").unwrap();
        assert_eq!(b - a, 1440);
        let c = parse_minutes("2026-02-28T23:59:00Z").unwrap();
        let d = parse_minutes("2026-03-01T00:00:00Z").unwrap();
        assert_eq!(d - c, 1, "2026 is not a leap year");
        assert_eq!(parse_minutes("garbage"), None);
    }
}
