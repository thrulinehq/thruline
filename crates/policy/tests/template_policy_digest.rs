//! A receipt's policy digest includes every offered template, while preserving
//! the established digests of policies with no templates. No files are opened.

use thruline_policy::{absent_digest, DoneTemplate, Policy};

fn with_template(name: &str, lines: &[&str]) -> Policy {
    let mut policy = Policy::default();
    policy.templates.insert(
        name.to_string(),
        DoneTemplate {
            lines: lines.iter().map(|line| (*line).to_string()).collect(),
        },
    );
    policy
}

#[test]
fn a_template_and_its_name_are_bound_into_the_policy_digest() {
    let template = with_template("bug_fix", &["Keep the input."]);
    assert_ne!(
        template.digest(),
        Policy::default().digest(),
        "adding a template must change the policy digest"
    );
    assert_ne!(
        template.digest(),
        with_template("repair", &["Keep the input."]).digest(),
        "renaming a template must change the policy digest"
    );
}

#[test]
fn each_template_line_and_its_position_are_bound_into_the_policy_digest() {
    let original = with_template("bug_fix", &["Keep the input.", "Report errors."]);
    for changed in [
        vec!["Change the input.", "Report errors."],
        vec!["Keep the input.", "Ignore errors."],
        vec!["Report errors.", "Keep the input."],
        vec!["Keep the input."],
        vec!["Keep the input.", "Report errors.", "Run the checks."],
    ] {
        assert_ne!(
            original.digest(),
            with_template("bug_fix", &changed).digest(),
            "changed template lines left the digest unchanged: {changed:?}"
        );
    }
}

#[test]
fn template_map_order_is_canonical() {
    let first = Policy::parse(
        "version: 1\ntemplates:\n  alpha:\n    lines: [\"First.\"]\n  beta:\n    lines: [\"Second.\"]\n",
    )
    .expect("named templates");
    let reversed = Policy::parse(
        "version: 1\ntemplates:\n  beta:\n    lines: [\"Second.\"]\n  alpha:\n    lines: [\"First.\"]\n",
    )
    .expect("same templates in reverse order");
    assert_eq!(first.canonical(), reversed.canonical());
    assert_eq!(first.digest(), reversed.digest());
}

#[test]
fn template_line_boundaries_and_escaped_characters_are_unambiguous() {
    for (left, right) in [
        (vec!["a,b", "c"], vec!["a", "b,c"]),
        (vec!["a\",\"b"], vec!["a", "b"]),
        (vec!["a\nb"], vec!["a\\nb"]),
        (vec!["a\tb"], vec!["a\\tb"]),
        (vec!["a\\b"], vec!["ab"]),
        (vec!["keep \"quoted\""], vec!["keep quoted"]),
        (vec!["café"], vec!["cafe"]),
    ] {
        let left_policy = with_template("bug_fix", &left);
        let right_policy = with_template("bug_fix", &right);
        assert_ne!(
            left_policy.canonical(),
            right_policy.canonical(),
            "different lines share a canonical representation: {left:?} and {right:?}"
        );
        assert_ne!(left_policy.digest(), right_policy.digest());
    }
}

#[test]
fn absent_and_empty_templates_preserve_the_existing_policy_digests() {
    // These are the v1 digests before templates were included in canonical().
    for (text, expected) in [
        (
            "version: 1\n",
            "sha256:690603f8b2b347a463ceb8fdfe797de11d48105195c702466cf034418b226a70",
        ),
        (
            "version: 1\ndecisions:\n",
            "sha256:f3e5ab0f41bcdfc14234bc8fbc468fea407bcb71c1eb5f10b32f42c1647f12f9",
        ),
    ] {
        let absent = Policy::parse(text).expect("policy without templates");
        let empty = Policy::parse(&format!("{text}templates:\n"))
            .expect("policy with an empty template map");
        assert!(absent.templates.is_empty());
        assert!(empty.templates.is_empty());
        assert_eq!(absent.digest(), expected);
        assert_eq!(empty.digest(), expected);
        assert_eq!(empty.canonical(), absent.canonical());
    }
    assert_eq!(
        absent_digest(),
        "sha256:cfbdf195a01ee22251ddcbc3c0bd23c88bff89a265080934cb00099d329d62ad"
    );
}
