//! A207 (MANDATE-v4 §13 / §33) — the evidence-binding document, its store,
//! the §9 vocabulary, scope classification, and the transcript ceiling.
//!
//! The golden vectors below were computed OUTSIDE the code under test:
//! python3 hashlib over the documented EVB_V1 canonical layout, assembled by
//! two independent implementations that had to agree, before the Rust
//! encoder ran (§32: never generate expected hashes with the code being
//! tested). If they ever disagree with the implementation, the
//! implementation moved — a versioned migration to EVB_V2, never a test
//! update.

use thruline_core::evidence_binding::{
    all_for_repo, assess_transcript_command, binding_id, bound_identity, canonical_bytes,
    classify_command, establishes_all_tests, latest_for_scope, latest_per_scope, load,
    record_command_bound, record_command_started, state_at_binding, store, ArtifactDigest,
    CommandScope, EvidenceBinding, EvidenceState, ExecMode, IdentityEdge, ScopeClass, StoreOutcome,
    StreamCapture, WsidScopeDecl,
};
use thruline_core::ledger::Ledger;
use thruline_core::observation::{of_type, verify_chain, ObservationType};

const EMPTY_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Vector E1: minimal direct run, everything complete.
fn vector_e1() -> EvidenceBinding {
    EvidenceBinding {
        argv: vec!["true".into()],
        mode: ExecMode::Direct,
        cwd: "/w".into(),
        root: "/w".into(),
        repository_identity: "repo:9f8a".into(),
        scope: CommandScope {
            class: ScopeClass::Unclassified,
            key: "bash:/w:true".into(),
        },
        wsid_scope: WsidScopeDecl {
            include_untracked: true,
            exclusions: vec![],
        },
        pre: IdentityEdge::complete(format!("wsid1:{}", "ab".repeat(32))),
        post: IdentityEdge::complete(format!("wsid1:{}", "ab".repeat(32))),
        exit_code: Some(0),
        term_signal: None,
        stdout: StreamCapture::Captured {
            sha256: EMPTY_SHA.into(),
            bytes: 0,
        },
        stderr: StreamCapture::Captured {
            sha256: EMPTY_SHA.into(),
            bytes: 0,
        },
        artifacts: vec![],
        wrapper_defects: vec![],
        started_at: "2026-09-01T10:00:00Z".into(),
        bound_at: "2026-09-01T10:00:01Z".into(),
        source_adapter: "thruline".into(),
        source_session: "cli:check:2026-09-01T10:00:00Z".into(),
    }
}

/// Vector E2: shell mode, incomplete post edge, truncated stderr capture,
/// artifacts, a recorded defect — the UNKNOWN-at-binding arm.
fn vector_e2() -> EvidenceBinding {
    EvidenceBinding {
        argv: vec![
            "/bin/bash".into(),
            "-o".into(),
            "pipefail".into(),
            "-c".into(),
            "npm test | grep -v noise".into(),
        ],
        mode: ExecMode::Shell,
        cwd: "/repo/sub".into(),
        root: "/repo".into(),
        repository_identity: "repo:0f".into(),
        scope: CommandScope {
            class: ScopeClass::Test,
            key: "bash:/repo/sub:npm test|grep".into(),
        },
        wsid_scope: WsidScopeDecl {
            include_untracked: true,
            exclusions: vec!["scratch/".into(), "tmp/".into()],
        },
        pre: IdentityEdge::complete(format!("wsid1:{}", "cd".repeat(32))),
        post: IdentityEdge::incomplete(
            "the workspace changed during observation — capture again".into(),
        ),
        exit_code: Some(1),
        term_signal: None,
        stdout: StreamCapture::Captured {
            sha256: "54034ac5c6e9ea95734ec2b729fd6d62abf64af34a9f9ce5d466cb788191a73d".into(),
            bytes: 4,
        },
        stderr: StreamCapture::Truncated {
            reason: "read failed: broken pipe".into(),
        },
        artifacts: vec![
            ArtifactDigest {
                path: "dist/app.js".into(),
                sha256: Some("11".repeat(32)),
            },
            ArtifactDigest {
                path: "logs/x.log".into(),
                sha256: None,
            },
        ],
        wrapper_defects: vec!["post-command observation was incomplete".into()],
        started_at: "2026-09-01T11:00:00Z".into(),
        bound_at: "2026-09-01T11:00:09Z".into(),
        source_adapter: "thruline".into(),
        source_session: "cli:check:2026-09-01T11:00:00Z".into(),
    }
}

