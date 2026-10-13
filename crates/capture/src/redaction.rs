//! BUILD-SPEC §4.6 — redaction, applied **before hashing and before bounding**.
//!
//! > *Before any event is hashed or stored: strip common secret formats (AWS
//! > keys, GitHub tokens, JWTs, `Authorization:` headers, `.env` values, private
//! > key blocks). Use a maintained pattern list plus entropy check; make it
//! > extensible via `.thruline/redact.yml`. Store a `redactions_applied` count
//! > per event.*
//!
//! # This wraps `thruline_core::redact`; it does not reimplement it
//!
//! That module is 2,892 lines carrying seventeen rounds of measured leak
//! repairs (`docs/P0-BUILD-BRIEF.md` A209 … A209-C15). §4.6's list is a
//! *subset* of what it already does, and every hard case in it was found by a
//! leak rather than by reading: terminated versus truncated PEM blocks, per-line
//! prefixes, wrap width, 64-character Ed25519 bodies, transport decoration,
//! mentioned-versus-standalone END banners in both letter cases.
//!
//! This layer adds only what the core masker does not structurally own, and is
//! held to the same 1,958-row corpus — see
//! `crates/capture/tests/redaction_corpus_gate.rs`, which runs the identical
//! gate through this layer so the additions below cannot regress it.
//!
//! # The two additions, and why both are structural
//!
//! §4.6 names `Authorization:` headers and `.env` values. Neither is a
//! *shape* — a bearer token can be any bytes at all — so neither can be found
//! by a value heuristic. Both are found by their **context**: the header name
//! and the assignment key say "what follows is a credential" regardless of what
//! follows. That is the same reasoning that makes a terminated PEM block
//! self-identifying, and it is the only kind of rule that catches a credential
//! whose format nobody has seen.
//!
//! The A209-C15 rule applies to both: **a predicate must not serve two callers
//! that disagree about which direction is safe when it is wrong.** These
//! predicates serve exactly one caller — the redactor — where a false negative
//! leaks and a false positive destroys context. They are not reused by any gate.

use std::collections::BTreeSet;
use std::path::Path;

use thruline_core::redact;

pub mod config;
pub use config::{CustomRule, RedactConfig, RedactConfigError};

/// Text that has been through the redactor, carrying what the redactor did.
///
/// Constructible only by [`Redactor::redact`]. The point is ordering: §4.6
/// requires redaction **before** hashing, and A209-C3 measured what happens
/// when bounding comes first — a 143-character command with a token past
/// character 105 reached the sealed capsule bytes because the cut removed
/// enough body for the detector to stop recognising the shape. Four separate
/// sites had that bug.
///
/// So the bound is a method on this type and there is no other way to get one.
/// You cannot bound text that has not been redacted, because [`Bounded`] can
/// only be produced from here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redacted {
    text: String,
    applied: u64,
    families: BTreeSet<String>,
    complete: bool,
}

impl Redacted {
    pub fn text(&self) -> &str {
        &self.text
    }

    /// How many spans were masked. This is §4.6's `redactions_applied`.
    pub fn applied(&self) -> u64 {
        self.applied
    }

    /// Which detector families fired, for the receipt's capture block.
    pub fn families(&self) -> &BTreeSet<String> {
        &self.families
    }

    /// False when some upstream masking pass cannot be accounted for. Mirrors
    /// `redact::MaskRecord::complete`, which exists because a producer that
    /// cannot say what it masked must not be reported as having masked nothing.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Bound to `max` characters **after** redaction, without ever cutting
    /// through a `[masked: …]` marker.
    ///
    /// The marker repair is `redact::truncate_respecting_markers`: A209-C4/B2
    /// measured a cut landing inside a marker persisting `[masked: aw`, which
    /// under the then-current text-parsed treatment also *opened* the consent
    /// gate. The record no longer lives in the string, but a half-marker in a
    /// rendered artifact is still a lie about the treatment.
    pub fn bounded(&self, max: usize) -> Bounded {
        // Masks, THEN cuts — locally, in this function.
        //
        // The ordering is already guaranteed by the type: `Bounded` is
        // constructible only from `Redacted`, so unredacted text cannot reach
        // this cut. But `redact::TRUNCATION_SITES`' verification checks the
        // invariant by PROXIMITY inside one function, and a guarantee that
        // spans two functions is invisible to it. Rather than argue the census
        // out of its check, the pass is repeated here so the invariant is true
        // locally as well as structurally.
        //
        // Idempotent, and cheap: the text has already been through the masker,
        // so this finds nothing — `[masked: …]` markers carry no detector
        // prefix and are far below the entropy floor. The count is deliberately
        // NOT incremented from this pass; `applied` describes the redaction,
        // not the rendering, and double-counting would make it a lie.
        let (remasked, _record) = redact::mask_recorded(&self.text);
        debug_assert_eq!(
            remasked, self.text,
            "re-masking already-redacted text changed it — the first pass was              not a fixpoint, which means a marker is being read as a secret"
        );
        Bounded {
            text: redact::truncate_respecting_markers(&remasked, max),
            applied: self.applied,
        }
    }

