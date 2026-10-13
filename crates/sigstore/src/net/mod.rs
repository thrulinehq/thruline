//! Everything that opens a socket, behind the `net` feature.
//!
//! Nothing in the default build reaches this module, and
//! `tests/no_network.rs` proves it from the dependency tree rather than from
//! this sentence.
//!
//! # There is one seam, and it exists so the flows can be tested
//!
//! Every request in [`oidc`], [`fulcio`] and [`rekor`] goes through
//! [`Transport`], a three-method trait whose production implementation is
//! [`Http`] (a `ureq` agent). The signing path is a sequence of round trips
//! whose *ordering* is load-bearing — the device code must reach a human
//! before the first poll, the Fulcio proof of possession must be over the
//! right string, the Rekor entry must be re-fetched before its inclusion proof
//! exists — and none of that is checkable if the only way to run it is to
//! actually sign something. The seam lets the whole sequence run against
//! recorded responses, offline, in `cargo test`.
//!
//! It is a seam for *tests*, not a plugin point: there is exactly one
//! production implementation and it is in this file.
//!
//! # Staging is the default and production is a thing you must type
//!
//! [`crate::sign::OidcProvider`] offers `staging_device_flow()` and no
//! production constructor. A Rekor entry is **permanent and public**: it
//! cannot be deleted, and the payload of a DSSE attestation is not encrypted.
//! A mistake against staging is a test; the same mistake against production is
//! a disclosure with no undo. When a caller does aim at production
//! deliberately, [`sign_keyless`] says so on stderr before it uploads —
//! loudly, once, rather than silently.

pub mod fulcio;
pub mod oidc;
pub mod rekor;

use crate::bundle::{
    b64_encode, Bundle, Certificate, Checkpoint, InclusionPromise, InclusionProof, KindVersion,
    LogId, TlogEntry, VerificationMaterial,
};
use crate::dsse::{pae, Envelope, Signature};
use crate::error::{Error, Result};
use crate::sign::OidcProvider;

/// The hosts that mean "this is the real, permanent, public log".
///
/// Matched as a suffix on the URL's host so `rekor.sigstore.dev` and
/// `log2025-alpha1.rekor.sigstore.dev` both count. Deliberately **not** used to
/// block anything — a caller who types a production URL means it — only to
/// make sure the fact is stated out loud before an irreversible write.
pub const PRODUCTION_HOST_SUFFIXES: &[&str] = &[
    "fulcio.sigstore.dev",
    "rekor.sigstore.dev",
    "oauth2.sigstore.dev",
];

/// Does this URL point at the production Sigstore instance?
///
/// # Three ways a real production URL used to slip past this
///
/// The host is normalised the way a resolver normalises it, because anything
/// less is a warning that does not fire on a URL that reaches the real log.
/// Measured, all three silently returning `false` before this was fixed:
///
/// - **`https://Rekor.SigStore.dev`** — DNS is case-insensitive and this
///   comparison was not. A config file with a capital letter in it got a
///   production signer and no warning at all.
/// - **`https://rekor.sigstore.dev./api`** — the DNS root label. Same host,
///   same log.
/// - **`https://rekor.sigstore.dev?x=1`** — the authority ends at the first
///   `/`, `?` **or** `#`, and only `/` was being cut.
///
/// It is still a host-suffix list and still only a warning: a production
/// deployment reachable under some other name gets none. That is stated in the
/// module docs and is not what these three were.
pub fn is_production(url: &str) -> bool {
    let after_scheme = match url.split_once("://") {
        Some((_, rest)) => rest,
        None => url,
    };
    // The authority ends at the first `/`, `?` or `#`.
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    // Strip userinfo, then the port: `https://evil@rekor.sigstore.dev` and
    // `https://rekor.sigstore.dev:443` are both the same host.
    let host_port = match authority.rsplit_once('@') {
        Some((_, h)) => h,
        None => authority,
    };
    // Case-fold and drop the DNS root label, both of which a resolver ignores.
    let host = host_port
        .split(':')
        .next()
        .unwrap_or("")
        .trim_end_matches('.')
        .to_ascii_lowercase();
    PRODUCTION_HOST_SUFFIXES
        .iter()
        .any(|s| host == *s || host.ends_with(&format!(".{s}")))
}

/// One HTTP response, reduced to the three things the flows here read.
///
/// `status` is kept as a number rather than turned into `Result` at the
/// transport, because **every** one of these flows has a non-2xx response that
/// is not an error: the OAuth token endpoint answers `authorization_pending`
/// with a 400, and Rekor answers a duplicate upload with a 409 plus the
/// existing entry's `Location`. A transport that raised on status would make
/// both of those unreachable, so `ureq`'s `http_status_as_error` is turned off
/// in [`Http::new`] and the decision is made by the caller who knows what a
/// given status means.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    /// The `Location` header, lowercased-name lookup. Rekor uses it to point at
    /// an already-existing entry on 409.
    pub location: Option<String>,
    pub body: String,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// The single round trip every flow in this module is built from.
