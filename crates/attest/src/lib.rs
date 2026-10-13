//! in-toto/DSSE receipts and commit trailers (BUILD-SPEC §8).
//!
//! Implemented: the in-toto Statement and ThruLine's predicate
//! ([`statement`]), DSSE envelopes and the PAE encoding they sign over
//! ([`dsse`]), local-key signing and key storage ([`key`]), and the
//! `ThruLine-Receipt:` commit trailer ([`trailer`]).
//!
//! **Sigstore keyless is implemented, and here is what that does and does not
//! mean** (Track B, DECISIONS #130). This paragraph replaced one reading *"Not
//! implemented: Sigstore keyless"*, and B's condition for replacing it was that
//! the new one must not be another unqualified claim — *"swapping one
//! unqualified claim for another would learn nothing from the failure guard 4
//! records in its own docstring."*
//!
//! Implemented, and each verified rather than asserted: the DSSE signature by
//! the certified key, the certificate chain to a Sigstore root, the embedded
//! SCT against the CT log, the Rekor inclusion proof, and the signer identity.
//! [`verify::verify`] reports those as **five separate checks**, because a
//! stale trust root and a tampered artifact must not look alike.
//!
//! **Still not built, and named rather than left to be discovered:** production
//! Rekor *upload* is gated off by default; TUF refresh is a separate explicit
//! act, and the trust root is handled as pinned **data** rather than by a live
//! TUF client — the convention cosign and sigstore-go have both moved to; and
//! RFC 3161 timestamp authorities are unhandled.
//!
//! **The local-key path is untouched.** Every receipt already issued under an
//! Ed25519 key keeps verifying. Keyless is additive, because an overwritten
//! guarantee is worse than a missing one.
//!
//! # Everything here is offline
//!
//! §0.5: receipts are verifiable offline. Nothing in this crate opens a socket,
//! and `thruline verify` reaches a network only if Rekor is asked for
//! explicitly, which it cannot be yet.

pub mod dsse;
pub mod key;
pub mod report;
pub mod session_v2;
pub mod statement;
pub mod trailer;
pub mod verify;

pub use dsse::{key_id, pae, sign, DsseError, Envelope, Signature};
pub use report::{badge, badge_route, check_run, CHECK_NAME};
pub use statement::{
    Predicate, Statement, StatementError, PAYLOAD_TYPE, PREDICATE_TYPE, SCHEMA_ID,
};
pub use verify::{LocalEvidence, Outcome, Report, Verified};

/// The BUILD-SPEC section that governs this crate.
pub const SPEC_SECTION: &str = "§8";

#[cfg(test)]
mod tests {
    #[test]
    fn spec_section_is_recorded() {
        assert!(super::SPEC_SECTION.starts_with('§'));
    }
}
