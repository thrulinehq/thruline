//! The pilot: prove the pipeline against a live agent **without spending the
//! holdout**.
//!
//! # Why the pilot is its own protocol
//!
//! Obtaining a [`crate::RunManifest`] records a spend — that is the whole point
//! of the ledger. So a pilot run through the production path would burn the real
//! split before the plumbing was proven. The pilot therefore commits its **own**
//! protocol, with its own hash and its own ledger entry, and the real protocol
//! stays uncommitted until the pilot passes.
//!
//! # Tune split only
//!
//! Pilot tasks are drawn from the **tune split** — the declared tasks minus the
//! holdout — and a pilot protocol whose ids are not a subset of tune **cannot
//! commit**. This is stricter than "disjoint from the holdout": it leaves the
//! holdout entirely unspent until the full run is approved, and it makes the
//! rule a refusal rather than a convention.
//!
//! # The harness matters, and this is the part that is easy to get wrong
//!
//! **Hook events only exist under the real harnesses.** Terminal-Bench's own
//! `terminus` agent is driven through litellm and produces shim capture alone,
//! so a pilot through it would exercise the `PATH` shim, the snapshot engine and
//! the binding — and would say *nothing* about the hook adapter, which is the
//! first-class capture path and most of the capture surface. Worse, it would
//! produce a corpus in which every session is hook-less, and the coverage number
//! would read as a property of the corpus rather than of the harness chosen.
//!
//! So [`Harness::Terminus`] is refused as a pilot harness. Terminal-Bench ships
//! `claude_code` and `codex` adapters that invoke the real CLIs
//! (`claude --verbose --output-format stream-json -p …`, `codex exec …`), so
//! this is a refusal with somewhere to go rather than a dead end.
//!
//! # The credential never enters anything
//!
//! The operator exports the key. Nothing here reads its value:
//! [`require_credential`] checks **presence** and returns `()`. The protocol
//! commits the model identifier and version, never the key. And because
//! Terminal-Bench's adapter injects `ANTHROPIC_API_KEY` into the container
//! environment, captured artifacts are scanned against §4.6's credential
//! predicate — a key leaking into a published trajectory would be the worst
//! possible failure of a corpus whose whole point is publication.

use std::collections::BTreeSet;

use crate::protocol::{ClassError, Protocol};

/// Which agent drives the pilot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    /// Terminal-Bench's `claude_code` adapter: the real `claude` CLI.
    ClaudeCode,
    /// Terminal-Bench's `codex` adapter: the real `codex exec`.
    Codex,
    /// Terminal-Bench's own agent, driven through litellm.
    Terminus,
}

impl Harness {
    pub fn as_str(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude-code",
            Harness::Codex => "codex",
            Harness::Terminus => "terminus",
        }
    }

    /// Parses a harness name, for error rendering.
    ///
    /// Not `FromStr`: this is a lookup for a message, and implementing the
    /// trait would invite `"anything".parse()` in a decision path.
    pub fn named(s: &str) -> Option<Harness> {
        match s {
            "claude-code" => Some(Harness::ClaudeCode),
            "codex" => Some(Harness::Codex),
            "terminus" => Some(Harness::Terminus),
            _ => None,
        }
    }

    /// The Terminal-Bench `--agent` value.
    pub fn terminal_bench_agent(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude-code",
            Harness::Codex => "codex",
            Harness::Terminus => "terminus",
        }
    }

    /// **Does this harness fire ThruLine's hooks?**
    ///
    /// Only the real CLIs do. Terminus is driven through litellm and produces
    /// shim capture alone, so a pilot on it measures a different thing while
    /// looking like the same measurement.
    pub fn provides_hook_events(self) -> bool {
        match self {
            // **Derived, not asserted.** This used to be a literal `true`
            // for Codex while `hook-install --harness codex` bailed — the
            // sixth shape in the guard family, *a constant asserting a
            // capability the code does not have*, and a container is what
            // disproved it. It now reads the one table that also decides what
            // the installer will write, so the two cannot say different things
            // again: delete the Codex pack and this answers `false` by
            // construction rather than by anyone remembering to edit it.
            Harness::ClaudeCode | Harness::Codex => {
                thruline_capture::harness::by_name(self.as_str()).is_some_and(|h| h.installable)
            }
            // Not a harness `thruline hook-install` has a row for at all:
            // Terminus is driven through litellm and fires no hooks, so its
            // answer is a property of the runner, not of our table.
            Harness::Terminus => false,
        }
    }

    /// Why this harness cannot produce hook events, when it cannot.
    pub fn hook_gap(self) -> Option<&'static str> {
        match self {
            Harness::ClaudeCode | Harness::Codex if self.provides_hook_events() => None,
            Harness::ClaudeCode | Harness::Codex => Some(
                "the harness fires hooks, but `thruline hook-install` has no installable \
                 pack for it in `capture::harness::ALL`",
            ),
            Harness::Terminus => {
                Some("terminus is driven through litellm and fires no hooks at all")
            }
        }
    }
}

/// The tasks a pilot may draw from: everything the protocol declared, minus the
/// holdout.
pub fn tune_split(tasks: &[String], holdout: &[String]) -> Vec<String> {
    let held: BTreeSet<&String> = holdout.iter().collect();
    let mut out: Vec<String> = tasks
        .iter()
        .filter(|t| !held.contains(t))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PilotError {
    /// Pilot ids outside the tune split. **The holdout stays unspent.**
    NotFromTuneSplit {
        ids: Vec<String>,
    },
    /// A re-pilot over tasks a previous pilot already ran.
    ReusesPilotedTasks {
        ids: Vec<String>,
    },
    /// A harness that cannot fire hooks, so (a) would be zero by construction.
    HarnessWithoutHooks {
        harness: &'static str,
    },
    TooFewTuneTasks {
        have: usize,
        want: usize,
    },
    /// The credential variable is unset. Named, never guessed at, and never
    /// silently downgraded to an unauthenticated path.
    MissingCredential {
        var: &'static str,
    },
    /// A base protocol with **no holdout at all**.
    ///
    /// This is `RunManifest::first()`'s shape in a different place. `plan`
    /// computes its pool as `tune_split(base.tasks, base.holdout)`, and an
    /// empty holdout makes that pool the whole corpus — after which
    /// `check_from_tune` compares the pilot against the same empty holdout and
    /// agrees. One argument, no lie told anywhere, and the holdout is spent.
    /// Refused here so the only remaining route is a protocol that carries one.
    UncommittedHoldout,
    Protocol(ClassError),
}

impl std::fmt::Display for PilotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PilotError::NotFromTuneSplit { ids } => write!(
                f,
                "{} pilot task(s) are not in the tune split ({}). Pilot tasks come \
                 from tune only: a pilot that samples into the holdout contaminates \
                 it exactly as a tuning run would, and the holdout must stay \
                 entirely unspent until the full run is approved",
                ids.len(),
                ids.join(", ")
            ),
            PilotError::ReusesPilotedTasks { ids } => write!(
                f,
                "{} task(s) were already piloted ({}). A re-pilot must use NEW tune \
                 tasks: a second pass over tasks whose failures you have already \
                 seen is a tuning run wearing a pilot's name",
                ids.len(),
                ids.join(", ")
            ),
            PilotError::HarnessWithoutHooks { harness } => write!(
                f,
                "`{harness}` cannot produce hook events, so that coverage measure \
                 would be zero by construction and would read as a property of the \
                 corpus rather than of the harness chosen. {} Only `claude-code` \
                 gives all three measures today.",
                Harness::named(harness)
                    .and_then(|h| h.hook_gap())
                    .unwrap_or("")
            ),
            PilotError::TooFewTuneTasks { have, want } => write!(
                f,
                "the tune split has {have} task(s) and the pilot wants {want}"
            ),
            PilotError::MissingCredential { var } => write!(
                f,
                "`{var}` is not set. Export it before running the pilot. This does \
                 not fall back to an unauthenticated path: that would produce empty \
                 rollouts and read as a coverage finding rather than a missing key"
            ),
            PilotError::UncommittedHoldout => write!(
                f,
                "the base protocol declares no holdout, so the tune split would be \
                 the entire corpus and a pilot could sample straight into held-out \
                 tasks with every later check still agreeing. Build it through \
                 `TerminalBenchSplit::tune_protocol`, or derive one with \
                 `Protocol::derive_holdout`"
            ),
            PilotError::Protocol(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PilotError {}

/// Asserts the credential is present **without reading its value**.
///
/// `var_os(..).is_some()` and nothing else: no binding, no return, no log line.
/// The value never enters this process's data, so it cannot reach a trajectory,
/// a protocol, a report or an error message by accident.
pub fn require_credential(var: &'static str) -> Result<(), PilotError> {
    if std::env::var_os(var).is_some() {
        Ok(())
    } else {
        Err(PilotError::MissingCredential { var })
    }
}

/// Builds the pilot's protocol.
///
/// `base` is the real protocol — **already holding its derived holdout, and
/// still uncommitted**. The pilot's own task list is its own holdout: a pilot
/// reports over everything it ran.
pub fn plan(
    base: &Protocol,
    size: usize,
    harness: Harness,
    already_piloted: &[String],
) -> Result<Protocol, PilotError> {
    if !harness.provides_hook_events() {
        return Err(PilotError::HarnessWithoutHooks {
            harness: harness.as_str(),
        });
    }
    if base.holdout.is_empty() {
        return Err(PilotError::UncommittedHoldout);
    }
    let tune = tune_split(&base.tasks, &base.holdout);
    let piloted: BTreeSet<&String> = already_piloted.iter().collect();
    let available: Vec<String> = tune
        .iter()
        .filter(|t| !piloted.contains(t))
        .cloned()
        .collect();
    if available.len() < size {
        return Err(PilotError::TooFewTuneTasks {
            have: available.len(),
            want: size,
        });
    }

    // Deterministic, and derived from the base protocol so nobody chooses it:
    // the same salt discipline the holdout uses.
    let salt = format!("{}\u{1}pilot", base.hash_without_holdout());
    let mut ranked: Vec<(u64, String)> = available
        .into_iter()
        .map(|t| (crate::protocol::bucket(&salt, &t), t))
        .collect();
    ranked.sort();
    let tasks: Vec<String> = ranked.into_iter().take(size).map(|(_, t)| t).collect();

    let mut p = base.clone();
    p.tasks = tasks.clone();
    p.holdout = tasks;
    p.parameters.insert("pilot".to_string(), "true".to_string());
    p.parameters
        .insert("pilot_of".to_string(), base.hash_without_holdout());
    p.parameters
        .insert("pilot_size".to_string(), size.to_string());
    p.parameters
        .insert("pilot_harness".to_string(), harness.as_str().to_string());
    Ok(p)
}

/// Refuses a pilot protocol whose tasks are not a subset of the tune split.
///
/// Separate from [`plan`] on purpose: `plan` builds one correctly, and this
/// checks one **however it was built**. A hand-assembled pilot protocol gets the
/// same refusal.
pub fn check_from_tune(pilot: &Protocol, base: &Protocol) -> Result<(), PilotError> {
    let tune: BTreeSet<String> = tune_split(&base.tasks, &base.holdout).into_iter().collect();
    let stray: Vec<String> = pilot
        .tasks
        .iter()
        .filter(|t| !tune.contains(*t))
        .cloned()
        .collect();
    if stray.is_empty() {
        Ok(())
    } else {
        Err(PilotError::NotFromTuneSplit { ids: stray })
    }
}

/// Tasks a previous pilot already ran, refused for a re-pilot.
pub fn check_new_tasks(pilot: &Protocol, already_piloted: &[String]) -> Result<(), PilotError> {
    let seen: BTreeSet<&String> = already_piloted.iter().collect();
    let reused: Vec<String> = pilot
        .tasks
        .iter()
        .filter(|t| seen.contains(*t))
        .cloned()
        .collect();
    if reused.is_empty() {
        Ok(())
    } else {
        Err(PilotError::ReusesPilotedTasks { ids: reused })
    }
}

/// What one pilot task cost. **Measured, never estimated.**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskCost {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub wall_ms: u64,
}

/// What a pilot task produced, for the three coverage measures.
///
/// They are **three separate booleans** because they fail for three different
/// reasons: hooks are the harness telling us what it did, shim execs are the
/// runner observed independently, and a bound check is the only thing §6 can
/// reason over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PilotTask {
    pub task_id: String,
    pub hook_events: u64,
    pub shim_exec_events: u64,
    pub checks_bound_to_a_tree: u64,
    /// The tree hash recorded **before the agent's first action**, or `None`
    /// when the precondition did not hold in the live container.
    pub tree_before_first_action: Option<String>,
    pub cost: Option<TaskCost>,
    /// A terminal state for every committed task, crashes and timeouts
    /// included. Absence is a trajectory, not a gap.
    pub terminal_state: String,
}

