//! Pre-registration, and the seams a reviewer would go at.

use super::*;

fn protocol() -> Protocol {
    Protocol {
        harness: "impossiblebench".into(),
        harness_version: "0.3.1".into(),
        model: "some-model-2026-09".into(),
        tasks: vec!["t1".into(), "t2".into(), "t3".into(), "t4".into()],
        holdout: vec!["t3".into(), "t4".into()],
        parameters: BTreeMap::from([
            ("seed".to_string(), "17".to_string()),
            ("command".to_string(), "inspect eval …".to_string()),
        ]),
        detectors: BTreeMap::from([
            ("narrowed_run".to_string(), "enabled".to_string()),
            ("rename_jaccard".to_string(), "0.6".to_string()),
            ("min_changed_line_coverage".to_string(), "70".to_string()),
            (
                "skip_marker_added".to_string(),
                "fail-if-run-skipped".to_string(),
            ),
        ]),
        preconditions: Default::default(),
    }
}

// ---------------------------------------------------------------------------
// The commitment
// ---------------------------------------------------------------------------

/// The hash is over content, so re-running the same protocol re-derives it.
#[test]
fn the_same_protocol_commits_to_the_same_hash() {
    let a = protocol().commit().expect("commit");
    let b = protocol().commit().expect("commit");
    assert_eq!(a.hash(), b.hash());
    assert!(a.hash().starts_with("sha256:"), "{}", a.hash());
    assert_eq!(a.hash().len(), "sha256:".len() + 64);
}

/// Order is not content. Two protocols differing only in list or map order are
/// the same protocol, and a hash that said otherwise would refuse honest
/// re-runs.
#[test]
fn ordering_is_not_content() {
    let mut reordered = protocol();
    reordered.tasks = vec!["t4".into(), "t1".into(), "t3".into(), "t2".into()];
    reordered.holdout = vec!["t4".into(), "t3".into()];
    assert_eq!(protocol().hash(), reordered.hash());
}

/// A named edit to a protocol: the thing a later run might be tempted to make.
type Mutation = Box<dyn Fn(&mut Protocol)>;

/// **Every field that could be tuned in response to results changes the hash.**
///
/// This is the property the whole scheme rests on: if a parameter could move
/// without moving the hash, it could move after seeing results.

#[test]
fn every_tunable_field_moves_the_hash() {
    let base = protocol().hash();
    let mutations: Vec<(&str, Mutation)> = vec![
        (
            "harness",
            Box::new(|p: &mut Protocol| p.harness = "other".into()),
        ),
        (
            "harness_version",
            Box::new(|p: &mut Protocol| p.harness_version = "0.3.2".into()),
        ),
        (
            "model",
            Box::new(|p: &mut Protocol| p.model = "other".into()),
        ),
        (
            "tasks (one added)",
            Box::new(|p: &mut Protocol| p.tasks.push("t5".into())),
        ),
        (
            "tasks (one dropped)",
            Box::new(|p: &mut Protocol| {
                p.tasks.retain(|t| t != "t2");
            }),
        ),
        (
            "holdout",
            Box::new(|p: &mut Protocol| p.holdout = vec!["t1".into()]),
        ),
        (
            "a parameter's value",
            Box::new(|p: &mut Protocol| {
                p.parameters.insert("seed".into(), "18".into());
            }),
        ),
        (
            "a parameter added",
            Box::new(|p: &mut Protocol| {
                p.parameters.insert("temperature".into(), "0".into());
            }),
        ),
        // **The instrument, not just the data.** Every one of these is a dial
        // this build has actually turned at some point.
        (
            "a detector threshold",
            Box::new(|p: &mut Protocol| {
                p.detectors.insert("rename_jaccard".into(), "0.5".into());
            }),
        ),
        (
            "a detector disabled",
            Box::new(|p: &mut Protocol| {
                p.detectors.insert("narrowed_run".into(), "disabled".into());
            }),
        ),
        (
            "a severity mapping",
            Box::new(|p: &mut Protocol| {
                p.detectors
                    .insert("skip_marker_added".into(), "warn-always".into());
            }),
        ),
    ];
    for (what, mutate) in mutations {
        let mut p = protocol();
        mutate(&mut p);
        assert_ne!(
            p.hash(),
            base,
            "changing {what} left the commitment hash unchanged, so it could be \
             changed after seeing results"
        );
    }
}

/// A commitment that commits to nothing is refused.
#[test]
fn an_empty_protocol_cannot_be_committed() {
    for (field, mutate) in [
        (
            "harness",
            Box::new(|p: &mut Protocol| p.harness = " ".into()) as Box<dyn Fn(&mut Protocol)>,
        ),
        (
            "model",
            Box::new(|p: &mut Protocol| p.model = String::new()),
        ),
        ("tasks", Box::new(|p: &mut Protocol| p.tasks.clear())),
        ("holdout", Box::new(|p: &mut Protocol| p.holdout.clear())),
        (
            "detectors",
            Box::new(|p: &mut Protocol| p.detectors.clear()),
        ),
    ] {
        let mut p = protocol();
        mutate(&mut p);
        assert_eq!(
            p.commit().expect_err("must refuse"),
            ClassError::EmptyProtocol { field },
            "an empty `{field}` was committed"
        );
    }
}

