//! A person's path exclusions are a presentation rule. The matcher takes the
//! standing, project-relative keys and a path; it reads no record or files.

use std::borrow::Cow;

use thruline_core::capture_exclusion::{count_field, marker, ExclusionPolicy};
use thruline_core::capture_hold::ExclusionView;

const ROOT: &str = "/work/app";

fn entry(key: &str, on: bool) -> ExclusionView {
    ExclusionView {
        key: key.into(),
        covers: key.into(),
        note: None,
        built_in: matches!(key, "env" | "secrets" | "generated"),
        on,
        source: "recorded",
        basis: None,
        since: None,
        by: None,
    }
}

fn policy(entries: &[ExclusionView]) -> ExclusionPolicy {
    ExclusionPolicy::from_exclusions(ROOT, entries).expect("synthetic policy")
}

#[test]
fn the_built_ins_match_only_the_files_or_directory_their_names_cover() {
    let p = policy(&[entry("env", true), entry("secrets", true)]);
    for path in [".env", "src/.env.local", "/work/app/deep/.env.test"] {
        assert_eq!(p.key_for_path(path, false), Some("env"), "{path}");
    }
    for path in ["secrets/key", "src/secrets/key.txt", "/work/app/secrets"] {
        assert_eq!(p.key_for_path(path, false), Some("secrets"), "{path}");
    }
    for path in [
        ".environment",
        "src/.envrc",
        "src/secrets-old/key",
        "secret/key",
    ] {
        assert_eq!(p.key_for_path(path, false), None, "{path}");
    }
    assert_eq!(
        p.key_for_path("secrets/.env.production", false),
        Some("env"),
        "first active key wins when more than one matches"
    );
}

#[test]
fn typed_patterns_are_anchored_to_the_project_and_wildcards_never_cross_a_component() {
    let p = policy(&[
        entry("vendor/legacy", true),
        entry("config/*.toml", true),
        entry("src/*/token.key", true),
    ]);
    for path in [
        "vendor/legacy",
        "vendor/legacy/deep/file.rs",
        "/work/app/vendor/legacy/a",
    ] {
        assert_eq!(p.key_for_path(path, false), Some("vendor/legacy"), "{path}");
    }
    assert_eq!(
        p.key_for_path("config/dev.toml", false),
        Some("config/*.toml")
    );
    assert_eq!(
        p.key_for_path("src/api/token.key", false),
        Some("src/*/token.key")
    );
    for path in [
        "vendor/legacy-old/a",
        "other/vendor/legacy/a",
        "config/deep/dev.toml",
        "src/api/deep/token.key",
        "/work/app2/vendor/legacy/a",
        "/work/other/vendor/legacy/a",
        "../app2/vendor/legacy/a",
    ] {
        assert_eq!(p.key_for_path(path, false), None, "{path}");
    }
    assert_eq!(
        p.key_for_path("src/../vendor/legacy/a", false),
        Some("vendor/legacy"),
        "an internal dot-dot is resolved before matching"
    );
}

#[test]
fn generated_is_supplied_by_the_caller_and_inactive_keys_have_no_effect() {
    let p = policy(&[
        entry("env", false),
        entry("secrets", false),
        entry("generated", true),
        entry("private/*", false),
    ]);
    assert_eq!(p.key_for_path("src/generated.rs", false), None);
    assert_eq!(p.key_for_path("src/generated.rs", true), Some("generated"));
    assert_eq!(p.key_for_path(".env", false), None);
    assert_eq!(p.key_for_path("secrets/key", false), None);
    assert_eq!(p.key_for_path("private/key", false), None);
    assert_eq!(p.key_for_path("/other/generated.rs", true), None);
    assert_eq!(p.project_root(), ROOT);
    assert!(!p.is_empty());
    assert!(p.has_generated());
    assert!(policy(&[]).is_empty());
    assert!(!policy(&[entry("generated", false)]).has_generated());
}

