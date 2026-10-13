use super::*;

fn saved(span: &str, path: &str, reason: &str) -> SavedNote {
    let anchor = RuleFileAnchor {
        version_id: "a".repeat(40),
        tree_hash: "b".repeat(40),
        blob_oid: "c".repeat(40),
        blob_sha256: "d".repeat(64),
        path: path.into(),
        raw_line: 1,
        span_start: 0,
        span_end: span.len() as u64,
        span_sha256: sha256_hex(span.as_bytes()),
    };
    let encoded = format!(
        "{ANCHOR_PREFIX}{}",
        serde_json::to_string(&anchor).expect("anchor")
    );
    let source = DecisionSource {
        source_kind: SourceKind::RuleFile,
        event_id: None,
        file_anchor: Some(encoded.clone()),
        raw_file: path.into(),
        raw_line: 1,
        span_start: 0,
        span_end: span.len() as u64,
        span: span.into(),
        actor: None,
        recorded_at: None,
    };
    SavedNote {
        id: format!("rule-note-{}", content_key("synthetic", &anchor, "D1")),
        project: "synthetic".into(),
        source_anchor: encoded,
        code: "D1".into(),
        span: span.into(),
        source_json: serde_json::to_string(&source).expect("source"),
        anchor_json: serde_json::to_string(&anchor).expect("anchor"),
        reason: reason.into(),
        at: "2026-10-08T00:00:00Z".into(),
        schema: 1,
    }
}

#[test]
fn saved_rule_notes_refuse_bare_credential_fields_and_preserve_plain_bindings() {
    assert!(decode_note(saved(
        "Only touch scoped_thing",
        "AGENTS.md",
        "needs a typed scope"
    ))
    .is_ok());
    for prefix in [
        "sk-",
        "sk-ant-",
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "AKIA",
        "ASIA",
        "AIza",
        "eyJ",
        "xoxb-",
    ] {
        for (span, path, reason) in [
            (
                format!("Only touch {prefix}"),
                "AGENTS.md".into(),
                "needs a typed scope".into(),
            ),
            (
                "Only touch scoped_thing".into(),
                format!("{prefix}/AGENTS.md"),
                "needs a typed scope".into(),
            ),
            (
                "Only touch scoped_thing".into(),
                "AGENTS.md".into(),
                format!("needs {prefix} scope"),
            ),
        ] {
            assert!(
                decode_note(saved(&span, &path, &reason)).is_err(),
                "bare prefix {prefix} reached a saved note"
            );
        }
    }
}