    pub fn into_text(self) -> String {
        self.text
    }
}

/// Redacted text that has additionally been cut to a bound.
///
/// Produced only by [`Redacted::bounded`], so the ordering is a type
/// consequence rather than a convention someone has to remember.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bounded {
    text: String,
    applied: u64,
}

impl Bounded {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn applied(&self) -> u64 {
        self.applied
    }
    pub fn into_text(self) -> String {
        self.text
    }
}

/// The §4.6 redactor.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    config: RedactConfig,
}

impl Redactor {
    /// A redactor with no repository configuration.
    pub fn new() -> Redactor {
        Redactor::default()
    }

    /// Load `.thruline/redact.yml` from `repo_root`, if present.
    ///
    /// A malformed config is an **error**, never a silent fall back to the
    /// defaults: the whole reason a user writes this file is that the built-in
    /// list does not cover something of theirs, and quietly ignoring it would
    /// leak exactly what they asked to be masked.
    pub fn from_repo(repo_root: &Path) -> Result<Redactor, RedactConfigError> {
        Ok(Redactor {
            config: RedactConfig::load(repo_root)?,
        })
    }

    /// Redact a JSON document, honouring **structural** key/value relationships.
    ///
    /// The text rules see `GITHUB_TOKEN=abc` because the key, the delimiter and
    /// the value are adjacent characters. In JSON they are not:
    /// `{"env": {"GITHUB_TOKEN": "abc"}}` relates them through the object, and a
    /// string redacted in isolation carries no context at all — so a credential
    /// matching no known shape survived, measured on the hook path.
    ///
    /// This is S1's generator one level further up, and the same answer: **find
    /// the name, then follow the structure to the value**, rather than hoping
    /// the two are adjacent. A key that would make a `.env` assignment a secret,
    /// or that is a credential header name, makes its string value a secret here
    /// too — with no payload heuristic, because the key is what identifies it.
    pub fn redact_json(
        &self,
        value: serde_json::Value,
    ) -> (serde_json::Value, u64, BTreeSet<String>) {
        self.redact_json_bounded(value, usize::MAX)
    }

    /// [`Redactor::redact_json`], bounding every string to `max` characters
    /// **after** masking.
    ///
    /// The bound goes through [`Redacted::bounded`] rather than a separate cut
    /// in the caller. The caller's version was a second truncation site, which
    /// the A209 census correctly refused: it verifies mask-then-cut by
    /// proximity within one function, and a cut whose mask happened two frames
    /// up is indistinguishable to it from a cut with no mask at all. Routing
    /// through the one enumerated seam keeps the invariant locally true
    /// everywhere instead of arguing about which sites deserve an exemption.
    pub fn redact_json_bounded(
        &self,
        value: serde_json::Value,
        max: usize,
    ) -> (serde_json::Value, u64, BTreeSet<String>) {
        let mut applied = 0u64;
        let mut families = BTreeSet::new();
        let out = self.walk_json(value, None, false, max, &mut applied, &mut families);
        (out, applied, families)
    }

