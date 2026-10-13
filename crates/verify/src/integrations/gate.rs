//! The actual production admission gate, kept free of allocation and OS calls
//! so its complete boolean state space can also be checked by Kani.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Imported,
    Unaccepted,
    Interrupted,
    Unidentified,
    ChangedDuringRun,
    Stale,
    IncompleteReport,
    Eligible,
}
#[derive(Clone, Copy)]
pub struct Preconditions {
    pub captured: bool,
    pub accepted: bool,
    pub completed: bool,
    pub identified: bool,
    pub stable: bool,
    pub current: bool,
    pub report_complete: bool,
}
pub fn gate(p: Preconditions) -> Gate {
    if !p.captured {
        Gate::Imported
    } else if !p.accepted {
        Gate::Unaccepted
    } else if !p.completed {
        Gate::Interrupted
    } else if !p.identified {
        Gate::Unidentified
    } else if !p.stable {
        Gate::ChangedDuringRun
    } else if !p.current {
        Gate::Stale
    } else if !p.report_complete {
        Gate::IncompleteReport
    } else {
        Gate::Eligible
    }
}