///
/// `context` is not decoration: it becomes the first half of
/// [`Error::Transport`], so a connection failure says *which* step of signing
/// could not reach the network rather than only naming a host.
pub trait Transport {
    fn get(&self, url: &str, context: &str) -> Result<HttpResponse>;
    fn post_form(&self, url: &str, fields: &[(&str, &str)], context: &str) -> Result<HttpResponse>;
    fn post_json(&self, url: &str, body: &serde_json::Value, context: &str)
        -> Result<HttpResponse>;
}

/// The production transport: a blocking `ureq` agent.
///
/// Blocking and not `tokio`, matching the project's own rule (*"HTTP: `ureq`
/// (blocking, simple) … No tokio until the watcher daemon milestone"*). The
/// device flow spends most of its life asleep between polls, which is the one
/// workload where async buys nothing.
pub struct Http {
    agent: ureq::Agent,
}

/// How long any single request may take.
///
/// The device flow's *whole* budget is 300 seconds (`expires_in` from real
/// staging, recorded in `fixtures/net/oidc-staging-device-code.json`), and it
/// is spent on a human reading a URL, not on a socket. A per-request ceiling
/// well under that keeps one wedged connection from eating the window a person
/// is standing in.
pub const REQUEST_TIMEOUT_SECS: u64 = 30;

impl Default for Http {
    fn default() -> Self {
        Http::new()
    }
}

impl Http {
    pub fn new() -> Http {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS)))
            // See `HttpResponse`: a 400 is data here, not an error.
            .http_status_as_error(false)
            .user_agent(concat!("thruline-sigstore/", env!("CARGO_PKG_VERSION")))
            .build();
        Http {
            agent: config.into(),
        }
    }
}

fn finish(
    r: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    context: &str,
) -> Result<HttpResponse> {
    let mut resp = r.map_err(|e| Error::Transport {
        context: context.to_string(),
        reason: e.to_string(),
    })?;
    let status = resp.status().as_u16();
    let location = resp
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Transport {
            context: context.to_string(),
            reason: format!("reading the response body: {e}"),
        })?;
    Ok(HttpResponse {
        status,
        location,
        body,
    })
}

impl Transport for Http {
    fn get(&self, url: &str, context: &str) -> Result<HttpResponse> {
        finish(self.agent.get(url).call(), context)
    }

    fn post_form(&self, url: &str, fields: &[(&str, &str)], context: &str) -> Result<HttpResponse> {
        finish(
            self.agent.post(url).send_form(fields.iter().copied()),
            context,
        )
    }

    fn post_json(
        &self,
        url: &str,
        body: &serde_json::Value,
        context: &str,
    ) -> Result<HttpResponse> {
        finish(self.agent.post(url).send_json(body), context)
    }
}

/// Where a keyless signature is issued and logged.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub fulcio_url: String,
    pub rekor_url: String,
}

impl Endpoints {
    /// Sigstore **staging**. See the module docs for why this is the default.
    pub fn staging() -> Endpoints {
        Endpoints {
            fulcio_url: "https://fulcio.sigstage.dev".to_string(),
            rekor_url: "https://rekor.sigstage.dev".to_string(),
        }
    }

    fn warn_if_production(&self) {
        warn_if_production(&self.fulcio_url);
        warn_if_production(&self.rekor_url);
    }
}

/// Say out loud, once, that this URL is the real thing.
///
/// A free function rather than a method on [`Endpoints`] because the OIDC
/// **issuer** is not one of the endpoints and still needs saying: signing
/// against production Dex issues a production identity, and
/// `oauth2.sigstore.dev` is in [`PRODUCTION_HOST_SUFFIXES`] for that reason. A
/// constant listed and never consulted is a check that looks like it runs.
pub fn warn_if_production(url: &str) {
    if is_production(url) {
        eprintln!(
            "thruline-sigstore: targeting PRODUCTION Sigstore at {url}. A Rekor entry is \
             permanent and public — it cannot be deleted, and the DSSE payload is not encrypted."
        );
    }
}