/// Vector E3: signal death — no exit code, a terminating signal.
fn vector_e3() -> EvidenceBinding {
    EvidenceBinding {
        argv: vec!["sh".into(), "-c".into(), "kill -TERM $$".into()],
        mode: ExecMode::Direct,
        cwd: "/w".into(),
        root: "/w".into(),
        repository_identity: "repo:9f8a".into(),
        scope: CommandScope {
            class: ScopeClass::Unclassified,
            key: "bash:/w:sh -c kill -term $$".into(),
        },
        wsid_scope: WsidScopeDecl {
            include_untracked: false,
            exclusions: vec![],
        },
        pre: IdentityEdge::complete(format!("wsid1:{}", "ef".repeat(32))),
        post: IdentityEdge::complete(format!("wsid1:{}", "ef".repeat(32))),
        exit_code: None,
        term_signal: Some(15),
        stdout: StreamCapture::Captured {
            sha256: EMPTY_SHA.into(),
            bytes: 0,
        },
        stderr: StreamCapture::Captured {
            sha256: EMPTY_SHA.into(),
            bytes: 0,
        },
        artifacts: vec![],
        wrapper_defects: vec![],
        started_at: "2026-09-01T12:00:00Z".into(),
        bound_at: "2026-09-01T12:00:01Z".into(),
        source_adapter: "thruline".into(),
        source_session: "cli:check:2026-09-01T12:00:00Z".into(),
    }
}

#[test]
fn golden_vectors_hand_computed_outside_this_code() {
    // python3, 2026-09-01: two independent byte assemblies agreed on every
    // vector before the Rust encoder existed.
    let e1 = vector_e1();
    assert_eq!(state_at_binding(&e1), EvidenceState::CurrentStateConfirmed);
    let bytes = canonical_bytes(&e1);
    assert_eq!(bytes.len(), 864, "hand-computed canonical length, E1");
    assert_eq!(
        binding_id(&e1),
        "4961cce94332dcb0d6a91e50a5db420f6cc64b91dbd930d682bb6aa832824dfc"
    );

    let e2 = vector_e2();
    assert_eq!(state_at_binding(&e2), EvidenceState::Unknown);
    let bytes = canonical_bytes(&e2);
    assert_eq!(bytes.len(), 1062, "hand-computed canonical length, E2");
    assert_eq!(
        binding_id(&e2),
        "cfdf209b441164573ef47aaa17d9e39f90e5b7a2e11c59cc1d41e9df82867719"
    );

    let e3 = vector_e3();
    assert_eq!(
        state_at_binding(&e3),
        EvidenceState::CurrentStateConfirmed,
        "a signal death still binds cleanly when the identities bracket"
    );
    let bytes = canonical_bytes(&e3);
    assert_eq!(bytes.len(), 909, "hand-computed canonical length, E3");
    assert_eq!(
        binding_id(&e3),
        "353c0dcd0ad5a086052c2e35332fdb0429c0ded313db3bb9fb3a27c9edac76de"
    );
}

#[test]
fn the_section_9_vocabulary_is_closed_at_eleven_and_verbatim() {
    assert_eq!(EvidenceState::ALL.len(), 11, "§9's full set, no more");
    let expected = [
        "ASSERTED",
        "RECORD_SUPPORTED",
        "CURRENT_STATE_CONFIRMED",
        "POLICY_SATISFIED",
        "UNKNOWN",
        "INCOMPLETE_SEARCH",
        "STALE",
        "INVALIDATED_BY_LATER_CHANGE",
        "SUBSEQUENTLY_CONFIRMED",
        "CONTRADICTED",
        "HUMAN_ADJUDICATED",
    ];
    for (state, name) in EvidenceState::ALL.iter().zip(expected) {
        assert_eq!(state.as_str(), name, "§9 name, verbatim");
        assert_eq!(EvidenceState::parse(name), Some(*state), "round trip");
    }
    // No legacy verdict string is a member of the new vocabulary.
    for legacy in ["verified", "supported", "observed", "no_proof_found"] {
        assert_eq!(EvidenceState::parse(legacy), None);
    }
}