    /// `credential_ancestor` carries "some key above me named a credential"
    /// down the subtree.
    ///
    /// Without it the context was lost at exactly one level of nesting:
    /// `{"token": X}` masked while `{"token": {"value": X}}` did not — and the
    /// second is the shape that matters, because a Kubernetes `Secret` and a
    /// Vault response are how a credential usually appears in a transcript at
    /// all. The property generator still applies to the leaf key, so
    /// `{"token": {"path": …}}` stays a path.
    fn walk_json(
        &self,
        value: serde_json::Value,
        parent_key: Option<&str>,
        credential_ancestor: bool,
        max: usize,
        applied: &mut u64,
        families: &mut BTreeSet<String>,
    ) -> serde_json::Value {
        use serde_json::Value as V;
        match value {
            V::String(s) => {
                if let Some(key) = parent_key {
                    // A length floor, as the text rule has. A JSON key named
                    // `key` is common and often holds something short and
                    // harmless — a keypress tool's `{"key": "Enter"}` — and
                    // destroying that is D1 context loss for no safety gain.
                    // An ancestor's naming governs the subtree, and the
                    // property exemption does NOT survive it.
                    //
                    // The two directions were measured against each other.
                    // Keeping the exemption left `{"secret":{"path":X}}`
                    // leaking, which the verifier reported. Dropping it means a
                    // path nested INSIDE a credential-named object is masked —
                    // `{"credentials":{"path":"/home/u/.aws/credentials"}}`
                    // loses its path. That is real context loss and it is
                    // accepted deliberately: the A209-C path regression was
                    // about FLAT env-style keys and paths in prose, both of
                    // which still hold (`TLS_CERT_PATH=/etc/ssl/x.pem` is
                    // untouched), while a key nested inside a secret blob is a
                    // far narrower context in which to lose a path than to leak
                    // a credential.
                    if (key_implies_credential(key) || credential_ancestor)
                        && s.chars().count() >= ENV_VALUE_MIN
                        && !is_variable_reference(&s)
                        && !is_already_masked(&s)
                    {
                        *applied += 1;
                        families.insert("structured_credential".to_string());
                        return V::String(marker("structured_credential"));
                    }
                }
                let redacted = self.redact(&s);
                *applied += redacted.applied();
                families.extend(redacted.families().iter().cloned());
                V::String(redacted.bounded(max).into_text())
            }
            V::Array(items) => V::Array(
                items
                    .into_iter()
                    // An array inherits its parent key: `{"tokens": ["a","b"]}`
                    // names both elements.
                    .map(|v| {
                        self.walk_json(v, parent_key, credential_ancestor, max, applied, families)
                    })
                    .collect(),
            ),
            V::Object(map) => V::Object(
                map.into_iter()
                    .map(|(k, v)| {
                        let inherited = credential_ancestor || key_implies_credential(&k);
                        let redacted_value =
                            self.walk_json(v, Some(&k), inherited, max, applied, families);
                        (k, redacted_value)
                    })
                    .collect(),
            ),
            other => other,
        }
    }

    /// Redact one string.
    ///
    /// Order matters and is fixed:
    ///
    /// 1. **Context rules first** — `Authorization:` headers and `.env`
    ///    assignments. They mask by position, so they must run while the
    ///    surrounding text is still intact; the token walk would otherwise have
    ///    already consumed the value.
    /// 2. **User rules** from `.thruline/redact.yml`.
    /// 3. **The core masker** — armoured blocks, the detector table, secret
    ///    flags, entropy. Seventeen rounds of measured behaviour, unmodified.
    pub fn redact(&self, text: &str) -> Redacted {
        let mut applied = 0u64;
        let mut families = BTreeSet::new();

        let staged = mask_authorization_headers(text, &mut applied, &mut families);
        let staged = mask_env_assignments(&staged, &mut applied, &mut families);
        let staged = self.config.apply(&staged, &mut applied, &mut families);

        let (masked, record) = redact::mask_recorded(&staged);
        applied += record.masked;
        families.extend(record.families.iter().cloned());

        Redacted {
            text: masked,
            applied,
            families,
            complete: record.complete,
        }
    }
}

/// The mask marker this layer emits, matching the core masker's spelling so a
/// reader cannot tell which layer fired — and so the core masker's own
/// marker-aware truncation repair covers ours too.
fn marker(family: &str) -> String {
    format!("[masked: {family}]")
}

/// Header names whose value is a credential by construction.
const CREDENTIAL_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "x-auth-token",
    "x-amz-security-token",
    "cookie",
    "set-cookie",
];

/// Auth schemes that precede the credential rather than being it.
///
/// Kept visible: `Bearer` and `Basic` carry no secret and knowing which scheme
/// was used is exactly the context D1 exists to preserve.
const AUTH_SCHEMES: &[&str] = &["bearer", "basic", "digest", "token", "negotiate", "ntlm"];

/// Minimum credential length before the header rule fires.
///
/// `Authorization: required` appears in prose and in API docs. A short word
/// after the colon is far more likely to be that than a credential, and masking
/// it is D1 context destruction for no safety gain — an 8-character secret is
/// already recoverable by other means. Measured against the corpus's `nomask`
/// rows, which are the over-mask controls.
const HEADER_VALUE_MIN: usize = 12;

