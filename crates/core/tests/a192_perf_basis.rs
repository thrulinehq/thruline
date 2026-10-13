//! P2b — the stamped-basis contract, in executable form.
//!
//! The rule these tests exist to enforce is one sentence: **a figure whose
//! basis has diverged renders "stale basis — re-measure" and cannot be quoted
//! as current.** Everything below is a way for that sentence to fail loudly.
//!
//! The reason it needs a test rather than a convention is on the record three
//! times over. CLAUDE.md asserted a twelve-minute cold export for three days
//! after A158 made it untrue. `perf-baseline.md` held a 44-project corpus size
//! and an inverted attribution while the corpus reached 90. A190 timed a query
//! in the `sqlite3` CLI against a warmed page cache and reported "~15 %" for
//! something that turned out to be 76 %. Every one of those was a person, or a
//! model, quoting a number in good faith from a document that had no way to
//! know it was stale.

use thruline_core::perf_basis::{
    render_doc_block, render_row, BasisCurrency, Observed, Registry, AXES, AXIS_QUERIES, STALE,
};

/// A minimal registry whose single basis matches [`observed`] exactly.
fn recoverable_registry() -> Registry {
    Registry::parse(
        r#"
version = 1

[[basis]]
id = "B-TEST"
recorded = "2026-08-16"
recoverable = true
projects = 90
events = 2012204
sessions = 76179
nodes = 87132
project_heads = 91
node_projections = 90
code_digest = "sha256:aaaaaaaaaaaaaaaa"
profile = "release"
ledger_bytes = 6847623168
commit = "deadbee"
machine = "a machine"

[[figure]]
id = "now-all"
label = "now --all"
basis = "B-TEST"
p50_ms = 39
p95_ms = 41
budget_ms = 100
"#,
    )
    .expect("the fixture registry must parse")
}

fn observed() -> Observed {
    Observed {
        projects: 90,
        events: 2_012_204,
        sessions: 76_179,
        nodes: 87_132,
        project_heads: 91,
        node_projections: 90,
        code_digest: "sha256:aaaaaaaaaaaaaaaa".into(),
        profile: "release".into(),
        ledger_bytes: Some(6_847_623_168),
        projects_table: 91,
    }
}

#[test]
fn a_matching_basis_is_current_and_the_figure_is_quotable() {
    let registry = recoverable_registry();
    let figure = &registry.figures[0];
    let currency = registry.currency(figure, &observed());
    assert_eq!(currency, BasisCurrency::Current);
    assert!(currency.quotable());
    let row = render_row(figure, &currency);
    assert!(row.contains("39 ms"), "{row}");
    assert!(row.contains("41 ms"), "{row}");
    assert!(row.contains("PASS"), "{row}");
    assert!(
        row.contains("B-TEST"),
        "the basis travels with the number: {row}"
    );
}

/// The heart of it. Perturb ONE axis at a time and require that the figure
/// stops being quotable and names the axis that moved.
///
/// Written as a loop over `AXES` on purpose: an axis added to the comparison
/// set without a way to perturb it fails here rather than silently never being
/// compared. A comparison that quietly skips an axis is the failure mode this
/// whole item exists to close.
#[test]
fn every_comparison_axis_diverges_on_its_own() {
    let registry = recoverable_registry();
    let figure = &registry.figures[0];
    for axis in AXES {
        let mut o = observed();
        match *axis {
            "projects" => o.projects += 1,
            "events" => o.events += 1,
            "sessions" => o.sessions += 1,
            "nodes" => o.nodes += 1,
            "project_heads" => o.project_heads += 1,
            "node_projections" => o.node_projections += 1,
            "code_digest" => o.code_digest = "sha256:bbbbbbbbbbbbbbbb".into(),
            "profile" => o.profile = "debug".into(),
            other => panic!(
                "axis {other} is compared but this test has no way to move it — \
                 add one, or it is being compared in name only"
            ),
        }
        let currency = registry.currency(figure, &o);
        assert!(
            !currency.quotable(),
            "moving {axis} left the figure quotable"
        );
        match &currency {
            BasisCurrency::Diverged(ds) => {
                assert_eq!(ds.len(), 1, "moving {axis} diverged {} axes", ds.len());
                assert_eq!(ds[0].axis, *axis);
            }
            other => panic!("moving {axis} produced {other:?}"),
        }
        let row = render_row(figure, &currency);
        assert!(row.contains(STALE), "{row}");
        assert!(row.contains(axis), "the row must name what moved: {row}");
    }
}