/// Sign `payload` and return a v0.3 bundle. Called from [`crate::sign::sign`].
///
/// The two halves are separate on purpose: acquiring an identity token is the
/// half that needs a human and a browser, and [`sign_with_token`] is everything
/// after it. Tests exercise the second half against recorded responses; only
/// the first half is unreachable without someone to approve the device code.
pub fn sign_keyless(payload: &[u8], identity: OidcProvider) -> Result<Bundle> {
    let http = Http::new();
    match identity {
        OidcProvider::DeviceFlow {
            issuer,
            client_id,
            fulcio_url,
            rekor_url,
        } => {
            let endpoints = Endpoints {
                fulcio_url,
                rekor_url,
            };
            endpoints.warn_if_production();
            // The issuer is not an endpoint, and is checked separately: a
            // production identity is what makes the certificate a production
            // certificate.
            warn_if_production(&issuer);
            let token = oidc::device_flow(&http, &issuer, &client_id, &mut oidc::print_prompt)?;
            sign_with_token(&http, &endpoints, &token, payload)
        }
        OidcProvider::Token {
            token,
            fulcio_url,
            rekor_url,
        } => {
            let endpoints = Endpoints {
                fulcio_url,
                rekor_url,
            };
            endpoints.warn_if_production();
            let token = oidc::IdToken::parse(&token)?;
            sign_with_token(&http, &endpoints, &token, payload)
        }
        // Unreachable through `crate::sign::sign`, which handles this arm
        // first. Named rather than `unreachable!()` because this function is
        // public and a caller can hand it anything.
        OidcProvider::LocalKey { .. } => Err(Error::FeatureDisabled {
            operation: "keyless signing with a local key",
            feature: "oidc",
        }),
    }
}

/// Everything after the identity token: ephemeral key → Fulcio → DSSE → Rekor.
///
/// # The ephemeral key never leaves this function
///
/// It is created here, used twice — once for Fulcio's proof of possession,
/// once for the DSSE signature — and dropped here. `ecdsa::SigningKey`
/// implements `ZeroizeOnDrop` (asserted in this module's tests, so the
/// property survives a dependency bump), so the scalar is wiped rather than
/// left on the stack. Nothing writes it to disk and nothing returns it: after
/// this call the only thing that can produce a signature under that
/// certificate is gone. That is the point of keyless signing — the certificate
/// outliving the key by ten minutes is what makes the Rekor timestamp
/// load-bearing rather than a nicety.
pub fn sign_with_token(
    t: &dyn Transport,
    endpoints: &Endpoints,
    token: &oidc::IdToken,
    payload: &[u8],
) -> Result<Bundle> {
    let key = fulcio::ephemeral_key()?;
    // `request_certificate` refuses a certificate that does not carry `key`,
    // so by the time this returns the binding is established — and it is
    // established *before* the Rekor write below, which cannot be undone. See
    // `fulcio::check_certifies`.
    let chain = fulcio::request_certificate(t, &endpoints.fulcio_url, token, &key)?;
    let leaf = chain
        .first()
        .ok_or_else(|| Error::CertificateParse("Fulcio returned an empty chain".into()))?;
    log_and_assemble(t, &endpoints.rekor_url, &key, leaf, payload)
}

/// Everything after issuance: DSSE-sign, upload, re-fetch, assemble.
///
/// Separated from [`sign_with_token`] for one reason: no real Fulcio
/// certificate in this repository has a private key behind it, because
/// issuance needs a human-approved identity token. Driving this half directly
/// is how the Rekor upload body, the hex→base64 crossing and the bundle
/// assembly get tested against **recorded staging responses** with a real
/// certificate — a pairing `sign_with_token` itself would (correctly) refuse.
fn log_and_assemble(
    t: &dyn Transport,
    rekor_url: &str,
    key: &p256::ecdsa::SigningKey,
    leaf: &x509_cert::Certificate,
    payload: &[u8],
) -> Result<Bundle> {
    let envelope = dsse_sign(key, payload)?;
    let leaf_pem = fulcio::certificate_to_pem(leaf)?;
    let entry = rekor::upload_dsse(t, rekor_url, &envelope, &leaf_pem)?;
    let leaf_der = fulcio::certificate_to_der(leaf)?;
    assemble_bundle(&leaf_der, envelope, &entry)
}

/// Wrap `payload` in a DSSE envelope signed by `key`.
///
/// The payload type is [`crate::IN_TOTO_PAYLOAD_TYPE`] and the signed bytes are
/// [`crate::dsse::pae`] — the *same* function the verifier uses, not a second
/// implementation of the same encoding. A signer and a verifier that build PAE
/// separately do not fail loudly when they diverge; they fail as "signature
/// invalid", which reads as tampering.
pub fn dsse_sign(key: &p256::ecdsa::SigningKey, payload: &[u8]) -> Result<Envelope> {
    use p256::ecdsa::{signature::Signer, DerSignature};
    let signed = pae(crate::IN_TOTO_PAYLOAD_TYPE, payload);
    let sig: DerSignature = key.sign(&signed);
    Ok(Envelope {
        payload: b64_encode(payload),
        payload_type: crate::IN_TOTO_PAYLOAD_TYPE.to_string(),
        signatures: vec![Signature {
            sig: b64_encode(sig.as_bytes()),
            keyid: None,
        }],
    })
}

