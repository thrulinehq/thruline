//! §9 amendment (M6.R2): the summary index is the contract, and *less* than it.
//!
//! L0 renders from this file alone, so what it may not carry matters as much as
//! what it must. These tests are the per-field privacy review made mechanical:
//! no ledger project id, no path, no raw command, no shadow class, no zero.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use thruline_core::derive::derive_project;
use thruline_core::export::{
    export_index, export_project, index_row, project_fixture_id, ExportOptions,
};
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;
use thruline_core::now::{now, Coverage};

const PROJ: &str = "-Users-dev-export";
const AT: &str = "2026-08-04T00:00:00Z";

fn built() -> (Value, Value) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    fs::create_dir_all(root.join(PROJ)).expect("mkdir");
    let body = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_code_sessions.jsonl"),
    )
    .expect("fixture");
    fs::write(
        root.join(PROJ)
            .join("00000000-0000-4000-8000-000000000000.jsonl"),
        body,
    )
    .expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    let graph = derive_project(&ledger, PROJ).expect("derive");
    let doc = export_project(&ledger, &graph, AT).expect("export");
    let queues = now(&ledger, None, false, Coverage::Complete).expect("now");
    let queue = queues
        .iter()
        .find(|q| q.project_id == PROJ)
        .expect("a queue for the project");
    let index = export_index(
        AT,
        ExportOptions::default(),
        vec![index_row(&doc, queue)],
        Some("2026-07-29"),
    );
    std::mem::forget(dir);
    (doc, index)
}

fn strings(v: &Value, path: &str, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                strings(x, &format!("{path}.{k}"), out);
            }
        }
        Value::Array(a) => {
            for x in a {
                strings(x, &format!("{path}[]"), out);
            }
        }
        Value::String(s) => out.push((path.to_string(), s.clone())),
        _ => {}
    }
}

#[test]
fn index_carries_exactly_the_amended_keys() {
    let (_doc, index) = built();
    let mut keys: Vec<&str> = index
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "contract_version",
            "diagnostics",
            "generated_at",
            "kind",
            "projects",
            "totals"
        ]
    );
    assert_eq!(index["kind"], "index");
    // A188 bumped this 2 → 3 when attention items gained the optional
    // `node_id`. Pinned to the CONSTANT rather than a literal: the emitted
    // document and the declared version must never be able to disagree, which a
    // hardcoded number here allowed. The version's meaning is documented on the
    // constant itself, and `a188_node_id.rs` asserts the widening it stands for.
    assert_eq!(
        index["contract_version"],
        thruline_core::export::INDEX_CONTRACT_VERSION
    );

    // The L0 headline is authored by the voice authority, not by the view.
    assert_eq!(
        index["totals"]["headline"],
        Value::String(thruline_core::voice::index_headline(
            index["totals"]["projects"].as_u64().expect("projects"),
            index["totals"]["needing_attention"]
                .as_u64()
                .expect("needing"),
        ))
    );
    assert_eq!(index["totals"]["newest_record"], "2026-07-29");
    // Nothing to date ⇒ the clause vanishes rather than reading "unknown".
    let empty = export_index(AT, ExportOptions::default(), Vec::new(), None);
    assert!(empty["totals"].get("newest_record").is_none());
    assert_eq!(empty["totals"]["headline"], "No records read yet.");

    let row = &index["projects"][0];
    let mut rk: Vec<&str> = row
        .as_object()
        .expect("row")
        .keys()
        .map(String::as_str)
        .collect();
    rk.sort_unstable();
    // The amendment's list: name, chip line, recency, attention. Plus the
    // fixture pointer and source. Nothing else — no sessions, no claims, no
    // events, no receipts, and no ledger project id.
    assert_eq!(
        rk,
        vec![
            "attention",
            "chip_line",
            "chips",
            "fixture",
            "name",
            "recency",
            "source"
        ]
    );
    for banned in ["sessions", "nodes", "edges", "receipts", "ledger", "id"] {
        assert!(
            row.get(banned).is_none(),
            "the index must not carry `{banned}` — L0 loads no project file"
        );
    }
}