/// The three coverage fractions, reported separately.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureCoverage {
    pub tasks: usize,
    pub with_hook_events: usize,
    pub with_shim_exec_events: usize,
    pub with_bound_check: usize,
    pub with_tree_before_first_action: usize,
}

/// The gate, not a metric: below this the plumbing is fixed and the pilot is
/// re-run **on new tune tasks**.
pub const COVERAGE_GATE: f64 = 0.90;

impl CaptureCoverage {
    pub fn of(tasks: &[PilotTask]) -> CaptureCoverage {
        CaptureCoverage {
            tasks: tasks.len(),
            with_hook_events: tasks.iter().filter(|t| t.hook_events > 0).count(),
            with_shim_exec_events: tasks.iter().filter(|t| t.shim_exec_events > 0).count(),
            with_bound_check: tasks
                .iter()
                .filter(|t| t.checks_bound_to_a_tree > 0)
                .count(),
            with_tree_before_first_action: tasks
                .iter()
                .filter(|t| t.tree_before_first_action.is_some())
                .count(),
        }
    }

    fn frac(&self, n: usize) -> f64 {
        if self.tasks == 0 {
            0.0
        } else {
            n as f64 / self.tasks as f64
        }
    }

    pub fn hook_fraction(&self) -> f64 {
        self.frac(self.with_hook_events)
    }
    pub fn shim_fraction(&self) -> f64 {
        self.frac(self.with_shim_exec_events)
    }
    pub fn bound_check_fraction(&self) -> f64 {
        self.frac(self.with_bound_check)
    }
    pub fn tree_fraction(&self) -> f64 {
        self.frac(self.with_tree_before_first_action)
    }

    /// All three must clear the gate. Each is named when it does not, because
    /// they fail for different reasons and the fix differs.
    pub fn gate(&self) -> Result<(), BelowGate> {
        let mut failed = Vec::new();
        for (name, got) in [
            ("hook events", self.hook_fraction()),
            ("shim exec events", self.shim_fraction()),
            ("checks bound to a git tree", self.bound_check_fraction()),
        ] {
            if got < COVERAGE_GATE {
                failed.push((name, got));
            }
        }
        if self.tasks == 0 {
            failed.push(("no pilot tasks at all", 0.0));
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(BelowGate {
                failed: failed
                    .into_iter()
                    .map(|(n, g)| (n.to_string(), g))
                    .collect(),
                gate: COVERAGE_GATE,
            })
        }
    }
}

/// Capture coverage under the gate.
#[derive(Debug, Clone, PartialEq)]
pub struct BelowGate {
    pub failed: Vec<(String, f64)>,
    pub gate: f64,
}

impl std::fmt::Display for BelowGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "capture coverage is below the {:.0}% gate: ",
            self.gate * 100.0
        )?;
        let parts: Vec<String> = self
            .failed
            .iter()
            .map(|(n, g)| format!("{n} {:.1}%", g * 100.0))
            .collect();
        write!(
            f,
            "{}. Fix the plumbing and re-pilot on NEW tune tasks; do not proceed to \
             the holdout. If `checks bound to a git tree` is the one that failed, \
             the `git init` precondition is not holding in a live container — which \
             is the failure the 0-of-241 measurement predicted, found for the price \
             of a pilot rather than a full run",
            parts.join(", ")
        )
    }
}

impl std::error::Error for BelowGate {}

/// Cost, measured and projected.
#[derive(Debug, Clone, PartialEq)]
pub struct CostReport {
    pub tasks_with_cost: usize,
    pub tasks: usize,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub wall_ms: u64,
}

impl CostReport {
    pub fn of(tasks: &[PilotTask]) -> CostReport {
        let measured: Vec<&TaskCost> = tasks.iter().filter_map(|t| t.cost.as_ref()).collect();
        CostReport {
            tasks_with_cost: measured.len(),
            tasks: tasks.len(),
            tokens_in: measured.iter().map(|c| c.tokens_in).sum(),
            tokens_out: measured.iter().map(|c| c.tokens_out).sum(),
            wall_ms: measured.iter().map(|c| c.wall_ms).sum(),
        }
    }

    /// Per-task means over the tasks that **reported a cost**, never over all
    /// tasks: dividing by a denominator that includes unmeasured tasks
    /// understates the figure the operator is about to buy with.
    pub fn per_task(&self) -> Option<TaskCost> {
        if self.tasks_with_cost == 0 {
            return None;
        }
        let n = self.tasks_with_cost as u64;
        Some(TaskCost {
            tokens_in: self.tokens_in / n,
            tokens_out: self.tokens_out / n,
            wall_ms: self.wall_ms / n,
        })
    }

    /// The projection the full-run request must state.
    ///
    /// `None` when nothing was measured — a projection from zero samples is an
    /// estimate wearing a measurement's name, and the operator is being asked to
    /// approve a spend on it.
    pub fn projected_for(&self, tasks: usize) -> Option<TaskCost> {
        let per = self.per_task()?;
        let n = tasks as u64;
        Some(TaskCost {
            tokens_in: per.tokens_in * n,
            tokens_out: per.tokens_out * n,
            wall_ms: per.wall_ms * n,
        })
    }

