//! Every way verification can fail, named separately.
//!
//! # Why this enum is large on purpose
//!
//! `crates/attest/src/verify.rs` is built against one failure: *a `verify` that
//! returns success because it never checked*. Its answer is a three-state
//! `Outcome` where a check that could not run is **not** a check that passed.
//!
//! This enum is the same discipline one layer down. There is no `Other`, no
//! `Unknown`, and nothing that maps to `Ok`. A caller that wants to know
//! *which* guarantee failed can ask, and — more importantly — a test suite can
//! assert that a specific corruption produces a specific variant, which is how
//! "every verification failure mode is observed" becomes checkable rather than
//! claimed.
//!
//! A collapsed error type is how a tampered payload and an unreachable trust
//! root come to look alike in a log.

use std::fmt;

/// Which of the five verification axes a failure sits on.
///
/// Reported alongside the specific error so a UI can say *what kind of trust
/// broke* without matching on thirty variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// The DSSE signature over the PAE bytes.
    Signature,
    /// The certificate chain to a trusted root.
    Chain,
    /// The embedded Signed Certificate Timestamp.
    Sct,
    /// Rekor: inclusion proof, checkpoint, and the log's identity.
    TransparencyLog,
    /// Who signed, against the policy that says who may.
    Identity,
    /// The bundle itself: shape, encoding, version.
    Bundle,
}

