//! C3 — the Recovery Capsule (A174).
//!
//! The smallest sufficient, receipt-carrying context for continuing interrupted
//! work in a fresh session. `docs/capsule.schema.json` is the public contract;
//! these types are its reference producer, and a round-trip test holds them to
//! it.
//!
//! **Reuse, stated.** The honesty machinery is `packet.rs`'s, adopted by law
//! rather than by type: every line carries an origin (packet's `Origin` has no
//! no-origin variant, and neither does this), and an empty section states WHICH
//! kind of empty it is (packet's three non-interchangeable `Empty` arms).
//! The types are re-declared here rather than imported because the packet's are
//! deliberately not `Serialize` and the capsule's whole point is that a stranger
//! can read it as JSON. That is a real duplication and it is chosen, not
//! overlooked.
//!
//! **What A173 measured, and what this compiler therefore refuses to do:**
//!
//! * `verified` is 0 corpus-wide and is itself registered shadow, so the
//!   supported section is titled for what it can actually hold.
//! * Every capture/attested table is empty, so the operations section carries
//!   only OBSERVED commands. Their replay classes come from C5's rule table
//!   (A182), which is proof provenance: the rule that entailed each word is
//!   stored on the row and travels with the line.
//! * `unsupported` is ungraduated, so the caution section renders a COUNT under
//!   an explicit ungraduated heading and names no claim — §5s-amendment (R1).
//!   Per-claim receipts appear only on the `diagnostics` surface.
//! * No project-level opening ask exists, so the objective section carries one
//!   line per session rather than electing one.
//! * No failed-approach object exists, so there is no rejected-paths section at
//!   all — and the write-up says so rather than the schema faking it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::candidates;
use crate::error::Result;
use crate::ledger::Ledger;
use crate::snapshot::WorkspaceSnapshot;

pub const CAPSULE_VERSION: u32 = 1;

/// The class an operation carries when no rule in C5's replay table claims its
/// shape. Retained as the honest default: "a rule looked and did not claim
/// this" is a different fact from "nothing has looked".
pub const REPLAY_CLASS_UNKNOWN: &str = "unknown";