#[test]
fn every_field_changed_alone_moves_the_id() {
    let reference = binding_id(&vector_e1());
    type Mutation<'a> = (&'a str, Box<dyn Fn(&mut EvidenceBinding)>);
    let mutations: Vec<Mutation<'_>> = vec![
        ("argv", Box::new(|b| b.argv.push("x".into()))),
        ("mode", Box::new(|b| b.mode = ExecMode::Shell)),
        ("cwd", Box::new(|b| b.cwd = "/x".into())),
        ("root", Box::new(|b| b.root = "/x".into())),
        (
            "repo",
            Box::new(|b| b.repository_identity = "repo:x".into()),
        ),
        (
            "scope class",
            Box::new(|b| b.scope.class = ScopeClass::Test),
        ),
        (
            "scope key",
            Box::new(|b| b.scope.key = "bash:/w:false".into()),
        ),
        (
            "wsid scope flag",
            Box::new(|b| b.wsid_scope.include_untracked = false),
        ),
        (
            "wsid exclusions",
            Box::new(|b| b.wsid_scope.exclusions.push("t/".into())),
        ),
        (
            "pre identity",
            Box::new(|b| b.pre = IdentityEdge::complete(format!("wsid1:{}", "aa".repeat(32)))),
        ),
        (
            "post identity",
            Box::new(|b| b.post = IdentityEdge::incomplete("moved".into())),
        ),
        ("exit code", Box::new(|b| b.exit_code = Some(1))),
        ("signal", Box::new(|b| b.term_signal = Some(9))),
        (
            "stdout digest",
            Box::new(|b| {
                b.stdout = StreamCapture::Captured {
                    sha256: "ab".repeat(32),
                    bytes: 0,
                }
            }),
        ),
        (
            "stdout byte count",
            Box::new(|b| {
                b.stdout = StreamCapture::Captured {
                    sha256: EMPTY_SHA.into(),
                    bytes: 1,
                }
            }),
        ),
        (
            "stderr capture arm",
            Box::new(|b| {
                b.stderr = StreamCapture::Truncated {
                    reason: "torn".into(),
                }
            }),
        ),
        (
            "artifacts",
            Box::new(|b| {
                b.artifacts.push(ArtifactDigest {
                    path: "a".into(),
                    sha256: None,
                })
            }),
        ),
        (
            "defects",
            Box::new(|b| b.wrapper_defects.push("late".into())),
        ),
        (
            "started",
            Box::new(|b| b.started_at = "2027-01-01T00:00:00Z".into()),
        ),
        (
            "bound",
            Box::new(|b| b.bound_at = "2027-01-01T00:00:01Z".into()),
        ),
        ("adapter", Box::new(|b| b.source_adapter = "codex".into())),
        ("session", Box::new(|b| b.source_session = "cli:x".into())),
    ];
    let mut seen = std::collections::BTreeSet::new();
    seen.insert(reference.clone());
    for (name, mutate) in mutations {
        let mut b = vector_e1();
        mutate(&mut b);
        let id = binding_id(&b);
        assert_ne!(id, reference, "changing {name} must move the id");
        assert!(seen.insert(id), "{name} collided with another mutation");
    }
}

