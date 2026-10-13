//! What `thruline verify-receipt --format json` said, parsed — **never
//! re-derived**.
//!
//! # Why the App shells out to the core instead of linking it
//!
//! §9's App "verifies signatures". There are two ways to do that: link
//! `thruline-attest` and call it, or run the same binary a developer runs and
//! read its answer. This does the second, on purpose.
//!
//! A hosted service that links the verifier can drift from the verifier
//! developers actually install — a version skew, a feature flag, a patched
//! branch — and the drift shows up as *the App and the CLI disagreeing about
//! the same receipt*, which is the single worst failure this product can have.
//! Running `thruline verify-receipt --format json` makes that impossible by
//! construction: there is one verifier, and the App is one of its callers.
//!
//! # This module refuses; it does not default
//!
//! Every parse failure here is an error, including a `conclusion` word this
//! build does not recognise. The tempting alternative — treat an unparseable
//! report as neutral and carry on — is a gate that passes exactly when it has
//! broken. A conclusion that could not be read is not a conclusion.

use thruline_policy::Conclusion;

/// The argv the App runs. **One place**, so the flags the App verifies with
/// are the flags a reader can see.
///
/// `--offline` is not a hardening option here, it is a statement of fact: the
/// App has no Rekor credentials and must not acquire a network dependency in
/// the middle of a verification. `--format json` is the stated-answer form —
/// §6's non-collapse rule lives in `conclusion`, and this crate reads that
/// field and no other.
pub fn argv(receipt: &str, repo_dir: &str, absent_ledger: &str, key: Option<&str>) -> Vec<String> {
    let mut v = vec![
        "verify-receipt".to_string(),
        receipt.to_string(),
        "--repo".to_string(),
        repo_dir.to_string(),
        // **`--exec-db` at a path that does not exist, and this is not
        // hygiene.** Measured: with it omitted, the verifier opens whatever
        // ledger the *host* has. A receipt from someone else's machine names a
        // session that host has never seen, and `chain_head` comes back
        //
        //     fail — "this machine has a ledger and no session <id> in it"
        //
        // which is `Verified::No`, which is `conclusion: failure`, which is a
        // check run saying a genuine receipt was *contradicted*. On a pull
        // request. Because the server happened to have ThruLine installed.
        //
        // A hosted gate has no ledger and must say so: pointed at an absent
        // path, the same check is `skipped` — "no ledger on this machine, so
        // the chain head was not checked — this is NOT a pass" — the integrity
        // axis is `partial`, and the conclusion is `neutral`. Unproved, which
        // is true, instead of contradicted, which is a false accusation.
        "--exec-db".to_string(),
        absent_ledger.to_string(),
        "--offline".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    if let Some(k) = key {
        v.push("--key".to_string());
        v.push(k.to_string());
    }
    v
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("the verifier's output is not JSON: {0}")]
    NotJson(String),
    #[error("`{0}` is missing from the verifier's output")]
    Missing(&'static str),
    #[error(
        "the verifier said `conclusion: {0}`, which this build does not recognise. \
         A conclusion that cannot be read is not a conclusion, so it is refused \
         rather than treated as neutral"
    )]
    UnknownConclusion(String),
    #[error(
        "the verifier's own output contradicts itself: conclusion `success` beside \
         {0} unproved claim(s). Neither half is publishable"
    )]
    SelfContradictory(u64),
}

/// The claim axis, as the verifier stated it. Buckets partition `total`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Claims {
    pub total: u64,
    pub supported: u64,
    pub unsupported: u64,
    pub unverifiable: u64,
    pub other: u64,
    /// **Stated, not summed.** Nothing here counts to find out.
    pub all_supported: bool,
    pub unsupported_claim_ids: Vec<String>,
    pub unverifiable_claim_ids: Vec<String>,
}

impl Claims {
    /// Did any claim go unproved? The question the ruling turns on.
    pub fn anything_unproved(&self) -> bool {
        self.unsupported > 0 || self.unverifiable > 0 || self.other > 0
    }
}