    /// Fraction of pilot tasks that reported a cost at all, carried beside the
    /// projection so a figure derived from three tasks cannot read as one
    /// derived from twenty.
    pub fn cost_coverage(&self) -> f64 {
        if self.tasks == 0 {
            0.0
        } else {
            self.tasks_with_cost as f64 / self.tasks as f64
        }
    }
}

/// A credential-shaped string found in a captured artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    pub artifact: String,
}

/// Scans captured artifacts for credential shapes.
///
/// `looks_like_credential` is §4.6's own predicate, passed in rather than
/// reimplemented: a second credential detector would be a second thing to keep
/// correct, and the one in `thruline-core` is the one with the corpus behind it.
///
/// Terminal-Bench's `claude_code` adapter injects `ANTHROPIC_API_KEY` into the
/// container environment, so the key really is present where artifacts are
/// produced. This is not a hypothetical.
pub fn scan_artifacts(
    artifacts: &[(String, String)],
    looks_like_credential: fn(&str) -> bool,
) -> Vec<Sighting> {
    artifacts
        .iter()
        .filter(|(_, body)| looks_like_credential(body))
        .map(|(name, _)| Sighting {
            artifact: name.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The budget guard. Built before the run, because $8 is a ceiling on someone's
// own credit and "it looped" is the failure mode.
// ---------------------------------------------------------------------------

/// Money in **integer micro-dollars**. No floats anywhere near a spend.
///
/// A float total accumulated over tasks drifts, and the drift is in whichever
/// direction the rounding happens to go. `1 USD == 1_000_000` here.
pub type Micros = u64;

/// One dollar, in micro-dollars.
pub const USD: Micros = 1_000_000;

/// A model's price, per million tokens, in micro-dollars.
///
/// **Pinned in the protocol, never inferred at spend time.** A price table
/// consulted after the fact turns a measurement into an estimate.
///
/// # Why there are five rates and not two
///
/// This struct carried `input` and `output` only, and **a two-rate model cannot
/// express the bill it is guarding**. Measured on a real Claude Code transcript
/// on this machine (533 assistant messages,
/// `~/.claude/projects/.../62b5372a-....jsonl`):
///
/// | bucket | tokens | at Haiku 4.5 rates |
/// |---|---|---|
/// | base input | 1,066 | $0.001 |
/// | output | 319,286 | $1.596 |
/// | cache **read** | 141,999,444 | $14.200 |
/// | cache **write** (1 h) | 1,532,602 | $3.065 |
///
/// A two-rate reader sees $1.60 of an $18.86 bill — it understates by **11.8x**,
/// and the two buckets it ignores are 91% of the spend. That is not a rounding
/// difference; it is a budget guard that cannot trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Price {
    pub model: &'static str,
    /// Base input: tokens that were neither read from nor written to cache.
    pub input_per_mtok: Micros,
    pub output_per_mtok: Micros,
    /// Cache **hit**. Anthropic: 0.1x base input.
    pub cache_read_per_mtok: Micros,
    /// Cache write with a 5-minute TTL. Anthropic: 1.25x base input.
    pub cache_write_5m_per_mtok: Micros,
    /// Cache write with a 1-hour TTL. Anthropic: 2x base input.
    pub cache_write_1h_per_mtok: Micros,
}

/// `gpt-5.6-luna` — the cheapest model on Codex CLI's own supported list.
///
/// $0.20 / $1.20 per million tokens, verified against OpenAI's live pricing page
/// on 2026-09-03. The alternatives Codex CLI lists are all dearer:
/// `gpt-5.4-mini` $0.75/$4.50, `gpt-5.3-codex` $1.75/$14.00, `gpt-5.6-terra`
/// $2.00/$12.00, `gpt-5.6-sol` $4.00/$20.00 — and Sol is frontier, which the
/// authorisation forbids outright.
///
/// Cheaper API models exist (`gpt-5-nano` at $0.05/$0.40), but Codex CLI's
/// documentation does **not** confirm they work with it, and a pilot that burns
/// the authorisation discovering that a model cannot drive the agent has spent
/// the budget on the wrong question.
pub const GPT_5_6_LUNA: Price = Price {
    model: "gpt-5.6-luna",
    input_per_mtok: 200_000,
    output_per_mtok: 1_200_000,
    // **Deliberate upper bounds, and they are labelled as such.** The two input
    // and output rates were verified against the vendor page; the cache rates
    // were not, and a budget guard must never carry an unverified rate that is
    // LOWER than the real one. So cached reads are charged at the full input
    // rate and cache writes at the full input rate too: the guard trips early
    // rather than late, and the day someone verifies the real discount the
    // number moves DOWN, which is the safe direction for a change to make.
    cache_read_per_mtok: 200_000,
    cache_write_5m_per_mtok: 200_000,
    cache_write_1h_per_mtok: 200_000,
};

/// `claude-haiku-4-5-20251001` — the cheapest current Claude model.
///
/// **Every one of these five rates was read off Anthropic's own live pricing
/// page on 2026-09-03**, not transcribed from a prompt:
/// <https://platform.claude.com/docs/en/about-claude/pricing> gives Claude
/// Haiku 4.5 as $1 base input, $1.25 5 m cache write, $2 1 h cache write, $0.10
/// cache hit, $5 output per MTok — the documented 1.25x / 2x / 0.1x multipliers
/// on base input.
///
/// The **model id** is the pinned snapshot from the models overview
/// (<https://platform.claude.com/docs/en/about-claude/models/overview>), whose
/// "Claude API ID" row for Haiku 4.5 is the dated form; `claude-haiku-4-5` is
/// listed separately as the alias. Both are accepted by the allowlist in
/// `scripts/pilot-run.sh`, because the harness may be handed either.
///
/// Haiku 4.5 is also the only current Claude model with a **200 K** context
/// window and no `effort` parameter — worth knowing before a task's context
/// blows past it and the run dies for a reason that is not the plumbing.
pub const CLAUDE_HAIKU_4_5: Price = Price {
    model: "claude-haiku-4-5-20251001",
    input_per_mtok: 1_000_000,
    output_per_mtok: 5_000_000,
    cache_read_per_mtok: 100_000,
    cache_write_5m_per_mtok: 1_250_000,
    cache_write_1h_per_mtok: 2_000_000,
};

/// Every price this build knows, for a lookup that **refuses** rather than
/// guessing. A model absent from here is unpriced, and unpriced is a halt.
pub const PRICES: &[Price] = &[GPT_5_6_LUNA, CLAUDE_HAIKU_4_5];

/// The price for `model`, or `None`.
///
/// The alias `claude-haiku-4-5` resolves to the pinned snapshot's price: the
/// two are the same model on the same rate card, and refusing the alias would
/// be a refusal with nowhere to go.
pub fn price_for(model: &str) -> Option<Price> {
    if model == "claude-haiku-4-5" {
        return Some(CLAUDE_HAIKU_4_5);
    }
    PRICES.iter().copied().find(|p| p.model == model)
}

/// Where a token count came from.
///
/// `codex exec --json` emits `turn.completed` with `input_tokens`,
/// `cached_input_tokens` and `output_tokens` — but **reasoning tokens are
/// documented as omitted** from that stream even for reasoning models, while
/// being persisted to the session rollout file. Reasoning tokens bill as
/// output, so a figure taken from the JSON stream alone is a **floor**, and
/// saying which source produced it is the difference between a measurement and
/// a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageSource {
    /// `turn.completed.usage` only. May understate: see [`UsageSource`].
    TurnCompleted,
    /// The session rollout file, which carries reasoning tokens.
    RolloutFile,
    /// Both agreed, or the larger of the two was taken.
    Reconciled,
}

impl UsageSource {
    /// Does a figure from this source risk understating the bill?
    pub fn may_understate(self) -> bool {
        matches!(self, UsageSource::TurnCompleted)
    }

    /// The token written to the spend ledger, and read back by
    /// [`Ledger::open`] to recover `any_may_understate` across a restart.
    pub fn as_str(self) -> &'static str {
        match self {
            UsageSource::TurnCompleted => "turn-completed",
            UsageSource::RolloutFile => "rollout-file",
            UsageSource::Reconciled => "reconciled",
        }
    }

    /// Parses one, for the CLI. Unknown names are `None` and the caller
    /// refuses: an unrecognised provenance must never silently become the
    /// optimistic one.
    pub fn named(s: &str) -> Option<UsageSource> {
        match s {
            "turn-completed" => Some(UsageSource::TurnCompleted),
            "rollout-file" => Some(UsageSource::RolloutFile),
            "reconciled" => Some(UsageSource::Reconciled),
            _ => None,
        }
    }
}