#[test]
fn encoding_is_injective_across_boundaries_and_nuisance_order_is_not_identity() {
    // argv token boundaries: ["ab"] vs ["a","b"].
    let mut one = vector_e1();
    one.argv = vec!["ab".into()];
    let mut two = vector_e1();
    two.argv = vec!["a".into(), "b".into()];
    assert_ne!(binding_id(&one), binding_id(&two));

    // Absent vs empty on an optional field.
    let mut none = vector_e1();
    none.term_signal = None;
    let mut zero = vector_e1();
    zero.term_signal = Some(0);
    assert_ne!(binding_id(&none), binding_id(&zero));

    // Exclusion order and duplication are nuisance, not identity.
    let mut sorted = vector_e2();
    sorted.wsid_scope.exclusions = vec!["scratch/".into(), "tmp/".into()];
    let mut shuffled = vector_e2();
    shuffled.wsid_scope.exclusions = vec!["tmp/".into(), "scratch/".into(), "tmp/".into()];
    assert_eq!(binding_id(&sorted), binding_id(&shuffled));

    // Artifact arrival order is nuisance too — the canonical form sorts.
    let mut swapped = vector_e2();
    swapped.artifacts.reverse();
    assert_eq!(binding_id(&vector_e2()), binding_id(&swapped));
}

#[test]
fn scope_classification_reuses_the_closed_recognition_sets() {
    let all = classify_command("cargo test --workspace", "/repo");
    assert_eq!(all.class, ScopeClass::Test);
    assert!(
        establishes_all_tests(&all),
        "an un-narrowed suite qualifies"
    );

    let narrow = classify_command("cargo test -p thruline-core", "/repo");
    assert_eq!(narrow.class, ScopeClass::Test);
    assert!(
        !establishes_all_tests(&narrow),
        "a narrow suite cannot establish an all-tests claim: {}",
        narrow.key
    );
    assert_ne!(all.key, narrow.key, "the selector is identity-bearing");

    let filtered = classify_command("npm test | grep -E '^not ok'", "/repo");
    assert_eq!(filtered.class, ScopeClass::Test);
    assert!(
        !establishes_all_tests(&filtered),
        "a filter stage narrows what the run appears to report"
    );

    let build = classify_command("cargo build --release", "/repo");
    assert_eq!(build.class, ScopeClass::Build);
    assert!(!establishes_all_tests(&build));

    let push = classify_command("git push origin main", "/repo");
    assert_eq!(push.class, ScopeClass::Unclassified);
    assert!(
        !establishes_all_tests(&push),
        "unclassified can bind — the execution is real — but can NEVER \
         satisfy an all-tests claim"
    );

    let script = classify_command("./test.sh", "/repo");
    assert_eq!(script.class, ScopeClass::Test);
    assert!(
        establishes_all_tests(&script),
        "a bare test script qualifies"
    );
    let script_args = classify_command("./test.sh --filter eviction", "/repo");
    assert!(
        !establishes_all_tests(&script_args),
        "a script with surviving arguments could be narrowed — undecidability \
         resolves toward no"
    );
}

/// The mirror-list drift guard: every bare runner the all-tests predicate
/// accepts must still be recognised by the §5i closed test set.
#[test]
fn the_all_tests_runner_list_cannot_drift_from_the_closed_classification() {
    for runner in [
        "cargo test",
        "npm test",
        "pnpm test",
        "yarn test",
        "swift test",
        "node --test",
        "pytest",
        "vitest",
        "jest",
        "go test",
    ] {
        let scope = classify_command(runner, "/repo");
        assert_eq!(
            scope.class,
            ScopeClass::Test,
            "{runner} must satisfy the §5i test recognition this module reuses"
        );
        assert!(establishes_all_tests(&scope), "{runner} bare = whole suite");
    }
}

