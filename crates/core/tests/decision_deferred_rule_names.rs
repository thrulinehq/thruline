use thruline_core::decisions::grammar::{propose, Actor, Source, SourceKind};
#[test]
fn closed_rule_names_are_actual_receipts_and_derived_or_traversal_files_are_refused() {
    for path in [
        "CLAUDE.md",
        "nested/AGENTS.md",
        ".cursorrules",
        "GEMINI.md",
        ".windsurfrules",
        ".thruline/policy.yml",
        ".github/copilot-instructions.md",
        ".cursor/rules/checks.mdc",
        "nested/.cursor/rules/checks.mdc",
    ] {
        let source = Source {
            kind: SourceKind::RuleFile,
            actor: Actor::Person,
            actor_id: "founder",
            work: false,
            raw_file: path,
            raw_line: 1,
            event_id: None,
            text: "Keep REQUIRED.",
        };
        let read = propose(&source).unwrap();
        assert_eq!(read.proposals[0].receipt.raw_file, path);
    }
    for path in [
        ".thruline/decisions.yml",
        "docs/notes.md",
        "../CLAUDE.md",
        "/CLAUDE.md",
        ".cursor/rules/nested/checks.mdc",
    ] {
        let source = Source {
            kind: SourceKind::RuleFile,
            actor: Actor::Person,
            actor_id: "founder",
            work: false,
            raw_file: path,
            raw_line: 1,
            event_id: None,
            text: "Keep REQUIRED.",
        };
        assert!(propose(&source).is_err(), "{path}");
    }
}

#[test]
fn a_verified_repository_rule_source_does_not_ascribe_person_authorship() {
    let rule = Source {
        kind: SourceKind::RuleFile,
        actor: Actor::Repository,
        actor_id: "repository",
        work: false,
        raw_file: "AGENTS.md",
        raw_line: 1,
        event_id: None,
        text: "Keep REQUIRED.",
    };
    assert!(propose(&rule).is_ok());
    assert!(propose(&Source {
        kind: SourceKind::WorkPrompt,
        event_id: Some("event-1"),
        work: true,
        ..rule
    })
    .is_err());
    assert!(propose(&Source {
        actor: Actor::Agent,
        ..rule
    })
    .is_err());
    assert!(propose(&Source {
        actor: Actor::Model,
        ..rule
    })
    .is_err());
}