/// A value that is a reference to a variable, not the value itself.
///
/// `$GITHUB_TOKEN`, `${GITHUB_TOKEN}`, `%GITHUB_TOKEN%`, `$(cat secret)`. These
/// are structurally identifiable and masking them destroys the one piece of
/// information a reader needs — *which* variable was used — while protecting
/// nothing, because the reference is not the secret.
fn is_variable_reference(value: &str) -> bool {
    let v = value.trim();
    v.starts_with('$')
        || (v.starts_with('%') && v.ends_with('%') && v.len() > 2)
        || v.starts_with("{{")
}

/// A value this layer has already masked.
///
/// Without this the layer is not a fixpoint: `GITHUB_TOKEN=[masked: env_value]`
/// re-masks to `[masked: env_value] env_value]` on a second pass — measured on
/// 44 of 1,958 corpus rows.
///
/// The doc comment on `Redacted::bounded` justified its own re-mask on
/// "markers carry no detector prefix", which is true of the CORE masker and
/// says nothing about these rules: they are context-keyed and need no prefix at
/// all. A safety argument that does not cover the code it is attached to is the
/// A209-C15 shape, and this is the same one caught early.
fn is_already_masked(value: &str) -> bool {
    // No trailing space in the pattern. The marker is `[masked: name]`, and
    // unquoted value extraction stops at the first whitespace — which lands
    // INSIDE the marker, yielding `[masked:`. Requiring `"[masked: "` therefore
    // never matched, and the layer kept re-masking its own output:
    // `TOKEN=[masked: env_value]` → `TOKEN=[masked: env_value] env_value]`.
    // Checked against the un-truncated body at each call site for the same
    // reason.
    value.trim_start().starts_with("[masked:")
}

/// Mask the credential in `Authorization:`-style headers, structurally.
///
/// # The delimiter is a FAMILY, not a character
///
/// Two rounds of findings, both the same generator at increasing distance.
/// First: the name and its colon need not be adjacent (JSON, dicts, HAR,
/// obs-fold). Then: the delimiter need not be a colon at all — TOML uses `=`,
/// XML and SOAP use `>`, an array-of-pairs uses position, and escaped JSON puts
/// backslashes between every token.
///
/// So this does not match delimiters; it walks *past* whatever separates a name
/// from its value, by family:
///
/// | family | shapes |
/// |---|---|
/// | assignment | `:` `=` `=>` |
/// | markup | `<Authorization>` … `</…>` |
/// | positional | `["Authorization", "Bearer …"]` |
/// | block scalar | YAML `\|` and `>` continuations |
///
/// Quote styles are likewise a set — `"`, `'`, `` ` `` — because a JS developer
/// writes a template literal and the first version of this rule's own fixture
/// used the double-quoted variant instead, which masked while the real shape
/// leaked.
///
/// # Linear, because the input size is agent-controlled
///
/// The first version re-scanned the remaining text for all seven header names
/// once per header found: O(n²). Measured — 4,000 headers (219 KB) took 338 ms
/// and 5,000 took 520 ms, 10.4x the 50 ms hook budget. An agent chooses how big
/// a `tool_input` is, so that is not a performance note, it is a way to blow the
/// budget from inside a session. This walks the text once.
fn mask_authorization_headers(
    text: &str,
    applied: &mut u64,
    families: &mut BTreeSet<String>,
) -> String {
    let spans = credential_spans(text);
    if spans.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for (from, to) in spans {
        if from < last {
            continue;
        }
        out.push_str(&text[last..from]);
        out.push_str(&marker("authorization_header"));
        *applied += 1;
        families.insert("authorization_header".to_string());
        last = to;
    }
    out.push_str(&text[last..]);
    out
}

/// Every credential span in `text`, found in ONE linear pass.
fn credential_spans(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    let mut skip_until = 0usize;

    while i < bytes.len() {
        if i < skip_until || !text.is_char_boundary(i) {
            i += 1;
            continue;
        }
        // A header name must start at a token boundary.
        if i > 0 && is_name_char(bytes[i - 1]) {
            i += 1;
            continue;
        }
        let Some(name_len) = header_name_at(bytes, i) else {
            i += 1;
            continue;
        };
        if let Some(span) = credential_span(text, i, i + name_len) {
            skip_until = span.1;
            spans.push(span);
        }
        i += name_len;
    }
    spans
}

