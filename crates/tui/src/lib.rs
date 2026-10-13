//! `thruline tui` — the ratatui flight recorder (BUILD-SPEC §10).
//!
//! > *"TUI: timeline pane, detail pane, claims pane, promises pane, search,
//! > jump-to-stop."*
//!
//! # Everything that can be decided is decided outside the event loop
//!
//! A terminal event loop is close to untestable: it owns the screen, it blocks
//! on input, and asserting on what it drew means asserting on a buffer of
//! styled cells. So none of the *decisions* live in it. [`Flight`] is a plain
//! data model with pure methods — filter, select, jump — and every one of them
//! has a test. [`run`] is a thin shell that reads keys, calls those methods and
//! draws.
//!
//! That split is not tidiness. §10 says the TUI must answer the same six
//! questions `explain` answers, and the failure mode is a TUI that answers them
//! *differently* — a claims pane whose verdicts disagree with `thruline claims`
//! because it recomputed them. It cannot here: [`Flight`] is built by the CLI
//! from the same `judge` call `claims`, `explain` and `receipt` all use, and
//! this crate cannot reach a verifier — it has no dependency on one.
//!
//! # What it deliberately cannot do
//!
//! No mutation of any kind. There is no key that writes to the ledger, and
//! there is nothing to save. A flight recorder that can be edited from the
//! cockpit is not evidence.

use std::collections::BTreeSet;

/// One row of the timeline (§10 item 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub seq: u64,
    pub kind: String,
    /// One line, already bounded and redacted by the caller.
    pub text: String,
    /// Everything a reader might want about this event, for the detail pane.
    pub detail: Vec<(String, String)>,
    /// §10 item 6: a risk flag attached to this event.
    pub risk: Option<String>,
}

/// One claim, and what the record proved (§10 item 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimRow {
    pub kind: String,
    pub text: String,
    /// `supported` | `unsupported` | `unverifiable`. **Carried, never
    /// recomputed**: this crate has no verifier and must not grow one.
    pub verdict: String,
    pub reasons: Vec<String>,
    pub findings: Vec<String>,
}

/// One promise (§10 item 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromiseRow {
    pub text: String,
    pub status: String,
}

/// Why the session stopped (§10 item 2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StopSummary {
    pub reason: String,
    pub detail: String,
    /// The timeline seq the stop sits at, when there is one.
    pub seq: Option<u64>,
}

/// Which pane has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    /// The default, because §10 lists it first and because a reader opens this
    /// to see what happened before they ask what was claimed.
    #[default]
    Timeline,
    Claims,
    Promises,
}

impl Pane {
    pub fn next(self) -> Pane {
        match self {
            Pane::Timeline => Pane::Claims,
            Pane::Claims => Pane::Promises,
            Pane::Promises => Pane::Timeline,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Pane::Timeline => "timeline",
            Pane::Claims => "claims",
            Pane::Promises => "promises",
        }
    }
}

/// Everything one session looks like, and the cursor over it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Flight {
    pub session: String,
    pub rows: Vec<Row>,
    pub claims: Vec<ClaimRow>,
    pub promises: Vec<PromiseRow>,
    pub stop: StopSummary,
    /// The search box's contents. Empty means no filter.
    pub query: String,
    /// Whether the search box has the keyboard.
    pub searching: bool,
    pub pane: Pane,
    /// The cursor, as an index into the **filtered** list.
    ///
    /// Private, and moved only through [`Flight::move_by`],
    /// [`Flight::jump_to_stop`] and [`Flight::set_query`]. Those three are what
    /// keep it inside the list: a public field would let a caller point it past
    /// the end, and the pane would render an empty selection that looks like an
    /// empty session.
    selected: usize,
}

impl Flight {
    /// A flight with a cursor at the top.
    ///
    /// The constructor exists so `selected` can stay private while callers
    /// still build one in a single expression.
    pub fn new(
        session: String,
        rows: Vec<Row>,
        claims: Vec<ClaimRow>,
        promises: Vec<PromiseRow>,
        stop: StopSummary,
    ) -> Flight {
        Flight {
            session,
            rows,
            claims,
            promises,
            stop,
            ..Flight::default()
        }
    }
}

impl Flight {
    /// Indices into [`Flight::rows`] that the current query admits.
    ///
    /// Case-insensitive, over the kind and the text. An empty query admits
    /// everything — **not** nothing, which is the version of this that shows a
    /// user an empty screen and looks like a session with no events.
    pub fn visible(&self) -> Vec<usize> {
        if self.query.is_empty() {
            return (0..self.rows.len()).collect();
        }
        let needle = self.query.to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.kind.to_lowercase().contains(&needle) || r.text.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// The row the cursor is on, if the current filter admits any.
    pub fn current(&self) -> Option<&Row> {
        let visible = self.visible();
        visible
            .get(self.selected.min(visible.len().saturating_sub(1)))
            .and_then(|i| self.rows.get(*i))
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Move the cursor, clamped to what is visible.
    ///
    /// Clamped rather than wrapped: a list that jumps from the end to the
    /// beginning on one extra keypress loses the reader's place, and this is a
    /// tool for reading a record in order.
    pub fn move_by(&mut self, delta: isize) {
        let n = self.visible().len();
        if n == 0 {
            self.selected = 0;
            return;
        }
        let last = n - 1;
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, last as isize) as usize;
    }

    /// §10's jump-to-stop.
    ///
    /// **Clears the query first.** Jumping to a row the filter hides would move
    /// the cursor somewhere the user cannot see, which reads as the key doing
    /// nothing.
    pub fn jump_to_stop(&mut self) -> bool {
        let Some(seq) = self.stop.seq else {
            return false;
        };
        self.query.clear();
        self.searching = false;
        match self.rows.iter().position(|r| r.seq == seq) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Editing the query resets the cursor to the top of the new list.
    ///
    /// Keeping the index would put the cursor on an unrelated row, because the
    /// index is into the *filtered* list and the filter just changed.
    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.selected = 0;
    }

    /// The claim kinds present, for the header line.
    pub fn verdict_counts(&self) -> Vec<(String, usize)> {
        let mut kinds: BTreeSet<&str> = BTreeSet::new();
        for c in &self.claims {
            kinds.insert(c.verdict.as_str());
        }
        kinds
            .into_iter()
            .map(|v| {
                (
                    v.to_string(),
                    self.claims.iter().filter(|c| c.verdict == v).count(),
                )
            })
            .collect()
    }

    /// A one-line summary for the header, in the product's own vocabulary.
    ///
    /// "claimed, and no proof found in the record" — never "lie", and never a
    /// number that reads as an accusation on its own.
    pub fn headline(&self) -> String {
        let unsupported = self
            .claims
            .iter()
            .filter(|c| c.verdict == "unsupported")
            .count();
        let unverifiable = self
            .claims
            .iter()
            .filter(|c| c.verdict == "unverifiable")
            .count();
        if self.claims.is_empty() {
            return "no completion claim was extracted from this session".to_string();
        }
        if unsupported == 0 && unverifiable == 0 {
            return format!(
                "{} claim(s), all supported by the record",
                self.claims.len()
            );
        }
        let mut parts = Vec::new();
        if unsupported > 0 {
            parts.push(format!(
                "{unsupported} claimed, and no proof found in the record"
            ));
        }
        if unverifiable > 0 {
            parts.push(format!("{unverifiable} could not be checked"));
        }
        parts.join(" · ")
    }
}

pub mod screen;

#[cfg(test)]
mod tests;
