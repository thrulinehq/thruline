//! BUILD-SPEC §6.1 — binding a check to the final tree.
//!
//! > *"A check counts only if `check.input_tree_hash == final_tree_hash` **or**
//! > no `fs_change` events touching non-test, non-ignored files occurred
//! > between the check and the final snapshot (record which rule applied)."*
//!
//! # Views, not events
//!
//! This crate has no dependency on `core` or `ledger` and does not want one:
//! §6's verifiers are pure functions, and a pure function that can open a
//! database is only pure by convention. The caller maps events onto the small
//! views here, which also makes every rule below testable without a repository.
//!
//! # "Which rule applied" is recorded, not implied
//!
//! The second rule is a *relaxation* — it lets a check that ran on an older
//! tree still count — so a reader must be able to tell the two apart. A
//! receipt saying "supported" without saying which binding it used is a receipt
//! that cannot be audited.

use serde::{Deserialize, Serialize};

/// A `snapshot` event, reduced to what §6.1 needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotView {
    pub seq: u64,
    pub tree_hash: String,
}

/// A worktree-observed file change.
///
/// **Worktree-observed, never derived from a tree diff** (BUILD-PLAN C.7/S1): a
/// `.gitattributes` clean filter makes the worktree change while the tree hash
/// does not, so a diff-derived change set is blind to exactly the write it
/// exists to detect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditView {
    pub seq: u64,
    pub path: String,
}

/// A `Check`, reduced to what the verifiers need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckView {
    pub id: String,
    /// Where the **record** places this check in the session chain, for
    /// ordering against edits.
    ///
    /// `None` means the record does not place it, and that is a fact rather
    /// than a position. It used to be `u64::MAX` — a value the process
    /// invented, standing where a recorded one belongs, and it made every
    /// re-executed check sort after every edit, so §6.1 rule 2's observed half
    /// skipped the whole edit record. Two checks whose position is unknown are
    /// not thereby simultaneous, and neither of them is last.
    pub seq: Option<u64>,
    /// §3.3 `category`: `test`, `build`, `lint`, `typecheck`.
    pub category: String,
    /// The runner or tool, as recognised.
    pub runner: String,
    /// A flag that suppresses the check itself (`--no-verify`, `--exit-zero`).
    /// §6.4 warns on these; this verifier refuses on them, because a command
    /// that suppressed its own check exiting 0 is not evidence of anything.
    pub bypass: Option<String>,
    /// The command, as the record holds it — **`None` when no producer
    /// recorded one**.
    ///
    /// An empty `Vec` here read as "a command with no narrowing flags", which
    /// is how a re-executed `pytest -k` presented as a clean full-suite run.
    /// An `Option` makes every reader decide what to do about an absent
    /// command instead of letting the empty case fall through a detector.
    pub argv: Option<Vec<String>>,
    pub exit_code: i64,
    /// Empty when no snapshot could be taken. An empty string can never equal a
    /// tree hash, so an unbound check cannot be mistaken for a bound one.
    pub input_tree_hash: String,
    /// Paths that differ between this check's input tree and the final tree.
    ///
    /// **`None` means "could not be computed", and rule 2 refuses on it.**
    /// This is the field that closes the laundering: rule 2 used to reason
    /// purely over `fs_change` events, and only the Edit and Write tools emit
    /// those — `sed -i`, `python -c`, `>>`, `patch` and `git apply` through the
    /// Bash tool emit none. So a check could pass, the agent could break the
    /// source with `sed`, and rule 2 would report "no non-test source file
    /// changed after it" while the tree demonstrably had.
    ///
    /// Rule 1 reasons over tree hashes and rule 2 reasoned over edit events;
    /// when they disagreed, rule 2 won. Now rule 2 asks the tree too, and a
    /// tree it cannot read is a refusal rather than a pass — evidence of
    /// absence, not absence of evidence.
    ///
    /// Used only to *contradict* an observed change set, never to enumerate
    /// one: C.7/S1 still holds, because a clean filter that hides a write from
    /// the tree also hides it here, and the `fs_change` path is what sees that.
    pub paths_changed_since_final: Option<Vec<String>>,
    pub parsed: Option<ParsedView>,
    /// How the run's stdout and stderr were connected: `pipe`, `tty`, or
    /// **`None` when the producer recorded nothing**.
    ///
    /// Carried because it changes the bytes: a runner that colourises or draws
    /// progress only on a terminal produces different output there. §7's
    /// comparison conditions on it rather than guessing, and it must be the
    /// *recorded* value — asserting `"pipe"` at the comparison would make the
    /// conditioning unreachable, which is the same defect as a sentinel
    /// position, one layer along.
    pub stdio: Option<String>,
    /// **Set when `thruline reexec` (§7) produced this row, `None` when the
    /// session captured it.**
    ///
    /// Without this field the two are indistinguishable here, and that
    /// indistinguishability was a verdict-upgrading operation: `reexec` writes
    /// the session's final tree hash onto its own row, so the row satisfies
    /// §6.1 rule 1 *by construction*, [`bind`] returned `RanOnFinalTree`, and
    /// §6.3 preferred it over the correctly-superseded original it re-ran. A
    /// session ThruLine had marked `unsupported` became `supported`, and the
    /// `tests_ran_before_final_edit` finding disappeared from the record.
    ///
    /// A re-execution is a different kind of fact from a capture and has to be
    /// readable as one: it ran a **materialised git tree** rather than the
    /// working directory the session ended on (no untracked files, no ignored
    /// files, submodule content absent), under a **different environment**, and
    /// its tree binding is asserted by the same process that ran it. It can
    /// confirm a captured run or contradict one. It cannot stand in for one.
    pub reexecution: Option<Reexecution>,
}

