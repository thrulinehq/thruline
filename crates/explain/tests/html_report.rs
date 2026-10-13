//! §10's HTML output, and the two properties that make it safe to share.

use thruline_explain::html::{escape, render, Report};

fn hostile_report() -> Report {
    Report {
        session: "s1".into(),
        repo: "/repo".into(),
        generated: "2026-09-02T00:00:00Z".into(),
        // Every one of these is attacker-influenced: §13's threat model says
        // the agent knows ThruLine is watching, and all of this is text the
        // agent wrote.
        timeline: vec![(1, "tool_call".into(), "<img src=x onerror=alert(1)>".into())],
        stop_kind: "agent_finished".into(),
        stop_summary: "</p><script>alert('stop')</script>".into(),
        last_events: vec!["<b>bold</b>".into()],
        compaction_note: "none".into(),
        promises: vec![("open".into(), "I'll fix <script>x</script> later".into())],
        claims: vec![(
            "unsupported".into(),
            "tests_pass".into(),
            "All tests pass\" onmouseover=\"alert(1)".into(),
            vec!["<script>reason</script>".into()],
        )],
        findings: vec![("fail".into(), "narrowed_run".into(), "a & b < c".into())],
        risks: vec![(2, "Bash".into(), "rm -rf <x>".into())],
        narrative: Some("it said 'couldn't' & meant it".into()),
    }
}

/// Every element name the renderer is allowed to emit.
///
/// The check is an allowlist over the tags actually present, not a search for
/// known-bad strings: `onerror=alert(1)` appears in the output as *escaped
/// text* and is inert, so searching for it fails on a page that is perfectly
/// safe. What matters is whether the agent's text became **markup**, and the
/// only way to ask that is to look at the markup.
const ALLOWED_TAGS: &[&str] = &[
    "!doctype", "html", "head", "meta", "title", "style", "body", "main", "h1", "h2", "p", "code",
    "q", "table", "thead", "tbody", "tr", "th", "td", "ul", "li", "span", "br", "footer",
];

fn tags_in(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '!' || *c == '/')
            .collect();
        let name = name.trim_start_matches('/').to_ascii_lowercase();
        if !name.is_empty() {
            out.push(name);
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn nothing_the_agent_wrote_can_become_markup_in_a_reviewer_s_browser() {
    // A `<script>` in a commit message that runs in the browser of the person
    // auditing the agent would be this product handing the agent a way to
    // reach its own auditor.
    let html = render(&hostile_report());
    let unexpected: Vec<String> = tags_in(&html)
        .into_iter()
        .filter(|t| !ALLOWED_TAGS.contains(&t.as_str()))
        .collect();
    assert!(
        unexpected.is_empty(),
        "the agent's text became markup: {unexpected:?}"
    );

    // And the text is still THERE, escaped — the point is to show it, not to
    // drop it.
    assert!(html.contains("&lt;script&gt;alert(&#39;stop&#39;)&lt;/script&gt;"));
    assert!(html.contains("a &amp; b &lt; c"));
    assert!(
        html.contains("&lt;img src=x onerror=alert(1)&gt;"),
        "the payload was dropped rather than escaped"
    );
    // The attribute-break attempt is neutralised by the quote escape.
    assert!(
        !html.contains("\" onmouseover=\"alert"),
        "an attribute break survived"
    );
}

#[test]
fn escaping_covers_all_five_including_the_apostrophe() {
    // An unescaped `'` inside a single-quoted attribute is an attribute break.
    assert_eq!(escape("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
    // Ordinary text is untouched.
    assert_eq!(escape("all tests pass"), "all tests pass");
}

#[test]
fn the_file_fetches_nothing_so_it_renders_offline_and_reports_no_reader() {
    // §10: "single self-contained file, shareable". A linked stylesheet or font
    // is a network request made by whoever opens it, which tells a third party
    // who read the report and when — and §0.8 says nothing leaves the machine
    // unless the user enabled it.
    let html = render(&hostile_report());
    for fetching in [
        "<link",
        "src=\"http",
        "src='http",
        "@import",
        "https://",
        "http://",
        "<iframe",
        "url(",
    ] {
        assert!(
            !html.to_ascii_lowercase().contains(fetching),
            "the report fetches something: {fetching:?}"
        );
    }
}

#[test]
fn the_report_leads_with_the_answer_a_reviewer_opened_it_for() {
    let html = render(&hostile_report());
    let claims_at = html.find("Claimed vs proved").expect("claims section");
    let timeline_at = html.find("What happened").expect("timeline section");
    assert!(
        claims_at < timeline_at,
        "the timeline came before the verdicts; a reviewer opens this to find out \
         whether the claim held"
    );
    assert!(html.contains("unsupported"));
}

#[test]
fn an_empty_session_renders_a_page_that_says_so_rather_than_a_blank_one() {
    let html = render(&Report {
        session: "s".into(),
        ..Report::default()
    });
    assert!(html.contains("No completion claims were made."));
    assert!(html.contains("No open promises."));
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.trim_end().ends_with("</html>"));
}
