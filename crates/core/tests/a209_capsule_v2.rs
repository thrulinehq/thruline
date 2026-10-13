//! A209 — the canonical Recovery Capsule v2 (MANDATE-v4 §15, §32, §33, §34).
//!
//! The TLCAP_V2 canonical layout is pinned by python3 hashlib vectors
//! computed BEFORE the Rust encoder existed (two independent assemblies
//! agreeing — the a205–a208 discipline). The §34 gates proven here at 100%:
//! capsule tamper detection (per hashed-field family), stable
//! unresolved-item transfer (deterministic, rewording-invariant,
//! collision-tested), pre-persistence redaction (with the forced-failure
//! quarantine), and no continuity claim from any v2 surface. Self-citation
//! is shown structurally impossible; the five §15 categories stay separate
//! arrays end-to-end; sealed means sealed — a later source event produces a
//! new capsule with lineage while the old bytes verify unchanged.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;
use thruline_core::capsule_v2::{
    self, canonical_bytes, capsule_id, obligation_id, AgentAssertion, CapsuleRecordOutcome,
    CapsuleV2, CompileContext, CompletenessV2, ContinuationRef, CutoffV2, DecisionLine, EmptyDecl,
    EstablishedFact, FailureLine, GeneratorV2, IncompleteMarker, MeasurementV2, OpenObligation,
    Recommendation, RedactionV2, RepositoryV2, ScopeV2, SourceV2, TaskLine, TrimDisclosure,
    UnknownItem, WorkspaceV2, ADVISORY_NO_PROOF, QUARANTINE_TEXT,
};
use thruline_core::ledger::Ledger;
use thruline_core::observation::{ReceiptTerminal, SourceReceipt};
use thruline_core::protected_store::ProtectedStore;
use thruline_core::redact::Treatment;

const NOW: &str = "2026-09-01T12:00:00Z";

/// One in-place JSON edit, used by the tamper and injection walks.
type Mutation = Box<dyn Fn(&mut Value)>;

/* ── fixtures ───────────────────────────────────────────────────────── */

/// A scope with an opening ask, claims in all three states, a failed
/// side-effect command, an unresolved edit run, an overflowed tool result,
/// and a resolving repository identity.
fn seeded() -> (Ledger, String) {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let project = "-tmp-a209-project".to_string();
    let overflow = "{\"content\":\"<persisted-output>\\nOutput too large (85KB). Full output \
                    saved to: /u/.claude/projects/-p/s1/tool-results/toolu_01.txt\\n\\nPreview \
                    (first 2KB):\\nx\\n</persisted-output>\"}";
    ledger
        .conn()
        .execute_batch(&format!(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
               VALUES ('-tmp-a209-project','/tmp/a209','claude_code','a209','2026-09-01T00:00:00Z');
             INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
               VALUES ('s1','-tmp-a209-project','/t/s1.jsonl','2026-09-01T00:00:00Z',7);
             INSERT INTO workspace_identities(project_id, repo_id, basis, observed_at)
               VALUES ('-tmp-a209-project','repo_a209','git','2026-09-01T00:00:00Z');
             INSERT INTO events(id, project_id, session_id, seq, ts, kind, text, tool_name,
                                tool_use_id, tool_input, tool_result, is_error,
                                raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
               VALUES
                ('e1','-tmp-a209-project','s1',0,'2026-09-01T00:00:01Z','user_msg',
                 'Make the digest work — key AKIAIOSFODNN7EXAMPLE is in env',NULL,NULL,NULL,
                 NULL,0,'/t/s1.jsonl',1,'x','g','c1'),
                ('e2','-tmp-a209-project','s1',1,'2026-09-01T00:00:02Z','tool_use',NULL,'Bash',
                 'tu1','{{\"command\":\"supabase functions deploy hook\"}}',NULL,0,
                 '/t/s1.jsonl',2,'x','c1','c2'),
                ('e3','-tmp-a209-project','s1',2,'2026-09-01T00:00:03Z','tool_result',NULL,NULL,
                 'tu1',NULL,'deploy failed',1,'/t/s1.jsonl',3,'x','c2','c3'),
                ('ec','-tmp-a209-project','s1',3,'2026-09-01T00:00:04Z','tool_use',NULL,'Bash',
                 'tuc','{{\"command\":\"git commit -m seed\"}}',NULL,0,
                 '/t/s1.jsonl',4,'x','c3','cc'),
                ('e4','-tmp-a209-project','s1',4,'2026-09-01T00:00:05Z','tool_use',NULL,'Edit',
                 'tu2','{{\"file_path\":\"src/lib.rs\"}}',NULL,0,'/t/s1.jsonl',5,'x','cc','c4'),
                ('e5','-tmp-a209-project','s1',5,'2026-09-01T00:00:06Z','tool_use',NULL,'Edit',
                 'tu3','{{\"file_path\":\"src/lib.rs\"}}',NULL,0,'/t/s1.jsonl',6,'x','c4','c5'),
                ('e6','-tmp-a209-project','s1',6,'2026-09-01T00:00:07Z','tool_result',NULL,NULL,
                 'tu9',NULL,'{overflow}',0,'/t/s1.jsonl',7,'x','c5','c6'),
                ('e7','-tmp-a209-project','s1',7,'2026-09-01T00:00:08Z','assistant_text',
                 'Deployed and done',NULL,NULL,NULL,NULL,0,'/t/s1.jsonl',8,'x','c6','c7');
             INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                               currency, session_id, anchor_event_id, derived_rev)
               VALUES ('n1','-tmp-a209-project','digest collapses bursts','supported','completed',
                       'supported','current','s1','e7',1),
                      ('n2','-tmp-a209-project','something unreadable','unknown','active',
                       'unknown','current','s1','e7',1),
                      ('n3','-tmp-a209-project','deployed to production','unsupported','completed',
                       'unknown','current','s1','e7',1);"
        ))
        .expect("seed");
    (ledger, project)
}

fn compile(ledger: &Ledger, project: &str) -> CapsuleV2 {
    capsule_v2::compile(ledger, project, &CompileContext::default(), NOW).expect("compile")
}

fn tr(file: &str, line: u64) -> SourceReceipt {
    SourceReceipt::new(ReceiptTerminal::Transcript {
        file: file.to_string(),
        line,
    })
}

fn pe(command: &str, exit_code: i64) -> SourceReceipt {
    SourceReceipt::new(ReceiptTerminal::ProcessExecution {
        command: command.to_string(),
        exit_code: Some(exit_code),
    })
}

fn sm(digest: &str) -> SourceReceipt {
    SourceReceipt::new(ReceiptTerminal::SnapshotManifest {
        manifest_sha256: digest.to_string(),
    })
}

/* ── the pinned python3 vectors ─────────────────────────────────────── */

/// V1: the minimal capsule — everything empty or absent.
fn vector_v1() -> CapsuleV2 {
    CapsuleV2 {
        capsule_version: 2,
        format: "TLCAP_V2".to_string(),
        capsule_id: String::new(),
        generator: GeneratorV2 {
            name: "thruline".to_string(),
            version: "0.0.0-vector".to_string(),
        },
        scope: ScopeV2 {
            kind: "project".to_string(),
            id: "p1".to_string(),
        },
        source: SourceV2 {
            agent: "claude_code".to_string(),
            session: "s1".to_string(),
        },
        repository: RepositoryV2 {
            resolves: false,
            repo_id: None,
            basis: None,
        },
        workspace: WorkspaceV2 {
            identity: None,
            basis: "no workspace root supplied".to_string(),
        },
        snapshot: None,
        cutoff: CutoffV2 {
            chain_position: "a".repeat(64),
            event_total: 0,
            created_at: "2026-09-01T00:00:00Z".to_string(),
            statement: "sealed at creation; later source-session output cannot change this \
                        capsule"
                .to_string(),
        },
        lineage: None,
        original_task: vec![],
        established_work: vec![],
        agent_assertions: vec![],
        unknowns: vec![],
        open_obligations: vec![],
        current_failures: vec![],
        incomplete_evidence: vec![],
        continuation_state: vec![],
        decisions_constraints: vec![],
        recommendations: vec![],
        category_empties: vec![],
        redaction: RedactionV2 {
            treatment: Treatment::Ran {
                detector: "v2".to_string(),
                masked: 0,
                patterns: vec![],
                statement: "detector v2 ran".to_string(),
            },
            statement: "masking ran before persistence".to_string(),
            structural_exclusions: vec![],
        },
        completeness: CompletenessV2 {
            state: "complete".to_string(),
            reasons: vec![],
        },
        measurement: MeasurementV2 {
            token_estimate: 0,
            budget: 2000,
            trimmed: vec![],
        },
    }
}