#[test]
fn the_marker_and_optional_count_field_leave_a_no_policy_view_byte_unchanged() {
    let empty = policy(&[]);
    let original = "src/public.rs (+2 −1)";
    assert!(matches!(
        empty.mark_entry("src/public.rs", false, original),
        Cow::Borrowed(s) if s == original
    ));
    assert_eq!(count_field(0), None);
    assert_eq!(count_field(2), Some(("left_out_by_exclusion", 2)));
    assert_eq!(
        marker("vendor/legacy"),
        "left out by your exclusion (`vendor/legacy`)"
    );

    let p = policy(&[entry("vendor/legacy", true)]);
    assert_eq!(
        p.mark_entry("vendor/legacy/api.key", false, original),
        Cow::<str>::Owned(marker("vendor/legacy"))
    );
    assert!(matches!(
        p.mark_entry("src/public.rs", false, original),
        Cow::Borrowed(s) if s == original
    ));
}

#[test]
fn a_bad_standing_pattern_is_refused_instead_of_being_silently_ignored() {
    for key in ["../outside", "/work/app/secrets", "private/[ab]"] {
        assert!(
            ExclusionPolicy::from_exclusions(ROOT, &[entry(key, true)]).is_err(),
            "{key}"
        );
    }
    assert!(ExclusionPolicy::from_exclusions("work/app", &[entry("env", true)]).is_err());
}

#[test]
fn the_fingerprint_names_only_the_active_key_set_not_its_history_or_project() {
    let first = policy(&[
        entry("env", true),
        entry("secrets", false),
        entry("private/*", true),
    ]);
    let same = policy(&[entry("private/*", true), entry("env", true)]);
    let restored = policy(&[
        entry("secrets", false),
        entry("private/*", true),
        entry("env", true),
    ]);
    let other_root = ExclusionPolicy::from_exclusions(
        "/some/other/project",
        &[entry("env", true), entry("private/*", true)],
    )
    .expect("other root");
    let changed = policy(&[entry("env", true), entry("private/*", false)]);
    assert_eq!(first.fingerprint(), same.fingerprint());
    assert_eq!(first.fingerprint(), restored.fingerprint());
    assert_eq!(first.fingerprint(), other_root.fingerprint());
    assert_ne!(first.fingerprint(), changed.fingerprint());
}

#[test]
fn text_matching_requires_a_path_token_with_component_boundaries() {
    let p = policy(&[
        entry("env", true),
        entry("secrets", true),
        entry("vendor/legacy", true),
    ]);
    assert_eq!(p.key_in_text("cat src/secrets/api.key"), Some("secrets"));
    assert_eq!(p.key_in_text("Edited `src/.env.local`."), Some("env"));
    assert_eq!(
        p.key_in_text("Please revisit vendor/legacy/a.rs"),
        Some("vendor/legacy")
    );
    assert_eq!(p.key_in_text("open ./secrets/key"), Some("secrets"));
    for text in [
        "secrets are useful",
        "the secrets-old/key is harmless",
        "https://example.com/secrets/key",
        "~/other/secrets/key",
        "$HOME/secrets/key",
        "the .environment file",
        "a generated file",
    ] {
        assert_eq!(p.key_in_text(text), None, "{text}");
    }
}

#[test]
fn a_quoted_path_with_spaces_is_one_project_relative_path() {
    let p = policy(&[entry("private notes", true), entry("notes", true)]);
    assert_eq!(
        p.key_in_text("cat 'private notes/key.txt'"),
        Some("private notes")
    );
    assert_eq!(p.key_in_text("cat 'other/private notes/key.txt'"), None);

    assert_eq!(
        p.key_in_text("cat 'private notes/key.txt'"),
        Some("private notes"),
        "the whole quoted path wins before any word inside it"
    );
    let different = policy(&[entry("notes", true)]);
    assert_eq!(
        different.key_in_text("cat 'private notes/key.txt'"),
        None,
        "an interior word of a quoted path is not a new project-root path"
    );
}

