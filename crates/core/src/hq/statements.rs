//! Every statement (claim) a project holds, with its verdict — one row per
//! claim and no text, for screens that count across many threads
//! (`GET /api/hq/projects/<id>/statements`), and the same verdict on each
//! claim of a thread view.
//!
//! # The verdict rule
//!
//! This is the rule `ui/src/hq/data/change.js` `proofOf` applies to one
//! statement (DESIGN-SPEC §4.0), restated over the engine's own record so the
//! counts and the Change screen agree. The UI rule was read on 2026-10-07; the
//! two must change together.
//!
//! Every edit, command and claim of a thread has a **step**: its place when the
//! thread's edits, commands and claims are sorted by recorded instant (a row
//! with no instant sorts as 0), then edit < command < claim, then by their
//! order in the thread view. A test run *passes* when it succeeded and its
//! tally names no failure; it *fails* when it failed or its tally names one.
//! The *current version* is the version of the thread's last row.
//!
//! * No verifier (the kind is not `tests_pass`/`builds`) → `recorded`
//!   (`no_verifier`): no run can settle it; what counts as done can.
//! * The last test run after the claim that has a result failed →
//!   `contradicted` (`later_run_failed`).
//! * A passing test run on the current version after the thread's last edit,
//!   other than the run the claim rests on → `proven`
//!   (`settled_by_later_run`).
//! * No test run before the claim → `not_proven` (`no_run_before`).
//! * The run it rests on failed → `contradicted` (`run_failed`); has no result
//!   → `not_proven` (`run_has_no_result`).
//! * The run passed: with no edit after it and on the current version →
//!   `proven` (`run_holds`); an edit came after it → `not_proven`
//!   (`edited_since_run`); otherwise → `not_proven` (`run_on_older_version`).
//!
//! The run a claim rests on is the latest test command before it in the
//! thread ([`super::view::ClaimItem::rests_on_command_id`]). A claim in no
//! thread has no runs to look at: `recorded` without a verifier, otherwise
//! `not_proven` (`not_in_a_thread`).
//!
//! # Three facts beside the verdict
//!
//! A screen that counts over time needs three facts the verdict alone does not
//! say. Each is read off the same steps, with no new rule:
//!
//! * `proven_at` — the recorded instant of the test run that proves the claim
//!   (the run it rests on for `run_holds`, the later passing run for
//!   `settled_by_later_run`); `None` for every other verdict, and for a proving
//!   run whose row has no instant.
//! * `run_before` — a test run precedes the claim in its thread. Only a claim
//!   with a verifier is asked; for every other claim it is `false`.
//! * `proof_moved` — the run the claim rests on passed, and the code has moved
//!   since: an edit came after that run, or the thread now stands on a later
//!   version. This is the "④ SINCE" of the proof chain: every
//!   `edited_since_run` and `run_on_older_version` claim has it, and so does a
//!   claim that a later run then proved again or failed.

use std::collections::BTreeSet;

use serde::Serialize;

use super::view::claim_kind;
use super::{command_id, Index, Outcome, Thread, SCHEMA_VERSION};
use crate::redact::MaskRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Proven,
    NotProven,
    Contradicted,
    Recorded,
}

/// One claim's verdict and the branch of the rule that gave it, with the three
/// facts read beside it (see the module header).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Judged {
    pub verdict: Verdict,
    pub basis: &'static str,
    /// The index row of the test run that proves the claim.
    pub proven_by: Option<usize>,
    pub run_before: bool,
    pub proof_moved: bool,
}