/// The length of a credential header name starting at `at`, if any.
fn header_name_at(bytes: &[u8], at: usize) -> Option<usize> {
    for name in CREDENTIAL_HEADERS {
        let n = name.len();
        let Some(window) = bytes.get(at..at + n) else {
            continue;
        };
        if !window
            .iter()
            .zip(name.bytes())
            .all(|(b, c)| b.to_ascii_lowercase() == c)
        {
            continue;
        }
        if bytes.get(at + n).is_some_and(|b| is_name_char(*b)) {
            continue;
        }
        return Some(n);
    }
    None
}

fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// Quote characters a value may be wrapped in.
///
/// Backtick included because a JS template literal is how a developer actually
/// writes `headers: { Authorization: ` + "`Bearer ${t}`" + ` }`, and it was the
/// one near-miss sibling still leaking after the others were closed.
const QUOTES: &[u8] = b"\"'`";

/// From a located header name, the byte span of the credential.
fn credential_span(text: &str, name_start: usize, after_name: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();

    // MARKUP: `<Authorization>Bearer …</Authorization>`. Recognised by the `<`
    // immediately before the name, so an attribute value is not mistaken for a
    // tag body.
    if name_start > 0 && bytes[name_start - 1] == b'<' {
        let close = text[after_name..].find('>')? + after_name + 1;
        let end = text[close..]
            .find('<')
            .map(|i| i + close)
            .unwrap_or(text.len());
        return bound_credential(text, close, end);
    }

    // HAR: the name is itself the value of a `name` key.
    if let Some(value_key) = har_value_key(text, after_name) {
        let start = skip_value_whitespace(bytes, value_key)?;
        return read_value(text, start);
    }

    let mut i = skip_separators(bytes, after_name);

    // POSITIONAL: `["Authorization", "Bearer …"]` — the pair is by order, and
    // the only delimiter is the comma.
    if bytes.get(i) == Some(&b',') {
        let start = skip_value_whitespace(bytes, i + 1)?;
        return read_value(text, start);
    }

    // ASSIGNMENT: `:`, `=`, `=>`.
    if bytes.get(i) == Some(&b'=') && bytes.get(i + 1) == Some(&b'>') {
        i += 2;
    } else if matches!(bytes.get(i), Some(b':') | Some(b'=')) {
        i += 1;
    } else {
        return None;
    }

    // YAML block scalar: `Authorization: |` or `: >`, value on the indented
    // lines below.
    let after_delim = skip_spaces(bytes, i);
    if matches!(bytes.get(after_delim), Some(b'|') | Some(b'>')) {
        let mut j = after_delim + 1;
        j = skip_spaces(bytes, j);
        if matches!(bytes.get(j), Some(b'\n') | Some(b'\r')) {
            let start = skip_value_whitespace(bytes, j)?;
            let end = text[start..]
                .find('\n')
                .map(|k| k + start)
                .unwrap_or(text.len());
            return bound_credential(text, start, end);
        }
    }

    let start = skip_value_whitespace(bytes, i)?;
    read_value(text, start)
}

/// Whitespace, closing quotes and escape backslashes between a name and its
/// delimiter.
///
/// Escaped JSON (`{\"Authorization\": \"Bearer …\"}`) puts a backslash before
/// every quote, and a walk that stopped at the first unexpected byte read it as
/// "no delimiter here" and moved on.
fn skip_separators(bytes: &[u8], mut i: usize) -> usize {
    loop {
        let before = i;
        while matches!(bytes.get(i), Some(b'\\')) {
            i += 1;
        }
        if bytes.get(i).is_some_and(|b| QUOTES.contains(b)) {
            i += 1;
        }
        i = skip_spaces(bytes, i);
        if i == before {
            return i;
        }
    }
}

/// Read a value at `start`, quoted or bare, and bound it to the credential.
fn read_value(text: &str, start: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut i = start;
    while matches!(bytes.get(i), Some(b'\\')) {
        i += 1;
    }
    if let Some(&q) = bytes.get(i).filter(|b| QUOTES.contains(b)) {
        i += 1;
        let mut j = i;
        while j < bytes.len() {
            if bytes[j] == b'\\' {
                j += 2;
                continue;
            }
            if bytes[j] == q {
                break;
            }
            j += 1;
        }
        return bound_credential(text, i, j.min(bytes.len()));
    }
    // A BARE value ends at a shell metacharacter as well as at a line break or
    // quote: `Authorization: Bearer X | tee log` is a credential followed by a
    // pipeline, and reading to end of line swallowed the tail, made the span
    // contain whitespace, and so read as prose and masked nothing. The tail is
    // context and must survive; the credential must not.
    let mut j = i;
    while j < bytes.len()
        && !matches!(bytes[j], b'\r' | b'\n' | b'<' | b'|' | b';' | b'&')
        && !QUOTES.contains(&bytes[j])
    {
        j += 1;
    }
    while j > i
        && matches!(
            bytes[j - 1],
            b',' | b'}' | b']' | b';' | b' ' | b'\t' | b'\\'
        )
    {
        j -= 1;
    }
    bound_credential(text, i, j)
}

