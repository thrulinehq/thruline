//! A209-C8 — transport encodings, as a first-class corpus dimension.
//!
//! Round 8 generalized on backslash DEPTH (`\n` vs `\\n` vs `\\\\n`). The axis
//! that mattered is **which escape denotes a line break**, and more deeply:
//! a scanner that parses transport encodings inline, one letter at a time,
//! reopens this class under every new letter. Round 7 broke it with
//! `split_whitespace`, round 8 fixed depth but not letters, and
//! percent-encoding was never considered at all.
//!
//! So the dimension is now enumerated rather than sampled: every must-mask
//! shape is re-run under LF, CRLF, escaped-CRLF at each depth, tab
//! separation, and percent-encoding. Each control below was verified
//! FAILING against `eb47eae` before the decoder landed.

mod keymat;

use keymat::{surviving_fragment, REAL_RSA_KEY};
use thruline_core::capsule_v2::{self, CompileContext};
use thruline_core::ledger::Ledger;
use thruline_core::redact::{mask_recorded, Treatment};

/// The distinct transports a key arrives in. Each is a *rendering* of the
/// same key; every one must mask.
fn transports() -> Vec<(&'static str, String)> {
    let lf = REAL_RSA_KEY.to_string();
    let crlf = lf.replace('\n', "\r\n");
    vec![
        ("raw LF", lf.clone()),
        ("raw CRLF", crlf.clone()),
        ("json-lf", lf.replace('\n', "\\n")),
        ("json-crlf", lf.replace('\n', "\\r\\n")),
        ("json-crlf-2x", lf.replace('\n', "\\\\r\\\\n")),
        ("json-lf-2x", lf.replace('\n', "\\\\n")),
        ("json-tab", lf.replace('\n', "\\t")),
        ("percent-lf", lf.replace('\n', "%0A")),
        ("percent-crlf", lf.replace('\n', "%0D%0A")),
        ("percent-tab", lf.replace('\n', "%09")),
    ]
}

/// EVERY payload line, not the first three. A209-C11: the `.take(3)` this
/// replaces is half of why two round-11 regressions were invisible — a
/// shipped FINAL wrap is by definition not among the first three lines.
fn key_fragments() -> Vec<String> {
    REAL_RSA_KEY
        .lines()
        .filter(|l| l.len() >= 60)
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn every_transport_encoding_of_a_real_key_is_masked() {
    let frags = key_fragments();
    assert!(!frags.is_empty(), "fixture has no payload lines");
    let mut failures = Vec::new();
    for (name, encoded) in transports() {
        let (out, record) = mask_recorded(&encoded);
        if !record.families.contains("private_key_block") {
            failures.push(format!("{name}: no detection"));
            continue;
        }
        for f in &frags {
            // The fragment as it appears in THIS transport.
            if out.contains(f.as_str()) {
                failures.push(format!("{name}: key material survived"));
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} transports leaked a real RSA-2048 key:\n{}",
        failures.len(),
        transports().len(),
        failures.join("\n")
    );
}

/// The service-account file shape, per transport. A209-C7's live-proof
/// claim — that this renders `[masked: private_key_block]` — is true only
/// for LF, and that is retracted in the register.
#[test]
fn a_service_account_file_masks_in_every_transport() {
    let mut failures = Vec::new();
    for (name, encoded) in transports() {
        let doc = format!(
            r#"{{"type":"service_account","private_key":"{}"}}"#,
            encoded.replace('"', "'")
        );
        let (out, record) = mask_recorded(&doc);
        if !record.families.contains("private_key_block") {
            failures.push(format!("{name}: no detection"));
        } else if let Some(f) = surviving_fragment(&out) {
            failures.push(format!("{name}: {f}"));
        }
    }
    assert!(
        failures.is_empty(),
        "service-account key leaked in transports: {failures:?}"
    );
}

/// End-to-end on the real path: a key in any transport must never reach a
/// persisted capsule, and must never leave the consent gate open.
#[test]
fn no_transport_puts_a_key_in_a_sealed_capsule_with_the_gate_open() {
    for (name, encoded) in transports() {
        let ledger = Ledger::open_in_memory().expect("ledger");
        ledger
            .conn()
            .execute_batch(
                "INSERT INTO projects(id, root_path, source, display_name, created_at)
                   VALUES ('-t','/tmp/t','claude_code','t','2026-09-01T00:00:00Z');
                 INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
                   VALUES ('s1','-t','/t/s1.jsonl','2026-09-01T00:00:00Z',1);
                 INSERT INTO workspace_identities(project_id, repo_id, basis, observed_at)
                   VALUES ('-t','repo_t','git','2026-09-01T00:00:00Z');",
            )
            .expect("seed");
        ledger
            .conn()
            .execute(
                "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                                    raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                 VALUES ('e1','-t','s1',0,'2026-09-01T00:00:01Z','user_msg',?1,
                         '/t/s1.jsonl',1,'x','g','c1')",
                [format!("rotate this key: {encoded}")],
            )
            .expect("event");
        let capsule = capsule_v2::compile(
            &ledger,
            "-t",
            &CompileContext::default(),
            "2026-09-01T12:00:00Z",
        )
        .expect("compile");
        let json = serde_json::to_string(&capsule).expect("json");
        for f in key_fragments() {
            assert!(
                !json.contains(f.as_str()),
                "{name}: a real RSA-2048 key reached the persisted capsule"
            );
        }
        assert!(
            surviving_fragment(&json).is_none(),
            "{name}: key material reached the persisted capsule: {:?}",
            surviving_fragment(&json)
        );
        let gate_open = !capsule.redaction.treatment.needs_acknowledgement();
        let says_nothing_matched = matches!(
            &capsule.redaction.treatment,
            Treatment::Ran { masked: 0, .. }
        );
        assert!(
            !(gate_open && says_nothing_matched),
            "{name}: the consent gate opened over a capsule built from a real key"
        );
    }
}