/// What `thruline reexec` (§7) recorded about a re-executed check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reexecution {
    /// The captured check this re-ran. `None` under `--mode full`, where the
    /// agent never ran the command and there is nothing it could confirm.
    pub of_check: Option<String>,
    /// §7's comparison against the agent's own run: `agrees`, `disagrees`, or
    /// `not_comparable`. **`None` means no comparison was made at all** —
    /// `--mode full` — which is not the same as one that could not be made.
    pub comparison: Option<String>,
    /// §8.1 `container_image_digest`: the image the re-execution ran under.
    pub image_digest: String,
}

impl CheckView {
    /// The command as text, for a human-readable reason or finding.
    ///
    /// **Display only.** No detector may read this: it renders an unrecorded
    /// command as a sentence, and a detector that matched on the sentence would
    /// be reading the stand-in rather than the record.
    pub fn command(&self) -> String {
        match self.argv.as_deref() {
            Some([]) | None => "<the command was not recorded>".to_string(),
            Some(argv) => argv.join(" "),
        }
    }
}

/// The §3.3 `parsed` block, or nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedView {
    pub tests_total: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub xfail: u64,
}

/// Everything a verifier reads about one session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionView {
    pub session_id: String,
    pub snapshots: Vec<SnapshotView>,
    pub edits: Vec<EditView>,
    pub checks: Vec<CheckView>,
    /// The commit the session started from, when there is one.
    ///
    /// §6.7's base tree comes from here rather than from the first snapshot:
    /// a fold-derived snapshot is taken when the fold runs, so every one in a
    /// session carries the same tree and "before" would equal "after".
    pub head_commit: Option<String>,
    /// §13: capture known to be incomplete. No claim in such a session can
    /// reach `supported`.
    pub capture_degraded: bool,
    #[allow(clippy::struct_field_names)]
    pub degraded_reason: Option<String>,
}

