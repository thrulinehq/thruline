//! BUILD-SPEC §3.2 — the claim, and the projection verifiers are handed.
//!
//! # `confidence` does not exist here, and cannot be added by accident
//!
//! **[RULING R3 / BUILD-PLAN C.8.]** §3.2 lists a `confidence` field. It is
//! deleted. The rules extractor is deterministic, so its confidence is the
//! constant 1.0 and carries no information; the only producer of a
//! *non-constant* value is §5.3's optional local-model extractor, which is off
//! by default and forbidden from verdicts. A field that is informative only
//! where it is forbidden should not exist.
//!
//! # The type boundary, which is the real enforcement
//!
//! Verifiers do not receive [`Claim`]. They receive [`VerifiableClaim`], which
//! structurally cannot carry `text`, `span` or `extractor` — so a verifier
//! *cannot* read model-derived data, and writing one that tries is a compile
//! error rather than a review finding. The projection is lossless for the
//! verdict path, because §6's verifiers are pure functions of
//! `(kind, args, events, snapshots, checks, policy)`; `text` and `span` exist
//! for display.
//!
//! `extractor` stays on `Claim`. It is categorical provenance — where a claim
//! came from — not a score, and it cannot be thresholded.

use serde::{Deserialize, Serialize};

/// §3.2 `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    TestsPass,
    BuildOk,
    LintOk,
    TypecheckOk,
    BehaviorUnchanged,
    MigrationAdded,
    SecretsAbsent,
    DocsUpdated,
    DependencyAdded,
    FileChanged,
    DidX,
    TaskComplete,
}

impl ClaimKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimKind::TestsPass => "tests_pass",
            ClaimKind::BuildOk => "build_ok",
            ClaimKind::LintOk => "lint_ok",
            ClaimKind::TypecheckOk => "typecheck_ok",
            ClaimKind::BehaviorUnchanged => "behavior_unchanged",
            ClaimKind::MigrationAdded => "migration_added",
            ClaimKind::SecretsAbsent => "secrets_absent",
            ClaimKind::DocsUpdated => "docs_updated",
            ClaimKind::DependencyAdded => "dependency_added",
            ClaimKind::FileChanged => "file_changed",
            ClaimKind::DidX => "did_x",
            ClaimKind::TaskComplete => "task_complete",
        }
    }

    pub fn parse(name: &str) -> Option<ClaimKind> {
        Some(match name {
            "tests_pass" => ClaimKind::TestsPass,
            "build_ok" => ClaimKind::BuildOk,
            "lint_ok" => ClaimKind::LintOk,
            "typecheck_ok" => ClaimKind::TypecheckOk,
            "behavior_unchanged" => ClaimKind::BehaviorUnchanged,
            "migration_added" => ClaimKind::MigrationAdded,
            "secrets_absent" => ClaimKind::SecretsAbsent,
            "docs_updated" => ClaimKind::DocsUpdated,
            "dependency_added" => ClaimKind::DependencyAdded,
            "file_changed" => ClaimKind::FileChanged,
            "did_x" => ClaimKind::DidX,
            "task_complete" => ClaimKind::TaskComplete,
            _ => return None,
        })
    }
}

/// §3.2 `args.scope`.
///
/// The distinction §6.3 turns on: a narrowed run supports `Named` or `Changed`
/// and **never** `All`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    All,
    Changed,
    Named,
}

/// §3.2 `args`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimArgs {
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
}

impl Default for ClaimArgs {
    fn default() -> Self {
        // `All` is the honest default for an unqualified sentence: "all tests
        // pass" with no qualifier is a claim about everything, and reading it
        // as `Named` would quietly make a narrowed run sufficient for it.
        ClaimArgs {
            scope: Scope::All,
            names: Vec::new(),
        }
    }
}

/// Where a claim's text sits, so a human can be shown the sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub event_id: String,
    /// Byte offsets into the event's message text.
    pub offset: (usize, usize),
}

/// Where a claim came from. Categorical, never a score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Extractor {
    Rules,
    /// §5.3's optional local model. Advisory only: §9 policy filters on
    /// `Rules`, so a model-added claim can appear in Explain and can never fail
    /// a gate.
    LocalModel(String),
}

impl Extractor {
    pub fn as_str(&self) -> String {
        match self {
            Extractor::Rules => "rules".to_string(),
            Extractor::LocalModel(name) => format!("local-model:{name}"),
        }
    }

    /// The one question §9 policy may ask about provenance.
    ///
    /// It reads `extractor` in order to **restrict** the model's reach, never
    /// to extend it — which is why it is the single permitted entry in the
    /// call-site registry C.8 describes.
    pub fn is_rules(&self) -> bool {
        matches!(self, Extractor::Rules)
    }
}

/// §3.2, minus `confidence`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub session_id: String,
    pub kind: ClaimKind,
    /// The exact sentence, for display. **Never read by a verifier.**
    pub text: String,
    pub span: Span,
    pub args: ClaimArgs,
    pub extractor: Extractor,
}

impl Claim {
    /// The projection §6 verifiers receive.
    ///
    /// Infallible and lossless for the verdict path. There is no inverse: a
    /// verifier holding one of these cannot get back to the prose.
    pub fn verifiable(&self) -> VerifiableClaim {
        VerifiableClaim {
            id: self.id.clone(),
            kind: self.kind,
            args: self.args.clone(),
        }
    }
}

/// What a verifier is allowed to know about a claim.
///
/// **No `text`, no `span`, no `extractor`, no score.** §0.1's boundary made
/// unrepresentable rather than documented: a verifier signature taking
/// `&Claim` is the bug this type exists to make impossible to write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiableClaim {
    pub id: String,
    pub kind: ClaimKind,
    pub args: ClaimArgs,
}

/// §3.5 — a forward-looking statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Promise {
    pub id: String,
    pub session_id: String,
    pub text: String,
    pub kind: PromiseKind,
    pub status: PromiseStatus,
    pub opened_event_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromiseKind {
    Todo,
    FollowUp,
    Deferred,
    Assumption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromiseStatus {
    Open,
    Fulfilled,
    Dropped,
}

/// A deterministic id with the §3.2 prefix.
pub fn derive_id(prefix: &str, material: &str) -> String {
    let derived = thruline_core::spec::event::EventId::derive(material.as_bytes());
    format!("{prefix}{}", derived.as_str().trim_start_matches("evt_"))
}
