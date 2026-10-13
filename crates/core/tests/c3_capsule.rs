//! C3.3 — the capsule's laws, each with teeth (A174).
//!
//! (a) no line without a provenance label · (b) the RB7 semantic-word ban
//! extended to capsule strings · (c) a secret canary cannot reach any capsule
//! byte · (d) a doctored capsule refuses to load · (e) an empty scope renders
//! honest measured-zero empties · (f) caution can never be populated from
//! anything but the coverage-scoped no-proof-found class — and in v1 it is
//! withheld rather than rendered, pending the §5s ruling A173 surfaced.

use serde_json::Value;
use thruline_core::capsule::{self, Capsule, Empty, Provenance};
use thruline_core::ledger::Ledger;

const NOW: &str = "2026-08-13T12:00:00Z";

/// A scope with one session, one supported claim, one unknown claim, one
/// unsupported claim (the shadow class), and a side-effect-shaped command.
fn seeded() -> (Ledger, String) {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let project = "-tmp-c3-project".to_string();
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
               VALUES ('-tmp-c3-project','/tmp/c3','claude_code','c3','2026-08-13T00:00:00Z');
             INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
               VALUES ('s1','-tmp-c3-project','/t/s1.jsonl','2026-08-13T00:00:00Z',3);
             INSERT INTO events(id, project_id, session_id, seq, ts, kind, text, tool_name,
                                tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
               VALUES ('e1','-tmp-c3-project','s1',0,'2026-08-13T00:00:01Z','user_msg',
                       'Make the notification digest work',NULL,NULL,'/t/s1.jsonl',1,'x','g','c1'),
                      ('e2','-tmp-c3-project','s1',1,'2026-08-13T00:00:02Z','tool_use',NULL,
                       'Bash','{\"command\":\"git push origin main\"}','/t/s1.jsonl',2,'x','c1','c2'),
                      ('e3','-tmp-c3-project','s1',2,'2026-08-13T00:00:03Z','assistant_text',
                       'Digest works now',NULL,NULL,'/t/s1.jsonl',3,'x','c2','c3');
             INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                               currency, session_id, anchor_event_id, derived_rev)
               VALUES ('n1','-tmp-c3-project','digest collapses bursts','supported','completed',
                       'supported','current','s1','e3',1),
                      ('n2','-tmp-c3-project','something unreadable','unknown','active',
                       'unknown','current','s1','e3',1),
                      ('n3','-tmp-c3-project','a claim with no proof','unsupported','completed',
                       'unknown','current','s1','e3',1);",
        )
        .expect("seed");
    (ledger, project)
}

fn compile(ledger: &Ledger, project: &str) -> Capsule {
    capsule::compile(ledger, project, None, NOW).expect("compile")
}

#[test]
fn law_a_no_line_renders_without_a_provenance_label() {
    let (ledger, project) = seeded();
    let cap = compile(&ledger, &project);
    let mut lines = 0usize;
    for section in &cap.sections {
        for line in &section.lines {
            match &line.provenance {
                Provenance::EvidenceBacked { receipts } => {
                    assert!(
                        !receipts.is_empty(),
                        "an evidence-backed line carries no receipt: {:?}",
                        line.text
                    );
                    for r in receipts {
                        assert!(!r.event_id.is_empty(), "a receipt names no event");
                    }
                }
                Provenance::Inferred { basis } => assert!(
                    basis.len() > 10,
                    "an inferred line carries no real basis: {:?}",
                    line.text
                ),
                Provenance::UserNote { .. } => {}
            }
            lines += 1;
        }
    }
    assert!(lines >= 4, "the walk covered only {lines} lines");
    // And the JSON form carries it too — the label is not a render-time
    // decoration, it is in the data.
    let json: Value = serde_json::to_value(&cap).expect("json");
    for section in json["sections"].as_array().expect("sections") {
        for line in section["lines"].as_array().expect("lines") {
            assert!(
                line["provenance"]["kind"].is_string(),
                "a serialized line lost its provenance: {line}"
            );
        }
    }
}

