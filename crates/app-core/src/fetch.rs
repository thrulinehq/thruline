//! Every GitHub call the App makes, as a **plan** and a **parser** — with no
//! socket between them.
//!
//! # Why a request is a value here
//!
//! A hosted gate's security lives in two places: what it asks for, and what it
//! believes about the answer. Both are pure, and both are the parts worth
//! testing, so both are here; `services/github-app` owns only the part that
//! cannot be — putting the bytes on a wire.
//!
//! The practical consequence is that the fixture replay in
//! `tests/replay.rs` exercises the **real** paths, headers and parsers. A
//! stubbed transport that answered a URL the App does not actually build would
//! be a test of nothing, and that is the usual shape of a webhook test suite.
//!
//! # Sizes, and why every parser has a cap
//!
//! Everything below is attacker-influenced: a pull request author chooses the
//! file, its size, and its contents. A parser with no cap is a memory limit
//! written in someone else's repository.

use crate::receipt_source::is_receipt_path;

/// The API this App talks to. Overridable for GitHub Enterprise; never taken
/// from a webhook payload.
pub const API: &str = "https://api.github.com";

/// The `Accept` every call sends.
pub const ACCEPT: &str = "application/vnd.github+json";

/// The API version header GitHub asks callers to pin.
pub const API_VERSION: &str = "2022-11-28";

/// Two megabytes decoded. A receipt over this is not a receipt.
pub const MAX_RECEIPT_BYTES: usize = 2 * 1024 * 1024;

/// One request, ready for whatever puts bytes on a wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: &'static str,
    /// Path and query, relative to [`API`]. Never contains a host.
    pub path: String,
    pub body: Option<serde_json::Value>,
}

impl Request {
    fn get(path: String) -> Request {
        Request {
            method: "GET",
            path,
            body: None,
        }
    }
    fn post(path: String, body: serde_json::Value) -> Request {
        Request {
            method: "POST",
            path,
            body: Some(body),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    #[error("the response is not JSON: {0}")]
    NotJson(String),
    #[error("`{0}` is missing from the response")]
    Missing(&'static str),
    #[error("the response is {0} bytes, over the {1} byte cap")]
    TooLarge(usize, usize),
    #[error("the content is not base64")]
    NotBase64,
    #[error("`encoding` is {0:?}; only `base64` is read")]
    UnknownEncoding(String),
}

// ---------------------------------------------------------------------------
// Request plans
// ---------------------------------------------------------------------------

/// The files a pull request touches. **`per_page=100` and one page only.**
///
/// A pull request with more than 100 changed files whose receipt is on page two
/// is reported as *no receipt found* — neutral — rather than paged for. That is
/// a real limitation and it is stated in the check run rather than papered
/// over; paging a public endpoint on an unauthenticated trigger is how a gate
/// becomes a rate-limit amplifier.
pub fn pr_files(repo: &str, pr: u64) -> Request {
    Request::get(format!("/repos/{repo}/pulls/{pr}/files?per_page=100"))
}

/// One file at one ref.
pub fn contents(repo: &str, path: &str, git_ref: &str) -> Request {
    Request::get(format!("/repos/{repo}/contents/{path}?ref={git_ref}"))
}

/// GitHub's artifact-attestations lookup, by subject digest.
pub fn attestations(repo: &str, sha256_hex: &str) -> Request {
    Request::get(format!("/repos/{repo}/attestations/sha256:{sha256_hex}"))
}

/// Comments on the pull request, newest last.
pub fn issue_comments(repo: &str, pr: u64) -> Request {
    Request::get(format!("/repos/{repo}/issues/{pr}/comments?per_page=100"))
}

/// The check run §9 names. The payload is [`crate::check_run::payload`]'s.
pub fn post_check_run(repo: &str, payload: serde_json::Value) -> Request {
    Request::post(format!("/repos/{repo}/check-runs"), payload)
}

/// The PR comment. Body is [`crate::comment::render`]'s.
pub fn post_comment(repo: &str, pr: u64, markdown: &str) -> Request {
    Request::post(
        format!("/repos/{repo}/issues/{pr}/comments"),
        serde_json::json!({ "body": markdown }),
    )
}

/// Existing comments, so the App edits its own rather than adding one per push.
pub fn patch_comment(repo: &str, comment_id: u64, markdown: &str) -> Request {
    Request {
        method: "PATCH",
        path: format!("/repos/{repo}/issues/comments/{comment_id}"),
        body: Some(serde_json::json!({ "body": markdown })),
    }
}

/// Exchange an App JWT for an installation token. The JWT itself is minted in
/// `services/github-app`, which is where the private key is.
pub fn installation_token(installation: u64) -> Request {
    Request::post(
        format!("/app/installations/{installation}/access_tokens"),
        serde_json::json!({}),
    )
}

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

/// Receipt paths from a pull-request file list, **filtered by
/// [`is_receipt_path`]**.
///
/// The filter is not tidying. `filename` is chosen by the pull request's
/// author, this value ends up in a URL path, and a name like
/// `.thruline/receipts/../../.github/workflows/x.json` is refused rather than
/// normalised. Removed files are skipped: a receipt this pull request deletes
/// is not a receipt this pull request has.
pub fn parse_pr_files(body: &[u8]) -> Result<Vec<String>, FetchError> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| FetchError::NotJson(e.to_string()))?;
    let arr = v.as_array().ok_or(FetchError::Missing("(array)"))?;
    let mut out: Vec<String> = arr
        .iter()
        .filter(|f| f.get("status").and_then(|s| s.as_str()) != Some("removed"))
        .filter_map(|f| f.get("filename").and_then(|s| s.as_str()))
        .filter(|p| is_receipt_path(p))
        .map(str::to_string)
        .collect();
    // Deterministic: two deliveries about the same tree must pick the same
    // receipt, or the check run flickers between them.
    out.sort();
    Ok(out)
}

/// A file body from the contents API.
pub fn parse_contents(body: &[u8]) -> Result<Vec<u8>, FetchError> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| FetchError::NotJson(e.to_string()))?;
    match v.get("encoding").and_then(|e| e.as_str()) {
        Some("base64") => {}
        Some(other) => return Err(FetchError::UnknownEncoding(other.to_string())),
        None => return Err(FetchError::Missing("encoding")),
    }
    let content = v
        .get("content")
        .and_then(|c| c.as_str())
        .ok_or(FetchError::Missing("content"))?;
    let bytes = base64_decode(content)?;
    if bytes.len() > MAX_RECEIPT_BYTES {
        return Err(FetchError::TooLarge(bytes.len(), MAX_RECEIPT_BYTES));
    }
    Ok(bytes)
}

