//! A209-C6 — round-6 findings, each pinned END-TO-END on the real path.
//!
//! The round-7 prohibition, which this file exists to obey: **every
//! control exercises the real path — `compile`, and for consent the actual
//! gate — not a locally-held value; and every control was verified FAILING
//! against `90806e0` before its fix landed.** Seven consecutive rounds
//! produced guards that could not detect their own defect, most recently a
//! consent-gate test that asserted on a `MaskRecord` the test itself held,
//! which by construction cannot see a record failing to REACH the
//! treatment.

use thruline_core::capsule_v2::{self, CompileContext};
use thruline_core::ledger::Ledger;
use thruline_core::redact::{mask_recorded, Treatment};

const B64: &str = "MIIEowIBAAKCAQEATLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ffUU66gg77";
const NOW: &str = "2026-09-01T12:00:00Z";

/* ── B8-1: the regression introduced at e7fd058 ─────────────────────── */

/// The `[A-Z ]`-charset + `ends_with` label rule rejected the RFC 4880
/// armor label `gpg --export-secret-keys --armor` emits — the single most
/// common way a private key appears in a transcript — plus every label
/// carrying a digit. All six masked correctly at `0460a9e` and `9796f49`.
#[test]
fn b8_1_exotic_but_real_key_labels_still_mask() {
    for label in [
        "PGP PRIVATE KEY BLOCK",
        "PRIVATE KEY BLOCK",
        "RSA PRIVATE KEY BLOCK",
        "SSH2 ENCRYPTED PRIVATE KEY",
        "X509 PRIVATE KEY",
        "VERY LONG ENCRYPTED RSA PRIVATE KEY FOR SERVICE",
    ] {
        let input = format!("-----BEGIN {label}-----\n{B64}\n-----END {label}-----");
        let (out, record) = mask_recorded(&input);
        assert!(
            record.families.contains("private_key_block"),
            "label {label:?} did not mask: {out}"
        );
        assert!(!out.contains(B64), "body shipped for {label:?}: {out}");
    }
}

/// B8-3: the discriminator is the BODY, not the label prose. All 37 live
/// fires were search commands, comments and test files; not one had a
/// base64-shaped line after the banner.
#[test]
fn b8_3_a_banner_without_a_base64_body_is_not_masked() {
    for prose in [
        "grep -rn -- '-----BEGIN PRIVATE KEY-----' src/",
        "// the file starts with -----BEGIN RSA PRIVATE KEY----- and is PEM",
        "-----BEGIN PRIVATE KEY----- is what a PEM header looks like",
        "assert!(text.contains(\"-----BEGIN OPENSSH PRIVATE KEY-----\"));",
        "-----BEGIN CHANGELOG----- notes about PRIVATE KEY handling -----END CHANGELOG----- keep this prose",
    ] {
        let (out, record) = mask_recorded(prose);
        assert_eq!(record.masked, 0, "prose masked as a key: {prose:?} -> {out}");
        assert_eq!(out, prose, "content destroyed: {out}");
    }
}

/* ── B8-4: the END banner must be a banner, not a mention ───────────── */

#[test]
fn b8_4_a_mentioned_end_marker_does_not_close_the_span() {
    for input in [
        format!("-----BEGIN RSA PRIVATE KEY-----\n{B64}\n-----END-----\nTAIL_MATTERS"),
        format!(
            "-----BEGIN RSA PRIVATE KEY-----\nthe block ends with -----END RSA PRIVATE KEY-----\n{B64}\nTAIL"
        ),
    ] {
        let (out, record) = mask_recorded(&input);
        assert!(record.families.contains("private_key_block"), "{out}");
        assert!(
            !out.contains(B64),
            "the body shipped while the treatment reported a mask: {out}"
        );
    }
}

/* ── B8-2 / B8-6: the record must REACH the treatment, on the real path ─ */

/// A scope whose only credential lives in a claim node's label — the
/// `derive` → `nodes.label` → `claim_rows` → category path that has now
/// failed three reviews. The test calls the production compiler; a test
/// holding a `MaskRecord` locally could never see this.
fn seeded_with_credential_in_a_node_label() -> (Ledger, String) {
    let ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
               VALUES ('-tmp-b8','/tmp/b8','claude_code','b8','2026-09-01T00:00:00Z');
             INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
               VALUES ('s1','-tmp-b8','/t/s1.jsonl','2026-09-01T00:00:00Z',1);
             INSERT INTO workspace_identities(project_id, repo_id, basis, observed_at)
               VALUES ('-tmp-b8','repo_b8','git','2026-09-01T00:00:00Z');
             INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                                raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
               VALUES ('e1','-tmp-b8','s1',0,'2026-09-01T00:00:01Z','assistant_text',
                       'done','/t/s1.jsonl',1,'x','g','c1');
             INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                               currency, session_id, anchor_event_id, derived_rev)
               VALUES ('n1','-tmp-b8','deployed with [masked: aws] credentials','supported',
                       'completed','supported','current','s1','e1',1);",
        )
        .expect("seed");
    (ledger, "-tmp-b8".to_string())
}