/// Measured usage for one task, in **disjoint billing buckets**.
///
/// # The invariant, because the two vendors disagree about it
///
/// Every field here is a bucket that is billed at exactly one rate and is
/// counted in exactly one field. That is *not* the shape either vendor emits:
///
///  * **Codex** reports `input_tokens` INCLUSIVE of `cached_input_tokens`, so
///    adding the two double-counts the cached half. [`Usage::from_codex`]
///    subtracts.
///  * **Claude Code** reports `input_tokens` EXCLUSIVE of
///    `cache_read_input_tokens` and `cache_creation_input_tokens`, so the three
///    are already disjoint. [`Usage::from_claude_code`] passes them through.
///
/// Getting this backwards for either vendor is a silent factor-of-two, in the
/// direction that understates. Hence two named constructors and no public
/// struct-literal path that could be built with the wrong convention by
/// accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    /// Input tokens billed at the **base** rate: neither a cache read nor a
    /// cache write.
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Cache hits. Ignored entirely before this change, and 75% of the bill on
    /// the real transcript measured in [`Price`]'s doc comment.
    pub cache_read_input_tokens: u64,
    /// Cache writes with a 5-minute TTL.
    pub cache_write_5m_input_tokens: u64,
    /// Cache writes with a 1-hour TTL.
    pub cache_write_1h_input_tokens: u64,
    pub source: UsageSource,
}

impl Usage {
    /// Usage from a Codex `total_token_usage` record.
    ///
    /// `total_input` is Codex's `input_tokens`, which **includes**
    /// `cached_input`; the subtraction here is the whole reason this is a
    /// constructor and not a struct literal. Codex bills no cache-write line,
    /// so both write buckets are zero by construction rather than by omission.
    pub fn from_codex(
        total_input: u64,
        cached_input: u64,
        output: u64,
        source: UsageSource,
    ) -> Usage {
        Usage {
            input_tokens: total_input.saturating_sub(cached_input),
            output_tokens: output,
            cache_read_input_tokens: cached_input.min(total_input),
            cache_write_5m_input_tokens: 0,
            cache_write_1h_input_tokens: 0,
            source,
        }
    }

    /// Usage from a Claude Code `message.usage` (or stream-json `result`)
    /// record, whose four token fields are already disjoint.
    pub fn from_claude_code(
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write_5m: u64,
        cache_write_1h: u64,
        source: UsageSource,
    ) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            cache_read_input_tokens: cache_read,
            cache_write_5m_input_tokens: cache_write_5m,
            cache_write_1h_input_tokens: cache_write_1h,
            source,
        }
    }

    /// Every token this usage will be billed for, across all five buckets.
    pub fn billable_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_input_tokens)
            .saturating_add(self.cache_write_5m_input_tokens)
            .saturating_add(self.cache_write_1h_input_tokens)
    }

    /// Cost in micro-dollars, rounded **up**.
    ///
    /// Up, not nearest: a budget guard that rounds a spend down can let the run
    /// total cross the ceiling while reporting that it did not.
    pub fn cost(&self, price: Price) -> Micros {
        let ceil_div = |tokens: u64, per_mtok: Micros| -> Micros {
            tokens.saturating_mul(per_mtok).div_ceil(1_000_000)
        };
        ceil_div(self.input_tokens, price.input_per_mtok)
            .saturating_add(ceil_div(self.output_tokens, price.output_per_mtok))
            .saturating_add(ceil_div(
                self.cache_read_input_tokens,
                price.cache_read_per_mtok,
            ))
            .saturating_add(ceil_div(
                self.cache_write_5m_input_tokens,
                price.cache_write_5m_per_mtok,
            ))
            .saturating_add(ceil_div(
                self.cache_write_1h_input_tokens,
                price.cache_write_1h_per_mtok,
            ))
    }
}

/// The caps, checked **between tasks**, and the money they sit inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Halts the run if any single task costs more than this.
    pub per_task: Micros,
    /// Halts the run when the accumulated total would exceed this.
    pub run_total: Micros,
    /// **What the operator authorised for this run.** Not the balance on the
    /// account — see [`Budget::credit`]. Nothing may be spent past it, so
    /// `run_total <= authorised` is an invariant and not a preference.
    pub authorised: Micros,
    /// The credit that exists on the account, of which `authorised` is the part
    /// released for this run. The remainder is **RESERVED**: it is money, and
    /// spending it needs a separate authorisation. Reported so that
    /// "can we re-pilot?" is answered from a number rather than a memory.
    pub credit: Micros,
}

impl Budget {
    /// The pilot's caps under the operator's standing terms.
    ///
    /// **$20 of credit exists; $3.00 of it is authorised for this pilot,
    /// conditional on the coverage gate; the other $17.00 is RESERVED and
    /// requires separate authorisation.**
    ///
    /// Per task $0.50 and run total $3.00, and `authorised` equals `run_total`
    /// because the guard must not permit a dollar that was not released. One
    /// re-pilot fits inside the credit that remains if capture coverage misses
    /// the gate — but only after someone authorises it, which is the difference
    /// between the two fields. At `gpt-5.6-luna`'s price, $0.50 buys roughly
    /// 2.5M input or 415k output tokens for a single task: a task that reaches
    /// it is looping, not working.
    ///
    /// # Why there is no number in the name
    ///
    /// This was `pilot_under_eight_dollars`, returning `authorised: 8 * USD` —
    /// an OpenAI figure that outlived its terms, so `pilot-budget show` printed
    /// **"authorised $8.000000"** while the enforced cap was $3.00. The test
    /// guarding it asserted `authorised == 8 * USD`, which is not a test of
    /// anything: it pinned the stale number and passed for the whole life of the
    /// staleness. A name that carries a figure ages the same way, so this one
    /// does not carry one, and the tests now assert the *relationships*.
    pub fn pilot_authorisation() -> Budget {
        Budget {
            per_task: USD / 2,
            run_total: 3 * USD,
            authorised: 3 * USD,
            credit: 20 * USD,
        }
    }

    /// What is left **of this run's authorisation**.
    ///
    /// Renamed from `remaining`, deliberately. When `authorised` fell from $8
    /// to the $3 that was actually released, `remaining` would have kept
    /// compiling and quietly changed from *remaining authorisation* to
    /// *remaining run cap* — the same figure, a different claim. Renaming made
    /// every call site a compile error, so each one had to say which it meant.
    pub fn authorised_remaining(&self, spent: Micros) -> Micros {
        self.authorised.saturating_sub(spent)
    }

    /// What is left **of the credit**, most of which is not authorised to be
    /// spent. This is the re-pilot question's number, and it is not a licence.
    pub fn credit_remaining(&self, spent: Micros) -> Micros {
        self.credit.saturating_sub(spent)
    }

    /// The part of the credit that is **not** released for this run.
    pub fn reserved(&self) -> Micros {
        self.credit.saturating_sub(self.authorised)
    }
}

/// Why a run stopped early. **A halt is a result, not a failure.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetStop {
    PerTask {
        task_id: String,
        cost: Micros,
        cap: Micros,
        spent_before: Micros,
    },
    RunTotal {
        after_tasks: usize,
        /// What had been spent BEFORE this task. The charge is refused, not
        /// applied, so this is also what is spent afterwards.
        spent: Micros,
        /// What the total would have become had the charge been applied.
        would_be: Micros,
        cap: Micros,
    },
    /// Refused **before** the task ran, because a worst-case task would not fit
    /// under the run cap. This is the variant that makes the cap the number it
    /// says it is.
    NoHeadroom {
        spent: Micros,
        per_task: Micros,
        cap: Micros,
        after_tasks: usize,
    },
    /// The charge was legal and **could not be written to the ledger**. Refused
    /// rather than allowed: an unrecorded spend is how a $3 cap becomes
    /// unbounded across invocations.
    NotRecorded {
        task_id: String,
        cost: Micros,
        reason: String,
    },
}

impl std::fmt::Display for BudgetStop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let usd = |m: Micros| format!("${:.4}", m as f64 / USD as f64);
        match self {
            BudgetStop::PerTask {
                task_id,
                cost,
                cap,
                spent_before,
            } => write!(
                f,
                "halted: `{task_id}` cost {} against a per-task cap of {}. \
                 {} was spent before it. A task that reaches this cap is looping \
                 rather than working, and halting is the result — report what it \
                 cost and how far it got",
                usd(*cost),
                usd(*cap),
                usd(*spent_before)
            ),
            BudgetStop::RunTotal {
                after_tasks,
                spent,
                would_be,
                cap,
            } => write!(
                f,
                "halted after {after_tasks} task(s): {} is spent and this task would \
                 have taken the total to {} against a run cap of {}. The charge was \
                 REFUSED, not applied, so the cap is the cap. This is a result, not \
                 a failure",
                usd(*spent),
                usd(*would_be),
                usd(*cap)
            ),
            BudgetStop::NoHeadroom {
                spent,
                per_task,
                cap,
                after_tasks,
            } => write!(
                f,
                "halted BEFORE task {}: {} is already spent, and a task may cost up \
                 to {} against a run cap of {} — so there is not room for one more \
                 at its worst price. Refusing before the model is called is the only \
                 moment at which refusing is free",
                after_tasks + 1,
                usd(*spent),
                usd(*per_task),
                usd(*cap)
            ),
            BudgetStop::NotRecorded {
                task_id,
                cost,
                reason,
            } => write!(
                f,
                "halted: `{task_id}` cost {} and the spend could NOT be written to \
                 the ledger ({reason}). An unrecorded spend is worse than a refused \
                 one: the next invocation would start from a total that is missing \
                 this task",
                usd(*cost)
            ),
        }
    }
}

impl std::error::Error for BudgetStop {}

