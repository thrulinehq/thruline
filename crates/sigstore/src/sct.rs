//! Embedded Signed Certificate Timestamps (RFC 6962 §3.2).
//!
//! The SCT lives in the leaf's extension **1.3.6.1.4.1.11129.2.4.2**. Verifying
//! it means reconstructing the *precertificate* the log actually signed:
//! take the leaf's TBSCertificate, **remove the SCT extension itself**, and
//! serialize a `PreCert` entry carrying the issuer's public-key hash.
//!
//! Getting the reconstruction wrong produces a signature that never verifies,
//! which is indistinguishable from a forged certificate — so this has its own
//! fixtures rather than only being exercised end to end. [`precert_tbs_der`]
//! and [`signature_input`] are public for exactly that reason: the two
//! intermediate blobs are checked byte-for-byte against files produced by an
//! independent Python implementation, so a failure says *which* of the two
//! steps went wrong instead of only "the signature did not verify".
//!
//! # `issuer_key_hash` is over the whole SubjectPublicKeyInfo
//!
//! RFC 6962 §3.2: *"the SHA-256 hash of the certificate issuer's public key,
//! calculated over the DER encoding of the key represented as
//! SubjectPublicKeyInfo"*. That is the **complete `SEQUENCE`** — algorithm
//! identifier, curve parameters and BIT STRING — not the BIT STRING's contents
//! and not the raw EC point. The two are easy to confuse and produce hashes
//! that differ in every byte; for the live Fulcio intermediate they are
//! `c355ee53…` (correct) and `00c71d31…` (the raw point). Both hash to
//! something that looks fine and only one verifies.
//!
//! # `CtExtensions` is not always empty
//!
//! RFC 6962 v1 defines no extensions and most SCTs carry none, so a verifier
//! that hard-codes a zero length passes its own tests. It then fails on every
//! certificate logged by a Static CT API ("tiled") log, which stashes the
//! leaf index there: the staging Fulcio fixture in `fixtures/sct` carries
//! `00 0005 00003a7c0d`, and one of the three SCTs on the web-PKI fixture
//! carries `00 0005 0160689f`. The bytes are opaque here and are copied into
//! the signed blob verbatim.
//!
//! # What this module does not decide
//!
//! It does **not** map an SCT's `log_id` to a key — the caller supplies the
//! key. That is deliberate: `log_id` is defined as SHA-256 of the log's DER
//! SubjectPublicKeyInfo, but trust-root documents do not all store the key in
//! that encoding. Sigstore's staging `trusted_root.json` ships the
//! `ctfe.sigstage.dev/test` log's key as a bare PKCS#1 `RSAPublicKey`
//! (`30 82 02 0a 02 82 …`), so `sha256(rawBytes)` is `b3d00e6f…` while the
//! document's own `logId.keyId` is `1b7c142a…`; re-wrapping it as a
//! SubjectPublicKeyInfo reproduces `1b7c142a…` exactly. A `log_id ==
//! sha256(key)` assertion here would reject that log while looking like a
//! security check. Selecting the key belongs to one place, and this is not it.
//!
//! Nor does it handle a **Precertificate Signing Certificate** (RFC 6962 §3.1,
//! EKU 1.3.6.1.4.1.11129.2.4.4). When one is used, `issuer_key_hash` is the
//! hash of the CA *above* it rather than of the signing certificate, so
//! passing the wrong one as `issuer` yields [`Error::SctInvalid`] — which reads
//! as forgery. Fulcio has never used one; if that changes, the caller must
//! pick the right issuer, and this note is the record that nothing here does
//! it for them.
//!
//! # Why the TLS decoding is written out rather than delegated
//!
//! `x509-cert` 0.3.0 does ship this: `ext::pkix::sct`, behind a non-default
//! `sct` feature that pulls in `tls_codec` and its proc-macro. Two things
//! argued against turning it on. The manifest is pinned at a resolved
//! dependency set that `crates/attest` depends on, and this module is a hundred
//! lines of big-endian length-prefixed fields. More to the point, its
//! `HashAlgorithm`/`SignatureAlgorithm`/`Version` are strict enums, so an SCT
//! naming an algorithm outside the set fails to *decode*; here the raw bytes
//! are preserved and [`verify`] refuses them by name, which is the difference
//! between "this certificate is malformed" and "this build cannot check that
//! signature". Its own test vectors are reused below to cross-check this
//! parser against the upstream one.

use const_oid::db::rfc5912::{ID_EC_PUBLIC_KEY, SECP_256_R_1};
use const_oid::db::rfc6962::CT_PRECERT_SCTS;
use const_oid::ObjectIdentifier;
use der::asn1::OctetString;
use der::{Decode, Encode};
use sha2::{Digest, Sha256};
use x509_cert::Certificate;

use crate::error::{Error, Result};
use crate::x509::verify_ecdsa_spki;

pub const OID_SCT_LIST: &str = "1.3.6.1.4.1.11129.2.4.2";

/// RFC 6962 `Version.v1`. There is no v2, and the field layout that follows
/// this byte is only defined for v1 — see [`parse_list`].
const SCT_VERSION_V1: u8 = 0;
/// RFC 6962 `SignatureType.certificate_timestamp`.
const SIGNATURE_TYPE_CERTIFICATE_TIMESTAMP: u8 = 0;
/// RFC 6962 `LogEntryType.precert_entry`. An *embedded* SCT is always a precert
/// entry; `x509_entry` (0) is for the detached SCT Fulcio returns alongside a
/// certificate it did not log.
const LOG_ENTRY_TYPE_PRECERT: u16 = 1;
/// TLS `HashAlgorithm.sha256` (RFC 5246 §7.4.1.4.1).
const HASH_ALGORITHM_SHA256: u8 = 4;
/// TLS `SignatureAlgorithm.ecdsa`.
const SIGNATURE_ALGORITHM_ECDSA: u8 = 3;

const TAG_SEQUENCE: u8 = 0x30;
const TAG_OID: u8 = 0x06;
/// `extensions [3] EXPLICIT Extensions OPTIONAL` in a TBSCertificate.
const TAG_EXTENSIONS: u8 = 0xa3;

/// A `TBSCertificate` is carried in a 3-byte length field, so the precert entry
/// cannot describe one larger than this.
const MAX_TBS_LEN: usize = 0xff_ffff;

#[derive(Debug, Clone)]
pub struct Sct {
    /// Always [`SCT_VERSION_V1`] for anything this module produced: a version
    /// byte it does not recognize means the rest of the record has an unknown
    /// layout, and parsing it as v1 would be inventing structure. Kept as a
    /// field rather than assumed, because it is echoed into the signed blob.
    pub version: u8,
    pub log_id: [u8; 32],
    pub timestamp_ms: u64,
    /// Opaque `CtExtensions`, copied through verbatim. Frequently empty, but
    /// see the module docs — Static CT API logs put a leaf index here.
    pub extensions: Vec<u8>,
    pub hash_algorithm: u8,
    pub signature_algorithm: u8,
    /// ASN.1 DER `ECDSA-Sig-Value`, as the CT log emitted it.
    pub signature: Vec<u8>,
}

impl Sct {
    /// The log id as lowercase hex — the spelling
    /// [`Error::SctUnknownLog`] and Sigstore's trust-root documents use.
    pub fn log_id_hex(&self) -> String {
        hex::encode(self.log_id)
    }
}

/// Extract every SCT embedded in the certificate.
///
/// Returns all of them. `sigstore` 0.14.0 accepts **exactly one** and rejects a
/// certificate carrying two, which is legal and does occur:
/// `crypto/transparency.rs` matches `[e] => e` and returns
/// `CertificateErrorKind::LeafSCTMissing` for every other length — so a
/// certificate with three SCTs is reported as having none. That is not a corner
/// case in the wider web PKI: the `webpki-three-scts-leaf.pem` fixture is a
/// live server certificate with three, all three of which verify here.
pub fn extract(cert: &Certificate) -> Result<Vec<Sct>> {
    let exts = cert
        .tbs_certificate()
        .extensions()
        .ok_or(Error::SctMissing)?;

    let mut matching = exts.iter().filter(|e| e.extn_id == CT_PRECERT_SCTS);
    let ext = matching.next().ok_or(Error::SctMissing)?;
    if matching.next().is_some() {
        // RFC 5280 §4.2: "A certificate MUST NOT include more than one
        // instance of a particular extension." Picking the first would mean
        // guessing which one the log signed, and the precertificate
        // reconstruction removes *all* of them, so the two would disagree.
        return Err(Error::SctMalformed(format!(
            "the certificate carries more than one {OID_SCT_LIST} extension; RFC 5280 §4.2 \
             permits one instance of an extension and which of them the log signed cannot be \
             determined"
        )));
    }

    // Two layers of OCTET STRING, not one. `extnValue` is an OCTET STRING whose
    // contents are "the DER encoding of an ASN.1 value corresponding to the
    // extension type" (RFC 5280 §4.1.2.9), and for this extension that value is
    // itself `SignedCertificateTimestampList ::= OCTET STRING` (RFC 6962 §3.3).
    // Reading only the outer one leaves a `04 81 f2 …` header in front of the
    // TLS list, and the total-length field then parses as garbage.
    let inner = OctetString::from_der(ext.extn_value.as_bytes()).map_err(|e| {
        Error::SctMalformed(format!(
            "the extension value is not a DER OCTET STRING wrapping a \
             SignedCertificateTimestampList: {e}"
        ))
    })?;

    parse_list(inner.as_bytes())
}

