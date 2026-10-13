//! Rekor v1: uploading a DSSE entry and reading it back with its inclusion
//! proof.
//!
//! # There are two log indices and they are not interchangeable
//!
//! Every Rekor entry carries **two** numbers called `logIndex`:
//!
//! - the **global** one, at the top level, counting entries across every shard
//!   the deployment has ever had;
//! - the **shard-local** one, inside `verification.inclusionProof`, counting
//!   entries within the tree the proof is against.
//!
//! The Merkle proof is against the shard's tree, so it needs the shard-local
//! index. The Signed Entry Timestamp is computed over a payload containing the
//! **global** one. Using the wrong index in either place is silent:
//!
//! Measured on `fixtures/net/rekor-staging-entry-hashedrekord.json`, a real
//! staging entry — global `24201800`, shard-local `24186981`, tree size
//! `31667593`:
//!
//! ```text
//! decomp(shard-local) : inner=24 border=1 sum=25   len(hashes)=25   PASSES
//! decomp(global)      : inner=24 border=1 sum=25   len(hashes)=25   PASSES
//! shard-local -> root ae6af751…56f1   MATCH   (== the checkpoint's root)
//! global      -> root e38bd0fb…7885   NO MATCH
//! ```
//!
//! **The proof-length check does not catch it.** Both indices produce a
//! well-shaped proof; only the root differs, and a verifier that computes a
//! root and then does not compare it to the *checkpoint's* root sees nothing
//! wrong at all. `wrong_log_index_computes_a_different_root_and_the_shape_check_misses_it`
//! is that measurement as a test.
//!
//! (On the production DSSE entry in `fixtures/net/rekor-prod-entry-dsse.json`
//! the shapes happen to differ — 30 hashes wanted, 21 computed — so *that* one
//! is caught by length. Which is exactly why length cannot be relied on: it
//! catches the mistake on some entries and not others.)
//!
//! # The UUID is the leaf hash
//!
//! A sharded entry UUID is 80 hex characters: 16 of tree id, then 64 of
//! `SHA256(0x00 ‖ canonicalizedBody)`. So the identifier you asked for and the
//! bytes you got back are checkable against each other without any signature at
//! all, which is what [`LogEntry::check_uuid_matches_body`] does. Verified
//! against both the staging and production fixtures.
//!
//! It is 80 **hex characters**, not 80 bytes, and that distinction is enforced
//! by [`uuid_parts`] before anything indexes into it — see its docs for the
//! panic that used to be reachable from a server's choice of JSON key.
//!
//! # Being the entry the UUID names is not the same as being *our* entry
//!
//! Three separate things could each be true of an answer from Rekor, and only
//! all three together mean the entry belongs in our bundle:
//!
//! 1. the entry is internally consistent — its UUID names its body, its proof
//!    reconstructs its root ([`LogEntry::check_uuid_matches_body`],
//!    [`LogEntry::check_self_consistent`]);
//! 2. it is the entry that was **asked for** ([`fetch_entry`]);
//! 3. it is the entry for the **envelope that was posted**
//!    ([`check_entry_names_envelope`]).
//!
//! A real, correctly signed, genuinely-in-the-log entry belonging to somebody
//! else satisfies (1) completely. sigstore-conformance ships one packaged with
//! the wrong envelope as a negative fixture, and it is a fixture here.
//!
//! # Entries are not byte-stable, so never compare them
//!
//! Rekor re-signs the Signed Entry Timestamp and the checkpoint on **every
//! fetch**, and ECDSA is randomized. Two GETs of the same UUID minutes apart
//! return different signature bytes over identical signed content:
//! `fixtures/net/rekor-staging-entry-hashedrekord.json` and
//! `…-refetch.json` are two real fetches of the same entry, and
//! `two_fetches_of_one_entry_differ_in_signature_but_not_in_content` asserts
//! exactly which fields moved. Caching or diffing whole entries produces
//! spurious "the log changed" alarms.

use serde::Deserialize;

use crate::dsse::Envelope;
use crate::error::{Error, Result};
use crate::net::Transport;

/// `POST` to create, `GET /{uuid}` to read back.
pub const ENTRIES_PATH: &str = "/api/v1/log/entries";
/// `GET` the current tree state, including every inactive shard.
pub const LOG_PATH: &str = "/api/v1/log";

/// The entry kind cosign and sigstore-python emit for a DSSE attestation.
pub const KIND_DSSE: &str = "dsse";
/// The kind that is roughly a fifth of live traffic; read, never written here.
pub const KIND_HASHEDREKORD: &str = "hashedrekord";
pub const API_VERSION_V001: &str = "0.0.1";

/// The **shard-local** index: where the entry sits in the tree its inclusion
/// proof is against.
///
/// A newtype so it cannot be passed where [`GlobalIndex`] belongs by being the
/// same primitive. It does not stop `.0` being taken deliberately — nothing
/// can — but it makes every crossing visible at the call site, which is where
/// the mistake is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShardIndex(pub u64);

/// The **global** index: the entry's position across every shard. This is the
/// number the Signed Entry Timestamp covers and the number the bundle's
/// top-level `logIndex` carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GlobalIndex(pub u64);

#[derive(Debug, Clone, Deserialize)]
pub struct InclusionProof {
    /// **Shard-local.** See the module docs.
    #[serde(rename = "logIndex")]
    pub log_index: u64,
    #[serde(rename = "rootHash")]
    pub root_hash: String,
    #[serde(rename = "treeSize")]
    pub tree_size: u64,
    #[serde(default)]
    pub hashes: Vec<String>,
    /// The signed note, as one string with embedded newlines.
    #[serde(default)]
    pub checkpoint: String,
}

impl InclusionProof {
    pub fn shard_index(&self) -> ShardIndex {
        ShardIndex(self.log_index)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verification {
    #[serde(rename = "signedEntryTimestamp", default)]
    pub signed_entry_timestamp: String,
    #[serde(rename = "inclusionProof", default)]
    pub inclusion_proof: Option<InclusionProof>,
}

/// One entry, as `GET /api/v1/log/entries/{uuid}` returns it.
///
/// The REST API wraps entries in a `{uuid: entry}` object, so `uuid` is filled
/// from the map key rather than from the entry body — it is not a field of the
/// entry itself.
#[derive(Debug, Clone, Deserialize)]
pub struct LogEntry {
    #[serde(skip)]
    pub uuid: String,
    /// Base64 of the canonicalized body. **Used verbatim** for the leaf hash:
    /// re-serializing the decoded JSON first changes the bytes for some entry
    /// kinds and not others.
    pub body: String,
    #[serde(rename = "integratedTime")]
    pub integrated_time: i64,
    /// Lowercase hex. The bundle wants this base64; see
    /// [`crate::net::assemble_bundle`].
    #[serde(rename = "logID")]
    pub log_id: String,
    /// **Global.** See the module docs.
    #[serde(rename = "logIndex")]
    pub log_index: u64,
    #[serde(default)]
    pub verification: Option<Verification>,
}

/// A Rekor entry UUID, split into the two halves that mean something.
///
/// `shard` is the 16-character tree id of a sharded deployment, absent on the
/// 64-character unsharded form. `entry_id` is always the 64 characters that
/// *are* `SHA256(0x00 ‖ canonicalizedBody)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UuidParts<'a> {
    shard: Option<&'a str>,
    entry_id: &'a str,
}

/// Split a UUID, refusing anything that is not hexadecimal.
///
/// # This is where "80 characters" stopped meaning "80 bytes"
///
/// [`LogEntry::uuid`] is filled from the *key of a JSON object the server
/// sent*, so it is an arbitrary UTF-8 string until something says otherwise.
/// The previous version of this code branched on `uuid.len()` — a byte count —
/// and then reached for `&uuid[16..]`. A key of `"a"×15 ‖ "é" ‖ "b"×63` is 80
/// bytes with a character boundary *inside* byte 16, and slicing it does not
/// return an error, it **panics**:
///
/// ```text
/// start byte index 16 is not a char boundary; it is inside 'é' (bytes 15..17)
/// ```
///
/// A Rekor that returned such a key — hostile, proxied, or merely buggy — would
/// abort the signing process from inside a library that is not allowed to
/// panic. `shard_tree_id` had the same edge at `&uuid[..16]`.
///
/// Requiring hex first fixes both: once every byte is an ASCII hex digit, byte
/// indices *are* character indices, and the split is total.
fn uuid_parts(uuid: &str) -> Result<UuidParts<'_>> {
    if !uuid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::CheckpointMalformed(format!(
            "a Rekor entry UUID is hexadecimal; {uuid:?} is not"
        )));
    }
    match uuid.len() {
        80 => Ok(UuidParts {
            shard: Some(&uuid[..16]),
            entry_id: &uuid[16..],
        }),
        64 => Ok(UuidParts {
            shard: None,
            entry_id: uuid,
        }),
        n => Err(Error::CheckpointMalformed(format!(
            "a Rekor entry UUID is 64 or 80 hex characters; this one is {n}"
        ))),
    }
}