/// An exclusive lock on one spend ledger, held across `load → compare → append`
/// and released when it drops.
///
/// # Why the lock is here and not in the runner
///
/// `thruline pilot-budget charge` is a **public subcommand**. A `flock` in
/// `scripts/pilot-run.sh` would guard the door the runner walks through and no
/// other — a parallel `tb run`, a retried task, or a person typing the
/// subcommand while a run is in flight all bypass it. The serialisation has to
/// live where the write lives.
///
/// # What it is holding off
///
/// Two things, both measured with five concurrent charges of the real binary
/// against a ledger with room for exactly one:
///
///  * **Every charge was accepted, 9 trials in 10.** Each process had read the
///    total before any of them appended, so all five compared against the same
///    stale figure and all five passed. A $3 cap admitted $4.
///  * **The file was shredded, 9 trials in 10**, into lines like
///    `t5\tt4t3300000\t\tt1claude-haiku-4-5-20251001…`. `O_APPEND` makes each
///    `write(2)` atomic; it says nothing about a *group* of them, and
///    `writeln!` on an unbuffered `File` issues one per fragment. After that,
///    `show` and `check` exit 1 on "expected 4" and **the record of what was
///    spent is gone** — the one file this module exists to keep.
///
/// # Failing closed
///
/// A lock left behind by a killed process is not cleaned up on a timer and is
/// not overridden after an interval: either would be a race that re-admits the
/// double spend. Acquisition times out and the charge is refused as
/// [`BudgetStop::NotRecorded`], naming the file, because a refused spend is
/// recoverable by hand and an unrecorded one is not.
struct LedgerLock {
    path: std::path::PathBuf,
}

impl LedgerLock {
    /// How long a charge waits for the lock. A charge takes microseconds, so
    /// this is only ever reached when a holder died without releasing.
    const WAIT: std::time::Duration = std::time::Duration::from_secs(10);

    fn acquire(ledger: &std::path::Path) -> std::io::Result<LedgerLock> {
        if let Some(dir) = ledger.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        let mut name = ledger.as_os_str().to_os_string();
        name.push(".lock");
        let path = std::path::PathBuf::from(name);

        let deadline = std::time::Instant::now() + Self::WAIT;
        let mut backoff = std::time::Duration::from_millis(1);
        loop {
            match std::fs::OpenOptions::new()
                .create_new(true) // O_CREAT | O_EXCL
                .write(true)
                .open(&path)
            {
                Ok(mut f) => {
                    // Diagnostic only. Nothing reads this to decide anything —
                    // a lock is held or it is not.
                    use std::io::Write as _;
                    let _ = f.write_all(format!("pid {}\n", std::process::id()).as_bytes());
                    return Ok(LedgerLock { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if std::time::Instant::now() >= deadline {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::WouldBlock,
                            format!(
                                "another charge has held {} for over {} s. Refusing to \
                                 write the spend without the lock: two writers is how \
                                 the ledger gets shredded and how a $3 cap admits $4. \
                                 If no run is live, delete that file by hand",
                                path.display(),
                                Self::WAIT.as_secs()
                            ),
                        ));
                    }
                    std::thread::sleep(backoff);
                    backoff = (backoff * 2).min(std::time::Duration::from_millis(20));
                }
                Err(e) => return Err(e),
            }
        }
    }
}

impl Drop for LedgerLock {
    fn drop(&mut self) {
        // A failure here leaves the lock behind, which fails every later charge
        // closed rather than admitting a second writer. Nothing to do about it
        // that would be safer than that.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A run's spend so far, and whether it may continue.
///
/// # This is `HoldoutLedger`'s lesson, applied to the thing that costs money
///
/// [`crate::RunManifest`] has no public constructor because
/// `RunManifest::first()` — one call, `prior_runs: Vec::new()` — was a
/// one-token bypass of the entire holdout ledger. **That lesson was applied to
/// the holdout and not to the money.** This struct was `#[derive(Default)]`
/// with a `pub spent`, which is the same shape twice over:
///
///  * `ledger.spent = 0;` is one statement and it erases the record;
///  * and there was no record. Spend lived in a process, and the runner is one
///    `tb run` per invocation, so ten tasks were ten amnesiac processes each
///    starting from $0. A "$3 run cap" enforced per-process across ten
///    processes is a $30 cap.
///
/// So: the fields are private, there is no `Default`, and
/// [`Ledger::open`] on an **append-only file** is the only way to obtain one.
/// Every charge is a line on disk before it is a number in memory, and a
/// restart re-reads it. A spend removed from the ledger is a visible deletion
/// in a diff, exactly as with the holdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ledger {
    path: std::path::PathBuf,
    spent: Micros,
    tasks_charged: usize,
    any_may_understate: bool,
}

impl Ledger {
    /// Reads the ledger at `path`. A missing file is a ledger that has spent
    /// nothing — the first charge creates it.
    ///
    /// A line that does not parse is a **hard error**, not a skipped line: a
    /// corrupt spend record silently read as $0 is the failure this type
    /// exists to prevent.
    pub fn open(path: &std::path::Path) -> std::io::Result<Ledger> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        let mut spent: Micros = 0;
        let mut tasks_charged = 0usize;
        let mut any_may_understate = false;
        for (n, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            // task_id 	 micros 	 model 	 source
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() != 4 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "{}:{}: spend ledger line has {} fields, expected 4. Refusing \
                         to read a corrupt spend record as zero",
                        path.display(),
                        n + 1,
                        f.len()
                    ),
                ));
            }
            let micros: Micros = f[1].parse().map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "{}:{}: `{}` is not a micro-dollar amount",
                        path.display(),
                        n + 1,
                        f[1]
                    ),
                )
            })?;
            spent = spent.saturating_add(micros);
            tasks_charged += 1;
            if f[3] == "turn-completed" {
                any_may_understate = true;
            }
        }
        Ok(Ledger {
            path: path.to_path_buf(),
            spent,
            tasks_charged,
            any_may_understate,
        })
    }

    pub fn spent(&self) -> Micros {
        self.spent
    }
    pub fn tasks_charged(&self) -> usize {
        self.tasks_charged
    }
    /// True when any charged task's usage came from a source that may
    /// understate. Carried onto the report: an understated total is worse than
    /// an unknown one, because it looks like a measurement.
    pub fn any_may_understate(&self) -> bool {
        self.any_may_understate
    }
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// **Is there room for one more task at the worst price it may cost?**
    ///
    /// Called *before* a task runs, which is the only moment at which refusing
    /// costs nothing. `charge` alone cannot hold a run total: it sees a task's
    /// cost only after the model has already been paid for it. Requiring a full
    /// per-task cap of headroom before starting makes the arithmetic closed —
    /// `spent_after <= spent_before + per_task <= run_total` — so the ceiling is
    /// the number in the doc comment rather than that number plus one task.
    pub fn headroom_for_one_more(&self, budget: Budget) -> Result<Micros, BudgetStop> {
        let need = self.spent.saturating_add(budget.per_task);
        if need > budget.run_total {
            return Err(BudgetStop::NoHeadroom {
                spent: self.spent,
                per_task: budget.per_task,
                cap: budget.run_total,
                after_tasks: self.tasks_charged,
            });
        }
        Ok(budget.run_total.saturating_sub(need))
    }

    /// Charges one task and decides whether the run continues.
    ///
    /// **Checked between tasks**, which is the only place a cap can be enforced
    /// without cancelling work already paid for. The run-total test happens
    /// **before** the addition and the charge is not recorded when it fails, so
    /// a $3 cap is $3. It used to add and then compare, which made the
    /// documented "$3 run total" a $3.50 tripwire — the per-task cap could be
    /// spent entirely on the far side of the line.
    ///
    /// A charge that is allowed is **written to the file before it is returned**.
    /// A crash between the model call and this line loses a record; a crash
    /// after it does not.
    ///
    /// # The comparison and the write are one operation
    ///
    /// `spent` in memory is a **cached copy of a shared file**, and it goes
    /// stale the moment another `pilot-budget charge` appends. Comparing
    /// against the cached copy is what let five concurrent charges all pass a
    /// cap with room for one — measured, 9 trials in 10. So the whole of
    /// `load → compare → append` happens under [`LedgerLock`], and the total
    /// this compares against is **re-read from disk inside it**, never the
    /// figure this handle was opened with.
    ///
    /// The re-read also runs on the refusal paths: a halt that reports a stale
    /// total is a halt reporting the wrong number, and this is the money.
    pub fn charge(
        &mut self,
        task_id: &str,
        usage: Usage,
        price: Price,
        budget: Budget,
    ) -> Result<Micros, BudgetStop> {
        let cost = usage.cost(price);
        let not_recorded = |e: std::io::Error| BudgetStop::NotRecorded {
            task_id: task_id.to_string(),
            cost,
            reason: e.to_string(),
        };

        let _lock = LedgerLock::acquire(&self.path).map_err(not_recorded)?;
        // Under the lock, and only here, is `spent` a fact rather than a memory.
        let fresh = Ledger::open(&self.path).map_err(not_recorded)?;
        let spent_before = fresh.spent;
        self.spent = fresh.spent;
        self.tasks_charged = fresh.tasks_charged;
        self.any_may_understate = fresh.any_may_understate;

        if cost > budget.per_task {
            return Err(BudgetStop::PerTask {
                task_id: task_id.to_string(),
                cost,
                cap: budget.per_task,
                spent_before,
            });
        }
        let would_be = spent_before.saturating_add(cost);
        if would_be > budget.run_total {
            return Err(BudgetStop::RunTotal {
                after_tasks: self.tasks_charged,
                spent: spent_before,
                would_be,
                cap: budget.run_total,
            });
        }
        self.append(task_id, cost, price.model, usage.source)
            .map_err(not_recorded)?;
        self.spent = would_be;
        self.tasks_charged += 1;
        if usage.source.may_understate() {
            self.any_may_understate = true;
        }
        Ok(cost)
    }

    /// Appends one record as **one buffer, in one `write_all`**.
    ///
    /// It used to be `writeln!(f, "{task_id}\t{cost}\t{model}\t{}", …)` on an
    /// unbuffered `File`, which is one `write(2)` per format fragment.
    /// `O_APPEND` makes each of those atomic and says nothing about the group,
    /// so two concurrent charges interleaved *within* a record and produced
    /// lines like `t5\tt4t3300000\t\tt1claude-haiku-4-5-20251001…` — measured,
    /// 9 trials in 10 at five concurrent charges. The caller holds
    /// [`LedgerLock`], which is the real guarantee; formatting first makes the
    /// unlocked case one syscall rather than five, so a record is torn only if
    /// the kernel short-writes.
    ///
    /// Must be called with the lock held. It is private, and [`Ledger::charge`]
    /// is the only caller.
    fn append(
        &self,
        task_id: &str,
        cost: Micros,
        model: &str,
        source: UsageSource,
    ) -> std::io::Result<()> {
        use std::io::Write as _;
        // Tabs are the field separator, so a task id carrying one would forge a
        // boundary. Task ids are already constrained to a Compose atom by the
        // runner, and this refuses anyway rather than trusting that. Checked
        // before the file is opened: a refused record must not leave a file
        // behind that it did not write to.
        if task_id.contains('\t') || task_id.contains('\n') {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "task id contains a field separator",
            ));
        }
        let record = format!("{task_id}\t{cost}\t{model}\t{}\n", source.as_str());
        if let Some(dir) = self.path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(record.as_bytes())?;
        f.sync_all()
    }

    /// Mean cost per charged task, in micro-dollars. `None` before any charge:
    /// a per-task figure over zero tasks is not a figure.
    pub fn per_task(&self) -> Option<Micros> {
        if self.tasks_charged == 0 {
            None
        } else {
            Some(self.spent / self.tasks_charged as u64)
        }
    }

    /// The projection the full-run request must state, from measured spend.
    pub fn projected_for(&self, tasks: usize) -> Option<Micros> {
        Some(self.per_task()?.saturating_mul(tasks as u64))
    }
}