#[test]
fn store_load_latest_and_tamper_detection() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let e1 = vector_e1();
    let StoreOutcome::Stored(stored) = store(&ledger, &e1).expect("store") else {
        panic!("first store must be new");
    };
    assert_eq!(stored.id, binding_id(&e1));
    assert_eq!(stored.seq, 0);

    // Idempotent by content identity.
    let again = store(&ledger, &e1).expect("store again");
    assert!(matches!(again, StoreOutcome::AlreadyStored(_)));

    // Load round-trips and re-verifies.
    let loaded = load(&ledger, &stored.id).expect("load").expect("present");
    assert_eq!(loaded.binding, e1);

    // A second binding for the same scope: latest_for_scope picks it.
    let mut later = e1.clone();
    later.bound_at = "2026-09-01T10:05:00Z".into();
    later.exit_code = Some(1);
    let StoreOutcome::Stored(second) = store(&ledger, &later).expect("store later") else {
        panic!("distinct content is a new row");
    };
    assert_eq!(second.seq, 1);
    let latest = latest_for_scope(&ledger, &e1.scope.key)
        .expect("query")
        .expect("present");
    assert_eq!(latest.id, second.id, "latest by seq, one indexed query");

    // latest_per_scope: one row per scope key for the repo.
    let e3 = vector_e3();
    store(&ledger, &e3).expect("store e3");
    let latest_all = latest_per_scope(&ledger, "repo:9f8a").expect("latest per scope");
    assert_eq!(latest_all.len(), 2, "two scopes, one latest each");
    assert_eq!(all_for_repo(&ledger, "repo:9f8a").expect("all").len(), 3);

    // Append-only triggers hold.
    assert!(
        ledger
            .conn()
            .execute("UPDATE evidence_bindings SET exit_code = 0", [])
            .is_err(),
        "updates must abort"
    );
    assert!(
        ledger
            .conn()
            .execute("DELETE FROM evidence_bindings", [])
            .is_err(),
        "deletes must abort"
    );

    // A tampered document is named on load — both digest layers.
    ledger
        .conn()
        .execute_batch(&format!(
            "DROP TRIGGER evidence_bindings_no_update;
             UPDATE evidence_bindings SET doc = replace(doc, '\"exit_code\": 0', '\"exit_code\": 9')
              WHERE id = '{}';",
            stored.id
        ))
        .expect("forge");
    let err = load(&ledger, &stored.id).expect_err("a rewritten doc must fail to load");
    assert!(
        err.to_string().contains("digest") || err.to_string().contains("canonical"),
        "the error names the mismatch: {err}"
    );

    // Rewriting the doc AND its doc_sha256 consistently still fails: the
    // canonical id no longer derives from the altered content.
    let forged_doc: String = ledger
        .conn()
        .query_row(
            "SELECT doc FROM evidence_bindings WHERE id = ?1",
            [&stored.id],
            |r| r.get(0),
        )
        .expect("read forged");
    let forged_sha = thruline_core::chain::sha256_hex(forged_doc.as_bytes());
    ledger
        .conn()
        .execute(
            "UPDATE evidence_bindings SET doc_sha256 = ?1 WHERE id = ?2",
            rusqlite::params![forged_sha, stored.id],
        )
        .expect("re-seal");
    let err = load(&ledger, &stored.id).expect_err("a consistently re-sealed forgery still fails");
    assert!(
        err.to_string().contains("canonical"),
        "the canonical id is the second lock: {err}"
    );
}

#[test]
fn the_observation_writers_append_the_section_11_trail() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let e1 = vector_e1();
    record_command_started(&ledger, &e1, 4242).expect("started");
    let StoreOutcome::Stored(stored) = store(&ledger, &e1).expect("store") else {
        panic!("new");
    };
    let (finished_id, bound_id) = record_command_bound(&ledger, &stored).expect("bound");

    let started = of_type(&ledger, ObservationType::CommandStarted).expect("read");
    assert_eq!(started.len(), 1);
    assert_eq!(
        started[0].workspace_identity.as_deref(),
        e1.pre.identity.as_deref(),
        "the started observation carries the pre-command identity"
    );

    let finished = of_type(&ledger, ObservationType::CommandFinished).expect("read");
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].id, finished_id);
    let receipt = finished[0].receipt().expect("receipt");
    match receipt.terminal {
        thruline_core::observation::ReceiptTerminal::ProcessExecution { exit_code, .. } => {
            assert_eq!(exit_code, Some(0), "the real exit code, in the terminal");
        }
        other => panic!("the terminal must be a process execution: {other:?}"),
    }

    let bound = of_type(&ledger, ObservationType::EvidenceBound).expect("read");
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].id, bound_id);
    let receipt = bound[0].receipt().expect("receipt");
    assert_eq!(receipt.derived_refs.len(), 1);
    assert_eq!(receipt.derived_refs[0].table, "evidence_bindings");
    assert_eq!(receipt.derived_refs[0].id, stored.id);
    assert_eq!(
        receipt
            .extra
            .get("state_at_binding")
            .and_then(|v| v.as_str()),
        Some("CURRENT_STATE_CONFIRMED"),
        "§9 verbatim in the hash-covered receipt"
    );

    assert_eq!(verify_chain(&ledger).expect("verify"), 3);
}