impl SessionView {
    /// The last `snapshot` of the session — §6.1's final tree and the seq that
    /// carries it, together, because they are one recorded fact.
    pub fn final_snapshot(&self) -> Option<&SnapshotView> {
        self.snapshots.iter().max_by_key(|s| s.seq)
    }

    /// §6.1's `final_tree_hash`: the last `snapshot` of the session.
    pub fn final_tree_hash(&self) -> Option<&str> {
        self.final_snapshot().map(|s| s.tree_hash.as_str())
    }

    pub fn final_snapshot_seq(&self) -> Option<u64> {
        self.final_snapshot().map(|s| s.seq)
    }

    /// Where the chain first recorded the tree a check ran against.
    ///
    /// This is how a check with no `exec` event of its own — a **re-execution**
    /// (§7), which deliberately does not write into the session's chain — gets
    /// a position that is a recorded fact rather than an invented one. The
    /// *earliest* snapshot carrying the tree is the conservative reading: if
    /// the same tree hash appears again later, the writes in between changed
    /// nothing git could see, and those are exactly the writes §6.1's observed
    /// half exists to catch.
    pub fn first_snapshot_seq_of(&self, tree_hash: &str) -> Option<u64> {
        if tree_hash.is_empty() {
            return None;
        }
        self.snapshots
            .iter()
            .filter(|s| s.tree_hash == tree_hash)
            .map(|s| s.seq)
            .min()
    }
}

/// Which §6.1 rule let a check count — or why none did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Binding {
    /// The check ran on the tree the session ended on.
    RanOnFinalTree,
    /// It ran earlier, and nothing relevant changed after it — confirmed
    /// against the tree, not merely unobserved in the edit events.
    NoRelevantChangeSince {
        edits_ignored: usize,
        paths_ignored: Vec<String>,
    },
    /// Source changed after the check, so it is evidence for a tree that is not
    /// the final one.
    SupersededByEdit { path: String, seq: u64 },
    /// The **tree** changed in a non-test path after the check, with no
    /// `fs_change` event naming it — a write through a tool that reports no
    /// path (`sed -i`, `patch`, `>>`). The check is evidence for a tree that no
    /// longer exists, and nothing in the edit record said so.
    SupersededByUnobservedChange { path: String },
    /// A **re-execution** (§7), not a capture.
    ///
    /// It never `counts()`. §7 says a claimed check may be re-run at the final
    /// tree and that verifiers prefer re-executed checks; it does not say a
    /// re-execution may *replace* the session's own evidence, and reading it
    /// that way makes `thruline reexec` a verdict-upgrading operation — run it
    /// on an `unsupported` session and the verdict improves, every time, for
    /// any session whose only defect was that its tests ran before its last
    /// edit. A re-execution confirms or contradicts. Promotion is
    /// [`crate::tests_pass`]'s to refuse, and this variant is how it can tell.
    Reexecuted {
        of_check: Option<String>,
        comparison: Option<String>,
        /// **What §6.1 would have said about this row had it been captured.**
        ///
        /// Carried rather than discarded, because the ordering question is
        /// still worth asking: a re-execution can itself be superseded — by a
        /// write to a gitignored path, say, which no tree can see and which
        /// `capture_degraded` does not defend against. Answering only "this is
        /// a re-execution" would drop that finding on the floor, which is the
        /// same class of loss the promotion was.
        underlying: Box<Binding>,
    },
    /// There is no final tree, or the check has no input tree. Not the same as
    /// superseded: nothing was observed either way.
    ///
    /// `cause` is the same fact as `reason`, as a closed slug (T8), so a reader
    /// never has to match the sentence. `None` only on a binding serialized
    /// before the field existed; every binding this crate makes names one.
    Unobservable {
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cause: Option<UnobservableCause>,
    },
}