/// Parse a TLS `SignedCertificateTimestampList` body.
///
/// The argument is the list itself — a 2-byte total length followed by
/// 2-byte-prefixed SCTs — not the DER wrapper [`extract`] peels off first.
///
/// An **empty list is refused** as [`Error::SctMissing`] rather than returned
/// as `Ok(vec![])`. A caller that loops "verify each SCT" over an empty vector
/// concludes that all of them verified; that is the same vacuous-success shape
/// [`Error::NoSignatures`] exists to prevent one axis over.
pub fn parse_list(tls_list: &[u8]) -> Result<Vec<Sct>> {
    let mut c = Cursor::new(tls_list, "SignedCertificateTimestampList");
    let declared = c.u16()? as usize;
    if c.remaining() != declared {
        return Err(Error::SctMalformed(format!(
            "the SCT list declares {declared} bytes but {} follow its length prefix",
            c.remaining()
        )));
    }

    let mut out = Vec::new();
    while c.remaining() > 0 {
        let one = c.opaque_u16()?;
        out.push(parse_one(one)?);
    }
    if out.is_empty() {
        return Err(Error::SctMissing);
    }
    Ok(out)
}

fn parse_one(bytes: &[u8]) -> Result<Sct> {
    let mut c = Cursor::new(bytes, "SignedCertificateTimestamp");
    let version = c.u8()?;
    if version != SCT_VERSION_V1 {
        // Everything after this byte — the log id, the timestamp, the
        // DigitallySigned — is only defined for v1. Decoding it as v1 anyway
        // would hand `verify` a struct assembled out of the wrong fields.
        return Err(Error::SctMalformed(format!(
            "SCT version is {version}; only RFC 6962 v1 (0) has a defined layout, so the rest of \
             this record was not decoded"
        )));
    }
    let log_id: [u8; 32] = c
        .take(32)?
        .try_into()
        .map_err(|_| Error::SctMalformed("the log id is not 32 bytes".into()))?;
    let timestamp_ms = c.u64()?;
    let extensions = c.opaque_u16()?.to_vec();
    let hash_algorithm = c.u8()?;
    let signature_algorithm = c.u8()?;
    let signature = c.opaque_u16()?.to_vec();
    if c.remaining() != 0 {
        return Err(Error::SctMalformed(format!(
            "{} bytes remain after the SCT's signature; the record's own length prefix disagrees \
             with its contents",
            c.remaining()
        )));
    }
    Ok(Sct {
        version,
        log_id,
        timestamp_ms,
        extensions,
        hash_algorithm,
        signature_algorithm,
        signature,
    })
}

/// `SHA-256` over the issuer's **complete DER SubjectPublicKeyInfo**.
///
/// See the module docs: the BIT STRING contents are the wrong input and hash to
/// something that looks equally plausible.
pub fn issuer_key_hash(issuer: &Certificate) -> Result<[u8; 32]> {
    let spki = issuer
        .tbs_certificate()
        .subject_public_key_info()
        .to_der()
        .map_err(|e| {
            Error::SctMalformed(format!(
                "re-encoding the issuer's SubjectPublicKeyInfo: {e}"
            ))
        })?;
    Ok(Sha256::digest(spki).into())
}

/// The leaf's TBSCertificate with the SCT list extension removed, in DER.
///
/// This is the precertificate the log signed, and it is built by **byte
/// surgery** on the re-encoded TBS rather than by rebuilding the structure:
/// every byte outside the removed extension is carried across untouched, and
/// only the four enclosing lengths — the Extension list, its `[3]` wrapper and
/// the outer `SEQUENCE` — are recomputed. A field-by-field re-encode would
/// silently re-normalize anything the parser chose to normalize, and the first
/// evidence of that would be a signature that does not verify.
///
/// `x509-cert` 0.3.0 makes `TbsCertificateInner::extensions` `pub(crate)`, so
/// `sigstore` 0.14.0's approach — clone the TBS, filter the extension vector,
/// re-encode — is not available from outside the crate in any case. Its
/// version of this step also ends in
/// `.expect("failed to re-encode Precertificate!")`.
pub fn precert_tbs_der(leaf: &Certificate) -> Result<Vec<u8>> {
    let tbs = leaf
        .tbs_certificate()
        .to_der()
        .map_err(|e| Error::SctMalformed(format!("re-encoding the TBSCertificate: {e}")))?;
    strip_sct_extension(&tbs)
}

/// The exact bytes the CT log signed: RFC 6962 §3.2's `CertificateTimestamp`,
/// TLS-serialized.
///
/// ```text
/// version(1) ‖ signature_type(1) ‖ timestamp(8) ‖ entry_type(2)
///   ‖ issuer_key_hash(32) ‖ tbs_certificate(3-byte length ‖ bytes)
///   ‖ extensions(2-byte length ‖ bytes)
/// ```
pub fn signature_input(sct: &Sct, leaf: &Certificate, issuer: &Certificate) -> Result<Vec<u8>> {
    let issuer_key_hash = issuer_key_hash(issuer)?;
    let pre_tbs = precert_tbs_der(leaf)?;
    if pre_tbs.len() > MAX_TBS_LEN {
        return Err(Error::SctMalformed(format!(
            "the precertificate TBSCertificate is {} bytes, beyond the {MAX_TBS_LEN} a \
             3-byte length field can describe",
            pre_tbs.len()
        )));
    }
    let ext_len = u16::try_from(sct.extensions.len()).map_err(|_| {
        Error::SctMalformed(format!(
            "the SCT's CtExtensions are {} bytes, beyond what a 2-byte length field can describe",
            sct.extensions.len()
        ))
    })?;

    let mut out = Vec::with_capacity(47 + pre_tbs.len() + sct.extensions.len());
    out.push(sct.version);
    out.push(SIGNATURE_TYPE_CERTIFICATE_TIMESTAMP);
    out.extend_from_slice(&sct.timestamp_ms.to_be_bytes());
    out.extend_from_slice(&LOG_ENTRY_TYPE_PRECERT.to_be_bytes());
    out.extend_from_slice(&issuer_key_hash);
    out.extend_from_slice(&(pre_tbs.len() as u32).to_be_bytes()[1..]);
    out.extend_from_slice(&pre_tbs);
    out.extend_from_slice(&ext_len.to_be_bytes());
    out.extend_from_slice(&sct.extensions);
    Ok(out)
}

/// Verify one SCT against a CT log's DER SubjectPublicKeyInfo.
///
/// `issuer` must be the CA that issued `leaf` — its public key is what binds
/// the SCT to this certificate rather than to some other one with the same
/// contents.
///
/// The algorithms the SCT declares are checked before the signature, and a pair
/// this build cannot honour is refused **by name** instead of being verified
/// with whatever digest happens to be handy: [`crate::x509::verify_ecdsa_spki`]
/// derives the digest from the key's curve, so handing it a P-384 CT log key
/// would silently check a SHA-384 signature against an SCT that declared
/// SHA-256 and report the mismatch as [`Error::SctInvalid`] — a forgery claim
/// about an honest certificate.
pub fn verify(
    sct: &Sct,
    leaf: &Certificate,
    issuer: &Certificate,
    log_spki_der: &[u8],
) -> Result<()> {
    if sct.version != SCT_VERSION_V1 {
        return Err(Error::SctMalformed(format!(
            "SCT version is {}; the signed blob's layout is defined for v1 (0) only, so nothing \
             was verified",
            sct.version
        )));
    }
    if sct.signature_algorithm != SIGNATURE_ALGORITHM_ECDSA
        || sct.hash_algorithm != HASH_ALGORITHM_SHA256
    {
        return Err(Error::SctMalformed(format!(
            "the SCT declares TLS hash algorithm {} with signature algorithm {}; this build \
             verifies ECDSA over SHA-256 ({HASH_ALGORITHM_SHA256}/{SIGNATURE_ALGORITHM_ECDSA}) \
             only, so the signature was NOT checked",
            sct.hash_algorithm, sct.signature_algorithm
        )));
    }
    require_p256(log_spki_der)?;
    require_der_ecdsa_signature(&sct.signature)?;

    let blob = signature_input(sct, leaf, issuer)?;
    match verify_ecdsa_spki(log_spki_der, &blob, &sct.signature) {
        Ok(()) => Ok(()),
        // Only a genuine "this signature does not verify" becomes SctInvalid.
        // A key that could not be *used* is a different fact and must never be
        // reported as a bad SCT.
        Err(Error::SignatureInvalid) => Err(Error::SctInvalid),
        // It must not be re-raised unchanged either, which is what this arm
        // used to do. `require_p256` above only proves the SubjectPublicKeyInfo
        // is *shaped* like a P-256 key: the algorithm OID, the curve OID, the
        // outer SEQUENCE. The EC point inside the BIT STRING is not decoded
        // until `verify_ecdsa_spki`, and when that fails it returns
        // `Error::CertificateParse`, which reads *"the leaf certificate could
        // not be parsed"* and sits on `Axis::Chain`.
        //
        // Measured, not imagined: flipping the last two bytes of the real
        // `ctfe.sigstore.dev/2022` SubjectPublicKeyInfo — a plausible outcome
        // of a truncated or corrupted `trusted_root.json` — made this function
        // answer "the leaf certificate could not be parsed: P-256 key: ASN.1
        // error: malformed ASN.1 DER value for BIT STRING" on the chain axis,
        // about a leaf that had parsed perfectly. A broken trust root came out
        // of the verifier looking like a broken artifact, on the wrong axis,
        // naming the wrong object — the exact confusion `error.rs` says it
        // exists to prevent.
        //
        // `verify_ecdsa_spki` answers with exactly three things: `Ok`,
        // `SignatureInvalid`, and `CertificateParse`. Only the *reason* inside
        // the last one is carried across — quoting its Display would drag
        // "the leaf certificate could not be parsed" along with it and put the
        // false sentence back inside the true one.
        Err(Error::CertificateParse(why)) => Err(Error::SctMalformed(format!(
            "the certificate transparency log's public key could not be used ({why}), so the \
             SCT signature was NOT checked"
        ))),
        // Unreachable against `x509` as it stands, and written anyway: if that
        // function grows a fourth answer, it arrives on the SCT axis saying it
        // did not check, rather than on whichever axis it happened to carry.
        Err(other) => Err(Error::SctMalformed(format!(
            "the SCT signature could not be checked against the certificate transparency log's \
             key: {other}"
        ))),
    }
}

