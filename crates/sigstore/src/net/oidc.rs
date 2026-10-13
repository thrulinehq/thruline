//! The OAuth 2.0 device authorization grant (RFC 8628), as Sigstore's Dex
//! actually implements it.
//!
//! # The window is 300 seconds and it belongs to a human
//!
//! Staging's real answer, recorded in
//! `fixtures/net/oidc-staging-device-code.json`, is `"expires_in": 300` with
//! `"interval": 5`. Those five minutes are not ours to spend: they start when
//! the server issues the code and they are consumed by a person finding a
//! browser, reading a nine-character user code, and clicking approve. So
//! [`device_flow`] hands the URL and the code to its caller **before it sleeps
//! for the first time**, through a callback rather than a return value. A
//! function that returns the code only when the flow completes has already
//! spent the window it was supposed to open.
//!
//! `device_code_reaches_the_caller_before_the_first_poll` is that ordering,
//! asserted rather than described.
//!
//! # PKCE is not optional here, and the server says so
//!
//! A request without `code_challenge` gets, verbatim:
//!
//! ```text
//! {"error":"invalid_request","error_description":"PKCE S256 is required."}
//! ```
//!
//! recorded in `fixtures/net/oidc-staging-device-code-no-pkce.json`. There is
//! therefore no "PKCE off" path in this module to get wrong: [`Pkce`] is a
//! required argument, and the challenge is always S256.
//!
//! # `slow_down` raises the interval; `authorization_pending` does not
//!
//! RFC 8628 §3.5 gives the token endpoint four answers that are not a token.
//! Two of them mean *keep going*, and they differ: `authorization_pending`
//! means poll again unchanged, `slow_down` means poll again **and add five
//! seconds to the interval, permanently**. A client that treats them alike
//! keeps hammering a server that just asked it to stop, and gets rate-limited
//! out of a window it cannot get back. The other two — `access_denied` and
//! `expired_token` — are terminal and are reported as themselves rather than
//! folded into "the flow failed".
//!
//! # This module parses the id_token and does not trust it
//!
//! [`IdToken::parse`] base64-decodes the JWT's claim set to answer one
//! question: *which string does Fulcio want signed?* It does **not** verify
//! the token's signature, check `aud`, or check `exp`, and it must never be
//! read as having done so. Fulcio verifies the token against the issuer's JWKS
//! — that is the whole point of handing it over — and a client that
//! independently "validated" a token it just received from the issuer over TLS
//! would be adding a second, weaker opinion, not a second check.

use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::net::{HttpResponse, Transport};

/// The scope Sigstore's Fulcio needs from an email issuer.
///
/// `email` is not decoration: the staging Dex issuer is configured
/// `issuerType: email` (see `fixtures/net/fulcio-staging-configuration.json`),
/// and Fulcio's proof of possession for that issuer is over the **email**
/// claim. Ask only for `openid` and the token has no email, so the proof is
/// computed over the wrong string and every request 400s.
pub const DEFAULT_SCOPE: &str = "openid email";

/// RFC 8628 §3.5's default when the server omits `interval`.
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;

/// What `slow_down` adds to the interval, per RFC 8628 §3.5.
pub const SLOW_DOWN_INCREMENT_SECS: u64 = 5;

/// The longest this client will ever sleep between polls, whatever the server
/// says.
///
/// # The server picks the sleep, and nothing used to bound it
///
/// `interval` comes off the wire. RFC 8628 puts no ceiling on it, and this
/// module's per-request timeout does not apply to a `thread::sleep`. A device
/// authorization response of `{"interval": 18446744073709551615}` — measured —
/// makes [`DeviceCode::poll_interval`] return 584 billion years and wedges the
/// signing process in a sleep no timeout will interrupt. The same value then
/// **panics** on the first `slow_down`: `Duration + Duration` overflows, and
/// `Duration`'s `Add` panics rather than saturating (*"overflow when adding
/// durations"*, reproduced as a test).
///
/// 300 seconds is the whole window a device code lives for on staging, so a
/// single sleep at the ceiling still cannot outlast the thing it is waiting
/// for. It is a bound on this client's behaviour, not a claim about the
/// server's.
pub const MAX_POLL_INTERVAL_SECS: u64 = 300;

// --- PKCE ------------------------------------------------------------------

/// A PKCE verifier and its S256 challenge (RFC 7636).
///
/// Held together in one type so the challenge cannot be computed from one
/// string and the verifier sent from another — a mismatch the server reports
/// only as a generic `invalid_request`, which is indistinguishable from a
/// dozen other mistakes.
#[derive(Debug, Clone)]
pub struct Pkce {
    verifier: String,
    challenge: String,
}