/// V2: every field and every category populated.
fn vector_v2() -> CapsuleV2 {
    let mut c = vector_v1();
    c.scope.id = "p2".to_string();
    c.source = SourceV2 {
        agent: "codex".to_string(),
        session: "sess-2".to_string(),
    };
    c.repository = RepositoryV2 {
        resolves: true,
        repo_id: Some("r_abc".to_string()),
        basis: Some("remote".to_string()),
    };
    c.workspace = WorkspaceV2 {
        identity: Some(format!("wsid1:{}", "b".repeat(64))),
        basis: "observed at capture".to_string(),
    };
    c.snapshot = Some(capsule_v2::SnapshotCitationV2 {
        snapshot_id: "01SNAPVECTOR".to_string(),
        doc_sha256: "c".repeat(64),
        manifest_digest: Some("d".repeat(64)),
        complete: true,
        head: Some("e".repeat(40)),
        branch: Some("main".to_string()),
        taken_at: "2026-09-01T01:00:00Z".to_string(),
    });
    c.cutoff = CutoffV2 {
        chain_position: "f".repeat(64),
        event_total: 1234,
        created_at: "2026-09-01T02:00:00Z".to_string(),
        statement: "sealed at creation".to_string(),
    };
    c.lineage = Some("9".repeat(64));
    c.original_task = vec![TaskLine {
        text: "fix the login flow".to_string(),
        receipt: tr("/t/s2.jsonl", 1),
    }];
    c.established_work = vec![EstablishedFact {
        text: "tests passed at bind time".to_string(),
        state: "RECORD_SUPPORTED".to_string(),
        receipt: pe("cargo test", 0),
    }];
    c.agent_assertions = vec![AgentAssertion {
        text: "deployed to prod".to_string(),
        state: "ASSERTED".to_string(),
        advisory: Some(ADVISORY_NO_PROOF.to_string()),
        receipt: tr("/t/s2.jsonl", 9),
    }];
    c.unknowns = vec![UnknownItem {
        text: "config source unreadable".to_string(),
        state: "UNKNOWN".to_string(),
        receipt: tr("/t/s2.jsonl", 11),
    }];
    c.open_obligations = vec![OpenObligation {
        item_id: obligation_id("s2", "uncommitted_edit_run", ""),
        kind: "uncommitted_edit_run".to_string(),
        origin: "s2".to_string(),
        artifact_key: String::new(),
        text: "3 edits after the last commit \u{b7} a.rs".to_string(),
        receipt: tr("/t/s2.jsonl", 5),
    }];
    c.current_failures = vec![FailureLine {
        text: "`cargo build` failed".to_string(),
        receipt: pe("cargo build", 101),
    }];
    c.incomplete_evidence = vec![IncompleteMarker {
        marker: "overflowed_tool_results".to_string(),
        reason: "1 tool result overflows to a side file".to_string(),
    }];
    c.continuation_state = vec![ContinuationRef {
        text: "1 unstaged file: a.rs".to_string(),
        receipt: sm(&"d".repeat(64)),
    }];
    c.decisions_constraints = vec![DecisionLine {
        text: "workspace edits stay on branch main".to_string(),
        receipt: tr("/t/s2.jsonl", 3),
    }];
    c.recommendations = vec![Recommendation {
        text: "acknowledge or reject each open obligation by its item id".to_string(),
        label: "recommendation".to_string(),
        basis: "compiler rule: obligations present".to_string(),
    }];
    c.redaction = RedactionV2 {
        treatment: Treatment::Ran {
            detector: "v2".to_string(),
            masked: 2,
            patterns: vec!["email".to_string(), "flag_value".to_string()],
            statement: "detector v2 \u{b7} 2 masked".to_string(),
        },
        statement: "masking ran before persistence".to_string(),
        // Unsorted on purpose: normalization is part of the pinned layout.
        structural_exclusions: vec!["b-exclusion".to_string(), "a-exclusion".to_string()],
    };
    c.measurement = MeasurementV2 {
        token_estimate: 180,
        budget: 2000,
        trimmed: vec![TrimDisclosure {
            category: "agent_assertions".to_string(),
            items_withheld: 2,
            tokens_withheld: 60,
        }],
    };
    c
}

/// V3: incomplete capture, a quarantined element, unsorted empties.
fn vector_v3() -> CapsuleV2 {
    let mut c = vector_v1();
    c.scope.id = "p3".to_string();
    c.source.session = "s3".to_string();
    c.repository = RepositoryV2 {
        resolves: true,
        repo_id: Some("r_x".to_string()),
        basis: None,
    };
    c.workspace = WorkspaceV2 {
        identity: Some(format!("wsid1:{}", "1".repeat(64))),
        basis: "from the cited snapshot manifest".to_string(),
    };
    c.snapshot = Some(capsule_v2::SnapshotCitationV2 {
        snapshot_id: "01SNAPV3".to_string(),
        doc_sha256: "2".repeat(64),
        manifest_digest: None,
        complete: false,
        head: None,
        branch: None,
        taken_at: "2026-09-01T03:00:00Z".to_string(),
    });
    c.cutoff = CutoffV2 {
        chain_position: "3".repeat(64),
        event_total: 7,
        created_at: "2026-09-01T04:00:00Z".to_string(),
        statement: "sealed at creation".to_string(),
    };
    c.original_task = vec![TaskLine {
        text: "resume the migration".to_string(),
        receipt: tr("/t/s3.jsonl", 1),
    }];
    c.incomplete_evidence = vec![IncompleteMarker {
        marker: "masking_failure".to_string(),
        reason: "one string in category original_task could not be masked and was withheld"
            .to_string(),
    }];
    // Unsorted on purpose — the encoder sorts empties by category.
    c.category_empties = vec![
        EmptyDecl {
            category: "unknowns".to_string(),
            kind: "source_queried_and_empty".to_string(),
            reason: "no rows".to_string(),
        },
        EmptyDecl {
            category: "established_work".to_string(),
            kind: "source_queried_and_empty".to_string(),
            reason: "no backed claims".to_string(),
        },
    ];
    c.redaction = RedactionV2 {
        treatment: Treatment::NoDetectorRan {
            statement: "no detector ran".to_string(),
        },
        statement: "masking attempted".to_string(),
        structural_exclusions: vec!["x".to_string()],
    };
    c.completeness = CompletenessV2 {
        state: "incomplete".to_string(),
        reasons: vec![
            "masking failed for one element; it was withheld".to_string(),
            "snapshot capture was incomplete".to_string(),
        ],
    };
    c.measurement = MeasurementV2 {
        token_estimate: 12,
        budget: 2000,
        trimmed: vec![],
    };
    c
}

