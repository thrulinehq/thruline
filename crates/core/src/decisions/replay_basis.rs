//! Replay comparison basis. Full outcomes remain stored and returned intact.
//!
//! Chrome may omit or make one implicit favicon request for a capture. Only
//! that exact empty server-owned response is diagnostic. Invalid
//! paths, statuses, and duplicate observations remain comparison inputs.
use super::checks::CheckOutcome;

pub fn stable_verdict_basis(outcome: &CheckOutcome) -> CheckOutcome {
    let mut basis = outcome.clone();
    for screen in &mut basis.visual {
        if screen.housekeeping_responses.len() <= 1
            && screen
                .housekeeping_responses
                .iter()
                .all(|response| response.path == "/favicon.ico" && response.status == 204)
        {
            screen.housekeeping_responses.clear();
        }
    }
    basis
}