/// base64url without padding — the only encoding RFC 7636 §4.2 permits for the
/// challenge, and what RFC 8628 clients use for the verifier.
pub fn b64url_nopad(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

impl Pkce {
    /// A fresh verifier from 32 bytes of system entropy.
    ///
    /// 32 bytes is 43 base64url characters, the maximum RFC 7636 §4.1 allows
    /// without exceeding its 128-character ceiling by any sensible margin, and
    /// comfortably above its 43-character floor.
    pub fn generate() -> Result<Pkce> {
        let mut raw = [0u8; 32];
        getrandom::fill(&mut raw).map_err(|e| {
            Error::Oidc(format!(
                "the operating system's random number generator failed while creating a PKCE \
             verifier: {e}"
            ))
        })?;
        let pkce = Pkce::from_verifier(&b64url_nopad(&raw));
        // The verifier is a secret only for the length of the flow, but there
        // is no reason to leave it on the stack afterwards.
        use p256::elliptic_curve::zeroize::Zeroize as _;
        raw.zeroize();
        Ok(pkce)
    }

    /// Derive the challenge from a given verifier.
    ///
    /// Public so the derivation can be checked against RFC 7636's own vector
    /// rather than only against itself.
    pub fn from_verifier(verifier: &str) -> Pkce {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(verifier.as_bytes());
        Pkce {
            verifier: verifier.to_string(),
            challenge: b64url_nopad(&digest),
        }
    }

    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    pub fn challenge(&self) -> &str {
        &self.challenge
    }
}

// --- discovery -------------------------------------------------------------

/// The issuer's `.well-known/openid-configuration`.
///
/// Fetched rather than guessed. Concatenating `<issuer>/device/code` happens
/// to be right for the staging Dex — the discovery document confirms it — and
/// is right for nothing in general. More to the point, a guessed endpoint that
/// 404s produces a confusing error three steps later, while a missing
/// `device_authorization_endpoint` is a *specific* fact worth naming: this
/// issuer does not do the device grant, so no amount of retrying will help.
#[derive(Debug, Clone, Deserialize)]
pub struct Discovery {
    pub issuer: String,
    #[serde(default)]
    pub device_authorization_endpoint: Option<String>,
    pub token_endpoint: String,
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
    #[serde(default)]
    pub grant_types_supported: Vec<String>,
}

/// RFC 8628's grant type URN. Sent as `grant_type`, and listed by the issuer in
/// `grant_types_supported`.
pub const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

impl Discovery {
    pub fn parse(body: &str) -> Result<Discovery> {
        serde_json::from_str(body).map_err(|e| {
            Error::Oidc(format!(
                "the issuer's openid-configuration is not the shape this build \
                                 understands: {e}"
            ))
        })
    }

    pub fn fetch(t: &dyn Transport, issuer: &str) -> Result<Discovery> {
        let url = format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        );
        let r = t.get(&url, "fetching the OIDC issuer's discovery document")?;
        if !r.is_success() {
            return Err(Error::Oidc(format!(
                "{url} answered {} rather than a discovery document: {}",
                r.status,
                truncate(&r.body)
            )));
        }
        let d = Discovery::parse(&r.body)?;
        d.check_issuer_matches(issuer)?;
        Ok(d)
    }

    /// RFC 8414 §3.3: the `issuer` in the metadata **MUST** be identical to the
    /// issuer that was asked.
    ///
    /// # What this document gets to decide
    ///
    /// Everything. `device_authorization_endpoint` and `token_endpoint` come
    /// out of it and are contacted verbatim — measured: a discovery response
    /// naming `https://elsewhere.example/...` sent the whole device flow there,
    /// and nothing said a word. That alone is not a trust break (the document
    /// arrived from the issuer over TLS), but this crate's *"staging is the
    /// default and production is a thing you must type"* rule is enforced by
    /// warning on the **configured** issuer string, and the identity a
    /// certificate ends up naming comes from wherever the token was actually
    /// minted. A document that answers for one issuer while naming another is
    /// the RFC's mix-up case, and it is one comparison to refuse.
    ///
    /// Compared with a trailing slash trimmed on both sides, because
    /// `https://oauth2.sigstage.dev/auth/` and `https://oauth2.sigstage.dev/auth`
    /// are the same configured issuer and only one of them is what Dex returns.
    pub fn check_issuer_matches(&self, asked: &str) -> Result<()> {
        if self.issuer.trim_end_matches('/') == asked.trim_end_matches('/') {
            return Ok(());
        }
        Err(Error::Oidc(format!(
            "the discovery document at {asked} identifies its issuer as {}; RFC 8414 §3.3 \
             requires these to be identical, and a document that answers for one issuer while \
             naming another chooses the endpoints that mint the identity",
            self.issuer
        )))
    }

    /// The device authorization endpoint, or a refusal that says why.
    ///
    /// **Not a fallback to a guessed path.** An issuer that advertises no
    /// device endpoint does not have one, and inventing a URL turns "this
    /// issuer cannot do what you asked" into "something went wrong at a URL
    /// you never configured".
    pub fn device_endpoint(&self) -> Result<&str> {
        self.device_authorization_endpoint
            .as_deref()
            .ok_or_else(|| {
                Error::Oidc(format!(
                "issuer {} advertises no device_authorization_endpoint, so it cannot complete a \
                 device flow; supply an identity token obtained some other way",
                self.issuer
            ))
            })
    }

    /// Refuse an issuer that cannot do PKCE S256.
    ///
    /// Checked only when the issuer publishes the list. An absent list is *not*
    /// treated as support: it is treated as unknown, and we proceed, because
    /// RFC 8414 makes the field optional and the server's own error is the
    /// authoritative answer. That is stated here so nobody later reads the
    /// missing check as a passed one.
    pub fn check_s256(&self) -> Result<()> {
        if self.code_challenge_methods_supported.is_empty() {
            return Ok(());
        }
        if self
            .code_challenge_methods_supported
            .iter()
            .any(|m| m == "S256")
        {
            return Ok(());
        }
        Err(Error::Oidc(format!(
            "issuer {} supports code challenge methods {:?}, and this build only sends S256; \
             `plain` is not implemented because Sigstore's own issuers require S256",
            self.issuer, self.code_challenge_methods_supported
        )))
    }
}

// --- the device code -------------------------------------------------------

/// The device authorization response (RFC 8628 §3.2).
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    /// The client's handle for the pending authorization. **Not** for a human.
    pub device_code: String,
    /// The short code a human types. Nine characters with a dash, on staging.
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    /// Seconds. Staging says 300.
    pub expires_in: u64,
    #[serde(default)]
    pub interval: Option<u64>,
}

impl DeviceCode {
    /// How long to sleep before the next poll, clamped to
    /// [`MAX_POLL_INTERVAL_SECS`].
    ///
    /// The clamp is here rather than in [`poll_for_token`] because this
    /// accessor's entire purpose is "how long should I sleep", and a caller
    /// driving [`poll_once`] in its own loop deserves the same bound. See the
    /// constant for the measurement.
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(
            self.interval
                .unwrap_or(DEFAULT_POLL_INTERVAL_SECS)
                .min(MAX_POLL_INTERVAL_SECS),
        )
    }

    pub fn lifetime(&self) -> Duration {
        Duration::from_secs(self.expires_in)
    }

    /// The URL to put in front of a person: the pre-filled one when the server
    /// offers it, since a code typed by hand is a code typed wrong.
    pub fn best_uri(&self) -> &str {
        self.verification_uri_complete
            .as_deref()
            .unwrap_or(&self.verification_uri)
    }
}

/// The default way to show a device code: three lines on stderr.
///
/// stderr, not stdout, because `thruline` commands write machine-readable
/// output to stdout and a signing prompt in the middle of it would corrupt a
/// pipe.
pub fn print_prompt(code: &DeviceCode) {
    eprintln!("\nSigstore needs you to approve this signature in a browser.");
    eprintln!("  open : {}", code.best_uri());
    eprintln!("  code : {}", code.user_code);
    eprintln!("  this expires in {} seconds.\n", code.lifetime().as_secs());
}