/// python3 hashlib (2026-09-01), two independent assemblies agreeing,
/// computed BEFORE this encoder existed. Changing any byte of the layout is
/// a migration to TLCAP_V3, and this test is what makes that loud.
#[test]
fn the_canonical_layout_matches_the_python3_vectors() {
    let v1 = vector_v1();
    let b1 = canonical_bytes(&v1).expect("encode v1");
    assert_eq!(b1.len(), 1018, "V1 byte length");
    assert_eq!(
        capsule_id(&v1).expect("id v1"),
        "6482f4153c7c992aaad4992ff4e62d122613edf81396c9f586c09e7d25adfc58",
        "V1 capsule id"
    );

    let v2 = vector_v2();
    let b2 = canonical_bytes(&v2).expect("encode v2");
    assert_eq!(b2.len(), 2901, "V2 byte length");
    assert_eq!(
        capsule_id(&v2).expect("id v2"),
        "9832f95c6f712b62b70ec5102385771056da26b7dc4915e3db8f10e46a952949",
        "V2 capsule id"
    );

    let v3 = vector_v3();
    let b3 = canonical_bytes(&v3).expect("encode v3");
    assert_eq!(b3.len(), 1587, "V3 byte length");
    assert_eq!(
        capsule_id(&v3).expect("id v3"),
        "13cb3a0d605f5f411f009349b01c729f3d194571ef66ccb494cb098d664f80c1",
        "V3 capsule id"
    );

    // The receipts the vectors embed serialize to the exact strings the
    // python assemblies used — pinned so the receipt encoding cannot drift
    // out from under the layout.
    assert_eq!(
        tr("/t/s2.jsonl", 1).to_canonical_json().expect("json"),
        r#"{"terminal":{"kind":"transcript","file":"/t/s2.jsonl","line":1}}"#
    );
    assert_eq!(
        pe("cargo test", 0).to_canonical_json().expect("json"),
        r#"{"terminal":{"kind":"process_execution","command":"cargo test","exit_code":0}}"#
    );
    assert_eq!(
        sm(&"d".repeat(64)).to_canonical_json().expect("json"),
        format!(
            r#"{{"terminal":{{"kind":"snapshot_manifest","manifest_sha256":"{}"}}}}"#,
            "d".repeat(64)
        )
    );
}

/// The TLOBL_V1 ids the python assemblies computed for the same preimages.
#[test]
fn the_obligation_id_vectors_match_python3() {
    assert_eq!(
        obligation_id("s2", "uncommitted_edit_run", ""),
        "obl1:361140cc8d7b52c956a79e3fbc6d5de7e8dbc93f53129d2b5476e4c59fcfe7e5"
    );
    assert_eq!(
        obligation_id("s9", "uncommitted_edit_run", ""),
        "obl1:ec4a34b21f116fdc25c4cdb0eea8daf94850e436ca44658a28b2b5b9c8de00fb"
    );
}

/* ── §34: tamper detection, per hashed-field family ─────────────────── */

/// Every §15 field family: mutate one field through the JSON, re-parse,
/// verify — the capsule id must no longer recompute. 100% gate: any miss
/// here is a sealed capsule that can lie.
#[test]
fn tampering_with_any_hashed_field_family_fails_verification() {
    let sealed = vector_v2();
    let mut with_id = sealed.clone();
    with_id.capsule_id = capsule_id(&sealed).expect("id");
    capsule_v2::verify_capsule(&with_id).expect("the untampered capsule verifies");

    let json = serde_json::to_value(&with_id).expect("json");
    let mutations: Vec<(&str, Mutation)> = vec![
        // §15's FIRST field family — schema version. The engine enforced it
        // from the start (verify_capsule checks both before hashing), but
        // A209 claimed "every §15 field family" while the suite skipped it.
        ("format", Box::new(|v| v["format"] = "TLCAP_V3".into())),
        (
            "capsule_version",
            Box::new(|v| v["capsule_version"] = 3.into()),
        ),
        (
            "generator",
            Box::new(|v| v["generator"]["version"] = "9.9.9".into()),
        ),
        (
            "scope",
            Box::new(|v| v["scope"]["id"] = "other-project".into()),
        ),
        (
            "source",
            Box::new(|v| v["source"]["session"] = "sess-FORGED".into()),
        ),
        (
            "repository",
            Box::new(|v| v["repository"]["repo_id"] = "r_forged".into()),
        ),
        (
            "workspace",
            Box::new(|v| v["workspace"]["identity"] = format!("wsid1:{}", "0".repeat(64)).into()),
        ),
        (
            "snapshot citation",
            Box::new(|v| v["snapshot"]["doc_sha256"] = "0".repeat(64).into()),
        ),
        (
            "snapshot completeness",
            Box::new(|v| v["snapshot"]["complete"] = false.into()),
        ),
        (
            "cutoff",
            Box::new(|v| v["cutoff"]["chain_position"] = "0".repeat(64).into()),
        ),
        ("lineage", Box::new(|v| v["lineage"] = Value::Null)),
        (
            "original task",
            Box::new(|v| v["original_task"][0]["text"] = "do something else".into()),
        ),
        (
            "established fact text",
            Box::new(|v| v["established_work"][0]["text"] = "tests never ran".into()),
        ),
        (
            "established fact state",
            Box::new(|v| v["established_work"][0]["state"] = "CURRENT_STATE_CONFIRMED".into()),
        ),
        (
            "assertion state escalation",
            Box::new(|v| v["agent_assertions"][0]["state"] = "RECORD_SUPPORTED".into()),
        ),
        (
            "assertion advisory",
            Box::new(|v| v["agent_assertions"][0]["advisory"] = Value::Null),
        ),
        (
            "unknown item",
            Box::new(|v| v["unknowns"][0]["text"] = "everything is known".into()),
        ),
        (
            "obligation id",
            Box::new(|v| v["open_obligations"][0]["item_id"] = "obl1:forged".into()),
        ),
        (
            "obligation origin",
            Box::new(|v| v["open_obligations"][0]["origin"] = "s99".into()),
        ),
        (
            "failure line",
            Box::new(|v| v["current_failures"][0]["text"] = "`cargo build` succeeded".into()),
        ),
        (
            "incomplete-evidence marker",
            Box::new(|v| v["incomplete_evidence"][0]["marker"] = "nothing_missing".into()),
        ),
        (
            "continuation state",
            Box::new(|v| v["continuation_state"][0]["text"] = "tree clean".into()),
        ),
        (
            "decision line",
            Box::new(|v| v["decisions_constraints"][0]["text"] = "no constraints".into()),
        ),
        (
            "recommendation relabelled",
            Box::new(|v| v["recommendations"][0]["label"] = "established fact".into()),
        ),
        (
            "receipt terminal",
            Box::new(|v| {
                v["original_task"][0]["receipt"]["terminal"]["line"] = 999.into();
            }),
        ),
        (
            "redaction treatment",
            Box::new(|v| v["redaction"]["treatment"]["masked"] = 0.into()),
        ),
        (
            "completeness state",
            Box::new(|v| v["completeness"]["state"] = "incomplete".into()),
        ),
        (
            "measurement / trim disclosure",
            Box::new(|v| v["measurement"]["trimmed"][0]["items_withheld"] = 0.into()),
        ),
        (
            "empties",
            Box::new(|v| {
                v["category_empties"] = serde_json::json!([{
                    "category": "current_failures",
                    "kind": "source_queried_and_empty",
                    "reason": "forged"
                }]);
            }),
        ),
    ];

    for (family, mutate) in mutations {
        let mut doctored = json.clone();
        mutate(&mut doctored);
        assert_ne!(doctored, json, "the {family} mutation changed nothing");
        let reparsed: CapsuleV2 =
            serde_json::from_value(doctored).unwrap_or_else(|e| panic!("{family}: reparse: {e}"));
        assert!(
            capsule_v2::verify_capsule(&reparsed).is_err(),
            "a tampered {family} field still verifies — the §34 gate is broken"
        );
    }

    // Tampering with the id itself is also caught.
    let mut forged_id = with_id.clone();
    forged_id.capsule_id = "0".repeat(64);
    assert!(capsule_v2::verify_capsule(&forged_id).is_err());

    // And a claimed-v2 document carrying a field this reader cannot cover
    // refuses at parse — loudly, not by silently dropping it.
    let mut extra = serde_json::to_value(&with_id).expect("json");
    extra["injected_field"] = "smuggled".into();
    assert!(
        serde_json::from_value::<CapsuleV2>(extra).is_err(),
        "an unknown field parsed silently — it would escape the hash"
    );
}