/// How many distinct opening asks the objective section shows before it starts
/// saying how many it left out. "Smallest sufficient" is the north star's
/// phrase; a capsule that reprints every session's preamble is neither.
const OBJECTIVE_MAX: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub event_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Law 1: exactly three kinds, and no fourth for "we do not know" — that is
/// what an empty section is for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Provenance {
    EvidenceBacked {
        receipts: Vec<Receipt>,
    },
    UserNote {
        #[serde(skip_serializing_if = "Option::is_none")]
        author: Option<String>,
    },
    /// An inferred line may state a POSITION in the record; it may never attach
    /// a meaning to one. "The last claim the record backs is X" is a position;
    /// "X was abandoned" is a meaning, and RB7 gives meanings exactly three
    /// carriers, none of which is inference.
    Inferred {
        basis: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    pub text: String,
    pub provenance: Provenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Line {
    fn evidence(text: String, origin: String, receipts: Vec<Receipt>) -> Self {
        Line {
            text,
            provenance: Provenance::EvidenceBacked { receipts },
            origin: Some(origin),
            extra: BTreeMap::new(),
        }
    }
    fn inferred(text: String, basis: &str, origin: Option<String>) -> Self {
        Line {
            text,
            provenance: Provenance::Inferred {
                basis: basis.to_string(),
            },
            origin,
            extra: BTreeMap::new(),
        }
    }
}

/// Law 2: an empty section states which kind of empty it is. Collapsing a
/// measured zero into a structural gap — or the reverse — is the costume-swap
/// this product exists to refuse.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Empty {
    /// Nothing in the model can produce this.
    NoSourceObject { reason: String },
    /// The source exists and measured zero.
    SourceQueriedAndEmpty { reason: String },
    /// Content exists but a policy holds it back. The reason names the policy.
    Withheld { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub key: String,
    pub title: String,
    pub lines: Vec<Line>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty: Option<Empty>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Section {
    fn new(key: &str, title: &str, lines: Vec<Line>, empty_if_none: Empty) -> Self {
        let empty = lines.is_empty().then_some(empty_if_none);
        Section {
            key: key.to_string(),
            title: title.to_string(),
            lines,
            empty,
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIdentity {
    pub resolves: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scope {
    pub kind: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub sessions: u64,
    pub repo_identity: RepoIdentity,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    pub chain_position: String,
    pub event_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotCitation {
    pub id: String,
    pub taken_at: String,
    pub doc_sha256: String,
    /// The dirty tree's own digest. Separate from `doc_sha256` because the doc
    /// envelope carries a fresh id and timestamp on every capture: comparing
    /// envelopes would make the continuity check structurally unable to pass.
    /// Found by the kill-9 walk (A181).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_diff_sha256: Option<String>,
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Generator {
    pub name: String,
    pub version: String,
}

/// What was DONE to this capsule, never what is true of it (R2). `scrubbed`
/// stays in the schema and stays **false**: a denylist cannot prove absence,
/// and a producer that flipped this to true on the strength of a detector
/// would be making exactly the promise the ruling forbids. What the artifact
/// asserts instead is `treatment` — the detector that ran and what it masked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsPolicy {
    pub scrubbed: bool,
    /// The detect-and-mask record. Present on every capsule.
    #[serde(default)]
    pub treatment: crate::redact::Treatment,
    pub statement: String,
    pub structural_exclusions: Vec<String>,
}

impl Default for SecretsPolicy {
    fn default() -> Self {
        SecretsPolicy {
            scrubbed: false,
            treatment: crate::redact::Treatment::none(),
            statement: "This capsule carries verbatim prose and verbatim command text from the \
                        recorded sessions. Known secret shapes are detected and masked; a shape \
                        the detector does not know is still here — review before sharing."
                .to_string(),
            structural_exclusions: vec![
                "environment variable values are never read; the workspace snapshot records names \
                 only"
                    .to_string(),
                "file contents are never copied; artifacts are named and digested, never inlined"
                    .to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capsule {
    pub capsule_version: u32,
    pub generator: Generator,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiled_at: Option<String>,
    pub scope: Scope,
    pub binding: Binding,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_snapshot: Option<SnapshotCitation>,
    pub sections: Vec<Section>,
    pub secrets_policy: SecretsPolicy,
    /// Law 4: a field this reader does not understand survives the round trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn repo_identity(ledger: &Ledger, project_id: &str) -> Result<RepoIdentity> {
    use rusqlite::OptionalExtension;
    let row: Option<(Option<String>, Option<String>)> = ledger
        .conn()
        .query_row(
            "SELECT repo_id, basis FROM workspace_identities WHERE project_id = ?1",
            [project_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(match row {
        Some((Some(repo_id), basis)) => RepoIdentity {
            resolves: true,
            repo_id: Some(repo_id),
            basis,
            note: None,
        },
        Some((None, basis)) => RepoIdentity {
            resolves: false,
            repo_id: None,
            basis,
            note: Some(
                "this scope does not resolve to a repository, so sibling working roots are not \
                 named — they are never guessed from path prefixes"
                    .to_string(),
            ),
        },
        None => RepoIdentity {
            resolves: false,
            repo_id: None,
            basis: Some("no identity row".to_string()),
            note: Some("repository identity has never been derived for this scope".to_string()),
        },
    })
}

/// Claims at a given state, newest first, with their anchor receipts.
fn claims(ledger: &Ledger, project_id: &str, state: &str, limit: usize) -> Result<Vec<Line>> {
    let mut stmt = ledger.conn().prepare(
        "SELECT n.id, n.label, n.anchor_event_id, n.session_id, n.currency
         FROM nodes n WHERE n.project_id = ?1 AND n.sort_key = ?2
         ORDER BY n.anchor_event_id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![project_id, state, limit as i64], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, label, anchor, session, currency) = row?;
        let mut line = Line::evidence(
            format!("\u{201c}{}\u{201d} · {currency}", label.trim()),
            format!("node:{id}"),
            vec![Receipt {
                event_id: anchor,
                session_id: Some(session),
                extra: BTreeMap::new(),
            }],
        );
        line.extra.insert(
            "axes".to_string(),
            serde_json::json!({ "evidence": state, "currency": currency }),
        );
        out.push(line);
    }
    Ok(out)
}

/// Compile a capsule for one project scope.
///
/// `compiled_at` is the caller's clock, never read here: the same rows must
/// produce the same capsule.
pub fn compile(
    ledger: &Ledger,
    project_id: &str,
    snapshot: Option<&WorkspaceSnapshot>,
    compiled_at: &str,
) -> Result<Capsule> {
    compile_with(ledger, project_id, snapshot, compiled_at, false)
}

/// The same compile, with the `--diagnostics` surface selected. Per-claim
/// identities of an ungraduated class are licensed on this surface and on no
/// other (§5s-amendment, R1).
pub fn compile_with(
    ledger: &Ledger,
    project_id: &str,
    snapshot: Option<&WorkspaceSnapshot>,
    compiled_at: &str,
    diagnostics: bool,
) -> Result<Capsule> {
    let cands = candidates::derive(ledger, project_id)?;
    let (chain_position, event_total) = ledger.compute_project_head(project_id)?;
    let sessions: u64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM sessions WHERE project_id = ?1",
        [project_id],
        |r| r.get::<_, i64>(0).map(|n| n.max(0) as u64),
    )?;
    let display_name: Option<String> = {
        use rusqlite::OptionalExtension;
        ledger
            .conn()
            .query_row(
                "SELECT display_name FROM projects WHERE id = ?1",
                [project_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten()
    };

    // ── objective ─────────────────────────────────────────────────────────
    // One line per DISTINCT opening ask. No project-level opening ask exists,
    // and electing one would be invention (A173) — but repeating the same text
    // once per session is not context, it is noise: the real corpus opens 10 of
    // 24 Telescope sessions with a byte-identical scheduled-task preamble.
    // Deduplication here is arithmetic, not judgement: identical text is
    // collapsed, the repeat count is STATED, and the first session to carry it
    // keeps the receipt. Nothing is filtered by what it looks like.
    let mut asks: Vec<(&crate::candidates::OpeningAsk, u64)> = Vec::new();
    for ask in &cands.opening_asks {
        match asks.iter_mut().find(|(seen, _)| seen.text == ask.text) {
            Some((_, count)) => *count += 1,
            None => asks.push((ask, 1)),
        }
    }
    let distinct = asks.len();
    let mut objective: Vec<Line> = asks
        .iter()
        .take(OBJECTIVE_MAX)
        .map(|(ask, count)| {
            let continues = if ask.continuation {
                " · continues an earlier run"
            } else {
                ""
            };
            let repeats = if *count > 1 {
                format!(" · {count} sessions opened with this same text")
            } else {
                String::new()
            };
            Line::evidence(
                format!(
                    "opening ask \u{201c}{}\u{201d}{continues}{repeats}",
                    ask.text.trim()
                ),
                format!("session:{}", ask.session_id),
                vec![Receipt {
                    event_id: ask.event_id.clone(),
                    session_id: Some(ask.session_id.clone()),
                    extra: BTreeMap::new(),
                }],
            )
        })
        .collect();
    if distinct > OBJECTIVE_MAX {
        // The cap is stated, never silent: a truncation nobody can see is the
        // shape of claim this product refuses.
        objective.push(Line::inferred(
            format!(
                "{} further distinct opening asks are not shown here",
                distinct - OBJECTIVE_MAX
            ),
            "the capsule shows the most recent distinct opening asks and states how many it left out",
            Some(format!("project:{project_id}")),
        ));
    }

    // ── supported, with receipts ──────────────────────────────────────────
    let supported = claims(ledger, project_id, "supported", 40)?;

    // ── caution ───────────────────────────────────────────────────────────
    // §5s-amendment (R1): an aggregate count of an ungraduated class may render
    // on a default surface under an explicit shadow heading and WITHOUT any
    // per-claim identity. The successor gets its warning signal; no accusation
    // is minted against a named claim. Receipts live behind `diagnostics`.
    let caution_count: i64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM nodes WHERE project_id = ?1 AND sort_key = 'unsupported'",
        [project_id],
        |r| r.get(0),
    )?;
    let caution: Vec<Line> = if caution_count == 0 {
        Vec::new()
    } else if diagnostics {
        // The flagged surface: per-claim identities are licensed here, and
        // only here.
        claims(ledger, project_id, "unsupported", 40)?
    } else {
        vec![Line::inferred(
            format!(
                "{caution_count} claim(s) in this scope carry no matching evidence \u{b7} this \
                 class has not graduated, so no claim is named here \u{b7} run with \
                 --diagnostics for the receipts"
            ),
            "a count over an ungraduated class, carrying no per-claim identity (§5s-amendment)",
            Some(format!("project:{project_id}")),
        )]
    };

    // ── in flight ─────────────────────────────────────────────────────────
    // Candidate vocabulary only: a count and a boundary are measurements.
    let in_flight: Vec<Line> = cands
        .tails
        .iter()
        .map(|tail| {
            Line::evidence(
                format!(
                    "{} edits after the last commit — unresolved · {}",
                    tail.edits_after_last_boundary,
                    tail.files.join(", ")
                ),
                format!("session:{}", tail.session_id),
                vec![Receipt {
                    event_id: tail.session_id.clone(),
                    session_id: Some(tail.session_id.clone()),
                    extra: BTreeMap::new(),
                }],
            )
        })
        .collect();

    // ── operations observed ───────────────────────────────────────────────
    let upstream_ops = crate::operations::derive(ledger, project_id)?;
    let operations = observed_operations(ledger, project_id)?;

    // ── where the work stopped ────────────────────────────────────────────
    // A position, never a meaning, and never the word "safe": safety is a C5
    // assessment this compiler has no rows for.
    let stopped = match (supported.first(), cands.tails.first()) {
        (last, Some(tail)) => vec![Line::inferred(
            format!(
                "the record's last unresolved edit run is in session {} ({} edits after the last commit){}",
                tail.session_id,
                tail.edits_after_last_boundary,
                last.map(|_| "; the most recent claim the record backs is listed above")
                    .unwrap_or("")
            ),
            "the last claim the record backs, and the first edit run it does not",
            Some(format!("session:{}", tail.session_id)),
        )],
        (Some(_), None) => vec![Line::inferred(
            "every recorded edit run ends at a commit boundary; the record shows no unresolved tail"
                .to_string(),
            "no uncommitted tail was derived for any session in this scope",
            None,
        )],
        (None, None) => Vec::new(),
    };

    // ── uncertainties ─────────────────────────────────────────────────────
    let mut uncertainties = claims(ledger, project_id, "unknown", 20)?;
    if let Some(snap) = snapshot {
        uncertainties.push(Line::inferred(
            format!("workspace snapshot: {}", snap.processes.gaps),
            "the workspace snapshot's own named gaps, carried verbatim",
            Some(format!("snapshot:{}", snap.id)),
        ));
        if !snap.complete {
            uncertainties.push(Line::inferred(
                snap.note
                    .clone()
                    .unwrap_or_else(|| "Snapshot incomplete — capture again.".to_string()),
                "the workspace moved while the snapshot was being taken",
                Some(format!("snapshot:{}", snap.id)),
            ));
        }
    }

    // ── freshness ─────────────────────────────────────────────────────────
    let mut freshness = vec![Line::inferred(
        format!("compiled at chain position {chain_position} over {event_total} recorded events"),
        "the project's chain position at compile time; a later position means this capsule is \
         no longer current",
        Some(format!("project:{project_id}")),
    )];
    match snapshot {
        Some(snap) => freshness.push(Line::inferred(
            format!(
                "workspace snapshot {} taken {} · doc sha256 {}",
                snap.id,
                snap.taken_at,
                snap.doc_digest()
            ),
            "the workspace snapshot this capsule was compiled beside, cited by digest",
            Some(format!("snapshot:{}", snap.id)),
        )),
        None => freshness.push(Line::inferred(
            "no workspace snapshot was taken beside this capsule".to_string(),
            "the caller supplied no snapshot; an absent snapshot is stated, never implied",
            None,
        )),
    }

    let sections = vec![
        Section::new(
            "objective",
            "What was asked",
            objective,
            Empty::SourceQueriedAndEmpty {
                reason: "no session in this scope opens with a recorded request".to_string(),
            },
        ),
        Section::new(
            "supported_with_receipts",
            "What the record backs",
            supported,
            Empty::SourceQueriedAndEmpty {
                reason: "no claim in this scope is backed by evidence in the same turn".to_string(),
            },
        ),
        Section::new(
            "caution",
            // The heading §5s-amendment requires: a shadow count may render
            // only under an explicit ungraduated heading.
            "Where proof was not found — an ungraduated class, counts only",
            caution,
            Empty::SourceQueriedAndEmpty {
                reason: "no claim in this scope lacks matching evidence".to_string(),
            },
        ),
        Section::new(
            "in_flight",
            "Work left unresolved",
            in_flight,
            Empty::SourceQueriedAndEmpty {
                reason: "every recorded edit run in this scope ends at a commit boundary"
                    .to_string(),
            },
        ),
        Section::new(
            "operations_observed",
            "Operations seen in the record",
            operations,
            Empty::SourceQueriedAndEmpty {
                reason: "no command matching a side-effect shape was recorded in this scope"
                    .to_string(),
            },
        ),
        Section::new(
            "where_the_work_stopped",
            "Where the work stopped",
            stopped,
            Empty::SourceQueriedAndEmpty {
                reason: "the record holds neither a backed claim nor an unresolved edit run"
                    .to_string(),
            },
        ),
        Section::new(
            "uncertainties",
            "What could not be read",
            uncertainties,
            Empty::SourceQueriedAndEmpty {
                reason: "nothing in this scope was recorded but unreadable".to_string(),
            },
        ),
        Section::new(
            "freshness",
            "How current this is",
            freshness,
            Empty::NoSourceObject {
                reason: "unreachable: freshness always carries the chain position".to_string(),
            },
        ),
    ];

    // R2 — detect-and-mask, applied to every rendered line and to the basis
    // strings that explain them, BEFORE the capsule exists as a document.
    // Masking after render would leave the unmasked text in the JSON, which is
    // the half a fresh session actually reads.
    // D2 — EVERY rendered channel, not just the one the comment named. The
    // first version masked `line.text` while its own comment claimed the basis
    // strings too: a claimed-but-never-did with no current victim is precisely
    // the shape this product exists to flag, and it does not get to carry one.
    // Rendered channels are: line text, the Inferred basis that explains it,
    // and the scope's display name.
    let mut sections = sections;
    let mut display_name = display_name;
    let treatment = {
        let mut targets: Vec<&mut String> = Vec::new();
        for section in sections.iter_mut() {
            for line in section.lines.iter_mut() {
                targets.push(&mut line.text);
                if let Provenance::Inferred { basis } = &mut line.provenance {
                    targets.push(basis);
                }
            }
        }
        if let Some(name) = display_name.as_mut() {
            targets.push(name);
        }
        let mut treatment = crate::redact::mask_all(&mut targets);
        // A209-C3: masks applied UPSTREAM must appear in this capsule's
        // treatment too. Moving masking before truncation (operations,
        // candidates, derive) means the text reaching here is often
        // already masked, so `mask_all` fires on nothing and the treatment
        // would report "nothing matched a known secret shape" over a
        // document full of masks — a false statement of the kind R2 exists
        // to prevent. Rule 1 makes the `[masked: X]` marker the record of
        // the mask, so the stated masks are counted wherever they ran.
        // Upstream masks arrive as values (A209-C4), not parsed markers.
        let mut record = crate::redact::MaskRecord {
            masked: treatment.masked(),
            families: match &treatment {
                crate::redact::Treatment::Ran { patterns, .. } => {
                    patterns.iter().cloned().collect()
                }
                _ => Default::default(),
            },
            complete: true,
        };
        // A209-C5/B7-7: only the records of values this capsule actually
        // renders. `objective` shows the deduped asks up to OBJECTIVE_MAX;
        // `operations_observed` shows the first 20 operations. A record
        // for a value no section carries would claim a mask the artifact
        // does not contain.
        for (ask, _) in asks.iter().take(OBJECTIVE_MAX) {
            record.merge(&ask.mask);
        }
        for op in upstream_ops.iter().take(20) {
            record.merge(&op.mask);
        }
        if record.masked > treatment.masked() {
            treatment = record.to_treatment();
        }
        treatment
    };

    Ok(Capsule {
        capsule_version: CAPSULE_VERSION,
        generator: Generator {
            name: "thruline".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        compiled_at: Some(compiled_at.to_string()),
        scope: Scope {
            kind: "project".to_string(),
            id: project_id.to_string(),
            display_name,
            sessions,
            repo_identity: repo_identity(ledger, project_id)?,
            extra: BTreeMap::new(),
        },
        binding: Binding {
            chain_position,
            event_total,
        },
        workspace_snapshot: snapshot.map(|s| SnapshotCitation {
            id: s.id.clone(),
            taken_at: s.taken_at.clone(),
            doc_sha256: s.doc_digest(),
            dirty_diff_sha256: s.tree.dirty_diff_sha256.clone(),
            complete: s.complete,
            head: s.git.as_ref().and_then(|g| g.head.clone()),
            branch: s.git.as_ref().and_then(|g| g.branch.clone()),
        }),
        sections,
        secrets_policy: SecretsPolicy {
            treatment,
            ..SecretsPolicy::default()
        },
        extra: BTreeMap::new(),
    })
}

/// Commands observed in the record whose replay may be unsafe. Every entry
/// carries `replay_class: unknown` citing C5 by name: no safety class is
/// invented here, because no rows support one.
fn observed_operations(ledger: &Ledger, project_id: &str) -> Result<Vec<Line>> {
    // C5 (A182): the replay class now comes from the operation ledger's rule
    // table, which is proof provenance — the rule that entailed the word is
    // stored on the row and travels with the line. Before C5 every line said
    // `unknown` because nothing could say otherwise; now `unknown` means a
    // rule looked and did not claim the shape.
    let ops = crate::operations::derive(ledger, project_id)?;
    let mut out = Vec::new();
    for op in ops.iter().take(20) {
        let mut line = Line::evidence(
            format!(
                "`{}` · {} · replay class {}",
                op.command_preview,
                op.outcome,
                op.replay_class.as_str()
            ),
            format!("event:{}", op.event_id),
            vec![Receipt {
                event_id: op.event_id.clone(),
                session_id: Some(op.session_id.clone()),
                extra: BTreeMap::new(),
            }],
        );
        line.extra.insert(
            "replay_class".to_string(),
            Value::String(op.replay_class.as_str().to_string()),
        );
        line.extra.insert(
            "replay_class_basis".to_string(),
            Value::String(op.basis.clone()),
        );
        line.extra
            .insert("op_id".to_string(), Value::String(op.op_id.clone()));
        line.extra
            .insert("outcome".to_string(), Value::String(op.outcome.clone()));
        out.push(line);
    }
    Ok(out)
}

/// The Markdown view. Rendered from the SAME structure the JSON serializes, so
/// the two cannot disagree — one selection, two views.
pub fn render_markdown(capsule: &Capsule) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Recovery Capsule · {}\n\n",
        capsule
            .scope
            .display_name
            .clone()
            .unwrap_or_else(|| capsule.scope.id.clone())
    ));
    out.push_str(&format!(
        "Scope: {} `{}` · {} session(s)\n",
        capsule.scope.kind, capsule.scope.id, capsule.scope.sessions
    ));
    if capsule.scope.repo_identity.resolves {
        out.push_str(&format!(
            "Repository: `{}`\n",
            capsule.scope.repo_identity.repo_id.as_deref().unwrap_or("")
        ));
    } else if let Some(note) = &capsule.scope.repo_identity.note {
        out.push_str(&format!("Repository: {note}\n"));
    }
    out.push_str(&format!(
        "Chain position: `{}` over {} recorded events\n\n",
        capsule.binding.chain_position, capsule.binding.event_total
    ));

    for section in &capsule.sections {
        out.push_str(&format!("## {}\n\n", section.title));
        if let Some(empty) = &section.empty {
            let (kind, reason) = match empty {
                Empty::NoSourceObject { reason } => {
                    ("nothing in the record can answer this", reason)
                }
                Empty::SourceQueriedAndEmpty { reason } => ("looked, and found none", reason),
                Empty::Withheld { reason } => ("held back", reason),
            };
            out.push_str(&format!("*({kind} — {reason})*\n\n"));
            continue;
        }
        for line in &section.lines {
            let tag = match &line.provenance {
                Provenance::EvidenceBacked { receipts } => {
                    format!(
                        "evidence-backed · {} receipt{}",
                        receipts.len(),
                        if receipts.len() == 1 { "" } else { "s" }
                    )
                }
                Provenance::UserNote { .. } => "user note".to_string(),
                Provenance::Inferred { basis } => format!("inferred · {basis}"),
            };
            out.push_str(&format!("- {}\n  <sub>{tag}</sub>\n", line.text));
        }
        out.push('\n');
    }

    out.push_str("## About this capsule\n\n");
    out.push_str(&format!("{}\n\n", capsule.secrets_policy.statement));
    for excl in &capsule.secrets_policy.structural_exclusions {
        out.push_str(&format!("- {excl}\n"));
    }
    out
}

impl WorkspaceSnapshot {
    /// The stored doc digest is computed at store time; a capsule cites the
    /// snapshot's own content digest so the two can be compared later.
    fn doc_digest(&self) -> String {
        crate::chain::sha256_hex(
            serde_json::to_string_pretty(self)
                .unwrap_or_default()
                .as_bytes(),
        )
    }
}
