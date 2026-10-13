//! C34 R-LEDGER (7), ruling Q6 condition 2 — **with zero `capture_holds` rows
//! every pause gate is a byte-for-byte no-op.**
//!
//! The gates live in six callers (the hook delivery, the Stop audit, the HQ load,
//! the first-use register, the Codex observer, `thruline ingest`). The ruling
//! lets them stand outside the frozen write path only if they provably do
//! nothing while no one has paused anything. This is that proof: ONE fixed
//! sequence — seven hook deliveries through the real `thruline sensor append`
//! (one of them a PreCompact, which seals a transcript), then `thruline ingest`
//! of a Claude Code project and of a Codex rollout, in a temporary HOME — is
//! run, and the world it leaves is digested: every table row (`events`,
//! `sensor_events`, the chain heads, everything else the ledger holds), the
//! event chain's head per session, the sensor chain, every mirror file's bytes,
//! and the names of the files beside the ledger.
//!
//! The expected digest, `tests/fixtures/capture_gates_noop_base.json`, was
//! produced by THE BASE CODE (commit 20f988d9, before any gate existed), plus
//! only migration 170's DDL from d96b5705, by running this very file there with
//! `THRULINE_WRITE_NOOP_FIXTURE=<path>`. The C51 refresh changes only the empty
//! `project_removals` table and schema digest; every other fixture byte stays
//! identical. The code under test must reproduce it exactly.
//!
//! WHAT IS NORMALISED, and nothing else: see `digest::NORMALISED` (it is also
//! written into the fixture). The temp directory's own path is replaced by
//! `<TMP>`. `the_digest_is_stable_across_two_runs_of_the_same_sequence` proves
//! the list is complete: two runs of the sequence must digest identically, so a
//! volatile column that is not on the list fails that test, loudly, instead of
//! being quietly tolerated here.
//!
//! No real store is opened: every process has HOME and THRULINE_HOME inside the
//! temp dir.

mod capture_gates_support;

use capture_gates_support::digest::{differing_columns, digest, Captured};
use capture_gates_support::{fixed_lock, run_sequence, World, APP};
use serde_json::Value;

const FIXTURE: &str = "tests/fixtures/capture_gates_noop_base.json";

/// Build the fixed world, run the sequence, digest it, and take the world down.
fn run_world() -> Captured {
    let w = World::fixed();
    for (what, out) in run_sequence(&w, APP) {
        assert!(
            out.status.success(),
            "{what} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if what.starts_with("sensor append") {
            assert!(
                out.stdout.is_empty(),
                "{what}: a hook prints nothing on stdout"
            );
        }
    }
    assert_eq!(
        w.count("capture_holds"),
        0,
        "this world has no pause and no exclusion"
    );
    let d = digest(&w);
    w.clean();
    d
}

fn tables_of(d: &Value) -> &serde_json::Map<String, Value> {
    d["tables"].as_object().expect("tables")
}

#[test]
fn the_digest_is_stable_across_two_runs_of_the_same_sequence() {
    let _one_world_at_a_time = fixed_lock();
    let a = run_world();
    // Every clock column in the ledger has one-second resolution; two runs in
    // the same second would hide one from this test. Run the second after one.
    std::thread::sleep(std::time::Duration::from_millis(1_100));
    let b = run_world();
    let differing = differing_columns(&a, &b);
    assert!(
        differing.is_empty(),
        "these columns differ between two runs of the same sequence, so digest::NORMALISED \
         must name them: {differing:?}"
    );
    assert!(
        a.summary == b.summary,
        "two runs of the same sequence digest differently: {:#?}",
        capture_gates_support::digest::diff(&a.summary, &b.summary)
    );
}

#[test]
fn with_no_holds_every_gate_leaves_the_world_exactly_as_the_base_left_it() {
    let _one_world_at_a_time = fixed_lock();
    let got = run_world().summary;
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    if let Some(out) = std::env::var_os("THRULINE_WRITE_NOOP_FIXTURE") {
        std::fs::write(
            &out,
            serde_json::to_string_pretty(&got).expect("json") + "\n",
        )
        .expect("write the fixture");
        return;
    }
    let want: Value = serde_json::from_str(
        &std::fs::read_to_string(&fixture)
            .unwrap_or_else(|e| panic!("the base digest {} is missing: {e}", fixture.display())),
    )
    .expect("the fixture is JSON");

    // Table by table first, so a failure names the table.
    for (table, summary) in tables_of(&want) {
        assert_eq!(
            Some(summary),
            tables_of(&got).get(table),
            "table {table} is not what the base wrote"
        );
    }
    assert_eq!(
        tables_of(&got).len(),
        tables_of(&want).len(),
        "the set of tables moved"
    );
    assert_eq!(
        got["event_chain_heads"], want["event_chain_heads"],
        "an event chain head moved"
    );
    assert_eq!(
        got["sensor_chain"], want["sensor_chain"],
        "the sensor chain moved"
    );
    assert_eq!(got["mirror"], want["mirror"], "a mirror file moved");
    assert_eq!(
        got["beside_the_ledger"], want["beside_the_ledger"],
        "a file appeared beside the ledger"
    );
    assert_eq!(got, want);
}