/// The refusal holds at EVERY level, not only the root (A209 correction:
/// an auditor injected a key into 12 nested positions of a real sealed
/// capsule and all 12 parsed, verified, and silently dropped it).
#[test]
fn an_unknown_field_refuses_at_every_nesting_level() {
    let mut sealed = vector_v2();
    sealed.capsule_id = capsule_id(&sealed).expect("id");
    let json = serde_json::to_value(&sealed).expect("json");

    let positions: Vec<(&str, Mutation)> = vec![
        (
            "generator",
            Box::new(|v: &mut Value| v["generator"]["x"] = 1.into()),
        ),
        (
            "scope",
            Box::new(|v: &mut Value| v["scope"]["x"] = 1.into()),
        ),
        (
            "source",
            Box::new(|v: &mut Value| v["source"]["x"] = 1.into()),
        ),
        (
            "repository",
            Box::new(|v: &mut Value| v["repository"]["x"] = 1.into()),
        ),
        (
            "workspace",
            Box::new(|v: &mut Value| v["workspace"]["x"] = 1.into()),
        ),
        (
            "snapshot",
            Box::new(|v: &mut Value| v["snapshot"]["x"] = 1.into()),
        ),
        (
            "cutoff",
            Box::new(|v: &mut Value| v["cutoff"]["x"] = 1.into()),
        ),
        (
            "redaction",
            Box::new(|v: &mut Value| v["redaction"]["x"] = 1.into()),
        ),
        (
            "completeness",
            Box::new(|v: &mut Value| v["completeness"]["x"] = 1.into()),
        ),
        (
            "measurement",
            Box::new(|v: &mut Value| v["measurement"]["x"] = 1.into()),
        ),
        (
            "measurement.trimmed[0]",
            Box::new(|v: &mut Value| v["measurement"]["trimmed"][0]["x"] = 1.into()),
        ),
        (
            "original_task[0]",
            Box::new(|v: &mut Value| v["original_task"][0]["x"] = 1.into()),
        ),
        (
            "established_work[0]",
            Box::new(|v: &mut Value| v["established_work"][0]["x"] = 1.into()),
        ),
        (
            "agent_assertions[0]",
            Box::new(|v: &mut Value| v["agent_assertions"][0]["x"] = 1.into()),
        ),
        (
            "unknowns[0]",
            Box::new(|v: &mut Value| v["unknowns"][0]["x"] = 1.into()),
        ),
        (
            "open_obligations[0]",
            Box::new(|v: &mut Value| v["open_obligations"][0]["x"] = 1.into()),
        ),
        (
            "current_failures[0]",
            Box::new(|v: &mut Value| v["current_failures"][0]["x"] = 1.into()),
        ),
        (
            "incomplete_evidence[0]",
            Box::new(|v: &mut Value| v["incomplete_evidence"][0]["x"] = 1.into()),
        ),
        (
            "continuation_state[0]",
            Box::new(|v: &mut Value| v["continuation_state"][0]["x"] = 1.into()),
        ),
        (
            "decisions_constraints[0]",
            Box::new(|v: &mut Value| v["decisions_constraints"][0]["x"] = 1.into()),
        ),
        (
            "recommendations[0]",
            Box::new(|v: &mut Value| v["recommendations"][0]["x"] = 1.into()),
        ),
    ];
    for (position, inject) in positions {
        let mut doctored = json.clone();
        inject(&mut doctored);
        assert_ne!(doctored, json, "{position}: the injection changed nothing");
        assert!(
            serde_json::from_value::<CapsuleV2>(doctored).is_err(),
            "a field injected at {position} parsed silently and was dropped"
        );
    }

    // The one deliberate exception: a receipt's unknown fields are PRESERVED
    // — and they are inside the hash, so preservation costs no coverage.
    let mut receipt_extra = json.clone();
    receipt_extra["original_task"][0]["receipt"]["vendor_note"] = "kept".into();
    let parsed: CapsuleV2 =
        serde_json::from_value(receipt_extra).expect("receipt extras are preserved, not refused");
    assert_eq!(
        parsed.original_task[0]
            .receipt
            .extra
            .get("vendor_note")
            .and_then(Value::as_str),
        Some("kept"),
        "the receipt dropped an unknown field"
    );
    assert!(
        capsule_v2::verify_capsule(&parsed).is_err(),
        "an added receipt field must change the id — it is hash-covered"
    );
}

/* ── §34: stable unresolved-item ids ────────────────────────────────── */

#[test]
fn obligation_ids_are_rewording_invariant_and_source_distinct() {
    // Metamorphic pair 1: same source, reworded text → same id. The id
    // function does not read wording at all; the compiled capsule proves
    // the wiring end-to-end below.
    let a = obligation_id("s1", "uncommitted_edit_run", "");
    let b = obligation_id("s1", "uncommitted_edit_run", "");
    assert_eq!(a, b);

    // Metamorphic pair 2: different source, same text → different id.
    assert_ne!(a, obligation_id("s2", "uncommitted_edit_run", ""));

    // Collision walk: distinct sources never share an id, and the
    // field-boundary cases (origin/kind/key shifts) stay distinct.
    let mut seen = BTreeSet::new();
    for origin in ["s1", "s2", "s3", "sA", "sB"] {
        for kind in ["uncommitted_edit_run", "failed_command", "k"] {
            for key in ["", "a.rs", "b.rs", "a"] {
                assert!(
                    seen.insert(obligation_id(origin, kind, key)),
                    "collision at ({origin},{kind},{key})"
                );
            }
        }
    }
    // Length-prefixing means shifted boundaries cannot collide.
    assert_ne!(obligation_id("ab", "c", ""), obligation_id("a", "bc", ""));
    assert_ne!(obligation_id("a", "", "b"), obligation_id("", "a", "b"));
}

#[test]
fn a_compiled_obligation_keeps_its_id_when_the_wording_grows() {
    // End-to-end: the same session's unresolved run keeps its item id as
    // more edits (different wording, growing file list) accumulate.
    let (ledger, project) = seeded();
    let before = compile(&ledger, &project);
    assert_eq!(before.open_obligations.len(), 1, "one unresolved run");
    let id_before = before.open_obligations[0].item_id.clone();

    ledger
        .conn()
        .execute_batch(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                                tool_use_id, tool_input, raw_file, raw_line, raw_sha256,
                                prev_hash, chain_hash)
               VALUES ('e8','-tmp-a209-project','s1',8,'2026-09-01T00:00:09Z','tool_use','Edit',
                       'tu4','{\"file_path\":\"src/other.rs\"}','/t/s1.jsonl',9,'x','c7','c8');",
        )
        .expect("more edits");

    let after = compile(&ledger, &project);
    assert_eq!(after.open_obligations.len(), 1);
    assert_eq!(
        after.open_obligations[0].item_id, id_before,
        "the same source's obligation id moved when its wording changed"
    );
    assert_ne!(
        before.open_obligations[0].text, after.open_obligations[0].text,
        "the wording did actually change"
    );
}

/* ── §15: the five categories stay structurally distinct ────────────── */