impl LogEntry {
    pub fn global_index(&self) -> GlobalIndex {
        GlobalIndex(self.log_index)
    }

    pub fn inclusion_proof(&self) -> Option<&InclusionProof> {
        self.verification.as_ref()?.inclusion_proof.as_ref()
    }

    /// The canonicalized body's raw bytes.
    pub fn body_bytes(&self) -> Result<Vec<u8>> {
        crate::bundle::b64_decode("logEntry.body", &self.body)
    }

    /// `(kind, apiVersion)` — e.g. `("dsse", "0.0.1")`.
    ///
    /// Read from the body rather than assumed, because the bundle must record
    /// the kind the log actually stored and a client that hard-codes its own
    /// writes a `kindVersion` that does not describe the
    /// `canonicalizedBody` beside it.
    pub fn kind_version(&self) -> Result<(String, String)> {
        #[derive(Deserialize)]
        struct KV {
            kind: String,
            #[serde(rename = "apiVersion")]
            api_version: String,
        }
        let kv: KV = serde_json::from_slice(&self.body_bytes()?).map_err(|e| {
            Error::CheckpointMalformed(format!("the entry body names no kind/apiVersion: {e}"))
        })?;
        Ok((kv.kind, kv.api_version))
    }

    /// `SHA256(0x00 ‖ body)`, the RFC 6962 leaf hash.
    pub fn leaf_hash(&self) -> Result<[u8; 32]> {
        Ok(crate::merkle::leaf_hash(&self.body_bytes()?))
    }

    /// The tree id in the first 16 hex characters of a sharded UUID.
    ///
    /// `Ok(None)` for a 64-character UUID, which is the unsharded form — *not*
    /// an error, and not a zero: an absent shard prefix is a fact about the
    /// deployment, and returning `0` would make it look like tree zero.
    ///
    /// A UUID that is *unreadable* is a third thing again, and it is an
    /// `Err`. It used to be `None`, which is the value that means "this
    /// deployment is not sharded": a caller looking for the shard's checkpoint
    /// would have gone looking for an unsharded log and found nothing wrong.
    /// An unreadable shard id is not an absent one.
    pub fn shard_tree_id(&self) -> Result<Option<u64>> {
        match uuid_parts(&self.uuid)?.shard {
            None => Ok(None),
            Some(hex) => u64::from_str_radix(hex, 16).map(Some).map_err(|e| {
                Error::CheckpointMalformed(format!(
                    "the shard prefix {hex:?} of a Rekor UUID is not a tree id: {e}"
                ))
            }),
        }
    }

    /// Does the UUID name this body?
    ///
    /// The tail of a Rekor UUID *is* the leaf hash, so this catches a server
    /// (or a cache, or a proxy) returning a different entry from the one asked
    /// for — without needing any key material. A cheap check that runs before
    /// any signature check is worth having, and it is a distinct failure from
    /// "the proof does not verify".
    pub fn check_uuid_matches_body(&self) -> Result<()> {
        let tail = uuid_parts(&self.uuid)?.entry_id;
        if tail.eq_ignore_ascii_case(&hex::encode(self.leaf_hash()?)) {
            Ok(())
        } else {
            Err(Error::LogEntryMismatch)
        }
    }

    /// Recompute the tree root this entry's proof implies.
    ///
    /// **Uses the shard-local index**, and there is deliberately no variant of
    /// this function that lets a caller supply an index. See the module docs
    /// for the measurement that says why.
    pub fn recompute_root(&self) -> Result<[u8; 32]> {
        let proof = self.inclusion_proof().ok_or(Error::InclusionProofMissing)?;
        let leaf = self.leaf_hash()?;
        let mut hashes = Vec::with_capacity(proof.hashes.len());
        for h in &proof.hashes {
            let raw = hex::decode(h).map_err(|e| Error::Base64 {
                field: "inclusionProof.hashes[]",
                reason: format!("Rekor returns proof hashes as hex, and this is not hex: {e}"),
            })?;
            let arr: [u8; 32] = raw.try_into().map_err(|_| Error::Base64 {
                field: "inclusionProof.hashes[]",
                reason: "a proof hash is not 32 bytes".to_string(),
            })?;
            hashes.push(arr);
        }
        crate::merkle::root_from_inclusion_proof(
            proof.shard_index().0,
            proof.tree_size,
            &leaf,
            &hashes,
        )
    }

    /// Check the recomputed root against the proof's own `rootHash`.
    ///
    /// # This is self-consistency, not proof of inclusion
    ///
    /// `rootHash` is an unsigned field in a document the artifact supplies, so
    /// matching it proves only that the proof hangs together. What ties the
    /// root to the log's identity is the **checkpoint's** signature over it,
    /// which is [`crate::checkpoint`]'s job and is deliberately not duplicated
    /// here. Named `check_self_consistent` rather than `verify_inclusion` so no
    /// caller mistakes one for the other.
    pub fn check_self_consistent(&self) -> Result<[u8; 32]> {
        let proof = self.inclusion_proof().ok_or(Error::InclusionProofMissing)?;
        let computed = self.recompute_root()?;
        let expected = hex::decode(&proof.root_hash).map_err(|e| Error::Base64 {
            field: "inclusionProof.rootHash",
            reason: format!("Rekor returns rootHash as hex, and this is not hex: {e}"),
        })?;
        if expected != computed {
            return Err(Error::InclusionProofInvalid {
                computed: hex::encode(computed),
                expected: proof.root_hash.clone(),
            });
        }
        Ok(computed)
    }
}

/// Parse a `{uuid: entry}` response into the single entry it holds.
///
/// Rekor returns a map even for a single entry. More than one key means the
/// caller asked for something this function does not model, and is refused
/// rather than silently taking the first — map iteration order is not the
/// order the server wrote.
pub fn parse_entry_response(body: &str) -> Result<LogEntry> {
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(body).map_err(|e| Error::RekorRejected {
            status: 0,
            body: format!("the log entry response is not a JSON object: {e}"),
        })?;
    let mut it = map.into_iter();
    let (uuid, value) = it.next().ok_or_else(|| Error::RekorRejected {
        status: 0,
        body: "the log entry response is an empty object; the entry was not returned".to_string(),
    })?;
    if it.next().is_some() {
        return Err(Error::RekorRejected {
            status: 0,
            body: "the log entry response holds more than one entry".to_string(),
        });
    }
    let mut entry: LogEntry = serde_json::from_value(value)?;
    entry.uuid = uuid;
    Ok(entry)
}