#[test]
fn bound_identity_requires_a_clean_bracket() {
    let e1 = vector_e1();
    assert!(bound_identity(&e1).is_some());

    let mut straddled = vector_e1();
    straddled.post = IdentityEdge::complete(format!("wsid1:{}", "ba".repeat(32)));
    assert_eq!(
        bound_identity(&straddled),
        None,
        "pre≠post straddles two states — nothing to confirm"
    );
    assert_eq!(state_at_binding(&straddled), EvidenceState::Unknown);

    let e2 = vector_e2();
    assert_eq!(bound_identity(&e2), None, "an incomplete edge never binds");
}

/* ── the §3 transcript ceiling, pinned ──────────────────────────────── */

/// A transcript asserting a push, maximally realistic: real porcelain
/// shapes for `git push` output. However real it looks, the record shows
/// text — not an executed push, and no remote fact.
#[test]
fn a_forged_git_transcript_cannot_rise_above_record_supported() {
    let forged = "Enumerating objects: 12, done.\n\
        Counting objects: 100% (12/12), done.\n\
        Delta compression using up to 10 threads\n\
        Compressing objects: 100% (6/6), done.\n\
        Writing objects: 100% (7/7), 1.24 KiB | 1.24 MiB/s, done.\n\
        Total 7 (delta 4), reused 0 (delta 0), pack-reused 0\n\
        remote: Resolving deltas: 100% (4/4), completed with 4 local objects.\n\
        To github.com:acme/widget.git\n\
        \x20\x20 e181ea7..9c40afa  main -> main\n";
    let assessment = assess_transcript_command(forged);
    assert_eq!(
        assessment.state,
        EvidenceState::RecordSupported,
        "the ceiling: the record shows this text, nothing more"
    );
    assert_ne!(assessment.state, EvidenceState::CurrentStateConfirmed);
    for banned in ["pushed", "remote ref", "the push succeeded"] {
        assert!(
            !assessment
                .note
                .to_lowercase()
                .starts_with(&banned.to_lowercase()),
            "no remote fact may be derived: {}",
            assessment.note
        );
    }
    assert!(
        assessment.note.contains("independent remote query"),
        "the refusal names the missing evidence: {}",
        assessment.note
    );
    assert!(assessment.overflow.is_none());
}

#[test]
fn an_unresolved_overflow_reference_is_an_incomplete_search() {
    let overflowed = "<persisted-output>\nOutput too large (113.4KB). Full output saved to: \
        /Users/u/.claude/projects/-p/sess/tool-results/b3ycw97ws.txt\n\nPreview (first 2KB):\n\
        running 916 tests…\n</persisted-output>";
    let assessment = assess_transcript_command(overflowed);
    assert_eq!(
        assessment.state,
        EvidenceState::IncompleteSearch,
        "only the excerpt was inspectable — §13 forbids calling that a search"
    );
    let overflow = assessment.overflow.expect("the reference is disclosed");
    assert!(overflow.path.ends_with("tool-results/b3ycw97ws.txt"));
    assert_eq!(overflow.stated_size.as_deref(), Some("113.4KB"));
    assert!(
        assessment.note.contains("incomplete"),
        "the incompleteness is stated, not implied: {}",
        assessment.note
    );
}

/// No legacy verdict string leaks through the new surfaces (the §9 rule):
/// the states, the notes, and the stored receipts speak §9 or plain prose.
#[test]
fn no_legacy_verdict_string_leaks_into_the_new_surfaces() {
    let texts = [
        assess_transcript_command("ok").note,
        assess_transcript_command(
            "<persisted-output>\nOutput too large (1KB). Full output saved to: /x/y.txt\n",
        )
        .note,
    ];
    for text in texts {
        for legacy in ["verified", "no_proof_found", "supported"] {
            assert!(
                !text.contains(legacy),
                "legacy verdict string {legacy:?} leaked: {text}"
            );
        }
    }
}
