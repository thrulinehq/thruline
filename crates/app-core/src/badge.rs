//! The `verified-by-thruline` badge (§9), rendered as SVG, and the hosted
//! endpoint that serves it.
//!
//! # Four states, and only one of them says verified
//!
//! A badge is the most-copied and least-read surface a product has. It ends up
//! in a README, screenshotted, and quoted back — so the one thing it must never
//! do is say "verified" about something that was not.
//!
//! | state | label | colour | when |
//! |---|---|---|---|
//! | success | `verified` | green | every claim proved, receipt reproduces, policy passed |
//! | neutral | `partial` | amber | nothing contradicted; something unproved or uncheckable |
//! | failure | `not verified` | red | a check contradicted the receipt, or policy refused |
//! | unknown | `no receipt` | grey | this App has never concluded anything about this ref |
//!
//! The labels are `attest::report::badge_of`'s, and that is a **correction**:
//! this module used to spell `Neutral` as *"not verified"* and `Failure` as
//! *"unsupported"*, while the binary every developer runs spelled `Neutral`
//! *"partial"* and `Failure` *"not verified"*. Two badges for one product, and
//! the collision was on the word `not verified`, which meant a reader could not
//! tell which of the two states they were looking at. One vocabulary now.
//!
//! # Why the hosted endpoint cannot be asked to be green
//!
//! `attest::report::badge_route` takes the state from a `?status=` query
//! parameter. That is correct for its purpose — it renders whatever the caller
//! computed — and it is **unusable for a hosted endpoint**, because on a public
//! URL the caller is anyone. `…/badge.svg?status=verified` in a README would be
//! a green badge with nothing behind it.
//!
//! So [`route`] parses only *which repository and ref* is being asked about,
//! and the answer comes from [`Store`] — a conclusion this App itself recorded
//! when it posted a check run. There is no input to this endpoint that can
//! raise its colour.

use thruline_policy::Conclusion;

/// What the badge can say. The fourth state is the honest one for a repository
/// this App has never concluded anything about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Known(Conclusion),
    /// No check run has ever been posted for this ref.
    Unknown,
}

/// Label and colour. **The only place `"verified"` is produced.**
pub fn label_and_colour(state: State) -> (&'static str, &'static str) {
    match state {
        State::Known(Conclusion::Success) => ("verified", "#2ea043"),
        State::Known(Conclusion::Neutral) => ("partial", "#bf8700"),
        State::Known(Conclusion::Failure) => ("not verified", "#cf222e"),
        // Grey, and a word that claims nothing. Not amber: "we have no record"
        // is a different sentence from "something went unproved", and giving
        // them one colour would let an unconfigured repository look like a
        // partially verified one.
        State::Unknown => ("no receipt", "#6e7781"),
    }
}

/// Escapes text for an SVG text node.
///
/// The badge's labels are all fixed strings from the table above, so nothing
/// user-controlled reaches it today. It is escaped anyway: the first time
/// someone parameterises this, the escaping should already be here rather than
/// be the thing they forgot.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// A self-contained SVG. No external font, no external image, no script.
pub fn svg(state: State) -> String {
    let (label, colour) = label_and_colour(state);
    let left = "thruline";
    // 6.2px per character at 11px, plus 10px padding each side. Approximate on
    // purpose: measuring text needs a font, and a font is an external resource.
    let lw = 10 + (left.len() as u32 * 7);
    let rw = 10 + (label.len() as u32 * 7);
    let w = lw + rw;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="20" role="img" aria-label="{left}: {label}">
  <title>{left}: {label}</title>
  <linearGradient id="s" x2="0" y2="100%">
    <stop offset="0" stop-color="#bbb" stop-opacity=".1"/>
    <stop offset="1" stop-opacity=".1"/>
  </linearGradient>
  <clipPath id="r"><rect width="{w}" height="20" rx="3" fill="#fff"/></clipPath>
  <g clip-path="url(#r)">
    <rect width="{lw}" height="20" fill="#555"/>
    <rect x="{lw}" width="{rw}" height="20" fill="{colour}"/>
    <rect width="{w}" height="20" fill="url(#s)"/>
  </g>
  <g fill="#fff" text-anchor="middle" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" font-size="11">
    <text x="{lx}" y="15" fill="#010101" fill-opacity=".3">{left}</text>
    <text x="{lx}" y="14">{left}</text>
    <text x="{rx}" y="15" fill="#010101" fill-opacity=".3">{label}</text>
    <text x="{rx}" y="14">{label}</text>
  </g>
</svg>
"##,
        left = escape(left),
        label = escape(label),
        lx = lw / 2,
        rx = lw + rw / 2,
    )
}

