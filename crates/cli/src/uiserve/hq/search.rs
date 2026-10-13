//! HQ Next — `GET /api/hq/projects/<id>/search`: the record search behind
//! ⌘K's Find rows (C30 R-ROUTES "Record search", census §3.08 row 4).
//!
//! ```text
//! GET /api/hq/projects/<id>/search?q=<text>[&limit=<n>][&thread=<tid>]
//! ```
//!
//! - `q` — what the person typed, percent-encoded (`+` is a space); 1 to
//!   [`MAX_QUERY_CHARS`] characters, not only spaces.
//! - `limit` — how many hits to serve, 1 to [`MAX_LIMIT`]; default
//!   [`DEFAULT_LIMIT`].
//! - `thread` — the Change open in the palette, by its thread id. Of that
//!   thread only `Read` and `Version` hits are served: the palette draws its
//!   prompts, edits, commands and statements from the thread's own view,
//!   where each has the step every Change screen names.
//!
//! The only `/api/hq/` address that takes a query string; every other one
//! still refuses one. Read-only: it builds the project's search corpus from
//! the cached index ([`thruline_core::hq::search::Corpus`]) once per index
//! fingerprint, keeps the latest one in memory, writes nothing, appends
//! nothing and adds no DDL. Every string it serves has been masked by
//! `redact::mask_recorded`, and the merged record is served as `masking`.

use super::*;

use thruline_core::hq::search::{Corpus, DEFAULT_LIMIT, MAX_LIMIT, MAX_QUERY_CHARS};

/// The parameters a search was asked with.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Params {
    pub q: String,
    pub limit: usize,
    pub thread: Option<String>,
}

/// The query string, or the refusal's code. A parameter given twice, or one
/// this route does not know, is refused rather than ignored: a parameter that
/// did nothing is a caller believing it asked something.
pub(super) fn params(query: &str) -> std::result::Result<Params, &'static str> {
    let mut q: Option<String> = None;
    let mut limit: Option<usize> = None;
    let mut thread: Option<String> = None;
    if query.is_empty() {
        return Err("missing_query");
    }
    for pair in query.split('&') {
        if pair.is_empty() {
            return Err("malformed_request");
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let decoded = percent_decode(&value.replace('+', " ")).ok_or("malformed_request")?;
        match name {
            "q" if q.is_none() => q = Some(decoded),
            "limit" if limit.is_none() => {
                let n: usize = decoded.parse().map_err(|_| "bad_limit")?;
                if n == 0 || n > MAX_LIMIT {
                    return Err("bad_limit");
                }
                limit = Some(n);
            }
            "thread" if thread.is_none() => {
                let ok = !decoded.is_empty()
                    && decoded.len() <= 64
                    && decoded.chars().all(|c| c.is_ascii_alphanumeric());
                if !ok {
                    return Err("malformed_request");
                }
                thread = Some(decoded);
            }
            "q" | "limit" | "thread" => return Err("malformed_request"),
            _ => return Err("unknown_query_parameter"),
        }
    }
    let q = q.ok_or("missing_query")?;
    if q.trim().is_empty() {
        return Err("missing_query");
    }
    if q.chars().count() > MAX_QUERY_CHARS {
        return Err("query_too_long");
    }
    Ok(Params {
        q,
        limit: limit.unwrap_or(DEFAULT_LIMIT),
        thread,
    })
}

pub(super) fn handle(
    state: &Arc<State>,
    method: &str,
    id: &str,
    query: &str,
    headers: &RequestHeaders,
    stream: &mut TcpStream,
) -> Result<()> {
    if headers.malformed {
        return refuse(stream, 400, "malformed_request");
    }
    if method != "GET" {
        return refuse(stream, 405, "method_not_allowed");
    }
    let Some(id) = decode(id) else {
        return refuse(stream, 400, "malformed_request");
    };
    let asked = match params(query) {
        Ok(p) => p,
        Err(code) => return refuse(stream, 400, code),
    };
    let Some((ledger, idx, fp)) = find_with_fingerprint(state, &id, stream)? else {
        return Ok(());
    };
    let policy = match exclusion_serve::policy(&ledger, &idx.project().root) {
        Ok(policy) => policy,
        Err(e) => return failed(state, stream, e),
    };
    let corpus = match corpus_for(&state.hq, &ledger, &id, &fp, &idx, &policy) {
        Ok(c) => c,
        Err(e) => return failed(state, stream, e),
    };
    if let Some(t) = asked.thread.as_deref() {
        if !corpus.has_thread(t) {
            return refuse(stream, 404, "no_such_thread");
        }
    }
    let doc = corpus.search(&asked.q, asked.limit, asked.thread.as_deref());
    json(stream, 200, &doc)
}