impl Judged {
    /// A verdict with none of the three facts.
    pub const fn of(verdict: Verdict, basis: &'static str) -> Self {
        Self {
            verdict,
            basis,
            proven_by: None,
            run_before: false,
            proof_moved: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StatementRow {
    pub id: String,
    pub thread_id: Option<String>,
    pub ts: Option<String>,
    /// The recording's source: `claude_code` | `codex`.
    pub agent: String,
    pub session_id: String,
    pub kind: &'static str,
    pub has_verifier: bool,
    pub verdict: Verdict,
    pub verdict_basis: &'static str,
    /// When the test run that proves it was recorded; `None` unless `proven`.
    pub proven_at: Option<String>,
    /// A test run precedes it in its thread (false for a claim with no verifier).
    pub run_before: bool,
    /// The run it rests on passed and the code has moved since.
    pub proof_moved: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSpan {
    pub id: String,
    pub source: String,
    pub started_at: Option<String>,
    pub last_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Statements {
    pub schema_version: u32,
    pub project: ProjectRef,
    pub claims: Vec<StatementRow>,
    pub sessions: Vec<SessionSpan>,
    pub masking: MaskRecord,
}

#[derive(Clone, Copy)]
struct Run {
    step: usize,
    /// The index row the command was recorded in.
    row: usize,
    version: usize,
    outcome: Outcome,
    tally_failed: bool,
}

impl Run {
    fn passing(&self) -> bool {
        self.outcome == Outcome::Succeeded && !self.tally_failed
    }
    fn failing(&self) -> bool {
        self.outcome == Outcome::Failed || self.tally_failed
    }
}

/// `(row, claim index) → verdict` for every claim in one thread, in thread
/// order.
pub(crate) fn judge_thread(index: &Index, t: &Thread) -> Vec<((usize, usize), Judged)> {
    let rows = &index.rows;
    // Items as the thread view lists them: (ms, type order, list index).
    // Type order: edit 1, command 2, claim 3 (prompts never change the
    // relative order of the others).
    let mut keyed: Vec<((i64, u8, usize), Item)> = Vec::new();
    let (mut ne, mut nc, mut nl) = (0usize, 0usize, 0usize);
    let ms = |i: usize| rows[i].ts.as_deref().and_then(super::parse_ms).unwrap_or(0);
    let mut test_order: Vec<usize> = Vec::new(); // positions in `keyed`, served order
    for (pos, &i) in t.rows.iter().enumerate() {
        let r = &rows[i];
        for _ in &r.files {
            keyed.push(((ms(i), 1, ne), Item::Edit));
            ne += 1;
        }
        for c in &r.cmds {
            if c.test {
                let outcome = index.outcome(i, c);
                let tally_failed = r
                    .result
                    .and_then(|ri| index.result_text.get(&ri))
                    .and_then(|text| super::tally::tally(text))
                    .is_some_and(|t| t.failed > 0);
                test_order.push(keyed.len());
                keyed.push((
                    (ms(i), 2, nc),
                    Item::Test {
                        id: command_id(&r.id, c),
                        pos,
                        run: Run {
                            step: 0,
                            row: i,
                            version: r.version,
                            outcome,
                            tally_failed,
                        },
                    },
                ));
            }
            nc += 1;
        }
        if let Some(found) = index.claims.get(&i) {
            for (n, c) in found.iter().enumerate() {
                keyed.push((
                    (ms(i), 3, nl),
                    Item::Claim {
                        row: i,
                        n,
                        pos,
                        verifier: matches!(claim_kind(c), "tests_pass" | "builds"),
                    },
                ));
                nl += 1;
            }
        }
    }
    // Steps.
    let mut order: Vec<usize> = (0..keyed.len()).collect();
    order.sort_by(|&a, &b| keyed[a].0.cmp(&keyed[b].0));
    let mut step_of = vec![0usize; keyed.len()];
    for (s, &k) in order.iter().enumerate() {
        step_of[k] = s + 1;
    }
    let mut edit_steps: Vec<usize> = keyed
        .iter()
        .enumerate()
        .filter(|(_, (_, it))| matches!(it, Item::Edit))
        .map(|(k, _)| step_of[k])
        .collect();
    edit_steps.sort_unstable();
    let last_edit = edit_steps.last().copied();
    // Tests in served order, with their steps.
    let tests: Vec<(String, usize, Run)> = test_order
        .iter()
        .filter_map(|&k| match &keyed[k].1 {
            Item::Test { id, pos, run } => Some((
                id.clone(),
                *pos,
                Run {
                    step: step_of[k],
                    ..*run
                },
            )),
            _ => None,
        })
        .collect();
    let current = t.rows.last().map(|&i| rows[i].version);

    let mut out = Vec::new();
    for (k, (_, it)) in keyed.iter().enumerate() {
        let Item::Claim {
            row,
            n,
            pos,
            verifier,
        } = it
        else {
            continue;
        };
        let step = step_of[k];
        let judged = if !verifier {
            Judged::of(Verdict::Recorded, "no_verifier")
        } else {
            // The run it rests on: the latest test before it in the thread.
            let cited = tests.iter().rev().find(|(_, p, _)| p < pos).map(|t| t.2);
            let last_after = tests
                .iter()
                .rfind(|(_, _, r)| r.step > step && r.outcome != Outcome::Unknown)
                .map(|t| t.2);
            let settle = tests
                .iter()
                .rev()
                .find(|(_, _, r)| {
                    Some(r.version) == current
                        && r.passing()
                        && last_edit.is_none_or(|e| r.step > e)
                })
                .map(|t| t.2);
            let settled = settle.filter(|s| cited.is_none_or(|c| c.step != s.step));
            let judged = if last_after.is_some_and(|r| r.failing()) {
                Judged::of(Verdict::Contradicted, "later_run_failed")
            } else if let Some(s) = settled {
                Judged {
                    proven_by: Some(s.row),
                    ..Judged::of(Verdict::Proven, "settled_by_later_run")
                }
            } else {
                match cited {
                    None => Judged::of(Verdict::NotProven, "no_run_before"),
                    Some(c) if !c.passing() => {
                        if c.failing() {
                            Judged::of(Verdict::Contradicted, "run_failed")
                        } else {
                            Judged::of(Verdict::NotProven, "run_has_no_result")
                        }
                    }
                    Some(c) => {
                        let since = edit_steps.iter().any(|&e| e > c.step);
                        if !since && Some(c.version) == current {
                            Judged {
                                proven_by: Some(c.row),
                                ..Judged::of(Verdict::Proven, "run_holds")
                            }
                        } else if since {
                            Judged::of(Verdict::NotProven, "edited_since_run")
                        } else {
                            Judged::of(Verdict::NotProven, "run_on_older_version")
                        }
                    }
                }
            };
            Judged {
                run_before: cited.is_some(),
                proof_moved: cited.is_some_and(|c| {
                    c.passing()
                        && (edit_steps.iter().any(|&e| e > c.step) || Some(c.version) != current)
                }),
                ..judged
            }
        };
        out.push(((*row, *n), judged));
    }
    out
}

enum Item {
    Edit,
    Test {
        id: String,
        pos: usize,
        run: Run,
    },
    Claim {
        row: usize,
        n: usize,
        pos: usize,
        verifier: bool,
    },
}

impl Index {
    /// The statement `statement_id` names, with the sentence the extractor gives
    /// for it now, or `None` when this project serves no such statement. A
    /// statement is a claim, and the ids are exactly the ones [`Index::statements`]
    /// and the thread view serve (`<events.id>#<n>`). The sentence is the
    /// extractor's own text, unmasked: it is hashed, never served, so a change
    /// in the masker cannot move an objection off its sentence.
    pub fn claim_sentence(
        &self,
        statement_id: &str,
    ) -> Option<crate::statement_objection::ClaimRef> {
        let (event_id, ordinal) = crate::statement_objection::split_statement_id(statement_id)?;
        let (_, found) = self
            .claims
            .iter()
            .find(|(&i, _)| self.rows[i].id == event_id)?;
        let claim = found.get(ordinal)?;
        Some(crate::statement_objection::ClaimRef {
            statement_id: statement_id.to_string(),
            event_id: event_id.to_string(),
            ordinal,
            sentence: claim.text.clone(),
        })
    }

    /// The events of one thread that hold at least one claim, sorted and
    /// without repeats, or `None` when the project has no such thread. The
    /// thread view's cache keys on the objections kept against exactly these.
    pub fn thread_claim_events(&self, thread_id: &str) -> Option<Vec<&str>> {
        let t = self.threads.iter().find(|t| t.id == thread_id)?;
        let events: BTreeSet<&str> = t
            .rows
            .iter()
            .filter(|i| self.claims.contains_key(i))
            .map(|&i| self.rows[i].id.as_str())
            .collect();
        Some(events.into_iter().collect())
    }

    /// Every claim of the project with its verdict, and every session's span.
    pub fn statements(&self) -> Statements {
        let mut record = MaskRecord::default();
        let mut judged: std::collections::HashMap<(usize, usize), (String, Judged)> =
            std::collections::HashMap::new();
        for t in &self.threads {
            for (key, j) in judge_thread(self, t) {
                judged.insert(key, (t.id.clone(), j));
            }
        }
        let mut claim_rows: Vec<usize> = self.claims.keys().copied().collect();
        claim_rows.sort_by_key(|&i| (self.rows[i].ms, self.rows[i].session, self.rows[i].seq));
        let mut claims = Vec::new();
        for i in claim_rows {
            let r = &self.rows[i];
            let session = &self.sessions[r.session];
            for (n, c) in self.claims[&i].iter().enumerate() {
                let kind = claim_kind(c);
                let has_verifier = matches!(kind, "tests_pass" | "builds");
                let (thread_id, j) = match judged.get(&(i, n)) {
                    Some((t, j)) => (Some(t.clone()), *j),
                    None => (
                        None,
                        if has_verifier {
                            Judged::of(Verdict::NotProven, "not_in_a_thread")
                        } else {
                            Judged::of(Verdict::Recorded, "no_verifier")
                        },
                    ),
                };
                claims.push(StatementRow {
                    id: format!("{}#{n}", r.id),
                    thread_id,
                    ts: r.ts.clone(),
                    agent: session.source.clone(),
                    session_id: session.id.clone(),
                    kind,
                    has_verifier,
                    verdict: j.verdict,
                    verdict_basis: j.basis,
                    proven_at: j.proven_by.and_then(|row| self.rows[row].ts.clone()),
                    run_before: j.run_before,
                    proof_moved: j.proof_moved,
                });
            }
        }
        let mut seen = BTreeSet::new();
        let sessions = self
            .sessions
            .iter()
            .filter(|s| seen.insert(s.id.clone()))
            .map(|s| SessionSpan {
                id: s.id.clone(),
                source: s.source.clone(),
                started_at: s.first_ts.clone(),
                last_at: s.last_ts.clone(),
            })
            .collect();
        Statements {
            schema_version: SCHEMA_VERSION,
            project: ProjectRef {
                id: self.project.id.clone(),
                name: super::masked(&self.project.name, &mut record),
            },
            claims,
            sessions,
            masking: record,
        }
    }
}
