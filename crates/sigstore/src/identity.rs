//! Who signed: Fulcio's X.509 identity extensions.
//!
//! # Two generations of extension, and only one is modern
//!
//! The legacy OIDs (1.3.6.1.4.1.57264.1.1 … .1.6) carry **raw UTF-8 strings**
//! in the extension value. The V2 OIDs (.1.8 … .1.24) carry **DER-encoded
//! UTF8String** — so reading a V2 extension with a raw-string parser yields the
//! value with two leading bytes of ASN.1 header attached.
//!
//! This is not a theoretical distinction. Here are the two issuer extensions of
//! one real leaf, `fixtures/identity-github-actions-leaf.pem`, byte for byte:
//!
//! ```text
//! 1.3.6.1.4.1.57264.1.1  len=43  68747470 733a2f2f …   "https://token.actions.githubusercontent.com"
//! 1.3.6.1.4.1.57264.1.8  len=45  0c2b6874 7470733a … "\x0c\x2bhttps://token.actions.githubusercontent.com"
//! ```
//!
//! `0x0c` is the UTF8String tag and `0x2b` is the length, 43. A verifier that
//! reads .1.8 as a raw string compares `"\x0c\x2bhttps://token.actions…"`
//! against the policy's `"https://token.actions…"` and refuses a genuine
//! certificate — or, worse, is written to *not* compare and simply reports the
//! wrong issuer to a human.
//!
//! `sigstore` 0.14.0 has both halves of that bug: its OID table stops at .1.7,
//! so **Issuer V2 cannot be asserted at all**, and the reader it would use is
//! `std::str::from_utf8(ext.extn_value.as_bytes())`
//! (`src/bundle/verify/policy.rs`), which is the raw-string path. Modern Fulcio
//! certificates carry .1.8 through .1.24, which is why this module reads both
//! generations and prefers V2.
//!
//! # A broken V2 is never quietly replaced by legacy — and neither is a
//! # *disagreeing* one
//!
//! When both issuer extensions are present they agree in every certificate in
//! the corpus. They are both inside the signed TBS, so they cannot disagree
//! without breaking the chain — but "cannot" is a property of the chain check,
//! not of this module, and this module runs on certificates the chain check has
//! not necessarily blessed yet. So: if .1.8 is present but does not decode as a
//! DER UTF8String, that is an error. It is **not** a reason to fall back to
//! .1.1 and report an issuer as though the modern extension had agreed with it.
//! A check that could not run must not read as a check that passed.
//!
//! That argument was first applied to only half the cases, and the missing half
//! was the dangerous one. A certificate holding
//! `.1.1 = "https://accounts.google.com"` and
//! `.1.8 = "https://evil.example.invalid"` — both perfectly well-formed, so no
//! encoding check fires — was answered with whichever generation this build
//! preferred, silently. `fixtures/identity-issuer-disagree-legacy-hostile.pem`
//! is the hostile direction: `.1.8` names Google, `.1.1` names the attacker, and
//! before this refusal existed `extract` returned
//! `"https://accounts.google.com"` and an `IdentityPolicy` written for Google
//! **passed**.
//!
//! The reason it must refuse rather than prefer is stated two paragraphs up in
//! these very docs: `sigstore` 0.14.0's OID table **stops at .1.7**. Both
//! extensions sit inside the same signed TBS, so such a certificate
//! authenticates as one issuer here and as a different issuer there — one
//! artifact, two answers, no complaint from either verifier. That is exactly the
//! failure the duplicate-extension check in [`extensions`] exists to prevent
//! ("choosing one of them silently is how a certificate comes to mean two
//! things"), and there is no principled reason it should apply to two copies of
//! `.1.8` but not to `.1.8` and `.1.1`.
//!
//! Refusing costs nothing measurable: across the 45 distinct Fulcio leaves in
//! this crate's corpus (`recon/`, deduplicated by SHA-256 fingerprint), 42 carry
//! both generations, 3 carry only the legacy one, and **zero disagree**. A
//! consequence worth stating: once agreement is enforced, *which* generation is
//! preferred stops being load-bearing, because the only certificate on which the
//! preference is observable is one that is now refused. Before the check
//! existed, reversing the preference passed the entire test suite.
//!
//! # The extension map exists because CI identity is policed by more than SAN
//!
//! "Signed by a GitHub Actions workflow" is not a useful claim on its own; the
//! useful claim is "signed by *this* reusable workflow, at *this* commit, on a
//! platform-hosted runner". Those are extensions .1.9, .1.10 and .1.11. They
//! are reachable through [`extensions`], which decodes every Fulcio extension
//! with the encoding its generation dictates and — importantly — reports the
//! ones it did **not** recognise rather than presenting a partial map as a
//! whole one.

use std::collections::BTreeMap;

use const_oid::ObjectIdentifier;
use der::asn1::Utf8StringRef;
use der::Decode;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::SubjectAltName;

use crate::error::{Error, Result};

/// OIDC issuer, legacy encoding (raw string).
pub const OID_ISSUER_LEGACY: &str = "1.3.6.1.4.1.57264.1.1";
/// OIDC issuer, V2 encoding (DER UTF8String). Preferred.
pub const OID_ISSUER_V2: &str = "1.3.6.1.4.1.57264.1.8";
/// Fulcio's username otherName SAN.
pub const OID_OTHERNAME_SAN: &str = "1.3.6.1.4.1.57264.1.7";

/// `subjectAltName`, RFC 5280 4.2.1.6.
const OID_SUBJECT_ALT_NAME: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.29.17");
const OID_ISSUER_LEGACY_DER: ObjectIdentifier = ObjectIdentifier::new_unwrap(OID_ISSUER_LEGACY);
const OID_ISSUER_V2_DER: ObjectIdentifier = ObjectIdentifier::new_unwrap(OID_ISSUER_V2);
const OID_OTHERNAME_SAN_DER: ObjectIdentifier = ObjectIdentifier::new_unwrap(OID_OTHERNAME_SAN);

/// The arc every Fulcio certificate extension hangs off: `1.3.6.1.4.1.57264.1`.
///
/// Used to tell "an extension Fulcio set that this build does not know" from
/// "an extension some other CA set", which are very different things to report.
pub const FULCIO_ARC: &str = "1.3.6.1.4.1.57264.1.";

/// How an extension's value is encoded — the whole point of this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    /// The extension value is the UTF-8 bytes of the string, with no ASN.1
    /// wrapper. Extensions .1.1 through .1.6 only.
    RawString,
    /// The extension value is a DER-encoded `UTF8String` (tag `0x0c`).
    /// Extensions .1.8 through .1.24.
    DerUtf8String,
}