/// The `dsse` v0.0.1 proposed entry.
///
/// # The envelope is a *string*, not an object
///
/// `dsse_v0_0_1_schema.json` types `proposedContent.envelope` as
/// `"type": "string"` — a stringified JSON object. Sending the envelope as a
/// nested object is a schema violation Rekor rejects, and the mistake looks
/// like a correct request in every editor.
///
/// `verifiers` is an array of base64-encoded PEM certificates. Rekor's own
/// error for getting that wrong is `invalid public key: failure decoding PEM`
/// (recorded in `fixtures/net/rekor-staging-upload-400.json`, from a probe that
/// sent a non-PEM key), which at least says what it means.
pub fn dsse_proposed_entry(envelope: &Envelope, verifier_pem: &str) -> Result<serde_json::Value> {
    let envelope_json = serde_json::to_string(envelope)?;
    Ok(serde_json::json!({
        "kind": KIND_DSSE,
        "apiVersion": API_VERSION_V001,
        "spec": {
            "proposedContent": {
                "envelope": envelope_json,
                "verifiers": [crate::bundle::b64_encode(verifier_pem.as_bytes())],
            }
        }
    }))
}

/// Upload a DSSE envelope and return the entry, **with its inclusion proof**.
///
/// # Two round trips, on purpose
///
/// The 201 from a create carries the entry, but the inclusion proof in it is
/// whatever the log could produce at that instant — for a log that batches
/// leaves into a tree head every few seconds, that can be absent. A bundle
/// without an inclusion proof is exactly the artifact this crate exists to
/// refuse, so the entry is re-fetched by UUID and the result is checked for
/// one.
///
/// # A 409 is not a failure
///
/// Rekor answers a duplicate with `409` and a `Location` header naming the
/// entry that already exists. The same payload signed twice by the same key is
/// a normal event — a re-run of a build, a retried upload — and treating it as
/// an error would make signing non-idempotent for no gain. The existing entry
/// is fetched instead, and it is the same entry by construction: the UUID is
/// the leaf hash.
pub fn upload_dsse(
    t: &dyn Transport,
    rekor_url: &str,
    envelope: &Envelope,
    verifier_pem: &str,
) -> Result<LogEntry> {
    let base = rekor_url.trim_end_matches('/');
    let url = format!("{base}{ENTRIES_PATH}");
    let r = t.post_json(
        &url,
        &dsse_proposed_entry(envelope, verifier_pem)?,
        "uploading a DSSE entry to Rekor",
    )?;

    let uuid = if r.status == 409 {
        // The Location is a path like /api/v1/log/entries/<uuid>.
        let loc = r.location.clone().ok_or_else(|| Error::RekorRejected {
            status: r.status,
            body: format!(
                "Rekor reported a duplicate entry but sent no Location header to find it: {}",
                r.body
            ),
        })?;
        loc.rsplit('/').next().unwrap_or_default().to_string()
    } else if r.is_success() {
        parse_entry_response(&r.body)?.uuid
    } else {
        return Err(Error::RekorRejected {
            status: r.status,
            body: r.body,
        });
    };

    if uuid.is_empty() {
        return Err(Error::RekorRejected {
            status: r.status,
            body: "Rekor's response named no entry UUID".to_string(),
        });
    }

    let entry = fetch_entry(t, rekor_url, &uuid)?;

    // We uploaded a `dsse` entry, so the entry we go on to put in the bundle
    // must be one — and it must be one **for our envelope**. `fetch_entry` has
    // established that the entry is internally consistent and is the one this
    // UUID names, but a *different* entry, correctly signed and genuinely in
    // the log, satisfies both of those.
    let (kind, _version) = entry.kind_version()?;
    if kind != KIND_DSSE {
        return Err(Error::LogEntryMismatch);
    }
    check_entry_names_envelope(&entry, envelope)?;

    if entry.inclusion_proof().is_none() {
        return Err(Error::InclusionProofMissing);
    }
    Ok(entry)
}

/// Is this log entry the entry for **this** envelope?
///
/// # The rule, and where it was measured rather than read
///
/// A stored `dsse` v0.0.1 body carries `spec.payloadHash`, which Rekor computes
/// as `sha256` of the *decoded* DSSE payload (`dsse_v001.go:277,299,355`). This
/// used to be left unchecked, on the argument that a misreading of Rekor's Go
/// source would become a signer that always fails having already written to the
/// public log — a fair worry about a rule derived from source alone.
///
/// It is no longer derived. The equality
///
/// ```text
/// spec.payloadHash.value == hex(sha256(base64_decode(dsseEnvelope.payload)))
/// ```
///
/// holds on **11 of 11** real `dsse` tlog entries paired with their own
/// envelopes across three independent corpora (sigstore-conformance,
/// sigstore-python and sigstore-rs test bundles), and *fails* on
/// sigstore-conformance's own `dsse-mismatch-envelope_fail` bundle — an entry
/// logged for one payload and packaged with another, which is precisely the
/// substitution this function exists to refuse. Both of those are fixtures
/// here (`rekor-dsse-entry-matching-envelope.json`,
/// `rekor-dsse-entry-mismatched-envelope.json`) and both branches are tested.
///
/// # An absent hash is a refusal
///
/// A body with no `payloadHash`, or one under an algorithm this build does not
/// compute, is refused by name rather than passed. The alternative — skip the
/// check when the field is missing — is a check that silently succeeds exactly
/// when it has nothing to work with, which is how an unverified artifact comes
/// to look verified.
pub fn check_entry_names_envelope(entry: &LogEntry, envelope: &Envelope) -> Result<()> {
    use sha2::{Digest, Sha256};

    let body: serde_json::Value = serde_json::from_slice(&entry.body_bytes()?)?;
    let hash = body.pointer("/spec/payloadHash").ok_or_else(|| {
        Error::CheckpointMalformed(
            "the stored dsse entry carries no spec.payloadHash, so nothing in it says which \
             payload was logged"
                .to_string(),
        )
    })?;
    let algorithm = hash.get("algorithm").and_then(|a| a.as_str());
    if algorithm != Some("sha256") {
        return Err(Error::CheckpointMalformed(format!(
            "the stored dsse entry's payloadHash is under algorithm {algorithm:?}, which this \
             build does not compute; it cannot be compared, and an uncompared hash is not a \
             matching one"
        )));
    }
    let logged = hash.get("value").and_then(|v| v.as_str()).ok_or_else(|| {
        Error::CheckpointMalformed(
            "the stored dsse entry's payloadHash carries no value".to_string(),
        )
    })?;
    let ours = hex::encode(Sha256::digest(envelope.payload_bytes()?));
    if !logged.eq_ignore_ascii_case(&ours) {
        return Err(Error::LogEntryMismatch);
    }
    Ok(())
}