/// One check line from the receipt's integrity report, for the comment table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    /// `pass` | `fail` | `skipped`, exactly as the verifier spelled it.
    pub outcome: String,
    pub detail: String,
}

/// `thruline verify-receipt --format json`, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyJson {
    /// **The one field the conclusion comes from.** Not `exit`, not `checks`.
    pub conclusion: Conclusion,
    /// The integrity axis alone: `verified` | `partial` | `failed`.
    pub integrity: String,
    pub claims: Claims,
    pub checks: Vec<Check>,
}

fn conclusion(word: &str) -> Result<Conclusion, ParseError> {
    match word {
        "success" => Ok(Conclusion::Success),
        "neutral" => Ok(Conclusion::Neutral),
        "failure" => Ok(Conclusion::Failure),
        other => Err(ParseError::UnknownConclusion(other.to_string())),
    }
}

/// Parses the verifier's stdout.
///
/// The **self-contradiction check** at the end is the App-side half of the rule
/// this whole service exists to keep: a `success` conclusion beside an unproved
/// claim is refused here even though the core is what produced it. Two guards
/// on one rule is not redundancy — the core's guard protects the CLI, and this
/// one protects the pull request, and they can be broken independently.
pub fn parse(stdout: &[u8]) -> Result<VerifyJson, ParseError> {
    let v: serde_json::Value =
        serde_json::from_slice(stdout).map_err(|e| ParseError::NotJson(e.to_string()))?;
    let word = v
        .get("conclusion")
        .and_then(|x| x.as_str())
        .ok_or(ParseError::Missing("conclusion"))?;
    let conclusion = conclusion(word)?;
    let integrity = v
        .pointer("/integrity/status")
        .and_then(|x| x.as_str())
        .ok_or(ParseError::Missing("integrity.status"))?
        .to_string();

    let c = v.get("claims").ok_or(ParseError::Missing("claims"))?;
    let n = |k: &'static str| {
        c.get(k)
            .and_then(|x| x.as_u64())
            .ok_or(ParseError::Missing(k))
    };
    let ids = |k: &str| -> Vec<String> {
        c.get(k)
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let claims = Claims {
        total: n("total")?,
        supported: n("supported")?,
        unsupported: n("unsupported")?,
        unverifiable: n("unverifiable")?,
        other: n("other")?,
        all_supported: c
            .get("all_supported")
            .and_then(|x| x.as_bool())
            .ok_or(ParseError::Missing("claims.all_supported"))?,
        unsupported_claim_ids: ids("unsupported_claim_ids"),
        unverifiable_claim_ids: ids("unverifiable_claim_ids"),
    };

    let checks = v
        .get("checks")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let name = c.get("name")?.as_str()?.to_string();
                    // `outcome` is serialized as either a bare word or a
                    // single-key object carrying the reason; both shapes are
                    // read, and an unrecognised one becomes an empty outcome
                    // rather than a pass.
                    let (outcome, detail) = match c.get("outcome") {
                        Some(serde_json::Value::String(s)) => (s.to_lowercase(), String::new()),
                        Some(serde_json::Value::Object(m)) => {
                            let (k, val) = m.iter().next()?;
                            let d = val
                                .get("detail")
                                .or_else(|| val.get("reason"))
                                .and_then(|x| x.as_str())
                                .unwrap_or_default()
                                .to_string();
                            (k.to_lowercase(), d)
                        }
                        _ => (String::new(), String::new()),
                    };
                    Some(Check {
                        name,
                        outcome,
                        detail,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // **`anything_unproved`, not `!all_supported`.** `all_supported` is false
    // for a receipt with *no claims at all*, which is a legitimate `success`:
    // nothing was claimed, so nothing went unproved. Keying the contradiction
    // check on the wrong field would have refused every claimless receipt and
    // been read as the guard working.
    if conclusion == Conclusion::Success && claims.anything_unproved() {
        return Err(ParseError::SelfContradictory(
            claims.unsupported + claims.unverifiable + claims.other,
        ));
    }
    Ok(VerifyJson {
        conclusion,
        integrity,
        claims,
        checks,
    })
}
