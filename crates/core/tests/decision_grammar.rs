use thruline_core::decisions::grammar::{propose, Actor, Refusal, Source, SourceKind};

fn prompt<'a>(text: &'a str) -> Source<'a> {
    Source {
        kind: SourceKind::WorkPrompt,
        actor: Actor::Person,
        actor_id: "founder",
        work: true,
        raw_file: "/scratch/session.jsonl",
        raw_line: 42,
        event_id: Some("event-42"),
        text,
    }
}

#[test]
fn trial_removed_words_become_one_verbatim_absence_proposal_and_one_note() {
    let source = prompt("No free trial anywhere. Only approving a version is paid.");
    let reading = propose(&source).unwrap();
    assert_eq!(reading.proposals.len(), 1);
    assert_eq!(reading.proposals[0].kind, "A1");
    assert_eq!(reading.proposals[0].text, "No free trial anywhere");
    assert_eq!(reading.proposals[0].object, "free trial anywhere");
    assert_eq!(reading.proposals[0].receipt.raw_file, source.raw_file);
    assert_eq!(reading.proposals[0].receipt.raw_line, source.raw_line);
    assert_eq!(
        reading.proposals[0].receipt.event_id.as_deref(),
        source.event_id
    );
    assert_eq!(reading.notes.len(), 1);
    assert_eq!(reading.notes[0].text, "Only approving a version is paid");
}

#[test]
fn founder_ruling_prefix_keeps_the_exact_person_span() {
    let text = "Founder ruling C20: no free trial anywhere. Only approving a version is paid, and there's no trial of it.";
    let reading = propose(&prompt(text)).expect("person work prompt");
    assert_eq!(reading.proposals.len(), 1);
    let proposal = &reading.proposals[0];
    assert_eq!(proposal.kind, "A1");
    assert_eq!(proposal.text, "no free trial anywhere");
    assert_eq!(
        &text[proposal.receipt.offset..proposal.receipt.offset + proposal.receipt.length],
        proposal.text
    );
    assert_eq!(reading.notes.len(), 1);
}

#[test]
fn absence_openers_classify_object_shape_without_rewording_it() {
    let cases = [
        ("Remove `LEGACY_FLAG`.", "A2", "`LEGACY_FLAG`"),
        ("Delete ui/src/old/.", "A4", "ui/src/old/"),
        (
            "Drop the reqwest dependency.",
            "A3",
            "the reqwest dependency",
        ),
        ("Get rid of the old badge.", "A1", "the old badge"),
        ("Strip out stale labels.", "A1", "stale labels"),
    ];
    for (sentence, kind, object) in cases {
        let reading = propose(&prompt(sentence)).unwrap();
        assert_eq!(reading.proposals.len(), 1, "{sentence}");
        assert_eq!(reading.proposals[0].kind, kind, "{sentence}");
        assert_eq!(reading.proposals[0].object, object, "{sentence}");
        assert!(reading.notes.is_empty(), "{sentence}");
    }
}

#[test]
fn preservation_scope_wording_and_quality_openers_have_closed_kinds() {
    let cases = [
        ("Keep LICENSE.", "B1", "LICENSE"),
        ("Keep test approval_is_paid.", "B2", "test approval_is_paid"),
        ("Don't touch release/**.", "D2", "release/**"),
        ("Only touch ui/src/**.", "D1", "ui/src/**"),
        ("Don't modify tests.", "D3", "tests"),
        ("Call it ThruLine HQ.", "G1", "ThruLine HQ"),
        ("Never say outdated.", "G2", "outdated"),
        ("Use the name RAAS Labs LLC.", "G4", "RAAS Labs LLC"),
        ("Must pass cargo test.", "E1", "cargo test"),
        ("Clippy clean.", "E1", "Clippy"),
        ("No warnings.", "E2", "warnings"),
    ];
    for (sentence, kind, object) in cases {
        let reading = propose(&prompt(sentence)).unwrap();
        assert_eq!(reading.proposals.len(), 1, "{sentence}");
        assert_eq!(reading.proposals[0].kind, kind, "{sentence}");
        assert_eq!(reading.proposals[0].object, object, "{sentence}");
        assert!(reading.notes.is_empty(), "{sentence}");
    }
}

#[test]
fn every_proposal_is_an_exact_source_span_including_bullets_and_unicode() {
    let text = "A prefatory note.\n- Remove `OLD_FLAG` and `OLD_COPY`.\n- Only touch ui/src/**.";
    let reading = propose(&prompt(text)).unwrap();
    assert_eq!(reading.proposals.len(), 2);
    for proposal in &reading.proposals {
        assert_eq!(
            &text[proposal.receipt.offset..proposal.receipt.offset + proposal.receipt.length],
            proposal.text
        );
        assert!(proposal.text.contains(&proposal.object));
    }
    assert_eq!(reading.proposals[0].terms, ["`OLD_FLAG`", "`OLD_COPY`"]);
    assert_eq!(reading.proposals[1].object, "ui/src/**");
}

#[test]
fn uncheckable_or_incomplete_language_stays_a_note() {
    let reading = propose(&prompt("Make it beautiful. Remove. Be careful.")).unwrap();
    assert!(reading.proposals.is_empty());
    assert_eq!(reading.notes.len(), 3);
    assert_eq!(reading.notes[0].reason, "uncheckable_language");
    assert_eq!(reading.notes[1].reason, "missing_object");
    assert_eq!(reading.notes[2].reason, "uncheckable_language");
}

