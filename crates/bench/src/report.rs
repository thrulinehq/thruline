//! The Overclaim Index page — BUILD-SPEC §12's public artefact.
//!
//! # What this module refuses to render
//!
//! §12 says the page must include methodology and fixture versions. That is
//! usually a documentation instruction; here it is a return type. [`render`]
//! takes a [`Headline`], which [`crate::Measurement::overclaim_index`] will not
//! produce for a self-authored corpus or one with too few honest shapes — so
//! the page cannot exist for a measurement that should not be published.
//!
//! And what it does render, it renders **whole**: coverage beside AUROC, the
//! per-shape false-positive table beside the pooled-looking headline, and the
//! composition of the honest set beside both. A reader who sees the number sees
//! what it was measured on, because there is no template branch that omits
//! them.
//!
//! Self-contained by construction: no stylesheet, script, font or image is
//! fetched. §0.8 — a linked resource is a network request made by whoever opens
//! the report, which tells a third party who read it and when.

use crate::{Headline, Measurement};

/// A page, or the reason there is not one.
pub fn render(headline: &Headline, methodology: &Methodology) -> Result<String, NotPublishable> {
    if methodology.corpus_version.trim().is_empty() {
        return Err(NotPublishable::NoFixtureVersion);
    }
    if methodology.thruline_version.trim().is_empty() {
        return Err(NotPublishable::NoToolVersion);
    }
    let mut html = String::new();
    html.push_str("<h1>Overclaim Index</h1>\n");
    html.push_str(&format!(
        "<p class=\"corpus\">Corpus <code>{}</code> at <code>{}</code>, measured by \
         ThruLine <code>{}</code>.</p>\n",
        esc(&headline.corpus),
        esc(&methodology.corpus_version),
        esc(&methodology.thruline_version)
    ));

    // The headline and its two inseparable companions, in one block.
    html.push_str("<table class=\"headline\">\n");
    html.push_str(&row("AUROC of `unsupported`", &fmt(headline.auroc)));
    html.push_str(&row("catch rate", &fmt(headline.catch_rate)));
    html.push_str(&row(
        "coverage (reached any verdict)",
        &format!(
            "{} — a detector that abstains on everything hard scores well on what \
             remains, and this is the number that says so",
            fmt(headline.coverage)
        ),
    ));
    if let Some((shape, rate)) = headline.worst_shape() {
        html.push_str(&row(
            "worst honest shape",
            &format!(
                "{} at {} ({}/{})",
                esc(shape),
                fmt(rate.rate()),
                rate.accused,
                rate.total
            ),
        ));
    }
    if let Some((shape, share)) = &headline.concentration {
        html.push_str(&row(
            "largest honest-work family",
            &format!(
                "{} at {:.1}% of the honest set — a per-shape table cannot be \
                 read without knowing how lopsided the set behind it is",
                esc(shape),
                share * 100.0
            ),
        ));
    }
    html.push_str(&row(
        "meets §12's targets",
        if headline.meets_targets() {
            "yes"
        } else {
            "no — see the per-shape table; a target is judged on the worst shape, \
             not the average"
        },
    ));
    html.push_str("</table>\n");

    // Never pooled.
    html.push_str("<h2>False positives on honest work, per shape</h2>\n");
    html.push_str(
        "<p>A pooled rate over a corpus dominated by one shape is that \
                   shape's rate wearing the corpus's name. This build measured a \
                   family at 80% while the pooled figure looked fine. No accessor \
                   produces a pooled rate, though one is reconstructable from \
                   this table — the design discourages it rather than \
                   preventing it.</p>\n",
    );
    html.push_str("<table><tr><th>shape</th><th>accused</th><th>total</th><th>rate</th></tr>\n");
    for (shape, rate) in &headline.by_shape {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            esc(shape),
            rate.accused,
            rate.total,
            fmt(rate.rate())
        ));
    }
    html.push_str("</table>\n");

    html.push_str("<h2>Composition of the set</h2>\n");
    html.push_str("<table><tr><th>label</th><th>fixtures</th></tr>\n");
    for (label, n) in &headline.composition {
        html.push_str(&format!("<tr><td>{}</td><td>{n}</td></tr>\n", esc(label)));
    }
    html.push_str("</table>\n");

    html.push_str("<h2>Methodology</h2>\n<pre>");
    html.push_str(&esc(&methodology.notes));
    html.push_str("</pre>\n");
    Ok(html)
}

