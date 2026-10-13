//! T-DIST-4 — offline licence verification, and the one entitlement table.
//!
//! Sam's question is "can I still see my own work if I don't pay?", and the
//! answer this crate is built to keep true is **yes**: exactly one surface is
//! paid (approving a version), and everything else — recording, reading,
//! checking, exporting, taking an approval back — never asks about a licence.
//!
//! WHAT IS HERE. Pure functions and nothing that opens a socket:
//!
//! - the licence file format ([`FORMAT.md`](../FORMAT.md)) and its parser;
//! - [`verify`], which walks parse → key known → signature → product → plan →
//!   expiry → machine, in that order, and answers with a [`Verdict`];
//! - every non-valid outcome as ONE plain sentence saying what is wrong and ONE
//!   saying what would fix it ([`Problem::problem`], [`Problem::fix`]);
//! - the machine fingerprint ([`fingerprint`]), derived locally and never sent;
//! - the entitlement table ([`PAID`]) and [`decide`], which grant approval
//!   only from a valid paid licence.
//!
//! WHAT IS NOT HERE. Where the licence file lives, what time it is, and which
//! keys are trusted: those are the caller's, passed in, so every function here
//! is a function of its arguments. The one production public key is compiled
//! in ([`PRODUCTION_PUBLIC_KEYS`], P11: FI-34, FI-35), and the paid gate is ON
//! ([`PAID_GATE`]).

use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod fingerprint;
mod time;

pub use fingerprint::Fingerprint;

/* ── the trusted key ───────────────────────────────────────────────────── */

/// The production public keys this binary trusts, as committed constants.
///
/// **Exactly one: the founder's production key** (FI-35, D-239), key id
/// `3ef3540d43dbf105`. Its private half lives only in the founder's login
/// Keychain; nothing in this repository can sign a licence this list accepts.
/// `tests/production_key.rs` pins the count, the key id, the fingerprint D-239
/// recorded and that the test-mode Worker's key is not here.
///
/// A shipped binary never learns a new key, because this list is compiled in:
/// rotation is a new release with a second entry, and a licence made under a
/// newer key reaches an older binary as [`Problem::UnknownKey`]. With nothing
/// here — a scratch build, never a shipped one — every licence is refused with
/// [`Problem::NoTrustedKey`], whose sentence says this copy doesn't check
/// licences and that nothing in it needs one, which is true because with no key
/// the paid gate is forced off ([`PAID_GATE`]).
pub const PRODUCTION_PUBLIC_KEYS: &[[u8; 32]] = &[[
    0x74, 0x6f, 0xf9, 0x1f, 0xea, 0xde, 0xfe, 0x50, 0x07, 0x96, 0xdd, 0x62, 0x7a, 0x21, 0x1b, 0xf0,
    0x92, 0x71, 0x6d, 0x20, 0x6d, 0x29, 0xcb, 0x5e, 0xe1, 0xae, 0xaf, 0x50, 0x68, 0xd0, 0x9c, 0x24,
]];

/// The domain tag the key id is hashed under, so a key id can never be
/// mistaken for (or collide by construction with) any other hash here.
const KEY_ID_TAG: &[u8] = b"thruline-licence-key-v1\0";

/// A short, stable name for a public key: the first 8 bytes of
/// SHA-256(tag || key), as 16 lowercase hex characters.
///
/// It is what lets a licence issued under a key this binary does not know be
/// told apart from a licence whose bytes were changed: the first names a key
/// that is not in the trusted list, the second names one that is and then
/// fails the signature.
pub fn key_id(public: &[u8; 32]) -> String {
    let mut h = Sha256::new();
    h.update(KEY_ID_TAG);
    h.update(public);
    hex(&h.finalize()[..8])
}

/// One public key the caller trusts, with its id computed once.
#[derive(Clone, Debug)]
pub struct TrustedKey {
    id: String,
    key: VerifyingKey,
}

impl TrustedKey {
    /// A trusted key from its 32 public bytes. Refused when the bytes are not a
    /// valid Ed25519 point, so a typo in a constant fails loudly.
    pub fn from_bytes(public: &[u8; 32]) -> Result<Self, LicenceError> {
        let key = VerifyingKey::from_bytes(public).map_err(|_| LicenceError::BadPublicKey)?;
        Ok(Self {
            id: key_id(public),
            key,
        })
    }

