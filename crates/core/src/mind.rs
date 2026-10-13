//! RB6a — the local mind, prompted baseline. Narrates and proposes, never decides.
//!
//! Blueprint v3 keeps RB6's phasing untouched: **prompted baseline → adjudication
//! corpus → gated LoRA**, and at every phase the local model *narrates and
//! proposes, never decides*. This module is the first phase, and it is cut
//! precisely: RB7's distilled layer already produces the deterministic skeleton,
//! and RB6a fills **Layer 3, and only Layer 3** — the interpretation slots the
//! skeleton deliberately leaves empty.
//!
//! ## Exactly three jobs
//!
//! [`MindJob`] has three variants and a test asserts the count. The constraint
//! is the design: a local model that may be asked anything becomes a local model
//! that is asked to decide things, and RB6's whole phasing exists to stop that.
//!
//! ## Everything it emits is labeled, and the label is not optional
//!
//! Every [`MindOutput`] carries `tier = "language"`, `mode = "shadow"` and
//! `truth_bearing = false`. Those are not fields a caller sets — they are
//! constants this module stamps, and [`MindOutput`] cannot be constructed
//! without them. The UX-DOCTRINE RB7 amendment requires Layer 3 to be gated on
//! provenance and rendered as `model-proposed-shadow`; a proposal that lost its
//! label on the way to a surface would be indistinguishable from a proof.
//!
//! ## No model is the normal case, not the error case
//!
//! [`NoModelProvider`] is the mandatory fallback and it is what runs when Ollama
//! is absent — which is the state of the machine this module was written on. It
//! **abstains**; it never fabricates. `insufficient evidence is acceptable` is
//! already doctrine for RB2's hypotheses (C4.3), and it is doctrine here.
//!
//! ## The basis this was built on, recorded honestly
//!
//! The `ollama` CLI is installed on the development machine; the server is not
//! running and **no model is present** (`ollama list` → *"could not find ollama
//! app"*). Downloading one is a network fetch and a disk cost nobody asked for,
//! so it was not done. The eval harness therefore runs against [`MockProvider`],
//! the documented mock tier, and [`OllamaProvider`] is **not exercised against a
//! live model anywhere in this test suite**. That is stated here, in the module
//! doc, rather than left for someone to discover — a provider whose only
//! evidence is that it compiles should say so.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::language::{ModelSelection, RamTier};

/// The label every output carries. Not a caller's choice.
pub const TIER: &str = "language";
/// Shadow-first, per RB6's phasing. Nothing here reaches a truth-bearing path.
pub const MODE: &str = "shadow";

/// The three jobs the local mind is allowed to do, and there is no fourth.
///
/// Blueprint v3: *narrates and proposes, never decides.* Each of these is a
/// narration or a proposal. None of them returns a verdict, a state, or a
/// classification that any other module consumes as fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MindJob {
    /// Layer-3 decision-extraction **proposals** for RB7's empty slots.
    DecisionProposal,
    /// A plain-English account of what a session did.
    SessionNarration,
    /// Help shortening a capsule without dropping what it must carry.
    CapsuleCompaction,
}