// ---------------------------------------------------------------------------
// Stop after task 1 and self-check before continuing.
// ---------------------------------------------------------------------------

/// What task 1 must demonstrate before the other nine are worth running.
///
/// The model slug is the single assumption that could waste the run. If
/// `gpt-5.6-luna` is not accepted, `codex exec --model` fails immediately at
/// near-zero cost — but ten sequential failures would still burn wall-clock and
/// produce a coverage number describing **a broken invocation rather than the
/// plumbing**. One task is cheap; nine wasted ones are not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstTask {
    pub task_id: String,
    /// `codex exec` exited without a usage/auth/model error.
    pub invocation_succeeded: bool,
    /// A session rollout file was produced.
    pub rollout_file_found: bool,
    /// And it parsed.
    pub rollout_parsed: bool,
    /// Where the token counts came from. **Must be measured, not inferred** —
    /// `Inferred` is not a variant, so a caller with no numbers has to say
    /// `rollout_file_found: false` rather than quietly supply an estimate.
    pub usage: Option<Usage>,
    pub cost: Micros,
}

/// Why the run stopped after one task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstTaskHalt {
    pub task_id: String,
    pub reasons: Vec<String>,
}

impl std::fmt::Display for FirstTaskHalt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "halted after task 1 (`{}`): {}. Nine more tasks would have produced a \
             coverage number describing this, not the plumbing",
            self.task_id,
            self.reasons.join("; ")
        )
    }
}

impl std::error::Error for FirstTaskHalt {}

/// The self-check. **Fails closed**: every condition must hold.
///
/// `expected` is the projected per-task cost; `tolerance` is how far above it a
/// single task may land before the projection is considered wrong. A first task
/// far outside the band means the estimate the operator approved does not
/// describe this run, and continuing spends against a number that no longer
/// holds.
pub fn check_first_task(
    t: &FirstTask,
    expected: Micros,
    tolerance_multiple: u64,
) -> Result<Micros, FirstTaskHalt> {
    let mut reasons = Vec::new();
    if !t.invocation_succeeded {
        reasons.push(
            "the invocation failed — most likely the model slug is not accepted by \
             this CLI, which is the one assumption worth a single task to test"
                .to_string(),
        );
    }
    if !t.rollout_file_found {
        reasons.push(
            "no session rollout file was produced, so usage cannot be measured and \
             any cost figure would be inferred"
                .to_string(),
        );
    }
    if !t.rollout_parsed {
        reasons.push("the rollout file did not parse".to_string());
    }
    match &t.usage {
        None => reasons.push(
            "usage was not measured. A projection built on an inferred figure is an \
             estimate wearing a measurement's name"
                .to_string(),
        ),
        Some(u) if u.source.may_understate() => reasons.push(format!(
            "usage came from `turn.completed`, which omits reasoning tokens; that is \
             a floor, not a measurement (input {}, output {})",
            u.input_tokens, u.output_tokens
        )),
        Some(_) => {}
    }
    let ceiling = expected.saturating_mul(tolerance_multiple);
    if t.cost > ceiling {
        reasons.push(format!(
            "task 1 cost {} micro-dollars against a projected {} — more than {}x, so \
             the projection the run was approved against does not describe it",
            t.cost, expected, tolerance_multiple
        ));
    }
    if reasons.is_empty() {
        Ok(t.cost)
    } else {
        Err(FirstTaskHalt {
            task_id: t.task_id.clone(),
            reasons,
        })
    }
}

// ---------------------------------------------------------------------------
// The committed Terminal-Bench split (BUILD-SPEC §20.2, operator Ruling 5).
// ---------------------------------------------------------------------------

/// The committed manifest, compiled in.
///
/// `include_str!` rather than a runtime read, deliberately: a path read at run
/// time is a path someone can point elsewhere, and the whole point of Ruling 5
/// is that membership is **pinned** rather than supplied. The bytes that
/// shipped are the bytes that decide.
pub const TERMINAL_BENCH_SPLIT: &str =
    include_str!("../../../fixtures/terminal-bench-holdout-v1.tsv");

/// How strong the record behind an exclusion is.
///
/// On every row, so the narrower definition of "observed" is one filter away
/// and nobody has to take the count on trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObservedTier {
    /// The task was staged, containerised and run: a Terminal-Bench
    /// `results.json` carries its `task_id`.
    RunRecord,
    /// The instrument reached the task — it was copied, instrumented and
    /// mounted — and produced nothing. An empty spool is task-specific
    /// knowledge too, which is §12's whole point about empty spools.
    StagedOnly,
    /// Never run. The task is named individually in a committed document or in
    /// the shipped instrument, and a property of *that* task is asserted there.
    RepoNamed,
}

impl ObservedTier {
    fn named(s: &str) -> Option<ObservedTier> {
        match s {
            "run-record" => Some(ObservedTier::RunRecord),
            "staged-only" => Some(ObservedTier::StagedOnly),
            "repo-named" => Some(ObservedTier::RepoNamed),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ObservedTier::RunRecord => "run-record",
            ObservedTier::StagedOnly => "staged-only",
            ObservedTier::RepoNamed => "repo-named",
        }
    }

    /// Did the instrument actually touch this task, as opposed to the
    /// investigation reasoning about it?
    pub fn is_execution(self) -> bool {
        matches!(self, ObservedTier::RunRecord | ObservedTier::StagedOnly)
    }
}

/// One excluded task, **with the record that proves it was observed**.
///
/// `record` is not optional and the parser refuses an empty one. A name with no
/// evidence beside it is exactly what Ruling 5 forbids, so it is not a shape
/// this type can hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub task: String,
    pub tier: ObservedTier,
    /// A path, a `file:line`, something re-checkable.
    pub record: String,
    /// `sha256:…` of the record file where one exists, or `git-tracked`.
    pub fingerprint: String,
    /// What the record actually says about *this* task.
    pub note: String,
}