/// What the hosted endpoint was asked about. **No conclusion in it.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeRequest {
    pub repo: String,
    /// A commit sha, or a branch name. `main` when unspecified.
    pub git_ref: String,
}

/// `GET /badge/{owner}/{repo}.svg[?ref=…]`.
///
/// Returns `None` for anything else, so a server mounting this cannot
/// accidentally serve a badge from a path it did not mean to.
///
/// **`?status=` is not read, and that is the point of this function.** The only
/// query parameter is `ref`, which selects *which* answer to show, never *what*
/// the answer is.
pub fn route(path_and_query: &str) -> Option<BadgeRequest> {
    let (path, query) = match path_and_query.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path_and_query, ""),
    };
    let rest = path.strip_prefix("/badge/")?.strip_suffix(".svg")?;
    let (owner, name) = rest.split_once('/')?;
    let repo = format!("{owner}/{name}");
    if !crate::is_repo_full_name(&repo) {
        return None;
    }
    let git_ref = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("ref="))
        .filter(|r| is_ref(r))
        .unwrap_or("main")
        .to_string();
    Some(BadgeRequest { repo, git_ref })
}

/// A git ref this endpoint will look up: a sha, or an ordinary branch name.
///
/// Refused rather than normalised, for the same reason
/// [`crate::receipt_source::is_receipt_path`] refuses: this value becomes a
/// lookup key, and normalising an attack into something safe-looking is how the
/// next normaliser gets it wrong.
pub fn is_ref(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && !s.starts_with('-')
        && !s.contains("..")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/'))
}

/// What this App has concluded, and nothing else.
///
/// Deliberately a trait: the shell decides whether that is a map in memory or a
/// row in a database, and neither choice can change what the badge is allowed
/// to say.
pub trait Store {
    /// The conclusion this App last posted for `repo` at `git_ref`.
    fn conclusion(&self, repo: &str, git_ref: &str) -> Option<Conclusion>;
}

/// The rendered response: status, content type, cache control, body.
///
/// # The cache header is load-bearing
///
/// GitHub proxies README images through camo and caches them hard. The stale
/// state a reader keeps believing is the green one, so the badge is served with
/// a short freshness window and `must-revalidate`. This reduces the window; it
/// does not close it, because the proxy's behaviour is not ours to set. A
/// repository that needs a badge it can reason about should render one in CI
/// from `thruline verify-receipt --format badge` and commit it.
pub fn respond(store: &dyn Store, req: &BadgeRequest) -> (u16, &'static str, String, String) {
    let state = match store.conclusion(&req.repo, &req.git_ref) {
        Some(c) => State::Known(c),
        None => State::Unknown,
    };
    let body = svg(state);
    // The guard, and it can only ever turn green into an error. Two renderings
    // of one conclusion disagreeing means neither is publishable — the same
    // refusal the Action makes in its badge step, at the hosted endpoint.
    //
    // A real branch and not a `debug_assert!`: this is the property the endpoint
    // exists to hold, and `debug_assert!` compiles out of exactly the build that
    // gets deployed. A badge that cannot be rendered honestly is served as a
    // 500 with no body — a broken image in a README is a visible failure, and a
    // wrong green one is not.
    if state != State::Known(Conclusion::Success) && body.contains(">verified<") {
        return (
            500,
            "text/plain; charset=utf-8",
            "no-store".to_string(),
            String::new(),
        );
    }
    (
        200,
        "image/svg+xml; charset=utf-8",
        "max-age=60, must-revalidate".to_string(),
        body,
    )
}