    /// A trusted key from 64 hex characters.
    pub fn from_hex(text: &str) -> Result<Self, LicenceError> {
        let bytes = unhex(text.trim()).ok_or(LicenceError::BadPublicKey)?;
        let array: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| LicenceError::BadPublicKey)?;
        Self::from_bytes(&array)
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Every production key, parsed. A constant that does not parse is dropped
/// here and counted by the test that holds the list, never silently trusted.
pub fn production_keys() -> Vec<TrustedKey> {
    PRODUCTION_PUBLIC_KEYS
        .iter()
        .filter_map(|k| TrustedKey::from_bytes(k).ok())
        .collect()
}

/// The ways building a trusted key can fail. Verification itself never
/// errors: it answers with a [`Verdict`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LicenceError {
    #[error("the public key is not 32 bytes of a valid Ed25519 point")]
    BadPublicKey,
}

/* ── plans ─────────────────────────────────────────────────────────────── */

/// The plans a licence can name. **PROVISIONAL**: the founder has not ruled
/// plans or prices (MANDATE-v4 §31 names the tiers). [`PLANS`] is the one
/// place the set lives, so a change is one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    Pro,
    Team,
}

/// The plan set, as `(wire name, plan, name a person reads)`. PROVISIONAL.
///
/// Founder ruling 2 (D-298): "Pro is the only purchasable plan. Team licence
/// code may stay dormant, but Team must not be buyable or shown anywhere a
/// user sees." So Team keeps its wire name — a Team licence still parses,
/// verifies and unlocks what [`PAID`] says — and has NO name a person reads:
/// its sentence says "a ThruLine licence", never "Team" and never "Pro".
pub const PLANS: &[(&str, Plan, Option<&str>)] =
    &[("pro", Plan::Pro, Some("Pro")), ("team", Plan::Team, None)];

/// The plans anyone can buy: Pro, and nothing else (ruling 2, D-298). Every
/// list of plans a person or a client is shown is drawn from this, never from
/// [`PLANS`] or [`PAID`], which keep dormant Team.
pub const SOLD: &[Plan] = &[Plan::Pro];

impl Plan {
    pub fn parse(text: &str) -> Option<Plan> {
        PLANS
            .iter()
            .find(|(w, _, _)| *w == text)
            .map(|(_, p, _)| *p)
    }

    pub fn as_str(self) -> &'static str {
        PLANS
            .iter()
            .find(|(_, p, _)| *p == self)
            .map(|(w, _, _)| *w)
            .unwrap_or("")
    }

    /// The plan's name as a person reads it — "Pro" — or `None` for a plan
    /// that is not sold and so is never named (Team, ruling 2).
    pub fn display(self) -> Option<&'static str> {
        PLANS
            .iter()
            .find(|(_, p, _)| *p == self)
            .and_then(|(_, _, d)| *d)
    }
}

/* ── the file ──────────────────────────────────────────────────────────── */

/// The first line of every licence file.
pub const HEADER: &str = "ThruLine licence";

/// The only payload `format` this build reads.
pub const FORMAT: u64 = 1;

/// The only product a ThruLine licence may name.
pub const PRODUCT: &str = "thruline";

/// The payload, as signed. Unknown fields are ignored so a later issuer can
/// add one without breaking an older binary; a field this build needs is
/// never optional.
#[derive(Debug, Deserialize)]
struct Payload {
    format: u64,
    licence_id: String,
    product: String,
    plan: String,
    issued_at: String,
    expires_at: String,
    #[serde(default)]
    machine: Option<String>,
}

/// The envelope, before anything is trusted.
struct Envelope<'a> {
    key_id: &'a str,
    payload: &'a [u8],
    signature: [u8; 64],
}

/// Split a licence file into its three fields. The payload is returned as the
/// exact bytes stored after `payload: `, which are the bytes the signature is
/// over — nothing is re-serialised, ever.
fn parse_envelope(bytes: &[u8]) -> Option<Envelope<'_>> {
    let mut lines = bytes
        .split(|b| *b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .filter(|l| !l.is_empty() && !l.starts_with(b"#"));
    if lines.next()? != HEADER.as_bytes() {
        return None;
    }
    let (mut key, mut payload, mut signature) = (None, None, None);
    for line in lines {
        let (slot, rest): (&mut Option<&[u8]>, &[u8]) =
            if let Some(rest) = line.strip_prefix(b"key: ") {
                (&mut key, rest)
            } else if let Some(rest) = line.strip_prefix(b"payload: ") {
                (&mut payload, rest)
            } else {
                // Anything that is not one of the three fields is not a licence.
                (&mut signature, line.strip_prefix(b"signature: ")?)
            };
        if slot.is_some() {
            return None; // a field twice is not a licence
        }
        *slot = Some(rest);
    }
    let key_id = std::str::from_utf8(key?).ok()?;
    let sig_bytes = unhex(std::str::from_utf8(signature?).ok()?)?;
    let signature: [u8; 64] = sig_bytes.as_slice().try_into().ok()?;
    Some(Envelope {
        key_id,
        payload: payload?,
        signature,
    })
}