/// RFC 6962 §3.2 carries the SCT signature in a TLS `digitally-signed` struct,
/// whose contents for ECDSA are an **ASN.1 DER `ECDSA-Sig-Value`** — never the
/// fixed-width `r ‖ s` form.
///
/// [`crate::x509::verify_ecdsa_spki`] accepts both on purpose, because bundle
/// signatures do arrive fixed-width. An SCT does not: all **59** ECDSA SCTs in
/// the certificate corpus under `fixtures/` and the recon capture — Sigstore
/// production and staging, plus Sectigo, Cloudflare, Google, DigiCert and
/// Let's Encrypt web-PKI logs — are DER, at 70, 71 or 72 bytes. Not one is 64.
///
/// Without this check a genuine SCT signature re-encoded fixed-width verified
/// as the same SCT, so one log signature had two accepted spellings. Nothing
/// forged gets in that way — the SCT bytes sit inside the TBSCertificate and a
/// CA signature covers them — but "either encoding will do" is the shape this
/// module refuses everywhere else, and the narrower rule is the one RFC 6962
/// actually states.
///
/// The strict parse is `p256`'s own, the same one that will do the real work a
/// few lines later, rather than a second hand-written DER walk that could
/// disagree with it.
fn require_der_ecdsa_signature(sig: &[u8]) -> Result<()> {
    if p256::ecdsa::DerSignature::try_from(sig).is_ok() {
        return Ok(());
    }
    Err(Error::SctMalformed(format!(
        "the SCT's signature is not a DER ECDSA-Sig-Value; RFC 6962 §3.2 encodes an ECDSA \
         signature as DER and never as the fixed-width r‖s form, so the signature was NOT \
         checked ({} bytes, beginning 0x{:02x})",
        sig.len(),
        sig.first().copied().unwrap_or(0)
    )))
}

/// The CT log's key must be ECDSA P-256, because that is the only curve for
/// which `verify_ecdsa_spki`'s curve-derived digest is the SHA-256 the SCT
/// declares. Sigstore's trust roots name `PKIX_ECDSA_P256_SHA_256` for every
/// live CT log; the one exception, staging's retired `ctfe.sigstage.dev/test`,
/// is RSA and is refused here rather than mis-verified.
fn require_p256(log_spki_der: &[u8]) -> Result<()> {
    let spki = spki::SubjectPublicKeyInfoRef::from_der(log_spki_der).map_err(|e| {
        Error::SctMalformed(format!(
            "the certificate transparency log's SubjectPublicKeyInfo could not be read, so the \
             SCT signature was NOT checked: {e}"
        ))
    })?;
    if spki.algorithm.oid != ID_EC_PUBLIC_KEY {
        return Err(Error::SctMalformed(format!(
            "the certificate transparency log's key algorithm is {}, not id-ecPublicKey; this \
             build verifies ECDSA SCTs only, so the signature was NOT checked",
            spki.algorithm.oid
        )));
    }
    let named: ObjectIdentifier = spki
        .algorithm
        .parameters
        .ok_or_else(|| {
            Error::SctMalformed(
                "the certificate transparency log's EC key names no curve, so the digest to \
                 verify with is unknown and the signature was NOT checked"
                    .into(),
            )
        })?
        .decode_as()
        .map_err(|e| {
            Error::SctMalformed(format!(
                "the certificate transparency log's EC curve parameters: {e}"
            ))
        })?;
    if named != SECP_256_R_1 {
        return Err(Error::SctMalformed(format!(
            "the certificate transparency log's key is on curve {named}, not P-256; RFC 6962 \
             binds the digest to the SCT's own hash algorithm and this build would use the \
             curve's, so the signature was NOT checked"
        )));
    }
    Ok(())
}

// --- DER surgery -----------------------------------------------------------

/// One DER element: `(tag, value_start, value_len, element_end)`.
fn read_tlv(buf: &[u8], off: usize) -> Result<(u8, usize, usize, usize)> {
    let tag = *buf
        .get(off)
        .ok_or_else(|| der_err(format!("a DER tag was expected at offset {off}")))?;
    if tag & 0x1f == 0x1f {
        return Err(der_err(format!(
            "a multi-byte ASN.1 tag at offset {off}; nothing in a TBSCertificate uses one"
        )));
    }
    let first = *buf
        .get(off + 1)
        .ok_or_else(|| der_err(format!("a DER length was expected at offset {}", off + 1)))?;
    let (len, header) = if first & 0x80 == 0 {
        (first as usize, 2)
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 {
            return Err(der_err(format!(
                "an indefinite-length encoding at offset {off}, which DER forbids"
            )));
        }
        if n > 4 {
            return Err(der_err(format!(
                "a {n}-byte length field at offset {off}, larger than any certificate needs"
            )));
        }
        let raw = buf
            .get(off + 2..off + 2 + n)
            .ok_or_else(|| der_err(format!("a truncated length field at offset {off}")))?;
        let mut v = 0usize;
        for b in raw {
            v = (v << 8) | *b as usize;
        }
        (v, 2 + n)
    };
    let value_start = off + header;
    let end = value_start
        .checked_add(len)
        .ok_or_else(|| der_err(format!("a length that overflows at offset {off}")))?;
    if end > buf.len() {
        return Err(der_err(format!(
            "an element at offset {off} claiming {len} bytes with only {} left",
            buf.len().saturating_sub(value_start)
        )));
    }
    Ok((tag, value_start, len, end))
}

/// Minimal definite-form DER length, which is the only form DER permits.
fn der_len(n: usize) -> Vec<u8> {
    if n < 0x80 {
        return vec![n as u8];
    }
    let be = n.to_be_bytes();
    let first = be.iter().position(|b| *b != 0).unwrap_or(be.len() - 1);
    let body = &be[first..];
    let mut out = Vec::with_capacity(1 + body.len());
    out.push(0x80 | body.len() as u8);
    out.extend_from_slice(body);
    out
}

fn der_err(what: String) -> Error {
    Error::SctMalformed(format!("the TBSCertificate contains {what}"))
}