/// Why the committed split would not load.
///
/// Every variant is a **drift detector**, not a parse inconvenience: the
/// manifest re-derives itself on load, so a hand-edited assignment, a padded
/// count or a nameless exclusion cannot reach a caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    Malformed {
        line: usize,
        why: &'static str,
    },
    MissingField {
        field: &'static str,
    },
    /// A `task` row whose declared side disagrees with `bucket(salt, id) < cut`.
    /// **This is the re-derivation.**
    AssignmentDrift {
        task: String,
        declared: &'static str,
        derived: &'static str,
    },
    /// A declared count that does not match what the rows add up to.
    CountDrift {
        field: &'static str,
        declared: usize,
        derived: usize,
    },
    /// `corpus_digest` does not match the ids listed in this same file.
    DigestDrift {
        declared: String,
        derived: String,
    },
    /// An `observed` row naming a task the split does not contain.
    ObservedNotInCorpus {
        task: String,
    },
    ObservedTwice {
        task: String,
    },
    /// The corpus on this machine is not the corpus this file was cut from.
    CorpusDrift {
        only_in_file: Vec<String>,
        only_on_disk: Vec<String>,
    },
    /// Someone asked for the holdout names without a recorded spend.
    HoldoutNotReleased {
        expected_protocol_hash: String,
        got: String,
    },
    /// The ledger says this holdout was already consumed.
    HoldoutAlreadySpent {
        prior_runs: Vec<String>,
    },
}

impl std::fmt::Display for SplitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SplitError::Malformed { line, why } => {
                write!(f, "terminal-bench split, line {line}: {why}")
            }
            SplitError::MissingField { field } => write!(
                f,
                "the terminal-bench split declares no `{field}`, and a split that \
                 does not state its own parameters cannot be recomputed by anyone"
            ),
            SplitError::AssignmentDrift {
                task,
                declared,
                derived,
            } => write!(
                f,
                "`{task}` is filed as `{declared}` and its hash puts it in \
                 `{derived}`. The committed assignment was edited by hand, or the \
                 salt moved: either way the split on disk is not the split the hash \
                 describes, and this is the drift the re-derivation exists to catch"
            ),
            SplitError::CountDrift {
                field,
                declared,
                derived,
            } => write!(
                f,
                "`{field}` says {declared}; the rows say {derived}. A header that \
                 disagrees with its own body is how a padded exclusion list looks \
                 complete"
            ),
            SplitError::DigestDrift { declared, derived } => write!(
                f,
                "`corpus_digest` is {declared}; the ids in this file hash to \
                 {derived}"
            ),
            SplitError::ObservedNotInCorpus { task } => write!(
                f,
                "`{task}` is excluded as observed but is not one of the corpus ids \
                 in this file"
            ),
            SplitError::ObservedTwice { task } => {
                write!(f, "`{task}` appears in two `observed` rows")
            }
            SplitError::CorpusDrift {
                only_in_file,
                only_on_disk,
            } => write!(
                f,
                "the corpus on this machine is not the one this split was cut from: \
                 {} id(s) only in the file ({}), {} only on disk ({}). A corpus bump \
                 is a re-measurement, not a refactor — re-cut the split",
                only_in_file.len(),
                only_in_file.join(", "),
                only_on_disk.len(),
                only_on_disk.join(", ")
            ),
            SplitError::HoldoutNotReleased {
                expected_protocol_hash,
                got,
            } => write!(
                f,
                "the holdout names were requested with a manifest for `{got}`, not \
                 for this split (`{expected_protocol_hash}`). Reading the holdout is \
                 a spend: obtain a `RunManifest` from `HoldoutLedger::spend` under \
                 this split's own hash, which records that you read it"
            ),
            SplitError::HoldoutAlreadySpent { prior_runs } => write!(
                f,
                "this holdout was already consumed by {} prior run(s) ({}). A \
                 holdout read a second time is a validation set wearing a holdout's \
                 name",
                prior_runs.len(),
                prior_runs.join(", ")
            ),
        }
    }
}

impl std::error::Error for SplitError {}

/// The committed 50/50 Terminal-Bench split, minus the observed set.
///
/// # The anti-contamination property, and why it is a construction
///
/// [`TerminalBenchSplit::tune`] returns `corpus \ holdout` — a **set difference
/// taken once, at load**. A draw from it cannot contain a holdout id because
/// the holdout ids are not elements of the slice being drawn from. There is no
/// per-draw filter to forget, no predicate to invert, and no argument a caller
/// can pass that widens the pool: [`TerminalBenchSplit::tune_protocol`] is the
/// only way to seed a [`Protocol`] from this file, and it seeds `tasks` from
/// `tune` and `holdout` from `holdout`.
///
/// # The bypass this closes
///
/// `RunManifest::first()` was one call with `prior_runs: Vec::new()` that
/// walked past the entire holdout ledger. The analogue here is
/// [`plan`] reading `base.holdout`: a caller who hands it a `Protocol` with an
/// **empty** holdout gets `tune_split(tasks, &[])` — the whole corpus — and
/// `check_from_tune` then compares against the same empty set and agrees. Two
/// functions, one hole, and neither of them lies. `plan` now refuses an empty
/// holdout ([`PilotError::UncommittedHoldout`]), and a protocol built through
/// `tune_protocol` never has one.
///
/// # Reading the holdout is itself a spend
///
/// [`TerminalBenchSplit::holdout`] takes a [`RunManifest`], which has no public
/// constructor: the only way to obtain one is [`HoldoutLedger::spend`], and
/// obtaining it appends the run to an append-only file. So the holdout's
/// **names** cannot be enumerated through this type without leaving a record
/// that they were, and the second attempt refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalBenchSplit {
    salt: String,
    cut: u64,
    corpus_digest: String,
    ids: Vec<String>,
    holdout: Vec<String>,
    tune: Vec<String>,
    observed: Vec<Observed>,
}

impl TerminalBenchSplit {
    /// Parses the compiled-in manifest, **re-deriving every assignment**.
    pub fn load() -> Result<TerminalBenchSplit, SplitError> {
        TerminalBenchSplit::parse(TERMINAL_BENCH_SPLIT)
    }

    /// Parses arbitrary manifest text. Public so a test can feed it a tampered
    /// copy and watch the refusal fire, which is the only way to know the
    /// re-derivation is load-bearing rather than decorative.
    pub fn parse(text: &str) -> Result<TerminalBenchSplit, SplitError> {
        let mut salt: Option<String> = None;
        let mut cut: Option<u64> = None;
        let mut digest: Option<String> = None;
        let mut declared: BTreeMapStr = BTreeMapStr::new();
        let mut ids: Vec<String> = Vec::new();
        let mut side: Vec<(String, bool)> = Vec::new();
        let mut observed: Vec<Observed> = Vec::new();

        for (n, raw) in text.lines().enumerate() {
            let line = n + 1;
            let raw = raw.trim_end_matches(['\r']);
            if raw.trim().is_empty() || raw.starts_with('#') {
                continue;
            }
            let mut f = raw.split('\t');
            let key = f.next().unwrap_or("");
            match key {
                "version" | "corpus" => {}
                "salt" => {
                    salt = Some(field(f.next(), line, "salt is empty")?.to_string());
                }
                "cut" => {
                    let v = field(f.next(), line, "cut is empty")?;
                    cut = Some(v.parse::<u64>().map_err(|_| SplitError::Malformed {
                        line,
                        why: "cut is not a u64",
                    })?);
                }
                "corpus_digest" => {
                    digest = Some(field(f.next(), line, "corpus_digest is empty")?.to_string());
                }
                "corpus_size"
                | "holdout_side"
                | "tune_side"
                | "observed_total"
                | "observed_run_record_or_staged"
                | "holdout_size"
                | "holdout_size_run_record_only" => {
                    let v = field(f.next(), line, "count is empty")?;
                    let n = v.parse::<usize>().map_err(|_| SplitError::Malformed {
                        line,
                        why: "count is not a number",
                    })?;
                    declared.push(key.to_string(), n);
                }
                "task" => {
                    let id = field(f.next(), line, "task row has no id")?;
                    let s = field(f.next(), line, "task row has no side")?;
                    let held = match s {
                        "holdout" => true,
                        "tune" => false,
                        _ => {
                            return Err(SplitError::Malformed {
                                line,
                                why: "task side must be `holdout` or `tune`",
                            })
                        }
                    };
                    ids.push(id.to_string());
                    side.push((id.to_string(), held));
                }
                "observed" => {
                    let task = field(f.next(), line, "observed row has no task")?;
                    let tier = field(f.next(), line, "observed row has no tier")?;
                    let record = field(f.next(), line, "observed row has no record")?;
                    let fingerprint = field(f.next(), line, "observed row has no fingerprint")?;
                    let note = f.next().unwrap_or("").trim();
                    let tier = ObservedTier::named(tier).ok_or(SplitError::Malformed {
                        line,
                        why: "unknown observed tier",
                    })?;
                    if note.is_empty() {
                        return Err(SplitError::Malformed {
                            line,
                            why: "observed row has no note saying what the record proves",
                        });
                    }
                    observed.push(Observed {
                        task: task.to_string(),
                        tier,
                        record: record.to_string(),
                        fingerprint: fingerprint.to_string(),
                        note: note.to_string(),
                    });
                }
                _ => {
                    return Err(SplitError::Malformed {
                        line,
                        why: "unknown row key",
                    })
                }
            }
        }

        let salt = salt.ok_or(SplitError::MissingField { field: "salt" })?;
        let cut = cut.ok_or(SplitError::MissingField { field: "cut" })?;
        let digest = digest.ok_or(SplitError::MissingField {
            field: "corpus_digest",
        })?;
        if ids.is_empty() {
            return Err(SplitError::MissingField { field: "task" });
        }

        // **The re-derivation.** Every declared side is recomputed from the
        // hash. A hand-edited row does not load.
        for (id, held) in &side {
            let derived = crate::protocol::bucket(&salt, id) < cut;
            if derived != *held {
                return Err(SplitError::AssignmentDrift {
                    task: id.clone(),
                    declared: if *held { "holdout" } else { "tune" },
                    derived: if derived { "holdout" } else { "tune" },
                });
            }
        }

        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != ids.len() {
            return Err(SplitError::CountDrift {
                field: "task",
                declared: ids.len(),
                derived: sorted.len(),
            });
        }
        let derived_digest = format!("sha256:{}", sha256_lines(&sorted));
        if derived_digest != digest {
            return Err(SplitError::DigestDrift {
                declared: digest,
                derived: derived_digest,
            });
        }

        let known: BTreeSet<&String> = sorted.iter().collect();
        let mut seen: BTreeSet<&String> = BTreeSet::new();
        for o in &observed {
            if !known.contains(&o.task) {
                return Err(SplitError::ObservedNotInCorpus {
                    task: o.task.clone(),
                });
            }
            if !seen.insert(&o.task) {
                return Err(SplitError::ObservedTwice {
                    task: o.task.clone(),
                });
            }
        }
        let excluded: BTreeSet<String> = observed.iter().map(|o| o.task.clone()).collect();
        let held: BTreeSet<String> = side
            .iter()
            .filter(|(_, h)| *h)
            .map(|(i, _)| i.clone())
            .collect();
        let holdout: Vec<String> = sorted
            .iter()
            .filter(|i| held.contains(*i) && !excluded.contains(*i))
            .cloned()
            .collect();
        let holdout_set: BTreeSet<&String> = holdout.iter().collect();
        let tune: Vec<String> = sorted
            .iter()
            .filter(|i| !holdout_set.contains(*i))
            .cloned()
            .collect();

        let narrow = sorted
            .iter()
            .filter(|i| {
                held.contains(*i)
                    && !observed
                        .iter()
                        .any(|o| &o.task == *i && o.tier.is_execution())
            })
            .count();
        declared.check("corpus_size", sorted.len())?;
        declared.check("holdout_side", held.len())?;
        declared.check("tune_side", sorted.len() - held.len())?;
        declared.check("observed_total", observed.len())?;
        declared.check(
            "observed_run_record_or_staged",
            observed.iter().filter(|o| o.tier.is_execution()).count(),
        )?;
        declared.check("holdout_size", holdout.len())?;
        declared.check("holdout_size_run_record_only", narrow)?;

        observed.sort_by(|a, b| a.task.cmp(&b.task));
        Ok(TerminalBenchSplit {
            salt,
            cut,
            corpus_digest: digest,
            ids: sorted,
            holdout,
            tune,
            observed,
        })
    }