/// Write a licence file for `payload`, signed by `signing`.
///
/// The issuer's half of the format (T-DIST-6 calls it; the tests call it with
/// a key generated at run time). The payload must be one line of JSON: the
/// bytes given here are the bytes stored, and the bytes signed.
pub fn seal(payload: &[u8], signing: &SigningKey) -> Option<String> {
    if payload.contains(&b'\n') || payload.contains(&b'\r') {
        return None;
    }
    let text = std::str::from_utf8(payload).ok()?;
    let public = signing.verifying_key().to_bytes();
    let signature = signing.sign(payload).to_bytes();
    Some(format!(
        "{HEADER}\nkey: {}\npayload: {text}\nsignature: {}\n",
        key_id(&public),
        hex(&signature)
    ))
}

/* ── the verdict ───────────────────────────────────────────────────────── */

/// A licence that passed every check, with what the screen may say about it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Licence {
    pub licence_id: String,
    pub plan: Plan,
    pub issued_at: String,
    pub expires_at: String,
    /// Whether the licence is tied to one machine. The fingerprint itself is
    /// never carried out of this crate by a verdict.
    pub machine_bound: bool,
    pub key_id: String,
}

/// Why a licence cannot be used. Each has its own sentence pair, and no two
/// share one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Problem {
    /// No licence file is installed.
    Absent,
    /// A file is there and could not be read.
    Unreadable,
    /// The bytes are not a licence (or a required part is missing).
    NotALicence,
    /// A payload `format` this build does not read.
    NewerFormat,
    /// This binary trusts no key at all.
    NoTrustedKey,
    /// Names a key that is not one of the keys built into this binary. That
    /// is all the binary knows: a licence made with a newer key (after a
    /// rotation) looks exactly like this to an older binary.
    UnknownKey,
    /// Does not check out against the key it names: changed after it was
    /// made, or never made with that key. The binary cannot tell which.
    Tampered,
    /// Signed, and for a different product.
    WrongProduct,
    /// Signed, and naming a plan this build does not know.
    UnknownPlan,
    /// Signed, and past its expiry. `on` is the day, as a person reads it.
    Expired { on: String },
    /// Tied to a different machine.
    WrongMachine,
    /// Tied to one machine, and this machine cannot be identified.
    MachineUnknown,
}

