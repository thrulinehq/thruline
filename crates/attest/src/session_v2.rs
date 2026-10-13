//! Version-specific session provenance and D22 presentation. V0.1 is unchanged.
//! The signed envelope is verified before rendering; the compatibility view is
//! only input to the existing deterministic evidence verifier, never re-signed.
use crate::report::PolicyConclusion;
use crate::statement::{
    Capture, CheckEntry, ClaimEntry, Predicate, Promises, Session, Statement, Subject,
    VerdictEntry, VerifierInfo,
};
use crate::verify::{Report, Verified};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const PREDICATE_TYPE: &str = "https://thrulinehq.com/attestation/execution-check/v0.2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Extractor {
    Rules,
    LocalModel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimOrigin {
    pub claim_id: String,
    pub extractor: Extractor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ground {
    AllClaimsProven,
    RulesUnsupported,
    RulesUnverifiable,
    ModelAdvisory,
    CaptureDegraded,
    NoClaims,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStatement {
    #[serde(rename = "_type")]
    type_: String,
    subject: Vec<Subject>,
    #[serde(rename = "predicateType")]
    predicate_type: String,
    pub predicate: SessionPredicate,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPredicate {
    version: u32,
    kind: String,
    session: Session,
    claims: Vec<ClaimEntry>,
    checks: Vec<CheckEntry>,
    verdicts: Vec<VerdictEntry>,
    promises: Promises,
    verifier: VerifierInfo,
    capture: Capture,
    pub claim_origins: Vec<ClaimOrigin>,
    pub ground: Ground,
    pub evidence_trust: String,
}

pub struct Decoded {
    pub statement: Statement,
    pub provenance: Option<SessionStatement>,
}

fn ground(statement: &Statement, origins: &[ClaimOrigin]) -> Ground {
    if statement.predicate.capture.capture_degraded {
        return Ground::CaptureDegraded;
    }
    if statement.predicate.claims.is_empty() {
        return Ground::NoClaims;
    }
    if statement
        .predicate
        .verdicts
        .iter()
        .all(|v| v.verdict == "supported")
    {
        return Ground::AllClaimsProven;
    }
    let rules = |id: &str| {
        origins
            .iter()
            .any(|o| o.claim_id == id && o.extractor == Extractor::Rules)
    };
    if statement
        .predicate
        .verdicts
        .iter()
        .any(|v| v.verdict == "unsupported" && rules(&v.claim_id))
    {
        return Ground::RulesUnsupported;
    }
    if statement
        .predicate
        .verdicts
        .iter()
        .any(|v| v.verdict != "supported" && rules(&v.claim_id))
    {
        return Ground::RulesUnverifiable;
    }
    Ground::ModelAdvisory
}

impl SessionStatement {
    pub fn compatibility_view(&self) -> Statement {
        let p = &self.predicate;
        Statement::new(
            self.subject.clone(),
            Predicate {
                session: p.session.clone(),
                claims: p.claims.clone(),
                checks: p.checks.clone(),
                verdicts: p.verdicts.clone(),
                promises: p.promises.clone(),
                verifier: p.verifier.clone(),
                capture: p.capture.clone(),
            },
        )
    }

    pub fn new(statement: Statement, origins: Vec<ClaimOrigin>) -> Result<Self, String> {
        let reason = ground(&statement, &origins);
        let p = statement.predicate;
        let result = Self {
            type_: statement.type_,
            subject: statement.subject,
            predicate_type: PREDICATE_TYPE.into(),
            predicate: SessionPredicate {
                version: 2,
                kind: "session_claims".into(),
                session: p.session,
                claims: p.claims,
                checks: p.checks,
                verdicts: p.verdicts,
                promises: p.promises,
                verifier: p.verifier,
                capture: p.capture,
                claim_origins: origins,
                ground: reason,
                evidence_trust: "local".into(),
            },
        };
        result.validate()?;
        Ok(result)
    }

    fn validate(&self) -> Result<(), String> {
        if self.type_ != crate::statement::STATEMENT_TYPE
            || self.predicate_type != PREDICATE_TYPE
            || self.predicate.version != 2
            || self.predicate.kind != "session_claims"
            || self.predicate.evidence_trust != "local"
        {
            return Err("unsupported v0.2 session type, version, kind or custody".into());
        }
        let legacy = self.compatibility_view();
        legacy.validate().map_err(|e| e.to_string())?;
        let claim_ids: BTreeSet<_> = self.predicate.claims.iter().map(|c| &c.id).collect();
        let check_ids: BTreeSet<_> = self.predicate.checks.iter().map(|c| &c.id).collect();
        if claim_ids.len() != self.predicate.claims.len()
            || check_ids.len() != self.predicate.checks.len()
        {
            return Err("duplicate claim or check identity".into());
        }
        let mut verdict_ids = BTreeSet::new();
        for verdict in &self.predicate.verdicts {
            if !verdict_ids.insert(&verdict.claim_id) || !claim_ids.contains(&verdict.claim_id) {
                return Err("duplicate or dangling verdict reference".into());
            }
            // Evidence may include event references in legacy sessions; every
            // supported machine claim must at least name a declared check.
            if verdict.verdict == "supported"
                && !verdict.evidence.iter().any(|id| check_ids.contains(id))
            {
                return Err("supported claim has no declared producing check".into());
            }
        }
        if verdict_ids != claim_ids {
            return Err("every claim needs exactly one verdict".into());
        }
        let mut ids = BTreeSet::new();
        for origin in &self.predicate.claim_origins {
            if !ids.insert(&origin.claim_id)
                || !self
                    .predicate
                    .claims
                    .iter()
                    .any(|c| c.id == origin.claim_id)
            {
                return Err("duplicate or dangling claim origin".into());
            }
        }
        if ids.len() != self.predicate.claims.len() {
            return Err("every claim needs categorical extractor provenance".into());
        }
        if self.predicate.ground != ground(&legacy, &self.predicate.claim_origins) {
            return Err("ground contradicts the signed claim origin or capture state".into());
        }
        Ok(())
    }
}

pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    if bytes.len() > 1024 * 1024 {
        return Err("session receipt exceeds 1 MiB".into());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    match value["predicateType"].as_str() {
        Some(crate::statement::PREDICATE_TYPE) => {
            let statement: Statement = serde_json::from_value(value).map_err(|e| e.to_string())?;
            statement.validate().map_err(|e| e.to_string())?;
            Ok(Decoded {
                statement,
                provenance: None,
            })
        }
        Some(PREDICATE_TYPE) => {
            let provenance: SessionStatement =
                serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
            // Nested v0.1 types have permissive readers. V0.2 accepts exactly
            // their emitted representation; unknown/missing required fields
            // cannot disappear through that compatibility layer.
            if serde_json::to_value(&provenance).map_err(|e| e.to_string())? != value {
                return Err("unknown or missing v0.2 session semantics".into());
            }
            provenance.validate()?;
            Ok(Decoded {
                statement: provenance.compatibility_view(),
                provenance: Some(provenance),
            })
        }
        _ => Err("unsupported predicateType; use a reader for that explicit version".into()),
    }
}

/// One computed provider conclusion and cause title. No renderer can upgrade it.
/// Model extraction remains advisory even under a supplied policy failure.
pub fn presentation(
    report: &Report,
    session: &SessionStatement,
    policy: Option<PolicyConclusion>,
) -> Value {
    let has_rules = session
        .predicate
        .claim_origins
        .iter()
        .any(|o| o.extractor == Extractor::Rules);
    let (conclusion, title, ground) = match report.verified() {
        Verified::No => (
            "failure",
            "Receipt integrity could not be verified",
            "integrity_mismatch",
        ),
        Verified::Partial => (
            "neutral",
            "Fog: evidence is incomplete on this machine",
            "integrity_incomplete",
        ),
        Verified::Yes if session.predicate.capture.capture_degraded => {
            ("neutral", "Fog: capture is degraded", "capture_degraded")
        }
        Verified::Yes if policy == Some(PolicyConclusion::Failure) && has_rules => (
            "failure",
            "Repository policy requires evidence that is not supported",
            "policy_failure",
        ),
        Verified::Yes if policy == Some(PolicyConclusion::Neutral) => (
            "neutral",
            "Repository policy left this result advisory",
            "policy_advisory",
        ),
        Verified::Yes => match session.predicate.ground {
            Ground::AllClaimsProven if policy != Some(PolicyConclusion::Failure) => (
                "success",
                "Every recorded claim is proven at the stated scope",
                "all_claims_proven",
            ),
            Ground::RulesUnsupported => (
                "action_required",
                "A rules-extracted claim is not supported by the record",
                "rules_unsupported",
            ),
            Ground::RulesUnverifiable => (
                "neutral",
                "Recorded: a rules-extracted claim was not judged",
                "rules_unverifiable",
            ),
            Ground::NoClaims => ("neutral", "No recorded claims to verify", "no_claims"),
            Ground::CaptureDegraded => ("neutral", "Fog: capture is degraded", "capture_degraded"),
            _ => (
                "neutral",
                "Model-extracted findings remain advisory",
                "model_advisory",
            ),
        },
    };
    // R8-VOCABULARY (design record §5, ruling R-2026-09-23-8): the word is
    // decided by the producing ground, not by the conclusion bucket. A
    // recording gap — the raw files are not here, or capture was degraded —
    // is Fog; every other neutral ground is a reading that exists and settles
    // nothing, which is Recorded. `conclusion`, `verified` and `ground`
    // themselves are untouched: this is the word for them, not a new one.
    let label = if conclusion == "success" {
        "Verified"
    } else if conclusion == "action_required" || conclusion == "failure" {
        "Not proven"
    } else if ground == "integrity_incomplete" || ground == "capture_degraded" {
        "Fog"
    } else {
        "Recorded"
    };
    json!({"conclusion":conclusion,"verified":conclusion == "success","ground":ground,"cause_title":title,
        "badge_label":label,"evidence_trust":"local","capture_ground":session.predicate.ground,
        "claim_origins":session.predicate.claim_origins,
        "verdict_summary":crate::report::VerdictSummary::of(Some(&session.compatibility_view()))})
}