#[test]
fn law_b_no_layer_three_semantic_word_reaches_a_capsule_string() {
    // The RB7 proxy rule, extended to the newest surface: a Layer-2 structure
    // is a proxy for a possible meaning and must never wear the meaning's name.
    let (ledger, project) = seeded();
    let cap = compile(&ledger, &project);
    let rendered = capsule::render_markdown(&cap);
    let json = serde_json::to_string(&cap).expect("json");
    for banned in [
        "dead end",
        "dead-end",
        "abandoned",
        "the purpose",
        "a decision was",
    ] {
        for (what, text) in [("markdown", &rendered), ("json", &json)] {
            assert!(
                !text.to_lowercase().contains(banned),
                "the capsule's {what} says `{banned}` — a Layer-3 meaning with no provenance \
                 carrying it"
            );
        }
    }
    // The candidate vocabulary is present where the meaning would have been.
    assert!(
        rendered.contains("unresolved") || rendered.contains("looked, and found none"),
        "the in-flight section lost its candidate vocabulary"
    );
    // And the section that would most tempt a meaning says a position instead.
    assert!(
        !rendered.contains("next safe action"),
        "the capsule claims safety, which is a C5 assessment it has no rows for"
    );
}

#[test]
fn law_c_a_secret_canary_reaches_no_capsule_byte() {
    let (ledger, project) = seeded();
    std::env::set_var("THRULINE_C3_CANARY", "supersecret-capsule-canary-4471");
    let snap = thruline_core::snapshot::capture(std::path::Path::new("/tmp"), NOW).expect("snap");
    std::env::remove_var("THRULINE_C3_CANARY");
    let cap = capsule::compile(&ledger, &project, Some(&snap), NOW).expect("compile");
    let json = serde_json::to_string(&cap).expect("json");
    let md = capsule::render_markdown(&cap);
    for (what, text) in [("json", &json), ("markdown", &md)] {
        assert!(
            !text.contains("supersecret-capsule-canary-4471"),
            "an environment VALUE reached the capsule's {what} — the names-never-values law \
             does not survive the snapshot citation"
        );
    }
    // The policy states honestly that nothing was scrubbed, because nothing was.
    assert!(!cap.secrets_policy.scrubbed);
    assert!(
        !json.contains("secrets excluded by default"),
        "the capsule prints a safety promise no test backs"
    );
}

#[test]
fn law_d_a_doctored_capsule_refuses_to_load() {
    let (ledger, project) = seeded();
    let cap = compile(&ledger, &project);
    let json = serde_json::to_string(&cap).expect("json");
    // Strip a line's provenance — the exact doctoring the schema forbids.
    let mut doc: Value = serde_json::from_str(&json).expect("value");
    doc["sections"][0]["lines"][0]
        .as_object_mut()
        .expect("line")
        .remove("provenance");
    let reload = serde_json::from_value::<Capsule>(doc);
    assert!(
        reload.is_err(),
        "a line without provenance deserialized — law 1 is decoration, not structure"
    );
}

#[test]
fn law_e_an_empty_scope_renders_honest_measured_zero_empties() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO projects(id, root_path, source, created_at)
               VALUES ('-tmp-empty','/tmp/empty','claude_code','2026-08-13T00:00:00Z');",
        )
        .expect("seed");
    let cap = capsule::compile(&ledger, "-tmp-empty", None, NOW).expect("compile");
    let mut empties = 0usize;
    for section in &cap.sections {
        if section.lines.is_empty() {
            let empty = section
                .empty
                .as_ref()
                .unwrap_or_else(|| panic!("section {} is empty and says nothing", section.key));
            let reason = match empty {
                Empty::NoSourceObject { reason }
                | Empty::SourceQueriedAndEmpty { reason }
                | Empty::Withheld { reason } => reason,
            };
            assert!(
                reason.len() > 15,
                "section {} gives no real reason: {reason:?}",
                section.key
            );
            empties += 1;
        }
    }
    assert!(
        empties >= 5,
        "only {empties} sections rendered honest empties"
    );
    // Freshness is never empty: the chain position always exists.
    let freshness = cap
        .sections
        .iter()
        .find(|s| s.key == "freshness")
        .expect("freshness section");
    assert!(!freshness.lines.is_empty());
    assert!(
        cap.scope.sessions == 0 && !cap.scope.repo_identity.resolves,
        "an empty scope must still state its scope honestly"
    );
}

