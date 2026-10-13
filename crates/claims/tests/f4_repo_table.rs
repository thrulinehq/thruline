//! **F4 — a committed repo file silently emptied the claim table.**
//!
//! `Table::load` let `.thruline/claims.yml` *replace* the built-in table
//! wholesale. Measured on `b5335c0`, in a repository whose only override was
//!
//! ```yaml
//! version: 1
//! patterns:
//!   - kind: tests_pass
//!     scope: all
//!     phrases: ["zzzzz never said by anyone zzzzz"]
//! ```
//!
//! ```text
//! before the file : conclusion neutral · 1 unverifiable claim · badge partial
//! after  the file : conclusion success · 0 claims             · badge verified
//! claims.yml in the receipt payload: 0 occurrences
//! ```
//!
//! The operator's ruling, implemented here: **a repo table may only ADD
//! patterns, never remove or narrow a default**, and a non-default table's
//! digest is disclosed.
//!
//! # The obvious way to defeat a naive union, and why it is refused
//!
//! Appending the repo's rows to the defaults does not, on its own, close the
//! hole. The matcher takes the FIRST pattern that matches and then blocks every
//! later pattern of the same kind (`seen_kinds`). So a repo row of the same
//! kind carrying a phrase the defaults already match, but a *narrower* scope,
//! can change `tests_pass/all` into `tests_pass/named` — and §6.3 lets a
//! narrowed test run support a `named` claim that it may not support for `all`.
//! That moves a verdict UP, from a file nobody signed.
//!
//! Two independent guards, both asserted below: the defaults always occupy the
//! front of the effective order, **and** a repo row whose phrase is already
//! reachable through a default row of the same kind with a different scope is
//! a load error.

use thruline_claims::{extract, Source, Table};

fn src(text: &str) -> Vec<Source> {
    vec![Source {
        event_id: "evt_x".into(),
        session_id: "s".into(),
        text: text.into(),
    }]
}

fn repo_with(yaml: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join(".thruline")).expect("mkdir");
    std::fs::write(dir.path().join(".thruline/claims.yml"), yaml).expect("write");
    dir
}

const EMPTYING: &str = r#"version: 1
patterns:
  - kind: tests_pass
    scope: all
    phrases:
      - "zzzzz never said by anyone zzzzz"
"#;

#[test]
fn a_repo_table_cannot_delete_a_default_pattern() {
    let dir = repo_with(EMPTYING);
    let table = Table::load(dir.path()).expect("the override is well formed");
    let e = extract(&src("All tests pass."), &table);
    assert_eq!(
        e.claims.len(),
        1,
        "the repo file replaced the built-in table instead of adding to it: {:?}",
        e.claims
    );
    assert_eq!(e.claims[0].kind.as_str(), "tests_pass");
}

#[test]
fn a_repo_table_adds_its_own_phrases() {
    let dir = repo_with(
        r#"version: 1
patterns:
  - kind: tests_pass
    scope: all
    phrases:
      - "the harness is green end to end"
"#,
    );
    let table = Table::load(dir.path()).expect("load");
    let mine = extract(&src("The harness is green end to end."), &table);
    assert_eq!(mine.claims.len(), 1, "the added phrase did not match");
    let default = extract(&src("All tests pass."), &table);
    assert_eq!(
        default.claims.len(),
        1,
        "the default phrase stopped matching"
    );
}

#[test]
fn the_defaults_come_first_in_the_effective_order() {
    let dir = repo_with(
        r#"version: 1
patterns:
  - kind: build_ok
    scope: all
    phrases:
      - "shipped it"
"#,
    );
    let table = Table::load(dir.path()).expect("load");
    let built_in = Table::default();
    let n = built_in.patterns().len();
    assert!(
        table.patterns().len() > n,
        "the repo row was not added at all"
    );
    assert_eq!(
        &table.patterns()[..n],
        built_in.patterns(),
        "a repo row was interleaved with, or reordered against, the defaults — \
         the matcher takes the first match, so order is a security property here"
    );
}

/// The shadow the brief names: same kind, a phrase the defaults already reach,
/// a narrower scope. A pure append-union would accept this.
#[test]
fn a_repo_row_may_not_narrow_the_scope_of_a_phrase_the_defaults_already_match() {
    for yaml in [
        // Exactly a default phrase, re-scoped.
        r#"version: 1
patterns:
  - kind: tests_pass
    scope: named
    phrases:
      - "all tests pass"
"#,
        // A superstring of a default phrase: every sentence it matches is a
        // sentence the default matches too, so its only possible effect is the
        // scope.
        r#"version: 1
patterns:
  - kind: tests_pass
    scope: named
    phrases:
      - "all tests pass on my machine"
"#,
    ] {
        let dir = repo_with(yaml);
        let err = Table::load(dir.path())
            .expect_err("a scope-narrowing shadow row must be refused, not merged");
        let text = err.to_string();
        assert!(
            text.contains("shadow") || text.contains("narrow"),
            "the refusal must say what it refused: {text}"
        );
    }
}

/// Same phrase, same scope is harmless redundancy — refusing it would make the
/// override unusable for anyone who copies a default row before editing it.
#[test]
fn a_repo_row_that_repeats_a_default_at_the_same_scope_is_not_an_error() {
    let dir = repo_with(
        r#"version: 1
patterns:
  - kind: tests_pass
    scope: all
    phrases:
      - "all tests pass"
      - "the pipeline is fully green"
"#,
    );
    let table = Table::load(dir.path()).expect("redundancy is not a shadow");
    let e = extract(&src("The pipeline is fully green."), &table);
    assert_eq!(e.claims.len(), 1);
}

#[test]
fn a_non_default_table_has_a_different_digest_and_says_so() {
    let built_in = Table::default();
    assert!(
        built_in.digest().starts_with("sha256:"),
        "{}",
        built_in.digest()
    );
    assert!(
        built_in.disclosure().is_none(),
        "the built-in table is not an override and must not be announced as one"
    );

    let dir = repo_with(
        r#"version: 1
patterns:
  - kind: tests_pass
    scope: all
    phrases:
      - "the harness is green end to end"
"#,
    );
    let table = Table::load(dir.path()).expect("load");
    assert_ne!(
        table.digest(),
        built_in.digest(),
        "an overridden table must not hash the same as the built-in one"
    );
    let said = table
        .disclosure()
        .expect("an override must disclose itself");
    assert!(said.contains(&table.digest()), "{said}");
    assert!(said.contains(".thruline/claims.yml"), "{said}");
}

/// A malformed override stays an error — it always was, and the union must not
/// have turned it into a silent fall back to the defaults.
#[test]
fn a_malformed_override_is_still_an_error_and_not_a_quiet_default() {
    let dir = repo_with("version: 9\npatterns:\n");
    assert!(Table::load(dir.path()).is_err());
}