impl Problem {
    /// The stable name the screen keys its copy on.
    pub fn slug(&self) -> &'static str {
        match self {
            Problem::Absent => "absent",
            Problem::Unreadable => "unreadable",
            Problem::NotALicence => "not_a_licence",
            Problem::NewerFormat => "newer_format",
            Problem::NoTrustedKey => "no_key",
            Problem::UnknownKey => "unknown_key",
            Problem::Tampered => "tampered",
            Problem::WrongProduct => "wrong_product",
            Problem::UnknownPlan => "unknown_plan",
            Problem::Expired { .. } => "expired",
            Problem::WrongMachine => "wrong_machine",
            Problem::MachineUnknown => "machine_unknown",
        }
    }

    /// Every slug, in one list, so a test can hold the screen's copy to it.
    pub const SLUGS: &'static [&'static str] = &[
        "absent",
        "unreadable",
        "not_a_licence",
        "newer_format",
        "no_key",
        "unknown_key",
        "tampered",
        "wrong_product",
        "unknown_plan",
        "expired",
        "wrong_machine",
        "machine_unknown",
    ];

    /// One plain sentence: what is wrong. Never accuses anybody; the file is
    /// the subject, never the person holding it.
    pub fn problem(&self) -> String {
        match self {
            Problem::Absent => "No licence is installed on this machine.".into(),
            Problem::Unreadable => "There is a licence file, but ThruLine can't open it.".into(),
            Problem::NotALicence => {
                "This file isn't a ThruLine licence, or part of it is missing.".into()
            }
            Problem::NewerFormat => "This licence was made for a newer version of ThruLine.".into(),
            // ADV-dist-copy-3: a copy built without a licence key. It was the
            // build that shipped until P11 (FI-34); the shipped build carries
            // the production key now, so this is a scratch build only. It
            // trusts no key, so it checks no licence; and with no key its
            // paid gate is forced off
            // ([`PAID_GATE`]), so nothing in it needs one. Nothing is wrong, and
            // the sentence says only that.
            Problem::NoTrustedKey => {
                "This copy of ThruLine doesn't check licences, and nothing in it needs one.".into()
            }
            // ADV-dist-copy-4: each says what the binary knows and no more. It
            // knows the key the file names is not one of its own — never who
            // made the file — and it knows the file does not check out against
            // the key it names, which a change and a different maker both do.
            Problem::UnknownKey => {
                "This licence names a maker this version of ThruLine doesn't know, so it can't check it."
                    .into()
            }
            Problem::Tampered => {
                "This licence has been changed, or wasn't made by the maker it names, so it can't be trusted."
                    .into()
            }
            Problem::WrongProduct => {
                "This licence is for a different product, not ThruLine.".into()
            }
            Problem::UnknownPlan => {
                "This licence names a plan this version of ThruLine doesn't know.".into()
            }
            Problem::Expired { on } => format!("This licence expired on {on}."),
            Problem::WrongMachine => "This licence is for a different machine.".into(),
            Problem::MachineUnknown => {
                "This licence is for one machine, and ThruLine can't tell which machine this is."
                    .into()
            }
        }
    }

    /// One plain sentence: what would fix it.
    pub fn fix(&self) -> String {
        match self {
            Problem::Absent => "Get Pro at https://thrulinehq.com/pricing, then install your licence with: thruline licence install <file>".into(),
            Problem::Unreadable => {
                "Check the file can be read by your user, or install the licence again.".into()
            }
            // No email carries the licence (R-2026-09-26-1: under Stripe
            // Managed Payments the receipt is Link's, and it holds no file);
            // the buyer downloads it from the licence page with their key.
            Problem::NotALicence | Problem::Tampered => {
                "Download it again from the licence page on ThruLine's website, with your licence key."
                    .into()
            }
            // A licence made with a newer key needs a newer ThruLine first.
            Problem::UnknownKey => {
                "Update ThruLine, then install the licence again; if it's still refused, download it again from the licence page on ThruLine's website, with your licence key."
                    .into()
            }
            Problem::NewerFormat | Problem::UnknownPlan => {
                "Update ThruLine, then install the licence again.".into()
            }
            Problem::NoTrustedKey => "You don't need to do anything.".into(),
            Problem::WrongProduct => {
                "Check you picked the right file, or download it again from the licence page on ThruLine's website, with your licence key."
                    .into()
            }
            // The renewal path must work for both people it reaches: one whose subscription
            // lapsed (renew, then install the new licence) and one whose card
            // renews on its own (nothing to renew; download this period's
            // licence and install it). The sentence before this one omitted
            // renewal (D-050 condition 1).
            Problem::Expired { .. } => "Go to the licence page on ThruLine's website with your licence key, renew there if your subscription has ended, then download the new licence and install it.".into(),
            Problem::WrongMachine => {
                "Install the licence issued for this machine, or ask for one issued for it.".into()
            }
            Problem::MachineUnknown => "Ask for a licence that isn't tied to one machine.".into(),
        }
    }
}

/// What [`verify`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Valid(Licence),
    Refused(Problem),
}

impl Verdict {
    /// The one sentence a status line says.
    pub fn sentence(&self) -> String {
        match self {
            Verdict::Valid(l) => {
                let until = time::day(&l.expires_at).unwrap_or_else(|| l.expires_at.clone());
                match l.plan.display() {
                    Some(plan) => {
                        format!("This machine has a ThruLine {plan} licence, valid until {until}.")
                    }
                    // A dormant plan (ruling 2): named by no plan at all.
                    None => format!("This machine has a ThruLine licence, valid until {until}."),
                }
            }
            Verdict::Refused(p) => p.problem(),
        }
    }

    pub fn slug(&self) -> &'static str {
        match self {
            Verdict::Valid(_) => "valid",
            Verdict::Refused(p) => p.slug(),
        }
    }
}