/// Apply the scheme, reference, marker and prose rules to a raw value span.
fn bound_credential(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if end <= start {
        return None;
    }
    let value = text.get(start..end)?.trim_end();
    if value.is_empty() || is_variable_reference(value) || is_already_masked(value) {
        return None;
    }
    let mut words = value.splitn(2, char::is_whitespace);
    let first = words.next().unwrap_or_default();
    let credential_start = if AUTH_SCHEMES.contains(&first.to_ascii_lowercase().as_str()) {
        let rest = words.next()?;
        start + value.len() - rest.len()
    } else {
        start
    };
    let credential = text.get(credential_start..start + value.len())?;
    if is_variable_reference(credential) || is_already_masked(credential) {
        return None;
    }
    // A credential is ONE token. Prose is not.
    if credential.chars().count() < HEADER_VALUE_MIN || credential.chars().any(char::is_whitespace)
    {
        return None;
    }
    Some((credential_start, start + value.len()))
}

/// If the header name was itself the value of a `name` key (a HAR entry),
/// return the offset just past the following `value` key's delimiter.
fn har_value_key(text: &str, after_name: usize) -> Option<usize> {
    // BOUNDED LOOKBACK AND LOOKAHEAD. The first version lowercased the entire
    // prefix (`key.to_ascii_lowercase()`) and the entire remainder
    // (`rest.to_ascii_lowercase()`) once per header found — O(n) work per
    // header, O(n²) overall. Measured: the layer took 13 ms at 500 headers and
    // 774 ms at 4,000, a 59x cost for 8x the input, while the core masker it
    // wraps stayed linear. An agent chooses how large a `tool_input` is, so
    // that was a way to blow the 50 ms hook budget from inside a session.
    //
    // Neither scan needs to be unbounded: a HAR pair's `name` key sits a few
    // characters behind, and its `value` key a few dozen ahead.
    const LOOKBACK: usize = 24;
    const LOOKAHEAD: usize = 256;

    let head = text.get(..after_name)?;
    let tail_start = floor_boundary(head, head.len().saturating_sub(LOOKBACK));
    let head = head.get(tail_start..)?;
    let trimmed = head.trim_end_matches(['"', '\'', '`']);
    let before = trimmed.trim_end().trim_end_matches(|c: char| c != ':');
    if !before.ends_with(':') {
        return None;
    }
    let key = before.trim_end_matches(':').trim_end();
    let key = key.trim_end_matches(['"', '\'', '`']);
    if !key.to_ascii_lowercase().ends_with("name") {
        return None;
    }
    let rest_end = ceil_boundary(text, (after_name + LOOKAHEAD).min(text.len()));
    let rest = text.get(after_name..rest_end)?;
    let at = rest
        .as_bytes()
        .windows(5)
        .position(|w| w.eq_ignore_ascii_case(b"value"))?;
    let after = after_name + at + "value".len();
    let bytes = text.as_bytes();
    let mut i = skip_separators(bytes, after);
    if bytes.get(i) == Some(&b':') {
        i += 1;
        Some(i)
    } else {
        None
    }
}