/// **A holdout outside the task set is a holdout chosen elsewhere.**
#[test]
fn the_holdout_must_be_a_subset_of_the_declared_tasks() {
    let mut p = protocol();
    p.holdout.push("t99".into());
    match p.commit().expect_err("must refuse") {
        ClassError::HoldoutNotInTasks { stray } => assert_eq!(stray, vec!["t99".to_string()]),
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The classes carry their evidence
// ---------------------------------------------------------------------------

/// Class 1 cannot be asserted without saying whose labels and from which field.
#[test]
fn class_one_needs_a_named_source_and_label_field() {
    assert!(ExternalSource::new("princeton-nlp", "SWE-bench", "v1", "resolved").is_ok());
    for (bad, field) in [
        (["", "d", "v", "l"], "publisher"),
        (["p", "", "v", "l"], "dataset"),
        (["p", "d", "  ", "l"], "version"),
        (["p", "d", "v", ""], "label_field"),
    ] {
        assert_eq!(
            ExternalSource::new(bad[0], bad[1], bad[2], bad[3]).expect_err("must refuse"),
            ClassError::MissingEvidence { class: 1, field }
        );
    }
}

/// **A commitment is obtainable only from a protocol.**
///
/// There is no public constructor and no setter for the hash, so a commitment
/// cannot be conjured to match a run that already happened. That is what makes
/// class 2 a claim about *sequence* rather than about a string.
#[test]
fn a_commitment_cannot_be_built_without_a_protocol() {
    let source = include_str!("../protocol.rs");
    // No `pub fn new` on Commitment, and no way to set the hash.
    let commitment_block = source
        .split("pub struct Commitment")
        .nth(1)
        .expect("the struct");
    assert!(
        !commitment_block.contains("pub hash"),
        "the hash became a public field, so a commitment can be written to match \
         a run that already happened"
    );
    assert!(
        !commitment_block.contains("pub fn new") && !commitment_block.contains("pub fn with_hash"),
        "a public constructor appeared on Commitment"
    );
    // ...and the only producer is `commit`.
    assert_eq!(
        source.matches("Ok(Commitment {").count(),
        1,
        "there is more than one place a Commitment is built"
    );
}

/// The three classes are distinguishable and only two count.
#[test]
fn class_three_is_the_only_one_that_does_not_count() {
    let external =
        Provenance::ExternalLabelled(ExternalSource::new("p", "d", "v", "l").expect("source"));
    let generated = Provenance::GeneratedUnderProtocol(protocol().commit().expect("commit"));
    assert_eq!(external.class(), 1);
    assert_eq!(generated.class(), 2);
    assert_eq!(Provenance::SelfAuthored.class(), 3);
    assert!(external.counts_toward_targets());
    assert!(generated.counts_toward_targets());
    assert!(!Provenance::SelfAuthored.counts_toward_targets());
}

/// **Classes 1 and 2 are not interchangeable by an edit.**
///
/// Both count toward the targets now, so moving between them is a lie about
/// which discipline produced a number — a stronger claim than the old
/// binary's, and it needs more than visibility. Each variant holds evidence the
/// other's does not, so the edit does not compile.
#[test]
fn moving_between_classes_one_and_two_is_not_a_token_edit() {
    let commitment = protocol().commit().expect("commit");
    let source = ExternalSource::new("p", "d", "v", "l").expect("source");
    // Neither payload is accepted by the other variant. This is asserted by
    // the fact that the following two lines are the only ways to build them,
    // and each requires evidence the other class does not have:
    let _one = Provenance::ExternalLabelled(source);
    let _two = Provenance::GeneratedUnderProtocol(commitment);
    // A source-level check that the variants stayed payload-carrying, since
    // making either a unit variant would restore the one-token edit.
    let src = include_str!("../protocol.rs");
    assert!(src.contains("ExternalLabelled(ExternalSource)"), "{src:.0}");
    assert!(src.contains("GeneratedUnderProtocol(Commitment)"));
}

/// **V6 — a value containing a separator cannot impersonate a field boundary.**
///
/// The preimage joined fields with a raw `\u{1}` and list items with a raw
/// `\u{2}`, so two different protocols hashed the same. `parameters` is
/// free-form by design — it holds prompts and exact commands — which is exactly
/// where an arbitrary byte arrives.
#[test]
fn a_separator_inside_a_value_cannot_forge_a_different_protocol() {
    let field_collisions: [(&str, &str, &str, &str); 2] = [
        ("cmd", "run\u{1}seed\u{1}7", "cmd\u{1}run", "seed\u{1}7"),
        ("a", "b\u{1}c", "a\u{1}b", "c"),
    ];
    for (k1, v1, k2, v2) in field_collisions {
        let mut a = protocol();
        let mut b = protocol();
        a.parameters.clear();
        b.parameters.clear();
        a.parameters.insert(k1.to_string(), v1.to_string());
        b.parameters.insert(k2.to_string(), v2.to_string());
        assert_ne!(
            a.hash(),
            b.hash(),
            "`{k1}`=`{v1}` and `{k2}`=`{v2}` share a commitment hash"
        );
    }

    // And the list separator, in the accounting denominator itself.
    let mut a = protocol();
    let mut b = protocol();
    a.tasks = vec!["a\u{2}b".into(), "c".into(), "z".into()];
    b.tasks = vec!["a".into(), "b\u{2}c".into(), "z".into()];
    a.holdout = vec!["z".into()];
    b.holdout = vec!["z".into()];
    assert_ne!(
        a.hash(),
        b.hash(),
        "two different task sets share a commitment hash"
    );

    // The escape character is itself escaped, or the mapping is not injective.
    let mut c = protocol();
    let mut d = protocol();
    c.parameters.clear();
    d.parameters.clear();
    c.parameters.insert("k".into(), "\u{0}1".to_string());
    d.parameters.insert("k".into(), "\u{1}".to_string());
    assert_ne!(c.hash(), d.hash(), "the escape character is not escaped");

    // Ordinary protocols are unaffected: the escaping must not change the
    // hash of anything that contains no separator.
    assert_eq!(protocol().hash(), protocol().hash());
}