/// Verify one licence's bytes. Pure: a function of its arguments and nothing
/// else — no clock, no file, no environment, no socket.
///
/// The order is the card's and it is load-bearing, because it decides which
/// sentence a person reads when more than one thing is wrong: parse → key
/// known → signature → product → plan known → not expired at `now` → machine.
///
/// `now` is seconds since the Unix epoch.
pub fn verify(bytes: &[u8], trusted: &[TrustedKey], now: i64, local: &Fingerprint) -> Verdict {
    use Verdict::Refused;
    let Some(envelope) = parse_envelope(bytes) else {
        return Refused(Problem::NotALicence);
    };
    if trusted.is_empty() {
        return Refused(Problem::NoTrustedKey);
    }
    let Some(key) = trusted.iter().find(|k| k.id == envelope.key_id) else {
        return Refused(Problem::UnknownKey);
    };
    let signature = Signature::from_bytes(&envelope.signature);
    if key.key.verify(envelope.payload, &signature).is_err() {
        return Refused(Problem::Tampered);
    }
    // Only now is anything inside the payload believed.
    let Ok(payload) = serde_json::from_slice::<Payload>(envelope.payload) else {
        return Refused(Problem::NotALicence);
    };
    if payload.format != FORMAT {
        return Refused(if payload.format > FORMAT {
            Problem::NewerFormat
        } else {
            Problem::NotALicence
        });
    }
    if ulid::Ulid::from_string(&payload.licence_id).is_err() {
        return Refused(Problem::NotALicence);
    }
    if payload.product != PRODUCT {
        return Refused(Problem::WrongProduct);
    }
    let Some(plan) = Plan::parse(&payload.plan) else {
        return Refused(Problem::UnknownPlan);
    };
    let (Some(_issued), Some(expires)) = (
        time::unix(&payload.issued_at),
        time::unix(&payload.expires_at),
    ) else {
        return Refused(Problem::NotALicence);
    };
    if now >= expires {
        return Refused(Problem::Expired {
            on: time::day(&payload.expires_at).unwrap_or_else(|| payload.expires_at.clone()),
        });
    }
    if let Some(machine) = &payload.machine {
        match local {
            Fingerprint::Unavailable => return Refused(Problem::MachineUnknown),
            Fingerprint::Available(mine) if mine != machine => {
                return Refused(Problem::WrongMachine)
            }
            Fingerprint::Available(_) => {}
        }
    }
    Verdict::Valid(Licence {
        licence_id: payload.licence_id,
        plan,
        issued_at: payload.issued_at,
        expires_at: payload.expires_at,
        machine_bound: payload.machine.is_some(),
        key_id: key.id.clone(),
    })
}

/// Read the licence at `path` and verify it. The two outcomes [`verify`] cannot
/// see — no file, and a file that cannot be opened — are their own problems.
///
/// Reading a file is the only I/O here; no socket is opened.
pub fn check_file(path: &Path, trusted: &[TrustedKey], now: i64, local: &Fingerprint) -> Verdict {
    if trusted.is_empty() {
        return Verdict::Refused(Problem::NoTrustedKey);
    }
    match std::fs::read(path) {
        Ok(bytes) => verify(&bytes, trusted, now, local),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Verdict::Refused(Problem::Absent),
        Err(_) => Verdict::Refused(Problem::Unreadable),
    }
}

/* ── entitlement ───────────────────────────────────────────────────────── */

/// A surface that needs a plan. **Only paid surfaces are variants**: a surface
/// that is not here cannot be refused, because there is nothing to ask about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// `POST /api/changes/<id>/decisions`, and any terminal door that calls the
    /// same function.
    ApproveVersion,
}

/// THE TABLE. Surface → the plans that unlock it. **PROVISIONAL (decision D-3
/// in HANDOFF.md)**: exactly one paid surface, approving a version.
///
/// Never here, and held by an enumerating test in the CLI: recording, every
/// read, withdrawing an approval, stopping a run, adding or removing a link,
/// exporting one's own records, removing stored data, and the licence
/// surfaces themselves.
///
/// Team stays here (ruling 2, D-298): it is not sold, and a dormant Team
/// licence that already exists keeps working. What a person is told unlocks
/// approving is [`SOLD`], not this table.
pub const PAID: &[(Surface, &[Plan])] = &[(Surface::ApproveVersion, &[Plan::Pro, Plan::Team])];

/// THE PAID-GATE FLAG (R-2026-09-24-25). **ON**, by the founder's P11 ruling
/// (FI-34, D-238): "don't ship v0.1.0 with the paid gate disabled". While it is
/// on, [`PAID`] refuses its surface unless the installed licence grants a plan
/// it accepts.
///
/// Off is still forced, whatever this says, in any build without a licence key
/// (a scratch build with [`PRODUCTION_PUBLIC_KEYS`] empty; the shipped build
/// carries one): such a build could never install a licence, so a gate in it
/// would lock approving for good (report §08, "Built without a licence key: the
/// gate must be OFF in any such build"). The CLI holds both halves at its one
/// entitlement call, `licence_cmd::entitled`.
pub const PAID_GATE: bool = true;