#[test]
fn agent_model_handover_and_non_work_prompt_cannot_propose() {
    let source = prompt("Remove stale labels.");
    assert_eq!(
        propose(&Source {
            actor: Actor::Agent,
            ..source
        })
        .unwrap_err(),
        Refusal::NotPerson
    );
    assert_eq!(
        propose(&Source {
            actor: Actor::Model,
            ..source
        })
        .unwrap_err(),
        Refusal::NotPerson
    );
    assert_eq!(
        propose(&Source {
            kind: SourceKind::Handover,
            ..source
        })
        .unwrap_err(),
        Refusal::UnsupportedSource
    );
    assert_eq!(
        propose(&Source {
            kind: SourceKind::CommitMessage,
            ..source
        })
        .unwrap_err(),
        Refusal::UnsupportedSource
    );
    assert_eq!(
        propose(&Source {
            work: false,
            ..source
        })
        .unwrap_err(),
        Refusal::NotWorkPrompt
    );
    assert_eq!(
        propose(&Source {
            raw_file: "",
            ..source
        })
        .unwrap_err(),
        Refusal::MissingReceipt
    );
    assert_eq!(
        propose(&Source {
            raw_line: 0,
            ..source
        })
        .unwrap_err(),
        Refusal::MissingReceipt
    );
    assert_eq!(
        propose(&Source {
            event_id: None,
            ..source
        })
        .unwrap_err(),
        Refusal::MissingReceipt
    );
    assert_eq!(
        propose(&Source {
            actor_id: "",
            ..source
        })
        .unwrap_err(),
        Refusal::MissingActor
    );
}

#[test]
fn rule_file_source_is_limited_and_keeps_its_own_line_number() {
    let source = Source {
        kind: SourceKind::RuleFile,
        actor: Actor::Person,
        actor_id: "founder",
        work: false,
        raw_file: "CLAUDE.md",
        raw_line: 41,
        event_id: None,
        text: "Never say outdated.",
    };
    let reading = propose(&source).unwrap();
    assert_eq!(reading.proposals.len(), 1);
    assert_eq!(reading.proposals[0].kind, "G2");
    assert_eq!(reading.proposals[0].receipt.raw_file, "CLAUDE.md");
    assert_eq!(reading.proposals[0].receipt.raw_line, 41);
    assert_eq!(
        propose(&Source {
            raw_file: "docs/CTO-HANDOVER.md",
            ..source
        })
        .unwrap_err(),
        Refusal::UnsupportedSource
    );
}

#[test]
fn lowercase_imperatives_and_separate_clauses_keep_exact_offsets() {
    let text = "remove the old badge, and delete ui/legacy/.\nno stale label anywhere.";
    let reading = propose(&prompt(text)).unwrap();
    assert_eq!(reading.proposals.len(), 3);
    assert_eq!(reading.proposals[0].kind, "A1");
    assert_eq!(reading.proposals[0].object, "the old badge");
    assert_eq!(reading.proposals[1].kind, "A4");
    assert_eq!(reading.proposals[1].object, "ui/legacy/");
    assert_eq!(reading.proposals[2].kind, "A1");
    assert_eq!(reading.proposals[2].scope_hint.as_deref(), Some("anywhere"));
    for proposal in &reading.proposals {
        assert_eq!(
            &text[proposal.receipt.offset..proposal.receipt.offset + proposal.receipt.length],
            proposal.text
        );
    }
}

#[test]
fn each_rule_file_line_gets_its_actual_line_receipt() {
    let source = Source {
        kind: SourceKind::RuleFile,
        actor: Actor::Person,
        actor_id: "founder",
        work: false,
        raw_file: "AGENTS.md",
        raw_line: 12,
        event_id: None,
        text: "- Keep LICENSE.\n- Never say outdated.",
    };
    let reading = propose(&source).unwrap();
    assert_eq!(reading.proposals.len(), 2);
    assert_eq!(reading.proposals[0].receipt.raw_line, 12);
    assert_eq!(reading.proposals[1].receipt.raw_line, 13);
}

#[test]
fn sentence_cut_preserves_paths_versions_quotes_and_soft_wraps() {
    let text = "Remove `src/lib.rs`. Keep v1.2.3 in CHANGELOG.md.\n\
                - No \"Preview. Run?\" text anywhere.\n\
                Remove the old\nlabel.";
    let reading = propose(&prompt(text)).unwrap();
    assert_eq!(reading.proposals.len(), 4);
    assert_eq!(reading.proposals[0].text, "Remove `src/lib.rs`");
    assert_eq!(reading.proposals[1].text, "Keep v1.2.3 in CHANGELOG.md");
    assert_eq!(
        reading.proposals[2].text,
        "No \"Preview. Run?\" text anywhere"
    );
    assert_eq!(reading.proposals[3].text, "Remove the old\nlabel");
    for proposal in &reading.proposals {
        assert_eq!(
            &text[proposal.receipt.offset..proposal.receipt.offset + proposal.receipt.length],
            proposal.text
        );
    }
}

#[test]
fn unicode_bullets_start_separate_rule_file_proposals() {
    let source = Source {
        kind: SourceKind::RuleFile,
        actor: Actor::Person,
        actor_id: "founder",
        work: false,
        raw_file: "AGENTS.md",
        raw_line: 8,
        event_id: None,
        text: "• Keep LICENSE.\n• Only touch ui/src/**.",
    };
    let reading = propose(&source).unwrap();
    assert_eq!(reading.proposals.len(), 2);
    assert_eq!(reading.proposals[0].receipt.raw_line, 8);
    assert_eq!(reading.proposals[1].receipt.raw_line, 9);
}