/// Ask the issuer for a device code.
pub fn request_device_code(
    t: &dyn Transport,
    endpoint: &str,
    client_id: &str,
    scope: &str,
    pkce: &Pkce,
) -> Result<DeviceCode> {
    let r = t.post_form(
        endpoint,
        &[
            ("client_id", client_id),
            ("scope", scope),
            ("code_challenge", pkce.challenge()),
            ("code_challenge_method", "S256"),
        ],
        "requesting a device code from the OIDC issuer",
    )?;
    if !r.is_success() {
        return Err(Error::Oidc(format!(
            "the device authorization endpoint answered {}: {}",
            r.status,
            truncate(&r.body)
        )));
    }
    serde_json::from_str(&r.body).map_err(|e| {
        Error::Oidc(format!(
            "the device authorization response is not the shape RFC 8628 §3.2 describes: {e}"
        ))
    })
}

// --- polling ---------------------------------------------------------------

/// The three non-terminal answers the token endpoint can give.
///
/// A three-state enum rather than `Option<IdToken>` for the same reason
/// `crates/attest`'s `Outcome` has three states: *"not yet"* and *"back off"*
/// are different instructions, and collapsing them loses the only thing the
/// server said.
#[derive(Debug)]
pub enum Poll {
    Granted(Box<IdToken>),
    /// Poll again after the current interval.
    Pending,
    /// Poll again, and raise the interval by [`SLOW_DOWN_INCREMENT_SECS`].
    SlowDown,
}

#[derive(Debug, Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenGrant {
    #[serde(default)]
    id_token: Option<String>,
}

/// Classify one token-endpoint response.
///
/// Pure, so every branch can be driven from the real bodies staging returns
/// (recorded under `fixtures/net/oidc-staging-token-*.json`) instead of being
/// reasoned about.
pub fn classify_token_response(r: &HttpResponse) -> Result<Poll> {
    if r.is_success() {
        let grant: TokenGrant = serde_json::from_str(&r.body).map_err(|e| {
            Error::Oidc(format!(
                "the token response is not JSON this build understands: {e}"
            ))
        })?;
        let raw = grant.id_token.ok_or_else(|| {
            Error::Oidc(
                "the token endpoint returned a grant with no id_token; the `openid` scope was \
                 probably not granted, and an access token cannot identify anyone to Fulcio"
                    .to_string(),
            )
        })?;
        return Ok(Poll::Granted(Box::new(IdToken::parse(&raw)?)));
    }

    // Everything else is an OAuth error object. RFC 8628 §3.5 defines four
    // codes; anything outside them is terminal by default, because guessing
    // that an unknown code means "retry" is how a client spins.
    let err: TokenError = serde_json::from_str(&r.body).map_err(|_| {
        Error::Oidc(format!(
            "the token endpoint answered {} with a body that is not an OAuth error object: {}",
            r.status,
            truncate(&r.body)
        ))
    })?;
    match err.error.as_str() {
        "authorization_pending" => Ok(Poll::Pending),
        "slow_down" => Ok(Poll::SlowDown),
        "access_denied" => Err(Error::Oidc(
            "the request was denied in the browser; nothing was signed".to_string(),
        )),
        "expired_token" => Err(Error::Oidc(
            "the device code expired before it was approved; start again".to_string(),
        )),
        other => Err(Error::Oidc(format!(
            "the token endpoint refused with {other:?}: {}",
            err.error_description.as_deref().unwrap_or("no description")
        ))),
    }
}

/// One poll of the token endpoint.
pub fn poll_once(
    t: &dyn Transport,
    token_endpoint: &str,
    client_id: &str,
    device_code: &str,
    pkce: &Pkce,
) -> Result<Poll> {
    let r = t.post_form(
        token_endpoint,
        &[
            ("grant_type", DEVICE_GRANT_TYPE),
            ("device_code", device_code),
            ("client_id", client_id),
            ("code_verifier", pkce.verifier()),
        ],
        "polling the OIDC token endpoint",
    )?;
    classify_token_response(&r)
}

/// Poll until the code is approved, denied, or the window closes.
///
/// `sleep` is a parameter so the interval arithmetic — the part `slow_down`
/// makes non-trivial — can be observed in a test without a test that takes
/// fifteen real seconds to run. `elapsed` likewise: a deadline that can only
/// be reached by waiting is a deadline that never gets tested.
pub fn poll_for_token(
    t: &dyn Transport,
    token_endpoint: &str,
    client_id: &str,
    code: &DeviceCode,
    pkce: &Pkce,
    sleep: &mut dyn FnMut(Duration),
    elapsed: &mut dyn FnMut() -> Duration,
) -> Result<IdToken> {
    let mut interval = code.poll_interval();
    let deadline = code.lifetime();
    loop {
        if elapsed() >= deadline {
            return Err(Error::Oidc(format!(
                "the device code expired after {} seconds without being approved",
                deadline.as_secs()
            )));
        }
        sleep(interval);
        match poll_once(t, token_endpoint, client_id, &code.device_code, pkce)? {
            Poll::Granted(token) => return Ok(*token),
            Poll::Pending => {}
            Poll::SlowDown => {
                // `saturating_add`, not `+`: `Duration`'s `Add` panics on
                // overflow, and the left operand came off the wire. The clamp
                // is the same one `poll_interval` applies, so an interval that
                // was already at the ceiling stays there instead of climbing.
                interval = interval
                    .saturating_add(Duration::from_secs(SLOW_DOWN_INCREMENT_SECS))
                    .min(Duration::from_secs(MAX_POLL_INTERVAL_SECS));
            }
        }
    }
}

/// The whole device flow: discover, request a code, show it, poll.
///
/// `prompt` is called **once, immediately**, with the device code — before the
/// first sleep and before the first poll. See the module docs.
pub fn device_flow(
    t: &dyn Transport,
    issuer: &str,
    client_id: &str,
    prompt: &mut dyn FnMut(&DeviceCode),
) -> Result<IdToken> {
    let discovery = Discovery::fetch(t, issuer)?;
    discovery.check_s256()?;
    let pkce = Pkce::generate()?;
    let code = request_device_code(
        t,
        discovery.device_endpoint()?,
        client_id,
        DEFAULT_SCOPE,
        &pkce,
    )?;
    prompt(&code);
    let started = Instant::now();
    poll_for_token(
        t,
        &discovery.token_endpoint,
        client_id,
        &code,
        &pkce,
        &mut |d| std::thread::sleep(d),
        &mut || started.elapsed(),
    )
}