/// The largest char boundary at or below `at`.
fn floor_boundary(s: &str, mut at: usize) -> usize {
    while at > 0 && !s.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The smallest char boundary at or above `at`.
fn ceil_boundary(s: &str, mut at: usize) -> usize {
    while at < s.len() && !s.is_char_boundary(at) {
        at += 1;
    }
    at
}

fn skip_spaces(bytes: &[u8], mut i: usize) -> usize {
    while matches!(bytes.get(i), Some(b' ') | Some(b'\t')) {
        i += 1;
    }
    i
}

/// Whitespace after a delimiter, including an RFC 7230 obs-fold and a YAML
/// indented continuation: a line break followed by a space or tab continues the
/// value.
fn skip_value_whitespace(bytes: &[u8], mut i: usize) -> Option<usize> {
    loop {
        while matches!(bytes.get(i), Some(b'\\')) {
            i += 1;
        }
        i = skip_spaces(bytes, i);
        let fold = match bytes.get(i) {
            Some(b'\r') if bytes.get(i + 1) == Some(&b'\n') => Some(i + 2),
            Some(b'\n') => Some(i + 1),
            _ => None,
        };
        match fold {
            Some(next) if matches!(bytes.get(next), Some(b' ') | Some(b'\t')) => i = next,
            Some(_) => return None,
            None => return Some(i),
        }
    }
}

/// Key-name segments that make an assignment's value a credential.
///
/// Matched per underscore-separated segment, never as a substring: `MONKEY_ID`
/// contains "KEY" and is not a secret.
const SECRET_KEY_SEGMENTS: &[&str] = &[
    "SECRET",
    "SECRETS",
    "TOKEN",
    "TOKENS",
    "PASSWORD",
    "PASSWD",
    "PWD",
    "APIKEY",
    "KEY",
    "KEYS",
    "CREDENTIAL",
    "CREDENTIALS",
    "PRIVATE",
    "AUTH",
    "SESSION",
    "DSN",
    "SALT",
    "SIGNATURE",
    "CERT",
];

/// Whether a key name means "the value beside me is a credential".
/// Whether a key name means "the value beside me is a credential".
///
/// Shared by the text rules and the structural JSON walk, so the two cannot
/// disagree about what a credential-naming key is. Case-insensitive and
/// separator-insensitive: JSON uses `githubToken` and `github-token` where a
/// shell uses `GITHUB_TOKEN`, and all three name the same thing.
pub fn key_implies_credential(key: &str) -> bool {
    let normalized = normalize_key(key);
    if CREDENTIAL_HEADERS.contains(&normalized.to_ascii_lowercase().replace('_', "-").as_str()) {
        return true;
    }
    names_a_secret(&normalized)
}

/// `githubToken`, `github-token`, `GITHUB_TOKEN` → `GITHUB_TOKEN`.
fn normalize_key(key: &str) -> String {
    {
        let mut out = String::with_capacity(key.len() + 4);
        let mut prev_lower = false;
        for c in key.chars() {
            if c == '-' || c == '.' || c == ' ' {
                out.push('_');
                prev_lower = false;
            } else if c.is_ascii_uppercase() {
                // camelCase boundary: `githubToken` -> `GITHUB_TOKEN`.
                if prev_lower {
                    out.push('_');
                }
                out.push(c);
                prev_lower = false;
            } else {
                out.push(c.to_ascii_uppercase());
                prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
            }
        }
        out
    }
}

/// Segments naming a **property of** a credential rather than a credential.
///
/// # A generator, not a list of exceptions
///
/// The first version enumerated sixteen non-secret key names by hand and
/// over-masked **11 of 16** config controls anyway — `AUTH_MODE=oauth2`,
/// `KEY_ALGORITHM=ed25519`, `PRIVATE_SUBNET=10.0.0.0/16`,
/// `SESSION_STORE=redis://localhost:6379`, `TOKEN_TTL_SECONDS`, and worst,
/// `TLS_CERT_PATH=/etc/ssl/certs/server.pem`, **a path**. A209-C spent a whole
/// round establishing that path destruction must not recur; enumerating
/// exceptions guarantees it does, because the list can only ever contain the
/// members somebody already thought of.
///
/// The rule instead: a key that names a mode, algorithm, path, interval, count,
/// endpoint, identifier or any other *attribute* is describing a credential, not
/// holding one. `AWS_SECRET_ACCESS_KEY` has no attribute word and is a secret;
/// `TLS_CERT_PATH` has `PATH` and is not.
const PROPERTY_SEGMENTS: &[&str] = &[
    "ALGO",
    "ALGORITHM",
    "AUDIENCE",
    "BACKEND",
    "CLASS",
    "COUNT",
    "DIR",
    "ENABLED",
    "ENDPOINT",
    "EXPIRES",
    "EXPIRY",
    "FILE",
    "FORMAT",
    "HEADER",
    "HOST",
    "ID",
    "INTERVAL",
    "ISSUER",
    "LENGTH",
    "LEVEL",
    "LIMIT",
    "LOCATION",
    "MAX",
    "METHOD",
    "MIN",
    "MODE",
    "MS",
    "NAME",
    "PATH",
    "POLICY",
    "PORT",
    "PREFIX",
    "PROVIDER",
    "REGION",
    "REQUIRED",
    "ROTATION",
    "SCHEME",
    "SECONDS",
    "SIZE",
    "SOCK",
    "SOCKET",
    "STORE",
    "STRATEGY",
    "SUBNET",
    "SUFFIX",
    "TIMEOUT",
    "TTL",
    "TYPE",
    "URI",
    "URL",
    "VERSION",
];

/// Minimum value length before the `.env` rule fires./// Minimum value length before the `.env` rule fires.
const ENV_VALUE_MIN: usize = 6;

/// Mask the value of `KEY=VALUE` assignments whose key names a credential.
///
/// Structural, like the header rule: the key says what the value is. Bounded by
/// an env-style key shape so ordinary code — `let key = compute()` — is
/// untouched.
fn mask_env_assignments(text: &str, applied: &mut u64, families: &mut BTreeSet<String>) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        match mask_one_assignment(line) {
            Some(masked) => {
                *applied += 1;
                families.insert("env_value".to_string());
                out.push_str(&masked);
            }
            None => out.push_str(line),
        }
    }
    out
}

