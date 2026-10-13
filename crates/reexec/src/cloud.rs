//! §7's cloud sandbox, behind the `reexec-cloud` feature.
//!
//! # Why this refuses instead of working
//!
//! §18 lists "cloud sandbox provider for `reexec-cloud`" among the decisions
//! **the agent must not guess**, with the interface built and the provider left
//! for the founder. So this is the interface, and it is the interface with no
//! success path in it: `prepare` refuses, naming the decision that is open.
//!
//! A stub that pretended to succeed would be worse than nothing here. Re-execution
//! is how a claim earns `supported`; a backend that returned a fabricated
//! `ExecResult` would manufacture exactly the evidence this product exists to
//! demand. And an unreachable success path is a future loosening — someone
//! wires it up, the refusal is the only thing they delete, and nothing notices.
//!
//! When a provider is chosen, the work is to implement three methods against
//! its API. Nothing else in this crate changes, because no caller reaches for
//! Docker directly.

use crate::{ExecResult, PrepareRequest, Prepared, RunRequest, Sandbox, SandboxError};

/// The cloud backend, pending §18's provider decision.
///
/// Constructible — a caller asking for `--backend cloud` should get a clear
/// refusal from the backend it asked for, not a confusing error from the one it
/// did not.
#[derive(Debug, Default)]
pub struct Cloud {
    endpoint: Option<String>,
}

impl Cloud {
    pub fn new(endpoint: Option<String>) -> Cloud {
        Cloud { endpoint }
    }

    fn undecided(&self) -> SandboxError {
        SandboxError::NoRuntime {
            detail: format!(
                "the cloud re-execution provider is BUILD-SPEC §18's open decision and has \
                 not been made{}. The interface is built and no implementation is wired to \
                 it, deliberately: a backend that returned a result it did not obtain would \
                 manufacture the evidence re-execution exists to demand. Use \
                 `--backend docker`, or leave the claim `unverifiable` — which is the \
                 honest answer, not a failure",
                match &self.endpoint {
                    Some(e) => format!(" (an endpoint was configured: {e})"),
                    None => String::new(),
                }
            ),
        }
    }
}

impl Sandbox for Cloud {
    fn prepare(&mut self, _req: &PrepareRequest) -> Result<Prepared, SandboxError> {
        Err(self.undecided())
    }

    fn run(&mut self, _req: &RunRequest) -> Result<ExecResult, SandboxError> {
        Err(self.undecided())
    }

    /// The one method that succeeds, because there is nothing to destroy.
    fn destroy(&mut self) -> Result<(), SandboxError> {
        Ok(())
    }
}