    pub fn salt(&self) -> &str {
        &self.salt
    }
    pub fn cut(&self) -> u64 {
        self.cut
    }
    pub fn corpus_digest(&self) -> &str {
        &self.corpus_digest
    }
    pub fn corpus_size(&self) -> usize {
        self.ids.len()
    }
    pub fn holdout_size(&self) -> usize {
        self.holdout.len()
    }
    pub fn observed(&self) -> &[Observed] {
        &self.observed
    }

    /// How many observed tasks the hash put on the holdout side.
    ///
    /// **This is the only number that shrinks the holdout**, and conflating it
    /// with the total observed count is how §20.2 arrived at "118 minus 13":
    /// most observed tasks were on the tune side already, where excluding them
    /// changes nothing.
    pub fn observed_in_holdout_side(&self) -> usize {
        self.observed
            .iter()
            .filter(|o| crate::protocol::bucket(&self.salt, &o.task) < self.cut)
            .count()
    }

    /// Everything a pilot or tuning run may draw from: `corpus \ holdout`.
    ///
    /// The exclusion happened at load. Nothing a caller does to this slice can
    /// produce a holdout id, because none is in it.
    pub fn tune(&self) -> &[String] {
        &self.tune
    }

    /// The holdout names — **and obtaining them records that you did**.
    ///
    /// `spend` must be a [`RunManifest`] for this split's own hash, and
    /// `RunManifest` has no public constructor: the only door is
    /// [`HoldoutLedger::spend`], which appends to an append-only file before it
    /// returns. A second call arrives with a non-empty `prior_runs` and is
    /// refused.
    pub fn holdout(&self, spend: &crate::RunManifest) -> Result<&[String], SplitError> {
        if spend.protocol_hash() != self.split_hash() {
            return Err(SplitError::HoldoutNotReleased {
                expected_protocol_hash: self.split_hash(),
                got: spend.protocol_hash().to_string(),
            });
        }
        if !spend.prior_runs().is_empty() {
            return Err(SplitError::HoldoutAlreadySpent {
                prior_runs: spend.prior_runs().to_vec(),
            });
        }
        Ok(&self.holdout)
    }

    /// The key this split is spent under in a [`HoldoutLedger`].
    ///
    /// Derived from the file's own parameters and its corpus digest, so a
    /// re-cut split is a different key and cannot inherit an old spend record.
    pub fn split_hash(&self) -> String {
        format!(
            "sha256:{}",
            sha256_lines(&[
                "terminal-bench-holdout-v1".to_string(),
                self.salt.clone(),
                self.cut.to_string(),
                self.corpus_digest.clone(),
                self.holdout.len().to_string(),
            ])
        )
    }

    /// Seeds a [`Protocol`] whose `tasks` are the tune split and whose
    /// `holdout` is the committed one.
    ///
    /// The only sanctioned way to build a Terminal-Bench protocol from this
    /// file. `tasks` is `tune`, so a sampler that walks `p.tasks` cannot reach
    /// a holdout id even if every later guard were deleted.
    pub fn tune_protocol(&self, base: &Protocol) -> Protocol {
        let mut p = base.clone();
        p.tasks = self.tune.clone();
        p.holdout = self.holdout.clone();
        p
    }

    /// Compares this file's ids against a corpus directory on disk and **fails
    /// on any difference**.
    ///
    /// This is the half of the re-derivation the compiled-in text cannot do by
    /// itself: the hash check proves the file is internally consistent, and
    /// only this proves it still describes the corpus.
    pub fn check_against_corpus(&self, dir: &std::path::Path) -> Result<(), SplitError> {
        let mut on_disk: Vec<String> = Vec::new();
        let entries = std::fs::read_dir(dir).map_err(|_| SplitError::CorpusDrift {
            only_in_file: self.ids.clone(),
            only_on_disk: Vec::new(),
        })?;
        for e in entries.flatten() {
            if e.path().is_dir() {
                if let Some(n) = e.file_name().to_str() {
                    on_disk.push(n.to_string());
                }
            }
        }
        on_disk.sort();
        let disk: BTreeSet<&String> = on_disk.iter().collect();
        let file: BTreeSet<&String> = self.ids.iter().collect();
        let only_in_file: Vec<String> = file.difference(&disk).map(|s| (*s).clone()).collect();
        let only_on_disk: Vec<String> = disk.difference(&file).map(|s| (*s).clone()).collect();
        if only_in_file.is_empty() && only_on_disk.is_empty() {
            Ok(())
        } else {
            Err(SplitError::CorpusDrift {
                only_in_file,
                only_on_disk,
            })
        }
    }
}

fn field<'a>(v: Option<&'a str>, line: usize, why: &'static str) -> Result<&'a str, SplitError> {
    match v.map(str::trim) {
        Some(s) if !s.is_empty() => Ok(s),
        _ => Err(SplitError::Malformed { line, why }),
    }
}

/// `sha256` over the values, one per line, each newline-terminated.
fn sha256_lines(v: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for s in v {
        h.update(s.as_bytes());
        h.update(b"\n");
    }
    let mut out = String::new();
    for b in h.finalize() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// The declared counts, kept in order so a mismatch names the field.
///
/// A three-line helper rather than a `BTreeMap` import: the only operation is
/// "was this declared, and does it match".
struct BTreeMapStr(Vec<(String, usize)>);

impl BTreeMapStr {
    fn new() -> BTreeMapStr {
        BTreeMapStr(Vec::new())
    }
    fn push(&mut self, k: String, v: usize) {
        self.0.push((k, v));
    }
    fn check(&self, field: &'static str, derived: usize) -> Result<(), SplitError> {
        match self.0.iter().find(|(k, _)| k == field) {
            None => Err(SplitError::MissingField { field }),
            Some((_, d)) if *d == derived => Ok(()),
            Some((_, d)) => Err(SplitError::CountDrift {
                field,
                declared: *d,
                derived,
            }),
        }
    }
}
