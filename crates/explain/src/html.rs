//! §10's HTML output: *"single self-contained file, shareable"*.
//!
//! # Self-contained means self-contained
//!
//! No stylesheet link, no script, no font, no image — one file that renders the
//! same on a machine with no network. That is not decoration: the audience for
//! this file is a reviewer who was sent it, and a report that phones home to
//! render is a report that leaks who read it and when. §0.8 says nothing leaves
//! the machine unless the user enables it, and a shared artifact is still the
//! user's machine's output.
//!
//! # Everything here is escaped, including what the agent wrote
//!
//! The final assistant message, tool inputs and command lines all reach this
//! page, and all of them are attacker-influenced in the threat model §13
//! describes: the agent knows ThruLine is watching. A `<script>` in a commit
//! message that executes in a reviewer's browser would be this product handing
//! an agent a way to reach the person auditing it.

use serde::Serialize;

/// Everything the page shows. Built by the caller so this module is pure.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub session: String,
    pub repo: String,
    pub generated: String,
    pub timeline: Vec<(u64, String, String)>,
    pub stop_kind: String,
    pub stop_summary: String,
    pub last_events: Vec<String>,
    pub compaction_note: String,
    pub promises: Vec<(String, String)>,
    /// `(verdict, kind, text, reasons)`
    pub claims: Vec<(String, String, String, Vec<String>)>,
    /// `(severity, detector, detail)`
    pub findings: Vec<(String, String, String)>,
    pub risks: Vec<(u64, String, String)>,
    pub narrative: Option<String>,
}

/// Escape for HTML text and attribute contexts.
///
/// All five, including `'` — an unescaped apostrophe inside a
/// single-quoted attribute is an attribute break, and this text is
/// attacker-influenced.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn verdict_class(v: &str) -> &'static str {
    match v {
        "supported" => "ok",
        "unsupported" => "bad",
        _ => "unknown",
    }
}

/// Render the whole report as one self-contained HTML document.
pub fn render(r: &Report) -> String {
    let mut h = String::with_capacity(8 * 1024);
    h.push_str("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    h.push_str(&format!(
        "<title>ThruLine — {}</title>\n",
        escape(&r.session)
    ));
    h.push_str(STYLE);
    h.push_str("</head><body>\n<main>\n");

    h.push_str(&format!(
        "<h1>What the agent did, and what it proved</h1>\n\
         <p class=\"meta\">session <code>{}</code> · {} · generated {}</p>\n",
        escape(&r.session),
        escape(&r.repo),
        escape(&r.generated)
    ));

    // 5 first: it is what a reviewer opened the file for.
    h.push_str("<h2>Claimed vs proved</h2>\n");
    if r.claims.is_empty() {
        h.push_str("<p class=\"none\">No completion claims were made.</p>\n");
    } else {
        h.push_str("<table><thead><tr><th>Verdict</th><th>Claim</th><th>What the record shows</th></tr></thead><tbody>\n");
        for (verdict, kind, text, reasons) in &r.claims {
            h.push_str(&format!(
                "<tr><td><span class=\"v {cls}\">{v}</span></td>\
                 <td><code>{k}</code><br><q>{t}</q></td><td><ul>",
                cls = verdict_class(verdict),
                v = escape(verdict),
                k = escape(kind),
                t = escape(text)
            ));
            for reason in reasons {
                h.push_str(&format!("<li>{}</li>", escape(reason)));
            }
            h.push_str("</ul></td></tr>\n");
        }
        h.push_str("</tbody></table>\n");
    }
    if let Some(n) = &r.narrative {
        h.push_str(&format!("<p class=\"note\">{}</p>\n", escape(n)));
    }

    h.push_str("<h2>Why it stopped</h2>\n");
    h.push_str(&format!(
        "<p><code>{}</code> — {}</p>\n",
        escape(&r.stop_kind),
        escape(&r.stop_summary)
    ));
    if !r.last_events.is_empty() {
        h.push_str("<ul class=\"tight\">\n");
        for e in &r.last_events {
            h.push_str(&format!("<li>{}</li>\n", escape(e)));
        }
        h.push_str("</ul>\n");
    }

    h.push_str("<h2>Risk flags</h2>\n");
    if r.risks.is_empty() && r.findings.is_empty() {
        h.push_str("<p class=\"none\">None.</p>\n");
    } else {
        h.push_str("<ul>\n");
        for (seq, text, why) in &r.risks {
            h.push_str(&format!(
                "<li><span class=\"v bad\">destructive</span> seq {seq} — <code>{}</code> {}</li>\n",
                escape(text),
                escape(why)
            ));
        }
        for (sev, detector, detail) in &r.findings {
            h.push_str(&format!(
                "<li><span class=\"v {cls}\">{s}</span> <code>{d}</code> — {x}</li>\n",
                cls = if sev.eq_ignore_ascii_case("fail") {
                    "bad"
                } else {
                    "warn"
                },
                s = escape(sev),
                d = escape(detector),
                x = escape(detail)
            ));
        }
        h.push_str("</ul>\n");
    }

    h.push_str("<h2>Promised and not done</h2>\n");
    if r.promises.is_empty() {
        h.push_str("<p class=\"none\">No open promises.</p>\n");
    } else {
        h.push_str("<ul>\n");
        for (status, text) in &r.promises {
            h.push_str(&format!(
                "<li><span class=\"v {cls}\">{s}</span> {t}</li>\n",
                cls = if status == "open" { "warn" } else { "ok" },
                s = escape(status),
                t = escape(text)
            ));
        }
        h.push_str("</ul>\n");
    }

    h.push_str("<h2>Lost across compaction</h2>\n");
    h.push_str(&format!("<p>{}</p>\n", escape(&r.compaction_note)));

    h.push_str("<h2>What happened</h2>\n<table class=\"timeline\"><tbody>\n");
    for (seq, kind, text) in &r.timeline {
        h.push_str(&format!(
            "<tr><td class=\"seq\">{seq}</td><td class=\"kind\">{}</td><td>{}</td></tr>\n",
            escape(kind),
            escape(text)
        ));
    }
    h.push_str("</tbody></table>\n");

    h.push_str(
        "<footer><p>Every line above is derived from a hash-chained event log on the \
         machine that produced it. Nothing here was taken from the agent's own account \
         of what it did, except the sentences shown in quotes — which are the claims, \
         not the evidence.</p></footer>\n",
    );
    h.push_str("</main></body></html>\n");
    h
}