/// Read one entry back, with its inclusion proof.
///
/// Two identity checks run before the entry is returned, and both are cheap
/// enough that there is no excuse for skipping either:
///
/// 1. the entry that came back is the entry that was asked for — a response
///    keyed by a different UUID is a different entry, however well-formed;
/// 2. the UUID names the body it arrived with — see
///    [`LogEntry::check_uuid_matches_body`].
///
/// The requested id may be the 80-character sharded UUID or the 64-character
/// entry id; Rekor keys its response by the sharded form either way, so the two
/// are compared on their **entry id** — the 64 hex characters both forms carry.
///
/// # Why not `ends_with`
///
/// This check used to be `entry.uuid.ends_with(requested)`, which is a
/// *suffix* test and is therefore satisfied by any short requested id. Measured:
/// `fetch_entry(t, url, "")` accepted a completely unrelated production entry
/// and returned it as the entry that was asked for, because every string ends
/// with the empty one; `"1"` did the same. The empty case is reachable — a 409
/// whose `Location` ends in a bare `/` yields an empty id — and the only thing
/// standing in front of it was an `is_empty` guard in one caller, which no test
/// covered (deleting it changed no test result). Comparing whole entry ids
/// makes the check total instead of leaving one caller responsible for it.
pub fn fetch_entry(t: &dyn Transport, rekor_url: &str, uuid: &str) -> Result<LogEntry> {
    // Refuse a malformed id before it becomes a URL: an id this function
    // cannot compare is an id it cannot check the answer against.
    let asked = uuid_parts(uuid)?;
    let url = format!("{}{ENTRIES_PATH}/{uuid}", rekor_url.trim_end_matches('/'));
    let r = t.get(&url, "fetching a Rekor log entry")?;
    if !r.is_success() {
        return Err(Error::RekorRejected {
            status: r.status,
            body: r.body,
        });
    }
    let entry = parse_entry_response(&r.body)?;
    let got = uuid_parts(&entry.uuid)?;
    if !got.entry_id.eq_ignore_ascii_case(asked.entry_id) {
        return Err(Error::LogEntryMismatch);
    }
    // If both forms name a shard, they must name the same one. The same entry
    // id in a different tree is a different entry.
    if let (Some(a), Some(b)) = (asked.shard, got.shard) {
        if !a.eq_ignore_ascii_case(b) {
            return Err(Error::LogEntryMismatch);
        }
    }
    entry.check_uuid_matches_body()?;
    Ok(entry)
}

/// One shard's signed tree head.
#[derive(Debug, Clone, Deserialize)]
pub struct Shard {
    #[serde(rename = "rootHash", default)]
    pub root_hash: String,
    #[serde(rename = "signedTreeHead", default)]
    pub signed_tree_head: String,
    #[serde(rename = "treeID", default)]
    pub tree_id: String,
    #[serde(rename = "treeSize", default)]
    pub tree_size: u64,
}

/// `GET /api/v1/log`: the active tree, plus every shard retired before it.
///
/// The inactive shards are not trivia. A bundle's inclusion proof is against
/// whichever shard held the entry, so an entry old enough to predate the
/// current tree verifies against a root that appears **only** in this list.
#[derive(Debug, Clone, Deserialize)]
pub struct LogState {
    #[serde(rename = "rootHash", default)]
    pub root_hash: String,
    #[serde(rename = "signedTreeHead", default)]
    pub signed_tree_head: String,
    #[serde(rename = "treeID", default)]
    pub tree_id: String,
    #[serde(rename = "treeSize", default)]
    pub tree_size: u64,
    #[serde(rename = "inactiveShards", default)]
    pub inactive_shards: Vec<Shard>,
}

impl LogState {
    pub fn parse(body: &str) -> Result<LogState> {
        serde_json::from_str(body).map_err(Error::Json)
    }

    /// The shard with this tree id — active or retired.
    pub fn shard_with_tree_id(&self, tree_id: u64) -> Option<(&str, u64)> {
        let want = tree_id.to_string();
        if self.tree_id == want {
            return Some((&self.signed_tree_head, self.tree_size));
        }
        self.inactive_shards
            .iter()
            .find(|s| s.tree_id == want)
            .map(|s| (s.signed_tree_head.as_str(), s.tree_size))
    }
}

