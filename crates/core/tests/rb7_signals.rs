//! RB7 — no distillation layer ahead of its provenance rules.
//!
//! The distilled node kinds may not appear in core until their layer lands
//! with provenance rules of its own.

#[test]
fn layer_two_never_wears_a_meanings_name() {
    // A157 evolved this gate DELIBERATELY: the layer now exists, so the pin
    // is the proxy rule itself — semantic words appear in no Layer-2 type,
    // field, or emitted key. They belong to Layer 3, attached only by proof,
    // operator attestation, or a shadow model proposal. (`interpretation` is
    // the SLOT for Layer 3 and carries no meaning while null.)
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/candidates.rs"))
        .expect("the Layer-2 module exists");
    let code: String = text
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("//!") && !t.starts_with("///")
        })
        .collect::<Vec<_>>()
        .join("\n");
    for word in [
        "Decision",
        "decision",
        "DeadEnd",
        "dead_end",
        "dead-end",
        "Purpose",
        "purpose",
        "Abandoned",
        "abandoned",
        "Outcome",
        "outcome",
    ] {
        assert!(
            !code.contains(word),
            "Layer-2 code wears a meaning's name: {word}"
        );
    }
    // The Layer-3 slot exists, honestly empty by construction.
    assert!(code.contains("interpretation: Option<String>"));
    assert!(code.contains("interpretation: None"));
    // And every structure carries provenance: event ids by name.
    for anchor in ["first_event_id", "last_event_id", "event_id"] {
        assert!(
            code.contains(anchor),
            "a structure lost its provenance field: {anchor}"
        );
    }
}