/// Inline, because §10 says self-contained and a linked stylesheet is a
/// network request made by whoever opens the file.
const STYLE: &str = r#"<style>
:root{--fg:#1a1a1a;--dim:#666;--line:#e2e2e2;--ok:#0a7a3d;--bad:#b3261e;--warn:#8a6100;--bg:#fff}
@media(prefers-color-scheme:dark){:root{--fg:#e8e8e8;--dim:#a0a0a0;--line:#333;--ok:#5fd08a;--bad:#f2857c;--warn:#e3b341;--bg:#141414}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.55 -apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif}
main{max-width:52rem;margin:0 auto;padding:2rem 1.25rem 4rem}
h1{font-size:1.5rem;margin:0 0 .25rem}
h2{font-size:1.05rem;margin:2rem 0 .5rem;padding-bottom:.25rem;border-bottom:1px solid var(--line)}
.meta{color:var(--dim);margin:0 0 1.5rem;font-size:.85rem}
code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:.85em}
q{color:var(--dim)}
table{width:100%;border-collapse:collapse;font-size:.9rem}
th{text-align:left;font-weight:600;color:var(--dim);font-size:.75rem;text-transform:uppercase;letter-spacing:.04em}
td,th{padding:.5rem .5rem .5rem 0;border-bottom:1px solid var(--line);vertical-align:top}
ul{margin:.25rem 0;padding-left:1.1rem}
ul.tight li{color:var(--dim);font-size:.9rem}
.v{display:inline-block;padding:.1rem .45rem;border-radius:3px;font-size:.72rem;font-weight:700;text-transform:uppercase;letter-spacing:.03em;white-space:nowrap}
.v.ok{background:color-mix(in srgb,var(--ok) 15%,transparent);color:var(--ok)}
.v.bad{background:color-mix(in srgb,var(--bad) 15%,transparent);color:var(--bad)}
.v.warn{background:color-mix(in srgb,var(--warn) 18%,transparent);color:var(--warn)}
.v.unknown{background:color-mix(in srgb,var(--dim) 15%,transparent);color:var(--dim)}
.none{color:var(--dim)}
.note{color:var(--dim);font-style:italic}
.timeline .seq{color:var(--dim);text-align:right;width:4rem;font-variant-numeric:tabular-nums}
.timeline .kind{color:var(--dim);width:9rem}
footer{margin-top:3rem;padding-top:1rem;border-top:1px solid var(--line);color:var(--dim);font-size:.82rem}
</style>
"#;