pub fn fetch_log(t: &dyn Transport, rekor_url: &str) -> Result<LogState> {
    let url = format!("{}{LOG_PATH}", rekor_url.trim_end_matches('/'));
    let r = t.get(&url, "reading Rekor's tree state")?;
    if !r.is_success() {
        return Err(Error::RekorRejected {
            status: r.status,
            body: r.body,
        });
    }
    LogState::parse(&r.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::testing::{fixture, logged_pair, Exchange, Script};

    /// A real production `dsse` entry and the envelope it was logged for.
    const LOGGED: &str = "rekor-prod-dsse-entry-and-its-envelope.json";
    /// The same shape, from sigstore-conformance's own `dsse-mismatch-envelope`
    /// negative fixture: a real entry packaged with an envelope it does not
    /// describe.
    const SUBSTITUTED: &str = "rekor-prod-dsse-entry-and-a-substituted-envelope.json";
    const LOGGED_URL: &str = "/api/v1/log/entries/";

    fn staging() -> LogEntry {
        parse_entry_response(&fixture("rekor-staging-entry-hashedrekord.json")).unwrap()
    }

    fn prod_dsse() -> LogEntry {
        parse_entry_response(&fixture("rekor-prod-entry-dsse.json")).unwrap()
    }

    // --- reading real entries --------------------------------------------

    /// The `dsse` 0.0.1 entry this module writes, read back from production.
    #[test]
    fn a_real_production_dsse_entry_parses() {
        let e = prod_dsse();
        assert_eq!(e.kind_version().unwrap(), ("dsse".into(), "0.0.1".into()));
        assert_eq!(e.log_index, 2_594_242_700);
        assert_eq!(e.integrated_time, 1_787_713_433);
        assert_eq!(e.inclusion_proof().unwrap().tree_size, 2_594_251_845);
        assert_eq!(e.inclusion_proof().unwrap().hashes.len(), 30);
    }

    /// `hashedrekord` 0.0.1 is a large share of live traffic and must be
    /// *readable* even though this module never writes one.
    #[test]
    fn a_real_staging_hashedrekord_entry_parses() {
        let e = staging();
        assert_eq!(
            e.kind_version().unwrap(),
            ("hashedrekord".into(), "0.0.1".into())
        );
        assert_eq!(e.log_index, 24_201_800);
    }

    // --- the two indices --------------------------------------------------

    /// **The measurement this module is built around.** On a real staging
    /// entry, the global and shard-local indices produce proofs of the *same
    /// length*, so the shape check passes for both — and only one of the two
    /// roots is the log's.
    #[test]
    fn wrong_log_index_computes_a_different_root_and_the_shape_check_misses_it() {
        let e = staging();
        let proof = e.inclusion_proof().unwrap();
        let global = e.global_index().0;
        let shard = proof.shard_index().0;
        assert_ne!(global, shard, "the fixture must actually differ");

        // Both indices want a proof of exactly the length we have.
        let (i1, b1) = crate::merkle::proof_shape(shard, proof.tree_size);
        let (i2, b2) = crate::merkle::proof_shape(global, proof.tree_size);
        assert_eq!(i1 + b1, proof.hashes.len());
        assert_eq!(
            i2 + b2,
            proof.hashes.len(),
            "the shape check cannot tell these apart"
        );

        // The shard-local index is the one that reconstructs the log's root.
        let right = e.check_self_consistent().unwrap();
        assert_eq!(hex::encode(right), proof.root_hash);

        // The global index reconstructs a root that is not the log's, and
        // nothing about the computation looks different.
        let leaf = e.leaf_hash().unwrap();
        let hashes: Vec<[u8; 32]> = proof
            .hashes
            .iter()
            .map(|h| {
                let v = hex::decode(h).unwrap();
                let mut a = [0u8; 32];
                a.copy_from_slice(&v);
                a
            })
            .collect();
        let wrong =
            crate::merkle::root_from_inclusion_proof(global, proof.tree_size, &leaf, &hashes)
                .expect("the shape check passes for the wrong index too — that is the point");
        assert_ne!(wrong, right);
    }

    /// On another real entry the lengths happen to differ, so the shape check
    /// *does* catch it — which is precisely why the shape check is not the
    /// defence. The defence is that no caller gets to choose the index.
    #[test]
    fn on_some_entries_the_wrong_index_is_caught_by_length_and_on_others_it_is_not() {
        let e = prod_dsse();
        let proof = e.inclusion_proof().unwrap();
        let (i, b) = crate::merkle::proof_shape(e.global_index().0, proof.tree_size);
        assert_eq!(i + b, 21);
        assert_eq!(proof.hashes.len(), 30);
        // The right index still reconstructs the right root.
        assert_eq!(
            hex::encode(e.check_self_consistent().unwrap()),
            proof.root_hash
        );
    }

    /// The recomputed root is the root the checkpoint states, on both real
    /// entries. If those disagreed, the inclusion proof would be proving
    /// membership of a tree the log never signed.
    #[test]
    fn the_recomputed_root_is_the_root_the_checkpoint_states() {
        use base64::Engine as _;
        for e in [staging(), prod_dsse()] {
            let proof = e.inclusion_proof().unwrap();
            let root = e.check_self_consistent().unwrap();
            let line = proof.checkpoint.lines().nth(2).unwrap();
            let from_checkpoint = base64::engine::general_purpose::STANDARD
                .decode(line)
                .unwrap();
            assert_eq!(from_checkpoint, root.to_vec());
            // …and the checkpoint is for the shard the UUID names.
            let tree_id = e.shard_tree_id().unwrap().unwrap();
            assert!(
                proof
                    .checkpoint
                    .lines()
                    .next()
                    .unwrap()
                    .ends_with(&tree_id.to_string()),
                "checkpoint origin {:?} does not name tree {tree_id}",
                proof.checkpoint.lines().next().unwrap()
            );
        }
    }

    /// A tampered leaf must not reconstruct the log's root. Without this the
    /// proof is arithmetic with no security content.
    #[test]
    fn a_tampered_body_no_longer_reconstructs_the_root() {
        let mut e = staging();
        let mut raw = e.body_bytes().unwrap();
        raw[10] ^= 0xff;
        e.body = crate::bundle::b64_encode(&raw);
        let err = e.check_self_consistent().unwrap_err();
        assert!(matches!(err, Error::InclusionProofInvalid { .. }), "{err}");
    }

    /// …and every single sibling in the proof is load-bearing.
    #[test]
    fn corrupting_any_proof_hash_breaks_the_root() {
        let base = staging();
        let n = base.inclusion_proof().unwrap().hashes.len();
        for k in 0..n {
            let mut e = base.clone();
            let proof = e
                .verification
                .as_mut()
                .unwrap()
                .inclusion_proof
                .as_mut()
                .unwrap();
            let mut bytes = hex::decode(&proof.hashes[k]).unwrap();
            bytes[0] ^= 0xff;
            proof.hashes[k] = hex::encode(bytes);
            assert!(
                e.check_self_consistent().is_err(),
                "corrupting hash {k} still verified"
            );
        }
    }

    // --- the UUID ---------------------------------------------------------

    /// The tail of the UUID *is* the leaf hash — checked against two real
    /// entries from two different deployments.
    #[test]
    fn the_uuid_names_the_body_it_carries() {
        for e in [staging(), prod_dsse()] {
            assert_eq!(e.uuid.len(), 80);
            e.check_uuid_matches_body().unwrap();
            assert_eq!(&e.uuid[16..], hex::encode(e.leaf_hash().unwrap()));
        }
    }

    /// A server that returns a different entry from the one asked for is
    /// caught before any key material is consulted.
    #[test]
    fn an_entry_whose_uuid_does_not_name_its_body_is_refused() {
        let mut e = staging();
        let mut raw = e.body_bytes().unwrap();
        raw[0] ^= 0x01;
        e.body = crate::bundle::b64_encode(&raw);
        assert!(matches!(
            e.check_uuid_matches_body().unwrap_err(),
            Error::LogEntryMismatch
        ));
    }

    #[test]
    fn a_uuid_of_the_wrong_length_is_refused_rather_than_indexed_into() {
        let mut e = staging();
        e.uuid = "deadbeef".to_string();
        assert!(e.check_uuid_matches_body().is_err());
        // The shard id is a refusal, not `None`. `None` is what an *unsharded*
        // deployment returns, and an unreadable UUID is not that.
        assert!(e.shard_tree_id().is_err());
    }

    /// The tree id is the first 16 hex characters, and it matches the tree the
    /// checkpoint names — for both deployments.
    #[test]
    fn the_shard_tree_id_comes_from_the_uuid_prefix() {
        assert_eq!(
            staging().shard_tree_id().unwrap(),
            Some(8_050_909_264_565_447_525)
        );
        assert_eq!(
            prod_dsse().shard_tree_id().unwrap(),
            Some(1_193_050_959_916_656_506)
        );
    }

    // --- entries are not byte-stable --------------------------------------

    /// Two real fetches of one staging entry, minutes apart. The signature
    /// bytes moved; nothing that is signed did.
    #[test]
    fn two_fetches_of_one_entry_differ_in_signature_but_not_in_content() {
        let a = staging();
        let b = parse_entry_response(&fixture("rekor-staging-entry-hashedrekord-refetch.json"))
            .unwrap();

        assert_eq!(a.uuid, b.uuid);
        assert_eq!(a.body, b.body);
        assert_eq!(a.log_index, b.log_index);
        assert_eq!(a.integrated_time, b.integrated_time);

        let (pa, pb) = (a.inclusion_proof().unwrap(), b.inclusion_proof().unwrap());
        assert_eq!(pa.log_index, pb.log_index);
        assert_eq!(pa.tree_size, pb.tree_size);
        assert_eq!(pa.root_hash, pb.root_hash);
        assert_eq!(pa.hashes, pb.hashes);

        // The Signed Entry Timestamp is re-generated per response: ECDSA is
        // randomized, so identical content signs to different bytes.
        let (va, vb) = (
            a.verification.as_ref().unwrap(),
            b.verification.as_ref().unwrap(),
        );
        assert_ne!(
            va.signed_entry_timestamp, vb.signed_entry_timestamp,
            "if these ever match, this test's premise needs re-measuring, not deleting"
        );
        // Which is exactly why whole entries must never be compared.
        assert_ne!(
            fixture("rekor-staging-entry-hashedrekord.json"),
            fixture("rekor-staging-entry-hashedrekord-refetch.json")
        );
    }

    // --- the upload body --------------------------------------------------

    /// The envelope is a *stringified* object, per the v0.0.1 schema, and the
    /// verifier is base64 of PEM.
    #[test]
    fn the_proposed_entry_stringifies_the_envelope_and_base64s_the_certificate() {
        let env = Envelope {
            payload: crate::bundle::b64_encode(b"{}"),
            payload_type: crate::IN_TOTO_PAYLOAD_TYPE.to_string(),
            signatures: vec![crate::dsse::Signature {
                sig: crate::bundle::b64_encode(b"sig"),
                keyid: None,
            }],
        };
        let pem = "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n";
        let body = dsse_proposed_entry(&env, pem).unwrap();

        assert_eq!(body["kind"], "dsse");
        assert_eq!(body["apiVersion"], "0.0.1");
        let inner = &body["spec"]["proposedContent"]["envelope"];
        assert!(
            inner.is_string(),
            "the envelope must be a string, not an object"
        );
        let back: Envelope = serde_json::from_str(inner.as_str().unwrap()).unwrap();
        assert_eq!(back, env);

        let verifiers = body["spec"]["proposedContent"]["verifiers"]
            .as_array()
            .unwrap();
        assert_eq!(verifiers.len(), 1);
        assert_eq!(
            crate::bundle::b64_decode("verifiers", verifiers[0].as_str().unwrap()).unwrap(),
            pem.as_bytes()
        );
    }

    // --- the round trips --------------------------------------------------

    /// Upload then re-fetch: two round trips, because the 201's proof may not
    /// exist yet.
    #[test]
    fn an_upload_is_followed_by_a_fetch_for_the_inclusion_proof() {
        // A real **dsse** entry paired with the envelope it was really logged
        // for, because `upload_dsse` now insists the entry it bundles is the
        // entry for the envelope it posted.
        let (env, entry_json) = logged_pair(LOGGED);
        let script = Script::new(vec![
            Exchange::status("/api/v1/log/entries", 201, entry_json.clone()),
            Exchange::ok("/api/v1/log/entries/", entry_json),
        ]);
        let entry = upload_dsse(&script, "https://rekor.sigstage.dev", &env, "pem").unwrap();
        assert!(entry.inclusion_proof().is_some());
        let urls = script.urls();
        assert_eq!(urls[0], "https://rekor.sigstage.dev/api/v1/log/entries");
        assert!(urls[1].ends_with(&entry.uuid));
    }

    /// An upload of a `dsse` entry that comes back as some *other* kind is
    /// refused. The substituted entry here is a real, correctly signed,
    /// genuinely-in-the-log staging `hashedrekord` — so neither the UUID check
    /// nor the self-consistency check sees anything wrong with it. Only the
    /// kind ties the entry back to what was actually posted.
    #[test]
    fn an_upload_answered_with_an_entry_of_another_kind_is_refused() {
        let wrong = fixture("rekor-staging-entry-hashedrekord.json");
        // The substitute really is a good entry on its own terms.
        let substitute = parse_entry_response(&wrong).unwrap();
        substitute.check_uuid_matches_body().unwrap();
        substitute.check_self_consistent().unwrap();

        let script = Script::new(vec![
            Exchange::status("/api/v1/log/entries", 201, wrong.clone()),
            Exchange::ok("/api/v1/log/entries/", wrong),
        ]);
        let env = Envelope {
            payload: crate::bundle::b64_encode(b"{}"),
            payload_type: crate::IN_TOTO_PAYLOAD_TYPE.to_string(),
            signatures: vec![],
        };
        let e = upload_dsse(&script, "https://rekor.sigstage.dev", &env, "pem").unwrap_err();
        assert!(matches!(e, Error::LogEntryMismatch), "{e}");
    }

    /// A duplicate is followed through its `Location`, not reported as a
    /// failure. Signing the same payload twice is a normal thing to do.
    #[test]
    fn a_409_duplicate_is_followed_to_the_existing_entry() {
        let (env, entry_json) = logged_pair(LOGGED);
        let uuid = parse_entry_response(&entry_json).unwrap().uuid;
        let script = Script::new(vec![
            Exchange {
                expect_url_contains: "/api/v1/log/entries",
                status: 409,
                location: Some(format!("/api/v1/log/entries/{uuid}")),
                body: r#"{"code":409,"message":"An equivalent entry already exists"}"#.into(),
            },
            Exchange::ok("/api/v1/log/entries/", entry_json),
        ]);
        let entry = upload_dsse(&script, "https://rekor.sigstage.dev", &env, "pem").unwrap();
        assert_eq!(entry.uuid, uuid);
        assert!(script.is_drained());
    }

    /// A 409 with no Location is a failure and not a silent skip — there is
    /// nothing to fetch.
    #[test]
    fn a_409_without_a_location_is_refused() {
        let script = Script::new(vec![Exchange::status("/api/v1/log/entries", 409, "{}")]);
        let env = Envelope {
            payload: crate::bundle::b64_encode(b"{}"),
            payload_type: crate::IN_TOTO_PAYLOAD_TYPE.to_string(),
            signatures: vec![],
        };
        let e = upload_dsse(&script, "https://rekor.sigstage.dev", &env, "pem").unwrap_err();
        assert!(matches!(e, Error::RekorRejected { status: 409, .. }), "{e}");
    }

    /// Rekor's real rejection body survives into the error, so a caller sees
    /// what the log actually said.
    #[test]
    fn a_rejected_upload_carries_rekors_own_message() {
        let script = Script::new(vec![Exchange::status(
            "/api/v1/log/entries",
            400,
            fixture("rekor-staging-upload-400.json"),
        )]);
        let env = Envelope {
            payload: crate::bundle::b64_encode(b"{}"),
            payload_type: crate::IN_TOTO_PAYLOAD_TYPE.to_string(),
            signatures: vec![],
        };
        let e = upload_dsse(&script, "https://rekor.sigstage.dev", &env, "pem").unwrap_err();
        match e {
            Error::RekorRejected { status, ref body } => {
                assert_eq!(status, 400);
                assert!(body.contains("failure decoding PEM"), "{body}");
            }
            other => panic!("wrong variant: {other}"),
        }
        assert_eq!(e.axis(), crate::Axis::Bundle);
    }

    /// An entry that comes back without a proof stops the signing flow. A
    /// bundle with no inclusion proof is the artifact this crate exists to
    /// refuse; producing one and hoping a verifier notices is not an option.
    #[test]
    fn an_upload_that_yields_no_inclusion_proof_fails_rather_than_returning_a_proofless_entry() {
        let (env, entry_json) = logged_pair(LOGGED);
        let mut doc: serde_json::Value = serde_json::from_str(&entry_json).unwrap();
        let key = doc.as_object().unwrap().keys().next().unwrap().clone();
        doc[&key]["verification"]
            .as_object_mut()
            .unwrap()
            .remove("inclusionProof");
        let stripped = doc.to_string();
        let script = Script::new(vec![
            Exchange::status("/api/v1/log/entries", 201, stripped.clone()),
            Exchange::ok("/api/v1/log/entries/", stripped),
        ]);
        let e = upload_dsse(&script, "https://rekor.sigstage.dev", &env, "pem").unwrap_err();
        assert!(matches!(e, Error::InclusionProofMissing), "{e}");
    }

    /// A fetch that returns somebody else's entry is refused — even though
    /// that entry is internally consistent, correctly signed, and really is in
    /// the production log. A substituted entry is the failure a
    /// self-consistency check cannot see.
    #[test]
    fn a_fetch_that_returns_a_different_entry_is_refused() {
        let wrong = fixture("rekor-prod-entry-dsse.json");
        // Sanity: the substitute is a perfectly good entry on its own terms.
        parse_entry_response(&wrong)
            .unwrap()
            .check_uuid_matches_body()
            .unwrap();

        let script = Script::new(vec![Exchange::ok("/api/v1/log/entries/", wrong)]);
        let e = fetch_entry(
            &script,
            "https://rekor.sigstage.dev",
            "6fba935211532b65061bfbba69c7fd43fba818433432791d86e25313e9be63b97fa24e0ec976c485",
        )
        .unwrap_err();
        assert!(matches!(e, Error::LogEntryMismatch), "{e}");
    }

    /// The 64-character entry id is accepted as well as the 80-character
    /// sharded UUID, because Rekor accepts both and answers with the sharded
    /// one. A strict equality check would refuse a request Rekor served
    /// correctly.
    #[test]
    fn the_short_entry_id_is_accepted_as_well_as_the_sharded_uuid() {
        let json = fixture("rekor-staging-entry-hashedrekord.json");
        let full =
            "6fba935211532b65061bfbba69c7fd43fba818433432791d86e25313e9be63b97fa24e0ec976c485";
        for id in [full, &full[16..]] {
            let script = Script::new(vec![Exchange::ok("/api/v1/log/entries/", json.clone())]);
            let entry = fetch_entry(&script, "https://rekor.sigstage.dev", id).unwrap();
            assert_eq!(entry.uuid, full);
        }
    }

    /// A response with a body the UUID does not name is refused at the fetch.
    #[test]
    fn a_fetch_whose_body_contradicts_its_own_uuid_is_refused() {
        let mut doc: serde_json::Value =
            serde_json::from_str(&fixture("rekor-staging-entry-hashedrekord.json")).unwrap();
        let key = doc.as_object().unwrap().keys().next().unwrap().clone();
        doc[&key]["body"] = serde_json::Value::String(crate::bundle::b64_encode(b"tampered"));
        let script = Script::new(vec![Exchange::ok("/api/v1/log/entries/", doc.to_string())]);
        let e = fetch_entry(&script, "https://rekor.sigstage.dev", &key).unwrap_err();
        assert!(matches!(e, Error::LogEntryMismatch), "{e}");
    }

    // --- the log state ----------------------------------------------------

    /// Real staging `/api/v1/log`, with three retired shards. The shard that
    /// holds the fixture entry is one of the *inactive* ones, which is the
    /// whole reason the list matters.
    #[test]
    fn the_real_log_state_names_the_retired_shard_the_fixture_entry_lives_in() {
        let s = LogState::parse(&fixture("rekor-staging-log.json")).unwrap();
        assert_eq!(s.tree_id, "8202293616175992157");
        assert_eq!(s.inactive_shards.len(), 3);

        let tree_id = staging().shard_tree_id().unwrap().unwrap();
        assert_ne!(
            tree_id.to_string(),
            s.tree_id,
            "the entry is in a retired shard, not the active tree"
        );
        let (sth, size) = s.shard_with_tree_id(tree_id).expect("shard not listed");
        assert_eq!(size, 31_667_593);
        assert_eq!(
            size,
            staging().inclusion_proof().unwrap().tree_size,
            "the proof is against this shard's tree head"
        );
        assert!(sth.starts_with(&format!("rekor.sigstage.dev - {tree_id}\n")));
    }

    #[test]
    fn an_unknown_tree_id_is_none_rather_than_the_active_shard() {
        let s = LogState::parse(&fixture("rekor-staging-log.json")).unwrap();
        assert!(s.shard_with_tree_id(1).is_none());
    }

    // --- parse refusals ---------------------------------------------------

    #[test]
    fn an_empty_or_multi_entry_response_is_refused_rather_than_guessed_at() {
        assert!(parse_entry_response("{}").is_err());
        assert!(parse_entry_response("[]").is_err());
        let two = format!(
            r#"{{"a":{},"b":{}}}"#,
            serde_json::to_string(&serde_json::json!({
                "body":"e30=","integratedTime":1,"logID":"aa","logIndex":1
            }))
            .unwrap(),
            serde_json::to_string(&serde_json::json!({
                "body":"e30=","integratedTime":1,"logID":"aa","logIndex":2
            }))
            .unwrap()
        );
        assert!(parse_entry_response(&two).is_err());
    }

    // --- the UUID is a hex string, and only a hex string ------------------

    /// **A panic, reachable from a server's choice of JSON key.** A UUID of 80
    /// *bytes* that is not 80 *characters* used to be sliced at byte 16, which
    /// aborts the process instead of returning an error. The library is not
    /// allowed to panic; a hostile, proxied or merely buggy Rekor is not
    /// allowed to make it.
    #[test]
    fn a_uuid_that_is_eighty_bytes_but_not_eighty_characters_is_refused_not_sliced() {
        let mut e = staging();
        // 15 ASCII + 'é' (two bytes, straddling index 16) + 63 ASCII = 80 bytes.
        e.uuid = format!("{}é{}", "a".repeat(15), "b".repeat(63));
        assert_eq!(
            e.uuid.len(),
            80,
            "the byte length is what misled the old code"
        );
        assert_ne!(e.uuid.chars().count(), 80);
        assert!(matches!(
            e.check_uuid_matches_body().unwrap_err(),
            Error::CheckpointMalformed(_)
        ));
        assert!(e.shard_tree_id().is_err());
    }

    /// The same input, all the way through a fetch — because a refusal in a
    /// helper is worth nothing if the caller reaches it by a path that panics
    /// first.
    #[test]
    fn a_hostile_rekor_cannot_panic_the_fetch_with_its_choice_of_key() {
        let uuid = format!("{}é{}", "a".repeat(15), "b".repeat(63));
        let doc = serde_json::json!({ uuid.clone(): {
            "body": crate::bundle::b64_encode(br#"{"kind":"dsse","apiVersion":"0.0.1"}"#),
            "integratedTime": 1, "logID": "aa", "logIndex": 1
        }});
        let script = Script::new(vec![Exchange::ok("/api/v1/log/entries/", doc.to_string())]);
        let e = fetch_entry(&script, "https://rekor.sigstage.dev", &uuid).unwrap_err();
        assert!(matches!(e, Error::CheckpointMalformed(_)), "{e}");
    }

    /// An 80-character UUID whose shard prefix is not hex used to report
    /// `shard_tree_id() == None` — the value that means *"this deployment is
    /// unsharded"*. A caller resolving the entry's checkpoint would have gone
    /// looking for an unsharded log and seen nothing wrong.
    #[test]
    fn an_unreadable_shard_prefix_is_an_error_and_not_the_unsharded_answer() {
        let good = staging();
        assert!(good.shard_tree_id().unwrap().is_some());

        let mut e = good.clone();
        e.uuid = format!("zzzzzzzzzzzzzzzz{}", &good.uuid[16..]);
        assert!(
            e.shard_tree_id().is_err(),
            "an unreadable id is not an absent one"
        );
        // …and it is refused before it can be mistaken for a valid entry.
        assert!(e.check_uuid_matches_body().is_err());
    }

    // --- the fetch really checks it got what it asked for ------------------

    /// **A check that every answer satisfies.** The identity test used to be
    /// `entry.uuid.ends_with(requested)`, and every string ends with the empty
    /// one: an empty requested id accepted an unrelated production entry and
    /// returned it as the entry that was asked for. A one-character id did the
    /// same. Both are now refused by shape, before a URL is even built.
    #[test]
    fn an_empty_or_truncated_entry_id_no_longer_matches_every_entry() {
        for id in ["", "1", "9fac", "not-hex-at-all"] {
            let script = Script::new(vec![Exchange::ok(
                LOGGED_URL,
                fixture("rekor-staging-entry-hashedrekord.json"),
            )]);
            let e = fetch_entry(&script, "https://rekor.sigstage.dev", id).unwrap_err();
            assert!(
                matches!(e, Error::CheckpointMalformed(_)),
                "{id:?} produced {e}"
            );
            assert!(
                script.sent.borrow().is_empty(),
                "{id:?} reached the network before being checked"
            );
        }
    }

    /// The other half of the same comparison: the shard matches, the entry id
    /// does not. Without this, deleting the entry-id check broke no test —
    /// every fetch-substitution case here happened to differ in its shard
    /// prefix as well, so the shard check was doing all the work and the
    /// entry-id check was untested.
    #[test]
    fn a_different_entry_in_the_same_shard_is_refused() {
        let real = staging();
        let asked = format!("{}{}", &real.uuid[..16], "0".repeat(64));
        assert_ne!(asked, real.uuid);
        let script = Script::new(vec![Exchange::ok(
            LOGGED_URL,
            fixture("rekor-staging-entry-hashedrekord.json"),
        )]);
        let e = fetch_entry(&script, "https://rekor.sigstage.dev", &asked).unwrap_err();
        assert!(matches!(e, Error::LogEntryMismatch), "{e}");
    }

    /// The same entry id under a *different* tree is a different entry, and
    /// the shard halves are compared when both forms carry one.
    #[test]
    fn a_matching_entry_id_in_a_different_shard_is_refused() {
        let real = staging();
        let asked = format!("ffffffffffffffff{}", &real.uuid[16..]);
        let script = Script::new(vec![Exchange::ok(
            LOGGED_URL,
            fixture("rekor-staging-entry-hashedrekord.json"),
        )]);
        let e = fetch_entry(&script, "https://rekor.sigstage.dev", &asked).unwrap_err();
        assert!(matches!(e, Error::LogEntryMismatch), "{e}");
    }

    // --- the entry is for OUR envelope ------------------------------------

    /// The accepting half, on a real production entry paired with the real
    /// envelope it was logged for.
    #[test]
    fn a_real_entry_is_recognised_as_the_entry_for_its_own_envelope() {
        let (env, json) = logged_pair(LOGGED);
        let entry = parse_entry_response(&json).unwrap();
        assert_eq!(entry.kind_version().unwrap().0, KIND_DSSE);
        entry.check_uuid_matches_body().unwrap();
        check_entry_names_envelope(&entry, &env).unwrap();
    }

    /// The refusing half, on sigstore-conformance's own negative fixture: a
    /// real, correctly-signed, genuinely-logged `dsse` entry packaged with an
    /// envelope it does not describe. The entry passes the UUID check and is of
    /// the right kind, so this is the substitution neither of those can see.
    #[test]
    fn a_real_entry_for_somebody_elses_payload_is_refused() {
        let (substituted, json) = logged_pair(SUBSTITUTED);
        let entry = parse_entry_response(&json).unwrap();
        // The substitute really is a good entry on its own terms.
        entry.check_uuid_matches_body().unwrap();
        assert_eq!(entry.kind_version().unwrap().0, KIND_DSSE);
        assert!(matches!(
            check_entry_names_envelope(&entry, &substituted).unwrap_err(),
            Error::LogEntryMismatch
        ));
    }

    /// …and the whole upload refuses it, not merely the helper. Upstream's
    /// pairing exactly: the envelope that is posted and the entry that comes
    /// back are the two halves of `dsse-mismatch-envelope_fail`.
    #[test]
    fn an_upload_answered_with_a_dsse_entry_for_a_different_envelope_is_refused() {
        let (ours, their_entry) = logged_pair(SUBSTITUTED);
        let script = Script::new(vec![
            Exchange::status("/api/v1/log/entries", 201, their_entry.clone()),
            Exchange::ok("/api/v1/log/entries/", their_entry),
        ]);
        let e = upload_dsse(&script, "https://rekor.sigstage.dev", &ours, "pem").unwrap_err();
        assert!(matches!(e, Error::LogEntryMismatch), "{e}");
        // Both round trips happened: the refusal is *after* the write, which is
        // the only place this check can live. It turns a permanently
        // unverifiable bundle into a named error beside a log entry that
        // exists — which is the better of the two things still available once
        // the log has been written to.
        assert_eq!(script.urls().len(), 2);
    }

    /// A second direction on the same rule: our envelope, somebody else's real
    /// entry. This is the answer a hostile or confused Rekor gives that
    /// `check_uuid_matches_body` and `check_self_consistent` both accept.
    #[test]
    fn an_upload_answered_with_a_real_entry_for_another_payload_is_refused() {
        let (ours, _) = logged_pair(SUBSTITUTED);
        let (theirs, their_entry) = logged_pair(LOGGED);
        assert_ne!(
            ours.payload_bytes().unwrap(),
            theirs.payload_bytes().unwrap()
        );
        let substitute = parse_entry_response(&their_entry).unwrap();
        // The substitute is a good entry on its own terms, and really is in the
        // production log.
        substitute.check_uuid_matches_body().unwrap();
        substitute.check_self_consistent().unwrap();

        let script = Script::new(vec![
            Exchange::status("/api/v1/log/entries", 201, their_entry.clone()),
            Exchange::ok("/api/v1/log/entries/", their_entry),
        ]);
        let e = upload_dsse(&script, "https://rekor.sigstage.dev", &ours, "pem").unwrap_err();
        assert!(matches!(e, Error::LogEntryMismatch), "{e}");
    }

    /// A body with no `payloadHash` is a refusal, not a skip. A check that
    /// passes exactly when it has nothing to work with is how an unverified
    /// artifact comes to look verified.
    #[test]
    fn an_entry_with_no_payload_hash_is_refused_rather_than_skipped() {
        let (env, json) = logged_pair(LOGGED);
        let mut entry = parse_entry_response(&json).unwrap();
        for mutate in [
            |b: &mut serde_json::Value| {
                b["spec"].as_object_mut().unwrap().remove("payloadHash");
            },
            |b: &mut serde_json::Value| {
                b["spec"]["payloadHash"]["algorithm"] = serde_json::json!("sha512");
            },
            |b: &mut serde_json::Value| {
                b["spec"]["payloadHash"]
                    .as_object_mut()
                    .unwrap()
                    .remove("value");
            },
        ] {
            let mut body: serde_json::Value =
                serde_json::from_slice(&parse_entry_response(&json).unwrap().body_bytes().unwrap())
                    .unwrap();
            mutate(&mut body);
            entry.body = crate::bundle::b64_encode(body.to_string().as_bytes());
            let e = check_entry_names_envelope(&entry, &env).unwrap_err();
            assert!(matches!(e, Error::CheckpointMalformed(_)), "{e}");
        }
    }

    #[test]
    fn an_entry_with_no_proof_names_that_specific_failure() {
        let mut e = staging();
        e.verification.as_mut().unwrap().inclusion_proof = None;
        assert!(matches!(
            e.check_self_consistent().unwrap_err(),
            Error::InclusionProofMissing
        ));
        assert!(matches!(
            e.recompute_root().unwrap_err(),
            Error::InclusionProofMissing
        ));
    }
}

// --- tests that genuinely need the network ---------------------------------
//
// `#[ignore]` so `cargo test` stays hermetic. Run with
// `cargo test --features net,oidc -- --ignored`.
#[cfg(test)]
mod live {
    use super::*;
    use crate::net::Http;

    const STAGING: &str = "https://rekor.sigstage.dev";
    const STAGING_UUID: &str =
        "6fba935211532b65061bfbba69c7fd43fba818433432791d86e25313e9be63b97fa24e0ec976c485";

    /// The fixtures are a snapshot; this is the thing itself. The *signed*
    /// content of an entry is immutable, so a live fetch must agree with the
    /// recording on everything except the re-generated signatures.
    #[test]
    #[ignore = "requires the network"]
    fn the_live_staging_entry_still_matches_the_recorded_one_where_it_must() {
        let http = Http::new();
        let live = fetch_entry(&http, STAGING, STAGING_UUID).unwrap();
        let recorded = parse_entry_response(&crate::net::testing::fixture(
            "rekor-staging-entry-hashedrekord.json",
        ))
        .unwrap();
        assert_eq!(live.body, recorded.body);
        assert_eq!(live.log_index, recorded.log_index);
        assert_eq!(live.integrated_time, recorded.integrated_time);
        let (a, b) = (
            live.inclusion_proof().unwrap(),
            recorded.inclusion_proof().unwrap(),
        );
        assert_eq!(a.log_index, b.log_index);
        assert_eq!(a.tree_size, b.tree_size);
        assert_eq!(a.root_hash, b.root_hash);
        live.check_self_consistent().unwrap();
    }

    #[test]
    #[ignore = "requires the network"]
    fn the_live_staging_log_still_lists_the_shard_the_fixture_entry_lives_in() {
        let http = Http::new();
        let state = fetch_log(&http, STAGING).unwrap();
        assert!(state
            .shard_with_tree_id(8_050_909_264_565_447_525)
            .is_some());
    }
}
