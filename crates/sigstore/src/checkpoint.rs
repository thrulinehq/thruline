//! Rekor checkpoints — the signed note that states what the log's root is.
//!
//! Format (C2SP signed-note):
//! ```text
//! <origin>\n<tree size>\n<base64 root hash>\n[optional extra lines]\n
//! \n
//! — <key name> <base64: 4-byte key hint ‖ signature>\n
//! ```
//! The blank line separates the signed body from the signature block; the
//! signature covers the body **including its trailing newline** and nothing
//! after it.
//!
//! # Why the checkpoint and not just `rootHash`
//!
//! `inclusionProof.rootHash` is an unsigned field in a document the artifact
//! supplies. Reconstructing a root that matches it proves only that the proof
//! is self-consistent — an attacker who forges both matches trivially. The
//! checkpoint is the log's *signature* over that root, so it is the only thing
//! tying the proof to the log's identity.
//!
//! **Checkpoint signatures are re-generated per response.** Rekor re-signs on
//! every fetch and ECDSA is randomized, so two GETs of the same entry return
//! different signature bytes over identical signed text. Never compare or cache
//! whole entries for equality; compare the signed body and re-verify.
//!
//! Measured, not assumed: two consecutive GETs of prod entry
//! `24296fb2…6f949fb9be0c` on 2026-09-05 returned byte-identical signed bodies
//! (`sha256 b90df776…`) and two different ECDSA signatures, one 70 bytes of DER
//! and one 71. Both fixtures are in `fixtures/` and
//! `two_snapshots_of_one_checkpoint_share_a_body_and_differ_in_signature`
//! holds the property.
//!
//! # The bytes are kept, never rebuilt
//!
//! [`Checkpoint::signed_body`] is a slice of the input, copied verbatim. It is
//! **not** re-serialized from `origin`/`tree_size`/`root_hash`, and that is the
//! central design decision here.
//!
//! `sigstore` 0.14.0 does the opposite: `SignedCheckpoint::verify_signature`
//! (`src/rekor/models/checkpoint.rs:112`) verifies over `self.note.marshal()`,
//! a string rebuilt from the parsed fields. That makes correctness depend on
//! the parser being an exact inverse of the log's serializer forever — and its
//! `marshal` already is not one, because `unmarshal` drops empty extra lines
//! (`:194`) and re-formats `<k>: <v>` lines from their split halves (`:195`).
//! A verifier that checks a signature over a string it built itself has stopped
//! checking the string it was given.
//!
//! # Two divergences from that crate this module does not inherit
//!
//! 1. **Witnessed checkpoints do not parse there at all.** `decode` splits the
//!    signature block on `"\n\n"` (`:82`) although real signature lines are
//!    separated by a *single* newline, so all four lines of a witnessed
//!    checkpoint arrive at `CheckpointSignature::decode` as one chunk, whose
//!    `s.split(' ')` yields nine fields against a three-field pattern (`:250`)
//!    — `DecodeError`. Replayed against the real
//!    `fixtures/checkpoint-rekor-v2-prod-witnessed.txt`, that split produces one
//!    chunk of nine fields. Every Rekor v2 checkpoint is witnessed.
//! 2. **The separator is an EM DASH, U+2014.** That same file documents it as
//!    `– <identity> …` with an EN DASH (U+2013) at `:233` while emitting U+2014
//!    at `:237`. Two glyphs a code reviewer cannot tell apart, one of which
//!    makes every checkpoint on earth unparseable. Hence
//!    [`SIGNATURE_LINE_PREFIX`] is a named constant with its codepoint written
//!    out, and a test asserts its bytes.

use base64::Engine as _;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// The three bytes that begin every signature line: U+2014 EM DASH, then a
/// space. **Not** U+2013 EN DASH (`e2 80 93`), which differs in one byte and is
/// what the format is misdocumented as in more than one implementation.
pub const SIGNATURE_LINE_PREFIX: &str = "\u{2014} ";

/// A signature blob is a 4-byte key hint followed by the signature, so a blob
/// shorter than five bytes carries no signature at all. Both upstream parsers
/// draw the line in the same place — `note.Open` in `golang.org/x/mod`
/// (`sumdb/note/note.go:551`, `len(sig) < 5`) and Rekor's own
/// `SignedNote.UnmarshalText` (`pkg/util/signed_note.go:177`).
const MIN_SIGNATURE_BLOB: usize = 5;

/// The body's first three lines are fixed: origin, size, root hash.
const REQUIRED_BODY_LINES: usize = 3;

/// The most signature lines a note may carry, matching `note.Open`'s own cap
/// (`sumdb/note/note.go:557`, `if numSig++; numSig > 100`) and the comment it
/// carries: *"Avoid spending forever parsing a note with many signatures."*
///
/// This is not a tidiness limit. `crate::verify` hands
/// `inclusionProof.checkpoint.envelope` — bytes wholly under the control of
/// whoever supplied the bundle — straight to [`Checkpoint::parse`], before any
/// trust decision has been made. Every extra line is one more allocation at
/// parse time and one more ECDSA verification in [`Checkpoint::verify`], which
/// tries them all.
///
/// Measured on this machine in release with the cap lifted, and the shape of
/// the attack matters: 100 000 *minimum-length* (5-byte) blobs cost only 77 ms,
/// because a 1-byte signature fails DER decoding before any elliptic-curve
/// arithmetic runs. Filling the lines with a **well-formed 71-byte DER
/// signature** — the real prod one, copied — buys the full verification each
/// time: 10 000 lines in a 1.24 MB note took **1.44 s**, and 100 000 lines in a
/// 12.4 MB note took **11.0 s**. That is ~1.2 s of CPU per megabyte of
/// checkpoint, chosen by whoever supplied the bundle, spent before the verifier
/// has decided to trust anything, and all of it on signatures that then fail.
///
/// Real notes are nowhere near this: prod Rekor v1 carries one signature line
/// and the witnessed v2 checkpoint in `fixtures/` carries four. Go, which
/// *writes* these notes, refuses to read a 101st.
const MAX_SIGNATURE_LINES: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// The log's origin string, e.g. `rekor.sigstage.dev - 8959784741570461564`.
    pub origin: String,
    pub tree_size: u64,
    pub root_hash: [u8; 32],
    /// The exact bytes the signatures cover.
    pub signed_body: Vec<u8>,
    pub signatures: Vec<NoteSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSignature {
    pub key_name: String,
    /// First 4 bytes of the log's key id — a hint for selecting a key, never
    /// evidence on its own.
    pub key_hint: [u8; 4],
    pub signature: Vec<u8>,
}