#[test]
fn a_command_can_name_a_bare_excluded_path_as_a_file_argument() {
    let p = policy(&[entry("secrets", true), entry("private notes", true)]);
    assert_eq!(p.key_in_text("cat secrets"), None);
    assert_eq!(p.key_in_command("cat secrets"), Some("secrets"));
    assert_eq!(p.key_in_command("cat -n secrets"), Some("secrets"));
    assert_eq!(
        p.key_in_command("cat 'private notes'"),
        Some("private notes")
    );
    assert_eq!(p.key_in_command("echo secrets"), None);
    assert_eq!(p.key_in_command("secrets other"), None);
    assert_eq!(p.key_in_command("cat secrets-old"), None);
    assert_eq!(p.key_in_command("cat secrets."), Some("secrets"));

    let generated = policy(&[entry("generated", true)]);
    assert_eq!(
        generated.key_in_command_with_generated("cat BUILDLOG", |path| path == "BUILDLOG"),
        Some("generated"),
        "a generated file may have no dot or path separator"
    );
    assert_eq!(
        generated.key_in_command_with_generated("echo BUILDLOG", |path| path == "BUILDLOG"),
        None,
        "ordinary prose is not a file argument"
    );
}

#[test]
fn sentence_punctuation_does_not_become_part_of_a_mentioned_path() {
    let p = policy(&[entry("cart/totals.py", true)]);
    for text in [
        "Fix cart/totals.py.",
        "Look at cart/totals.py:",
        "Inspect cart/totals.py,",
        "Read cart/totals.py;",
        "Update cart/totals.py!",
        "Changed cart/totals.py?",
    ] {
        assert_eq!(p.key_in_text(text), Some("cart/totals.py"), "{text}");
    }
    assert_eq!(p.key_in_text("Fix cart/totals.py-old."), None);
    assert_eq!(p.key_for_path("cart/totals.py:", false), None);
    assert_eq!(
        p.key_in_text("\"I edited cart/totals.py\""),
        Some("cart/totals.py"),
        "a quote may contain prose about a path"
    );
    assert_eq!(
        p.key_in_text("\"Removed cart/totals.py\""),
        Some("cart/totals.py"),
        "a past-tense instruction lead is quoted prose"
    );
    for text in ["Read cart/totals.py:42", "Inspect cart/totals.py:42:7"] {
        assert_eq!(p.key_in_text(text), Some("cart/totals.py"), "{text}");
    }
    assert_eq!(p.key_in_text("Read cart/totals.py:review"), None);
    assert_eq!(p.key_for_path("cart/totals.py:42", false), None);
    let literal_colon = policy(&[entry("cart/totals.py:42", true)]);
    assert_eq!(
        literal_colon.key_in_text("Read cart/totals.py:42"),
        Some("cart/totals.py:42"),
        "a real filename with a colon wins before line-reference parsing"
    );
    let generated = policy(&[entry("generated", true)]);
    assert_eq!(
        generated.key_in_text_with_generated("Read cart/totals.py:42", |path| {
            path == "cart/totals.py"
        }),
        Some("generated")
    );
}

#[test]
fn quoted_generated_paths_with_spaces_use_the_callers_whole_path_fact() {
    let p = policy(&[entry("generated", true)]);
    let command = "cat \"generated/my file.rs\"";
    assert_eq!(p.key_in_text(command), None);
    assert_eq!(
        p.key_in_text_with_generated(command, |path| path == "generated/my file.rs"),
        Some("generated")
    );
    assert_eq!(
        p.key_in_text_with_generated(command, |path| path == "generated/my other file.rs"),
        None
    );
    assert_eq!(
        p.key_in_text_with_generated("cat generated/my file.rs-old", |path| {
            path == "generated/my file.rs"
        }),
        None,
        "a generated fact cannot match a textual suffix"
    );
}