/// Remove the SCT list extension from a DER TBSCertificate.
///
/// Refuses rather than guesses when the shape is not what RFC 5280 describes:
/// two `[3]` wrappers, an extensions block that is not the last element, an
/// `Extension` that does not begin with an OBJECT IDENTIFIER, or more than one
/// SCT extension. Each of those would otherwise produce a plausible-looking
/// blob that simply fails to verify.
fn strip_sct_extension(tbs: &[u8]) -> Result<Vec<u8>> {
    let (tag, body_start, body_len, end) = read_tlv(tbs, 0)?;
    if tag != TAG_SEQUENCE {
        return Err(der_err(format!(
            "tag 0x{tag:02x} where the outer SEQUENCE should be"
        )));
    }
    if end != tbs.len() {
        return Err(der_err(format!(
            "{} trailing bytes after the outer SEQUENCE",
            tbs.len() - end
        )));
    }
    let body = &tbs[body_start..body_start + body_len];

    // Find `extensions [3]`.
    let mut p = 0usize;
    let mut found: Option<(usize, usize, usize, usize)> = None;
    while p < body.len() {
        let (t, vs, vl, e) = read_tlv(body, p)?;
        if t == TAG_EXTENSIONS {
            if found.is_some() {
                return Err(der_err("two [3] EXPLICIT extension blocks".into()));
            }
            found = Some((p, vs, vl, e));
        }
        p = e;
    }
    let (a3_start, a3_value, a3_len, a3_end) = found.ok_or(Error::SctMissing)?;
    if a3_end != body.len() {
        // Everything after the extensions block would be dropped by the splice
        // below. RFC 5280 puts extensions last, so this is a refusal and not a
        // reshuffle.
        return Err(der_err(
            "elements after the extensions block, which RFC 5280 places last".into(),
        ));
    }

    let (seq_tag, seq_value, _, seq_end) = read_tlv(body, a3_value)?;
    if seq_tag != TAG_SEQUENCE {
        return Err(der_err(format!(
            "tag 0x{seq_tag:02x} inside the [3] wrapper where a SEQUENCE OF Extension should be"
        )));
    }
    if seq_end != a3_value + a3_len {
        return Err(der_err(
            "trailing bytes inside the [3] extensions wrapper".into(),
        ));
    }
    let seq = &body[seq_value..seq_end];

    let mut kept = Vec::with_capacity(seq.len());
    let mut removed = 0usize;
    let mut q = 0usize;
    while q < seq.len() {
        let (ext_tag, ext_value, _, ext_end) = read_tlv(seq, q)?;
        if ext_tag != TAG_SEQUENCE {
            return Err(der_err(format!(
                "an extension with tag 0x{ext_tag:02x} rather than SEQUENCE"
            )));
        }
        // Read the OID inside **this** extension's bounds. `read_tlv(seq, ..)`
        // is bounded by the whole extension list, so an Extension whose own
        // length stopped short of its OID would have that OID read out of the
        // bytes belonging to the extension *after* it — and the OID is the only
        // thing deciding which extension gets removed. Slicing to `ext_end`
        // first makes the refusal deliberate rather than an accident of the
        // walk desynchronising two extensions later. No re-encoded
        // TBSCertificate can be shaped this way, which is exactly why the bound
        // has to be written down instead of relied upon.
        let (oid_tag, oid_value, oid_len, _) = read_tlv(&seq[..ext_end], ext_value)?;
        if oid_tag != TAG_OID {
            return Err(der_err(
                "an extension that does not begin with an OBJECT IDENTIFIER".into(),
            ));
        }
        if &seq[oid_value..oid_value + oid_len] == CT_PRECERT_SCTS.as_bytes() {
            removed += 1;
        } else {
            kept.extend_from_slice(&seq[q..ext_end]);
        }
        q = ext_end;
    }
    if removed == 0 {
        return Err(Error::SctMissing);
    }
    if removed > 1 {
        return Err(der_err(format!(
            "{removed} SCT list extensions, so which one the log signed cannot be determined"
        )));
    }

    // The extensions vector can legally end up empty only for a certificate
    // whose sole extension was the SCT list, which no CA issues. `Extensions
    // ::= SEQUENCE SIZE (1..MAX)` would make that non-conformant; an empty
    // SEQUENCE is emitted anyway, matching what `sigstore` 0.14.0 and
    // certificate-transparency-go produce, because interoperating with the
    // logs matters more here than refusing a certificate that does not exist.
    let mut inner = Vec::with_capacity(kept.len() + 6);
    inner.push(TAG_SEQUENCE);
    inner.extend_from_slice(&der_len(kept.len()));
    inner.extend_from_slice(&kept);

    let mut wrapper = Vec::with_capacity(inner.len() + 6);
    wrapper.push(TAG_EXTENSIONS);
    wrapper.extend_from_slice(&der_len(inner.len()));
    wrapper.extend_from_slice(&inner);

    let mut new_body = Vec::with_capacity(a3_start + wrapper.len());
    new_body.extend_from_slice(&body[..a3_start]);
    new_body.extend_from_slice(&wrapper);

    let mut out = Vec::with_capacity(new_body.len() + 6);
    out.push(TAG_SEQUENCE);
    out.extend_from_slice(&der_len(new_body.len()));
    out.extend_from_slice(&new_body);
    Ok(out)
}

// --- TLS cursor ------------------------------------------------------------

