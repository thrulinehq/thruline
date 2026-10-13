//! Sigstore keyless signing and offline bundle verification — BUILD-SPEC §8.1.
//!
//! This is the crate `~/thruline/crates/attest` deliberately did not build. Its
//! `lib.rs` says so in as many words: *"Not implemented: Sigstore keyless."*
//! That was the right call at the time — a stub in a verifier is a code path a
//! reader believes in — and this crate is the other half of that decision.
//!
//! # The core is offline, and that is a property of the dependency tree
//!
//! `crates/attest/tests/no_network.rs:77` forbids `reqwest ureq hyper curl
//! rustls native-tls tokio tough` (among others) in `crates/attest`'s manifest.
//! `attest` will depend on this crate, so the default feature set resolves
//! **none** of them: 108 transitive crates, zero transports.
//! `tests/no_network.rs` here proves it with `cargo tree` rather than trusting
//! a manifest substring scan, because a substring scan cannot see a transitive
//! `tokio`.
//!
//! Fulcio, Rekor and TUF refresh live behind `net`; the OAuth device flow lives
//! behind `oidc`. A verifier that *can* open a socket is a verifier whose
//! offline guarantee rests on it choosing not to.
//!
//! # What is verified, and why each one is load-bearing
//!
//! 1. **DSSE signature** over the PAE bytes, by the key the certificate binds.
//! 2. **Certificate chain** to a CA in the trust root — including the
//!    intermediate's signature by the root, which `sigstore` 0.14.0 skips by
//!    passing an empty intermediates list and treating the intermediate as an
//!    anchor.
//! 3. **SCT**, so the certificate is known to certificate transparency.
//! 4. **Rekor inclusion proof and checkpoint**, so the entry is *in the log*
//!    rather than merely *shaped like* a log entry. `sigstore` 0.14.0 leaves
//!    both as `TODO` in the bundle path (`verifier.rs:195-202`); this is the
//!    gap that decided build-over-adopt. See `docs/DECISION.md`.
//! 5. **Identity and issuer** against a policy, from the modern DER-encoded
//!    Fulcio extensions (`1.3.6.1.4.1.57264.1.8`+) as well as the legacy ones.
//!
//! # The ten-minute certificate
//!
//! A Fulcio certificate is valid for ten minutes and is therefore **always
//! expired** by the time anyone verifies it. Checking its validity against
//! wall-clock `now` rejects every honest bundle. Skipping the window accepts a
//! certificate used long after issuance. The rule is *"was it valid at the
//! moment the log says it signed"*, and the log entry supplies that moment —
//! which is why the inclusion proof is not optional: it is what makes the
//! timestamp trustworthy enough to date the certificate by.

#![forbid(unsafe_code)]

pub mod bundle;
pub mod checkpoint;
pub mod dsse;
pub mod error;
pub mod identity;
pub mod key;
pub mod merkle;
pub mod policy;
pub mod sct;
pub mod sign;
pub mod trustroot;
pub mod verify;
pub mod x509;

#[cfg(feature = "net")]
pub mod net;

pub use bundle::Bundle;
pub use error::{Axis, Error, Result};
pub use identity::Identity;
pub use policy::{IdentityPolicy, Policy};
pub use sign::{sign, OidcProvider};
pub use trustroot::TrustRoot;
pub use verify::{public_key_id, verify, verify_with_public_key};

/// The BUILD-SPEC section that governs this crate, matching
/// `thruline_attest::SPEC_SECTION`.
pub const SPEC_SECTION: &str = "§8.1";

/// Bundle media types this build accepts.
///
/// **There are two spellings of v0.3 and both are current.** The dotted form is
/// what sigstore-go and cosign emit today; the parameterised form is the legacy
/// spelling and still appears in real bundles. A verifier that knows only one
/// rejects honest artifacts from the other half of the ecosystem.
pub const BUNDLE_V03: &str = "application/vnd.dev.sigstore.bundle.v0.3+json";
/// The legacy spelling of v0.3. See [`BUNDLE_V03`].
pub const BUNDLE_V03_LEGACY: &str = "application/vnd.dev.sigstore.bundle+json;version=0.3";

/// DSSE payload type for an in-toto statement — the same constant
/// `thruline_attest::statement::PAYLOAD_TYPE` uses, repeated here so this crate
/// has no dependency on `attest` and the two can be merged in either order.
pub const IN_TOTO_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

/// ThruLine's predicate type, mirroring
/// `thruline_attest::statement::PREDICATE_TYPE`.
///
/// # This is a mirror, and mirrors drift
///
/// The domain **moved** on 2026-09-05 (core `290845c`, "Predicate domain ruled:
/// thrulinehq.com"), and `docs/STATUS.md` files it under *"Changes that would
/// break a pinned fixture"*: the string is in every receipt ever issued and
/// verifiers key on it, so a receipt carrying the old domain fails against the
/// current binary.
///
/// This crate carried the old value in two example payloads for exactly as long
/// as nothing compared them to anything. It is a constant now, in one place,
/// and `tests/core_contract.rs` asserts it still equals the core's — so the next
/// move is a failing test here rather than a silently wrong receipt.
pub const PREDICATE_TYPE: &str = "https://thrulinehq.com/attestation/execution-check/v0.1";

/// in-toto Statement type, mirroring `thruline_attest::statement::STATEMENT_TYPE`.
pub const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";

#[cfg(test)]
mod tests {
    #[test]
    fn the_two_v03_media_types_are_distinct_and_both_named() {
        assert_ne!(super::BUNDLE_V03, super::BUNDLE_V03_LEGACY);
        assert!(super::BUNDLE_V03.contains("v0.3"));
        assert!(super::BUNDLE_V03_LEGACY.contains("version=0.3"));
    }
}