#[test]
fn the_five_categories_are_separate_arrays_the_renderer_cannot_blend() {
    let (ledger, project) = seeded();
    let c = compile(&ledger, &project);

    // In the JSON: five distinct top-level arrays, no shared bucket.
    let json = serde_json::to_value(&c).expect("json");
    for key in [
        "established_work",
        "agent_assertions",
        "unknowns",
        "open_obligations",
        "recommendations",
    ] {
        assert!(json[key].is_array(), "{key} is not its own array");
    }

    // In the render: each category's content appears under its own §15
    // heading and under no other.
    let md = capsule_v2::render_markdown(&c);
    let heading_above = |needle: &str| -> String {
        let at = md
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} rendered"));
        let start = md[..at].rfind("\n## ").expect("a heading above the item");
        md[start + 1..].lines().next().unwrap_or("").to_string()
    };
    assert!(
        heading_above("digest collapses bursts").contains("Established facts"),
        "an established fact rendered outside its own heading"
    );
    assert!(
        heading_above("deployed to production").contains("Agent assertions"),
        "an agent assertion rendered outside its own heading"
    );
    // The assertion never gains a fact-state and never loses its advisory.
    for a in &c.agent_assertions {
        assert_eq!(a.state, "ASSERTED");
        assert_eq!(a.advisory.as_deref(), Some(ADVISORY_NO_PROOF));
    }
    for e in &c.established_work {
        assert_eq!(e.state, "RECORD_SUPPORTED");
    }
    // Recommendations carry their label in data and in render.
    assert!(md.contains("## Recommendations — labelled, never facts"));

    // Every §15 envelope field is present in the render.
    for required in [
        "Capsule id:",
        "Repository:",
        "Workspace:",
        "Snapshot:",
        "Source:",
        "Cutoff:",
        "Lineage:",
        "## Original task",
        "## Known unknowns",
        "## Open obligations",
        "## Current failures",
        "## Incomplete evidence markers",
        "## Dirty and untracked work needed for continuation",
        "## Decisions and constraints",
        "## Redaction declaration",
        "## Completeness declaration",
        "## Measurement",
    ] {
        assert!(md.contains(required), "the render lost {required:?}");
    }
}

/* ── §34: self-citation is structurally impossible ──────────────────── */

#[test]
fn a_capsule_cannot_cite_itself_because_no_capsule_terminal_exists() {
    let (ledger, project) = seeded();
    let c = compile(&ledger, &project);

    // The six §11 terminals are the only receipt endings; none is a
    // capsule. Exhaustive match — a seventh arm fails this compile.
    let kind_of = |t: &ReceiptTerminal| match t {
        ReceiptTerminal::Transcript { .. } => "transcript",
        ReceiptTerminal::Filesystem { .. } => "filesystem",
        ReceiptTerminal::ProcessExecution { .. } => "process_execution",
        ReceiptTerminal::SnapshotManifest { .. } => "snapshot_manifest",
        ReceiptTerminal::ProviderResponse { .. } => "provider_response",
        ReceiptTerminal::HumanAdjudication { .. } => "human_adjudication",
    };
    let all_receipts: Vec<&SourceReceipt> = c
        .original_task
        .iter()
        .map(|t| &t.receipt)
        .chain(c.established_work.iter().map(|e| &e.receipt))
        .chain(c.agent_assertions.iter().map(|a| &a.receipt))
        .chain(c.unknowns.iter().map(|u| &u.receipt))
        .chain(c.open_obligations.iter().map(|o| &o.receipt))
        .chain(c.current_failures.iter().map(|f| &f.receipt))
        .chain(c.continuation_state.iter().map(|t| &t.receipt))
        .chain(c.decisions_constraints.iter().map(|d| &d.receipt))
        .collect();
    assert!(!all_receipts.is_empty());
    for r in &all_receipts {
        assert!(!kind_of(&r.terminal).is_empty());
    }

    // And no receipt byte anywhere in the sealed document carries the
    // capsule's own id — the belt on the structural braces.
    let own_id = c.capsule_id.clone();
    for r in &all_receipts {
        let json = r.to_canonical_json().expect("receipt json");
        assert!(
            !json.contains(&own_id),
            "a receipt carries the capsule's own id"
        );
    }
}

/* ── §34: pre-persistence redaction ─────────────────────────────────── */

#[test]
fn a_secret_in_the_record_is_masked_before_the_document_exists() {
    let (ledger, project) = seeded();
    let c = compile(&ledger, &project);
    let json = serde_json::to_string(&c).expect("json");
    let md = capsule_v2::render_markdown(&c);
    for (surface, text) in [("json", &json), ("markdown", &md)] {
        assert!(
            !text.contains("AKIAIOSFODNN7EXAMPLE"),
            "the canary key reached the {surface}"
        );
    }
    assert!(
        json.contains("[masked: aws]"),
        "the mask is stated in place"
    );
    assert!(
        c.redaction.treatment.masked() >= 1,
        "the treatment records it"
    );
    // And the sealed bytes are the masked bytes — masking ran before
    // persistence, not on a rendering path after it.
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let sealed = capsule_v2::seal(&store, &c).expect("seal");
    let stored = String::from_utf8(store.get(&sealed.digest).expect("get")).expect("utf8");
    assert!(!stored.contains("AKIAIOSFODNN7EXAMPLE"));
    assert!(stored.contains("[masked: aws]"));
}