/// Every Fulcio extension, its human name, and which generation's encoding it
/// uses.
///
/// Ordered by OID. `.1.7` is absent on purpose: it is not a certificate
/// extension at all, it is the `type-id` of an `otherName` *inside* the SAN.
///
/// `.1.23` (Deployment Environment) and `.1.24` (Token Subject) are in Fulcio's
/// published directory but **not** in sigstore-go's `ParseExtensions`, which
/// stops at `.1.22`. A live prod leaf captured for this crate carries `.1.24`,
/// so that gap is observable rather than hypothetical.
const FULCIO_EXTENSIONS: &[(ObjectIdentifier, &str, Encoding)] = &[
    (
        OID_ISSUER_LEGACY_DER,
        "Issuer (deprecated)",
        Encoding::RawString,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.2"),
        "GitHub Workflow Trigger (deprecated)",
        Encoding::RawString,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.3"),
        "GitHub Workflow SHA (deprecated)",
        Encoding::RawString,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.4"),
        "GitHub Workflow Name (deprecated)",
        Encoding::RawString,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.5"),
        "GitHub Workflow Repository (deprecated)",
        Encoding::RawString,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.6"),
        "GitHub Workflow Ref (deprecated)",
        Encoding::RawString,
    ),
    (OID_ISSUER_V2_DER, "Issuer", Encoding::DerUtf8String),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.9"),
        "Build Signer URI",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.10"),
        "Build Signer Digest",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.11"),
        "Runner Environment",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.12"),
        "Source Repository URI",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.13"),
        "Source Repository Digest",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.14"),
        "Source Repository Ref",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.15"),
        "Source Repository Identifier",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.16"),
        "Source Repository Owner URI",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.17"),
        "Source Repository Owner Identifier",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.18"),
        "Build Config URI",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.19"),
        "Build Config Digest",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.20"),
        "Build Trigger",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.21"),
        "Run Invocation URI",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.22"),
        "Source Repository Visibility At Signing",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.23"),
        "Deployment Environment",
        Encoding::DerUtf8String,
    ),
    (
        ObjectIdentifier::new_unwrap("1.3.6.1.4.1.57264.1.24"),
        "Token Subject",
        Encoding::DerUtf8String,
    ),
];

/// The human name Fulcio's directory gives an extension OID, if this build
/// knows it.
pub fn extension_name(oid: &str) -> Option<&'static str> {
    FULCIO_EXTENSIONS
        .iter()
        .find(|(o, _, _)| o.to_string() == oid)
        .map(|(_, name, _)| *name)
}

/// The identity a bundle proves — the *answer*, not a boolean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The subject alternative name: an email, a URI, or an otherName.
    pub san: String,
    /// Which SAN form carried it.
    pub san_kind: SanKind,
    /// The OIDC issuer that vouched for it.
    pub issuer: String,
    pub not_before: time::OffsetDateTime,
    pub not_after: time::OffsetDateTime,
    /// The log entry that dates the signature.
    pub log_index: Option<u64>,
    pub integrated_time: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SanKind {
    Email,
    Uri,
    OtherName,
}

impl SanKind {
    /// The RFC 5280 `GeneralName` alternative this identity arrived in.
    ///
    /// Worth printing next to the identity: `foo@example.com` as an
    /// `rfc822Name` and `foo@example.com` as a Fulcio `otherName` are different
    /// facts, and a policy written for one does not police the other.
    pub fn as_str(self) -> &'static str {
        match self {
            SanKind::Email => "rfc822Name",
            SanKind::Uri => "uniformResourceIdentifier",
            SanKind::OtherName => "otherName:1.3.6.1.4.1.57264.1.7",
        }
    }
}

impl std::fmt::Display for SanKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every Fulcio extension on a certificate, decoded by its own generation's
/// rules.
///
/// Values are keyed by dotted OID string rather than by struct field so that a
/// Fulcio release adding `.1.25` does not require a code change to *see* — see
/// [`FulcioExtensions::unrecognized`], which is the honest half of that
/// bargain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FulcioExtensions {
    values: BTreeMap<String, String>,
    unrecognized: Vec<String>,
}

impl FulcioExtensions {
    /// The decoded value of one extension, by dotted OID.
    pub fn get(&self, oid: &str) -> Option<&str> {
        self.values.get(oid).map(String::as_str)
    }

    /// `(oid, human name, value)` for every extension this build decoded, in
    /// OID order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &'static str, &str)> {
        self.values.iter().map(|(oid, value)| {
            (
                oid.as_str(),
                extension_name(oid).unwrap_or("unknown"),
                value.as_str(),
            )
        })
    }

    /// OIDs under Fulcio's arc that this build does **not** know how to decode.
    ///
    /// Never empty-by-omission: a caller that polices CI provenance and needs
    /// the map to be complete must check this, because a `1.3.6.1.4.1.57264.1.`
    /// extension we skipped could be exactly the constraint it wanted. Reading
    /// a short map as a complete one is the failure this accessor exists to
    /// prevent.
    pub fn unrecognized(&self) -> &[String] {
        &self.unrecognized
    }

    /// True only when the certificate carries **no** extension under Fulcio's
    /// arc at all — decoded or not.
    ///
    /// Deliberately not "the decoded map is empty". A certificate carrying
    /// nothing but `1.3.6.1.4.1.57264.1.99` used to answer `is_empty() == true`
    /// while [`unrecognized`](Self::unrecognized) held that OID, so a caller
    /// branching on `if exts.is_empty() { /* no Fulcio constraints to police */ }`
    /// concluded "not a Fulcio certificate" about a certificate carrying a
    /// Fulcio constraint this build could not read. A collection that is empty
    /// only because everything in it was skipped is the vacuous pass this whole
    /// module is written against; `identity-only-unrecognized.pem` is that
    /// certificate.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.unrecognized.is_empty()
    }

    /// How many Fulcio extensions this build **decoded** — which is not
    /// necessarily how many the certificate carries. The difference is
    /// [`unrecognized`](Self::unrecognized), which is why `len() == 0` does not
    /// imply [`is_empty`](Self::is_empty).
    pub fn len(&self) -> usize {
        self.values.len()
    }
}

/// Decode every Fulcio extension on `cert`.
///
/// Each value is read with the encoding its OID generation mandates: raw UTF-8
/// for .1.1–.1.6, DER `UTF8String` for .1.8–.1.24. A value that does not decode
/// is an error, not a skipped entry.
pub fn extensions(cert: &x509_cert::Certificate) -> Result<FulcioExtensions> {
    let mut out = FulcioExtensions::default();
    for ext in cert.tbs_certificate().extensions().into_iter().flatten() {
        let oid = ext.extn_id.to_string();
        let Some((_, name, encoding)) = FULCIO_EXTENSIONS
            .iter()
            .find(|(candidate, _, _)| *candidate == ext.extn_id)
        else {
            if oid.starts_with(FULCIO_ARC) {
                out.unrecognized.push(oid);
            }
            continue;
        };
        let value = decode_extension(&oid, name, *encoding, ext.extn_value.as_bytes())?;
        // RFC 5280 4.2: "A certificate MUST NOT include more than one instance
        // of a particular extension." Two .1.8s with different values is a
        // certificate that names two issuers; last-write-wins would pick one
        // and say nothing.
        if let Some(first) = out.values.insert(oid.clone(), value) {
            return Err(Error::IdentityMalformed(format!(
                "extension {oid} ({name}) appears more than once, first as {first:?} — RFC 5280 \
                 4.2 forbids this, and choosing one of them silently is how a certificate comes \
                 to mean two things"
            )));
        }
    }
    Ok(out)
}

fn decode_extension(oid: &str, name: &str, encoding: Encoding, value: &[u8]) -> Result<String> {
    match encoding {
        Encoding::RawString => std::str::from_utf8(value).map(str::to_owned).map_err(|e| {
            Error::IdentityMalformed(format!(
                "extension {oid} ({name}) is a raw UTF-8 string per Fulcio's directory, but is not \
                 valid UTF-8: {e}"
            ))
        }),
        Encoding::DerUtf8String => Utf8StringRef::from_der(value)
            .map(|s| s.as_str().to_owned())
            .map_err(|e| {
                Error::IdentityMalformed(format!(
                    "extension {oid} ({name}) is documented as a DER-encoded UTF8String but did \
                     not decode as one: {e}. Reading it as a raw string instead — which \
                     `sigstore` 0.14.0 does — would have returned the value with its ASN.1 \
                     header bytes still attached"
                ))
            }),
    }
}