/// The DSSE envelope out of an artifact-attestations response.
///
/// **The strongest source**, because GitHub vouches for the association between
/// the attestation and the repository — an author cannot put one there by
/// committing a file. The first attestation is taken; a subject digest with
/// several is a case this build does not rank, and taking the first
/// deterministically beats picking a "best" one by a rule nobody wrote down.
pub fn parse_attestation(body: &[u8]) -> Result<Vec<u8>, FetchError> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| FetchError::NotJson(e.to_string()))?;
    let first = v
        .get("attestations")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .ok_or(FetchError::Missing("attestations[0]"))?;
    let envelope = first
        .pointer("/bundle/dsseEnvelope")
        .ok_or(FetchError::Missing("attestations[0].bundle.dsseEnvelope"))?;
    let bytes = serde_json::to_vec(envelope).map_err(|e| FetchError::NotJson(e.to_string()))?;
    if bytes.len() > MAX_RECEIPT_BYTES {
        return Err(FetchError::TooLarge(bytes.len(), MAX_RECEIPT_BYTES));
    }
    Ok(bytes)
}

/// The marker a comment must carry for its fenced block to be read as a
/// receipt.
///
/// Deliberately explicit: without it, any comment containing a JSON code fence
/// becomes a candidate receipt, and a reviewer pasting an example into the
/// thread changes what the gate verifies.
pub const COMMENT_MARKER: &str = "<!-- thruline-receipt -->";

