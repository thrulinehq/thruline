//! C0.4 — the truthful adapter capability matrix, generated from tests.
//!
//! The blueprint's rule: "truthful per-adapter capability manifests" — a
//! capability may be advertised only at the strength a test actually proves.
//! This file is the single source: every SUPPORTED cell names the test that
//! proves it, `every_supported_cell_cites_a_test_that_exists` verifies each
//! citation mechanically, and `the_committed_matrix_matches_the_generated_one`
//! holds `docs/ADAPTER-CAPABILITIES.md` byte-identical to what these rows
//! render — the doc cannot drift from the tests without failing the suite.
//!
//! Regenerate: THRULINE_BLESS_MATRIX=1 cargo test -p thruline-core --test adapter_matrix

use std::fs;
use std::path::PathBuf;

/// What one adapter can do for one capability, at tested strength.
enum Cell {
    /// Proven by the named test: `"<path relative to crates/core>::<fn>"`.
    Supported(&'static str),
    /// Not supported, with the honest reason. Never a logo, never a dash.
    NotSupported(&'static str),
    /// Scheduled work with a checklist home. Advertised as "not yet supported".
    Planned(&'static str),
}

struct Row {
    capability: &'static str,
    claude_code: Cell,
    codex: Cell,
    gemini: Cell,
}

/// The matrix. Rows are capabilities, cells are per-adapter tested strength.
const ROWS: &[Row] = &[
    Row {
        capability: "Transcript ingest — vendor records parsed to hash-chained events",
        claude_code: Cell::Supported("tests/parse_fixtures.rs::normalizes_the_twelve_real_specimens"),
        codex: Cell::Supported(
            "tests/codex_mapping.rs::committed_synthetic_fixture_is_grammar_preserving_and_private_free",
        ),
        gemini: Cell::NotSupported("no adapter exists; the lane reads nothing (A154)"),
    },
    Row {
        capability: "Idempotent re-ingest — append-only, exactly-once",
        claude_code: Cell::Supported(
            "tests/ingest_idempotency.rs::re_ingest_adds_nothing_and_leaves_the_head_untouched",
        ),
        codex: Cell::Supported("tests/codex_atomic_ingest.rs::g1_identical_and_prefix_g2_are_exactly_once"),
        gemini: Cell::NotSupported("no adapter exists"),
    },
    Row {
        capability: "Byte preservation — the source is sealed into the mirror before any parse",
        claude_code: Cell::Supported("tests/rb15_mirror_parity.rs::every_vendor_ingest_path_seals_before_parse"),
        codex: Cell::Supported("tests/rb15_mirror_parity.rs::every_vendor_ingest_path_seals_before_parse"),
        gemini: Cell::NotSupported("no adapter exists"),
    },
    Row {
        capability: "Vendor-sweep survival — verify still passes after the vendor deletes the source",
        claude_code: Cell::Supported(
            "tests/rb15_mirror_parity.rs::a_swept_source_file_still_verifies_through_its_sealed_generation",
        ),
        codex: Cell::Supported(
            "tests/codex_atomic_ingest.rs::accepted_mirror_and_receipts_survive_source_move_and_disappearance",
        ),
        gemini: Cell::NotSupported("no adapter exists"),
    },
    Row {
        capability: "Repo identity — projects grouped by repository, across vendors",
        claude_code: Cell::Supported("tests/workspace_identity.rs::a_repo_cloned_to_two_paths_has_one_identity"),
        codex: Cell::Supported(
            "tests/a129_repo_scope.rs::a_path_resolves_to_its_repo_and_spans_every_project_in_it",
        ),
        gemini: Cell::NotSupported("no adapter exists"),
    },
    Row {
        capability: "Session status — shadow diagnostics derived at read, never a rendered verdict",
        claude_code: Cell::Supported("src/session_status.rs::vendor_terminal_markers_close_a_session"),
        codex: Cell::Supported("src/session_status.rs::a_codex_abort_is_interrupted_on_the_vendors_word"),
        gemini: Cell::NotSupported("no adapter exists"),
    },
    Row {
        capability: "Assured Capture — execution-boundary proof for attested runs",
        claude_code: Cell::NotSupported(
            "no qualified command profile; bounded observations only (A78 readiness states)",
        ),
        codex: Cell::Supported("tests/attested_run.rs::a_genuine_attested_run_verifies_a_matching_success_claim"),
        gemini: Cell::NotSupported("no adapter exists"),
    },
    Row {
        capability: "Lifecycle hook sensor — session/prompt/tool/stop/compaction events captured live, verbatim, chain-hashed",
        claude_code: Cell::Supported(
            "tests/c12_sensor_pack.rs::every_pinned_event_appends_with_its_spine_and_verbatim_payload",
        ),
        codex: Cell::Planned("aborts and completions are read from rollouts at ingest today, not sensed live"),
        gemini: Cell::NotSupported("no adapter exists"),
    },
    Row {
        capability: "Recovery injection — capsule delivered into a fresh session at start",
        claude_code: Cell::Planned("C4; the SessionStart injection surface is probed in C1"),
        codex: Cell::Planned("C4; clipboard/file fallback until a native surface is qualified"),
        gemini: Cell::NotSupported("no adapter exists"),
    },
];

fn cell_text(cell: &Cell) -> String {
    match cell {
        Cell::Supported(test) => format!("**Supported** — `{test}`"),
        Cell::NotSupported(reason) => format!("**Not supported** — {reason}"),
        Cell::Planned(reason) => format!("**Not yet supported** — planned: {reason}"),
    }
}

/// The whole document. Prose lives here so the doc and the rows cannot drift.
fn render() -> String {
    let mut out = String::from(
        "# ThruLine — Adapter Capabilities (generated from tests)\n\n\
         <!-- GENERATED by crates/core/tests/adapter_matrix.rs — do not hand-edit.\n\
         \x20    Regenerate: THRULINE_BLESS_MATRIX=1 cargo test -p thruline-core --test adapter_matrix -->\n\n\
         **A capability appears here at the strength a test proves, and no stronger.** Every\n\
         *Supported* cell names the test that proves it; the suite verifies each citation exists\n\
         and holds this file byte-identical to the generating rows. Vendor record formats are\n\
         officially unstable, so adapters are versioned and these contracts re-run per release.\n\n\
         | Capability | Claude Code | Codex | Gemini |\n|---|---|---|---|\n",
    );
    for row in ROWS {
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            row.capability,
            cell_text(&row.claude_code),
            cell_text(&row.codex),
            cell_text(&row.gemini),
        ));
    }
    out.push_str(
        "\nCross-vendor handoff — one repo spanning both launch adapters — is walked end to end\n\
         by `tests/l8_handoff_demo.rs` (`cargo test --test l8_handoff_demo -- --nocapture`).\n",
    );
    out
}

fn core_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn every_supported_cell_cites_a_test_that_exists() {
    let mut checked = 0usize;
    for row in ROWS {
        for cell in [&row.claude_code, &row.codex, &row.gemini] {
            let Cell::Supported(citation) = cell else {
                continue;
            };
            let (file, func) = citation
                .split_once("::")
                .unwrap_or_else(|| panic!("citation {citation:?} is not file::fn"));
            let path = core_root().join(file);
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cited file {} is unreadable: {e}", path.display()));
            assert!(
                source.contains(&format!("fn {func}(")),
                "{file} does not define the cited test `{func}` — the matrix is advertising \
                 a capability nothing proves"
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 10,
        "only {checked} citations checked — rows went missing"
    );
}

#[test]
fn the_committed_matrix_matches_the_generated_one() {
    let path = core_root().join("../../docs/ADAPTER-CAPABILITIES.md");
    let on_disk = fs::read_to_string(&path).expect("read docs/ADAPTER-CAPABILITIES.md");
    assert_eq!(
        on_disk,
        render(),
        "\ndocs/ADAPTER-CAPABILITIES.md has drifted from the matrix rows.\n\
         The rows are the authority. Regenerate with:\n\
           THRULINE_BLESS_MATRIX=1 cargo test -p thruline-core --test adapter_matrix\n"
    );
}

#[test]
fn bless_the_matrix() {
    // The generator, gated so a normal run never writes.
    if std::env::var("THRULINE_BLESS_MATRIX").as_deref() != Ok("1") {
        return;
    }
    let path = core_root().join("../../docs/ADAPTER-CAPABILITIES.md");
    fs::write(&path, render()).expect("write docs/ADAPTER-CAPABILITIES.md");
}