#[test]
fn law_f_r1_caution_renders_a_count_under_a_shadow_heading_and_names_no_claim() {
    // §5s-amendment (R1): an aggregate count of an ungraduated class may render
    // on a default surface IFF it sits under an explicit ungraduated heading
    // and carries NO per-claim identity. Both halves are checked here, because
    // either half alone would be the wrong law.
    let (ledger, project) = seeded();
    let cap = compile(&ledger, &project);
    let caution = cap
        .sections
        .iter()
        .find(|s| s.key == "caution")
        .expect("caution section");

    // (a) the heading declares the class ungraduated.
    let title = caution.title.to_lowercase();
    assert!(
        title.contains("ungraduated") || title.contains("shadow"),
        "a shadow count rendered without its heading: {:?}",
        caution.title
    );
    // (b) exactly one count line, and it points at the flagged surface.
    assert_eq!(
        caution.lines.len(),
        1,
        "caution should carry one count line"
    );
    let line = &caution.lines[0];
    assert!(
        line.text.contains('1'),
        "the count is the content: {:?}",
        line.text
    );
    assert!(
        line.text.contains("--diagnostics"),
        "the count must point at where the receipts live: {:?}",
        line.text
    );
    // (c) NO per-claim identity reaches this surface. The seeded unsupported
    // node is `n3` with label "a claim with no proof" — neither may appear.
    let json = serde_json::to_string(&cap).expect("json");
    let md = capsule::render_markdown(&cap);
    for (what, text) in [("json", &json), ("markdown", &md)] {
        assert!(
            !text.contains("a claim with no proof"),
            "an ungraduated claim was NAMED on the default {what} surface"
        );
        assert!(
            !text.contains("\"node:n3\""),
            "an ungraduated claim identity reached the default {what} surface"
        );
    }
    // (d) a count is a derivation, so it is `inferred` with a stated basis —
    // never `evidence_backed`, which would require the receipts R1 withholds.
    match &line.provenance {
        Provenance::Inferred { basis } => assert!(
            basis.contains("ungraduated"),
            "the basis must name the rule that shaped it: {basis}"
        ),
        other => panic!("a withheld-identity count claimed receipts: {other:?}"),
    }
}

#[test]
fn r1_the_diagnostics_surface_is_the_only_place_a_shadow_claim_is_named() {
    let (ledger, project) = seeded();
    let flagged = capsule::compile_with(&ledger, &project, None, NOW, true).expect("compile");
    let caution = flagged
        .sections
        .iter()
        .find(|s| s.key == "caution")
        .expect("caution section");
    assert_eq!(caution.lines.len(), 1, "the one seeded unsupported claim");
    assert!(
        caution.lines[0].text.contains("a claim with no proof"),
        "the flagged surface must actually name the claim: {:?}",
        caution.lines[0].text
    );
    match &caution.lines[0].provenance {
        Provenance::EvidenceBacked { receipts } => assert!(!receipts.is_empty()),
        other => panic!("a named claim rendered without its receipt: {other:?}"),
    }
}