/// The A209 blocking defect, pinned end-to-end at the artifact — across
/// credential FORMATS and injection POSITIONS.
///
/// A known-format secret inside an edit FILE PATH reached
/// `capsule-v2.json`, the markdown, and the decrypted CAS bytes, because
/// the known-format scan was segment-boundary-scoped and `_` is not a
/// segment boundary. Round 3 widened this: the original version used one
/// format in one position, so it could not have caught `ghu_`/`ghr_`
/// (absent from the detector table entirely) or a leak arriving through
/// `original_task` or `current_failures`. Every format now travels through
/// every reachable category.
#[test]
fn known_format_secrets_are_masked_in_every_sealed_byte_across_categories() {
    // One canary per format, each with a real-length body.
    const BODY: &str = "TLCANARYaaZZ11bbYY22ccXX33ddWW44e";
    let canaries: Vec<(&str, String)> = vec![
        ("github_p", format!("ghp_{BODY}")),
        ("github_u", format!("ghu_{BODY}")),
        ("github_r", format!("ghr_{BODY}")),
        ("openai", format!("sk-{BODY}")),
        ("anthropic", format!("sk-ant-api03-{BODY}")),
        ("aws", "AKIATLCANARY7EXAMPLE".to_string()),
        ("google", "AIzaTLCANARYaaZZ11bbYY22ccXX33ddWW4".to_string()),
    ];
    let (ledger, project) = seeded();
    let mut seq = 40i64;
    for (name, canary) in &canaries {
        // Position 1: an edit file path -> open_obligations text.
        // Position 2: a user message   -> original_task text.
        // Position 3: a failed command -> current_failures text.
        ledger
            .conn()
            .execute_batch(&format!(
                "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text, tool_name,
                                    tool_use_id, tool_input, tool_result, is_error,
                                    raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                   VALUES
                    ('ep{name}','-tmp-a209-project','s1',{seq},'2026-09-01T00:30:00Z','tool_use',
                     NULL,'Edit','tp{name}','{{\"file_path\":\"/tmp/r/secrets_{canary}.env\"}}',
                     NULL,0,'/t/s1.jsonl',{seq},'x','pa{name}','pb{name}'),
                    ('eu{name}','-tmp-a209-project','s1',{seq2},'2026-09-01T00:30:01Z','user_msg',
                     '{pad} rotate {canary} before shipping',NULL,NULL,NULL,NULL,0,
                     '/t/s1.jsonl',{seq2},'x','pb{name}','pc{name}'),
                    ('eb{name}','-tmp-a209-project','s1',{seq3},'2026-09-01T00:30:02Z','tool_use',
                     NULL,'Bash','tb{name}',
                     '{{\"command\":\"curl -sS -X POST https://api.example.com/deploy \
-H accept:application/json --data seed=1 --fail zzzzzzzz {canary}\"}}',NULL,0,
                     '/t/s1.jsonl',{seq3},'x','pc{name}','pd{name}'),
                    ('er{name}','-tmp-a209-project','s1',{seq4},'2026-09-01T00:30:03Z',
                     'tool_result',NULL,NULL,'tb{name}',NULL,'boom',1,
                     '/t/s1.jsonl',{seq4},'x','pd{name}','pe{name}');",
                // 258 characters, so the credential begins at ~265 and the
                // 280-char ASK_TRUNCATE cut lands INSIDE it. A cut that
                // merely discards the credential proves nothing.
                pad = "p".repeat(258),
                seq2 = seq + 1,
                seq3 = seq + 2,
                seq4 = seq + 3,
            ))
            .unwrap_or_else(|e| panic!("seed {name}: {e}"));
        seq += 4;
    }

    let c = compile(&ledger, &project);
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let sealed = capsule_v2::seal(&store, &c).expect("seal");
    let from_cas = String::from_utf8(store.get(&sealed.digest).expect("get")).expect("utf8");
    let surfaces = [
        ("compiled json", serde_json::to_string(&c).expect("json")),
        ("markdown", capsule_v2::render_markdown(&c)),
        (
            "sealed bytes",
            String::from_utf8(sealed.bytes.clone()).expect("utf8"),
        ),
        ("decrypted CAS bytes", from_cas),
    ];
    for (name, canary) in &canaries {
        for (surface, text) in &surfaces {
            assert!(
                !text.contains(canary.as_str()),
                "the {name} canary reached the {surface} — §34 no-secret-canary-leakage fails"
            );
        }
    }
    // The canary body itself must not survive anywhere either.
    for (surface, text) in &surfaces {
        assert!(
            !text.contains("TLCANARY"),
            "a canary body reached the {surface}"
        );
    }
    // The two assertions the earlier widening dropped, restored — a bare
    // `masked() >= n` cannot catch a FALSE treatment string, which is
    // exactly what the PEM defect was.
    //
    // (a) the mask is stated IN PLACE in the obligation text;
    assert!(
        c.open_obligations
            .iter()
            .any(|o| o.text.contains("[masked: ")),
        "no obligation reports a mask in place: {:?}",
        c.open_obligations
            .iter()
            .map(|o| &o.text)
            .collect::<Vec<_>>()
    );
    // (b) the treatment NAMES every family that fired, not just a count.
    match &c.redaction.treatment {
        thruline_core::redact::Treatment::Ran {
            masked, patterns, ..
        } => {
            assert!(
                *masked >= canaries.len() as u64,
                "the treatment under-reports: {masked}"
            );
            for family in ["github", "openai", "anthropic", "aws", "google"] {
                assert!(
                    patterns.iter().any(|p| p == family),
                    "the treatment does not name {family}, which fired: {patterns:?}"
                );
            }
        }
        other => panic!("no detector ran over a capsule full of canaries: {other:?}"),
    }
    // The three positions this test is credited with must all be NON-EMPTY,
    // or the zero it reports is a zero over nothing.
    assert!(!c.open_obligations.is_empty(), "position 1 was empty");
    assert!(!c.original_task.is_empty(), "position 2 was empty");
    assert!(
        !c.current_failures.is_empty(),
        "position 3 (current_failures) was empty — the test would report a \
         vacuous zero over a category it never exercised"
    );
}

#[test]
fn a_masking_failure_quarantines_marks_incomplete_and_leaks_nothing() {
    let (ledger, project) = seeded();
    // The forced-failure seam: the masker refuses any string carrying the
    // canary. The refusal itself carries no content by type — it is None.
    // A209-C3: the seam must be forced on text the CAPSULE masks itself.
    // The opening ask is now masked upstream (mask-before-truncate), so a
    // canary planted there never reaches this masker — the quarantine path
    // is unchanged, but the old injection point no longer exercises it.
    // Node labels are inserted directly by this fixture, bypassing derive,
    // so `established_work` is capsule-masked only.
    let canary = "digest collapses bursts";
    let mut failing = |s: &str| {
        if s.contains(canary) {
            None
        } else {
            Some(thruline_core::redact::mask(s))
        }
    };
    let c = capsule_v2::compile_with_masker(
        &ledger,
        &project,
        &CompileContext::default(),
        NOW,
        &mut failing,
    )
    .expect("a masking failure must not fail the compile — it quarantines");

    // The element is quarantined, not emitted.
    assert!(
        c.established_work.iter().any(|e| e.text == QUARANTINE_TEXT),
        "the affected element was not quarantined: {:?}",
        c.established_work
            .iter()
            .map(|e| &e.text)
            .collect::<Vec<_>>()
    );
    // The capsule says it is incomplete, and names the marker.
    assert_eq!(c.completeness.state, "incomplete");
    assert!(
        c.incomplete_evidence
            .iter()
            .any(|m| m.marker == "masking_failure" && m.reason.contains("established_work")),
        "no named masking_failure marker: {:?}",
        c.incomplete_evidence
    );
    // The raw value reaches NOTHING: not the JSON, not the render, not the
    // markers, not the completeness reasons.
    let json = serde_json::to_string(&c).expect("json");
    let md = capsule_v2::render_markdown(&c);
    for (surface, text) in [("json", &json), ("markdown", &md)] {
        assert!(
            !text.contains(canary),
            "the raw value leaked into the {surface} after a masking failure"
        );
    }

    // Error paths leak nothing either. A209's first version asked for this
    // against `-no-such-project`, which returns Ok (the head fold over zero
    // events succeeds) — so the assertion body never ran and the test would
    // have passed even if error Display carried raw secrets. Force a REAL
    // error instead: a claim node whose anchor event has no `events` row,
    // which `transcript_receipt` refuses because it cannot build a receipt.
    // A dangling anchor is what a DAMAGED ledger looks like, and the
    // foreign key is what normally prevents one — so the corrupt state is
    // constructed deliberately, with the guard dropped only for the insert.
    // The error under test is the compiler's refusal to invent a receipt it
    // cannot resolve; the point of the test is that its message carries no
    // secret.
    ledger
        .conn()
        .execute_batch("PRAGMA foreign_keys=OFF")
        .expect("relax fk");
    ledger
        .conn()
        .execute(
            "INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                               currency, session_id, anchor_event_id, derived_rev)
             VALUES ('n-dangling','-tmp-a209-project',?1,'supported','completed','supported',
                     'current','s1','e-does-not-exist',1)",
            [format!("a claim mentioning {canary}")],
        )
        .expect("insert dangling node");
    ledger
        .conn()
        .execute_batch("PRAGMA foreign_keys=ON")
        .expect("restore fk");
    let err = capsule_v2::compile_with_masker(
        &ledger,
        &project,
        &CompileContext::default(),
        NOW,
        &mut failing,
    );
    let e = err.expect_err("a node with no anchor event must fail loudly, not silently");
    let shown = format!("{e}");
    assert!(
        shown.contains("e-does-not-exist"),
        "the error must name what it could not resolve: {shown}"
    );
    assert!(
        !shown.contains(canary),
        "an error path carried the raw value: {shown}"
    );
    assert!(
        !format!("{e:?}").contains(canary),
        "the Debug form carried the raw value"
    );
}

/* ── §15: token budget, trim disclosure, never-trimmed categories ───── */