/// The OIDC issuer that vouched for this certificate's identity.
///
/// V2 (.1.8) wins when present. Legacy (.1.1) is the fallback for certificates
/// issued before Fulcio added the DER-encoded form; the fixture
/// `identity-legacy-only-leaf.pem` is one, issued in January 2023.
///
/// An issuer extension that is present but undecodable is an error rather than
/// a fallback. So is a certificate whose two generations name **different**
/// issuers: this function reports the issuer a certificate names, and a
/// certificate that names two does not have one. See the module docs.
pub fn issuer(cert: &x509_cert::Certificate) -> Result<String> {
    let exts = extensions(cert)?;
    let v2 = exts.get(OID_ISSUER_V2);
    let legacy = exts.get(OID_ISSUER_LEGACY);

    // Both present and disagreeing: refuse, and name both values. Preferring one
    // answers a question the certificate answers twice, and the losing value is
    // exactly what a verifier whose OID table stops before .1.8 — `sigstore`
    // 0.14.0's does — reports instead.
    if let (Some(v2), Some(legacy)) = (v2, legacy) {
        if v2 != legacy {
            return Err(Error::IdentityMalformed(format!(
                "the certificate names two different OIDC issuers: {OID_ISSUER_V2} says {v2:?} and \
                 the legacy {OID_ISSUER_LEGACY} says {legacy:?}. Both sit inside the same signed \
                 TBS, so this certificate authenticates as {v2:?} here and as {legacy:?} to any \
                 verifier whose Fulcio OID table stops before .1.8 — one artifact, two issuers, no \
                 complaint from either side. Which one vouched for this identity is not a question \
                 this build will answer by picking"
            )));
        }
    }

    match v2.or(legacy) {
        // An empty issuer names nobody. Returning `""` here would let a policy
        // with `issuer: Some("")` match, and would print as though the question
        // had been answered. `identity-issuer-empty-only.pem` is a certificate
        // whose .1.8 decodes cleanly to the empty string.
        Some("") | None => Err(Error::NoIssuer),
        Some(v) => Ok(v.to_owned()),
    }
}

/// Every identity-bearing subject alternative name on the certificate, in the
/// order the certificate lists them.
///
/// `dNSName`, `iPAddress`, `directoryName`, `ediPartyName` and `registeredID`
/// are **not** identities in Sigstore's model; an `otherName` whose `type-id` is
/// not Fulcio's `1.3.6.1.4.1.57264.1.7` is somebody else's naming scheme. A
/// certificate carrying *only* those is [`Error::NoIdentity`]: this build did
/// not find a name it is able to police, which it must say rather than pick
/// something.
///
/// # Why a name we cannot police is a refusal and not an omission
///
/// A certificate carrying **both** — say `rfc822Name = nobody@example.invalid`
/// and `dNSName = evil.example.invalid` — used to return exactly
/// `[(Email, "nobody@example.invalid")]`, and that list is the only thing this
/// module offers a caller who wants to police *"this certificate names only
/// X"*. Answering that question from a list we silently shortened is the
/// vacuous pass in its purest form: the second name is not reported as skipped,
/// it is not reported at all, and `names == vec![(Email, X)]` reads as proof of
/// "only X". So the shortened list is never returned; the certificate is
/// refused and the name we could not police is quoted in the error.
///
/// Fulcio marks `subjectAltName` **critical** on every leaf in this crate's
/// corpus, and RFC 5280 4.2 requires a system that encounters a critical
/// extension containing information it cannot process to reject the
/// certificate — so for a real Sigstore leaf this is not merely defensible, it
/// is the specified behaviour. The cost is bounded and measured: of the 45
/// distinct Fulcio leaves in the corpus, **every one carries exactly one SAN
/// entry** and none carries a `dNSName`, so nothing real is refused by this.
/// `fixtures/identity-san-extra-dnsname.pem` is the certificate that is.
pub fn subject_alternative_names(cert: &x509_cert::Certificate) -> Result<Vec<(SanKind, String)>> {
    let tbs = cert.tbs_certificate();

    // Counted first, because `get_extension` collapses "two SAN extensions"
    // into the same `Err` as "one malformed SAN extension", and those want
    // different words.
    let count = tbs
        .extensions()
        .into_iter()
        .flatten()
        .filter(|e| e.extn_id == OID_SUBJECT_ALT_NAME)
        .count();
    if count > 1 {
        return Err(Error::IdentityMalformed(format!(
            "the certificate carries {count} subjectAltName extensions; RFC 5280 4.2 permits one, \
             and a verifier that reads the first is policing a name the certificate may not have \
             meant"
        )));
    }

    let (critical, san) = match tbs.get_extension::<SubjectAltName>() {
        // A `subjectAltName` that is present and does not decode is **not** a
        // certificate with no subject alternative name. Collapsing the two —
        // `Err(_) => Err(Error::NoIdentity)` — would report "this certificate
        // names nobody" about a certificate whose names could not be read, and
        // no test in this module noticed the difference until
        // `identity-san-undecodable.pem` existed.
        Ok(Some(pair)) => pair,
        Ok(None) => return Err(Error::NoIdentity),
        Err(e) => {
            return Err(Error::IdentityMalformed(format!(
                "subjectAltName (2.5.29.17) did not decode: {e}"
            )))
        }
    };

    let mut out = Vec::new();
    let mut unpoliceable = Vec::new();
    for name in san.0.iter() {
        match name {
            GeneralName::Rfc822Name(email) => out.push((SanKind::Email, email.as_str().to_owned())),
            GeneralName::UniformResourceIdentifier(uri) => {
                out.push((SanKind::Uri, uri.as_str().to_owned()))
            }
            GeneralName::OtherName(other) if other.type_id == OID_OTHERNAME_SAN_DER => {
                // Fulcio's otherName is `[0] EXPLICIT UTF8String` — see
                // sigstore/sigstore's `MarshalOtherNameSAN`, whose Go struct tag
                // is literally `asn1:"utf8,explicit,tag:0"`. Anything else under
                // Fulcio's own OID is malformed, not "a name we skipped".
                let value = Utf8StringRef::try_from(&other.value).map_err(|e| {
                    Error::IdentityMalformed(format!(
                        "the otherName SAN under Fulcio's {OID_OTHERNAME_SAN} is not a UTF8String: \
                         {e}"
                    ))
                })?;
                out.push((SanKind::OtherName, value.as_str().to_owned()));
            }
            other => unpoliceable.push(describe_general_name(other)),
        }
    }

    // A name that is present and empty is not a name. `""` would otherwise
    // satisfy a policy of `Some(String::new())` and print as an answer.
    out.retain(|(_, v)| !v.is_empty());

    // A SAN extension that exists but holds no identity we can police is the
    // same answer as no SAN at all, and must not read as one that matched.
    if out.is_empty() {
        return Err(Error::NoIdentity);
    }

    if !unpoliceable.is_empty() {
        let basis = if critical {
            "and this certificate marks subjectAltName critical, which under RFC 5280 4.2 forbids \
             ignoring the part of it this build cannot process"
        } else {
            "and a list this build shortened still reads as the complete one"
        };
        return Err(Error::IdentityMalformed(format!(
            "the certificate's subjectAltName carries {} name(s) this build cannot police as a \
             Sigstore identity ({}) beside {} it can ({}). Returning only the ones it understands \
             would answer \"this certificate names only X\" from a silently shortened list, {basis}",
            unpoliceable.len(),
            unpoliceable.join(", "),
            out.len(),
            out.iter()
                .map(|(k, v)| format!("{k}:{v}"))
                .collect::<Vec<_>>()
                .join(", "),
        )));
    }

    Ok(out)
}