/// Why a check could not be bound — the typed half of
/// [`Binding::Unobservable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnobservableCause {
    /// The session has no snapshot, so there is no final tree.
    NoFinalTree,
    /// The check has no input tree: no snapshot could be taken when it ran.
    NoInputTree,
    /// The record does not place the check in the chain.
    Unplaced,
    /// The tree moved after the check and the workspace (git) could not say
    /// which paths changed. Missing, never "no change".
    WorkspaceUnreadable,
}

impl UnobservableCause {
    pub fn as_str(self) -> &'static str {
        match self {
            UnobservableCause::NoFinalTree => "no_final_tree",
            UnobservableCause::NoInputTree => "no_input_tree",
            UnobservableCause::Unplaced => "unplaced",
            UnobservableCause::WorkspaceUnreadable => "workspace_unreadable",
        }
    }
}

impl Binding {
    pub fn counts(&self) -> bool {
        matches!(
            self,
            Binding::RanOnFinalTree | Binding::NoRelevantChangeSince { .. }
        )
    }

    pub fn describe(&self) -> String {
        match self {
            Binding::RanOnFinalTree => "ran on the final tree".to_string(),
            Binding::NoRelevantChangeSince {
                edits_ignored,
                paths_ignored,
            } => format!(
                "ran before the final snapshot; the tree differs only in test paths, \
                 confirmed against both the edit record ({edits_ignored} test-only edit(s)) \
                 and the tree itself ({} path(s): {})",
                paths_ignored.len(),
                paths_ignored.join(", ")
            ),
            Binding::SupersededByEdit { path, seq } => {
                format!("superseded: {path} changed at seq {seq}, after this check ran")
            }
            Binding::SupersededByUnobservedChange { path } => format!(
                "superseded: the tree shows {path} changed after this check ran, and NO \
                 edit event named it — a write through a tool that reports no path"
            ),
            Binding::Reexecuted {
                of_check,
                comparison,
                underlying,
            } => format!(
                "re-executed in a clean container{}{} — a re-execution confirms or \
                 contradicts a captured run, it does not stand in for one (as §6.1 \
                 evidence it would have been: {})",
                match of_check {
                    Some(id) => format!(", re-running check {id}"),
                    None => ", re-running nothing the agent ran (`--mode full`)".to_string(),
                },
                match comparison {
                    Some(outcome) => format!(", comparison: {outcome}"),
                    None => ", with no comparison made".to_string(),
                },
                underlying.describe()
            ),
            Binding::Unobservable { reason, .. } => format!("not bindable: {reason}"),
        }
    }
}

/// Bind one check to the session's final tree.
///
/// Rule 1 is tree-hash identity. Rule 2 is §6.1's relaxation, and it now needs
/// **two** things to agree: no observed non-test edit, *and* no non-test path
/// differing between the two trees. Either alone is defeatable — the edit
/// record misses `sed`, and a tree diff misses a clean filter.
pub fn bind(session: &SessionView, check: &CheckView) -> Binding {
    let Some(final_tree) = session.final_tree_hash() else {
        return Binding::Unobservable {
            reason: "the session has no snapshot, so there is no final tree".to_string(),
            cause: Some(UnobservableCause::NoFinalTree),
        };
    };
    if check.input_tree_hash.is_empty() {
        return Binding::Unobservable {
            reason: "the check has no input tree hash; no snapshot could be taken when it ran"
                .to_string(),
            cause: Some(UnobservableCause::NoInputTree),
        };
    }
    // **Before rule 1, and that ordering is the fix.** `reexec` writes the
    // session's final tree hash onto the row it inserts, so a re-executed row
    // satisfies rule 1 by construction — the equality below is not an
    // observation about where the check ran, it is a restatement of what the
    // re-executor chose. Reached in the other order, every re-execution came
    // back `RanOnFinalTree` and outranked the original it re-ran.
    if let Some(r) = &check.reexecution {
        return Binding::Reexecuted {
            of_check: r.of_check.clone(),
            comparison: r.comparison.clone(),
            underlying: Box::new(bind_as_captured(session, check, final_tree)),
        };
    }
    bind_as_captured(session, check, final_tree)
}