impl MindJob {
    /// Every job, for the surfaces that enumerate them and the test that pins
    /// the count.
    pub const ALL: &'static [MindJob] = &[
        MindJob::DecisionProposal,
        MindJob::SessionNarration,
        MindJob::CapsuleCompaction,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            MindJob::DecisionProposal => "decision_proposal",
            MindJob::SessionNarration => "session_narration",
            MindJob::CapsuleCompaction => "capsule_compaction",
        }
    }

    /// The slot a proposal from this job lands in. Layer-3 slots are named by
    /// RB7's skeleton; the other two jobs name themselves.
    pub fn slot(self) -> &'static str {
        match self {
            MindJob::DecisionProposal => "layer3_decision",
            MindJob::SessionNarration => "layer3_narration",
            MindJob::CapsuleCompaction => "layer3_compaction",
        }
    }

    /// The instruction the model is given. Kept here rather than in the
    /// provider so that a prompt change is a change to the *job*, reviewable in
    /// one place, and so the eval harness prompts exactly what production does.
    pub fn instruction(self) -> &'static str {
        match self {
            MindJob::DecisionProposal => {
                "You are proposing, not deciding. Given the deterministic record of one \
                 coding session, propose in one sentence the DECISION that was made. If \
                 the record does not show a decision, answer exactly: INSUFFICIENT. \
                 Never invent a file, command, or result that is not in the record."
            }
            MindJob::SessionNarration => {
                "Narrate what this coding session did, in at most two plain sentences, \
                 for someone who was not there. Use only what the record states. If the \
                 record is too thin to narrate, answer exactly: INSUFFICIENT."
            }
            MindJob::CapsuleCompaction => {
                "Shorten this handoff text without dropping any file path, command, \
                 identifier, or open question it contains. If you cannot shorten it \
                 without losing one of those, answer exactly: INSUFFICIENT."
            }
        }
    }
}

impl fmt::Display for MindJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why the mind produced nothing. Abstention is a first-class answer, never a
/// failure to be papered over (C4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Abstention {
    /// No local model is installed or reachable. The mandatory fallback.
    NoModel,
    /// The provider was reachable and failed.
    Provider,
    /// The model answered, and the answer could not be used as it stands.
    Unusable,
    /// The model said so itself — the instruction's own escape hatch.
    ModelSaidInsufficient,
}

impl Abstention {
    pub fn as_str(self) -> &'static str {
        match self {
            Abstention::NoModel => "no_model",
            Abstention::Provider => "provider",
            Abstention::Unusable => "unusable",
            Abstention::ModelSaidInsufficient => "model_said_insufficient",
        }
    }
}

/// What the mind produced, if anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum MindResult {
    /// A proposal for one Layer-3 slot. Never more than one per call: a batch
    /// is a place for one bad answer to hide behind four good ones.
    Proposal { slot: String, text: String },
    /// Nothing, and why.
    Abstained { reason: Abstention, detail: String },
}

impl MindResult {
    pub fn is_proposal(&self) -> bool {
        matches!(self, MindResult::Proposal { .. })
    }
}

/// What produced an output, named so a reader can tell a model's guess from a
/// deterministic fact without knowing how either was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Model id, or `none` when no model ran.
    pub model: String,
    /// Hardware tier the model was selected for, or `none`.
    pub tier: String,
    /// Provider implementation: `ollama`, `mock`, or `no-model`.
    pub provider: String,
}

impl Provenance {
    pub fn none(provider: &str) -> Self {
        Provenance {
            model: "none".to_string(),
            tier: "none".to_string(),
            provider: provider.to_string(),
        }
    }
}

/// One labeled output. The label travels with the content, always.
///
/// There is deliberately no public constructor that takes `tier`, `mode` or
/// `truth_bearing`: they are stamped by [`MindOutput::new`] and a caller cannot
/// set them to anything else. A Layer-3 proposal that reached a surface without
/// its `model-proposed-shadow` provenance would be indistinguishable from a
/// proof, which is the one thing the RB7 amendment forbids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MindOutput {
    pub job: MindJob,
    pub tier: String,
    pub mode: String,
    pub truth_bearing: bool,
    pub provenance: Provenance,
    pub result: MindResult,
}

impl MindOutput {
    fn new(job: MindJob, provenance: Provenance, result: MindResult) -> Self {
        MindOutput {
            job,
            tier: TIER.to_string(),
            mode: MODE.to_string(),
            truth_bearing: false,
            provenance,
            result,
        }
    }

    /// The words a surface may show beside this output. One string, so the
    /// label cannot be shown in one place and forgotten in another.
    pub fn label(&self) -> String {
        format!(
            "model-proposed-shadow · {} · {}",
            self.provenance.provider, self.provenance.model
        )
    }
}