fn mask_one_assignment(line: &str) -> Option<String> {
    let eq = line.find('=')?;
    let (key_span, value_span) = line.split_at(eq);
    let value_span = &value_span[1..];

    // The key is the last whitespace-separated word before `=`, so
    // `export FOO=bar` and `FOO=bar` both work.
    let key_start = key_span
        .rfind(|c: char| c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);
    let key = key_span[key_start..].trim();
    // ONE predicate, asked the same way the JSON walk asks it.
    //
    // This used to be `names_a_secret(key)` on the RAW key while the JSON walk
    // asked `key_implies_credential`, which normalises case and separators
    // first. The two disagreed, and the disagreement leaked: the predicate said
    // `aws_secret_access_key` names a credential and this caller never heard
    // it, because `is_env_style_key` required upper case and rejected the key
    // before the question was asked. Measured on the exec path —
    // `aws_secret_access_key=wJalrXUtnFEMI/…` survived into a stored output
    // file in plaintext, which is the spelling `~/.aws/credentials` and
    // `aws configure` actually use.
    //
    // That is the A209-C15 shape once more: a predicate serving two callers
    // that disagree about the question. The fix is not to add a spelling, it is
    // to make the two callers ask the same function.
    if key.is_empty() || !is_env_style_key(key) || !key_implies_credential(key) {
        return None;
    }

    // Value ends at whitespace or a quote boundary.
    let quoted = value_span.starts_with('"') || value_span.starts_with('\'');
    let (open, body) = if quoted {
        (&value_span[..1], &value_span[1..])
    } else {
        ("", value_span)
    };
    let end = if quoted {
        body.find(['"', '\'']).unwrap_or(body.len())
    } else {
        body.find(char::is_whitespace).unwrap_or(body.len())
    };
    // Tested against the UN-truncated body: the marker contains a space, so a
    // whitespace-terminated value never sees the whole of it.
    if is_already_masked(body) {
        return None;
    }
    let value = &body[..end];
    if value.chars().count() < ENV_VALUE_MIN || is_variable_reference(value) {
        return None;
    }
    Some(format!(
        "{}={}{}{}",
        key_span,
        open,
        marker("env_value"),
        &body[end..]
    ))
}

/// Does this look like an assignment key rather than an identifier in prose?
///
/// Letters (either case), digits and underscores, **and** either an upper-case
/// letter or a separator. The second half is the precision budget: a bare
/// lower-case word is an identifier — `let token = compute()`, `key = 3` — and
/// masking beside it destroys context for no gain, which A209-C established is
/// a real cost rather than a safe default. `aws_secret_access_key`,
/// `githubToken` and `AWS_SECRET_ACCESS_KEY` all qualify; `token` alone does
/// not.
fn is_env_style_key(key: &str) -> bool {
    key.len() >= 3
        && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && key.bytes().any(|b| b.is_ascii_alphabetic())
        && (key.bytes().any(|b| b.is_ascii_uppercase()) || key.contains('_'))
}

fn names_a_secret(key: &str) -> bool {
    let segments: Vec<&str> = key.split('_').collect();
    // An attribute word anywhere disqualifies the whole key: the value is a
    // property of a credential, not the credential.
    if segments.iter().any(|s| PROPERTY_SEGMENTS.contains(s)) {
        return false;
    }
    segments.iter().any(|s| SECRET_KEY_SEGMENTS.contains(s))
}

#[cfg(test)]
mod tests;