/// The RFC 5280 name of a `GeneralName` alternative, with enough of its value to
/// recognise the certificate again.
///
/// Exhaustive on purpose: a `_` arm here would turn "a name we cannot police"
/// back into "a name we did not mention", which is the bug this function exists
/// to report. `x509-cert` 0.3 does not model `x400Address`, so the eight arms
/// below are all of `GeneralName`.
fn describe_general_name(name: &GeneralName) -> String {
    match name {
        GeneralName::OtherName(other) => format!("otherName:{}", other.type_id),
        GeneralName::Rfc822Name(v) => format!("rfc822Name:{}", v.as_str()),
        GeneralName::DnsName(v) => format!("dNSName:{}", v.as_str()),
        GeneralName::DirectoryName(_) => "directoryName".to_owned(),
        GeneralName::EdiPartyName(_) => "ediPartyName".to_owned(),
        GeneralName::UniformResourceIdentifier(v) => {
            format!("uniformResourceIdentifier:{}", v.as_str())
        }
        GeneralName::IpAddress(_) => "iPAddress".to_owned(),
        GeneralName::RegisteredId(oid) => format!("registeredID:{oid}"),
    }
}

/// Extract SAN and issuer from a parsed certificate.
///
/// Returns `(san, kind, issuer)`.
///
/// # Which SAN, when there are several
///
/// Fulcio issues exactly one, and every certificate in this crate's corpus has
/// exactly one. When a certificate has more, the order here is URI, then email,
/// then otherName — matching sigstore-go's `SummarizeCertificate`, so this
/// build does not reject an artifact the reference verifier accepts. The rest
/// stay visible through [`subject_alternative_names`]; a caller that needs
/// "this certificate names *only* X" must ask for the list, because a single
/// returned string cannot express it.
///
/// Note the difference from `sigstore` 0.14.0, which tests
/// `names.contains(&self.identity)` — *any* SAN matching satisfies its policy.
/// Here one name is chosen and compared, so an extra SAN cannot be the one that
/// makes a policy pass.
pub fn extract(cert: &x509_cert::Certificate) -> Result<(String, SanKind, String)> {
    let names = subject_alternative_names(cert)?;
    let pick = |kind: SanKind| {
        names
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(k, v)| (*k, v.clone()))
    };
    let (kind, san) = pick(SanKind::Uri)
        .or_else(|| pick(SanKind::Email))
        .or_else(|| pick(SanKind::OtherName))
        .ok_or(Error::NoIdentity)?;
    // Unreachable via `subject_alternative_names`, which drops empties, but the
    // invariant is the one the caller depends on, so it is asserted here too.
    if san.is_empty() {
        return Err(Error::NoIdentity);
    }
    let issuer = issuer(cert)?;
    Ok((san, kind, issuer))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where each fixture came from, because a synthesized certificate proving
    /// a property about real ones is worth exactly as much as the honesty of
    /// this list.
    ///
    /// **Captured from the wild** — these are the certificates that carry the
    /// facts the module is built around:
    ///
    /// | fixture | source | what it proves |
    /// |---|---|---|
    /// | `identity-email-leaf.pem` | leaf of the CPython 3.13.7 release bundle (`python.org`, Aug 2025) | an `rfc822Name` identity with both issuer generations |
    /// | `identity-github-actions-leaf.pem` | leaf of a sigstore-conformance beacon bundle (Jun 2025) | a URI identity with .1.1–.1.6 **and** .1.8–.1.22 |
    /// | `identity-legacy-only-leaf.pem` | leaf of `sigstore-python`'s `bundle_no_log_entry.txt.sigstore` (Jan 2023) | a certificate predating .1.8 entirely |
    /// | `identity-fulcio-root.pem` | the live production Fulcio root | a real certificate with neither SAN nor Fulcio extensions |
    ///
    /// **Synthesized**, because the corpus of ~95 real bundles contains no
    /// `otherName` identity and no malformed certificate at all — the negative
    /// cases had to be built. Each is a self-signed P-256 certificate written
    /// with `openssl req -x509`; `identity-dupsan.pem` and
    /// `identity-dupissuer.pem` were then spliced at the DER level to repeat an
    /// extension, so their signatures no longer verify (irrelevant here, and
    /// checked elsewhere): `identity-othername-leaf.pem`,
    /// `identity-nosan.pem`, `identity-noissuer.pem`, `identity-rawv2.pem`,
    /// `identity-emptyother.pem`, `identity-foreignother.pem`,
    /// `identity-multi.pem`, `identity-trailing.pem`,
    /// `identity-unknownarc.pem`, `identity-dupsan.pem`,
    /// `identity-dupissuer.pem`.
    ///
    /// **Captured from another implementation** —
    /// `identity-othername-real-sigstore-go.pem` is the leaf of
    /// `pkg/testing/data/bundles/othername.sigstore.json` in sigstore-go, issued
    /// 2024-07-12 by their `oidc.local` test CA. It is the only `otherName`
    /// identity in a corpus of 49 certificates, and it is not mine: it settles
    /// whether the synthesized `identity-othername-leaf.pem` encodes an
    /// `otherName` the way a real Sigstore implementation does. It is test data
    /// from a test CA, not a production Fulcio leaf, and is labelled that way
    /// wherever it is used.
    ///
    /// **Synthesized by an adversarial pass over this module**, each one an
    /// input the module accepted, mis-reported, or had no test for. Written with
    /// `cryptography`'s `x509.CertificateBuilder` (self-signed P-256, extension
    /// values supplied as raw DER via `UnrecognizedExtension`):
    /// `identity-issuer-disagree.pem`,
    /// `identity-issuer-disagree-legacy-hostile.pem`,
    /// `identity-issuer-empty-only.pem`, `identity-issuer-longform-len.pem`,
    /// `identity-only-unrecognized.pem`, `identity-san-empty-seq.pem`,
    /// `identity-san-undecodable.pem`, `identity-san-extra-dnsname.pem`,
    /// `identity-san-two-uris.pem`, `identity-othername-email-shaped.pem`,
    /// `identity-othername-octetstring.pem`, `identity-cn-only.pem`,
    /// `identity-othername-oid-as-extension.pem`.
    fn cert(name: &str) -> x509_cert::Certificate {
        let pem = std::fs::read(format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("fixture {name}: {e}"));
        let mut chain = crate::x509::parse_pem_chain(&pem).expect("fixture parses");
        assert_eq!(chain.len(), 1, "fixture {name} should hold one certificate");
        chain.remove(0)
    }

    // --- the encoding trap, proved on real bytes ---------------------------

    #[test]
    fn v2_issuer_decodes_without_the_asn1_header_that_a_raw_read_leaves_behind() {
        let leaf = cert("identity-github-actions-leaf.pem");
        let (_, _, issuer) = extract(&leaf).expect("real Fulcio leaf");
        assert_eq!(issuer, "https://token.actions.githubusercontent.com");
        assert!(!issuer.starts_with('\x0c'), "ASN.1 tag byte leaked through");

        // And show what the raw read this module refuses to do would have given.
        let raw = leaf
            .tbs_certificate()
            .extensions()
            .expect("extensions")
            .iter()
            .find(|e| e.extn_id == OID_ISSUER_V2_DER)
            .map(|e| e.extn_value.as_bytes().to_vec())
            .expect(".1.8 present");
        assert_eq!(raw[0], 0x0c, "UTF8String tag");
        assert_eq!(raw[1] as usize, issuer.len(), "DER length byte");
        assert_eq!(raw.len(), issuer.len() + 2);
        let as_sigstore_014_would_read_it = std::str::from_utf8(&raw).expect("utf8");
        assert_ne!(as_sigstore_014_would_read_it, issuer);
        assert!(as_sigstore_014_would_read_it.starts_with("\u{0c}\u{2b}https://"));
    }

    #[test]
    fn legacy_and_v2_issuer_extensions_decode_to_the_same_clean_string() {
        // One certificate, both generations present, two different encodings.
        let leaf = cert("identity-github-actions-leaf.pem");
        let exts = extensions(&leaf).expect("decodes");
        let legacy = exts.get(OID_ISSUER_LEGACY).expect(".1.1 present");
        let v2 = exts.get(OID_ISSUER_V2).expect(".1.8 present");
        assert_eq!(legacy, v2);
        assert_eq!(v2, "https://token.actions.githubusercontent.com");

        // And on a leaf whose issuer is a different provider entirely.
        let email_leaf = cert("identity-email-leaf.pem");
        let exts = extensions(&email_leaf).expect("decodes");
        assert_eq!(
            exts.get(OID_ISSUER_LEGACY),
            exts.get(OID_ISSUER_V2),
            "the two generations must agree"
        );
        assert_eq!(exts.get(OID_ISSUER_V2), Some("https://accounts.google.com"));
    }

    #[test]
    fn a_certificate_from_before_v2_still_yields_its_issuer() {
        // Issued 2023-01-31, before Fulcio emitted .1.8 at all.
        let leaf = cert("identity-legacy-only-leaf.pem");
        let exts = extensions(&leaf).expect("decodes");
        assert_eq!(exts.get(OID_ISSUER_V2), None, "fixture must be legacy-only");
        let (san, kind, issuer) = extract(&leaf).expect("legacy-only leaf");
        assert_eq!(san, "alex.cameron@trailofbits.com");
        assert_eq!(kind, SanKind::Email);
        assert_eq!(issuer, "https://accounts.google.com");
    }

    // --- the three SAN forms ------------------------------------------------

    #[test]
    fn an_email_identity_is_read_from_rfc822name() {
        let leaf = cert("identity-email-leaf.pem");
        let (san, kind, issuer) = extract(&leaf).expect("real CPython release leaf");
        assert_eq!(san, "thomas@python.org");
        assert_eq!(kind, SanKind::Email);
        assert_eq!(issuer, "https://accounts.google.com");
        assert_eq!(kind.as_str(), "rfc822Name");
    }

    #[test]
    fn a_github_actions_identity_is_read_from_the_uri_san() {
        let leaf = cert("identity-github-actions-leaf.pem");
        let (san, kind, _) = extract(&leaf).expect("real conformance beacon leaf");
        assert_eq!(kind, SanKind::Uri);
        assert_eq!(
            san,
            "https://github.com/sigstore-conformance/extremely-dangerous-public-oidc-beacon/\
             .github/workflows/extremely-dangerous-oidc-beacon.yml@refs/heads/main"
        );
    }

    #[test]
    fn a_username_identity_is_read_from_fulcios_othername() {
        let leaf = cert("identity-othername-leaf.pem");
        let (san, kind, issuer) = extract(&leaf).expect("otherName leaf");
        assert_eq!(san, "jdoe");
        assert_eq!(kind, SanKind::OtherName);
        assert_eq!(issuer, "https://accounts.google.com");
        assert_eq!(kind.as_str(), "otherName:1.3.6.1.4.1.57264.1.7");
    }

    // --- the CI extensions, which are what actually polices a workflow ------

    #[test]
    fn the_ci_extensions_are_reachable_and_carry_no_der_headers() {
        let leaf = cert("identity-github-actions-leaf.pem");
        let exts = extensions(&leaf).expect("decodes");

        assert_eq!(
            exts.get("1.3.6.1.4.1.57264.1.9"),
            Some(
                "https://github.com/sigstore-conformance/extremely-dangerous-public-oidc-beacon/\
                 .github/workflows/extremely-dangerous-oidc-beacon.yml@refs/heads/main"
            ),
            "Build Signer URI"
        );
        assert_eq!(
            exts.get("1.3.6.1.4.1.57264.1.10"),
            Some("9739798945b2a848e911bcf1dddb97298cade0b1"),
            "Build Signer Digest"
        );
        assert_eq!(
            exts.get("1.3.6.1.4.1.57264.1.11"),
            Some("github-hosted"),
            "Runner Environment — the difference between a hosted runner and \
             someone's laptop"
        );
        assert_eq!(
            exts.get("1.3.6.1.4.1.57264.1.12"),
            Some("https://github.com/sigstore-conformance/extremely-dangerous-public-oidc-beacon"),
            "Source Repository URI"
        );
        assert_eq!(
            exts.get("1.3.6.1.4.1.57264.1.14"),
            Some("refs/heads/main"),
            "Source Repository Ref"
        );
        assert_eq!(
            exts.get("1.3.6.1.4.1.57264.1.22"),
            Some("public"),
            "Source Repository Visibility At Signing"
        );

        // Six legacy + fifteen V2 on this leaf, none of them unrecognized.
        assert_eq!(exts.len(), 21);
        assert!(exts.unrecognized().is_empty());

        // No decoded value may retain an ASN.1 header.
        for (oid, name, value) in exts.iter() {
            assert!(
                !value.starts_with('\u{0c}'),
                "{oid} ({name}) kept its UTF8String tag"
            );
        }
    }

    #[test]
    fn every_extension_the_map_reports_has_a_name_this_build_knows() {
        let leaf = cert("identity-github-actions-leaf.pem");
        for (oid, name, _) in extensions(&leaf).expect("decodes").iter() {
            assert_ne!(name, "unknown", "{oid} decoded but has no directory name");
            assert_eq!(extension_name(oid), Some(name));
        }
    }

    // --- negative cases -----------------------------------------------------

    #[test]
    fn a_certificate_with_no_san_names_no_identity() {
        let c = cert("identity-nosan.pem");
        // The issuer extension IS present, so this isolates the SAN failure.
        assert_eq!(
            issuer(&c).expect("issuer present"),
            "https://accounts.google.com"
        );
        assert!(matches!(extract(&c), Err(Error::NoIdentity)));
        assert!(matches!(
            subject_alternative_names(&c),
            Err(Error::NoIdentity)
        ));
    }

    #[test]
    fn a_certificate_with_no_issuer_extension_names_no_issuer() {
        let c = cert("identity-noissuer.pem");
        // The SAN IS present, so this isolates the issuer failure — and proves
        // the two errors are not interchangeable.
        let names = subject_alternative_names(&c).expect("SAN present");
        assert_eq!(
            names,
            vec![(SanKind::Email, "nobody@example.invalid".to_string())]
        );
        assert!(matches!(issuer(&c), Err(Error::NoIssuer)));
        assert!(matches!(extract(&c), Err(Error::NoIssuer)));
    }

    #[test]
    fn a_real_fulcio_root_has_neither_and_says_so_rather_than_returning_empty() {
        // A CA certificate, not a leaf: no SAN, no Fulcio extensions at all.
        let root = cert("identity-fulcio-root.pem");
        assert!(extensions(&root).expect("decodes").is_empty());
        assert!(matches!(issuer(&root), Err(Error::NoIssuer)));
        assert!(matches!(extract(&root), Err(Error::NoIdentity)));
    }

    #[test]
    fn a_v2_issuer_that_is_not_der_is_refused_and_not_quietly_replaced_by_legacy() {
        // .1.8 holding a raw string — the exact mistake a producer makes when it
        // copies the legacy encoding forward. .1.1 holds the correct legacy raw
        // string, so a silent fallback would return a plausible issuer and
        // nothing would look wrong.
        let c = cert("identity-rawv2.pem");
        let err = extract(&c).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains("1.3.6.1.4.1.57264.1.8"), "{msg}");
        assert!(msg.contains("DER-encoded UTF8String"), "{msg}");
        assert!(
            !msg.contains("accounts.google.com"),
            "the legacy value must not be reported as the answer: {msg}"
        );
        assert!(matches!(err, Error::IdentityMalformed(_)));
    }

    #[test]
    fn an_empty_othername_is_not_an_identity() {
        // Fulcio would never issue this, but "the SAN decoded" and "the SAN
        // names somebody" are different claims, and `""` satisfies a policy of
        // `Some(String::new())`.
        let c = cert("identity-emptyother.pem");
        assert!(matches!(extract(&c), Err(Error::NoIdentity)));
        assert!(matches!(
            subject_alternative_names(&c),
            Err(Error::NoIdentity)
        ));
    }

    #[test]
    fn an_othername_under_somebody_elses_oid_is_not_a_sigstore_identity() {
        // A Microsoft UPN otherName (1.3.6.1.4.1.311.20.2.3). It is a perfectly
        // good name — it is just not the one Fulcio issues, and accepting it
        // would let a certificate from another naming scheme satisfy a
        // Sigstore identity policy.
        let c = cert("identity-foreignother.pem");
        assert!(matches!(extract(&c), Err(Error::NoIdentity)));
    }

    #[test]
    fn an_extra_san_cannot_be_the_one_that_makes_a_policy_pass() {
        // URI + email on one certificate. `extract` returns the URI (matching
        // sigstore-go's precedence) and the email stays visible to a caller
        // that wants to police "names only X" — but it is never returned as
        // *the* identity, which is how `sigstore` 0.14.0's
        // `names.contains(identity)` would treat it.
        let c = cert("identity-multi.pem");
        let (san, kind, _) = extract(&c).expect("has identities");
        assert_eq!(kind, SanKind::Uri);
        assert_eq!(
            san,
            "https://github.com/o/r/.github/workflows/w.yml@refs/heads/main"
        );
        let all = subject_alternative_names(&c).expect("has identities");
        assert_eq!(all.len(), 2);
        assert!(all.contains(&(SanKind::Email, "decoy@example.invalid".to_string())));
        assert_ne!(san, "decoy@example.invalid");
    }

    #[test]
    fn a_der_string_with_trailing_bytes_is_refused_rather_than_truncated() {
        // .1.9 holds `0C 03 "abc"` followed by a stray `FF`. A decoder that
        // reads the first TLV and stops returns "abc" and never mentions the
        // byte it ignored — so the certificate would be reported as naming a
        // build signer it does not name.
        let c = cert("identity-trailing.pem");
        let err = extensions(&c).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains("1.3.6.1.4.1.57264.1.9"), "{msg}");
        assert!(
            !msg.contains("abc"),
            "the truncated value must not be the answer: {msg}"
        );
    }

    #[test]
    fn an_extension_fulcio_added_after_this_build_is_reported_not_skipped() {
        // .1.99 does not exist today. When it does, a caller policing CI
        // provenance must be able to see that the map it is reading is short.
        let c = cert("identity-unknownarc.pem");
        let exts = extensions(&c).expect("decodes what it knows");
        assert_eq!(exts.get(OID_ISSUER_V2), Some("https://accounts.google.com"));
        assert_eq!(exts.unrecognized(), ["1.3.6.1.4.1.57264.1.99"]);
        // An extension outside Fulcio's arc is somebody else's business and is
        // not reported as a Fulcio gap.
        assert!(!exts
            .unrecognized()
            .iter()
            .any(|o| o.starts_with("1.3.6.1.4.1.99999")));
    }

    #[test]
    fn a_certificate_that_names_two_issuers_is_refused_rather_than_reduced_to_one() {
        // Two .1.8 extensions. `BTreeMap::insert` would keep the last and say
        // nothing; RFC 5280 4.2 forbids the shape outright.
        let c = cert("identity-dupissuer.pem");
        let err = issuer(&c).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains("appears more than once"), "{msg}");
        assert!(msg.contains("1.3.6.1.4.1.57264.1.8"), "{msg}");
    }

    #[test]
    fn two_san_extensions_are_refused_rather_than_read_first_one_wins() {
        let c = cert("identity-dupsan.pem");
        let err = subject_alternative_names(&c).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains("2 subjectAltName extensions"), "{msg}");
        // And the refusal is not mistaken for "this certificate has no name".
        assert!(!matches!(err, Error::NoIdentity));
    }

    #[test]
    fn the_identity_error_and_the_issuer_error_sit_on_the_identity_axis() {
        use crate::error::Axis;
        assert_eq!(Error::NoIdentity.axis(), Axis::Identity);
        assert_eq!(Error::NoIssuer.axis(), Axis::Identity);
    }

    #[test]
    fn the_othername_oid_is_not_in_the_extension_table_because_it_is_not_an_extension() {
        assert_eq!(extension_name(OID_OTHERNAME_SAN), None);
        assert_eq!(extension_name(OID_ISSUER_V2), Some("Issuer"));
        assert_eq!(
            extension_name(OID_ISSUER_LEGACY),
            Some("Issuer (deprecated)")
        );
        assert_eq!(
            extension_name("1.3.6.1.4.1.57264.1.24"),
            Some("Token Subject")
        );
        // .1.23 and .1.24 are in Fulcio's directory and absent from
        // sigstore-go's parser; this build knows them.
        assert_eq!(
            extension_name("1.3.6.1.4.1.57264.1.23"),
            Some("Deployment Environment")
        );
    }

    #[test]
    fn the_extension_table_is_ordered_and_free_of_duplicates() {
        // A duplicated OID would make `find` return the first entry and the
        // second unreachable, which no test on a real certificate would notice.
        let mut seen = std::collections::BTreeSet::new();
        for (oid, name, encoding) in FULCIO_EXTENSIONS {
            assert!(seen.insert(oid.to_string()), "duplicate OID {oid}");
            assert!(!name.is_empty());
            let last: u32 = oid
                .to_string()
                .rsplit('.')
                .next()
                .and_then(|s| s.parse().ok())
                .expect("numeric final arc");
            let expected = if last <= 6 {
                Encoding::RawString
            } else {
                Encoding::DerUtf8String
            };
            assert_eq!(*encoding, expected, "{oid} has the wrong generation");
        }
        assert_eq!(seen.len(), 23, ".1.1-.1.6 and .1.8-.1.24, without .1.7");
    }

    // --- what the adversarial pass found -----------------------------------

    /// The `subjectAltName` extension value, as it sits in the certificate.
    fn san_bytes(c: &x509_cert::Certificate) -> Vec<u8> {
        c.tbs_certificate()
            .extensions()
            .expect("extensions")
            .iter()
            .find(|e| e.extn_id == OID_SUBJECT_ALT_NAME)
            .map(|e| e.extn_value.as_bytes().to_vec())
            .expect("a subjectAltName")
    }

    #[test]
    fn a_certificate_whose_two_issuer_generations_disagree_is_refused_not_reconciled() {
        // .1.1 = accounts.google.com, .1.8 = evil.example.invalid. Both decode
        // cleanly, so no encoding check fires; before this refusal existed,
        // `issuer` answered "evil.example.invalid" and said nothing about the
        // value it had passed over.
        let c = cert("identity-issuer-disagree.pem");
        let err = issuer(&c).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains("two different OIDC issuers"), "{msg}");
        // Both values must be named. A refusal that hides the one it did not
        // pick leaves the reader unable to see which verifier saw what.
        assert!(msg.contains("https://accounts.google.com"), "{msg}");
        assert!(msg.contains("https://evil.example.invalid"), "{msg}");
        assert!(matches!(extract(&c), Err(Error::IdentityMalformed(_))));
    }

    #[test]
    fn the_dangerous_direction_is_the_one_where_the_legacy_value_is_the_hostile_one() {
        // .1.8 = accounts.google.com, .1.1 = evil.example.invalid. This build
        // prefers .1.8, so before the agreement check `extract` returned
        // ("nobody@example.invalid", Email, "https://accounts.google.com") and an
        // `IdentityPolicy { issuer: Some("https://accounts.google.com"), .. }`
        // PASSED — on a certificate that tells `sigstore` 0.14.0, whose OID table
        // stops at .1.7, that the issuer is evil.example.invalid.
        let c = cert("identity-issuer-disagree-legacy-hostile.pem");
        let err = issuer(&c).expect_err("must refuse");
        assert!(
            !matches!(err, Error::NoIssuer),
            "a certificate that names two issuers is not one that names none"
        );
        let msg = err.to_string();
        assert!(msg.contains("two different OIDC issuers"), "{msg}");
        // The value a policy would have matched must not come back as the answer.
        assert!(extract(&c).is_err());
    }

    #[test]
    fn preferring_one_issuer_generation_over_the_other_is_no_longer_load_bearing() {
        // Once agreement is enforced, the only certificate on which the
        // preference is observable is one that is refused. Reversing it used to
        // pass all 22 tests; this pins the property that made that possible.
        for name in [
            "identity-github-actions-leaf.pem",
            "identity-email-leaf.pem",
            "identity-legacy-only-leaf.pem",
        ] {
            let c = cert(name);
            let exts = extensions(&c).expect("decodes");
            if let (Some(v2), Some(legacy)) = (exts.get(OID_ISSUER_V2), exts.get(OID_ISSUER_LEGACY))
            {
                assert_eq!(v2, legacy, "{name} carries two generations that disagree");
            }
            assert!(issuer(&c).is_ok(), "{name}");
        }
    }

    #[test]
    fn an_issuer_extension_that_decodes_to_the_empty_string_names_nobody() {
        // .1.8 present, valid DER, and empty — `0c 00`. Returning `""` would
        // satisfy `IdentityPolicy { issuer: Some(String::new()), .. }` and print
        // as though the question had been answered. No fixture exercised this
        // guard: deleting it passed all 22 tests.
        let c = cert("identity-issuer-empty-only.pem");
        assert_eq!(
            extensions(&c).expect("decodes").get(OID_ISSUER_V2),
            Some(""),
            "the fixture must carry an empty .1.8, not a missing one"
        );
        assert!(matches!(issuer(&c), Err(Error::NoIssuer)));
        assert!(matches!(extract(&c), Err(Error::NoIssuer)));
    }

    #[test]
    fn a_map_emptied_by_skipping_everything_does_not_report_itself_as_empty() {
        // One Fulcio extension, and this build cannot decode it. `is_empty()`
        // used to answer `true` — "this certificate carries no Fulcio
        // constraints" — about a certificate carrying one we could not read.
        let c = cert("identity-only-unrecognized.pem");
        let exts = extensions(&c).expect("decodes what it knows");
        assert_eq!(exts.len(), 0, "nothing decoded");
        assert_eq!(exts.unrecognized(), ["1.3.6.1.4.1.57264.1.99"]);
        assert!(
            !exts.is_empty(),
            "a certificate carrying a Fulcio extension must not report as carrying none"
        );
        // And the honestly-empty case still reports empty.
        assert!(extensions(&cert("identity-fulcio-root.pem"))
            .expect("decodes")
            .is_empty());
    }

    #[test]
    fn a_subject_alt_name_holding_an_empty_list_is_not_vacuously_satisfied() {
        // `GeneralNames ::= SEQUENCE SIZE (1..MAX)`, so `30 00` is malformed —
        // but it decodes, and a loop over an empty list finishes without finding
        // anything wrong. The empty-collection-satisfies-everything shape.
        let c = cert("identity-san-empty-seq.pem");
        assert_eq!(
            san_bytes(&c),
            [0x30, 0x00],
            "fixture must hold an empty list"
        );
        assert!(matches!(
            subject_alternative_names(&c),
            Err(Error::NoIdentity)
        ));
        assert!(matches!(extract(&c), Err(Error::NoIdentity)));
        // The issuer is fine, so this isolates the SAN failure.
        assert_eq!(
            issuer(&c).expect("issuer present"),
            "https://accounts.google.com"
        );
    }

    #[test]
    fn a_subject_alt_name_that_does_not_decode_is_not_a_certificate_without_one() {
        // The extension is present and its GeneralNames are truncated. The
        // tempting shape — `Err(_) => Err(Error::NoIdentity)` — reports "the
        // certificate carries no subject alternative name" about a certificate
        // whose names could not be read. That mutation passed all 22 tests.
        let c = cert("identity-san-undecodable.pem");
        let err = subject_alternative_names(&c).expect_err("must refuse");
        assert!(
            !matches!(err, Error::NoIdentity),
            "a SAN that could not be read is not a SAN that was read and found empty: {err}"
        );
        let msg = err.to_string();
        assert!(msg.contains("2.5.29.17"), "{msg}");
        assert!(matches!(extract(&c), Err(Error::IdentityMalformed(_))));
    }

    #[test]
    fn an_othername_that_is_not_a_utf8string_is_refused_rather_than_read_for_its_bytes() {
        // Fulcio's otherName is `[0] EXPLICIT UTF8String`. This fixture holds
        // `[0] EXPLICIT OCTET STRING` whose content octets are still "jdoe", so a
        // reader that takes `Any::value()` and skips the tag returns the identity
        // "jdoe" from a structure Fulcio never emits. That mutation passed all 22
        // tests, because every otherName fixture was well-formed.
        let c = cert("identity-othername-octetstring.pem");
        let err = subject_alternative_names(&c).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains(OID_OTHERNAME_SAN), "{msg}");
        assert!(msg.contains("UTF8String"), "{msg}");
        assert!(
            !msg.contains("jdoe"),
            "the value read through the wrong tag must not be the answer: {msg}"
        );
        // And the bytes really are the ones the tag check is the only defence
        // against: an OCTET STRING (0x04) wrapping the same four characters.
        let raw = san_bytes(&c);
        assert!(
            raw.windows(6)
                .any(|w| w == [0x04, 0x04, b'j', b'd', b'o', b'e']),
            "{raw:02x?}"
        );
    }

    #[test]
    fn a_real_othername_leaf_from_another_implementation_decodes_the_same_way() {
        // The synthesized `identity-othername-leaf.pem` proves this module reads
        // an otherName the way this module writes one, which is worth nothing on
        // its own. This is sigstore-go's own `othername.sigstore.json` leaf —
        // their test CA, not production Fulcio, but an encoder nobody here wrote.
        let real = cert("identity-othername-real-sigstore-go.pem");
        let (san, kind, iss) = extract(&real).expect("names somebody");
        assert_eq!(san, "foo!oidc.local");
        assert_eq!(kind, SanKind::OtherName);
        assert_eq!(iss, "http://oidc.local:8080");

        // Byte for byte: SEQUENCE { [0] { OID .1.7, [0] { UTF8String } } }.
        let raw = san_bytes(&real);
        assert_eq!(&raw[..2], &[0x30, 0x20], "GeneralNames SEQUENCE");
        assert_eq!(raw[2], 0xa0, "otherName is GeneralName alternative [0]");
        assert_eq!(&raw[4..6], &[0x06, 0x0a], "type-id OBJECT IDENTIFIER");
        assert_eq!(raw[16], 0xa0, "value is [0] EXPLICIT");
        assert_eq!(raw[18], 0x0c, "and the explicit content is a UTF8String");
        assert!(raw.ends_with(b"foo!oidc.local"));

        // The synthesized fixture has the same tag skeleton, which is the claim
        // that was previously untested.
        let synth = san_bytes(&cert("identity-othername-leaf.pem"));
        assert_eq!(synth[0], raw[0]);
        assert_eq!(synth[2], raw[2]);
        assert_eq!(&synth[4..16], &raw[4..16], "same type-id encoding");
        assert_eq!(synth[16], raw[16]);
        assert_eq!(synth[18], raw[18]);

        // Fulcio namespaces a username identity as `user!issuer-domain`, which is
        // why the SanKind confusion below is hard to reach against the production
        // instance — a fact about Fulcio's configuration, not about this
        // verifier.
        assert!(san.contains('!'));
    }

    #[test]
    fn a_name_this_build_cannot_police_is_never_dropped_from_the_list_it_polices() {
        // rfc822Name = nobody@example.invalid, dNSName = evil.example.invalid.
        // The list returned here is the only way a caller can ask "does this
        // certificate name ONLY X", and it used to come back as
        // `[(Email, "nobody@example.invalid")]` — a one-element list that reads
        // as proof of "only", assembled by dropping the other name.
        let c = cert("identity-san-extra-dnsname.pem");
        let err = subject_alternative_names(&c).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains("dNSName:evil.example.invalid"), "{msg}");
        assert!(msg.contains("nobody@example.invalid"), "{msg}");
        assert!(
            !matches!(err, Error::NoIdentity),
            "the certificate does name someone; that is the problem"
        );
        assert!(extract(&c).is_err());

        // A certificate carrying *only* names we cannot police is still
        // NoIdentity — "I found nobody I can police" is the honest answer there,
        // and it stays on the identity axis.
        assert!(matches!(
            extract(&cert("identity-foreignother.pem")),
            Err(Error::NoIdentity)
        ));
    }

    #[test]
    fn two_names_of_the_same_kind_leave_the_second_visible_only_through_the_list() {
        // Precedence is between kinds; within a kind it is document order, which
        // matches sigstore-go's `cert.URIs[0]`. Nothing pinned that, so this
        // records what `extract` actually answers — and records that the second
        // URI is reachable, because a caller policing "only this workflow" has to
        // be able to see it.
        let c = cert("identity-san-two-uris.pem");
        let (san, kind, _) = extract(&c).expect("names somebody");
        assert_eq!(kind, SanKind::Uri);
        assert_eq!(
            san,
            "https://github.com/legit/repo/.github/workflows/w.yml@refs/heads/main"
        );
        let all = subject_alternative_names(&c).expect("names somebody");
        assert_eq!(all.len(), 2);
        assert_eq!(
            all[1].1,
            "https://github.com/evil/repo/.github/workflows/w.yml@refs/heads/main"
        );
    }

    #[test]
    fn a_common_name_that_looks_like_an_identity_is_not_one() {
        // Subject CN = thomas@python.org, no SAN. Verifiers that fall back to the
        // subject DN when the SAN is absent — the habit RFC 6125 spent a
        // specification talking browsers out of — would report this certificate
        // as naming a CPython release manager.
        let c = cert("identity-cn-only.pem");
        assert!(matches!(extract(&c), Err(Error::NoIdentity)));
        assert!(matches!(
            subject_alternative_names(&c),
            Err(Error::NoIdentity)
        ));
        // The CN really is there, so this test is not passing by accident.
        assert!(c
            .tbs_certificate()
            .subject()
            .to_string()
            .contains("thomas@python.org"));
    }

    #[test]
    fn the_othername_type_id_used_as_a_certificate_extension_is_not_an_identity() {
        // `1.3.6.1.4.1.57264.1.7` names an otherName *inside* the SAN. A
        // certificate that instead hangs it off the TBS as a top-level extension
        // is naming nobody by it — but it is a Fulcio-arc extension this build
        // does not decode, so it must show up as a gap rather than vanish.
        let c = cert("identity-othername-oid-as-extension.pem");
        let (san, kind, _) = extract(&c).expect("the rfc822Name is the identity");
        assert_eq!(san, "nobody@example.invalid");
        assert_eq!(kind, SanKind::Email);
        assert_eq!(
            extensions(&c).expect("decodes").unrecognized(),
            [OID_OTHERNAME_SAN]
        );
    }

    #[test]
    fn a_non_minimal_der_length_is_not_a_second_spelling_of_the_same_issuer() {
        // `.1.8` holding `0C 81 1B "https://accounts.google.com"` — the correct
        // string under a long-form length byte that DER forbids for a value this
        // short. A BER-tolerant reader accepts it, and the certificate then has
        // two byte encodings that mean the same thing, only one of which any
        // other verifier will reproduce.
        let c = cert("identity-issuer-longform-len.pem");
        let err = issuer(&c).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains(OID_ISSUER_V2), "{msg}");
        assert!(
            !msg.contains("https://accounts.google.com"),
            "the value read through a non-DER length must not be the answer: {msg}"
        );
    }

    #[test]
    fn an_email_shaped_othername_is_reported_as_an_othername_and_not_as_an_email() {
        // A Fulcio otherName whose value is the *string* "thomas@python.org". It
        // is a username that looks like an address, and it is not an rfc822Name.
        //
        // This module keeps them apart. `verify::verify` does not: it takes the
        // `SanKind` from `extract` and compares only the string against
        // `IdentityPolicy::san`, so a policy written for the rfc822Name
        // thomas@python.org is satisfied by an otherName of the same text. That
        // is a hole in `verify.rs`, not here, and it is pinned from this side so
        // the distinction cannot be lost from this side.
        let c = cert("identity-othername-email-shaped.pem");
        let (san, kind, _) = extract(&c).expect("names somebody");
        assert_eq!(san, "thomas@python.org");
        assert_eq!(kind, SanKind::OtherName);
        assert_ne!(kind, SanKind::Email);
        assert_eq!(kind.as_str(), "otherName:1.3.6.1.4.1.57264.1.7");
        assert_eq!(
            subject_alternative_names(&c).expect("names somebody"),
            vec![(SanKind::OtherName, "thomas@python.org".to_string())]
        );
    }

    #[test]
    fn no_hostile_byte_mutation_of_a_real_leaf_panics_this_module() {
        // Every function here runs on a certificate whose chain has not
        // necessarily been checked yet, so a panic on malformed input is a
        // denial of service reachable by anyone who can hand a verifier a
        // bundle. Deterministic xorshift, so a failure is reproducible from the
        // seed printed in this source.
        use der::Encode;
        struct R(u64);
        impl R {
            fn next(&mut self) -> u64 {
                let mut x = self.0;
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                self.0 = x;
                x
            }
        }
        let mut parsed = 0u64;
        for name in [
            "identity-github-actions-leaf.pem",
            "identity-othername-real-sigstore-go.pem",
            "identity-email-leaf.pem",
        ] {
            let der = cert(name).to_der().expect("re-encodes");
            let mut r = R(0x9E37_79B9_7F4A_7C15);
            for _ in 0..6_000 {
                let mut d = der.clone();
                for _ in 0..=(r.next() % 4) {
                    let i = (r.next() as usize) % d.len();
                    d[i] ^= (r.next() % 255 + 1) as u8;
                }
                let Ok(c) = x509_cert::Certificate::from_der(&d) else {
                    continue;
                };
                parsed += 1;
                let _ = extract(&c);
                let _ = subject_alternative_names(&c);
                let _ = issuer(&c);
                let _ = extensions(&c).map(|e| {
                    (
                        e.len(),
                        e.is_empty(),
                        e.unrecognized().len(),
                        e.iter().count(),
                    )
                });
            }
        }
        // Without this the test would pass just as happily if every mutant had
        // been rejected by the certificate parser before reaching this module.
        assert!(
            parsed > 1_000,
            "only {parsed} mutants reached this module; the fuzz proved nothing"
        );
    }
}