/// A source of completions. Deliberately narrow: one string in, one string out.
///
/// Anything richer would let a provider return structure that a caller could
/// mistake for an engine fact.
pub trait MindProvider: Send + Sync {
    /// Who this is, for [`Provenance`].
    fn provenance(&self) -> Provenance;
    /// Run one job. `Err` is a provider failure; `Ok` is whatever the model
    /// said, unvalidated.
    fn complete(&self, job: MindJob, record: &str) -> std::result::Result<String, String>;
}

/// The mandatory fallback: no model, no egress, no answer, no pretending.
///
/// This is not a degraded mode to be apologised for. On a machine with no local
/// model it is the *only* correct behaviour, and it is the state of the machine
/// RB6a was written on.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoModelProvider;

impl MindProvider for NoModelProvider {
    fn provenance(&self) -> Provenance {
        Provenance::none("no-model")
    }
    fn complete(&self, _job: MindJob, _record: &str) -> std::result::Result<String, String> {
        Err("no local model installed".to_string())
    }
}

/// The documented mock tier.
///
/// It exists because the eval harness has to run somewhere, and because a
/// harness that only runs when a 5 GB model is present is a harness nobody runs.
/// It is **deterministic and obviously not a model**: it echoes a fixed
/// transformation of the record, so a specimen that "passes" against it proves
/// the plumbing, the labeling and the abstention paths — and proves nothing at
/// all about model quality. That distinction is the whole reason it is named
/// `mock` in the provenance every output carries.
#[derive(Debug, Clone)]
pub struct MockProvider {
    /// Records containing this marker make the mock abstain, so the abstention
    /// path is exercised by real specimens rather than only by unit tests.
    pub insufficient_marker: String,
}

impl Default for MockProvider {
    fn default() -> Self {
        MockProvider {
            insufficient_marker: "<<thin>>".to_string(),
        }
    }
}

impl MindProvider for MockProvider {
    fn provenance(&self) -> Provenance {
        Provenance {
            model: "mock-deterministic-v1".to_string(),
            tier: "none".to_string(),
            provider: "mock".to_string(),
        }
    }

    fn complete(&self, job: MindJob, record: &str) -> std::result::Result<String, String> {
        if record.contains(&self.insufficient_marker) || record.trim().is_empty() {
            return Ok("INSUFFICIENT".to_string());
        }
        let first = record
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("");
        Ok(match job {
            MindJob::DecisionProposal => format!("Proposed decision: {first}"),
            MindJob::SessionNarration => format!("The session {first}"),
            MindJob::CapsuleCompaction => first.to_string(),
        })
    }
}

/// The Ollama tier, hardware-selected by the existing `language` module.
///
/// **Not exercised against a live model in this test suite.** No model is
/// installed on the development machine and fetching one is a network and disk
/// cost nobody asked for, so the eval harness runs on [`MockProvider`] and this
/// type's evidence is that it compiles and that its request shape matches
/// `language::OllamaAnnotator`, which is exercised. Recorded here rather than
/// discovered later.
#[derive(Debug, Clone)]
pub struct OllamaProvider {
    pub base_url: String,
    pub selection: ModelSelection,
    pub timeout_secs: u64,
}

impl OllamaProvider {
    /// Build a provider for this machine's memory tier.
    ///
    /// Loopback only, and checked here rather than trusted: the core's single
    /// permitted egress is a local model server, and a base URL pointing
    /// anywhere else would turn a shadow-mode narration into an exfiltration
    /// channel for transcript text.
    pub fn new(base_url: &str, selection: ModelSelection) -> std::result::Result<Self, String> {
        if !is_loopback(base_url) {
            return Err(format!(
                "the local mind refuses a non-loopback model endpoint: {base_url}"
            ));
        }
        Ok(OllamaProvider {
            base_url: base_url.trim_end_matches('/').to_string(),
            selection,
            timeout_secs: 60,
        })
    }

    pub fn tier(&self) -> RamTier {
        self.selection.tier
    }
}