#[test]
fn index_publishes_no_path_no_command_and_no_shadow_class() {
    let (_doc, index) = built();
    let mut found = Vec::new();
    strings(&index, "", &mut found);
    assert!(!found.is_empty());
    for (path, s) in &found {
        let lower = s.to_lowercase();
        assert!(
            !s.contains("/Users/") && !s.contains("/home/") && !s.contains("-Users-"),
            "{path} publishes a private absolute path: {s:?}"
        );
        // "no proof found" replaced "unproven" as the shadow word at C0.2; the
        // class stays shadow, so neither the slug nor the word may surface.
        for shadow in ["unsupported", "dropped", "no proof found", "abandoned"] {
            assert!(
                !lower.contains(shadow),
                "{path} names the shadow class `{shadow}`: {s:?}"
            );
        }
        for jargon in ["entailment", "fidelity", "matched pair", "nli"] {
            assert!(
                !lower.contains(jargon),
                "{path} leaks engine vocabulary `{jargon}`: {s:?}"
            );
        }
    }
    // The project's own ledger id is the one string that would carry the home
    // directory out; the fixture pointer replaces it and reveals nothing.
    assert_eq!(
        index["projects"][0]["fixture"],
        Value::String(project_fixture_id(PROJ))
    );
    assert_eq!(project_fixture_id(PROJ).len(), 16);
}

#[test]
fn index_chips_obey_the_zero_count_rule_and_carry_nouns() {
    let (doc, index) = built();
    let chips = index["projects"][0]["chips"].as_array().expect("chips");
    for chip in chips {
        assert!(
            chip["count"].as_u64().expect("count") > 0,
            "a zero-count chip rendered: {chip}"
        );
        let word = chip["word"].as_str().expect("word");
        assert!(!word.is_empty(), "a count without a noun is a naked number");
        assert!(chip["label"]
            .as_str()
            .expect("label")
            .starts_with(&word[..1].to_uppercase()));
    }
    // The chip line is the same list, composed once by the voice authority.
    let line = index["projects"][0]["chip_line"].as_str().expect("line");
    for chip in chips {
        assert!(
            line.contains(chip["word"].as_str().expect("word")),
            "chip line and chips disagree: {line:?}"
        );
    }
    // And it agrees with the project file that ships beside it.
    let counts = doc["counts"].as_object().expect("counts");
    for chip in chips {
        let word = chip["word"].as_str().expect("word");
        let slug = counts
            .iter()
            .find(|(k, _)| thruline_core::voice::verdict(k).is_some_and(|t| t.word == word))
            .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0)))
            .expect("every chip word maps back to a public count");
        assert_eq!(
            chip["count"].as_u64(),
            Some(slug.1),
            "count drift on {word}"
        );
    }
}

#[test]
fn index_is_byte_identical_across_two_builds() {
    let (_d1, a) = built();
    let (_d2, b) = built();
    assert_eq!(
        serde_json::to_string_pretty(&a).expect("a"),
        serde_json::to_string_pretty(&b).expect("b"),
        "the index must rebuild byte-identically"
    );
}

#[test]
fn index_row_order_is_part_of_the_contract() {
    let (doc, _index) = built();
    let queues_dummy = export_index(AT, ExportOptions::default(), Vec::new(), None);
    assert_eq!(queues_dummy["totals"]["projects"], 0);

    // Three rows, deliberately out of order, must land sorted by name.
    let mut rows = Vec::new();
    for name in ["zeta", "alpha", "Mid"] {
        let mut d = doc.clone();
        d["project"]["name"] = Value::String(name.to_string());
        d["project"]["id"] = Value::String(format!("-Users-dev-{name}"));
        rows.push(json_row(&d));
    }
    let index = export_index(AT, ExportOptions::default(), rows, None);
    let names: Vec<&str> = index["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .map(|r| r["name"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(names, vec!["alpha", "Mid", "zeta"]);
}

/// A row built without a live queue, for ordering assertions only.
fn json_row(doc: &Value) -> Value {
    serde_json::json!({
        "fixture": project_fixture_id(doc["project"]["id"].as_str().unwrap_or_default()),
        "name": doc["project"]["name"].clone(),
        "source": doc["project"]["source"].clone(),
        "chips": [],
        "chip_line": "",
        "attention": { "headline": "", "needing_attention": 0, "items": [] },
    })
}