#[test]
fn the_trim_discloses_what_it_withheld_and_never_touches_critical_facts() {
    let (ledger, project) = seeded();
    // Flood the assertion class so a tiny budget forces a real trim.
    for i in 0..30 {
        ledger
            .conn()
            .execute(
                "INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                                   currency, session_id, anchor_event_id, derived_rev)
                 VALUES (?1, '-tmp-a209-project', ?2, 'unsupported', 'completed', 'unknown',
                         'current', 's1', 'e7', 1)",
                rusqlite::params![format!("nx{i}"), format!("claimed step {i} finished")],
            )
            .expect("insert");
    }
    let ctx = CompileContext {
        budget: Some(700),
        ..Default::default()
    };
    let c = capsule_v2::compile(&ledger, &project, &ctx, NOW).expect("compile");

    // Something was trimmed, and the trim is disclosed with its tokens.
    let trimmed: Vec<&TrimDisclosure> = c.measurement.trimmed.iter().collect();
    assert!(
        !trimmed.is_empty(),
        "a 700-token budget over 30+ assertions trimmed nothing"
    );
    for t in &trimmed {
        assert!(t.items_withheld > 0 && t.tokens_withheld > 0);
        assert!(
            capsule_v2::TRIM_ORDER.contains(&t.category.as_str()),
            "{} was trimmed but is not in TRIM_ORDER",
            t.category
        );
        assert!(
            !capsule_v2::NEVER_TRIMMED.contains(&t.category.as_str()),
            "{} is correctness-critical and was trimmed",
            t.category
        );
    }
    // The correctness-critical categories survive in full: the failure,
    // the obligation, and the incomplete-evidence markers are all present.
    assert_eq!(
        c.current_failures.len(),
        1,
        "a failure was dropped for size"
    );
    assert_eq!(
        c.open_obligations.len(),
        1,
        "an obligation was dropped for size"
    );
    assert!(
        c.incomplete_evidence
            .iter()
            .any(|m| m.marker == "overflowed_tool_results"),
        "the overflow disclosure was dropped for size"
    );
    // A trimmed category keeps its first item — trimmed, never erased.
    assert!(!c.agent_assertions.is_empty());
    // The measurement is stamped and the budget recorded.
    assert_eq!(c.measurement.budget, 700);
    assert!(c.measurement.token_estimate > 0);
}

#[test]
fn an_untrimmed_capsule_measures_within_the_default_budget() {
    let (ledger, project) = seeded();
    let c = compile(&ledger, &project);
    assert!(c.measurement.trimmed.is_empty(), "nothing to trim here");
    assert!(
        c.measurement.token_estimate <= capsule_v2::DEFAULT_TOKEN_BUDGET,
        "a small scope's capsule blew the {} budget: {}",
        capsule_v2::DEFAULT_TOKEN_BUDGET,
        c.measurement.token_estimate
    );
}

/// The live finding, pinned: `current_failures` is never trimmed, so its
/// honesty lives at its ceiling — "current" cannot mean every failure ever
/// recorded (the first live seal read 84 all-time failures as current and
/// measured 6,854 tokens against the 2,000 target). The category carries
/// the NEWEST failures up to [`capsule_v2::FAILURE_CEILING`] and the
/// elision is a named incomplete-evidence marker, never silent.
#[test]
fn current_failures_carry_the_newest_up_to_the_ceiling_with_the_elision_disclosed() {
    let (ledger, project) = seeded();
    // 30 more failed side-effect commands, oldest first.
    for i in 0..30 {
        ledger
            .conn()
            .execute_batch(&format!(
                "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                                    tool_use_id, tool_input, tool_result, is_error,
                                    raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                   VALUES ('ef{i}','-tmp-a209-project','s1',{seq},'2026-09-01T00:10:{i:02}Z',
                           'tool_use','Bash','tf{i}',
                           '{{\"command\":\"supabase functions deploy step{i}\"}}',NULL,0,
                           '/t/s1.jsonl',{line},'x','h{i}','i{i}'),
                          ('er{i}','-tmp-a209-project','s1',{seq2},'2026-09-01T00:10:{i:02}Z',
                           'tool_result',NULL,'tf{i}',NULL,'boom',1,
                           '/t/s1.jsonl',{line2},'x','i{i}','j{i}');",
                seq = 100 + i * 2,
                seq2 = 101 + i * 2,
                line = 100 + i * 2,
                line2 = 101 + i * 2,
            ))
            .expect("insert failure pair");
    }
    let c = compile(&ledger, &project);
    assert_eq!(
        c.current_failures.len(),
        capsule_v2::FAILURE_CEILING,
        "the category holds exactly the ceiling"
    );
    // Newest first: the last-inserted failure leads, the oldest fall away.
    assert!(
        c.current_failures[0].text.contains("step29"),
        "the newest failure leads: {}",
        c.current_failures[0].text
    );
    assert!(
        c.incomplete_evidence.iter().any(|m| {
            m.marker == "category_ceiling"
                && m.reason.contains("current_failures")
                && m.reason.contains("31")
        }),
        "the elision is disclosed with the true total: {:?}",
        c.incomplete_evidence
    );
}

/* ── §34: sealed is sealed; new state is a new capsule with lineage ─── */

#[test]
fn a_later_source_event_cannot_mutate_a_sealed_capsule() {
    let (ledger, project) = seeded();
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");

    let first = compile(&ledger, &project);
    let sealed_first = capsule_v2::seal(&store, &first).expect("seal");
    let out = capsule_v2::record_capsule_created(
        &ledger,
        &sealed_first,
        Path::new("/tmp/a209/capsule-v2.json"),
        NOW,
        "cli:test:1",
    )
    .expect("record");
    assert!(matches!(out, CapsuleRecordOutcome::Recorded(_)));

    // The source session moves on.
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text, raw_file,
                                raw_line, raw_sha256, prev_hash, chain_hash)
               VALUES ('e9','-tmp-a209-project','s1',9,'2026-09-01T01:00:00Z','user_msg',
                       'now do the next thing','/t/s1.jsonl',10,'x','c8','c9');",
        )
        .expect("advance");

    // The new state is a NEW capsule, linked by lineage.
    let lineage = capsule_v2::latest_capsule_id(&ledger, "repo_a209").expect("lineage");
    assert_eq!(lineage.as_deref(), Some(first.capsule_id.as_str()));
    let ctx = CompileContext {
        lineage: lineage.clone(),
        ..Default::default()
    };
    let second =
        capsule_v2::compile(&ledger, &project, &ctx, "2026-09-01T13:00:00Z").expect("compile");
    assert_ne!(
        second.capsule_id, first.capsule_id,
        "new state, new identity"
    );
    assert_eq!(second.lineage.as_deref(), Some(first.capsule_id.as_str()));
    let sealed_second = capsule_v2::seal(&store, &second).expect("seal second");
    assert_ne!(sealed_second.digest, sealed_first.digest);

    // And the FIRST capsule's bytes verify unchanged — nothing about the
    // source's later life reached them.
    let reloaded = capsule_v2::open_from_store(&store, &sealed_first.digest)
        .expect("the sealed capsule still loads and verifies byte-identically");
    assert_eq!(reloaded, first);
    assert_eq!(
        store.get(&sealed_first.digest).expect("bytes"),
        sealed_first.bytes,
        "the stored bytes moved"
    );
}

#[test]
fn compile_is_deterministic_over_the_same_rows() {
    let (ledger, project) = seeded();
    let a = compile(&ledger, &project);
    let b = compile(&ledger, &project);
    assert_eq!(
        a, b,
        "same rows, same context, same moment — different capsule"
    );
    assert_eq!(
        serde_json::to_string(&a).expect("json"),
        serde_json::to_string(&b).expect("json")
    );
}