/// B8-2: a label that ARRIVED already masked upstream must not render as
/// "nothing matched". Either the record reaches the treatment, or the
/// treatment says UNKNOWN — a confident zero over masked content is the
/// false-treatment fault this stage has graded blocking three times.
#[test]
fn b8_2_a_masked_node_label_never_yields_a_confident_zero() {
    let (ledger, project) = seeded_with_credential_in_a_node_label();
    let capsule =
        capsule_v2::compile(&ledger, &project, &CompileContext::default(), NOW).expect("compile");
    // The masked marker really is in the rendered artifact.
    let json = serde_json::to_string(&capsule).expect("json");
    assert!(
        json.contains("[masked: aws]"),
        "the fixture did not reach the capsule: {json}"
    );
    match &capsule.redaction.treatment {
        Treatment::Ran {
            masked, patterns, ..
        } => {
            assert!(
                *masked > 0 && patterns.iter().any(|p| p == "aws"),
                "the capsule renders a masked span while its treatment reports \
                 nothing matched: masked={masked} patterns={patterns:?}"
            );
        }
        Treatment::Unknown { .. } => { /* honest: provenance unavailable */ }
        other => panic!("no detector accounted for a masked document: {other:?}"),
    }
    // Either way the consent gate must be closed.
    assert!(
        capsule.redaction.treatment.needs_acknowledgement(),
        "the consent gate opened over a document containing a masked span"
    );
}

/// B8-6: `Treatment::Unknown` must be reachable from a real compile, not
/// only by hand-construction in a test.
#[test]
fn b8_6_unknown_provenance_is_reachable_from_production_code() {
    let src =
        std::fs::read_to_string(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"))
            .err();
    assert!(src.is_some(), "sanity: src is a directory");
    // Count production (non-test) callers of MaskRecord::unknown().
    let mut callers = 0usize;
    for entry in std::fs::read_dir(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"))
        .expect("read src")
    {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read");
        let body = text.split("#[cfg(test)]").next().unwrap_or("");
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name == "redact.rs" {
            continue; // the definition itself
        }
        callers += body.matches("MaskRecord::unknown()").count()
            + body.matches("Masked::unknown(").count();
    }
    assert!(
        callers > 0,
        "unknown provenance has no production caller — Treatment::Unknown \
         is a pillar that does not exist"
    );

    // And it is reachable from a real compile, not only by construction:
    // a capsule whose label arrived already masked renders UNKNOWN.
    let (ledger, project) = seeded_with_credential_in_a_node_label();
    let capsule =
        capsule_v2::compile(&ledger, &project, &CompileContext::default(), NOW).expect("compile");
    assert!(
        matches!(capsule.redaction.treatment, Treatment::Unknown { .. }),
        "a capsule built from an upstream-masked label did not render UNKNOWN: {:?}",
        capsule.redaction.treatment
    );
    assert!(capsule.redaction.treatment.needs_acknowledgement());
}

/// The shapes that survived two intermediate attempts at the body gate,
/// found by re-measuring the live ledger rather than by review: shell
/// scripts and regex patterns whose LATER lines happen to contain a long
/// base64-ish run. PEM grammar puts the payload immediately after the
/// banner, past encapsulated headers and blanks only.
#[test]
fn b8_3b_a_script_mentioning_a_banner_is_not_a_key() {
    for script in [
        "if rg -n -- '-----BEGIN PRIVATE KEY-----' .; then exit 1; fi\n\
         if rg -n '^\\s*schedule:' .github/workflows; then exit 1; fi\n\
         SHA=da39a3ee5e6b4b0d3255bfef95601890afd80709aabbccddeeff0011\n",
        "pattern='-----BEGIN ([A-Z0-9 ]+ )?PRIVATE KEY-----'\n\
         developer_path_pattern='(^|[^A-Za-z0-9._-])/(Users|home)/[[:alnum:]_.-]+/'\n\
         digest=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n",
    ] {
        let (out, record) = mask_recorded(script);
        assert_eq!(
            record.masked, 0,
            "a script mentioning a banner was masked as a key: {out}"
        );
        assert_eq!(out, script, "content destroyed");
    }
}