// --- the token -------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
struct Claims {
    #[serde(default)]
    iss: String,
    #[serde(default)]
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    exp: Option<i64>,
}

/// An OIDC identity token, plus the claims that decide Fulcio's challenge.
///
/// The raw string is what goes to Fulcio; the parsed claims never leave this
/// process. See the module docs on why nothing here is *verified*.
#[derive(Debug, Clone)]
pub struct IdToken {
    raw: String,
    claims: Claims,
}

impl IdToken {
    /// Split a JWT and decode its claim set.
    ///
    /// The claim set is base64**url** with the padding stripped — a JWT is not
    /// standard base64, and a decoder that assumes it fails on roughly one
    /// token in four (whenever the payload length is not a multiple of three).
    pub fn parse(raw: &str) -> Result<IdToken> {
        use base64::Engine as _;
        let raw = raw.trim();
        let mut parts = raw.split('.');
        let (_header, payload) = match (parts.next(), parts.next(), parts.next()) {
            (Some(h), Some(p), Some(_sig)) => (h, p),
            _ => {
                return Err(Error::Oidc(
                    "the identity token is not a three-part JWT".to_string(),
                ))
            }
        };
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|e| {
                Error::Oidc(format!(
                    "the identity token's claim set is not base64url: {e}"
                ))
            })?;
        let claims: Claims = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Oidc(format!("the identity token's claim set is not JSON: {e}")))?;
        Ok(IdToken {
            raw: raw.to_string(),
            claims,
        })
    }

    /// The bytes Fulcio is handed.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn issuer(&self) -> &str {
        &self.claims.iss
    }

    pub fn subject(&self) -> &str {
        &self.claims.sub
    }

    pub fn email(&self) -> Option<&str> {
        self.claims.email.as_deref()
    }

    pub fn expiry(&self) -> Option<i64> {
        self.claims.exp
    }

    /// **The string Fulcio verifies the proof of possession over.**
    ///
    /// # The documentation is wrong and the code is right
    ///
    /// `fulcio.proto` says of `proof_of_possession`:
    ///
    /// > *This is a currently a signature over the `sub` claim from the OIDC
    /// > identity token*
    ///
    /// and Fulcio's Swagger says the same. Both are wrong for the issuer this
    /// crate defaults to. `pkg/server/grpc_server.go` verifies the proof
    /// against `principal.Name(ctx)`, and for an **email** issuer —
    /// `oauth2.sigstage.dev/auth` is configured `issuerType: email`, see
    /// `fixtures/net/fulcio-staging-configuration.json` —
    /// `pkg/identity/email/principal.go` returns the *email address*.
    ///
    /// The rule below is not a special case for email issuers; it is
    /// `sigstore/sigstore`'s own `oauthflow.SubjectFromToken`, which Fulcio's
    /// Chainguard issuer adopts with the comment *"we MUST use this to
    /// implement Name() or tokens that embed an email claim will fail to
    /// sign"*. Every other principal in Fulcio's tree returns `token.Subject`,
    /// which is what this returns when there is no email claim. Getting it
    /// wrong is not subtle in its effect and is completely opaque in its
    /// message: **every** request 400s with `"There was an error processing
    /// the identity token"`, which is the same body Fulcio returns for a
    /// malformed token, a wrong audience, and an unknown issuer.
    ///
    /// The `email_verified` check is Fulcio's too: an unverified email is
    /// rejected server-side, so refusing here turns a remote 400 into a local
    /// sentence that says which claim was at fault.
    pub fn proof_subject(&self) -> Result<&str> {
        match self.claims.email.as_deref() {
            Some(email) if !email.is_empty() => {
                if self.claims.email_verified != Some(true) {
                    return Err(Error::Oidc(format!(
                        "the identity token carries email {email:?} but email_verified is not \
                         true; Fulcio refuses unverified email identities"
                    )));
                }
                Ok(email)
            }
            _ if !self.claims.sub.is_empty() => Ok(&self.claims.sub),
            _ => Err(Error::Oidc(
                "the identity token has neither an email nor a sub claim, so there is no string \
                 to prove possession over"
                    .to_string(),
            )),
        }
    }
}

fn truncate(s: &str) -> String {
    const MAX: usize = 400;
    if s.len() <= MAX {
        return s.to_string();
    }
    let mut end = MAX;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes)", &s[..end], s.len())
}

#[cfg(test)]
pub(crate) mod testing {
    //! Constructed identity tokens.
    //!
    //! **These are built here, not recorded.** Completing the device flow needs
    //! a human to approve in a browser, which a test suite cannot do, so no
    //! real staging `id_token` exists in this repository. What a constructed
    //! token *does* exercise is every line of client code that touches one:
    //! JWT splitting, base64url decoding, claim extraction, and the proof
    //! subject rule. What it cannot exercise is Fulcio's acceptance of it —
    //! see `docs/` and this module's report for that gap, stated rather than
    //! papered over.

    use super::b64url_nopad;