/// Build the v0.3 bundle from a leaf certificate, a signed envelope and a log
/// entry.
///
/// # Two encodings meet here, and getting it wrong is silent
///
/// Rekor's REST API returns `rootHash`, `hashes[]` and `logID` as **lowercase
/// hex**. The bundle's ProtoJSON encodes every `bytes` field as **standard
/// base64**. A bundle assembled by copying Rekor's strings across is
/// well-formed JSON that no verifier can check: `b64_decode` on a hex string
/// either fails or — for hex that happens to be valid base64 — yields 24 bytes
/// where 32 were meant. Every field crossing that boundary is converted here,
/// and `the_rekor_hex_becomes_bundle_base64` proves the round trip against a
/// real entry.
pub fn assemble_bundle(
    leaf_der: &[u8],
    envelope: Envelope,
    entry: &rekor::LogEntry,
) -> Result<Bundle> {
    let tlog = tlog_entry_from_rekor(entry)?;
    Ok(Bundle {
        media_type: crate::BUNDLE_V03.to_string(),
        verification_material: VerificationMaterial {
            certificate: Some(Certificate {
                raw_bytes: b64_encode(leaf_der),
            }),
            x509_certificate_chain: None,
            public_key: None,
            tlog_entries: vec![tlog],
        },
        message_signature: None,
        dsse_envelope: Some(envelope),
    })
}

fn hex_to_b64(field: &'static str, s: &str) -> Result<String> {
    let raw = hex::decode(s).map_err(|e| Error::Base64 {
        field,
        reason: format!("Rekor returns this field as hex, and this is not hex: {e}"),
    })?;
    Ok(b64_encode(&raw))
}

fn tlog_entry_from_rekor(entry: &rekor::LogEntry) -> Result<TlogEntry> {
    let kind = entry.kind_version()?;
    let proof = entry
        .inclusion_proof()
        .ok_or(Error::InclusionProofMissing)?;
    Ok(TlogEntry {
        // The GLOBAL index. The bundle's top-level `logIndex` is the one the
        // Signed Entry Timestamp covers; the shard-local one belongs inside
        // `inclusionProof` and nowhere else. See `rekor`'s module docs.
        log_index: entry.log_index,
        log_id: LogId {
            key_id: hex_to_b64("tlogEntries[].logId.keyId", &entry.log_id)?,
        },
        kind_version: KindVersion {
            kind: kind.0,
            version: kind.1,
        },
        integrated_time: entry.integrated_time,
        inclusion_promise: entry.verification.as_ref().map(|v| InclusionPromise {
            signed_entry_timestamp: v.signed_entry_timestamp.clone(),
        }),
        inclusion_proof: Some(InclusionProof {
            // The SHARD-LOCAL index.
            log_index: proof.log_index,
            root_hash: hex_to_b64("inclusionProof.rootHash", &proof.root_hash)?,
            tree_size: proof.tree_size,
            hashes: proof
                .hashes
                .iter()
                .map(|h| hex_to_b64("inclusionProof.hashes[]", h))
                .collect::<Result<Vec<_>>>()?,
            checkpoint: Checkpoint {
                envelope: proof.checkpoint.clone(),
            },
        }),
        canonicalized_body: entry.body.clone(),
    })
}

#[cfg(test)]
pub(crate) mod testing {
    //! A [`Transport`] that answers from a script instead of a socket.
    //!
    //! Every response it returns is a file under `fixtures/net/`, and
    //! `fixtures/net/PROVENANCE.md` records where each one came from and
    //! whether it was recorded or constructed. A test that "passes" against an
    //! invented response has proved nothing about the protocol.

    use std::cell::RefCell;

    use super::{HttpResponse, Result, Transport};

    /// One scripted exchange: the request we expect, and the answer to give.
    pub struct Exchange {
        pub expect_url_contains: &'static str,
        pub status: u16,
        pub location: Option<String>,
        pub body: String,
    }

    impl Exchange {
        pub fn ok(url: &'static str, body: impl Into<String>) -> Exchange {
            Exchange {
                expect_url_contains: url,
                status: 200,
                location: None,
                body: body.into(),
            }
        }

        pub fn status(url: &'static str, status: u16, body: impl Into<String>) -> Exchange {
            Exchange {
                expect_url_contains: url,
                status,
                location: None,
                body: body.into(),
            }
        }
    }

