//! Plain explanations for exact receipts that the frozen v0.2 producers lack.
//!
//! These words are attached only to an unavailable result with the matching
//! structured missing fact. A complete checker finding, including a narrow
//! positive `broken` witness, keeps its own reason and verdict.

use super::super::params::CheckParams;

pub(super) struct ReceiptLimit {
    pub reason: &'static str,
    pub checkable_if: &'static str,
}

pub(super) fn for_incomplete(
    params: &CheckParams,
    missing: &[&str],
    current_reason: Option<&str>,
) -> Option<ReceiptLimit> {
    let (code, generic_reason, reason, checkable_if) = match params {
        CheckParams::TestsBeforeCode(_) => (
            "event_specific_change_and_run_order",
            "process census is not complete for this exact version",
            "The record cannot link this Change's test edits and runs to code edits in a verified order.",
            "v0.3: record exact-version Change links and ordered test-edit, code-edit, and run-result receipts; then check this version again.",
        ),
        CheckParams::ConsentRecorded(_) => (
            "typed_person_consent",
            "process census is not complete for this exact version",
            "The record has no typed person consent answer linked before this action.",
            "v0.3: record the person's typed answer, action, and verified order in the same session; then check this version again.",
        ),
        CheckParams::OneHeavyJob(_) => (
            "command_end_time",
            "process census is not complete for this exact version",
            "The record has no complete start and end times for matching heavy jobs across sessions.",
            "v0.3: record complete cross-session command start and end intervals for the accepted window; then check this version again.",
        ),
        CheckParams::RuleFilePresent(_) => (
            "person_edit_provenance",
            "rule-file edit census is incomplete or not on this version",
            "The protected rule files exist, but the record cannot prove who edited them on this version.",
            "v0.3: record exact-version rule-file edit results and verified person-edit provenance; then check this version again.",
        ),
        CheckParams::DecisionSatisfied(_) => (
            "claim_exact_version_tree_binding",
            "claim census is incomplete or not bound to an exact version",
            "The claim has no verified link to an accepted decision verdict on its exact version.",
            "v0.3: bind the source claim, accepted decision revision, and verdict to one exact version and tree; then check again.",
        ),
        CheckParams::StaleClaim(_) => (
            "complete_closed_session_and_relevant_edit_census",
            "claim census is incomplete or not bound to an exact version",
            "The record cannot order this claim, later relevant edits, and a rerun on one exact version.",
            "v0.3: record the complete closed-session claim, relevant edit, and rerun order with exact versions; then check again.",
        ),
        CheckParams::SummedTotals(_) => (
            "claim_to_run_same_session_binding",
            "claim census is incomplete or not bound to an exact version",
            "The claimed total has no verified single-run tally on this exact version.",
            "v0.3: bind the typed claim total to one complete run result in its source session and exact version; then check again.",
        ),
        CheckParams::EmptyRun(_) => (
            "selected_run_lacks_a_complete_exact_tree_wrapper_receipt",
            "claim census is incomplete or not bound to an exact version",
            "The selected run has no complete verified result and examined-input receipt on this exact version.",
            "v0.3: record the selected run's result, examined inputs, and exact tree with a source claim link; then check again.",
        ),
        CheckParams::DelegatedRun(_) => (
            "claim_to_run_same_session_binding",
            "claim census is incomplete or not bound to an exact version",
            "The record cannot link the claimed pass to a run's actual session or CI origin.",
            "v0.3: record the claim-to-run link and verified run origin, preserving its actual session or CI identity; then check again.",
        ),
        CheckParams::EditDuringRun(_) => (
            "J6_requires_a_run_and_edit_adapter_not_a_synthetic_claim",
            "claim census is incomplete or not bound to an exact version",
            "The record cannot order the selected edit inside a verified run interval.",
            "v0.3: record a complete run interval and exact-version edit timestamps with their source links; then check again.",
        ),
        _ => return None,
    };
    (missing.contains(&code) && current_reason == Some(generic_reason)).then_some(ReceiptLimit {
        reason,
        checkable_if,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::checks_preservation::RuleFilePresentParams;

    #[test]
    fn b5_named_file_gap_does_not_replace_a_specific_scope_failure() {
        let params = CheckParams::RuleFilePresent(RuleFilePresentParams {
            paths: vec!["**".into()],
            person_edit_basis: "same_version".into(),
        });
        assert!(for_incomplete(
            &params,
            &["person_edit_provenance"],
            Some("rule-file presence needs named files")
        )
        .is_none());
    }
}
