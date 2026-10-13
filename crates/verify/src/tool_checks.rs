//! BUILD-SPEC §6.4 — `build_ok`, `lint_ok`, `typecheck_ok`.
//!
//! > *Same binding rules; recognized commands per ecosystem; exit code 0 on
//! > final tree; warn on `--no-verify`-style bypasses or narrowed paths.*
//!
//! # These are simpler than `tests_pass`, and the simplicity is the trap
//!
//! A build check's whole evidence is its **exit code**. There are no counts to
//! disagree with it, which makes the two failure modes cheap to reach: treating
//! "no build command ran" as "the build is fine", and treating a command that
//! bypassed its own check as one that ran it. Both are refused explicitly
//! below, and both have a fixture.

use crate::binding::{bind, CheckView, SessionView};
use crate::verdict::{Finding, Severity, Verdict, VerdictRecord};

/// Paths that narrow a check to part of the tree.
///
/// `eslint src/one.js` checks one file and exits 0 exactly as a full run does.
fn narrowed_to_paths(argv: &[String]) -> Option<String> {
    let paths: Vec<&String> = argv
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with('-') && (a.contains('/') || a.contains('.')))
        .filter(|a| !a.ends_with(".json") && !a.ends_with(".toml") && !a.ends_with(".cfg"))
        .collect();
    (!paths.is_empty()).then(|| paths[0].clone())
}

/// Verify a §6.4 claim for one category.
pub fn verify(
    claim_id: &str,
    category: &str,
    session: &SessionView,
    scope_is_all: bool,
) -> VerdictRecord {
    let verifier = format!("{category}_ok@1");
    if session.capture_degraded {
        return VerdictRecord::new(claim_id, &verifier, Verdict::Unverifiable).reason(format!(
            "capture is degraded for this session ({}), so no claim in it can be supported",
            session
                .degraded_reason
                .clone()
                .unwrap_or_else(|| "reason not recorded".to_string())
        ));
    }

    let matching: Vec<&CheckView> = session
        .checks
        .iter()
        .filter(|c| c.category == category)
        .collect();
    if matching.is_empty() {
        // **Not `unsupported`.** The claim may be perfectly true; nothing ran
        // that could show it either way, and §6 forbids collapsing that into
        // either direction.
        return VerdictRecord::new(claim_id, &verifier, Verdict::Unverifiable).reason(format!(
            "no {category} command was captured in this session, so there is no \
             evidence either way"
        ));
    }

    // A failing run is evidence against, and outranks any passing one.
    if let Some(failed) = matching.iter().find(|c| c.exit_code != 0) {
        return VerdictRecord::new(claim_id, &verifier, Verdict::Unsupported)
            .reason(format!(
                "a captured {category} command failed: `{}` exited {}",
                failed.command(),
                failed.exit_code
            ))
            .with_finding(Finding::new(
                Severity::Fail,
                &format!("{category}_failed"),
                format!("`{}` exited {}", failed.command(), failed.exit_code),
            ));
    }

    // Prefer one bound to the final tree.
    let mut chosen = matching[0];
    let mut binding = bind(session, chosen);
    for c in &matching {
        let b = bind(session, c);
        if matches!(b, crate::binding::Binding::RanOnFinalTree)
            && !matches!(binding, crate::binding::Binding::RanOnFinalTree)
        {
            chosen = c;
            binding = b;
        }
    }

    let mut record = VerdictRecord::new(claim_id, &verifier, Verdict::Supported);
    record.evidence.push(chosen.id.clone());
    record.reasons.push(binding.describe());
    record
        .reasons
        .push(format!("`{}` exited 0", chosen.command()));
    record.binding = Some(binding.clone());

    if !binding.counts() {
        record.verdict = match binding {
            crate::binding::Binding::SupersededByEdit { .. }
            | crate::binding::Binding::SupersededByUnobservedChange { .. } => {
                record = record.with_finding(Finding::new(
                    Severity::Fail,
                    "ran_before_final_edit",
                    binding.describe(),
                ));
                Verdict::Unsupported
            }
            _ => Verdict::Unverifiable,
        };
        return record;
    }

    // §6.4: warn on a bypass or a narrowed path — and refuse when the claim
    // was about everything.
    if let Some(flag) = &chosen.bypass {
        record = record.with_finding(Finding::new(
            Severity::Fail,
            "check_bypassed",
            format!(
                "`{}` carries `{flag}`, which suppresses the very check the claim is \
                 about — exiting 0 says nothing",
                chosen.command()
            ),
        ));
    }
    // Same rule as §6.3's narrowing detector: a command the record does not
    // hold is a detector that could not look, not a detector that looked and
    // found nothing.
    match chosen.argv.as_deref() {
        Some(argv) if !argv.is_empty() => {
            if let Some(path) = narrowed_to_paths(argv) {
                record = record.with_finding(Finding::new(
                    if scope_is_all {
                        Severity::Fail
                    } else {
                        Severity::Warn
                    },
                    "narrowed_paths",
                    format!(
                        "`{}` was limited to {path}, so it exercised part of the tree",
                        chosen.command()
                    ),
                ));
            }
        }
        _ => {
            record = record.with_finding(Finding::new(
                Severity::Warn,
                "narrowing_unchecked",
                format!(
                    "the command behind this {category} check is not in the record, so \
                     §6.4's narrowed-path detector could not look at it — an absence of \
                     `narrowed_paths` here is not evidence"
                ),
            ));
            if scope_is_all {
                record.verdict = record.verdict.worst(crate::verdict::Verdict::Unverifiable);
            }
        }
    }
    record
}