/// The numbers do not survive divergence anywhere on the row.
///
/// A stale row that still prints "39 ms" beside a warning is a stale row whose
/// number gets copied into a document by the next reader.
#[test]
fn a_stale_row_carries_no_numbers_at_all() {
    let registry = recoverable_registry();
    let figure = &registry.figures[0];
    let mut o = observed();
    o.events += 1;
    let row = render_row(figure, &registry.currency(figure, &o));
    for forbidden in ["39", "41", "PASS", "FAIL"] {
        assert!(
            !row.contains(forbidden),
            "a diverged row printed {forbidden}: {row}"
        );
    }
}

/// A figure whose value is prose — peak RSS in MiB, a head read in
/// microseconds — must still print that prose when its basis is current. An
/// earlier draft rendered those rows as three em-dashes and silently dropped
/// the only content they had.
#[test]
fn a_current_figure_with_no_millisecond_pair_still_prints_its_value() {
    let registry = Registry::parse(
        r#"
version = 1
[[basis]]
id = "B-TEST"
recorded = "2026-08-16"
recoverable = true
projects = 90
events = 2012204
sessions = 76179
nodes = 87132
project_heads = 91
node_projections = 90
code_digest = "sha256:aaaaaaaaaaaaaaaa"
profile = "release"
[[figure]]
id = "peak-rss"
label = "peak RSS"
basis = "B-TEST"
note = "15 MiB against a 1024 MiB budget."
"#,
    )
    .expect("parses");
    let figure = &registry.figures[0];
    let currency = registry.currency(figure, &observed());
    assert!(currency.quotable());
    let row = render_row(figure, &currency);
    assert!(row.contains("15 MiB"), "{row}");
}

/// Descriptive context is recorded and never compared. `ledger_bytes` is the
/// specimen: opening the ledger once, without writing a single corpus row,
/// moved it by 1.1 MB while this item was being built.
#[test]
fn descriptive_fields_never_decide_currency() {
    let registry = recoverable_registry();
    let figure = &registry.figures[0];
    let mut o = observed();
    o.ledger_bytes = Some(1);
    o.projects_table = 4242;
    assert_eq!(registry.currency(figure, &o), BasisCurrency::Current);
}

#[test]
fn an_unrecoverable_basis_can_never_be_current() {
    let registry = Registry::parse(
        r#"
version = 1
[[basis]]
id = "B-OLD"
recorded = "2026-08-06"
recoverable = false
note = "the conditions were never written down"
[[figure]]
id = "verify"
label = "verify"
basis = "B-OLD"
p50_ms = 21900
"#,
    )
    .expect("parses");
    let figure = &registry.figures[0];
    let currency = registry.currency(figure, &observed());
    assert_eq!(currency, BasisCurrency::Unrecoverable);
    assert!(!currency.quotable());
    let row = render_row(figure, &currency);
    assert!(row.contains(STALE), "{row}");
    assert!(!row.contains("21900"), "{row}");
}

#[test]
fn a_recoverable_basis_missing_an_axis_is_rejected() {
    let err = Registry::parse(
        r#"
version = 1
[[basis]]
id = "B-HALF"
recorded = "2026-08-16"
recoverable = true
projects = 90
events = 1
sessions = 1
nodes = 1
project_heads = 1
code_digest = "sha256:x"
profile = "release"
"#,
    )
    .expect_err("a recoverable basis missing node_projections must be refused");
    assert!(
        err.to_string().contains("node_projections"),
        "the error must name the missing axis: {err}"
    );
}

/// The "never assert two figures without named bases" clause.
///
/// Two readings of one measurement are allowed — they are often both true, of
/// two different corpora. What is forbidden is two readings that cannot be told
/// apart, which is what a shared basis means.
#[test]
fn two_figures_for_one_measurement_must_sit_on_different_bases() {
    let err = Registry::parse(
        r#"
version = 1
[[basis]]
id = "B-ONE"
recorded = "2026-08-16"
recoverable = false
[[figure]]
id = "verify"
label = "verify"
basis = "B-ONE"
p50_ms = 21900
[[figure]]
id = "verify"
label = "verify"
basis = "B-ONE"
p50_ms = 600000
"#,
    )
    .expect_err("two figures on one basis for one measurement must be refused");
    assert!(err.to_string().contains("unstamped assertion"), "{err}");
}

#[test]
fn a_figure_naming_a_basis_that_does_not_exist_is_rejected() {
    let err = Registry::parse(
        r#"
version = 1
[[figure]]
id = "now-all"
label = "now --all"
basis = "B-GHOST"
p50_ms = 39
"#,
    )
    .expect_err("a dangling basis reference must be refused");
    assert!(err.to_string().contains("B-GHOST"), "{err}");
}