impl fmt::Display for Axis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Axis::Signature => "signature",
            Axis::Chain => "certificate chain",
            Axis::Sct => "SCT",
            Axis::TransparencyLog => "transparency log",
            Axis::Identity => "identity",
            Axis::Bundle => "bundle",
        };
        f.write_str(s)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // --- bundle shape ------------------------------------------------------
    #[error("the bundle is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("mediaType {found:?} is not a Sigstore bundle this build verifies (expected v0.3)")]
    MediaType { found: String },

    #[error(
        "a v0.3 bundle must carry verificationMaterial.certificate (a single leaf); this one \
         carries {found} — the chain form was removed in v0.3 precisely so a verifier cannot be \
         handed its own intermediates"
    )]
    WrongVerificationMaterial { found: &'static str },

    #[error("the bundle carries neither a messageSignature nor a dsseEnvelope, so it attests to nothing")]
    NoContent,

    #[error(
        "this bundle was signed with a bare public key, not a Fulcio certificate, so it proves \
         possession of a key and nothing about who holds it. There is no identity to return — \
         verify it with `verify_with_public_key` against a key you already trust"
    )]
    NoCertificateOnlyAPublicKey,

    #[error("{field} is not valid base64: {reason}")]
    Base64 { field: &'static str, reason: String },

    #[error("{field} is not a valid RFC 3339 timestamp: {value:?}")]
    Timestamp { field: &'static str, value: String },

    // --- DSSE --------------------------------------------------------------
    #[error("the DSSE envelope carries no signatures; an empty list satisfies \"all signatures are valid\" vacuously")]
    NoSignatures,

    #[error("the DSSE signature does not verify over the PAE bytes with the certified public key")]
    SignatureInvalid,

    #[error("payloadType is {found:?}, expected {expected:?} — the signature may be valid over a different kind of document entirely")]
    PayloadType { found: String, expected: String },

    // --- certificate chain -------------------------------------------------
    #[error("the leaf certificate could not be parsed: {0}")]
    CertificateParse(String),

    #[error(
        "the signing certificate does not chain to any certificate authority in the trust root \
         — it may be genuine and from a different Sigstore instance, or it may be self-issued"
    )]
    NoTrustedRoot,

    #[error(
        "{subject} is not signed by {issuer}: the signature over the TBS bytes does not verify"
    )]
    ChainSignatureInvalid { subject: String, issuer: String },

    #[error(
        "the certificate was not valid at {at}: it was valid from {not_before} to {not_after}. \
         A Fulcio certificate lives ten minutes, so this is checked against the time the log says \
         it signed, never against now"
    )]
    CertificateExpired {
        at: String,
        not_before: String,
        not_after: String,
    },

    #[error(
        "the certificate's extended key usage does not include code signing (1.3.6.1.5.5.7.3.3)"
    )]
    NotCodeSigning,

    #[error("{which} is not a CA certificate (basicConstraints cA is false or absent), so it cannot have issued anything")]
    NotACa { which: String },

    #[error("the certificate chain is {depth} deep, beyond the maximum of {max}")]
    ChainTooDeep { depth: usize, max: usize },

    // --- SCT ---------------------------------------------------------------
    #[error(
        "the certificate carries no embedded SCT (extension 1.3.6.1.4.1.11129.2.4.2), so there is \
         no evidence it was ever submitted to a certificate transparency log"
    )]
    SctMissing,

    #[error("the embedded SCT could not be decoded: {0}")]
    SctMalformed(String),

    #[error("no certificate transparency log in the trust root has key id {log_id}")]
    SctUnknownLog { log_id: String },

    #[error("the SCT signature does not verify against the certificate transparency log's key")]
    SctInvalid,

    // --- transparency log --------------------------------------------------
    #[error(
        "the bundle carries no inclusion proof, so nothing shows the entry is in the log. A log \
         entry without one is a claim about a transparency log, not an entry in it"
    )]
    InclusionProofMissing,

    #[error(
        "the Merkle inclusion proof does not reconstruct the log's root hash: computed {computed}, \
         the checkpoint says {expected}"
    )]
    InclusionProofInvalid { computed: String, expected: String },

    #[error("the inclusion proof has {found} hashes; a tree of size {tree_size} at index {index} needs {expected}")]
    InclusionProofShape {
        found: usize,
        expected: usize,
        index: u64,
        tree_size: u64,
    },

    #[error("log index {index} is outside a tree of size {tree_size}")]
    InclusionIndexOutOfRange { index: u64, tree_size: u64 },

    #[error("the checkpoint could not be parsed as a signed note: {0}")]
    CheckpointMalformed(String),

    #[error(
        "the checkpoint's signature does not verify against the transparency log's public key"
    )]
    CheckpointSignatureInvalid,

    #[error("the checkpoint is for origin {found:?}, but the log id in the entry belongs to {expected:?}")]
    CheckpointOrigin { found: String, expected: String },

    #[error("no transparency log in the trust root has key id {log_id}")]
    UnknownLog { log_id: String },

    #[error("the canonicalized entry body does not match the bundle's own content: the log entry is for a different artifact")]
    LogEntryMismatch,

    // --- identity ----------------------------------------------------------
    #[error("the certificate's identity is {found:?}, but the policy requires {expected:?}")]
    IdentityMismatch { found: String, expected: String },

    #[error("the certificate's OIDC issuer is {found:?}, but the policy requires {expected:?}")]
    IssuerMismatch { found: String, expected: String },

    #[error(
        "the certificate carries no OIDC issuer extension (neither 1.3.6.1.4.1.57264.1.8 nor the \
         legacy 1.3.6.1.4.1.57264.1.1), so who vouched for this identity is unknown"
    )]
    NoIssuer,

    #[error("the certificate carries no subject alternative name, so it names no identity")]
    NoIdentity,

    #[error("the certificate's identity extensions could not be read: {0}")]
    IdentityMalformed(String),

    #[error(
        "the certificate's identity {value:?} arrived as {found}, but the policy {expected}. An \
         otherName's contents are arbitrary text chosen upstream, so it can be made to read \
         exactly like an email address or a URI; matching one against a policy that did not ask \
         for it is how a username impersonates a mailbox"
    )]
    IdentityKind {
        value: String,
        found: &'static str,
        expected: String,
    },

    // --- trust root --------------------------------------------------------
    #[error("the trust root could not be read from {path}: {reason}")]
    TrustRootUnreadable { path: String, reason: String },

    #[error("the trust root has mediaType {found:?}, expected {expected:?}")]
    TrustRootMediaType { found: String, expected: String },

    #[error("the trust root contains no {kind} valid at {at} — key rotation windows are checked, not ignored")]
    NoValidKey { kind: &'static str, at: String },

    // --- keys --------------------------------------------------------------
    #[error("{path}: {reason}")]
    Key { path: String, reason: String },

    #[error("the private key file is encrypted and no passphrase was supplied")]
    PassphraseRequired,

    #[error("the passphrase does not decrypt this key")]
    PassphraseWrong,

    #[error("unsupported scrypt parameters N={n} r={r} p={p}; cosign writes N of 32768, 65536 or 131072")]
    ScryptParams { n: u32, r: u32, p: u32 },

    // --- signing / transport ----------------------------------------------
    #[error(
        "{operation} needs the {feature:?} feature, which this build does not have. The core is \
         offline by construction so that `crates/attest` can depend on it without gaining a socket"
    )]
    FeatureDisabled {
        operation: &'static str,
        feature: &'static str,
    },

    #[error("{context}: {reason}")]
    Transport { context: String, reason: String },

    #[error("Fulcio refused the certificate request: {status} {body}")]
    FulcioRejected { status: u16, body: String },

    #[error("Rekor refused the entry: {status} {body}")]
    RekorRejected { status: u16, body: String },

    #[error("the OIDC flow did not complete: {0}")]
    Oidc(String),

    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    /// Which trust axis this failure sits on.
    ///
    /// Exhaustive by construction: adding a variant without classifying it
    /// fails to compile, so a new failure mode cannot arrive unclassified.
    pub fn axis(&self) -> Axis {
        match self {
            Error::Json(_)
            | Error::MediaType { .. }
            | Error::WrongVerificationMaterial { .. }
            | Error::NoContent
            | Error::NoCertificateOnlyAPublicKey
            | Error::Base64 { .. }
            | Error::Timestamp { .. }
            | Error::TrustRootUnreadable { .. }
            | Error::TrustRootMediaType { .. }
            | Error::NoValidKey { .. }
            | Error::Key { .. }
            | Error::PassphraseRequired
            | Error::PassphraseWrong
            | Error::ScryptParams { .. }
            | Error::FeatureDisabled { .. }
            | Error::Transport { .. }
            | Error::FulcioRejected { .. }
            | Error::RekorRejected { .. }
            | Error::Oidc(_)
            | Error::Io { .. } => Axis::Bundle,

            Error::NoSignatures | Error::SignatureInvalid | Error::PayloadType { .. } => {
                Axis::Signature
            }

            Error::CertificateParse(_)
            | Error::NoTrustedRoot
            | Error::ChainSignatureInvalid { .. }
            | Error::CertificateExpired { .. }
            | Error::NotCodeSigning
            | Error::NotACa { .. }
            | Error::ChainTooDeep { .. } => Axis::Chain,

            Error::SctMissing
            | Error::SctMalformed(_)
            | Error::SctUnknownLog { .. }
            | Error::SctInvalid => Axis::Sct,

            Error::InclusionProofMissing
            | Error::InclusionProofInvalid { .. }
            | Error::InclusionProofShape { .. }
            | Error::InclusionIndexOutOfRange { .. }
            | Error::CheckpointMalformed(_)
            | Error::CheckpointSignatureInvalid
            | Error::CheckpointOrigin { .. }
            | Error::UnknownLog { .. }
            | Error::LogEntryMismatch => Axis::TransparencyLog,

            Error::IdentityMismatch { .. }
            | Error::IssuerMismatch { .. }
            | Error::NoIssuer
            | Error::NoIdentity
            | Error::IdentityMalformed(_)
            | Error::IdentityKind { .. } => Axis::Identity,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