/// The **weakest** source: a receipt attached to a pull request comment.
///
/// Anyone who can comment can attach one, which is exactly why
/// [`crate::receipt_source::SEARCH_ORDER`] looks here last and why the check run
/// always names the source. The bytes still have to carry their own signature;
/// this only decides where to look.
pub fn parse_comment_attachment(body: &[u8]) -> Result<Option<Vec<u8>>, FetchError> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| FetchError::NotJson(e.to_string()))?;
    let arr = v.as_array().ok_or(FetchError::Missing("(array)"))?;
    // Last wins: the most recent attachment is the one a human would expect to
    // be judged, and the order is the API's, not ours.
    for c in arr.iter().rev() {
        let Some(text) = c.get("body").and_then(|b| b.as_str()) else {
            continue;
        };
        let Some(after) = text.split_once(COMMENT_MARKER).map(|(_, a)| a) else {
            continue;
        };
        let Some(open) = after.find("```json") else {
            continue;
        };
        let rest = &after[open + "```json".len()..];
        let Some(close) = rest.find("```") else {
            continue;
        };
        let block = rest[..close].trim();
        if block.len() > MAX_RECEIPT_BYTES {
            return Err(FetchError::TooLarge(block.len(), MAX_RECEIPT_BYTES));
        }
        // Parsed, not just sliced: a fence that is not JSON is not a receipt,
        // and finding out here rather than in the verifier keeps the source
        // ordering honest — a malformed comment must not shadow nothing.
        if serde_json::from_str::<serde_json::Value>(block).is_ok() {
            return Ok(Some(block.as_bytes().to_vec()));
        }
    }
    Ok(None)
}

/// The App's own previous comment on this pull request, if any.
pub fn find_own_comment(body: &[u8], marker: &str) -> Result<Option<u64>, FetchError> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| FetchError::NotJson(e.to_string()))?;
    let arr = v.as_array().ok_or(FetchError::Missing("(array)"))?;
    Ok(arr
        .iter()
        .filter(|c| {
            c.get("body")
                .and_then(|b| b.as_str())
                .is_some_and(|t| t.contains(marker))
        })
        .filter_map(|c| c.get("id").and_then(|i| i.as_u64()))
        .next_back())
}

/// The installation token out of the exchange response.
///
/// Returns the token; **never logged, never rendered**. The type is a plain
/// `String` because wrapping it would only move the discipline, and the one
/// rule is stated where it is enforced: nothing in this crate formats it.
pub fn parse_installation_token(body: &[u8]) -> Result<String, FetchError> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| FetchError::NotJson(e.to_string()))?;
    v.get("token")
        .and_then(|t| t.as_str())
        .map(str::to_string)
        .ok_or(FetchError::Missing("token"))
}

// ---------------------------------------------------------------------------
// base64
// ---------------------------------------------------------------------------

/// Standard base64 with padding, tolerating the newlines GitHub's contents API
/// wraps at.
///
/// Hand-written rather than pulled in: this crate's dependency list is a
/// security property (see `tests/no_network.rs`), and 30 lines with a test that
/// includes the wrapped form is cheaper than defending an addition to it. It
/// **refuses** malformed input rather than skipping bad characters — a decoder
/// that silently drops what it does not understand turns a corrupted receipt
/// into a different, well-formed one.
pub fn base64_decode(s: &str) -> Result<Vec<u8>, FetchError> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut quad = [0u8; 4];
    let mut n = 0usize;
    let mut pad = 0usize;
    for c in s.bytes() {
        if c == b'\n' || c == b'\r' {
            continue;
        }
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                pad += 1;
                0
            }
            _ => return Err(FetchError::NotBase64),
        };
        // Padding may only appear at the end, and only as one or two bytes.
        if pad > 0 && c != b'=' {
            return Err(FetchError::NotBase64);
        }
        quad[n] = v;
        n += 1;
        if n == 4 {
            let b = (u32::from(quad[0]) << 18)
                | (u32::from(quad[1]) << 12)
                | (u32::from(quad[2]) << 6)
                | u32::from(quad[3]);
            out.push((b >> 16) as u8);
            out.push((b >> 8) as u8);
            out.push(b as u8);
            n = 0;
        }
    }
    if n != 0 {
        // A trailing group that is not four characters and not padded.
        return Err(FetchError::NotBase64);
    }
    if pad > 2 {
        return Err(FetchError::NotBase64);
    }
    out.truncate(out.len() - pad);
    Ok(out)
}