/// A typo in a stamp is worse than a missing stamp, because it looks recorded.
#[test]
fn an_unknown_field_is_refused_rather_than_ignored() {
    let err = Registry::parse(
        r#"
version = 1
[[basis]]
id = "B-TYPO"
recorded = "2026-08-16"
recoverable = false
nodez = 87132
"#,
    )
    .expect_err("a misspelled axis must be refused, not silently dropped");
    assert!(err.to_string().contains("nodez"), "{err}");
}

// --- the committed registry and the committed document ---------------------

/// Every compared axis must have a way to be read. An axis in `AXES` with no
/// entry in `AXIS_QUERIES` would be compared against a value nothing produced.
#[test]
fn every_compared_axis_has_a_documented_reading() {
    let queried: Vec<&str> = AXIS_QUERIES.iter().map(|(a, _)| *a).collect();
    assert_eq!(
        AXES.to_vec(),
        queried,
        "AXES and AXIS_QUERIES must name the same axes in the same order"
    );
    for (axis, query) in AXIS_QUERIES {
        assert!(!query.is_empty(), "axis {axis} has no documented reading");
    }
}

#[test]
fn the_committed_registry_parses_validates_and_is_not_empty() {
    let registry = Registry::load().expect("docs/perf-basis.toml");
    assert!(
        !registry.figures.is_empty(),
        "a registry with no figures enforces nothing"
    );
}

/// `perf-baseline.md` used to hold the `verify` contradiction as two numbers in
/// running prose, left standing together with an apology. It is now two rows,
/// each stamped, which is the shape the ruling asked for.
#[test]
fn the_verify_contradiction_is_held_as_two_stamped_rows() {
    let registry = Registry::load().expect("docs/perf-basis.toml");
    let verify: Vec<_> = registry
        .figures
        .iter()
        .filter(|f| f.id == "verify")
        .collect();
    assert!(
        verify.len() >= 2,
        "the contradictory `verify` readings are all on the record, not reconciled away"
    );
    // The count is deliberately NOT pinned. An earlier version of this test
    // asserted exactly two and failed the moment a third reading was measured —
    // which is the registry working. What must hold is the property: every
    // reading of one measurement is distinguished by a named basis, never by
    // prose.
    let bases: std::collections::BTreeSet<&str> = verify.iter().map(|f| f.basis.as_str()).collect();
    assert_eq!(
        bases.len(),
        verify.len(),
        "two `verify` readings share a basis, which makes them indistinguishable"
    );
}

/// The document may not state a figure the registry does not know about, and
/// may not state one without naming its basis. Enforced the only way that
/// actually holds: the document carries the registry's own block, byte for
/// byte, between markers.
#[test]
fn perf_baseline_md_carries_the_generated_block_exactly() {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/perf-baseline.md"),
    )
    .expect("docs/perf-baseline.md");
    let registry = Registry::load().expect("docs/perf-basis.toml");
    let expected = render_doc_block(&registry);

    const BEGIN: &str = "<!-- BEGIN perf-figures (generated from docs/perf-basis.toml) -->";
    const END: &str = "<!-- END perf-figures -->";
    let start = doc
        .find(BEGIN)
        .unwrap_or_else(|| panic!("docs/perf-baseline.md must carry {BEGIN}"))
        + BEGIN.len();
    let end = doc
        .find(END)
        .unwrap_or_else(|| panic!("docs/perf-baseline.md must carry {END}"));
    let block = doc[start..end].trim_matches('\n');

    assert_eq!(
        block,
        expected.trim_matches('\n'),
        "docs/perf-baseline.md has drifted from docs/perf-basis.toml. \
         Regenerate the block; do not hand-edit it."
    );
}

/// Every basis id the prose mentions must exist. A document that cites
/// `B-A191` after someone renames the basis is a document quoting a figure it
/// cannot resolve.
#[test]
fn every_basis_id_cited_in_the_document_exists() {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/perf-baseline.md"),
    )
    .expect("docs/perf-baseline.md");
    let registry = Registry::load().expect("docs/perf-basis.toml");
    let mut cited = 0;
    for token in doc.split(|c: char| !(c.is_alphanumeric() || c == '-')) {
        if token.starts_with("B-") && token.len() > 2 {
            cited += 1;
            assert!(
                registry.basis(token).is_some(),
                "docs/perf-baseline.md cites basis {token}, which is not in the registry"
            );
        }
    }
    assert!(cited > 0, "the document cites no basis at all");
}