/// §6.1's two rules, with no question asked about provenance.
///
/// Split out so a re-execution can still be *ordered* — against the edit record
/// and against the tree — without that ordering being mistaken for a licence to
/// carry the claim. [`bind`] is the only caller that decides which of the two
/// answers a reader gets.
fn bind_as_captured(session: &SessionView, check: &CheckView, final_tree: &str) -> Binding {
    if check.input_tree_hash == final_tree {
        return Binding::RanOnFinalTree;
    }

    // --- the observed half: `fs_change` events ----------------------------
    //
    // Both bounds are recorded facts or the rule does not run. `final_seq` was
    // `unwrap_or(u64::MAX)` and `check.seq` was `u64::MAX` for anything the
    // lookup missed; either sentinel makes the loop below skip every edit and
    // call the silence agreement.
    let Some(final_seq) = session.final_snapshot_seq() else {
        return Binding::Unobservable {
            reason: "the session has no snapshot, so there is no final tree".to_string(),
            cause: Some(UnobservableCause::NoFinalTree),
        };
    };
    let Some(check_seq) = check.seq else {
        return Binding::Unobservable {
            reason: format!(
                "the check ran on tree {} and the record does not place it in the \
                 chain, so nothing can be ordered against it",
                short(&check.input_tree_hash)
            ),
            cause: Some(UnobservableCause::Unplaced),
        };
    };
    let mut ignored = 0usize;
    for edit in &session.edits {
        if edit.seq <= check_seq || edit.seq > final_seq {
            continue;
        }
        if is_test_path(&edit.path) {
            // §6.1 says NON-test files, and a test edit is §6.7's business.
            // Counted so the relaxation is visible rather than invisible.
            ignored += 1;
            continue;
        }
        return Binding::SupersededByEdit {
            path: edit.path.clone(),
            seq: edit.seq,
        };
    }

    // --- the tree half, which is what closes the laundering ---------------
    //
    // Reaching here means the trees DIFFER (rule 1 already handled equality),
    // so something changed. The only question is what. Not knowing is a
    // refusal: the previous version treated an empty edit list as proof that
    // nothing relevant changed, which is exactly "absence of evidence" standing
    // in for "evidence of absence".
    let Some(changed) = check.paths_changed_since_final.as_ref() else {
        return Binding::Unobservable {
            reason: format!(
                "the tree changed between this check ({}) and the final snapshot ({}), \
                 and the changed paths could not be computed — so whether the change \
                 was relevant is unknown",
                short(&check.input_tree_hash),
                short(final_tree)
            ),
            cause: Some(UnobservableCause::WorkspaceUnreadable),
        };
    };
    let mut paths_ignored = Vec::new();
    for path in changed {
        if is_test_path(path) {
            paths_ignored.push(path.clone());
            continue;
        }
        return Binding::SupersededByUnobservedChange { path: path.clone() };
    }

    Binding::NoRelevantChangeSince {
        edits_ignored: ignored,
        paths_ignored,
    }
}

fn short(hash: &str) -> String {
    hash.chars().take(12).collect()
}

/// Does this path look like a test file?
///
/// Conventional across the ecosystems §6.2 covers. Deliberately generous: a
/// false *positive* here relaxes binding (a source file mistaken for a test is
/// not treated as superseding), so the predicate must not be trusted alone —
/// which is why §6.7 examines every changed test file regardless.
pub fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    lower.split('/').any(|seg| {
        seg == "test" || seg == "tests" || seg == "spec" || seg == "specs" || seg == "__tests__"
    }) || file.starts_with("test_")
        || file.starts_with("test-")
        || file.ends_with("_test.py")
        || file.ends_with("_test.go")
        || file.ends_with("_test.rs")
        || file.contains(".test.")
        || file.contains(".spec.")
        || file == "conftest.py"
}