impl Surface {
    pub fn slug(self) -> &'static str {
        match self {
            Surface::ApproveVersion => "approve_version",
        }
    }

    /// What this surface is, in a sentence that says it needs a licence.
    pub fn needs(self) -> &'static str {
        match self {
            Surface::ApproveVersion => "Approving a version needs a Pro licence.",
        }
    }

    /// The plans that unlock it; empty for a surface not in [`PAID`].
    pub fn plans(self) -> &'static [Plan] {
        PAID.iter()
            .find(|(s, _)| *s == self)
            .map(|(_, p)| *p)
            .unwrap_or(&[])
    }
}

/// One place an entitlement can come from: the installed licence.
#[derive(Clone, Debug)]
pub enum Source {
    /// The installed licence's verdict, and whether a licence file is there
    /// at all. A file that is there speaks for itself, even when it cannot be used.
    Licence { verdict: Verdict, installed: bool },
}

impl Source {
    fn grants(&self) -> Option<Plan> {
        match self {
            Source::Licence {
                verdict: Verdict::Valid(l),
                ..
            } => Some(l.plan),
            Source::Licence { .. } => None,
        }
    }

    /// Whether this source is the one a person should hear about first: a
    /// licence that works, or a licence file that is there.
    fn speaks(&self) -> bool {
        match self {
            Source::Licence { verdict, installed } => {
                *installed || matches!(verdict, Verdict::Valid(_))
            }
        }
    }

    /// Why this source does not open a paid surface: (what is wrong, what
    /// would fix it, the slug). `None` when it does, or when nothing is wrong.
    fn refusal(&self) -> Option<(String, String, String)> {
        match self {
            Source::Licence {
                verdict: Verdict::Refused(p),
                ..
            } => Some((p.problem(), p.fix(), p.slug().to_string())),
            Source::Licence { .. } => None,
        }
    }
}

/// The source a status line speaks for: the first that [`Source::speaks`], in
/// the list's order. `None` only for an empty list.
pub fn speaking(sources: &[Source]) -> Option<&Source> {
    sources
        .iter()
        .find(|s| s.speaks())
        .or_else(|| sources.first())
}

/// Whether a surface is open, and if not, both halves of why (rule 6).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "entitled", rename_all = "snake_case")]
pub enum Entitlement {
    Allowed,
    Refused {
        /// Why it is off: what the surface needs, then what is wrong.
        reason: String,
        /// What would turn it on.
        enable: String,
        /// The problem's slug, for the screen's copy.
        status: String,
    },
}

/// Decide one surface against its installed licence. Allowed when it grants a
/// plan the surface accepts; refused otherwise, with the licence's own problem
/// and a path to fix it.
pub fn decide(surface: Surface, sources: &[Source]) -> Entitlement {
    let plans = surface.plans();
    if plans.is_empty() {
        return Entitlement::Allowed;
    }
    if sources
        .iter()
        .filter_map(Source::grants)
        .any(|p| plans.contains(&p))
    {
        return Entitlement::Allowed;
    }
    let (problem, enable, status) = speaking(sources)
        .and_then(Source::refusal)
        .or_else(|| sources.iter().find_map(Source::refusal))
        .unwrap_or_else(|| {
            let p = Problem::Absent;
            (p.problem(), p.fix(), p.slug().to_string())
        });
    Entitlement::Refused {
        reason: format!("{} {problem}", surface.needs()),
        enable,
        status,
    }
}

/* ── small helpers ─────────────────────────────────────────────────────── */

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    text.as_bytes()
        .chunks(2)
        .map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

/// Lowercase hex, for callers that print a key or a fingerprint.
pub fn to_hex(bytes: &[u8]) -> String {
    hex(bytes)
}

/// A day, as a person reads it ("23 Sep 2027"), from a stored timestamp.
pub fn day(timestamp: &str) -> Option<String> {
    time::day(timestamp)
}

/// Seconds since the Unix epoch in the one timestamp shape a licence carries
/// (`YYYY-MM-DDTHH:MM:SSZ`, UTC). The issuer (T-DIST-6) writes `issued_at` and
/// `expires_at` with this, so the writer and the reader share one definition.
pub fn timestamp(unix: i64) -> String {
    time::timestamp(unix)
}