/// A bounds-checked reader over TLS-serialized bytes.
///
/// Every read is fallible and names the structure it was reading, because the
/// alternative — slicing and letting a panic stand in for a parse error — turns
/// a malformed certificate into a crash in a verifier.
struct Cursor<'a> {
    buf: &'a [u8],
    at: usize,
    what: &'static str,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8], what: &'static str) -> Self {
        Cursor { buf, at: 0, what }
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.at
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or_else(|| self.short(n))?;
        if end > self.buf.len() {
            return Err(self.short(n));
        }
        let out = &self.buf[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn short(&self, n: usize) -> Error {
        Error::SctMalformed(format!(
            "{}: {n} bytes were needed at offset {} but only {} remain",
            self.what,
            self.at,
            self.remaining()
        ))
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_be_bytes(a))
    }

    /// `opaque x<0..2^16-1>`: a 2-byte length followed by that many bytes.
    fn opaque_u16(&mut self) -> Result<&'a [u8]> {
        let n = self.u16()? as usize;
        self.take(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use der::DecodePem;

    // --- real fixtures -----------------------------------------------------
    //
    // `fulcio-*` are live Fulcio certificates captured 2026-09-04 and their
    // issuing intermediates; the `.der`/`.bin` files beside them are the
    // precertificate TBS and the signed blob as produced by an independent
    // Python implementation that removes the extension by byte-preserving
    // surgery. `webpki-three-scts-*` is a live www.python.org server
    // certificate with three SCTs from three different operators.

    const PROD_LEAF: &[u8] = include_bytes!("../fixtures/sct/fulcio-prod-leaf.pem");
    const PROD_ISSUER: &[u8] = include_bytes!("../fixtures/sct/fulcio-prod-intermediate.pem");
    const PROD_CTLOG: &[u8] =
        include_bytes!("../fixtures/sct/ctlog-sigstore-prod-ctfe2022.spki.der");
    const PROD_PRECERT_TBS: &[u8] = include_bytes!("../fixtures/sct/fulcio-prod-precert-tbs.der");
    const PROD_SIG_INPUT: &[u8] =
        include_bytes!("../fixtures/sct/fulcio-prod-sct-signature-input.bin");

    const STAGING_LEAF: &[u8] = include_bytes!("../fixtures/sct/fulcio-staging-leaf.pem");
    const STAGING_ISSUER: &[u8] = include_bytes!("../fixtures/sct/fulcio-staging-intermediate.pem");
    const STAGING_CTLOG: &[u8] =
        include_bytes!("../fixtures/sct/ctlog-sigstage-log2026-1.spki.der");
    const STAGING_PRECERT_TBS: &[u8] =
        include_bytes!("../fixtures/sct/fulcio-staging-precert-tbs.der");
    const STAGING_SIG_INPUT: &[u8] =
        include_bytes!("../fixtures/sct/fulcio-staging-sct-signature-input.bin");

    const WEBPKI_LEAF: &[u8] = include_bytes!("../fixtures/sct/webpki-three-scts-leaf.pem");
    const WEBPKI_ISSUER: &[u8] = include_bytes!("../fixtures/sct/webpki-three-scts-issuer.pem");
    const CTLOG_SECTIGO: &[u8] =
        include_bytes!("../fixtures/sct/ctlog-sectigo-elephant2027h1.spki.der");
    const CTLOG_CLOUDFLARE: &[u8] =
        include_bytes!("../fixtures/sct/ctlog-cloudflare-nimbus2027.spki.der");
    const CTLOG_LETSENCRYPT: &[u8] =
        include_bytes!("../fixtures/sct/ctlog-letsencrypt-sycamore2027h1.spki.der");

    fn cert(pem: &[u8]) -> Certificate {
        Certificate::from_pem(pem).expect("fixture certificate parses")
    }

    fn only_sct(pem: &[u8]) -> (Certificate, Sct) {
        let c = cert(pem);
        let mut scts = extract(&c).expect("fixture carries an SCT");
        assert_eq!(scts.len(), 1);
        let s = scts.remove(0);
        (c, s)
    }

    // --- the real thing, end to end ---------------------------------------

    #[test]
    fn a_live_production_fulcio_certificates_embedded_sct_verifies() {
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        assert_eq!(
            sct.log_id_hex(),
            "dd3d306ac6c7113263191e1c99673702a24a5eb8de3cadff878a72802f29ee8e",
            "ctfe.sigstore.dev/2022, as named in the production trusted_root"
        );
        assert_eq!(sct.timestamp_ms, 1_788_555_868_053);
        assert!(sct.extensions.is_empty());
        verify(&sct, &leaf, &issuer, PROD_CTLOG).expect("the production SCT verifies");
    }

    #[test]
    fn a_live_staging_fulcio_certificates_embedded_sct_verifies() {
        let (leaf, sct) = only_sct(STAGING_LEAF);
        let issuer = cert(STAGING_ISSUER);
        assert_eq!(
            sct.log_id_hex(),
            "3e607153746e1892e090b236b14976272f67b44d6f92ce6f77114ab722a0a6b6",
            "log2026-1.ctfe.sigstage.dev"
        );
        verify(&sct, &leaf, &issuer, STAGING_CTLOG).expect("the staging SCT verifies");
    }

    #[test]
    fn a_static_ct_logs_leaf_index_extension_is_carried_into_the_signed_blob() {
        // The staging log is a Static CT API log and puts an 8-byte leaf-index
        // extension in the SCT. A verifier that assumes CtExtensions is empty —
        // which RFC 6962 v1's text invites — builds a blob eight bytes short
        // and every staging certificate fails.
        let (leaf, sct) = only_sct(STAGING_LEAF);
        let issuer = cert(STAGING_ISSUER);
        assert_eq!(hex::encode(&sct.extensions), "00000500003a7c0d");

        let mut stripped = sct.clone();
        stripped.extensions.clear();
        assert!(
            matches!(
                verify(&stripped, &leaf, &issuer, STAGING_CTLOG),
                Err(Error::SctInvalid)
            ),
            "dropping the extensions must break the signature, or they are not in the blob"
        );
    }

    #[test]
    fn all_three_scts_on_a_real_web_pki_certificate_verify() {
        // `sigstore` 0.14.0 reports this certificate as having no SCT at all:
        // `crypto/transparency.rs` matches `[e] => e` on the parsed list and
        // returns LeafSCTMissing for any other length.
        let leaf = cert(WEBPKI_LEAF);
        let issuer = cert(WEBPKI_ISSUER);
        let scts = extract(&leaf).expect("three SCTs");
        assert_eq!(scts.len(), 3);

        let keys = [CTLOG_SECTIGO, CTLOG_CLOUDFLARE, CTLOG_LETSENCRYPT];
        for (sct, key) in scts.iter().zip(keys) {
            verify(sct, &leaf, &issuer, key).unwrap_or_else(|e| {
                panic!("SCT from log {} did not verify: {e}", sct.log_id_hex())
            });
        }
        // The Let's Encrypt Sycamore log is tiled, so this one is non-empty
        // even outside Sigstore.
        assert_eq!(hex::encode(&scts[2].extensions), "000005000160689f");
    }

    // --- the two intermediate blobs, against an independent implementation --

    #[test]
    fn the_precertificate_tbs_matches_a_byte_preserving_implementation() {
        // If `x509-cert`'s re-encoding of the TBSCertificate were not faithful,
        // this comparison against a Python implementation that never re-encodes
        // anything is where it would show.
        assert_eq!(
            precert_tbs_der(&cert(PROD_LEAF)).expect("prod precert"),
            PROD_PRECERT_TBS
        );
        assert_eq!(
            precert_tbs_der(&cert(STAGING_LEAF)).expect("staging precert"),
            STAGING_PRECERT_TBS
        );
    }

    #[test]
    fn the_precertificate_tbs_is_shorter_than_the_certificates_and_still_parses() {
        let leaf = cert(PROD_LEAF);
        let full = leaf.tbs_certificate().to_der().expect("re-encode");
        let pre = precert_tbs_der(&leaf).expect("precert");
        assert!(pre.len() < full.len());
        // It must still be a TBSCertificate, and the extension must be gone.
        let reparsed = x509_cert::certificate::TbsCertificate::from_der(&pre)
            .expect("the precertificate TBS is still a TBSCertificate");
        assert!(
            reparsed
                .extensions()
                .expect("extensions survive")
                .iter()
                .all(|e| e.extn_id != CT_PRECERT_SCTS),
            "the SCT extension is gone"
        );
        assert_eq!(
            reparsed.extensions().expect("extensions").len() + 1,
            leaf.tbs_certificate()
                .extensions()
                .expect("extensions")
                .len(),
            "exactly one extension was removed"
        );
    }

    #[test]
    fn the_signed_blob_matches_a_byte_preserving_implementation() {
        let (leaf, sct) = only_sct(PROD_LEAF);
        assert_eq!(
            signature_input(&sct, &leaf, &cert(PROD_ISSUER)).expect("blob"),
            PROD_SIG_INPUT
        );
        let (leaf, sct) = only_sct(STAGING_LEAF);
        assert_eq!(
            signature_input(&sct, &leaf, &cert(STAGING_ISSUER)).expect("blob"),
            STAGING_SIG_INPUT
        );
    }

    #[test]
    fn the_issuer_key_hash_is_over_the_whole_spki_not_the_bit_string() {
        // The classic mistake hashes the EC point. Both values are 32 bytes and
        // only one verifies; naming both here is what makes the difference
        // testable rather than remembered.
        let issuer = cert(PROD_ISSUER);
        assert_eq!(
            hex::encode(issuer_key_hash(&issuer).expect("hash")),
            "c355ee53d69e68aade04e5c6cc202cfac11fbcaa67e9a2ba7a64ced2aec8ccd0"
        );
        let point = issuer
            .tbs_certificate()
            .subject_public_key_info()
            .subject_public_key
            .as_bytes()
            .expect("whole bytes");
        assert_eq!(
            hex::encode(Sha256::digest(point)),
            "00c71d310238edfe67391cfdb669e6131108ac1de138be74e3d424389a6629cd",
            "the wrong input, recorded so a future change cannot silently adopt it"
        );
    }

    // --- negative: the signature ------------------------------------------

    #[test]
    fn a_single_flipped_signature_bit_is_rejected() {
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        for bit in 0..8 {
            let mut bad = sct.clone();
            let last = bad.signature.len() - 1;
            bad.signature[last] ^= 1 << bit;
            assert!(
                matches!(
                    verify(&bad, &leaf, &issuer, PROD_CTLOG),
                    Err(Error::SctInvalid)
                ),
                "flipping bit {bit} of the signature must be rejected"
            );
        }
    }

    #[test]
    fn the_wrong_ct_log_key_is_rejected() {
        // The staging certificate against the production log's key: both are
        // real, both are P-256, and only the pairing is wrong.
        let (leaf, sct) = only_sct(STAGING_LEAF);
        let issuer = cert(STAGING_ISSUER);
        assert!(matches!(
            verify(&sct, &leaf, &issuer, PROD_CTLOG),
            Err(Error::SctInvalid)
        ));
        // And the reverse, so this is not an artifact of one direction.
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        assert!(matches!(
            verify(&sct, &leaf, &issuer, STAGING_CTLOG),
            Err(Error::SctInvalid)
        ));
    }

    #[test]
    fn a_tampered_timestamp_is_rejected() {
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        let mut bad = sct.clone();
        bad.timestamp_ms += 1;
        assert!(matches!(
            verify(&bad, &leaf, &issuer, PROD_CTLOG),
            Err(Error::SctInvalid)
        ));
    }

    #[test]
    fn the_wrong_issuer_is_rejected() {
        // Swapping in the staging intermediate changes only `issuer_key_hash`.
        // This is the check that the hash is in the blob at all.
        let (leaf, sct) = only_sct(PROD_LEAF);
        assert!(matches!(
            verify(&sct, &leaf, &cert(STAGING_ISSUER), PROD_CTLOG),
            Err(Error::SctInvalid)
        ));
    }

    #[test]
    fn a_tampered_subject_alternative_name_is_rejected() {
        // The SAN is `https://issuer.enforce.dev/a580…`. Substituting one byte
        // for another of the same length keeps the DER valid and every length
        // unchanged, so nothing but the signature can catch it.
        let der = pem_to_der(PROD_LEAF);
        let needle = b"issuer.enforce.dev";
        let at = find(&der, needle).expect("the SAN is in the fixture");
        let mut tampered = der.clone();
        tampered[at] = b'1'; // "1ssuer.enforce.dev"
        let leaf = Certificate::from_der(&tampered).expect("still a valid certificate");

        let scts = extract(&leaf).expect("the SCT survives the edit");
        assert_eq!(scts.len(), 1);
        assert!(
            matches!(
                verify(&scts[0], &leaf, &cert(PROD_ISSUER), PROD_CTLOG),
                Err(Error::SctInvalid)
            ),
            "an altered SAN must break the SCT signature"
        );
    }

    #[test]
    fn a_certificate_with_no_sct_extension_says_so_rather_than_verifying_nothing() {
        // The Fulcio intermediate is a real certificate that was never logged.
        assert!(matches!(
            extract(&cert(PROD_ISSUER)),
            Err(Error::SctMissing)
        ));
        assert!(matches!(
            precert_tbs_der(&cert(PROD_ISSUER)),
            Err(Error::SctMissing)
        ));
    }

    // --- negative: decoding ------------------------------------------------

    #[test]
    fn an_empty_sct_list_is_missing_rather_than_vacuously_satisfied() {
        assert!(matches!(parse_list(&[0x00, 0x00]), Err(Error::SctMissing)));
    }

    #[test]
    fn a_list_whose_declared_length_disagrees_with_its_contents_is_rejected() {
        let (_, sct) = only_sct(PROD_LEAF);
        let one = serialize_sct(&sct);
        let mut list = Vec::new();
        list.extend_from_slice(&((one.len() + 2 + 1) as u16).to_be_bytes()); // one byte too many
        list.extend_from_slice(&(one.len() as u16).to_be_bytes());
        list.extend_from_slice(&one);
        assert!(matches!(parse_list(&list), Err(Error::SctMalformed(_))));
    }

    #[test]
    fn trailing_bytes_inside_one_sct_are_rejected() {
        let (_, sct) = only_sct(PROD_LEAF);
        let mut one = serialize_sct(&sct);
        one.push(0xff);
        let mut list = Vec::new();
        list.extend_from_slice(&((one.len() + 2) as u16).to_be_bytes());
        list.extend_from_slice(&(one.len() as u16).to_be_bytes());
        list.extend_from_slice(&one);
        match parse_list(&list) {
            Err(Error::SctMalformed(m)) => assert!(m.contains("remain after the SCT's signature")),
            other => panic!("expected SctMalformed, got {other:?}"),
        }
    }

    #[test]
    fn a_truncated_sct_is_rejected_rather_than_panicking() {
        let (_, sct) = only_sct(PROD_LEAF);
        let full = serialize_sct(&sct);
        for cut in 0..full.len() {
            let one = &full[..cut];
            let mut list = Vec::new();
            list.extend_from_slice(&((one.len() + 2) as u16).to_be_bytes());
            list.extend_from_slice(&(one.len() as u16).to_be_bytes());
            list.extend_from_slice(one);
            assert!(
                parse_list(&list).is_err(),
                "a {cut}-byte SCT must not decode"
            );
        }
    }

    #[test]
    fn an_unknown_sct_version_is_refused_instead_of_decoded_as_v1() {
        let (_, sct) = only_sct(PROD_LEAF);
        let mut one = serialize_sct(&sct);
        one[0] = 1;
        let mut list = Vec::new();
        list.extend_from_slice(&((one.len() + 2) as u16).to_be_bytes());
        list.extend_from_slice(&(one.len() as u16).to_be_bytes());
        list.extend_from_slice(&one);
        match parse_list(&list) {
            Err(Error::SctMalformed(m)) => assert!(m.contains("not decoded")),
            other => panic!("expected SctMalformed, got {other:?}"),
        }
    }

    // --- negative: algorithms we cannot honour ----------------------------

    #[test]
    fn an_algorithm_this_build_cannot_verify_is_named_not_silently_accepted() {
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);

        let mut rsa = sct.clone();
        rsa.signature_algorithm = 1; // TLS SignatureAlgorithm.rsa
        match verify(&rsa, &leaf, &issuer, PROD_CTLOG) {
            Err(Error::SctMalformed(m)) => assert!(m.contains("NOT checked")),
            other => panic!("expected a refusal, got {other:?}"),
        }

        let mut sha384 = sct.clone();
        sha384.hash_algorithm = 5; // TLS HashAlgorithm.sha384
        assert!(matches!(
            verify(&sha384, &leaf, &issuer, PROD_CTLOG),
            Err(Error::SctMalformed(_))
        ));
    }

    #[test]
    fn a_non_p256_ct_log_key_is_refused_rather_than_verified_with_the_wrong_digest() {
        use p384::ecdsa::SigningKey;
        use spki::EncodePublicKey;
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        let sk = SigningKey::from_bytes(&[9u8; 48].into()).expect("p384 key");
        let spki = sk
            .verifying_key()
            .to_public_key_der()
            .expect("encode")
            .as_bytes()
            .to_vec();
        match verify(&sct, &leaf, &issuer, &spki) {
            Err(Error::SctMalformed(m)) => {
                assert!(m.contains("not P-256"), "{m}");
                assert!(m.contains("NOT checked"), "{m}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_unreadable_ct_log_key_is_not_reported_as_an_invalid_sct() {
        // `SctInvalid` is a claim about the certificate. A key that could not
        // be read is a claim about the trust root, and conflating them is how a
        // broken deployment comes to look like an attack.
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        assert!(matches!(
            verify(&sct, &leaf, &issuer, b"not a SubjectPublicKeyInfo"),
            Err(Error::SctMalformed(_))
        ));
    }

    // --- the axis ----------------------------------------------------------

    #[test]
    fn every_failure_this_module_raises_sits_on_the_sct_axis() {
        use crate::error::Axis;
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        let mut bad = sct.clone();
        bad.timestamp_ms ^= 1;
        for e in [
            extract(&cert(PROD_ISSUER)).unwrap_err(),
            parse_list(&[0x00, 0x00]).unwrap_err(),
            parse_list(&[0x00, 0x05, 0x00]).unwrap_err(),
            verify(&bad, &leaf, &issuer, PROD_CTLOG).unwrap_err(),
            // A CT log key that parses as a SubjectPublicKeyInfo but not as a
            // point. This one used to come back on `Axis::Chain`; the list
            // above was four hand-picked errors and did not reach it.
            verify(&sct, &leaf, &issuer, &broken_ct_log_key()).unwrap_err(),
        ] {
            assert_eq!(e.axis(), Axis::Sct, "{e}");
        }
    }

    // --- cross-check against x509-cert's own vectors -----------------------

    /// `x509-cert` 0.3.0 `ext/pkix/sct.rs`'s `SCT_EXAMPLE`: a real
    /// two-SCT extension value, DER OCTET STRING and all. Reusing it checks
    /// this parser against the upstream one whose feature was not enabled.
    const X509_CERT_SCT_EXAMPLE: [u8; 245] = [
        4, 129, 242, 0, 240, 0, 119, 0, 122, 50, 140, 84, 216, 183, 45, 182, 32, 234, 56, 224, 82,
        30, 233, 132, 22, 112, 50, 19, 133, 77, 59, 210, 43, 193, 58, 87, 163, 82, 235, 82, 0, 0,
        1, 135, 224, 74, 186, 106, 0, 0, 4, 3, 0, 72, 48, 70, 2, 33, 0, 170, 82, 81, 162, 157, 234,
        14, 189, 167, 13, 247, 211, 97, 112, 248, 172, 149, 125, 58, 18, 238, 60, 150, 157, 124,
        245, 188, 138, 102, 212, 244, 187, 2, 33, 0, 209, 79, 31, 63, 208, 79, 240, 233, 193, 187,
        28, 33, 190, 95, 130, 66, 183, 222, 187, 42, 22, 83, 0, 119, 226, 246, 19, 197, 47, 237,
        198, 149, 0, 117, 0, 173, 247, 190, 250, 124, 255, 16, 200, 139, 157, 61, 156, 30, 62, 24,
        106, 180, 103, 41, 93, 207, 177, 12, 36, 202, 133, 134, 52, 235, 220, 130, 138, 0, 0, 1,
        135, 224, 74, 186, 164, 0, 0, 4, 3, 0, 70, 48, 68, 2, 32, 29, 110, 144, 37, 157, 227, 170,
        70, 67, 16, 68, 195, 212, 168, 246, 37, 94, 69, 210, 136, 42, 113, 217, 230, 34, 152, 253,
        116, 13, 174, 232, 191, 2, 32, 16, 25, 200, 223, 59, 176, 40, 145, 76, 85, 242, 133, 130,
        212, 61, 216, 83, 238, 115, 130, 82, 240, 196, 162, 249, 54, 199, 120, 175, 72, 223, 14,
    ];

    #[test]
    fn upstreams_two_sct_vector_decodes_to_both_of_them() {
        let inner = OctetString::from_der(&X509_CERT_SCT_EXAMPLE).expect("outer OCTET STRING");
        let scts = parse_list(inner.as_bytes()).expect("two SCTs");
        assert_eq!(scts.len(), 2);
        assert_eq!(scts[0].log_id, X509_CERT_SCT_EXAMPLE[8..40]);
        assert_eq!(
            scts[0].timestamp_ms,
            u64::from_be_bytes(X509_CERT_SCT_EXAMPLE[40..48].try_into().expect("8 bytes"))
        );
        assert_eq!(scts[0].signature, &X509_CERT_SCT_EXAMPLE[54..126]);
        assert_eq!(scts[1].log_id, X509_CERT_SCT_EXAMPLE[129..161]);
        assert_eq!(scts[1].signature, &X509_CERT_SCT_EXAMPLE[175..]);
        for s in &scts {
            assert_eq!(s.hash_algorithm, HASH_ALGORITHM_SHA256);
            assert_eq!(s.signature_algorithm, SIGNATURE_ALGORITHM_ECDSA);
            assert!(s.extensions.is_empty());
        }
    }

    #[test]
    fn a_round_trip_through_this_modules_own_serializer_is_stable() {
        // Only a test helper writes SCTs, so this is really a check that the
        // negative tests above are perturbing well-formed input.
        let (_, sct) = only_sct(STAGING_LEAF);
        let one = serialize_sct(&sct);
        let mut list = Vec::new();
        list.extend_from_slice(&((one.len() + 2) as u16).to_be_bytes());
        list.extend_from_slice(&(one.len() as u16).to_be_bytes());
        list.extend_from_slice(&one);
        let back = parse_list(&list).expect("round trip");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].log_id, sct.log_id);
        assert_eq!(back[0].timestamp_ms, sct.timestamp_ms);
        assert_eq!(back[0].extensions, sct.extensions);
        assert_eq!(back[0].signature, sct.signature);
    }

    // --- DER surgery, in isolation ----------------------------------------

    #[test]
    fn der_lengths_use_the_minimal_definite_form() {
        assert_eq!(der_len(0), vec![0x00]);
        assert_eq!(der_len(0x7f), vec![0x7f]);
        assert_eq!(der_len(0x80), vec![0x81, 0x80]);
        assert_eq!(der_len(0xff), vec![0x81, 0xff]);
        assert_eq!(der_len(0x100), vec![0x82, 0x01, 0x00]);
        assert_eq!(der_len(595), vec![0x82, 0x02, 0x53]);
    }

    #[test]
    fn an_indefinite_length_is_refused_because_it_is_not_der() {
        // 0x30 0x80 … is valid BER and forbidden in DER; accepting it would
        // mean the precertificate depended on a length nobody wrote down.
        assert!(matches!(
            read_tlv(&[0x30, 0x80, 0x00, 0x00], 0),
            Err(Error::SctMalformed(_))
        ));
    }

    #[test]
    fn a_truncated_element_is_refused_rather_than_slicing_past_the_end() {
        assert!(read_tlv(&[0x30, 0x05, 0x01, 0x02], 0).is_err());
        assert!(read_tlv(&[0x30], 0).is_err());
        assert!(read_tlv(&[], 0).is_err());
    }

    #[test]
    fn surgery_on_something_that_is_not_a_tbs_certificate_is_refused() {
        assert!(matches!(
            strip_sct_extension(&[0x02, 0x01, 0x00]),
            Err(Error::SctMalformed(_))
        ));
        // A well-formed SEQUENCE with no extensions block at all.
        assert!(matches!(
            strip_sct_extension(&[0x30, 0x03, 0x02, 0x01, 0x00]),
            Err(Error::SctMissing)
        ));
    }

    #[test]
    fn trailing_bytes_after_the_tbs_certificate_are_refused() {
        let mut tbs = cert(PROD_LEAF).tbs_certificate().to_der().expect("der");
        tbs.push(0x00);
        assert!(matches!(
            strip_sct_extension(&tbs),
            Err(Error::SctMalformed(_))
        ));
    }

    #[test]
    fn an_sct_extension_that_is_not_last_keeps_the_extensions_after_it() {
        // Every real certificate to hand — both Fulcio fixtures and the
        // web-PKI one — puts the SCT list last among its extensions, so the
        // branch that copies extensions *after* the removed one is never
        // exercised by a fixture and a splice that dropped them would still
        // pass every other test here.
        //
        // Swapping the final two extensions is legal DER and changes no
        // length, so it needs no re-encoding. And because deleting B from
        // `[.., A, B]` and from `[.., B, A]` leaves the same list, the
        // expected output is the recorded precertificate TBS byte for byte.
        let tbs = cert(PROD_LEAF).tbs_certificate().to_der().expect("der");
        let swapped = swap_last_two_extensions(&tbs);
        assert_ne!(swapped, tbs, "the fixture must actually have been altered");
        assert_eq!(
            strip_sct_extension(&swapped).expect("strip"),
            PROD_PRECERT_TBS
        );
    }

    #[test]
    fn a_certificate_carrying_two_sct_extensions_is_refused_at_both_ends() {
        // RFC 5280 §4.2 forbids it, and it matters here beyond conformance:
        // `extract` would report the first list while `strip_sct_extension`
        // removes both, so the SCTs and the blob they are checked against
        // would describe different certificates.
        let doubled = duplicate_sct_extension(&pem_to_der(PROD_LEAF));
        let leaf = Certificate::from_der(&doubled).expect("still parses as a certificate");
        match extract(&leaf) {
            Err(Error::SctMalformed(m)) => assert!(m.contains("more than one"), "{m}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
        match precert_tbs_der(&leaf) {
            Err(Error::SctMalformed(m)) => assert!(m.contains("cannot be determined"), "{m}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    // --- helpers -----------------------------------------------------------

    /// Ranges of the individual `Extension` elements, and the absolute offset
    /// of the extension SEQUENCE's contents, within a DER TBSCertificate.
    fn extension_ranges(tbs: &[u8]) -> (usize, Vec<(usize, usize)>) {
        let (_, body_start, body_len, _) = read_tlv(tbs, 0).expect("outer SEQUENCE");
        let body = &tbs[body_start..body_start + body_len];
        let mut p = 0usize;
        let mut a3_value = None;
        while p < body.len() {
            let (t, vs, _, e) = read_tlv(body, p).expect("a TBS child");
            if t == TAG_EXTENSIONS {
                a3_value = Some(vs);
            }
            p = e;
        }
        let a3_value = a3_value.expect("the fixture has extensions");
        let (_, seq_value, _, seq_end) = read_tlv(body, a3_value).expect("SEQUENCE OF Extension");
        let seq = &body[seq_value..seq_end];
        let mut ranges = Vec::new();
        let mut q = 0usize;
        while q < seq.len() {
            let (_, _, _, e) = read_tlv(seq, q).expect("an Extension");
            ranges.push((q, e));
            q = e;
        }
        (body_start + seq_value, ranges)
    }

    fn swap_last_two_extensions(tbs: &[u8]) -> Vec<u8> {
        let (abs, ranges) = extension_ranges(tbs);
        let (a0, a1) = ranges[ranges.len() - 2];
        let (b0, b1) = ranges[ranges.len() - 1];
        let seq_len = b1;
        let seq = &tbs[abs..abs + seq_len];
        let mut new_seq = Vec::with_capacity(seq_len);
        new_seq.extend_from_slice(&seq[..a0]);
        new_seq.extend_from_slice(&seq[b0..b1]);
        new_seq.extend_from_slice(&seq[a0..a1]);
        assert_eq!(
            new_seq.len(),
            seq_len,
            "the swap must not change any length"
        );
        let mut out = tbs.to_vec();
        out[abs..abs + seq_len].copy_from_slice(&new_seq);
        out
    }

    /// Append a second copy of the SCT extension to a DER **Certificate**,
    /// rebuilding each of the four enclosing lengths. The result is not
    /// signature-valid and is not meant to be; only the extension list matters.
    fn duplicate_sct_extension(cert_der: &[u8]) -> Vec<u8> {
        let (_, cert_body, cert_body_len, _) = read_tlv(cert_der, 0).expect("Certificate");
        let (_, _, _, tbs_end) = read_tlv(cert_der, cert_body).expect("TBSCertificate");
        let tbs = &cert_der[cert_body..tbs_end];
        let after_tbs = &cert_der[tbs_end..cert_body + cert_body_len];

        let (_, tbs_body_start, tbs_body_len, _) = read_tlv(tbs, 0).expect("TBS SEQUENCE");
        let tbs_body = &tbs[tbs_body_start..tbs_body_start + tbs_body_len];

        let mut p = 0usize;
        let mut a3 = None;
        while p < tbs_body.len() {
            let (t, vs, _, e) = read_tlv(tbs_body, p).expect("a TBS child");
            if t == TAG_EXTENSIONS {
                a3 = Some((p, vs));
            }
            p = e;
        }
        let (a3_start, a3_value) = a3.expect("the fixture has extensions");
        let (_, seq_value, _, seq_end) =
            read_tlv(tbs_body, a3_value).expect("SEQUENCE OF Extension");
        let seq = &tbs_body[seq_value..seq_end];

        let mut sct_ext = None;
        let mut q = 0usize;
        while q < seq.len() {
            let (_, ev, _, ee) = read_tlv(seq, q).expect("an Extension");
            let (_, ov, ol, _) = read_tlv(seq, ev).expect("its OID");
            if &seq[ov..ov + ol] == CT_PRECERT_SCTS.as_bytes() {
                sct_ext = Some(seq[q..ee].to_vec());
            }
            q = ee;
        }
        let sct_ext = sct_ext.expect("the fixture carries an SCT extension");

        let mut new_seq = seq.to_vec();
        new_seq.extend_from_slice(&sct_ext);
        let mut inner = vec![TAG_SEQUENCE];
        inner.extend_from_slice(&der_len(new_seq.len()));
        inner.extend_from_slice(&new_seq);
        let mut wrapper = vec![TAG_EXTENSIONS];
        wrapper.extend_from_slice(&der_len(inner.len()));
        wrapper.extend_from_slice(&inner);

        let mut new_tbs_body = tbs_body[..a3_start].to_vec();
        new_tbs_body.extend_from_slice(&wrapper);
        let mut new_tbs = vec![TAG_SEQUENCE];
        new_tbs.extend_from_slice(&der_len(new_tbs_body.len()));
        new_tbs.extend_from_slice(&new_tbs_body);

        let mut new_cert_body = new_tbs;
        new_cert_body.extend_from_slice(after_tbs);
        let mut out = vec![TAG_SEQUENCE];
        out.extend_from_slice(&der_len(new_cert_body.len()));
        out.extend_from_slice(&new_cert_body);
        out
    }

    fn serialize_sct(s: &Sct) -> Vec<u8> {
        let mut v = vec![s.version];
        v.extend_from_slice(&s.log_id);
        v.extend_from_slice(&s.timestamp_ms.to_be_bytes());
        v.extend_from_slice(&(s.extensions.len() as u16).to_be_bytes());
        v.extend_from_slice(&s.extensions);
        v.push(s.hash_algorithm);
        v.push(s.signature_algorithm);
        v.extend_from_slice(&(s.signature.len() as u16).to_be_bytes());
        v.extend_from_slice(&s.signature);
        v
    }

    fn pem_to_der(pem: &[u8]) -> Vec<u8> {
        cert(pem).to_der().expect("re-encode the fixture")
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    // --- negative: a trust root that is broken rather than lying -----------
    //
    // These four came out of an adversarial pass over the module. The first
    // two are a defect that was here; the last two are checks that were here
    // and that nothing exercised — a deliberately broken copy of the module
    // passed the whole suite with either of them deleted.

    /// The real production CT log key with its last two bytes flipped: still a
    /// SubjectPublicKeyInfo, still id-ecPublicKey, still secp256r1, but the
    /// point no longer decodes. This is what a truncated or half-written
    /// `trusted_root.json` looks like, not what an attacker sends.
    fn broken_ct_log_key() -> Vec<u8> {
        let mut k = PROD_CTLOG.to_vec();
        let n = k.len();
        k[n - 1] ^= 0xff;
        k[n - 2] ^= 0xff;
        k
    }

    #[test]
    fn a_ct_log_key_that_cannot_be_used_stays_on_the_sct_axis_and_blames_the_key() {
        use crate::error::Axis;
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        match verify(&sct, &leaf, &issuer, &broken_ct_log_key()) {
            Err(e) => {
                // It used to answer `Error::CertificateParse`, whose text is
                // "the leaf certificate could not be parsed" and whose axis is
                // `Axis::Chain` — a chain-axis complaint naming a leaf that
                // parsed perfectly, raised because the *trust root's* copy of
                // the log key was corrupt.
                assert_eq!(e.axis(), Axis::Sct, "{e}");
                let m = e.to_string();
                assert!(m.contains("transparency log's public key"), "{m}");
                assert!(m.contains("NOT checked"), "{m}");
                assert!(
                    !m.contains("leaf certificate could not be parsed"),
                    "the message must not blame the leaf: {m}"
                );
                assert!(
                    !matches!(e, Error::SctInvalid),
                    "and it must not read as forgery either"
                );
            }
            Ok(()) => panic!("a key that cannot be used must not verify anything"),
        }
    }

    #[test]
    fn a_ct_log_key_whose_point_is_the_wrong_length_is_refused_the_same_way() {
        use crate::error::Axis;
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        // Shorten the BIT STRING by one byte and fix the two enclosing lengths,
        // so the SPKI stays well-formed and only the point is wrong.
        let mut k = PROD_CTLOG.to_vec();
        let bs = k.iter().rposition(|b| *b == 0x03).expect("BIT STRING tag");
        k.pop();
        k[bs + 1] -= 1;
        k[1] -= 1;
        let e = verify(&sct, &leaf, &issuer, &k).expect_err("must not verify");
        assert_eq!(e.axis(), Axis::Sct, "{e}");
        assert!(e.to_string().contains("NOT checked"), "{e}");
    }

    #[test]
    fn the_fixed_width_signature_form_is_refused_because_rfc6962_encodes_scts_in_der() {
        // `x509::verify_ecdsa_spki` accepts r‖s as a fallback, which is right
        // for bundle signatures and wrong for an SCT: RFC 6962 §3.2 says DER.
        // The point of this test is that the *same* log signature, merely
        // re-spelled, is refused here — so the assertion below that it would
        // have verified is load-bearing, not decoration.
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        let fixed = p256::ecdsa::Signature::from_der(&sct.signature)
            .expect("the real SCT signature is DER")
            .to_bytes()
            .to_vec();
        assert_eq!(fixed.len(), 64);
        assert_ne!(fixed, sct.signature);

        let blob = signature_input(&sct, &leaf, &issuer).expect("blob");
        crate::x509::verify_ecdsa_spki(PROD_CTLOG, &blob, &fixed)
            .expect("the fixed-width form is the same signature and does verify");

        let mut alt = sct.clone();
        alt.signature = fixed;
        match verify(&alt, &leaf, &issuer, PROD_CTLOG) {
            Err(Error::SctMalformed(m)) => {
                assert!(m.contains("DER"), "{m}");
                assert!(m.contains("NOT checked"), "{m}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn verify_refuses_a_non_v1_sct_of_its_own_accord_not_only_through_the_parser() {
        // `Sct`'s fields are public, so `verify` is reachable with a struct
        // `parse_one` would never have produced. Deleting `verify`'s own
        // version check failed no test in this file before this one existed.
        let (leaf, sct) = only_sct(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        let mut v2 = sct.clone();
        v2.version = 1;
        match verify(&v2, &leaf, &issuer, PROD_CTLOG) {
            Err(Error::SctMalformed(m)) => assert!(m.contains("nothing was verified"), "{m}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn elements_after_the_extensions_block_are_refused_rather_than_silently_dropped() {
        // The splice keeps `body[..a3_start]` and appends the rebuilt block, so
        // anything following `[3]` would vanish from the signed precertificate
        // without a word. RFC 5280 puts extensions last and no real fixture can
        // exercise this, so deleting the guard failed nothing here before.
        let tbs = cert(PROD_LEAF).tbs_certificate().to_der().expect("der");
        let (_, body_start, body_len, _) = read_tlv(&tbs, 0).expect("outer SEQUENCE");
        let mut body = tbs[body_start..body_start + body_len].to_vec();
        body.extend_from_slice(&[0x02, 0x01, 0x2a]); // an INTEGER after [3]
        let mut forged = vec![TAG_SEQUENCE];
        forged.extend_from_slice(&der_len(body.len()));
        forged.extend_from_slice(&body);

        match strip_sct_extension(&forged) {
            Err(Error::SctMalformed(m)) => assert!(m.contains("after the extensions block"), "{m}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_extension_whose_oid_runs_past_its_own_end_is_refused() {
        // Extension A declares two bytes of content but its first child is a
        // ten-byte OID TLV — the SCT OID — lying in the bytes after it.
        // Bounded by the extension, the OID cannot be read at all and the
        // extension is refused. Unbounded, it matched, A was removed as if it
        // were the SCT extension, and the walk then desynchronised into a
        // different error two lines later. Both refuse; only one refuses for
        // the reason it says.
        let oid = CT_PRECERT_SCTS.as_bytes();
        let mut seq = vec![TAG_SEQUENCE, 0x02, TAG_OID, oid.len() as u8];
        seq.extend_from_slice(oid);
        seq.extend_from_slice(&[0x04, 0x00]);

        let mut inner = vec![TAG_SEQUENCE];
        inner.extend_from_slice(&der_len(seq.len()));
        inner.extend_from_slice(&seq);
        let mut wrapper = vec![TAG_EXTENSIONS];
        wrapper.extend_from_slice(&der_len(inner.len()));
        wrapper.extend_from_slice(&inner);
        let mut body = vec![0x02, 0x01, 0x00];
        body.extend_from_slice(&wrapper);
        let mut tbs = vec![TAG_SEQUENCE];
        tbs.extend_from_slice(&der_len(body.len()));
        tbs.extend_from_slice(&body);

        match strip_sct_extension(&tbs) {
            Err(Error::SctMalformed(m)) => {
                assert!(
                    m.contains("claiming 10 bytes"),
                    "the OID was read past its own end: {m}"
                )
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    // --- negative: nothing hostile reaches a panic -------------------------

    /// Deterministic, so a failure is reproducible rather than a story about a
    /// seed nobody wrote down.
    fn lcg(seed: u64) -> impl FnMut() -> u32 {
        let mut st = seed;
        move || {
            st = st
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (st >> 33) as u32
        }
    }

    #[test]
    fn no_mutated_sct_list_reaches_a_panic() {
        let (_, sct) = only_sct(PROD_LEAF);
        let one = serialize_sct(&sct);
        let mut list = Vec::new();
        list.extend_from_slice(&((one.len() + 2) as u16).to_be_bytes());
        list.extend_from_slice(&(one.len() as u16).to_be_bytes());
        list.extend_from_slice(&one);
        assert!(parse_list(&list).is_ok(), "the unmutated list must parse");

        let mut next = lcg(0x2545_F491_4F6C_DD1D);
        for _ in 0..20_000 {
            let mut m = list.clone();
            for _ in 0..1 + (next() % 6) as usize {
                let i = (next() as usize) % m.len();
                m[i] = next() as u8;
            }
            if next().is_multiple_of(4) {
                let cut = (next() as usize) % m.len();
                m.truncate(cut);
            }
            let _ = parse_list(&m);
        }
        for _ in 0..20_000 {
            let n = (next() as usize) % 96;
            let b: Vec<u8> = (0..n).map(|_| next() as u8).collect();
            let _ = parse_list(&b);
        }
    }

    #[test]
    fn no_mutated_certificate_reaches_a_panic_in_extract_or_the_surgery() {
        let der = pem_to_der(PROD_LEAF);
        let issuer = cert(PROD_ISSUER);
        let mut next = lcg(12345);
        let mut still_parsed = 0u32;
        for _ in 0..6_000 {
            let mut m = der.clone();
            for _ in 0..1 + (next() % 4) as usize {
                let i = (next() as usize) % m.len();
                m[i] = next() as u8;
            }
            let Ok(c) = Certificate::from_der(&m) else {
                continue;
            };
            still_parsed += 1;
            let _ = precert_tbs_der(&c);
            if let Ok(v) = extract(&c) {
                for s in &v {
                    let _ = signature_input(s, &c, &issuer);
                    let _ = verify(s, &c, &issuer, PROD_CTLOG);
                }
            }
        }
        assert!(
            still_parsed > 500,
            "only {still_parsed} mutants stayed parseable; the fuzz is not reaching the module"
        );
    }
}