/// What a page must say about how it was made.
#[derive(Debug, Clone)]
pub struct Methodology {
    pub corpus_version: String,
    pub thruline_version: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotPublishable {
    /// §12 requires fixture versions. A page that cannot say which version it
    /// measured cannot be reproduced, and an irreproducible number is a claim.
    NoFixtureVersion,
    NoToolVersion,
}

impl std::fmt::Display for NotPublishable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotPublishable::NoFixtureVersion => write!(
                f,
                "the corpus names no version. §12 requires fixture versions in the \
                 methodology; a number nobody can reproduce is a claim."
            ),
            NotPublishable::NoToolVersion => {
                write!(f, "the measurement names no ThruLine version")
            }
        }
    }
}

/// A plain-text summary for a terminal, including the ablation table.
///
/// Takes the `Measurement` rather than the `Headline`, because an ablation is a
/// property of the run and a headline is a published figure.
pub fn ablation_table(m: &Measurement, detectors: &[&str]) -> String {
    let mut out = String::from("detector removed        catch rate   worst honest shape\n");
    let base = m.catch_rate();
    out.push_str(&format!(
        "{:<22}  {:>10}   {}\n",
        "(none)",
        base.map(fmt).unwrap_or_else(|| "n/a".into()),
        m.worst_shape()
            .map(|(s, r)| format!("{s} {}", fmt(r.rate())))
            .unwrap_or_else(|| "n/a".into())
    ));
    for d in detectors {
        let ablated = m.without_detector(d);
        out.push_str(&format!(
            "{:<22}  {:>10}   {}\n",
            d,
            ablated
                .catch_rate()
                .map(fmt)
                .unwrap_or_else(|| "n/a".into()),
            ablated
                .worst_shape()
                .map(|(s, r)| format!("{s} {}", fmt(r.rate())))
                .unwrap_or_else(|| "n/a".into())
        ));
    }
    out
}

fn row(label: &str, value: &str) -> String {
    format!("<tr><th>{}</th><td>{}</td></tr>\n", esc(label), esc(value))
}

fn fmt(x: f64) -> String {
    format!("{:.3}", x)
}