#[test]
fn r3_the_semantic_word_ban_applies_to_inferred_lines_specifically() {
    // R3 ratified `Inferred` as a provenance for POSITIONS and DERIVATIONS with
    // a stated basis — never meanings. RB7's trio stays the only source of
    // meaning, so the ban is pinned on exactly the lines that could smuggle one
    // (including in an adjective: "safe" was the word that tried).
    let (ledger, project) = seeded();
    let mut checked = 0usize;
    for diagnostics in [false, true] {
        let cap = capsule::compile_with(&ledger, &project, None, NOW, diagnostics).expect("c");
        for section in &cap.sections {
            for line in &section.lines {
                let Provenance::Inferred { basis } = &line.provenance else {
                    continue;
                };
                let hay = format!("{} {}", line.text, basis).to_lowercase();
                for banned in [
                    "dead end",
                    "dead-end",
                    "abandoned",
                    "purpose",
                    "decision",
                    "outcome",
                    "safe",
                    "unsafe",
                    "dangerous",
                ] {
                    assert!(
                        !hay.contains(banned),
                        "an inferred line carries the Layer-3 word `{banned}`, which only \
                         proof, an operator attestation, or a labeled shadow proposal may \
                         carry: {:?}",
                        line.text
                    );
                }
                assert!(
                    basis.len() > 10,
                    "an inferred line must state its basis: {:?}",
                    line.text
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 3, "the walk found only {checked} inferred lines");
}

#[test]
fn r4_there_is_no_unknown_provenance_variant() {
    // R4: a line that cannot state its origin does not render. The enum is the
    // enforcement — this test fails the moment a fourth arm appears, and the
    // deserializer refuses any kind outside the three.
    let json = r#"{"text":"x","provenance":{"kind":"unknown"}}"#;
    assert!(
        serde_json::from_str::<capsule::Line>(json).is_err(),
        "a line with `unknown` provenance deserialized — R4 is decoration, not structure"
    );
    for kind in ["evidence_backed", "user_note", "inferred"] {
        let ok = match kind {
            "evidence_backed" => {
                r#"{"text":"x","provenance":{"kind":"evidence_backed","receipts":[{"event_id":"e1"}]}}"#
            }
            "user_note" => r#"{"text":"x","provenance":{"kind":"user_note"}}"#,
            _ => r#"{"text":"x","provenance":{"kind":"inferred","basis":"a stated basis here"}}"#,
        };
        assert!(
            serde_json::from_str::<capsule::Line>(ok).is_ok(),
            "the {kind} arm stopped round-tripping"
        );
    }
}

#[test]
fn r2_a_secret_shaped_span_in_the_record_is_masked_and_the_treatment_is_recorded() {
    // The end-to-end R2 property: a secret that a real session typed into a
    // real command must not travel to a fresh model, and the artifact must say
    // what was done rather than what is true.
    let ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
               VALUES ('p2','/tmp/p2','claude_code','p2','2026-08-13T00:00:00Z');
             INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
               VALUES ('s2','p2','/t/s2.jsonl','2026-08-13T00:00:00Z',1);
             INSERT INTO events(id, project_id, session_id, seq, ts, kind, text, tool_name,
                                tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
               VALUES ('x1','p2','s2',0,'2026-08-13T00:00:01Z','user_msg',
                       'deploy with AKIAIOSFODNN7EXAMPLE please',NULL,NULL,'/t/s2.jsonl',1,'x','g','h1'),
                      ('x2','p2','s2',1,'2026-08-13T00:00:02Z','tool_use',NULL,'Bash',
                       '{\"command\":\"git push https://ghp_ZZZZaaaa1111bbbb2222cccc3333dddd@github.com/x/y\"}',
                       '/t/s2.jsonl',2,'x','h1','h2');",
        )
        .expect("seed");
    let cap = capsule::compile(&ledger, "p2", None, NOW).expect("compile");
    let json = serde_json::to_string(&cap).expect("json");
    let md = capsule::render_markdown(&cap);

    for (what, text) in [("json", &json), ("markdown", &md)] {
        assert!(
            !text.contains("AKIAIOSFODNN7EXAMPLE"),
            "an AWS-shaped key survived into the capsule's {what}"
        );
        assert!(
            !text.contains("ghp_ZZZZaaaa1111bbbb2222cccc3333dddd"),
            "a GitHub-shaped token survived into the capsule's {what}"
        );
        assert!(
            text.contains("[masked:"),
            "the mask must be visible where it happened in the {what}"
        );
    }
    // The treatment record says what ran, and refuses to promise absence.
    let t = &cap.secrets_policy.treatment;
    assert!(
        t.masked() >= 2,
        "expected both shapes masked, got {}",
        t.masked()
    );
    match t {
        thruline_core::redact::Treatment::Ran { patterns, .. } => {
            assert!(patterns.contains(&"aws".to_string()));
            assert!(patterns.contains(&"github".to_string()));
        }
        other => panic!("a capsule that masked spans reported {other:?}"),
    }
    assert!(t.statement().contains("best-effort, not a guarantee"));
    assert!(
        t.needs_acknowledgement(),
        "a capsule with masked spans must require acknowledgement before injection"
    );
    // scrubbed stays false forever: a denylist cannot prove absence.
    assert!(!cap.secrets_policy.scrubbed);
    assert!(
        !json.contains("secrets excluded by default"),
        "the capsule made a promise the ruling forbids"
    );
    // The surrounding words survive — a detector that eats the context makes
    // the capsule useless, which is its own failure.
    assert!(md.contains("deploy with") && md.contains("please"));
}

#[test]
fn d2_a_secret_in_an_inferred_basis_is_masked_like_any_other_channel() {
    // D2: the first version masked `line.text` while its own comment claimed
    // the basis strings too. Every RENDERED channel is masked — text, the
    // Inferred basis that explains it, and the scope display name.
    let (ledger, project) = seeded();
    let mut cap = compile(&ledger, &project);
    for section in cap.sections.iter_mut() {
        for line in section.lines.iter_mut() {
            if let Provenance::Inferred { basis } = &mut line.provenance {
                *basis = format!("{basis} · via AKIAIOSFODNN7EXAMPLE");
            }
        }
    }
    cap.scope.display_name = Some("proj ghp_ZZZZaaaa1111bbbb2222cccc3333dddd".to_string());
    // Re-run exactly the masking the compiler runs, over the same channels.
    let mut targets: Vec<&mut String> = Vec::new();
    for section in cap.sections.iter_mut() {
        for line in section.lines.iter_mut() {
            targets.push(&mut line.text);
            if let Provenance::Inferred { basis } = &mut line.provenance {
                targets.push(basis);
            }
        }
    }
    if let Some(name) = cap.scope.display_name.as_mut() {
        targets.push(name);
    }
    let t = thruline_core::redact::mask_all(&mut targets);
    assert!(
        t.masked() >= 2,
        "a seeded secret survived a rendered channel"
    );
    let json = serde_json::to_string(&cap).expect("json");
    let md = capsule::render_markdown(&cap);
    for (what, text) in [("json", &json), ("markdown", &md)] {
        assert!(
            !text.contains("AKIAIOSFODNN7EXAMPLE"),
            "a secret survived in an Inferred basis in the {what}"
        );
        assert!(
            !text.contains("ghp_ZZZZaaaa1111bbbb2222cccc3333dddd"),
            "a secret survived in the scope display name in the {what}"
        );
    }
    // The basis is rendered to humans, so this is not hypothetical.
    assert!(md.contains("inferred ·"), "the basis is a rendered channel");
}

#[test]
fn d3_a_capsule_with_no_treatment_field_fails_toward_consent() {
    // Every pre-A177 capsule and every foreign producer arrives without this
    // field. The gate's default state must be CLOSED.
    let (ledger, project) = seeded();
    let cap = compile(&ledger, &project);
    let mut doc: serde_json::Value = serde_json::to_value(&cap).expect("json");
    doc["secrets_policy"]
        .as_object_mut()
        .expect("policy")
        .remove("treatment");
    let reloaded: Capsule = serde_json::from_value(doc).expect("a treatment-less capsule loads");
    let t = &reloaded.secrets_policy.treatment;
    assert!(
        matches!(t, thruline_core::redact::Treatment::NoDetectorRan { .. }),
        "a missing treatment did not become NoDetectorRan: {t:?}"
    );
    assert!(
        t.needs_acknowledgement(),
        "an unscanned capsule opened the injection gate — this is the D3 defect"
    );
    assert!(t.statement().contains("no detector ran"));
}

#[test]
fn unknown_fields_survive_a_round_trip() {
    // Law 4 of the schema: a newer producer must not lose data through an
    // older consumer.
    let (ledger, project) = seeded();
    let cap = compile(&ledger, &project);
    let mut doc: Value = serde_json::to_value(&cap).expect("json");
    doc.as_object_mut()
        .expect("obj")
        .insert("a_future_field".to_string(), Value::from(42));
    doc["sections"][0]
        .as_object_mut()
        .expect("section")
        .insert("future_section_field".to_string(), Value::from("kept"));
    let reloaded: Capsule = serde_json::from_value(doc).expect("reload");
    let out = serde_json::to_value(&reloaded).expect("re-serialize");
    assert_eq!(out["a_future_field"], Value::from(42));
    assert_eq!(
        out["sections"][0]["future_section_field"],
        Value::from("kept")
    );
}

#[test]
fn the_markdown_and_the_json_are_two_views_of_one_selection() {
    let (ledger, project) = seeded();
    let cap = compile(&ledger, &project);
    let md = capsule::render_markdown(&cap);
    // Every rendered line's text appears in the Markdown, and every section
    // title does: one selection, two views, no second query.
    for section in &cap.sections {
        assert!(
            md.contains(&section.title),
            "section {} is missing from the Markdown view",
            section.key
        );
        for line in &section.lines {
            assert!(
                md.contains(&line.text),
                "a line in {} is in the JSON and not the Markdown: {:?}",
                section.key,
                line.text
            );
        }
    }
}

#[test]
fn observed_operations_carry_a_class_only_when_a_named_rule_entails_it() {
    // Evolved at A182. Before C5 every operation said `unknown` because
    // nothing could say otherwise, and this test forbade the safety words
    // outright. C5's rule table is PROOF provenance under RB7 — a
    // deterministic rule that entails the word — so the words are licensed
    // now, and the law becomes: no class without the rule that produced it,
    // stored on the line and travelling with it.
    let (ledger, project) = seeded();
    let cap = compile(&ledger, &project);
    let ops = cap
        .sections
        .iter()
        .find(|s| s.key == "operations_observed")
        .expect("operations section");
    assert_eq!(ops.lines.len(), 1, "the seeded git push should be observed");
    let line = &ops.lines[0];
    assert!(line.text.contains("git push origin main"));
    // The class came from the rule table, and the outcome was READ (there is
    // no captured result for the seeded call, so it is `unknown`, not a guess
    // at success).
    assert!(
        line.text.contains("replay class conditional"),
        "the class did not come from the rule table: {:?}",
        line.text
    );
    assert!(
        line.text.contains("· unknown ·"),
        "an outcome was assumed rather than read: {:?}",
        line.text
    );
    let json = serde_json::to_value(line).expect("json");
    assert_eq!(json["replay_class"], "conditional");
    assert!(
        json["op_id"].is_string(),
        "an operation with no stable identity"
    );
    let basis = json["replay_class_basis"]
        .as_str()
        .expect("every class carries the rule that entailed it");
    assert!(
        basis.len() > 30,
        "a class with no reasoning behind it is not proof provenance: {basis}"
    );
    // And no class may appear anywhere in the capsule without a basis beside it.
    let whole = serde_json::to_string(&cap).expect("json");
    for word in ["\"unsafe\"", "\"conditional\""] {
        if whole.contains(word) {
            assert!(
                whole.contains("replay_class_basis"),
                "the class {word} appears with no rule stored beside it"
            );
        }
    }
}