#[test]
fn seal_refuses_a_capsule_whose_id_does_not_verify() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let mut lying = vector_v2();
    lying.capsule_id = "0".repeat(64);
    assert!(
        capsule_v2::seal(&store, &lying).is_err(),
        "a capsule with a forged id was sealed"
    );
}

/* ── the observation writers ────────────────────────────────────────── */

#[test]
fn capsule_created_and_offered_are_recorded_with_receipts_and_dedup() {
    let (ledger, project) = seeded();
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let c = compile(&ledger, &project);
    let sealed = capsule_v2::seal(&store, &c).expect("seal");
    let path = Path::new("/tmp/a209/capsule-v2.json");

    let created = capsule_v2::record_capsule_created(&ledger, &sealed, path, NOW, "cli:test:1")
        .expect("record created");
    let CapsuleRecordOutcome::Recorded(outcome) = created else {
        panic!("the resolving scope abstained");
    };
    let record = outcome.record().clone();
    assert_eq!(record.observation_type_raw, "CAPSULE_CREATED");
    assert_eq!(record.repository_identity, "repo_a209");
    let receipt = record.receipt().expect("typed receipt");
    match &receipt.terminal {
        ReceiptTerminal::Filesystem { path: p, sha256 } => {
            assert_eq!(p, "/tmp/a209/capsule-v2.json");
            assert_eq!(sha256.as_deref(), Some(sealed.digest.as_str()));
        }
        other => panic!("wrong terminal: {other:?}"),
    }
    assert_eq!(
        receipt.extra.get("capsule_id").and_then(Value::as_str),
        Some(c.capsule_id.as_str())
    );
    assert_eq!(
        receipt.extra.get("capsule_digest").and_then(Value::as_str),
        Some(sealed.digest.as_str())
    );

    // Identical content re-recorded is the standing row — an observation
    // is a fact recorded once, never a verification timestamp (the
    // SNAPSHOT_VERIFIED dedup semantics, held here on purpose).
    let again = capsule_v2::record_capsule_created(&ledger, &sealed, path, NOW, "cli:test:1")
        .expect("record again");
    let CapsuleRecordOutcome::Recorded(outcome2) = again else {
        panic!("abstained on repeat");
    };
    assert!(
        matches!(
            *outcome2,
            thruline_core::observation::AppendOutcome::AlreadyRecorded(_)
        ),
        "identical content appended a second observation"
    );

    // OFFERED is its own observation type with the agent in the receipt.
    let offered =
        capsule_v2::record_capsule_offered(&ledger, &sealed, "claude", path, NOW, "cli:test:1")
            .expect("record offered");
    let CapsuleRecordOutcome::Recorded(outcome3) = offered else {
        panic!("abstained");
    };
    let record3 = outcome3.record().clone();
    assert_eq!(record3.observation_type_raw, "CAPSULE_OFFERED");
    let receipt3 = record3.receipt().expect("typed receipt");
    assert_eq!(
        receipt3
            .extra
            .get("offered_to_agent")
            .and_then(Value::as_str),
        Some("claude")
    );
    // The chain now folds over all appended observations.
    assert_eq!(
        thruline_core::observation::verify_chain(&ledger).expect("verify"),
        2
    );
}

#[test]
fn writers_abstain_loudly_when_the_scope_resolves_to_no_repository() {
    let (ledger, project) = seeded();
    ledger
        .conn()
        .execute(
            "UPDATE workspace_identities SET repo_id = NULL, basis = 'unresolvable'
              WHERE project_id = ?1",
            [&project],
        )
        .expect("unresolve");
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let c = compile(&ledger, &project);
    assert!(!c.repository.resolves);
    let sealed = capsule_v2::seal(&store, &c).expect("seal");
    let out = capsule_v2::record_capsule_created(
        &ledger,
        &sealed,
        Path::new("/tmp/x.json"),
        NOW,
        "cli:test:1",
    )
    .expect("record");
    match out {
        CapsuleRecordOutcome::NoRepositoryIdentity { reason } => {
            assert!(
                reason.contains("guess"),
                "the abstention names its rule: {reason}"
            );
        }
        CapsuleRecordOutcome::Recorded(_) => {
            panic!("an unresolvable scope was recorded against a guess")
        }
    }
    assert_eq!(
        thruline_core::observation::verify_chain(&ledger).expect("verify"),
        0,
        "the abstention appended nothing"
    );
}

/* ── §34: no continuity vocabulary on any v2 surface ────────────────── */

/// The c43-style wording walk, applied to the new surface: a capsule is an
/// OFFER of context. No v2 render or document may use graduated continuity
/// vocabulary — receipt is not acceptance, and offering is not recovery.
#[test]
fn no_v2_surface_speaks_continuity_vocabulary() {
    let (ledger, project) = seeded();
    // Walk both an ordinary capsule and a fully-populated synthetic one, so
    // every render path is covered.
    for c in [compile(&ledger, &project), {
        let mut v = vector_v2();
        v.capsule_id = capsule_id(&v).expect("id");
        v
    }] {
        let md = capsule_v2::render_markdown(&c);
        let json = serde_json::to_string(&c).expect("json");
        for banned in [
            "continuity",
            "recovered",
            "resumed",
            "resume the work", // "resume the migration" fixture text is user content, not ours
            "seamless",
            "handoff complete",
            "successor accepted",
            "work continues",
            "picks up where",
        ] {
            for (surface, text) in [("markdown", &md), ("json", &json)] {
                assert!(
                    !text.to_lowercase().contains(banned),
                    "the v2 {surface} says `{banned}` — a continuity claim with no observation \
                     behind it"
                );
            }
        }
        // And the render states the §3 rule in the affirmative.
        assert!(
            md.contains("Receiving it establishes nothing"),
            "the offer-not-acceptance statement is gone"
        );
    }
}

/// §5 is absolute in capsule copy: the advisory sentence is verbatim, and
/// nothing in the module phrases a no-proof note as an authoritative fact.
#[test]
fn the_advisory_phrasing_is_exact_and_never_authoritative() {
    assert_eq!(
        ADVISORY_NO_PROOF,
        "claim observed; required supporting proof was not found within the stated search scope"
    );
    let (ledger, project) = seeded();
    let c = compile(&ledger, &project);
    assert!(!c.agent_assertions.is_empty());
    for a in &c.agent_assertions {
        assert_eq!(a.advisory.as_deref(), Some(ADVISORY_NO_PROOF));
        assert_eq!(a.state, "ASSERTED");
    }
    let md = capsule_v2::render_markdown(&c);
    for banned in ["false claim", "proof of falsehood", "fabricated"] {
        assert!(
            !md.to_lowercase().contains(banned),
            "an advisory note drifted into an accusation: `{banned}`"
        );
    }
}

/* ── round trips and empties ────────────────────────────────────────── */

#[test]
fn the_document_round_trips_and_empty_categories_state_their_kind() {
    let (ledger, project) = seeded();
    let c = compile(&ledger, &project);
    let json = serde_json::to_string(&c).expect("json");
    let back: CapsuleV2 = serde_json::from_str(&json).expect("parse");
    assert_eq!(back, c);
    capsule_v2::verify_capsule(&back).expect("verifies after round trip");

    // decisions_constraints has no source object in the model — the empty
    // declaration says exactly that, not a measured zero.
    let decl = c
        .category_empties
        .iter()
        .find(|e| e.category == "decisions_constraints")
        .expect("decisions empty is declared");
    assert_eq!(decl.kind, "no_source_object");
    // continuation_state without a snapshot is also structural.
    let cont = c
        .category_empties
        .iter()
        .find(|e| e.category == "continuation_state")
        .expect("continuation empty is declared");
    assert_eq!(cont.kind, "no_source_object");
    // A populated category has no empty declaration.
    assert!(c
        .category_empties
        .iter()
        .all(|e| e.category != "agent_assertions"));
}