/// Escape everything. A corpus id and a shape name are external input.
fn esc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#39;".to_string(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ground, Label, Measurement, Outcome, Provenance, Verdict};

    fn reportable() -> Measurement {
        let shapes = [
            "literal_change",
            "rename",
            "test_split",
            "platform_skip",
            "xfail",
        ];
        let mut outcomes: Vec<Outcome> = Vec::new();
        for (i, s) in shapes.iter().enumerate() {
            outcomes.push(Outcome {
                id: format!("h{i}"),
                task_id: None,
                label: Ground::from_oracle("resolved", "1", Label::HonestPass),
                verdict: Some(Verdict::Supported),
                shape: (*s).to_string(),
                duration_ms: 5,
                detectors: Vec::new(),
                executed_checks: 1,
                protocol_hash: None,
            });
            outcomes.push(Outcome {
                id: format!("p{i}"),
                task_id: None,
                label: Ground::from_oracle("resolved", "0", Label::Cheated),
                verdict: Some(Verdict::Unsupported),
                shape: "-".to_string(),
                duration_ms: 7,
                detectors: vec!["narrowed_run".into()],
                executed_checks: 1,
                protocol_hash: None,
            });
        }
        Measurement::of(
            "corpus",
            Provenance::ExternalLabelled(
                crate::ExternalSource::new("princeton-nlp", "swe-bench", "v1", "resolved")
                    .expect("source"),
            ),
            None,
            outcomes,
        )
    }

    fn methodology() -> Methodology {
        Methodology {
            corpus_version: "v1.2.3".into(),
            thruline_version: "0.1.0".into(),
            notes: "fetched, unmodified".into(),
        }
    }

    #[test]
    fn a_page_carries_coverage_composition_and_the_per_shape_table() {
        let h = reportable().overclaim_index().expect("reportable");
        let page = render(&h, &methodology()).expect("publishable");
        assert!(page.contains("AUROC"), "{page}");
        assert!(
            page.contains("coverage"),
            "a page without coverage lets an abstaining detector look good"
        );
        assert!(page.contains("per shape"), "{page}");
        assert!(page.contains("Composition"), "{page}");
        for shape in ["literal_change", "rename", "xfail"] {
            assert!(page.contains(shape), "{shape} missing from {page}");
        }
        assert!(page.contains("v1.2.3"), "the fixture version must be on it");
    }

    /// §12 requires fixture versions, and this is that requirement as a type.
    #[test]
    fn a_page_without_a_fixture_version_is_refused() {
        let h = reportable().overclaim_index().expect("reportable");
        let mut m = methodology();
        m.corpus_version = "  ".into();
        assert_eq!(
            render(&h, &m).expect_err("must refuse"),
            NotPublishable::NoFixtureVersion
        );
        let mut m = methodology();
        m.thruline_version = String::new();
        assert_eq!(
            render(&h, &m).expect_err("must refuse"),
            NotPublishable::NoToolVersion
        );
    }

    /// The page fetches nothing. §0.8.
    #[test]
    fn the_page_is_self_contained() {
        let h = reportable().overclaim_index().expect("reportable");
        let page = render(&h, &methodology()).expect("publishable");
        for forbidden in ["http://", "https://", "<script", "<link", "@import", "src="] {
            assert!(
                !page.contains(forbidden),
                "the page reaches out: {forbidden} in {page}"
            );
        }
    }

    /// A corpus name is external input and reaches the page as text.
    #[test]
    fn a_corpus_name_cannot_put_markup_on_the_page() {
        let mut outcomes = reportable().outcomes().to_vec();
        outcomes[0].shape = "<script>alert(1)</script>".into();
        let m = Measurement::of(
            "<img onerror=x>",
            Provenance::ExternalLabelled(
                crate::ExternalSource::new("p", "d", "v", "resolved").expect("source"),
            ),
            None,
            outcomes,
        );
        let h = m.overclaim_index().expect("reportable");
        let page = render(&h, &methodology()).expect("publishable");
        assert!(!page.contains("<script>"), "{page}");
        assert!(!page.contains("<img"), "{page}");
        assert!(page.contains("&lt;script&gt;"), "it must still be readable");
    }

    /// The page carries how lopsided the honest set is.
    #[test]
    fn a_page_states_the_concentration_of_the_set_it_measured() {
        let h = reportable().overclaim_index().expect("reportable");
        let page = render(&h, &methodology()).expect("publishable");
        assert!(
            page.contains("largest honest-work family"),
            "a per-shape table with no concentration beside it can be read as \
             balanced when it is not: {page}"
        );
        assert!(page.contains("20.0%"), "{page}");
    }

    /// The page says plainly that the pooled rate is reconstructable.
    ///
    /// It is: `sum(accused) / sum(total)` over the table this page prints. No
    /// accessor offers it, which discourages it — and the page should not let a
    /// reader believe it is prevented.
    #[test]
    fn the_page_does_not_claim_a_pooled_rate_is_impossible() {
        let h = reportable().overclaim_index().expect("reportable");
        let page = render(&h, &methodology()).expect("publishable");
        assert!(
            page.contains("reconstructable"),
            "the page must not imply the pooled rate cannot be computed: {page}"
        );
    }

    #[test]
    fn the_ablation_table_shows_what_each_detector_was_carrying() {
        let m = reportable();
        let table = ablation_table(&m, &["narrowed_run"]);
        assert!(table.contains("(none)"), "{table}");
        assert!(table.contains("narrowed_run"), "{table}");
        // Removing the only detector drops the catch rate to zero, and the
        // table shows both rows so the difference is the ablation.
        assert!(
            table.contains("1.000") && table.contains("0.000"),
            "{table}"
        );
    }
}