fn malformed(reason: impl Into<String>) -> Error {
    Error::CheckpointMalformed(reason.into())
}

/// The key hint a **Rekor v1** log puts in front of its signature: the first
/// four bytes of `SHA-256(DER SubjectPublicKeyInfo)`, which is also the
/// `logId.keyId` in a Sigstore trusted root. Rekor documents it in those words
/// at `pkg/util/signed_note.go:147-149`.
///
/// Advisory only. [`Checkpoint::verify`] does not gate on it, and the reason is
/// concrete: **the derivation is not the same for every Sigstore log.** The
/// tiled prod log `log2025-1.rekor.sigstore.dev` signs with Ed25519 and its
/// hint is `cf119915`, matching its trusted-root `logId`, while
/// `SHA-256(SPKI)` of the same key is `b54813cb…`. A verifier that filtered
/// signature lines by this function would discard the only signature a Rekor v2
/// checkpoint carries. Selecting a candidate key is a hint's whole job; the
/// signature is the evidence.
pub fn rekor_v1_key_hint(spki_der: &[u8]) -> [u8; 4] {
    let digest: [u8; 32] = Sha256::digest(spki_der).into();
    [digest[0], digest[1], digest[2], digest[3]]
}

impl Checkpoint {
    /// Parse a signed note. See the module docs for the grammar.
    ///
    /// # Where the body ends
    ///
    /// At the **last** `"\n\n"` in the note. Not a guess: both parsers on the
    /// producing side do exactly this — `note.Open` in `golang.org/x/mod`
    /// (`sumdb/note/note.go:520`, `bytes.LastIndex(msg, sigSplit)`, then
    /// `msg[:split+1]` and `msg[split+2:]`) and the code that writes these
    /// checkpoints, Rekor's `SignedNote.UnmarshalText`
    /// (`pkg/util/signed_note.go:153`, character for character the same). The
    /// slicing below is the same two expressions.
    ///
    /// Splitting at the *first* `"\n\n"` instead — as `sigstore` 0.14.0 does at
    /// `checkpoint.rs:78` — truncates the body of any note whose extra lines
    /// include a blank one, and the resulting signature failure is
    /// indistinguishable from tampering. Signature lines cannot themselves be
    /// blank, so the last `"\n\n"` is unambiguous.
    ///
    /// # What is refused by name rather than left to the signature check
    ///
    /// A CRLF-mangled note, a hyphen or en dash where the em dash belongs, a
    /// non-decimal size, a root hash that is not 32 bytes, an empty signature
    /// block. All of those would also fail signature verification, but they
    /// would fail it looking exactly like an attack. They are different
    /// problems with different causes and they get different errors.
    pub fn parse(text: &str) -> Result<Checkpoint> {
        let bytes = text.as_bytes();

        // The trailing quotes `sigstore` 0.14.0 strips at `checkpoint.rs:76`
        // are deliberately NOT stripped: a checkpoint still wearing its JSON
        // quotes was not decoded, and quietly repairing that hides the bug in
        // whatever handed it over. It fails the trailing-newline rule below.
        // `note.Open` scans the **whole message** for ASCII control characters
        // before it splits anything (`sumdb/note/note.go:510-516`), not just the
        // body. Scanning only the body — which this function used to do — let a
        // note through that Go calls malformed: `— rekor\0.sigstore.dev <b64>`
        // parsed here, verified here (the body is untouched, so the log's real
        // signature still checks out), and handed the caller a key name with a
        // NUL in it. A verifier and the implementation that writes its input
        // must agree on what is well-formed, or "valid" means two things.
        //
        // Go's loop also rejects invalid UTF-8 (`utf8.RuneError && size == 1`);
        // that arm has no counterpart here because the parameter is `&str`.
        //
        // One deliberate divergence, in the strict direction: Go's condition is
        // `r < 0x20 && r != '\n'`, which permits U+007F DEL. This refuses it.
        // No Rekor origin, key name or base64 blob can contain DEL, and Rekor's
        // own `Fscanf`-based reader would not survive one either.
        if let Some(bad) = bytes
            .iter()
            .position(|&c| c != b'\n' && is_ascii_control(c))
        {
            return Err(malformed(format!(
                "the note contains the control byte {:#04x} at offset {bad}; a checkpoint is \
                 newline-separated and a carriage return means something rewrote it in transit",
                bytes[bad]
            )));
        }

        let split = find_last(bytes, b"\n\n").ok_or_else(|| {
            malformed(
                "no blank line separates the body from the signature block; a note is \
                 <body>\\n\\n<signature lines>",
            )
        })?;
        let body = &bytes[..split + 1];
        let sig_block = &bytes[split + 2..];

        if sig_block.is_empty() {
            return Err(malformed(
                "the signature block is empty; a note with no signatures is a claim about a log, \
                 not a statement by one",
            ));
        }
        if sig_block.last() != Some(&b'\n') {
            return Err(malformed(
                "the last signature line does not end in a newline, so the note is truncated",
            ));
        }
        // `body` ends in '\n' by construction, so drop that one and split.
        let lines: Vec<&[u8]> = body[..body.len() - 1].split(|&c| c == b'\n').collect();
        if lines.len() < REQUIRED_BODY_LINES {
            return Err(malformed(format!(
                "the body has {} line(s); a checkpoint needs at least {REQUIRED_BODY_LINES} \
                 (origin, tree size, root hash)",
                lines.len()
            )));
        }

        let origin = std::str::from_utf8(lines[0])
            .map_err(|e| malformed(format!("the origin line is not UTF-8: {e}")))?;
        if origin.trim().is_empty() {
            return Err(malformed(
                "the origin line is empty, so the note names no log",
            ));
        }

        let tree_size = parse_tree_size(lines[1])?;
        let root_hash = parse_root_hash(lines[2])?;

        let mut signatures = Vec::new();
        for line in sig_block[..sig_block.len() - 1].split(|&c| c == b'\n') {
            // Refused before the allocation and before `verify` would try it,
            // so a hostile note costs the length of the note and not the length
            // of the note times an ECDSA verification. See MAX_SIGNATURE_LINES.
            if signatures.len() == MAX_SIGNATURE_LINES {
                return Err(malformed(format!(
                    "the note carries more than {MAX_SIGNATURE_LINES} signature lines; \
                     `note.Open` refuses the same note at the same count, and every extra line \
                     is an ECDSA verification an attacker chose for us"
                )));
            }
            signatures.push(parse_signature_line(line)?);
        }
        if signatures.is_empty() {
            return Err(malformed("the signature block contains no signature lines"));
        }

        Ok(Checkpoint {
            origin: origin.to_string(),
            tree_size,
            root_hash,
            signed_body: body.to_vec(),
            signatures,
        })
    }