/// Whether a URL addresses this machine only.
///
/// Written as an allowlist of hosts, not a denylist of bad ones: a denylist for
/// "is this my own machine" is a denylist that eventually lets something
/// through.
pub fn is_loopback(url: &str) -> bool {
    let rest = match url.split_once("://") {
        Some(("http", rest)) | Some(("https", rest)) => rest,
        _ => return false,
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or_else(|| rest.split(['/', '?', '#']).next().unwrap_or(""));
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

impl MindProvider for OllamaProvider {
    fn provenance(&self) -> Provenance {
        Provenance {
            model: self.selection.model.clone(),
            tier: format!("{:?}", self.selection.tier).to_lowercase(),
            provider: "ollama".to_string(),
        }
    }

    fn complete(&self, job: MindJob, record: &str) -> std::result::Result<String, String> {
        #[derive(Serialize)]
        struct Message<'a> {
            role: &'a str,
            content: &'a str,
        }
        #[derive(Serialize)]
        struct Request<'a> {
            model: &'a str,
            messages: Vec<Message<'a>>,
            stream: bool,
            temperature: f32,
            seed: u32,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: Content,
        }
        #[derive(Deserialize)]
        struct Content {
            content: String,
        }
        #[derive(Deserialize)]
        struct Response {
            choices: Vec<Choice>,
        }

        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(self.timeout_secs)))
            .build()
            .new_agent();
        let request = Request {
            model: &self.selection.model,
            messages: vec![
                Message {
                    role: "system",
                    content: job.instruction(),
                },
                Message {
                    role: "user",
                    content: record,
                },
            ],
            stream: false,
            temperature: 0.0,
            seed: 0,
        };
        let mut response = agent
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", "Bearer ollama")
            .send_json(&request)
            .map_err(|e| e.to_string())?;
        let parsed: Response = response.body_mut().read_json().map_err(|e| e.to_string())?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| "the model returned no choices".to_string())
    }
}

/// Run one job, and label whatever comes back.
///
/// This is the only way to obtain a [`MindOutput`], which is what makes the
/// labeling unavoidable rather than conventional.
pub fn run(provider: &dyn MindProvider, job: MindJob, record: &str) -> MindOutput {
    let provenance = provider.provenance();
    match provider.complete(job, record) {
        Err(detail) => {
            let reason = if provenance.provider == "no-model" {
                Abstention::NoModel
            } else {
                Abstention::Provider
            };
            MindOutput::new(job, provenance, MindResult::Abstained { reason, detail })
        }
        Ok(raw) => {
            let text = raw.trim();
            if text.is_empty() {
                return MindOutput::new(
                    job,
                    provenance,
                    MindResult::Abstained {
                        reason: Abstention::Unusable,
                        detail: "the model returned nothing".to_string(),
                    },
                );
            }
            // The instruction gives the model an explicit way to decline, and
            // taking it is an answer rather than a failure. Checked before any
            // other parsing so a declining model is never re-read as content.
            if text.split_whitespace().any(|w| {
                w.trim_matches(|c: char| !c.is_alphabetic())
                    .eq_ignore_ascii_case("INSUFFICIENT")
            }) {
                return MindOutput::new(
                    job,
                    provenance,
                    MindResult::Abstained {
                        reason: Abstention::ModelSaidInsufficient,
                        detail: text.to_string(),
                    },
                );
            }
            MindOutput::new(
                job,
                provenance,
                MindResult::Proposal {
                    slot: job.slot().to_string(),
                    text: text.to_string(),
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_only() {
        assert!(is_loopback("http://127.0.0.1:11434/v1"));
        assert!(is_loopback("http://localhost:11434/v1"));
        assert!(!is_loopback("http://192.168.1.10:11434/v1"));
        assert!(!is_loopback("http://evil.example.com/v1"));
        assert!(!is_loopback("file:///etc/passwd"));
        // The one that matters: a host that merely CONTAINS "localhost".
        assert!(!is_loopback("http://localhost.evil.example.com/v1"));
    }
}