    pub fn token_with_claims(claims: serde_json::Value) -> String {
        let header = b64url_nopad(br#"{"alg":"RS256","kid":"test"}"#);
        let payload = b64url_nopad(claims.to_string().as_bytes());
        let signature = b64url_nopad(b"not-a-real-signature");
        format!("{header}.{payload}.{signature}")
    }

    /// A token shaped like what `oauth2.sigstage.dev/auth` issues for
    /// `scope=openid email`: the claim names and the issuer string are real
    /// (from the recorded discovery document and Fulcio's issuer config), the
    /// values are not.
    pub fn recorded_staging_token() -> String {
        token_with_claims(serde_json::json!({
            "iss": "https://oauth2.sigstage.dev/auth",
            "sub": "ChUxMDkyNzE1MjE2MjA0NjcxNzQ2NjcSBmdvb2dsZQ",
            "aud": "sigstore",
            "exp": 1_899_999_999i64,
            "email": "signer@example.com",
            "email_verified": true,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{recorded_staging_token, token_with_claims};
    use super::*;
    use crate::net::testing::{fixture, Exchange, Script};

    // --- PKCE ------------------------------------------------------------

    /// RFC 7636 Appendix B's own vector. Checking the derivation against the
    /// RFC rather than against itself is the difference between "this function
    /// is self-consistent" and "this function computes S256".
    #[test]
    fn the_s256_challenge_matches_rfc7636_appendix_b() {
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(
            pkce.challenge(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    /// And the vector this crate actually sent to staging, whose 200 is
    /// recorded in `fixtures/net/oidc-staging-device-code.json`. If the
    /// derivation ever drifts, this catches it against a challenge a real
    /// server accepted.
    #[test]
    fn the_challenge_sent_to_staging_is_reproducible() {
        let pkce = Pkce::from_verifier("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE");
        assert_eq!(
            pkce.challenge(),
            "VtX6czP210fbQsI5QH5dpMMvTHnzXQkrE0_TWkAtnFw"
        );
    }

    /// base64url, not base64: a challenge containing `+` or `/` is rejected by
    /// the server, and padding makes it a different string.
    #[test]
    fn the_challenge_encoding_is_url_safe_and_unpadded() {
        // 0xfb 0xff 0xbe encodes to "+/++" in standard base64.
        let s = b64url_nopad(&[0xfb, 0xff, 0xbe, 0x01]);
        assert!(
            !s.contains('+') && !s.contains('/') && !s.contains('='),
            "{s}"
        );
        for _ in 0..8 {
            let p = Pkce::generate().unwrap();
            assert_eq!(p.verifier().len(), 43);
            assert_eq!(p.challenge().len(), 43);
            assert!(!p.challenge().contains('='));
        }
    }

    #[test]
    fn two_generated_verifiers_differ() {
        let a = Pkce::generate().unwrap();
        let b = Pkce::generate().unwrap();
        assert_ne!(a.verifier(), b.verifier());
        assert_ne!(a.challenge(), b.challenge());
    }

    // --- discovery -------------------------------------------------------

    #[test]
    fn the_recorded_staging_discovery_document_parses_and_names_the_device_endpoint() {
        let d = Discovery::parse(&fixture("oidc-staging-openid-configuration.json")).unwrap();
        assert_eq!(d.issuer, "https://oauth2.sigstage.dev/auth");
        assert_eq!(d.token_endpoint, "https://oauth2.sigstage.dev/auth/token");
        assert_eq!(
            d.device_endpoint().unwrap(),
            "https://oauth2.sigstage.dev/auth/device/code"
        );
        assert!(d
            .grant_types_supported
            .iter()
            .any(|g| g == DEVICE_GRANT_TYPE));
        d.check_s256().unwrap();
    }

    /// An issuer with no device endpoint is refused by name rather than by a
    /// guessed URL that 404s later.
    #[test]
    fn an_issuer_without_a_device_endpoint_is_refused_by_name() {
        let d = Discovery::parse(
            r#"{"issuer":"https://example.test","token_endpoint":"https://example.test/token"}"#,
        )
        .unwrap();
        let e = d.device_endpoint().unwrap_err();
        assert!(
            e.to_string().contains("device_authorization_endpoint"),
            "{e}"
        );
    }

    /// A `plain`-only issuer is refused, because this build never sends
    /// `plain` and a challenge the server cannot check is not a challenge.
    #[test]
    fn an_issuer_that_cannot_do_s256_is_refused() {
        let d = Discovery::parse(
            r#"{"issuer":"https://example.test","token_endpoint":"https://example.test/token",
                "code_challenge_methods_supported":["plain"]}"#,
        )
        .unwrap();
        assert!(d.check_s256().is_err());
        // Silence is not consent, but it is also not a refusal: an issuer that
        // publishes no list is proceeded with, deliberately.
        let quiet = Discovery::parse(
            r#"{"issuer":"https://example.test","token_endpoint":"https://example.test/token"}"#,
        )
        .unwrap();
        quiet.check_s256().unwrap();
    }

    // --- the device code -------------------------------------------------

    /// Staging's real 200, field for field.
    #[test]
    fn the_recorded_staging_device_code_response_parses() {
        let c: DeviceCode =
            serde_json::from_str(&fixture("oidc-staging-device-code.json")).unwrap();
        assert_eq!(c.user_code, "PZNG-PNJS");
        assert_eq!(
            c.verification_uri,
            "https://oauth2.sigstage.dev/auth/device"
        );
        assert_eq!(
            c.verification_uri_complete.as_deref(),
            Some("https://oauth2.sigstage.dev/auth/device?user_code=PZNG-PNJS")
        );
        assert_eq!(c.expires_in, 300);
        assert_eq!(c.poll_interval(), Duration::from_secs(5));
        assert_eq!(c.lifetime(), Duration::from_secs(300));
        // The pre-filled URL is what a human should be given.
        assert!(c.best_uri().contains("user_code=PZNG-PNJS"));
    }

    #[test]
    fn a_device_code_without_a_complete_uri_falls_back_to_the_plain_one() {
        let c: DeviceCode = serde_json::from_str(
            r#"{"device_code":"d","user_code":"U","verification_uri":"https://x/y","expires_in":600}"#,
        )
        .unwrap();
        assert_eq!(c.best_uri(), "https://x/y");
        // RFC 8628 §3.2: absent `interval` means five seconds.
        assert_eq!(c.poll_interval(), Duration::from_secs(5));
    }

    /// The request carries PKCE. Staging's answer to one that does not is
    /// recorded, so this is a rule with a receipt and not a preference.
    #[test]
    fn the_device_code_request_always_carries_an_s256_challenge() {
        let no_pkce = fixture("oidc-staging-device-code-no-pkce.json");
        assert!(no_pkce.contains("PKCE S256 is required"), "{no_pkce}");

        let script = Script::new(vec![Exchange::ok(
            "/device/code",
            fixture("oidc-staging-device-code.json"),
        )]);
        let pkce = Pkce::from_verifier("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE");
        request_device_code(
            &script,
            "https://oauth2.sigstage.dev/auth/device/code",
            "sigstore",
            DEFAULT_SCOPE,
            &pkce,
        )
        .unwrap();

        let sent = script.sent.borrow();
        let form: std::collections::HashMap<_, _> = sent[0].form.iter().cloned().collect();
        assert_eq!(form["client_id"], "sigstore");
        assert_eq!(form["scope"], "openid email");
        assert_eq!(form["code_challenge_method"], "S256");
        assert_eq!(
            form["code_challenge"],
            "VtX6czP210fbQsI5QH5dpMMvTHnzXQkrE0_TWkAtnFw"
        );
        // The verifier must never be sent with the challenge; that would make
        // PKCE decorative.
        assert!(!sent[0].form.iter().any(|(k, _)| k == "code_verifier"));
    }

    // --- polling ---------------------------------------------------------

    /// Staging's real `authorization_pending` body, classified as "keep going"
    /// despite arriving with a 400.
    #[test]
    fn a_real_authorization_pending_400_is_not_an_error() {
        let r = HttpResponse {
            status: 400,
            location: None,
            body: fixture("oidc-staging-token-authorization-pending.json"),
        };
        assert!(matches!(
            classify_token_response(&r).unwrap(),
            Poll::Pending
        ));
    }

    /// Staging's real answer to a device code it has never seen. This one *is*
    /// terminal: retrying a code the server does not know never succeeds.
    #[test]
    fn a_real_invalid_device_code_400_is_terminal() {
        let r = HttpResponse {
            status: 400,
            location: None,
            body: fixture("oidc-staging-token-invalid-device-code.json"),
        };
        let e = classify_token_response(&r).unwrap_err();
        assert!(e.to_string().contains("Invalid Device code"), "{e}");
    }

    #[test]
    fn slow_down_and_pending_are_told_apart() {
        let pending = HttpResponse {
            status: 400,
            location: None,
            body: r#"{"error":"authorization_pending"}"#.into(),
        };
        let slow = HttpResponse {
            status: 400,
            location: None,
            body: r#"{"error":"slow_down"}"#.into(),
        };
        assert!(matches!(
            classify_token_response(&pending).unwrap(),
            Poll::Pending
        ));
        assert!(matches!(
            classify_token_response(&slow).unwrap(),
            Poll::SlowDown
        ));
    }

    #[test]
    fn denial_and_expiry_are_reported_as_themselves() {
        let denied = HttpResponse {
            status: 400,
            location: None,
            body: r#"{"error":"access_denied"}"#.into(),
        };
        let expired = HttpResponse {
            status: 400,
            location: None,
            body: r#"{"error":"expired_token"}"#.into(),
        };
        assert!(classify_token_response(&denied)
            .unwrap_err()
            .to_string()
            .contains("denied in the browser"));
        assert!(classify_token_response(&expired)
            .unwrap_err()
            .to_string()
            .contains("expired"));
    }

    /// A grant with no `id_token` is not a success. An access token cannot
    /// identify anyone to Fulcio, and treating the 200 as a win would send an
    /// empty string as the credential.
    #[test]
    fn a_grant_without_an_id_token_is_refused() {
        let r = HttpResponse {
            status: 200,
            location: None,
            body: r#"{"access_token":"a","token_type":"bearer"}"#.into(),
        };
        let e = classify_token_response(&r).unwrap_err();
        assert!(e.to_string().contains("no id_token"), "{e}");
    }

    /// An unrecognised OAuth error code is terminal, not a retry. A client
    /// that retries on anything it does not recognise spins until the window
    /// closes and then reports the wrong cause.
    #[test]
    fn an_unknown_oauth_error_code_stops_the_flow() {
        let r = HttpResponse {
            status: 400,
            location: None,
            body: r#"{"error":"invalid_client","error_description":"who?"}"#.into(),
        };
        let e = classify_token_response(&r).unwrap_err();
        assert!(e.to_string().contains("invalid_client"), "{e}");
    }

    /// `slow_down` must raise the interval, and `authorization_pending` must
    /// not. Observed through the injected sleeper, so the arithmetic is
    /// checked rather than the elapsed time.
    #[test]
    fn slow_down_raises_the_poll_interval_and_pending_leaves_it_alone() {
        let code: DeviceCode =
            serde_json::from_str(&fixture("oidc-staging-device-code.json")).unwrap();
        let script = Script::new(vec![
            Exchange::status("/token", 400, r#"{"error":"authorization_pending"}"#),
            Exchange::status("/token", 400, r#"{"error":"slow_down"}"#),
            Exchange::status("/token", 400, r#"{"error":"authorization_pending"}"#),
            Exchange::status("/token", 400, r#"{"error":"slow_down"}"#),
            Exchange::ok(
                "/token",
                format!(r#"{{"id_token":"{}"}}"#, recorded_staging_token()),
            ),
        ]);
        let mut slept: Vec<u64> = Vec::new();
        let token = poll_for_token(
            &script,
            "https://oauth2.sigstage.dev/auth/token",
            "sigstore",
            &code,
            &Pkce::from_verifier("v"),
            &mut |d| slept.push(d.as_secs()),
            &mut || Duration::from_secs(0),
        )
        .unwrap();

        assert_eq!(slept, vec![5, 5, 10, 10, 15]);
        assert_eq!(token.email(), Some("signer@example.com"));
    }

    /// **The sleep the server picks, and the two ways it went wrong.** An
    /// `interval` of `u64::MAX` used to make `poll_interval()` return 584
    /// billion years — a `thread::sleep` no request timeout can interrupt — and
    /// then *panic* on the first `slow_down`, because `Duration`'s `Add`
    /// overflows rather than saturating.
    #[test]
    fn a_hostile_poll_interval_is_clamped_and_does_not_overflow() {
        let code: DeviceCode = serde_json::from_str(
            r#"{"device_code":"d","user_code":"U","verification_uri":"https://x",
                "expires_in":300,"interval":18446744073709551615}"#,
        )
        .unwrap();
        assert_eq!(
            code.poll_interval(),
            Duration::from_secs(MAX_POLL_INTERVAL_SECS),
            "the raw interval must never reach a sleep"
        );

        let script = Script::new(vec![
            Exchange::status("/token", 400, r#"{"error":"slow_down"}"#),
            Exchange::status("/token", 400, r#"{"error":"slow_down"}"#),
            Exchange::ok(
                "/token",
                format!(r#"{{"id_token":"{}"}}"#, recorded_staging_token()),
            ),
        ]);
        let mut slept: Vec<u64> = Vec::new();
        // Reaching this line at all is the assertion: `interval + 5s` on an
        // interval of u64::MAX aborts the process.
        poll_for_token(
            &script,
            "https://x/token",
            "c",
            &code,
            &Pkce::from_verifier("v"),
            &mut |d| slept.push(d.as_secs()),
            &mut || Duration::from_secs(0),
        )
        .unwrap();
        assert_eq!(
            slept,
            vec![300, 300, 300],
            "the ceiling holds across slow_down"
        );
    }

    /// The ceiling does not disturb the interval arithmetic a real server
    /// produces: staging says 5, and 5 is nowhere near it.
    #[test]
    fn the_recorded_staging_interval_is_far_below_the_ceiling() {
        let c: DeviceCode =
            serde_json::from_str(&fixture("oidc-staging-device-code.json")).unwrap();
        assert_eq!(c.poll_interval(), Duration::from_secs(5));
        assert_eq!(
            c.interval,
            Some(5),
            "the clamp must not be what produced the 5"
        );
    }

    // --- the discovery document does not get to be somebody else ----------

    /// RFC 8414 §3.3. Before this check, a discovery document could name any
    /// endpoints it liked and the whole flow followed them: measured, a
    /// response served for `oauth2.sigstage.dev` sent the device-code request
    /// and the token exchange to `elsewhere.example` and nothing said a word.
    #[test]
    fn a_discovery_document_that_names_a_different_issuer_is_refused() {
        let script = Script::new(vec![Exchange::ok(
            "/.well-known/openid-configuration",
            r#"{"issuer":"https://oauth2.sigstore.dev/auth",
                "device_authorization_endpoint":"https://elsewhere.example/device/code",
                "token_endpoint":"https://elsewhere.example/token"}"#,
        )]);
        let mut prompts = 0;
        let e = device_flow(
            &script,
            "https://oauth2.sigstage.dev/auth",
            "sigstore",
            &mut |_| prompts += 1,
        )
        .unwrap_err();
        assert!(e.to_string().contains("identifies its issuer as"), "{e}");
        // One request. Nothing at `elsewhere.example` was ever contacted, and
        // no human was prompted to approve anything there.
        assert_eq!(script.urls().len(), 1);
        assert_eq!(prompts, 0);
    }

    /// A trailing slash is the same configured issuer, and must not be a
    /// refusal — Dex answers with the unslashed form.
    #[test]
    fn a_trailing_slash_on_the_configured_issuer_is_not_a_mismatch() {
        let d = Discovery::parse(&fixture("oidc-staging-openid-configuration.json")).unwrap();
        d.check_issuer_matches("https://oauth2.sigstage.dev/auth")
            .unwrap();
        d.check_issuer_matches("https://oauth2.sigstage.dev/auth/")
            .unwrap();
        assert!(d
            .check_issuer_matches("https://oauth2.sigstore.dev/auth")
            .is_err());
    }

    /// The deadline is enforced. Without it a flow whose code silently expired
    /// polls a dead endpoint forever.
    #[test]
    fn the_poll_loop_gives_up_when_the_device_code_window_closes() {
        let code: DeviceCode =
            serde_json::from_str(&fixture("oidc-staging-device-code.json")).unwrap();
        let script = Script::new(vec![]);
        let e = poll_for_token(
            &script,
            "https://oauth2.sigstage.dev/auth/token",
            "sigstore",
            &code,
            &Pkce::from_verifier("v"),
            &mut |_| {},
            &mut || Duration::from_secs(300),
        )
        .unwrap_err();
        assert!(e.to_string().contains("expired after 300 seconds"), "{e}");
        assert!(
            script.sent.borrow().is_empty(),
            "an expired code must not be polled even once"
        );
    }

    /// **The 300-second property.** The URL and user code must reach the
    /// caller before the first poll, because the window they open is being
    /// spent by a human from the moment the server issues them.
    #[test]
    fn device_code_reaches_the_caller_before_the_first_poll() {
        let script = Script::new(vec![
            Exchange::ok(
                "/.well-known/openid-configuration",
                fixture("oidc-staging-openid-configuration.json"),
            ),
            Exchange::ok("/device/code", fixture("oidc-staging-device-code.json")),
            Exchange::ok(
                "/token",
                format!(r#"{{"id_token":"{}"}}"#, recorded_staging_token()),
            ),
        ]);
        // Record how many requests had already been made when the prompt fired.
        let mut prompted_after: Option<usize> = None;
        let mut seen_uri = String::new();
        {
            let script_ref = &script;
            let mut prompt = |c: &DeviceCode| {
                prompted_after = Some(script_ref.sent.borrow().len());
                seen_uri = c.best_uri().to_string();
            };
            device_flow(
                &script,
                "https://oauth2.sigstage.dev/auth",
                "sigstore",
                &mut prompt,
            )
            .unwrap();
        }
        assert_eq!(
            prompted_after,
            Some(2),
            "the prompt must fire straight after the device-code response — discovery and the \
             device-code request, and nothing else"
        );
        assert_eq!(
            seen_uri,
            "https://oauth2.sigstage.dev/auth/device?user_code=PZNG-PNJS"
        );
        assert!(script.is_drained());
    }

    /// The whole flow does not prompt at all if it cannot get a code — a
    /// prompt with no code in it wastes the only thing a human can act on.
    #[test]
    fn a_failed_device_code_request_prompts_nobody() {
        let script = Script::new(vec![
            Exchange::ok(
                "/.well-known/openid-configuration",
                fixture("oidc-staging-openid-configuration.json"),
            ),
            Exchange::status(
                "/device/code",
                400,
                fixture("oidc-staging-device-code-no-pkce.json"),
            ),
        ]);
        let mut prompts = 0;
        let e = device_flow(
            &script,
            "https://oauth2.sigstage.dev/auth",
            "sigstore",
            &mut |_| prompts += 1,
        )
        .unwrap_err();
        assert_eq!(prompts, 0);
        assert!(e.to_string().contains("400"), "{e}");
    }

    // --- the token -------------------------------------------------------

    /// The whole point of parsing the token: for an email issuer the proof of
    /// possession is over the **email**, not the `sub` the proto documents.
    #[test]
    fn the_proof_subject_is_the_email_for_an_email_issuer() {
        let t = IdToken::parse(&recorded_staging_token()).unwrap();
        assert_eq!(t.issuer(), "https://oauth2.sigstage.dev/auth");
        assert_eq!(t.subject(), "ChUxMDkyNzE1MjE2MjA0NjcxNzQ2NjcSBmdvb2dsZQ");
        assert_eq!(t.proof_subject().unwrap(), "signer@example.com");
        assert_ne!(t.proof_subject().unwrap(), t.subject());
    }

    /// …and the `sub` when there is no email claim, which is every CI issuer.
    #[test]
    fn the_proof_subject_is_the_sub_when_there_is_no_email_claim() {
        let raw = token_with_claims(serde_json::json!({
            "iss": "https://token.actions.githubusercontent.com",
            "sub": "repo:sigstore/fulcio:ref:refs/heads/main",
        }));
        let t = IdToken::parse(&raw).unwrap();
        assert_eq!(t.email(), None);
        assert_eq!(
            t.proof_subject().unwrap(),
            "repo:sigstore/fulcio:ref:refs/heads/main"
        );
    }

    /// An unverified email is refused locally rather than remotely. Fulcio
    /// rejects it too, with a body that says only *"There was an error
    /// processing the identity token"* — the same sentence it returns for a
    /// malformed token and for an unknown issuer.
    #[test]
    fn an_unverified_email_claim_is_refused_before_the_request_is_made() {
        for verified in [serde_json::json!(false), serde_json::Value::Null] {
            let raw = token_with_claims(serde_json::json!({
                "iss": "https://oauth2.sigstage.dev/auth",
                "sub": "abc",
                "email": "nobody@example.com",
                "email_verified": verified,
            }));
            let t = IdToken::parse(&raw).unwrap();
            let e = t.proof_subject().unwrap_err();
            assert!(e.to_string().contains("email_verified"), "{e}");
        }
    }

    /// A token with neither claim names no one, and is refused rather than
    /// signing an empty string — which Fulcio would answer with the same
    /// opaque 400 as everything else.
    #[test]
    fn a_token_naming_nobody_is_refused() {
        let raw = token_with_claims(serde_json::json!({"iss": "https://x"}));
        let t = IdToken::parse(&raw).unwrap();
        assert!(t.proof_subject().is_err());
    }

    /// A JWT is base64url-without-padding. A parser that assumes standard
    /// base64 fails on any claim set whose length is not a multiple of three,
    /// which is most of them.
    #[test]
    fn the_claim_set_is_decoded_as_unpadded_base64url() {
        // 61 bytes of JSON: not a multiple of 3, so standard base64 would need
        // padding this token does not carry.
        let raw = token_with_claims(serde_json::json!({
            "iss": "https://issuer.example/x",
            "sub": "aaaaaaaaaaaaaaaa+/",
        }));
        assert!(!raw.contains('='));
        let t = IdToken::parse(&raw).unwrap();
        assert_eq!(t.subject(), "aaaaaaaaaaaaaaaa+/");
    }

    #[test]
    fn a_token_that_is_not_a_jwt_is_refused_by_shape() {
        for bad in ["", "one.two", "not-a-jwt", "a.!!!.c"] {
            assert!(IdToken::parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    /// The raw token is passed through untouched — Fulcio verifies the
    /// signature over exactly these bytes, so any normalisation here would
    /// break it.
    #[test]
    fn the_raw_token_survives_parsing_byte_for_byte() {
        let raw = recorded_staging_token();
        assert_eq!(IdToken::parse(&raw).unwrap().raw(), raw);
        // Surrounding whitespace — a token read from a file with a trailing
        // newline is the common case — is trimmed, and nothing else is.
        assert_eq!(IdToken::parse(&format!("  {raw}\n")).unwrap().raw(), raw);
    }
}

// --- tests that genuinely need the network ---------------------------------
//
// `#[ignore]` so the default `cargo test` is hermetic. Run with
// `cargo test --features net,oidc -- --ignored`.
//
// **None of these can finish a device flow.** Completing one requires a human
// to open a browser and approve, which no test can do. What they check is the
// half of the protocol a machine can drive on its own: that the endpoints are
// where the discovery document says, that a well-formed PKCE request is
// accepted, that the documented fields come back, and that the server still
// refuses a request without PKCE. The token exchange itself is unproven here,
// and is reported as unproven rather than inferred from the parts around it.
#[cfg(test)]
mod live {
    use super::*;
    use crate::net::Http;

    const STAGING_ISSUER: &str = "https://oauth2.sigstage.dev/auth";

    #[test]
    #[ignore = "requires the network"]
    fn staging_discovery_still_matches_the_recorded_document() {
        let http = Http::new();
        let live = Discovery::fetch(&http, STAGING_ISSUER).unwrap();
        let recorded = Discovery::parse(&crate::net::testing::fixture(
            "oidc-staging-openid-configuration.json",
        ))
        .unwrap();
        assert_eq!(live.issuer, recorded.issuer);
        assert_eq!(live.token_endpoint, recorded.token_endpoint);
        assert_eq!(
            live.device_endpoint().unwrap(),
            recorded.device_endpoint().unwrap()
        );
        live.check_s256().unwrap();
    }

    /// A real device-code request against real staging. This *does* create a
    /// pending authorization — which nobody approves, so it expires in 300
    /// seconds having signed nothing and written nothing to any log.
    #[test]
    #[ignore = "requires the network"]
    fn staging_issues_a_device_code_with_the_documented_fields() {
        let http = Http::new();
        let discovery = Discovery::fetch(&http, STAGING_ISSUER).unwrap();
        let pkce = Pkce::generate().unwrap();
        let code = request_device_code(
            &http,
            discovery.device_endpoint().unwrap(),
            "sigstore",
            DEFAULT_SCOPE,
            &pkce,
        )
        .unwrap();

        assert!(!code.device_code.is_empty());
        assert!(!code.user_code.is_empty());
        assert!(code.verification_uri.starts_with("https://"));
        assert!(code
            .verification_uri_complete
            .as_deref()
            .unwrap()
            .contains(&code.user_code));
        assert_eq!(code.expires_in, 300, "the recorded 300-second window");
        assert_eq!(code.poll_interval(), Duration::from_secs(5));

        // …and the very first poll says "pending", which is the branch the
        // offline tests drive from a recorded body.
        let poll = poll_once(
            &http,
            &discovery.token_endpoint,
            "sigstore",
            &code.device_code,
            &pkce,
        )
        .unwrap();
        assert!(
            matches!(poll, Poll::Pending),
            "expected authorization_pending, got {poll:?}"
        );
    }

    /// The rule PKCE-is-mandatory rests on, re-asked of the live server.
    #[test]
    #[ignore = "requires the network"]
    fn staging_still_refuses_a_device_code_request_without_pkce() {
        let http = Http::new();
        let r = http
            .post_form(
                "https://oauth2.sigstage.dev/auth/device/code",
                &[("client_id", "sigstore"), ("scope", "openid email")],
                "probing the device endpoint without PKCE",
            )
            .unwrap();
        assert_eq!(r.status, 400);
        assert!(r.body.contains("PKCE S256 is required"), "{}", r.body);
    }
}