    /// Verify at least one signature against the log's DER SubjectPublicKeyInfo.
    ///
    /// # Three outcomes, not two
    ///
    /// - `Ok(())` — some signature verified over [`Self::signed_body`].
    /// - [`Error::CheckpointSignatureInvalid`] — every signature was checked
    ///   and none verified.
    /// - **anything else** — the check could not run. The key was not one this
    ///   build can use, and the underlying error from
    ///   [`crate::x509::verify_ecdsa_spki`] is returned unchanged rather than
    ///   flattened into "invalid".
    ///
    /// That third case is live, not theoretical. Rekor's tiled logs
    /// (`log2025-1.rekor.sigstore.dev` and every `log2026-*` shard) sign their
    /// checkpoints with **Ed25519**, which this build has no verifier for; the
    /// trusted root labels them `PKIX_ED25519`. Reporting those as "signature
    /// invalid" would say the log lied when the truth is that we did not look.
    /// `an_ed25519_log_key_is_refused_as_unusable_not_as_a_bad_signature` holds
    /// that line.
    ///
    /// The key hint is not consulted. See [`rekor_v1_key_hint`] for why.
    pub fn verify(&self, log_spki_der: &[u8]) -> Result<()> {
        if self.signatures.is_empty() {
            return Err(malformed(
                "the checkpoint carries no signatures; an empty list satisfies \"at least one \
                 signature verifies\" vacuously",
            ));
        }
        for sig in &self.signatures {
            match crate::x509::verify_ecdsa_spki(log_spki_der, &self.signed_body, &sig.signature) {
                Ok(()) => return Ok(()),
                // This signature is not by this key. Try the next line: a
                // witnessed checkpoint carries several, and only one of them is
                // the log's own.
                Err(Error::SignatureInvalid) => continue,
                // The key itself could not be used. Every line would fail for
                // the same reason, and the reason is not "the log lied".
                Err(unusable) => return Err(unusable),
            }
        }
        Err(Error::CheckpointSignatureInvalid)
    }

    /// Does `hint` appear on any signature line?
    ///
    /// For picking which trusted-root key to *try*. Never a substitute for
    /// [`Self::verify`] — see [`rekor_v1_key_hint`].
    pub fn has_key_hint(&self, hint: [u8; 4]) -> bool {
        self.signatures.iter().any(|s| s.key_hint == hint)
    }
}

/// Last index at which `needle` occurs in `haystack`.
///
/// `slice::rsplit` would not do: the needle overlaps itself in `"\n\n\n"` and
/// this must agree with Go's `bytes.LastIndex`, which scans positions.
fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .rev()
        .find(|&i| &haystack[i..i + needle.len()] == needle)
}

fn is_ascii_control(c: u8) -> bool {
    c < 0x20 || c == 0x7f
}

/// The size line, as a decimal `u64`.
///
/// ASCII digits only. Rust's `u64::from_str` accepts a leading `+`, so `"+7"`
/// would otherwise parse here and not in Go's `strconv.ParseUint` — a parser
/// differential on the one field that says how big the tree is.
fn parse_tree_size(line: &[u8]) -> Result<u64> {
    if line.is_empty() || !line.iter().all(|c| c.is_ascii_digit()) {
        return Err(malformed(format!(
            "the tree size line {:?} is not a decimal number",
            String::from_utf8_lossy(line)
        )));
    }
    let mut n: u64 = 0;
    for &c in line {
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_add(u64::from(c - b'0')))
            .ok_or_else(|| {
                malformed(format!(
                    "the tree size {:?} does not fit in a u64",
                    String::from_utf8_lossy(line)
                ))
            })?;
    }
    Ok(n)
}

/// The root hash line: standard-alphabet base64 of exactly 32 bytes.
///
/// Strict — canonical padding, no trailing bits, no whitespace. Unlike
/// [`crate::bundle::b64_decode`], which tolerates line wrapping because real
/// bundles contain it, this field is a single line inside bytes a signature
/// covers: anything a lenient decoder would forgive is a byte the log did not
/// sign.
fn parse_root_hash(line: &[u8]) -> Result<[u8; 32]> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(line)
        .map_err(|e| {
            malformed(format!(
                "the root hash line {:?} is not base64: {e}",
                String::from_utf8_lossy(line)
            ))
        })?;
    <[u8; 32]>::try_from(raw.as_slice()).map_err(|_| {
        malformed(format!(
            "the root hash is {} bytes; RFC 6962 roots are 32 (SHA-256)",
            raw.len()
        ))
    })
}