    /// What the code under test actually sent, so a test can assert on the
    /// request and not only on what it did with the reply.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Sent {
        pub url: String,
        pub form: Vec<(String, String)>,
        pub json: Option<serde_json::Value>,
    }

    pub struct Script {
        remaining: RefCell<std::collections::VecDeque<Exchange>>,
        pub sent: RefCell<Vec<Sent>>,
    }

    impl Script {
        pub fn new(exchanges: Vec<Exchange>) -> Script {
            Script {
                remaining: RefCell::new(exchanges.into()),
                sent: RefCell::new(Vec::new()),
            }
        }

        pub fn urls(&self) -> Vec<String> {
            self.sent.borrow().iter().map(|s| s.url.clone()).collect()
        }

        pub fn is_drained(&self) -> bool {
            self.remaining.borrow().is_empty()
        }

        fn next(&self, url: &str) -> HttpResponse {
            let e = self
                .remaining
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| panic!("no scripted response left for {url}"));
            assert!(
                url.contains(e.expect_url_contains),
                "request went to {url}, script expected something containing {:?}",
                e.expect_url_contains
            );
            HttpResponse {
                status: e.status,
                location: e.location,
                body: e.body,
            }
        }
    }

    impl Transport for Script {
        fn get(&self, url: &str, _context: &str) -> Result<HttpResponse> {
            self.sent.borrow_mut().push(Sent {
                url: url.to_string(),
                form: Vec::new(),
                json: None,
            });
            Ok(self.next(url))
        }

        fn post_form(
            &self,
            url: &str,
            fields: &[(&str, &str)],
            _context: &str,
        ) -> Result<HttpResponse> {
            self.sent.borrow_mut().push(Sent {
                url: url.to_string(),
                form: fields
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                json: None,
            });
            Ok(self.next(url))
        }

        fn post_json(
            &self,
            url: &str,
            body: &serde_json::Value,
            _context: &str,
        ) -> Result<HttpResponse> {
            self.sent.borrow_mut().push(Sent {
                url: url.to_string(),
                form: Vec::new(),
                json: Some(body.clone()),
            });
            Ok(self.next(url))
        }
    }

    /// Read a fixture by name from `fixtures/net/`.
    pub fn fixture(name: &str) -> String {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/net/").to_string() + name;
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
    }

    /// A real production `dsse` log entry together with the envelope it was
    /// logged for, as one pair: `(envelope, the Rekor REST response body)`.
    ///
    /// # Why one file and not two
    ///
    /// The *pairing* is the fact under test.
    /// [`super::rekor::check_entry_names_envelope`] compares an entry against
    /// an envelope, so an entry and an envelope kept in separate files could
    /// drift apart and the test would go on passing while checking nothing.
    ///
    /// # Re-shaped, not invented
    ///
    /// Every field is the corresponding field of a real sigstore-conformance
    /// bundle, converted from the bundle's ProtoJSON encoding (base64 bytes,
    /// quoted int64) into Rekor's REST encoding (lowercase hex, bare numbers) —
    /// the inverse of [`super::tlog_entry_from_rekor`]. The UUID is recomputed
    /// as `SHA256(0x00 ‖ canonicalizedBody)`: the 64-character unsharded form,
    /// which is what an unsharded Rekor deployment returns and which no other
    /// test here drives through a full upload.
    pub fn logged_pair(name: &str) -> (crate::dsse::Envelope, String) {
        let doc: serde_json::Value = serde_json::from_str(&fixture(name)).unwrap();
        let envelope: crate::dsse::Envelope =
            serde_json::from_value(doc["envelope"].clone()).unwrap();
        (envelope, doc["entry"].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{fixture, logged_pair, Exchange, Script};
    use super::*;

    /// A real production `dsse` entry and the envelope it was logged for. See
    /// [`testing::logged_pair`].
    const LOGGED: &str = "rekor-prod-dsse-entry-and-its-envelope.json";

    /// The ephemeral key's guarantee is a trait bound, so a dependency bump
    /// that dropped it would be a compile error here rather than a silent loss.
    #[test]
    fn the_ephemeral_signing_key_zeroizes_on_drop() {
        fn assert_zeroizes<T: p256::elliptic_curve::zeroize::ZeroizeOnDrop>() {}
        assert_zeroizes::<p256::ecdsa::SigningKey>();
    }

    #[test]
    fn production_hosts_are_recognised_and_staging_is_not() {
        assert!(is_production("https://rekor.sigstore.dev"));
        assert!(is_production(
            "https://fulcio.sigstore.dev/api/v2/signingCert"
        ));
        assert!(is_production("https://log2025-alpha1.rekor.sigstore.dev"));
        assert!(!is_production("https://rekor.sigstage.dev"));
        assert!(!is_production("https://fulcio.sigstage.dev"));
        // The check must not be fooled by a lookalike host that merely ends in
        // the same characters without a dot boundary.
        assert!(!is_production("https://evilrekor.sigstore.dev.example.com"));
        assert!(!is_production("https://notrekor.sigstore.dev"));
        // The OIDC issuer is on the list too, and is consulted: a production
        // identity is what makes the certificate a production certificate.
        assert!(is_production("https://oauth2.sigstore.dev/auth"));
        assert!(!is_production("https://oauth2.sigstage.dev/auth"));
        // A userinfo prefix and a port must not hide the host.
        assert!(is_production("https://evil@rekor.sigstore.dev:443/x"));
    }

    /// **The three real production URLs this check used to miss.** Each one
    /// reaches the real, permanent, public log, and each one produced no
    /// warning at all before the host was normalised.
    #[test]
    fn a_production_host_is_recognised_however_a_resolver_would_see_it() {
        for u in [
            // DNS is case-insensitive. A config file with a capital letter in
            // it is the whole exploit.
            "https://Rekor.SigStore.dev",
            "https://REKOR.SIGSTORE.DEV/api/v1/log/entries",
            "https://Fulcio.Sigstore.dev/api/v2/signingCert",
            "https://OAuth2.Sigstore.dev/auth",
            // The DNS root label: same host, same log.
            "https://rekor.sigstore.dev./api/v1/log",
            "https://Log2025-Alpha1.Rekor.SigStore.dev.",
            // The authority ends at `?` and `#` too, not only `/`.
            "https://rekor.sigstore.dev?trace=1",
            "https://rekor.sigstore.dev#x",
            "https://evil@Rekor.SigStore.dev.:443/x?y",
        ] {
            assert!(is_production(u), "missed {u}");
        }
        // …and normalisation must not start matching things that are not
        // production. A trailing dot does not turn a lookalike into the log.
        for u in [
            "https://Rekor.SigStage.dev",
            "https://notrekor.sigstore.dev.",
            "https://EvilRekor.SigStore.dev.example.com",
        ] {
            assert!(!is_production(u), "wrongly flagged {u}");
        }
    }

    /// Every host in the list is actually recognised. A suffix that is listed
    /// and never matched is a warning that would never fire.
    #[test]
    fn every_production_suffix_in_the_list_is_recognised() {
        for s in PRODUCTION_HOST_SUFFIXES {
            assert!(is_production(&format!("https://{s}")), "{s}");
            assert!(is_production(&format!("https://sub.{s}/path")), "{s}");
        }
    }

    #[test]
    fn the_envelope_this_module_signs_verifies_with_the_certified_key() {
        use spki::EncodePublicKey;
        let key = fulcio::ephemeral_key().unwrap();
        let env = dsse_sign(&key, b"{\"_type\":\"https://in-toto.io/Statement/v1\"}").unwrap();
        let spki = key.verifying_key().to_public_key_der().unwrap();
        assert_eq!(env.verify_with_spki(spki.as_bytes()).unwrap(), 0);
        assert_eq!(env.payload_type, crate::IN_TOTO_PAYLOAD_TYPE);
    }

    /// The negative half: a different key must not verify, or the signature is
    /// decoration.
    #[test]
    fn a_dsse_envelope_does_not_verify_under_a_different_key() {
        use spki::EncodePublicKey;
        let a = fulcio::ephemeral_key().unwrap();
        let b = fulcio::ephemeral_key().unwrap();
        let env = dsse_sign(&a, b"payload").unwrap();
        let spki = b.verifying_key().to_public_key_der().unwrap();
        assert!(matches!(
            env.verify_with_spki(spki.as_bytes()).unwrap_err(),
            Error::SignatureInvalid
        ));
    }

    /// Rekor speaks hex, the bundle speaks base64, and the conversion is the
    /// only thing standing between the two. Checked against a real staging
    /// entry rather than a hand-written pair.
    #[test]
    fn the_rekor_hex_becomes_bundle_base64() {
        let entry =
            rekor::parse_entry_response(&fixture("rekor-staging-entry-hashedrekord.json")).unwrap();
        let key = fulcio::ephemeral_key().unwrap();
        let env = dsse_sign(&key, b"payload").unwrap();
        let bundle = assemble_bundle(&[0x30, 0x00], env, &entry).unwrap();

        let te = &bundle.verification_material.tlog_entries[0];
        // 32 raw bytes as base64 is 44 characters; as hex it is 64.
        assert_eq!(te.log_id.key_id.len(), 44);
        assert_eq!(
            crate::bundle::b64_decode("logId", &te.log_id.key_id).unwrap(),
            hex::decode(&entry.log_id).unwrap()
        );
        let ip = te.inclusion_proof.as_ref().unwrap();
        assert_eq!(
            crate::bundle::b64_decode("rootHash", &ip.root_hash).unwrap(),
            hex::decode(&entry.inclusion_proof().unwrap().root_hash).unwrap()
        );
        for (b64, hexed) in ip
            .hashes
            .iter()
            .zip(&entry.inclusion_proof().unwrap().hashes)
        {
            assert_eq!(
                crate::bundle::b64_decode("hashes", b64).unwrap(),
                hex::decode(hexed).unwrap()
            );
        }
    }

    /// The two indices land in the two places the spec puts them, and they are
    /// genuinely different numbers in this fixture — so a test that mixed them
    /// up would fail rather than pass by coincidence.
    #[test]
    fn the_global_index_goes_outside_and_the_shard_local_one_inside() {
        let entry =
            rekor::parse_entry_response(&fixture("rekor-staging-entry-hashedrekord.json")).unwrap();
        let key = fulcio::ephemeral_key().unwrap();
        let env = dsse_sign(&key, b"payload").unwrap();
        let bundle = assemble_bundle(&[0x30, 0x00], env, &entry).unwrap();
        let te = &bundle.verification_material.tlog_entries[0];

        assert_eq!(
            te.log_index, 24_201_800,
            "top level must be the global index"
        );
        assert_eq!(
            te.inclusion_proof.as_ref().unwrap().log_index,
            24_186_981,
            "the proof must carry the shard-local index"
        );
        assert_ne!(te.log_index, te.inclusion_proof.as_ref().unwrap().log_index);
    }

    /// An entry with no inclusion proof cannot become a bundle. A tlog entry
    /// without one is a claim about a log, not an entry in it — and the whole
    /// reason this crate exists is that `sigstore` 0.14.0 leaves that check
    /// as a TODO.
    #[test]
    fn a_log_entry_with_no_inclusion_proof_refuses_to_become_a_bundle() {
        let mut entry =
            rekor::parse_entry_response(&fixture("rekor-staging-entry-hashedrekord.json")).unwrap();
        if let Some(v) = entry.verification.as_mut() {
            v.inclusion_proof = None;
        }
        let key = fulcio::ephemeral_key().unwrap();
        let env = dsse_sign(&key, b"payload").unwrap();
        let e = assemble_bundle(&[0x30, 0x00], env, &entry).unwrap_err();
        assert!(matches!(e, Error::InclusionProofMissing), "{e}");
    }

    /// The assembled bundle survives its own shape validation and round-trips
    /// through the parser the verifier will use.
    #[test]
    fn the_assembled_bundle_is_a_v03_bundle_the_parser_accepts() {
        let entry = rekor::parse_entry_response(&fixture("rekor-prod-entry-dsse.json")).unwrap();
        let key = fulcio::ephemeral_key().unwrap();
        let env = dsse_sign(&key, b"{}").unwrap();
        let bundle = assemble_bundle(&[0x30, 0x00], env, &entry).unwrap();
        let json = bundle.to_json().unwrap();
        let back = Bundle::from_json(&json).unwrap();
        assert_eq!(back.media_type, crate::BUNDLE_V03);
        assert!(back.is_v03());
        assert!(back.dsse_envelope.is_some());
        assert!(back.verification_material.certificate.is_some());
        assert!(back.verification_material.x509_certificate_chain.is_none());
        assert_eq!(back.verification_material.tlog_entries.len(), 1);
        assert_eq!(
            back.verification_material.tlog_entries[0].kind_version.kind,
            "dsse"
        );
    }

    /// The real staging leaf, out of the recorded `signingCert` response.
    ///
    /// Its private half does not exist here and cannot: Fulcio issued it to
    /// somebody else on 2026-09-04, and getting one of our own needs a human
    /// to approve a device code in a browser. Everything below is therefore
    /// driven through [`log_and_assemble`] rather than [`sign_with_token`],
    /// which refuses exactly this pairing — see the test that proves it does.
    fn recorded_staging_leaf() -> x509_cert::Certificate {
        fulcio::parse_signing_cert_response(&fixture(
            "fulcio-staging-signingcert-embedded-sct.json",
        ))
        .unwrap()
        .chain
        .remove(0)
    }

    /// The whole post-certificate sequence, driven from recorded staging
    /// responses: Rekor accepts, Rekor is re-read for the proof, and a bundle
    /// comes out. This is the part of `sign_keyless` that a human's browser is
    /// not needed for, and it is the part with all the encodings in it.
    #[test]
    fn signing_with_a_recorded_certificate_produces_a_bundle_from_recorded_responses() {
        // A real **dsse** entry, and the payload it was really logged for:
        // `upload_dsse` insists the entry it goes on to bundle is the entry for
        // the envelope that was posted, not merely one of the right kind.
        let (logged, entry_json) = logged_pair(LOGGED);
        let payload = logged.payload_bytes().unwrap();
        let script = Script::new(vec![
            Exchange::status("/api/v1/log/entries", 201, entry_json.clone()),
            Exchange::ok("/api/v1/log/entries/", entry_json),
        ]);
        let key = fulcio::ephemeral_key().unwrap();
        let bundle = log_and_assemble(
            &script,
            "https://rekor.sigstage.dev",
            &key,
            &recorded_staging_leaf(),
            &payload,
        )
        .unwrap();

        assert!(script.is_drained(), "not every scripted response was used");
        let urls = script.urls();
        assert!(urls[0].starts_with("https://rekor.sigstage.dev/api/v1/log/entries"));
        assert_eq!(bundle.media_type, crate::BUNDLE_V03);

        // The leaf in the bundle is the leaf Fulcio returned, not the chain.
        let der = bundle.leaf_der().unwrap();
        let leaf = crate::x509::parse_der(&der).unwrap();
        assert_eq!(
            leaf.tbs_certificate().issuer().to_string(),
            "CN=sigstore-intermediate,O=sigstore.dev"
        );
        // Exactly one certificate, per v0.3.
        assert!(bundle
            .verification_material
            .x509_certificate_chain
            .is_none());

        // The envelope verifies under the key that signed it.
        use spki::EncodePublicKey;
        let envelope = bundle.dsse_envelope.as_ref().unwrap();
        assert_eq!(
            envelope
                .verify_with_spki(key.verifying_key().to_public_key_der().unwrap().as_bytes())
                .unwrap(),
            0
        );

        // **And the half this fixture cannot check.** The recorded certificate
        // binds a key whose private half does not exist here, so the bundle
        // this test builds is deliberately one whose leaf does *not* certify
        // its own signature. Asserting that keeps the test from being read as
        // an end-to-end proof it is not; closing the gap needs a
        // human-approved identity token.
        let leaf_spki =
            der::Encode::to_der(leaf.tbs_certificate().subject_public_key_info()).unwrap();
        assert!(
            envelope.verify_with_spki(&leaf_spki).is_err(),
            "if this ever passes, a real end-to-end signature became available and this test \
             should be strengthened rather than deleted"
        );
    }

    /// **The ordering property that matters most in this file.** A certificate
    /// that does not carry the signing key stops the flow *before* the Rekor
    /// upload — because a Rekor entry is permanent, and a bundle whose leaf
    /// does not certify its own signature is unverifiable forever.
    #[test]
    fn a_certificate_that_does_not_carry_the_signing_key_stops_before_rekor() {
        let token = oidc::IdToken::parse(&oidc::testing::recorded_staging_token()).unwrap();
        let script = Script::new(vec![Exchange::ok(
            "/api/v2/signingCert",
            fixture("fulcio-staging-signingcert-embedded-sct.json"),
        )]);
        let e = sign_with_token(&script, &Endpoints::staging(), &token, b"{}").unwrap_err();
        match e {
            Error::Key { ref reason, .. } => {
                assert!(reason.contains("does not carry the public key"), "{reason}")
            }
            other => panic!("wrong variant: {other}"),
        }
        // One request, to Fulcio. Nothing was written to any log.
        assert_eq!(script.urls().len(), 1);
        assert!(script.urls()[0].contains("/api/v2/signingCert"));
    }

    /// The upload body is the one Rekor's `dsse` v0.0.1 schema describes, and
    /// the envelope inside it is the envelope that ends up in the bundle.
    #[test]
    fn the_rekor_upload_carries_the_same_envelope_the_bundle_does() {
        let (logged, entry_json) = logged_pair(LOGGED);
        let payload = logged.payload_bytes().unwrap();
        let script = Script::new(vec![
            Exchange::status("/api/v1/log/entries", 201, entry_json.clone()),
            Exchange::ok("/api/v1/log/entries/", entry_json),
        ]);
        let key = fulcio::ephemeral_key().unwrap();
        let bundle = log_and_assemble(
            &script,
            "https://rekor.sigstage.dev",
            &key,
            &recorded_staging_leaf(),
            &payload,
        )
        .unwrap();

        let sent = script.sent.borrow();
        let proposed = sent[0].json.as_ref().unwrap();
        assert_eq!(proposed["kind"], "dsse");
        assert_eq!(proposed["apiVersion"], "0.0.1");
        let uploaded: Envelope = serde_json::from_str(
            proposed["spec"]["proposedContent"]["envelope"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(&uploaded, bundle.dsse_envelope.as_ref().unwrap());
        assert_eq!(
            crate::bundle::b64_decode("payload", &uploaded.payload).unwrap(),
            payload
        );

        // The certificate Rekor is given as the verifier is the leaf that ends
        // up in the bundle. A log entry verified against a different
        // certificate from the one the bundle carries would be an entry for a
        // different signer.
        let verifier_pem = crate::bundle::b64_decode(
            "verifiers[]",
            proposed["spec"]["proposedContent"]["verifiers"][0]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let from_rekor = crate::x509::parse_pem_chain(&verifier_pem).unwrap();
        assert_eq!(
            fulcio::certificate_to_der(&from_rekor[0]).unwrap(),
            bundle.leaf_der().unwrap()
        );
    }
}