/// The search corpus of a project's index, from memory when it was built for
/// the same fingerprint. One build at a time; the latest corpus is kept.
fn corpus_for(
    hq: &HqState,
    ledger: &Ledger,
    project: &str,
    fp: &str,
    idx: &core_hq::Index,
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
) -> anyhow::Result<Arc<Corpus>> {
    let key = format!("{fp}|exclusions:{}", policy.fingerprint());
    let cached = || {
        hq.search.lock().ok().and_then(|kept| {
            kept.as_ref()
                .filter(|(p, f, _)| p == project && f == &key)
                .map(|(_, _, c)| c.clone())
        })
    };
    // The policy key cannot encode current Git attributes. With generated
    // enabled, rebuild on every read so edits to .gitattributes or Git's
    // attribute sources take effect without an index or hold mutation.
    let cacheable = !policy.has_generated();
    if cacheable {
        if let Some(c) = cached() {
            return Ok(c);
        }
    }
    // A poisoned lock means a build panicked; building again is still right.
    let _building = hq.search_build.lock().unwrap_or_else(|p| p.into_inner());
    if cacheable {
        if let Some(c) = cached() {
            return Ok(c);
        }
    }
    let mut built = Corpus::build(ledger, idx)?;
    let generated = exclusion_serve::generated_for_texts(policy, built.searchable_texts())?;
    built.apply_exclusions(policy, |path| generated.contains(path));
    let corpus = Arc::new(built);
    if cacheable {
        if let Ok(mut kept) = hq.search.lock() {
            *kept = Some((project.to_string(), key, corpus.clone()));
        }
    }
    Ok(corpus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_string_is_read_strictly() {
        assert_eq!(
            params("q=totals.py"),
            Ok(Params {
                q: "totals.py".into(),
                limit: DEFAULT_LIMIT,
                thread: None
            })
        );
        assert_eq!(
            params("q=was+a%20migration+run%3F&limit=5&thread=0a1b2c3d4e5f6071"),
            Ok(Params {
                q: "was a migration run?".into(),
                limit: 5,
                thread: Some("0a1b2c3d4e5f6071".into())
            })
        );
        assert_eq!(params("q=a%2Bb").map(|p| p.q), Ok("a+b".into()));
        assert_eq!(params(""), Err("missing_query"));
        assert_eq!(params("q=a&"), Err("malformed_request"));
        assert_eq!(params("limit=5"), Err("missing_query"));
        assert_eq!(params("q=+++"), Err("missing_query"));
        assert_eq!(params("q=a&q=b"), Err("malformed_request"));
        assert_eq!(params("q=a&limit=0"), Err("bad_limit"));
        assert_eq!(params("q=a&limit=101"), Err("bad_limit"));
        assert_eq!(params("q=a&limit=x"), Err("bad_limit"));
        assert_eq!(params("q=a&thread=../x"), Err("malformed_request"));
        assert_eq!(params("q=a&page=2"), Err("unknown_query_parameter"));
        assert_eq!(params("q=%ZZ"), Err("malformed_request"));
        assert_eq!(params("q=%00"), Err("malformed_request"));
        let long = "a".repeat(MAX_QUERY_CHARS + 1);
        assert_eq!(params(&format!("q={long}")), Err("query_too_long"));
        let just = "a".repeat(MAX_QUERY_CHARS);
        assert!(params(&format!("q={just}")).is_ok());
    }
}