fn parse_signature_line(line: &[u8]) -> Result<NoteSignature> {
    let line = std::str::from_utf8(line)
        .map_err(|e| malformed(format!("a signature line is not UTF-8: {e}")))?;
    let rest = line.strip_prefix(SIGNATURE_LINE_PREFIX).ok_or_else(|| {
        malformed(format!(
            "the signature line {line:?} does not begin with U+2014 EM DASH and a space"
        ))
    })?;
    let (key_name, blob_b64) = rest.split_once(' ').ok_or_else(|| {
        malformed(format!(
            "the signature line {line:?} has no space between the key name and the signature"
        ))
    })?;
    // `note.go:237-239`: a name is non-empty, carries no Unicode space, and
    // carries no `+`. The plus is not arbitrary — `note.NewVerifier` splits an
    // encoded verifier key on it (`:243`), so a name containing one names two
    // different keys depending on which parser reads it. Control characters are
    // already gone: `Checkpoint::parse` scans the whole note for them.
    if key_name.is_empty() || key_name.chars().any(char::is_whitespace) || key_name.contains('+') {
        return Err(malformed(format!("{key_name:?} is not a usable key name")));
    }
    let blob = base64::engine::general_purpose::STANDARD
        .decode(blob_b64)
        .map_err(|e| malformed(format!("the signature for {key_name:?} is not base64: {e}")))?;
    if blob.len() < MIN_SIGNATURE_BLOB {
        return Err(malformed(format!(
            "the signature blob for {key_name:?} is {} bytes; the 4-byte key hint leaves nothing \
             for a signature",
            blob.len()
        )));
    }
    let mut key_hint = [0u8; 4];
    key_hint.copy_from_slice(&blob[..4]);
    Ok(NoteSignature {
        key_name: key_name.to_string(),
        key_hint,
        signature: blob[4..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- real Rekor, captured so the suite runs offline ---------------------
    //
    // Every one of these was fetched live and checked against an independent
    // Python oracle (`cryptography`) before being frozen here, so a failure in
    // this module cannot be explained away as a bad fixture.

    const PROD: &str = include_str!("../fixtures/checkpoint-rekor-v1-prod.txt");
    const PROD_KEY_PEM: &str = include_str!("../fixtures/checkpoint-rekor-v1-prod-key.pem");
    const STAGING: &str = include_str!("../fixtures/checkpoint-rekor-v1-staging.txt");
    const STAGING_KEY_PEM: &str = include_str!("../fixtures/checkpoint-rekor-v1-staging-key.pem");
    /// The checkpoint embedded in a real prod `hashedrekord` inclusion proof —
    /// a different Rekor shard (tree `2605736670972794746`), same log key.
    const SHARD: &str = include_str!("../fixtures/checkpoint-rekor-v1-prod-shard.txt");
    /// The same shard checkpoint, re-fetched. Same body, different signature.
    const SHARD_REFETCHED: &str =
        include_str!("../fixtures/checkpoint-rekor-v1-prod-shard-refetched.txt");
    /// Rekor v2, tile-backed and witnessed: four signature lines, Ed25519.
    const V2_WITNESSED: &str = include_str!("../fixtures/checkpoint-rekor-v2-prod-witnessed.txt");
    const V2_KEY_PEM: &str = include_str!("../fixtures/checkpoint-rekor-v2-prod-key.pem");

    fn spki_der(pem: &str) -> Vec<u8> {
        let (label, doc) = der::Document::from_pem(pem).expect("fixture PEM");
        assert_eq!(label, "PUBLIC KEY");
        doc.as_bytes().to_vec()
    }

    // --- the thing this module exists to do --------------------------------

    #[test]
    fn a_real_production_rekor_checkpoint_verifies_against_the_real_log_key() {
        let cp = Checkpoint::parse(PROD).expect("parse");
        assert_eq!(cp.origin, "rekor.sigstore.dev - 1193050959916656506");
        assert_eq!(cp.tree_size, 2_594_242_780);
        assert_eq!(
            hex::encode(cp.root_hash),
            "7dc2cf4430ffed6d50c3d82249ff6211ae12ad9a0c822c5e26f58d04313c2e19"
        );
        assert_eq!(cp.signatures.len(), 1);
        assert_eq!(cp.signatures[0].key_name, "rekor.sigstore.dev");
        cp.verify(&spki_der(PROD_KEY_PEM)).expect("verify");
    }

    #[test]
    fn a_real_staging_rekor_checkpoint_verifies_against_the_real_staging_key() {
        let cp = Checkpoint::parse(STAGING).expect("parse");
        assert_eq!(cp.origin, "rekor.sigstage.dev - 8202293616175992157");
        assert_eq!(cp.tree_size, 24_201_842);
        cp.verify(&spki_der(STAGING_KEY_PEM)).expect("verify");
    }

    /// A checkpoint lifted out of a real `inclusionProof`, which is where the
    /// bundle path will actually find one.
    #[test]
    fn a_checkpoint_embedded_in_a_real_inclusion_proof_verifies() {
        let cp = Checkpoint::parse(SHARD).expect("parse");
        assert_eq!(cp.tree_size, 117_740_831);
        // The unsigned `inclusionProof.rootHash` of that same entry. Equal here
        // — which is the point: the checkpoint is what makes it evidence.
        assert_eq!(
            hex::encode(cp.root_hash),
            "2b9c578071e120aa84c9858783daa671565e4165f3a7dfa199aad81940299d8e"
        );
        cp.verify(&spki_der(PROD_KEY_PEM)).expect("verify");
    }

    // --- signed_body is exactly the covered bytes --------------------------

    /// The whole module turns on this. An off-by-one newline in either
    /// direction makes every real checkpoint fail, so the expected bytes are
    /// written out in full rather than derived from the parser under test, and
    /// the digest is the one an independent `cryptography`-based oracle printed
    /// for the same note.
    #[test]
    fn signed_body_is_the_body_including_its_trailing_newline_and_nothing_after_it() {
        let cp = Checkpoint::parse(PROD).expect("parse");
        assert_eq!(
            cp.signed_body,
            b"rekor.sigstore.dev - 1193050959916656506\n\
              2594242780\n\
              fcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhk=\n"
        );
        assert_eq!(
            hex::encode(Sha256::digest(&cp.signed_body)),
            "af79801b1da11257986c52abe3e184c460949d43e13f24a2b61a64739ea2679f"
        );
        assert!(cp.signed_body.ends_with(b"\n"));
        assert!(!cp.signed_body.ends_with(b"\n\n"));
        assert!(!cp
            .signed_body
            .windows(3)
            .any(|w| w == "\u{2014}".as_bytes()));
    }

    /// One byte more or one byte less, and nothing verifies. Proven rather than
    /// asserted, because "get this precisely right" is not a check.
    #[test]
    fn a_body_off_by_one_newline_in_either_direction_does_not_verify() {
        let key = spki_der(PROD_KEY_PEM);
        let good = Checkpoint::parse(PROD).expect("parse");
        good.verify(&key).expect("the unmodified body verifies");

        let mut short = good.clone();
        short.signed_body.pop();
        assert!(matches!(
            short.verify(&key),
            Err(Error::CheckpointSignatureInvalid)
        ));

        let mut long = good;
        long.signed_body.push(b'\n');
        assert!(matches!(
            long.verify(&key),
            Err(Error::CheckpointSignatureInvalid)
        ));
    }

    /// Two snapshots of one checkpoint: identical signed text, different
    /// signature bytes, both valid. This is the property that makes caching a
    /// whole checkpoint for equality wrong, and it is a live measurement, not a
    /// constructed one.
    #[test]
    fn two_snapshots_of_one_checkpoint_share_a_body_and_differ_in_signature() {
        let a = Checkpoint::parse(SHARD).expect("parse a");
        let b = Checkpoint::parse(SHARD_REFETCHED).expect("parse b");
        assert_eq!(a.signed_body, b.signed_body);
        assert_eq!(a.root_hash, b.root_hash);
        assert_ne!(a.signatures[0].signature, b.signatures[0].signature);
        assert_ne!(
            a.signatures[0].signature.len(),
            b.signatures[0].signature.len(),
            "the two captured DER signatures also differ in length (70 vs 71)"
        );
        let key = spki_der(PROD_KEY_PEM);
        a.verify(&key).expect("snapshot a verifies");
        b.verify(&key).expect("snapshot b verifies");
    }

    // --- negative: the body ------------------------------------------------

    #[test]
    fn a_tampered_root_hash_does_not_verify() {
        let key = spki_der(PROD_KEY_PEM);
        // Same length, same base64 alphabet, a different 32-byte root.
        let bad = PROD.replace(
            "fcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhk=",
            "fcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhg=",
        );
        let cp = Checkpoint::parse(&bad).expect("still a well-formed note");
        assert_ne!(cp.root_hash, Checkpoint::parse(PROD).unwrap().root_hash);
        assert!(matches!(
            cp.verify(&key),
            Err(Error::CheckpointSignatureInvalid)
        ));
    }

    #[test]
    fn a_tampered_tree_size_does_not_verify() {
        let key = spki_der(PROD_KEY_PEM);
        let bad = PROD.replace("\n2594242780\n", "\n2594242781\n");
        let cp = Checkpoint::parse(&bad).expect("still a well-formed note");
        assert_eq!(cp.tree_size, 2_594_242_781);
        assert!(matches!(
            cp.verify(&key),
            Err(Error::CheckpointSignatureInvalid)
        ));
    }

    #[test]
    fn a_tampered_origin_does_not_verify() {
        let key = spki_der(PROD_KEY_PEM);
        let bad = PROD.replace("rekor.sigstore.dev - ", "rekor.sigstore.dev - 9");
        let cp = Checkpoint::parse(&bad).expect("still a well-formed note");
        assert!(matches!(
            cp.verify(&key),
            Err(Error::CheckpointSignatureInvalid)
        ));
    }

    /// An extra body line is inside the signed bytes, so appending one is
    /// tampering — not "optional extra content" the verifier may ignore.
    #[test]
    fn an_appended_extra_body_line_does_not_verify() {
        let key = spki_der(PROD_KEY_PEM);
        let bad = PROD.replace("Lhk=\n\n", "Lhk=\nSize: 1\n\n");
        let cp = Checkpoint::parse(&bad).expect("extra body lines are legal");
        assert_eq!(cp.tree_size, 2_594_242_780);
        assert!(cp.signed_body.ends_with(b"Size: 1\n"));
        assert!(matches!(
            cp.verify(&key),
            Err(Error::CheckpointSignatureInvalid)
        ));
    }

    #[test]
    fn a_truncated_body_is_refused_before_any_signature_check() {
        // Root hash line gone: two body lines left.
        let bad = PROD.replace("\nfcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhk=\n\n", "\n\n");
        let e = Checkpoint::parse(&bad).unwrap_err();
        assert!(matches!(e, Error::CheckpointMalformed(_)), "{e}");
        assert_eq!(e.axis(), crate::Axis::TransparencyLog);
    }

    #[test]
    fn a_root_hash_that_is_not_32_bytes_is_refused() {
        let bad = PROD.replace("fcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhk=", "AAAA");
        let e = Checkpoint::parse(&bad).unwrap_err();
        assert!(
            format!("{e}").contains("RFC 6962 roots are 32"),
            "expected a length refusal, got {e}"
        );
    }

    #[test]
    fn a_root_hash_that_is_not_base64_is_refused() {
        let bad = PROD.replace(
            "fcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhk=",
            "not base64!!",
        );
        assert!(matches!(
            Checkpoint::parse(&bad),
            Err(Error::CheckpointMalformed(_))
        ));
    }

    /// Rust would happily parse `+2594242780`; Go would not. A size that two
    /// implementations read differently is a size nobody agreed on.
    #[test]
    fn a_tree_size_with_a_leading_plus_is_refused_even_though_rust_would_parse_it() {
        assert_eq!("+2594242780".parse::<u64>().ok(), Some(2_594_242_780));
        let bad = PROD.replace("\n2594242780\n", "\n+2594242780\n");
        assert!(matches!(
            Checkpoint::parse(&bad),
            Err(Error::CheckpointMalformed(_))
        ));
    }

    #[test]
    fn a_tree_size_beyond_u64_is_refused_rather_than_wrapped() {
        let bad = PROD.replace("\n2594242780\n", "\n18446744073709551616\n");
        let e = Checkpoint::parse(&bad).unwrap_err();
        assert!(format!("{e}").contains("does not fit in a u64"), "{e}");
    }

    #[test]
    fn an_empty_origin_is_refused() {
        let bad = PROD.replace("rekor.sigstore.dev - 1193050959916656506\n", "\n");
        let e = Checkpoint::parse(&bad).unwrap_err();
        assert!(format!("{e}").contains("names no log"), "{e}");
    }

    /// A note that came through something that rewrote line endings is a
    /// mangled note, and saying so beats reporting the log's signature as bad.
    #[test]
    fn a_crlf_mangled_body_is_named_as_mangled_not_as_a_bad_signature() {
        let bad = PROD.replace("\n2594242780\n", "\r\n2594242780\n");
        let e = Checkpoint::parse(&bad).unwrap_err();
        assert!(format!("{e}").contains("carriage return"), "{e}");
    }

    // --- negative: the signature block -------------------------------------

    #[test]
    fn a_note_with_no_blank_line_is_refused() {
        let bad = PROD.replace("Lhk=\n\n\u{2014}", "Lhk=\n\u{2014}");
        let e = Checkpoint::parse(&bad).unwrap_err();
        assert!(format!("{e}").contains("no blank line"), "{e}");
    }

    #[test]
    fn a_note_with_an_empty_signature_block_is_refused() {
        let body = "rekor.sigstore.dev - 1\n1\nfcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhk=\n";
        let e = Checkpoint::parse(&format!("{body}\n")).unwrap_err();
        assert!(format!("{e}").contains("signature block is empty"), "{e}");
    }

    /// The struct is constructible by hand, so the vacuous-pass guard has to
    /// live in `verify` too and not only in `parse`.
    #[test]
    fn verifying_a_checkpoint_with_zero_signatures_is_an_error_not_a_vacuous_pass() {
        let mut cp = Checkpoint::parse(PROD).expect("parse");
        cp.signatures.clear();
        let e = cp.verify(&spki_der(PROD_KEY_PEM)).unwrap_err();
        assert!(format!("{e}").contains("vacuously"), "{e}");
        assert_eq!(e.axis(), crate::Axis::TransparencyLog);
    }

    /// U+2013 and U+2014 differ by one byte and look the same in a diff.
    #[test]
    fn an_en_dash_or_a_hyphen_where_the_em_dash_belongs_is_refused() {
        assert_eq!(SIGNATURE_LINE_PREFIX.as_bytes(), b"\xe2\x80\x94 ");
        for wrong in ["\u{2013}", "-", "--", "\u{2015}"] {
            let bad = PROD.replace("\n\u{2014} ", &format!("\n{wrong} "));
            let e = Checkpoint::parse(&bad).unwrap_err();
            assert!(
                format!("{e}").contains("U+2014 EM DASH"),
                "{wrong:?} was accepted or misreported: {e}"
            );
        }
    }

    /// The boundary `MIN_SIGNATURE_BLOB` names is **four bytes versus five**,
    /// not three versus four. This test used to pass a 3-byte blob only, which
    /// meant the constant could be quietly lowered to 4 and nothing would fail:
    /// a 4-byte blob is all hint and no signature, and an empty signature never
    /// verifies, so the suite stayed green while the stated guarantee moved.
    /// Every length either side of the line is exercised here instead.
    #[test]
    fn a_signature_blob_too_short_to_hold_a_signature_is_refused() {
        const REAL_SIG_B64: &str = "wNI9ajBFAiBMiPeE7Cu33WMpJlvF9a1OrLo3ezCcoUNhwwOFZPgPiwIhANZjueIEegJYP70zO3UP2+hyCVFyfsG75lMyqw9SG/GN";
        let with_blob = |n: usize| {
            PROD.replace(
                REAL_SIG_B64,
                &base64::engine::general_purpose::STANDARD.encode(vec![0u8; n]),
            )
        };
        // 0..=4 bytes: the 4-byte key hint consumes the whole blob.
        for n in 0..MIN_SIGNATURE_BLOB {
            let e = Checkpoint::parse(&with_blob(n)).unwrap_err();
            assert!(
                format!("{e}").contains("leaves nothing for a signature"),
                "a {n}-byte blob was not refused as too short: {e}"
            );
        }
        // 5 bytes parses — one byte of "signature", which then fails to verify
        // rather than being refused as malformed. Two different failures.
        let cp = Checkpoint::parse(&with_blob(MIN_SIGNATURE_BLOB)).expect("5 bytes is a blob");
        assert_eq!(cp.signatures[0].signature, [0u8]);
        assert!(matches!(
            cp.verify(&spki_der(PROD_KEY_PEM)),
            Err(Error::CheckpointSignatureInvalid)
        ));
    }

    /// `note.Open` stops at 100 signature lines and says why
    /// (`sumdb/note/note.go:557`). Without the same cap, a bundle chooses how
    /// much ECDSA a verifier performs before it has trusted anything: 10 000
    /// well-formed DER lines in a 1.24 MB note measured 1.44 s of verification
    /// on this machine in release, 100 000 measured 11.0 s. The refusal has to
    /// happen in `parse`, because `verify` tries every line it is given.
    #[test]
    fn more_signature_lines_than_note_open_will_read_is_refused_at_parse_time() {
        let body = "example.log - 1\n7\nfcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhk=\n";
        let line = |i: usize| ("w", vec![i as u8; 8]);

        let ok: Vec<(&str, Vec<u8>)> = (0..MAX_SIGNATURE_LINES).map(line).collect();
        let cp = Checkpoint::parse(&signed_note(body, &ok))
            .expect("100 lines is the limit, not one past it");
        assert_eq!(cp.signatures.len(), MAX_SIGNATURE_LINES);

        let too_many: Vec<(&str, Vec<u8>)> = (0..MAX_SIGNATURE_LINES + 1).map(line).collect();
        let e = Checkpoint::parse(&signed_note(body, &too_many)).unwrap_err();
        assert!(
            format!("{e}").contains("more than 100 signature lines"),
            "{e}"
        );
        assert_eq!(e.axis(), crate::Axis::TransparencyLog);
    }

    /// The body is not the only place a control byte matters. `note.Open`
    /// scans the whole message (`note.go:510-516`), and this note is the
    /// difference: the body is untouched, so the log's real signature still
    /// verifies over it, and only the *name* attached to that signature has a
    /// NUL in it. Scanning the body alone accepted it and returned `Ok` from
    /// `verify` — a note Go calls malformed, called valid here.
    #[test]
    fn a_control_byte_in_the_signature_block_is_refused_though_the_body_is_untouched() {
        for (label, mangled) in [
            (
                "NUL in the key name",
                PROD.replace("\u{2014} rekor", "\u{2014} rekor\u{0}"),
            ),
            (
                "BEL in the key name",
                PROD.replace("\u{2014} rekor", "\u{2014} \u{7}rekor"),
            ),
            (
                "CR ending a signature line",
                PROD.replace("SG/GN\n", "SG/GN\r\n"),
            ),
        ] {
            let e = Checkpoint::parse(&mangled)
                .map(|_| ())
                .expect_err(&format!("{label} was accepted"));
            assert!(format!("{e}").contains("control byte"), "{label}: {e}");
        }
        // and the body's own signature was never the problem: unmangled, it verifies.
        Checkpoint::parse(PROD)
            .expect("parse")
            .verify(&spki_der(PROD_KEY_PEM))
            .expect("the same note, unmangled, is genuine");
    }

    /// `note.go:237-239` forbids `+` in a key name because
    /// `note.NewVerifier` splits an encoded verifier key on it — a name with a
    /// plus names two different keys depending on who reads it.
    #[test]
    fn a_key_name_containing_a_plus_is_refused_as_note_open_refuses_it() {
        let bad = PROD.replace("\u{2014} rekor", "\u{2014} +rekor");
        let e = Checkpoint::parse(&bad).unwrap_err();
        assert!(format!("{e}").contains("not a usable key name"), "{e}");
    }

    #[test]
    fn a_signature_line_with_no_key_name_is_refused() {
        let bad = PROD.replace("\u{2014} rekor.sigstore.dev ", "\u{2014}  ");
        assert!(matches!(
            Checkpoint::parse(&bad),
            Err(Error::CheckpointMalformed(_))
        ));
    }

    #[test]
    fn a_signature_block_not_ending_in_a_newline_is_refused() {
        let bad = PROD.trim_end_matches('\n');
        let e = Checkpoint::parse(bad).unwrap_err();
        assert!(format!("{e}").contains("truncated"), "{e}");
    }

    /// A checkpoint still wearing the JSON quotes it arrived in was not
    /// decoded. `sigstore` 0.14.0 strips them; stripping them hides the bug.
    #[test]
    fn a_checkpoint_still_wrapped_in_json_quotes_is_refused() {
        let bad = format!("\"{PROD}\"");
        assert!(matches!(
            Checkpoint::parse(&bad),
            Err(Error::CheckpointMalformed(_))
        ));
    }

    // --- negative: the cryptography ----------------------------------------

    #[test]
    fn the_staging_key_does_not_verify_a_production_checkpoint() {
        let cp = Checkpoint::parse(PROD).expect("parse");
        assert!(matches!(
            cp.verify(&spki_der(STAGING_KEY_PEM)),
            Err(Error::CheckpointSignatureInvalid)
        ));
        // and the converse, so this is not an accident of one direction
        let cp = Checkpoint::parse(STAGING).expect("parse");
        assert!(matches!(
            cp.verify(&spki_der(PROD_KEY_PEM)),
            Err(Error::CheckpointSignatureInvalid)
        ));
    }

    #[test]
    fn a_single_flipped_signature_bit_does_not_verify() {
        let key = spki_der(PROD_KEY_PEM);
        let good = Checkpoint::parse(PROD).expect("parse");
        good.verify(&key).expect("baseline");
        for i in 0..good.signatures[0].signature.len() {
            for bit in [0x01u8, 0x80u8] {
                let mut cp = good.clone();
                cp.signatures[0].signature[i] ^= bit;
                assert!(
                    matches!(cp.verify(&key), Err(Error::CheckpointSignatureInvalid)),
                    "flipping bit {bit:#04x} of signature byte {i} still verified"
                );
            }
        }
    }

    #[test]
    fn a_truncated_signature_does_not_verify() {
        let key = spki_der(PROD_KEY_PEM);
        let mut cp = Checkpoint::parse(PROD).expect("parse");
        cp.signatures[0].signature.truncate(20);
        assert!(matches!(
            cp.verify(&key),
            Err(Error::CheckpointSignatureInvalid)
        ));
    }

    // --- the key hint is a hint --------------------------------------------

    #[test]
    fn the_key_hint_of_a_v1_checkpoint_is_the_first_four_bytes_of_the_log_id() {
        let cp = Checkpoint::parse(PROD).expect("parse");
        let key = spki_der(PROD_KEY_PEM);
        // c0d23d6ad406973f… is the prod Rekor `logId.keyId` in the trusted root.
        assert_eq!(hex::encode(Sha256::digest(&key))[..8], *"c0d23d6a");
        assert_eq!(cp.signatures[0].key_hint, rekor_v1_key_hint(&key));
        assert!(cp.has_key_hint(rekor_v1_key_hint(&key)));
    }

    /// The hint is metadata, not evidence: corrupt it and a valid signature is
    /// still a valid signature. A verifier that filtered on it would refuse a
    /// genuine checkpoint the moment a log changed how it derives one — which
    /// Rekor v2 already has.
    #[test]
    fn a_wrong_key_hint_does_not_stop_a_valid_signature_from_verifying() {
        let mut cp = Checkpoint::parse(PROD).expect("parse");
        cp.signatures[0].key_hint = [0xde, 0xad, 0xbe, 0xef];
        cp.verify(&spki_der(PROD_KEY_PEM))
            .expect("the signature is what decides");
    }

    /// The Ed25519 tiled log's hint is *not* `SHA-256(SPKI)`, so the v1 rule
    /// does not generalise. Measured against the real prod trusted root.
    #[test]
    fn the_v1_hint_derivation_does_not_hold_for_the_ed25519_tiled_log() {
        let cp = Checkpoint::parse(V2_WITNESSED).expect("parse");
        let key = spki_der(V2_KEY_PEM);
        assert_eq!(hex::encode(cp.signatures[0].key_hint), "cf119915");
        assert_eq!(hex::encode(rekor_v1_key_hint(&key)), "b54813cb");
        assert_ne!(cp.signatures[0].key_hint, rekor_v1_key_hint(&key));
    }

    // --- Rekor v2: witnessed, and out of this build's reach -----------------

    /// Four signature lines, single-newline separated. `sigstore` 0.14.0 cannot
    /// parse this shape at all (see the module docs); it must not be a shape
    /// that quietly parses as *one* signature either.
    #[test]
    fn a_real_witnessed_rekor_v2_checkpoint_parses_all_four_signature_lines() {
        let cp = Checkpoint::parse(V2_WITNESSED).expect("parse");
        assert_eq!(cp.origin, "log2025-1.rekor.sigstore.dev");
        assert_eq!(cp.tree_size, 96_629_173);
        let names: Vec<&str> = cp.signatures.iter().map(|s| s.key_name.as_str()).collect();
        assert_eq!(
            names,
            [
                "log2025-1.rekor.sigstore.dev",
                "witness.stagemole.eu",
                "witness.navigli.sunlight.geomys.org",
                "staging.witness.transparency.goog/ring-any-bells",
            ]
        );
        // The log signs 64 bytes (Ed25519); the witnesses prepend an 8-byte
        // cosignature timestamp, which is why theirs are 72.
        assert_eq!(cp.signatures[0].signature.len(), 64);
        assert!(cp.signatures[1..].iter().all(|s| s.signature.len() == 72));
        assert!(cp
            .signed_body
            .ends_with(b"eFKDJE0nk/r0KAEfau4GNAHKXFGXwkBFEoKyWVRZtJA=\n"));
    }

    /// The failure this whole error type exists to prevent, in its most
    /// specific form: an Ed25519 log key reaching an ECDSA-only verifier must
    /// produce "could not check", never "checked and it passed", and never the
    /// libel of "the log's signature is invalid".
    #[test]
    fn an_ed25519_log_key_is_refused_as_unusable_not_as_a_bad_signature() {
        let cp = Checkpoint::parse(V2_WITNESSED).expect("parse");
        let e = cp.verify(&spki_der(V2_KEY_PEM)).unwrap_err();
        assert!(
            !matches!(e, Error::CheckpointSignatureInvalid),
            "an unverifiable algorithm was reported as a bad signature: {e}"
        );
        assert!(format!("{e}").contains("ECDSA only"), "{e}");
    }

    // --- multi-signature semantics, on keys we control ---------------------

    /// Real v1 checkpoints carry one signature, so "at least one verifies" is
    /// exercised here on a note this test signs itself.
    fn signed_note(body: &str, sigs: &[(&str, Vec<u8>)]) -> String {
        let mut s = format!("{body}\n");
        for (name, blob) in sigs {
            s.push_str(SIGNATURE_LINE_PREFIX);
            s.push_str(name);
            s.push(' ');
            s.push_str(&base64::engine::general_purpose::STANDARD.encode(blob));
            s.push('\n');
        }
        s
    }

    fn p256_key(seed: u8) -> (p256::ecdsa::SigningKey, Vec<u8>) {
        use spki::EncodePublicKey;
        let sk = p256::ecdsa::SigningKey::from_bytes(&[seed; 32].into()).expect("seed");
        let spki = sk
            .verifying_key()
            .to_public_key_der()
            .expect("encode")
            .as_bytes()
            .to_vec();
        (sk, spki)
    }

    #[test]
    fn one_valid_signature_among_several_is_enough_whatever_its_position() {
        use p256::ecdsa::{signature::Signer, DerSignature};
        let body = "example.log - 1\n7\nfcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhk=\n";
        let (sk, spki) = p256_key(9);
        let sig: DerSignature = sk.sign(body.as_bytes());
        let mut real = vec![0xaa, 0xbb, 0xcc, 0xdd];
        real.extend_from_slice(sig.as_bytes());
        let junk = ("witness.example", vec![1u8; 40]);

        for order in [
            vec![("log.example", real.clone()), junk.clone()],
            vec![junk.clone(), ("log.example", real.clone())],
            vec![junk.clone(), ("log.example", real), junk],
        ] {
            let note = signed_note(body, &order);
            let cp = Checkpoint::parse(&note).expect("parse");
            assert_eq!(cp.signed_body, body.as_bytes());
            cp.verify(&spki).expect("one good signature is enough");
        }
    }

    #[test]
    fn a_note_where_every_signature_is_junk_is_refused() {
        let body = "example.log - 1\n7\nfcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhk=\n";
        let (_, spki) = p256_key(9);
        let note = signed_note(body, &[("a", vec![7u8; 40]), ("b", vec![8u8; 71])]);
        let cp = Checkpoint::parse(&note).expect("parse");
        assert_eq!(cp.signatures.len(), 2);
        assert!(matches!(
            cp.verify(&spki),
            Err(Error::CheckpointSignatureInvalid)
        ));
    }

    /// A blank line inside the body is legal note text. Splitting at the
    /// *first* `"\n\n"` would cut the body short here and the signature would
    /// then fail — a false rejection dressed as tampering.
    #[test]
    fn a_blank_line_inside_the_body_does_not_truncate_it() {
        use p256::ecdsa::{signature::Signer, DerSignature};
        let body = "example.log - 1\n7\nfcLPRDD/7W1Qw9giSf9iEa4SrZoMgixeJvWNBDE8Lhk=\n\ntrailer\n";
        let (sk, spki) = p256_key(11);
        let sig: DerSignature = sk.sign(body.as_bytes());
        let mut blob = vec![0u8; 4];
        blob.extend_from_slice(sig.as_bytes());
        let note = signed_note(body, &[("log.example", blob)]);
        let cp = Checkpoint::parse(&note).expect("parse");
        assert_eq!(cp.signed_body, body.as_bytes());
        assert_eq!(cp.tree_size, 7);
        cp.verify(&spki).expect("verify over the whole body");
    }

    #[test]
    fn find_last_agrees_with_a_naive_scan_on_overlapping_needles() {
        assert_eq!(find_last(b"\n\n\n", b"\n\n"), Some(1));
        assert_eq!(find_last(b"a\n\nb\n\nc", b"\n\n"), Some(4));
        assert_eq!(find_last(b"abc", b"\n\n"), None);
        assert_eq!(find_last(b"\n", b"\n\n"), None);
        assert_eq!(find_last(b"", b"\n\n"), None);
    }
}
